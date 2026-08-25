use std::path::Path;

/// Search, replace and goto-line. A child module rather than a sibling so it
/// reaches `App`'s private fields without any of them widening to `pub(crate)`
/// — the extraction is meant to shorten this file, not to open it up.
mod picker;
mod search;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use typ_buffer::SearchQuery;
use typ_core::{
    Action, Direction, KeyChord, Keymap, Panel, PanelEvent, RenderContext, ThemeColors,
};
use typ_find::{FileHit, FindWorker, Found, LineHit};
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
    editor: EditorPanel,
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
    /// An open was refused because the buffer was dirty, and the input event it
    /// was refused on. Repeating the same open on the very next event goes
    /// through; anything else in between abandons it.
    ///
    /// Carries the path because confirming one file must not arm every other
    /// file, and carries the event number because a confirmation the user
    /// answers ten minutes later is not an answer — it is a stale trap that
    /// discards their work. `quit_pending` avoids the same trap by expiring in
    /// `clear_transient`; an open cannot use that mechanism, because the
    /// keypress that repeats the open runs `clear_transient` on its way in and
    /// would erase the very flag it is meant to satisfy.
    open_pending: Option<(std::path::PathBuf, u64)>,
    /// Counts input events, so `open_pending` can be valid for exactly one.
    event_seq: u64,
    /// The status-bar prompt, when one is open. It owns the keyboard while it
    /// is.
    prompt: Option<Prompt>,
    /// What F3 repeats.
    last_query: Option<SearchQuery>,
    /// Handed to workers so they can wake the loop. `None` in tests that do not
    /// care, and in `App::new` before `run` wires it up.
    sender: Option<crate::run::AppSender>,
    /// The watch on the open file. Dropping it stops the watching, so opening
    /// another file replaces this rather than accumulating watches.
    watch: Option<typ_buffer::FileWatch>,
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
    /// The buffer revision the last parse was requested for.
    ///
    /// `None` means the buffer was replaced and must be parsed regardless —
    /// revisions restart at zero with a new `TextBuffer`, so comparing across
    /// one would silently skip the parse of a newly opened file.
    parsed_revision: Option<u64>,
    /// The generation of the only parse result worth applying.
    ///
    /// **The panel cannot make this decision.** `EditorPanel::set_syntax`
    /// discards a result older than the one it holds, which is right within
    /// one buffer and useless across two: `open_path` builds a *new* panel
    /// whose counter starts at zero, while the worker's counter is app-global
    /// and never resets, so a result still in flight for the previous file
    /// arrives above zero and passes that guard — painting the newly opened
    /// file with the previous one's tree.
    ///
    /// Exact match rather than a floor, because only one result is ever wanted:
    /// the worker coalesces queued jobs down to the newest, so the newest
    /// request is always the one that runs and always arrives. Anything else
    /// is a job that was already mid-parse when the buffer changed.
    awaited_generation: Option<u64>,
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
}

/// Between status segments. Two spaces rather than a glyph separator: a
/// separator needs a colour decision of its own and a Nerd Font question at
/// M6, and whitespace has neither.
const SEGMENT_GAP: &str = "  ";

/// Shown when there is nothing more urgent to say. Discoverability is part of
/// the product: bindings nobody can find are bindings that do not exist.
const HINT: &str = "Tab focus  ·  Enter open  ·  Ctrl+S save  ·  Ctrl+Q quit";

impl App {
    pub fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            tree: TreePanel::new(root)?,
            editor: EditorPanel::from_str(""),
            registry: Registry::with_builtins(),
            keymap: Keymap::default_bindings(),
            theme: ThemeColors::default(),
            focus: Focus::Tree,
            quit: false,
            status: None,
            quit_pending: false,
            open_pending: None,
            event_seq: 0,
            prompt: None,
            last_query: None,
            sender: None,
            watch: None,
            dirty: true,
            indent_width: None,
            whitespace: Whitespace::default(),
            parse_worker: None,
            parsed_revision: None,
            awaited_generation: None,
            syntax_theme: typ_core::SyntaxTheme::default(),
            find_worker: None,
            awaited_filter: None,
            find_hits: Vec::new(),
            grep_hits: Vec::new(),
            grep_complete: true,
            root: root.to_path_buf(),
            picker: None,
            index_requested: false,
        })
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
        self.sender = Some(sender);
        self.rewatch();
        // Whatever is already open has never been parsed.
        self.request_parse_if_stale();
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
        let Some(language) = self.editor.language() else {
            return;
        };
        let revision = self.editor.buffer().revision();
        if self.parsed_revision == Some(revision) {
            return;
        }
        let Some(worker) = &mut self.parse_worker else {
            return;
        };

        worker.request(language, self.editor.buffer().snapshot());
        self.parsed_revision = Some(revision);
        self.awaited_generation = Some(worker.generation());
    }

    /// A completed parse arrived.
    ///
    /// One editor panel exists today, so there is no question of which buffer
    /// this belongs to. When tabs arrive in M4 the answer becomes a panel id
    /// on the event rather than a lookup here.
    pub fn handle_parsed(&mut self, parsed: typ_syntax::Parsed) -> bool {
        // A tree for text nobody is looking at any more. Dropping it is not a
        // loss: the buffer that replaced it queued its own parse.
        if self.editor.language().is_none() {
            return false;
        }
        // Not the parse we are waiting for. It describes a buffer that has
        // since been replaced, and its byte offsets index text that is gone.
        if self.awaited_generation != Some(parsed.generation) {
            return false;
        }
        self.editor.set_syntax(parsed.generation, parsed.syntax);
        true
    }

    /// Walk the project and make it the picker's corpus.
    ///
    /// **Never called at startup.** Cold start is budgeted at 100 ms and a
    /// parallel walk of a mid-size projects directory measured 94.7 ms, which
    /// would spend the entire budget on a list nobody has asked to see yet. The
    /// picker calls this when it opens.
    pub fn request_index(&mut self) {
        if let Some(worker) = &mut self.find_worker {
            worker.index(self.root.clone());
        }
    }

    /// Ask for the best `limit` matches, and return the generation to await.
    pub fn request_filter(&mut self, query: String, limit: usize) -> u64 {
        let Some(worker) = &mut self.find_worker else {
            // No sender wired up, which is most tests. Advance the counter
            // anyway so a caller comparing two generations still sees them
            // differ — returning 0 twice would make a staleness test pass by
            // accident.
            self.awaited_filter = Some(self.awaited_filter.unwrap_or(0) + 1);
            return self.awaited_filter.expect("just set");
        };
        let generation = worker.filter(query, limit);
        self.awaited_filter = Some(generation);
        generation
    }

    /// A find result arrived. Returns whether anything changed on screen.
    pub fn handle_found(&mut self, found: Found) -> bool {
        match found {
            // Answers no query, so it carries no generation to check against.
            // Filtering it through the staleness test below would drop every
            // walk that landed while a filter was outstanding.
            Found::Indexed { .. } => true,
            Found::Lines {
                generation,
                hits,
                complete,
            } => {
                if self.awaited_filter != Some(generation) {
                    return false;
                }
                self.grep_hits = hits;
                self.grep_complete = complete;
                self.push_hits_to_picker();
                true
            }
            Found::Files { generation, hits } => {
                if self.awaited_filter != Some(generation) {
                    // A ranking for a query the user has already typed past.
                    return false;
                }
                self.find_hits = hits;
                self.push_hits_to_picker();
                true
            }
        }
    }

    /// The visible page of find results.
    pub fn find_hits(&self) -> &[FileHit] {
        &self.find_hits
    }

    /// The visible page of project-search results.
    pub fn grep_hits(&self) -> &[LineHit] {
        &self.grep_hits
    }

    /// Whether the last project search ran to completion.
    pub fn grep_complete(&self) -> bool {
        self.grep_complete
    }

    /// Watch whatever file is open now, and stop watching the last one.
    ///
    /// A failure here is not worth interrupting anyone over: the editor keeps
    /// working, it just stops noticing outside writes. It goes to the log,
    /// which is where the answer will be looked for.
    fn rewatch(&mut self) {
        self.watch = None;
        let (Some(sender), Some(path)) = (self.sender.clone(), self.editor.path()) else {
            return;
        };
        let path = path.to_path_buf();
        match typ_buffer::watch_file(&path, move |changed| {
            let _ = sender.send(typ_core::AppEvent::FileChanged(changed));
        }) {
            Ok(watch) => self.watch = Some(watch),
            Err(e) => crate::log_warn!("watching {} failed: {e:#}", path.display()),
        }
    }

    /// The file changed on disk. Three states, one of them automatic.
    ///
    /// Reloading a dirty buffer discards the user's edits; ignoring the change
    /// discards the other writer's on the next save. Only the user knows which
    /// matters, so the only thing to do is say so and touch nothing.
    /// Returns whether anything on screen changed, so the loop can decline to
    /// repaint for a watcher event that turned out to be our own save.
    pub fn handle_external_change(&mut self, path: &Path) -> Result<bool> {
        if self.editor.path() != Some(path) {
            return Ok(false);
        }

        if !path.exists() {
            self.status = Some(format!(
                "{} was deleted on disk. Ctrl+S writes it back.",
                self.editor.file_name()
            ));
            return Ok(true);
        }

        // Covers our own save: the watcher reports the write, and what is on
        // disk is what we have, so there is nothing to do.
        if self.editor.matches_disk() {
            return Ok(false);
        }

        if self.editor.is_dirty() {
            self.status = Some(format!(
                "{} changed on disk. Your unsaved changes are kept; Ctrl+S overwrites it.",
                self.editor.file_name()
            ));
            return Ok(true);
        }

        self.editor.reload()?;
        // `reload` swaps in a fresh `TextBuffer`, so revisions restart and the
        // comparison in `request_parse_if_stale` would be against a number
        // from a buffer that no longer exists.
        self.parsed_revision = None;
        self.request_parse_if_stale();
        Ok(true)
    }

    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// Left half of the status bar: whatever needs saying, else the hint.
    pub fn status_left(&self) -> String {
        // The prompt outranks any message: while it is open it is the only
        // thing the user is looking at.
        if let Some(prompt) = &self.prompt {
            return format!("{} {}", prompt.label(), prompt.input());
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
        let cursor = self.editor.cursor();
        let path = self.editor.path();
        let file_type = crate::status::file_type_of(path);
        let file_name = self.editor.file_name();
        segments(&StatusFacts {
            file_name: &file_name,
            modified: self.editor.is_dirty(),
            file_type: file_type.as_deref(),
            line_ending: self.editor.line_ending().label(),
            indent_width: self.editor.tab_width(),
            selection_count: self.editor.selections().len(),
            line: cursor.line,
            col: cursor.col,
            total_lines: self.editor.line_count(),
        })
    }

    /// Drop anything that should not outlive the next keypress.
    ///
    /// A pending quit expires with the message that announced it — otherwise a
    /// Ctrl+Q from ten minutes ago silently arms the next one.
    ///
    /// Called once per input event — every keypress but Ctrl+Q, and every mouse
    /// press — which is what makes `event_seq` a count of input events and lets
    /// a pending open be valid for exactly the next one.
    pub fn clear_transient(&mut self) {
        self.status = None;
        self.quit_pending = false;
        self.event_seq = self.event_seq.wrapping_add(1);
    }

    /// Quit, unless a panel has something to confirm first.
    fn request_quit(&mut self) {
        if self.quit_pending {
            self.quit = true;
            return;
        }
        let blocker = self
            .editor
            .needs_close_confirmation()
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
            Focus::Editor => self.editor.name(),
        }
    }

    pub fn editor_title(&self) -> String {
        self.editor.title()
    }

    pub fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Tree => Focus::Editor,
            Focus::Editor => Focus::Tree,
        };
    }

    /// Open a file, unless doing so would discard unsaved work.
    ///
    /// Until tabs land at M4 an open *replaces* the buffer, so this is the one
    /// path in the editor that can lose work. It asks the same question
    /// `request_quit` asks, through the same `needs_close_confirmation` method,
    /// and takes the same answer: do it again and it goes through.
    ///
    /// M4 turns this into a per-tab guard on *close* rather than on open. The
    /// trigger moves; the question does not.
    pub fn open_path(&mut self, path: &Path) -> Result<()> {
        if let Some(message) = self.editor.needs_close_confirmation() {
            let confirmed = self
                .open_pending
                .as_ref()
                .is_some_and(|(pending, at)| pending == path && self.event_seq <= at + 1);
            if !confirmed {
                self.status = Some(format!("{message}  Open again to discard, Ctrl+S to save."));
                self.open_pending = Some((path.to_path_buf(), self.event_seq));
                return Ok(());
            }
        }

        // The registry decides the handler. There is one content panel today,
        // but the lookup runs from day one so adding viewers never touches this.
        let _handler = self.registry.handler_for(path);
        // A path with no file behind it is one to create, not an error. `typ
        // notes.md` and opening a not-yet-existing file from the tree are the
        // same operation, so they take the same branch.
        self.editor = if path.exists() {
            EditorPanel::from_path(path)?
        } else {
            EditorPanel::new_at(path)
        };
        self.apply_indent_width();
        self.editor.set_whitespace(self.whitespace);
        self.focus = Focus::Editor;
        self.open_pending = None;
        self.rewatch();
        // A new buffer counts revisions from zero, so this is an invalidation
        // rather than a comparison.
        self.parsed_revision = None;
        self.request_parse_if_stale();
        Ok(())
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
            self.editor.set_tab_width(width);
        }
    }

    /// Which whitespace the editor marks.
    pub fn set_whitespace(&mut self, whitespace: Whitespace) {
        self.whitespace = whitespace;
        self.editor.set_whitespace(whitespace);
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
    /// That is a command-surface question and it lands with the palette at M4;
    /// until then the raw-key fallback is four lines and invents no vocabulary
    /// that would have to be guessed at now and lived with later.
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

        // Every key except Ctrl+Q retires the current status message and any
        // quit it left pending, so a confirmation is answered by the very next
        // keystroke or not at all.
        if chord.canonical != "ctrl+q" {
            self.clear_transient();
        }

        if let Some(action) = self.keymap.lookup(&chord) {
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
            Action::Quit => self.request_quit(),
            Action::Save => match self.editor.save() {
                Ok(()) => self.status = Some("Saved.".to_string()),
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

    /// Process events emitted by panels.
    pub fn apply(&mut self, events: Vec<PanelEvent>) -> Result<()> {
        for event in events {
            match event {
                PanelEvent::Quit => self.request_quit(),
                PanelEvent::OpenFile { path, line, col } => {
                    self.open_path(&path)?;
                    // **The event has carried `line` and `col` since M1 and
                    // nothing read them until M2.8.** Harmless while the only
                    // producer was the file tree, which always means the top of
                    // the file; a project-search result that opens at line 0 has
                    // thrown away the only thing the search found out.
                    if line > 0 || col > 0 {
                        self.editor.goto(line, col);
                    }
                }
                PanelEvent::OpenWith { path, .. } => {
                    self.open_path(&path)?;
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

    pub fn render(&mut self, frame: &mut ratatui::Frame) {
        let (body, status_area) = crate::layout::split_frame(frame.area());
        let (tree_area, editor_area) = crate::layout::split(body);
        let (w, h) = (frame.area().width, frame.area().height);

        let tree_ctx = RenderContext {
            theme: &self.theme,
            syntax: &self.syntax_theme,
            is_focused: self.focus == Focus::Tree,
            panel_index: 0,
            terminal_width: w,
            terminal_height: h,
        };
        let editor_ctx = RenderContext {
            theme: &self.theme,
            syntax: &self.syntax_theme,
            is_focused: self.focus == Focus::Editor,
            panel_index: 1,
            terminal_width: w,
            terminal_height: h,
        };

        // The focused panel draws last.
        //
        // The two rects share a column — see `layout::split` — so one cell
        // carries both panels' border, and a shared border cannot be two
        // colours. Drawing the focused panel second gives that cell its colour,
        // which is the right answer: the focused panel's box is the complete
        // one, and the unfocused panel is the one that gives ground.
        match self.focus {
            Focus::Editor => {
                self.tree.render(tree_area, frame.buffer_mut(), &tree_ctx);
                self.editor
                    .render(editor_area, frame.buffer_mut(), &editor_ctx);
            }
            Focus::Tree => {
                self.editor
                    .render(editor_area, frame.buffer_mut(), &editor_ctx);
                self.tree.render(tree_area, frame.buffer_mut(), &tree_ctx);
            }
        }

        self.render_status(status_area, frame.buffer_mut());

        // The overlay draws last, over the body — after the status bar too, so a
        // tall picker on a short terminal covers the bar rather than being
        // clipped by it. `chrome::frame` fills every cell of its rect, which is
        // what stops the editor showing through.
        if self.picker.is_some() {
            let area = crate::layout::picker_area(frame.area());
            let ctx = RenderContext {
                theme: &self.theme,
                syntax: &self.syntax_theme,
                // Always focused: it owns the keyboard for as long as it is up,
                // so a dimmed border would be lying about where keys go.
                is_focused: true,
                panel_index: 2,
                terminal_width: w,
                terminal_height: h,
            };
            if let Some(picker) = self.picker.as_mut() {
                picker.render(area, frame.buffer_mut(), &ctx);
            }
            // The overlay has its own text cursor at the end of the query, and
            // the panel underneath must not also claim one.
            return;
        }

        // Only the focused panel gets a cursor, and it is the terminal's real
        // one — set after drawing, so it lands on top of the frame. Panels with
        // nothing to edit return None and the cursor stays hidden.
        let focused_area = match self.focus {
            Focus::Tree => tree_area,
            Focus::Editor => editor_area,
        };
        if let Some((x, y)) = self.focused().cursor_position(focused_area) {
            frame.set_cursor_position((x, y));
        }
    }

    fn render_status(&self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let background = Style::default()
            .fg(self.theme.status_bar_fg)
            .bg(self.theme.status_bar_bg);
        let left = self.status_left();
        let right_segments = self.status_segments();
        let right_width: usize = right_segments
            .iter()
            .map(|s| s.text.chars().count())
            .sum::<usize>()
            + SEGMENT_GAP.len() * right_segments.len().saturating_sub(1);

        // The right half is the fixed cost; the left is truncated to whatever
        // is left over, so a long message never pushes the position off-screen.
        let width = area.width as usize;
        let room = width.saturating_sub(right_width + 2);
        let left: String = left.chars().take(room).collect();
        let gap = width.saturating_sub(left.chars().count() + right_width);

        // Each segment carries its own emphasis. This is where
        // `status_bar_inactive_fg` and `status_bar_accent` earn their place:
        // without them the bar is one weight of text and a reader has to parse
        // it rather than glance at it.
        let mut spans = vec![
            Span::styled(left, background),
            Span::styled(" ".repeat(gap), background),
        ];
        for (index, segment) in right_segments.iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled(SEGMENT_GAP, background));
            }
            spans.push(Span::styled(
                segment.text.clone(),
                background.fg(segment.emphasis.colour(&self.theme)),
            ));
        }

        Paragraph::new(Line::from(spans))
            .style(background)
            .render(area, buf);
    }

    fn focused(&self) -> &dyn Panel {
        match self.focus {
            Focus::Tree => &self.tree,
            Focus::Editor => &self.editor,
        }
    }

    /// Areas for hit-testing mouse events, in the same order as `render`.
    /// Excludes the status bar row, so a click on it hits neither panel.
    pub fn areas(&self, area: Rect) -> (Rect, Rect) {
        let (body, _) = crate::layout::split_frame(area);
        crate::layout::split(body)
    }

    pub fn tree_mut(&mut self) -> &mut TreePanel {
        &mut self.tree
    }

    pub fn editor(&self) -> &EditorPanel {
        &self.editor
    }

    pub fn editor_mut(&mut self) -> &mut EditorPanel {
        &mut self.editor
    }

    pub fn focused_mut(&mut self) -> &mut dyn Panel {
        match self.focus {
            Focus::Tree => &mut self.tree,
            Focus::Editor => &mut self.editor,
        }
    }
}
