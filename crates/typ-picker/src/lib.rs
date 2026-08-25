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

use crossterm::event::{KeyCode, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_core::{KeyChord, Panel, PanelEvent, RenderContext};
use typ_find::FileHit;
use unicode_segmentation::UnicodeSegmentation;

mod render;

/// The overlay.
#[derive(Default)]
pub struct Picker {
    query: String,
    hits: Vec<FileHit>,
    /// Index into `hits`. Meaningless when `hits` is empty — ask
    /// [`selection`](Self::selection) rather than reading it directly.
    selected: usize,
    /// First visible row. Moved only to keep `selected` on screen.
    offset: usize,
}

impl Picker {
    pub fn new() -> Self {
        Picker::default()
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
        self.selected = self.selected.min(self.hits.len().saturating_sub(1));
        if self.hits.is_empty() {
            self.selected = 0;
            self.offset = 0;
        }
        self.offset = self.offset.min(self.selected);
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

    /// The rows that fit in `rows` lines, with the selection guaranteed among
    /// them.
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
        if index < self.hits.len() {
            self.selected = index;
        }
    }

    /// Move the selection by `delta`, clamped at both ends.
    ///
    /// Clamped rather than wrapping. A list that wraps means holding Down past
    /// the last row silently returns to the top, and the row under the cursor
    /// stops being predictable from how far you have travelled.
    pub fn move_selection(&mut self, delta: isize) {
        if self.hits.is_empty() {
            return;
        }
        let last = self.hits.len() - 1;
        let next = self.selected as isize + delta;
        self.selected = next.clamp(0, last as isize) as usize;
    }

    /// Scroll without moving the selection — what a wheel event resolves to.
    pub fn scroll(&mut self, delta: isize, rows: usize) {
        let max = self.hits.len().saturating_sub(rows);
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
        "Open file".to_string()
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
                // No selection is a real state, not row zero. Emitting an
                // `OpenFile` here with an empty path fails somewhere far from
                // the keypress that caused it.
                let Some(hit) = self.selection() else {
                    return vec![PanelEvent::NeedsRedraw];
                };
                return vec![PanelEvent::OpenFile {
                    path: hit.path.clone().into(),
                    line: 0,
                    col: 0,
                }];
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
            KeyCode::End => self.move_selection(self.hits.len() as isize),
            _ => return Vec::new(),
        }
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
