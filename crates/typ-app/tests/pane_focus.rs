//! Moving between panes: by number, by direction, and by clicking.

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
use typ_core::{Action, AppEvent, ThemeColors};

const FRAME: Rect = Rect {
    x: 0,
    y: 0,
    width: 100,
    height: 30,
};

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-pane-focus").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (file, text) in [
        ("a.rs", "alpha\n"),
        ("b.rs", "first line\nsecond line\nthird line\n"),
        ("c.rs", "gamma\n"),
        ("d.rs", "delta\n"),
    ] {
        std::fs::write(dir.join(file), text).unwrap();
    }
    dir
}

fn draw(app: &mut App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(FRAME.width, FRAME.height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    terminal.backend().buffer().clone()
}

fn row(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
}

fn key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    step(
        app,
        AppEvent::Input(Event::Key(KeyEvent::new(code, mods))),
        FRAME,
    )
    .unwrap();
}

fn split(app: &mut App, action: Action, dir: &std::path::Path, file: &str) {
    app.apply_named_action(action).unwrap();
    app.close_picker();
    app.open_path(&dir.join(file)).unwrap();
}

/// a.rs top-left, b.rs top-right, c.rs bottom-left, d.rs bottom-right.
fn two_by_two(name: &str) -> (App, PathBuf) {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("a.rs")).unwrap();
    split(&mut app, Action::SplitRight, &dir, "b.rs");
    split(&mut app, Action::SplitDown, &dir, "d.rs");
    app.apply_named_action(Action::FocusPane(1)).unwrap();
    assert_eq!(app.editor_title(), "a.rs");
    split(&mut app, Action::SplitDown, &dir, "c.rs");
    app.apply_named_action(Action::FocusPane(1)).unwrap();
    draw(&mut app);
    (app, dir)
}

#[test]
fn panes_are_numbered_in_reading_order_and_the_number_is_in_the_tab_row() {
    let (mut app, _dir) = two_by_two("numbers");
    let buf = draw(&mut app);
    // Columns 30..64 and 65..100, rows 0..14 and 15..29.
    assert!(row(&buf, 0)[..].contains(" 1 "), "{}", row(&buf, 0));
    let top: String = row(&buf, 0).chars().skip(65).collect();
    assert!(top.starts_with(" 2 "), "{top}");
    let bottom_left: String = row(&buf, 15).chars().skip(30).collect();
    assert!(bottom_left.starts_with(" 3 "), "{bottom_left}");
    let bottom_right: String = row(&buf, 15).chars().skip(65).collect();
    assert!(bottom_right.starts_with(" 4 "), "{bottom_right}");

    // The focused pane's number is its label: accent and bold.
    let theme = ThemeColors::default();
    let one = (30..64).find(|&x| buf[(x, 0)].symbol() == "1").unwrap();
    assert_eq!(buf[(one, 0)].fg, theme.border_focused);
    let two = (65..100).find(|&x| buf[(x, 0)].symbol() == "2").unwrap();
    assert_ne!(buf[(two, 0)].fg, theme.border_focused);
}

#[test]
fn ctrl_k_and_a_digit_focuses_that_pane() {
    let (mut app, _dir) = two_by_two("digit");
    key(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    key(&mut app, KeyCode::Char('3'), KeyModifiers::NONE);
    assert_eq!(app.current_pane(), 2);
    assert_eq!(app.editor_title(), "c.rs");
    assert_eq!(app.focus(), Focus::Editor);
}

#[test]
fn alt_arrows_move_to_the_neighbour_on_that_side() {
    let (mut app, _dir) = two_by_two("arrows");

    key(&mut app, KeyCode::Right, KeyModifiers::ALT);
    assert_eq!(app.editor_title(), "b.rs", "alt+right from top-left");
    key(&mut app, KeyCode::Down, KeyModifiers::ALT);
    assert_eq!(app.editor_title(), "d.rs", "alt+down from top-right");
    key(&mut app, KeyCode::Left, KeyModifiers::ALT);
    assert_eq!(app.editor_title(), "c.rs", "alt+left from bottom-right");
    key(&mut app, KeyCode::Up, KeyModifiers::ALT);
    assert_eq!(app.editor_title(), "a.rs", "alt+up from bottom-left");

    // Nothing above the top row: focus stays where it is.
    key(&mut app, KeyCode::Up, KeyModifiers::ALT);
    assert_eq!(app.editor_title(), "a.rs");
    key(&mut app, KeyCode::Down, KeyModifiers::ALT);
    assert_eq!(app.editor_title(), "c.rs", "alt+down from top-left");
}

#[test]
fn a_neighbour_is_the_one_beside_the_cursor() {
    // Left column one pane, right column two: from the left, alt+right goes to
    // whichever right-hand pane the cursor's row is level with.
    let dir = fixture("beside");
    let mut app = App::new(&dir).unwrap();
    std::fs::write(dir.join("long.rs"), "x\n".repeat(40)).unwrap();
    app.open_path(&dir.join("long.rs")).unwrap();
    split(&mut app, Action::SplitRight, &dir, "b.rs");
    split(&mut app, Action::SplitDown, &dir, "d.rs");
    app.apply_named_action(Action::FocusPane(1)).unwrap();
    draw(&mut app);

    key(&mut app, KeyCode::Right, KeyModifiers::ALT);
    assert_eq!(app.editor_title(), "b.rs", "the cursor is on the top row");

    app.apply_named_action(Action::FocusPane(1)).unwrap();
    for _ in 0..20 {
        key(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    draw(&mut app);
    key(&mut app, KeyCode::Right, KeyModifiers::ALT);
    assert_eq!(
        app.editor_title(),
        "d.rs",
        "the cursor is level with the lower pane"
    );
}

#[test]
fn a_click_in_a_pane_focuses_it_and_puts_the_cursor_there() {
    let (mut app, _dir) = two_by_two("click");
    let buf = draw(&mut app);
    // b.rs is top-right: its tab row is row 0, its text starts on row 1, so
    // "second line" is on row 2.
    let y = 2;
    let x = (65..100)
        .find(|&x| buf[(x, y)].symbol() == "s")
        .expect("the second line drawn on row 2");

    step(
        &mut app,
        AppEvent::Input(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x + 2,
            row: y,
            modifiers: KeyModifiers::NONE,
        })),
        FRAME,
    )
    .unwrap();

    assert_eq!(app.editor_title(), "b.rs");
    assert_eq!(app.current_pane(), 1);
    assert_eq!(app.focus(), Focus::Editor);
    let cursor = app.editor().cursor();
    assert_eq!(
        (cursor.line, cursor.col),
        (1, 2),
        "second line, third grapheme"
    );
}

#[test]
fn a_click_on_a_pane_number_focuses_it() {
    let (mut app, _dir) = two_by_two("number-click");
    let buf = draw(&mut app);
    let x = (30..64).find(|&x| buf[(x, 15)].symbol() == "3").unwrap();
    step(
        &mut app,
        AppEvent::Input(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: 15,
            modifiers: KeyModifiers::NONE,
        })),
        FRAME,
    )
    .unwrap();
    assert_eq!(app.editor_title(), "c.rs");
    assert_eq!(app.tab_count(), 1, "the number is not a tab");
}

#[test]
fn the_wheel_scrolls_the_pane_under_the_pointer() {
    let dir = fixture("wheel");
    let mut app = App::new(&dir).unwrap();
    std::fs::write(dir.join("long.rs"), "x\n".repeat(200)).unwrap();
    app.open_path(&dir.join("a.rs")).unwrap();
    split(&mut app, Action::SplitRight, &dir, "long.rs");
    app.apply_named_action(Action::FocusPane(1)).unwrap();
    draw(&mut app);

    step(
        &mut app,
        AppEvent::Input(Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 80,
            row: 10,
            modifiers: KeyModifiers::NONE,
        })),
        FRAME,
    )
    .unwrap();

    assert_eq!(app.editor_title(), "a.rs", "the wheel moved focus");
    app.apply_named_action(Action::FocusPane(2)).unwrap();
    assert!(
        app.editor().top_line() > 0,
        "the pane under the pointer did not scroll"
    );
}

#[test]
fn the_unfocused_panes_active_tab_recedes() {
    let (mut app, _dir) = two_by_two("recede");
    let theme = ThemeColors::default();
    let buf = draw(&mut app);
    let b = row(&buf, 0).find("b.rs").unwrap();
    let b = row(&buf, 0)[..b].chars().count() as u16;
    assert_eq!(buf[(b, 0)].fg, theme.receded_fg);
    let a = row(&buf, 0).find("a.rs").unwrap();
    let a = row(&buf, 0)[..a].chars().count() as u16;
    assert_eq!(buf[(a, 0)].fg, theme.border_focused);
}
