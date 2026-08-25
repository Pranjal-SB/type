//! Ctrl+P, from the binding to the file being open.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::run::{AppReceiver, channel, step};
use typ_core::{Action, KeyChord, Keymap};

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("typ-flow-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("fixture root");
        for (rel, body) in [
            ("src/main.rs", "fn main() {}\n"),
            ("src/highlight.rs", "pub fn paint() {}\n"),
            ("README.md", "# hi\n"),
        ] {
            let path = dir.join(rel);
            fs::create_dir_all(path.parent().expect("has a parent")).expect("fixture dirs");
            fs::write(path, body).expect("fixture file");
        }
        Fixture(dir)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn chord(code: KeyCode) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn ctrl_p_is_bound_to_the_file_picker() {
    // Invariant 3: a table row, never a match arm.
    let keymap = Keymap::default_bindings();
    let chord = KeyChord::from_event(KeyEvent::new(KeyCode::Char('p'), KeyModifiers::CONTROL));
    assert_eq!(keymap.lookup(&chord), Some(Action::OpenFilePicker));
}

#[test]
fn ctrl_p_opens_the_overlay() {
    let fixture = Fixture::new("open");
    let mut app = App::new(&fixture.0).expect("app");
    assert!(app.picker().is_none());

    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )))
    .expect("key");

    assert!(app.picker().is_some(), "ctrl+p did not open the picker");
}

/// Drive the loop until the picker holds results, or fail rather than hang.
///
/// The walk and the ranking are on a worker, so an integration test has to wait
/// for them — a synchronous assertion right after `open_picker` asserts that a
/// thread has already been scheduled, which is true most of the time and is
/// exactly the flake that costs an afternoon.
fn pump_until_hits(app: &mut App, rx: &AppReceiver) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while app.picker().is_some_and(|p| p.hits().is_empty()) {
        assert!(Instant::now() < deadline, "no results within 20s");
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(event) => step(app, event, AREA).expect("step"),
            Err(e) => panic!("no event: {e}"),
        };
    }
}

#[test]
fn typing_a_name_and_pressing_enter_opens_that_file() {
    let fixture = Fixture::new("endtoend");
    let (tx, rx) = channel();
    let mut app = App::new(&fixture.0).expect("app");
    app.set_event_sender(tx);

    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )))
    .expect("key");

    for c in "highlight".chars() {
        app.handle_chord(chord(KeyCode::Char(c))).expect("key");
    }
    pump_until_hits(&mut app, &rx);

    assert_eq!(
        app.picker().expect("open").selection().expect("a hit").path,
        "src/highlight.rs"
    );

    app.handle_chord(chord(KeyCode::Enter)).expect("key");

    assert!(
        app.picker().is_none(),
        "choosing a file left the overlay up"
    );
    assert!(
        app.editor().buffer().text().contains("paint"),
        "the wrong file is open: {:?}",
        app.editor().buffer().text()
    );
}

#[test]
fn an_empty_query_lists_the_project() {
    // The opening screen. A blank list would make the picker look broken until
    // the first keystroke.
    let fixture = Fixture::new("opening");
    let (tx, rx) = channel();
    let mut app = App::new(&fixture.0).expect("app");
    app.set_event_sender(tx);

    app.open_picker();
    pump_until_hits(&mut app, &rx);

    assert_eq!(app.picker().expect("open").hits().len(), 3);
}
