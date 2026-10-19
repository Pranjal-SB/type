//! On a narrow bar the status segments drop by width (interface §7), and the
//! paint, the click and the `f6` walk all see the same ones.

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use typ_app::prompt::PromptKind;
use typ_app::run::step;
use typ_app::status::SegmentId;
use typ_app::{App, Focus};
use typ_core::AppEvent;

fn app(name: &str) -> App {
    let dir = std::env::temp_dir().join("typ-status-drop").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "one\ntwo\nthree\n").unwrap();
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("a.rs")).unwrap();
    app
}

fn status_row(app: &mut App, frame: Rect) -> Vec<String> {
    app.set_screen(frame);
    let mut terminal = Terminal::new(TestBackend::new(frame.width, frame.height)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    let buf = terminal.backend().buffer();
    (0..frame.width)
        .map(|x| buf[(x, frame.height - 1)].symbol().to_string())
        .collect()
}

fn find(row: &[String], text: &str) -> Option<u16> {
    let want: Vec<String> = text.chars().map(String::from).collect();
    row.windows(want.len())
        .rposition(|w| w == want.as_slice())
        .map(|at| at as u16)
}

#[test]
fn at_60_columns_position_shows_and_the_line_ending_does_not() {
    let frame = Rect::new(0, 0, 60, 24);
    let mut app = app("sixty");
    let row = status_row(&mut app, frame);
    let text = row.concat();
    assert!(find(&row, "1:1").is_some(), "{text}");
    assert!(find(&row, "LF").is_none(), "{text}");

    let column = find(&row, "1:1").unwrap();
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Up(MouseButton::Left),
    ] {
        let m = MouseEvent {
            kind,
            column,
            row: frame.height - 1,
            modifiers: KeyModifiers::NONE,
        };
        step(&mut app, AppEvent::Input(Event::Mouse(m)), frame).unwrap();
    }
    assert_eq!(app.prompt().map(|p| p.kind()), Some(PromptKind::GotoLine));
}

#[test]
fn the_walk_skips_a_segment_that_dropped() {
    let frame = Rect::new(0, 0, 40, 24);
    let mut app = app("forty");
    let row = status_row(&mut app, frame);
    assert!(find(&row, "\u{25a3}").is_none(), "{}", row.concat());

    let key = |app: &mut App, code| {
        let event = AppEvent::Input(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
        step(app, event, frame).unwrap();
    };
    while app.focus() != Focus::Status {
        key(&mut app, KeyCode::F(6));
    }
    key(&mut app, KeyCode::Right);
    assert_eq!(app.focused_segment(), Some(SegmentId::Position));
}
