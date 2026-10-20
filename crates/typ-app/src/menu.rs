//! The `ctrl+k` menu: every row under a pending prefix, in columns by group.
//!
//! Generated from `Resolved::Pending`'s payload and nothing else, so a rebind
//! changes it and nobody has a second list to keep in step (`controls.md` §2).
//!
//! Layout is one function that render and hit-testing both call. Two copies of
//! the arithmetic is how a click lands a row away from the pointer, which the
//! picker and the gutter each learned once already.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use typ_core::{Action, Group, ThemeColors};

/// Before the first column.
const MARGIN: u16 = 1;
/// Between two columns.
const GAP: u16 = 3;
/// Between a row's key and what it does.
const KEY_GAP: u16 = 2;
/// What the status bar says while the menu is up. On the bar rather than a
/// last row of the menu: at 80x24 the menu has a third of the frame, and the
/// row the footer took was the one the groups needed once warp joined them.
pub const FOOTER: &str = "esc cancels";

pub struct Menu {
    prefix: String,
    /// In display order: by group, and by key within one, which is table order.
    rows: Vec<Row>,
    selected: usize,
}

/// One line of the menu: a binding, or a run of them collapsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// The step after the prefix: `f`, or `1…9` for a run.
    pub key: String,
    /// What it runs. For a run, the first of it.
    pub action: Action,
    pub description: &'static str,
    /// A run of digits bound to numbered siblings: which one is the next
    /// key's to say, so a click or `enter` asks for it.
    pub run: bool,
}

/// The fewest consecutive digits that collapse into one row. Two rows read as
/// well as one, and say which two.
const MIN_RUN: usize = 3;

#[derive(Clone, Copy)]
enum Item {
    Heading(Group),
    Row(usize),
}

/// One heading or row, where the layout put it.
struct Placed {
    x: u16,
    line: u16,
    width: u16,
    item: Item,
}

impl Menu {
    pub fn new(prefix: String, mut rows: Vec<(String, Action)>) -> Self {
        // Stable, so the range scan's key order survives inside a group.
        rows.sort_by_key(|(_, action)| action.group());
        Menu {
            prefix,
            rows: collapse(rows),
            selected: 0,
        }
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// The highlighted row.
    pub fn selected(&self) -> Option<&Row> {
        self.rows.get(self.selected)
    }

    pub fn row(&self, index: usize) -> Option<&Row> {
        self.rows.get(index)
    }

    /// Highlight a row, as a click on it does.
    pub fn select(&mut self, index: usize) {
        self.selected = index.min(self.rows.len().saturating_sub(1));
    }

    /// Move the highlight, stopping at either end.
    pub fn move_selection(&mut self, delta: isize) {
        let last = self.rows.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    /// How many rows the menu takes at the bottom of a frame this size.
    ///
    /// The tallest band of columns, capped at a third of the frame. Past
    /// that it scrolls: on 80x24 a taller menu would leave the file behind it
    /// a sliver.
    pub fn height(&self, width: u16, frame_height: u16) -> u16 {
        let cap = (frame_height / 3).max(1);
        let (_, lines) = layout(&self.rows, width, cap);
        lines.min(cap)
    }

    /// How many rows are scrolled out of `area`.
    pub fn hidden(&self, area: Rect) -> usize {
        self.rows.len() - self.row_rects(area).len()
    }

    /// The row under a cell of `area`, if one is.
    pub fn hit(&self, area: Rect, column: u16, row: u16) -> Option<usize> {
        self.row_rects(area)
            .into_iter()
            .find(|(_, rect)| rect.contains((column, row).into()))
            .map(|(index, _)| index)
    }

    /// Every row on screen in `area` and the cells it takes: what a click
    /// can land on, and so what a warp label can.
    pub fn row_rects(&self, area: Rect) -> Vec<(usize, Rect)> {
        let visible = area.height;
        let (placed, _) = layout(&self.rows, area.width, visible);
        let first = self.first_line(&placed, visible);
        placed
            .iter()
            .filter_map(|p| {
                let Item::Row(index) = p.item else {
                    return None;
                };
                let shown = p.line >= first && p.line < first + visible;
                let x = area.x + p.x;
                let width = p.width.min(area.right().saturating_sub(x));
                (shown && width > 0)
                    .then(|| (index, Rect::new(x, area.y + p.line - first, width, 1)))
            })
            .collect()
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, theme: &ThemeColors) {
        let ground = Style::default().fg(theme.fg).bg(theme.chrome_bg);
        buf.set_style(area, ground);
        if area.height == 0 {
            return;
        }

        let visible = area.height;
        let (placed, _) = layout(&self.rows, area.width, visible);
        let first = self.first_line(&placed, visible);
        for p in &placed {
            if p.line < first || p.line >= first + visible {
                continue;
            }
            let (x, y) = (area.x + p.x, area.y + p.line - first);
            let room = area.right().saturating_sub(x);
            match p.item {
                Item::Heading(group) => {
                    let style = ground.add_modifier(Modifier::BOLD);
                    buf.set_stringn(x, y, group.label(), room.into(), style);
                }
                Item::Row(index) => {
                    let row = if index == self.selected {
                        let row = ground.bg(theme.selection_primary_bg);
                        buf.set_style(Rect::new(x, y, p.width.min(room), 1), row);
                        row
                    } else {
                        ground
                    };
                    let item = &self.rows[index];
                    let key_style = row.fg(theme.status_bar_accent);
                    let (after, _) = buf.set_stringn(x, y, &item.key, room.into(), key_style);
                    let at = after + KEY_GAP;
                    let room = area.right().saturating_sub(at);
                    buf.set_stringn(at, y, item.description, room.into(), row);
                }
            }
        }
    }

    /// The first content line on screen: the top, unless the highlight is
    /// further down than `visible` lines reach.
    fn first_line(&self, placed: &[Placed], visible: u16) -> u16 {
        let line = placed
            .iter()
            .find(|p| matches!(p.item, Item::Row(i) if i == self.selected))
            .map_or(0, |p| p.line);
        (line + 1).saturating_sub(visible)
    }
}

/// Where every heading and row goes, and how many lines that takes.
///
/// A column per group, as wide as its widest row. Columns fill a band left to
/// right, and the one that does not fit starts a band underneath: at 80
/// columns the menu wraps rather than running off the edge.
///
/// Packed first: groups stacked into as few lines as fit `width`, no more
/// than `limit`. Only when nothing packs does the menu fall back to bands of
/// one group per column, which may run past `limit` and scroll.
fn layout(rows: &[Row], width: u16, limit: u16) -> (Vec<Placed>, u16) {
    let blocks = blocks(rows);
    let tallest = blocks.iter().map(|b| b.height).max().unwrap_or(0);
    (tallest..=limit)
        .find_map(|lines| pack(&blocks, width, lines))
        .unwrap_or_else(|| bands(rows, width))
}

/// One group's heading and rows, measured.
struct Block {
    group: Group,
    rows: std::ops::Range<usize>,
    width: u16,
    height: u16,
}

/// The groups of `rows`, which are already in group order.
fn blocks(rows: &[Row]) -> Vec<Block> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < rows.len() {
        let group = rows[start].action.group();
        let end = rows[start..]
            .iter()
            .position(|row| row.action.group() != group)
            .map_or(rows.len(), |n| start + n);
        let width = rows[start..end]
            .iter()
            .map(|row| cells(&row.key) + KEY_GAP + cells(row.description))
            .fold(cells(group.label()), u16::max);
        let tall = u16::try_from(end - start).unwrap_or(u16::MAX);
        out.push(Block {
            group,
            rows: start..end,
            width,
            height: tall.saturating_add(1),
        });
        start = end;
    }
    out
}

/// The groups stacked into columns `lines` tall, tallest placed first in the
/// first column with room (first fit decreasing), then each column read in
/// group order and the columns ordered by their first group. `None` when the
/// columns are wider than `width`.
///
/// Why packing: the panes group alone is six lines, and at 80x24 the menu
/// has seven, so a band of one group per column left most groups scrolled
/// out of sight.
fn pack(blocks: &[Block], width: u16, lines: u16) -> Option<(Vec<Placed>, u16)> {
    let mut order: Vec<usize> = (0..blocks.len()).collect();
    // Stable, so groups of one height keep their order.
    order.sort_by_key(|&i| std::cmp::Reverse(blocks[i].height));
    let mut columns: Vec<(u16, Vec<usize>)> = Vec::new();
    for i in order {
        let height = blocks[i].height;
        match columns.iter_mut().find(|(used, _)| used + height <= lines) {
            Some((used, column)) => {
                *used += height;
                column.push(i);
            }
            None => columns.push((height, vec![i])),
        }
    }
    for (_, column) in &mut columns {
        column.sort_unstable();
    }
    columns.sort_by_key(|(_, column)| column[0]);

    let wide = |column: &[usize]| column.iter().map(|&i| blocks[i].width).max().unwrap_or(0);
    let total = columns
        .iter()
        .map(|(_, column)| wide(column))
        .fold(MARGIN, |sum, w| sum.saturating_add(w).saturating_add(GAP))
        .saturating_sub(GAP);
    if total > width {
        return None;
    }

    let mut placed = Vec::new();
    let mut x = MARGIN;
    for (_, column) in &columns {
        let column_width = wide(column);
        let mut line = 0;
        for &i in column {
            let block = &blocks[i];
            placed.push(Placed {
                x,
                line,
                width: column_width,
                item: Item::Heading(block.group),
            });
            for (at, index) in (line + 1..).zip(block.rows.clone()) {
                placed.push(Placed {
                    x,
                    line: at,
                    width: column_width,
                    item: Item::Row(index),
                });
            }
            line += block.height;
        }
        x = x.saturating_add(column_width + GAP);
    }
    Some((placed, lines))
}

/// One group per column, a band of them left to right and the next band
/// underneath when the width runs out.
fn bands(rows: &[Row], width: u16) -> (Vec<Placed>, u16) {
    let mut placed = Vec::new();
    let (mut x, mut top, mut band) = (MARGIN, 0u16, 0u16);
    let mut start = 0;
    while start < rows.len() {
        let group = rows[start].action.group();
        let end = rows[start..]
            .iter()
            .position(|row| row.action.group() != group)
            .map_or(rows.len(), |n| start + n);
        let column = rows[start..end]
            .iter()
            .map(|row| cells(&row.key) + KEY_GAP + cells(row.description))
            .fold(cells(group.label()), u16::max);

        if x > MARGIN && x.saturating_add(column) > width {
            top += band;
            band = 0;
            x = MARGIN;
        }
        placed.push(Placed {
            x,
            line: top,
            width: column,
            item: Item::Heading(group),
        });
        for (line, index) in (top + 1..).zip(start..end) {
            placed.push(Placed {
                x,
                line,
                width: column,
                item: Item::Row(index),
            });
        }
        let tall = u16::try_from(end - start).unwrap_or(u16::MAX);
        band = band.max(tall.saturating_add(1));
        x = x.saturating_add(column + GAP);
        start = end;
    }
    (placed, top + band)
}

/// The rows as the menu shows them: each run of [`MIN_RUN`] or more digits
/// bound to consecutive numbered siblings becomes one row, `1…9  focus
/// pane n`. Generated, so a rebind that breaks the run splits the row.
fn collapse(rows: Vec<(String, Action)>) -> Vec<Row> {
    let single = |(key, action): &(String, Action)| Row {
        key: key.clone(),
        action: *action,
        description: action.description(),
        run: false,
    };
    let mut out = Vec::with_capacity(rows.len());
    let mut start = 0;
    while start < rows.len() {
        let mut end = start + 1;
        while end < rows.len() && continues(&rows[end - 1], &rows[end]) {
            end += 1;
        }
        match rows[start].1.numbered() {
            Some((_, family)) if end - start >= MIN_RUN => out.push(Row {
                key: format!("{}\u{2026}{}", rows[start].0, rows[end - 1].0),
                action: rows[start].1,
                description: family,
                run: true,
            }),
            _ => out.extend(rows[start..end].iter().map(single)),
        }
        start = end;
    }
    out
}

/// Whether `b` is the next of `a`'s run: the next digit, bound to the next
/// sibling of the same family.
fn continues(a: &(String, Action), b: &(String, Action)) -> bool {
    let digit = |key: &str| match key.as_bytes() {
        [d @ b'0'..=b'9'] => Some(d - b'0'),
        _ => None,
    };
    let (Some(x), Some(y)) = (digit(&a.0), digit(&b.0)) else {
        return false;
    };
    match (a.1.numbered(), b.1.numbered()) {
        (Some((m, one)), Some((n, other))) => y == x + 1 && n == m + 1 && one == other,
        _ => false,
    }
}

/// Columns `text` occupies.
fn cells(text: &str) -> u16 {
    u16::try_from(typ_buffer::display_width(text)).unwrap_or(u16::MAX)
}
