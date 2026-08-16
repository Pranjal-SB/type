//! What the status bar says, as a list of named segments.
//!
//! Helix's statusline is 24 named elements the user can reorder; ttt puts git
//! blame and an indent picker in its own. TYPE's carried three things: a
//! message, a filename, and `line:col`.
//!
//! # Why this is not `status_segments()` yet
//!
//! Architecture §5 plans a `Panel::status_segments()` for M4 — clickable chips
//! contributed by the focused panel and routed back by id. That is the better
//! design, because a panel owns what it can say about itself and the app should
//! not have to know that an editor has a line ending.
//!
//! This is not that. It fills the existing bar with content **shaped so that M4
//! moves the source of each segment without rewriting what any of them say**:
//! the segment list, its ids, and its emphasis rules all survive that move, and
//! only `segments()`'s argument changes from a struct the app fills in to a call
//! on the focused panel.

use typ_core::ThemeColors;

/// Names a segment, so M4 can route a click back to whatever produced it and so
/// a config can eventually order them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentId {
    FileName,
    FileType,
    LineEnding,
    Indent,
    Selections,
    Position,
    Percentage,
}

/// How loudly a segment is drawn.
///
/// Three levels rather than a colour per segment: the status bar is one strip
/// of small text, and the only thing a reader needs from its styling is a
/// ranking — what is this file, what state is it in, and what is merely true.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    /// Identity and position: what a user looks for deliberately.
    Normal,
    /// Facts that matter when they are wrong and never otherwise — the line
    /// ending, the indent width.
    Quiet,
    /// Something is unusual and worth noticing: unsaved changes, thirty cursors.
    Accent,
}

impl Emphasis {
    pub fn colour(self, theme: &ThemeColors) -> ratatui::style::Color {
        match self {
            Emphasis::Normal => theme.status_bar_fg,
            Emphasis::Quiet => theme.status_bar_inactive_fg,
            Emphasis::Accent => theme.status_bar_accent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub id: SegmentId,
    pub text: String,
    pub emphasis: Emphasis,
}

/// Everything the right-hand segments are built from.
///
/// A struct of plain facts rather than a borrow of the app: this is the seam
/// that becomes `Panel::status_segments()` at M4, and keeping it free of
/// application types is what makes that a move rather than a rewrite.
pub struct StatusFacts<'a> {
    pub file_name: &'a str,
    pub modified: bool,
    /// `None` for a file with no extension.
    pub file_type: Option<&'a str>,
    pub line_ending: &'a str,
    pub indent_width: usize,
    pub selection_count: usize,
    /// Zero-based, as everything inside TYPE is.
    pub line: usize,
    pub col: usize,
    pub total_lines: usize,
}

/// The right-hand segments, in order.
///
/// Segments that cannot be answered honestly are omitted rather than filled
/// with a placeholder — a status bar that says `--` has spent a cell to tell
/// you nothing.
pub fn segments(facts: &StatusFacts) -> Vec<Segment> {
    let mut out = Vec::with_capacity(7);

    out.push(Segment {
        id: SegmentId::FileName,
        text: if facts.modified {
            format!("{} *", facts.file_name)
        } else {
            facts.file_name.to_string()
        },
        // Unsaved work is the one piece of state on this bar that a user needs
        // to notice without looking for it.
        emphasis: if facts.modified {
            Emphasis::Accent
        } else {
            Emphasis::Normal
        },
    });

    if let Some(file_type) = facts.file_type {
        out.push(Segment {
            id: SegmentId::FileType,
            text: file_type.to_string(),
            emphasis: Emphasis::Quiet,
        });
    }

    out.push(Segment {
        id: SegmentId::LineEnding,
        text: facts.line_ending.to_string(),
        emphasis: Emphasis::Quiet,
    });

    out.push(Segment {
        id: SegmentId::Indent,
        // Honest rather than aspirational: indentation *is* hardcoded to spaces
        // at this width, and saying so on screen is the first step to fixing it.
        // `.editorconfig` and detection land at M2.5 and change this text, not
        // this segment.
        text: format!("Spaces: {}", facts.indent_width),
        emphasis: Emphasis::Quiet,
    });

    if facts.selection_count > 1 {
        out.push(Segment {
            id: SegmentId::Selections,
            text: format!("{} cursors", facts.selection_count),
            // Thirty cursors is a state you can forget you are in, and the next
            // keystroke edits in thirty places.
            emphasis: Emphasis::Accent,
        });
    }

    out.push(Segment {
        id: SegmentId::Position,
        // Counted from 1, the way every compiler error and every other editor
        // does.
        text: format!("{}:{}", facts.line + 1, facts.col + 1),
        emphasis: Emphasis::Normal,
    });

    out.push(Segment {
        id: SegmentId::Percentage,
        text: format!("{}%", percentage(facts.line, facts.total_lines)),
        emphasis: Emphasis::Quiet,
    });

    out
}

/// How far through the file the cursor is, 0–100.
fn percentage(line: usize, total_lines: usize) -> usize {
    if total_lines <= 1 {
        // A one-line file is entirely on screen; anything other than 100 would
        // be arithmetic pretending to be information.
        return 100;
    }
    ((line + 1) * 100 / total_lines).min(100)
}

/// The filetype shown for a path.
///
/// The extension itself, not a language name. A name table (`rs` → `Rust`) is
/// exactly the kind of thing that looks like a small addition and becomes a
/// maintenance surface, and at M2.5 the honest answer arrives for free: the
/// filetype *is* the tree-sitter grammar that claimed the file. Inventing a
/// second naming scheme now would mean throwing one away then.
pub fn file_type_of(path: Option<&std::path::Path>) -> Option<String> {
    path?
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_lowercase)
}
