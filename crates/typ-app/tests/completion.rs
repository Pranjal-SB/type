//! Completion: asked off the render thread, once per batch, and only of a
//! server that offers it. A reply describing an older document is dropped.

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

const WAIT: Duration = Duration::from_secs(10);

/// The last line is empty, so what is typed there has nothing after it.
const SOURCE: &str = "fn one() {}\nfn two() {}\n";

const METHOD: &str = "textDocument/completion";

fn fake() -> &'static str {
    env!("CARGO_BIN_EXE_typ-app-fake-server")
}

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-lsp-completion").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), SOURCE).unwrap();
    dir
}

fn server(flags: &[&str]) -> ServerConfig {
    ServerConfig {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: fake().into(),
        args: flags.iter().map(|f| f.to_string()).collect(),
        roots: vec!["Cargo.toml".into()],
    }
}

fn key(code: KeyCode) -> AppEvent {
    AppEvent::Input(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
}

/// Each character its own batch, the way a person types.
fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        step_batch(app, vec![key(KeyCode::Char(c))], AREA).unwrap();
    }
}

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

/// An app with `a.rs` open, its server initialized and the caret on the
/// empty last line.
fn ready(name: &str, flags: &[&str]) -> (App, AppReceiver) {
    let dir = fixture(name);
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.add_language_server(server(flags));
    app.set_event_sender(tx);
    app.open_path(&dir.join("a.rs")).unwrap();
    assert!(
        pump_until(&mut app, &rx, |a| a
            .lsp_notifications_of("textDocument/didOpen")
            == 1),
        "the server never initialized"
    );
    app.editor_mut().goto(2, 0);
    (app, rx)
}

#[test]
fn typing_fo_lists_both_things_the_server_offers() {
    let (mut app, rx) = ready("offers", &[]);
    type_text(&mut app, "fo");
    assert!(
        pump_until(&mut app, &rx, |a| a.completion_labels().is_some()),
        "no list arrived"
    );
    let labels = app.completion_labels().unwrap();
    assert!(labels.contains(&"foo".to_string()), "{labels:?}");
    assert!(labels.contains(&"format".to_string()), "{labels:?}");
}

#[test]
fn a_reply_for_an_older_version_is_dropped_and_asked_again() {
    // The fake holds each answer until the next `didChange`, so the one for
    // `f` lands after `o` has been sent: it describes a document the server no
    // longer has.
    let (mut app, rx) = ready("late", &["--complete-late"]);
    type_text(&mut app, "f");
    assert!(pump_until(&mut app, &rx, |a| a.lsp_requests_of(METHOD) == 1));
    type_text(&mut app, "o");
    assert!(
        pump_until(&mut app, &rx, |a| a.lsp_requests_of(METHOD) == 2),
        "the stale answer was not followed by a fresh question"
    );
    settle(&mut app, &rx);
    assert_eq!(app.completion_labels(), None, "a stale answer was shown");
}

#[test]
fn a_server_without_a_completion_provider_is_never_asked() {
    let (mut app, rx) = ready("unoffered", &["--no-completion"]);
    type_text(&mut app, "fo");
    settle(&mut app, &rx);
    assert_eq!(app.lsp_requests_of(METHOD), 0);
    assert_eq!(app.completion_labels(), None);
}
