//! Golden-frame tests: render the whole app into an in-memory backend and
//! assert the cells.
//!
//! CI compiles and runs on three platforms but no human ever looks at the
//! output there. These are what stands in for that — every border, every
//! highlight, and every wide-character alignment is asserted rather than
//! eyeballed.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;
use typ_app::App;
use typ_core::{KeyChord, Panel, ThemeColors};

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-frame-test").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("main.rs"), "fn main() {}\nlet x = 1;\n").unwrap();
    std::fs::write(dir.join("src/wide.txt"), "日本語 ok\nplain\n").unwrap();
    dir
}

fn draw(app: &mut App, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    terminal
}

/// The frame as visible text, one string per row.
fn rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

/// Slice a row by columns, not bytes — a CJK grapheme is three bytes wide and
/// one entry in the buffer, and byte slicing splits it.
fn cols(row: &str, from: usize, to: usize) -> String {
    row.chars().skip(from).take(to - from).collect()
}

fn key(code: KeyCode) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn the_opening_frame_draws_both_panels_and_the_status_bar() {
    let mut app = App::new(&fixture("opening")).unwrap();
    let terminal = draw(&mut app, 60, 8);
    let rows = rows(&terminal);

    // Sidebar is the fixed 30 columns at this width; the editor takes the rest.
    // The editor's `1` is the gutter: an empty buffer is still one line long,
    // and numbering it from the first frame is what makes the column furniture
    // rather than something that appears once there is text.
    let expected = [
        "┌─ opening ──────────────────┬─ untitled ──────────────────┐",
        "│> src                       │ 1                           │",
        "│  main.rs                   │                             │",
        "│                            │                             │",
        "│                            │                             │",
        "│                            │                             │",
        "└────────────────────────────┴─────────────────────────────┘",
        // The hint is truncated to whatever the segments leave: the right half
        // is the fixed cost, so a message can never shove the position off the
        // end of the bar. Sixty columns is a tight terminal and this is what
        // tight looks like.
        "Tab focus  ·  Enter open  untitled  LF  Spaces: 4  1:1  100%",
    ];
    assert_eq!(rows, expected);

    // The literal above catches any change at all, which is its job and also
    // its weakness: eight hand-counted sixty-column strings are exactly the
    // kind of thing somebody adjusts until it goes green. These say why it is
    // right, so a future edit has to satisfy the reasoning rather than the
    // arithmetic.
    //
    // The claim that matters is that column 29 is the *only* divider. Two
    // panels drawing full boxes into adjacent columns is the seam this layout
    // exists to prevent, and it would show up as a second vertical at 28 or 30.
    for (y, row) in rows[..7].iter().enumerate() {
        assert_eq!(row.chars().count(), 60, "row {y} changed width");
        assert!(
            ["│", "┬", "┴"].contains(&cols(row, 29, 30).as_str()),
            "row {y} lost the divider: {row}"
        );
        assert_ne!(cols(row, 28, 29), "│", "row {y}: second divider left of 29");
        assert_ne!(
            cols(row, 30, 31),
            "│",
            "row {y}: second divider right of 29"
        );
    }
}

#[test]
fn the_focused_panel_is_the_one_with_the_lit_border() {
    let mut app = App::new(&fixture("focus")).unwrap();
    let theme = ThemeColors::default();

    let terminal = draw(&mut app, 60, 8);
    let buffer = terminal.backend().buffer();
    // A cell each panel owns outright: column 0 is the tree's own corner and 31
    // sits in the editor's top rule. Deliberately *not* column 29 — that is the
    // shared divider, which belongs to whichever panel has focus and so says
    // nothing about the other one. The tree holds focus at startup.
    assert_eq!(buffer[(0, 0)].fg, theme.border_focused);
    assert_eq!(buffer[(31, 0)].fg, theme.border);

    app.cycle_focus();
    let terminal = draw(&mut app, 60, 8);
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(0, 0)].fg, theme.border);
    assert_eq!(buffer[(31, 0)].fg, theme.border_focused);

    // And the divider itself goes to the focused panel, since a shared border
    // cannot be two colours.
    assert_eq!(buffer[(29, 0)].fg, theme.border_focused);
}

#[test]
fn the_selected_tree_row_is_highlighted_and_only_that_row() {
    let mut app = App::new(&fixture("selection")).unwrap();
    let theme = ThemeColors::default();
    let terminal = draw(&mut app, 60, 8);
    let buffer = terminal.backend().buffer();

    // The primary colour, not the secondary one: the tree has exactly one
    // selected row and it is the thing being steered, which is the same job the
    // editor's primary selection does. A tree cursor in the quieter colour
    // would be the one selection on screen that is hard to find.
    assert_eq!(buffer[(1, 1)].bg, theme.selection_primary_bg);
    assert_eq!(buffer[(1, 1)].fg, theme.selection_fg);
    assert_ne!(buffer[(1, 2)].bg, theme.selection_primary_bg);
}

#[test]
fn expanding_a_directory_indents_its_children_under_it() {
    let dir = fixture("expand");
    let mut app = App::new(&dir).unwrap();
    app.tree_mut().handle_key(key(KeyCode::Enter)); // src/
    let terminal = draw(&mut app, 60, 8);
    let rows = rows(&terminal);

    assert_eq!(cols(&rows[1], 0, 16), "│v src          ");
    assert_eq!(cols(&rows[2], 0, 16), "│    wide.txt   ");
    assert_eq!(cols(&rows[3], 0, 16), "│  main.rs      ");
}

#[test]
fn an_open_file_renders_its_text_with_the_cursor_on_it() {
    let dir = fixture("open");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("main.rs")).unwrap();
    // Through the dispatcher, the way a real keypress arrives — the editor has
    // no raw-key behavior of its own any more.
    app.handle_chord(key(KeyCode::Down)).unwrap();
    app.handle_chord(key(KeyCode::Right)).unwrap();

    let mut terminal = draw(&mut app, 60, 8);
    let rows = rows(&terminal);
    assert!(rows[0].contains("main.rs"), "title missing: {}", rows[0]);
    assert_eq!(cols(&rows[1], 30, 60), " 1 fn main() {}              │");
    assert_eq!(cols(&rows[2], 30, 60), " 2 let x = 1;                │");

    // Divider at 29, gutter 30-32 — a diagnostic sign, a digit, a spacer — so
    // editor text starts at column 33. The cursor is one line down and one
    // grapheme in.
    assert_eq!(terminal.get_cursor_position().unwrap(), (34, 2).into());
}

#[test]
fn wide_characters_do_not_push_the_border_out_of_line() {
    let dir = fixture("wide");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("src/wide.txt")).unwrap();
    let terminal = draw(&mut app, 60, 8);
    let rows = rows(&terminal);

    // A wide grapheme occupies its cell plus a blank continuation cell, so one
    // char here is one column — which is the whole point: if the width maths
    // were wrong, the trailing border would move.
    assert_eq!(cols(&rows[1], 30, 60), " 1 日 本 語  ok                 │");
    // The trailing border is the evidence: get the width maths wrong and it
    // moves. The divider at 29 is the same check on the other side — a tree row
    // that overran would push it right.
    for row in &rows[1..6] {
        assert_eq!(row.chars().count(), 60, "row changed width: {row}");
        assert!(row.ends_with('│'), "border broke on: {row}");
        assert!(row.starts_with('│'), "border broke on: {row}");
        assert_eq!(cols(row, 29, 30), "│", "the divider moved: {row}");
    }
}

#[test]
fn the_cursor_lands_past_a_wide_character_not_inside_it() {
    let dir = fixture("wide-cursor");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("src/wide.txt")).unwrap();
    app.handle_chord(key(KeyCode::Right)).unwrap();

    let mut terminal = draw(&mut app, 60, 8);
    // Text starts at 33 — the divider at 29, then a three-cell gutter: a
    // diagnostic sign, a digit and a spacer. One CJK grapheme is two
    // columns, so 33 + 2.
    assert_eq!(terminal.get_cursor_position().unwrap(), (35, 1).into());
}

#[test]
fn the_tree_shows_no_cursor_when_it_holds_focus() {
    let mut app = App::new(&fixture("no-cursor")).unwrap();
    let terminal = draw(&mut app, 60, 8);
    // TestBackend reports a position regardless, so assert on visibility by
    // checking the panel itself declines to place one.
    assert!(
        app.tree_mut()
            .cursor_position(ratatui::layout::Rect::new(0, 0, 30, 7))
            .is_none()
    );
    let _ = terminal;
}

#[test]
fn a_long_message_is_truncated_rather_than_shoving_the_position_off_screen() {
    let dir = fixture("truncate");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("main.rs")).unwrap();
    app.apply(vec![typ_core::PanelEvent::Notify {
        level: typ_core::NotifyLevel::Error,
        message: "x".repeat(200),
    }])
    .unwrap();

    let terminal = draw(&mut app, 60, 8);
    let rows = rows(&terminal);
    let status = &rows[7];
    assert_eq!(status.chars().count(), 60);
    assert!(
        status.ends_with("main.rs  rs  LF  Spaces: 4  1:1  33%"),
        "status was: {status}"
    );
}

#[test]
fn the_status_bar_spans_the_frame_in_its_own_colors() {
    let mut app = App::new(&fixture("status-style")).unwrap();
    let theme = ThemeColors::default();
    let terminal = draw(&mut app, 60, 8);
    let buffer = terminal.backend().buffer();

    for x in 0..60 {
        assert_eq!(buffer[(x, 7)].bg, theme.status_bar_bg, "gap at column {x}");
    }
    assert_eq!(buffer[(0, 7)].fg, theme.status_bar_fg);
    assert_ne!(theme.status_bar_bg, Color::Reset);
}

fn chord(code: KeyCode, mods: KeyModifiers) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, mods))
}

/// The background a cell was painted with, for asserting selection highlight.
fn bg(terminal: &Terminal<TestBackend>, x: u16, y: u16) -> Option<Color> {
    terminal.backend().buffer()[(x, y)].style().bg
}

#[test]
fn a_selection_is_visible_in_the_rendered_frame() {
    let dir = fixture("selection-frame");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("main.rs")).unwrap();
    app.handle_chord(chord(KeyCode::Right, KeyModifiers::SHIFT))
        .unwrap();
    app.handle_chord(chord(KeyCode::Right, KeyModifiers::SHIFT))
        .unwrap();

    let terminal = draw(&mut app, 60, 8);
    let theme = ThemeColors::default();
    // Editor text begins at column 33: the divider at 29, then a three-cell
    // gutter — a diagnostic sign, a digit and a spacer.
    // A lone selection is the primary one — the thing every motion is
    // relative to, and the only one on screen worth pointing at.
    assert_eq!(bg(&terminal, 33, 1), Some(theme.selection_primary_bg));
    assert_eq!(bg(&terminal, 34, 1), Some(theme.selection_primary_bg));
    assert_eq!(
        bg(&terminal, 35, 1),
        Some(theme.bg),
        "the highlight must stop where the selection does"
    );
    assert_eq!(
        bg(&terminal, 30, 1),
        Some(theme.gutter_bg),
        "and it must not bleed back into the gutter"
    );
}

#[test]
fn several_cursors_are_all_visible_as_the_frame_is_drawn() {
    let dir = fixture("multicursor-frame");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("main.rs")).unwrap();
    app.handle_chord(chord(
        KeyCode::Down,
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    ))
    .unwrap();
    app.handle_chord(chord(KeyCode::Char('#'), KeyModifiers::NONE))
        .unwrap();

    let terminal = draw(&mut app, 60, 8);
    let rows = rows(&terminal);
    assert!(rows[1].contains("#fn main"), "row 1: {}", rows[1]);
    assert!(rows[2].contains("#let x"), "row 2: {}", rows[2]);
}

#[test]
fn an_open_prompt_takes_over_the_left_of_the_status_bar() {
    let dir = fixture("prompt-frame");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("main.rs")).unwrap();
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL))
        .unwrap();
    for c in "main".chars() {
        app.handle_chord(chord(KeyCode::Char(c), KeyModifiers::NONE))
            .unwrap();
    }

    let terminal = draw(&mut app, 60, 8);
    let rows = rows(&terminal);
    assert!(rows[7].starts_with("Search: main"), "status: {}", rows[7]);
    assert!(
        rows[7].ends_with("main.rs  rs  LF  Spaces: 4  1:1  33%"),
        "status: {}",
        rows[7]
    );
}

#[test]
fn a_long_line_scrolled_right_keeps_its_borders() {
    let dir = fixture("horizontal-frame");
    std::fs::write(dir.join("wide.rs"), "x".repeat(200) + "\n").unwrap();
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("wide.rs")).unwrap();

    // One frame first: the panel learns its width at render time, so a motion
    // before any draw cannot scroll and the test would pass without exercising
    // anything.
    draw(&mut app, 60, 8);
    app.handle_chord(chord(KeyCode::End, KeyModifiers::NONE))
        .unwrap();
    let terminal = draw(&mut app, 60, 8);

    assert!(
        app.editor_mut().left_col() > 0,
        "the view must have scrolled sideways"
    );
    let rows = rows(&terminal);
    assert_eq!(rows[1].chars().count(), 60);
    assert!(rows[1].ends_with('│'), "row 1: {}", rows[1]);
    assert!(rows[1].starts_with('│'), "row 1: {}", rows[1]);
    // And the scrolled line stopped at the divider rather than running under
    // the tree, which the outer borders alone would not catch.
    assert_eq!(cols(&rows[1], 29, 30), "│", "row 1: {}", rows[1]);
}
