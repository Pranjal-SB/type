//! The completion list (interface §8 2c): what a server offered for the word
//! under the caret.
//!
//! Filtered here, as the word grows, rather than asked again: a keystroke never
//! waits on the server (plan decision 1). The server is asked once per word,
//! and again only when it said its list was incomplete.

use std::collections::HashMap;
use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget, Wrap};
use typ_buffer::{Position, display_width};
use typ_core::RenderContext;
use unicode_segmentation::UnicodeSegmentation;

/// The list's name and exit, cut into its border.
pub const NAME: &str = "completion";
pub const EXIT: &str = "esc";
/// The docs column's name. It has no exit of its own: it goes with the list.
pub const DOCS_NAME: &str = "docs";
/// The narrowest frame that gets a docs column (interface §8 2c).
pub const DOCS_FROM: u16 = 100;
const DOCS_WIDTH: u16 = 44;
const DOCS_MIN_WIDTH: u16 = 12;
const DOCS_MAX_HEIGHT: u16 = 12;
/// Rows on show before the list scrolls.
pub const MAX_ROWS: usize = 10;
/// Wide enough for the border to carry its name and its exit.
const MIN_WIDTH: u16 = 24;
const MAX_WIDTH: u16 = 60;
/// The kind column: the longest label and a space.
const KIND_WIDTH: usize = 7;
/// Between a label and its detail, at the least.
const DETAIL_GAP: usize = 2;

/// One thing a server offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    /// `CompletionItemKind`, as the protocol numbers it.
    pub kind: Option<u64>,
    pub detail: Option<String>,
    pub docs: Option<String>,
    /// What typing is matched against: `filterText`, or the label.
    pub filter: String,
    /// What accepting puts in.
    pub text: String,
    /// Where the server's `textEdit` starts, converted from its encoding.
    /// `None` replaces the word before the caret.
    pub edit_start: Option<Position>,
}

/// The list while it is up.
pub struct List {
    /// The document it was asked about.
    pub path: PathBuf,
    pub items: Vec<Item>,
    /// The server said there is more than it sent, so a longer word asks again.
    pub incomplete: bool,
    /// Where the word being completed starts.
    pub start: Position,
    /// Each item's filter text, kept for ranking on every key.
    filters: Vec<String>,
    /// Indices into `items` the typed word leaves, best first.
    shown: Vec<usize>,
    /// Into `shown`.
    selected: usize,
    /// What `shown` was ranked for.
    query: Option<String>,
}

/// Where the list and its docs column go, and the first row on show.
pub struct Areas {
    pub list: Rect,
    pub docs: Option<Rect>,
    first: usize,
}

impl List {
    pub fn new(path: PathBuf, items: Vec<Item>, incomplete: bool, start: Position) -> Self {
        let filters = items.iter().map(|item| item.filter.clone()).collect();
        List {
            path,
            items,
            incomplete,
            start,
            filters,
            shown: Vec::new(),
            selected: 0,
            query: None,
        }
    }

    /// Narrow to what `query` matches, the picker's ranking (`typ_find::rank`)
    /// on each item's filter text. Returns whether anything is left.
    ///
    /// The choice goes back to the top when the word changes, since the top is
    /// what the new word ranks best.
    pub fn filter(&mut self, query: &str) -> bool {
        if self.query.as_deref() == Some(query) {
            return !self.shown.is_empty();
        }
        // A label can appear twice (an overload, a re-export), so a hit is
        // matched back to the first item with that text not yet taken.
        let mut by_text: HashMap<&str, Vec<usize>> = HashMap::new();
        for (index, text) in self.filters.iter().enumerate().rev() {
            by_text.entry(text.as_str()).or_default().push(index);
        }
        self.shown = typ_find::rank(query, &self.filters, self.filters.len())
            .iter()
            .filter_map(|hit| by_text.get_mut(hit.path.as_str())?.pop())
            .collect();
        self.selected = 0;
        self.query = Some(query.to_string());
        !self.shown.is_empty()
    }

    /// The items on show, best first.
    pub fn shown(&self) -> impl Iterator<Item = &Item> {
        self.shown.iter().map(|&i| &self.items[i])
    }

    pub fn selected_item(&self) -> Option<&Item> {
        self.items.get(*self.shown.get(self.selected)?)
    }

    /// Move the choice, stopping at either end.
    pub fn move_selection(&mut self, delta: isize) {
        let last = self.shown.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    pub fn select(&mut self, place: usize) {
        self.selected = place.min(self.shown.len().saturating_sub(1));
    }

    /// The rest of the chosen item after what is typed: the ghost text.
    ///
    /// Only when the item's text carries on from the typed word. A fuzzy match
    /// whose text does not start with it would draw letters that accepting
    /// does not put there.
    pub fn ghost(&self) -> Option<&str> {
        let typed = self.query.as_deref()?;
        let rest = self.selected_item()?.text.strip_prefix(typed)?;
        (!rest.is_empty() && !rest.contains('\n')).then_some(rest)
    }

    /// Where it goes in `body`, for a caret at `caret` on a word starting at
    /// column `word_x`. Render, the click and `App::targets` all ask this.
    ///
    /// Below the caret's row with a row of page between, as the hover box is;
    /// above when below is short. Labels line up under the word. As wide as its
    /// widest item, so typing does not move its edge.
    pub fn layout(&self, body: Rect, frame_width: u16, caret: (u16, u16), word_x: u16) -> Areas {
        let content = self
            .items
            .iter()
            .map(|item| {
                let detail = item
                    .detail
                    .as_deref()
                    .map_or(0, |d| DETAIL_GAP + display_width(d));
                KIND_WIDTH + display_width(&item.label) + detail
            })
            .max()
            .unwrap_or(0);
        let width = u16::try_from(content + 4)
            .unwrap_or(u16::MAX)
            .clamp(MIN_WIDTH, MAX_WIDTH)
            .min(body.width);
        let needed = u16::try_from(self.shown.len().min(MAX_ROWS) + 2).unwrap_or(u16::MAX);

        // The border and a space before the kind column, then the kind.
        let x = word_x
            .saturating_sub(2 + KIND_WIDTH as u16)
            .min(body.right().saturating_sub(width))
            .max(body.x);
        let (_, y) = caret;
        let below = body.bottom().saturating_sub(y.saturating_add(2));
        let above = y.saturating_sub(body.y).saturating_sub(1);
        let (top, height) = if needed <= below || below >= above {
            (y.saturating_add(2), needed.min(below))
        } else {
            let height = needed.min(above);
            (y - 1 - height, height)
        };
        let list = Rect::new(x, top, width, height).intersection(body);
        let visible = usize::from(list.height.saturating_sub(2));
        let first = (self.selected + 1).saturating_sub(visible);

        let docs_rows = self.selected_item().map_or(0, docs_rows);
        let docs_height = u16::try_from(docs_rows + 2)
            .unwrap_or(u16::MAX)
            .min(DOCS_MAX_HEIGHT);
        let docs_x = list.right().saturating_add(1);
        let docs = (frame_width >= DOCS_FROM && docs_rows > 0 && list.height > 2)
            .then(|| Rect::new(docs_x, list.y, DOCS_WIDTH, docs_height).intersection(body))
            .filter(|docs| docs.width >= DOCS_MIN_WIDTH && docs.height > 2);
        Areas { list, docs, first }
    }

    /// Every row on show and its cells: what a click can land on.
    pub fn row_rects(&self, areas: &Areas) -> Vec<(usize, Rect)> {
        let inner = typ_core::chrome::inner(areas.list);
        (inner.y..inner.bottom())
            .zip(areas.first..self.shown.len())
            .map(|(y, place)| (place, Rect::new(inner.x, y, inner.width, 1)))
            .collect()
    }

    /// The row on show under a cell, if one is.
    pub fn hit(&self, areas: &Areas, column: u16, row: u16) -> Option<usize> {
        self.row_rects(areas)
            .into_iter()
            .find(|(_, rect)| rect.contains((column, row).into()))
            .map(|(place, _)| place)
    }

    /// The list, and its docs column beside it when there is one. `focused`
    /// draws the list; the docs never have the keys.
    pub fn render(&self, areas: &Areas, buf: &mut Buffer, focused: &RenderContext) {
        typ_core::chrome::float(areas.list, buf, NAME, EXIT, focused);
        let theme = focused.theme;
        let ground = Style::default().fg(theme.fg).bg(theme.chrome_bg);
        for (place, rect) in self.row_rects(areas) {
            let row = match place == self.selected {
                true => ground.bg(theme.selection_primary_bg),
                false => ground,
            };
            buf.set_style(rect, row);
            let item = &self.items[self.shown[place]];
            let room = usize::from(rect.width.saturating_sub(2));
            let quiet = row.fg(theme.receded_fg);
            buf.set_stringn(rect.x + 1, rect.y, kind_label(item.kind), room, quiet);
            let label_x = rect.x + 1 + KIND_WIDTH as u16;
            let label_room = room.saturating_sub(KIND_WIDTH);
            let (end, _) = buf.set_stringn(label_x, rect.y, &item.label, label_room, row);
            if let Some(detail) = item.detail.as_deref() {
                let width = u16::try_from(display_width(detail)).unwrap_or(u16::MAX);
                let detail_x = rect.right().saturating_sub(width.saturating_add(1));
                if detail_x >= end.saturating_add(DETAIL_GAP as u16) {
                    buf.set_stringn(detail_x, rect.y, detail, usize::from(width), quiet);
                }
            }
        }

        let (Some(docs), Some(item)) = (areas.docs, self.selected_item()) else {
            return;
        };
        let unfocused = RenderContext {
            is_focused: false,
            ..*focused
        };
        typ_core::chrome::float(docs, buf, DOCS_NAME, "", &unfocused);
        let lines: Vec<Line> = docs_lines(item).into_iter().map(Line::raw).collect();
        let inner = typ_core::chrome::inner(docs);
        let inner = Rect::new(
            inner.x + 1,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height,
        );
        Paragraph::new(lines)
            .style(ground)
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }
}

/// What the docs column says about an item: its detail, then its docs.
fn docs_lines(item: &Item) -> Vec<&str> {
    let mut lines: Vec<&str> = item.detail.as_deref().into_iter().collect();
    if let Some(text) = item.docs.as_deref() {
        if !lines.is_empty() {
            lines.push("");
        }
        lines.extend(text.lines());
    }
    lines
}

/// Rows the docs column needs at its width, its lines wrapped.
fn docs_rows(item: &Item) -> usize {
    let width = usize::from(DOCS_WIDTH - 4);
    docs_lines(item)
        .iter()
        .map(|line| display_width(line).div_ceil(width).max(1))
        .sum()
}

/// A `CompletionItemKind` as the list's first column says it.
fn kind_label(kind: Option<u64>) -> &'static str {
    match kind {
        Some(1) => "text",
        Some(2) => "method",
        Some(3) => "fn",
        Some(4) => "new",
        Some(5) => "field",
        Some(6) => "var",
        Some(7) => "class",
        Some(8) => "trait",
        Some(9) => "mod",
        Some(10) => "prop",
        Some(11) => "unit",
        Some(12) => "value",
        Some(13) => "enum",
        Some(14) => "kw",
        Some(15) => "snip",
        Some(16) => "color",
        Some(17) => "file",
        Some(18) => "ref",
        Some(19) => "dir",
        Some(20) => "member",
        Some(21) => "const",
        Some(22) => "struct",
        Some(23) => "event",
        Some(24) => "op",
        Some(25) => "type",
        _ => "",
    }
}

/// Whether a character is part of a word, for completion's purposes.
///
/// Letters, digits and `_` in every script, which is what an identifier is in
/// every language TYPE ships a grammar for.
pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The column the word ending at `col` starts at, in graphemes.
pub fn word_start(line: &str, col: usize) -> usize {
    let before: Vec<&str> = line.graphemes(true).take(col).collect();
    let word = before
        .iter()
        .rev()
        .take_while(|g| g.chars().all(is_word))
        .count();
    before.len() - word
}

/// The graphemes of `line` from `from` up to `to`.
pub fn between(line: &str, from: usize, to: usize) -> String {
    line.graphemes(true)
        .skip(from)
        .take(to.saturating_sub(from))
        .collect()
}
