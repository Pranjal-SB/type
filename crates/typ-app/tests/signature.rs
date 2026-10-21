//! Signature help: asked on `(` and `,` of a server that offers it, shown above
//! the caret with the active parameter marked, gone on `)` or `esc`. It never
//! takes a key.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
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
const SOURCE: &str =
    "fn one() {}\nfn two() {}\nfn three() {}\nfn four() {}\nfn five() {}\nfn six() {}\n";

const METHOD: &str = "textDocument/signatureHelp";

fn fake() -> &'static str {
    env!("CARGO_BIN_EXE_typ-app-fake-server")
}

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-lsp-signature").join(name);
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
    app.editor_mut().goto(6, 0);
    (app, rx)
}

fn press(app: &mut App, code: KeyCode) {
    let event = AppEvent::Input(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    step_batch(app, vec![event], AREA).unwrap();
}

fn draw(app: &mut App) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

/// The active parameter, once a signature with one is up.
fn active(app: &mut App, rx: &AppReceiver, wanted: &str) -> bool {
    pump_until(app, rx, |a| {
        a.signature_help()
            .is_some_and(|(_, active)| active.as_deref() == Some(wanted))
    })
}

#[test]
fn an_open_paren_shows_the_signature_with_its_first_parameter_active() {
    let (mut app, rx) = ready("open", &[]);
    type_text(&mut app, "f(");
    assert!(
        active(&mut app, &rx, "a: i32"),
        "{:?}",
        app.signature_help()
    );
    let (label, _) = app.signature_help().unwrap();
    assert_eq!(label, "fn f(a: i32, b: &str)");
}

#[test]
fn a_comma_moves_to_the_next_parameter_and_a_close_paren_puts_it_away() {
    let (mut app, rx) = ready("comma", &[]);
    type_text(&mut app, "f(");
    assert!(active(&mut app, &rx, "a: i32"));
    type_text(&mut app, "1,");
    assert!(
        active(&mut app, &rx, "b: &str"),
        "{:?}",
        app.signature_help()
    );
    type_text(&mut app, " x)");
    assert_eq!(app.signature_help(), None, "a close paren left it up");
    settle(&mut app, &rx);
    assert_eq!(app.signature_help(), None, "it came back");
}

#[test]
fn it_floats_above_the_caret_and_never_takes_a_key() {
    let (mut app, rx) = ready("float", &[]);
    type_text(&mut app, "f(");
    assert!(active(&mut app, &rx, "a: i32"));
    let rows = draw(&mut app);
    let caret_row = 7;
    let shown = rows[..caret_row]
        .iter()
        .any(|row| row.contains("fn f(a: i32, b: &str)"));
    assert!(shown, "not above the caret:\n{}", rows.join("\n"));

    // Typed into the buffer, not taken by the float.
    type_text(&mut app, "7");
    assert_eq!(app.editor().line_text(6), "f(7");
    assert!(
        app.signature_help().is_some(),
        "typing an argument closed it"
    );

    press(&mut app, KeyCode::Esc);
    assert_eq!(app.signature_help(), None, "esc left it up");
}

#[test]
fn a_server_without_signature_help_is_never_asked() {
    let (mut app, rx) = ready("unoffered", &["--no-signature"]);
    type_text(&mut app, "f(");
    settle(&mut app, &rx);
    assert_eq!(app.lsp_requests_of(METHOD), 0);
    assert_eq!(app.signature_help(), None);
}
