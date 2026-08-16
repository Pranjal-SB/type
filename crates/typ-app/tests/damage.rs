//! A frame is drawn because state changed, never because the loop went round.
//!
//! `PanelEvent::NeedsRedraw` was returned by every panel from M1 and did
//! nothing: the loop repainted every pass, so the event was decorative. That
//! was survivable while only a keypress could wake the loop. Task 1 let a
//! worker wake it, and an unconditional repaint per wakeup is a full render
//! pass for every event a watcher reports.

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::run::{Flow, step, step_batch};
use typ_core::AppEvent;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-damage-test").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hello.rs"), "fn main() {}\n").unwrap();
    dir
}

fn app_with_file(name: &str) -> App {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("hello.rs")).unwrap();
    // Opening marks dirty, as it should. Clear it so each test starts from a
    // painted screen.
    app.take_dirty();
    app
}

fn key(ch: char) -> AppEvent {
    AppEvent::Input(Event::Key(KeyEvent::new(
        KeyCode::Char(ch),
        KeyModifiers::NONE,
    )))
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> AppEvent {
    AppEvent::Input(Event::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }))
}

#[test]
fn a_keypress_that_inserts_a_character_marks_the_frame_dirty() {
    let mut app = app_with_file("insert");

    step(&mut app, key('x'), AREA).unwrap();

    assert!(app.take_dirty(), "an edit did not ask for a repaint");
}

#[test]
fn a_wakeup_that_changes_nothing_draws_nothing() {
    let mut app = app_with_file("idle");

    // A watcher reporting a file we do not have open. Task 2 already ignores
    // it; this asserts it also costs no frame.
    let event = AppEvent::FileChanged(PathBuf::from("/somewhere/else.rs"));
    step(&mut app, event, AREA).unwrap();

    assert!(!app.take_dirty(), "an idle wakeup repainted the screen");
}

#[test]
fn mouse_motion_without_a_drag_draws_nothing() {
    let mut app = app_with_file("motion");

    step(&mut app, mouse(MouseEventKind::Moved, 40, 5), AREA).unwrap();

    assert!(
        !app.take_dirty(),
        "a mouse move repainted; this is what flattered the M0 frame metrics"
    );
}

#[test]
fn a_drag_still_marks_dirty() {
    let mut app = app_with_file("drag");

    step(
        &mut app,
        mouse(MouseEventKind::Drag(MouseButton::Left), 40, 5),
        AREA,
    )
    .unwrap();

    assert!(app.take_dirty(), "a drag selection did not repaint");
}

#[test]
fn a_key_release_draws_nothing() {
    let mut app = app_with_file("release");

    let release = AppEvent::Input(Event::Key(KeyEvent::new_with_kind(
        KeyCode::Char('x'),
        KeyModifiers::NONE,
        crossterm::event::KeyEventKind::Release,
    )));
    step(&mut app, release, AREA).unwrap();

    assert!(!app.take_dirty(), "a key release repainted the screen");
}

#[test]
fn a_batch_of_edits_asks_for_one_frame_not_one_each() {
    let mut app = app_with_file("batch");

    let flow = step_batch(&mut app, vec![key('a'), key('b'), key('c')], AREA).unwrap();

    assert_eq!(flow, Flow::Continue);
    assert_eq!(app.editor_mut().line_text(0), "abcfn main() {}");
    // One answer for the batch. The loop draws once after it, not once per
    // event, which is the whole point of draining before drawing.
    assert!(app.take_dirty());
    assert!(!app.take_dirty(), "dirty was not cleared by taking it");
}

#[test]
fn a_batch_that_quits_stops_at_the_quit() {
    let mut app = app_with_file("batch-quit");
    let quit = AppEvent::Input(Event::Key(KeyEvent::new(
        KeyCode::Char('q'),
        KeyModifiers::CONTROL,
    )));

    // A clean buffer: Ctrl+Q on a dirty one asks for confirmation rather than
    // quitting, which is a different behaviour and has its own test.
    let flow = step_batch(&mut app, vec![quit, key('z')], AREA).unwrap();

    assert_eq!(flow, Flow::Quit);
    assert_eq!(
        app.editor_mut().line_text(0),
        "fn main() {}",
        "an event after the quit was still dispatched"
    );
}
