//! The menu bar (interface §8 4a): hidden, and summoned by `f10`. A row of
//! the action groups; `←→` move, `↓` or `enter` drop a group's actions down
//! with their keys, `esc` steps back. Clicks do the same.

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::app::targets::{Target, TargetKind};
use typ_app::float_menu::actions_for;
use typ_app::run::step;
use typ_core::{Action, AppEvent, Group};

const FRAME: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

fn app(name: &str) -> App {
    let dir: PathBuf = std::env::temp_dir().join("typ-menu-bar").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "fn main() {}\nfn other() {}\n").unwrap();
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("a.rs")).unwrap();
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

fn line(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width)
        .map(|x| buf[(x, y)].symbol().to_string())
        .collect()
}

fn press(app: &mut App, code: KeyCode) {
    let event = AppEvent::Input(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
    step(app, event, FRAME).unwrap();
}

fn click(app: &mut App, (column, row): (u16, u16)) {
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
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

fn selected_something(app: &App) -> bool {
    !app.editor().selections().primary().is_empty()
}

#[test]
fn hidden_the_bar_takes_no_row() {
    let mut app = app("hidden");
    let areas = app.areas(FRAME);
    let top = line(&draw(&mut app), 0);
    assert!(app.menu_bar().is_none());
    assert!(!top.contains("selection"), "{top}");

    press(&mut app, KeyCode::F(10));
    let shown = line(&draw(&mut app), 0);
    assert!(
        shown.contains("edit") && shown.contains("selection"),
        "{shown}"
    );
    // Drawn over the top row: nothing below it moves.
    assert_eq!(app.areas(FRAME), areas);
    // The bar's keys are on the status bar while it is up (interface §8).
    assert!(
        app.status_left().contains("esc closes"),
        "{}",
        app.status_left()
    );
    press(&mut app, KeyCode::Down);
    assert!(
        app.status_left().contains("esc steps back"),
        "{}",
        app.status_left()
    );
    press(&mut app, KeyCode::Esc);

    press(&mut app, KeyCode::Esc);
    assert!(app.menu_bar().is_none());
    assert_eq!(line(&draw(&mut app), 0), top);
}

#[test]
fn f10_right_down_enter_runs_the_second_groups_first_action() {
    let mut app = app("keys");
    assert_eq!(Group::ALL[1], Group::Selection);
    assert_eq!(actions_for(&[Group::Selection])[0], Action::SelectAll);

    press(&mut app, KeyCode::F(10));
    press(&mut app, KeyCode::Right);
    assert_eq!(app.menu_bar().map(|(g, _)| g), Some(Group::Selection));
    press(&mut app, KeyCode::Down);
    let (_, rows) = app.menu_bar().unwrap();
    assert_eq!(rows.unwrap()[0], Action::SelectAll);
    press(&mut app, KeyCode::Enter);

    assert!(app.menu_bar().is_none(), "the bar stayed up");
    assert!(selected_something(&app));
}

#[test]
fn the_same_by_mouse() {
    let mut app = app("mouse");
    press(&mut app, KeyCode::F(10));
    draw(&mut app);
    let point = target(&app, TargetKind::BarGroup(1)).point;
    click(&mut app, point);
    assert_eq!(app.menu_bar().map(|(g, _)| g), Some(Group::Selection));
    draw(&mut app);
    let point = target(&app, TargetKind::FloatRow(0)).point;
    click(&mut app, point);

    assert!(app.menu_bar().is_none());
    assert!(selected_something(&app));
}

#[test]
fn the_dropdown_shows_each_row_with_its_key() {
    let mut app = app("dropdown");
    press(&mut app, KeyCode::F(10));
    press(&mut app, KeyCode::Enter);
    let buf = draw(&mut app);
    let row = target(&app, TargetKind::FloatRow(0)).rect;
    let text = line(&buf, row.y);
    let first = actions_for(&[Group::Edit])[0];
    assert!(text.contains(first.description()), "{text}");
    let key = app.keymap().bindings_for(first)[0].to_string();
    assert!(text.contains(&key), "{text} lacks {key}");
}

#[test]
fn esc_steps_back_a_level_at_a_time() {
    let mut app = app("esc");
    press(&mut app, KeyCode::F(10));
    press(&mut app, KeyCode::Down);
    assert!(app.menu_bar().unwrap().1.is_some());
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.menu_bar().map(|(_, rows)| rows), Some(None));
    press(&mut app, KeyCode::Esc);
    assert!(app.menu_bar().is_none());
    // The next key is text again.
    press(&mut app, KeyCode::Char('x'));
    assert_eq!(app.editor().line_text(0), "xfn main() {}");
}

#[test]
fn left_and_right_in_a_dropdown_move_to_the_next_group() {
    let mut app = app("across");
    press(&mut app, KeyCode::F(10));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Right);
    let (group, rows) = app.menu_bar().unwrap();
    assert_eq!(group, Group::Selection);
    assert_eq!(rows.unwrap(), actions_for(&[Group::Selection]));
    // From the first group, left wraps to the last.
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Left);
    assert_eq!(app.menu_bar().map(|(g, _)| g), Some(Group::App));
}

#[test]
fn a_click_elsewhere_closes_it() {
    let mut app = app("elsewhere");
    press(&mut app, KeyCode::F(10));
    press(&mut app, KeyCode::Down);
    draw(&mut app);
    let editor = app.areas(FRAME).1;
    let caret = app.editor().selections().primary();
    click(&mut app, (editor.right() - 2, editor.bottom() - 2));
    assert!(app.menu_bar().is_none());
    assert_eq!(
        app.editor().selections().primary(),
        caret,
        "the click went through"
    );
}
