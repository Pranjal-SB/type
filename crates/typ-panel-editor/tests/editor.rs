use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use typ_buffer::Position;
use typ_core::{Action, Direction, KeyChord, Motion, Panel, PanelEvent};
use typ_panel_editor::EditorPanel;

fn mv(motion: Motion) -> Action {
    Action::Move {
        motion,
        extend: false,
    }
}

#[test]
fn typing_inserts_text_and_advances_the_cursor() {
    let mut p = EditorPanel::from_str("\n");
    p.apply_action(Action::InsertChar('h'));
    p.apply_action(Action::InsertChar('i'));
    assert_eq!(p.cursor(), Position { line: 0, col: 2 });
}

#[test]
fn arrow_keys_move_the_cursor() {
    let mut p = EditorPanel::from_str("abc\ndef\n");
    p.apply_action(mv(Motion::Right));
    p.apply_action(mv(Motion::Down));
    assert_eq!(p.cursor(), Position { line: 1, col: 1 });
}

#[test]
fn cursor_cannot_move_left_past_the_start() {
    let mut p = EditorPanel::from_str("abc\n");
    p.apply_action(mv(Motion::Left));
    assert_eq!(p.cursor(), Position { line: 0, col: 0 });
}

#[test]
fn moving_down_clamps_the_column_to_a_shorter_line() {
    let mut p = EditorPanel::from_str("abcdef\nab\n");
    for _ in 0..5 {
        p.apply_action(mv(Motion::Right));
    }
    p.apply_action(mv(Motion::Down));
    assert_eq!(p.cursor(), Position { line: 1, col: 2 });
}

#[test]
fn backspace_deletes_the_previous_grapheme() {
    let mut p = EditorPanel::from_str("\n");
    p.apply_action(Action::InsertChar('a'));
    p.apply_action(Action::InsertChar('b'));
    p.apply_action(Action::Delete {
        direction: Direction::Backward,
        by_word: false,
    });
    assert_eq!(p.cursor(), Position { line: 0, col: 1 });
}

#[test]
fn every_action_requests_a_redraw() {
    let mut p = EditorPanel::from_str("\n");
    assert_eq!(
        p.apply_action(Action::InsertChar('a')),
        Some(vec![PanelEvent::NeedsRedraw])
    );
}

#[test]
fn the_editor_has_no_raw_key_behaviour_of_its_own() {
    // Every key that does anything is a keymap row resolving to an Action. A
    // handle_key arm here would be unreachable from the command palette and
    // from the vim layer, which is the invariant M2 exists to establish.
    let mut p = EditorPanel::from_str("\n");
    let chord = KeyChord::from_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    assert!(p.handle_key(chord).is_empty());
    assert_eq!(p.line_text(0), "", "a raw key must not reach the buffer");
}

#[test]
fn clicking_places_the_cursor_at_that_position() {
    let mut p = EditorPanel::from_str("hello\nworld\n");
    let area = Rect::new(0, 0, 40, 10);
    // One row and column of border, then two cells of gutter, so column 6 is
    // the fourth grapheme of "world".
    let ev = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 6,
        row: 2,
        modifiers: KeyModifiers::NONE,
    };
    p.handle_mouse(ev, area);
    assert_eq!(p.cursor(), Position { line: 1, col: 3 });
}

#[test]
fn clicking_inside_a_wide_char_selects_that_char() {
    let mut p = EditorPanel::from_str("日本語\n");
    let area = Rect::new(0, 0, 40, 10);
    let ev = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 2, // right half of the first CJK grapheme, past the border
        row: 1,
        modifiers: KeyModifiers::NONE,
    };
    p.handle_mouse(ev, area);
    assert_eq!(p.cursor(), Position { line: 0, col: 0 });
}

#[test]
fn scrolling_moves_the_viewport_not_the_cursor() {
    let text = (0..100).map(|i| format!("line {i}\n")).collect::<String>();
    let mut p = EditorPanel::from_str(&text);
    p.handle_scroll(5, Rect::new(0, 0, 40, 10));
    assert_eq!(p.top_line(), 5);
    assert_eq!(p.cursor(), Position { line: 0, col: 0 });
}
