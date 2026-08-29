//! The picker in search mode: different corpus, different rows, same widget.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_core::{KeyChord, Panel, PanelEvent, RenderContext, ThemeColors};
use typ_find::{FileHit, LineHit};
use typ_picker::{Mode, Picker};

fn chord(code: KeyCode) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, KeyModifiers::NONE))
}

fn line_hits() -> Vec<LineHit> {
    vec![
        LineHit {
            path: "src/main.rs".into(),
            line: 41,
            col: 4,
            text: "    let needle = 1;".into(),
        },
        LineHit {
            path: "src/lib.rs".into(),
            line: 7,
            col: 0,
            text: "needle()".into(),
        },
    ]
}

#[test]
fn a_new_picker_is_in_file_mode() {
    assert_eq!(Picker::new().mode(), Mode::Files);
}

#[test]
fn search_mode_holds_line_hits() {
    let mut picker = Picker::search();
    assert_eq!(picker.mode(), Mode::Search);
    picker.set_lines(line_hits(), true);
    assert_eq!(picker.lines().len(), 2);
}

#[test]
fn enter_in_search_mode_opens_at_the_line_and_column() {
    // The whole point of the second mode. A search result that opens the file
    // at line 0 has thrown away the only thing the search found out.
    let mut picker = Picker::search();
    picker.set_lines(line_hits(), true);

    let events = picker.handle_key(chord(KeyCode::Enter));
    let opened = events.iter().find_map(|event| match event {
        PanelEvent::OpenFile { path, line, col } => {
            Some((path.to_string_lossy().to_string(), *line, *col))
        }
        _ => None,
    });
    assert_eq!(
        opened,
        Some(("src/main.rs".to_string(), 41, 4)),
        "got {events:?}"
    );
}

#[test]
fn the_selection_moves_through_line_hits_too() {
    let mut picker = Picker::search();
    picker.set_lines(line_hits(), true);

    picker.handle_key(chord(KeyCode::Down));
    let events = picker.handle_key(chord(KeyCode::Enter));
    let opened = events.iter().find_map(|event| match event {
        PanelEvent::OpenFile { path, line, .. } => {
            Some((path.to_string_lossy().to_string(), *line))
        }
        _ => None,
    });
    assert_eq!(opened, Some(("src/lib.rs".to_string(), 7)));
}

#[test]
fn enter_with_no_search_results_opens_nothing() {
    let mut picker = Picker::search();
    let events = picker.handle_key(chord(KeyCode::Enter));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, PanelEvent::OpenFile { .. })),
        "got {events:?}"
    );
}

#[test]
fn a_click_in_search_mode_opens_at_the_line() {
    // Invariant 8 applies to the second mode as much as the first.
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 60,
        height: 12,
    };
    let mut picker = Picker::search();
    picker.set_lines(line_hits(), true);

    let events = picker.handle_mouse(
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            // Border, query, rule, then row 0; row 1 is one lower.
            row: AREA.y + 4,
            modifiers: KeyModifiers::NONE,
        },
        AREA,
    );
    let opened = events.iter().find_map(|event| match event {
        PanelEvent::OpenFile { path, line, .. } => {
            Some((path.to_string_lossy().to_string(), *line))
        }
        _ => None,
    });
    assert_eq!(
        opened,
        Some(("src/lib.rs".to_string(), 7)),
        "got {events:?}"
    );
}

#[test]
fn the_two_modes_have_different_titles() {
    // The overlay looks identical otherwise, and a user who pressed the wrong
    // chord should be able to tell without typing.
    assert_ne!(Picker::new().title(), Picker::search().title());
}

#[test]
fn a_search_row_shows_the_path_the_line_number_and_the_text() {
    let theme = ThemeColors::default();
    let mut picker = Picker::search();
    picker.set_lines(line_hits(), true);

    let area = Rect::new(0, 0, 60, 10);
    let ctx = RenderContext {
        theme: &theme,
        syntax: typ_core::SyntaxTheme::empty(),
        diagnostics: &[],
        is_focused: true,
        panel_index: 0,
        terminal_width: 60,
        terminal_height: 10,
    };
    let mut buf = Buffer::empty(area);
    picker.render(area, &mut buf, &ctx);

    let row: String = (1..59).map(|x| buf[(x, 3)].symbol()).collect();
    assert!(row.contains("src/main.rs"), "got {row:?}");
    // 1-based for display: the buffer stores 0-based, and a user reading a
    // result against their own editor's gutter expects the gutter's numbering.
    assert!(
        row.contains("42"),
        "expected a 1-based line number, got {row:?}"
    );
    assert!(row.contains("needle"), "got {row:?}");
}

#[test]
fn a_capped_search_says_so_in_the_title() {
    let mut picker = Picker::search();
    picker.set_lines(line_hits(), false);
    assert!(
        picker.title().contains('+') || picker.title().to_lowercase().contains("more"),
        "a truncated result set did not say so: {:?}",
        picker.title()
    );
}

#[test]
fn file_mode_ignores_line_hits_and_search_mode_ignores_file_hits() {
    // The two lists are separate fields, so a late result from the other mode
    // cannot overwrite the one on screen. This is the assertion that keeps
    // them that way.
    let mut picker = Picker::new();
    picker.set_hits(vec![FileHit {
        path: "a.rs".into(),
        indices: vec![],
    }]);
    picker.set_lines(line_hits(), true);
    assert_eq!(picker.hits().len(), 1, "file hits were clobbered");

    let events = picker.handle_key(chord(KeyCode::Enter));
    let opened = events.iter().find_map(|event| match event {
        PanelEvent::OpenFile { path, .. } => Some(path.to_string_lossy().to_string()),
        _ => None,
    });
    assert_eq!(
        opened,
        Some("a.rs".to_string()),
        "file mode opened a search result"
    );
}
