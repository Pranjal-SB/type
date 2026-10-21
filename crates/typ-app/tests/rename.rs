//! Rename: `f2` asks `prepareRename` when the server offers it, seeds the
//! status bar prompt with the symbol, and applies the server's answer through
//! the one `WorkspaceEdit` path: every file it names edited in its buffer,
//! none saved.

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

const A: &str = "fn foo() {}\nfoo();\n";
const B: &str = "use foo;\n";
const PREPARE: &str = "textDocument/prepareRename";
const RENAME: &str = "textDocument/rename";

fn fake() -> &'static str {
    env!("CARGO_BIN_EXE_typ-app-fake-server")
}

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-rename").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), A).unwrap();
    std::fs::write(dir.join("b.rs"), B).unwrap();
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

fn press(app: &mut App, code: KeyCode) {
    let event = AppEvent::Input(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    step_batch(app, vec![event], AREA).unwrap();
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

/// `a.rs` open with its server up and the caret on the `foo` of its second
/// line; `b.rs` beside it, not open.
fn ready(name: &str, flags: &[&str]) -> (App, AppReceiver, PathBuf) {
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
    app.editor_mut().goto(1, 1);
    (app, rx, dir)
}

fn prompt_input(app: &App) -> Option<String> {
    app.prompt().map(|p| p.input().to_string())
}

#[test]
fn renaming_foo_to_bar_edits_both_files_in_their_buffers_and_saves_neither() {
    let (mut app, rx, dir) = ready("two-files", &[]);
    press(&mut app, KeyCode::F(2));
    assert!(
        pump_until(&mut app, &rx, |a| prompt_input(a).is_some()),
        "no prompt: {:?}",
        app.status()
    );
    assert_eq!(prompt_input(&app).as_deref(), Some("foo"));
    assert_eq!(app.lsp_requests_of(PREPARE), 1);
    for _ in 0..3 {
        press(&mut app, KeyCode::Backspace);
    }
    for c in "bar".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    press(&mut app, KeyCode::Enter);
    assert!(app.prompt().is_none(), "the prompt stayed up");
    assert!(
        pump_until(&mut app, &rx, |a| a.tab_count() == 2),
        "b.rs was never opened: {:?}",
        app.status()
    );
    assert_eq!(app.editor().line_text(0), "fn bar() {}");
    assert_eq!(app.editor().line_text(1), "bar();");
    assert_eq!(app.tab(1).line_text(0), "use bar;");
    assert!(app.editor().is_dirty() && app.tab(1).is_dirty());
    assert_eq!(std::fs::read_to_string(dir.join("a.rs")).unwrap(), A);
    assert_eq!(std::fs::read_to_string(dir.join("b.rs")).unwrap(), B);
}

#[test]
fn a_refused_prepare_leaves_no_prompt_and_says_why() {
    let (mut app, rx, _) = ready("refused", &["--refuse-rename"]);
    press(&mut app, KeyCode::F(2));
    assert!(pump_until(&mut app, &rx, |a| a.status().is_some()));
    assert!(app.prompt().is_none(), "a prompt for a refused rename");
    let said = app.status().unwrap().to_string();
    assert!(said.contains("cannot be renamed"), "{said}");
}

#[test]
fn esc_in_the_prompt_changes_nothing() {
    let (mut app, rx, _) = ready("esc", &[]);
    press(&mut app, KeyCode::F(2));
    assert!(pump_until(&mut app, &rx, |a| prompt_input(a).is_some()));
    press(&mut app, KeyCode::Char('x'));
    press(&mut app, KeyCode::Esc);
    assert!(app.prompt().is_none());
    settle(&mut app, &rx);
    assert_eq!(app.lsp_requests_of(RENAME), 0);
    assert_eq!(app.editor().line_text(1), "foo();");
    assert_eq!(app.tab_count(), 1);
}

#[test]
fn without_prepare_the_prompt_is_seeded_with_the_word_at_the_caret() {
    let (mut app, _rx, _) = ready("no-prepare", &["--no-prepare-rename"]);
    press(&mut app, KeyCode::F(2));
    assert_eq!(prompt_input(&app).as_deref(), Some("foo"));
    assert_eq!(app.lsp_requests_of(PREPARE), 0);
}
