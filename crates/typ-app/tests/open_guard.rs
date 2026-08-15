//! Opening a file must not silently discard unsaved work.
//!
//! `Ctrl+Q` has always guarded a dirty buffer; opening did not, and the two are
//! the same question asked by different keys. Until tabs land at M4 an open
//! *replaces* the buffer, so an unguarded open is the one path in the editor
//! that can lose work without saying anything.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_app::App;
use typ_core::{KeyChord, PanelEvent};

/// One directory per test — see the tree panel fixture for why sharing races.
fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-open-guard").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("first.rs"), "fn first() {}\n").unwrap();
    std::fs::write(dir.join("second.rs"), "fn second() {}\n").unwrap();
    dir
}

fn chord(c: char) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

/// Open a file and type into it, so the buffer is dirty.
fn app_with_unsaved_edit(name: &str) -> (App, PathBuf) {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("first.rs")).unwrap();
    app.handle_chord(chord('X')).unwrap();
    assert_eq!(
        app.editor_title(),
        "first.rs *",
        "the fixture must leave the buffer dirty or the test proves nothing"
    );
    (app, dir)
}

#[test]
fn opening_another_file_over_unsaved_changes_is_refused() {
    let (mut app, dir) = app_with_unsaved_edit("refused");

    app.open_path(&dir.join("second.rs")).unwrap();

    assert_eq!(
        app.editor_title(),
        "first.rs *",
        "the dirty buffer must still be open"
    );
}

#[test]
fn refusing_to_open_says_why() {
    let (mut app, dir) = app_with_unsaved_edit("says-why");

    app.open_path(&dir.join("second.rs")).unwrap();

    let status = app.status().unwrap_or_default().to_string();
    assert!(
        status.contains("Unsaved changes"),
        "a refusal the user cannot see is indistinguishable from a broken key, got {status:?}"
    );
}

#[test]
fn opening_the_same_file_again_discards_and_goes_through() {
    let (mut app, dir) = app_with_unsaved_edit("second-try");

    app.open_path(&dir.join("second.rs")).unwrap();
    app.open_path(&dir.join("second.rs")).unwrap();

    assert_eq!(
        app.editor_title(),
        "second.rs",
        "a second open of the same path is the user answering the question"
    );
}

#[test]
fn opening_a_clean_buffer_is_never_interrupted() {
    let dir = fixture("clean");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("first.rs")).unwrap();

    app.open_path(&dir.join("second.rs")).unwrap();

    assert_eq!(app.editor_title(), "second.rs");
    assert!(
        app.status().is_none(),
        "nothing to confirm means nothing to say"
    );
}

#[test]
fn the_tree_opening_a_file_is_guarded_too() {
    let (mut app, dir) = app_with_unsaved_edit("tree-open");

    // The path a user actually takes: the tree emits an event, the app applies
    // it. A guard that only covers direct `open_path` calls would pass every
    // other test in this file and still lose work in the running editor.
    app.apply(vec![PanelEvent::OpenFile {
        path: dir.join("second.rs"),
        line: 0,
        col: 0,
    }])
    .unwrap();

    assert_eq!(app.editor_title(), "first.rs *");
}

#[test]
fn saving_clears_the_guard() {
    let (mut app, dir) = app_with_unsaved_edit("saved");

    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    app.open_path(&dir.join("second.rs")).unwrap();

    assert_eq!(
        app.editor_title(),
        "second.rs",
        "saving answers the question, so the open proceeds"
    );
}
