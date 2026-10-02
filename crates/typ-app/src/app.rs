use std::path::Path;

mod background;
mod dispatch;
mod door;
pub(crate) mod panes;
mod picker;
mod render;
/// Search, replace and goto-line. A child module rather than a sibling so it
/// reaches `App`'s private fields without any of them widening to `pub(crate)`:
/// the extraction is meant to shorten this file, not to open it up.
mod search;
mod servers;
mod tabs;

use anyhow::Result;
use typ_buffer::SearchQuery;
use typ_core::{Keymap, Panel, ThemeColors};
use typ_find::{FileHit, FindWorker, LineHit};
use typ_panel_editor::EditorPanel;
use typ_panel_editor::render::Whitespace;
use typ_panel_tree::TreePanel;
use typ_picker::Picker;
use typ_registry::Registry;
use typ_syntax::ParseWorker;

use crate::prompt::Prompt;
use crate::status::{Segment, StatusFacts, segments};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tree,
    Editor,
}

pub struct App {
    tree: TreePanel,
    /// Every pane, in no particular order: `layout` says where each one is.
    /// **Never empty**, for the reason a pane's `tabs` never is.
    panes: Vec<Pane>,
    /// Index into `panes` of the current pane: the one focus goes back to, and
    /// where the tree opens a file. Always valid.
    pane: usize,
    /// Where each pane is on screen.
    layout: crate::pane::Layout,
    /// The id the next pane gets. Never reused, so a stale id names nothing.
    next_pane: u32,
    /// The frame the last render drew into.
    ///
    /// What a key that moves between panes measures against: `alt+right`
    /// means the pane beside this one on the screen the user is looking at,
    /// and a chord arrives with no area of its own. 80x24 until the first
    /// frame, the terminal's default size.
    screen: ratatui::layout::Rect,
    /// Resize mode is on: the arrows move a rule rather than the cursor.
    resizing: bool,
    /// The rule a held mouse button is dragging, if one is.
    dragging: Option<crate::pane::SplitPath>,
    /// A pane close was refused because a tab in it held unsaved work, and
    /// which pane. Closing the same pane again goes through; anything else in
    /// between abandons it, as `close_pending` does for a tab.
    close_pane_pending: Option<crate::pane::PaneId>,
    registry: Registry,
    keymap: Keymap,
    theme: ThemeColors,
    focus: Focus,
    /// Where focus was before it came here, for `esc` to go back to. A stack
    /// one deep, because two panels is as deep as it gets until docks land.
    came_from: Option<Focus>,
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
    /// The chord a menu row could have been, and when to stop saying so.
    ///
    /// Not transient in the `clear_transient` sense: it lasts its two seconds
    /// through whatever keys come next, and goes with the first event after.
    teaching: Option<(String, std::time::Instant)>,
}

/// One region of the editor area and the files open in it.
struct Pane {
    id: crate::pane::PaneId,
    /// Every file open in this pane. **Never empty**: closing the last tab
    /// leaves an empty buffer, which is the state the editor starts in. An
    /// empty `tabs` would make `editor()` return an `Option` and every one of
    /// its callers handle a state that never occurs.
    tabs: Vec<Tab>,
    /// Index into `tabs`. Always valid, for the same reason.
    active: usize,
}

impl Pane {
    fn new(id: crate::pane::PaneId) -> Self {
        Pane {
            id,
            tabs: vec![Tab::new(EditorPanel::from_str(""))],
            active: 0,
        }
    }

    /// The tab on screen in this pane.
    fn shown(&self) -> &Tab {
        &self.tabs[self.active]
    }

    fn shown_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }
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

/// Between status segments. Two spaces rather than a glyph separator: a
/// separator needs a colour decision of its own and a Nerd Font question at
/// M6, and whitespace has neither.
const SEGMENT_GAP: &str = "  ";

/// The order `f6` walks. Fixed, so a screen-reader user can learn it
/// (interface §5); docks and the status bar join it when they exist.
const FOCUS_ORDER: [Focus; 2] = [Focus::Tree, Focus::Editor];

/// What the status bar says while resize mode is on.
const RESIZE_HINT: &str = "Resize: arrows move the rule  ·  = evens it  ·  Enter or Esc done";

/// Shown when there is nothing more urgent to say. Discoverability is part of
/// the product: bindings nobody can find are bindings that do not exist.
const HINT: &str = "Tab focus  ·  Enter open  ·  Ctrl+S save  ·  Ctrl+Q quit";

impl App {
    pub fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            tree: TreePanel::new(root)?,
            panes: vec![Pane::new(crate::pane::PaneId(0))],
            pane: 0,
            layout: crate::pane::Layout::Leaf(crate::pane::PaneId(0)),
            next_pane: 1,
            screen: ratatui::layout::Rect::new(0, 0, 80, 24),
            resizing: false,
            dragging: None,
            close_pane_pending: None,
            registry: Registry::with_builtins(),
            keymap: Keymap::default_bindings(),
            theme: ThemeColors::default(),
            focus: Focus::Tree,
            came_from: None,
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
            teaching: None,
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
        for pane in 0..self.panes.len() {
            for index in 0..self.panes[pane].tabs.len() {
                self.watch_tab(pane, index);
            }
        }
        // Whatever is already open has never been parsed.
        self.request_parse_if_stale();
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
        // The mode owns the arrows, so it says so for as long as it is on.
        if self.resizing {
            return RESIZE_HINT.to_string();
        }
        if let Some(message) = &self.status {
            return message.clone();
        }
        match &self.teaching {
            Some((line, _)) => line.clone(),
            None => HINT.to_string(),
        }
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
        let cursor = self.panes[self.pane].shown().panel.cursor();
        let path = self.panes[self.pane].shown().panel.path();
        let file_type = crate::status::file_type_of(path);
        let file_name = self.panes[self.pane].shown().panel.file_name();
        segments(&StatusFacts {
            file_name: &file_name,
            modified: self.panes[self.pane].shown().panel.is_dirty(),
            file_type: file_type.as_deref(),
            line_ending: self.panes[self.pane].shown().panel.line_ending().label(),
            indent_width: self.panes[self.pane].shown().panel.tab_width(),
            selection_count: self.panes[self.pane].shown().panel.selections().len(),
            line: cursor.line,
            col: cursor.col,
            total_lines: self.panes[self.pane].shown().panel.line_count(),
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
        self.close_pane_pending = None;
        // The hover described a position. Any key at all may have left it, and
        // a box standing over somewhere the cursor no longer is says something
        // true about the wrong thing. `handle_chord` calls this before running
        // the action, so `Hover` still gets to set it afterwards.
        self.hover = None;
        // A click or a paste abandons a half-typed sequence rather than
        // leaving its second key to land on whatever comes next.
        self.menu = None;
    }

    /// Quit, unless a panel has something to confirm first.
    fn request_quit(&mut self) {
        if self.quit_pending {
            self.quit = true;
            return;
        }
        // **Every tab, not the active one.** Quitting closes all of them, so a
        // dirty buffer two tabs over is exactly as unsaved as the one on
        // screen, and it is the one the user has forgotten about. Every pane
        // too, for the same reason.
        let blocker = self
            .all_tabs()
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
            Focus::Editor => self.panes[self.pane].shown().panel.name(),
        }
    }

    pub fn editor_title(&self) -> String {
        self.panes[self.pane].shown().panel.title()
    }

    /// The next region in `FOCUS_ORDER`, wrapping.
    pub fn cycle_focus(&mut self) {
        self.step_focus(1);
    }

    fn step_focus(&mut self, delta: isize) {
        let at = FOCUS_ORDER
            .iter()
            .position(|f| *f == self.focus)
            .unwrap_or(0);
        let next = (at as isize + delta).rem_euclid(FOCUS_ORDER.len() as isize);
        self.set_focus(FOCUS_ORDER[next as usize]);
    }

    /// Move focus, remembering where it came from.
    pub fn set_focus(&mut self, focus: Focus) {
        if focus != self.focus {
            self.came_from = Some(self.focus);
            self.focus = focus;
        }
    }

    /// `esc` with nothing of the panel's own to cancel: back to where focus
    /// came from. The editor is home, so `esc` there never moves focus: it is
    /// the pane every other one returns to (interface §5, "back to the
    /// editor"). Returns whether focus moved.
    fn focus_back(&mut self) -> bool {
        if self.focus == Focus::Editor || self.focused().captures_escape() {
            return false;
        }
        let back = self
            .came_from
            .filter(|f| *f != self.focus)
            .unwrap_or(Focus::Editor);
        self.set_focus(back);
        true
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
            self.panes[self.pane].shown_mut().panel.set_tab_width(width);
        }
    }

    /// Which whitespace the editor marks.
    pub fn set_whitespace(&mut self, whitespace: Whitespace) {
        self.whitespace = whitespace;
        self.panes[self.pane]
            .shown_mut()
            .panel
            .set_whitespace(whitespace);
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

    pub fn prompt(&self) -> Option<&Prompt> {
        self.prompt.as_ref()
    }

    /// Say something in the status bar.
    ///
    /// Startup warnings go here rather than to stderr: stderr is invisible once
    /// the alternate screen is up, so a config complaint printed there is a
    /// complaint nobody reads.
    pub fn notify(&mut self, message: String) {
        self.status = Some(message);
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

    pub fn tree_mut(&mut self) -> &mut TreePanel {
        &mut self.tree
    }

    pub fn focused_mut(&mut self) -> &mut dyn Panel {
        match self.focus {
            Focus::Tree => &mut self.tree,
            Focus::Editor => &mut self.panes[self.pane].shown_mut().panel,
        }
    }

    /// Every open tab, in every pane.
    ///
    /// What "all the open files" means once there is more than one pane: quit
    /// asks about each, the servers hear about each, a search overrides each.
    fn all_tabs(&self) -> impl Iterator<Item = &Tab> {
        self.panes.iter().flat_map(|pane| pane.tabs.iter())
    }

    fn all_tabs_mut(&mut self) -> impl Iterator<Item = &mut Tab> {
        self.panes.iter_mut().flat_map(|pane| pane.tabs.iter_mut())
    }

    /// The first tab `wanted` picks, in any pane, as `(pane, tab)` indices.
    fn find_tab(&self, wanted: impl Fn(&Tab) -> bool) -> Option<(usize, usize)> {
        self.panes
            .iter()
            .enumerate()
            .find_map(|(index, pane)| pane.tabs.iter().position(&wanted).map(|tab| (index, tab)))
    }
}
