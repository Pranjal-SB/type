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
