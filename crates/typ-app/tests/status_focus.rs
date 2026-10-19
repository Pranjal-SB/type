//! `f6` over every region (interface §5): the dock, the panes in screen order,
//! then the status bar, whose segments that do something take `←→` and
//! `enter`, or a click.

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_app::prompt::PromptKind;
use typ_app::run::step;
use typ_app::status::SegmentId;
use typ_app::{App, Focus};
use typ_core::{Action, AppEvent, ThemeColors};

const FRAME: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

fn app(name: &str) -> (App, PathBuf) {
    let dir = std::env::temp_dir().join("typ-status-focus").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["a.rs", "b.rs"] {
        std::fs::write(dir.join(file), "one\ntwo\nthree\n").unwrap();
    }
    let mut app = App::new(&dir).unwrap();
    // About the open dock at 80 columns, which adapting to width would fold.
    app.set_adapt(false);
    app.open_path(&dir.join("a.rs")).unwrap();
    (app, dir)
}

fn two_panes(name: &str) -> App {
    let (mut app, dir) = app(name);
    app.apply_named_action(Action::SplitRight).unwrap();
    app.close_picker();
    app.open_path(&dir.join("b.rs")).unwrap();
    app
}

fn key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    step(
        app,
        AppEvent::Input(Event::Key(KeyEvent::new(code, mods))),
        FRAME,
    )
    .unwrap();
}

fn f6(app: &mut App) {
    key(app, KeyCode::F(6), KeyModifiers::NONE);
}

/// Where focus is, as the walk names it.
fn place(app: &App) -> String {
    match app.focus() {
        Focus::Tree => "dock".to_string(),
        Focus::Editor => format!("pane {}", app.current_pane() + 1),
        Focus::Status => "status".to_string(),
    }
}

fn draw(app: &mut App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(FRAME.width, FRAME.height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    terminal.backend().buffer().clone()
}

/// The column `text` starts at on the status row.
fn status_column(buf: &Buffer, text: &str) -> u16 {
    // By cell, not by byte: the hint's `·` is two bytes in one cell.
    let cells: Vec<String> = (0..buf.area.width)
        .map(|x| buf[(x, FRAME.height - 1)].symbol().to_string())
        .collect();
    let want: Vec<String> = text.chars().map(String::from).collect();
    cells
        .windows(want.len())
        .rposition(|w| w == want.as_slice())
        .expect("not on the status bar") as u16
}

#[test]
fn f6_walks_the_dock_the_panes_and_the_status_bar_in_order() {
    let mut app = two_panes("cycle");
    app.set_focus(Focus::Tree);

    let mut walk = vec![place(&app)];
    for _ in 0..4 {
        f6(&mut app);
        walk.push(place(&app));
    }
    assert_eq!(walk, ["dock", "pane 1", "pane 2", "status", "dock"]);

    let mut back = vec![];
    for _ in 0..4 {
        key(&mut app, KeyCode::F(6), KeyModifiers::SHIFT);
        back.push(place(&app));
    }
    assert_eq!(back, ["status", "pane 2", "pane 1", "dock"]);
}

#[test]
fn a_folded_dock_is_not_in_the_walk() {
    let mut app = two_panes("folded");
    app.set_focus(Focus::Tree);
    app.apply_named_action(Action::ToggleLeftDock).unwrap();
    assert!(!app.dock_open());
    app.apply_named_action(Action::FocusPane(1)).unwrap();

    let mut walk = vec![];
    for _ in 0..3 {
        f6(&mut app);
        walk.push(place(&app));
    }
    assert_eq!(walk, ["pane 2", "status", "pane 1"]);
}

#[test]
fn enter_on_the_position_opens_the_goto_prompt() {
    let (mut app, _) = app("enter");
    while app.focus() != Focus::Status {
        f6(&mut app);
    }
    assert_eq!(app.focused_segment(), Some(SegmentId::Position));

    // Nothing else on the bar does anything yet, so the arrows stay put.
    key(&mut app, KeyCode::Right, KeyModifiers::NONE);
    key(&mut app, KeyCode::Left, KeyModifiers::NONE);
    assert_eq!(app.focused_segment(), Some(SegmentId::Position));
    key(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(app.prompt().map(|p| p.kind()), Some(PromptKind::GotoLine));
}

#[test]
fn a_click_on_the_position_opens_the_goto_prompt() {
    let (mut app, _) = app("click");
    let buf = draw(&mut app);
    let column = status_column(&buf, "1:1");

    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        let m = MouseEvent {
            kind,
            column,
            row: FRAME.height - 1,
            modifiers: KeyModifiers::NONE,
        };
        step(&mut app, AppEvent::Input(Event::Mouse(m)), FRAME).unwrap();
    }

    assert_eq!(app.prompt().map(|p| p.kind()), Some(PromptKind::GotoLine));
}

#[test]
fn the_focused_segment_takes_the_primary_selection_ground() {
    let (mut app, _) = app("paint");
    while app.focus() != Focus::Status {
        f6(&mut app);
    }
    let buf = draw(&mut app);
    let theme = ThemeColors::default();
    let column = status_column(&buf, "1:1");

    assert_eq!(buf[(column, 23)].bg, theme.selection_primary_bg);
    assert_ne!(buf[(column - 2, 23)].bg, theme.selection_primary_bg);
}

#[test]
fn keys_in_the_status_bar_do_not_edit_the_buffer() {
    let (mut app, _) = app("typing");
    while app.focus() != Focus::Status {
        f6(&mut app);
    }

    key(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    key(&mut app, KeyCode::Down, KeyModifiers::NONE);

    assert!(!app.editor().is_dirty());
    assert_eq!(app.editor().cursor().line, 0);
}
