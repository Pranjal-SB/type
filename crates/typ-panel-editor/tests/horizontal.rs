use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_core::{Action, Motion, Panel, RenderContext, ThemeColors};
use typ_panel_editor::EditorPanel;

fn render(panel: &mut EditorPanel, area: Rect) -> Buffer {
    let theme = ThemeColors::default();
    let ctx = RenderContext {
        theme: &theme,
        syntax: typ_core::SyntaxTheme::empty(),
        diagnostics: &[],
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
    width: 12,
    height: 4,
};

fn to_line_end(panel: &mut EditorPanel) {
    panel.apply_action(Action::Move {
        motion: Motion::LineEnd,
        extend: false,
    });
}

#[test]
fn a_short_line_is_not_scrolled() {
    let mut p = EditorPanel::from_str("abc\n");
    let buf = render(&mut p, AREA);
    assert_eq!(p.left_col(), 0);
    assert_eq!(row(&buf, 1), "│ 1 abc    │");
}

#[test]
fn moving_past_the_right_edge_scrolls_the_view() {
    let mut p = EditorPanel::from_str("abcdefghijklmnop\n");
    render(&mut p, AREA); // learn the width: 12 minus borders = 10 columns
    to_line_end(&mut p);
    let buf = render(&mut p, AREA);
    assert!(p.left_col() > 0, "the view must follow the cursor");
    assert!(
        row(&buf, 1).contains('p'),
        "the end of the line must be visible: {}",
        row(&buf, 1)
    );
}

#[test]
fn coming_back_left_scrolls_the_view_back() {
    let mut p = EditorPanel::from_str("abcdefghijklmnop\n");
    render(&mut p, AREA);
    to_line_end(&mut p);
    render(&mut p, AREA);
    p.apply_action(Action::Move {
        motion: Motion::LineStart,
        extend: false,
    });
    let buf = render(&mut p, AREA);
    assert_eq!(p.left_col(), 0);
    assert!(row(&buf, 1).contains('a'));
}

#[test]
fn the_cursor_is_reported_within_the_visible_window() {
    let mut p = EditorPanel::from_str("abcdefghijklmnop\n");
    render(&mut p, AREA);
    to_line_end(&mut p);
    render(&mut p, AREA);
    let (x, _) = p.cursor_position(AREA).expect("the cursor is on screen");
    assert!((1..11).contains(&x), "cursor x was {x}");
}

#[test]
fn a_wide_character_is_not_split_across_the_left_edge() {
    let mut p = EditorPanel::from_str("日本語日本語日本語\n");
    render(&mut p, AREA);
    to_line_end(&mut p);
    let buf = render(&mut p, AREA);
    let text = row(&buf, 1);
    // A half-drawn CJK cell shows as a stray blank in the first text column:
    // the trailing half of a grapheme whose head has scrolled off.
    //
    // This used to read `!text.starts_with("│ ")`, which could not fail —
    // column 1 is the line number, never a blank — and once the frame stopped
    // drawing a vertical it could not even be reached. Column 4 is where the
    // text actually begins: one margin cell, then the three-cell gutter.
    assert_ne!(
        text.chars().nth(4),
        Some(' '),
        "a wide grapheme was cut in half: {text}"
    );
}

#[test]
fn vertical_scrolling_still_works_alongside_it() {
    let text = (0..50).map(|i| format!("line {i}\n")).collect::<String>();
    let mut p = EditorPanel::from_str(&text);
    render(&mut p, AREA);
    p.handle_scroll(5, AREA);
    let buf = render(&mut p, AREA);
    assert!(row(&buf, 1).contains("line 5"));
}

#[test]
fn a_click_in_a_scrolled_view_lands_on_the_character_under_it() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    let mut p = EditorPanel::from_str("abcdefghijklmnop\n");
    render(&mut p, AREA);
    to_line_end(&mut p);
    render(&mut p, AREA);
    let left = p.left_col();
    assert!(left > 0, "this test is only meaningful once scrolled");

    // Click the first text cell. Inner area starts at x=1 because of the
    // border, so this is display column 0 of the window.
    p.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: 1,
            modifiers: KeyModifiers::NONE,
        },
        AREA,
    );

    assert_eq!(
        p.cursor().col,
        left,
        "a click must account for the horizontal scroll, not just the vertical one"
    );
}

#[test]
fn selection_highlighting_stays_on_the_text_after_scrolling() {
    let mut p = EditorPanel::from_str("abcdefghijklmnop\n");
    render(&mut p, AREA);
    // Select the last two characters, so the highlight sits at a known place
    // inside a scrolled window.
    to_line_end(&mut p);
    p.apply_action(Action::Move {
        motion: Motion::Left,
        extend: true,
    });
    p.apply_action(Action::Move {
        motion: Motion::Left,
        extend: true,
    });
    let buf = render(&mut p, AREA);

    let visible = row(&buf, 1);
    let styled: String = (0..buf.area.width)
        .filter(|x| buf[(*x, 1)].style().bg == Some(ThemeColors::default().selection_primary_bg))
        .map(|x| buf[(x, 1)].symbol())
        .collect();
    assert_eq!(
        styled, "op",
        "the highlight drifted off the text it covers: row {visible}"
    );
}
