//! The keyboard view of the keybindings tab (M4.5 Task 4).
//!
//! Every test points the tab at a `keys.toml` in its own temp directory
//! through `OpenWith`, so none reads or writes a real config.

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::keybindings::KEYBINDINGS;
use typ_core::{AppEvent, KeyChord, PanelEvent};

const CTRL: KeyModifiers = KeyModifiers::CONTROL;
const NONE: KeyModifiers = KeyModifiers::NONE;

fn open(name: &str) -> (App, PathBuf) {
    let dir = std::env::temp_dir().join("typ-keys-board").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("keys.toml");
    let mut app = App::new(&dir).unwrap();
    app.set_preset(typ_app::dock::Preset::Focus);
    app.apply(vec![PanelEvent::OpenWith {
        handler: KEYBINDINGS,
        path: path.clone(),
    }])
    .unwrap();
    (app, path)
}

fn draw(app: &mut App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    terminal.backend().buffer().clone()
}

fn line(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
}

fn screen(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| line(buf, y))
        .collect::<Vec<_>>()
        .join("\n")
}

fn find(buf: &Buffer, text: &str) -> Option<(u16, u16)> {
    (0..buf.area.height).find_map(|y| {
        let row = line(buf, y);
        let byte = row.find(text)?;
        Some((row[..byte].chars().count() as u16, y))
    })
}

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    app.handle_chord(KeyChord::from_event(KeyEvent::new(code, mods)))
        .unwrap();
}

fn click(app: &mut App, (column, row): (u16, u16), area: Rect) {
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: NONE,
    };
    typ_app::run::step(app, AppEvent::Input(Event::Mouse(event)), area).unwrap();
}

fn board(app: &mut App) -> String {
    press(app, KeyCode::Char('t'), CTRL);
    screen(&draw(app))
}

#[test]
fn ctrl_t_draws_the_keyboard_under_the_modifier_strip() {
    let (mut app, _) = open("draw");

    let text = board(&mut app);

    for chip in ["ctrl", "ctrl+shift", "alt", "ctrl+k \u{25b8}", "f-keys"] {
        assert!(text.contains(chip), "no {chip} in the strip:\n{text}");
    }
    // Bound, free and tty-owned keys, under ctrl.
    assert!(text.contains("quit"), "ctrl+q shows its action:\n{text}");
    assert!(
        text.contains("[ e ]"),
        "ctrl+e is free and outlined:\n{text}"
    );
    assert!(
        text.contains("tty"),
        "ctrl+i is taken by the terminal:\n{text}"
    );
}

#[test]
fn arrows_select_a_key_and_the_line_under_says_it_in_full() {
    let (mut app, _) = open("arrows");
    board(&mut app);

    // From `1`, down a row is `q`.
    press(&mut app, KeyCode::Down, NONE);
    let text = screen(&draw(&mut app));

    let detail = text
        .lines()
        .find(|l| l.contains("Ctrl+Q"))
        .unwrap_or_else(|| panic!("no detail line:\n{text}"));
    assert!(detail.contains("quit the editor"), "{detail}");
    assert!(detail.contains("universal"), "{detail}");
}

#[test]
fn tab_moves_along_the_strip_and_a_prefix_shows_what_is_under_it() {
    let (mut app, _) = open("strip");
    board(&mut app);

    // ctrl, ctrl+shift, alt, then the door.
    for _ in 0..3 {
        press(&mut app, KeyCode::Tab, NONE);
    }
    let text = screen(&draw(&mut app));
    assert!(text.contains("zoom"), "ctrl+k z shows zoom:\n{text}");

    press(&mut app, KeyCode::BackTab, KeyModifiers::SHIFT);
    press(&mut app, KeyCode::BackTab, KeyModifiers::SHIFT);
    // ctrl+shift+f, needs the protocol: `f` is row 2, column 3.
    press(&mut app, KeyCode::Down, NONE);
    press(&mut app, KeyCode::Down, NONE);
    for _ in 0..3 {
        press(&mut app, KeyCode::Right, NONE);
    }
    let text = screen(&draw(&mut app));
    let detail = text
        .lines()
        .find(|l| l.contains("Ctrl+Shift+F"))
        .unwrap_or_else(|| panic!("no detail line:\n{text}"));
    assert!(detail.contains("needs kitty"), "{detail}");
}

#[test]
fn enter_on_a_bound_key_records_over_it_and_del_unbinds_it() {
    let (mut app, path) = open("enter-del");
    board(&mut app);
    press(&mut app, KeyCode::Down, NONE);

    press(&mut app, KeyCode::Enter, NONE);
    let text = screen(&draw(&mut app));
    assert!(text.contains("for: quit the editor"), "{text}");
    press(&mut app, KeyCode::Esc, NONE);

    press(&mut app, KeyCode::Delete, NONE);
    assert_eq!(app.keymap().action_for("ctrl+q"), None);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "\"ctrl+q\" = \"\"\n"
    );
}

#[test]
fn by_mouse_the_toggle_a_chip_and_a_key_do_what_their_keys_do() {
    let (mut app, _) = open("mouse");
    let buf = draw(&mut app);
    let toggle = find(&buf, "ctrl+t keyboard").expect("no toggle on the search row");
    click(&mut app, toggle, buf.area);

    let buf = draw(&mut app);
    let alt = find(&buf, " alt ").expect("no alt chip");
    click(&mut app, (alt.0 + 1, alt.1), buf.area);
    let buf = draw(&mut app);
    // alt+. is the next tab; `.` is on the bottom row.
    let dot = find(&buf, "next_").expect("alt+. does not show next_tab");
    click(&mut app, dot, buf.area);
    let text = screen(&draw(&mut app));
    assert!(
        text.lines()
            .any(|l| l.contains("Alt+.") && l.contains("switch to the next tab")),
        "{text}"
    );

    // A second click on the selected key records over it, as enter does.
    let buf = draw(&mut app);
    click(&mut app, dot, buf.area);
    assert!(screen(&draw(&mut app)).contains("for: switch to the next tab"));
    press(&mut app, KeyCode::Esc, NONE);

    let buf = draw(&mut app);
    let back = find(&buf, "ctrl+t list").expect("no way back to the list");
    click(&mut app, back, buf.area);
    let list = screen(&draw(&mut app));
    assert!(list.contains("global"), "the list is not back:\n{list}");
}
