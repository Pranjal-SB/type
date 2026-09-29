//! Focus is shown by what steps back (interface §4), and it moves both ways.

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_app::run::step;
use typ_app::{App, Focus};
use typ_core::{AppEvent, ThemeColors};

const FRAME: Rect = Rect {
    x: 0,
    y: 0,
    width: 60,
    height: 10,
};

fn app(name: &str) -> App {
    let dir: PathBuf = std::env::temp_dir().join("typ-recede").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "").unwrap();
    std::fs::write(dir.join("main.rs"), "let x = 1;\n").unwrap();
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("main.rs")).unwrap();
    app
}

fn draw(app: &mut App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(FRAME.width, FRAME.height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    terminal.backend().buffer().clone()
}

/// The `m` of `main.rs` in the tree and the `x` of `let x` in the editor.
///
/// Row 2 in the tree: `a.rs` above it is the selected row, which never
/// recedes, so it would say nothing here.
fn tree_and_editor_text(buf: &Buffer) -> (ratatui::style::Color, ratatui::style::Color) {
    let tree = (0..29).find(|&x| buf[(x, 2)].symbol() == "m").unwrap();
    let editor = (30..60).find(|&x| buf[(x, 1)].symbol() == "x").unwrap();
    (buf[(tree, 2)].fg, buf[(editor, 1)].fg)
}

fn click(x: u16, y: u16) -> AppEvent {
    AppEvent::Input(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }))
}

#[test]
fn the_keyboard_moves_the_recede() {
    let theme = ThemeColors::default();
    let mut app = app("keyboard");
    assert_eq!(app.focus(), Focus::Editor);
    let (tree, editor) = tree_and_editor_text(&draw(&mut app));
    assert_eq!(tree, theme.receded_fg, "the unfocused tree did not recede");
    assert_eq!(editor, theme.fg);

    let f6 = AppEvent::Input(Event::Key(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE)));
    step(&mut app, f6, FRAME).unwrap();
    assert_eq!(app.focus(), Focus::Tree);
    let (tree, editor) = tree_and_editor_text(&draw(&mut app));
    assert_eq!(tree, theme.tree_file_fg);
    assert_eq!(
        editor, theme.receded_fg,
        "the unfocused editor did not recede"
    );
}

#[test]
fn the_mouse_moves_the_recede() {
    let theme = ThemeColors::default();
    let mut app = app("mouse");
    // Row 4 is below both entries, so the click focuses without opening.
    step(&mut app, click(5, 4), FRAME).unwrap();
    assert_eq!(app.focus(), Focus::Tree);
    let (tree, editor) = tree_and_editor_text(&draw(&mut app));
    assert_eq!(tree, theme.tree_file_fg);
    assert_eq!(editor, theme.receded_fg);

    step(&mut app, click(45, 3), FRAME).unwrap();
    assert_eq!(app.focus(), Focus::Editor);
    let (tree, editor) = tree_and_editor_text(&draw(&mut app));
    assert_eq!(tree, theme.receded_fg);
    assert_eq!(editor, theme.fg);
}
