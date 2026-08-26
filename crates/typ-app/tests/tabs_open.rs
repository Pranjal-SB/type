//! Opening a file adds a tab instead of replacing the buffer.
//!
//! This file replaces `open_guard.rs`. That guard existed for one reason —
//! until tabs, an open *replaced* the buffer, and so was the one path in the
//! editor that could lose unsaved work. With tabs it guards nothing, and its
//! tests assert behaviour that was deliberately removed rather than behaviour
//! that broke. The two of them that describe opening rather than guarding are
//! kept below.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_app::App;
use typ_core::{KeyChord, Panel, PanelEvent};

/// One directory per test — see the tree panel fixture for why sharing races.
fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-tabs-open").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("first.rs"), "fn first() {}\n").unwrap();
    std::fs::write(dir.join("second.rs"), "fn second() {}\n").unwrap();
    dir
}

fn chord(c: char) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

#[test]
fn the_first_file_opened_reuses_the_scratch_buffer() {
    // `typ` with no arguments starts on an empty untitled buffer. Appending to
    // it would leave every session one tab wider than the number of files the
    // user has actually opened, with an empty one on the left that can never be
    // useful.
    let dir = fixture("scratch");
    let mut app = App::new(&dir).unwrap();
    assert_eq!(app.tab_count(), 1, "the editor starts on one empty buffer");

    app.open_path(&dir.join("first.rs")).unwrap();

    assert_eq!(app.tab_count(), 1, "the scratch buffer was not reused");
    assert_eq!(app.editor_title(), "first.rs");
}

#[test]
fn a_scratch_buffer_with_typing_in_it_is_not_reused() {
    // The other half. An untitled buffer someone has typed into holds work,
    // and replacing it is exactly the loss the old open guard existed to
    // prevent.
    let dir = fixture("scratch-dirty");
    let mut app = App::new(&dir).unwrap();
    // Focus starts on the tree, so a keystroke goes there until it is moved.
    app.cycle_focus();
    app.handle_chord(chord('X')).unwrap();
    assert!(
        app.editor().is_dirty(),
        "the fixture must leave work at risk"
    );

    app.open_path(&dir.join("first.rs")).unwrap();

    assert_eq!(app.tab_count(), 2, "unsaved scratch work was discarded");
    assert_eq!(app.editor_title(), "first.rs");
}

#[test]
fn opening_a_second_file_appends_a_tab_and_activates_it() {
    let dir = fixture("append");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("first.rs")).unwrap();

    app.open_path(&dir.join("second.rs")).unwrap();

    assert_eq!(app.tab_count(), 2);
    assert_eq!(app.active_tab(), 1);
    assert_eq!(app.editor_title(), "second.rs");
}

#[test]
fn opening_a_file_that_is_already_open_switches_to_its_tab() {
    let dir = fixture("already-open");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("first.rs")).unwrap();
    app.open_path(&dir.join("second.rs")).unwrap();

    app.open_path(&dir.join("first.rs")).unwrap();

    assert_eq!(app.tab_count(), 2, "the same file was opened twice");
    assert_eq!(app.active_tab(), 0);
    assert_eq!(app.editor_title(), "first.rs");
}

#[test]
fn two_spellings_of_one_path_are_one_tab() {
    // `./first.rs` and `first.rs` name the same file, and a picker, a tree and
    // a command line can each produce a different spelling of it.
    let dir = fixture("spellings");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("first.rs")).unwrap();

    app.open_path(&dir.join(".").join("first.rs")).unwrap();

    assert_eq!(app.tab_count(), 1);
}

#[test]
fn unsaved_changes_no_longer_stop_an_open() {
    // The milestone's user-visible win. Opening used to refuse while the buffer
    // was dirty, because opening discarded it; a new tab discards nothing, so
    // there is nothing left to ask about.
    let dir = fixture("dirty-open");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("first.rs")).unwrap();
    app.handle_chord(chord('X')).unwrap();
    assert_eq!(
        app.editor_title(),
        "first.rs *",
        "the fixture must be dirty"
    );

    app.open_path(&dir.join("second.rs")).unwrap();

    assert_eq!(
        app.editor_title(),
        "second.rs",
        "an open was refused when nothing was at risk"
    );
    assert!(app.status().is_none(), "nothing to confirm, nothing to say");
    assert_eq!(app.tab_count(), 2);
    assert!(
        app.tab(0).is_dirty(),
        "the edit must still be there, one tab over"
    );
}

#[test]
fn the_tree_opens_into_a_tab_too() {
    // The path a user actually takes: the tree emits an event, the app applies
    // it. Routing that only `open_path` callers get would pass every other test
    // in this file.
    let dir = fixture("tree-open");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("first.rs")).unwrap();

    app.apply(vec![PanelEvent::OpenFile {
        path: dir.join("second.rs"),
        line: 0,
        col: 0,
    }])
    .unwrap();

    assert_eq!(app.tab_count(), 2);
    assert_eq!(app.editor_title(), "second.rs");
}

#[test]
fn several_paths_open_as_several_tabs_with_the_first_active() {
    // `typ a.rs b.rs` dropped everything after the first path without a word.
    // The gap analysis called it "honest until tabs exist, a real bug the
    // moment they do" — and then tabs shipped without it.
    //
    // The first stays active because that is what `vim a b` and `code a b`
    // both do: the extra paths are context you asked to have open, not the
    // thing you asked to look at.
    let dir = fixture("several");
    let mut app = App::new(&dir).unwrap();

    app.open_all(&[dir.join("first.rs"), dir.join("second.rs")])
        .unwrap();

    assert_eq!(app.tab_count(), 2);
    assert_eq!(app.active_tab(), 0);
    assert_eq!(app.editor_title(), "first.rs");
}

#[test]
fn one_path_opens_one_tab_and_reuses_the_scratch_buffer() {
    let dir = fixture("several-one");
    let mut app = App::new(&dir).unwrap();

    app.open_all(&[dir.join("first.rs")]).unwrap();

    assert_eq!(app.tab_count(), 1);
    assert_eq!(app.editor_title(), "first.rs");
}

#[test]
fn no_paths_leaves_the_scratch_buffer_alone() {
    // `typ .` resolves to a workspace and no file, so this is the ordinary
    // no-argument start rather than an edge case.
    let dir = fixture("several-none");
    let mut app = App::new(&dir).unwrap();

    app.open_all(&[]).unwrap();

    assert_eq!(app.tab_count(), 1);
    assert_eq!(app.editor_title(), "untitled");
}

#[test]
fn the_same_path_twice_on_the_command_line_is_one_tab() {
    let dir = fixture("several-dup");
    let mut app = App::new(&dir).unwrap();

    app.open_all(&[dir.join("first.rs"), dir.join("first.rs")])
        .unwrap();

    assert_eq!(app.tab_count(), 1);
}

#[test]
fn a_name_with_no_file_behind_it_still_gets_a_tab() {
    // `typ a.rs new.md` opens an empty buffer for the second, the same way
    // `typ new.md` alone does. Whether the *directory* exists is a
    // command-line question and `main.rs` answers it before this is called —
    // see `resolve`.
    let dir = fixture("several-new");
    let new = dir.join("not-yet.rs");

    let mut app = App::new(&dir).unwrap();
    app.open_all(&[dir.join("first.rs"), new.clone()]).unwrap();

    assert_eq!(app.tab_count(), 2);
    assert_eq!(app.tab(1).title(), "not-yet.rs");
    assert!(!new.exists(), "opening must not create the file");
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
fn opening_a_path_that_does_not_exist_yet_starts_an_empty_buffer() {
    let dir = fixture("new-file");
    let mut app = App::new(&dir).unwrap();

    let path = dir.join("not-yet.rs");
    app.open_path(&path).unwrap();

    assert_eq!(app.editor_title(), "not-yet.rs");
    assert_eq!(app.editor_mut().line_text(0), "");
    assert!(!path.exists(), "opening must not create the file");
}
