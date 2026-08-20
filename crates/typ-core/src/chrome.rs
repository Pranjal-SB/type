//! The frame a panel draws around itself.
//!
//! A rule above the panel's content and a rule below it, carrying the title,
//! and nothing down the sides.
//!
//! **The verticals are the thing being removed, and their absence is the
//! feature.** Two panels laid edge to edge each used to draw a full box, so a
//! `┐` landed in the last column of one and a `┌` in the first column of the
//! next — two adjacent rules, in two different colours whenever one panel held
//! focus, reading as a seam down the middle of the screen. A bracket has
//! nothing at the boundary to collide with, so a panel never has to know what
//! sits beside it and drawing its own frame stays the panel's business. That is
//! what keeps `Panel` unchanged.
//!
//! The rules span the panel's *content* width rather than its full width, and
//! that inset is what leaves the blank columns between two neighbours. Running
//! them the full width would put the same adjacency back, rotated.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;

use crate::RenderContext;

/// The content area inside a bracketed panel.
///
/// Exactly what `Block::bordered().inner(area)` reserved before this module
/// existed, and it has to stay that way: `text_area`, `gutter_area`, mouse
/// hit-testing and the horizontal-scroll arithmetic all subtract it, and a
/// one-cell drift lands every click a column from the pointer.
pub fn inner(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1).min(area.right()),
        y: area.y.saturating_add(1).min(area.bottom()),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

/// Paint a panel's background and its two rules.
///
/// Call this before the panel draws its content: it fills the whole rect, and
/// anything drawn first would be painted over.
pub fn bracket(area: Rect, buf: &mut Buffer, title: &str, ctx: &RenderContext) {
    // Every cell, including the ones the rules do not reach. A blank cell left
    // at `Color::Reset` shows the user's terminal background rather than the
    // theme's, which draws stripes of the wrong colour down the screen wherever
    // the frame reserved a cell and drew nothing into it.
    buf.set_style(area, Style::default().bg(ctx.theme.bg));

    // Two columns for the margins and at least one for a rule; two rows for the
    // rules themselves. Below that there is no frame to draw — and the sidebar
    // really does get this narrow, degrading to a third of the width under 60
    // columns.
    if area.width < 3 || area.height < 2 {
        return;
    }

    let colour = if ctx.is_focused {
        ctx.theme.border_focused
    } else {
        ctx.theme.border
    };
    let style = Style::default().fg(colour).bg(ctx.theme.bg);
    let width = area.width as usize - 2;
    let x = area.x + 1;

    // Built at full length and clipped by `set_stringn`, so a title longer than
    // the panel loses its tail rather than writing past the rect.
    let head = format!("┌─ {title} ");
    let fill = width.saturating_sub(head.chars().count() + 1);
    let top = format!("{head}{}┐", "─".repeat(fill));
    buf.set_stringn(x, area.y, &top, width, style);

    let bottom = format!("└{}┘", "─".repeat(width.saturating_sub(2)));
    buf.set_stringn(x, area.y + area.height - 1, &bottom, width, style);
}
