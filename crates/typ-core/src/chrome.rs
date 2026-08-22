//! The frame a panel draws around itself.
//!
//! A box, with the title set into the top edge, and a rule about what happens
//! in the one cell where two boxes meet.
//!
//! **The problem this solves is adjacency, not borders.** Two panels laid edge
//! to edge each drew a full box, so a `┐` landed in the last column of one and
//! a `┌` in the first column of the next: two rules, touching, in two different
//! colours whenever one panel held focus. That reads as a seam.
//!
//! Deleting the verticals fixed the seam and cost the boundary — the sidebar
//! and the editor stopped reading as two things. So the boxes stay and the
//! *overlap* is what changes: `layout::split` hands the two panels rects that
//! share a column, and the frame **merges** the glyph it finds there instead of
//! overwriting it. `┐` meeting `┌` is `┬`. One vertical on screen, drawn twice,
//! and neither panel has to know what sits beside it.
//!
//! The merge survives the second panel's background fill because
//! `Buffer::set_style` patches style without touching symbols, so the first
//! panel's corner is still in the cell when the second one looks.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::RenderContext;

/// The content area inside a framed panel.
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

/// Merge two box-drawing glyphs meeting in one cell.
///
/// Only the pairs two side-by-side panels can actually produce are listed;
/// anything else keeps the incoming glyph, so a frame drawn over ordinary
/// content still just draws itself.
fn merge(existing: &str, incoming: char) -> char {
    match (existing, incoming) {
        ("┐", '┌') | ("┌", '┐') => '┬',
        ("┘", '└') | ("└", '┘') => '┴',
        ("┬", _) => '┬',
        ("┴", _) => '┴',
        _ => incoming,
    }
}

/// Write one glyph, merging with whatever is already in the cell.
fn put(buf: &mut Buffer, x: u16, y: u16, glyph: char, style: Style) {
    if !buf.area.contains((x, y).into()) {
        return;
    }
    let merged = merge(buf[(x, y)].symbol(), glyph);
    buf[(x, y)].set_char(merged).set_style(style);
}

/// Paint a panel's background and the box around it.
///
/// `background` is an argument rather than read from the theme because the
/// sidebar and the editor no longer share a surface: chrome sits on
/// `chrome_bg`, content on `bg`.
///
/// Call before the panel draws its content — this fills the whole rect, and
/// anything drawn first would be painted over.
pub fn frame(area: Rect, buf: &mut Buffer, title: &str, ctx: &RenderContext, background: Color) {
    // Every cell, including the ones the box does not reach. A blank cell left
    // at `Color::Reset` shows the user's terminal background rather than the
    // theme's, which draws stripes of the wrong colour down the screen wherever
    // the frame reserved a cell and drew nothing into it.
    buf.set_style(area, Style::default().bg(background));

    // Two columns for the sides and one between them; two rows for the top and
    // bottom. Below that there is no box to draw — and the sidebar really does
    // get this narrow, degrading to a third of the width under 60 columns.
    if area.width < 3 || area.height < 2 {
        return;
    }

    let colour = if ctx.is_focused {
        ctx.theme.border_focused
    } else {
        ctx.theme.border
    };
    let style = Style::default().fg(colour).bg(background);
    let (left, right) = (area.x, area.right() - 1);
    let (top, bottom) = (area.y, area.bottom() - 1);

    // The horizontals, title set into the top one. Written before the corners
    // so a clipped title cannot eat them.
    let span = (area.width - 2) as usize;
    let head = format!("─ {title} ");
    let fill = span.saturating_sub(head.chars().count());
    let rule = format!("{head}{}", "─".repeat(fill));
    buf.set_stringn(left + 1, top, &rule, span, style);
    buf.set_stringn(left + 1, bottom, "─".repeat(span), span, style);

    // The verticals.
    for y in (top + 1)..bottom {
        put(buf, left, y, '│', style);
        put(buf, right, y, '│', style);
    }

    // The corners, merged with whatever a neighbouring panel already left here.
    put(buf, left, top, '┌', style);
    put(buf, right, top, '┐', style);
    put(buf, left, bottom, '└', style);
    put(buf, right, bottom, '┘', style);
}
