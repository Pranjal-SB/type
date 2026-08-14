//! Turning a line of text plus the selections covering it into styled spans.
//!
//! Split out of `lib.rs` because this is where display-column arithmetic
//! lives, and it is the part most likely to grow: highlighting arrives in M2.5
//! and has to compose with selection styling rather than fight it.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use typ_buffer::{Position, Selection};
use typ_core::ThemeColors;
use unicode_segmentation::UnicodeSegmentation;

/// Build one rendered line, splitting it into spans wherever the selection
/// state changes.
///
/// Spans are cut at grapheme boundaries and styled per run, so a wide
/// character is highlighted as one unit and never half-painted.
pub fn styled_line(
    text: &str,
    line_index: usize,
    selections: &[Selection],
    theme: &ThemeColors,
) -> Line<'static> {
    let plain = Style::default().fg(theme.fg).bg(theme.bg);
    let selected = Style::default()
        .fg(theme.selection_fg)
        .bg(theme.selection_bg);

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut current = String::new();
    let mut current_selected: Option<bool> = None;

    for (col, grapheme) in text.graphemes(true).enumerate() {
        let position = Position {
            line: line_index,
            col,
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
