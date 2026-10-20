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
use typ_lsp::{Client, Encoding, Incoming, LspEvent, ServerId, SpawnError};

pub mod config;
mod progress;
mod reply;
mod requests;

pub use config::ServerConfig;
pub(crate) use reply::{
    completion_items, definition_target, hover_text, signature_help, to_diagnostic,
};
pub(crate) use requests::{Ask, CONTENT_MODIFIED, MAX_RETRIES, NoAnswer, Pending};

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

/// JSON-RPC `MethodNotFound`. The answer to a server request TYPE does not
/// implement: an answer being the point, since the alternative is silence and
/// the spec has no silence in it.
const METHOD_NOT_FOUND: i32 = -32601;

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

    /// How many of a notification or request have been sent, over the app's
    /// whole life.
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
        let command = self.command(id)?;
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

    /// The command a server was started with: its name, as far as a log line or
    /// a status message is concerned.
    pub(crate) fn command(&self, id: ServerId) -> Option<&str> {
        let server = self.servers.get(id.0 as usize)?;
        Some(&self.configs.get(server.config)?.command)
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

    /// The characters a document's server asks to be asked on, as a
    /// capability's `triggerCharacters` and `retriggerCharacters` name them.
    /// Empty without a server.
    pub(crate) fn triggers(&self, path: &Path, capability: &str) -> Vec<char> {
        let Some(doc) = self.docs.get(path) else {
            return Vec::new();
        };
        let Some(provider) = self
            .servers
            .get(doc.server.0 as usize)
            .and_then(|s| s.client.as_ref())
            .and_then(|c| c.capabilities())
            .and_then(|caps| caps.get(capability))
        else {
            return Vec::new();
        };
        ["triggerCharacters", "retriggerCharacters"]
            .iter()
            .filter_map(|field| provider.get(field)?.as_array())
            .flatten()
            .filter_map(|c| c.as_str()?.chars().next())
            .collect()
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
                // **Closed and opened again is an open.** `close_absent` sends
                // `didClose` and clears `synced` but keeps the `Doc` (the only
                // removal anywhere is on server exit) so a reopened file
                // landed here and was announced with `didChange` for a document
                // the server had closed. rust-analyzer logs that and drops it,
                // and nothing ever re-announces the file, so it has no
                // diagnostics for the rest of the session. Gap 77.
                let reopened = doc.synced.is_none();
                let Some(client) = self.client(id) else {
                    return;
                };
                let sent = if reopened {
                    let opened =
                        client.did_open(&uri, &language_id, version, snapshot.rope.clone());
                    self.tally("textDocument/didOpen", opened);
                    opened
                } else {
                    let changed = client.did_change(&uri, version, snapshot.rope.clone());
                    self.tally("textDocument/didChange", changed);
                    changed
                };
                if !sent {
                    // Not initialized yet. Record nothing, so the next pass
                    // tries again: the same rule the first open follows.
                    return;
                }
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
                let Some(client) = self.client(id) else {
                    return;
                };
                if !client.did_open(&uri, &language_id, 0, snapshot.rope.clone()) {
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
                    // **Accepting is a null result.** TYPE advertises
                    // `window.workDoneProgress`, which is exactly what makes
                    // rust-analyzer send this, and then nothing answered it.
                    // The spec requires a response to every request, so an
                    // unanswered one sits in the server's outgoing queue for
                    // the life of the session, one per progress token.
                    "window/workDoneProgress/create" => {
                        if let Some(client) = self.client(id) {
                            client.respond(req, serde_json::Value::Null);
                        }
                        None
                    }
                    // **Answered, not merely reported.** This used to pass the
                    // request up to `App`, whose arm for it was `=> false`, so
                    // every request TYPE does not implement went unanswered,
                    // and `Client::respond_error` had no callers at all.
                    // `MethodNotFound` is the honest answer for a capability
                    // that was never advertised. Gap 78.
                    _ => {
                        if let Some(client) = self.client(id) {
                            client.respond_error(
                                req,
                                METHOD_NOT_FOUND,
                                &format!("{method} is not supported"),
                            );
                        }
                        None
                    }
                }
            }
            other => Some((id, other)),
        }
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
