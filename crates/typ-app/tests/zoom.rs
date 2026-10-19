//! Zoom (interface §2): `ctrl+k z`, the `◰` cut into a heading or a tab row,
//! or a double-click on the dock's heading, gives the focused region the whole
//! body, and the same again puts everything back.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_app::run::{step, step_at};
use typ_app::{App, Focus};
use typ_core::{Action, AppEvent};

const FRAME: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

/// The frame less its status bar.
const BODY: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 23,
};

fn app(name: &str) -> (App, PathBuf) {
    let dir = std::env::temp_dir().join("typ-zoom").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let text: String = (0..40).map(|n| format!("line {n}\n")).collect();
    for file in ["a.rs", "b.rs"] {
        std::fs::write(dir.join(file), &text).unwrap();
    }
    let mut app = App::new(&dir).unwrap();
    // About the open dock at 80 columns, which adapting to width would fold.
    app.set_adapt(false);
    app.open_path(&dir.join("a.rs")).unwrap();
    (app, dir)
}

/// Two panes side by side, a.rs left and b.rs right, the left one current.
fn two_panes(name: &str) -> App {
    let (mut app, dir) = app(name);
    app.apply_named_action(Action::SplitRight).unwrap();
    app.close_picker();
    app.open_path(&dir.join("b.rs")).unwrap();
    app.apply_named_action(Action::FocusPane(1)).unwrap();
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

fn zoom(app: &mut App) {
    key(app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    key(app, KeyCode::Char('z'), KeyModifiers::NONE);
}

fn mouse_at(app: &mut App, kind: MouseEventKind, column: u16, row: u16, now: Instant) {
    let m = MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    step_at(app, AppEvent::Input(Event::Mouse(m)), FRAME, now).unwrap();
}

fn click_at(app: &mut App, column: u16, row: u16, now: Instant) {
    mouse_at(
        app,
        MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        now,
    );
    mouse_at(app, MouseEventKind::Up(MouseButton::Left), column, row, now);
}

fn click(app: &mut App, column: u16, row: u16) {
    click_at(app, column, row, Instant::now());
}

fn draw(app: &mut App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(FRAME.width, FRAME.height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    terminal.backend().buffer().clone()
}

#[test]
fn zooming_a_pane_gives_it_the_whole_body() {
    let mut app = two_panes("pane");

    zoom(&mut app);

    assert_eq!(app.pane_rects(FRAME), vec![BODY]);
    assert_eq!(app.areas(FRAME).0.width, 0, "the dock is hidden");
    assert_eq!(app.focus(), Focus::Editor);
    assert!(
        app.status_right().contains("zoomed"),
        "{}",
        app.status_right()
    );
}

#[test]
fn zooming_the_dock_gives_it_the_whole_body() {
    let (mut app, _) = app("dock");
    app.set_focus(Focus::Tree);

    zoom(&mut app);

    assert_eq!(app.areas(FRAME).0, Rect::new(0, 1, 80, 22));
    assert!(app.pane_rects(FRAME).is_empty(), "the panes are hidden");
    assert!(app.status_right().contains("zoomed"));
}

#[test]
fn a_second_zoom_restores_the_layout_exactly() {
    let mut app = two_panes("restore");
    // An uneven split first, so a restore that evened it would show.
    let rule = app.pane_rects(FRAME)[0].right();
    mouse_at(
        &mut app,
        MouseEventKind::Down(MouseButton::Left),
        rule,
        5,
        Instant::now(),
    );
    mouse_at(
        &mut app,
        MouseEventKind::Drag(MouseButton::Left),
        rule - 7,
        5,
        Instant::now(),
    );
    mouse_at(
        &mut app,
        MouseEventKind::Up(MouseButton::Left),
        rule - 7,
        5,
        Instant::now(),
    );
    let before = app.pane_rects(FRAME);
    let dock_before = app.areas(FRAME).0;

    zoom(&mut app);
    zoom(&mut app);

    assert_eq!(app.pane_rects(FRAME), before);
    assert_eq!(app.areas(FRAME).0, dock_before);
    assert!(!app.status_right().contains("zoomed"));
}

#[test]
fn the_zoom_glyph_on_a_tab_row_zooms_that_pane() {
    let mut app = two_panes("pane-glyph");
    let right = app.pane_rects(FRAME)[1];
    let buf = draw(&mut app);
    assert_eq!(buf[(right.right() - 1, 0)].symbol(), "◰");

    click(&mut app, right.right() - 1, 0);

    assert_eq!(app.pane_rects(FRAME), vec![BODY]);
    assert_eq!(app.current_pane(), 0, "the one zoomed is the only one");
    let buf = draw(&mut app);
    assert_eq!(buf[(79, 0)].symbol(), "◰");
    click(&mut app, 79, 0);
    assert_eq!(app.pane_rects(FRAME).len(), 2, "and again restores");
}

#[test]
fn the_zoom_glyph_on_the_dock_heading_zooms_the_dock() {
    let (mut app, _) = app("dock-glyph");
    let buf = draw(&mut app);
    // `── name ── ◰─` along the dock's 29 columns.
    assert_eq!(buf[(27, 0)].symbol(), "◰");

    click(&mut app, 27, 0);

    assert_eq!(app.focus(), Focus::Tree);
    assert_eq!(app.areas(FRAME).0, Rect::new(0, 1, 80, 22));
}

#[test]
fn a_double_click_on_the_dock_heading_zooms_it() {
    let (mut app, _) = app("double");
    let now = Instant::now();

    click_at(&mut app, 8, 0, now);
    assert_eq!(app.focus(), Focus::Tree, "one click focuses the dock");
    assert!(!app.pane_rects(FRAME).is_empty(), "and does not zoom it");
    click_at(&mut app, 8, 0, now + Duration::from_millis(200));

    assert!(app.pane_rects(FRAME).is_empty(), "the second click zooms");

    // Slow clicks are two single clicks.
    let later = now + Duration::from_secs(5);
    click_at(&mut app, 8, 0, later);
    click_at(&mut app, 8, 0, later + Duration::from_secs(2));
    assert!(app.pane_rects(FRAME).is_empty(), "still zoomed");
}

#[test]
fn a_click_where_a_hidden_pane_was_lands_in_the_zoomed_one() {
    let mut app = two_panes("hidden");
    let hidden = app.pane_rects(FRAME)[1];
    zoom(&mut app);

    click(&mut app, hidden.x + 3, 6);

    assert_eq!(app.current_pane(), 0);
    assert_eq!(app.focus(), Focus::Editor);
    assert_eq!(
        app.editor().cursor().line,
        5,
        "the click is the zoomed pane's"
    );
    // And where the dock was.
    click(&mut app, 3, 9);
    assert_eq!(app.focus(), Focus::Editor);
    assert_eq!(app.editor().cursor().line, 8);
}
