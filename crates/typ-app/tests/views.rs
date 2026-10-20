//! A tab that holds a view rather than an editor (M4.4 Task 1).
//!
//! The view here draws its name and counts the keys it was given. It opens
//! through `OpenWith` and the registry, as invariant 6 says every new panel
//! does, and every editor-only path is shown to skip it.

use std::any::Any;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_app::App;
use typ_core::{Action, HandlerId, KeyChord, Panel, PanelEvent, RenderContext};

const PROBE: HandlerId = HandlerId("probe");

struct Probe {
    keys: usize,
}

impl Panel for Probe {
    fn name(&self) -> &'static str {
        "probe"
    }

    fn title(&self) -> String {
        format!("Probe {}", self.keys)
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, _ctx: &RenderContext) {
        buf.set_string(
            area.x,
            area.y,
            "probe drawn",
            ratatui::style::Style::default(),
        );
    }

    fn handle_key(&mut self, _chord: KeyChord) -> Vec<PanelEvent> {
        self.keys += 1;
        vec![PanelEvent::NeedsRedraw]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

fn probe(_path: &Path) -> Box<dyn Panel> {
    Box::new(Probe { keys: 0 })
}

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-views").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "fn main() {}\n").unwrap();
    dir
}

fn app_with_probe(dir: &Path) -> App {
    let mut app = App::new(dir).unwrap();
    app.registry_mut().register_view(PROBE, probe);
    app
}

fn open_probe(app: &mut App, dir: &Path) {
    app.apply(vec![PanelEvent::OpenWith {
        handler: PROBE,
        path: dir.join("probe.view"),
    }])
    .unwrap();
}

fn screen(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn press(app: &mut App, code: KeyCode) {
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        code,
        KeyModifiers::NONE,
    )))
    .unwrap();
}

#[test]
fn open_with_a_registered_view_puts_up_a_tab_titled_by_the_view() {
    let dir = fixture("open");
    let mut app = app_with_probe(&dir);
    app.open_path(&dir.join("a.rs")).unwrap();

    open_probe(&mut app, &dir);

    assert_eq!(app.tab_count(), 2);
    assert_eq!(app.editor_title(), "Probe 0");
    assert!(app.shown_editor().is_none());
    let text = screen(&mut app);
    assert!(text.contains("probe drawn"), "{text}");
    assert!(text.contains("Probe 0"), "the tab row: {text}");
}

#[test]
fn opening_the_same_view_twice_switches_to_it() {
    let dir = fixture("twice");
    let mut app = app_with_probe(&dir);
    app.open_path(&dir.join("a.rs")).unwrap();
    open_probe(&mut app, &dir);
    app.activate_tab(0);

    open_probe(&mut app, &dir);

    assert_eq!(app.tab_count(), 2);
    assert_eq!(app.active_tab(), 1);
}

#[test]
fn the_view_takes_keys_and_closes() {
    let dir = fixture("keys");
    let mut app = app_with_probe(&dir);
    app.open_path(&dir.join("a.rs")).unwrap();
    open_probe(&mut app, &dir);

    press(&mut app, KeyCode::Char('x'));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.editor_title(), "Probe 2");

    app.apply_named_action(Action::CloseTab).unwrap();
    assert_eq!(app.tab_count(), 1);
    assert_eq!(app.editor().file_name(), "a.rs");
}

#[test]
fn a_save_on_a_view_says_there_is_nothing_to_save() {
    let dir = fixture("save");
    let mut app = app_with_probe(&dir);
    open_probe(&mut app, &dir);

    app.apply_named_action(Action::Save).unwrap();

    assert!(
        app.status().is_some_and(|s| s.contains("no file")),
        "{:?}",
        app.status()
    );
}

#[test]
fn quitting_with_a_view_and_a_clean_editor_open_does_not_ask() {
    let dir = fixture("quit");
    let mut app = app_with_probe(&dir);
    app.open_path(&dir.join("a.rs")).unwrap();
    open_probe(&mut app, &dir);

    app.apply_named_action(Action::Quit).unwrap();

    assert!(app.should_quit());
}

#[test]
fn a_session_with_a_view_open_restores_without_it() {
    let dir = fixture("session");
    let mut app = app_with_probe(&dir);
    app.open_path(&dir.join("a.rs")).unwrap();
    open_probe(&mut app, &dir);

    let session = app.session();

    let tabs: Vec<&Path> = session.panes[0]
        .tabs
        .iter()
        .map(|t| t.path.as_path())
        .collect();
    assert_eq!(tabs, vec![dir.join("a.rs").as_path()]);
    assert_eq!(session.panes[0].active, 0);
}

#[test]
fn a_handler_with_no_view_still_says_it_has_none() {
    let dir = fixture("none");
    let mut app = App::new(&dir).unwrap();

    app.apply(vec![PanelEvent::OpenWith {
        handler: HandlerId("nothing"),
        path: dir.join("x.view"),
    }])
    .unwrap();

    assert_eq!(app.tab_count(), 1);
    assert!(
        app.status().is_some_and(|s| s.contains("nothing")),
        "{:?}",
        app.status()
    );
}

#[test]
fn the_status_bar_names_the_view_and_shows_no_cursor_position() {
    let dir = fixture("status");
    let mut app = app_with_probe(&dir);
    open_probe(&mut app, &dir);
    screen(&mut app);

    let right = app.status_right();

    assert!(right.contains("Probe 0"), "{right}");
    assert!(!right.contains("1:1"), "{right}");
    assert!(
        right.contains('\u{25a3}'),
        "the layout segment stays: {right}"
    );
}

#[test]
fn the_context_menu_on_a_view_lists_tab_actions_not_editing_ones() {
    let dir = fixture("context");
    let mut app = app_with_probe(&dir);
    open_probe(&mut app, &dir);
    app.set_focus(typ_app::app::Focus::Editor);

    app.apply_named_action(Action::ContextMenu).unwrap();

    let rows = app.context_rows().expect("a menu");
    assert!(rows.contains(&Action::CloseTab), "{rows:?}");
    assert!(!rows.contains(&Action::Copy), "{rows:?}");
}
