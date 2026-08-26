//! The overlay's half of the app: opening it, feeding it, routing keys to it.
//!
//! A child module of `app` for the same reason `search` is — it reaches `App`'s
//! private fields, and the alternative is widening half of them to
//! `pub(crate)`. Task 0 shortened `app.rs` to make room for this; putting it
//! back in there would undo that in one milestone.

use anyhow::Result;
use typ_core::{KeyChord, Panel, PanelEvent};
use typ_picker::{CommandRow, Mode, Picker};

use super::App;

/// Types the file query into the command list. VS Code's convention — `>` for
/// commands, `@` for symbols, `:` for a line, `#` for workspace symbols — and
/// the one path into the palette that survives a terminal which cannot deliver
/// `Ctrl+Shift+letter` at all.
const COMMAND_PREFIX: &str = ">";

/// How many command rows to rank. Above the number of actions there are, so the
/// list is never truncated by this rather than by the query.
const COMMANDS: usize = 200;

/// How many ranked rows to ask the worker for.
///
/// The picker shows perhaps twenty; asking for a hundred means paging and
/// resizing do not need a round trip, and the cost is a hundred `indices` calls
/// rather than fifty thousand. Not the corpus size — that is the number this
/// whole design exists to keep off the render thread.
pub(crate) const HITS: usize = 100;

/// How many matching lines one project search returns.
///
/// Larger than `HITS` because a search over a real project finds many more
/// matches than a filename query does, and smaller than unbounded because the
/// user is typing the query — every keystroke starts another search, and an
/// uncapped one is an unbounded allocation driven by a half-written pattern.
pub(crate) const GREP_HITS: usize = 500;

impl App {
    /// Put the overlay up and start a walk.
    ///
    /// The previous results stay on screen while the walk runs. 94.7 ms is
    /// invisible behind a stale list and obvious behind an empty one.
    pub fn open_picker(&mut self) {
        let mut picker = Picker::new();
        picker.set_hits(self.find_hits.clone());
        self.picker = Some(picker);
        self.request_index();
        self.index_requested = true;
        // Ask for the opening screen — an empty query, which ranks nothing and
        // lists the corpus.
        self.request_filter(String::new(), HITS);
        self.dirty = true;
    }

    /// Put the overlay up in search mode.
    ///
    /// No index and no opening list: the corpus for a project search is the
    /// project's *text*, which nothing can rank until there is something to
    /// search for. An empty query legitimately shows nothing.
    pub fn open_search(&mut self) {
        self.picker = Some(Picker::search());
        self.grep_hits.clear();
        self.grep_complete = true;
        self.dirty = true;
    }

    /// Put the overlay up over the command list.
    ///
    /// **Implemented as typing the `>`**, not as a separate mode switch. The
    /// chord is Enhanced-tier and may never arrive in a given terminal, so it
    /// cannot be the path the palette actually works through; making it a
    /// shortcut for the prefix leaves one mode-switch path rather than two that
    /// can disagree.
    pub fn open_command_palette(&mut self) {
        let mut picker = Picker::new();
        picker.set_query(COMMAND_PREFIX.to_string());
        picker.set_mode(Mode::Commands);
        self.picker = Some(picker);
        // No walk and no filter: the corpus here is a static list of action
        // names. Backspacing the `>` away turns this into the file picker, and
        // that is where the walk gets asked for — see `request_for_mode`.
        self.rank_commands("");
        self.dirty = true;
    }

    /// Read the query's first character and set the mode it implies.
    ///
    /// Only from `Files`/`Commands`. A project search is text, and `>` in it is
    /// a perfectly ordinary thing to look for.
    fn refresh_picker_mode(&mut self) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        if picker.mode() == Mode::Search {
            return;
        }
        match picker.query().strip_prefix(COMMAND_PREFIX) {
            Some(_) => picker.set_mode(Mode::Commands),
            None => picker.set_mode(Mode::Files),
        }
    }

    /// Rank the action names and hand the rows to the overlay.
    ///
    /// **Ranked here rather than on the worker.** The corpus is sixty-odd
    /// static names — the round trip would cost more than the ranking, and the
    /// worker exists to keep a 37,000-entry corpus off this thread, not a
    /// sixty-entry one.
    fn rank_commands(&mut self, needle: &str) {
        let names: Vec<String> = typ_core::Action::ALL
            .iter()
            // A row that reopens the overlay you are already in is a no-op
            // nobody can explain.
            .filter(|action| **action != typ_core::Action::OpenCommandPalette)
            .map(|action| action.name().to_string())
            .collect();

        let rows: Vec<CommandRow> = typ_find::rank(needle, &names, COMMANDS)
            .into_iter()
            .map(|hit| {
                let binding = typ_core::Action::from_name(&hit.path)
                    .map(|action| self.keymap.bindings_for(action))
                    .unwrap_or_default()
                    .first()
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                CommandRow {
                    name: hit.path,
                    binding,
                    indices: hit.indices,
                }
            })
            .collect();

        if let Some(picker) = self.picker.as_mut() {
            picker.set_commands(rows);
        }
    }

    /// Run the highlighted command, if the overlay is showing one.
    ///
    /// The same two steps a keypress takes — the focused panel first, then the
    /// app — because a palette that only tried the panel would silently do
    /// nothing for every action the app owns. The overlay closes *first*: an
    /// action that opens a prompt or another picker must not find this one
    /// still standing.
    fn run_selected_command(&mut self) -> Result<()> {
        let Some(action) = self
            .picker
            .as_ref()
            .and_then(|picker| picker.selected_command())
            .and_then(|row| typ_core::Action::from_name(&row.name))
        else {
            return Ok(());
        };
        self.close_picker();

        if let Some(events) = self.focused_mut().apply_action(action) {
            return self.apply(events);
        }
        self.perform_app_action(action);
        Ok(())
    }

    pub fn close_picker(&mut self) {
        self.picker = None;
        self.dirty = true;
    }

    pub fn picker(&self) -> Option<&Picker> {
        self.picker.as_ref()
    }

    /// Whether a walk has ever been asked for.
    pub fn index_requested(&self) -> bool {
        self.index_requested
    }

    /// A click while the overlay is up.
    ///
    /// Hit-tested against `layout::picker_area`, the same function `render`
    /// draws with, so the two cannot drift apart.
    pub fn route_picker_mouse(
        &mut self,
        event: crossterm::event::MouseEvent,
        frame: ratatui::layout::Rect,
    ) -> Vec<PanelEvent> {
        let area = crate::layout::picker_area(frame);
        let Some(picker) = self.picker.as_mut() else {
            return Vec::new();
        };
        let events = picker.handle_mouse(event, area);

        if events.contains(&PanelEvent::CloseSelf) {
            self.close_picker();
            return vec![PanelEvent::NeedsRedraw];
        }
        self.dirty = true;
        self.absolutise(events)
    }

    /// A wheel notch while the overlay is up.
    pub fn route_picker_scroll(
        &mut self,
        delta: i32,
        frame: ratatui::layout::Rect,
    ) -> Vec<PanelEvent> {
        let area = crate::layout::picker_area(frame);
        match self.picker.as_mut() {
            Some(picker) => picker.handle_scroll(delta, area),
            None => Vec::new(),
        }
    }

    /// Rewrite root-relative candidate paths into ones the app can open, and
    /// close the overlay if one of them is a choice.
    ///
    /// Shared by the key and the mouse routes — invariant 8 means both produce
    /// the same `OpenFile`, and two copies of this join is two chances for one
    /// of them to be wrong.
    fn absolutise(&mut self, events: Vec<PanelEvent>) -> Vec<PanelEvent> {
        let mut opened = false;
        let events: Vec<PanelEvent> = events
            .into_iter()
            .map(|event| match event {
                PanelEvent::OpenFile { path, line, col } => {
                    opened = true;
                    PanelEvent::OpenFile {
                        path: self.root.join(path),
                        line,
                        col,
                    }
                }
                other => other,
            })
            .collect();
        if opened {
            self.close_picker();
        }
        events
    }

    /// Hand the open overlay whichever result list its mode calls for.
    ///
    /// One place rather than one per arm of `handle_found`, so a new result
    /// kind cannot land in the app and quietly fail to reach the screen.
    pub(crate) fn push_hits_to_picker(&mut self) {
        let Some(picker) = self.picker.as_mut() else {
            return;
        };
        match picker.mode() {
            Mode::Files => picker.set_hits(self.find_hits.clone()),
            Mode::Search => picker.set_lines(self.grep_hits.clone(), self.grep_complete),
            // Ranked here, not on the worker, so a late `Found` has nothing to
            // deliver to this mode.
            Mode::Commands => {}
        }
    }

    /// Ask the worker for whatever the current mode's query means.
    ///
    /// The two modes ask different questions of the same worker: file mode
    /// ranks against a corpus the worker holds, search mode hands it a query to
    /// run. They share one generation counter, so a late answer from the mode
    /// you have left can never be mistaken for an answer to the one you are in.
    fn request_for_mode(&mut self, mode: Mode, query: String) {
        match mode {
            Mode::Files => {
                // The palette can be opened by its own chord, which asks for no
                // walk because it does not need one. Backspacing the `>` away
                // arrives here, and without this the file picker would rank
                // against a corpus nothing ever filled.
                if !self.index_requested {
                    self.request_index();
                    self.index_requested = true;
                }
                self.request_filter(query, HITS);
            }
            Mode::Search => {
                self.request_grep(query);
            }
            // No round trip: the corpus is sixty static names and the answer is
            // already on this thread. The `>` is not part of what gets matched.
            Mode::Commands => {
                let needle = query
                    .strip_prefix(COMMAND_PREFIX)
                    .unwrap_or(&query)
                    .to_string();
                self.rank_commands(&needle);
            }
        }
    }

    /// Run a project search, with the open buffer searched from memory.
    ///
    /// **The override is the point.** A search that reports what is on disk
    /// while the user is looking at unsaved edits is answering a question
    /// nobody asked — and with tabs, "looking at" includes the two files behind
    /// this one. Every dirty buffer is sent, not only the active one: an edit
    /// in a background tab is exactly as unsaved as the one on screen, and it
    /// is the one the user has stopped thinking about.
    ///
    /// Clean tabs are left out. The override is a copy of the whole buffer over
    /// a channel, the walk already reads the identical bytes off disk, and with
    /// twenty tabs open that is twenty needless copies per keystroke.
    fn request_grep(&mut self, query: String) -> u64 {
        let overrides: Vec<(std::path::PathBuf, String)> = self
            .tabs
            .iter()
            .filter_map(|tab| {
                let path = tab.panel.path()?;
                tab.panel
                    .buffer()
                    .is_dirty()
                    .then(|| (path.to_path_buf(), tab.panel.buffer().text()))
            })
            .collect();
        let root = self.root.clone();
        let Some(worker) = &mut self.find_worker else {
            self.awaited_filter = Some(self.awaited_filter.unwrap_or(0) + 1);
            return self.awaited_filter.expect("just set");
        };
        let generation = worker.grep(root, query, GREP_HITS, overrides);
        self.awaited_filter = Some(generation);
        generation
    }

    /// Keys while the overlay is up.
    ///
    /// **The query is compared before and after rather than reported by the
    /// panel.** `PanelEvent` is a closed vocabulary and "my query changed" is
    /// not worth one of its variants; the app owns this overlay, so it can
    /// simply look. See `typ-picker`'s crate docs.
    pub(crate) fn handle_picker_chord(&mut self, chord: KeyChord) -> Result<()> {
        let Some(picker) = self.picker.as_mut() else {
            return Ok(());
        };

        let code = chord.raw.code;
        let before = picker.query().to_string();
        let events = picker.handle_key(chord);
        let after = picker.query().to_string();

        // The mode is read *after* the key, because the key may have been the
        // `>` that changed it — or the backspace that took it away again.
        if before != after {
            self.refresh_picker_mode();
            let mode = self.picker.as_ref().map(Picker::mode).unwrap_or_default();
            self.request_for_mode(mode, after);
        }
        self.dirty = true;

        // A command leaves through here rather than as a `PanelEvent`. The
        // picker returned `NeedsRedraw` for that Enter, because `open_at` has
        // nothing to open — invariant 6 keeps the vocabulary closed.
        if code == crossterm::event::KeyCode::Enter
            && self.picker.as_ref().map(Picker::mode) == Some(Mode::Commands)
        {
            return self.run_selected_command();
        }

        // `CloseSelf` from the overlay means the overlay, not the focused
        // panel. Routing it through `apply` would close the editor or the tree
        // and leave the picker floating over whatever was left.
        if events.contains(&PanelEvent::CloseSelf) {
            self.close_picker();
            return Ok(());
        }

        // **Candidates are root-relative and `OpenFile` is not.** The picker
        // holds `src/highlight.rs` because that is what gets ranked — an
        // absolute path puts the user's home directory in front of every
        // candidate, wasting the width and giving the matcher a long identical
        // prefix to score through. The root lives here, so the join happens
        // here; the panel knowing about filesystems is the thing being avoided.
        //
        // Choosing a file also closes the overlay: leaving it up over the file
        // just opened reads as a bug even when every test passes.
        let events = self.absolutise(events);
        self.apply(events)
    }
}
