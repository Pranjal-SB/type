//! Code actions: asked when the caret settles, only of a server that offers
//! them; `✦` in the gutter and after the line where some exist; `ctrl+.`, a
//! click on either `✦` or the context menu lists them, and choosing one
//! applies its edit as one undo or runs its command on the server.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::app::targets::TargetKind;
use typ_app::lsp::ServerConfig;
use typ_app::run::{AppReceiver, channel, step_batch};
use typ_core::{Action, AppEvent};

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

const WAIT: Duration = Duration::from_secs(10);

/// The fake offers its fixes on the third line, index 2.
const SOURCE: &str = "fn one() {}\nfn two() {}\nbad();\nfn four() {}\n";
const FIXED: &str = "fix();";
const RAN: &str = "ran();";
const QUICK_FIX: &str = "Replace with fix";
const COMMAND: &str = "Run the fake fix";
const METHOD: &str = "textDocument/codeAction";
const MARK: &str = "\u{2726}";

fn fake() -> &'static str {
    env!("CARGO_BIN_EXE_typ-app-fake-server")
}

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-code-actions").join(name);
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

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    let event = AppEvent::Input(Event::Key(KeyEvent::new(code, mods)));
    step_batch(app, vec![event], AREA).unwrap();
}

fn click(app: &mut App, (column, row): (u16, u16)) {
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    step_batch(app, vec![AppEvent::Input(Event::Mouse(event))], AREA).unwrap();
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

/// An app with `a.rs` open and its server initialized, the caret moved to
/// `line` in its own batch so the end of that batch asks.
fn ready(name: &str, flags: &[&str], line: usize) -> (App, AppReceiver) {
    let dir = fixture(name);
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_preset(typ_app::dock::Preset::Focus);
    app.add_language_server(server(flags));
    app.set_event_sender(tx);
    app.open_path(&dir.join("a.rs")).unwrap();
    assert!(
        pump_until(&mut app, &rx, |a| a
            .lsp_notifications_of("textDocument/didOpen")
            == 1),
        "the server never initialized"
    );
    app.editor_mut().goto(line, 0);
    step_batch(&mut app, Vec::new(), AREA).unwrap();
    (app, rx)
}

fn offered(name: &str) -> (App, AppReceiver) {
    let (mut app, rx) = ready(name, &[], 2);
    assert!(
        pump_until(&mut app, &rx, |a| a.code_action_titles().is_some()),
        "no code actions arrived"
    );
    (app, rx)
}

#[test]
fn a_fix_on_the_third_line_shows_its_mark_in_the_gutter_and_after_the_line() {
    let (mut app, _rx) = offered("mark");
    let titles = app.code_action_titles().unwrap();
    assert_eq!(titles, [QUICK_FIX, COMMAND]);
    let rows = draw(&mut app);
    // Line index 2 is the third row under the tab row.
    let row = &rows[3];
    assert_eq!(row.matches(MARK).count(), 2, "{}", rows.join("\n"));
    let (gutter, text) = row.split_once("bad();").expect("the line is not there");
    assert!(gutter.contains(MARK), "no mark in the gutter: {row:?}");
    assert!(text.contains(MARK), "no mark after the line: {row:?}");
    assert!(
        rows.iter()
            .enumerate()
            .all(|(y, r)| y == 3 || !r.contains(MARK)),
        "a mark on another line"
    );
}

#[test]
fn ctrl_dot_then_enter_applies_the_fix_as_one_undo() {
    let (mut app, _rx) = offered("apply");
    press(&mut app, KeyCode::Char('.'), KeyModifiers::CONTROL);
    assert_eq!(
        app.context_fixes(),
        Some(vec![QUICK_FIX.to_string(), COMMAND.to_string()])
    );
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.context_fixes(), None, "the list stayed up");
    assert_eq!(app.editor().line_text(2), FIXED);
    app.apply_named_action(Action::Undo).unwrap();
    assert_eq!(app.editor().line_text(2), "bad();", "not one undo");
}

#[test]
fn a_click_on_either_mark_opens_the_same_list() {
    let (mut app, _rx) = offered("click");
    draw(&mut app);
    let marks: Vec<(u16, u16)> = app
        .targets(AREA)
        .into_iter()
        .filter(|t| t.kind == TargetKind::CodeActionMark)
        .map(|t| t.point)
        .collect();
    assert_eq!(marks.len(), 2, "both marks are targets");
    for mark in marks {
        click(&mut app, mark);
        assert_eq!(
            app.context_fixes(),
            Some(vec![QUICK_FIX.to_string(), COMMAND.to_string()]),
            "a click on {mark:?} did not open the list"
        );
        press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(app.editor().line_text(2), "bad();");
    }
}

#[test]
fn a_command_runs_on_the_server_and_the_edit_it_sends_back_lands() {
    let (mut app, rx) = offered("command");
    press(&mut app, KeyCode::Char('.'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(
        pump_until(&mut app, &rx, |a| a.editor().line_text(2) == RAN),
        "the command's edit never landed: {:?}",
        app.editor().line_text(2)
    );
    assert_eq!(app.lsp_requests_of("workspace/executeCommand"), 1);
}

#[test]
fn the_context_menu_carries_the_same_rows() {
    let (mut app, _rx) = offered("context");
    press(&mut app, KeyCode::F(10), KeyModifiers::SHIFT);
    let fixes = app.context_fixes().expect("no context menu");
    assert_eq!(fixes, [QUICK_FIX, COMMAND]);
    assert!(
        app.context_rows().is_some_and(|rows| !rows.is_empty()),
        "the menu lost its actions"
    );
}

#[test]
fn a_line_without_fixes_shows_no_mark() {
    let (mut app, rx) = ready("elsewhere", &[], 0);
    assert!(pump_until(&mut app, &rx, |a| a.lsp_requests_of(METHOD) >= 1));
    settle(&mut app, &rx);
    assert_eq!(app.code_action_titles(), None);
    assert!(!draw(&mut app).join("\n").contains(MARK));
}

#[test]
fn a_server_without_code_actions_is_never_asked_and_shows_no_mark() {
    let (mut app, rx) = ready("unoffered", &["--no-code-action"], 2);
    settle(&mut app, &rx);
    assert_eq!(app.lsp_requests_of(METHOD), 0);
    assert_eq!(app.code_action_titles(), None);
    assert!(!draw(&mut app).join("\n").contains(MARK));
    press(&mut app, KeyCode::Char('.'), KeyModifiers::CONTROL);
    assert_eq!(app.context_fixes(), None);
    assert!(app.status().is_some(), "ctrl+. said nothing");
}
