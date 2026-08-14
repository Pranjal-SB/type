//! Keyboard coverage beyond the walking skeleton's arrows-and-typing set, plus
//! the cursor the app draws from.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_buffer::Position;
use typ_core::{KeyChord, Panel, RenderContext, ThemeColors};
use typ_panel_editor::EditorPanel;

fn chord(code: KeyCode) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, KeyModifiers::NONE))
}

fn ctrl(c: char) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}

/// Panels learn their height at render time, so page motions need one frame.
fn render(p: &mut EditorPanel, area: Rect) {
    let theme = ThemeColors::default();
    let ctx = RenderContext {
        theme: &theme,
        is_focused: true,
        panel_index: 0,
        terminal_width: area.width,
        terminal_height: area.height,
    };
    let mut buf = Buffer::empty(area);
    p.render(area, &mut buf, &ctx);
}

#[test]
fn enter_splits_the_line_and_moves_to_its_start() {
    let mut p = EditorPanel::from_str("ab\n");
    p.handle_key(chord(KeyCode::Right));
    p.handle_key(chord(KeyCode::Enter));
    assert_eq!(p.cursor(), Position { line: 1, col: 0 });
    assert_eq!(p.line_text(0), "a");
    assert_eq!(p.line_text(1), "b");
}

#[test]
fn home_and_end_jump_to_the_line_bounds() {
    let mut p = EditorPanel::from_str("hello\n");
    p.handle_key(chord(KeyCode::End));
    assert_eq!(p.cursor(), Position { line: 0, col: 5 });
    p.handle_key(chord(KeyCode::Home));
    assert_eq!(p.cursor(), Position { line: 0, col: 0 });
}

#[test]
fn delete_removes_the_grapheme_under_the_cursor() {
    let mut p = EditorPanel::from_str("abc\n");
    p.handle_key(chord(KeyCode::Delete));
    assert_eq!(p.line_text(0), "bc");
    assert_eq!(p.cursor(), Position { line: 0, col: 0 });
}

#[test]
fn backspace_at_column_zero_joins_the_previous_line() {
    let mut p = EditorPanel::from_str("ab\ncd\n");
    p.handle_key(chord(KeyCode::Down));
    p.handle_key(chord(KeyCode::Backspace));
    assert_eq!(p.line_text(0), "abcd");
    assert_eq!(p.cursor(), Position { line: 0, col: 2 });
}

#[test]
fn page_down_moves_a_screen_at_a_time() {
    let text = (0..100).map(|i| format!("line {i}\n")).collect::<String>();
    let mut p = EditorPanel::from_str(&text);
    render(&mut p, Rect::new(0, 0, 40, 12)); // 12 minus the border = 10 text rows
    p.handle_key(chord(KeyCode::PageDown));
    assert_eq!(p.cursor().line, 10);
    p.handle_key(chord(KeyCode::PageUp));
    assert_eq!(p.cursor().line, 0);
}

#[test]
fn ctrl_z_undoes_and_ctrl_y_redoes() {
    let mut p = EditorPanel::from_str("a\n");
    p.handle_key(chord(KeyCode::End));
    p.handle_key(chord(KeyCode::Char('b')));
    assert_eq!(p.line_text(0), "ab");
    p.handle_key(ctrl('z'));
    assert_eq!(p.line_text(0), "a");
    p.handle_key(ctrl('y'));
    assert_eq!(p.line_text(0), "ab");
}

#[test]
fn the_cursor_sits_inside_the_border_at_the_text_position() {
    let mut p = EditorPanel::from_str("hello\n");
    let area = Rect::new(0, 0, 40, 10);
    render(&mut p, area);
    p.handle_key(chord(KeyCode::Right));
    p.handle_key(chord(KeyCode::Right));
    // One column and one row of border, then two columns of text.
    assert_eq!(p.cursor_position(area), Some((3, 1)));
}

#[test]
fn the_cursor_accounts_for_wide_characters() {
    let mut p = EditorPanel::from_str("日本語\n");
    let area = Rect::new(0, 0, 40, 10);
    render(&mut p, area);
    p.handle_key(chord(KeyCode::Right));
    // One CJK grapheme is two display columns, plus the border.
    assert_eq!(p.cursor_position(area), Some((3, 1)));
}

#[test]
fn the_cursor_follows_the_viewport_when_scrolled() {
    let text = (0..100).map(|i| format!("line {i}\n")).collect::<String>();
    let mut p = EditorPanel::from_str(&text);
    let area = Rect::new(0, 0, 40, 10);
    render(&mut p, area);
    p.handle_scroll(5, area);
    // The cursor is on line 0, which is now above the viewport.
    assert_eq!(p.cursor_position(area), None);
}
