//! A focus stack, not a cycle: `esc` in a panel with nothing of its own to
//! cancel goes back to where you came from (interface.md §4).

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use typ_app::{App, Focus};
use typ_core::{AppEvent, KeyChord};

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

fn app(name: &str) -> App {
    let dir: PathBuf = std::env::temp_dir().join("typ-focus-stack").join(name);
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

fn esc(app: &mut App) {
    press(app, KeyCode::Esc, KeyModifiers::NONE);
}

fn select_all(app: &mut App) {
    press(app, KeyCode::Char('a'), KeyModifiers::CONTROL);
    assert!(!app.editor().selections().primary().is_empty());
}

#[test]
fn esc_in_the_tree_goes_back_to_the_editor_with_its_selection() {
    let mut app = app("keyboard");
    select_all(&mut app);
    let selected: Vec<_> = app.editor().selections().iter().copied().collect();

    press(&mut app, KeyCode::F(6), KeyModifiers::NONE);
    assert_eq!(app.focus(), Focus::Tree);
    esc(&mut app);

    assert_eq!(app.focus(), Focus::Editor);
    let after: Vec<_> = app.editor().selections().iter().copied().collect();
    assert_eq!(after, selected, "the selection was lost");
}

#[test]
fn a_click_into_the_tree_is_undone_by_esc_the_same_way() {
    let mut app = app("mouse");
    select_all(&mut app);
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row: 5,
        modifiers: KeyModifiers::NONE,
    };
    typ_app::run::step(&mut app, AppEvent::Input(Event::Mouse(click)), AREA).unwrap();
    assert_eq!(app.focus(), Focus::Tree);

    esc(&mut app);

    assert_eq!(app.focus(), Focus::Editor);
}

#[test]
fn esc_in_the_editor_collapses_a_selection_and_stays() {
    let mut app = app("collapse");
    select_all(&mut app);

    esc(&mut app);

    assert_eq!(app.focus(), Focus::Editor);
    assert!(app.editor().selections().primary().is_empty());
}

#[test]
fn shift_f6_goes_the_other_way_round() {
    let mut app = app("previous");
    press(&mut app, KeyCode::F(6), KeyModifiers::SHIFT);
    assert_eq!(app.focus(), Focus::Tree);
    press(&mut app, KeyCode::F(6), KeyModifiers::SHIFT);
    assert_eq!(app.focus(), Focus::Editor);
}
