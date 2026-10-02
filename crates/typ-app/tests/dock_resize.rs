//! Resizing the left dock: drag its rule, or `ctrl+k =` with the dock focused
//! and the arrows. Clamped at 12 columns and at half the body.

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use typ_app::run::step;
use typ_app::{App, Focus};
use typ_core::AppEvent;

const FRAME: Rect = Rect {
    x: 0,
    y: 0,
    width: 100,
    height: 30,
};

fn app(name: &str) -> App {
    let dir: PathBuf = std::env::temp_dir().join("typ-dock-resize").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "alpha\n").unwrap();
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("a.rs")).unwrap();
    app
}

fn mouse(app: &mut App, kind: MouseEventKind, column: u16, row: u16) {
    let event = MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    step(app, AppEvent::Input(Event::Mouse(event)), FRAME).unwrap();
}

fn drag(app: &mut App, from: u16, to: u16) {
    mouse(app, MouseEventKind::Down(MouseButton::Left), from, 10);
    mouse(app, MouseEventKind::Drag(MouseButton::Left), to, 10);
    mouse(app, MouseEventKind::Up(MouseButton::Left), to, 10);
}

fn key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    step(
        app,
        AppEvent::Input(Event::Key(KeyEvent::new(code, mods))),
        FRAME,
    )
    .unwrap();
}

/// The column the dock's rule is on: the tree's width, the tree starting at 0.
fn rule(app: &App) -> u16 {
    app.areas(FRAME).0.width
}

#[test]
fn dragging_the_dock_rule_moves_it_with_the_pointer() {
    let mut app = app("drag");
    assert_eq!(rule(&app), 29);

    drag(&mut app, 29, 39);

    assert_eq!(rule(&app), 39, "ten columns right");
    assert_eq!(app.pane_rects(FRAME)[0].x, 40, "the panes take the rest");
    drag(&mut app, 39, 34);
    assert_eq!(rule(&app), 34, "and back five");
}

#[test]
fn the_dock_is_clamped_at_twelve_columns_and_half_the_body() {
    let mut app = app("clamp");

    drag(&mut app, 29, 2);
    assert_eq!(rule(&app), 11, "twelve columns, its rule included");

    drag(&mut app, 11, 90);
    assert_eq!(rule(&app), 49, "half of a hundred");
}

#[test]
fn ctrl_k_equals_on_the_focused_dock_moves_its_rule() {
    let mut app = app("keys");
    app.set_focus(Focus::Tree);

    key(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    key(&mut app, KeyCode::Char('='), KeyModifiers::NONE);
    for _ in 0..10 {
        key(&mut app, KeyCode::Right, KeyModifiers::NONE);
    }
    assert_eq!(rule(&app), 39, "the same distance a drag moves it");

    for _ in 0..40 {
        key(&mut app, KeyCode::Left, KeyModifiers::NONE);
    }
    assert_eq!(rule(&app), 11, "the same clamp");

    key(&mut app, KeyCode::Char('='), KeyModifiers::NONE);
    assert_eq!(rule(&app), 29, "= puts it back");
    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(app.focus(), Focus::Tree, "esc left the mode, not the dock");
}
