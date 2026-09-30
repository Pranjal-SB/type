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
/// The last row of the menu.
pub const FOOTER: &str = "esc cancels";

pub struct Menu {
    prefix: String,
    /// In display order: by group, and by key within one, which is table order.
    rows: Vec<(String, Action)>,
    selected: usize,
}

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
            rows,
            selected: 0,
        }
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    /// The highlighted row, as (its key, its action).
    pub fn selected(&self) -> Option<&(String, Action)> {
        self.rows.get(self.selected)
    }

    pub fn row(&self, index: usize) -> Option<&(String, Action)> {
        self.rows.get(index)
    }

    /// Move the highlight, stopping at either end.
    pub fn move_selection(&mut self, delta: isize) {
        let last = self.rows.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    /// How many rows the menu takes at the bottom of a frame this size.
    ///
    /// The tallest band of columns plus the footer, capped at a third of the
    /// frame. Past that it scrolls: on 80x24 a taller menu would leave the
    /// file behind it a sliver.
    pub fn height(&self, width: u16, frame_height: u16) -> u16 {
        let (_, lines) = layout(&self.rows, width);
        let cap = (frame_height / 3).max(2);
        lines.saturating_add(1).min(cap)
    }

    /// The row under a cell of `area`, if one is.
    pub fn hit(&self, area: Rect, column: u16, row: u16) -> Option<usize> {
        let visible = area.height.saturating_sub(1);
        let (placed, _) = layout(&self.rows, area.width);
        let first = self.first_line(&placed, visible);
        placed.iter().find_map(|p| {
            let Item::Row(index) = p.item else {
                return None;
            };
            let y = area.y + p.line.checked_sub(first)?;
            let x = area.x + p.x;
            let shown = p.line >= first && p.line < first + visible;
            (shown && row == y && column >= x && column < x + p.width).then_some(index)
        })
    }

    pub fn render(&self, area: Rect, buf: &mut Buffer, theme: &ThemeColors) {
        let ground = Style::default().fg(theme.fg).bg(theme.chrome_bg);
        buf.set_style(area, ground);
        if area.height == 0 {
            return;
        }

        let visible = area.height - 1;
        let (placed, _) = layout(&self.rows, area.width);
        let first = self.first_line(&placed, visible);
        let mut hidden = 0;
        for p in &placed {
            if p.line < first || p.line >= first + visible {
                hidden += usize::from(matches!(p.item, Item::Row(_)));
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
                    let (key, action) = &self.rows[index];
                    let key_style = row.fg(theme.status_bar_accent);
                    let (after, _) = buf.set_stringn(x, y, key, room.into(), key_style);
                    let at = after + KEY_GAP;
                    let room = area.right().saturating_sub(at);
                    buf.set_stringn(at, y, action.description(), room.into(), row);
                }
            }
        }

        let footer = match hidden {
            0 => FOOTER.to_string(),
            n => format!("{FOOTER}  \u{2026} {n} more"),
        };
        let quiet = ground.fg(theme.status_bar_inactive_fg);
        let y = area.bottom() - 1;
        buf.set_stringn(area.x + MARGIN, y, footer, area.width.into(), quiet);
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
fn layout(rows: &[(String, Action)], width: u16) -> (Vec<Placed>, u16) {
    let mut placed = Vec::new();
    let (mut x, mut top, mut band) = (MARGIN, 0u16, 0u16);
    let mut start = 0;
    while start < rows.len() {
        let group = rows[start].1.group();
        let end = rows[start..]
            .iter()
            .position(|(_, action)| action.group() != group)
            .map_or(rows.len(), |n| start + n);
        let column = rows[start..end]
            .iter()
            .map(|(key, action)| cells(key) + KEY_GAP + cells(action.description()))
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

/// Columns `text` occupies.
fn cells(text: &str) -> u16 {
    u16::try_from(typ_buffer::display_width(text)).unwrap_or(u16::MAX)
}
