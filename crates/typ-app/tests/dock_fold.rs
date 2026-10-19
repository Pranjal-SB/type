//! `ctrl+b` and the folded dock (interface §5): closed opens and focuses it,
//! open without focus focuses it, focused folds it and focus goes back.

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_app::run::step;
use typ_app::{App, Focus};
use typ_core::{Action, AppEvent};

const FRAME: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

fn app(name: &str) -> (App, PathBuf) {
    let dir = std::env::temp_dir().join("typ-dock-fold").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["a.rs", "b.rs"] {
        std::fs::write(dir.join(file), "fn main() {}\n").unwrap();
    }
    let mut app = App::new(&dir).unwrap();
    // About the open dock at 80 columns, which adapting to width would fold.
    app.set_adapt(false);
    app.open_path(&dir.join("a.rs")).unwrap();
    (app, dir)
}

fn ctrl_b(app: &mut App) {
    step(
        app,
        AppEvent::Input(Event::Key(KeyEvent::new(
            KeyCode::Char('b'),
            KeyModifiers::CONTROL,
        ))),
        FRAME,
    )
    .unwrap();
}

fn click(app: &mut App, column: u16, row: u16) {
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        let m = MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        step(app, AppEvent::Input(Event::Mouse(m)), FRAME).unwrap();
    }
}

fn draw(app: &mut App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(FRAME.width, FRAME.height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    terminal.backend().buffer().clone()
}

#[test]
fn ctrl_b_from_the_editor_focuses_the_dock_then_folds_it() {
    let (mut app, _) = app("from-editor");
    assert_eq!(app.focus(), Focus::Editor);
    assert!(app.dock_open());

    ctrl_b(&mut app);
    assert_eq!(app.focus(), Focus::Tree, "open without focus: focus it");
    assert!(app.dock_open());

    ctrl_b(&mut app);
    assert!(!app.dock_open(), "focused: fold it");
    assert_eq!(app.focus(), Focus::Editor, "and focus goes back");

    ctrl_b(&mut app);
    assert!(app.dock_open(), "folded: open it");
    assert_eq!(app.focus(), Focus::Tree, "and focus it");
}

#[test]
fn ctrl_b_on_the_focused_dock_folds_it_at_once() {
    let (mut app, _) = app("from-tree");
    app.set_focus(Focus::Tree);

    ctrl_b(&mut app);

    assert!(!app.dock_open());
    assert_eq!(app.focus(), Focus::Editor);
}

#[test]
fn the_folded_dock_is_one_column_with_its_marker_on_top() {
    let (mut app, _) = app("folded-frame");
    app.apply_named_action(Action::ToggleLeftDock).unwrap();
    app.apply_named_action(Action::ToggleLeftDock).unwrap();
    assert!(!app.dock_open());

    let buf = draw(&mut app);

    assert_eq!(buf[(0, 0)].symbol(), "▸");
    for y in 1..23 {
        assert_eq!(buf[(0, y)].symbol(), "│", "row {y}");
    }
    // The panes take the rest: the tab row starts on the next column.
    assert_eq!(buf[(1, 0)].symbol(), " ");
    assert_eq!(buf[(2, 0)].symbol(), "a");
    let (tree, editor) = app.areas(FRAME);
    assert_eq!(tree.width, 0);
    assert_eq!(editor.x, 1);
    assert_eq!(editor.width, 79);
}

#[test]
fn a_click_on_the_folded_column_opens_the_dock() {
    let (mut app, _) = app("click");
    app.set_focus(Focus::Tree);
    ctrl_b(&mut app);
    assert!(!app.dock_open());

    click(&mut app, 0, 10);

    assert!(app.dock_open());
    assert_eq!(app.focus(), Focus::Tree);
    assert_eq!(app.areas(FRAME).0.width, 29);
}

#[test]
fn folding_the_dock_returns_focus_to_the_pane_it_came_from() {
    let (mut app, dir) = app("came-from");
    app.apply_named_action(Action::SplitRight).unwrap();
    app.close_picker();
    app.open_path(&dir.join("b.rs")).unwrap();
    app.apply_named_action(Action::FocusPane(1)).unwrap();
    app.apply_named_action(Action::FocusPane(2)).unwrap();
    assert_eq!(app.current_pane(), 1);

    ctrl_b(&mut app);
    ctrl_b(&mut app);

    assert!(!app.dock_open());
    assert_eq!(app.focus(), Focus::Editor);
    assert_eq!(app.current_pane(), 1, "back to pane 2, not pane 1");
}

#[test]
fn f6_does_not_land_on_a_folded_dock() {
    let (mut app, _) = app("f6");
    app.set_focus(Focus::Tree);
    ctrl_b(&mut app);
    assert!(!app.dock_open());

    for _ in 0..3 {
        step(
            &mut app,
            AppEvent::Input(Event::Key(KeyEvent::new(KeyCode::F(6), KeyModifiers::NONE))),
            FRAME,
        )
        .unwrap();
        assert_ne!(app.focus(), Focus::Tree);
    }
}
