//! The overlay's half of the app: opening it, feeding it, routing keys to it.
//!
//! A child module of `app` for the same reason `search` is — it reaches `App`'s
//! private fields, and the alternative is widening half of them to
//! `pub(crate)`. Task 0 shortened `app.rs` to make room for this; putting it
//! back in there would undo that in one milestone.

use anyhow::Result;
use typ_core::{KeyChord, Panel};
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
        if events.contains(&typ_core::PanelEvent::CloseSelf) {
            self.close_picker();
            return Ok(());
        }

        // An `OpenFile` closes the overlay too: choosing a file is the other
        // way out of it, and leaving it up over the file just opened is the
        // kind of thing that reads as a bug even when every test passes.
        let opened = events
            .iter()
            .any(|event| matches!(event, typ_core::PanelEvent::OpenFile { .. }));
        if opened {
            self.close_picker();
        }
        self.apply(events)
    }
}
