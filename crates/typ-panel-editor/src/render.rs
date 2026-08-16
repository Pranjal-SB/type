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

/// What a cell is painted as. Ordered by precedence, highest last.
///
/// Spelling this out as a type rather than as a chain of `if`s inside the loop
/// is what makes the precedence reviewable: there is exactly one place that
/// decides, and adding syntax highlighting at M2.5 adds a variant here rather
/// than another branch in the middle of a run-accumulating loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Paint {
    Plain,
    /// The line a caret sits on.
    CursorLine,
    /// A bracket and its partner.
    Bracket,
    /// One of the non-primary selections.
    Selection,
    /// The selection every motion is relative to.
    PrimarySelection,
}

impl Paint {
    fn style(self, theme: &ThemeColors) -> Style {
        match self {
            Paint::Plain => Style::default().fg(theme.fg).bg(theme.bg),
            Paint::CursorLine => Style::default().fg(theme.fg).bg(theme.cursor_line_bg),
            Paint::Bracket => Style::default()
                .fg(theme.bracket_match_fg)
                .bg(theme.bracket_match_bg),
            Paint::Selection => Style::default()
                .fg(theme.selection_fg)
                .bg(theme.selection_bg),
            Paint::PrimarySelection => Style::default()
                .fg(theme.selection_fg)
                .bg(theme.selection_primary_bg),
        }
    }
}

/// Everything needed to draw one visible line.
///
/// A struct rather than nine positional arguments: the call site was already at
/// six and the three effects added here would have made it a row of unlabelled
/// values where swapping two `usize`s compiles cleanly and renders wrong.
pub struct LineStyle<'a> {
    pub line: usize,
    pub left_col: usize,
    /// Text-area width in cells, for padding the current-line highlight out to
    /// the edge.
    pub width: usize,
    pub tab_width: usize,
    pub selections: &'a [Selection],
    pub primary: Selection,
    /// Whether a caret sits on this line *with nothing selected*. A line
    /// carrying a real selection does not also get the stripe — the selection
    /// is already saying where the user is, and two answers to one question is
    /// how a interface starts to look busy.
    pub cursor_line: bool,
    pub brackets: Option<(Position, Position)>,
    pub theme: &'a ThemeColors,
}

/// Build one rendered line, splitting it into spans wherever the paint changes.
///
/// Spans are cut at grapheme boundaries and styled per run, so a wide
/// character is highlighted as one unit and never half-painted.
pub fn styled_line(text: &str, ctx: &LineStyle) -> Line<'static> {
    // Selections are stated in grapheme columns of the whole line, so the
    // dropped count is what keeps highlighting on the text it covers rather
    // than sliding left with the window.
    let (visible, skipped) = window(text, ctx.left_col, ctx.tab_width);

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut current = String::new();
    let mut current_paint: Option<Paint> = None;
    let mut columns = 0usize;

    for (offset, grapheme) in visible.graphemes(true).enumerate() {
        let position = Position {
            line: ctx.line,
            col: skipped + offset,
        };
        let paint = paint_for(position, ctx);

        if current_paint != Some(paint) && !current.is_empty() {
            let style = current_paint.unwrap_or(Paint::Plain).style(ctx.theme);
            spans.push(Span::styled(std::mem::take(&mut current), style));
        }
        current_paint = Some(paint);
        current.push_str(grapheme);
        columns += display_width_with_tabs(grapheme, ctx.tab_width).max(1);
    }

    if !current.is_empty() {
        let style = current_paint.unwrap_or(Paint::Plain).style(ctx.theme);
        spans.push(Span::styled(current, style));
    }

    // Carry the current-line tint past the end of the text. A highlight that
    // stops at the last character leaves a ragged right edge that reads as a
    // rendering bug rather than as a feature.
    if ctx.cursor_line && columns < ctx.width {
        spans.push(Span::styled(
            " ".repeat(ctx.width - columns),
            Paint::CursorLine.style(ctx.theme),
        ));
    }

    Line::from(spans)
}

fn paint_for(position: Position, ctx: &LineStyle) -> Paint {
    if ctx.selections.iter().any(|s| s.contains(position)) {
        // A selection outranks a bracket: both mean "where you are", and the
        // selection is the one the next keystroke acts on.
        if ctx.primary.contains(position) {
            Paint::PrimarySelection
        } else {
            Paint::Selection
        }
    } else if ctx
        .brackets
        .is_some_and(|(open, close)| open == position || close == position)
    {
        Paint::Bracket
    } else if ctx.cursor_line {
        Paint::CursorLine
    } else {
        Paint::Plain
    }
}
