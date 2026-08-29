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
use std::time::{Duration, Instant};

use ropey::Rope;
use typ_buffer::EditSpan;
use typ_core::Diagnostic;
use typ_lsp::RequestId;
use typ_lsp::{Client, Encoding, Incoming, LspEvent, ServerId, SpawnError};

pub mod config;

pub use config::ServerConfig;

/// One running server, and everything decided about it.
///
/// Keyed by `(command, args, root)` rather than by language: two languages
/// configured to the same binary in the same project are one process, which is
/// what `taplo` for `.toml` beside a Rust project would otherwise duplicate.
struct Server {
    config: usize,
    root: PathBuf,
    client: Option<Box<Client>>,
    /// When this server has exited, oldest first.
    ///
    /// **VS Code's sliding window, and it needs no clock.** The restart is
    /// triggered by the exit itself, so the only thing measured is the span
    /// between the exits already recorded — `DefaultErrorHandler` in
    /// `vscode-languageclient` keeps exactly this list and stops when the
    /// window fills. The plan's "restart once, after a delay" wanted a timer
    /// the event loop does not have, and the delay was never the part that
    /// prevented the loop.
    exits: Vec<Instant>,
    /// Whether it ever finished a handshake.
    ///
    /// A server that dies before one is not installed rather than crashed, and
    /// the two want different words on the status bar.
    ever_ready: bool,
    /// Given up on. Restarting is then a thing the user asks for.
    stopped: bool,
    /// The server's last words, kept because the client is dropped with it.
    last_stderr: Vec<String>,
}

/// How many times a server is restarted before the window is consulted.
///
/// Four, which is `maxRestartCount`'s default in `vscode-languageclient`. The
/// fifth exit inside [`CRASH_WINDOW`] is what stops it.
const MAX_RESTARTS: usize = 4;

/// The span VS Code measures a crash loop over.
const CRASH_WINDOW: Duration = Duration::from_secs(3 * 60);

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
    /// Every server started this session. `ServerId(i)` indexes it, and an
    /// entry is never removed — an id in a pending request has to keep meaning
    /// the same thing after another server starts.
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

    /// Add one server configuration, replacing a default of the same language.
    pub(crate) fn add(&mut self, config: ServerConfig) {
        match self
            .configs
            .iter()
            .position(|c| c.language_id == config.language_id)
        {
            Some(existing) => self.configs[existing] = config,
            None => self.configs.push(config),
        }
    }

    /// Replace the whole set, as `config.toml` does at startup.
    pub(crate) fn set_configs(&mut self, configs: Vec<ServerConfig>) {
        self.configs = configs;
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

    /// Which server handles this path, starting one if nothing has yet.
    ///
    /// `None` when no configuration matches, when the binary would not start,
    /// or when the server has been given up on. All three are ordinary and all
    /// three leave the editor exactly as it was.
    fn server_for(&mut self, path: &Path) -> Option<ServerId> {
        let index = self.configs.iter().position(|c| c.handles(path))?;
        let root = config::root_for(path, &self.configs[index].roots, &self.root);

        // **One process per `(command, args, root)`**, not per language and not
        // per file. Two files in one project share a server; the same project
        // opened twice does not start two.
        // **Stopped counts as present.** The lookup has to see a server that
        // has been given up on, or the next reconciliation pass finds no entry
        // for this root, starts a fresh one, and the crash guard is bypassed by
        // the thing it was guarding. Only a root nothing has ever been started
        // for reaches `start`.
        if let Some(at) = self
            .servers
            .iter()
            .position(|s| s.config == index && s.root == root)
        {
            return (self.servers[at].client.is_some()).then_some(ServerId(at as u32));
        }

        self.start(index, root)
    }

    /// Spawn one, and record what became of it either way.
    fn start(&mut self, config: usize, root: PathBuf) -> Option<ServerId> {
        // Nothing to send results to yet: `App::new` builds an app with no
        // channel, and a server whose answers go nowhere is worse than none.
        let sender = self.sender.clone()?;
        let id = ServerId(self.servers.len() as u32);
        let (command, args) = {
            let c = &self.configs[config];
            (c.command.clone(), c.args.clone())
        };

        let mut server = Server {
            config,
            root: root.clone(),
            client: None,
            exits: Vec::new(),
            ever_ready: false,
            stopped: false,
            last_stderr: Vec::new(),
        };

        match Client::start(id, &command, &args, &root, sender) {
            Ok(client) => server.client = Some(Box::new(client)),
            Err(SpawnError::NotFound { command }) => {
                // The ordinary case on a machine without the toolchain, and not
                // something to interrupt anyone over.
                crate::log_info!("no language server: {command} is not on PATH");
                server.stopped = true;
            }
            Err(e) => {
                crate::log_warn!("language server: {e}");
                server.stopped = true;
            }
        }

        let started = server.client.is_some();
        self.servers.push(server);
        started.then_some(id)
    }

    /// Start a server again after it exited, reusing its slot's history.
    fn restart(&mut self, id: ServerId) -> bool {
        let Some(server) = self.servers.get(id.0 as usize) else {
            return false;
        };
        let (config, root) = (server.config, server.root.clone());
        let Some(sender) = self.sender.clone() else {
            return false;
        };
        let (command, args) = {
            let c = &self.configs[config];
            (c.command.clone(), c.args.clone())
        };

        match Client::start(id, &command, &args, &root, sender) {
            Ok(client) => {
                let server = &mut self.servers[id.0 as usize];
                server.client = Some(Box::new(client));
                server.ever_ready = false;
                // Its documents were dropped when it died, so the next
                // reconciliation pass announces them again. Nothing here has to
                // remember which they were.
                true
            }
            Err(e) => {
                crate::log_warn!("language server would not restart: {e}");
                self.servers[id.0 as usize].stopped = true;
                false
            }
        }
    }

    /// Decide what a server's exit means, and act on it.
    ///
    /// **VS Code's `DefaultErrorHandler`, and it needs no clock.** Restart
    /// while fewer than [`MAX_RESTARTS`] exits are on record; after that, stop
    /// if the whole run of them fits inside [`CRASH_WINDOW`], and otherwise
    /// drop the oldest and restart. A server that dies once an hour is not a
    /// crash loop and is not treated as one.
    fn after_exit(&mut self, id: ServerId) {
        let Some(server) = self.servers.get_mut(id.0 as usize) else {
            return;
        };
        // Taken before the client goes: the drain thread fills the tail as the
        // pipe closes, which is exactly when this runs.
        if let Some(client) = server.client.as_ref() {
            server.last_stderr = client.stderr_tail();
        }
        server.client = None;
        server.exits.push(Instant::now());

        // Never started properly. Restarting a binary that is not there just
        // burns the window — the reason is in its stderr and the app says it.
        if !server.ever_ready {
            server.stopped = true;
            return;
        }

        if server.exits.len() <= MAX_RESTARTS {
            self.restart(id);
            return;
        }

        let span = server.exits[server.exits.len() - 1] - server.exits[0];
        if span <= CRASH_WINDOW {
            server.stopped = true;
        } else {
            server.exits.remove(0);
            self.restart(id);
        }
    }

    /// Why a server that has just exited did, in one line.
    ///
    /// **Its own stderr, not a sentence TYPE composed.** Zed reports a failed
    /// start as the error plus the captured stderr for the same reason: a
    /// rustup shim answering "Unknown binary 'rust-analyzer.exe' in official
    /// toolchain" says more than any wording here could, and the shim being on
    /// `PATH` while the component is not is the ordinary state of a machine
    /// with rustup — measured, not guessed.
    pub(crate) fn exit_reason(&self, id: ServerId) -> Option<String> {
        let server = self.servers.get(id.0 as usize)?;
        let command = self.configs.get(server.config)?.command.clone();
        let last = server
            .last_stderr
            .iter()
            .rev()
            .find(|line| !line.trim().is_empty())
            .cloned();

        Some(match (server.ever_ready, last) {
            (_, Some(line)) => format!("{command}: {line}"),
            (true, None) => format!("{command} exited."),
            (false, None) => format!("{command} did not start."),
        })
    }

    /// Whether a server has been given up on.
    pub(crate) fn is_stopped(&self, id: ServerId) -> bool {
        self.servers.get(id.0 as usize).is_some_and(|s| s.stopped)
    }

    /// How many servers are running.
    pub(crate) fn running(&self) -> usize {
        self.servers.iter().filter(|s| s.client.is_some()).count()
    }

    /// Start a stopped server again because the user asked.
    ///
    /// Helix's `:lsp-restart` and Zed's restart command, which is the half a
    /// crash-loop guard needs: something has to be able to say "I fixed it".
    /// Clears the window, so the count starts again.
    pub(crate) fn restart_all(&mut self) -> usize {
        let ids: Vec<ServerId> = (0..self.servers.len())
            .map(|i| ServerId(i as u32))
            .filter(|id| {
                let s = &self.servers[id.0 as usize];
                s.stopped || s.client.is_none()
            })
            .collect();
        let mut started = 0;
        for id in ids {
            let server = &mut self.servers[id.0 as usize];
            server.stopped = false;
            server.exits.clear();
            if self.restart(id) {
                started += 1;
            }
        }
        started
    }

    /// The `languageId` the server at `id` was configured for.
    fn language_id(&self, id: ServerId) -> Option<String> {
        let server = self.servers.get(id.0 as usize)?;
        Some(self.configs.get(server.config)?.language_id.clone())
    }

    fn client(&mut self, id: ServerId) -> Option<&mut Client> {
        self.servers.get_mut(id.0 as usize)?.client.as_deref_mut()
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
        self.servers
            .get(id.0 as usize)
            .and_then(|s| s.client.as_ref())
            .map_or(Encoding::Utf16, |c| c.encoding())
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
        // **Through the server, not the id.** `ServerId` indexes `servers` and
        // has done since one process per (command, root) replaced one per
        // configuration; indexing `configs` with it read fine and panicked the
        // moment a second server started.
        let Some(language_id) = self.language_id(id) else {
            return;
        };

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
        let event = self.client(id)?.handle(incoming);

        // Recorded on the way past rather than asked for later: once the client
        // is gone, nothing can say whether it ever answered `initialize`, and
        // that is the difference between a binary that is not installed and a
        // server that crashed.
        if let Some(client) = self.client(id)
            && client.is_initialized()
            && let Some(server) = self.servers.get_mut(id.0 as usize)
        {
            server.ever_ready = true;
        }
        let event = event?;

        match event {
            LspEvent::Exited => {
                // Its documents were never closed politely, and there is nobody
                // left to tell. Forgetting them is what lets a restart announce
                // them again — the reconciliation pass sees them missing and
                // sends `didOpen`.
                self.docs.retain(|_, doc| doc.server != id);
                self.forget_progress(id);
                self.after_exit(id);
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
            if let Some(client) = server.client.as_mut() {
                client.shutdown(SHUTDOWN_GRACE);
            }
            server.client = None;
            server.stopped = true;
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
