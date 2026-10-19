//! What the app draws between and around regions, and the float.
//!
//! **Lines mean docked, boxes mean floating** (interface §0.3). A docked panel
//! draws its content and nothing around it; the app separates regions with a
//! [`rule`] and heads a dock with a [`labelled_rule`]. Only something drawn
//! over other content gets a box, and that box is a [`float`].
//!
//! Until M4 every panel drew its own box and the layout overlapped two rects by
//! a column so their corners merged into tees. That made the panel responsible
//! for something it could not see, the panel beside it, and charged every panel
//! a column and two rows. The app knows where the regions meet; the app draws
//! the line.

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

use crate::{RenderContext, ThemeColors};

/// The content area inside a float's border.
///
/// `text_area`-style arithmetic and mouse hit-testing both subtract it, and a
/// one-cell drift lands every click a column from the pointer, so render and
/// hit-test ask this one function.
pub fn inner(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1).min(area.right()),
        y: area.y.saturating_add(1).min(area.bottom()),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

/// A vertical rule down every row of `area`'s first column (interface §2).
///
/// The line between two docked regions: in `border`, the 3:1 slot, whichever
/// side has focus. Focus is said by the label, not by the line.
pub fn rule(area: Rect, buf: &mut Buffer, theme: &ThemeColors, background: Color) {
    let area = area.intersection(buf.area);
    if area.width == 0 {
        return;
    }
    let style = Style::default().fg(theme.border).bg(background);
    for y in area.top()..area.bottom() {
        buf[(area.x, y)].set_symbol("│").set_style(style);
    }
}

/// A horizontal rule along `area`'s first row (interface §2).
///
/// Between two regions stacked one above the other, and the plain half of a
/// [`labelled_rule`].
pub fn horizontal_rule(area: Rect, buf: &mut Buffer, theme: &ThemeColors, background: Color) {
    let area = area.intersection(buf.area);
    if area.height == 0 {
        return;
    }
    let style = Style::default().fg(theme.border).bg(background);
    for x in area.left()..area.right() {
        buf[(x, area.y)].set_symbol("─").set_style(style);
    }
}

/// A docked region's heading: `── label ───…` along `area`'s first row.
///
/// Interface §2 option A. The rule is `border`; the label recedes while the
/// region is unfocused, and when it has focus the label turns accent and bold
/// and the two cells of rule before it turn accent too. The rest of the rule
/// stays the rule.
///
/// The label is clipped, never wrapped, and written through `set_stringn`, which
/// drops control characters: it is a directory name, and those are chosen by
/// whoever made the repository.
pub fn labelled_rule(
    area: Rect,
    buf: &mut Buffer,
    label: &str,
    focused: bool,
    theme: &ThemeColors,
    background: Color,
) {
    let area = area.intersection(buf.area);
    if area.height == 0 || area.width == 0 {
        return;
    }
    horizontal_rule(area, buf, theme, background);

    let style = Style::default().fg(theme.border).bg(background);
    let (lead, text) = if focused {
        let accent = style.fg(theme.border_focused);
        (accent, accent.add_modifier(Modifier::BOLD))
    } else {
        (style, style.fg(theme.receded_fg))
    };
    for x in area.x..area.right().min(area.x + 2) {
        buf[(x, area.y)].set_style(lead);
    }

    // `── ` then the label then a space, and at least one cell of rule after.
    let start = area.x + 3;
    if label.is_empty() || start + 3 > area.right() {
        return;
    }
    buf[(start - 1, area.y)].set_symbol(" ");
    let room = (area.right() - start - 2) as usize;
    let (end, _) = buf.set_stringn(start, area.y, label, room, text);
    if end < area.right() {
        buf[(end, area.y)].set_symbol(" ");
    }
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
/// a docked panel only restyles its rect, which is right for something that
/// paints every cell anyway and wrong for a box over text it does not own.
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
    let exit_at = float_exit(area, name, exit);
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
    if end < right {
        buf[(end, top)].set_symbol(" ");
    }
}

/// Where a float's exit label sits on its top border, as `(x, width)`.
///
/// `None` when the box is too narrow to carry it beside the whole name: the
/// exit gives way before the name is cut, because the name says what the box
/// is and `esc` works whether or not it is written down. One function for the
/// render and the hit-test, the lesson `inner` already taught.
pub fn float_exit(area: Rect, name: &str, exit: &str) -> Option<(u16, u16)> {
    let width = u16::try_from(typ_buffer::display_width(exit)).ok()?;
    let named = u16::try_from(typ_buffer::display_width(name)).ok()?;
    // `╭─ name `, a cell of rule, ` exit ─╮`.
    let needed = named.checked_add(width)?.checked_add(9)?;
    if width == 0 || area.width < needed || area.height < 2 {
        return None;
    }
    Some((area.right() - 3 - width, width))
}

/// Repaint `area` one step back, behind a modal float (interface §3).
///
/// The ground becomes `scrim` and the text takes SGR dim, so what was there
/// stays legible enough to keep your place and plainly is not where the keys
/// go. The symbols are left alone. The caller passes the area, and it never
/// includes the status bar: that still says what the editor is doing.
pub fn scrim(buf: &mut Buffer, area: Rect, scrim: Color) {
    let area = area.intersection(buf.area);
    let style = Style::default().bg(scrim).add_modifier(Modifier::DIM);
    buf.set_style(area, style);
}
