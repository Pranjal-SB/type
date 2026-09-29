//! A modal float dims what is behind it; a non-modal one does not (interface §3).

use std::path::PathBuf;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Modifier;
use typ_app::{App, layout};
use typ_core::ThemeColors;

fn app(name: &str) -> App {
    let dir: PathBuf = std::env::temp_dir().join("typ-scrim").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.rs"), "fn main() {}\n").unwrap();
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("main.rs")).unwrap();
    app
}

fn draw(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    terminal.backend().buffer().clone()
}

#[test]
fn the_picker_dims_everything_behind_it_but_the_status_bar() {
    let theme = ThemeColors::default();
    let mut app = app("picker");
    app.open_picker();
    let buf = draw(&mut app, 100, 30);
    let picker = layout::picker_area(buf.area);

    // A tree cell, an editor cell and a border cell, all well clear of it.
    for (x, y) in [(3, 2), (40, 1), (0, 0), (99, 10)] {
        assert_eq!(buf[(x, y)].bg, theme.scrim, "{x},{y} was not dimmed");
        assert!(buf[(x, y)].modifier.contains(Modifier::DIM), "{x},{y}");
    }
    // The status bar says what the editor is doing, picker or not.
    for x in [0, 50, 99] {
        assert_eq!(buf[(x, 29)].bg, theme.status_bar_bg, "status bar at {x}");
        assert!(!buf[(x, 29)].modifier.contains(Modifier::DIM));
    }
    // The gutter is page, not scrim, and the float itself is not dimmed.
    assert_eq!(buf[(picker.x - 1, picker.y + 3)].bg, theme.bg);
    assert!(
        !buf[(picker.x + 2, picker.y + 1)]
            .modifier
            .contains(Modifier::DIM)
    );
}

#[test]
fn with_no_float_up_nothing_is_dimmed() {
    let theme = ThemeColors::default();
    let mut app = app("none");
    let buf = draw(&mut app, 100, 30);
    for y in 0..30 {
        for x in 0..100 {
            assert_ne!(buf[(x, y)].bg, theme.scrim, "{x},{y}");
        }
    }
}
