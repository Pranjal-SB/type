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
use ratatui::style::{Color, Modifier, Style};
use unicode_segmentation::UnicodeSegmentation;

/// The graphemes of `text` that are safe to put in a cell, in order.
///
/// **`Cell::set_symbol` does not filter and the backend does not either.**
/// ratatui strips control characters in `Span::styled_graphemes` and
/// `Buffer::set_stringn`, and says in its own source that `set_symbol` is a
/// low-level API where the caller is responsible. `TypBackend` then writes a
/// cell's symbol to the terminal verbatim, so an `ESC` that reaches a cell is
/// an escape sequence the terminal obeys.
///
/// Both strings TYPE paints through `set_symbol` are attacker-reachable: a tab
/// label is a file name, and a picker row is a path from the walk or a line of
/// bytes from a searched file. A filename is any byte but `/` and NUL on POSIX
/// and git checks one out happily, so `typ .` on a cloned repository was enough
/// to hand a terminal an OSC 52 clipboard write. Gap 69.
///
/// One function rather than the same `filter` at each site, for the reason gap
/// 68 records: a predicate written twice is a predicate that drifts once.
pub fn printable(text: &str) -> impl Iterator<Item = &str> {
    text.graphemes(true)
        .filter(|g| !g.contains(char::is_control))
}

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

    // The title is the panel's label (interface §4): accent and bold when it
    // has focus, receded with the rest of the panel when it does not. Written
    // over the name the rule already carries, clipped at the same column.
    let label = if ctx.is_focused {
        style.add_modifier(Modifier::BOLD)
    } else {
        style.fg(ctx.theme.receded_fg)
    };
    buf.set_stringn(left + 3, top, title, span.saturating_sub(2), label);

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

/// A float: anything drawn over other content (interface §3).
///
/// A rounded box on `chrome_bg`, bordered in `float_border` while it has the
/// keyboard and in the rule (`border`) while it does not. Its `name` is cut into
/// the top-left of the border and its `exit` into the top-right.
///
/// **It paints one cell outside `area`**, and only there: a ring of plain page
/// (`bg`) so the border never touches code. That is the one exception to a
/// panel keeping to its rect, and it is the float's whole reason for being
/// readable. The ring is clipped to the buffer, so a float against the frame's
/// edge simply loses that side of it.
///
/// Every cell inside is cleared, symbol and style, before anything is drawn:
/// `frame` only restyles, which is right for a docked panel that paints every
/// cell anyway and wrong for a box over text it does not own.
///
/// No shadow and no animation: a float appears whole in one frame.
pub fn float(area: Rect, buf: &mut Buffer, name: &str, exit: &str, ctx: &RenderContext) {
    let theme = ctx.theme;
    let area = area.intersection(buf.area);

    let gutter = Style::default().fg(theme.fg).bg(theme.bg);
    let ring = Rect::new(
        area.x.saturating_sub(1),
        area.y.saturating_sub(1),
        area.width.saturating_add(2),
        area.height.saturating_add(2),
    )
    .intersection(buf.area);
    for y in ring.top()..ring.bottom() {
        for x in ring.left()..ring.right() {
            if !area.contains((x, y).into()) {
                buf[(x, y)].reset();
                buf[(x, y)].set_style(gutter);
            }
        }
    }

    let fill = Style::default().fg(theme.fg).bg(theme.chrome_bg);
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buf[(x, y)].reset();
            buf[(x, y)].set_style(fill);
        }
    }

    if area.width < 3 || area.height < 2 {
        return;
    }

    let colour = if ctx.is_focused {
        theme.float_border
    } else {
        theme.border
    };
    let style = Style::default().fg(colour).bg(theme.chrome_bg);
    let (left, right) = (area.x, area.right() - 1);
    let (top, bottom) = (area.y, area.bottom() - 1);

    for x in (left + 1)..right {
        buf[(x, top)].set_symbol("─").set_style(style);
        buf[(x, bottom)].set_symbol("─").set_style(style);
    }
    for y in (top + 1)..bottom {
        buf[(left, y)].set_symbol("│").set_style(style);
        buf[(right, y)].set_symbol("│").set_style(style);
    }
    buf[(left, top)].set_symbol("╭").set_style(style);
    buf[(right, top)].set_symbol("╮").set_style(style);
    buf[(left, bottom)].set_symbol("╰").set_style(style);
    buf[(right, bottom)].set_symbol("╯").set_style(style);

    // The exit first, because it is what gives way when the box is narrow and
    // so decides how much room the name gets.
    let exit_at = float_exit(area, exit);
    if let Some((x, width)) = exit_at {
        buf[(x - 1, top)].set_symbol(" ");
        buf.set_stringn(x, top, exit, width as usize, style);
        buf[(x + width, top)].set_symbol(" ");
    }

    // The name is the float's label (interface §4), styled as a docked
    // panel's title is: accent and bold with the keys, receded without.
    let start = left + 3;
    let limit = match exit_at {
        // A space, at least one cell of rule and a space before the exit.
        Some((x, _)) => x.saturating_sub(3),
        None => right,
    };
    if name.is_empty() || start >= limit {
        return;
    }
    let label = if ctx.is_focused {
        style.add_modifier(Modifier::BOLD)
    } else {
        style.fg(theme.receded_fg)
    };
    buf[(start - 1, top)].set_symbol(" ");
    let (end, _) = buf.set_stringn(start, top, name, (limit - start) as usize, label);
    if end < limit {
        buf[(end, top)].set_symbol(" ");
    }
}

/// Where a float's exit label sits on its top border, as `(x, width)`.
///
/// `None` when the box is too narrow to carry it beside a cell of name: the
/// exit gives way before the name does, because the name says what the box
/// is and `esc` works whether or not it is written down. One function for the
/// render and the hit-test, the lesson `inner` already taught.
pub fn float_exit(area: Rect, exit: &str) -> Option<(u16, u16)> {
    let width = u16::try_from(typ_buffer::display_width(exit)).ok()?;
    // `╭─ ` and a cell of name on the left, ` ─ ` before the exit, ` ─╮` after.
    let needed = width.checked_add(10)?;
    if width == 0 || area.width < needed || area.height < 2 {
        return None;
    }
    Some((area.right() - 3 - width, width))
}
