//! The settings tab, read-only (M4.4 Task 4).
//!
//! Every test but the one that opens it by its key points the tab at a
//! `config.toml` in its own temp directory through `OpenWith`, so none reads
//! or writes a real config. The one that opens it by key sets
//! `TYP_CONFIG_DIR`, the same value for every test in this binary.

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

fn fixture(name: &str, config: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join("typ-settings-tab").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    if !config.is_empty() {
        std::fs::write(&path, config).unwrap();
    }
    (dir, path)
}

fn open(name: &str, config: &str) -> (App, PathBuf) {
    let (dir, path) = fixture(name, config);
    let mut app = App::new(&dir).unwrap();
    // The shipped default: the tab gets the whole width, as it would.
    app.set_preset(typ_app::dock::Preset::Focus);
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
        .expect("the settings tab is not the one on screen")
}

fn draw(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
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

/// Where `text` starts, as (column, row).
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
        press(app, KeyCode::Char(c), KeyModifiers::NONE);
    }
}

fn click(app: &mut App, (column, row): (u16, u16), area: Rect) {
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    typ_app::run::step(app, AppEvent::Input(Event::Mouse(event)), area).unwrap();
}

#[test]
fn ctrl_k_comma_opens_the_settings_tab_on_the_config_file() {
    let (dir, _) = fixture("by-key", "");
    // SAFETY: the only test in this binary that reads the variable, and every
    // other one passes its path explicitly.
    unsafe { std::env::set_var("TYP_CONFIG_DIR", &dir) };
    let mut app = App::new(&dir).unwrap();

    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char(','), KeyModifiers::NONE);

    assert_eq!(app.editor_title(), "Settings");
    assert_eq!(view(&app).path(), dir.join("config.toml").as_path());
}

#[test]
fn the_settings_are_grouped_under_labelled_rules_two_lines_a_row() {
    let (mut app, _) = open("rules", "");

    let buf = draw(&mut app, 120, 30);
    let text = screen(&buf);

    assert!(text.contains("appearance"), "{text}");
    assert!(text.contains("editor \u{203a} indentation"), "{text}");
    let (_, y) = find(&buf, "theme").unwrap_or_else(|| panic!("{text}"));
    let first = line(&buf, y);
    let second = line(&buf, y + 1);
    assert!(first.contains("\u{2039} slate \u{203a}"), "{first}");
    assert!(second.contains("default slate"), "{second}");
    assert!(second.contains("the colour theme"), "{second}");
    let (_, y) = find(&buf, "adapt").unwrap_or_else(|| panic!("{text}"));
    assert!(line(&buf, y).contains("[x]"), "{}", line(&buf, y));
}

#[test]
fn a_changed_value_is_marked_and_says_where_it_was_set() {
    let (mut app, _) = open("modified", "theme = \"mocha\"\n");

    let buf = draw(&mut app, 120, 30);
    let text = screen(&buf);

    let (_, y) = find(&buf, "theme").unwrap_or_else(|| panic!("{text}"));
    assert!(line(&buf, y).contains('\u{25cf}'), "{}", line(&buf, y));
    assert!(
        line(&buf, y + 1).contains("config.toml"),
        "{}",
        line(&buf, y + 1)
    );
    let (_, y) = find(&buf, "whitespace ").unwrap_or_else(|| panic!("{text}"));
    assert!(!line(&buf, y).contains('\u{25cf}'), "{}", line(&buf, y));
}

#[test]
fn at_a_section_filters_to_it() {
    let (mut app, _) = open("at-section", "");

    typed(&mut app, "@editor");
    let text = screen(&draw(&mut app, 120, 30));

    assert_eq!(view(&app).query(), "@editor");
    assert!(text.contains("indent_width"), "{text}");
    assert!(text.contains("whitespace"), "{text}");
    assert!(!text.contains("\u{2039} slate"), "{text}");
}

#[test]
fn colon_modified_filters_to_changed_values() {
    let (mut app, _) = open("modified-filter", "adapt = false\n");

    typed(&mut app, ":modified");
    let text = screen(&draw(&mut app, 120, 30));

    assert!(text.contains("adapt"), "{text}");
    assert!(!text.contains("\u{2039} slate"), "{text}");
    assert!(!text.contains("session"), "{text}");
}

#[test]
fn a_section_column_stands_at_100_columns_and_folds_below() {
    let (mut app, _) = open("column", "");

    let wide = draw(&mut app, 120, 30);
    let narrow = draw(&mut app, 80, 24);

    let (x, _) = find(&wide, "language servers").unwrap_or_else(|| panic!("{}", screen(&wide)));
    assert!(x < 30, "the section column: {}", screen(&wide));
    // Narrow, the sections are only the rules over the list.
    let text = screen(&narrow);
    assert!(text.contains("appearance"), "{text}");
    assert!(
        (0..narrow.area.height).all(|y| !line(&narrow, y).trim_end().ends_with("layout")
            || line(&narrow, y).contains('\u{2500}')),
        "{text}"
    );
}

#[test]
fn a_click_on_a_section_in_the_column_filters_to_it() {
    let (mut app, _) = open("column-click", "");
    let area = Rect::new(0, 0, 120, 30);
    let buf = draw(&mut app, 120, 30);

    click(&mut app, find(&buf, "layout").unwrap(), area);

    assert_eq!(view(&app).query(), "@layout");
}

#[test]
fn a_click_on_a_row_selects_it_and_down_moves_the_selection() {
    let (mut app, _) = open("select", "");
    let area = Rect::new(0, 0, 120, 30);
    let buf = draw(&mut app, 120, 30);

    let (x, y) = find(&buf, "session").unwrap();
    click(&mut app, (x, y + 1), area);
    assert_eq!(view(&app).selected(), Some("session"));

    press(&mut app, KeyCode::Up, KeyModifiers::NONE);
    assert_eq!(view(&app).selected(), Some("adapt"));
}

#[test]
fn the_keyboard_walks_from_the_search_row_into_the_list() {
    let (mut app, _) = open("keys", "");

    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(view(&app).selected(), Some("theme"));
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(view(&app).selected(), Some("color_depth"));
    // Typing goes back to the query, wherever the selection is.
    typed(&mut app, "sess");
    assert_eq!(view(&app).query(), "sess");
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    assert_eq!(view(&app).selected(), Some("session"));
}

#[test]
fn a_long_list_scrolls_to_keep_the_selection_on_screen() {
    let (mut app, _) = open("scroll", "");
    draw(&mut app, 80, 12);

    for _ in 0..8 {
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    }
    let buf = draw(&mut app, 80, 12);

    assert_eq!(view(&app).selected(), Some("language"));
    assert!(find(&buf, "language").is_some(), "{}", screen(&buf));
}
