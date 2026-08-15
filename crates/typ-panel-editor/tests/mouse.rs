use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use typ_buffer::Position;
use typ_core::Panel;
use typ_panel_editor::EditorPanel;

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 40,
    height: 10,
};

fn at(kind: MouseEventKind, column: u16, row: u16, modifiers: KeyModifiers) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers,
    }
}

fn down(column: u16, row: u16) -> MouseEvent {
    at(
        MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        KeyModifiers::NONE,
    )
}

fn drag(column: u16, row: u16) -> MouseEvent {
    at(
        MouseEventKind::Drag(MouseButton::Left),
        column,
        row,
        KeyModifiers::NONE,
    )
}

#[test]
fn a_click_places_a_caret_and_clears_any_selection() {
    let mut p = EditorPanel::from_str("hello\nworld\n");
    p.handle_mouse(down(3, 2), AREA);
    assert_eq!(p.cursor(), pos(1, 2));
    assert!(p.selections().primary().is_empty());
    assert_eq!(p.selections().len(), 1);
}

#[test]
fn dragging_extends_from_where_the_press_landed() {
    let mut p = EditorPanel::from_str("hello world\n");
    p.handle_mouse(down(1, 1), AREA);
    p.handle_mouse(drag(6, 1), AREA);
    let s = p.selections().primary();
    assert_eq!(s.anchor, pos(0, 0));
    assert_eq!(s.head, pos(0, 5));
}

#[test]
fn dragging_backwards_selects_the_same_text() {
    let mut p = EditorPanel::from_str("hello world\n");
    p.handle_mouse(down(6, 1), AREA);
    p.handle_mouse(drag(1, 1), AREA);
    assert_eq!(p.selections().primary().range(), (pos(0, 0), pos(0, 5)));
}

#[test]
fn dragging_across_lines_selects_across_them() {
    let mut p = EditorPanel::from_str("abc\ndef\n");
    p.handle_mouse(down(2, 1), AREA);
    p.handle_mouse(drag(2, 2), AREA);
    assert_eq!(p.selections().primary().range(), (pos(0, 1), pos(1, 1)));
}

#[test]
fn a_drag_without_a_press_does_not_start_a_selection() {
    let mut p = EditorPanel::from_str("hello\n");
    p.handle_mouse(drag(4, 1), AREA);
    assert!(p.selections().primary().is_empty());
}

#[test]
fn alt_click_adds_a_cursor_instead_of_replacing_the_one_there() {
    let mut p = EditorPanel::from_str("abc\ndef\n");
    p.handle_mouse(down(1, 1), AREA);
    p.handle_mouse(
        at(
            MouseEventKind::Down(MouseButton::Left),
            2,
            2,
            KeyModifiers::ALT,
        ),
        AREA,
    );
    assert_eq!(p.selections().len(), 2);
    let heads: Vec<Position> = p.selections().iter().map(|s| s.head).collect();
    assert_eq!(heads, vec![pos(0, 0), pos(1, 1)]);
}

#[test]
fn a_second_click_in_the_same_place_selects_the_word() {
    let mut p = EditorPanel::from_str("let value = 1;\n");
    p.handle_mouse(down(6, 1), AREA);
    p.handle_mouse(down(6, 1), AREA);
    assert_eq!(p.selections().primary().range(), (pos(0, 4), pos(0, 9)));
}

#[test]
fn a_second_click_somewhere_else_is_just_another_click() {
    let mut p = EditorPanel::from_str("let value = 1;\n");
    p.handle_mouse(down(6, 1), AREA);
    p.handle_mouse(down(2, 1), AREA);
    assert!(p.selections().primary().is_empty());
    assert_eq!(p.cursor(), pos(0, 1));
}

#[test]
fn releasing_the_button_ends_the_drag() {
    let mut p = EditorPanel::from_str("hello world\n");
    p.handle_mouse(down(1, 1), AREA);
    p.handle_mouse(
        at(
            MouseEventKind::Up(MouseButton::Left),
            6,
            1,
            KeyModifiers::NONE,
        ),
        AREA,
    );
    p.handle_mouse(drag(9, 1), AREA);
    // The drag after the release must not keep extending.
    assert_eq!(p.selections().primary().head, pos(0, 0));
}

#[test]
fn clicking_a_wide_character_selects_that_character_when_clicked_twice() {
    let mut p = EditorPanel::from_str("日本語 ok\n");
    // Column 3 is the right half of 本, which is grapheme 1.
    p.handle_mouse(down(4, 1), AREA);
    assert_eq!(p.cursor(), pos(0, 1));
    p.handle_mouse(down(4, 1), AREA);
    assert_eq!(p.selections().primary().range(), (pos(0, 0), pos(0, 3)));
}
