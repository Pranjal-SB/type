//! Where everything in the keybindings tab is, and drawing it there.
//!
//! [`KeysView::geometry`] is the one answer to "what is at this cell": the
//! frame draws from it and a click reads it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use typ_core::RenderContext;

use super::record::Recording;
use super::{Binding, KeysView};
use crate::settings::{draw_footer, footer_places, search_row};

/// What the search row says while there is no query.
const SEARCH_HINT: &str = "type to filter \u{b7} :modified :conflict :unbound :kitty";
/// What it says while `ctrl+r` waits for a key.
const CAPTURE_HINT: &str = "press the key to look up";
/// The right-hand columns, each with the gap after it.
const KEY_WIDTH: u16 = 18;
const FLAG_WIDTH: u16 = 7;
const LAYER_WIDTH: u16 = 8;
const SOURCE_WIDTH: u16 = 8;

/// One line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Item {
    /// A labelled rule over an action group.
    Rule(&'static str),
    Row(Binding),
}

/// Something a click does more than select a row with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Target {
    /// A row's key: record over it.
    Key(Binding),
    /// The footer's.
    Unbind,
    Reset,
    Undo,
    Capture,
    OpenRaw,
    /// The float's two buttons.
    Replace,
    Cancel,
}

/// The float's size.
const FLOAT_WIDTH: u16 = 64;
const FLOAT_HEIGHT: u16 = 7;
/// Its buttons, and the gap between them.
const REPLACE: &str = "enter replace";
const CANCEL: &str = "esc cancel";
const BUTTON_GAP: u16 = 5;

pub(super) struct Geometry {
    pub search: Rect,
    /// Every list line on screen, after the scroll.
    pub items: Vec<(Rect, Item)>,
    pub targets: Vec<(Rect, Target)>,
    /// The recording float, while it is up.
    pub float: Option<Rect>,
}

impl KeysView {
    /// The whole list, before the scroll: a rule where the group changes,
    /// then a line a row.
    fn lines(&self) -> Vec<Item> {
        let mut out = Vec::new();
        let mut group = None;
        for row in self.visible() {
            let label = row.1.group().label();
            if group != Some(label) {
                group = Some(label);
                out.push(Item::Rule(label));
            }
            out.push(Item::Row(row));
        }
        out
    }

    fn footer_parts() -> [(&'static str, Target); 5] {
        [
            ("del unbind", Target::Unbind),
            ("r reset", Target::Reset),
            ("ctrl+z undo", Target::Undo),
            ("ctrl+r find", Target::Capture),
            ("ctrl+o keys.toml", Target::OpenRaw),
        ]
    }

    /// Where the key column starts on a row line.
    fn key_x(line: Rect) -> u16 {
        let columns = KEY_WIDTH + FLAG_WIDTH + LAYER_WIDTH + SOURCE_WIDTH;
        line.right().saturating_sub(columns).max(line.x + 3)
    }

    /// The float, centred in the tab.
    fn float_area(area: Rect) -> Rect {
        let width = FLOAT_WIDTH.min(area.width.saturating_sub(4));
        let height = FLOAT_HEIGHT.min(area.height);
        Rect::new(
            area.x + (area.width - width) / 2,
            area.y + (area.height - height) / 2,
            width,
            height,
        )
    }

    /// Below the search row and a blank line, above the footer.
    fn list_area(area: Rect) -> Rect {
        Rect::new(
            area.x,
            area.y + 2,
            area.width,
            area.height.saturating_sub(3),
        )
    }

    pub(super) fn geometry(&self, area: Rect) -> Geometry {
        let search = Rect::new(area.x, area.y, area.width, area.height.min(1));
        let list = Self::list_area(area);
        let items: Vec<(Rect, Item)> = self
            .lines()
            .into_iter()
            .skip(self.top)
            .take(list.height as usize)
            .enumerate()
            .map(|(n, item)| (Rect::new(list.x, list.y + n as u16, list.width, 1), item))
            .collect();
        let parts = Self::footer_parts();
        let texts: Vec<&str> = parts.iter().map(|(text, _)| *text).collect();
        let mut targets: Vec<(Rect, Target)> = footer_places(area, &texts)
            .into_iter()
            .zip(parts)
            .map(|(rect, (_, target))| (rect, target))
            .collect();
        for (rect, item) in &items {
            if let Item::Row(binding) = item {
                let x = Self::key_x(*rect);
                let width = KEY_WIDTH.min(rect.right().saturating_sub(x));
                targets.push((Rect::new(x, rect.y, width, 1), Target::Key(binding.clone())));
            }
        }
        let float = self.recording.as_ref().map(|_| Self::float_area(area));
        if let Some(float) = float {
            let y = float.bottom().saturating_sub(2);
            let x = float.x + 3;
            let replace = REPLACE.len() as u16;
            targets.push((Rect::new(x, y, replace, 1), Target::Replace));
            let cancel = Rect::new(x + replace + BUTTON_GAP, y, CANCEL.len() as u16, 1);
            targets.push((cancel, Target::Cancel));
        }
        Geometry {
            search,
            items,
            targets,
            float,
        }
    }

    /// Scroll so the selected row is on screen.
    fn keep_selection_in_view(&mut self, height: usize) {
        let lines = self.lines();
        self.top = self.top.min(lines.len().saturating_sub(height));
        let Some(selected) = self.selected.clone() else {
            return;
        };
        let Some(at) = lines
            .iter()
            .position(|item| *item == Item::Row(selected.clone()))
        else {
            return;
        };
        let first = if at > 0 && matches!(lines[at - 1], Item::Rule(_)) {
            at - 1
        } else {
            at
        };
        if first < self.top {
            self.top = first;
        } else if at + 1 > self.top + height {
            self.top = (at + 1).saturating_sub(height);
        }
    }

    pub(super) fn draw(&mut self, area: Rect, buf: &mut Buffer, ctx: &RenderContext) {
        let theme = ctx.theme;
        let text = if ctx.is_focused {
            theme.fg
        } else {
            theme.receded_fg
        };
        buf.set_style(area, Style::default().fg(text).bg(theme.bg));
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_symbol(" ");
            }
        }
        self.keep_selection_in_view(Self::list_area(area).height as usize);
        let geometry = self.geometry(area);

        if self.capturing {
            search_row(geometry.search, buf, ctx, true, "", CAPTURE_HINT);
        } else {
            let has_keys = self.selected.is_none();
            search_row(
                geometry.search,
                buf,
                ctx,
                has_keys,
                &self.query,
                SEARCH_HINT,
            );
        }
        for (rect, item) in &geometry.items {
            match item {
                Item::Rule(label) => {
                    typ_core::chrome::labelled_rule(*rect, buf, label, false, theme, theme.bg)
                }
                Item::Row(binding) => self.draw_row(*rect, buf, binding, ctx),
            }
        }
        let parts = Self::footer_parts();
        let texts: Vec<&str> = parts.iter().map(|(text, _)| *text).collect();
        let placed: Vec<Rect> = geometry
            .targets
            .iter()
            .filter(|(_, t)| parts.iter().any(|(_, p)| p == t))
            .map(|(rect, _)| *rect)
            .collect();
        draw_footer(buf, ctx, &texts, &placed);
        if let (Some(float), Some(recording)) = (geometry.float, &self.recording) {
            typ_core::chrome::scrim(buf, area, theme.scrim);
            self.draw_float(float, buf, recording, ctx);
        }
    }

    /// The recording float: what is being rebound, the keycaps, the
    /// capability and conflict lines, and the two buttons.
    fn draw_float(&self, area: Rect, buf: &mut Buffer, recording: &Recording, ctx: &RenderContext) {
        let theme = ctx.theme;
        typ_core::chrome::float(area, buf, "record a key", "esc", ctx);
        let inner = typ_core::chrome::inner(area);
        let room = inner.width.saturating_sub(4) as usize;
        let plain = Style::default().fg(theme.fg).bg(theme.chrome_bg);
        let quiet = Style::default()
            .fg(theme.line_number_fg)
            .bg(theme.chrome_bg);
        let x = inner.x + 2;
        let action = recording.row.1;
        buf.set_stringn(
            x,
            inner.y,
            format!("for: {}", action.description()),
            room,
            quiet,
        );

        let caps: Vec<String> = recording
            .steps
            .iter()
            .map(|step| format!("[{}]", crate::app::spelled(step)))
            .collect();
        let mut keys = if caps.is_empty() {
            "press the key".to_string()
        } else {
            caps.join(" ")
        };
        if recording.waiting {
            keys.push_str(" \u{2026}");
        }
        buf.set_stringn(
            x,
            inner.y + 1,
            keys,
            room,
            plain.add_modifier(Modifier::BOLD),
        );
        if let Some(chord) = recording.chord() {
            buf.set_stringn(x, inner.y + 2, Self::capability(&chord), room, quiet);
            let conflict = self.conflict_line(&chord, action);
            let style = if conflict.starts_with('\u{25b2}') {
                Style::default()
                    .fg(theme.diagnostic_warning)
                    .bg(theme.chrome_bg)
            } else {
                quiet
            };
            buf.set_stringn(x, inner.y + 3, conflict, room, style);
        }
        let buttons = Rect::new(x, area.bottom().saturating_sub(2), inner.width, 1);
        buf.set_stringn(buttons.x, buttons.y, REPLACE, room, plain);
        let cancel = buttons.x + REPLACE.len() as u16 + BUTTON_GAP;
        buf.set_stringn(cancel - 3, buttons.y, "\u{b7}", 1, quiet);
        buf.set_stringn(cancel, buttons.y, CANCEL, room, plain);
    }

    /// One row: marker, description, then key, flag, layer and source in
    /// columns on the right.
    fn draw_row(&self, rect: Rect, buf: &mut Buffer, binding: &Binding, ctx: &RenderContext) {
        let theme = ctx.theme;
        let selected = self.selected.as_ref() == Some(binding);
        let ground = if selected {
            theme.cursor_line_bg
        } else {
            theme.bg
        };
        let text = if ctx.is_focused {
            theme.fg
        } else {
            theme.receded_fg
        };
        let plain = Style::default().fg(text).bg(ground);
        let quiet = Style::default().fg(theme.line_number_fg).bg(ground);
        buf.set_style(rect, plain);

        let mark = if self.conflict(binding) {
            Some(("\u{25b2}", theme.diagnostic_warning))
        } else if self.modified(binding) {
            Some(("\u{25cf}", theme.border_focused))
        } else {
            None
        };
        if let Some((mark, colour)) = mark {
            buf.set_stringn(
                rect.x + 1,
                rect.y,
                mark,
                1,
                Style::default().fg(colour).bg(ground),
            );
        }

        let right = Self::key_x(rect);
        let room = right.saturating_sub(rect.x + 4) as usize;
        let description = if selected {
            plain.add_modifier(Modifier::BOLD)
        } else {
            plain
        };
        buf.set_stringn(
            rect.x + 3,
            rect.y,
            binding.1.description(),
            room,
            description,
        );

        let (chord, flag) = match &binding.0 {
            Some(chord) => (
                crate::app::spelled(chord),
                if Self::kitty(binding) { "kitty" } else { "" },
            ),
            None => ("unbound".to_string(), ""),
        };
        let key_style = if binding.0.is_some() { plain } else { quiet };
        let mut x = right;
        for (cell, width, style) in [
            (chord.as_str(), KEY_WIDTH, key_style),
            (flag, FLAG_WIDTH, quiet),
            ("global", LAYER_WIDTH, quiet),
            (self.source(binding), SOURCE_WIDTH, quiet),
        ] {
            let room = width.min(rect.right().saturating_sub(x)).saturating_sub(1);
            buf.set_stringn(x, rect.y, cell, room as usize, style);
            x += width;
        }
    }
}
