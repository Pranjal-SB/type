//! Resizing panes: drag the rule, or `ctrl+k =` and the arrows.

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::run::step;
use typ_core::{Action, AppEvent};

const FRAME: Rect = Rect {
    x: 0,
    y: 0,
    width: 100,
    height: 30,
};

fn app(name: &str, split: Action) -> App {
    let dir: PathBuf = std::env::temp_dir().join("typ-resize").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "alpha\n").unwrap();
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("a.rs")).unwrap();
    app.apply_named_action(split).unwrap();
    app.close_picker();
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

fn drag(app: &mut App, from: (u16, u16), to: (u16, u16)) {
    mouse(app, MouseEventKind::Down(MouseButton::Left), from.0, from.1);
    mouse(app, MouseEventKind::Drag(MouseButton::Left), to.0, to.1);
    mouse(app, MouseEventKind::Up(MouseButton::Left), to.0, to.1);
}

fn key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    step(
        app,
        AppEvent::Input(Event::Key(KeyEvent::new(code, mods))),
        FRAME,
    )
    .unwrap();
}

fn resize_mode(app: &mut App) {
    key(app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    key(app, KeyCode::Char('='), KeyModifiers::NONE);
}

/// The first pane's width, which is the rule's offset from the editor's left.
fn first_width(app: &App) -> u16 {
    app.pane_rects(FRAME)[0].width
}

#[test]
fn dragging_the_rule_moves_it_with_the_pointer() {
    let mut app = app("drag", Action::SplitRight);
    // Columns 30..100: 34, the rule at 64, 35.
    assert_eq!(first_width(&app), 34);

    drag(&mut app, (64, 10), (54, 10));

    assert_eq!(first_width(&app), 24, "ten columns left");
    assert_eq!(app.pane_rects(FRAME)[1], Rect::new(55, 0, 45, 29));
}

#[test]
fn a_drag_is_clamped_so_no_pane_goes_under_ten_columns() {
    let mut app = app("clamp", Action::SplitRight);
    drag(&mut app, (64, 10), (31, 10));
    assert_eq!(first_width(&app), 10);
    drag(&mut app, (40, 10), (99, 10));
    assert_eq!(app.pane_rects(FRAME)[1].width, 10);
}

#[test]
fn a_press_on_the_rule_does_not_reach_a_pane() {
    let mut app = app("press", Action::SplitRight);
    let before = app.current_pane();
    mouse(&mut app, MouseEventKind::Down(MouseButton::Left), 64, 10);
    mouse(&mut app, MouseEventKind::Up(MouseButton::Left), 64, 10);
    assert_eq!(first_width(&app), 34);
    assert_eq!(app.current_pane(), before);
}

#[test]
fn the_keyboard_mode_moves_the_same_rule_the_same_distance() {
    let mut app = app("keys", Action::SplitRight);
    resize_mode(&mut app);
    assert!(
        app.status_left().contains("arrows move the rule"),
        "{}",
        app.status_left()
    );

    for _ in 0..10 {
        key(&mut app, KeyCode::Left, KeyModifiers::NONE);
    }
    assert_eq!(first_width(&app), 24);
    assert!(app.status_left().contains("arrows move the rule"));

    key(&mut app, KeyCode::Char('='), KeyModifiers::NONE);
    assert_eq!(first_width(&app), 34, "= evens the split");

    for _ in 0..40 {
        key(&mut app, KeyCode::Left, KeyModifiers::NONE);
    }
    assert_eq!(first_width(&app), 10, "the clamp holds for keys too");

    key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(!app.status_left().contains("arrows move the rule"));
    // Out of the mode, an arrow is the editor's again.
    key(&mut app, KeyCode::Left, KeyModifiers::NONE);
    assert_eq!(first_width(&app), 10);
}

#[test]
fn up_and_down_move_a_horizontal_rule_and_enter_leaves() {
    let mut app = app("rows", Action::SplitDown);
    // 29 body rows: 14, the rule, 14.
    assert_eq!(first_width(&app), 70);
    assert_eq!(app.pane_rects(FRAME)[0].height, 14);
    resize_mode(&mut app);
    for _ in 0..4 {
        key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    assert_eq!(app.pane_rects(FRAME)[0].height, 18);
    for _ in 0..40 {
        key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    assert_eq!(
        app.pane_rects(FRAME)[1].height,
        3,
        "no pane under three rows"
    );
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(!app.status_left().contains("arrows move the rule"));

    drag(&mut app, (40, 25), (40, 14));
    assert_eq!(
        app.pane_rects(FRAME)[0].height,
        14,
        "a drag moves a row rule"
    );
}
