use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_app::App;
use typ_app::prompt::PromptKind;
use typ_core::KeyChord;

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-goto-line").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let text: String = (1..=50).map(|i| format!("line {i}\n")).collect();
    std::fs::write(dir.join("long.txt"), text).unwrap();
    dir
}

fn chord(code: KeyCode, mods: KeyModifiers) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, mods))
}

fn app(name: &str) -> App {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("long.txt")).unwrap();
    app
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        app.handle_chord(chord(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
}

fn open_goto(app: &mut App) {
    app.handle_chord(chord(KeyCode::Char('g'), KeyModifiers::CONTROL))
        .unwrap();
}

fn enter(app: &mut App) {
    app.handle_chord(chord(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
}

#[test]
fn ctrl_g_opens_a_goto_prompt() {
    let mut app = app("open");
    open_goto(&mut app);
    let prompt = app.prompt().expect("a prompt");
    assert_eq!(prompt.kind(), PromptKind::GotoLine);
    assert_eq!(prompt.label(), "Go to line:");
}

#[test]
fn a_line_number_jumps_there_counted_from_one() {
    let mut app = app("jump");
    open_goto(&mut app);
    typed(&mut app, "20");
    enter(&mut app);

    assert!(app.prompt().is_none(), "the prompt closes on a good answer");
    // Line 20 as a user counts it is index 19.
    assert_eq!(app.editor_mut().cursor().line, 19);
    assert_eq!(app.editor_mut().cursor().col, 0);
}

#[test]
fn typing_in_the_prompt_does_not_reach_the_buffer() {
    let mut app = app("capture");
    open_goto(&mut app);
    typed(&mut app, "12");
    assert_eq!(app.editor_mut().line_text(0), "line 1");
    assert_eq!(app.prompt().unwrap().input(), "12");
}

#[test]
fn a_number_past_the_end_clamps_to_the_last_line() {
    let mut app = app("clamp");
    open_goto(&mut app);
    typed(&mut app, "9999");
    enter(&mut app);

    // They meant "the end". Erroring at someone who asked to go to the bottom
    // of the file is pedantry, not correctness.
    let last = app.editor_mut().line_count() - 1;
    assert_eq!(app.editor_mut().cursor().line, last);
}

#[test]
fn line_zero_is_treated_as_line_one() {
    let mut app = app("zero");
    open_goto(&mut app);
    typed(&mut app, "0");
    enter(&mut app);
    assert_eq!(app.editor_mut().cursor().line, 0);
}

#[test]
fn non_numeric_input_is_rejected_without_closing_the_prompt() {
    let mut app = app("garbage");
    open_goto(&mut app);
    typed(&mut app, "abc");
    enter(&mut app);

    // Closing on a typo would throw the input away and make the user reopen it.
    assert!(app.prompt().is_some(), "the prompt stays open");
    assert_eq!(app.editor_mut().cursor().line, 0, "and nothing moved");
    assert!(
        app.status().unwrap_or_default().contains("line number"),
        "status: {:?}",
        app.status()
    );
}

#[test]
fn the_rejected_input_is_left_in_place_to_be_corrected() {
    let mut app = app("keep-input");
    open_goto(&mut app);
    typed(&mut app, "2x");
    enter(&mut app);
    assert_eq!(
        app.prompt().unwrap().input(),
        "2x",
        "retyping from scratch after one bad character is the annoying version"
    );
}

#[test]
fn escape_abandons_the_prompt_without_moving() {
    let mut app = app("escape");
    open_goto(&mut app);
    typed(&mut app, "30");
    app.handle_chord(chord(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt().is_none());
    assert_eq!(app.editor_mut().cursor().line, 0);
}

#[test]
fn an_empty_answer_closes_the_prompt_and_does_nothing() {
    let mut app = app("empty");
    open_goto(&mut app);
    enter(&mut app);
    assert!(app.prompt().is_none());
    assert_eq!(app.editor_mut().cursor().line, 0);
}

#[test]
fn the_target_line_is_centred_rather_than_left_at_the_edge() {
    let mut app = app("centre");
    // Give the editor a height by drawing once.
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(60, 12)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();

    open_goto(&mut app);
    typed(&mut app, "40");
    enter(&mut app);
    terminal.draw(|frame| app.render(frame)).unwrap();

    let top = app.editor_mut().top_line();
    let cursor = app.editor_mut().cursor().line;
    // Landing on the last visible row is technically "scrolled into view" and
    // useless: you jumped there to read around it.
    assert!(
        top < cursor,
        "line {cursor} sat at the top of the viewport (top {top})"
    );
    assert!(
        cursor - top >= 2,
        "line {cursor} is jammed against the top edge (top {top})"
    );
}
