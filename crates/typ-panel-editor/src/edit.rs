//! The one shape every edit reduces to.
//!
//! Split out of `actions.rs` when that file crossed the 800-line cap in
//! AGENTS.md. The seam is a real one rather than a convenient byte count:
//! `Edit` is the *description* of a change, `actions.rs` is the behaviour that
//! produces and applies them. Everything here is data and arithmetic, and none
//! of it touches `EditorPanel`.

use typ_buffer::Position;
use unicode_segmentation::UnicodeSegmentation;

/// One edit, described rather than performed: replace `start..end` with `text`.
///
/// An empty range inserts and an empty text deletes, so every editing action
/// reduces to this one shape and the position mapping only has to understand
/// one thing.
pub(crate) struct Edit {
    pub(crate) start: Position,
    pub(crate) end: Position,
    pub(crate) text: String,
}

impl Edit {
    pub(crate) fn delete(start: Position, end: Position) -> Self {
        Self {
            start,
            end,
            text: String::new(),
        }
    }

    /// An edit that changes nothing, for a caret with nowhere to go — the
    /// start of the buffer for backspace, the end for delete.
    pub(crate) fn nothing(at: Position) -> Self {
        Self {
            start: at,
            end: at,
            text: String::new(),
        }
    }
}

/// Where a position ends up once `text` has been inserted at `start`.
pub(crate) fn position_after(start: Position, text: &str) -> Position {
    let mut line = start.line;
    let mut col = start.col;
    for grapheme in text.graphemes(true) {
        if grapheme == "\n" || grapheme == "\r\n" {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Position { line, col }
}
