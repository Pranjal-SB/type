//! The keybindings tab, read-only (M4.5 Task 2).
//!
//! Every test but the one that opens it by its key points the tab at a
//! `keys.toml` in its own temp directory through `OpenWith`, so none reads or
//! writes a real config. The one that opens it by key sets `TYP_CONFIG_DIR`.

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

fn fixture(name: &str, keys: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join("typ-keys-tab").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("keys.toml");
    if !keys.is_empty() {
        std::fs::write(&path, keys).unwrap();
    }
    (dir, path)
}

fn open(name: &str, keys: &str) -> App {
    let (dir, path) = fixture(name, keys);
    let mut app = App::new(&dir).unwrap();
    app.set_preset(typ_app::dock::Preset::Focus);
    app.apply(vec![PanelEvent::OpenWith {
        handler: KEYBINDINGS,
        path,
    }])
    .unwrap();
    app
}

fn view(app: &App) -> &KeysView {
    app.shown_panel()
        .as_any()
        .downcast_ref::<KeysView>()
        .expect("the keybindings tab is not the one on screen")
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

fn shown(app: &App) -> Vec<(Option<String>, Action)> {
    view(app).visible()
}

#[test]
fn ctrl_k_k_opens_the_keybindings_tab_on_keys_toml() {
    let (dir, _) = fixture("by-key", "");
    // SAFETY: the only test in this binary that reads the variable.
    unsafe { std::env::set_var("TYP_CONFIG_DIR", &dir) };
    let mut app = App::new(&dir).unwrap();

    press(&mut app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('k'), KeyModifiers::NONE);

    assert_eq!(app.editor_title(), "Keybindings");
    assert_eq!(view(&app).path(), dir.join("keys.toml").as_path());
}

#[test]
fn a_row_says_the_action_its_key_as_the_hint_spells_it_the_layer_and_the_source() {
    let mut app = open("row", "\"alt+s\" = \"save\"\n");
    typed(&mut app, "save");

    let text = screen(&draw(&mut app, 100, 24));

    let default = text
        .lines()
        .find(|l| l.contains("Ctrl+S"))
        .unwrap_or_else(|| panic!("no Ctrl+S row:\n{text}"));
    assert!(default.contains("save the file"), "{default}");
    assert!(default.contains("global"), "{default}");
    assert!(default.contains("default"), "{default}");
    let user = text
        .lines()
        .find(|l| l.contains("Alt+S"))
        .unwrap_or_else(|| panic!("no Alt+S row:\n{text}"));
    assert!(user.contains("save the file"), "{user}");
    assert!(user.contains("user"), "{user}");
    assert!(
        user.contains('\u{25cf}'),
        "a user row is marked modified: {user}"
    );
}

#[test]
fn the_rows_sit_under_a_labelled_rule_per_group() {
    let mut app = open("rules", "");

    let text = screen(&draw(&mut app, 100, 24));

    let rule = text
        .lines()
        .find(|l| l.contains(" edit "))
        .unwrap_or_else(|| panic!("no edit rule:\n{text}"));
    assert!(rule.contains('\u{2500}'), "{rule}");
    assert!(text.contains("move the cursor left"), "{text}");
}

#[test]
fn unbound_lists_exactly_the_actions_with_no_binding() {
    let mut app = open("unbound", "\"ctrl+d\" = \"\"\n");
    typed(&mut app, ":unbound");

    let keymap = typ_app::config::load_keymap(Some(view(&app).path())).0;
    let mut want: Vec<Action> = Action::ALL
        .iter()
        .copied()
        .filter(|a| keymap.bindings_for(*a).is_empty())
        .collect();
    let mut got: Vec<Action> = shown(&app).into_iter().map(|(_, a)| a).collect();
    want.sort_by_key(|a| a.name());
    got.sort_by_key(|a| a.name());
    assert_eq!(got, want);
    assert!(got.contains(&Action::SelectNextOccurrence), "{got:?}");
    assert!(shown(&app).iter().all(|(chord, _)| chord.is_none()));
}

#[test]
fn modified_keeps_the_rows_keys_toml_set() {
    let mut app = open("modified", "\"alt+s\" = \"save\"\n");
    typed(&mut app, ":modified");

    assert_eq!(shown(&app), vec![(Some("alt+s".to_string()), Action::Save)]);
}

#[test]
fn conflict_keeps_a_user_row_that_took_a_default_chord_and_marks_it() {
    let mut app = open("conflict", "\"ctrl+d\" = \"save\"\n");
    typed(&mut app, ":conflict");

    assert_eq!(
        shown(&app),
        vec![(Some("ctrl+d".to_string()), Action::Save)]
    );
    let text = screen(&draw(&mut app, 100, 24));
    assert!(text.contains('\u{25b2}'), "{text}");
}

#[test]
fn kitty_keeps_the_chords_that_need_the_protocol() {
    let mut app = open("kitty", "");
    typed(&mut app, ":kitty");

    let rows = shown(&app);
    assert!(rows.contains(&(Some("ctrl+shift+f".to_string()), Action::OpenProjectSearch)));
    assert!(!rows.iter().any(|(c, _)| c.as_deref() == Some("ctrl+f")));
    let text = screen(&draw(&mut app, 100, 24));
    let row = text
        .lines()
        .find(|l| l.contains("Ctrl+Shift+F"))
        .unwrap_or_else(|| panic!("no Ctrl+Shift+F row:\n{text}"));
    assert!(row.contains("kitty"), "{row}");
}

#[test]
fn plain_text_matches_the_action_or_the_key() {
    let mut app = open("text", "");
    typed(&mut app, "occurrence");
    let actions: Vec<Action> = shown(&app).into_iter().map(|(_, a)| a).collect();
    assert_eq!(
        actions,
        vec![Action::SelectNextOccurrence, Action::SelectAllOccurrences]
    );

    let mut app = open("text-key", "");
    typed(&mut app, "ctrl+k z");
    assert_eq!(
        shown(&app),
        vec![(Some("ctrl+k z".to_string()), Action::Zoom)]
    );
}

#[test]
fn ctrl_r_then_a_key_filters_to_what_that_key_does() {
    let mut app = open("ctrl-r", "");

    press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('d'), KeyModifiers::CONTROL);

    assert_eq!(
        shown(&app),
        vec![(Some("ctrl+d".to_string()), Action::SelectNextOccurrence)]
    );
}

#[test]
fn ctrl_r_takes_a_key_the_keymap_would_have_run() {
    let mut app = open("ctrl-r-quit", "");

    press(&mut app, KeyCode::Char('r'), KeyModifiers::CONTROL);
    press(&mut app, KeyCode::Char('q'), KeyModifiers::CONTROL);

    assert!(
        !app.should_quit(),
        "ctrl+q quit instead of being searched for"
    );
    assert_eq!(
        shown(&app),
        vec![(Some("ctrl+q".to_string()), Action::Quit)]
    );
}

#[test]
fn a_click_on_a_row_selects_it() {
    let mut app = open("click", "");
    typed(&mut app, "save");
    let buf = draw(&mut app, 100, 24);
    let at = find(&buf, "save the file").expect("no save row");

    click(&mut app, at, buf.area);

    assert_eq!(
        view(&app).selected(),
        Some((Some("ctrl+s".to_string()), Action::Save))
    );
}

#[test]
fn a_click_on_the_footers_ctrl_r_waits_for_a_key_as_the_chord_does() {
    let mut app = open("click-ctrl-r", "");
    let buf = draw(&mut app, 100, 24);
    let at = find(&buf, "ctrl+r").expect("no ctrl+r in the footer");

    click(&mut app, at, buf.area);
    press(&mut app, KeyCode::Char('d'), KeyModifiers::CONTROL);

    assert_eq!(
        shown(&app),
        vec![(Some("ctrl+d".to_string()), Action::SelectNextOccurrence)]
    );
}
