//! Splitting the editor area into panes, and closing them again.

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_app::{App, Focus};
use typ_core::{Action, AppEvent, KeyChord, ThemeColors};

const FRAME: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-panes").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "alpha\n").unwrap();
    std::fs::write(dir.join("b.rs"), "beta\n").unwrap();
    dir
}

fn app(name: &str) -> (App, PathBuf) {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("a.rs")).unwrap();
    (app, dir)
}

fn draw(app: &mut App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(FRAME.width, FRAME.height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    terminal.backend().buffer().clone()
}

fn row(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
}

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    app.handle_chord(KeyChord::from_event(KeyEvent::new(code, mods)))
        .unwrap();
}

#[test]
fn split_right_gives_two_halves_with_one_rule_between() {
    let theme = ThemeColors::default();
    let (mut app, _dir) = app("split-right");

    app.apply_named_action(Action::SplitRight).unwrap();

    // The editor region is columns 30..80: 49 after the rule, 24 and 25.
    assert_eq!(
        app.pane_rects(FRAME),
        vec![Rect::new(30, 0, 24, 23), Rect::new(55, 0, 25, 23)]
    );
    // The new pane opens empty, with the file picker up to fill it.
    assert_eq!(app.current_pane(), 1);
    assert!(app.picker().is_some(), "no picker in the new pane");
    app.close_picker();

    let buf = draw(&mut app);
    for y in 0..23 {
        assert_eq!(buf[(54, y)].symbol(), "│", "row {y}: {}", row(&buf, y));
        assert_eq!(buf[(54, y)].fg, theme.border);
        assert_ne!(buf[(53, y)].symbol(), "│", "row {y}: a second rule");
    }
    assert!(row(&buf, 0)[..].contains("a.rs"), "{}", row(&buf, 0));
    assert!(row(&buf, 0).contains("untitled"), "{}", row(&buf, 0));
}

#[test]
fn split_down_stacks_the_new_pane_under_a_horizontal_rule() {
    let (mut app, _dir) = app("split-down");

    app.apply_named_action(Action::SplitDown).unwrap();
    app.close_picker();

    // 23 body rows: 11, the rule, 11.
    assert_eq!(
        app.pane_rects(FRAME),
        vec![Rect::new(30, 0, 50, 11), Rect::new(30, 12, 50, 11)]
    );
    let buf = draw(&mut app);
    assert_eq!(
        row(&buf, 11)[..].chars().skip(30).collect::<String>(),
        "─".repeat(50)
    );
}

#[test]
fn the_door_rows_split_and_close() {
    let (mut app, _dir) = app("door-rows");
    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('\\'), KeyModifiers::NONE);
    assert_eq!(app.pane_count(), 2);
    app.close_picker();

    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('-'), KeyModifiers::NONE);
    assert_eq!(app.pane_count(), 3);
    app.close_picker();

    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('x'), KeyModifiers::NONE);
    assert_eq!(app.pane_count(), 2);
}

#[test]
fn a_click_on_the_menu_row_splits() {
    // The mouse path: the door's rows are clickable, and every action is one.
    // A tall frame, so the menu (a third of it at most) shows every row.
    const TALL: Rect = Rect::new(0, 0, 120, 90);
    let (mut app, _dir) = app("menu-click");
    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    let mut terminal = Terminal::new(TestBackend::new(TALL.width, TALL.height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let (x, y) = (0..TALL.height)
        .find_map(|y| {
            let text = row(&buf, y);
            let byte = text.find("split right")?;
            Some((text[..byte].chars().count() as u16, y))
        })
        .expect("a split row in the menu");
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x + 2,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    typ_app::run::step(&mut app, AppEvent::Input(Event::Mouse(event)), TALL).unwrap();

    assert_eq!(app.pane_count(), 2);
}

#[test]
fn opening_a_file_open_in_another_pane_focuses_that_pane() {
    let (mut app, dir) = app("open-elsewhere");
    app.apply_named_action(Action::SplitRight).unwrap();
    app.close_picker();
    assert_eq!(app.current_pane(), 1);

    app.open_path(&dir.join("a.rs")).unwrap();

    assert_eq!(app.current_pane(), 0, "a.rs is open in the first pane");
    assert_eq!(app.focus(), Focus::Editor);
    // No second buffer: the new pane still holds only its empty one.
    let buf = draw(&mut app);
    let tabs = row(&buf, 0);
    assert_eq!(tabs.matches("a.rs").count(), 1, "{tabs}");
    assert!(tabs.contains("untitled"), "{tabs}");
}

#[test]
fn closing_a_pane_with_unsaved_work_asks_first() {
    let (mut app, _dir) = app("dirty-close");
    app.apply_named_action(Action::SplitRight).unwrap();
    app.close_picker();
    press(&mut app, KeyCode::Char('z'), KeyModifiers::NONE);

    app.apply_named_action(Action::ClosePane).unwrap();
    assert_eq!(app.pane_count(), 2, "closed without asking");
    assert!(
        app.status().is_some_and(|s| s.contains("again")),
        "{:?}",
        app.status()
    );

    app.apply_named_action(Action::ClosePane).unwrap();
    assert_eq!(app.pane_count(), 1);
    assert_eq!(
        app.editor_title(),
        "a.rs",
        "focus went to the pane that is left"
    );
    assert_eq!(app.pane_rects(FRAME), vec![Rect::new(30, 0, 50, 23)]);
}

#[test]
fn the_last_pane_does_not_close() {
    let (mut app, _dir) = app("last");
    app.apply_named_action(Action::ClosePane).unwrap();
    assert_eq!(app.pane_count(), 1);
    assert_eq!(app.editor_title(), "a.rs");
    assert!(app.status().is_some());
}
