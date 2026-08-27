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
use typ_lsp::{Client, Incoming, LspEvent, ServerId, SpawnError};

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
            self.sync_one(doc);
        }
        self.close_absent(docs);
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
