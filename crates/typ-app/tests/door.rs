//! `ctrl+k` is a door: a prefix whose second key picks from everything bound
//! under it. `controls.md` §2, `interface.md` §6.

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_app::{App, Focus};
use typ_core::{Action, AppEvent, KeyChord, Keymap, ThemeColors};
use typ_picker::Mode;

fn app(name: &str) -> App {
    let dir: PathBuf = std::env::temp_dir().join("typ-door").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.rs"), "fn main() {}\n").unwrap();
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("main.rs")).unwrap();
    app
}

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    app.handle_chord(KeyChord::from_event(KeyEvent::new(code, mods)))
        .unwrap();
}

fn ctrl_k(app: &mut App) {
    press(app, KeyCode::Char('k'), KeyModifiers::CONTROL);
}

fn key(app: &mut App, c: char) {
    press(app, KeyCode::Char(c), KeyModifiers::NONE);
}

#[test]
fn ctrl_k_then_f_opens_project_search() {
    let mut app = app("f");
    ctrl_k(&mut app);
    assert_eq!(app.pending_prefix(), Some("ctrl+k"));

    key(&mut app, 'f');

    assert_eq!(app.picker().map(|p| p.mode()), Some(Mode::Search));
    assert_eq!(app.pending_prefix(), None);
    assert_eq!(app.editor().line_text(0), "fn main() {}", "f was typed");
}

#[test]
fn ctrl_k_then_an_unbound_key_cancels_and_says_so() {
    let mut app = app("q");
    ctrl_k(&mut app);

    key(&mut app, 'q');

    assert!(!app.should_quit(), "q under the prefix quit");
    assert_eq!(app.pending_prefix(), None);
    assert_eq!(app.status(), Some("ctrl+k q is not bound"));
    assert_eq!(app.editor().line_text(0), "fn main() {}", "q was typed");
}

#[test]
fn ctrl_k_then_esc_leaves_nothing_pending() {
    let mut app = app("esc");
    ctrl_k(&mut app);

    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);

    assert_eq!(app.pending_prefix(), None);
    assert_eq!(app.status(), None, "esc cancels silently");
    // The next key is text again.
    key(&mut app, 'f');
    assert_eq!(app.editor().line_text(0), "ffn main() {}");
}

#[test]
fn the_global_layer_answers_from_any_panel() {
    // Lookup order is the focused panel's rows, then global (`controls.md` §3).
    // No panel has rows of its own yet, so from the tree the global row is
    // what answers.
    let mut app = app("layer");
    app.cycle_focus();
    assert_eq!(app.focus(), Focus::Tree);

    ctrl_k(&mut app);
    key(&mut app, 'f');

    assert_eq!(app.picker().map(|p| p.mode()), Some(Mode::Search));
}

#[test]
fn a_paste_abandons_a_pending_prefix() {
    let mut app = app("paste");
    ctrl_k(&mut app);

    app.handle_paste("x".into()).unwrap();

    assert_eq!(app.pending_prefix(), None);
}

// --- the menu (interface.md §6) ----------------------------------------------

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

fn draw(app: &mut App, area: Rect) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
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
        .join(
            "
",
        )
}

/// Where `text` starts on screen, as (column, row).
fn find(buf: &Buffer, text: &str) -> Option<(u16, u16)> {
    (0..buf.area.height).find_map(|y| {
        let row = line(buf, y);
        let byte = row.find(text)?;
        // Every cell here is one column, so the char count before it is the x.
        Some((row[..byte].chars().count() as u16, y))
    })
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

fn with_keys(app: &mut App, toml: &str) {
    let mut keymap = Keymap::default_bindings();
    keymap.merge_toml(toml).unwrap();
    app.set_keymap(keymap);
}

#[test]
fn ctrl_k_docks_a_menu_above_the_status_bar() {
    let theme = ThemeColors::default();
    let mut app = app("menu-drawn");
    let editor_before = app.areas(AREA).1;
    ctrl_k(&mut app);

    let buf = draw(&mut app, AREA);

    let (_, row) =
        find(&buf, "f  search the project").unwrap_or_else(|| panic!("{}", screen(&buf)));
    assert!(find(&buf, "find").is_some(), "no group heading");
    // The menu's last row is the one directly above the status bar.
    assert!(line(&buf, 22).contains("esc cancels"), "{}", screen(&buf));
    assert!(row < 22);
    for x in [0, 40, 79] {
        assert_eq!(buf[(x, 22)].bg, theme.chrome_bg, "menu cell {x},22");
        assert_eq!(buf[(x, row)].bg, theme.chrome_bg, "menu cell {x},{row}");
    }
    // The body gave up the rows; it did not move sideways.
    let editor = app.areas(AREA).1;
    assert_eq!(editor.x, editor_before.x);
    assert_eq!(editor.width, editor_before.width);
    assert!(editor.bottom() <= row, "{editor:?} runs under the menu");
}

#[test]
fn the_menu_is_gone_once_its_action_runs() {
    let mut app = app("menu-gone");
    ctrl_k(&mut app);
    key(&mut app, 'f');
    app.close_picker();

    let buf = draw(&mut app, AREA);

    assert!(!screen(&buf).contains("esc cancels"));
}

#[test]
fn a_click_on_a_row_runs_it() {
    let mut app = app("menu-click");
    ctrl_k(&mut app);
    let buf = draw(&mut app, AREA);
    let (x, y) = find(&buf, "search the project").unwrap();

    click(&mut app, (x + 3, y));

    assert_eq!(app.picker().map(|p| p.mode()), Some(Mode::Search));
    assert_eq!(app.pending_prefix(), None);
}

#[test]
fn a_click_elsewhere_cancels() {
    let mut app = app("menu-click-away");
    ctrl_k(&mut app);
    draw(&mut app, AREA);

    click(&mut app, (40, 5));

    assert_eq!(app.pending_prefix(), None);
    assert!(app.picker().is_none());
}

#[test]
fn arrows_move_the_highlight_and_enter_runs_it() {
    let theme = ThemeColors::default();
    let mut app = app("menu-arrows");
    with_keys(&mut app, "\"ctrl+k g\" = \"goto_line\"");
    ctrl_k(&mut app);

    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    let buf = draw(&mut app, AREA);
    let (x, y) = find(&buf, "g  go to a line number").unwrap_or_else(|| panic!("{}", screen(&buf)));
    assert_eq!(buf[(x, y)].bg, theme.selection_primary_bg);
    assert_eq!(app.pending_prefix(), Some("ctrl+k"), "an arrow ended it");

    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);

    assert!(app.prompt().is_some(), "enter did not run goto_line");
    assert_eq!(app.pending_prefix(), None);
}

#[test]
fn a_rebind_changes_the_row() {
    let mut app = app("menu-rebind");
    with_keys(
        &mut app,
        "\"ctrl+k f\" = \"\"
\"ctrl+k s\" = \"open_project_search\"",
    );
    ctrl_k(&mut app);

    let buf = draw(&mut app, AREA);

    assert!(
        find(&buf, "s  search the project").is_some(),
        "{}",
        screen(&buf)
    );
    assert!(find(&buf, "f  search the project").is_none());
}

/// Enough rows in enough groups that 80 columns cannot hold them in one band.
const MANY: &str = "
\"ctrl+k a\" = \"select_all\"
\"ctrl+k b\" = \"select_line\"
\"ctrl+k c\" = \"copy\"
\"ctrl+k e\" = \"select_all_occurrences\"
\"ctrl+k g\" = \"goto_line\"
\"ctrl+k i\" = \"indent\"
\"ctrl+k j\" = \"next_tab\"
\"ctrl+k k\" = \"prev_tab\"
\"ctrl+k l\" = \"select_next_occurrence\"
\"ctrl+k n\" = \"search_next\"
\"ctrl+k o\" = \"open_file_picker\"
\"ctrl+k s\" = \"save\"
\"ctrl+k t\" = \"focus_next\"
\"ctrl+k u\" = \"outdent\"
\"ctrl+k v\" = \"paste\"
\"ctrl+k x\" = \"cut\"
\"ctrl+k y\" = \"redo\"
\"ctrl+k z\" = \"undo\"
\"ctrl+k 1\" = \"go_to_tab_1\"
\"ctrl+k 2\" = \"go_to_tab_2\"
\"ctrl+k 3\" = \"go_to_tab_3\"
";

#[test]
fn a_tall_menu_is_capped_at_a_third_and_the_arrows_reach_the_rest() {
    let mut app = app("menu-many");
    with_keys(&mut app, MANY);
    ctrl_k(&mut app);

    let buf = draw(&mut app, AREA);
    let text = screen(&buf);
    // A third of 24 rows, and the body keeps the rest.
    assert!(app.areas(AREA).1.bottom() >= 24 - 1 - 8, "{text}");
    assert!(line(&buf, 22).contains("more"), "{text}");
    assert!(!text.contains("switch to tab 3"), "{text}");

    // Every row comes on screen as the highlight walks down to it.
    let mut seen = text;
    for _ in 0..30 {
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        seen.push_str(&screen(&draw(&mut app, AREA)));
    }
    for (_, action) in app.keymap().under("ctrl+k") {
        assert!(
            seen.contains(action.description()),
            "{action:?} never shown"
        );
    }
}

#[test]
fn the_palette_opens_the_menu_for_a_terminal_that_eats_ctrl_k() {
    let mut app = app("menu-palette");
    app.apply_named_action(Action::OpenMenu).unwrap();
    assert_eq!(app.pending_prefix(), Some("ctrl+k"));
}

// --- the teaching line (interface.md §6, step 3) -----------------------------

const TEACHING: &str = "ctrl+k f \u{b7} search the project";

fn resize() -> AppEvent {
    AppEvent::Input(Event::Resize(80, 24))
}

#[test]
fn running_a_row_from_the_menu_teaches_its_chord() {
    let mut app = app("teach-key");
    ctrl_k(&mut app);
    key(&mut app, 'f');
    assert_eq!(app.status_left(), TEACHING);
}

#[test]
fn enter_and_a_click_teach_the_chord_too() {
    let mut app = app("teach-enter");
    ctrl_k(&mut app);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.status_left(), TEACHING);

    let mut app = self::app("teach-click");
    ctrl_k(&mut app);
    let buf = draw(&mut app, AREA);
    click(&mut app, find(&buf, "search the project").unwrap());
    assert_eq!(app.status_left(), TEACHING);
}

#[test]
fn a_direct_chord_teaches_nothing() {
    let mut app = app("teach-direct");
    press(
        &mut app,
        KeyCode::Char('F'),
        KeyModifiers::CONTROL | KeyModifiers::SHIFT,
    );
    assert_eq!(app.picker().map(|p| p.mode()), Some(Mode::Search));
    assert!(
        !app.status_left().contains("search the project"),
        "{}",
        app.status_left()
    );
}

#[test]
fn the_teaching_line_goes_with_the_first_event_after_two_seconds() {
    let mut app = app("teach-expire");
    let start = std::time::Instant::now();
    ctrl_k(&mut app);
    key(&mut app, 'f');

    typ_app::run::step_at(&mut app, resize(), AREA, start).unwrap();
    assert_eq!(app.status_left(), TEACHING, "gone before its time");

    let later = start + std::time::Duration::from_secs(3);
    typ_app::run::step_at(&mut app, resize(), AREA, later).unwrap();
    assert!(!app.status_left().contains("search the project"));
}

#[test]
fn the_teaching_line_does_not_wake_an_idle_editor() {
    // It clears on the next event after its deadline. Nothing sets a timer for
    // it, so a user who walks away does not cost a wakeup.
    let mut app = app("teach-idle");
    assert_eq!(app.wake_deadline(), None);
    ctrl_k(&mut app);
    key(&mut app, 'f');
    assert_eq!(app.status_left(), TEACHING);
    assert_eq!(app.wake_deadline(), None);
}
