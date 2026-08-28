//! Language servers, and the documents they are told about.
//!
//! **One reconciliation pass per batch, not a notification per event.** The
//! event loop already blocks for one event and drains everything queued behind
//! it before drawing; [`Lsp::sync`] runs at the end of that, compares what each
//! server has been told against what the tabs now hold, and sends the
//! difference. A ten-key burst is one `didChange`. The same argument
//! `typ-syntax`'s worker makes against a fixed reparse debounce, reused rather
//! than reinvented: the batch is the machine telling you how fast it is.
//!
//! Making it a reconciliation rather than a set of hooks is what makes the
//! awkward cases fall out for free. A document opened before the handshake
//! completed is announced on the pass after the `initialize` response lands. A
//! tab closed is noticed by its absence. Nothing needs a queue.
//!
//! **Nothing here may fail a keystroke.** A server that is missing, crashed,
//! slow or wrong degrades to the editor TYPE already is, and every path in this
//! file either sends something or returns.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ropey::Rope;
use typ_buffer::EditSpan;
use typ_core::Diagnostic;
use typ_lsp::RequestId;
use typ_lsp::{Client, Encoding, Incoming, LspEvent, ServerId, SpawnError};

/// One language server, and what it is for.
///
/// Keyed by extension rather than by TYPE's `Language` enum: TYPE highlights
/// five languages and can talk to a server for any file at all, so the set of
/// things that can have a server is not the set of things that have a grammar.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// The `languageId` sent in `didOpen`. `rust`, `toml`, `python`.
    pub language_id: String,
    /// Extensions this server handles, without the dot.
    pub extensions: Vec<String>,
    /// The binary. Not found on `PATH` is the ordinary case, not an error.
    pub command: String,
    pub args: Vec<String>,
}

impl ServerConfig {
    fn handles(&self, path: &Path) -> bool {
        let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
            return false;
        };
        self.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext))
    }
}

/// A configured server and whatever became of it.
enum Server {
    /// Nothing has needed it yet. Servers are never started on the cold-start
    /// path: the 100 ms budget cannot wait for rust-analyzer, which takes tens
    /// of seconds to be useful.
    NotStarted,
    Running(Box<Client>),
    /// It could not be started, or it died. **Never retried here.** A crash
    /// loop is worse than an absent server; the restart policy is Task 14's.
    Gone,
}

/// What one server has been told about one document.
struct Doc {
    server: ServerId,
    uri: String,
    /// Monotonic, and never reset — the specification only requires that it
    /// increase, so a document closed and reopened keeps counting rather than
    /// handing a server a version it has already seen.
    version: i32,
    /// The buffer revision the server was last sent. `None` while the document
    /// is known but closed.
    synced: Option<u64>,
    /// What the server has pushed about this document.
    ///
    /// Named `pushed` rather than `diagnostics` because the pulled set is a
    /// second store beside it — they share a namespace, so a client that merges
    /// them has each source clearing the other on every update, which is
    /// neovim#37936. The server's `diagnosticProvider.identifier` exists to
    /// keep them apart.
    pushed: Vec<Diagnostic>,
}

/// One tab, as the sync pass needs to see it.
///
/// Taken as a snapshot so the pass borrows the app once rather than holding it
/// across every send. The rope clone is an atomic bump: ropey shares structure,
/// which is what lets the text be turned into a string on the writer thread.
pub(crate) struct DocSnapshot {
    pub path: PathBuf,
    pub revision: u64,
    pub rope: Rope,
    /// What the buffer did since the last pass, so anything held against the
    /// older text can be moved to where it now belongs.
    pub edits: Vec<EditSpan>,
}

/// Every server, every document, and the tally of what has been sent.
#[derive(Default)]
pub(crate) struct Lsp {
    configs: Vec<ServerConfig>,
    /// Parallel to `configs`; `ServerId(i)` indexes both.
    servers: Vec<Server>,
    docs: HashMap<PathBuf, Doc>,
    /// How many of each notification have gone out.
    ///
    /// Bounded by the number of distinct methods, so it cannot grow with the
    /// session. It exists because "the app sent one `didChange` for that burst"
    /// is otherwise only observable from inside the server.
    sent: HashMap<&'static str, usize>,
    sender: Option<crate::run::AppSender>,
    root: PathBuf,
    /// Requests sent and not yet answered.
    ///
    /// **The app remembers what it asked**, which is why `LspEvent::Response`
    /// carries raw JSON rather than a variant per feature. Bounded by what is
    /// in flight, and a superseded entry is removed when it is cancelled.
    pending: Vec<Pending>,
    /// Work a server has said it is doing, in the order it said so.
    ///
    /// **Several, not one.** rust-analyzer runs indexing, fetching and proc
    /// macros at once, and a single slot would make the bar flicker between
    /// them. A `Vec` rather than a map because the order is the point: the bar
    /// shows the first, and the first is the work the user has been waiting on
    /// longest. A `BTreeMap` sorted this by token name, which put "Fetching"
    /// ahead of an "Indexing" that had been running since startup.
    progress: Vec<((ServerId, String), String)>,
}

/// A request waiting for its answer.
pub(crate) struct Pending {
    pub id: RequestId,
    pub server: ServerId,
    pub kind: Ask,
    /// Where the cursor was when the question was asked.
    ///
    /// The generation lesson from M2.7's parses, applied to requests: an answer
    /// that arrives after the cursor moved describes somewhere the user is no
    /// longer asking about, and acting on it is a jump nobody requested.
    pub asked_at: (PathBuf, typ_buffer::Position),
}

/// What was asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ask {
    Definition,
    Hover,
}

impl Lsp {
    pub(crate) fn new(root: &Path) -> Lsp {
        Lsp {
            root: root.to_path_buf(),
            ..Lsp::default()
        }
    }

    pub(crate) fn add(&mut self, config: ServerConfig) {
        self.configs.push(config);
        self.servers.push(Server::NotStarted);
    }

    pub(crate) fn set_sender(&mut self, sender: crate::run::AppSender) {
        self.sender = Some(sender);
    }

    /// How many of a notification have been sent, over the app's whole life.
    pub(crate) fn notifications_of(&self, method: &str) -> usize {
        self.sent.get(method).copied().unwrap_or(0)
    }

    /// The version the server holds for a document, if it holds one.
    pub(crate) fn version(&self, path: &Path) -> Option<i32> {
        self.docs.get(path).map(|doc| doc.version)
    }

    fn tally(&mut self, method: &'static str, sent: bool) {
        if sent {
            *self.sent.entry(method).or_insert(0) += 1;
        }
    }

    /// Which configured server handles this path, starting it if nothing has
    /// yet. `None` when there is no server, or it would not start.
    fn server_for(&mut self, path: &Path) -> Option<ServerId> {
        let index = self.configs.iter().position(|c| c.handles(path))?;
        match self.servers[index] {
            Server::Running(_) => return Some(ServerId(index as u32)),
            Server::Gone => return None,
            Server::NotStarted => {}
        }

        // Nothing to send results to yet: `App::new` builds an app with no
        // channel, and a server whose answers go nowhere is worse than none.
        let sender = self.sender.clone()?;
        let id = ServerId(index as u32);
        let config = &self.configs[index];
        match Client::start(id, &config.command, &config.args, &self.root, sender) {
            Ok(client) => {
                self.servers[index] = Server::Running(Box::new(client));
                Some(id)
            }
            Err(SpawnError::NotFound { command }) => {
                // The ordinary case on most machines, and not something to
                // interrupt anyone over. It goes to the log, which is where the
                // answer will be looked for.
                crate::log_info!("no language server: {command} is not on PATH");
                self.servers[index] = Server::Gone;
                None
            }
            Err(e) => {
                crate::log_warn!("language server: {e}");
                self.servers[index] = Server::Gone;
                None
            }
        }
    }

    fn client(&mut self, id: ServerId) -> Option<&mut Client> {
        match self.servers.get_mut(id.0 as usize)? {
            Server::Running(client) => Some(client),
            _ => None,
        }
    }

    /// Bring every server's idea of the open documents up to date.
    ///
    /// Runs once per batch. Everything it does is derived from the difference
    /// between `docs` and what it is told, so calling it twice with the same
    /// input sends nothing the second time — which is what makes it safe to
    /// call from the end of every pass.
    pub(crate) fn sync(&mut self, docs: &[DocSnapshot]) {
        for doc in docs {
            self.shift(doc);
            self.sync_one(doc);
        }
        self.close_absent(docs);
    }

    /// Move this document's diagnostics through the edits just applied.
    ///
    /// The server described the file as it was some milliseconds ago and the
    /// user has kept typing since. Without this the squiggles sit under the
    /// wrong words until the next publish, which on a slow server is a long
    /// time to look broken.
    fn shift(&mut self, snapshot: &DocSnapshot) {
        if snapshot.edits.is_empty() {
            return;
        }
        let Some(doc) = self.docs.get_mut(&snapshot.path) else {
            return;
        };
        for diagnostic in &mut doc.pushed {
            diagnostic.range = (
                typ_buffer::shift_through(diagnostic.range.0, &snapshot.edits),
                typ_buffer::shift_through(diagnostic.range.1, &snapshot.edits),
            );
        }
    }

    /// Which unit a server counts positions in, if it is running.
    pub(crate) fn encoding(&self, id: ServerId) -> Encoding {
        match self.servers.get(id.0 as usize) {
            Some(Server::Running(client)) => client.encoding(),
            _ => Encoding::Utf16,
        }
    }

    /// The version a server last heard about a document.
    pub(crate) fn synced_version(&self, path: &Path) -> Option<i32> {
        self.docs.get(path).map(|doc| doc.version)
    }

    /// Replace what a server has pushed about a document.
    ///
    /// A publish naming a document nothing has open is dropped: the server was
    /// told `didClose` and this crossed it on the wire, and there is nowhere on
    /// screen for it to go.
    pub(crate) fn set_pushed(&mut self, path: &Path, diagnostics: Vec<Diagnostic>) {
        if let Some(doc) = self.docs.get_mut(path) {
            doc.pushed = diagnostics;
        }
    }

    /// What is known about a document, for the frame that draws it.
    pub(crate) fn diagnostics(&self, path: &Path) -> &[Diagnostic] {
        self.docs.get(path).map_or(&[], |doc| &doc.pushed)
    }

    fn sync_one(&mut self, snapshot: &DocSnapshot) {
        let Some(id) = self.server_for(&snapshot.path) else {
            return;
        };
        let language_id = self.configs[id.0 as usize].language_id.clone();

        match self.docs.get(&snapshot.path) {
            // Known and current. The common case, and it sends nothing.
            Some(doc) if doc.synced == Some(snapshot.revision) => {}
            Some(doc) => {
                let (uri, version) = (doc.uri.clone(), doc.version + 1);
                let Some(client) = self.client(id) else {
                    return;
                };
                let sent = client.did_change(&uri, version, snapshot.rope.clone());
                self.tally("textDocument/didChange", sent);
                if let Some(doc) = self.docs.get_mut(&snapshot.path) {
                    doc.version = version;
                    doc.synced = Some(snapshot.revision);
                }
            }
            None => {
                // A path with no URI is a path no server can be told about.
                // Absolute paths always have one; this is the guard for the
                // ones that are not, rather than a case that is expected.
                let Some(uri) = typ_lsp::path_to_uri(&snapshot.path) else {
                    return;
                };
                let uri = uri.as_str().to_string();
                let text = snapshot.rope.to_string();
                let Some(client) = self.client(id) else {
                    return;
                };
                if !client.did_open(&uri, &language_id, 0, text) {
                    // Not initialized yet. Nothing is recorded, so the next
                    // pass tries again — which is how a document opened before
                    // the handshake finishes gets announced when it does.
                    return;
                }
                self.tally("textDocument/didOpen", true);
                self.docs.insert(
                    snapshot.path.clone(),
                    Doc {
                        server: id,
                        uri,
                        version: 0,
                        synced: Some(snapshot.revision),
                        pushed: Vec::new(),
                    },
                );
            }
        }
    }

    /// Tell servers about documents that are no longer open in any tab.
    fn close_absent(&mut self, docs: &[DocSnapshot]) {
        let gone: Vec<PathBuf> = self
            .docs
            .iter()
            .filter(|(path, doc)| doc.synced.is_some() && !docs.iter().any(|d| &&d.path == path))
            .map(|(path, _)| path.clone())
            .collect();

        for path in gone {
            let Some(doc) = self.docs.get_mut(&path) else {
                continue;
            };
            doc.synced = None;
            // A file that is not open has nowhere to show a diagnostic, and
            // keeping them would mean reopening it showed a stale set before
            // the server had said anything.
            doc.pushed.clear();
            let (id, uri) = (doc.server, doc.uri.clone());
            let Some(client) = self.client(id) else {
                continue;
            };
            let sent = client.did_close(&uri);
            self.tally("textDocument/didClose", sent);
        }
    }

    /// The file was written to disk.
    ///
    /// Event-driven rather than reconciled: a save leaves no trace in the
    /// buffer for a later pass to notice, and it is the notification
    /// rust-analyzer's `cargo check` output depends on entirely.
    pub(crate) fn did_save(&mut self, path: &Path, rope: Rope) {
        let Some(doc) = self.docs.get(path) else {
            return;
        };
        let (id, uri) = (doc.server, doc.uri.clone());
        let Some(client) = self.client(id) else {
            return;
        };
        let sent = client.did_save(&uri, rope);
        self.tally("textDocument/didSave", sent);
    }

    /// Sort one thing a server said, answering anything that is ours to answer.
    ///
    /// Returns what the app still has to deal with. A server request answered
    /// here is one the app never sees, and answering is not optional: a server
    /// waiting on `workspace/configuration` waits forever.
    pub(crate) fn handle(&mut self, incoming: Incoming) -> Option<(ServerId, LspEvent)> {
        let id = incoming.server();
        let event = self.client(id)?.handle(incoming)?;

        match event {
            LspEvent::Exited => {
                self.servers[id.0 as usize] = Server::Gone;
                // Its documents were never closed politely, and there is nobody
                // left to tell. Forgetting them is what lets a restart announce
                // them again.
                self.docs.retain(|_, doc| doc.server != id);
                self.forget_progress(id);
                Some((id, LspEvent::Exited))
            }
            LspEvent::ServerRequest {
                id: req,
                method,
                params,
            } => {
                match method.as_str() {
                    // One null per item asked for. TYPE has no per-server
                    // settings yet, and a null is "use your default" — which is
                    // an answer, where silence is a hang.
                    "workspace/configuration" => {
                        let count = params
                            .get("items")
                            .and_then(|i| i.as_array())
                            .map_or(0, |i| i.len());
                        let nulls = vec![serde_json::Value::Null; count];
                        if let Some(client) = self.client(id) {
                            client.respond(req, serde_json::Value::Array(nulls));
                        }
                        None
                    }
                    _ => Some((
                        id,
                        LspEvent::ServerRequest {
                            id: req,
                            method,
                            params,
                        },
                    )),
                }
            }
            other => Some((id, other)),
        }
    }

    /// Ask the server for `path` a question about a position.
    ///
    /// Returns nothing when there is no server, when it has not finished the
    /// handshake, or when it says it cannot answer: all three are ordinary, and
    /// all three have to leave the editor exactly as it was.
    pub(crate) fn ask(
        &mut self,
        kind: Ask,
        path: &Path,
        char_index: usize,
        rope: &Rope,
    ) -> Result<(), NoAnswer> {
        let Some(doc) = self.docs.get(path) else {
            return Err(NoAnswer::NoServer);
        };
        let (id, uri) = (doc.server, doc.uri.clone());
        let encoding = self.encoding(id);
        let capability = match kind {
            Ask::Definition => "definitionProvider",
            Ask::Hover => "hoverProvider",
        };
        let method = match kind {
            Ask::Definition => "textDocument/definition",
            Ask::Hover => "textDocument/hover",
        };

        let Some(client) = self.client(id) else {
            return Err(NoAnswer::NoServer);
        };
        if !client.is_initialized() {
            return Err(NoAnswer::NotReady);
        }
        if !client.supports(capability) {
            return Err(NoAnswer::Unsupported);
        }

        let position = typ_lsp::to_lsp(encoding, rope.slice(..), char_index);
        let request = client.request(
            method,
            serde_json::json!({
                "textDocument": { "uri": uri },
                "position": { "line": position.line, "character": position.character },
            }),
        );
        self.pending.push(Pending {
            id: request,
            server: id,
            kind,
            asked_at: (path.to_path_buf(), typ_buffer::Position::default()),
        });
        Ok(())
    }

    /// Record where the cursor was for the request just sent.
    pub(crate) fn stamp_last(&mut self, at: typ_buffer::Position) {
        if let Some(last) = self.pending.last_mut() {
            last.asked_at.1 = at;
        }
    }

    /// Abandon any question of this kind that is still unanswered.
    ///
    /// Hover fires on demand today and on cursor movement later, so most
    /// questions are superseded before they are answered. A server still
    /// grinding on one nobody wants is burning a core for nothing.
    pub(crate) fn cancel(&mut self, kind: Ask) {
        let stale: Vec<(ServerId, RequestId)> = self
            .pending
            .iter()
            .filter(|p| p.kind == kind)
            .map(|p| (p.server, p.id.clone()))
            .collect();
        self.pending.retain(|p| p.kind != kind);
        for (server, id) in stale {
            if let Some(client) = self.client(server) {
                client.cancel(&id);
            }
            self.tally("$/cancelRequest", true);
        }
    }

    /// Take the question an answer belongs to, if anyone is still waiting.
    pub(crate) fn take_pending(&mut self, id: &RequestId) -> Option<Pending> {
        let at = self.pending.iter().position(|p| &p.id == id)?;
        Some(self.pending.remove(at))
    }

    /// Send a notification to whichever server owns a document.
    pub(crate) fn notify(&mut self, path: &Path, method: &str, params: serde_json::Value) {
        let Some(doc) = self.docs.get(path) else {
            return;
        };
        let id = doc.server;
        if let Some(client) = self.client(id) {
            client.notify(method, params);
        }
    }

    /// What the servers are busy with, oldest token first.
    pub(crate) fn progress(&self) -> Vec<&str> {
        self.progress
            .iter()
            .map(|(_, text)| text.as_str())
            .collect()
    }

    /// The entry for a token, if it has begun.
    fn progress_slot(&mut self, key: &(ServerId, String)) -> Option<&mut String> {
        self.progress
            .iter_mut()
            .find(|(existing, _)| existing == key)
            .map(|(_, text)| text)
    }

    /// Record a `$/progress` notification. Returns whether the bar changed.
    ///
    /// Three kinds. `begin` names the work, `report` refines it, `end` removes
    /// it — and a `report` for a token nobody began is ignored rather than
    /// invented, because a percentage with no title is a number with no noun.
    pub(crate) fn note_progress(&mut self, server: ServerId, params: &serde_json::Value) -> bool {
        let Some(token) = params.get("token").and_then(token_name) else {
            return false;
        };
        let Some(value) = params.get("value") else {
            return false;
        };
        let key = (server, token);

        match value.get("kind").and_then(|k| k.as_str()) {
            Some("begin") => {
                let title = value
                    .get("title")
                    .and_then(|t| t.as_str())
                    .unwrap_or("Working")
                    .to_string();
                let text = describe(&title, value);
                match self.progress_slot(&key) {
                    Some(existing) => *existing = text,
                    None => self.progress.push((key, text)),
                }
                true
            }
            Some("report") => {
                let Some(existing) = self.progress_slot(&key) else {
                    // A report for a token nobody began is a percentage with no
                    // noun. Inventing a title for it would put a made-up word
                    // on the bar.
                    return false;
                };
                // The title came with `begin` and `report` does not repeat it,
                // so it is carried forward from what is already shown.
                let title = existing
                    .split_whitespace()
                    .next()
                    .unwrap_or("Working")
                    .to_string();
                *existing = describe(&title, value);
                true
            }
            Some("end") => {
                let before = self.progress.len();
                self.progress.retain(|(existing, _)| existing != &key);
                self.progress.len() != before
            }
            _ => false,
        }
    }

    /// Forget everything a server said it was doing.
    pub(crate) fn forget_progress(&mut self, server: ServerId) {
        self.progress.retain(|((id, _), _)| *id != server);
    }

    /// Ask every running server to stop, and wait briefly for it.
    ///
    /// The polite half. Dropping does the rest — the process tree goes with the
    /// editor either way — but rust-analyzer writes state on `shutdown`, and
    /// killing it every time is how a stale lock outlives TYPE.
    pub(crate) fn shutdown(&mut self) {
        for server in &mut self.servers {
            if let Server::Running(client) = server {
                client.shutdown(SHUTDOWN_GRACE);
            }
            *server = Server::Gone;
        }
    }
}

/// How long a server gets to exit on its own before the tree kill takes it.
const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_millis(500);

/// One LSP diagnostic, in TYPE's coordinates.
///
/// Two conversions, in this order and no other: the server's encoding to a char
/// offset, which `typ-lsp` owns because char is ropey's native unit; then the
/// char offset to a grapheme position, which `typ-buffer` owns because that is
/// where grapheme logic lives. A server may legitimately name a position inside
/// a cluster and `TextBuffer::position` snaps down to its start, because there
/// is no `Position` for the middle of one and `Selections` could not hold it.
pub(crate) fn to_diagnostic(
    buffer: &typ_buffer::TextBuffer,
    encoding: Encoding,
    diagnostic: &typ_lsp::lsp_types::Diagnostic,
) -> Diagnostic {
    let rope = buffer.rope().slice(..);
    let at = |pos| buffer.position(typ_lsp::from_lsp(encoding, rope, pos));
    Diagnostic {
        range: (at(diagnostic.range.start), at(diagnostic.range.end)),
        severity: severity(diagnostic.severity),
        message: message(&diagnostic.message),
        source: diagnostic.source.clone(),
    }
}

/// A diagnostic's message, whichever of the two shapes it arrived in.
///
/// 3.18 lets a message be `MarkupContent` rather than a string. Painting the
/// markup's backticks is worse than painting nothing, so the value is taken and
/// the kind is dropped — rendering markdown is Task 12's problem, and it has a
/// box to do it in.
fn message(message: &typ_lsp::lsp_types::Message) -> String {
    match message {
        typ_lsp::lsp_types::Message::String(text) => text.clone(),
        typ_lsp::lsp_types::Message::MarkupContent(markup) => markup.value.clone(),
    }
}

/// The protocol's number as one of TYPE's four.
///
/// **Anything unrecognised is a warning, not a discard.** `DiagnosticSeverity`
/// has a `Custom(u32)` variant for numbers the protocol does not define, and
/// one of those is still the server saying something is wrong with that range.
/// Dropping it loses information to make a match tidy.
fn severity(severity: Option<typ_lsp::lsp_types::DiagnosticSeverity>) -> typ_core::Severity {
    use typ_core::Severity;
    use typ_lsp::lsp_types::DiagnosticSeverity as Lsp;
    match severity {
        Some(Lsp::Error) => Severity::Error,
        Some(Lsp::Information) => Severity::Information,
        Some(Lsp::Hint) => Severity::Hint,
        // `Warning`, an absent severity, and `Custom(n)` for an `n` the
        // protocol does not define.
        _ => Severity::Warning,
    }
}

/// Why a question could not be asked.
///
/// Data rather than an error: every one of these is an ordinary state that the
/// status bar says a sentence about and nothing else happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoAnswer {
    /// Nothing is configured for this file, or it would not start.
    NoServer,
    /// It is up and has not finished the handshake.
    NotReady,
    /// It answered the handshake and does not offer this.
    Unsupported,
}

impl NoAnswer {
    /// What the status bar says.
    pub(crate) fn message(self) -> &'static str {
        match self {
            NoAnswer::NoServer => "No language server for this file.",
            NoAnswer::NotReady => "The language server is still starting.",
            NoAnswer::Unsupported => "This language server does not offer that.",
        }
    }
}

/// The path and position a `textDocument/definition` answer names.
///
/// **Four shapes, all legal.** The protocol allows `Location`, `Location[]`,
/// `LocationLink[]` and null, and a client that handles only the first works
/// against exactly one server. The first entry of a list wins — a definition
/// with several answers is a `goto` that has to pick one, and picking the first
/// is what every editor in the field does before it grows a picker for them.
pub(crate) fn definition_target(
    result: &serde_json::Value,
) -> Option<(PathBuf, typ_lsp::lsp_types::Position)> {
    let one = match result {
        serde_json::Value::Array(items) => items.first()?,
        serde_json::Value::Null => return None,
        object => object,
    };

    // `LocationLink` names the target differently, and a server that returns
    // them returns nothing this would otherwise recognise.
    let (uri, range) = match one.get("targetUri") {
        Some(uri) => (
            uri,
            one.get("targetSelectionRange").or(one.get("targetRange"))?,
        ),
        None => (one.get("uri")?, one.get("range")?),
    };

    let uri: typ_lsp::lsp_types::Uri = uri.as_str()?.parse().ok()?;
    let path = typ_lsp::uri_to_path(&uri)?;
    let start = range.get("start")?;
    Some((
        path,
        typ_lsp::lsp_types::Position {
            line: start.get("line")?.as_u64()? as u32,
            character: start.get("character")?.as_u64()? as u32,
        },
    ))
}

/// The text of a `textDocument/hover` answer, as text.
///
/// **Three shapes here too**, and the markup is flattened rather than rendered:
/// painting the backticks of a fenced block is worse than painting nothing, and
/// a terminal box is not where a markdown renderer belongs. `MarkedString` is
/// deprecated and still what several servers send.
pub(crate) fn hover_text(result: &serde_json::Value) -> Option<String> {
    let contents = result.get("contents")?;
    let text = match contents {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(marked_string)
            .collect::<Vec<_>>()
            .join("\n"),
        object => marked_string(object)?,
    };
    let text = flatten_markdown(&text);
    (!text.is_empty()).then_some(text)
}

/// One `MarkedString` or `MarkupContent`, as its text.
fn marked_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        object => object.get("value")?.as_str().map(str::to_string),
    }
}

/// Markdown as the words in it.
///
/// Deliberately not a renderer. Fences, inline code and emphasis are the three
/// things every server's hover uses and the three whose punctuation reads as
/// noise in a one-line box; anything else markdown can do arrives as its own
/// text, which is the honest floor.
fn flatten_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            continue;
        }
        let line = line.replace("**", "").replace('`', "");
        if line.trim().is_empty() {
            if !out.ends_with('\n') && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out.trim().to_string()
}

/// A progress token, which the protocol allows to be a string or a number.
fn token_name(token: &serde_json::Value) -> Option<String> {
    match token {
        serde_json::Value::String(name) => Some(name.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// One line of progress: the title, and the most specific thing said about it.
///
/// A percentage when there is one, a message otherwise, nothing when the server
/// offered neither — which is the case where a title on its own is the whole
/// truth.
fn describe(title: &str, value: &serde_json::Value) -> String {
    if let Some(percentage) = value.get("percentage").and_then(|p| p.as_u64()) {
        return format!("{title} {percentage}%");
    }
    match value.get("message").and_then(|m| m.as_str()) {
        Some(message) if !message.is_empty() => format!("{title} {message}"),
        _ => title.to_string(),
    }
}
