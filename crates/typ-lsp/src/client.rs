//! One server, from `initialize` to `exit`.
//!
//! **The client does not own a thread.** `Transport`'s reader thread delivers
//! [`Incoming`] to whatever channel the app gave it, and the app hands each one
//! back to [`Client::handle`] on its own thread. So every field here is touched
//! from one place, there is no lock, and the ordering questions that make
//! protocol clients hard do not arise. It is the same shape as `handle_parsed`.

use std::path::Path;
use std::sync::mpsc::Sender;

use lsp_server::{Message, Notification, Request, RequestId, Response, ResponseError};

use crate::position::Encoding;
use crate::transport::{Incoming, ServerId, SpawnError, Transport};

/// Everything a server says, once it has been sorted into kinds.
///
/// **Four variants, and it stays at four.** A response carries its payload as
/// raw JSON because the app remembers what it asked and can deserialise into
/// the right type; a notification carries its method because the app knows
/// which ones it cares about. Growing a variant per LSP feature is how this
/// type would become the chokepoint `PanelEvent` is deliberately not.
#[derive(Debug)]
pub enum LspEvent {
    /// A server-initiated notification: `textDocument/publishDiagnostics`,
    /// `$/progress`, `window/logMessage`.
    Notification {
        method: String,
        params: serde_json::Value,
    },
    /// An answer to something we asked.
    Response {
        id: RequestId,
        result: Result<serde_json::Value, ResponseError>,
    },
    /// A request *from* the server, which must be answered.
    ///
    /// The half that gets forgotten. rust-analyzer sends
    /// `workspace/configuration`, `client/registerCapability` and
    /// `window/workDoneProgress/create`, and a client that never answers
    /// leaves it waiting forever.
    ServerRequest {
        id: RequestId,
        method: String,
        params: serde_json::Value,
    },
    /// The server is gone. Always last, exactly once.
    Exited,
}

/// The encodings TYPE offers, best first.
///
/// UTF-32 is ropey's char index unchanged, so it costs nothing. UTF-8 is one
/// `char_to_byte`. UTF-16 is last because it is mandatory rather than good —
/// and it is also the only one most servers implement, so it is the path that
/// has to be right as well as the path that is least wanted.
const PREFERRED: [Encoding; 3] = [Encoding::Utf32, Encoding::Utf8, Encoding::Utf16];

/// How much of a document the server wants on each change.
///
/// TYPE's own enum rather than a number, so a server that says nothing and a
/// server that says none are the same branch and adding incremental sync is a
/// compiler error at every match rather than a silent third case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncKind {
    /// Send nothing. `didChange` is not just unnecessary, it is unwanted.
    None,
    /// The whole document, every time. What TYPE asks for and what it sends.
    Full,
    /// Ranges. TYPE does not offer it, so a server asking for it gets full
    /// documents — which is legal, and the reason the client capability says
    /// `FULL`.
    Incremental,
}

/// A language server and the state of our conversation with it.
pub struct Client {
    id: ServerId,
    transport: Transport,
    next_id: i32,
    encoding: Encoding,
    capabilities: Option<serde_json::Value>,
    /// The id of the `initialize` we are waiting on, if we still are.
    awaiting_initialize: Option<RequestId>,
}

impl Client {
    /// Start the server and send `initialize`. Returns immediately.
    ///
    /// The handshake completes later, when the app feeds the response back
    /// through [`handle`](Self::handle). Nothing here waits, because the 100 ms
    /// cold-start budget cannot afford to and rust-analyzer takes seconds.
    pub fn start<E>(
        id: ServerId,
        command: &str,
        args: &[String],
        root: &Path,
        results: Sender<E>,
    ) -> Result<Client, SpawnError>
    where
        E: From<Incoming> + Send + 'static,
    {
        let transport = Transport::spawn(id, command, args, root, results)?;
        let mut client = Client {
            id,
            transport,
            next_id: 0,
            // Until the server says otherwise. The specification's default,
            // and the only encoding a server is required to support.
            encoding: Encoding::Utf16,
            capabilities: None,
            awaiting_initialize: None,
        };

        let id = client.request("initialize", initialize_params(root));
        client.awaiting_initialize = Some(id);
        Ok(client)
    }

    /// Sort one thing the server said, applying anything that is ours to apply.
    ///
    /// Returns `None` when the message was entirely internal — the
    /// `initialize` response is the only such case, and swallowing it is what
    /// keeps capability negotiation out of the app.
    pub fn handle(&mut self, incoming: Incoming) -> Option<LspEvent> {
        let message = match incoming {
            Incoming::Closed(_) => return Some(LspEvent::Exited),
            Incoming::Message(_, message) => *message,
        };

        match message {
            Message::Response(Response {
                id,
                response_result,
            }) => {
                if self.awaiting_initialize.as_ref() == Some(&id) {
                    self.awaiting_initialize = None;
                    self.finish_initialize(response_result.ok());
                    return None;
                }
                Some(LspEvent::Response {
                    id,
                    result: response_result,
                })
            }
            Message::Notification(Notification { method, params }) => {
                Some(LspEvent::Notification { method, params })
            }
            Message::Request(Request { id, method, params }) => {
                Some(LspEvent::ServerRequest { id, method, params })
            }
        }
    }

    fn finish_initialize(&mut self, result: Option<serde_json::Value>) {
        let Some(result) = result else {
            // A server that refuses to initialize is a server that will not be
            // used. It stays at the default encoding and advertises nothing,
            // so every capability check answers no.
            return;
        };

        if let Some(named) = result.get("positionEncoding").and_then(|e| e.as_str())
            && let Some(encoding) = Encoding::from_wire(named)
        {
            self.encoding = encoding;
        }
        self.capabilities = result.get("capabilities").cloned();

        self.notify("initialized", serde_json::json!({}));
    }

    /// Which server this is.
    pub fn id(&self) -> ServerId {
        self.id
    }

    /// Which unit this server counts `Position::character` in.
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// Whether the handshake has completed.
    pub fn is_initialized(&self) -> bool {
        self.awaiting_initialize.is_none()
    }

    /// What the server said it can do, as it said it.
    ///
    /// Raw rather than typed for the same reason responses are: the app asks
    /// about the one capability it is about to use, and a typed struct would
    /// have to be complete to be useful.
    pub fn capabilities(&self) -> Option<&serde_json::Value> {
        self.capabilities.as_ref()
    }

    /// Whether the server advertised a named capability at the top level.
    pub fn supports(&self, capability: &str) -> bool {
        self.capabilities
            .as_ref()
            .and_then(|caps| caps.get(capability))
            .is_some_and(|value| !value.is_null() && value != &serde_json::Value::Bool(false))
    }

    /// How much of a document this server wants on each change.
    ///
    /// **A server that says nothing is treated as wanting the whole document.**
    /// The specification's own default for an absent `textDocumentSync` is to
    /// send nothing, and that is the wrong way round for a client: a server
    /// that genuinely wants no sync says so, while a server that forgot to
    /// declare it and gets nothing is a server with no documents and no way to
    /// answer anything. Sending to the first costs a notification it ignores;
    /// not sending to the second breaks it completely.
    pub fn sync_kind(&self) -> SyncKind {
        let Some(sync) = self
            .capabilities
            .as_ref()
            .and_then(|c| c.get("textDocumentSync"))
        else {
            return SyncKind::Full;
        };
        let kind = match sync {
            // The short form: the number *is* the kind.
            serde_json::Value::Number(n) => n.as_i64(),
            // The long form. An object with no `change` means no changes.
            _ => Some(sync.get("change").and_then(|c| c.as_i64()).unwrap_or(0)),
        };
        match kind {
            Some(1) => SyncKind::Full,
            Some(2) => SyncKind::Incremental,
            _ => SyncKind::None,
        }
    }

    /// Whether this server wants to be told about documents opening and
    /// closing. Absent means yes, for the reason [`sync_kind`](Self::sync_kind)
    /// gives.
    pub fn wants_open_close(&self) -> bool {
        let Some(sync) = self
            .capabilities
            .as_ref()
            .and_then(|c| c.get("textDocumentSync"))
        else {
            return true;
        };
        match sync {
            serde_json::Value::Number(n) => n.as_i64() != Some(0),
            _ => sync
                .get("openClose")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        }
    }

    /// Whether this server wants `didSave`, and whether it wants the text with
    /// it.
    ///
    /// `save` is either a bool or a `SaveOptions`, and the text goes only to a
    /// server that asked for it — it is the whole document, and sending it
    /// unasked doubles the cost of every save for nothing.
    fn save_wanted(&self) -> Option<bool> {
        let sync = self.capabilities.as_ref()?.get("textDocumentSync")?;
        match sync {
            // The short form cannot express save options. A server that syncs
            // at all is told about saves.
            serde_json::Value::Number(n) => (n.as_i64() != Some(0)).then_some(false),
            _ => match sync.get("save")? {
                serde_json::Value::Bool(false) => None,
                serde_json::Value::Bool(true) => Some(false),
                options => Some(
                    options
                        .get("includeText")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                ),
            },
        }
    }

    /// Tell the server about a document it has not seen. Whether it was sent.
    pub fn did_open(&mut self, uri: &str, language_id: &str, version: i32, text: String) -> bool {
        if !self.is_initialized() || !self.wants_open_close() {
            return false;
        }
        self.notify(
            "textDocument/didOpen",
            serde_json::json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": language_id,
                    "version": version,
                    "text": text,
                },
            }),
        );
        true
    }

    /// Send the whole document. Whether it was sent.
    ///
    /// **The rope is not read here.** It is a snapshot — cloning one is an
    /// atomic bump, not a copy — and the writer thread turns it into text, so
    /// the 1.3 ms a 50k-line file costs is paid off the render thread. A server
    /// that asked for incremental sync still gets the whole document, which is
    /// legal and is why the client capability says `FULL`.
    pub fn did_change(&mut self, uri: &str, version: i32, rope: ropey::Rope) -> bool {
        if !self.is_initialized() || self.sync_kind() == SyncKind::None {
            return false;
        }
        let uri = uri.to_string();
        self.transport.send_deferred(move || {
            Message::Notification(Notification {
                method: "textDocument/didChange".to_string(),
                params: serde_json::json!({
                    "textDocument": { "uri": uri, "version": version },
                    "contentChanges": [ { "text": rope.to_string() } ],
                }),
            })
        });
        true
    }

    /// Tell the server the document was written to disk. Whether it was sent.
    ///
    /// rust-analyzer runs `cargo check` here, and everything that arrives by
    /// `publishDiagnostics` arrives because of this notification.
    pub fn did_save(&mut self, uri: &str, rope: ropey::Rope) -> bool {
        if !self.is_initialized() {
            return false;
        }
        let Some(include_text) = self.save_wanted() else {
            return false;
        };
        let uri = uri.to_string();
        self.transport.send_deferred(move || {
            let mut params = serde_json::json!({ "textDocument": { "uri": uri } });
            if include_text {
                params["text"] = serde_json::Value::String(rope.to_string());
            }
            Message::Notification(Notification {
                method: "textDocument/didSave".to_string(),
                params,
            })
        });
        true
    }

    /// The document is gone; the server owns its own copy again. Whether it was
    /// sent.
    pub fn did_close(&mut self, uri: &str) -> bool {
        if !self.is_initialized() || !self.wants_open_close() {
            return false;
        }
        self.notify(
            "textDocument/didClose",
            serde_json::json!({ "textDocument": { "uri": uri } }),
        );
        true
    }

    /// Send a request and return the id its answer will carry.
    pub fn request(&mut self, method: &str, params: serde_json::Value) -> RequestId {
        self.next_id += 1;
        let id = RequestId::from(self.next_id);
        self.transport.send(Message::Request(Request {
            id: id.clone(),
            method: method.to_string(),
            params,
        }));
        id
    }

    /// Send a notification. Nothing comes back, by definition.
    pub fn notify(&mut self, method: &str, params: serde_json::Value) {
        self.transport.send(Message::Notification(Notification {
            method: method.to_string(),
            params,
        }));
    }

    /// Answer a request the server made.
    pub fn respond(&mut self, id: RequestId, result: serde_json::Value) {
        self.transport.send(Message::Response(Response {
            id,
            response_result: Ok(result),
        }));
    }

    /// Tell the server we could not do what it asked.
    pub fn respond_error(&mut self, id: RequestId, code: i32, message: &str) {
        self.transport.send(Message::Response(Response {
            id,
            response_result: Err(ResponseError {
                code,
                message: message.to_string(),
                data: None,
            }),
        }));
    }

    /// Ask a request we sent to be abandoned.
    ///
    /// Hover fires on cursor movement and completion on every keystroke, so
    /// most requests are superseded before they are answered. A server still
    /// grinding on one nobody wants is burning a core for nothing.
    pub fn cancel(&mut self, id: &RequestId) {
        self.notify(
            "$/cancelRequest",
            serde_json::json!({ "id": id_to_json(id) }),
        );
    }

    /// The last lines the server wrote to stderr.
    pub fn stderr_tail(&self) -> Vec<String> {
        self.transport.stderr_tail()
    }

    /// Ask the server to stop, and wait briefly for it to.
    ///
    /// `shutdown` then `exit` is the sequence the specification asks for, and
    /// rust-analyzer writes state on it. Dropping without this still works —
    /// the process tree is killed — but it is the difference between closing an
    /// editor and pulling its plug.
    pub fn shutdown(&mut self, within: std::time::Duration) {
        self.request("shutdown", serde_json::Value::Null);
        self.notify("exit", serde_json::Value::Null);
        self.transport.close_input();
        self.transport.wait_for_exit(within);
    }
}

/// `RequestId` has no public accessor, and `$/cancelRequest` needs the value.
fn id_to_json(id: &RequestId) -> serde_json::Value {
    serde_json::to_value(id).unwrap_or(serde_json::Value::Null)
}

/// What TYPE tells a server about itself.
///
/// Two lines here are decisions rather than boilerplate:
///
/// `general.positionEncodings` is how UTF-16 stops being the only option. It
/// is a list in preference order and the server picks one.
///
/// `didChangeWatchedFiles.dynamicRegistration` is **false**, deliberately. A
/// server can only register for that notification through
/// `client/registerCapability`; claiming support and then not handling the
/// registration is the failure mode. False makes rust-analyzer watch files
/// itself, which works, and matches TYPE watching one file rather than a
/// workspace until M4.
fn initialize_params(root: &Path) -> serde_json::Value {
    let root_uri = crate::uri::path_to_uri(root).map(|u| u.as_str().to_string());

    serde_json::json!({
        "processId": std::process::id(),
        "clientInfo": { "name": "typ", "version": env!("CARGO_PKG_VERSION") },
        "rootUri": root_uri,
        "capabilities": {
            "general": {
                "positionEncodings": PREFERRED.map(|e| e.as_str()),
            },
            "workspace": {
                "didChangeWatchedFiles": { "dynamicRegistration": false },
                "configuration": true,
            },
            "textDocument": {
                "synchronization": {
                    "didSave": true,
                    "dynamicRegistration": false,
                },
                "publishDiagnostics": { "relatedInformation": true },
                "definition": { "dynamicRegistration": false },
                "hover": { "contentFormat": ["markdown", "plaintext"] },
            },
            "window": { "workDoneProgress": true },
        },
    })
}
