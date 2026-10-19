//! Layout presets (interface §7): Focus folds the dock, Standard opens it.
//! Chosen in `config.toml`, cycled by `ctrl+k l`, the palette, or the `▣`
//! segment on the status bar.

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use typ_app::config::load_settings;
use typ_app::dock::Preset;
use typ_app::{App, Focus};
use typ_core::{AppEvent, KeyChord};

const AREA: Rect = Rect::new(0, 0, 120, 30);

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-presets").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.rs"), "fn main() {}\n").unwrap();
    dir
}

fn app(name: &str) -> App {
    let dir = dir(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("main.rs")).unwrap();
    app
}

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    app.handle_chord(KeyChord::from_event(KeyEvent::new(code, mods)))
        .unwrap();
}

fn ctrl_k_l(app: &mut App) {
    press(app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    press(app, KeyCode::Char('l'), KeyModifiers::NONE);
}

fn status_row(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..AREA.width)
        .map(|x| buffer[(x, AREA.height - 1)].symbol())
        .collect()
}

fn settings_from(name: &str, toml: &str) -> (typ_app::config::Settings, Option<String>) {
    let path = dir(name).join("config.toml");
    std::fs::write(&path, toml).unwrap();
    load_settings(Some(&path))
}

#[test]
fn layout_focus_in_config_starts_folded() {
    let (settings, warning) = settings_from("config-focus", "layout = \"focus\"\n");
    assert_eq!(warning, None);
    assert_eq!(settings.layout, Preset::Focus);

    let mut app = App::new(&dir("config-focus")).unwrap();
    app.set_preset(settings.layout);

    assert!(!app.dock_open());
    assert_eq!(
        app.focus(),
        Focus::Editor,
        "a folded dock cannot hold focus"
    );
    assert_eq!(app.preset(), Preset::Focus);
}

#[test]
fn layout_standard_in_config_starts_open() {
    let (settings, _) = settings_from("config-standard", "layout = \"standard\"\n");
    let mut app = App::new(&dir("config-standard")).unwrap();
    app.set_preset(settings.layout);
    assert!(app.dock_open());
}

#[test]
fn an_unknown_layout_warns_and_falls_back() {
    let (settings, warning) = settings_from("config-wide", "layout = \"wide\"\n");
    let warning = warning.expect("an unknown preset said nothing");
    assert!(warning.contains("layout"), "{warning}");
    assert_eq!(settings.layout, typ_app::config::Settings::default().layout);
}

#[test]
fn ctrl_k_l_cycles_the_preset() {
    let mut app = app("cycle");
    assert!(app.dock_open());

    ctrl_k_l(&mut app);
    assert_eq!(app.preset(), Preset::Focus);
    assert!(!app.dock_open());

    ctrl_k_l(&mut app);
    assert_eq!(app.preset(), Preset::Standard);
    assert!(app.dock_open());
}

#[test]
fn the_status_bar_names_the_preset() {
    let mut app = app("segment");
    assert!(status_row(&mut app).contains("\u{25a3} standard"));
    ctrl_k_l(&mut app);
    assert!(status_row(&mut app).contains("\u{25a3} focus"));
}

#[test]
fn a_click_on_the_segment_cycles() {
    let mut app = app("click");
    let row = status_row(&mut app);
    let column = row.chars().position(|c| c == '\u{25a3}').unwrap() as u16;

    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row: AREA.height - 1,
        modifiers: KeyModifiers::NONE,
    };
    typ_app::run::step(&mut app, AppEvent::Input(Event::Mouse(click)), AREA).unwrap();

    assert_eq!(app.preset(), Preset::Focus);
    assert!(!app.dock_open());
}

#[test]
fn enter_on_the_segment_cycles() {
    let mut app = app("enter");
    app.set_focus(Focus::Editor);
    status_row(&mut app);
    press(&mut app, KeyCode::F(6), KeyModifiers::NONE);
    assert_eq!(app.focus(), Focus::Status);
    // The preset is the last segment that does something.
    for _ in 0..4 {
        press(&mut app, KeyCode::Right, KeyModifiers::NONE);
    }
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert_eq!(app.preset(), Preset::Focus);
}

// Adapt to width (interface §7): under 100 columns Standard renders as Focus.

fn resize_to(app: &mut App, width: u16) {
    let area = Rect::new(0, 0, width, 30);
    typ_app::run::step(app, AppEvent::Input(Event::Resize(width, 30)), area).unwrap();
}

#[test]
fn a_narrow_terminal_folds_standard_and_a_wide_one_reopens_it() {
    let mut app = app("adapt");
    app.set_focus(Focus::Editor);

    resize_to(&mut app, 90);
    assert!(!app.dock_open(), "90 columns kept the dock open");
    assert_eq!(app.preset(), Preset::Standard, "adapting is not a choice");

    resize_to(&mut app, 120);
    assert!(app.dock_open(), "120 columns left the dock folded");
    assert_eq!(app.focus(), Focus::Editor, "reopening took the keys");
}

#[test]
fn a_preset_picked_by_hand_wins_over_the_width() {
    let mut app = app("adapt-picked");
    app.set_focus(Focus::Editor);
    ctrl_k_l(&mut app);
    ctrl_k_l(&mut app);
    assert!(app.dock_open());

    resize_to(&mut app, 90);
    assert!(app.dock_open(), "the resize overrode ctrl+k l");
}

#[test]
fn ctrl_b_on_a_narrow_terminal_is_a_choice_too() {
    let mut app = app("adapt-ctrl-b");
    app.set_focus(Focus::Editor);
    resize_to(&mut app, 90);
    assert!(!app.dock_open());

    press(&mut app, KeyCode::Char('b'), KeyModifiers::CONTROL);
    assert!(app.dock_open());
    resize_to(&mut app, 91);
    assert!(app.dock_open(), "the next resize folded what ctrl+b opened");
}

#[test]
fn adapt_false_in_config_leaves_the_layout_alone() {
    let (settings, warning) = settings_from("adapt-off", "adapt = false\n");
    assert_eq!(warning, None);
    assert!(!settings.adapt);
    assert!(
        typ_app::config::Settings::default().adapt,
        "adapt is on by default"
    );

    let mut app = app("adapt-off");
    app.set_adapt(settings.adapt);
    resize_to(&mut app, 90);
    assert!(app.dock_open());
}

#[test]
fn a_fresh_config_starts_in_focus() {
    let config = std::env::temp_dir().join("typ-presets-fresh-config");
    let _ = std::fs::remove_dir_all(&config);
    std::fs::create_dir_all(&config).unwrap();
    // SAFETY: the only test in this binary that reads the variable.
    unsafe { std::env::set_var("TYP_CONFIG_DIR", &config) };
    let (settings, warning) = load_settings(typ_app::config::settings_path().as_deref());
    assert_eq!(warning, None);

    let mut app = app("fresh");
    app.set_preset(settings.layout);
    assert_eq!(app.preset(), Preset::Focus);
    assert!(
        !app.dock_open(),
        "the shipped default is Focus (interface §7)"
    );
}
