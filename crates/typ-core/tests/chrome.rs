//! The lines between docked regions, and the float drawn over them.
//!
//! Every claim here is about a cell, because a cell is the only thing a
//! terminal has.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_core::{RenderContext, ThemeColors, chrome};

fn context(theme: &ThemeColors, is_focused: bool) -> RenderContext<'_> {
    RenderContext {
        theme,
        syntax: typ_core::SyntaxTheme::empty(),
        diagnostics: &[],
        is_focused,
        panel_index: 0,
        terminal_width: 40,
        terminal_height: 10,
    }
}

fn row(buf: &Buffer, y: u16) -> String {
    (buf.area.x..buf.area.right())
        .map(|x| buf[(x, y)].symbol())
        .collect()
}

#[test]
fn the_content_area_is_exactly_what_a_bordered_block_reserved() {
    // Geometry must not move. The float's body and its mouse hit-testing both
    // subtract this, and a one-cell drift lands every click in the wrong place.
    assert_eq!(
        chrome::inner(Rect::new(0, 0, 30, 7)),
        Rect::new(1, 1, 28, 5)
    );
    assert_eq!(
        chrome::inner(Rect::new(29, 0, 31, 7)),
        Rect::new(30, 1, 29, 5)
    );
}

#[test]
fn a_float_too_small_to_hold_a_border_reports_no_content_rather_than_underflowing() {
    for width in 0..3u16 {
        for height in 0..3u16 {
            let inner = chrome::inner(Rect::new(0, 0, width, height));
            assert_eq!(inner.width, 0, "width {width} should reserve nothing");
            assert_eq!(inner.height, 0, "height {height} should reserve nothing");
        }
    }
}

// ---------------------------------------------------------------------------
// Rules (interface §2)
// ---------------------------------------------------------------------------

fn heading(width: u16, label: &str, focused: bool) -> (Buffer, ThemeColors) {
    let theme = ThemeColors::default();
    let area = Rect::new(0, 0, width, 1);
    let mut buf = Buffer::empty(area);
    chrome::labelled_rule(area, &mut buf, label, focused, &theme, theme.chrome_bg);
    (buf, theme)
}

#[test]
fn a_labelled_rule_reads_rule_label_rule() {
    let (buf, _) = heading(20, "notes", false);
    assert_eq!(row(&buf, 0), "── notes ───────────");
}

#[test]
fn a_focused_label_is_accent_and_bold_and_so_is_the_rule_before_it() {
    use ratatui::style::Modifier;
    let (focused, theme) = heading(20, "notes", true);
    let (unfocused, _) = heading(20, "notes", false);

    for x in 0..2 {
        assert_eq!(focused[(x, 0)].fg, theme.border_focused, "column {x}");
        assert_eq!(unfocused[(x, 0)].fg, theme.border, "column {x}");
    }
    // `── notes `: the name is columns 3 to 7.
    for x in 3..8 {
        assert_eq!(focused[(x, 0)].fg, theme.border_focused, "column {x}");
        assert!(focused[(x, 0)].modifier.contains(Modifier::BOLD));
        assert_eq!(unfocused[(x, 0)].fg, theme.receded_fg, "column {x}");
        assert!(!unfocused[(x, 0)].modifier.contains(Modifier::BOLD));
    }
    // The rule after the name is still the rule.
    assert_eq!(focused[(10, 0)].fg, theme.border);
    assert_eq!(focused[(10, 0)].bg, theme.chrome_bg);
}

#[test]
fn a_label_longer_than_the_rule_is_clipped_and_one_cell_of_rule_survives() {
    let (buf, _) = heading(12, "a-very-long-directory-name", false);
    let text = row(&buf, 0);
    assert_eq!(text.chars().count(), 12);
    assert!(text.ends_with('─'), "{text:?}");
}

#[test]
fn a_rule_too_narrow_for_its_label_is_still_a_rule_and_does_not_panic() {
    for width in 0..6u16 {
        let (buf, _) = heading(width, "notes", true);
        assert!(row(&buf, 0).chars().all(|c| c == '─'));
    }
}

#[test]
fn a_vertical_rule_is_one_column_in_border() {
    let theme = ThemeColors::default();
    let mut buf = Buffer::empty(Rect::new(0, 0, 5, 4));
    chrome::rule(Rect::new(2, 0, 1, 4), &mut buf, &theme, theme.bg);
    for y in 0..4 {
        assert_eq!(row(&buf, y), "  │  ");
        assert_eq!(buf[(2, y)].fg, theme.border);
        assert_eq!(buf[(2, y)].bg, theme.bg);
    }
}

// ---------------------------------------------------------------------------
// Floats (interface §3)
// ---------------------------------------------------------------------------

/// A 22x6 float at (2, 1) in a 26x8 buffer, over text, so a cell the float
/// failed to clear still says `x`.
fn draw_float(focused: bool) -> (Buffer, ThemeColors) {
    let theme = ThemeColors::default();
    let mut buf = Buffer::with_lines(vec!["x".repeat(26); 8]);
    chrome::float(
        Rect::new(2, 1, 22, 6),
        &mut buf,
        "Open file",
        "esc",
        &context(&theme, focused),
    );
    (buf, theme)
}

#[test]
fn a_float_is_a_rounded_box_with_its_name_and_its_exit_cut_into_the_top() {
    let (buf, _) = draw_float(true);
    let top: String = (2..24).map(|x| buf[(x, 1)].symbol()).collect();
    let bottom: String = (2..24).map(|x| buf[(x, 6)].symbol()).collect();
    assert_eq!(top, "╭─ Open file ── esc ─╮");
    assert_eq!(bottom, "╰────────────────────╯");
    assert_eq!(buf[(2, 3)].symbol(), "│");
    assert_eq!(buf[(23, 3)].symbol(), "│");
}

#[test]
fn a_float_clears_what_it_is_drawn_over() {
    let (buf, theme) = draw_float(true);
    for y in 2..6 {
        for x in 3..23 {
            assert_eq!(buf[(x, y)].symbol(), " ", "text showed through at {x},{y}");
            assert_eq!(buf[(x, y)].bg, theme.chrome_bg, "{x},{y}");
        }
    }
}

#[test]
fn a_float_keeps_one_clear_cell_of_page_between_its_border_and_the_code() {
    // Interface §3: the gutter is what stops a border touching code.
    let (buf, theme) = draw_float(true);
    let ring = (1..25)
        .flat_map(|x| [(x, 0), (x, 7)])
        .chain((0..8).flat_map(|y| [(1, y), (24, y)]));
    for (x, y) in ring {
        assert_eq!(buf[(x, y)].symbol(), " ", "gutter at {x},{y}");
        assert_eq!(buf[(x, y)].bg, theme.bg, "gutter at {x},{y}");
    }
    // And nothing past it.
    assert_eq!(buf[(0, 3)].symbol(), "x");
    assert_eq!(buf[(25, 3)].symbol(), "x");
}

#[test]
fn a_float_with_the_keys_is_bordered_in_float_border_and_one_without_in_the_rule() {
    let (focused, theme) = draw_float(true);
    let (unfocused, _) = draw_float(false);
    assert_eq!(focused[(2, 1)].fg, theme.float_border);
    assert_eq!(focused[(23, 4)].fg, theme.float_border);
    assert_eq!(unfocused[(2, 1)].fg, theme.border);
    assert_eq!(unfocused[(23, 4)].fg, theme.border);
}

#[test]
fn the_exit_is_where_the_hit_test_says_it_is() {
    // Render and the mouse ask one function, so a click on `esc` cannot land a
    // column off.
    let (buf, _) = draw_float(true);
    let (x, width) = chrome::float_exit(Rect::new(2, 1, 22, 6), "Open file", "esc").unwrap();
    let drawn: String = (x..x + width).map(|x| buf[(x, 1)].symbol()).collect();
    assert_eq!(drawn, "esc");
}

#[test]
fn a_float_too_narrow_for_its_exit_drops_the_exit_before_the_name() {
    let theme = ThemeColors::default();
    // One column short of `╭─ Open file ─ esc ─╮`: the exit goes, the name
    // stays whole.
    let area = Rect::new(0, 0, 20, 3);
    let mut buf = Buffer::empty(area);
    chrome::float(area, &mut buf, "Open file", "esc", &context(&theme, true));
    assert_eq!(chrome::float_exit(area, "Open file", "esc"), None);
    let top: String = (0..20).map(|x| buf[(x, 0)].symbol()).collect();
    assert_eq!(top, "╭─ Open file ──────╮");
    // And at exactly the width, both, with one cell of rule between.
    let area = Rect::new(0, 0, 21, 3);
    let mut buf = Buffer::empty(area);
    chrome::float(area, &mut buf, "Open file", "esc", &context(&theme, true));
    let top: String = (0..21).map(|x| buf[(x, 0)].symbol()).collect();
    assert_eq!(top, "╭─ Open file ─ esc ─╮");
    // Narrower than the name, the name is clipped and the corner survives.
    let area = Rect::new(0, 0, 10, 3);
    let mut buf = Buffer::empty(area);
    chrome::float(area, &mut buf, "Open file", "esc", &context(&theme, true));
    assert_eq!(buf[(9, 0)].symbol(), "╮", "the name ate the corner");
    for width in 0..4u16 {
        for height in 0..3u16 {
            let area = Rect::new(0, 0, width, height);
            let mut buf = Buffer::empty(area);
            chrome::float(area, &mut buf, "x", "esc", &context(&theme, true));
        }
    }
}
