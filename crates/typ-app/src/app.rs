use std::path::Path;

/// Search, replace and goto-line. A child module rather than a sibling so it
/// reaches `App`'s private fields without any of them widening to `pub(crate)`
/// — the extraction is meant to shorten this file, not to open it up.
mod door;
mod picker;
mod render;
mod search;
mod tabs;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use typ_buffer::SearchQuery;
use typ_core::{Action, Direction, KeyChord, Keymap, Panel, PanelEvent, ThemeColors};
use typ_find::{FileHit, FindWorker, LineHit};
use typ_panel_editor::EditorPanel;
use typ_panel_editor::render::Whitespace;
use typ_panel_tree::TreePanel;
use typ_picker::Picker;
use typ_registry::Registry;
use typ_syntax::ParseWorker;

use crate::prompt::{Prompt, PromptKind};
use crate::status::{Segment, StatusFacts, segments};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tree,
    Editor,
}

pub struct App {
    tree: TreePanel,
    /// Every open file. **Never empty** — closing the last tab leaves an empty
    /// buffer, which is the state the editor starts in. An empty `tabs` would
    /// make `editor()` return an `Option` and every one of its callers handle a
    /// state that never occurs.
    tabs: Vec<Tab>,
    /// Index into `tabs`. Always valid, for the same reason.
    active: usize,
    registry: Registry,
    keymap: Keymap,
    theme: ThemeColors,
    focus: Focus,
    quit: bool,
    /// Message shown in the status bar until the next keypress.
    status: Option<String>,
    /// A quit was refused because a panel had something to confirm. The next
    /// quit goes through.
    quit_pending: bool,
    /// A close was refused because that tab held unsaved work, and **which
    /// tab** it was. Repeating the close on the same one goes through; anything
    /// else in between abandons it, because a confirmation answered forty
    /// keystrokes later is not an answer.
    ///
    /// The index rather than a bare flag: with a close box on every tab, an
    /// answer about one tab must not discard a different one. That is the trap
    /// the old open guard carried a whole path to avoid.
    close_pending: Option<usize>,
    /// Stamps `Tab::last_used`. Monotonic, so the highest stamp is always the
    /// most recently active tab however many have been opened and closed.
    next_use: u64,
    /// The status-bar prompt, when one is open. It owns the keyboard while it
    /// is.
    prompt: Option<Prompt>,
    /// What F3 repeats.
    last_query: Option<SearchQuery>,
    /// Handed to workers so they can wake the loop. `None` in tests that do not
    /// care, and in `App::new` before `run` wires it up.
    sender: Option<crate::run::AppSender>,
    /// Something changed and the screen does not show it yet.
    ///
    /// Starts true: the first frame has to be painted.
    dirty: bool,
    /// `indent_width` from `config.toml`, if it was set.
    ///
    /// Held here rather than pushed into the editor once, because opening a
    /// file builds a new `EditorPanel` and the setting has to survive that.
    indent_width: Option<usize>,
    /// `whitespace` from `config.toml`. Held here for the same reason as
    /// `indent_width`: opening a file builds a new panel.
    whitespace: Whitespace,
    /// Parses buffers off the render thread. `None` until `set_event_sender`,
    /// because the worker needs somewhere to send results.
    ///
    /// The app owns it, not the panel: a panel returns `PanelEvent`s and never
    /// holds a channel to the app.
    parse_worker: Option<ParseWorker>,
    /// Syntax capture styles, degraded at load like `theme` is.
    ///
    /// Empty until a theme with a `[syntax]` table is loaded, which is a normal
    /// state: an empty table means every scope lookup misses and the buffer
    /// renders in one colour, exactly as it did before this milestone.
    syntax_theme: typ_core::SyntaxTheme,
    /// Walks the project and ranks queries against it, off the render thread.
    ///
    /// `None` until `set_event_sender`, like `parse_worker` and for the same
    /// reason: the worker needs somewhere to send results.
    find_worker: Option<FindWorker>,
    /// The generation of the only filter result worth applying.
    ///
    /// Exact match rather than a floor, the same shape `awaited_generation` uses
    /// for parses and for the same reason: the worker coalesces, so the newest
    /// request always runs and always arrives. Anything else was mid-rank when
    /// the query changed under it.
    awaited_filter: Option<u64>,
    /// The visible page of results. Never the corpus — that lives on the worker.
    find_hits: Vec<FileHit>,
    /// The visible page of project-search results.
    ///
    /// Separate from `find_hits` rather than an enum over the two: the picker
    /// keeps whichever list belongs to the mode it is in, and a single field
    /// would mean a late result from one mode overwriting the other's list.
    grep_hits: Vec<LineHit>,
    /// False when the last search hit its cap.
    grep_complete: bool,
    /// The project root, kept so the picker can index it without re-deriving it.
    root: std::path::PathBuf,
    /// The overlay, when it is up. `None` is the ordinary state.
    ///
    /// Not a `Panel` in the panel list: it floats over the body rather than
    /// tiling beside it, and everything about focus, layout and hit-testing
    /// treats it as "ahead of the others" rather than "one of them".
    picker: Option<Picker>,
    /// Whether a walk has ever been asked for. The corpus survives the picker
    /// closing, so reopening shows the previous list while the re-walk runs.
    index_requested: bool,
    /// Language servers, and what each has been told about the open documents.
    lsp: crate::lsp::Lsp,
    /// What a server said about the position the cursor was on.
    ///
    /// **Dismissed by the next cursor movement**, because it described a
    /// position: a box left standing over a different one is saying something
    /// true about somewhere else.
    hover: Option<String>,
    /// A prefix pressed and waiting for its second key, and the menu of what
    /// that key can be. `None` is the ordinary state.
    menu: Option<crate::menu::Menu>,
}

/// One open file, and the parse state that belongs to it rather than to the app.
///
/// **Both fields were app-global until tabs, and both were wrong for two
/// buffers in a different way.** `parsed_revision` compared a revision across
/// buffers that each start counting at zero, so the second file opened matched
/// the first's number and was never parsed at all. `awaited_generation` was one
/// slot, so requesting a parse forgot the previous request — switch tabs while
/// one is in flight and its result is discarded on arrival, while the revision
/// it optimistically recorded stops anything from ever asking again.
///
/// Keeping them here also removes the question of how to address a tab from a
/// distance. State that lives on the tab moves with it, so closing a tab cannot
/// silently re-point an in-flight request at whichever file shifted into that
/// index.
struct Tab {
    panel: EditorPanel,
    /// A counter stamped every time this tab becomes active.
    ///
    /// Closing activates the highest, which is the tab the user was last
    /// working in. Both mature answers in the field are history-based — VS
    /// Code's `focusRecentEditorAfterClose` defaults to true and Helix walks
    /// its jumplist — and the failure a neighbour rule causes is the one the
    /// picker made common: open a file to check one thing, close it, and land
    /// somewhere unrelated instead of back in the work.
    ///
    /// A stamp rather than a stack of indices, because an index stops naming
    /// the same tab the moment an earlier one is removed.
    last_used: u64,
    /// The buffer revision the last parse was requested for. `None` means the
    /// buffer was replaced and must be parsed regardless.
    parsed_revision: Option<u64>,
    /// The generation of the only parse result this tab will accept.
    ///
    /// Exact match rather than a floor, because the worker coalesces queued
    /// jobs down to the newest: the newest request is always the one that runs
    /// and always arrives. Anything else was mid-parse when the buffer changed.
    awaited_generation: Option<u64>,
    /// The watch on this tab's file. **One per tab, not one for the active
    /// tab:** a single watch that followed the active tab never heard about a
    /// write to any other open file, and switching back showed it stale. Gap
    /// 85. Dropped with the tab, which is what stops the watching.
    watch: Option<typ_buffer::FileWatch>,
    /// The file changed on disk while this tab held unsaved work and was not
    /// on screen. Said on arrival, because a status written while the user was
    /// looking at another file is gone by the time they come back.
    changed_on_disk: bool,
}

impl Tab {
    fn new(panel: EditorPanel) -> Self {
        Tab {
            panel,
            parsed_revision: None,
            awaited_generation: None,
            last_used: 0,
            watch: None,
            changed_on_disk: false,
        }
    }
}

/// What one tab's frame carries.
///
/// A free function rather than a method because `App::render` needs the result
/// while borrowing `self.tabs` mutably to draw into it: taking the two fields
/// separately keeps the tab's borrow from outliving the call, which a method on
/// `&self` could not.
fn diagnostics_for<'a>(lsp: &'a crate::lsp::Lsp, tab: &Tab) -> &'a [typ_core::Diagnostic] {
    match tab.panel.path() {
        Some(path) => lsp.diagnostics(path),
        None => &[],
    }
}

/// Between status segments. Two spaces rather than a glyph separator: a
/// separator needs a colour decision of its own and a Nerd Font question at
/// M6, and whitespace has neither.
/// The two actions whose confirmation is the next press of the same thing.
///
/// Both dispatch paths have to make the same exception, because clearing
/// transient state on the way in erases the answer these two are about to
/// read. It is a function rather than a `matches!` in each of them because it
/// was already wrong in one: the palette cleared the flag it then set, so on a
/// dirty buffer `quit` and `close_tab` could never complete from it: the
/// message just came back forever. Gap 68.
fn answers_its_own_confirmation(action: Action) -> bool {
    matches!(action, Action::Quit | Action::CloseTab)
}

const SEGMENT_GAP: &str = "  ";

/// The prefix `Action::OpenMenu` opens, when no key was pressed to say which.
const DOOR: &str = "ctrl+k";

/// Shown when there is nothing more urgent to say. Discoverability is part of
/// the product: bindings nobody can find are bindings that do not exist.
const HINT: &str = "Tab focus  ·  Enter open  ·  Ctrl+S save  ·  Ctrl+Q quit";

impl App {
    pub fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            tree: TreePanel::new(root)?,
            tabs: vec![Tab::new(EditorPanel::from_str(""))],
            active: 0,
            registry: Registry::with_builtins(),
            keymap: Keymap::default_bindings(),
            theme: ThemeColors::default(),
            focus: Focus::Tree,
            quit: false,
            status: None,
            quit_pending: false,
            close_pending: None,
            next_use: 0,
            prompt: None,
            last_query: None,
            sender: None,
            dirty: true,
            indent_width: None,
            whitespace: Whitespace::default(),
            parse_worker: None,
            syntax_theme: typ_core::SyntaxTheme::default(),
            find_worker: None,
            awaited_filter: None,
            find_hits: Vec::new(),
            grep_hits: Vec::new(),
            grep_complete: true,
            root: root.to_path_buf(),
            picker: None,
            index_requested: false,
            lsp: crate::lsp::Lsp::new(root),
            hover: None,
            menu: None,
        })
    }

    /// Whether the app has been given the channel its workers report through.
    ///
    /// The one thing a test of `App` alone could not check, because a test
    /// always wires it. See `run::wire`.
    pub fn is_wired(&self) -> bool {
        self.sender.is_some() && self.parse_worker.is_some() && self.find_worker.is_some()
    }

    /// Something changed that the screen does not show yet.
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Whether to draw, clearing the flag.
    ///
    /// The loop asks this once per batch rather than once per event, so a burst
    /// of thirty events costs one frame.
    pub fn take_dirty(&mut self) -> bool {
        std::mem::replace(&mut self.dirty, false)
    }

    /// Give the app the channel workers report through.
    ///
    /// Separate from `new` because the channel belongs to the loop, and an app
    /// without one is exactly what most tests want: no watcher thread, no
    /// events arriving from somewhere the test did not ask about.
    pub fn set_event_sender(&mut self, sender: crate::run::AppSender) {
        self.parse_worker = Some(ParseWorker::spawn(sender.clone()));
        self.find_worker = Some(FindWorker::spawn(sender.clone()));
        self.lsp.set_sender(sender.clone());
        self.sender = Some(sender);
        for index in 0..self.tabs.len() {
            self.watch_tab(index);
        }
        // Whatever is already open has never been parsed.
        self.request_parse_if_stale();
    }

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
    /// in one pass is one `didChange`. Cheap when nothing changed — a revision
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
            // dies seconds later with one line on stderr saying exactly that —
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
    fn ask_server(&mut self, kind: crate::lsp::Ask) {
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
    fn notify_saved(&mut self) {
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

    /// Ask for a parse if the buffer has changed since the last request.
    ///
    /// Called from three places, and the third is the one that is easy to miss
    /// reading only "after an edit": a buffer opened, an edit, and M2.4's
    /// external-change reload — which replaces the text without going through
    /// an `Action` at all. Missing that one leaves a file edited outside TYPE
    /// showing the previous version's highlights indefinitely.
    ///
    /// Cheap enough to call on every event loop pass: a `u64` comparison, and
    /// nothing at all for a buffer whose extension has no grammar.
    pub(crate) fn request_parse_if_stale(&mut self) {
        let tab = &mut self.tabs[self.active];
        let Some(language) = tab.panel.language() else {
            return;
        };
        let revision = tab.panel.buffer().revision();
        if tab.parsed_revision == Some(revision) {
            return;
        }
        let Some(worker) = &mut self.parse_worker else {
            return;
        };

        let Some(generation) = worker.request(language, tab.panel.buffer().snapshot()) else {
            // The thread is gone: a panic compiling a grammar's queries is
            // how. Recording a request that was never delivered is what left
            // highlighting dead while `is_wired` still said yes. Gap 91.
            self.parse_worker = None;
            crate::log_error!("the parse worker is gone; syntax highlighting is off");
            self.status = Some("Syntax highlighting stopped: its worker thread died.".into());
            return;
        };
        tab.parsed_revision = Some(revision);
        tab.awaited_generation = Some(generation);
    }

    /// A completed parse arrived. Returns whether the screen changed.
    ///
    /// The generation says which *request* this answers, and every tab records
    /// the one it is waiting for — so the result goes to the tab that asked for
    /// it, which is not always the active one.
    ///
    /// Applying it to a backgrounded tab is not a nicety. That tab already
    /// recorded the revision as requested, so throwing the answer away would
    /// leave the buffer unhighlighted for as long as it stays open: nothing
    /// would ever ask again.
    pub fn handle_parsed(&mut self, parsed: typ_syntax::Parsed) -> bool {
        // **Anything older will never come.** The worker answers in generation
        // order and drops whatever is queued behind the job it is running,
        // which with tabs includes other tabs' requests. A tab still waiting on
        // an older generation is forgotten, so the next pass that finds it
        // active asks again rather than trusting a revision nobody parsed.
        // Gap 111.
        for tab in &mut self.tabs {
            if tab
                .awaited_generation
                .is_some_and(|g| g < parsed.generation)
            {
                tab.awaited_generation = None;
                tab.parsed_revision = None;
            }
        }

        // Not a parse anyone is still waiting for. It describes a buffer that
        // has since been replaced, and its byte offsets index text that is gone.
        let Some(index) = self
            .tabs
            .iter()
            .position(|tab| tab.awaited_generation == Some(parsed.generation))
        else {
            return false;
        };
        let tab = &mut self.tabs[index];
        tab.awaited_generation = None;
        tab.panel.set_syntax(parsed.generation, parsed.syntax);
        // Only a tree for the buffer on screen changes what is painted.
        index == self.active
    }

    /// Watch the file in the tab at `index`, if it has one and nothing is
    /// watching it yet.
    ///
    /// A failure here is not worth interrupting anyone over: the editor keeps
    /// working, it just stops noticing outside writes. It goes to the log,
    /// which is where the answer will be looked for.
    fn watch_tab(&mut self, index: usize) {
        let tab = &mut self.tabs[index];
        if tab.watch.is_some() {
            return;
        }
        let (Some(sender), Some(path)) = (self.sender.clone(), tab.panel.path()) else {
            return;
        };
        let path = path.to_path_buf();
        match typ_buffer::watch_file(&path, move |changed, what| {
            let event = match what {
                typ_buffer::WatchEvent::Changed => typ_core::AppEvent::FileChanged(changed),
                typ_buffer::WatchEvent::Failed(reason) => typ_core::AppEvent::WatchFailed {
                    path: changed,
                    reason,
                },
            };
            let _ = sender.send(event);
        }) {
            Ok(watch) => tab.watch = Some(watch),
            Err(e) => crate::log_warn!("watching {} failed: {e:#}", path.display()),
        }
    }

    /// Say that a tab's file changed under unsaved work.
    fn warn_changed_on_disk(&mut self, index: usize) {
        self.tabs[index].changed_on_disk = false;
        self.status = Some(format!(
            "{} changed on disk. Your unsaved changes are kept; Ctrl+S overwrites it.",
            self.tabs[index].panel.file_name()
        ));
    }

    /// A tab that became active may have been written to while it was not.
    fn warn_if_changed_on_disk(&mut self) {
        if self.tabs[self.active].changed_on_disk {
            self.warn_changed_on_disk(self.active);
        }
    }

    /// The file changed on disk. Three states, one of them automatic.
    ///
    /// Reloading a dirty buffer discards the user's edits; ignoring the change
    /// discards the other writer's on the next save. Only the user knows which
    /// matters, so the only thing to do is say so and touch nothing.
    /// Returns whether anything on screen changed, so the loop can decline to
    /// repaint for a watcher event that turned out to be our own save.
    ///
    /// **Any tab, not only the active one.** A clean background tab reloads in
    /// place; a dirty one is flagged and warns when it is switched to. Gap 85.
    pub fn handle_external_change(&mut self, path: &Path) -> Result<bool> {
        let Some(index) = self
            .tabs
            .iter()
            .position(|tab| tab.panel.path() == Some(path))
        else {
            return Ok(false);
        };
        let on_screen = index == self.active;

        if !path.exists() {
            self.status = Some(format!(
                "{} was deleted on disk. Ctrl+S writes it back.",
                self.tabs[index].panel.file_name()
            ));
            return Ok(true);
        }

        // Covers our own save: the watcher reports the write, and what is on
        // disk is what we have, so there is nothing to do.
        if self.tabs[index].panel.matches_disk() {
            return Ok(false);
        }

        if self.tabs[index].panel.is_dirty() {
            if on_screen {
                self.warn_changed_on_disk(index);
            } else {
                self.tabs[index].changed_on_disk = true;
            }
            return Ok(on_screen);
        }

        // **Not `?`.** `reload` reads through `read_to_string`, so a build step
        // or a `git checkout` that leaves non-UTF-8 bytes at the path fails
        // here, and propagating that ends the process from `step_batch`,
        // discarding every *other* tab's unsaved work on a watcher event rather
        // than on anything the user did. Gap 70.
        if let Err(e) = self.tabs[index].panel.reload() {
            self.status = Some(format!(
                "Could not reload {}: {e:#}",
                self.tabs[index].panel.file_name()
            ));
            return Ok(true);
        }
        // `reload` swaps in a fresh `TextBuffer`, so revisions restart and the
        // comparison in `request_parse_if_stale` would be against a number
        // from a buffer that no longer exists. A background tab is parsed
        // when it is next activated.
        self.tabs[index].parsed_revision = None;
        self.request_parse_if_stale();
        Ok(on_screen)
    }

    /// The watch on a file failed. Say so: until it is re-established an
    /// outside write goes unnoticed, and Ctrl+S would overwrite it.
    pub fn handle_watch_failure(&mut self, path: &Path, reason: &str) -> bool {
        if self.tabs[self.active].panel.path() != Some(path) {
            return false;
        }
        self.status = Some(format!(
            "No longer watching {} for outside changes: {reason}",
            self.tabs[self.active].panel.file_name()
        ));
        true
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// Left half of the status bar: whatever needs saying, else the hint.
    pub fn status_left(&self) -> String {
        // The prompt outranks any message: while it is open it is the only
        // thing the user is looking at.
        // A message beside it is about the prompt (a rejected answer) and
        // is shown after the input rather than written somewhere nothing
        // draws. Gap 126.
        if let Some(prompt) = &self.prompt {
            return match &self.status {
                Some(message) => format!("{} {}  {message}", prompt.label(), prompt.input()),
                None => format!("{} {}", prompt.label(), prompt.input()),
            };
        }
        self.status.clone().unwrap_or_else(|| HINT.to_string())
    }

    /// Right half: what is open, what state it is in, and where the cursor is.
    pub fn status_right(&self) -> String {
        self.status_segments()
            .into_iter()
            .map(|s| s.text)
            .collect::<Vec<_>>()
            .join(SEGMENT_GAP)
    }

    /// The right-hand segments.
    ///
    /// The app assembles the facts today. At M4 this becomes a call to
    /// `Panel::status_segments()` on the focused panel, and because the segment
    /// list and its emphasis rules live in `status.rs` rather than here, that is
    /// a change of source rather than a rewrite of content.
    pub fn status_segments(&self) -> Vec<Segment> {
        let cursor = self.tabs[self.active].panel.cursor();
        let path = self.tabs[self.active].panel.path();
        let file_type = crate::status::file_type_of(path);
        let file_name = self.tabs[self.active].panel.file_name();
        segments(&StatusFacts {
            file_name: &file_name,
            modified: self.tabs[self.active].panel.is_dirty(),
            file_type: file_type.as_deref(),
            line_ending: self.tabs[self.active].panel.line_ending().label(),
            indent_width: self.tabs[self.active].panel.tab_width(),
            selection_count: self.tabs[self.active].panel.selections().len(),
            line: cursor.line,
            col: cursor.col,
            total_lines: self.tabs[self.active].panel.line_count(),
            errors: self.count_diagnostics(typ_core::Severity::Error),
            warnings: self.count_diagnostics(typ_core::Severity::Warning),
            progress: &self.lsp.progress(),
        })
    }

    /// How many diagnostics of one severity the open file has.
    ///
    /// Counted rather than cached: the set changes only when a server publishes
    /// and it is bounded by what is wrong with one file, so a count per frame
    /// is cheaper than the invalidation a cache would need.
    fn count_diagnostics(&self, severity: typ_core::Severity) -> usize {
        self.diagnostics()
            .iter()
            .filter(|d| d.severity == severity)
            .count()
    }

    /// Drop anything that should not outlive the next keypress.
    ///
    /// A pending quit expires with the message that announced it — otherwise a
    /// Ctrl+Q from ten minutes ago silently arms the next one.
    ///
    /// Called once per input event: every keypress but Ctrl+Q, and every mouse
    /// press.
    pub fn clear_transient(&mut self) {
        self.status = None;
        self.quit_pending = false;
        self.close_pending = None;
        // The hover described a position. Any key at all may have left it, and
        // a box standing over somewhere the cursor no longer is says something
        // true about the wrong thing. `handle_chord` calls this before running
        // the action, so `Hover` still gets to set it afterwards.
        self.hover = None;
        // A click or a paste abandons a half-typed sequence rather than
        // leaving its second key to land on whatever comes next.
        self.menu = None;
    }

    /// Run an action by name, the way the command palette does.
    ///
    /// Panel first, then the app — the same order `handle_chord` uses, because
    /// an action reachable one way and not the other is exactly the split the
    /// palette exists to close.
    pub fn apply_named_action(&mut self, action: Action) -> Result<()> {
        if !answers_its_own_confirmation(action) {
            self.clear_transient();
        }
        if let Some(events) = self.focused_mut().apply_action(action) {
            return self.apply(events);
        }
        self.perform_app_action(action);
        Ok(())
    }

    /// Quit, unless a panel has something to confirm first.
    fn request_quit(&mut self) {
        if self.quit_pending {
            self.quit = true;
            return;
        }
        // **Every tab, not the active one.** Quitting closes all of them, so a
        // dirty buffer two tabs over is exactly as unsaved as the one on
        // screen — and it is the one the user has forgotten about.
        let blocker = self
            .tabs
            .iter()
            .find_map(|tab| tab.panel.needs_close_confirmation())
            .or_else(|| self.tree.needs_close_confirmation());
        match blocker {
            Some(message) => {
                self.status = Some(format!(
                    "{message}  Ctrl+Q again to discard, Ctrl+S to save."
                ));
                self.quit_pending = true;
            }
            None => self.quit = true,
        }
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn focused_name(&self) -> &'static str {
        match self.focus {
            Focus::Tree => self.tree.name(),
            Focus::Editor => self.tabs[self.active].panel.name(),
        }
    }

    pub fn editor_title(&self) -> String {
        self.tabs[self.active].panel.title()
    }

    pub fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Tree => Focus::Editor,
            Focus::Editor => Focus::Tree,
        };
    }

    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    /// The configured indent width, or `None` to measure each file.
    pub fn set_indent_width(&mut self, width: Option<usize>) {
        self.indent_width = width;
        self.apply_indent_width();
    }

    fn apply_indent_width(&mut self) {
        if let Some(width) = self.indent_width {
            self.tabs[self.active].panel.set_tab_width(width);
        }
    }

    /// Which whitespace the editor marks.
    pub fn set_whitespace(&mut self, whitespace: Whitespace) {
        self.whitespace = whitespace;
        self.tabs[self.active].panel.set_whitespace(whitespace);
    }

    pub fn set_keymap(&mut self, keymap: Keymap) {
        self.keymap = keymap;
    }

    /// Use a loaded palette instead of the compiled-in default.
    ///
    /// Separate from `new` for the same reason `set_keymap` is: config lives in
    /// files the app has to go and read, and a test that wants a known palette
    /// should not have to arrange a config directory to get one.
    ///
    /// The palette arrives already degraded to the terminal's colour depth. No
    /// panel branches on depth, and none should — see `config::load_theme`.
    pub fn set_theme(&mut self, theme: ThemeColors) {
        self.theme = theme;
    }

    /// The syntax half of the theme, also already degraded.
    ///
    /// Separate setter rather than a second argument to `set_theme` because
    /// `ThemeColors` is `Copy` and this is not; keeping them apart is what
    /// stopped a `BTreeMap` landing inside the palette every render path holds
    /// by value.
    pub fn set_syntax_theme(&mut self, syntax: typ_core::SyntaxTheme) {
        self.syntax_theme = syntax;
    }

    /// Route one keypress.
    ///
    /// Order matters and is deliberate:
    ///
    /// 1. A bound chord becomes an `Action`, and the focused panel gets first
    ///    refusal. `None` means "I do not handle this action", which is a
    ///    different answer from handling it and having nothing to report.
    /// 2. Then the app tries it — focus, quit, save.
    /// 3. Then the panel gets the *raw key*, because a bound chord may still
    ///    mean something to a panel that has no action for it.
    /// 4. Anything unbound and printable is text. A chord carrying Ctrl or Alt
    ///    is never text — that is what stops an unbound Ctrl+J typing a `j`.
    ///
    /// Step 3 is not in the milestone plan and the file tree does not work
    /// without it. The tree navigates on raw `Up`/`Down`/`Enter`/`Left`/`Right`,
    /// and the keymap binds all five to editor actions, so a dispatcher that
    /// stops after step 2 swallows every key the tree needs.
    ///
    /// ponytail: the honest fix is naming the tree's primitives as actions the
    /// way the editor's are — "activate the selected entry" has no name today.
    ///
    /// **This comment used to say that lands with the palette. The palette
    /// landed and it did not.** The two turned out to be independent: the
    /// palette lists whatever is in `Action::ALL`, and the tree's primitives
    /// are not in it, so the palette covers the editor and the app and the
    /// raw-key fallback stays exactly as it was. Naming five tree primitives is
    /// a vocabulary decision with no second consumer asking for it yet, which
    /// is the only thing invariant 2 actually requires. It moves to the
    /// milestone that gives the tree a keymap of its own.
    pub fn handle_chord(&mut self, chord: KeyChord) -> Result<()> {
        // The picker owns the keyboard while it is up, ahead of the prompt and
        // ahead of the keymap — otherwise typing a filename fires every editing
        // action bound to a letter, which edits the buffer behind the overlay.
        if self.picker.is_some() {
            return self.handle_picker_chord(chord);
        }

        // An open prompt owns the keyboard, ahead of everything. Routing
        // through the keymap first would let a chord bound to an editing action
        // fire while the user is typing a search term.
        if self.prompt.is_some() {
            return self.handle_prompt_chord(chord);
        }

        if self.menu.is_some() {
            return self.handle_menu_chord(chord);
        }

        let bound = match self.keymap.resolve(None, &chord) {
            typ_core::Resolved::Matched(action) => Some(action),
            // Nothing runs yet, so nothing transient is cleared: a close
            // armed by `ctrl+k w` has to survive the `ctrl+k` that confirms it.
            typ_core::Resolved::Pending(rows) => {
                self.menu = Some(crate::menu::Menu::new(chord.canonical, rows));
                return Ok(());
            }
            typ_core::Resolved::NotFound => None,
        };

        // Every key retires the current status message and anything it left
        // pending, so a confirmation is answered by the very next keystroke or
        // not at all — except the keys whose confirmation it is. Those two ask
        // "press it again", so clearing on the way in would erase the answer
        // they are about to read.
        //
        // Keyed on the action rather than on the chord: this used to compare
        // `chord.canonical` against the literal `"ctrl+q"`, which meant
        // rebinding quit silently broke its own confirmation.
        if !bound.is_some_and(answers_its_own_confirmation) {
            self.clear_transient();
        }

        if let Some(action) = bound {
            if let Some(events) = self.focused_mut().apply_action(action) {
                return self.apply(events);
            }
            if self.perform_app_action(action) {
                return Ok(());
            }
            let events = self.focused_mut().handle_key(chord);
            return self.apply(events);
        }

        let is_chorded = chord
            .raw
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if let KeyCode::Char(c) = chord.raw.code
            && !is_chorded
            && let Some(events) = self.focused_mut().apply_action(Action::InsertChar(c))
        {
            return self.apply(events);
        }

        // Unbound and not text: the panel may still want it.
        let events = self.focused_mut().handle_key(chord);
        self.apply(events)
    }

    pub fn prompt(&self) -> Option<&Prompt> {
        self.prompt.as_ref()
    }

    /// Text arriving as one bracketed-paste event rather than as N keypresses.
    ///
    /// Without this a paste is delivered a character at a time: one loop pass
    /// and one repaint each, and — worse — any chord inside the pasted text
    /// executes as a command instead of being inserted.
    ///
    /// The text travels through the clipboard register rather than through an
    /// `Action` carrying a `String`. `Action` is `Copy` and the keymap depends
    /// on that; widening it for one variant would touch the keymap, its tests
    /// and every match in the dispatcher. Pasting is still an action, still
    /// reachable from the palette and the vim layer — the payload just goes the
    /// way payloads already go.
    pub fn handle_paste(&mut self, text: String) -> Result<()> {
        self.clear_transient();

        // Same order as `handle_chord`: the overlay is ahead of the prompt.
        if self.picker.is_some() {
            self.paste_into_picker(&text);
            return Ok(());
        }

        // A paste into an open prompt is a search term, not an edit.
        if let Some(prompt) = self.prompt.as_mut() {
            for ch in text.chars().filter(|c| !c.is_control()) {
                prompt.insert_char(ch);
            }
            return Ok(());
        }

        typ_buffer::clipboard::set_register(&text);
        let events = self
            .focused_mut()
            .apply_action(Action::Paste)
            .unwrap_or_default();
        self.apply(events)
    }

    /// Say something in the status bar.
    ///
    /// Startup warnings go here rather than to stderr: stderr is invisible once
    /// the alternate screen is up, so a config complaint printed there is a
    /// complaint nobody reads.
    pub fn notify(&mut self, message: String) {
        self.status = Some(message);
    }

    /// Actions no panel claimed. Returns whether the app handled it.
    ///
    /// The bool is what lets an unclaimed action fall through to the raw key
    /// rather than being silently dropped — `_ => {}` here would look identical
    /// and would be the bug that kills the file tree.
    fn perform_app_action(&mut self, action: Action) -> bool {
        match action {
            Action::FocusNext => self.cycle_focus(),
            Action::OpenFilePicker => self.open_picker(),
            Action::OpenProjectSearch => self.open_search(),
            Action::OpenCommandPalette => self.open_command_palette(),
            Action::OpenMenu => self.open_menu(DOOR),
            Action::Quit => self.request_quit(),
            Action::NextTab => self.next_tab(),
            Action::PrevTab => self.prev_tab(),
            Action::CloseTab => self.request_close_tab(self.active),
            // Counted from one, and a digit past the last open file is a
            // no-op rather than a clamp: landing on the last tab because you
            // pressed Alt+9 with three open is a jump you did not ask for.
            Action::GoToTab(n) => self.activate_tab((n as usize).saturating_sub(1)),
            Action::RestartLanguageServers => {
                let started = self.lsp.restart_all();
                self.status = Some(match started {
                    0 => "No language server to restart.".to_string(),
                    1 => "Restarted the language server.".to_string(),
                    n => format!("Restarted {n} language servers."),
                });
            }
            Action::GotoDefinition => self.ask_server(crate::lsp::Ask::Definition),
            Action::Hover => self.ask_server(crate::lsp::Ask::Hover),
            Action::Save => match self.tabs[self.active].panel.save() {
                Ok(()) => {
                    self.status = Some("Saved.".to_string());
                    // After the atomic save, never before it: a server told
                    // about a write that then failed is holding a document
                    // TYPE does not have.
                    self.notify_saved();
                }
                // A save that fails silently is how work gets lost. The status
                // bar says so and the log keeps the whole error chain, which is
                // the part that is actually diagnosable.
                Err(e) => {
                    crate::log_error!("save failed: {e:#}");
                    self.status = Some(format!("Save failed: {e:#}"));
                }
            },
            Action::GotoLine => self.prompt = Some(Prompt::new(PromptKind::GotoLine)),
            Action::SearchOpen => self.prompt = Some(Prompt::new(PromptKind::Search)),
            Action::ReplaceOpen => {
                let mut prompt = Prompt::new(PromptKind::Search);
                prompt.become_replace_after_needle();
                self.prompt = Some(prompt);
            }
            Action::SearchNext | Action::SearchPrevious => {
                let Some(query) = self.last_query.clone() else {
                    self.status = Some("Nothing to search for yet".to_string());
                    return true;
                };
                let direction = if action == Action::SearchNext {
                    Direction::Forward
                } else {
                    Direction::Backward
                };
                self.jump_to_match(&query, direction);
            }
            _ => return false,
        }
        true
    }

    /// Open a file, putting the reason on the status bar if it will not open.
    ///
    /// Returns whether a tab was opened, and **never an `Err`**. Every caller
    /// is an ordinary gesture (Enter on a tree entry, a picker result, a
    /// goto-definition) and a path that is not UTF-8 is one a user can point
    /// at by accident. Propagating instead ran out through `apply` to
    /// `step_batch` and ended the process, discarding every other tab's unsaved
    /// work because someone pressed Enter on a PNG. Gap 71.
    fn open_or_report(&mut self, path: &Path) -> bool {
        match self.open_path(path) {
            Ok(()) => true,
            Err(e) => {
                self.status = Some(format!("Could not open {}: {e:#}", path.display()));
                false
            }
        }
    }

    /// Process events emitted by panels.
    pub fn apply(&mut self, events: Vec<PanelEvent>) -> Result<()> {
        for event in events {
            match event {
                PanelEvent::OpenFile { path, at } => {
                    let (line, col) = (at.line, at.col);
                    // **The event has carried `line` and `col` since M1 and
                    // nothing read them until M2.8.** Harmless while the only
                    // producer was the file tree, which always means the top of
                    // the file; a project-search result that opens at line 0 has
                    // thrown away the only thing the search found out.
                    if self.open_or_report(&path) && (line > 0 || col > 0) {
                        self.tabs[self.active].panel.goto(line, col);
                    }
                }
                PanelEvent::OpenWith { path, .. } => {
                    self.open_or_report(&path);
                }
                // Redraw happens every loop pass in the walking skeleton.
                PanelEvent::NeedsRedraw => {}
                // Two fixed panels, so these are no-ops until the layout
                // system lands.
                PanelEvent::CloseSelf | PanelEvent::Focus(_) => {}
                PanelEvent::Notify { message, .. } => self.status = Some(message),
                PanelEvent::RunCommand { .. } => {}
            }
        }
        Ok(())
    }

    pub fn tree_mut(&mut self) -> &mut TreePanel {
        &mut self.tree
    }

    pub fn focused_mut(&mut self) -> &mut dyn Panel {
        match self.focus {
            Focus::Tree => &mut self.tree,
            Focus::Editor => &mut self.tabs[self.active].panel,
        }
    }
}
