//! The overlay's half of the app: opening it, feeding it, routing keys to it.
//!
//! A child module of `app` for the same reason `search` is — it reaches `App`'s
//! private fields, and the alternative is widening half of them to
//! `pub(crate)`. Task 0 shortened `app.rs` to make room for this; putting it
//! back in there would undo that in one milestone.

use anyhow::Result;
use typ_core::{KeyChord, Panel, PanelEvent};
use typ_picker::Picker;

use super::App;

/// How many ranked rows to ask the worker for.
///
/// The picker shows perhaps twenty; asking for a hundred means paging and
/// resizing do not need a round trip, and the cost is a hundred `indices` calls
/// rather than fifty thousand. Not the corpus size — that is the number this
/// whole design exists to keep off the render thread.
pub(crate) const HITS: usize = 100;

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
        picker.set_hits(self.find_hits.clone());
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

        let before = picker.query().to_string();
        let events = picker.handle_key(chord);
        let after = picker.query().to_string();

        if before != after {
            self.request_filter(after, HITS);
        }
        self.dirty = true;

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
