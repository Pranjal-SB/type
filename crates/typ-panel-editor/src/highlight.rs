//! Turning a parsed tree into per-line, per-column styles for the render loop.
//!
//! Three conversions happen here, once per frame, and each is here rather than
//! in the render loop because the loop runs once per *cell*:
//!
//! 1. One `highlights` query for the whole viewport, not one per line.
//! 2. Scope indices resolved against the theme once per distinct scope, not
//!    once per span — `SyntaxTheme::get` walks dot-separated prefixes through a
//!    `BTreeMap`.
//! 3. Byte offsets converted to grapheme columns, because invariant 4 says
//!    `col` is a grapheme index everywhere.

use std::ops::Range;

use ratatui::style::Style;
use ropey::Rope;
use typ_core::SyntaxTheme;
use typ_syntax::{Scope, Syntax};
use unicode_segmentation::UnicodeSegmentation;

/// Resolved styles for each line in `lines`, in grapheme columns.
///
/// One entry per line requested, empty where nothing is captured.
pub(crate) fn for_viewport(
    syntax: &Syntax,
    rope: &Rope,
    theme: &SyntaxTheme,
    lines: Range<usize>,
) -> Vec<Vec<(Range<usize>, Style)>> {
    let count = lines.end.saturating_sub(lines.start);
    let mut per_line = vec![Vec::new(); count];
    if count == 0 || theme.is_empty() {
        // No `[syntax]` table means every lookup returns `None`. Skipping the
        // query entirely is the difference between a theme that does not
        // highlight and a frame that walks the tree to learn that.
        return per_line;
    }

    let spans = syntax.highlights(rope, lines.clone());
    let mut cache = StyleCache::default();
    let len_lines = rope.len_lines();

    // Both sequences are ascending, so this walks each once.
    let mut first = 0usize;
    for (idx, line) in (lines.start..lines.end).enumerate() {
        if line >= len_lines {
            break;
        }
        let line_start = rope.line_to_byte(line);
        let line_end = if line + 1 >= len_lines {
            rope.len_bytes()
        } else {
            rope.line_to_byte(line + 1)
        };

        while first < spans.len() && spans[first].end <= line_start {
            first += 1;
        }

        // A block comment or a multi-line string is one span crossing several
        // lines. Clip rather than assign it to the line it started on, or the
        // rest of it renders unstyled.
        let mut bytes: Vec<(Range<usize>, Style)> = Vec::new();
        let mut at = first;
        while at < spans.len() && spans[at].start < line_end {
            let span = &spans[at];
            let start = span.start.max(line_start) - line_start;
            let end = span.end.min(line_end) - line_start;
            if start < end
                && let Some(style) = cache.get(theme, span.scope)
            {
                bytes.push((start..end, style));
            }
            at += 1;
        }

        if !bytes.is_empty() {
            let slice = rope.line(line);
            per_line[idx] = match slice.as_str() {
                Some(text) => to_columns(text, &bytes),
                // A line straddling a ropey chunk. Rare — chunks are about a
                // kilobyte — and the owned string is the price of not having a
                // borrowed one.
                None => to_columns(&slice.to_string(), &bytes),
            };
        }
    }

    per_line
}

/// Scope index to style, resolved at most once per frame per scope.
///
/// `SyntaxTheme::get` walks dot-separated prefixes through a `BTreeMap` on a
/// string key. Doing that per span would put a map walk on the frame path once
/// for every keyword on screen; doing it per cell would put it there thousands
/// of times.
#[derive(Default)]
struct StyleCache {
    /// Indexed by scope. `None` means not yet looked up, `Some(None)` means the
    /// theme has nothing for it — a distinction worth keeping, because "the
    /// theme does not style comments" is the common case and re-resolving it
    /// every frame is exactly the cost this type exists to avoid.
    styles: Vec<Option<Option<Style>>>,
}

impl StyleCache {
    fn get(&mut self, theme: &SyntaxTheme, scope: Scope) -> Option<Style> {
        let idx = scope.0 as usize;
        if idx >= self.styles.len() {
            self.styles.resize(idx + 1, None);
        }
        *self.styles[idx].get_or_insert_with(|| theme.get(typ_syntax::scope_name(scope)))
    }
}

/// Byte offsets into one line become grapheme columns.
///
/// Invariant 4, enforced at the panel boundary: everything below this point
/// counts graphemes, and a render loop handed byte offsets would be wrong on
/// the first non-ASCII line while every ASCII test kept passing.
fn to_columns(text: &str, spans: &[(Range<usize>, Style)]) -> Vec<(Range<usize>, Style)> {
    // The overwhelmingly common case, and the whole conversion collapses: one
    // byte is one grapheme, so the offsets are already columns.
    if text.is_ascii() {
        return spans.to_vec();
    }

    let mut boundaries: Vec<usize> = text.grapheme_indices(true).map(|(byte, _)| byte).collect();
    boundaries.push(text.len());

    spans
        .iter()
        .map(|(range, style)| {
            let start = boundaries.partition_point(|&b| b < range.start);
            let end = boundaries.partition_point(|&b| b < range.end);
            (start..end, *style)
        })
        .collect()
}
