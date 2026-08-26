//! Switching and closing tabs from the keyboard.
//!
//! The close rule is the one worth stating: closing the active tab activates
//! the tab used most recently, not a neighbour. Both mature answers in the
//! field are history-based — VS Code's `focusRecentEditorAfterClose` defaults
//! to true, and Helix walks its jumplist backwards until it finds a different
//! buffer — and the failure positional ordering causes is the one the picker
//! made common: open a file to check one thing, close it, and land somewhere
//! unrelated instead of back where the work was.

use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_core::{Action, KeyChord, Keymap};

use typ_app::App;

/// One directory per test — see the tree panel fixture for why sharing races.
fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-tabs-switch").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(dir.join(file), format!("fn {}() {{}}\n", &file[..1])).unwrap();
    }
    dir
}

/// Three files open, tab 2 active, in the order a, b, c.
fn three(name: &str) -> (App, PathBuf) {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    for file in ["a.rs", "b.rs", "c.rs"] {
        app.open_path(&dir.join(file)).unwrap();
    }
    assert_eq!(app.tab_count(), 3);
    (app, dir)
}

fn chord(canonical: &str) -> KeyChord {
    let (code, modifiers) = match canonical {
        "ctrl+w" => (KeyCode::Char('w'), KeyModifiers::CONTROL),
        "ctrl+pageup" => (KeyCode::PageUp, KeyModifiers::CONTROL),
        "ctrl+pagedown" => (KeyCode::PageDown, KeyModifiers::CONTROL),
        "alt+," => (KeyCode::Char(','), KeyModifiers::ALT),
        "alt+." => (KeyCode::Char('.'), KeyModifiers::ALT),
        other => {
            let digit = other.strip_prefix("alt+").expect("a chord this test knows");
            (
                KeyCode::Char(digit.chars().next().unwrap()),
                KeyModifiers::ALT,
            )
        }
    };
    KeyChord::from_event(KeyEvent::new(code, modifiers))
}

#[test]
fn the_switching_chords_are_bound() {
    let keymap = Keymap::default_bindings();

    assert_eq!(
        keymap.lookup(&chord("ctrl+pagedown")),
        Some(Action::NextTab)
    );
    assert_eq!(keymap.lookup(&chord("ctrl+pageup")), Some(Action::PrevTab));
    assert_eq!(keymap.lookup(&chord("ctrl+w")), Some(Action::CloseTab));
    assert_eq!(keymap.lookup(&chord("alt+3")), Some(Action::GoToTab(3)));
}

#[test]
fn a_universal_chord_reaches_next_and_previous_too() {
    // `controls.md` §1 lists the universally deliverable keys, and PageUp and
    // PageDown are not among them — arrows, Home, End, Tab, Enter and Esc are.
    // `Alt+punctuation` is, so these two are the pair that works everywhere and
    // the Ctrl+Page chords are the familiar spelling for terminals with them.
    let keymap = Keymap::default_bindings();

    assert_eq!(keymap.lookup(&chord("alt+.")), Some(Action::NextTab));
    assert_eq!(keymap.lookup(&chord("alt+,")), Some(Action::PrevTab));
}

#[test]
fn next_moves_one_tab_along() {
    let (mut app, _dir) = three("next");
    app.activate_tab(0);

    app.handle_chord(chord("ctrl+pagedown")).unwrap();

    assert_eq!(app.active_tab(), 1);
}

#[test]
fn next_past_the_last_tab_wraps_to_the_first() {
    let (mut app, _dir) = three("wrap-forward");
    assert_eq!(app.active_tab(), 2, "the fixture must end on the last tab");

    app.handle_chord(chord("ctrl+pagedown")).unwrap();

    assert_eq!(app.active_tab(), 0);
}

#[test]
fn previous_before_the_first_tab_wraps_to_the_last() {
    let (mut app, _dir) = three("wrap-back");
    app.activate_tab(0);

    app.handle_chord(chord("ctrl+pageup")).unwrap();

    assert_eq!(app.active_tab(), 2);
}

#[test]
fn a_digit_jumps_to_that_tab_counting_from_one() {
    // Every tabbed application counts these from one, including the terminals
    // this runs inside.
    let (mut app, _dir) = three("digit");

    app.handle_chord(chord("alt+1")).unwrap();
    assert_eq!(app.active_tab(), 0);

    app.handle_chord(chord("alt+3")).unwrap();
    assert_eq!(app.active_tab(), 2);
}

#[test]
fn a_digit_past_the_last_tab_does_nothing() {
    let (mut app, _dir) = three("digit-past-end");
    app.activate_tab(1);

    app.handle_chord(chord("alt+9")).unwrap();

    assert_eq!(app.active_tab(), 1, "alt+9 moved to a tab that is not open");
}

#[test]
fn closing_the_active_tab_activates_the_one_used_most_recently() {
    // The rule, and the case where it differs from every positional answer.
    // Visited in the order a, b, c, then a, then c. Closing c must land on a —
    // the tab actually being worked in — where a neighbour rule lands on b.
    let (mut app, _dir) = three("mru");
    app.activate_tab(0);
    app.activate_tab(2);

    app.handle_chord(chord("ctrl+w")).unwrap();

    assert_eq!(app.tab_count(), 2);
    assert_eq!(
        app.editor_title(),
        "a.rs",
        "closing fell back to a neighbour instead of the last tab used"
    );
}

#[test]
fn closing_a_background_tab_leaves_the_active_one_active() {
    // Its index moves when an earlier tab is removed, which is the whole reason
    // an index is not a handle.
    let (mut app, _dir) = three("close-background");
    assert_eq!(app.editor_title(), "c.rs");

    app.close_tab(0);

    assert_eq!(app.tab_count(), 2);
    assert_eq!(app.editor_title(), "c.rs", "the wrong tab is on screen");
    assert_eq!(app.active_tab(), 1, "the index did not follow the file");
}

#[test]
fn closing_the_last_tab_leaves_one_empty_buffer() {
    // Never zero tabs: `editor()` would have to return an `Option` and every
    // one of its callers would handle a state that has no meaning.
    let dir = fixture("close-last");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("a.rs")).unwrap();

    app.handle_chord(chord("ctrl+w")).unwrap();

    assert_eq!(app.tab_count(), 1);
    assert_eq!(app.editor_title(), "untitled");
}

#[test]
fn closing_a_dirty_tab_asks_first() {
    let (mut app, _dir) = three("dirty-close");
    // `open_path` leaves focus on the editor, so the keystroke lands in the buffer.
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('X'),
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert_eq!(app.editor_title(), "c.rs *");

    app.handle_chord(chord("ctrl+w")).unwrap();

    assert_eq!(app.tab_count(), 3, "unsaved work was discarded silently");
    let status = app.status().unwrap_or_default().to_string();
    assert!(
        status.contains("Unsaved changes"),
        "a refusal nobody can see is indistinguishable from a broken key: {status:?}"
    );
}

#[test]
fn closing_a_dirty_tab_twice_goes_through() {
    let (mut app, _dir) = three("dirty-close-twice");
    // `open_path` leaves focus on the editor, so the keystroke lands in the buffer.
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('X'),
        KeyModifiers::NONE,
    )))
    .unwrap();

    app.handle_chord(chord("ctrl+w")).unwrap();
    app.handle_chord(chord("ctrl+w")).unwrap();

    assert_eq!(app.tab_count(), 2, "the second press did not go through");
}

#[test]
fn any_other_key_between_the_two_abandons_the_close() {
    // The trap `quit_pending` already avoids: a confirmation the user answers
    // ten minutes and forty keystrokes later is not an answer.
    let (mut app, _dir) = three("dirty-close-abandoned");
    // `open_path` leaves focus on the editor, so the keystroke lands in the buffer.
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('X'),
        KeyModifiers::NONE,
    )))
    .unwrap();

    app.handle_chord(chord("ctrl+w")).unwrap();
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('Y'),
        KeyModifiers::NONE,
    )))
    .unwrap();
    app.handle_chord(chord("ctrl+w")).unwrap();

    assert_eq!(app.tab_count(), 3, "a stale confirmation discarded work");
}

#[test]
fn switching_tabs_moves_the_watcher_to_the_file_on_screen() {
    // `rewatch` follows the active file, and until tabs there was only ever one
    // to follow. A switch that skipped it would leave the editor watching the
    // file just left — and this test would hang on the channel rather than fail
    // on an assertion, which is the honest shape for "the event never came".
    let dir = fixture("rewatch");
    let (tx, rx) = typ_app::run::channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);

    app.open_path(&dir.join("a.rs")).unwrap();
    app.open_path(&dir.join("b.rs")).unwrap();
    app.activate_tab(0);

    let watched = dir.join("a.rs");
    std::fs::write(&watched, "fn a() { changed_on_disk() }\n").unwrap();

    loop {
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(typ_core::AppEvent::FileChanged(path)) if path == watched => break,
            Ok(_) => continue,
            Err(e) => panic!("the watcher never reported a.rs after the switch: {e}"),
        }
    }
}
