//! Where everything in the settings tab is, and drawing it there.
//!
//! [`SettingsView::geometry`] is the one answer to "what is at this cell":
//! the frame draws from it and a click reads it, so the two cannot disagree
//! about the scroll or the section column.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use typ_core::RenderContext;

use super::edit::Edit;
use super::{SettingsView, slug};
use crate::config::schema::{Kind, ROWS, Row};

/// At this width and wider, the sections get a column of their own (§8b).
pub(super) const WIDE: u16 = 100;
/// The section column, rule excluded.
const COLUMN: u16 = 26;
/// What the search row says before the query.
pub(super) const SEARCH_LABEL: &str = " search  ";
/// What the search row says while there is no query.
const SEARCH_HINT: &str = "type to filter \u{b7} @section \u{b7} :modified";
/// Between the footer's keys.
const FOOTER_GAP: &str = "  \u{b7}  ";

/// One line of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Item {
    /// A labelled rule over a section.
    Rule(&'static str),
    /// A setting's first line: marker, name, control.
    Name(usize),
    /// Its second: description, default, source.
    Detail(usize),
    /// One choice in the list open under row `.0`.
    Choice(usize, usize),
}

/// Something a click does more than select a row with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    /// The left arrow of a choice.
    Less(usize),
    /// The right arrow.
    More(usize),
    /// The value itself: the list, the box, the field, the file.
    Value(usize),
    /// A choice in an open list.
    Choice(usize, usize),
    /// The footer's three.
    Reset,
    Undo,
    OpenRaw,
}

pub(super) struct Geometry {
    pub search: Rect,
    /// The section column's entries, empty below [`WIDE`].
    pub sections: Vec<(Rect, &'static str)>,
    /// The rule between the column and the list, when there is a column.
    pub column_rule: Option<Rect>,
    /// Every list line on screen, after the scroll.
    pub items: Vec<(Rect, Item)>,
    /// The controls and the footer's keys, each a click target.
    pub targets: Vec<(Rect, Target)>,
}

/// Every section, in table order.
fn sections() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for row in ROWS {
        if !out.contains(&row.section) {
            out.push(row.section);
        }
    }
    out
}

impl SettingsView {
    /// The whole list, before the scroll: a rule where the section changes,
    /// then two lines a row.
    fn lines(&self) -> Vec<Item> {
        let mut out = Vec::new();
        let mut section = None;
        for index in self.visible() {
            let row = &ROWS[index];
            if section != Some(row.section) {
                section = Some(row.section);
                out.push(Item::Rule(row.section));
            }
            out.push(Item::Name(index));
            out.push(Item::Detail(index));
            if self.selected == Some(index) && matches!(self.edit, Some(Edit::List { .. })) {
                let count = self.choices(row).len();
                out.extend((0..count).map(|n| Item::Choice(index, n)));
            }
        }
        out
    }

    /// The footer's parts, with what each does.
    fn footer_parts(&self) -> [(String, Target); 3] {
        let file = self.path.file_name().map_or_else(
            || "config.toml".into(),
            |n| n.to_string_lossy().into_owned(),
        );
        [
            ("r reset".to_string(), Target::Reset),
            ("ctrl+z undo".to_string(), Target::Undo),
            (format!("ctrl+o {file}"), Target::OpenRaw),
        ]
    }

    /// Where a row's control sits on its name line: right-aligned, a cell in.
    fn control_rect(&self, line: Rect, row: &Row) -> Rect {
        let width = typ_buffer::display_width(&self.control(row)) as u16;
        let x = line.right().saturating_sub(width + 1).max(line.x);
        Rect::new(x, line.y, width.min(line.width), 1)
    }

    /// The list's area in `area`: below the search row and a blank line,
    /// above the footer, right of the section column when there is one.
    fn list_area(area: Rect) -> Rect {
        let left = if area.width >= WIDE { COLUMN + 1 } else { 0 };
        Rect::new(
            area.x + left,
            area.y + 2,
            area.width.saturating_sub(left),
            area.height.saturating_sub(3),
        )
    }

    pub(super) fn geometry(&self, area: Rect) -> Geometry {
        let search = Rect::new(area.x, area.y, area.width, area.height.min(1));
        let list = Self::list_area(area);
        let wide = area.width >= WIDE;
        let sections = if wide {
            sections()
                .into_iter()
                .enumerate()
                .take(list.height as usize)
                .map(|(n, section)| (Rect::new(area.x, list.y + n as u16, COLUMN, 1), section))
                .collect()
        } else {
            Vec::new()
        };
        let column_rule = wide.then(|| Rect::new(area.x + COLUMN, list.y, 1, list.height));
        let items: Vec<(Rect, Item)> = self
            .lines()
            .into_iter()
            .skip(self.top)
            .take(list.height as usize)
            .enumerate()
            .map(|(n, item)| (Rect::new(list.x, list.y + n as u16, list.width, 1), item))
            .collect();

        let mut targets = Vec::new();
        for (rect, item) in &items {
            match *item {
                Item::Name(index) => {
                    let row = &ROWS[index];
                    let control = self.control_rect(*rect, row);
                    if matches!(row.kind, Kind::Choice(_) | Kind::Theme) && control.width >= 3 {
                        let (x, w) = (control.x, control.width);
                        targets.push((Rect::new(x, rect.y, 1, 1), Target::Less(index)));
                        targets.push((Rect::new(x + w - 1, rect.y, 1, 1), Target::More(index)));
                        targets.push((Rect::new(x + 1, rect.y, w - 2, 1), Target::Value(index)));
                    } else {
                        targets.push((control, Target::Value(index)));
                    }
                }
                Item::Choice(index, n) => targets.push((*rect, Target::Choice(index, n))),
                Item::Rule(_) | Item::Detail(_) => {}
            }
        }

        // The footer, the frame's last line.
        if area.height > 3 {
            let y = area.bottom() - 1;
            let mut x = area.x + 1;
            for (text, target) in self.footer_parts() {
                let width = typ_buffer::display_width(&text) as u16;
                if x + width > area.right() {
                    break;
                }
                targets.push((Rect::new(x, y, width, 1), target));
                x += width + FOOTER_GAP.chars().count() as u16;
            }
        }
        Geometry {
            search,
            sections,
            column_rule,
            items,
            targets,
        }
    }

    /// Scroll so the selected row's two lines are on screen.
    fn keep_selection_in_view(&mut self, height: usize) {
        let lines = self.lines();
        self.top = self.top.min(lines.len().saturating_sub(height));
        let Some(selected) = self.selected else {
            return;
        };
        let Some(at) = lines.iter().position(|item| *item == Item::Name(selected)) else {
            return;
        };
        // The rule above the first row of a section comes into view with it.
        let first = if at > 0 && matches!(lines[at - 1], Item::Rule(_)) {
            at - 1
        } else {
            at
        };
        if first < self.top {
            self.top = first;
        } else if at + 2 > self.top + height {
            self.top = (at + 2).saturating_sub(height);
        }
    }

    pub(super) fn draw(&mut self, area: Rect, buf: &mut Buffer, ctx: &RenderContext) {
        let theme = ctx.theme;
        let text = if ctx.is_focused {
            theme.fg
        } else {
            theme.receded_fg
        };
        let page = Style::default().fg(text).bg(theme.bg);
        buf.set_style(area, page);
        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                buf[(x, y)].set_symbol(" ");
            }
        }
        self.keep_selection_in_view(Self::list_area(area).height as usize);
        let geometry = self.geometry(area);

        self.draw_search(geometry.search, buf, ctx);
        self.draw_sections(&geometry, buf, ctx);
        for (rect, item) in &geometry.items {
            match *item {
                Item::Rule(section) => {
                    typ_core::chrome::labelled_rule(*rect, buf, section, false, theme, theme.bg)
                }
                Item::Name(index) => self.draw_name(*rect, buf, &ROWS[index], index, ctx),
                Item::Detail(index) => self.draw_detail(*rect, buf, &ROWS[index], index, ctx),
                Item::Choice(index, n) => self.draw_choice(*rect, buf, index, n, ctx),
            }
        }
        self.draw_footer(&geometry, buf, ctx);
    }

    fn draw_footer(&self, geometry: &Geometry, buf: &mut Buffer, ctx: &RenderContext) {
        let theme = ctx.theme;
        let quiet = Style::default().fg(theme.line_number_fg).bg(theme.bg);
        let keys = Style::default().fg(theme.fg).bg(theme.bg);
        let parts = self.footer_parts();
        let placed: Vec<&Rect> = geometry
            .targets
            .iter()
            .filter(|(_, t)| matches!(t, Target::Reset | Target::Undo | Target::OpenRaw))
            .map(|(rect, _)| rect)
            .collect();
        for (n, rect) in placed.iter().enumerate() {
            buf.set_stringn(rect.x, rect.y, &parts[n].0, rect.width as usize, keys);
            if n + 1 < placed.len() {
                buf.set_stringn(rect.right(), rect.y, FOOTER_GAP, FOOTER_GAP.len(), quiet);
            }
        }
    }

    /// One line of the list under an enum: a rule down its left, the
    /// highlighted choice on the selection's ground.
    fn draw_choice(
        &self,
        rect: Rect,
        buf: &mut Buffer,
        index: usize,
        n: usize,
        ctx: &RenderContext,
    ) {
        let theme = ctx.theme;
        let at = match self.edit {
            Some(Edit::List { at }) => at,
            _ => 0,
        };
        let ground = if n == at {
            theme.selection_primary_bg
        } else {
            theme.bg
        };
        let border = Style::default().fg(theme.float_border).bg(theme.bg);
        buf.set_stringn(rect.x + 3, rect.y, "\u{2502}", 1, border);
        let choices = self.choices(&ROWS[index]);
        let current = (ROWS[index].show)(&self.settings);
        let mark = if choices.get(n) == Some(&current) {
            "\u{25cf} "
        } else {
            "  "
        };
        let text = format!(" {mark}{}", choices.get(n).map_or("", String::as_str));
        let style = Style::default().fg(theme.fg).bg(ground);
        let width = rect.width.saturating_sub(5).min(28);
        buf.set_style(Rect::new(rect.x + 4, rect.y, width, 1), style);
        buf.set_stringn(rect.x + 4, rect.y, text, width as usize, style);
    }
    fn draw_search(&self, rect: Rect, buf: &mut Buffer, ctx: &RenderContext) {
        let theme = ctx.theme;
        if rect.height == 0 {
            return;
        }
        let label = if self.selected.is_none() && ctx.is_focused {
            Style::default()
                .fg(theme.border_focused)
                .bg(theme.bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.line_number_fg).bg(theme.bg)
        };
        let (x, _) = buf.set_stringn(rect.x, rect.y, SEARCH_LABEL, rect.width as usize, label);
        let room = rect.right().saturating_sub(x) as usize;
        if self.query.is_empty() {
            let hint = Style::default().fg(theme.line_number_fg).bg(theme.bg);
            buf.set_stringn(x, rect.y, SEARCH_HINT, room, hint);
        } else {
            let query = Style::default().fg(theme.fg).bg(theme.bg);
            buf.set_stringn(x, rect.y, &self.query, room, query);
        }
    }

    fn draw_sections(&self, geometry: &Geometry, buf: &mut Buffer, ctx: &RenderContext) {
        let theme = ctx.theme;
        let Some(rule) = geometry.column_rule else {
            return;
        };
        let column = Rect::new(rule.x - COLUMN, rule.y, COLUMN, rule.height);
        buf.set_style(column, Style::default().bg(theme.chrome_bg));
        typ_core::chrome::rule(rule, buf, theme, theme.bg);
        let chosen = self
            .query
            .split_whitespace()
            .find_map(|word| word.strip_prefix('@'));
        for (rect, section) in &geometry.sections {
            let style = if chosen.is_some_and(|c| !c.is_empty() && slug(section).starts_with(c)) {
                Style::default()
                    .fg(theme.border_focused)
                    .bg(theme.chrome_bg)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.tree_file_fg).bg(theme.chrome_bg)
            };
            buf.set_stringn(
                rect.x + 1,
                rect.y,
                section,
                rect.width.saturating_sub(2) as usize,
                style,
            );
        }
    }

    /// The ground of a row's two lines: the cursor line's step when selected.
    fn row_ground(&self, index: usize, ctx: &RenderContext) -> ratatui::style::Color {
        if self.selected == Some(index) {
            ctx.theme.cursor_line_bg
        } else {
            ctx.theme.bg
        }
    }

    fn draw_name(
        &self,
        rect: Rect,
        buf: &mut Buffer,
        row: &Row,
        index: usize,
        ctx: &RenderContext,
    ) {
        let theme = ctx.theme;
        let ground = self.row_ground(index, ctx);
        let text = if ctx.is_focused {
            theme.fg
        } else {
            theme.receded_fg
        };
        buf.set_style(rect, Style::default().fg(text).bg(ground));
        if self.modified(row) {
            let mark = Style::default().fg(theme.border_focused).bg(ground);
            buf.set_stringn(rect.x + 1, rect.y, "\u{25cf}", 1, mark);
        }
        let mut name = Style::default().fg(text).bg(ground);
        if self.selected == Some(index) {
            name = name.add_modifier(Modifier::BOLD);
        }
        buf.set_stringn(
            rect.x + 3,
            rect.y,
            row.key,
            rect.width.saturating_sub(4) as usize,
            name,
        );

        let control = self.control(row);
        let place = self.control_rect(rect, row);
        if place.x > rect.x + 4 + row.key.len() as u16 {
            let refused = self.selected == Some(index)
                && matches!(&self.edit, Some(Edit::Field { error: Some(_), .. }));
            let style = if refused {
                Style::default().fg(theme.diagnostic_error).bg(ground)
            } else {
                Style::default().fg(text).bg(ground)
            };
            buf.set_stringn(place.x, place.y, &control, place.width as usize, style);
        }
    }

    /// The row's control as text: `‹ value ›`, `[x]`, `[ 4 ]`, or the file.
    pub(super) fn control(&self, row: &Row) -> String {
        let mut value = (row.show)(&self.settings);
        if let (Some(Edit::Field { input, .. }), Some(index)) = (&self.edit, self.selected)
            && ROWS[index].key == row.key
        {
            value = format!("{input}\u{2581}");
        }
        match row.kind {
            Kind::Choice(_) | Kind::Theme => format!("\u{2039} {value} \u{203a}"),
            Kind::Toggle => if value == "true" { "[x]" } else { "[ ]" }.to_string(),
            Kind::Number { .. } => format!("[ {value} ]"),
            Kind::File => format!("{value} \u{b7} in the file"),
        }
    }

    fn draw_detail(
        &self,
        rect: Rect,
        buf: &mut Buffer,
        row: &Row,
        index: usize,
        ctx: &RenderContext,
    ) {
        let ground = self.row_ground(index, ctx);
        let quiet = Style::default().fg(ctx.theme.line_number_fg).bg(ground);
        buf.set_style(rect, quiet);
        // A restart is said straight after what the setting is, so it is read
        // before the value is changed and is the last part a narrow row cuts.
        let restart = if row.restart {
            " \u{b7} needs a restart"
        } else {
            ""
        };
        let mut text = format!(
            "{}{restart} \u{b7} default {} \u{b7} {}",
            row.description,
            row.default,
            self.source(row)
        );
        let mut style = quiet;
        if self.selected == Some(index)
            && let Some(Edit::Field {
                error: Some(error), ..
            }) = &self.edit
        {
            text = error.clone();
            style = Style::default().fg(ctx.theme.diagnostic_error).bg(ground);
        }
        buf.set_stringn(
            rect.x + 3,
            rect.y,
            text,
            rect.width.saturating_sub(4) as usize,
            style,
        );
    }
}
