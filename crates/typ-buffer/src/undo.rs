use ropey::Rope;

/// Whole-content undo history, stored as rope snapshots.
///
/// A snapshot is a `Rope` rather than a `String` because ropey clones are O(1)
/// and copy-on-write: two snapshots share every node they have in common, so a
/// deep undo stack over a large file costs the edits, not one full copy of the
/// text per step. `to_string()` would allocate and copy the whole buffer on
/// every keystroke.
///
/// Whole-content snapshots stay the right call at this size — they are correct
/// for any edit shape, and with structural sharing they are no longer expensive
/// enough to justify per-edit deltas.
#[derive(Default)]
pub struct History {
    undo: Vec<Rope>,
    redo: Vec<Rope>,
}

impl History {
    pub fn record(&mut self, before: Rope) {
        self.undo.push(before);
        self.redo.clear();
    }

    /// Returns the content to restore, banking `current` for redo.
    pub fn undo(&mut self, current: Rope) -> Option<Rope> {
        let prev = self.undo.pop()?;
        self.redo.push(current);
        Some(prev)
    }

    pub fn redo(&mut self, current: Rope) -> Option<Rope> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }
}
