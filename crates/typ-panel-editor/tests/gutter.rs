//! The gutter: its width, its digits, and the thing it silently breaks.
//!
//! The last of those is why this file has a mouse test in it. A gutter narrows
//! the text area, and every screen-cell-to-buffer-position conversion has to
//! subtract it. Miss one and every click lands `gutter_width` graphemes to the
//! left — which no test of the gutter's own output would ever notice.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_buffer::{Position, Selection};
use typ_core::{Panel, RenderContext, ThemeColors};
use typ_panel_editor::EditorPanel;
use typ_panel_editor::gutter::{Gutter, GutterComponent};

fn render(panel: &mut EditorPanel, area: Rect) -> Buffer {
    let theme = ThemeColors::default();
    let ctx = RenderContext {
        theme: &theme,
        is_focused: true,
        panel_index: 0,
        terminal_width: area.width,
        terminal_height: area.height,
    };
    let mut buf = Buffer::empty(area);
    panel.render(area, &mut buf, &ctx);
    buf
}

fn row(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
}

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 24,
    height: 6,
};

// --- width ---------------------------------------------------------------

#[test]
fn the_gutter_is_as_wide_as_the_longest_line_number() {
    let gutter = Gutter::default();
    // One digit plus the spacer that keeps the digits off the text.
    assert_eq!(gutter.width(9), 2);
    assert_eq!(gutter.width(10), 3);
    assert_eq!(gutter.width(100_000), 7);
}

#[test]
fn a_single_line_file_still_gets_a_column() {
    let gutter = Gutter::default();
    assert_eq!(gutter.width(1), 2, "line 1 needs one digit and a space");
}

#[test]
fn width_comes_from_the_whole_buffer_not_the_visible_lines() {
    // Scrolled to the top of a 200-line file, only lines 1..20 are on screen —
    // but the column must already be three wide, or the text shifts sideways
    // the moment the view reaches line 100.
    let gutter = Gutter::default();
    assert_eq!(gutter.width(200), 4);
}

#[test]
fn an_empty_gutter_takes_no_cells() {
    let gutter = Gutter::new(vec![]);
    assert_eq!(gutter.width(50_000), 0);
}

#[test]
fn components_that_ship_empty_still_reserve_their_column() {
    // Diagnostics arrive at M3 and diff markers at M5. Both draw nothing today
    // and both hold a cell, so filling them in later is writing a function
    // rather than re-laying-out the editor.
    let gutter = Gutter::new(vec![GutterComponent::Diagnostics, GutterComponent::Diff]);
    assert_eq!(gutter.width(9), 2);
}

// --- what it draws -------------------------------------------------------

#[test]
fn line_numbers_start_at_one() {
    let mut panel = EditorPanel::from_str("alpha\nbeta\n");
    let buf = render(&mut panel, AREA);
    assert!(
        row(&buf, 1).starts_with(" 1 alpha"),
        "row 1 was: {}",
        row(&buf, 1)
    );
    assert!(
        row(&buf, 2).starts_with(" 2 beta"),
        "row 2 was: {}",
        row(&buf, 2)
    );
}

#[test]
fn numbers_are_right_aligned_so_the_text_edge_stays_straight() {
    let text: String = (1..=10).map(|i| format!("line {i}\n")).collect();
    let mut panel = EditorPanel::from_str(&text);
    let buf = render(&mut panel, AREA);
    // Two-digit file: line 1 pads to " 1", line 10 does not pad.
    assert!(
        row(&buf, 1).starts_with("  1 line 1"),
        "row 1 was: {}",
        row(&buf, 1)
    );
}

#[test]
fn the_cursors_line_is_styled_differently_from_the_rest() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("alpha\nbeta\n");
    let buf = render(&mut panel, AREA);
    // The cursor starts on line 0, drawn at row 1 inside the border.
    let current = buf[(1, 1)].style().fg;
    let other = buf[(1, 2)].style().fg;
    assert_ne!(
        current, other,
        "the cursor's line number must stand out from the others"
    );
    assert_eq!(other, Some(theme.line_number_fg));
    assert_eq!(current, Some(theme.line_number_current_fg));
}

#[test]
fn the_number_column_does_not_scroll_sideways_with_the_text() {
    let mut panel = EditorPanel::from_str(&format!("{}\n", "x".repeat(200)));
    render(&mut panel, AREA);
    panel.apply_action(typ_core::Action::Move {
        motion: typ_core::Motion::LineEnd,
        extend: false,
    });
    let buf = render(&mut panel, AREA);
    assert!(panel.left_col() > 0, "the text must have scrolled");
    assert!(
        row(&buf, 1).starts_with(" 1 "),
        "the gutter is fixed furniture, not part of the scrolled text: {}",
        row(&buf, 1)
    );
}

// --- the thing it silently breaks ----------------------------------------

#[test]
fn a_click_lands_on_the_grapheme_under_the_pointer_not_gutter_width_to_its_left() {
    let mut panel = EditorPanel::from_str("hello\nworld\n");
    render(&mut panel, AREA);

    // Border at x=0, gutter "1 " at x=1..3, so the text starts at x=3 and the
    // third grapheme of "hello" is drawn at x=5.
    panel.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 1,
            modifiers: KeyModifiers::NONE,
        },
        AREA,
    );
    assert_eq!(panel.cursor(), Position { line: 0, col: 2 });
}

#[test]
fn the_terminal_cursor_is_drawn_past_the_gutter() {
    let mut panel = EditorPanel::from_str("hello\n");
    render(&mut panel, AREA);
    panel.apply_action(typ_core::Action::Move {
        motion: typ_core::Motion::Right,
        extend: false,
    });
    let (x, y) = panel
        .cursor_position(AREA)
        .expect("the cursor is on screen");
    // One border, two gutter cells, one grapheme in.
    assert_eq!((x, y), (4, 1));
}

#[test]
fn a_click_in_the_gutter_lands_at_the_start_of_that_line() {
    let mut panel = EditorPanel::from_str("hello\nworld\n");
    render(&mut panel, AREA);
    panel.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: 2,
            modifiers: KeyModifiers::NONE,
        },
        AREA,
    );
    assert_eq!(
        panel.cursor(),
        Position { line: 1, col: 0 },
        "clicking the number selects the line it labels"
    );
}

#[test]
fn the_text_area_loses_exactly_the_gutters_width() {
    // 24 wide, two borders, two gutter cells: 20 columns of text. A line one
    // grapheme longer than that must scroll, and one exactly that long must not.
    let mut panel = EditorPanel::from_str(&format!("{}\n", "x".repeat(20)));
    render(&mut panel, AREA);
    panel.apply_action(typ_core::Action::Move {
        motion: typ_core::Motion::LineEnd,
        extend: false,
    });
    render(&mut panel, AREA);
    assert_eq!(
        panel.left_col(),
        1,
        "a cursor one past the last visible column scrolls by exactly one"
    );
}

// --- relative numbering --------------------------------------------------

#[test]
fn relative_numbering_counts_distance_from_the_cursor() {
    let theme = ThemeColors::default();
    let gutter = Gutter::new(vec![GutterComponent::LineNumbers { relative: true }]);
    let content = |line| {
        gutter.render_line(line, 5, 20, &theme)[0]
            .content
            .to_string()
    };

    assert_eq!(content(3).trim(), "2", "two lines above");
    assert_eq!(content(7).trim(), "2", "two lines below, same number");
    assert_eq!(
        content(5).trim(),
        "6",
        "the cursor's own line keeps its absolute number — the pair is what \
         makes relative numbering useful, rather than a lone zero"
    );
}

#[test]
fn relative_numbering_is_off_by_default() {
    // TYPE is non-modal by default and relative numbers are a modal idiom. The
    // field exists so the vim layer flips a bool rather than replacing the
    // component; the default stays absolute.
    let theme = ThemeColors::default();
    let absolute = Gutter::default().render_line(3, 5, 20, &theme)[0]
        .content
        .to_string();
    assert_eq!(absolute.trim(), "4");
}

#[test]
fn a_line_carrying_a_selection_does_not_tint_its_gutter() {
    // The text does not tint a line that has a real selection on it — the
    // selection already says where the user is. The gutter has to agree, or the
    // number lights up on a row whose text stayed plain.
    let mut panel = EditorPanel::from_str("alpha\nbeta\n");
    let theme = ThemeColors::default();
    panel.set_selections_for_test(vec![Selection {
        anchor: Position { line: 0, col: 0 },
        head: Position { line: 0, col: 3 },
    }]);
    let buf = render(&mut panel, Rect::new(0, 0, 30, 6));

    assert_eq!(buf[(1, 1)].bg, theme.gutter_bg);
}
