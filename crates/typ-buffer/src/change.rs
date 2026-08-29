//! Mapping a position forward through edits that have already been applied.
//!
//! This is the one thing in the tree that knows how a position moves when the
//! text before it changes. It lived inside the editor panel, private, which
//! meant every consumer that holds a position across an edit — search results,
//! diagnostics, git hunks — would have had to rediscover it.
//!
//! **This is a shift map, not an anchor system.** It maps positions forward
//! through one batch of edits, in the order those edits were applied, and is
//! then discarded. Zed's `Anchor` and Neovim's extmarks survive arbitrary later
//! edits because the buffer tracks them; nothing here does. That is enough for
//! every consumer named above, and it is what the code already did correctly.
//!
//! ponytail: when something needs a position to survive an arbitrary edit
//! sequence rather than one batch, that is anchors, and anchors are a separate
//! decision — not an extension of this.

use crate::position::Position;

/// The accumulated effect of edits already applied, in original coordinates.
///
/// Column shifts apply only to positions on the line where the last edit ended;
/// line shifts apply to everything after it. Tracking both is what lets several
/// cursors edit the same line without the later ones landing in the wrong place.
#[derive(Debug, Default, Clone, Copy)]
pub struct Shift {
    lines: isize,
    cols: isize,
    /// Original line index the column shift belongs to.
    col_line: Option<usize>,
}

impl Shift {
    /// Where `pos` — stated in the coordinates that existed before this batch —
    /// sits now.
    pub fn apply(&self, pos: Position) -> Position {
        let col = if self.col_line == Some(pos.line) {
            (pos.col as isize + self.cols).max(0) as usize
        } else {
            pos.col
        };
        Position {
            line: (pos.line as isize + self.lines).max(0) as usize,
            col,
        }
    }

    /// Record what an edit did.
    ///
    /// `original_end_line` is in original coordinates; `applied_end` and `after`
    /// are in current ones — `applied_end` is where the edit's range ended once
    /// the shift so far was applied, and `after` is where the replacement text
    /// left the position.
    pub fn record(&mut self, original_end_line: usize, applied_end: Position, after: Position) {
        let col_delta = after.col as isize - applied_end.col as isize;
        if self.col_line == Some(original_end_line) {
            self.cols += col_delta;
        } else {
            self.cols = col_delta;
            self.col_line = Some(original_end_line);
        }
        self.lines += after.line as isize - applied_end.line as isize;
    }
}

/// One edit that has been applied, in the coordinates current when it ran.
///
/// [`Shift`] above answers "where does the *next* edit in this batch go", which
/// is a different question from "where did the thing I was already holding
/// move to" — `Shift::apply` moves every position, including ones before the
/// edit, because within a batch there are none. A diagnostic can sit anywhere,
/// so it needs the edit's own extent to compare against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditSpan {
    /// Where the replaced range began.
    pub start: Position,
    /// Where it ended, before the replacement.
    pub old_end: Position,
    /// Where the replacement text left it.
    pub new_end: Position,
}

/// Where `pos` sits after `edits` have been applied to the text it described.
///
/// The edits are in the order they ran, each stated in the coordinates that
/// were current at the time, which is what makes folding them left to right
/// correct.
///
/// A position **inside** an edit's range clamps to the range's start. There is
/// nowhere else honest for it to go: the text it named is gone.
///
/// **This does not survive an undo, a redo, or a reload from disk.** Those
/// replace the whole rope rather than going through `replace_range`, so no
/// spans are recorded and everything held against the old text keeps stale
/// coordinates until whatever produced it produces it again.
pub fn shift_through(pos: Position, edits: &[EditSpan]) -> Position {
    edits.iter().fold(pos, shift_one)
}

fn shift_one(pos: Position, edit: &EditSpan) -> Position {
    if before(pos, edit.start) {
        return pos;
    }
    if !before(edit.old_end, pos) {
        // Inside the replaced range, or exactly at its end.
        return edit.start;
    }
    let lines = edit.new_end.line as isize - edit.old_end.line as isize;
    let col = if pos.line == edit.old_end.line {
        (pos.col as isize + (edit.new_end.col as isize - edit.old_end.col as isize)).max(0) as usize
    } else {
        pos.col
    };
    Position {
        line: (pos.line as isize + lines).max(0) as usize,
        col,
    }
}

fn before(a: Position, b: Position) -> bool {
    (a.line, a.col) < (b.line, b.col)
}
