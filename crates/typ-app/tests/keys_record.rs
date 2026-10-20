//! Recording a key in the keybindings tab (M4.5 Task 3).
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
use typ_app::keybindings::{KEYBINDINGS, KeysView};
use typ_core::{Action, AppEvent, KeyChord, PanelEvent};

const CTRL: KeyModifiers = KeyModifiers::CONTROL;
const NONE: KeyModifiers = KeyModifiers::NONE;

fn open(name: &str, keys: &str) -> (App, PathBuf) {
    let dir = std::env::temp_dir().join("typ-keys-record").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("keys.toml");
    if !keys.is_empty() {
        std::fs::write(&path, keys).unwrap();
    }
    let mut app = App::new(&dir).unwrap();
    app.set_preset(typ_app::dock::Preset::Focus);
    app.apply(vec![PanelEvent::OpenWith {
        handler: KEYBINDINGS,
        path: path.clone(),
    }])
    .unwrap();
    (app, path)
}

fn view(app: &App) -> &KeysView {
    app.shown_panel()
        .as_any()
        .downcast_ref::<KeysView>()
        .expect("the keybindings tab is not the one on screen")
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

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c), NONE);
    }
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

/// Filter to the save row and put the keys on it.
fn select_save(app: &mut App) {
    typed(app, "save");
    press(app, KeyCode::Down, NONE);
    assert_eq!(
        view(app).selected(),
        Some((Some("ctrl+s".to_string()), Action::Save))
    );
}

fn file(path: &std::path::Path) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[test]
fn rebinding_save_to_ctrl_k_s_moves_the_keymap_the_menu_the_hint_and_the_palette() {
    let (mut app, path) = open("rebind", "");
    select_save(&mut app);

    press(&mut app, KeyCode::Enter, NONE);
    press(&mut app, KeyCode::Char('k'), CTRL);
    press(&mut app, KeyCode::Char('s'), NONE);
    press(&mut app, KeyCode::Enter, NONE);

    assert_eq!(
        file(&path).as_deref(),
        Some("\"ctrl+k s\" = \"save\"\n\"ctrl+s\" = \"\"\n")
    );
    // The keymap.
    assert_eq!(app.keymap().action_for("ctrl+k s"), Some(Action::Save));
    assert_eq!(app.keymap().action_for("ctrl+s"), None);
    // The hint.
    assert!(
        app.status_left().contains("Ctrl+K S save"),
        "{}",
        app.status_left()
    );
    // The palette.
    app.apply_named_action(Action::OpenCommandPalette).unwrap();
    let row = app
        .picker()
        .unwrap()
        .commands()
        .iter()
        .find(|row| row.name == "save")
        .expect("no save row in the palette")
        .binding
        .clone();
    assert_eq!(row, "ctrl+k s");
    app.close_picker();
    // The ctrl+k menu, from an editor tab so the list is not behind it.
    press(&mut app, KeyCode::Char('1'), KeyModifiers::ALT);
    press(&mut app, KeyCode::Char('k'), CTRL);
    let menu = screen(&draw(&mut app));
    let save = menu
        .lines()
        .find(|l| l.contains("save the file"))
        .unwrap_or_else(|| panic!("the menu has no save row:\n{menu}"));
    assert!(save.contains("s  save the file"), "{save}");
    press(&mut app, KeyCode::Esc, NONE);
    // And the old chord no longer saves.
    press(&mut app, KeyCode::Char('s'), CTRL);
    assert!(
        !app.status().is_some_and(|s| s.starts_with("Save")),
        "{:?}",
        app.status()
    );
}

#[test]
fn the_float_shows_keycaps_and_a_prefix_waits_for_the_next_key() {
    let (mut app, path) = open("keycaps", "");
    select_save(&mut app);

    press(&mut app, KeyCode::Enter, NONE);
    let before = screen(&draw(&mut app));
    assert!(before.contains("press the key"), "{before}");

    press(&mut app, KeyCode::Char('k'), CTRL);
    let waiting = screen(&draw(&mut app));
    assert!(waiting.contains("[Ctrl+K]"), "{waiting}");
    assert!(waiting.contains("enter replace"), "{waiting}");

    press(&mut app, KeyCode::Char('s'), NONE);
    let done = screen(&draw(&mut app));
    assert!(done.contains("[Ctrl+K] [S]"), "{done}");
    assert_eq!(file(&path), None, "nothing is written before enter");
}

#[test]
fn a_key_already_bound_says_what_it_does_now_before_enter_replaces_it() {
    let (mut app, path) = open("conflict", "");
    select_save(&mut app);

    press(&mut app, KeyCode::Enter, NONE);
    press(&mut app, KeyCode::Char('d'), CTRL);
    let text = screen(&draw(&mut app));
    assert!(
        text.contains("add the next match of the selection"),
        "the conflict is not named:\n{text}"
    );
    assert_eq!(file(&path), None);

    press(&mut app, KeyCode::Enter, NONE);
    assert_eq!(
        file(&path).as_deref(),
        Some("\"ctrl+d\" = \"save\"\n\"ctrl+s\" = \"\"\n")
    );
    for _ in "save".chars() {
        press(&mut app, KeyCode::Backspace, NONE);
    }
    typed(&mut app, ":unbound");
    assert!(
        view(&app)
            .visible()
            .contains(&(None, Action::SelectNextOccurrence)),
        "the replaced action did not lose its key"
    );
}

#[test]
fn the_capability_line_says_tty_owned_needs_kitty_or_universal() {
    let (mut app, _) = open("capability", "");
    select_save(&mut app);
    press(&mut app, KeyCode::Enter, NONE);

    press(&mut app, KeyCode::Char('i'), CTRL);
    assert!(screen(&draw(&mut app)).contains("tty-owned"));

    press(&mut app, KeyCode::Char('e'), CTRL | KeyModifiers::SHIFT);
    assert!(screen(&draw(&mut app)).contains("needs kitty"));

    press(&mut app, KeyCode::Char('x'), KeyModifiers::ALT);
    assert!(screen(&draw(&mut app)).contains("universal"));
}

#[test]
fn esc_cancels_and_writes_nothing() {
    let (mut app, path) = open("esc", "");
    select_save(&mut app);

    press(&mut app, KeyCode::Enter, NONE);
    press(&mut app, KeyCode::Char('k'), CTRL);
    press(&mut app, KeyCode::Char('s'), NONE);
    assert!(screen(&draw(&mut app)).contains("[Ctrl+K] [S]"), "no float");
    press(&mut app, KeyCode::Esc, NONE);

    assert_eq!(file(&path), None);
    assert!(!screen(&draw(&mut app)).contains("enter replace"));
    assert_eq!(app.keymap().action_for("ctrl+s"), Some(Action::Save));
}

#[test]
fn del_unbinds_the_row() {
    let (mut app, path) = open("del", "");
    select_save(&mut app);

    press(&mut app, KeyCode::Delete, NONE);

    assert_eq!(file(&path).as_deref(), Some("\"ctrl+s\" = \"\"\n"));
    assert_eq!(app.keymap().action_for("ctrl+s"), None);
}

#[test]
fn r_puts_the_action_back_to_its_default() {
    let (mut app, path) = open(
        "reset",
        "# mine\n\"ctrl+k s\" = \"save\"\n\"ctrl+s\" = \"\"\n",
    );
    typed(&mut app, "save");
    press(&mut app, KeyCode::Down, NONE);

    press(&mut app, KeyCode::Char('r'), NONE);

    assert_eq!(file(&path).as_deref(), Some("# mine\n"));
    assert_eq!(app.keymap().action_for("ctrl+s"), Some(Action::Save));
    assert_eq!(app.keymap().action_for("ctrl+k s"), None);
}

#[test]
fn ctrl_z_undoes_the_last_write() {
    let (mut app, path) = open("undo", "# mine\n");
    select_save(&mut app);
    press(&mut app, KeyCode::Delete, NONE);
    assert_eq!(app.keymap().action_for("ctrl+s"), None);

    press(&mut app, KeyCode::Char('z'), CTRL);

    assert_eq!(file(&path).as_deref(), Some("# mine\n"));
    assert_eq!(app.keymap().action_for("ctrl+s"), Some(Action::Save));
}

#[test]
fn by_mouse_a_click_on_the_key_records_and_replace_writes() {
    let (mut app, path) = open("mouse", "");
    typed(&mut app, "save");
    let buf = draw(&mut app);
    let key = find(&buf, "Ctrl+S").expect("no Ctrl+S on screen");

    click(&mut app, key, buf.area);
    press(&mut app, KeyCode::Char('k'), CTRL);
    press(&mut app, KeyCode::Char('s'), NONE);
    let buf = draw(&mut app);
    let replace = find(&buf, "replace").expect("no replace in the float");
    click(&mut app, replace, buf.area);

    assert_eq!(app.keymap().action_for("ctrl+k s"), Some(Action::Save));
    assert!(file(&path).is_some_and(|f| f.contains("\"ctrl+k s\" = \"save\"")));
}

#[test]
fn by_mouse_cancel_writes_nothing() {
    let (mut app, path) = open("mouse-cancel", "");
    typed(&mut app, "save");
    let buf = draw(&mut app);
    click(&mut app, find(&buf, "Ctrl+S").unwrap(), buf.area);
    press(&mut app, KeyCode::Char('x'), KeyModifiers::ALT);

    let buf = draw(&mut app);
    click(&mut app, find(&buf, "cancel").expect("no cancel"), buf.area);

    assert_eq!(file(&path), None);
    assert!(!screen(&draw(&mut app)).contains("enter replace"));
}

#[test]
fn by_mouse_the_footer_unbinds_resets_and_undoes() {
    let (mut app, path) = open("mouse-footer", "");
    select_save(&mut app);

    let buf = draw(&mut app);
    click(
        &mut app,
        find(&buf, "del unbind").expect("no del"),
        buf.area,
    );
    assert_eq!(app.keymap().action_for("ctrl+s"), None);

    let buf = draw(&mut app);
    click(
        &mut app,
        find(&buf, "ctrl+z undo").expect("no undo"),
        buf.area,
    );
    assert_eq!(app.keymap().action_for("ctrl+s"), Some(Action::Save));
    assert_eq!(file(&path), None);

    press(&mut app, KeyCode::Delete, NONE);
    let buf = draw(&mut app);
    // The save row is unbound now; select it and reset by click.
    let at = find(&buf, "save the file").expect("no unbound save row");
    click(&mut app, at, buf.area);
    let buf = draw(&mut app);
    click(&mut app, find(&buf, "r reset").expect("no reset"), buf.area);
    assert_eq!(app.keymap().action_for("ctrl+s"), Some(Action::Save));
}
