//! The language servers' half of the app: what they are told about the open
//! tabs, what they say back, and the two questions a key can ask them.
//!
//! A child module of `app` for the reason `search` and `picker` are: it reaches
//! `App`'s private fields without widening them. Split out at M4 because it was
//! the largest block in `app.rs` answering to nothing else there, and the layout
//! tree was about to land in the same file.

use std::path::Path;

use anyhow::Result;

use super::{App, Tab};

/// What one tab's frame carries.
///
/// A free function rather than a method because `App::render` needs the result
/// while borrowing `self.tabs` mutably to draw into it: taking the two fields
/// separately keeps the tab's borrow from outliving the call, which a method on
/// `&self` could not.
pub(super) fn diagnostics_for<'a>(
    lsp: &'a crate::lsp::Lsp,
    tab: &Tab,
) -> &'a [typ_core::Diagnostic] {
    match tab.panel.path() {
        Some(path) => lsp.diagnostics(path),
        None => &[],
    }
}

impl App {
    /// How many pieces of work the servers have said they are doing.
    ///
    /// The bar shows one of them; this is what says a second was not displaced
    /// by the first.
    pub fn progress_count(&self) -> usize {
        self.lsp.progress().len()
    }

    /// Send a bare notification to the active file's server.
    ///
    /// For tests: it lets one end a piece of work at a moment it chooses rather
    /// than racing whatever the server would have done on its own.
    pub fn notify_server_for_test(&mut self, method: &str) {
        if let Some(path) = self.tabs[self.active].panel.path().map(|p| p.to_path_buf()) {
            self.lsp.notify(&path, method, serde_json::Value::Null);
        }
    }

    /// What a server last said about the cursor's position, if anything.
    pub fn hover(&self) -> Option<&str> {
        self.hover.as_deref()
    }

    /// Replace every server configuration, as `config.toml` does at startup.
    pub fn set_language_servers(&mut self, configs: Vec<crate::lsp::ServerConfig>) {
        self.lsp.set_configs(configs);
    }

    /// How many language servers are running.
    pub fn language_servers_running(&self) -> usize {
        self.lsp.running()
    }

    /// Register a language server. Nothing starts until a file needs it.
    ///
    /// Servers are never started on the cold-start path: the 100 ms budget
    /// cannot wait for rust-analyzer, which takes tens of seconds to be useful.
    pub fn add_language_server(&mut self, config: crate::lsp::ServerConfig) {
        self.lsp.add(config);
    }

    /// How many of a notification have gone to language servers this session.
    pub fn lsp_notifications_of(&self, method: &str) -> usize {
        self.lsp.notifications_of(method)
    }

    /// How many of a request have gone to language servers this session.
    pub fn lsp_requests_of(&self, method: &str) -> usize {
        self.lsp.notifications_of(method)
    }

    /// The document version a server holds for a path, if one does.
    pub fn lsp_document_version(&self, path: &Path) -> Option<i32> {
        self.lsp.version(path)
    }

    /// Bring every server's documents up to date with the tabs.
    ///
    /// Called once at the end of a batch rather than once per event: ten keys
    /// in one pass is one `didChange`. Cheap when nothing changed: a revision
    /// comparison per tab and nothing else.
    ///
    /// **Every tab is drained, including the ones no server cares about.**
    /// `take_edits` is what bounds the buffer's record of them, so a tab
    /// skipped here is a `Vec` that grows for the length of the session.
    pub fn sync_language_servers(&mut self) {
        let docs: Vec<crate::lsp::DocSnapshot> = self
            .tabs
            .iter_mut()
            .filter_map(|tab| {
                let edits = tab.panel.take_edits();
                let path = tab.panel.path()?;
                Some(crate::lsp::DocSnapshot {
                    path: path.to_path_buf(),
                    revision: tab.panel.buffer().revision(),
                    rope: tab.panel.buffer().snapshot(),
                    edits,
                })
            })
            .collect();
        self.lsp.sync(&docs);
    }

    /// What the servers have said about the buffer on screen.
    ///
    /// The same call the frame makes, so what a test reads here is what the
    /// panel was handed.
    pub fn diagnostics(&self) -> &[typ_core::Diagnostic] {
        diagnostics_for(&self.lsp, &self.tabs[self.active])
    }

    /// A server said something. Returns whether the screen changed.
    pub fn handle_lsp(&mut self, incoming: typ_lsp::Incoming) -> bool {
        let Some((server, event)) = self.lsp.handle(incoming) else {
            return false;
        };
        match event {
            typ_lsp::LspEvent::Notification { method, params }
                if method == "textDocument/publishDiagnostics" =>
            {
                self.publish_diagnostics(server, params)
            }
            typ_lsp::LspEvent::Notification { method, params } if method == "$/progress" => {
                self.lsp.note_progress(server, &params)
            }
            // `$/progress` and `window/logMessage` land here. Task 13 gives
            // the first one the status bar; until then a server talking about
            // itself changes nothing on screen.
            typ_lsp::LspEvent::Notification { .. } => false,
            typ_lsp::LspEvent::Response { id, result } => self.handle_answer(server, id, result),
            typ_lsp::LspEvent::ServerRequest { .. } => false,
            // **Say why, in the server's own words.** A rustup shim for a
            // component that is not installed is on `PATH`, spawns fine, and
            // dies seconds later with one line on stderr saying exactly that,
            // measured on this machine. Composing a sentence here instead
            // would throw away the only thing that explains it.
            typ_lsp::LspEvent::Exited => {
                if self.lsp.is_stopped(server) {
                    self.status = self.lsp.exit_reason(server);
                }
                true
            }
        }
    }

    /// A server published diagnostics. Returns whether the screen changed.
    ///
    /// Three ways this ends without drawing anything, and all three are
    /// ordinary rather than errors: the payload does not parse, the URI names
    /// no open document, or it describes a version older than the one the
    /// server has already been sent.
    fn publish_diagnostics(
        &mut self,
        server: typ_lsp::ServerId,
        params: serde_json::Value,
    ) -> bool {
        let published =
            match serde_json::from_value::<typ_lsp::lsp_types::PublishDiagnosticsParams>(params) {
                Ok(published) => published,
                Err(e) => {
                    crate::log_warn!(
                        "a publishDiagnostics payload from {} did not parse: {e}",
                        self.lsp.command(server).unwrap_or("a language server")
                    );
                    return false;
                }
            };
        let Some(named) = typ_lsp::uri_to_path(&published.uri) else {
            return false;
        };
        // **The tab's spelling, not the server's.** `tab_for` canonicalises and
        // the document map is keyed by the tab's raw path, so a URI naming the
        // same file differently (a lowercase drive letter, a symlinked
        // directory) found the tab and then missed the map. Gap 114.
        let Some(index) = self.tab_for(&named) else {
            return false;
        };
        let Some(path) = self.tabs[index].panel.path().map(Path::to_path_buf) else {
            return false;
        };

        // Stale. The server was describing the document as it was before an
        // edit it has already been told about, and showing it would replace
        // what is on screen with something older.
        if let (Some(sent), Some(described)) = (self.lsp.synced_version(&path), published.version)
            && described < sent
        {
            return false;
        }

        let encoding = self.lsp.encoding(server);
        let buffer = self.tabs[index].panel.buffer();
        let converted: Vec<typ_core::Diagnostic> = published
            .diagnostics
            .iter()
            .map(|d| crate::lsp::to_diagnostic(buffer, encoding, d))
            .collect();

        self.lsp.set_pushed(&path, converted);
        index == self.active
    }

    /// Ask the active file's server about the cursor's position.
    ///
    /// The one place both requests are made, so cancellation, staleness and the
    /// four ways there is no answer are decided once rather than twice.
    pub(super) fn ask_server(&mut self, kind: crate::lsp::Ask) {
        self.ask_server_again(kind, 0);
    }

    /// The same question, carrying how many times it has already been cancelled.
    ///
    /// Asked from the cursor's position *now* rather than from where the first
    /// attempt was aimed: if the caret moved while the server was cancelling,
    /// the answer the user wants is about where they are.
    fn ask_server_again(&mut self, kind: crate::lsp::Ask, retries: u8) {
        // A second question of the same kind abandons the first. Most are
        // superseded before they are answered.
        self.lsp.cancel(kind);

        let tab = &self.tabs[self.active];
        let Some(path) = tab.panel.path().map(|p| p.to_path_buf()) else {
            self.status = Some(crate::lsp::NoAnswer::NoServer.message().to_string());
            return;
        };
        let cursor = tab.panel.cursor();
        let char_index = tab.panel.buffer().char_index(cursor);
        let rope = tab.panel.buffer().snapshot();

        match self.lsp.ask(kind, &path, char_index, &rope, retries) {
            Ok(()) => self.lsp.stamp_last(cursor),
            Err(reason) => self.status = Some(reason.message().to_string()),
        }
    }

    /// An answer arrived. Returns whether the screen changed.
    fn handle_answer(
        &mut self,
        server: typ_lsp::ServerId,
        id: typ_lsp::RequestId,
        result: Result<serde_json::Value, typ_lsp::ResponseError>,
    ) -> bool {
        // Nobody is waiting: it was cancelled, or its tab closed under it.
        // Keyed on the server too: a `RequestId` is unique within one
        // conversation and not across them. Gap 76.
        let Some(pending) = self.lsp.take_pending(server, &id) else {
            return false;
        };
        let result = match result {
            Ok(result) => result,
            // **Not a refusal.** The server cancelled its own work (its state
            // changed under the request) and the question is still open. Ask
            // it again rather than answering the user with nothing.
            Err(error) if error.code == crate::lsp::CONTENT_MODIFIED => {
                if pending.retries < crate::lsp::MAX_RETRIES {
                    self.ask_server_again(pending.kind, pending.retries + 1);
                }
                return false;
            }
            // Said on the status bar like every neighbouring arm: `TYP_LOG` is
            // unset by default, so a log line alone is a key that did nothing.
            Err(error) => {
                crate::log_warn!(
                    "language server {server:?} refused a request: {} ({})",
                    error.message,
                    error.code
                );
                self.status = Some(format!("Language server: {}", error.message));
                return true;
            }
        };

        // **Stale.** The question was about a position the cursor has left, and
        // a jump to where the user used to be is a jump nobody asked for.
        let moved = self.tabs[self.active].panel.path() != Some(&pending.asked_at.0)
            || self.tabs[self.active].panel.cursor() != pending.asked_at.1;
        if moved {
            return false;
        }

        match pending.kind {
            crate::lsp::Ask::Definition => self.jump_to_definition(pending.server, result),
            crate::lsp::Ask::Hover => {
                self.hover = crate::lsp::hover_text(&result);
                if self.hover.is_none() {
                    self.status = Some(
                        crate::lsp::NoAnswer::Empty(crate::lsp::Ask::Hover)
                            .message()
                            .into(),
                    );
                }
                true
            }
        }
    }

    /// Open where a definition lives and put the cursor on it.
    fn jump_to_definition(&mut self, server: typ_lsp::ServerId, result: serde_json::Value) -> bool {
        let Some((path, position)) = crate::lsp::definition_target(&result) else {
            // A server with nothing to say is the ordinary state for the first
            // minute of any real project, and saying so is the difference
            // between "not yet" and "this key does nothing".
            self.status = Some(
                crate::lsp::NoAnswer::Empty(crate::lsp::Ask::Definition)
                    .message()
                    .into(),
            );
            return true;
        };
        if !path.exists() {
            // The index is older than the tree. Saying so beats opening an
            // empty buffer at a path nothing will ever write.
            self.status = Some(format!("{} is gone.", path.display()));
            return true;
        }

        if !self.open_or_report(&path) {
            return true;
        }

        // Converted against the buffer that is now open, in the encoding of the
        // server that named it: the two conversions of invariant 4, in the
        // order `to_diagnostic` uses them.
        let encoding = self.lsp.encoding(server);
        let buffer = self.tabs[self.active].panel.buffer();
        let char_index = typ_lsp::from_lsp(encoding, buffer.rope().slice(..), position);
        let target = buffer.position(char_index);
        self.tabs[self.active]
            .panel
            .select_range(typ_buffer::Selection::caret(target));
        true
    }

    /// The active buffer was written to disk.
    pub(super) fn notify_saved(&mut self) {
        let tab = &self.tabs[self.active];
        let (Some(path), rope) = (tab.panel.path(), tab.panel.buffer().snapshot()) else {
            return;
        };
        let path = path.to_path_buf();
        self.lsp.did_save(&path, rope);
    }

    /// Ask every server to stop, politely, before the process goes.
    pub fn shutdown_language_servers(&mut self) {
        self.lsp.shutdown();
    }
}
