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
    // About the open dock at 80 columns, which adapting to width would fold.
    app.set_adapt(false);
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

    // Backwards from the one pane is the dock.
    press(&mut app, KeyCode::F(6), KeyModifiers::SHIFT);
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
    assert_eq!(app.focus(), Focus::Status, "the walk wraps to the bar");
    press(&mut app, KeyCode::F(6), KeyModifiers::SHIFT);
    assert_eq!(app.focus(), Focus::Editor);
}

/// Where focus is: the dock, the status bar, or the file in the pane.
fn place(app: &App) -> String {
    match app.focus() {
        Focus::Tree => "dock".to_string(),
        Focus::Status => "status".to_string(),
        Focus::Editor => app.editor_title(),
    }
}

/// Three panes side by side, a.rs, b.rs and c.rs, the last one current.
fn three_panes(name: &str) -> App {
    let dir: PathBuf = std::env::temp_dir().join("typ-focus-stack").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(dir.join(file), "fn main() {}\n").unwrap();
    }
    let mut app = App::new(&dir).unwrap();
    // About the open dock at 80 columns, which adapting to width would fold.
    app.set_adapt(false);
    app.open_path(&dir.join("a.rs")).unwrap();
    for file in ["b.rs", "c.rs"] {
        app.apply_named_action(typ_core::Action::SplitRight)
            .unwrap();
        app.close_picker();
        app.open_path(&dir.join(file)).unwrap();
    }
    app
}

fn focus_pane(app: &mut App, n: u8) {
    app.apply_named_action(typ_core::Action::FocusPane(n))
        .unwrap();
}

#[test]
fn esc_walks_back_through_the_regions_in_the_order_they_were_visited() {
    let mut app = three_panes("walk");
    focus_pane(&mut app, 2);
    app.set_focus(Focus::Tree);
    // Backwards from the dock wraps to the status bar.
    press(&mut app, KeyCode::F(6), KeyModifiers::SHIFT);
    assert_eq!(place(&app), "status");

    let mut back = vec![];
    for _ in 0..3 {
        esc(&mut app);
        back.push(place(&app));
    }
    // Status bar to dock to the pane, and the pane is home: the third stays.
    assert_eq!(back, ["dock", "b.rs", "b.rs"]);
}

#[test]
fn esc_in_a_pane_never_moves_focus_even_with_a_way_back() {
    let mut app = three_panes("home-with-stack");
    focus_pane(&mut app, 1);
    app.set_focus(Focus::Tree);
    focus_pane(&mut app, 2);
    assert_eq!(place(&app), "b.rs");

    esc(&mut app);

    assert_eq!(
        place(&app),
        "b.rs",
        "esc in a pane clears, it does not leave"
    );
}

#[test]
fn a_closed_pane_is_skipped_on_the_way_back() {
    let mut app = three_panes("closed");
    focus_pane(&mut app, 1);
    focus_pane(&mut app, 2);
    app.apply_named_action(typ_core::Action::ClosePane).unwrap();
    app.set_focus(Focus::Tree);

    esc(&mut app);

    // b.rs was the newer place on the way back, and it is gone.
    assert_eq!(place(&app), "c.rs");
}

#[test]
fn esc_in_a_pane_with_an_empty_stack_stays() {
    let mut app = app("home");
    // Whatever the opening left on the stack, walk it out.
    for _ in 0..10 {
        esc(&mut app);
    }
    assert_eq!(app.focus(), Focus::Editor);
    esc(&mut app);
    assert_eq!(app.focus(), Focus::Editor, "the editor is home");
}

#[test]
fn folding_the_dock_drops_it_from_the_way_back() {
    let mut app = three_panes("fold");
    focus_pane(&mut app, 1);
    app.set_focus(Focus::Tree);
    focus_pane(&mut app, 2);
    app.set_focus(Focus::Tree);
    app.apply_named_action(typ_core::Action::ToggleLeftDock)
        .unwrap();
    assert!(!app.dock_open());
    assert_eq!(
        place(&app),
        "b.rs",
        "folding went back to where focus came from"
    );

    app.set_focus(Focus::Status);
    esc(&mut app);

    assert_eq!(place(&app), "b.rs", "not the folded dock");
}
