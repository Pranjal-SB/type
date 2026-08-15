//! The status bar and the quit guard it exists to make possible.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::layout::split_frame;
use typ_core::{KeyChord, NotifyLevel, PanelEvent};

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-status-test").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hello.rs"), "fn main() {}\n").unwrap();
    dir
}

/// An app with a dirty editor buffer.
fn app_with_edits(name: &str) -> App {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("hello.rs")).unwrap();
    // Through the dispatcher, the way a real keypress arrives — the editor has
    // no raw-key behavior of its own any more.
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('x'),
        KeyModifiers::NONE,
    )))
    .unwrap();
    app
}

#[test]
fn the_frame_reserves_one_row_for_the_status_bar() {
    let (body, status) = split_frame(Rect::new(0, 0, 100, 30));
    assert_eq!(body.height, 29);
    assert_eq!(status.height, 1);
    assert_eq!(status.y, 29);
}

#[test]
fn quitting_a_clean_workspace_needs_no_confirmation() {
    let mut app = App::new(&fixture("clean")).unwrap();
    app.apply(vec![PanelEvent::Quit]).unwrap();
    assert!(app.should_quit());
}

#[test]
fn quitting_with_unsaved_changes_asks_first() {
    let mut app = app_with_edits("dirty");
    app.apply(vec![PanelEvent::Quit]).unwrap();
    assert!(!app.should_quit(), "the first quit must not discard edits");
    assert!(app.status().unwrap().contains("Unsaved"));
}

#[test]
fn a_second_quit_discards_the_changes() {
    let mut app = app_with_edits("dirty-twice");
    app.apply(vec![PanelEvent::Quit]).unwrap();
    app.apply(vec![PanelEvent::Quit]).unwrap();
    assert!(app.should_quit());
}

#[test]
fn any_other_input_cancels_a_pending_quit() {
    let mut app = app_with_edits("cancelled");
    app.apply(vec![PanelEvent::Quit]).unwrap();
    app.clear_transient();
    assert!(app.status().is_none());
    app.apply(vec![PanelEvent::Quit]).unwrap();
    assert!(!app.should_quit(), "the prompt must start over");
}

#[test]
fn saving_clears_the_way_to_quit() {
    let mut app = app_with_edits("saved");
    app.editor_mut().save().unwrap();
    app.apply(vec![PanelEvent::Quit]).unwrap();
    assert!(app.should_quit());
}

#[test]
fn notify_events_become_the_status_message() {
    let mut app = App::new(&fixture("notify")).unwrap();
    app.apply(vec![PanelEvent::Notify {
        level: NotifyLevel::Error,
        message: "permission denied".into(),
    }])
    .unwrap();
    assert_eq!(app.status().unwrap(), "permission denied");
}

#[test]
fn the_idle_status_bar_advertises_the_core_bindings() {
    let app = App::new(&fixture("hint")).unwrap();
    assert!(app.status().is_none());
    let hint = app.status_left();
    assert!(hint.contains("Tab"), "hint was: {hint}");
    assert!(hint.contains("Ctrl+S"), "hint was: {hint}");
    assert!(hint.contains("Ctrl+Q"), "hint was: {hint}");
}

#[test]
fn the_status_bar_reports_the_cursor_position_one_based() {
    let dir = fixture("position");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("hello.rs")).unwrap();
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Right,
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert_eq!(app.status_right(), "hello.rs  1:2");
}

#[test]
fn the_status_bar_marks_an_unsaved_buffer() {
    let app = app_with_edits("marker");
    assert!(app.status_right().contains('*'));
}
