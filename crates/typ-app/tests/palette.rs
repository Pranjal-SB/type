//! The command palette: every named action, reachable by name.
//!
//! **The `>` prefix is the primary path, not a convenience.** `controls.md` §1
//! puts `Ctrl+Shift+letter` in the Enhanced tier — it needs the kitty keyboard
//! protocol — and says so about this exact chord: Ctrl+Shift+P cannot be a
//! default in a terminal. A prefix typed into an overlay that is already open
//! needs no chord to survive. VS Code reaches the same design from the other
//! direction: `>` for commands, `@` for symbols, `:` for a line, `#` for
//! workspace symbols, all inside the one Ctrl+P box.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_app::App;
use typ_core::{Action, KeyChord, Keymap};
use typ_picker::Mode;

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-palette").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "fn a() {}\n").unwrap();
    dir
}

fn app(name: &str) -> (App, PathBuf) {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("a.rs")).unwrap();
    (app, dir)
}

fn key(code: KeyCode) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, KeyModifiers::NONE))
}

fn ch(c: char) -> KeyChord {
    key(KeyCode::Char(c))
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        app.handle_chord(ch(c)).unwrap();
    }
}

fn mode(app: &App) -> Mode {
    app.picker().expect("the overlay is up").mode()
}

#[test]
fn the_palette_chord_is_bound() {
    // Ships as a documented Enhanced-tier exception, beside the clipboard
    // chords already in the table. The `>` prefix is what works everywhere.
    let keymap = Keymap::default_bindings();
    let chord = KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('P'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    ));

    assert_eq!(keymap.lookup(&chord), Some(Action::OpenCommandPalette));
}

#[test]
fn a_leading_angle_bracket_turns_the_finder_into_the_palette() {
    let (mut app, _dir) = app("prefix");
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    assert_eq!(mode(&app), Mode::Files);

    typed(&mut app, ">");

    assert_eq!(mode(&app), Mode::Commands);
}

#[test]
fn deleting_the_angle_bracket_turns_it_back_into_the_finder() {
    let (mut app, _dir) = app("un-prefix");
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    typed(&mut app, ">quit");
    assert_eq!(mode(&app), Mode::Commands);

    for _ in 0..5 {
        app.handle_chord(key(KeyCode::Backspace)).unwrap();
    }

    assert_eq!(mode(&app), Mode::Files);
    assert_eq!(app.picker().unwrap().query(), "");
}

#[test]
fn an_angle_bracket_that_is_not_first_is_an_ordinary_character() {
    // `a>b` is a filename fragment. Only the first column is a mode switch.
    let (mut app, _dir) = app("mid-query");
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();

    typed(&mut app, "a>b");

    assert_eq!(mode(&app), Mode::Files);
    assert_eq!(app.picker().unwrap().query(), "a>b");
}

#[test]
fn the_chord_opens_the_palette_with_the_prefix_already_typed() {
    // The chord is a shortcut for typing `>`, and is literally implemented as
    // one — so there is a single mode-switch path rather than two that can
    // disagree.
    let (mut app, _dir) = app("chord-opens");

    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('P'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )))
    .unwrap();

    assert_eq!(mode(&app), Mode::Commands);
    assert_eq!(app.picker().unwrap().query(), ">");
}

#[test]
fn the_palette_lists_actions_and_the_query_filters_them() {
    let (mut app, _dir) = app("filter");
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('P'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )))
    .unwrap();
    let all = app.picker().unwrap().commands().len();
    assert!(
        all > 20,
        "an empty query should list every action, got {all}"
    );

    typed(&mut app, "focusnext");

    let rows = app.picker().unwrap().commands();
    assert_eq!(rows.first().map(|r| r.name.as_str()), Some("focus_next"));
    assert!(rows.len() < all, "the query did not narrow the list");
}

#[test]
fn enter_runs_the_selected_action() {
    // Proved with an action that has a visible effect and no side effect worth
    // cleaning up. If this passes, invariant 2 has paid off: every editing
    // primitive being an `Action` is what makes them all reachable here.
    let (mut app, _dir) = app("run");
    let before = app.focus();

    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('P'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )))
    .unwrap();
    typed(&mut app, "focusnext");
    app.handle_chord(key(KeyCode::Enter)).unwrap();

    assert_ne!(app.focus(), before, "the action did not run");
    assert!(
        app.picker().is_none(),
        "the overlay stayed up over its result"
    );
}

#[test]
fn an_action_the_panel_does_not_handle_still_reaches_the_app() {
    // Two dispatch steps, the same two a keypress takes: the focused panel
    // first, then the app. A palette that only tried the panel would silently
    // do nothing for every app-owned action.
    let (mut app, _dir) = app("app-owned");
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('P'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )))
    .unwrap();
    typed(&mut app, "gotoline");
    app.handle_chord(key(KeyCode::Enter)).unwrap();

    assert!(
        app.prompt().is_some(),
        "goto_line is the app's, and it never got there"
    );
}

#[test]
fn the_palette_shows_what_key_runs_each_command() {
    // Helix ships this and VS Code ships this, for the same reason: a palette
    // that only executes is a menu, and one that also teaches the binding stops
    // being needed for that command.
    let (mut app, _dir) = app("bindings");
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('P'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )))
    .unwrap();
    typed(&mut app, "quit");

    let rows = app.picker().unwrap().commands();
    let quit = rows
        .iter()
        .find(|r| r.name == "quit")
        .expect("quit is an action");
    assert_eq!(quit.binding, "ctrl+q");
}

#[test]
fn an_unbound_action_shows_no_binding_rather_than_a_wrong_one() {
    let (mut app, _dir) = app("unbound");
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('P'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )))
    .unwrap();

    let rows = app.picker().unwrap().commands();
    let unbound: Vec<&str> = rows
        .iter()
        .filter(|r| r.binding.is_empty())
        .map(|r| r.name.as_str())
        .collect();
    // Not an assertion that some action is unbound — that changes as the keymap
    // grows. The assertion is that an empty binding is the representation, so
    // nothing has to invent a placeholder that looks like a key.
    for name in unbound {
        assert!(
            Keymap::default_bindings()
                .bindings_for(Action::from_name(name).expect("a listed name"))
                .is_empty()
        );
    }
}

#[test]
fn backspacing_out_of_a_chord_opened_palette_asks_for_the_walk() {
    // The palette needs no corpus, so opening it by chord requests no walk.
    // Deleting the `>` turns it into the file picker, which does — and without
    // this the list would rank against a corpus nothing ever filled.
    let (mut app, _dir) = app("backspace-to-files");
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('P'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )))
    .unwrap();
    assert!(
        !app.index_requested(),
        "the palette walked the project for a list of sixty static names"
    );

    app.handle_chord(key(KeyCode::Backspace)).unwrap();

    assert_eq!(mode(&app), Mode::Files);
    assert!(
        app.index_requested(),
        "the file picker has no corpus to rank"
    );
}

#[test]
fn the_palette_does_not_offer_to_open_itself() {
    // A row that reopens the overlay you are already in is a no-op nobody can
    // explain.
    let (mut app, _dir) = app("no-recursion");
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('P'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )))
    .unwrap();

    let rows = app.picker().unwrap().commands();
    assert!(
        !rows.iter().any(|r| r.name == "open_command_palette"),
        "the palette listed itself"
    );
}

#[test]
fn escape_closes_the_palette_without_running_anything() {
    let (mut app, _dir) = app("escape");
    let before = app.focus();
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('P'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )))
    .unwrap();
    typed(&mut app, "focusnext");

    app.handle_chord(key(KeyCode::Esc)).unwrap();

    assert!(app.picker().is_none());
    assert_eq!(app.focus(), before);
}

#[test]
fn enter_on_an_empty_result_list_runs_nothing() {
    let (mut app, _dir) = app("no-match");
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('P'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    )))
    .unwrap();
    typed(&mut app, "zzzznotacommandzzzz");
    assert!(app.picker().unwrap().commands().is_empty());

    app.handle_chord(key(KeyCode::Enter)).unwrap();

    assert!(
        app.picker().is_some(),
        "Enter on nothing closed the overlay anyway"
    );
}
