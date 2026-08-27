//! Diagnostics arrive, land on the right tab, and survive the next keystroke.
//!
//! The last one is the part that is easy to leave out and impossible to miss
//! once it is wrong: a server describes the file as it was some milliseconds
//! ago, and between that publish and the next the user keeps typing. Without a
//! shift the squiggles sit under the wrong words, and on a slow server that is
//! a long time to look broken.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::lsp::ServerConfig;
use typ_app::run::{AppReceiver, channel, step_batch};
use typ_core::{AppEvent, Severity};

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

const WAIT: Duration = Duration::from_secs(10);

/// Eight lines, so the fake server's diagnostic at line 5 lands inside it.
const SOURCE: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n    let c = 3;\n    let d = 4;\n    let e = 5;\n    let f = 6;\n}\n";

fn fake() -> &'static str {
    env!("CARGO_BIN_EXE_typ-app-fake-server")
}

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-lsp-diagnostics").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), SOURCE).unwrap();
    std::fs::write(dir.join("b.rs"), SOURCE).unwrap();
    dir
}

fn server(flags: &[&str]) -> ServerConfig {
    ServerConfig {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: fake().into(),
        args: flags.iter().map(|f| f.to_string()).collect(),
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

/// An app with `a.rs` open and one diagnostic already published against it.
fn app_with_diagnostic(name: &str, flags: &[&str]) -> (App, AppReceiver, PathBuf) {
    let dir = fixture(name);
    let path = dir.join("a.rs");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.add_language_server(server(flags));
    app.set_event_sender(tx);
    app.open_path(&path).unwrap();
    assert!(
        pump_until(&mut app, &rx, |a| !a.diagnostics().is_empty()),
        "nothing was ever published"
    );
    (app, rx, path)
}

#[test]
fn a_publish_reaches_the_tab_it_names() {
    let (app, _rx, _) = app_with_diagnostic("reaches", &["--push"]);
    let diagnostics = app.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0].range.0.line, 5);
    assert_eq!(diagnostics[0].severity, Severity::Error);
    assert_eq!(diagnostics[0].message, "fake: on open");
    assert_eq!(diagnostics[0].source.as_deref(), Some("fake"));
}

#[test]
fn a_publish_for_a_file_no_longer_open_is_dropped() {
    let (mut app, rx, _) = app_with_diagnostic("dropped", &["--push"]);
    app.close_tab(app.active_tab());
    settle(&mut app, &rx);
    assert!(
        app.diagnostics().is_empty(),
        "a closed file kept its diagnostics: {:?}",
        app.diagnostics()
    );
}

#[test]
fn a_diagnostic_shifts_when_a_line_is_inserted_above_it() {
    let (mut app, _rx, _) = app_with_diagnostic("shift", &["--push"]);
    assert_eq!(app.diagnostics()[0].range.0.line, 5);
    // The caret starts at the top of the file. One Enter pushes everything
    // below it down a line, including the diagnostic.
    step_batch(&mut app, vec![key(KeyCode::Enter)], AREA).unwrap();
    assert_eq!(app.diagnostics()[0].range.0.line, 6);
}

#[test]
fn an_edit_below_a_diagnostic_leaves_it_alone() {
    // The other half of the same rule, and the one `Shift` on its own gets
    // wrong: it moves every position, because within an edit batch there are
    // none before the edit.
    let (mut app, _rx, _) = app_with_diagnostic("below", &["--push"]);
    for _ in 0..7 {
        step_batch(&mut app, vec![key(KeyCode::Down)], AREA).unwrap();
    }
    step_batch(&mut app, vec![key(KeyCode::Enter)], AREA).unwrap();
    assert_eq!(app.diagnostics()[0].range.0.line, 5);
}

#[test]
fn a_stale_version_is_dropped_when_a_newer_one_has_been_sent() {
    // The server answers a change describing the document as it was when it was
    // opened. Showing it would replace what is on screen with something older.
    let (mut app, rx, _) = app_with_diagnostic("stale", &["--push-stale"]);
    step_batch(
        &mut app,
        vec![AppEvent::Input(Event::Key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::NONE,
        )))],
        AREA,
    )
    .unwrap();
    settle(&mut app, &rx);
    let messages: Vec<&str> = app
        .diagnostics()
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(messages, vec!["fake: on open"], "the stale publish won");
}

#[test]
fn a_save_publishes_over_what_was_there() {
    let (mut app, rx, _) = app_with_diagnostic("save", &["--push"]);
    let ctrl_s = AppEvent::Input(Event::Key(KeyEvent::new(
        KeyCode::Char('s'),
        KeyModifiers::CONTROL,
    )));
    step_batch(&mut app, vec![ctrl_s], AREA).unwrap();
    assert!(pump_until(&mut app, &rx, |a| a.diagnostics().len() == 2));
    let severities: Vec<Severity> = app.diagnostics().iter().map(|d| d.severity).collect();
    assert!(severities.contains(&Severity::Warning), "{severities:?}");
}

#[test]
fn diagnostics_reach_the_panel_through_render_context() {
    // Not through a setter. `RenderContext` carries them, so a tab switch moves
    // them the way it moves both halves of the theme. `App::diagnostics` is the
    // call the frame itself makes, which is what makes this an assertion about
    // what the panel was handed rather than about a parallel accessor.
    let (mut app, _rx, _) = app_with_diagnostic("context", &["--push"]);
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(AREA.width, AREA.height))
            .unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    assert_eq!(app.diagnostics().len(), 1);
}

#[test]
fn a_second_tab_does_not_show_the_first_tabs_diagnostics() {
    let (mut app, rx, _) = app_with_diagnostic("two-tabs", &["--push"]);
    let dir = std::env::temp_dir()
        .join("typ-lsp-diagnostics")
        .join("two-tabs");
    app.open_path(&dir.join("b.rs")).unwrap();
    settle(&mut app, &rx);
    // `b.rs` gets its own publish from the same server; what must never happen
    // is `a.rs`'s set showing under `b.rs` because the store is app-global.
    assert!(
        app.diagnostics()
            .iter()
            .all(|d| d.message == "fake: on open"),
        "{:?}",
        app.diagnostics()
    );
    assert_eq!(app.diagnostics().len(), 1);
}
