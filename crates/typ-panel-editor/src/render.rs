//! Turning a line of text plus the selections covering it into styled spans.
//!
//! Split out of `lib.rs` because this is where display-column arithmetic
//! lives, and it is the part most likely to grow: highlighting arrives in M2.5
//! and has to compose with selection styling rather than fight it.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use typ_buffer::{Position, Selection, display_width_with_tabs};
use typ_core::ThemeColors;
use unicode_segmentation::UnicodeSegmentation;

/// Drop `left_col` display columns from the front of a line.
///
/// Returns the remaining text and how many graphemes were dropped, because the
/// caller still has to line the result up against selections that are stated in
/// grapheme columns.
///
/// A wide grapheme straddling the boundary is dropped entirely rather than
/// half-drawn: a terminal cannot render half a cell, so the alternatives are a
/// dropped character or a row one column out of alignment with every other row.
/// Slicing by display column rather than by grapheme is the whole point — a line
/// of CJK scrolls by cells the way it is drawn.
pub fn window(text: &str, left_col: usize, tab_width: usize) -> (&str, usize) {
    if left_col == 0 {
        return (text, 0);
    }

    let mut column = 0usize;
    for (skipped, (byte, grapheme)) in text.grapheme_indices(true).enumerate() {
        if column >= left_col {
            return (&text[byte..], skipped);
        }
        // `.max(1)` so a zero-width grapheme cannot stall the walk. Tabs are
        // measured from their real column, which is why this tracks `column`
        // rather than summing widths in isolation.
        column += if grapheme == "\t" {
            tab_width - (column % tab_width)
        } else {
            display_width_with_tabs(grapheme, tab_width).max(1)
        };
    }
    // Scrolled entirely past the end of this line.
    ("", text.graphemes(true).count())
}

/// Build one rendered line, splitting it into spans wherever the selection
/// state changes.
///
/// Spans are cut at grapheme boundaries and styled per run, so a wide
/// character is highlighted as one unit and never half-painted.
pub fn styled_line(
    text: &str,
    line_index: usize,
    left_col: usize,
    tab_width: usize,
    selections: &[Selection],
    theme: &ThemeColors,
) -> Line<'static> {
    let plain = Style::default().fg(theme.fg).bg(theme.bg);
    let selected = Style::default()
        .fg(theme.selection_fg)
        .bg(theme.selection_bg);

    // Selections are stated in grapheme columns of the whole line, so the
    // dropped count is what keeps highlighting on the text it covers rather
    // than sliding left with the window.
    let (visible, skipped) = window(text, left_col, tab_width);

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut current = String::new();
    let mut current_selected: Option<bool> = None;

    for (offset, grapheme) in visible.graphemes(true).enumerate() {
        let position = Position {
            line: line_index,
            col: skipped + offset,
        };
        let is_selected = selections.iter().any(|s| s.contains(position));

        if current_selected != Some(is_selected) && !current.is_empty() {
            let style = if current_selected == Some(true) {
                selected
            } else {
                plain
            };
            spans.push(Span::styled(std::mem::take(&mut current), style));
        }
        current_selected = Some(is_selected);
        current.push_str(grapheme);
    }

    if !current.is_empty() {
        let style = if current_selected == Some(true) {
            selected
        } else {
            plain
        };
        spans.push(Span::styled(current, style));
    }

    Line::from(spans)
}
