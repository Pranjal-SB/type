//! The picker floats: where it lands, and who gets the keyboard while it is up.

use std::fs;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typ_app::{App, layout};
use typ_core::KeyChord;

struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("typ-overlay-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("fixture root");
        fs::write(dir.join("main.rs"), "fn main() {}\n").expect("fixture file");
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

fn contains(outer: Rect, inner: Rect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.right() <= outer.right()
        && inner.bottom() <= outer.bottom()
}

#[test]
fn a_centered_rect_sits_inside_the_frame() {
    let frame = Rect::new(0, 0, 80, 24);
    let centered = layout::centered(frame, 60, 15);
    assert!(contains(frame, centered), "{centered:?} escaped {frame:?}");
    assert_eq!((centered.width, centered.height), (60, 15));
}

#[test]
fn a_centered_rect_is_actually_centered() {
    let centered = layout::centered(Rect::new(0, 0, 80, 24), 60, 16);
    assert_eq!(centered.x, 10);
    assert_eq!(centered.y, 4);
}

#[test]
fn a_frame_smaller_than_the_overlay_clamps_rather_than_overflowing() {
    // 20x5 is smaller than any sane picker. The rect must still be valid and
    // inside the frame: a Rect wider than the buffer panics on the first write.
    let frame = Rect::new(0, 0, 20, 5);
    let centered = layout::centered(frame, 60, 15);
    assert!(contains(frame, centered), "{centered:?} escaped {frame:?}");
}

#[test]
fn a_zero_sized_frame_produces_a_zero_sized_rect() {
    let frame = Rect::new(0, 0, 0, 0);
    let centered = layout::centered(frame, 60, 15);
    assert_eq!((centered.width, centered.height), (0, 0));
}

#[test]
fn a_centered_rect_respects_a_frame_that_does_not_start_at_the_origin() {
    let frame = Rect::new(4, 2, 40, 20);
    let centered = layout::centered(frame, 20, 10);
    assert!(contains(frame, centered), "{centered:?} escaped {frame:?}");
}

#[test]
fn the_picker_is_closed_until_it_is_opened() {
    let fixture = Fixture::new("closed");
    let app = App::new(&fixture.0).expect("app");
    assert!(app.picker().is_none());
}

#[test]
fn opening_the_picker_takes_the_keyboard() {
    // Typing into the picker must not also reach the editor or the tree. The
    // failure this guards is silent and destructive: `j` selects a row *and*
    // inserts a character into the open buffer.
    let fixture = Fixture::new("keyboard");
    let mut app = App::new(&fixture.0).expect("app");
    app.open_picker();

    app.handle_chord(chord(KeyCode::Char('m'))).expect("key");

    assert_eq!(app.picker().expect("open").query(), "m");
    assert_eq!(
        app.editor().buffer().text(),
        "",
        "the keystroke reached the buffer as well"
    );
}

#[test]
fn escape_closes_the_picker_and_returns_focus() {
    let fixture = Fixture::new("escape");
    let mut app = App::new(&fixture.0).expect("app");
    let before = app.focus();
    app.open_picker();
    assert!(app.picker().is_some());

    app.handle_chord(chord(KeyCode::Esc)).expect("key");

    assert!(app.picker().is_none(), "escape left the overlay up");
    assert_eq!(app.focus(), before, "focus did not come back");
}

#[test]
fn with_the_picker_closed_nothing_about_dispatch_changes() {
    // The overlay must be free when it is not up. This is the regression test
    // for wiring it into `handle_chord` and accidentally swallowing every key.
    let fixture = Fixture::new("closed-dispatch");
    let mut app = App::new(&fixture.0).expect("app");
    // `open_path` focuses the editor itself; cycling here would move away
    // from it.
    app.open_path(&fixture.0.join("main.rs")).expect("open");

    app.handle_chord(chord(KeyCode::Char('x'))).expect("key");

    assert!(
        app.editor().buffer().text().starts_with('x'),
        "got {:?}",
        app.editor().buffer().text()
    );
}

#[test]
fn opening_the_picker_starts_an_index() {
    // The walk happens on open, never at startup: cold start is budgeted at
    // 100 ms and a parallel walk of a mid-size tree measured 94.7 ms.
    let fixture = Fixture::new("index");
    let mut app = App::new(&fixture.0).expect("app");
    assert!(!app.index_requested());
    app.open_picker();
    assert!(app.index_requested());
}

#[test]
fn reopening_the_picker_keeps_the_previous_results() {
    // 94.7 ms of walk is invisible behind a stale list and obvious behind an
    // empty one.
    let fixture = Fixture::new("reopen");
    let mut app = App::new(&fixture.0).expect("app");
    app.open_picker();
    let generation = app.request_filter(String::new(), 10);
    app.handle_found(typ_find::Found::Files {
        generation,
        hits: vec![typ_find::FileHit {
            path: "main.rs".into(),
            indices: Vec::new(),
        }],
    });
    app.handle_chord(chord(KeyCode::Esc)).expect("key");

    app.open_picker();
    assert_eq!(
        app.picker().expect("open").hits().len(),
        1,
        "the previous results were thrown away"
    );
}
