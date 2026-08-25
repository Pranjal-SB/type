//! The picker overlay: a query line and a list of what matched it.
//!
//! **It knows nothing about filesystems, regexes or ranking.** It holds a query
//! string and whatever rows were last handed to it, and it emits
//! `PanelEvent::OpenFile` when one is chosen. The corpus lives on `typ-find`'s
//! worker, so the render thread's work here is proportional to the visible rows
//! rather than to the repository — which is what lets a 37,000-file project
//! paint in the same time as a three-file one.
//!
//! **There is no "query changed" event, deliberately.** `PanelEvent` is a closed
//! vocabulary of about twelve variants (invariant 6) and a picker is not worth
//! spending one on. The app owns this overlay, so it reads [`Picker::query`]
//! after dispatching a key and issues a filter if it moved. That keeps the
//! panel contract untouched and puts the worker plumbing where the worker
//! already lives.

use std::any::Any;

use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_core::{KeyChord, Panel, PanelEvent, RenderContext};
use typ_find::{FileHit, LineHit};
use unicode_segmentation::UnicodeSegmentation;

mod render;

/// What the query means.
///
/// **Two modes, one widget.** The rows differ and where the corpus lives
/// differs — file candidates are ranked against a list the worker holds, search
/// results are produced by the worker per query — but the query line, the
/// selection, the scrolling and every mouse interaction are identical. A second
/// widget would be the same four hundred lines with different row text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Fuzzy-rank a corpus of paths. `ctrl+p`.
    #[default]
    Files,
    /// Search the project's text. `ctrl+shift+f`.
    Search,
}

/// The overlay.
#[derive(Default)]
pub struct Picker {
    mode: Mode,
    query: String,
    hits: Vec<FileHit>,
    /// Search results. **A separate field from `hits`, not an enum over the
    /// two.** A late result from the mode you are no longer in must not
    /// overwrite the list you are looking at, and one field would make that a
    /// question of arrival order.
    lines: Vec<LineHit>,
    /// False when the last search hit its cap.
    complete: bool,
    /// Index into `hits`. Meaningless when `hits` is empty — ask
    /// [`selection`](Self::selection) rather than reading it directly.
    selected: usize,
    /// First visible row. Moved only to keep `selected` on screen.
    offset: usize,
}

impl Picker {
    pub fn new() -> Self {
        Picker {
            complete: true,
            ..Picker::default()
        }
    }

    /// A picker over the project's text rather than its filenames.
    pub fn search() -> Self {
        Picker {
            mode: Mode::Search,
            complete: true,
            ..Picker::default()
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Replace the search results.
    pub fn set_lines(&mut self, lines: Vec<LineHit>, complete: bool) {
        self.lines = lines;
        self.complete = complete;
        self.clamp_selection();
    }

    pub fn lines(&self) -> &[LineHit] {
        &self.lines
    }

    /// How many rows the current mode has.
    fn len(&self) -> usize {
        match self.mode {
            Mode::Files => self.hits.len(),
            Mode::Search => self.lines.len(),
        }
    }

    /// What Enter or a click on `index` opens.
    ///
    /// `None` when there is no such row — an empty list has no row zero, and
    /// conflating the two is how `OpenFile { path: "" }` reaches the app.
    fn open_at(&self, index: usize) -> Option<PanelEvent> {
        match self.mode {
            Mode::Files => self.hits.get(index).map(|hit| PanelEvent::OpenFile {
                path: hit.path.clone().into(),
                line: 0,
                col: 0,
            }),
            Mode::Search => self.lines.get(index).map(|hit| PanelEvent::OpenFile {
                path: hit.path.clone().into(),
                line: hit.line,
                col: hit.col,
            }),
        }
    }

    fn clamp_selection(&mut self) {
        let len = self.len();
        self.selected = self.selected.min(len.saturating_sub(1));
        if len == 0 {
            self.selected = 0;
            self.offset = 0;
        }
        self.offset = self.offset.min(self.selected);
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// Replace the visible rows.
    ///
    /// Every keystroke lands here with a shorter list than the last one, so the
    /// selection is clamped rather than kept: a selection past the end is an
    /// out-of-bounds index on the very next render.
    pub fn set_hits(&mut self, hits: Vec<FileHit>) {
        self.hits = hits;
        self.clamp_selection();
    }

    pub fn hits(&self) -> &[FileHit] {
        &self.hits
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    /// The chosen row, or `None` when nothing matched.
    ///
    /// Distinct from `selected() == 0`: an empty list has no row zero, and
    /// conflating the two is how `OpenFile { path: "" }` reaches the app.
    pub fn selection(&self) -> Option<&FileHit> {
        self.hits.get(self.selected)
    }

    /// Whether the last search ran to completion.
    pub fn complete(&self) -> bool {
        self.complete
    }

    /// The file rows that fit in `rows` lines, with the selection guaranteed
    /// among them. File mode only — search mode has no `FileHit`s.
    ///
    /// `&mut` because keeping that guarantee means moving the offset, and the
    /// height is not known until someone asks. An earlier draft scrolled inside
    /// `render` instead and left this promise false for every caller that had
    /// not painted a frame first — including the mouse hit-test, which resolves
    /// a click against exactly this slice.
    pub fn visible(&mut self, rows: usize) -> &[FileHit] {
        if rows == 0 {
            return &[];
        }
        self.scroll_into_view(rows);
        let start = self.offset.min(self.hits.len());
        let end = (start + rows).min(self.hits.len());
        &self.hits[start..end]
    }

    /// First visible row.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Choose a row directly — what a mouse click resolves to.
    pub fn select(&mut self, index: usize) {
        if index < self.len() {
            self.selected = index;
        }
    }

    /// Move the selection by `delta`, clamped at both ends.
    ///
    /// Clamped rather than wrapping. A list that wraps means holding Down past
    /// the last row silently returns to the top, and the row under the cursor
    /// stops being predictable from how far you have travelled.
    pub fn move_selection(&mut self, delta: isize) {
        if self.len() == 0 {
            return;
        }
        let last = self.len() - 1;
        let next = self.selected as isize + delta;
        self.selected = next.clamp(0, last as isize) as usize;
    }

    /// Scroll without moving the selection — what a wheel event resolves to.
    pub fn scroll(&mut self, delta: isize, rows: usize) {
        let max = self.len().saturating_sub(rows);
        let next = self.offset as isize + delta;
        self.offset = next.clamp(0, max as isize) as usize;
    }

    /// Bring the selection into a window of `rows` lines.
    ///
    /// Called from `render`, which is the only place that knows how tall the
    /// list actually is — the widget is handed its rect rather than choosing it.
    fn scroll_into_view(&mut self, rows: usize) {
        if rows == 0 {
            return;
        }
        if self.selected < self.offset {
            self.offset = self.selected;
        } else if self.selected >= self.offset + rows {
            self.offset = self.selected + 1 - rows;
        }
    }

    /// How many rows of list fit in `panel_area`.
    ///
    /// Two rows to the border, one to the query, one to the rule. **The single
    /// place that arithmetic lives** — render, the hit-test and the scroll all
    /// ask here, because three copies of "minus four" is three chances for one
    /// of them to drift and land every click a row from the pointer.
    pub fn list_rows(panel_area: Rect) -> usize {
        panel_area.height.saturating_sub(4) as usize
    }

    /// Which list row a screen `y` falls on, if any.
    ///
    /// `None` for the border, the query line and the rule — parts of the
    /// overlay that are not results.
    fn row_at(&self, y: u16, panel_area: Rect) -> Option<usize> {
        let first = panel_area.y.checked_add(3)?;
        if y < first || y >= panel_area.bottom().saturating_sub(1) {
            return None;
        }
        Some((y - first) as usize)
    }

    fn insert(&mut self, c: char) {
        self.query.push(c);
    }

    /// Remove one grapheme, not one byte or one char.
    ///
    /// The same rule `Prompt::delete_backward` follows: the picker accepts the
    /// text the buffer does, and half a combining sequence is neither valid on
    /// screen nor parseable by the matcher.
    fn delete_backward(&mut self) {
        let mut graphemes: Vec<&str> = self.query.graphemes(true).collect();
        graphemes.pop();
        self.query = graphemes.concat();
    }
}

impl Panel for Picker {
    fn name(&self) -> &'static str {
        "picker"
    }

    fn title(&self) -> String {
        match self.mode {
            Mode::Files => "Open file".to_string(),
            // The `+` says the cap stopped it: without it a full list implies
            // the project holds exactly that many matches.
            Mode::Search if !self.complete => format!("Search  {}+ matches", self.lines.len()),
            Mode::Search => format!("Search  {} matches", self.lines.len()),
        }
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &RenderContext) {
        render::draw(self, area, buf, ctx);
    }

    fn handle_key(&mut self, chord: KeyChord) -> Vec<PanelEvent> {
        // A chord is never text, in the picker exactly as in the prompt and the
        // buffer — otherwise Ctrl+P while the picker is open types a "p".
        let is_chorded = chord
            .raw
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);

        match chord.raw.code {
            KeyCode::Esc => return vec![PanelEvent::CloseSelf],
            KeyCode::Enter => {
                // No selection is a real state, not row zero.
                let Some(event) = self.open_at(self.selected) else {
                    return vec![PanelEvent::NeedsRedraw];
                };
                return vec![event];
            }
            KeyCode::Backspace if !is_chorded => self.delete_backward(),
            KeyCode::Char(c) if !is_chorded => self.insert(c),
            KeyCode::Down => self.move_selection(1),
            KeyCode::Up => self.move_selection(-1),
            // A page is resolved at render time against the real height; here
            // it is a fixed jump, which is what every list in the editor does
            // before it has been drawn once.
            KeyCode::PageDown => self.move_selection(PAGE as isize),
            KeyCode::PageUp => self.move_selection(-(PAGE as isize)),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.move_selection(self.len() as isize),
            _ => return Vec::new(),
        }
        vec![PanelEvent::NeedsRedraw]
    }

    /// Invariant 8: a click does what Enter does, a wheel does what Up and Down
    /// do.
    fn handle_mouse(&mut self, event: MouseEvent, panel_area: Rect) -> Vec<PanelEvent> {
        if !matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            return Vec::new();
        }

        let (x, y) = (event.column, event.row);
        let inside = x >= panel_area.x
            && x < panel_area.right()
            && y >= panel_area.y
            && y < panel_area.bottom();
        if !inside {
            // Every GUI picker closes on a click away from it, and a modal
            // whose only exit is the keyboard is what invariant 8 exists to
            // prevent.
            return vec![PanelEvent::CloseSelf];
        }

        let Some(row) = self.row_at(y, panel_area) else {
            // The border, the query line or the rule. All part of the overlay,
            // so not a dismissal, and none of them is a result.
            return Vec::new();
        };

        // **Against the offset, not the whole list.** Clicking the third
        // visible row after scrolling must open the third visible file, not the
        // third file in the project — a distinction that is invisible until
        // someone scrolls, which is why it has a test of its own.
        let index = self.offset + row;
        let Some(event) = self.open_at(index) else {
            // A blank row below the last result.
            return Vec::new();
        };
        self.selected = index;
        vec![event]
    }

    fn handle_scroll(&mut self, delta: i32, panel_area: Rect) -> Vec<PanelEvent> {
        self.scroll(delta as isize, Self::list_rows(panel_area));
        vec![PanelEvent::NeedsRedraw]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// The picker owns Escape while it is open, or the app's own handling
    /// closes something else and leaves the overlay on screen.
    fn captures_escape(&self) -> bool {
        true
    }
}

/// Rows a Page Up or Down moves through.
const PAGE: usize = 10;
