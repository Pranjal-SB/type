//! Finding the partner of the bracket at a position.
//!
//! # Bounded on purpose
//!
//! The search takes a line budget and gives up rather than exceeding it. This
//! runs on the render path — the match is recomputed as the cursor moves — and
//! architecture §4 puts a 16 ms ceiling on a keystroke. An unmatched bracket on
//! a 50k-line file is a far smaller cost than a scan of it, so the caller passes
//! its viewport height plus a margin and the answer is "no match" beyond that.
//!
//! # Known limitation, until M2.5
//!
//! This is a character scan with no idea what a string or a comment is, so the
//! `(` in `"a ( b"` counts, and a `)` inside a comment can be matched as a
//! partner. Fixing that needs the syntax tree, which arrives with tree-sitter at
//! M2.5 — at which point this function takes a predicate for "is this position
//! code" and the scan is otherwise unchanged. Recorded here rather than left to
//! be filed as a bug later.

use unicode_segmentation::UnicodeSegmentation;

use crate::buffer::TextBuffer;
use crate::position::Position;

/// The pairs TYPE matches. Angle brackets are deliberately absent: `<` is a
/// comparison far more often than a bracket, and highlighting it as one is
/// wrong more often than it is right.
const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

fn close_for(open: char) -> Option<char> {
    PAIRS.iter().find(|(o, _)| *o == open).map(|(_, c)| *c)
}

fn open_for(close: char) -> Option<char> {
    PAIRS.iter().find(|(_, c)| *c == close).map(|(o, _)| *o)
}

/// The character at a grapheme position, if there is one.
fn char_at(buffer: &TextBuffer, at: Position) -> Option<char> {
    buffer.with_line_str(at.line, |line| {
        line.graphemes(true)
            .nth(at.col)
            .and_then(|g| g.chars().next())
    })
}

/// The matching pair for the bracket at, or immediately before, `at`.
///
/// Returns `(open, close)` in document order regardless of which end was found
/// first, so a caller highlights both without caring which way the scan ran.
///
/// Probing both sides of the cursor matters more than it looks: typing `)`
/// leaves the caret *after* it, which is exactly the moment a user wants to see
/// what it closed.
pub fn match_at(
    buffer: &TextBuffer,
    at: Position,
    max_lines: usize,
) -> Option<(Position, Position)> {
    let mut probes = Vec::with_capacity(2);
    probes.push(at);
    if at.col > 0 {
        probes.push(Position {
            line: at.line,
            col: at.col - 1,
        });
    }

    for probe in probes {
        let Some(ch) = char_at(buffer, probe) else {
            continue;
        };
        if let Some(close) = close_for(ch) {
            if let Some(found) = scan_forward(buffer, probe, ch, close, max_lines) {
                return Some((probe, found));
            }
        } else if let Some(open) = open_for(ch)
            && let Some(found) = scan_backward(buffer, probe, open, ch, max_lines)
        {
            return Some((found, probe));
        }
    }
    None
}

/// Walk one line's graphemes in a fixed direction, tracking nesting depth.
///
/// Returns the grapheme index of the partner if the depth reached zero on this
/// line. `depth` is carried across lines by the callers.
fn scan_line(
    buffer: &TextBuffer,
    line: usize,
    range: impl Iterator<Item = usize>,
    open: char,
    close: char,
    entering: char,
    depth: &mut usize,
) -> Option<usize> {
    // One borrow of the line, then indexed walking. Collecting borrowed slices
    // rather than owned strings keeps this to a single allocation per line, and
    // the line budget keeps the number of lines small.
    buffer.with_line_str(line, |text| {
        let graphemes: Vec<&str> = text.graphemes(true).collect();
        for i in range {
            let Some(c) = graphemes.get(i).and_then(|g| g.chars().next()) else {
                continue;
            };
            if c == entering {
                *depth += 1;
            } else if (c == open || c == close) && c != entering {
                *depth -= 1;
                if *depth == 0 {
                    return Some(i);
                }
            }
        }
        None
    })
}

fn scan_forward(
    buffer: &TextBuffer,
    from: Position,
    open: char,
    close: char,
    max_lines: usize,
) -> Option<Position> {
    let last = (from.line + max_lines).min(buffer.line_count().saturating_sub(1));
    let mut depth = 1usize;

    for line in from.line..=last {
        let count = buffer.line_grapheme_count(line);
        let start = if line == from.line { from.col + 1 } else { 0 };
        if start >= count {
            continue;
        }
        if let Some(col) = scan_line(buffer, line, start..count, open, close, open, &mut depth) {
            return Some(Position { line, col });
        }
    }
    None
}

fn scan_backward(
    buffer: &TextBuffer,
    from: Position,
    open: char,
    close: char,
    max_lines: usize,
) -> Option<Position> {
    let first = from.line.saturating_sub(max_lines);
    let mut depth = 1usize;

    for line in (first..=from.line).rev() {
        let count = buffer.line_grapheme_count(line);
        let end = if line == from.line { from.col } else { count };
        if end == 0 {
            continue;
        }
        // Reversed, so the nearest candidate is met first and nesting unwinds
        // in the order a reader would follow it.
        if let Some(col) = scan_line(buffer, line, (0..end).rev(), open, close, close, &mut depth) {
            return Some(Position { line, col });
        }
    }
    None
}
