//! The menu bar (interface §8 4a): `f10` puts the action groups on the top
//! row, and a group drops its actions down in a `FloatMenu`, the context
//! menu's widget.
//!
//! **Drawn over the top row rather than given one.** Hidden it takes zero
//! rows, and shown it does not reflow the panes either: it is summoned over
//! the rest, like the floats it opens, so nothing under it jumps a row while
//! the user is choosing. `f10` only (plan decision 2).

use anyhow::Result;
use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use typ_core::{Action, Group, KeyChord};

use super::App;
use crate::float_menu::{FloatMenu, Outcome};

/// The status bar's left half over a float, the bar alone, and a dropdown.
const FLOAT_KEYS: &str = "type to filter  \u{b7}  enter runs  \u{b7}  esc closes";
const BAR_KEYS: &str = "\u{2190}\u{2192} groups  \u{b7}  \u{2193} opens  \u{b7}  esc closes";
const DROPDOWN_KEYS: &str = "type to filter  \u{b7}  enter runs  \u{b7}  esc steps back";

/// The bar while it is up: which group is highlighted, and its dropdown if
/// one is open.
pub(crate) struct MenuBar {
    group: usize,
    dropdown: Option<FloatMenu>,
}

/// Where each group's name sits on the top row of `frame`, a space either
/// side of it, clipped to the frame. Render, clicks and `targets` all ask.
pub(crate) fn cells(frame: Rect) -> Vec<(usize, Rect)> {
    let mut x = frame.x.saturating_add(1);
    let mut out = Vec::new();
    for (index, group) in Group::ALL.iter().enumerate() {
        let width = u16::try_from(group.label().len() + 2).unwrap_or(u16::MAX);
        let rect = Rect::new(x, frame.y, width, 1).intersection(frame);
        if !rect.is_empty() {
            out.push((index, rect));
        }
        x = x.saturating_add(width);
    }
    out
}

impl MenuBar {
    /// The open dropdown's rows on screen.
    pub(crate) fn dropdown_rows(&self, frame: Rect) -> Vec<(usize, Rect)> {
        let (body, _) = crate::layout::split_frame(frame);
        self.dropdown
            .as_ref()
            .map_or_else(Vec::new, |menu| menu.row_rects(body))
    }
}

impl App {
    /// The highlighted group and, if its dropdown is open, what it lists.
    pub fn menu_bar(&self) -> Option<(Group, Option<Vec<Action>>)> {
        let bar = self.bar.as_ref()?;
        let rows = bar.dropdown.as_ref().map(FloatMenu::actions);
        Some((Group::ALL[bar.group], rows))
    }

    /// The keys of whichever float has the keyboard, if one does.
    pub(super) fn float_hint(&self) -> Option<&'static str> {
        if self.context.is_some() {
            return Some(FLOAT_KEYS);
        }
        let bar = self.bar.as_ref()?;
        Some(match bar.dropdown {
            Some(_) => DROPDOWN_KEYS,
            None => BAR_KEYS,
        })
    }

    pub(super) fn open_menu_bar(&mut self) {
        self.bar = Some(MenuBar {
            group: 0,
            dropdown: None,
        });
        self.mark_dirty();
    }

    /// A group's actions, hung under its name.
    fn drop_down(&self, group: usize) -> Option<FloatMenu> {
        let frame = self.screen;
        let x = cells(frame)
            .into_iter()
            .find(|(index, _)| *index == group)
            .map_or(frame.x, |(_, rect)| rect.x);
        FloatMenu::new(&[Group::ALL[group]], &self.keymap, (x, frame.y))
    }

    /// A key while the bar is up. `←→` move between groups, in a dropdown
    /// too; `↓` or `enter` open one; `esc` steps back a level; anything else
    /// is the dropdown's, as the context menu's keys are.
    pub(super) fn handle_bar_chord(&mut self, chord: &KeyChord) -> Result<()> {
        let Some(mut bar) = self.bar.take() else {
            return Ok(());
        };
        self.mark_dirty();
        let count = Group::ALL.len();
        let step = match chord.raw.code {
            KeyCode::Left => Some(count - 1),
            KeyCode::Right => Some(1),
            _ => None,
        };
        if let Some(step) = step {
            bar.group = (bar.group + step) % count;
            if bar.dropdown.is_some() {
                bar.dropdown = self.drop_down(bar.group);
            }
        } else if let Some(menu) = bar.dropdown.as_mut() {
            match menu.key(chord) {
                Outcome::Stay => {}
                Outcome::Close => bar.dropdown = None,
                Outcome::Run(action) => return self.apply_named_action(action),
            }
        } else {
            match chord.raw.code {
                KeyCode::Down | KeyCode::Enter => bar.dropdown = self.drop_down(bar.group),
                KeyCode::Esc | KeyCode::F(10) => return Ok(()),
                _ => {}
            }
        }
        self.bar = Some(bar);
        Ok(())
    }

    /// A mouse event while the bar is up. A press on a group's name opens
    /// it, on a dropdown row runs it, anywhere else closes the bar and is not
    /// also a click on what is under it. Returns whether the bar took it.
    pub fn route_bar_mouse(&mut self, event: MouseEvent, frame: Rect) -> Result<bool> {
        if self.bar.is_none() {
            return Ok(false);
        }
        if !matches!(event.kind, MouseEventKind::Down(_)) {
            return Ok(true);
        }
        let Some(mut bar) = self.bar.take() else {
            return Ok(true);
        };
        self.mark_dirty();
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return Ok(true);
        }
        let at = (event.column, event.row).into();
        if let Some((group, _)) = cells(frame).into_iter().find(|(_, r)| r.contains(at)) {
            bar.group = group;
            bar.dropdown = self.drop_down(group);
            self.bar = Some(bar);
            return Ok(true);
        }
        let (body, _) = crate::layout::split_frame(frame);
        if let Some(action) = bar
            .dropdown
            .as_mut()
            .and_then(|menu| menu.click(body, event.column, event.row))
        {
            self.apply_named_action(action)?;
        }
        Ok(true)
    }

    /// The dropdown, then the bar over its top border's outer ring.
    pub(super) fn render_bar(&self, frame: Rect, buf: &mut Buffer) {
        let Some(bar) = &self.bar else {
            return;
        };
        if let Some(menu) = &bar.dropdown {
            let (body, _) = crate::layout::split_frame(frame);
            menu.render(body, buf, &self.float_context(frame));
        }
        let ground = Style::default().fg(self.theme.fg).bg(self.theme.chrome_bg);
        let row = Rect { height: 1, ..frame }.intersection(buf.area);
        for x in row.left()..row.right() {
            buf[(x, row.y)].reset();
            buf[(x, row.y)].set_style(ground);
        }
        for (index, rect) in cells(frame) {
            let style = match index == bar.group {
                true => ground
                    .bg(self.theme.selection_primary_bg)
                    .add_modifier(Modifier::BOLD),
                false => ground,
            };
            let name = format!(" {} ", Group::ALL[index].label());
            buf.set_stringn(rect.x, rect.y, name, rect.width.into(), style);
        }
    }
}
