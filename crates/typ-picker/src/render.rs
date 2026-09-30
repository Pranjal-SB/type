//! Painting the overlay: a query line, a rule, and the rows under it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use typ_core::{Panel, RenderContext, chrome};
use typ_find::LineHit;

use crate::{Mode, Picker};

/// Drawn before the query, so an empty prompt still reads as one.
const CARET: &str = "> ";

pub(crate) fn draw(picker: &mut Picker, area: Rect, buf: &mut Buffer, ctx: &RenderContext) {
    // A float (interface §3): it clears every cell it covers, so nothing of the
    // editor shows through, and keeps a cell of page around its border.
    chrome::float(area, buf, &picker.title(), crate::EXIT, ctx);

    let inner = chrome::inner(area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    // Row 0 is the query, row 1 the rule, the rest is the list. Asked of
    // `Picker` against the *outer* rect so render, the hit-test and the scroll
    // cannot disagree by a row.
    let list_rows = Picker::list_rows(area);
    // Settle the offset once, here, where the height is known. Everything below
    // reads it.
    picker.visible(list_rows);

    draw_query(picker, inner, buf, ctx);
    if inner.height >= 2 {
        draw_rule(inner, buf, ctx);
    }
    draw_rows(picker, inner, buf, ctx, list_rows);
}

fn draw_query(picker: &Picker, inner: Rect, buf: &mut Buffer, ctx: &RenderContext) {
    let style = Style::default()
        .fg(ctx.theme.fg)
        .bg(ctx.theme.chrome_bg)
        .add_modifier(Modifier::BOLD);
    let text = format!("{CARET}{}", picker.query());
    write_clipped(
        buf,
        inner.x,
        inner.y,
        inner.width,
        &text,
        style,
        &[],
        style.fg.unwrap_or(ctx.theme.fg),
    );
}

fn draw_rule(inner: Rect, buf: &mut Buffer, ctx: &RenderContext) {
    let style = Style::default()
        .fg(ctx.theme.border)
        .bg(ctx.theme.chrome_bg);
    for x in inner.x..inner.right() {
        buf[(x, inner.y + 1)].set_symbol("─").set_style(style);
    }
}

fn draw_rows(
    picker: &Picker,
    inner: Rect,
    buf: &mut Buffer,
    ctx: &RenderContext,
    list_rows: usize,
) {
    // The one accent, the same one focus and links already use. A second colour
    // invented here would be a widget mixing its own — see the palette note in
    // `typ_core::panel`, which is what keeps the greys from drifting apart.
    let matched = ctx.theme.status_bar_accent;
    let offset = picker.offset();
    let selected = picker.selected();

    let count = match picker.mode() {
        Mode::Files => picker.hits().len(),
        Mode::Search => picker.lines().len(),
        Mode::Commands => picker.commands().len(),
    };
    let end = (offset + list_rows).min(count);
    let start = offset.min(end);

    for row in 0..end.saturating_sub(start) {
        let y = inner.y + 2 + row as u16;
        if y >= inner.bottom() {
            break;
        }
        let index = start + row;
        let style = if index == selected {
            Style::default()
                .fg(ctx.theme.selection_fg)
                .bg(ctx.theme.selection_primary_bg)
        } else {
            Style::default()
                .fg(ctx.theme.tree_file_fg)
                .bg(ctx.theme.chrome_bg)
        };
        // The selected row's background runs the full width, so the highlight
        // reads as a bar rather than as a differently-coloured filename.
        for x in inner.x..inner.right() {
            buf[(x, y)].set_symbol(" ").set_style(style);
        }

        match picker.mode() {
            Mode::Files => {
                // Borrowed rather than formatted: a `String` per row per frame
                // is the `line_text` trap in miniature — cheap once, and this
                // runs on every keystroke for every visible row.
                let hit = &picker.hits()[index];
                write_clipped(
                    buf,
                    inner.x,
                    y,
                    inner.width,
                    &hit.path,
                    style,
                    &hit.indices,
                    matched,
                );
            }
            Mode::Search => draw_search_row(
                &picker.lines()[index],
                inner,
                y,
                buf,
                ctx,
                style,
                index == selected,
            ),
            Mode::Commands => {
                let row = &picker.commands()[index];
                write_clipped(
                    buf,
                    inner.x,
                    y,
                    inner.width,
                    &row.description,
                    style,
                    &row.indices,
                    matched,
                );
                draw_binding(&row.binding, inner, y, buf, ctx, style);
            }
        }
    }
}

/// The key that runs a command, right-aligned and quiet.
///
/// **Right-aligned because it is a second column, not a suffix.** The names are
/// what the eye scans; the bindings line up beside them so the palette can be
/// read as a keymap listing without being one. Helix's palette put its bindings
/// on the left for exactly the reason its author gave for wanting them on the
/// right, and it has looked like a compromise ever since.
///
/// An unbound action draws nothing at all rather than a dash or a placeholder,
/// which would have to be explained.
fn draw_binding(
    binding: &str,
    inner: Rect,
    y: u16,
    buf: &mut Buffer,
    ctx: &RenderContext,
    style: Style,
) {
    let width = cells(binding);
    if binding.is_empty() || width >= inner.width {
        return;
    }
    write_clipped(
        buf,
        inner.right() - width,
        y,
        width,
        binding,
        style.fg(ctx.theme.status_bar_inactive_fg),
        &[],
        matched_placeholder(ctx),
    );
}

/// Columns `text` occupies. Cells, not graphemes: a CJK grapheme is two.
fn cells(text: &str) -> u16 {
    u16::try_from(typ_buffer::display_width(text)).unwrap_or(u16::MAX)
}

/// `path:line  the matching text`, in two colours.
///
/// The location is quieter than the text: you scan a search result for the line
/// you meant, and the code is what tells you, not the path you already typed.
///
/// **The line number is rendered 1-based.** `LineHit.line` is 0-based because
/// that is what `PanelEvent::OpenFile` takes, and every gutter in every editor —
/// including this one — counts from one. Storing one and showing the other is
/// the only arrangement where neither is surprising.
fn draw_search_row(
    hit: &LineHit,
    inner: Rect,
    y: u16,
    buf: &mut Buffer,
    ctx: &RenderContext,
    style: Style,
    is_selected: bool,
) {
    let location = format!("{}:{}", hit.path, hit.line + 1);
    let location_style = if is_selected {
        style
    } else {
        style.fg(ctx.theme.status_bar_inactive_fg)
    };
    write_clipped(
        buf,
        inner.x,
        y,
        inner.width,
        &location,
        location_style,
        &[],
        matched_placeholder(ctx),
    );

    // Two spaces, the same gap the status bar uses between segments.
    let gap = 2u16;
    let used = cells(&location).saturating_add(gap);
    if used >= inner.width {
        return;
    }
    // Leading whitespace is indentation, and indentation in a one-line excerpt
    // is width spent saying nothing.
    write_clipped(
        buf,
        inner.x + used,
        y,
        inner.width - used,
        hit.text.trim_start(),
        style,
        &[],
        matched_placeholder(ctx),
    );
}

/// `write_clipped` always takes a match colour; with no indices it is never
/// read. Naming it here beats threading an `Option` through the hot path.
fn matched_placeholder(ctx: &RenderContext) -> Color {
    ctx.theme.status_bar_accent
}

/// Write `text` at `(x, y)`, stopping at `width` cells.
///
/// Grapheme by grapheme rather than by byte or char: a path can carry anything
/// a filesystem allows, and slicing a `String` by a column count is how a CJK
/// filename ends up half-drawn. **Each grapheme advances by its width in
/// cells.** This used to advance by one, so everything after a wide grapheme
/// was drawn over that grapheme's second half. Gap 105.
///
/// `matched_indices` names the graphemes the query hit, ascending. They are
/// **grapheme** indices, which is what makes this a walk in step rather than a
/// lookup — see `typ_find::rank`, where that unit is established. Anything past
/// the end of `text` is ignored rather than panicking: the indices arrive from
/// another crate across a channel.
#[allow(clippy::too_many_arguments)]
fn write_clipped(
    buf: &mut Buffer,
    x: u16,
    y: u16,
    width: u16,
    text: &str,
    style: Style,
    matched_indices: &[u32],
    matched_colour: Color,
) {
    // Both sequences are ascending, so one cursor into `matched_indices` walks
    // alongside the graphemes instead of searching it per cell.
    let mut next = matched_indices.iter().copied().peekable();
    let end = x.saturating_add(width).min(buf.area.right());
    let mut cell_x = x;
    // `printable`, not `graphemes`: a path from the walk and a matched line
    // from a searched file are both attacker-reachable, and `set_symbol` hands
    // whatever it is given straight to the terminal. Gap 69.
    for (i, grapheme) in typ_core::printable(text).enumerate() {
        // At least one cell, so a zero-width grapheme cannot stack on the next.
        let columns = cells(grapheme).max(1);
        // A wide grapheme that would straddle the edge is dropped whole.
        if cell_x.saturating_add(columns) > end || y >= buf.area.bottom() {
            break;
        }
        while next.peek().is_some_and(|&index| (index as usize) < i) {
            next.next();
        }
        let style = if next.peek() == Some(&(i as u32)) {
            next.next();
            style.fg(matched_colour).add_modifier(Modifier::BOLD)
        } else {
            style
        };
        buf[(cell_x, y)].set_symbol(grapheme).set_style(style);
        cell_x += columns;
    }
}
