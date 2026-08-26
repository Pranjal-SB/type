//! Project search reads unsaved buffers from memory, and there is more than one.
//!
//! The override exists because a search that reports what is on disk while the
//! user is looking at unsaved edits is answering a question nobody asked. It
//! covered the one open buffer, which was every buffer until this milestone —
//! `request_grep` said so in a comment that predicted its own expiry: "One
//! editor panel today makes this a one-element vector; M4's tabs make it a
//! list."

use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::run::{channel, step};
use typ_core::{AppEvent, KeyChord};

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 100,
    height: 30,
};

const WAIT: Duration = Duration::from_secs(10);

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-grep-overrides").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "fn a() {}\n").unwrap();
    std::fs::write(dir.join("b.rs"), "fn b() {}\n").unwrap();
    dir
}

fn ch(c: char) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

/// Pump until the app is actually holding search results.
///
/// Not "until a `Lines` event arrives": the worker coalesces, so the answer to
/// an earlier keystroke can land first and be discarded as stale by
/// `handle_found`. Stopping on the event rather than on the state reads that
/// discarded answer as an empty result set.
fn pump_until_lines(app: &mut App, rx: &typ_app::run::AppReceiver) {
    loop {
        match rx.recv_timeout(WAIT) {
            Ok(event) => {
                let was_found = matches!(event, AppEvent::Found(typ_find::Found::Lines { .. }));
                step(app, event, AREA).unwrap();
                if was_found && !app.grep_hits().is_empty() {
                    return;
                }
            }
            Err(e) => panic!("no search result ever arrived: {e}"),
        }
    }
}

#[test]
fn a_dirty_background_tab_is_searched_from_memory_too() {
    // Type a word into a.rs, switch to b.rs, then search for it. The word is
    // nowhere on disk, so a search that reads only the active buffer from
    // memory finds nothing — and reports that the project does not contain what
    // the user is looking at.
    let dir = fixture("background");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);

    app.open_path(&dir.join("a.rs")).unwrap();
    for c in "zqxword".chars() {
        app.handle_chord(ch(c)).unwrap();
    }
    assert!(app.editor().is_dirty(), "the fixture must leave a.rs dirty");

    app.open_path(&dir.join("b.rs")).unwrap();
    assert_eq!(app.active_tab(), 1);

    app.open_search();
    for c in "zqxword".chars() {
        app.handle_chord(ch(c)).unwrap();
    }
    pump_until_lines(&mut app, &rx);

    let hits = app.grep_hits();
    assert!(
        hits.iter().any(|hit| hit.path.ends_with("a.rs")),
        "the unsaved edit in the background tab was not searched: {hits:?}"
    );
}

#[test]
fn a_clean_tab_is_not_shipped_to_the_worker() {
    // The override is a copy of the whole buffer over a channel. Sending one
    // for a file that matches what is on disk is bytes for nothing, and with
    // twenty tabs open it is twenty of them per keystroke.
    let dir = fixture("clean");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);

    app.open_path(&dir.join("a.rs")).unwrap();
    app.open_path(&dir.join("b.rs")).unwrap();

    app.open_search();
    for c in "fn".chars() {
        app.handle_chord(ch(c)).unwrap();
    }
    pump_until_lines(&mut app, &rx);

    // Both files say `fn` on disk, so both are found either way. What this
    // asserts is that nothing was *duplicated* by an override shadowing the
    // same path the walk already visited.
    let a_hits = app
        .grep_hits()
        .iter()
        .filter(|hit| hit.path.ends_with("a.rs"))
        .count();
    assert_eq!(a_hits, 1, "a.rs was searched twice: {:?}", app.grep_hits());
}
