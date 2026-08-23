//! Indent guides: a vertical rule at each completed level of indentation.
//!
//! Computed for the visible rows only — Zed's restriction, and the direct
//! answer to scrolling a 100k-line file without a whole-buffer pass. The
//! active-guide highlight Zed draws on top is deliberately absent: it needs an
//! unbounded scan for the enclosing block, and the bounded version of it draws
//! the guide at the wrong depth whenever the block outruns the bound. A wrong
//! guide is a lie; a missing one is not.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_core::{Panel, RenderContext, ThemeColors};
use typ_panel_editor::EditorPanel;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 32,
    height: 8,
};

fn render(panel: &mut EditorPanel) -> Buffer {
    let theme = ThemeColors::default();
    let ctx = RenderContext {
        theme: &theme,
        is_focused: true,
        panel_index: 0,
        terminal_width: AREA.width,
        terminal_height: AREA.height,
    };
    let mut buf = Buffer::empty(AREA);
    panel.render(AREA, &mut buf, &ctx);
    buf
}

/// Screen x of a display column of text: the frame, then the gutter, which is
/// as wide as the line count needs plus one blank.
fn tx(line_count: usize, col: u16) -> u16 {
    let mut digits = 1;
    let mut n = line_count;
    while n >= 10 {
        n /= 10;
        digits += 1;
    }
    1 + digits + 1 + col
}

fn ty(line: u16) -> u16 {
    1 + line
}

/// The rendered row of text, as the string of what each cell shows.
fn row(buf: &Buffer, line_count: usize, line: u16, cols: u16) -> String {
    (0..cols)
        .map(|col| buf[(tx(line_count, col), ty(line))].symbol())
        .collect()
}

// --- a guide at each level -----------------------------------------------

#[test]
fn a_guide_at_every_completed_level_of_indent() {
    // The spec's own example: a four-space-indented block three levels deep
    // draws guides at columns 0, 4 and 8, and the text still starts at 12.
    let mut panel =
        EditorPanel::from_str("fn a() {\n    if b {\n        c(|| {\n            d();\n");
    panel.set_tab_width(4);
    let buf = render(&mut panel);
    assert_eq!(row(&buf, 4, 3, 16), "│   │   │   d();");
}

#[test]
fn the_outermost_level_still_gets_its_guide() {
    let mut panel = EditorPanel::from_str("fn a() {\n    b();\n}\n");
    panel.set_tab_width(4);
    let buf = render(&mut panel);
    assert_eq!(row(&buf, 3, 1, 8), "│   b();");
}

#[test]
fn an_unindented_line_gets_no_guide() {
    let mut panel = EditorPanel::from_str("fn a() {\n    b();\n}\n");
    panel.set_tab_width(4);
    let buf = render(&mut panel);
    assert_eq!(row(&buf, 3, 0, 8), "fn a() {");
}

#[test]
fn a_part_level_of_indent_is_not_a_level() {
    // Two spaces where the file indents in fours is alignment, not nesting.
    // Drawing a rule through it would put one in the middle of a wrapped
    // argument list on every file that has one.
    let mut panel = EditorPanel::from_str("a\n  b\n");
    panel.set_tab_width(4);
    let buf = render(&mut panel);
    assert_eq!(row(&buf, 2, 1, 3), "  b");
}

#[test]
fn the_guide_takes_the_themes_indent_guide_colour_over_the_pages_ground() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("fn a() {\n    b();\n}\n");
    panel.set_tab_width(4);
    let buf = render(&mut panel);

    let cell = &buf[(tx(3, 0), ty(1))];
    assert_eq!(cell.symbol(), "│");
    assert_eq!(cell.fg, theme.indent_guide, "the guide's own colour");
    assert_eq!(cell.bg, theme.bg, "and the ground it is drawn on");
}

#[test]
fn a_tab_indent_is_guided_at_its_tab_stops() {
    let mut panel = EditorPanel::from_str("a\n\t\tb\n");
    panel.set_tab_width(4);
    let buf = render(&mut panel);
    assert_eq!(row(&buf, 2, 1, 9), "│   │   b");
}

// --- through blank lines -------------------------------------------------

#[test]
fn a_blank_line_inside_a_block_keeps_the_blocks_guides() {
    // The classic hole. Without this the guides break every time somebody
    // leaves a line between two statements, which is most of the time.
    let mut panel = EditorPanel::from_str("fn a() {\n    b();\n\n    c();\n}\n");
    panel.set_tab_width(4);
    let buf = render(&mut panel);
    assert_eq!(row(&buf, 5, 2, 4), "│   ");
}

#[test]
fn a_blank_line_between_blocks_takes_the_shallower_side() {
    // Otherwise the guides of the block above run on past its end and into
    // whatever follows.
    let mut panel = EditorPanel::from_str("fn a() {\n    b();\n}\n\nfn c() {\n");
    panel.set_tab_width(4);
    let buf = render(&mut panel);
    assert_eq!(row(&buf, 5, 3, 4), "    ");
}

#[test]
fn a_blank_line_whose_block_never_resumes_gets_nothing() {
    // The lookahead is bounded, so a gap longer than the bound draws no guide
    // rather than walking to the end of the file looking for one. A guide
    // missing through a two-hundred-line gap is a cosmetic miss; a scan of the
    // whole buffer on the keystroke path is the trap `line_text` already taught
    // this codebase once.
    let text = format!("    a\n{}    b\n", "\n".repeat(200));
    let mut panel = EditorPanel::from_str(&text);
    panel.set_tab_width(4);
    let buf = render(&mut panel);

    let drawn = row(&buf, 202, 1, 8);
    assert!(!drawn.contains('│'), "got {drawn:?}");
}
