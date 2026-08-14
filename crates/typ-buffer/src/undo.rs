/// Whole-content undo history.
///
/// Snapshotting entire buffer content is the simplest thing that is correct
/// for any edit shape. If memory shows up in profiling on large files, switch
/// to per-edit deltas.
#[derive(Default)]
pub struct History {
    undo: Vec<String>,
    redo: Vec<String>,
}

impl History {
    pub fn record(&mut self, before: String) {
        self.undo.push(before);
        self.redo.clear();
    }

    /// Returns the content to restore, banking `current` for redo.
    pub fn undo(&mut self, current: String) -> Option<String> {
        let prev = self.undo.pop()?;
        self.redo.push(current);
        Some(prev)
    }

    pub fn redo(&mut self, current: String) -> Option<String> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }
}
