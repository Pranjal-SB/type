use std::path::Path;

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
use typ_panel_editor::EditorPanel;
use typ_panel_tree::TreePanel;
use typ_registry::Registry;

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
        self.sender = Some(sender);
        self.rewatch();
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
            indent_width: typ_panel_editor::TAB_WIDTH,
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
        self.focus = Focus::Editor;
        self.open_pending = None;
        self.rewatch();
        Ok(())
    }

    pub fn keymap(&self) -> &Keymap {
        &self.keymap
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

    /// Keys while a prompt is open.
    fn handle_prompt_chord(&mut self, chord: KeyChord) -> Result<()> {
        // Decide first, mutate second. Holding `self.prompt.as_mut()` across an
        // assignment to `self.prompt` does not compile, and threading the
        // borrow through every arm is worse than naming the outcome.
        enum Outcome {
            Stay,
            Close,
            Search(String),
            AskReplacement(String),
            Replace { needle: String, replacement: String },
            Goto(String),
        }

        // A chord is never text, in the prompt exactly as in the buffer —
        // otherwise Ctrl+F while searching types an "f" into the needle.
        let is_chorded = chord
            .raw
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);

        let Some(prompt) = self.prompt.as_mut() else {
            return Ok(());
        };

        let outcome = match chord.raw.code {
            KeyCode::Esc => Outcome::Close,
            KeyCode::Backspace if !is_chorded => {
                prompt.delete_backward();
                Outcome::Stay
            }
            KeyCode::Char(c) if !is_chorded => {
                prompt.insert_char(c);
                Outcome::Stay
            }
            KeyCode::Enter => {
                let input = prompt.take_input();
                match prompt.kind() {
                    // Ctrl+H's first Enter banks the needle and asks the second
                    // question; the prompt stays open across both.
                    PromptKind::Search if prompt.is_replace_flow() => {
                        Outcome::AskReplacement(input)
                    }
                    PromptKind::Search => Outcome::Search(input),
                    PromptKind::Replace => Outcome::Replace {
                        needle: prompt.pending_needle().unwrap_or_default().to_string(),
                        replacement: input,
                    },
                    PromptKind::GotoLine => Outcome::Goto(input),
                }
            }
            _ => Outcome::Stay,
        };

        match outcome {
            Outcome::Stay => {}
            Outcome::Close => self.prompt = None,
            Outcome::Search(needle) => {
                self.prompt = None;
                self.run_search(needle);
            }
            Outcome::AskReplacement(needle) => {
                if let Some(prompt) = self.prompt.as_mut() {
                    prompt.set_pending_needle(needle);
                    prompt.become_replace();
                }
            }
            Outcome::Replace {
                needle,
                replacement,
            } => {
                self.prompt = None;
                self.run_replace_all(&needle, &replacement);
            }
            Outcome::Goto(input) => {
                if input.is_empty() {
                    // Answering nothing is answering "never mind".
                    self.prompt = None;
                } else if let Some(line) = parse_line_number(&input) {
                    self.prompt = None;
                    self.editor.goto_line(line);
                } else {
                    // Rejected, and the prompt stays open with the input still
                    // in it: closing on a typo throws the answer away and makes
                    // the user reopen and retype it.
                    self.status = Some(format!("Not a line number: {input}"));
                    if let Some(prompt) = self.prompt.as_mut() {
                        prompt.restore_input(input);
                    }
                }
            }
        }
        Ok(())
    }

    /// Select the first match at or after the cursor, wrapping.
    fn run_search(&mut self, needle: String) {
        if needle.is_empty() {
            return;
        }
        // Case-insensitive unless the user typed a capital — "smart case",
        // which is what makes a lowercase search find everything without a
        // setting, and a capitalised one mean it.
        let case_sensitive = needle.chars().any(char::is_uppercase);
        let query = SearchQuery::new(needle, case_sensitive);
        self.last_query = Some(query.clone());
        self.jump_to_match(&query, Direction::Forward);
    }

    fn jump_to_match(&mut self, query: &SearchQuery, direction: Direction) {
        let hits = self.editor.buffer_find_all(query);
        if hits.is_empty() {
            self.status = Some(format!("No matches for {}", query.needle));
            return;
        }
        let from = self.editor.cursor();
        let next = match direction {
            // `>=`, not `>`: opening a search with the cursor at the top of the
            // file must find a match that starts there. Jumping leaves the
            // cursor at the match's *end*, so repeating never re-finds the one
            // it is sitting on.
            Direction::Forward => hits
                .iter()
                .find(|hit| hit.range().0 >= from)
                .or_else(|| hits.first()),
            Direction::Backward => hits
                .iter()
                .rev()
                .find(|hit| hit.range().1 < from)
                .or_else(|| hits.last()),
        };
        if let Some(hit) = next.copied() {
            self.editor.select_range(hit);
            self.status = Some(format!("{} matches", hits.len()));
        }
    }

    fn run_replace_all(&mut self, needle: &str, replacement: &str) {
        if needle.is_empty() {
            return;
        }
        let case_sensitive = needle.chars().any(char::is_uppercase);
        let query = SearchQuery::new(needle.to_string(), case_sensitive);
        let count = self.editor.replace_all(&query, replacement);
        self.status = Some(match count {
            0 => format!("No matches for {needle}"),
            1 => "1 replacement".to_string(),
            n => format!("{n} replacements"),
        });
    }

    /// Actions no panel claimed. Returns whether the app handled it.
    ///
    /// The bool is what lets an unclaimed action fall through to the raw key
    /// rather than being silently dropped — `_ => {}` here would look identical
    /// and would be the bug that kills the file tree.
    fn perform_app_action(&mut self, action: Action) -> bool {
        match action {
            Action::FocusNext => self.cycle_focus(),
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
                PanelEvent::OpenFile { path, .. } | PanelEvent::OpenWith { path, .. } => {
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
            is_focused: self.focus == Focus::Tree,
            panel_index: 0,
            terminal_width: w,
            terminal_height: h,
        };
        let editor_ctx = RenderContext {
            theme: &self.theme,
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

/// A 1-based line number typed into the goto prompt, as a 0-based index.
///
/// Line 0 is line 1: a user who types `0` means the top of the file, and there
/// is no other thing they could have meant.
fn parse_line_number(input: &str) -> Option<usize> {
    let n: usize = input.trim().parse().ok()?;
    Some(n.saturating_sub(1))
}
