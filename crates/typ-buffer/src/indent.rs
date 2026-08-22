//! Measuring a file's indent width instead of asserting one.
//!
//! A port of VS Code's `guessIndentation`
//! (`src/vs/editor/common/model/indentationGuesser.ts`, MIT). Chosen over the
//! shorter histogram-of-deltas approach ttt uses, for reasons that were read
//! out of both sources rather than assumed:
//!
//! - ttt buckets deltas into a Go map and takes the maximum with a strict `>`,
//!   and Go randomises map iteration. A file with equal counts of 2- and
//!   4-deltas detects 2 on one run and 4 on the next. Scoring in a fixed
//!   preference order fixes that and costs nothing.
//! - ttt counts a tab as one column, so a file containing both units produces
//!   deltas that are arithmetic over two different things. Counting tabs and
//!   spaces separately per line, and rejecting any line that mixes them, keeps
//!   the units apart.
//! - Continuation alignment reads as indentation unless something looks for
//!   it, and it is common enough in real source to matter.
//!
//! Pure over an iterator of lines rather than over a `TextBuffer`: the tests
//! need no buffer, and the signature makes it impossible to reach for
//! `line_text` in a loop.

/// Widths worth guessing, in the order they are scored.
///
/// Even before odd, and the scoring comparison is strict `>`, so the earliest
/// entry wins a tie. That is the whole determinism guarantee.
const ALLOWED: [usize; 7] = [2, 4, 6, 8, 3, 5, 7];

/// `max(ALLOWED)`. A delta wider than this is not an indent level.
const MAX_GUESS: usize = 8;

/// How two lines' indentation differs, and whether the difference is really
/// alignment rather than an indent level.
#[derive(Default)]
struct SpacesDiff {
    spaces: usize,
    looks_like_alignment: bool,
}

/// Compare the indentation of `a` (indent `a_len` bytes) with `b` (indent
/// `b_len` bytes).
///
/// Byte indices throughout: indentation is ASCII, and comparing bytes means an
/// index landing inside a multi-byte character elsewhere on the line cannot
/// panic.
fn spaces_diff(a: &str, a_len: usize, b: &str, b_len: usize) -> SpacesDiff {
    let (ab, bb) = (a.as_bytes(), b.as_bytes());

    // Skip the shared prefix first, so `"\t"` -> `"\t    "` reads as four
    // spaces rather than as a reset to nothing.
    let mut i = 0;
    while i < a_len && i < b_len && ab[i] == bb[i] {
        i += 1;
    }

    let count = |line: &[u8], end: usize| -> (usize, usize) {
        let mut spaces = 0;
        let mut tabs = 0;
        for &byte in &line[i..end] {
            if byte == b' ' {
                spaces += 1;
            } else {
                tabs += 1;
            }
        }
        (spaces, tabs)
    };
    let (a_spaces, a_tabs) = count(ab, a_len);
    let (b_spaces, b_tabs) = count(bb, b_len);

    // A differing region that is part tab and part space says nothing about
    // either unit.
    if (a_spaces > 0 && a_tabs > 0) || (b_spaces > 0 && b_tabs > 0) {
        return SpacesDiff::default();
    }

    let tabs_diff = a_tabs.abs_diff(b_tabs);
    let spaces_diff = a_spaces.abs_diff(b_spaces);

    if tabs_diff == 0 {
        // ```text
        // const a = b + c,
        //       d = b - c;
        // ```
        // Alignment, not a six-wide indent: the previous line ends in a comma
        // and holds a space at the column just before the two diverge.
        let looks_like_alignment = spaces_diff > 0
            && b_spaces >= 1
            && b_spaces - 1 < ab.len()
            && b_spaces < bb.len()
            && bb[b_spaces] != b' '
            && ab[b_spaces - 1] == b' '
            && ab[ab.len() - 1] == b',';
        return SpacesDiff {
            spaces: spaces_diff,
            looks_like_alignment,
        };
    }

    // The unit changed between the two lines. Eight spaces where two tabs
    // stood is four spaces to the tab.
    if spaces_diff % tabs_diff == 0 {
        return SpacesDiff {
            spaces: spaces_diff / tabs_diff,
            looks_like_alignment: false,
        };
    }
    SpacesDiff::default()
}

/// The indent width this file was written with, if it says.
///
/// `None` for a file that never indents, and for one indented with tabs — a
/// tab's display width is the reader's preference, not a fact about the file.
/// Either way the caller keeps its own default.
///
/// The caller also applies any line cap. VS Code stops at 10,000 lines, but
/// where that limit belongs is a question about buffers, not about counting.
pub fn detect_indent_width<'a>(lines: impl Iterator<Item = &'a str>) -> Option<usize> {
    // Scores indexed by candidate width; `0..=MAX_GUESS`.
    let mut scores = [0usize; MAX_GUESS + 1];
    // The last line that had content, and where its content started.
    let mut previous = "";
    let mut previous_indent = 0;

    for line in lines {
        // Blank and whitespace-only lines are not evidence of anything.
        let Some(indent) = line.bytes().position(|b| b != b'\t' && b != b' ') else {
            continue;
        };

        let diff = spaces_diff(previous, previous_indent, line, indent);
        // Alignment contributes nothing and does not become the line the next
        // one is compared against, so a wrapped argument list cannot drag the
        // whole file's baseline sideways.
        if diff.looks_like_alignment {
            continue;
        }
        if diff.spaces <= MAX_GUESS {
            scores[diff.spaces] += 1;
        }
        previous = line;
        previous_indent = indent;
    }

    let mut best: Option<usize> = None;
    let mut best_score = 0;
    for width in ALLOWED {
        if scores[width] > best_score {
            best_score = scores[width];
            best = Some(width);
        }
    }

    // VS Code's one special case, and it names the reason: YAML. Deep
    // two-space nesting produces spurious four-deltas, so 2 takes 4's crown
    // once it holds two thirds of 4's count.
    if best == Some(4) && scores[2] > 0 && scores[2] * 3 >= scores[4] * 2 {
        return Some(2);
    }
    best
}
