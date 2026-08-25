//! Painting the overlay: a query line, a rule, and the rows under it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use typ_core::{Panel, RenderContext, chrome};
use unicode_segmentation::UnicodeSegmentation;

use crate::Picker;

/// Drawn before the query, so an empty prompt still reads as one.
const CARET: &str = "> ";

pub(crate) fn draw(picker: &mut Picker, area: Rect, buf: &mut Buffer, ctx: &RenderContext) {
    // `frame` fills every cell in the rect, which is what stops the body of the
    // editor showing through the overlay — this panel is the only one drawn
    // over something else, so a cell left unpainted is visibly wrong rather
    // than merely the wrong shade.
    chrome::frame(area, buf, &picker.title(), ctx, ctx.theme.chrome_bg);

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
    let hits = picker.hits();
    let end = (offset + list_rows).min(hits.len());
    // Indexed rather than collected: a `String` per row per frame is the
    // `line_text` trap in miniature — cheap once, and this runs on every
    // keystroke for every visible row.
    for (row, hit) in hits[offset.min(end)..end].iter().enumerate() {
        let y = inner.y + 2 + row as u16;
        if y >= inner.bottom() {
            break;
        }
        let style = if offset + row == selected {
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
}

/// Write `text` at `(x, y)`, stopping at `width` cells.
///
/// Grapheme by grapheme rather than by byte or char: a path can carry anything
/// a filesystem allows, and slicing a `String` by a column count is how a CJK
/// filename ends up half-drawn. Wide graphemes still occupy one cell here —
/// full width-aware layout is `typ-buffer`'s job and the picker has no cursor to
/// keep aligned, so the cost of getting it slightly wrong is a row that ends one
/// column early rather than a mispositioned caret.
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
    for (i, grapheme) in text.graphemes(true).take(width as usize).enumerate() {
        let cell_x = x + i as u16;
        if cell_x >= buf.area.right() || y >= buf.area.bottom() {
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
    }
}
