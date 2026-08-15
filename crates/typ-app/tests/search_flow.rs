use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_app::App;
use typ_core::KeyChord;

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-search-flow").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hits.txt"), "alpha\nbeta alpha\ngamma\n").unwrap();
    dir
}

fn chord(code: KeyCode, mods: KeyModifiers) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, mods))
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        app.handle_chord(chord(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }
}

fn app_with_hits(name: &str) -> App {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("hits.txt")).unwrap();
    app
}

#[test]
fn ctrl_f_opens_a_search_prompt() {
    let mut app = app_with_hits("open");
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.prompt().is_some());
}

#[test]
fn typing_in_the_prompt_does_not_reach_the_buffer() {
    let mut app = app_with_hits("capture");
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    typed(&mut app, "alpha");
    assert_eq!(
        app.editor_mut().line_text(0),
        "alpha",
        "the file is unchanged"
    );
    assert_eq!(app.prompt().unwrap().input(), "alpha");
}

#[test]
fn enter_jumps_to_the_first_match_after_the_cursor() {
    let mut app = app_with_hits("jump");
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    typed(&mut app, "alpha");
    app.handle_chord(chord(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt().is_none(), "the prompt closes on Enter");
    assert_eq!(app.editor_mut().cursor().line, 0);
    assert!(
        !app.editor_mut().selections().primary().is_empty(),
        "the match is selected"
    );
}

#[test]
fn search_next_walks_through_the_matches_and_wraps() {
    let mut app = app_with_hits("walk");
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    typed(&mut app, "alpha");
    app.handle_chord(chord(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    app.handle_chord(chord(KeyCode::F(3), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.editor_mut().cursor().line, 1);
    app.handle_chord(chord(KeyCode::F(3), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.editor_mut().cursor().line, 0, "wraps to the top");
}

#[test]
fn a_search_with_no_matches_says_so_and_moves_nothing() {
    let mut app = app_with_hits("miss");
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    typed(&mut app, "zeta");
    app.handle_chord(chord(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.editor_mut().cursor().line, 0);
    assert!(
        app.status().unwrap().contains("No matches"),
        "status: {:?}",
        app.status()
    );
}

#[test]
fn escape_abandons_the_prompt_without_moving_the_cursor() {
    let mut app = app_with_hits("escape");
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    typed(&mut app, "gamma");
    app.handle_chord(chord(KeyCode::Esc, KeyModifiers::NONE))
        .unwrap();
    assert!(app.prompt().is_none());
    assert_eq!(app.editor_mut().cursor().line, 0);
}

#[test]
fn replace_swaps_every_match_in_one_undo_step() {
    let mut app = app_with_hits("replace");
    app.handle_chord(chord(KeyCode::Char('h'), KeyModifiers::CONTROL))
        .unwrap();
    typed(&mut app, "alpha");
    app.handle_chord(chord(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    typed(&mut app, "ALPHA");
    app.handle_chord(chord(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();

    assert_eq!(app.editor_mut().line_text(0), "ALPHA");
    assert_eq!(app.editor_mut().line_text(1), "beta ALPHA");

    app.handle_chord(chord(KeyCode::Char('z'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.editor_mut().line_text(0), "alpha");
    assert_eq!(
        app.editor_mut().line_text(1),
        "beta alpha",
        "one undo, both lines"
    );
}

#[test]
fn the_status_bar_shows_the_prompt_while_it_is_open() {
    let mut app = app_with_hits("status");
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    typed(&mut app, "al");
    assert_eq!(app.status_left(), "Search: al");
}

// Not in the plan. Both are cases the plan's dispatcher gets wrong.

#[test]
fn a_chord_typed_into_the_prompt_is_not_treated_as_text() {
    let mut app = app_with_hits("prompt-chord");
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    // Ctrl+F again while the prompt is open must not type an "f" into it.
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.prompt().unwrap().input(), "");
}

#[test]
fn search_next_before_any_search_says_so() {
    let mut app = app_with_hits("no-query");
    app.handle_chord(chord(KeyCode::F(3), KeyModifiers::NONE))
        .unwrap();
    assert!(
        app.status().unwrap().contains("Nothing to search for"),
        "status: {:?}",
        app.status()
    );
}
