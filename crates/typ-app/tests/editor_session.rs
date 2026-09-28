//! Invariant 10, the part that is not an error path. Gap 138.
//!
//! `typ <file>` opens that file, blocks until closed, exits with an honest
//! code, never detaches. `typ/tests/cli.rs` covers only the failures, because
//! the rest needs a terminal and the workspace has no pseudo-terminal to give
//! the binary one. So this drives the session through `step_batch`, the loop
//! body `run` calls, from open to quit: the same dispatch the binary runs, one
//! frame below the tty.

use std::path::PathBuf;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::run::{Flow, step_batch};
use typ_core::AppEvent;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

/// `COMMIT_EDITMSG`, the file `git commit` hands `$EDITOR`.
fn commit_message(name: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join("typ-editor-session").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("COMMIT_EDITMSG");
    std::fs::write(&file, "\n# Please enter the commit message.\n").unwrap();
    (dir, file)
}

fn key(code: KeyCode, modifiers: KeyModifiers) -> AppEvent {
    AppEvent::Input(Event::Key(KeyEvent::new(code, modifiers)))
}

fn press(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> Flow {
    step_batch(app, vec![key(code, modifiers)], AREA).unwrap()
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        let flow = press(app, KeyCode::Char(c), KeyModifiers::NONE);
        assert_eq!(flow, Flow::Continue, "typing {c:?} ended the session");
    }
}

#[test]
fn an_editor_session_opens_the_file_and_ends_only_when_closed() {
    let (_dir, file) = commit_message("happy");
    let mut app = App::new(file.parent().unwrap()).unwrap();

    // What `main` does with `typ <file>`.
    app.open_all(std::slice::from_ref(&file)).unwrap();
    assert_eq!(
        app.editor().path(),
        Some(file.as_path()),
        "opened something else"
    );

    type_text(&mut app, "fix: a thing");
    assert_eq!(
        press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL),
        Flow::Continue,
        "saving ended the session: the caller would read a half-written file"
    );
    let on_disk = std::fs::read_to_string(&file).unwrap();
    assert!(
        on_disk.starts_with("fix: a thing"),
        "the caller reads the file after the editor exits, and it says {on_disk:?}"
    );

    assert_eq!(
        press(&mut app, KeyCode::Char('q'), KeyModifiers::CONTROL),
        Flow::Quit,
        "a clean quit did not end the session"
    );
}

#[test]
fn a_failed_save_does_not_let_the_first_quit_through() {
    // The dishonest exit: a save fails, the user presses Ctrl+Q believing it
    // worked, and the caller is told the edit is done. The session must stop
    // and say so instead.
    let (dir, file) = commit_message("failed-save");
    let mut app = App::new(&dir).unwrap();
    app.open_all(std::slice::from_ref(&file)).unwrap();
    type_text(&mut app, "fix: a thing");

    std::fs::remove_dir_all(&dir).unwrap();
    press(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
    let status = app.status().unwrap_or_default().to_string();
    assert!(status.contains("Save failed"), "status was {status:?}");

    assert_eq!(
        press(&mut app, KeyCode::Char('q'), KeyModifiers::CONTROL),
        Flow::Continue,
        "quit went straight through after a failed save"
    );
    assert!(
        app.status().is_some_and(|s| s.contains("again to discard")),
        "status was {:?}",
        app.status()
    );
}
