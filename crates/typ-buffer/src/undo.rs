use ropey::Rope;

use crate::selection::Selections;

/// What an edit did, for the purpose of deciding whether it continues the
/// previous one.
///
/// Coarse on purpose: the question is only "is this the same kind of thing the
/// user was already doing", and a finer taxonomy would split runs the user
/// experiences as one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    Insert,
    Delete,
    /// Anything that should always stand alone — a paste, a replace-all.
    Other,
}

/// The buffer as it stood before an edit, and where the cursors were.
///
/// Storing the selections is what makes undo put the caret back where the edit
/// happened rather than wherever clamping left it. Every editor in the field
/// does this; an undo that leaves the cursor somewhere unrelated is disorienting
/// enough that users stop trusting it.
#[derive(Clone)]
pub struct Snapshot {
    pub rope: Rope,
    pub selections: Selections,
}

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
///
/// **Runs have no clock.** VS Code and Zed break undo groups on an idle timer.
/// A timer means the buffer needs a clock, which means tests need to inject one,
/// which means the rule is only ever exercised through a fake. The rule here is
/// structural instead: consecutive edits of the same `EditKind` coalesce, and
/// anything that is not an edit — a motion, a click, a save — calls `boundary`.
/// That is deterministic and it matches what a user means by "undo what I just
/// typed": the run ends when they moved.
///
/// ponytail: pausing mid-word for ten minutes without moving still coalesces.
/// If that ever bites, a timer goes beside this rule, not instead of it.
/// How many undo steps are kept.
///
/// vim's `undolevels` default, and there is no reason to be cleverer until
/// someone measures a session where it bites. Structural sharing makes each
/// snapshot cheap but not free — every one pins the rope nodes it replaced, so
/// an uncapped stack is an uncapped retention of every version of the file for
/// as long as the editor is open.
pub const MAX_UNDO_STEPS: usize = 1000;

#[derive(Default)]
pub struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The kind of the run currently open. `None` means the next edit starts a
    /// new step regardless of its kind.
    open_run: Option<EditKind>,
}

impl History {
    /// Record the state before an edit, unless it continues the open run.
    ///
    /// Continuing a run means *not* pushing: the snapshot already on the stack
    /// predates the whole run, which is exactly the state undo should restore.
    pub fn record(&mut self, kind: EditKind, before: Rope, selections: &Selections) {
        self.redo.clear();
        if self.open_run == Some(kind) && kind != EditKind::Other {
            return;
        }
        self.undo.push(Snapshot {
            rope: before,
            selections: selections.clone(),
        });
        // Forget the oldest step, never the newest. `remove(0)` is O(n) on a
        // 1000-element Vec of cheap clones and runs once per *step*, not per
        // keystroke — a VecDeque would trade that for a less obvious type on
        // every other line of this file.
        if self.undo.len() > MAX_UNDO_STEPS {
            self.undo.remove(0);
        }
        self.open_run = Some(kind);
    }

    /// How many steps are on the undo stack. For tests and for a future status
    /// segment; nothing in the editor branches on it.
    pub fn depth(&self) -> usize {
        self.undo.len()
    }

    /// End the open run, so the next edit starts a new undo step.
    ///
    /// Called on anything that is not an edit — a motion, a click, a save.
    pub fn boundary(&mut self) {
        self.open_run = None;
    }

    /// Returns the state to restore, banking `current` for redo.
    pub fn undo(&mut self, current: Rope, selections: &Selections) -> Option<Snapshot> {
        let previous = self.undo.pop()?;
        self.redo.push(Snapshot {
            rope: current,
            selections: selections.clone(),
        });
        // An undo always ends the run: typing after an undo must not fold into
        // the step that was just undone.
        self.open_run = None;
        Some(previous)
    }

    pub fn redo(&mut self, current: Rope, selections: &Selections) -> Option<Snapshot> {
        let next = self.redo.pop()?;
        self.undo.push(Snapshot {
            rope: current,
            selections: selections.clone(),
        });
        self.open_run = None;
        Some(next)
    }
}
