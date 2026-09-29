//! Task 16's by-hand checklist, as tests.
//!
//! The plan asks a human to drive the release binary and confirm eight things.
//! A human doing that once, on one platform, is worth less than the same eight
//! assertions running on three platforms on every push — and unlike the human,
//! these say which one broke.
//!
//! They deliberately go through `handle_chord` end to end rather than calling
//! panels directly: the question is whether a keypress reaches the behavior,
//! which is the exact thing that was untested for ten tasks of this milestone.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_app::App;
use typ_core::KeyChord;

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-milestone").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("code.rs"), "foo::bar baz\nsecond line\n").unwrap();
    dir
}

fn app(name: &str) -> App {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("code.rs")).unwrap();
    app
}

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    app.handle_chord(KeyChord::from_event(KeyEvent::new(code, mods)))
        .unwrap();
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c), KeyModifiers::NONE);
    }
}

#[test]
fn shift_arrows_select_and_the_cursor_stays_at_the_moving_end() {
    let mut a = app("select");
    press(&mut a, KeyCode::Right, KeyModifiers::SHIFT);
    press(&mut a, KeyCode::Right, KeyModifiers::SHIFT);
    let primary = a.editor_mut().selections().primary();
    assert_eq!(primary.anchor.col, 0);
    assert_eq!(primary.head.col, 2, "the head is the end that moved");
}

#[test]
fn ctrl_arrows_move_by_word_and_stop_at_punctuation() {
    let mut a = app("word");
    // "foo::bar baz" — foo, then ::, then bar. Punctuation is its own run,
    // which is what makes word motion useful in code rather than only in prose.
    press(&mut a, KeyCode::Right, KeyModifiers::CONTROL);
    assert_eq!(a.editor_mut().cursor().col, 3);
    press(&mut a, KeyCode::Right, KeyModifiers::CONTROL);
    assert_eq!(a.editor_mut().cursor().col, 5);
}

#[test]
fn typing_with_several_cursors_edits_every_one_and_undoes_in_a_single_step() {
    let mut a = app("multi");
    press(
        &mut a,
        KeyCode::Down,
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );
    assert_eq!(a.editor_mut().selections().len(), 2);

    typed(&mut a, "xy");
    assert_eq!(a.editor_mut().line_text(0), "xyfoo::bar baz");
    assert_eq!(a.editor_mut().line_text(1), "xysecond line");

    press(&mut a, KeyCode::Char('z'), KeyModifiers::CONTROL);
    assert_eq!(a.editor_mut().line_text(0), "foo::bar baz");
    assert_eq!(
        a.editor_mut().line_text(1),
        "second line",
        "one press, both cursors, both characters"
    );
}

#[test]
fn escape_collapses_several_cursors_back_to_one() {
    let mut a = app("collapse");
    press(
        &mut a,
        KeyCode::Down,
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );
    assert_eq!(a.editor_mut().selections().len(), 2);
    press(&mut a, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(a.editor_mut().selections().len(), 1);
}

#[test]
fn search_walks_the_matches_and_escape_abandons_the_prompt() {
    let mut a = app("search");
    press(&mut a, KeyCode::Char('f'), KeyModifiers::CONTROL);
    typed(&mut a, "line");
    press(&mut a, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(a.editor_mut().cursor().line, 1);

    press(&mut a, KeyCode::Char('f'), KeyModifiers::CONTROL);
    assert!(a.prompt().is_some());
    press(&mut a, KeyCode::Esc, KeyModifiers::NONE);
    assert!(a.prompt().is_none(), "escape abandons the prompt");
}

#[test]
fn replace_rewrites_every_match_and_undoes_in_one_step() {
    let mut a = app("replace");
    press(&mut a, KeyCode::Char('h'), KeyModifiers::CONTROL);
    typed(&mut a, "line");
    press(&mut a, KeyCode::Enter, KeyModifiers::NONE);
    typed(&mut a, "LINE");
    press(&mut a, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(a.editor_mut().line_text(1), "second LINE");

    press(&mut a, KeyCode::Char('z'), KeyModifiers::CONTROL);
    assert_eq!(a.editor_mut().line_text(1), "second line");
}

#[test]
fn quit_still_guards_unsaved_work() {
    let mut a = app("quit");
    typed(&mut a, "x");
    press(&mut a, KeyCode::Char('q'), KeyModifiers::CONTROL);
    assert!(!a.should_quit(), "the first quit must not discard edits");
    press(&mut a, KeyCode::Char('q'), KeyModifiers::CONTROL);
    assert!(a.should_quit());
}

// `every_default_binding_resolves_to_something_that_handles_it` lived here and
// checked app-owned actions against a hand-written list rather than running
// them. `app_actions.rs` runs each one instead. Gap 127.
