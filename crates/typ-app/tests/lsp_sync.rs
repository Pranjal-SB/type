//! The document the server sees is the document on screen.
//!
//! `typ-lsp`'s own tests prove a server starts and a frame arrives. None of
//! them touches `App`, so a document that is never announced, announced twice,
//! or announced with a stale version passes every one of them. This is the
//! test that fails when the two halves are not connected.
//!
//! **The most important tests here are the last three.** A file with no server
//! configured, a server that is not installed, and the untitled buffer must all
//! be indistinguishable from the editor TYPE was before this milestone.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::lsp::ServerConfig;
use typ_app::run::{AppReceiver, channel, step_batch};
use typ_core::AppEvent;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

/// Wait on the channel, never on the clock.
const WAIT: Duration = Duration::from_secs(10);

fn fake() -> &'static str {
    env!("CARGO_BIN_EXE_typ-app-fake-server")
}

fn fixture(name: &str, file: &str, contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-lsp-sync").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(file), contents).unwrap();
    dir
}

fn rust_server(command: &str) -> ServerConfig {
    ServerConfig {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: command.into(),
        args: Vec::new(),
        roots: vec!["Cargo.toml".into()],
    }
}

fn key(c: char) -> AppEvent {
    AppEvent::Input(Event::Key(KeyEvent::new(
        KeyCode::Char(c),
        KeyModifiers::NONE,
    )))
}

/// Drive the loop until `done` holds, applying every event the way `run` does.
///
/// The handshake is not synchronous — `Client::start` returns immediately and
/// the app learns the server is ready when the response arrives on the channel.
/// A test that does not pump asserts against a client that has not been allowed
/// to finish saying hello.
fn pump_until(app: &mut App, rx: &AppReceiver, done: impl Fn(&App) -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if done(app) {
            return true;
        }
        if let Ok(event) = rx.recv_timeout(Duration::from_millis(250)) {
            let mut batch = vec![event];
            batch.extend(rx.try_iter());
            step_batch(app, batch, AREA).unwrap();
        }
    }
    done(app)
}

/// Let anything already queued settle, without waiting for a condition.
fn settle(app: &mut App, rx: &AppReceiver) {
    let deadline = Instant::now() + Duration::from_millis(400);
    while Instant::now() < deadline {
        if let Ok(event) = rx.recv_timeout(Duration::from_millis(50)) {
            let mut batch = vec![event];
            batch.extend(rx.try_iter());
            step_batch(app, batch, AREA).unwrap();
        }
    }
}

/// An app with one Rust file open and a server that has finished initializing.
fn app_with_fake_server(name: &str) -> (App, AppReceiver, PathBuf) {
    let dir = fixture(name, "a.rs", "fn main() {}\n");
    let path = dir.join("a.rs");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.add_language_server(rust_server(fake()));
    app.set_event_sender(tx);
    // `open_path` settles on the editor already — `settle_active_tab` sets the
    // focus — so nothing here has to ask for it.
    app.open_path(&path).unwrap();
    assert!(
        pump_until(&mut app, &rx, |a| a
            .lsp_notifications_of("textDocument/didOpen")
            == 1),
        "the handshake never completed and nothing was announced"
    );
    (app, rx, path)
}

#[test]
fn opening_a_file_sends_did_open_once() {
    let (mut app, rx, _) = app_with_fake_server("open-once");
    // Another pass over the loop must not announce it again.
    step_batch(&mut app, vec![key('x')], AREA).unwrap();
    settle(&mut app, &rx);
    assert_eq!(app.lsp_notifications_of("textDocument/didOpen"), 1);
}

#[test]
fn a_burst_of_keystrokes_in_one_batch_sends_one_did_change() {
    // The coalescing point already exists: `step_batch` drains the queue and
    // draws one frame. Five keys in one batch is one notification, not five.
    let (mut app, _rx, _) = app_with_fake_server("one-per-batch");
    let batch: Vec<AppEvent> = "hello".chars().map(key).collect();
    step_batch(&mut app, batch, AREA).unwrap();
    assert_eq!(app.lsp_notifications_of("textDocument/didChange"), 1);
}

#[test]
fn the_version_increases_with_every_change() {
    let (mut app, _rx, path) = app_with_fake_server("versions");
    let first = app.lsp_document_version(&path).expect("it is open");
    step_batch(&mut app, vec![key('a')], AREA).unwrap();
    let second = app.lsp_document_version(&path).unwrap();
    step_batch(&mut app, vec![key('b')], AREA).unwrap();
    let third = app.lsp_document_version(&path).unwrap();
    assert!(first < second && second < third, "{first} {second} {third}");
}

#[test]
fn a_clean_buffer_sends_nothing() {
    let (mut app, _rx, _) = app_with_fake_server("clean");
    // A batch that changes no text: a cursor move.
    let right = AppEvent::Input(Event::Key(KeyEvent::new(
        KeyCode::Right,
        KeyModifiers::NONE,
    )));
    step_batch(&mut app, vec![right], AREA).unwrap();
    assert_eq!(app.lsp_notifications_of("textDocument/didChange"), 0);
}

#[test]
fn closing_a_tab_sends_did_close() {
    let (mut app, _rx, _) = app_with_fake_server("close");
    app.close_tab(app.active_tab());
    step_batch(&mut app, vec![key('x')], AREA).unwrap();
    assert_eq!(app.lsp_notifications_of("textDocument/didClose"), 1);
}

#[test]
fn saving_sends_did_save() {
    // rust-analyzer runs `cargo check` on save. Without this the pushed half of
    // diagnostics never fires at all and the client looks completely broken.
    let (mut app, _rx, _) = app_with_fake_server("save");
    step_batch(&mut app, vec![key('x')], AREA).unwrap();
    let ctrl_s = AppEvent::Input(Event::Key(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
    )));
    step_batch(&mut app, vec![ctrl_s], AREA).unwrap();
    assert_eq!(app.lsp_notifications_of("textDocument/didSave"), 1);
}

#[test]
fn a_file_that_does_not_exist_yet_is_still_announced() {
    // `typ newfile.rs` opens a buffer at a path with nothing behind it. The
    // client owns the document's content either way, so the server is told
    // about it — and the URI it gets names a file it cannot read, which is
    // legal, and is the case a client that stats the path first gets wrong.
    let dir = fixture("unborn", "a.rs", "fn main() {}\n");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.add_language_server(rust_server(fake()));
    app.set_event_sender(tx);
    app.open_path(&dir.join("later.rs")).unwrap();
    assert!(
        pump_until(&mut app, &rx, |a| a
            .lsp_notifications_of("textDocument/didOpen")
            == 1),
        "a file with nothing on disk was never announced"
    );
}

#[test]
fn the_untitled_scratch_buffer_is_never_announced_to_a_server() {
    // `typ` with no arguments starts here, so this is the default state rather
    // than an edge case. No path means no URI, and a client that invents one
    // tells the server about a file that does not exist.
    let dir = fixture("untitled", "a.rs", "fn main() {}\n");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.add_language_server(rust_server(fake()));
    app.set_event_sender(tx);
    app.cycle_focus();
    step_batch(&mut app, vec![key('x')], AREA).unwrap();
    settle(&mut app, &rx);
    assert_eq!(app.lsp_notifications_of("textDocument/didOpen"), 0);
    assert_eq!(app.editor().buffer().line_text(0), "x", "editing must work");
}

#[test]
fn a_file_with_no_configured_server_sends_nothing_and_still_edits() {
    // The degradation path, and the most important test in the file.
    let dir = fixture("no-server", "a.md", "hello\n");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.add_language_server(rust_server(fake()));
    app.set_event_sender(tx);
    app.open_path(&dir.join("a.md")).unwrap();
    step_batch(&mut app, vec![key('x')], AREA).unwrap();
    settle(&mut app, &rx);
    assert_eq!(app.lsp_notifications_of("textDocument/didOpen"), 0);
    assert_eq!(app.editor().buffer().line_text(0), "xhello");
}

#[test]
fn a_server_that_is_not_installed_is_silent_and_editing_continues() {
    // The default state on most machines: the binary named in config is not on
    // PATH. Nothing about that may reach the user as an error, and nothing
    // about it may stop a keystroke.
    let dir = fixture("absent", "a.rs", "fn main() {}\n");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.add_language_server(rust_server("definitely-not-a-language-server"));
    app.set_event_sender(tx);
    app.open_path(&dir.join("a.rs")).unwrap();
    step_batch(&mut app, vec![key('x')], AREA).unwrap();
    settle(&mut app, &rx);
    assert_eq!(app.lsp_notifications_of("textDocument/didOpen"), 0);
    assert_eq!(app.editor().buffer().line_text(0), "xfn main() {}");
}
