//! A float of actions with their keys: the context menu (interface §8 4b),
//! the menu bar's dropdown, and the code actions a server offers, which lead
//! the context menu over text and are the whole float `ctrl+.` opens.
//!
//! A query over the action table by `Group`, as the `ctrl+k` menu is, so a
//! new action shows up here without a second list. Layout is one function,
//! `layout`, that render, hit-testing and `App::targets` all go through.

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use typ_core::{Action, Group, KeyChord, Keymap, RenderContext};

/// The float's exit label.
const EXIT: &str = "esc";
/// The float's name while nothing is typed.
const NAME: &str = "actions";
/// Its name when it lists only a server's code actions.
const FIXES_NAME: &str = "code actions";
/// The heading over the code actions when other groups are on show too.
const FIXES_HEADING: &str = "fixes";
/// Between a row's description and its key, at the least.
const KEY_GAP: usize = 2;

pub struct FloatMenu {
    rows: Vec<Entry>,
    name: &'static str,
    filter: String,
    /// Into the rows the filter shows.
    selected: usize,
    /// The cell the float hangs from: the pointer, or the caret.
    anchor: (u16, u16),
}

/// What a row does when it is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Action(Action),
    /// The code action at this index in the list the float was built from.
    Fix(usize),
}

struct Entry {
    pick: Pick,
    label: String,
    key: String,
}

impl Entry {
    fn heading(&self) -> &'static str {
        match self.pick {
            Pick::Action(action) => action.group().label(),
            Pick::Fix(_) => FIXES_HEADING,
        }
    }
}

/// What a key did to a float.
pub enum Outcome {
    /// It moved or filtered; the float stays.
    Stay,
    /// `esc`, or `enter` with nothing to run.
    Close,
    /// `enter` on a row.
    Run(Pick),
}

enum Line {
    Heading(&'static str),
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
                    | Action::MenuBar
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
        Self::with_fixes(&[], groups, keymap, anchor)
    }

    /// The code actions titled `fixes` first, then the groups' actions.
    /// `None` when there is nothing to list.
    pub fn with_fixes(
        fixes: &[String],
        groups: &[Group],
        keymap: &Keymap,
        anchor: (u16, u16),
    ) -> Option<Self> {
        let fixes = fixes.iter().enumerate().map(|(index, title)| Entry {
            pick: Pick::Fix(index),
            label: title.clone(),
            key: String::new(),
        });
        let actions = actions_for(groups).into_iter().map(|action| Entry {
            pick: Pick::Action(action),
            label: action.description().to_string(),
            key: keymap
                .bindings_for(action)
                .first()
                .map(|k| k.to_string())
                .unwrap_or_default(),
        });
        let rows: Vec<Entry> = fixes.chain(actions).collect();
        (!rows.is_empty()).then_some(FloatMenu {
            rows,
            name: if groups.is_empty() { FIXES_NAME } else { NAME },
            filter: String::new(),
            selected: 0,
            anchor,
        })
    }

    /// The actions the filter leaves, in the order shown.
    pub fn actions(&self) -> Vec<Action> {
        self.shown()
            .into_iter()
            .filter_map(|i| match self.rows[i].pick {
                Pick::Action(action) => Some(action),
                Pick::Fix(_) => None,
            })
            .collect()
    }

    /// The code actions' titles the filter leaves, in the order shown.
    pub fn fixes(&self) -> Vec<String> {
        self.shown()
            .into_iter()
            .filter(|&i| matches!(self.rows[i].pick, Pick::Fix(_)))
            .map(|i| self.rows[i].label.clone())
            .collect()
    }

    /// The highlighted row's pick.
    pub fn selected(&self) -> Option<Pick> {
        let shown = self.shown();
        Some(self.rows[*shown.get(self.selected)?].pick)
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

    /// A key while the float has the keyboard: typing filters, arrows move,
    /// `enter` runs, `esc` closes. Anything else is ignored rather than let
    /// through to what is behind.
    pub fn key(&mut self, chord: &KeyChord) -> Outcome {
        let plain = !chord
            .raw
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        match chord.raw.code {
            KeyCode::Esc => return Outcome::Close,
            KeyCode::Enter => return self.selected().map_or(Outcome::Close, Outcome::Run),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Backspace => self.backspace(),
            KeyCode::Char(c) if plain => self.type_char(c),
            _ => {}
        }
        Outcome::Stay
    }

    fn shown(&self) -> Vec<usize> {
        (0..self.rows.len())
            .filter(|&i| self.rows[i].label.to_lowercase().contains(&self.filter))
            .collect()
    }

    /// Headings only when more than one group is on show: a dropdown is
    /// one group, and the bar above it already names it.
    fn lines(&self) -> Vec<Line> {
        let shown = self.shown();
        let group_of = |i: usize| self.rows[i].heading();
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
            .map(|r| r.label.chars().count() + KEY_GAP + r.key.chars().count())
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

    /// A left press at a cell: the row there, highlighted and returned.
    pub fn click(&mut self, body: Rect, column: u16, row: u16) -> Option<Pick> {
        self.selected = self.hit(body, column, row)?;
        self.selected()
    }

    pub fn render(&self, body: Rect, buf: &mut Buffer, ctx: &RenderContext) {
        let lines = self.lines();
        let (area, first) = self.layout(body, &lines);
        let name = match self.filter.as_str() {
            "" => self.name,
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
                    buf.set_stringn(inner.x + 1, y, *group, room, style);
                }
                Line::Row(place) => {
                    let row = match *place == self.selected {
                        true => ground.bg(theme.selection_primary_bg),
                        false => ground,
                    };
                    buf.set_style(Rect::new(inner.x, y, inner.width, 1), row);
                    let entry = &self.rows[shown[*place]];
                    buf.set_stringn(inner.x + 1, y, &entry.label, room, row);
                    let key_width = u16::try_from(entry.key.chars().count()).unwrap_or(0);
                    let key_x = inner.right().saturating_sub(key_width + 1).max(inner.x);
                    let key_style = row.fg(theme.status_bar_accent);
                    buf.set_stringn(key_x, y, &entry.key, usize::from(key_width), key_style);
                }
            }
        }
    }
}
