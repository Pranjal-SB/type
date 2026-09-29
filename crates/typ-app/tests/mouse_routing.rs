//! Which panel a click belongs to. Gap 84.

use std::path::PathBuf;

use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
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

/// The last row of the frame, which `split_frame` gives the status bar.
const STATUS_ROW: u16 = FRAME.height - 1;

fn app_with_long_file(name: &str) -> App {
    let dir: PathBuf = std::env::temp_dir().join("typ-mouse-routing").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let body: String = (0..60).map(|i| format!("line {i}\n")).collect();
    let file = dir.join("long.rs");
    std::fs::write(&file, body).unwrap();
    let mut app = App::new(&dir).unwrap();
    app.open_path(&file).unwrap();
    app
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
fn a_click_on_the_status_bar_does_not_move_the_caret() {
    let mut app = app_with_long_file("status-editor");
    assert_eq!(app.focus(), Focus::Editor);

    step(&mut app, click(60, STATUS_ROW), FRAME).unwrap();

    assert_eq!(
        app.editor().cursor().line,
        0,
        "the status bar was hit-tested as a row of the editor"
    );
}

#[test]
fn a_click_on_the_status_bar_under_the_tree_does_not_take_focus() {
    let mut app = app_with_long_file("status-tree");
    assert_eq!(app.focus(), Focus::Editor);

    step(&mut app, click(5, STATUS_ROW), FRAME).unwrap();

    assert_eq!(
        app.focus(),
        Focus::Editor,
        "the status bar was hit-tested as part of the tree"
    );
}

fn pointer(kind: MouseEventKind, x: u16, y: u16) -> AppEvent {
    AppEvent::Input(Event::Mouse(MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }))
}

#[test]
fn pointer_motion_over_the_picker_does_not_cost_a_frame() {
    // Gap 124. Every motion report while the overlay was up marked the frame
    // dirty, and one arrives per cell the pointer crosses: a full render pass
    // each, for a widget that ignores everything but a left press.
    let mut app = app_with_long_file("picker-motion");
    app.open_picker();
    app.take_dirty();

    for x in 10..20 {
        step(&mut app, pointer(MouseEventKind::Moved, x, 10), FRAME).unwrap();
    }

    assert!(
        !app.take_dirty(),
        "ten motion reports over the picker asked for a repaint"
    );
}

#[test]
fn a_click_away_from_the_picker_still_repaints() {
    // The guard must not swallow an event the overlay does answer.
    let mut app = app_with_long_file("picker-click");
    app.open_picker();
    app.take_dirty();

    // Outside the overlay: a dismissal, which changes the screen.
    step(&mut app, click(0, 0), FRAME).unwrap();

    assert!(app.picker().is_none());
    assert!(app.take_dirty());
}

#[test]
fn a_click_inside_the_editor_still_moves_the_caret() {
    // The guard must not swallow the clicks it is not about.
    let mut app = app_with_long_file("inside");
    let (_, editor) = app.areas(FRAME);

    step(&mut app, click(editor.x + 5, editor.y + 4), FRAME).unwrap();

    assert!(app.editor().cursor().line > 0);
}
