use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_core::{KeyChord, Panel, PanelEvent, RenderContext, ThemeColors};
use typ_find::FileHit;
use typ_picker::Picker;

fn chord(code: KeyCode) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, KeyModifiers::NONE))
}

fn typed(c: char) -> KeyChord {
    chord(KeyCode::Char(c))
}

fn hits(paths: &[&str]) -> Vec<FileHit> {
    paths
        .iter()
        .map(|path| FileHit {
            path: path.to_string(),
            indices: Vec::new(),
        })
        .collect()
}

fn open(paths: &[&str]) -> Picker {
    let mut picker = Picker::new();
    picker.set_hits(hits(paths));
    picker
}

#[test]
fn typing_builds_the_query() {
    let mut picker = Picker::new();
    picker.handle_key(typed('h'));
    picker.handle_key(typed('i'));
    assert_eq!(picker.query(), "hi");
}

#[test]
fn backspace_removes_one_grapheme_not_one_byte() {
    // The prompt already learned this; the picker accepts the same text the
    // buffer does. Deleting a byte out of a combining sequence leaves invalid
    // text on screen and a query the matcher cannot parse.
    let mut picker = Picker::new();
    for c in "e\u{301}x".chars() {
        picker.handle_key(typed(c));
    }
    assert_eq!(picker.query(), "e\u{301}x");

    picker.handle_key(chord(KeyCode::Backspace));
    assert_eq!(picker.query(), "e\u{301}");

    picker.handle_key(chord(KeyCode::Backspace));
    assert_eq!(
        picker.query(),
        "",
        "the whole cluster should go, not one char"
    );
}

#[test]
fn backspace_on_an_empty_query_is_not_a_panic() {
    let mut picker = Picker::new();
    picker.handle_key(chord(KeyCode::Backspace));
    assert_eq!(picker.query(), "");
}

#[test]
fn a_chorded_key_is_never_text() {
    // Ctrl+P while the picker is open must not type a "p" into the query. The
    // same rule `handle_prompt_chord` follows, and for the same reason.
    let mut picker = Picker::new();
    picker.handle_key(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('p'),
        KeyModifiers::CONTROL,
    )));
    assert_eq!(picker.query(), "");
}

#[test]
fn down_moves_the_selection_and_stops_at_the_end() {
    let mut picker = open(&["a.rs", "b.rs", "c.rs"]);
    assert_eq!(picker.selected(), 0);

    picker.handle_key(chord(KeyCode::Down));
    assert_eq!(picker.selected(), 1);
    picker.handle_key(chord(KeyCode::Down));
    picker.handle_key(chord(KeyCode::Down));
    picker.handle_key(chord(KeyCode::Down));
    assert_eq!(picker.selected(), 2, "ran off the end of the list");
}

#[test]
fn up_stops_at_the_top() {
    let mut picker = open(&["a.rs", "b.rs"]);
    picker.handle_key(chord(KeyCode::Up));
    assert_eq!(picker.selected(), 0);
}

#[test]
fn the_selection_survives_a_list_that_shrinks_under_it() {
    // Every keystroke replaces the list. Typing one more character while the
    // last row is selected must not leave the selection pointing past the end —
    // which is an index into a shorter vector on the very next render.
    let mut picker = open(&["a.rs", "b.rs", "c.rs"]);
    picker.handle_key(chord(KeyCode::Down));
    picker.handle_key(chord(KeyCode::Down));
    assert_eq!(picker.selected(), 2);

    picker.set_hits(hits(&["a.rs"]));
    assert_eq!(picker.selected(), 0, "selection left dangling past the end");
}

#[test]
fn an_empty_list_selects_nothing_rather_than_row_zero() {
    let mut picker = open(&["a.rs"]);
    picker.set_hits(Vec::new());
    assert!(picker.selection().is_none());
}

#[test]
fn enter_opens_the_selected_hit() {
    let mut picker = open(&["a.rs", "b.rs"]);
    picker.handle_key(chord(KeyCode::Down));
    let events = picker.handle_key(chord(KeyCode::Enter));

    let opened = events.iter().find_map(|event| match event {
        PanelEvent::OpenFile { path, line, col } => Some((path.clone(), *line, *col)),
        _ => None,
    });
    let (path, line, col) = opened.expect("expected an OpenFile");
    assert_eq!(path.to_string_lossy(), "b.rs");
    assert_eq!((line, col), (0, 0));
}

#[test]
fn enter_with_no_hits_emits_nothing_rather_than_opening_an_empty_path() {
    // The bug this guards writes `OpenFile { path: "" }` into the app, which
    // fails somewhere far away from the keypress that caused it.
    let mut picker = Picker::new();
    let events = picker.handle_key(chord(KeyCode::Enter));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, PanelEvent::OpenFile { .. })),
        "got {events:?}"
    );
}

#[test]
fn escape_closes() {
    let mut picker = open(&["a.rs"]);
    let events = picker.handle_key(chord(KeyCode::Esc));
    assert!(events.contains(&PanelEvent::CloseSelf), "got {events:?}");
}

#[test]
fn the_picker_captures_escape() {
    // Otherwise the app's own escape handling closes something else while the
    // picker stays open.
    assert!(Picker::new().captures_escape());
}

#[test]
fn a_list_longer_than_the_area_scrolls_to_keep_the_selection_visible() {
    let paths: Vec<String> = (0..50).map(|i| format!("file{i}.rs")).collect();
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let mut picker = open(&refs);

    // Four rows of list in a six-row box: two go to the border, one to the
    // query line.
    let rows = 4;
    for _ in 0..10 {
        picker.handle_key(chord(KeyCode::Down));
    }
    let visible = picker.visible(rows);
    assert!(
        visible.iter().any(|hit| hit.path == "file10.rs"),
        "the selection scrolled out of view: {:?}",
        visible.iter().map(|h| &h.path).collect::<Vec<_>>()
    );
    assert!(visible.len() <= rows);
}

#[test]
fn scrolling_back_up_brings_the_top_into_view() {
    let paths: Vec<String> = (0..50).map(|i| format!("file{i}.rs")).collect();
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let mut picker = open(&refs);

    for _ in 0..20 {
        picker.handle_key(chord(KeyCode::Down));
    }
    for _ in 0..20 {
        picker.handle_key(chord(KeyCode::Up));
    }
    let visible = picker.visible(4);
    assert_eq!(visible[0].path, "file0.rs");
}

#[test]
fn rendering_fits_inside_its_area() {
    // A panel that writes outside its rect corrupts whatever it floats over,
    // and an overlay is the one panel where that is guaranteed to be something.
    let theme = ThemeColors::default();
    let mut picker = open(&["crates/typ-core/src/theme.rs", "README.md"]);
    let area = Rect::new(5, 3, 30, 8);
    let ctx = RenderContext {
        theme: &theme,
        syntax: typ_core::SyntaxTheme::empty(),
        is_focused: true,
        panel_index: 0,
        terminal_width: 80,
        terminal_height: 24,
    };
    // A buffer larger than the area, so writing outside it is detectable rather
    // than a panic.
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
    picker.render(area, &mut buf, &ctx);

    for y in 0..24u16 {
        for x in 0..80u16 {
            let inside = x >= area.x && x < area.right() && y >= area.y && y < area.bottom();
            if !inside {
                assert_eq!(
                    buf[(x, y)].symbol(),
                    " ",
                    "wrote outside the area at ({x}, {y})"
                );
            }
        }
    }
}

#[test]
fn rendering_a_narrow_area_does_not_panic() {
    let theme = ThemeColors::default();
    let mut picker = open(&["a/very/long/path/that/will/not/fit.rs"]);
    let ctx = RenderContext {
        theme: &theme,
        syntax: typ_core::SyntaxTheme::empty(),
        is_focused: true,
        panel_index: 0,
        terminal_width: 8,
        terminal_height: 3,
    };
    let area = Rect::new(0, 0, 8, 3);
    let mut buf = Buffer::empty(area);
    picker.render(area, &mut buf, &ctx);
}

#[test]
fn a_zero_sized_area_does_not_panic() {
    let theme = ThemeColors::default();
    let mut picker = open(&["a.rs"]);
    let ctx = RenderContext {
        theme: &theme,
        syntax: typ_core::SyntaxTheme::empty(),
        is_focused: true,
        panel_index: 0,
        terminal_width: 0,
        terminal_height: 0,
    };
    let area = Rect::new(0, 0, 0, 0);
    let mut buf = Buffer::empty(Rect::new(0, 0, 1, 1));
    picker.render(area, &mut buf, &ctx);
}

#[test]
fn matched_graphemes_are_styled_differently_from_the_rest() {
    // The thing that makes a fuzzy picker readable: without it you can see
    // *that* a row matched but not *why*, and a fuzzy match on a long path is
    // not obvious from looking at it.
    use ratatui::style::Color;

    let theme = ThemeColors::default();
    let mut picker = Picker::new();
    picker.set_hits(vec![FileHit {
        path: "abcdef.rs".to_string(),
        // "a" and "c" matched.
        indices: vec![0, 2],
    }]);

    let area = Rect::new(0, 0, 30, 8);
    let ctx = RenderContext {
        theme: &theme,
        syntax: typ_core::SyntaxTheme::empty(),
        is_focused: true,
        panel_index: 0,
        terminal_width: 30,
        terminal_height: 8,
    };
    let mut buf = Buffer::empty(area);
    picker.render(area, &mut buf, &ctx);

    // Border column 0, so text starts at column 1; the list starts at row 3.
    let row = 3;
    let fg_at = |x: u16| buf[(x, row)].fg;
    assert_eq!(buf[(1, row)].symbol(), "a", "the row is not where expected");

    let matched: Color = fg_at(1);
    let plain: Color = fg_at(2);
    assert_ne!(matched, plain, "matched and unmatched share a colour");
    assert_eq!(
        fg_at(3),
        matched,
        "the second matched grapheme is not styled"
    );
    assert_eq!(fg_at(4), plain);
}

#[test]
fn an_index_past_the_end_of_a_path_does_not_panic() {
    // Defensive: indices arrive from another crate across a channel, and a row
    // whose path was truncated between ranking and rendering must not take the
    // editor down.
    let theme = ThemeColors::default();
    let mut picker = Picker::new();
    picker.set_hits(vec![FileHit {
        path: "ab".to_string(),
        indices: vec![0, 99],
    }]);
    let area = Rect::new(0, 0, 20, 8);
    let ctx = RenderContext {
        theme: &theme,
        syntax: typ_core::SyntaxTheme::empty(),
        is_focused: true,
        panel_index: 0,
        terminal_width: 20,
        terminal_height: 8,
    };
    let mut buf = Buffer::empty(area);
    picker.render(area, &mut buf, &ctx);
}
