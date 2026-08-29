//! What the server is doing, while it is doing it.
//!
//! Without this the editor looks broken for the first minute on any real
//! project: rust-analyzer takes tens of seconds to index and says nothing a
//! user can see, so every request in that window answers nothing and there is
//! no way to tell that from a client that does not work.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::layout::Rect;
use typ_app::App;
use typ_app::lsp::ServerConfig;
use typ_app::run::{AppReceiver, channel, step_batch};
use typ_app::status::SegmentId;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 12,
};

const WAIT: Duration = Duration::from_secs(10);

fn fake() -> &'static str {
    env!("CARGO_BIN_EXE_typ-app-fake-server")
}

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-lsp-progress").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "fn main() {}\n").unwrap();
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
    let deadline = Instant::now() + Duration::from_millis(500);
    while Instant::now() < deadline {
        if let Ok(event) = rx.recv_timeout(Duration::from_millis(50)) {
            let mut batch = vec![event];
            batch.extend(rx.try_iter());
            step_batch(app, batch, AREA).unwrap();
        }
    }
}

fn ready(name: &str, flags: &[&str]) -> (App, AppReceiver) {
    let dir = fixture(name);
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.add_language_server(server(flags));
    app.set_event_sender(tx);
    app.open_path(&dir.join("a.rs")).unwrap();
    (app, rx)
}

fn progress_segment(app: &App) -> Option<String> {
    app.status_segments()
        .into_iter()
        .find(|s| s.id == SegmentId::Progress)
        .map(|s| s.text)
}

#[test]
fn a_begin_puts_the_work_in_the_status_bar() {
    let (mut app, rx) = ready("begin", &["--progress"]);
    assert!(pump_until(&mut app, &rx, |a| progress_segment(a).is_some()));
    let text = progress_segment(&app).unwrap();
    assert!(text.contains("Indexing"), "segment was: {text}");
}

#[test]
fn a_percentage_is_shown_when_the_server_gives_one() {
    let (mut app, rx) = ready("percent", &["--progress"]);
    assert!(pump_until(&mut app, &rx, |a| progress_segment(a)
        .is_some_and(|t| t.contains("40"))));
}

#[test]
fn an_end_clears_it() {
    let (mut app, rx) = ready("end", &["--progress"]);
    assert!(pump_until(&mut app, &rx, |a| progress_segment(a).is_some()));
    app.notify_server_for_test("fake/endProgress");
    // "Fetching" never ends, so the segment stays — what must change is which
    // work it names.
    assert!(pump_until(&mut app, &rx, |a| progress_segment(a)
        .is_some_and(|t| !t.contains("Indexing"))));
}

#[test]
fn two_concurrent_tokens_do_not_clobber_each_other() {
    // rust-analyzer runs several at once — indexing, fetching, building proc
    // macros. A single slot would make the bar flicker between them.
    let (mut app, rx) = ready("two", &["--progress"]);
    assert!(pump_until(&mut app, &rx, |a| a.progress_count() == 2));
    // The bar shows the first — the work that has been running longest — and
    // the second is still there behind it rather than having replaced it.
    assert!(
        progress_segment(&app).is_some_and(|t| t.contains("Indexing")),
        "the newer token took the bar: {:?}",
        progress_segment(&app)
    );
}

#[test]
fn a_server_that_asks_to_create_a_token_is_answered() {
    // `window/workDoneProgress/create` is a *request*. A client that never
    // answers leaves the server waiting, and rust-analyzer really sends it.
    let (mut app, rx) = ready("create", &["--progress-create"]);
    assert!(
        pump_until(&mut app, &rx, |a| progress_segment(a).is_some()),
        "the server never got past its own request"
    );
}

#[test]
fn a_quiet_server_puts_nothing_on_the_bar() {
    let (mut app, rx) = ready("quiet", &[]);
    settle(&mut app, &rx);
    assert_eq!(progress_segment(&app), None);
}

#[test]
fn progress_sits_before_the_position() {
    // Read right to left the bar goes from "where am I" outwards, and what a
    // background job is doing is the outermost thing on it.
    let (mut app, rx) = ready("order", &["--progress"]);
    assert!(pump_until(&mut app, &rx, |a| progress_segment(a).is_some()));
    let ids: Vec<SegmentId> = app.status_segments().into_iter().map(|s| s.id).collect();
    let progress = ids.iter().position(|id| *id == SegmentId::Progress);
    let position = ids.iter().position(|id| *id == SegmentId::Position);
    assert!(progress < position, "{ids:?}");
}
