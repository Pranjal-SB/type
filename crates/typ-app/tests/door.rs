//! `ctrl+k` is a door: a prefix whose second key picks from everything bound
//! under it. `controls.md` §2, `interface.md` §6.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_app::{App, Focus};
use typ_core::KeyChord;
use typ_picker::Mode;

fn app(name: &str) -> App {
    let dir: PathBuf = std::env::temp_dir().join("typ-door").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.rs"), "fn main() {}\n").unwrap();
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("main.rs")).unwrap();
    app
}

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    app.handle_chord(KeyChord::from_event(KeyEvent::new(code, mods)))
        .unwrap();
}

fn ctrl_k(app: &mut App) {
    press(app, KeyCode::Char('k'), KeyModifiers::CONTROL);
}

fn key(app: &mut App, c: char) {
    press(app, KeyCode::Char(c), KeyModifiers::NONE);
}

#[test]
fn ctrl_k_then_f_opens_project_search() {
    let mut app = app("f");
    ctrl_k(&mut app);
    assert_eq!(app.pending_prefix(), Some("ctrl+k"));

    key(&mut app, 'f');

    assert_eq!(app.picker().map(|p| p.mode()), Some(Mode::Search));
    assert_eq!(app.pending_prefix(), None);
    assert_eq!(app.editor().line_text(0), "fn main() {}", "f was typed");
}

#[test]
fn ctrl_k_then_an_unbound_key_cancels_and_says_so() {
    let mut app = app("q");
    ctrl_k(&mut app);

    key(&mut app, 'q');

    assert!(!app.should_quit(), "q under the prefix quit");
    assert_eq!(app.pending_prefix(), None);
    assert_eq!(app.status(), Some("ctrl+k q is not bound"));
    assert_eq!(app.editor().line_text(0), "fn main() {}", "q was typed");
}

#[test]
fn ctrl_k_then_esc_leaves_nothing_pending() {
    let mut app = app("esc");
    ctrl_k(&mut app);

    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!(app.pending_prefix(), None);
    assert_eq!(app.status(), None, "esc cancels silently");
    // The next key is text again.
    key(&mut app, 'f');
    assert_eq!(app.editor().line_text(0), "ffn main() {}");
}

#[test]
fn the_global_layer_answers_from_any_panel() {
    // Lookup order is the focused panel's rows, then global (`controls.md` §3).
    // No panel has rows of its own yet, so from the tree the global row is
    // what answers.
    let mut app = app("layer");
    app.cycle_focus();
    assert_eq!(app.focus(), Focus::Tree);

    ctrl_k(&mut app);
    key(&mut app, 'f');

    assert_eq!(app.picker().map(|p| p.mode()), Some(Mode::Search));
}

#[test]
fn a_paste_abandons_a_pending_prefix() {
    let mut app = app("paste");
    ctrl_k(&mut app);

    app.handle_paste("x".into()).unwrap();

    assert_eq!(app.pending_prefix(), None);
}
