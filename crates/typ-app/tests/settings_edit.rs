//! Changing a value in the settings tab (M4.4 Task 5): it writes through
//! `config::write_setting`, keeps the file's comments, and applies live.
//!
//! Every test points the tab at a `config.toml` in its own temp directory.

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::settings::{SETTINGS, SettingsView};
use typ_core::{AppEvent, KeyChord, PanelEvent};

const AREA: Rect = Rect::new(0, 0, 120, 30);

const COMMENTED: &str = "# Kept by the tab.\ntheme = \"slate\" # dark\n";

fn open(name: &str, config: &str) -> (App, PathBuf) {
    let dir = std::env::temp_dir().join("typ-settings-edit").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    if !config.is_empty() {
        std::fs::write(&path, config).unwrap();
    }
    std::fs::write(dir.join("a.rs"), "fn a() {}\n").unwrap();
    let mut app = App::new(&dir).unwrap();
    app.set_preset(typ_app::dock::Preset::Focus);
    app.open_path(&dir.join("a.rs")).unwrap();
    app.apply(vec![PanelEvent::OpenWith {
        handler: SETTINGS,
        path: path.clone(),
    }])
    .unwrap();
    (app, path)
}

fn view(app: &App) -> &SettingsView {
    app.shown_panel()
        .as_any()
        .downcast_ref::<SettingsView>()
        .expect("the settings tab is not on screen")
}

fn draw(app: &mut App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).unwrap();
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

/// Where `text` starts on the row that also holds `on`, as (column, row).
fn find_on(buf: &Buffer, on: &str, text: &str) -> Option<(u16, u16)> {
    (0..buf.area.height).find_map(|y| {
        let row = line(buf, y);
        if !row.contains(on) {
            return None;
        }
        let byte = row.rfind(text)?;
        Some((row[..byte].chars().count() as u16, y))
    })
}

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    app.handle_chord(KeyChord::from_event(KeyEvent::new(code, mods)))
        .unwrap();
}

fn key(app: &mut App, code: KeyCode) {
    press(app, code, KeyModifiers::NONE);
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        key(app, KeyCode::Char(c));
    }
}

fn click(app: &mut App, (column, row): (u16, u16)) {
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    typ_app::run::step(app, AppEvent::Input(Event::Mouse(event)), AREA).unwrap();
}

/// Select the row for `setting` from the search row.
fn select(app: &mut App, setting: &str) {
    typed(app, setting);
    key(app, KeyCode::Down);
    assert_eq!(view(app).selected(), Some(setting));
}

fn file(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn right_on_the_theme_switches_the_running_theme_and_writes_it_keeping_comments() {
    let (mut app, path) = open("theme-key", COMMENTED);
    let before = app.theme_colors();
    select(&mut app, "theme");

    key(&mut app, KeyCode::Right);

    assert_eq!(
        file(&path),
        "# Kept by the tab.\ntheme = \"mocha\" # dark\n"
    );
    assert_ne!(app.theme_colors(), before, "the palette did not move");
    assert!(
        !app.syntax_theme().is_empty(),
        "the syntax half came with it"
    );
    let text = screen(&draw(&mut app));
    assert!(text.contains("\u{2039} mocha \u{203a}"), "{text}");
}

#[test]
fn a_click_on_the_right_arrow_does_what_right_does() {
    let (mut app, path) = open("theme-click", COMMENTED);
    let buf = draw(&mut app);

    click(&mut app, find_on(&buf, "theme", "\u{203a}").unwrap());

    assert!(
        file(&path).contains("theme = \"mocha\" # dark"),
        "{}",
        file(&path)
    );
}

#[test]
fn space_toggles_and_a_click_on_the_box_toggles_it_back() {
    let (mut app, path) = open("toggle", "");
    select(&mut app, "adapt");

    key(&mut app, KeyCode::Char(' '));
    assert_eq!(file(&path), "adapt = false\n");

    let buf = draw(&mut app);
    click(&mut app, find_on(&buf, "adapt", "[ ]").unwrap());
    assert_eq!(file(&path), "adapt = true\n");
}

#[test]
fn an_invalid_indent_width_says_why_and_writes_nothing() {
    let (mut app, path) = open("number-bad", COMMENTED);
    select(&mut app, "indent_width");

    key(&mut app, KeyCode::Enter);
    typed(&mut app, "40");
    key(&mut app, KeyCode::Enter);

    assert_eq!(file(&path), COMMENTED);
    let text = screen(&draw(&mut app));
    assert!(text.contains("a whole number from 1 to 16"), "{text}");
    // Esc reverts and leaves the field.
    key(&mut app, KeyCode::Esc);
    let text = screen(&draw(&mut app));
    assert!(text.contains("[ auto ]"), "{text}");
    assert_eq!(file(&path), COMMENTED);
}

#[test]
fn a_valid_indent_width_is_written_and_applied() {
    let (mut app, path) = open("number-good", "");
    select(&mut app, "indent_width");

    key(&mut app, KeyCode::Enter);
    typed(&mut app, "2");
    key(&mut app, KeyCode::Enter);

    assert_eq!(file(&path), "indent_width = 2\n");
    app.activate_tab(0);
    assert_eq!(app.editor().tab_width(), 2);
}

#[test]
fn a_click_on_the_number_field_edits_it() {
    let (mut app, path) = open("number-click", "");
    let buf = draw(&mut app);

    click(&mut app, find_on(&buf, "indent_width", "[ auto ]").unwrap());
    typed(&mut app, "3");
    key(&mut app, KeyCode::Enter);

    assert_eq!(file(&path), "indent_width = 3\n");
}

#[test]
fn r_writes_the_default_back() {
    let (mut app, path) = open("reset", "# note\nwhitespace = \"all\"\n");
    select(&mut app, "whitespace");

    key(&mut app, KeyCode::Char('r'));

    assert_eq!(file(&path), "# note\n");
    assert!(!screen(&draw(&mut app)).contains("\u{25cf}"));
}

#[test]
fn ctrl_z_restores_the_previous_value_and_file() {
    let (mut app, path) = open("undo", COMMENTED);
    select(&mut app, "theme");
    key(&mut app, KeyCode::Right);
    assert_ne!(file(&path), COMMENTED);

    press(&mut app, KeyCode::Char('z'), KeyModifiers::CONTROL);

    assert_eq!(file(&path), COMMENTED);
    let text = screen(&draw(&mut app));
    assert!(text.contains("\u{2039} slate \u{203a}"), "{text}");
}

#[test]
fn the_footer_resets_and_undoes_by_mouse() {
    let (mut app, path) = open("footer", "layout = \"standard\"\n");
    select(&mut app, "layout");
    let buf = draw(&mut app);

    click(&mut app, find_on(&buf, "undo", "reset").unwrap());
    assert_eq!(file(&path), "");
    let buf = draw(&mut app);
    click(&mut app, find_on(&buf, "reset", "undo").unwrap());
    assert_eq!(file(&path), "layout = \"standard\"\n");
}

#[test]
fn ctrl_o_and_its_footer_link_open_the_raw_file_in_an_editor_tab() {
    let (mut app, path) = open("raw", COMMENTED);

    press(&mut app, KeyCode::Char('o'), KeyModifiers::CONTROL);
    assert_eq!(app.editor().path(), Some(path.as_path()));

    app.activate_tab(1);
    let buf = draw(&mut app);
    app.activate_tab(0);
    app.activate_tab(1);
    click(&mut app, find_on(&buf, "undo", "config.toml").unwrap());
    assert_eq!(app.editor().path(), Some(path.as_path()));
}

#[test]
fn enter_on_a_choice_opens_its_list_and_enter_picks() {
    let (mut app, path) = open("list", "");
    select(&mut app, "whitespace");

    key(&mut app, KeyCode::Enter);
    let text = screen(&draw(&mut app));
    assert!(text.contains("trailing"), "the list: {text}");
    key(&mut app, KeyCode::Down);
    key(&mut app, KeyCode::Enter);

    assert_eq!(file(&path), "whitespace = \"all\"\n");
}

#[test]
fn a_setting_that_needs_a_restart_says_so() {
    let (mut app, path) = open("restart", "");
    select(&mut app, "color_depth");

    key(&mut app, KeyCode::Right);

    assert_eq!(file(&path), "color_depth = \"truecolor\"\n");
    assert!(
        app.status().is_some_and(|s| s.contains("next start")),
        "{:?}",
        app.status()
    );
}

#[test]
fn a_click_on_the_value_opens_the_list_and_a_click_on_a_choice_picks_it() {
    let (mut app, path) = open("list-click", "");
    let buf = draw(&mut app);

    click(&mut app, find_on(&buf, "whitespace", "selection").unwrap());
    let buf = draw(&mut app);
    click(&mut app, find_on(&buf, "\u{2502}", "trailing").unwrap());

    assert_eq!(file(&path), "whitespace = \"trailing\"\n");
}

#[test]
fn turning_session_off_in_the_tab_stops_the_quit_writing_one() {
    let (mut app, _) = open("session", "");
    let state = std::env::temp_dir().join("typ-settings-edit-state");
    let session = state.join("project.toml");
    let _ = std::fs::remove_file(&session);
    app.set_session_file(Some(session.clone()));
    select(&mut app, "session");

    key(&mut app, KeyCode::Char(' '));
    app.save_session();

    assert!(!session.exists(), "a session was written with session off");
}
