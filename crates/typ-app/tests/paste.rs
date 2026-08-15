//! Bracketed paste: one edit, not one per character.
//!
//! Without it a paste arrives as N key events — N loop passes, N repaints, N
//! undo steps — and any chord inside the pasted text executes as a command
//! rather than being inserted, which is the part that corrupts a file rather
//! than merely being slow.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use typ_app::App;
use typ_core::Panel;

/// The clipboard register is process-wide, which is what a clipboard *is*, and
/// cargo runs tests in threads inside one process. Every test here routes a
/// paste through that register, so without this they read each other's text.
///
/// The same hazard `typ-panel-editor/tests/clipboard.rs` documents — and it was
/// reintroduced here two tasks after being fixed there. It passed on Windows
/// and Linux by timing and failed on macOS, which is the entire argument for
/// running CI on three platforms rather than trusting a local run.
fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-paste-test").join(name);
    // Created, never removed first: on Windows `remove_dir_all` can return
    // before the directory is gone, and each test already owns its own name.
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("file.rs"), "\n").unwrap();
    dir
}

fn app_with_file(name: &str) -> App {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("file.rs")).unwrap();
    app
}

#[test]
fn a_multi_line_paste_lands_whole() {
    let _guard = exclusive();
    let mut app = app_with_file("multi-line");

    app.handle_paste("hello\nworld".to_string()).unwrap();

    assert_eq!(app.editor_mut().line_text(0), "hello");
    assert_eq!(app.editor_mut().line_text(1), "world");
}

#[test]
fn a_paste_is_one_undo_step() {
    let _guard = exclusive();
    let mut app = app_with_file("one-undo");

    app.handle_paste("one\ntwo\nthree".to_string()).unwrap();
    assert_eq!(app.editor_mut().line_text(0), "one");

    app.editor_mut().apply_action(typ_core::Action::Undo);

    assert_eq!(
        app.editor_mut().line_text(0),
        "",
        "a paste is one thing the user did, so it is one thing to take back"
    );
}

#[test]
fn pasting_into_an_open_prompt_types_into_it() {
    let _guard = exclusive();
    let mut app = app_with_file("into-prompt");
    app.handle_chord(typ_core::KeyChord::from_event(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('f'),
            crossterm::event::KeyModifiers::CONTROL,
        ),
    ))
    .unwrap();
    assert!(app.prompt().is_some(), "Ctrl+F opens the search prompt");

    app.handle_paste("needle".to_string()).unwrap();

    assert_eq!(app.prompt().unwrap().input(), "needle");
    assert_eq!(
        app.editor_mut().line_text(0),
        "",
        "a paste while searching is a search term, not an edit"
    );
}

#[test]
fn control_characters_never_reach_the_prompt() {
    let _guard = exclusive();
    let mut app = app_with_file("prompt-control");
    app.handle_chord(typ_core::KeyChord::from_event(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('f'),
            crossterm::event::KeyModifiers::CONTROL,
        ),
    ))
    .unwrap();

    app.handle_paste("two\nlines".to_string()).unwrap();

    assert_eq!(
        app.prompt().unwrap().input(),
        "twolines",
        "a one-line prompt cannot hold a newline, so it must not pretend to"
    );
}
