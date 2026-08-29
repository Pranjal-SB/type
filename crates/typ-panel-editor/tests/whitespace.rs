//! Whitespace marks: a dot for a space, an arrow for a tab, and only where the
//! setting asks for them.
//!
//! Four values rather than VS Code's five. `boundary` — "every run except a
//! single space between words" — is cut because it needs word segmentation
//! inside the render loop and is the least useful of the five.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_buffer::{Position, Selection};
use typ_core::{Panel, RenderContext, ThemeColors};
use typ_panel_editor::EditorPanel;
use typ_panel_editor::render::Whitespace;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 24,
    height: 6,
};

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

fn render(panel: &mut EditorPanel) -> Buffer {
    let theme = ThemeColors::default();
    let ctx = RenderContext {
        theme: &theme,
        syntax: typ_core::SyntaxTheme::empty(),
        diagnostics: &[],
        is_focused: true,
        panel_index: 0,
        terminal_width: AREA.width,
        terminal_height: AREA.height,
    };
    let mut buf = Buffer::empty(AREA);
    panel.render(AREA, &mut buf, &ctx);
    buf
}

/// Screen x for a display column of text: the frame, then a two-cell gutter for
/// the small fixtures here.
fn tx(col: u16) -> u16 {
    // Asked rather than hardcoded -- the default gutter grew a diagnostic sign
    // at M3, and a constant here would have made that land as a selection bug.
    1 + typ_panel_editor::gutter::Gutter::default().width(1) as u16 + col
}

/// Screen y for a buffer line at the top of the viewport.
fn ty(line: u16) -> u16 {
    1 + line
}

/// The whole rendered row of text, as a string of what each cell shows.
fn row(buf: &Buffer, line: u16, cols: u16) -> String {
    (0..cols)
        .map(|col| buf[(tx(col), ty(line))].symbol())
        .collect()
}

// --- the substitution ----------------------------------------------------

#[test]
fn all_marks_every_space_and_every_tab() {
    let mut panel = EditorPanel::from_str("a b\n");
    panel.set_tab_width(4);
    panel.set_whitespace(Whitespace::All);
    let buf = render(&mut panel);
    assert_eq!(row(&buf, 0, 3), "a·b");
}

#[test]
fn none_leaves_a_space_a_space() {
    let mut panel = EditorPanel::from_str("a b\n");
    panel.set_whitespace(Whitespace::None);
    let buf = render(&mut panel);
    assert_eq!(row(&buf, 0, 3), "a b");
}

#[test]
fn a_marked_tab_still_occupies_its_full_column_count() {
    // The trap this test exists for: a tab drawn as one arrow that does not
    // pad out to its tab stop loses three columns, and every line after it is
    // misaligned against a cursor that is still counting the tab as four.
    let mut panel = EditorPanel::from_str("\tx\n");
    panel.set_tab_width(4);
    panel.set_whitespace(Whitespace::All);
    let buf = render(&mut panel);
    assert_eq!(row(&buf, 0, 5), "→   x");
}

#[test]
fn an_unmarked_tab_occupies_the_same_columns_a_marked_one_does() {
    // Otherwise selecting a line that contains a tab would shift its text three
    // columns to the left, which is a worse bug than the one the marks fix.
    let mut panel = EditorPanel::from_str("\tx\n");
    panel.set_tab_width(4);
    panel.set_whitespace(Whitespace::None);
    let buf = render(&mut panel);
    // The leading cell is an indent guide, not whitespace: one tab at a width
    // of four is one full level, so the line is genuinely a level deep. What
    // this test holds is the column count after it - four cells before `x`.
    assert_eq!(row(&buf, 0, 5), "│   x");
}

#[test]
fn a_mark_is_drawn_in_the_themes_whitespace_colour_over_the_cells_own_ground() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("a b\ncd\n");
    panel.set_whitespace(Whitespace::All);
    panel.set_selections_for_test(vec![Selection {
        anchor: pos(0, 0),
        head: pos(0, 3),
    }]);
    let buf = render(&mut panel);

    let cell = &buf[(tx(1), ty(0))];
    assert_eq!(cell.symbol(), "·");
    assert_eq!(cell.fg, theme.whitespace, "the mark takes the mark colour");
    assert_eq!(
        cell.bg, theme.selection_primary_bg,
        "and keeps the ground the cell was already on"
    );
}

// --- which whitespace ----------------------------------------------------

#[test]
fn selection_marks_inside_the_selection_and_not_one_column_outside_it() {
    let mut panel = EditorPanel::from_str("a b c\n");
    panel.set_whitespace(Whitespace::Selection);
    // Covers "a b" — the space at column 1 is inside, the one at column 3 is
    // one past the end.
    panel.set_selections_for_test(vec![Selection {
        anchor: pos(0, 0),
        head: pos(0, 3),
    }]);
    let buf = render(&mut panel);
    assert_eq!(row(&buf, 0, 5), "a·b c");
}

#[test]
fn selection_marks_nothing_when_nothing_is_selected() {
    // The default, and the reason it is the default: a bare caret costs no
    // marks at all.
    let mut panel = EditorPanel::from_str("a b\n");
    panel.set_whitespace(Whitespace::Selection);
    let buf = render(&mut panel);
    assert_eq!(row(&buf, 0, 3), "a b");
}

#[test]
fn trailing_marks_the_tail_and_leaves_the_indent_alone() {
    // The one value that catches a defect rather than answering curiosity.
    let mut panel = EditorPanel::from_str("  a  \n");
    panel.set_whitespace(Whitespace::Trailing);
    let buf = render(&mut panel);
    // Column 0 is an indent guide. Detection reads this file as two-space
    // indented - two columns is a real delta against an empty preceding line -
    // so the row sits one level deep and the guide belongs there. What
    // Trailing is held to is that the indent keeps no marks and the tail does.
    assert_eq!(row(&buf, 0, 5), "│ a··");
}

#[test]
fn a_line_that_is_nothing_but_whitespace_is_all_trailing() {
    let mut panel = EditorPanel::from_str("x\n   \n");
    panel.set_whitespace(Whitespace::Trailing);
    let buf = render(&mut panel);
    assert_eq!(row(&buf, 1, 3), "···");
}

#[test]
fn selection_is_the_default() {
    // VS Code's, and the right one: whitespace shows where it is diagnostic and
    // nowhere else.
    assert_eq!(Whitespace::default(), Whitespace::Selection);
}
