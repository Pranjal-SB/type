//! Goto-definition and hover.
//!
//! The first requests with answers, so this is where correlation, staleness
//! and cancellation get exercised. A response is not a result: it describes a
//! question asked some milliseconds ago, and by the time it lands the cursor
//! may have moved, the tab may have changed, or a newer question may already
//! be in flight.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::lsp::ServerConfig;
use typ_app::run::{AppReceiver, channel, step_batch};
use typ_buffer::Position;
use typ_core::AppEvent;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 60,
    height: 12,
};

const WAIT: Duration = Duration::from_secs(10);

const SOURCE: &str =
    "fn one() {}\nfn two() {}\nfn three() {}\nfn four() {}\nfn five() {}\nfn six() {}\n";

fn fake() -> &'static str {
    env!("CARGO_BIN_EXE_typ-app-fake-server")
}

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-lsp-navigate").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), SOURCE).unwrap();
    std::fs::write(dir.join("target.rs"), SOURCE).unwrap();
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

/// An app with `a.rs` open and its server initialized.
fn ready(name: &str, flags: &[&str]) -> (App, AppReceiver, PathBuf) {
    let dir = fixture(name);
    let path = dir.join("a.rs");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.add_language_server(server(flags));
    app.set_event_sender(tx);
    app.open_path(&path).unwrap();
    assert!(
        pump_until(&mut app, &rx, |a| a
            .lsp_notifications_of("textDocument/didOpen")
            == 1),
        "the server never initialized"
    );
    (app, rx, path)
}

fn act(app: &mut App, action: typ_core::Action) {
    app.apply_named_action(action).unwrap();
}

#[test]
fn goto_definition_in_the_same_file_moves_without_opening_a_second_tab() {
    let (mut app, rx, _) = ready("same-file", &[]);
    let before = app.tab_count();
    act(&mut app, typ_core::Action::GotoDefinition);
    assert!(pump_until(&mut app, &rx, |a| a.editor().cursor().line == 4));
    assert_eq!(app.editor().cursor(), Position { line: 4, col: 2 });
    assert_eq!(app.tab_count(), before, "it opened a tab for its own file");
}

#[test]
fn goto_definition_opens_the_target_and_puts_the_cursor_on_it() {
    let (mut app, rx, _) = ready("elsewhere", &["--definition-elsewhere"]);
    act(&mut app, typ_core::Action::GotoDefinition);
    assert!(pump_until(&mut app, &rx, |a| a.tab_count() == 2));
    assert!(
        app.editor()
            .path()
            .is_some_and(|p| p.ends_with("target.rs")),
        "the wrong file is on screen"
    );
    assert_eq!(app.editor().cursor(), Position { line: 4, col: 2 });
}

#[test]
fn a_definition_naming_a_file_that_no_longer_exists_says_so() {
    let (mut app, rx, _) = ready("missing", &["--definition-missing"]);
    act(&mut app, typ_core::Action::GotoDefinition);
    assert!(pump_until(&mut app, &rx, |a| a.status().is_some()));
    let status = app.status().unwrap_or_default().to_string();
    assert!(status.contains("not-there.rs"), "status was: {status}");
    assert_eq!(app.tab_count(), 1, "a missing file must not open a tab");
}

#[test]
fn a_server_with_nothing_to_say_leaves_the_cursor_alone() {
    // The ordinary state for the first minute of any real project: the server
    // is up and has not finished indexing.
    let (mut app, rx, _) = ready("no-answer", &["--no-definition"]);
    act(&mut app, typ_core::Action::GotoDefinition);
    settle(&mut app, &rx);
    assert_eq!(app.editor().cursor(), Position { line: 0, col: 0 });
    assert_eq!(app.tab_count(), 1);
}

#[test]
fn goto_definition_with_no_server_says_so_and_changes_nothing() {
    let dir = fixture("no-server");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);
    app.open_path(&dir.join("a.rs")).unwrap();
    act(&mut app, typ_core::Action::GotoDefinition);
    settle(&mut app, &rx);
    assert_eq!(app.editor().cursor(), Position { line: 0, col: 0 });
    assert!(
        app.status().is_some_and(|s| s.contains("language server")),
        "status was: {:?}",
        app.status()
    );
}

#[test]
fn a_response_that_arrives_after_the_cursor_moved_is_discarded() {
    // The generation lesson from M2.7's parses, applied to requests. A jump
    // that lands after the user has moved on takes them somewhere they were no
    // longer asking about.
    let (mut app, rx, _) = ready("stale", &[]);
    act(&mut app, typ_core::Action::GotoDefinition);
    // Move before the answer is pumped in.
    step_batch(&mut app, vec![key(KeyCode::Down)], AREA).unwrap();
    settle(&mut app, &rx);
    assert_eq!(
        app.editor().cursor(),
        Position { line: 1, col: 0 },
        "a stale answer moved the cursor"
    );
}

#[test]
fn hover_renders_markdown_as_text_rather_than_as_markup() {
    // A server returns `MarkupContent`. Painting the backticks is worse than
    // painting nothing.
    let (mut app, rx, _) = ready("hover", &[]);
    act(&mut app, typ_core::Action::Hover);
    assert!(pump_until(&mut app, &rx, |a| a.hover().is_some()));
    let hover = app.hover().unwrap().to_string();
    assert!(hover.contains("fn fake()"), "hover was: {hover:?}");
    assert!(!hover.contains("```"), "the fences were painted: {hover:?}");
    assert!(!hover.contains("**"), "the emphasis was painted: {hover:?}");
}

#[test]
fn a_plain_string_hover_is_shown_as_it_arrived() {
    let (mut app, rx, _) = ready("hover-plain", &["--hover-plain"]);
    act(&mut app, typ_core::Action::Hover);
    assert!(pump_until(&mut app, &rx, |a| a.hover().is_some()));
    assert_eq!(app.hover().unwrap(), "plain words");
}

#[test]
fn a_superseded_hover_request_is_cancelled() {
    // Hover fires on demand today and on cursor movement later. A server still
    // grinding on an answer nobody wants is burning a core for nothing.
    let (mut app, _rx, _) = ready("cancel", &[]);
    act(&mut app, typ_core::Action::Hover);
    act(&mut app, typ_core::Action::Hover);
    assert_eq!(app.lsp_notifications_of("$/cancelRequest"), 1);
}

#[test]
fn moving_the_cursor_dismisses_the_hover() {
    // It described a position. Leaving it up over a different one is a box
    // saying something true about somewhere else.
    let (mut app, rx, _) = ready("dismiss", &[]);
    act(&mut app, typ_core::Action::Hover);
    assert!(pump_until(&mut app, &rx, |a| a.hover().is_some()));
    step_batch(&mut app, vec![key(KeyCode::Right)], AREA).unwrap();
    assert!(app.hover().is_none());
}

#[test]
fn escape_dismisses_the_hover() {
    let (mut app, rx, _) = ready("escape", &[]);
    act(&mut app, typ_core::Action::Hover);
    assert!(pump_until(&mut app, &rx, |a| a.hover().is_some()));
    step_batch(&mut app, vec![key(KeyCode::Esc)], AREA).unwrap();
    assert!(app.hover().is_none());
}

#[test]
fn both_actions_are_reachable_from_the_keymap_and_the_palette() {
    // Every named action is in the palette for free. The bindings are the part
    // that has to be decided.
    let keymap = typ_core::Keymap::default_bindings();
    assert_eq!(
        keymap.bindings_for(typ_core::Action::GotoDefinition),
        vec!["f12"]
    );
    assert_eq!(keymap.bindings_for(typ_core::Action::Hover), vec!["alt+h"]);
    assert!(
        typ_core::Action::ALL.contains(&typ_core::Action::Hover),
        "an action outside ALL cannot be reached by name"
    );
}

// --- content modified ----------------------------------------------------
//
// `-32801 ContentModified` is not a refusal. rust-analyzer answers with it
// whenever a salsa cancellation lands mid-request and the handler has no
// `ALLOW_RETRYING` of its own, which is most of them, so a client that treats
// it as an error gets nothing back from a healthy server, and gets it silently.
// Driving the real rust-analyzer, the first hover after indexing finished came
// back this way every time; the second one answered.

#[test]
fn a_content_modified_answer_is_asked_again() {
    let (mut app, rx, _) = ready("retry-once", &["--content-modified=1"]);
    act(&mut app, typ_core::Action::Hover);
    assert!(
        pump_until(&mut app, &rx, |a| a.hover().is_some()),
        "the retry never happened, so a healthy server said nothing"
    );
    assert!(app.hover().unwrap().contains("fn fake()"));
    assert_eq!(app.lsp_requests_of("textDocument/hover"), 2);
}

#[test]
fn retrying_gives_up_rather_than_spinning() {
    // A server that always cancels must not be asked forever. The retry is
    // paced by the round trip rather than by a timer, so a loop here is a busy
    // server rather than a busy client: still worth bounding. One ask and
    // three retries.
    let (mut app, rx, _) = ready("retry-forever", &["--content-modified=99"]);
    act(&mut app, typ_core::Action::Hover);
    assert!(pump_until(&mut app, &rx, |a| a
        .lsp_requests_of("textDocument/hover")
        == 4));
    settle(&mut app, &rx);
    assert_eq!(app.lsp_requests_of("textDocument/hover"), 4);
    assert!(app.hover().is_none());
}

#[test]
fn a_real_error_is_not_retried() {
    // Only -32801 means "ask again". Anything else is an answer. The fake sends
    // a real `InternalError` here, so a client retrying on every code fails.
    let (mut app, rx, _) = ready("real-error", &["--hover-error"]);
    act(&mut app, typ_core::Action::Hover);
    settle(&mut app, &rx);
    assert_eq!(app.lsp_requests_of("textDocument/hover"), 1);
    assert!(app.hover().is_none());
}

// --- an answer that arrived empty ----------------------------------------
//
// `null` is a legal answer to both requests and the ordinary one while a
// server is still indexing. Silence is the wrong way to report it: both of
// these are explicit keypresses, and a keypress that produces nothing at all
// is indistinguishable from a feature that does not work. Driving the real
// rust-analyzer, an `Alt+H` that answered `null` looked exactly like the
// broken hover it was not.

#[test]
fn a_definition_that_is_not_there_yet_says_so() {
    let (mut app, rx, _) = ready("empty-definition", &["--no-definition"]);
    act(&mut app, typ_core::Action::GotoDefinition);
    assert!(pump_until(&mut app, &rx, |a| a.status().is_some()));
    assert!(
        app.status().is_some_and(|s| s.contains("definition")),
        "status was: {:?}",
        app.status()
    );
    assert_eq!(app.editor().cursor(), Position { line: 0, col: 0 });
}

#[test]
fn a_hover_with_nothing_to_show_says_so() {
    let (mut app, rx, _) = ready("empty-hover", &["--hover-empty"]);
    act(&mut app, typ_core::Action::Hover);
    assert!(pump_until(&mut app, &rx, |a| a.status().is_some()));
    assert!(
        app.status().is_some_and(|s| s.contains("hover")),
        "status was: {:?}",
        app.status()
    );
    assert!(app.hover().is_none(), "there was nothing to put in a box");
}

#[test]
fn an_answer_that_had_something_says_nothing() {
    // The message is for the empty case only. A status line that reports every
    // success is a status line nobody reads.
    let (mut app, rx, _) = ready("quiet-on-success", &[]);
    act(&mut app, typ_core::Action::Hover);
    assert!(pump_until(&mut app, &rx, |a| a.hover().is_some()));
    assert_eq!(app.status(), None);
}
