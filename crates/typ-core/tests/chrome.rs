//! The frame a panel draws around itself.
//!
//! A box, and the one cell where two of them meet. Every claim here is about a
//! cell, because a cell is the only thing a terminal has.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use typ_core::{RenderContext, ThemeColors, chrome};

fn context(theme: &ThemeColors, is_focused: bool) -> RenderContext<'_> {
    RenderContext {
        theme,
        is_focused,
        panel_index: 0,
        terminal_width: 40,
        terminal_height: 10,
    }
}

fn draw(area: Rect, title: &str, focused: bool) -> (Buffer, ThemeColors) {
    let theme = ThemeColors::default();
    let mut buf = Buffer::empty(area);
    chrome::frame(area, &mut buf, title, &context(&theme, focused), theme.bg);
    (buf, theme)
}

fn row(buf: &Buffer, y: u16) -> String {
    (buf.area.x..buf.area.right())
        .map(|x| buf[(x, y)].symbol())
        .collect()
}

#[test]
fn the_content_area_is_exactly_what_a_bordered_block_reserved() {
    // Geometry must not move. `text_area`, `gutter_area`, mouse hit-testing and
    // the horizontal-scroll arithmetic all subtract this, and a one-cell drift
    // lands every click in the wrong place — the failure no test of the
    // gutter's own output would ever notice.
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
fn a_panel_too_small_to_hold_a_frame_reports_no_content_rather_than_underflowing() {
    // The sidebar degrades to a third of the width below 60 columns, so a panel
    // three cells wide is reachable and one cell wide is reachable at the
    // minimum. Unguarded subtraction here panics rather than drawing badly.
    for width in 0..3u16 {
        for height in 0..3u16 {
            let inner = chrome::inner(Rect::new(0, 0, width, height));
            assert_eq!(inner.width, 0, "width {width} should reserve nothing");
            assert_eq!(inner.height, 0, "height {height} should reserve nothing");
        }
    }
}

#[test]
fn the_title_sits_in_the_top_rule_with_a_space_either_side() {
    let (buf, _) = draw(Rect::new(0, 0, 20, 5), "notes", true);
    assert_eq!(row(&buf, 0), "┌─ notes ──────────┐");
}

#[test]
fn the_bottom_rule_closes_the_frame() {
    let (buf, _) = draw(Rect::new(0, 0, 20, 5), "notes", true);
    assert_eq!(row(&buf, 4), "└──────────────────┘");
}

#[test]
fn the_box_has_sides() {
    // A panel is a closed shape again. Brackets removed the seam and took the
    // boundary with it — the tree and the editor stopped reading as two things.
    // The complaint was never that a border existed, it was that two of them
    // touched in different colours, and that is the layout's problem now.
    let (buf, _) = draw(Rect::new(0, 0, 20, 5), "notes", true);
    for y in 1..4 {
        assert_eq!(buf[(0, y)].symbol(), "│", "left side missing at row {y}");
        assert_eq!(buf[(19, y)].symbol(), "│", "right side missing at row {y}");
    }
}

#[test]
fn two_panels_sharing_a_column_meet_in_a_tee() {
    // The layout overlaps the rects by one column, so the left panel writes `┐`
    // into the shared cell and the right panel writes `┌`. Neither is correct
    // and a tee is, so the frame merges what it finds instead of overwriting.
    //
    // This works because `set_style` patches style without touching symbols:
    // the first panel's corner survives the second panel's background fill and
    // is still there to be merged with.
    let theme = ThemeColors::default();
    let mut buf = Buffer::empty(Rect::new(0, 0, 40, 5));
    let left = Rect::new(0, 0, 20, 5);
    let right = Rect::new(19, 0, 21, 5);
    chrome::frame(left, &mut buf, "left", &context(&theme, false), theme.bg);
    chrome::frame(right, &mut buf, "right", &context(&theme, true), theme.bg);

    assert_eq!(buf[(19, 0)].symbol(), "┬", "top junction should be a tee");
    assert_eq!(
        buf[(19, 4)].symbol(),
        "┴",
        "bottom junction should be a tee"
    );
    assert_eq!(
        buf[(19, 2)].symbol(),
        "│",
        "the shared side stays one vertical"
    );
    // A shared border cannot be two colours. The focused panel draws last, so
    // the junction belongs to it and the focused box is the complete one.
    assert_eq!(buf[(19, 0)].fg, theme.border_focused);
}

#[test]
fn a_lone_panel_keeps_ordinary_corners() {
    // The merge must only fire where two frames actually meet. A panel drawn on
    // an empty buffer has nothing to merge with and gets plain corners.
    let (buf, _) = draw(Rect::new(0, 0, 20, 5), "notes", true);
    assert_eq!(buf[(0, 0)].symbol(), "┌");
    assert_eq!(buf[(19, 0)].symbol(), "┐");
    assert_eq!(buf[(0, 4)].symbol(), "└");
    assert_eq!(buf[(19, 4)].symbol(), "┘");
}

#[test]
fn every_cell_carries_the_given_background_not_the_terminals() {
    // A blank cell left at `Color::Reset` shows through to whatever the user's
    // terminal background happens to be, which draws stripes of the wrong
    // colour down the screen wherever the frame reserved a cell and drew
    // nothing in it.
    let (buf, theme) = draw(Rect::new(0, 0, 20, 5), "notes", true);
    for y in 0..5 {
        for x in 0..20 {
            assert_eq!(buf[(x, y)].bg, theme.bg, "reset cell at {x},{y}");
        }
    }
    assert_ne!(theme.bg, Color::Reset);
}

#[test]
fn the_background_is_the_callers_choice_not_the_themes_page_colour() {
    // The sidebar and the editor no longer share a surface, so the frame cannot
    // assume one.
    let theme = ThemeColors::default();
    let area = Rect::new(0, 0, 20, 5);
    let mut buf = Buffer::empty(area);
    chrome::frame(
        area,
        &mut buf,
        "notes",
        &context(&theme, true),
        theme.status_bar_bg,
    );
    assert_eq!(buf[(5, 2)].bg, theme.status_bar_bg);
    assert_ne!(theme.status_bar_bg, theme.bg);
}

#[test]
fn a_focused_panel_lights_its_border_and_an_unfocused_one_recedes() {
    let (focused, theme) = draw(Rect::new(0, 0, 20, 5), "notes", true);
    let (unfocused, _) = draw(Rect::new(0, 0, 20, 5), "notes", false);
    assert_eq!(focused[(0, 0)].fg, theme.border_focused);
    assert_eq!(unfocused[(0, 0)].fg, theme.border);
    assert_ne!(theme.border, theme.border_focused);
}

#[test]
fn a_title_longer_than_the_panel_is_clipped_rather_than_overflowing() {
    let (buf, _) = draw(Rect::new(0, 0, 12, 4), "a-very-long-directory-name", true);
    assert_eq!(row(&buf, 0).chars().count(), 12);
    // The box still closes. A title that overran would eat its own corner and
    // then keep going into whatever is drawn to the right of this panel.
    assert_eq!(buf[(11, 0)].symbol(), "┐");
}

#[test]
fn a_panel_narrower_than_its_own_frame_draws_nothing_and_does_not_panic() {
    for width in 0..4u16 {
        for height in 0..3u16 {
            let area = Rect::new(0, 0, width, height);
            let theme = ThemeColors::default();
            let mut buf = Buffer::empty(area);
            chrome::frame(area, &mut buf, "x", &context(&theme, true), theme.bg);
        }
    }
}
