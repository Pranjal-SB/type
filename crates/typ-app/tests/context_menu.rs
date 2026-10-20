//! The context menu (interface §5, §8 4b): `shift+f10`, the menu key,
//! `ctrl+k .` or a right-click open a float of the actions for what is under
//! the cursor, by group, with their keys. Type to filter, arrows, `enter`,
//! `esc`, click.

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_app::app::targets::{Target, TargetKind};
use typ_app::run::step;
use typ_app::{App, Focus};
use typ_core::{Action, AppEvent, Group, Panel};

const FRAME: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

/// One pane holding a.rs, b.rs and c.rs, c.rs active, the dock folded.
fn app(name: &str) -> App {
    let dir: PathBuf = std::env::temp_dir().join("typ-context").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(dir.join(file), "fn main() {}\n").unwrap();
    }
    let mut app = App::new(&dir).unwrap();
    for file in ["a.rs", "b.rs", "c.rs"] {
        app.open_path(&dir.join(file)).unwrap();
    }
    // A first step at this size, so the layout is the one a click meets:
    // under 100 columns the dock folds on it.
    let resize = AppEvent::Input(Event::Resize(FRAME.width, FRAME.height));
    step(&mut app, resize, FRAME).unwrap();
    draw(&mut app);
    app
}

fn draw(app: &mut App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(FRAME.width, FRAME.height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    terminal.backend().buffer().clone()
}

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    let event = AppEvent::Input(Event::Key(KeyEvent::new(code, mods)));
    step(app, event, FRAME).unwrap();
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c), KeyModifiers::NONE);
    }
}

fn mouse(app: &mut App, button: MouseButton, (column, row): (u16, u16)) {
    let event = MouseEvent {
        kind: MouseEventKind::Down(button),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    step(app, AppEvent::Input(Event::Mouse(event)), FRAME).unwrap();
}

fn target(app: &App, kind: TargetKind) -> Target {
    *app.targets(FRAME)
        .iter()
        .find(|t| t.kind == kind)
        .unwrap_or_else(|| panic!("no {kind:?} on screen"))
}

fn rows(app: &App) -> Vec<Action> {
    app.context_rows().expect("no context menu")
}

fn titles(app: &App) -> Vec<String> {
    (0..app.tab_count()).map(|i| app.tab(i).title()).collect()
}

#[test]
fn right_click_a_background_tab_and_close_closes_that_tab() {
    let mut app = app("bg-tab");
    let tab = target(&app, TargetKind::Tab { pane: 0, tab: 0 });
    mouse(&mut app, MouseButton::Right, tab.point);

    assert_eq!(
        app.active_tab(),
        0,
        "the right-click did not make it current"
    );
    let shown = rows(&app);
    assert!(shown.contains(&Action::CloseTab), "{shown:?}");
    assert!(shown.iter().all(|a| a.group() == Group::Tabs), "{shown:?}");

    typed(&mut app, "close");
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(app.context_rows().is_none(), "the menu stayed up");
    assert_eq!(titles(&app), ["b.rs", "c.rs"]);
}

#[test]
fn a_click_on_a_row_runs_it() {
    let mut app = app("click-row");
    let tab = target(&app, TargetKind::Tab { pane: 0, tab: 1 });
    mouse(&mut app, MouseButton::Right, tab.point);
    draw(&mut app);

    let index = rows(&app)
        .iter()
        .position(|a| *a == Action::CloseTab)
        .unwrap();
    let row = target(&app, TargetKind::FloatRow(index));
    mouse(&mut app, MouseButton::Left, row.point);

    assert!(app.context_rows().is_none());
    assert_eq!(titles(&app), ["a.rs", "c.rs"]);
}

#[test]
fn shift_f10_in_the_editor_lists_edit_and_code_rows() {
    let mut app = app("shift-f10");
    press(&mut app, KeyCode::F(10), KeyModifiers::SHIFT);

    let shown = rows(&app);
    assert!(shown.contains(&Action::Copy), "{shown:?}");
    assert!(shown.contains(&Action::GotoDefinition), "{shown:?}");
    assert!(
        app.status_left().contains("type to filter"),
        "{}",
        app.status_left()
    );
    assert!(!shown.contains(&Action::CloseTab), "{shown:?}");
    // Moving the caret is typing, not something anyone right-clicks for.
    assert!(
        !shown.iter().any(|a| matches!(a, Action::Move { .. })),
        "{shown:?}"
    );
}

#[test]
fn typing_def_filters_to_go_to_definition() {
    let mut app = app("filter");
    press(&mut app, KeyCode::F(10), KeyModifiers::SHIFT);
    typed(&mut app, "def");
    assert_eq!(rows(&app), [Action::GotoDefinition]);
    press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
    assert!(rows(&app).len() > 1, "backspace did not widen the filter");
}

#[test]
fn ctrl_k_dot_and_the_menu_key_open_it_too() {
    let mut app = app("doors");
    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    typed(&mut app, ".");
    assert!(rows(&app).contains(&Action::Copy));
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.context_rows().is_none());

    press(&mut app, KeyCode::Menu, KeyModifiers::NONE);
    assert!(rows(&app).contains(&Action::Copy));
}

#[test]
fn arrows_and_enter_run_the_highlighted_row() {
    let mut app = app("arrows");
    press(&mut app, KeyCode::F(10), KeyModifiers::SHIFT);
    typed(&mut app, "whole");
    assert_eq!(rows(&app), [Action::SelectAll]);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert!(app.context_rows().is_none());
    assert!(
        !app.editor().selections().primary().is_empty(),
        "nothing selected"
    );
}

#[test]
fn esc_closes_it_and_the_next_key_is_text() {
    let mut app = app("esc");
    press(&mut app, KeyCode::F(10), KeyModifiers::SHIFT);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.context_rows().is_none());
    typed(&mut app, "x");
    assert_eq!(app.editor().line_text(0), "xfn main() {}");
}

#[test]
fn right_click_a_tree_row_selects_it_without_opening_it() {
    let mut app = app("tree");
    // Folded under 100 columns until asked for.
    app.apply_named_action(Action::ToggleLeftDock).unwrap();
    app.set_focus(Focus::Editor);
    draw(&mut app);
    let row = app
        .targets(FRAME)
        .into_iter()
        .find(|t| matches!(t.kind, TargetKind::TreeRow(_)))
        .expect("no tree row on screen");
    let before = app.tab_count();
    mouse(&mut app, MouseButton::Right, row.point);

    assert_eq!(app.focus(), Focus::Tree);
    assert_eq!(app.tab_count(), before, "a right-click opened a file");
    let shown = rows(&app);
    assert!(shown.iter().all(|a| a.group() == Group::Files), "{shown:?}");
}

#[test]
fn right_click_in_the_text_lists_the_text_rows() {
    let mut app = app("text");
    let editor = app.areas(FRAME).1;
    mouse(&mut app, MouseButton::Right, (editor.x + 4, editor.y + 2));
    assert_eq!(app.focus(), Focus::Editor);
    assert!(rows(&app).contains(&Action::Paste));
}

#[test]
fn a_click_elsewhere_closes_it_and_clicks_nothing() {
    let mut app = app("elsewhere");
    let tab = target(&app, TargetKind::Tab { pane: 0, tab: 0 });
    press(&mut app, KeyCode::F(10), KeyModifiers::SHIFT);
    assert!(
        app.targets(FRAME)
            .iter()
            .all(|t| matches!(t.kind, TargetKind::FloatRow(_)))
    );
    mouse(&mut app, MouseButton::Left, tab.point);
    assert!(app.context_rows().is_none());
    assert_eq!(app.active_tab(), 2, "the closing click also switched tabs");
}

#[test]
fn it_draws_as_a_float_at_the_pointer_with_keys() {
    let mut app = app("drawn");
    let editor = app.areas(FRAME).1;
    let at = (editor.x + 4, editor.y + 2);
    mouse(&mut app, MouseButton::Right, at);
    let buf = draw(&mut app);

    let rows_on_screen: Vec<Target> = app
        .targets(FRAME)
        .into_iter()
        .filter(|t| matches!(t.kind, TargetKind::FloatRow(_)))
        .collect();
    assert!(!rows_on_screen.is_empty());
    let first = rows_on_screen[0].rect;
    // Below and right of the pointer, inside the frame.
    assert!(first.x >= at.0 && first.y > at.1, "{first:?}");
    let line: String = (first.x..first.right())
        .map(|x| buf[(x, first.y)].symbol().to_string())
        .collect();
    let shown = rows(&app);
    assert!(line.contains(shown[0].description()), "{line:?}");
    let key = app.keymap().bindings_for(shown[0])[0].to_string();
    assert!(line.contains(&key), "{line:?} lacks {key}");
    for t in &rows_on_screen {
        assert!(FRAME.contains((t.rect.right() - 1, t.rect.bottom() - 1).into()));
    }
}
