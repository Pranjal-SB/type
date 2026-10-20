//! Warp labels (interface §5): `ctrl+k j` puts a two-letter label on every
//! mouse target, the region's letter then its place in screen order, and
//! typing one clicks that target. The keyboard path is the mouse path.

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::app::targets::TargetKind;
use typ_app::prompt::PromptKind;
use typ_app::run::step;
use typ_app::status::SegmentId;
use typ_core::{AppEvent, ThemeColors};

const FRAME: Rect = Rect {
    x: 0,
    y: 0,
    width: 120,
    height: 30,
};

/// One pane holding a.rs, b.rs and c.rs, c.rs active, the dock open.
fn app(name: &str) -> App {
    let dir: PathBuf = std::env::temp_dir().join("typ-warp").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = App::new(&dir).unwrap();
    for file in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(dir.join(file), "fn main() {}\n").unwrap();
        app.open_path(&dir.join(file)).unwrap();
    }
    draw(&mut app);
    app
}

fn draw(app: &mut App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(FRAME.width, FRAME.height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    terminal.backend().buffer().clone()
}

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    let event = AppEvent::Input(Event::Key(KeyEvent::new(code, mods)));
    step(app, event, FRAME).unwrap();
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        press(app, KeyCode::Char(c), KeyModifiers::NONE);
    }
}

fn warp(app: &mut App) {
    press(app, KeyCode::Char('k'), KeyModifiers::CONTROL);
    typed(app, "j");
}

#[test]
fn ctrl_k_j_then_t2_shows_the_second_tab() {
    let mut app = app("t2");
    assert_eq!(app.active_tab(), 2);
    warp(&mut app);
    assert!(app.warp_labels().is_some(), "ctrl+k j put up no labels");

    typed(&mut app, "t2");

    assert_eq!(app.active_tab(), 1);
    assert!(app.warp_labels().is_none(), "the labels stayed up");
}

#[test]
fn b1_does_what_the_first_status_segment_does() {
    let mut app = app("b1");
    warp(&mut app);
    let labels = app.warp_labels().unwrap();
    let (_, first) = labels.iter().find(|(label, _)| label == "b1").unwrap();
    assert_eq!(first.kind, TargetKind::Segment(SegmentId::Position));

    typed(&mut app, "b1");

    assert_eq!(app.prompt().map(|p| p.kind()), Some(PromptKind::GotoLine));
}

#[test]
fn the_labels_on_screen_are_the_targets() {
    let mut app = app("drawn");
    let targets = app.targets(FRAME);
    warp(&mut app);
    let labels = app.warp_labels().unwrap().to_vec();
    assert_eq!(labels.len(), targets.len());
    for target in &targets {
        assert!(
            labels.iter().any(|(_, t)| t == target),
            "{target:?} has no label"
        );
    }

    let buf = draw(&mut app);
    let theme = ThemeColors::default();
    for (label, target) in &labels {
        assert!(label.starts_with(target.region()), "{label} on {target:?}");
        let shown: String = (0..label.len() as u16)
            .map(|dx| {
                buf[(target.rect.x + dx, target.rect.y)]
                    .symbol()
                    .to_string()
            })
            .collect();
        assert_eq!(&shown, label, "{target:?}");
        let cell = &buf[(target.rect.x, target.rect.y)];
        assert_eq!(cell.fg, theme.status_bar_accent, "{label}");
        assert_eq!(cell.bg, theme.chrome_bg, "{label}");
    }
    // The rest of the body steps back, as behind the picker.
    let editor = app.areas(FRAME).1;
    assert_eq!(buf[(editor.x + 5, editor.y + 3)].bg, theme.scrim);
}

#[test]
fn labels_count_in_screen_order_within_a_region() {
    let mut app = app("order");
    warp(&mut app);
    let labels = app.warp_labels().unwrap();
    let tabs: Vec<&str> = labels
        .iter()
        .filter(|(l, _)| l.starts_with('t'))
        .map(|(l, _)| l.as_str())
        .collect();
    // Three tabs and the zoom button.
    assert_eq!(tabs, ["t1", "t2", "t3", "t4"]);
}

#[test]
fn esc_leaves_nothing_pending() {
    let mut app = app("esc");
    warp(&mut app);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert!(app.warp_labels().is_none());
    assert_eq!(app.pending_prefix(), None);
    // The next key is text again.
    typed(&mut app, "x");
    assert_eq!(app.editor().line_text(0), "xfn main() {}");
}

#[test]
fn a_label_that_names_nothing_cancels_and_says_so() {
    let mut app = app("mismatch");
    warp(&mut app);
    typed(&mut app, "t9");
    assert!(app.warp_labels().is_none());
    assert_eq!(app.active_tab(), 2, "something was clicked");
    assert!(app.status_left().contains("t9"), "{}", app.status_left());
}

#[test]
fn a_click_while_the_labels_are_up_puts_them_away() {
    let mut app = app("click");
    warp(&mut app);
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 60,
        row: 10,
        modifiers: KeyModifiers::NONE,
    };
    step(&mut app, AppEvent::Input(Event::Mouse(event)), FRAME).unwrap();
    assert!(app.warp_labels().is_none());
}
