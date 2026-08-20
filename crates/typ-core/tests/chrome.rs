//! The frame a panel draws around itself.
//!
//! A rule above its content and a rule below, and nothing down the sides —
//! which is what stops two adjacent panels drawing two adjacent borders. Every
//! claim here is about a cell, because a cell is the only thing a terminal has.

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
    chrome::bracket(area, &mut buf, title, &context(&theme, focused));
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
        chrome::inner(Rect::new(30, 0, 30, 7)),
        Rect::new(31, 1, 28, 5)
    );
}

#[test]
fn a_panel_too_small_to_hold_a_frame_reports_no_content_rather_than_underflowing() {
    // The sidebar degrades to a third of the width below 60 columns, so a
    // panel three cells wide is reachable and one cell wide is reachable at the
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
    assert_eq!(row(&buf, 0), " ┌─ notes ────────┐ ");
}

#[test]
fn the_bottom_rule_closes_the_frame() {
    let (buf, _) = draw(Rect::new(0, 0, 20, 5), "notes", true);
    assert_eq!(row(&buf, 4), " └────────────────┘ ");
}

#[test]
fn nothing_is_drawn_down_the_sides() {
    // The whole point of the exercise. A vertical is what collides with the
    // neighbouring panel's vertical; there is no vertical to collide.
    let (buf, _) = draw(Rect::new(0, 0, 20, 5), "notes", true);
    for y in 1..4 {
        assert_eq!(row(&buf, y), " ".repeat(20), "row {y} is not blank");
    }
}

#[test]
fn the_rule_stops_short_of_the_panel_edge_so_two_panels_cannot_touch() {
    // Two panels laid edge to edge used to put a `┐` in the last column of one
    // and a `┌` in the first column of the next, in two different colours when
    // one held focus. Leaving the outer column blank is what makes that
    // impossible rather than merely unlikely.
    let (buf, _) = draw(Rect::new(0, 0, 20, 5), "notes", true);
    assert_eq!(buf[(0, 0)].symbol(), " ");
    assert_eq!(buf[(19, 0)].symbol(), " ");
    assert_eq!(buf[(0, 4)].symbol(), " ");
    assert_eq!(buf[(19, 4)].symbol(), " ");
}

#[test]
fn every_cell_carries_the_themes_background_not_the_terminals() {
    // A blank cell left at `Color::Reset` shows through to whatever the user's
    // terminal background happens to be, which draws stripes of the wrong
    // colour down the screen wherever the frame reserved a cell and drew
    // nothing in it. That is worse than the seam this module exists to remove.
    let (buf, theme) = draw(Rect::new(0, 0, 20, 5), "notes", true);
    for y in 0..5 {
        for x in 0..20 {
            assert_eq!(buf[(x, y)].bg, theme.bg, "reset cell at {x},{y}");
        }
    }
    assert_ne!(theme.bg, Color::Reset);
}

#[test]
fn a_focused_panel_lights_its_rule_and_an_unfocused_one_recedes() {
    let (focused, theme) = draw(Rect::new(0, 0, 20, 5), "notes", true);
    let (unfocused, _) = draw(Rect::new(0, 0, 20, 5), "notes", false);
    assert_eq!(focused[(1, 0)].fg, theme.border_focused);
    assert_eq!(unfocused[(1, 0)].fg, theme.border);
    assert_ne!(theme.border, theme.border_focused);
}

#[test]
fn a_title_longer_than_the_panel_is_clipped_rather_than_overflowing() {
    let (buf, _) = draw(Rect::new(0, 0, 12, 4), "a-very-long-directory-name", true);
    assert_eq!(row(&buf, 0).chars().count(), 12);
    // Still nothing in the margins, which is what a naive write past the end
    // would destroy.
    assert_eq!(buf[(0, 0)].symbol(), " ");
    assert_eq!(buf[(11, 0)].symbol(), " ");
}

#[test]
fn a_panel_narrower_than_its_own_frame_draws_nothing_and_does_not_panic() {
    for width in 0..4u16 {
        for height in 0..3u16 {
            let area = Rect::new(0, 0, width, height);
            let theme = ThemeColors::default();
            let mut buf = Buffer::empty(area);
            chrome::bracket(area, &mut buf, "x", &context(&theme, true));
        }
    }
}
