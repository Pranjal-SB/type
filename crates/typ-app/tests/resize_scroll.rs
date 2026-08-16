//! Two defects that are the same defect: the loop's input handling loses
//! things.
//!
//! Defect 10, resize, was harmless only while the loop repainted every pass.
//! Task 3 stopped doing that, so an unhandled resize is now a frozen screen.
//! And the scroll coalescing that Task 1 removed rather than ported used to
//! drop any event that was not a scroll.

use std::path::PathBuf;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::{Backend, TestBackend};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::run::{step, step_batch};
use typ_core::AppEvent;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

fn fixture(name: &str, lines: usize) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-resize-scroll").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let text: String = (0..lines).map(|i| format!("line {i}\n")).collect();
    std::fs::write(dir.join("long.rs"), text).unwrap();
    dir
}

fn app_with_file(name: &str, lines: usize) -> App {
    let dir = fixture(name, lines);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("long.rs")).unwrap();
    app.take_dirty();
    app
}

fn scroll_down(column: u16) -> AppEvent {
    AppEvent::Input(Event::Mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column,
        row: 5,
        modifiers: KeyModifiers::NONE,
    }))
}

/// Over the editor, which starts at column 30: the tree owns the left edge.
const OVER_EDITOR: u16 = 50;

#[test]
fn a_resize_repaints() {
    let mut app = app_with_file("resize-dirty", 100);

    step(&mut app, AppEvent::Input(Event::Resize(120, 40)), AREA).unwrap();

    assert!(
        app.take_dirty(),
        "a resize did not repaint, which is a frozen screen"
    );
}

#[test]
fn the_panels_see_the_new_size_after_a_resize() {
    let mut app = app_with_file("resize-size", 100);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();

    // ratatui's `draw` autoresizes a fullscreen viewport, so the resize event
    // only has to get us to a draw. This asserts the whole path, not the flag.
    terminal.backend_mut().resize(120, 40);
    step(&mut app, AppEvent::Input(Event::Resize(120, 40)), AREA).unwrap();
    assert!(app.take_dirty());
    terminal.draw(|frame| app.render(frame)).unwrap();

    assert_eq!(terminal.backend().size().unwrap().height, 40);
    let rows = terminal.backend().buffer().area.height;
    assert_eq!(rows, 40, "the frame was drawn at the old height");
}

#[test]
fn three_notches_in_one_batch_scroll_as_far_as_three_notches() {
    let mut one = app_with_file("scroll-one", 500);
    for _ in 0..3 {
        step(&mut one, scroll_down(OVER_EDITOR), AREA).unwrap();
    }
    let separately = one.editor_mut().top_line();

    let mut batched = app_with_file("scroll-batch", 500);
    step_batch(
        &mut batched,
        vec![
            scroll_down(OVER_EDITOR),
            scroll_down(OVER_EDITOR),
            scroll_down(OVER_EDITOR),
        ],
        AREA,
    )
    .unwrap();

    assert_eq!(
        batched.editor_mut().top_line(),
        separately,
        "coalescing changed how far a flick scrolls"
    );
    assert!(separately > 0, "the scroll did not reach the editor at all");
}

#[test]
fn a_key_pressed_during_a_scroll_is_not_lost() {
    let mut app = app_with_file("scroll-eats-key", 500);

    // The wheel and the keyboard at the same time. The old coalescing drained
    // pending events and `break`ed on the first non-scroll, dropping it.
    step_batch(
        &mut app,
        vec![
            scroll_down(OVER_EDITOR),
            scroll_down(OVER_EDITOR),
            AppEvent::Input(Event::Key(KeyEvent::new(
                KeyCode::Char('z'),
                KeyModifiers::NONE,
            ))),
            scroll_down(OVER_EDITOR),
        ],
        AREA,
    )
    .unwrap();

    let typed =
        (0..app.editor_mut().line_count()).any(|i| app.editor_mut().line_text(i).contains('z'));
    assert!(typed, "the character typed mid-scroll was eaten");
}
