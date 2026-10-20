//! A float of actions with their keys: the context menu (interface §8 4b),
//! and the menu bar's dropdown.
//!
//! A query over the action table by `Group`, as the `ctrl+k` menu is, so a
//! new action shows up here without a second list. Layout is one function,
//! `layout`, that render, hit-testing and `App::targets` all go through.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use typ_core::{Action, Group, Keymap, RenderContext};

/// The float's exit label.
const EXIT: &str = "esc";
/// The float's name while nothing is typed.
const NAME: &str = "actions";
/// Between a row's description and its key, at the least.
const KEY_GAP: usize = 2;

pub struct FloatMenu {
    rows: Vec<Entry>,
    filter: String,
    /// Into the rows the filter shows.
    selected: usize,
    /// The cell the float hangs from: the pointer, or the caret.
    anchor: (u16, u16),
}

struct Entry {
    action: Action,
    key: String,
}

enum Line {
    Heading(Group),
    /// Its place among the rows shown.
    Row(usize),
}

/// What a float lists for these groups, in group order and table order
/// within one.
///
/// Leaves out what is typed rather than chosen (caret motion, deletes, a
/// newline), the numbered siblings the `ctrl+k` menu folds into a run, and
/// the doors themselves: a row that opens a menu from a menu is a no-op.
pub fn actions_for(groups: &[Group]) -> Vec<Action> {
    let mut out: Vec<Action> = Action::ALL
        .iter()
        .copied()
        .filter(|a| groups.contains(&a.group()))
        .filter(|a| {
            !matches!(
                a,
                Action::Move { .. }
                    | Action::Delete { .. }
                    | Action::InsertNewline
                    | Action::ContextMenu
                    | Action::OpenMenu
                    | Action::OpenCommandPalette
            ) && a.numbered().is_none()
        })
        .collect();
    out.sort_by_key(Action::group);
    out
}

impl FloatMenu {
    /// `None` when the groups hold nothing to list.
    pub fn new(groups: &[Group], keymap: &Keymap, anchor: (u16, u16)) -> Option<Self> {
        let rows: Vec<Entry> = actions_for(groups)
            .into_iter()
            .map(|action| Entry {
                action,
                key: keymap
                    .bindings_for(action)
                    .first()
                    .map(|k| k.to_string())
                    .unwrap_or_default(),
            })
            .collect();
        (!rows.is_empty()).then_some(FloatMenu {
            rows,
            filter: String::new(),
            selected: 0,
            anchor,
        })
    }

    /// The actions the filter leaves, in the order shown.
    pub fn actions(&self) -> Vec<Action> {
        self.shown()
            .into_iter()
            .map(|i| self.rows[i].action)
            .collect()
    }

    /// The highlighted row's action.
    pub fn selected(&self) -> Option<Action> {
        self.actions().get(self.selected).copied()
    }

    /// Highlight a shown row, as a click on it does.
    pub fn select(&mut self, index: usize) {
        self.selected = index;
    }

    /// Move the highlight, stopping at either end.
    pub fn move_selection(&mut self, delta: isize) {
        let last = self.shown().len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    /// Narrow the rows to those whose description contains what is typed.
    pub fn type_char(&mut self, c: char) {
        self.filter.extend(c.to_lowercase());
        self.selected = 0;
    }

    pub fn backspace(&mut self) {
        self.filter.pop();
        self.selected = 0;
    }

    fn shown(&self) -> Vec<usize> {
        (0..self.rows.len())
            .filter(|&i| self.rows[i].action.description().contains(&self.filter))
            .collect()
    }

    /// Headings only when more than one group is on show: a dropdown is
    /// one group, and the bar above it already names it.
    fn lines(&self) -> Vec<Line> {
        let shown = self.shown();
        let group_of = |i: usize| self.rows[i].action.group();
        // Sorted by group, so two groups means two neighbours differ.
        let several = shown.windows(2).any(|w| group_of(w[0]) != group_of(w[1]));
        let mut out = Vec::new();
        let mut last = None;
        for (place, &i) in shown.iter().enumerate() {
            let group = group_of(i);
            if several && last != Some(group) {
                out.push(Line::Heading(group));
            }
            last = Some(group);
            out.push(Line::Row(place));
        }
        out
    }

    /// The float's box in `body`, and the first line on screen.
    ///
    /// Below the anchor when it fits, or when below has at least as much room
    /// as above; above otherwise. Too tall for either and it scrolls. As wide
    /// as its widest row with every row in, so typing does not move its edge.
    fn layout(&self, body: Rect, lines: &[Line]) -> (Rect, usize) {
        let content = self
            .rows
            .iter()
            .map(|r| r.action.description().chars().count() + KEY_GAP + r.key.chars().count())
            .max()
            .unwrap_or(0);
        // A border and a space either side.
        let width = u16::try_from(content + 4)
            .unwrap_or(u16::MAX)
            .min(body.width);
        let needed = u16::try_from(lines.len() + 2).unwrap_or(u16::MAX);
        let (x, y) = self.anchor;
        let x = x.min(body.right().saturating_sub(width)).max(body.x);
        let below = body.bottom().saturating_sub(y.saturating_add(1));
        let above = y.saturating_sub(body.y);
        let (top, height) = if needed <= below || below >= above {
            (y.saturating_add(1), needed.min(below))
        } else {
            let height = needed.min(above);
            (y - height, height)
        };
        let area = Rect::new(x, top, width, height).intersection(body);
        let visible = usize::from(area.height.saturating_sub(2));
        let at = lines
            .iter()
            .position(|l| matches!(l, Line::Row(p) if *p == self.selected))
            .unwrap_or(0);
        (area, (at + 1).saturating_sub(visible))
    }

    /// Every shown row on screen and its cells: what a click can land on.
    pub fn row_rects(&self, body: Rect) -> Vec<(usize, Rect)> {
        let lines = self.lines();
        let (area, first) = self.layout(body, &lines);
        let inner = typ_core::chrome::inner(area);
        (inner.y..inner.bottom())
            .zip(lines.iter().skip(first))
            .filter_map(|(y, line)| match line {
                Line::Row(place) => Some((*place, Rect::new(inner.x, y, inner.width, 1))),
                Line::Heading(_) => None,
            })
            .collect()
    }

    /// The shown row under a cell, if one is.
    pub fn hit(&self, body: Rect, column: u16, row: u16) -> Option<usize> {
        self.row_rects(body)
            .into_iter()
            .find(|(_, rect)| rect.contains((column, row).into()))
            .map(|(place, _)| place)
    }

    pub fn render(&self, body: Rect, buf: &mut Buffer, ctx: &RenderContext) {
        let lines = self.lines();
        let (area, first) = self.layout(body, &lines);
        let name = match self.filter.as_str() {
            "" => NAME,
            typed => typed,
        };
        typ_core::chrome::float(area, buf, name, EXIT, ctx);

        let theme = ctx.theme;
        let ground = Style::default().fg(theme.fg).bg(theme.chrome_bg);
        let inner = typ_core::chrome::inner(area);
        let shown = self.shown();
        for (y, line) in (inner.y..inner.bottom()).zip(lines.iter().skip(first)) {
            let room = usize::from(inner.width.saturating_sub(1));
            match line {
                Line::Heading(group) => {
                    let style = ground.add_modifier(Modifier::BOLD);
                    buf.set_stringn(inner.x + 1, y, group.label(), room, style);
                }
                Line::Row(place) => {
                    let row = match *place == self.selected {
                        true => ground.bg(theme.selection_primary_bg),
                        false => ground,
                    };
                    buf.set_style(Rect::new(inner.x, y, inner.width, 1), row);
                    let entry = &self.rows[shown[*place]];
                    buf.set_stringn(inner.x + 1, y, entry.action.description(), room, row);
                    let key_width = u16::try_from(entry.key.chars().count()).unwrap_or(0);
                    let key_x = inner.right().saturating_sub(key_width + 1).max(inner.x);
                    let key_style = row.fg(theme.status_bar_accent);
                    buf.set_stringn(key_x, y, &entry.key, usize::from(key_width), key_style);
                }
            }
        }
    }
}
