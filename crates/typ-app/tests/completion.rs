//! Completion: asked off the render thread, once per batch, and only of a
//! server that offers it. A reply describing an older document is dropped.
//! The list filters as the word grows, its top item is ghost text after the
//! caret, and accepting it is one edit and one undo.

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

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    let event = AppEvent::Input(Event::Key(KeyEvent::new(code, mods)));
    step_batch(app, vec![event], AREA).unwrap();
}

fn draw(app: &mut App) -> Vec<String> {
    draw_at(app, AREA.width, AREA.height)
}

fn draw_at(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
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

/// Type `text` and wait for a list that has something in it.
fn listed(app: &mut App, rx: &AppReceiver, text: &str) -> Vec<String> {
    type_text(app, text);
    assert!(
        pump_until(app, rx, |a| a.completion_labels().is_some()),
        "no list arrived for {text:?}"
    );
    app.completion_labels().unwrap()
}

#[test]
fn tab_accepts_the_top_item_as_one_undo() {
    let (mut app, rx) = ready("accept", &[]);
    let top = listed(&mut app, &rx, "fo")[0].clone();
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(app.editor().line_text(2), top);
    assert_eq!(app.completion_labels(), None, "the list stayed up");

    app.apply_named_action(typ_core::Action::Undo).unwrap();
    assert_eq!(
        app.editor().line_text(2),
        "fo",
        "accepting was not one undo"
    );
}

#[test]
fn enter_accepts_too_and_types_no_newline() {
    let (mut app, rx) = ready("enter", &[]);
    let top = listed(&mut app, &rx, "fo")[0].clone();
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.editor().line_text(2), top);
    assert_eq!(app.editor().line_count(), 3);
}

#[test]
fn ghost_text_is_the_rest_of_the_top_item_and_goes_with_the_list() {
    let (mut app, rx) = ready("ghost", &[]);
    let top = listed(&mut app, &rx, "fo")[0].clone();
    let rest = top["fo".len()..].to_string();
    assert_eq!(app.ghost_text().as_deref(), Some(rest.as_str()));
    let rows = draw(&mut app);
    assert!(
        rows.iter().any(|row| row.contains(&format!("fo{rest}"))),
        "the ghost is not after the caret:\n{}",
        rows.join("\n")
    );

    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(app.completion_labels(), None);
    assert_eq!(app.ghost_text(), None);
    assert_eq!(app.editor().line_text(2), "fo", "esc changed the text");
}

#[test]
fn arrows_move_the_choice_and_the_ghost_follows() {
    let (mut app, rx) = ready("arrows", &[]);
    let labels = listed(&mut app, &rx, "fo");
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(
        app.ghost_text().as_deref(),
        Some(&labels[1]["fo".len()..]),
        "down did not move the choice"
    );
    assert_eq!(app.editor().cursor().line, 2, "down moved the caret");
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(app.editor().line_text(2), labels[1]);
}

#[test]
fn a_character_that_matches_nothing_closes_the_list() {
    let (mut app, rx) = ready("nothing", &[]);
    listed(&mut app, &rx, "fo");
    settle(&mut app, &rx);
    let asked = app.lsp_requests_of(METHOD);
    type_text(&mut app, "z");
    assert_eq!(app.completion_labels(), None);
    settle(&mut app, &rx);
    assert_eq!(app.completion_labels(), None, "it came back");
    assert_eq!(
        app.lsp_requests_of(METHOD),
        asked,
        "a complete list was asked again"
    );
}

#[test]
fn typing_more_of_the_word_filters_without_asking_again() {
    let (mut app, rx) = ready("filter", &[]);
    listed(&mut app, &rx, "fo");
    settle(&mut app, &rx);
    let asked = app.lsp_requests_of(METHOD);
    type_text(&mut app, "r");
    assert_eq!(app.completion_labels(), Some(vec!["format".to_string()]));
    settle(&mut app, &rx);
    assert_eq!(app.lsp_requests_of(METHOD), asked);
}

#[test]
fn a_click_on_the_second_item_accepts_it() {
    let (mut app, rx) = ready("click", &[]);
    let labels = listed(&mut app, &rx, "fo");
    draw(&mut app);
    let row = app
        .targets(AREA)
        .into_iter()
        .find(|t| t.kind == TargetKind::CompletionRow(1))
        .expect("the second row is not a target");
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: row.point.0,
        row: row.point.1,
        modifiers: KeyModifiers::NONE,
    };
    step_batch(&mut app, vec![AppEvent::Input(Event::Mouse(event))], AREA).unwrap();
    assert_eq!(app.editor().line_text(2), labels[1]);
    assert_eq!(app.completion_labels(), None);
}

#[test]
fn a_server_edit_is_placed_by_its_own_encoding() {
    // The fake counts in UTF-8 bytes, so the word after `語 ` starts at byte
    // 4 and char 2. Read as chars, the edit would land after the word rather
    // than over it.
    let (mut app, rx) = ready("cjk", &["--complete-edit"]);
    type_text(&mut app, "語 ");
    let labels = listed(&mut app, &rx, "日本");
    assert_eq!(labels, ["日本語"]);
    assert_eq!(app.ghost_text().as_deref(), Some("語"));
    press(&mut app, KeyCode::Tab, KeyModifiers::NONE);
    assert_eq!(app.editor().line_text(2), "語 日本語");
}

#[test]
fn a_docs_column_joins_the_list_at_a_hundred_columns_and_the_bar_says_its_keys() {
    let (mut app, rx) = ready("docs", &[]);
    listed(&mut app, &rx, "fo");
    let narrow = draw(&mut app).join("\n");
    assert!(narrow.contains("completion"), "{narrow}");
    assert!(
        !narrow.contains("Does foo."),
        "a docs column under 100 columns"
    );
    assert!(
        narrow.contains("tab accepts"),
        "the bar does not say the keys"
    );

    let wide = draw_at(&mut app, 100, 24).join("\n");
    assert!(wide.contains("Does foo."), "{wide}");
}

#[test]
fn a_press_off_the_list_puts_it_away_and_still_reaches_the_text() {
    let (mut app, rx) = ready("press-off", &[]);
    listed(&mut app, &rx, "fo");
    draw(&mut app);
    let caret = app.editor().cursor();
    // The first line's text, well clear of the list two rows below the caret.
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 10,
        row: 1,
        modifiers: KeyModifiers::NONE,
    };
    step_batch(&mut app, vec![AppEvent::Input(Event::Mouse(event))], AREA).unwrap();
    assert_eq!(app.completion_labels(), None, "the list stayed up");
    assert_ne!(
        app.editor().cursor(),
        caret,
        "the press did not reach the text"
    );
    assert_eq!(app.editor().line_text(2), "fo");
}
