use std::any::Any;
use std::path::Path;

use anyhow::Result;
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Widget};
use typ_buffer::{
    EditKind, Position, SearchQuery, Selection, Selections, TextBuffer, display_to_grapheme_col,
    grapheme_to_display_col,
};
use typ_core::{KeyChord, Panel, PanelEvent, RenderContext};

pub mod actions;
pub mod render;

pub(crate) const TAB_WIDTH: usize = 4;

pub struct EditorPanel {
    pub(crate) buffer: TextBuffer,
    /// Never a bare cursor: a caret is an empty selection, so every editing
    /// path is written once and works for one cursor or thirty.
    pub(crate) selections: Selections,
    pub(crate) top_line: usize,
    /// Leftmost *display* column drawn. Display, not grapheme: a line of CJK
    /// scrolls by cells the way it is drawn, not by characters.
    pub(crate) left_col: usize,
    /// Display column the cursor "wants", preserved across vertical movement
    /// so passing through short lines does not permanently lose the column.
    pub(crate) goal_col: Option<usize>,
    pub(crate) height: usize,
    /// Learned at render time, beside `height`: a panel does not know its size
    /// until it is asked to draw.
    pub(crate) width: usize,
    /// Where the current drag began, so a drag extends from the press rather
    /// than from wherever the cursor happened to be.
    drag_anchor: Option<Position>,
    /// The last cell clicked, so a second click in the same place can mean
    /// "select the word" without a double-click timer.
    last_click: Option<Position>,
}

impl EditorPanel {
    // Mirrors TextBuffer::from_str: infallible construction, so the FromStr
    // trait's Result shape would misrepresent it.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Self::new(TextBuffer::from_str(s))
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        Ok(Self::new(TextBuffer::from_path(path)?))
    }

    fn new(buffer: TextBuffer) -> Self {
        Self {
            buffer,
            selections: Selections::default(),
            top_line: 0,
            left_col: 0,
            goal_col: None,
            height: 0,
            width: 0,
            drag_anchor: None,
            last_click: None,
        }
    }

    pub fn selections(&self) -> &Selections {
        &self.selections
    }

    /// The primary head — where the terminal cursor is drawn.
    pub fn cursor(&self) -> Position {
        self.selections.primary().head
    }

    /// Set selections directly. Test-only: production code goes through
    /// actions, so every path a user can take is one a test can take.
    #[doc(hidden)]
    pub fn set_selections_for_test(&mut self, list: Vec<Selection>) {
        assert!(!list.is_empty(), "selections are never empty");
        let mut selections = Selections::single(list[0]);
        for selection in &list[1..] {
            selections.push(*selection);
        }
        self.selections = selections;
    }

    pub fn top_line(&self) -> usize {
        self.top_line
    }

    pub fn left_col(&self) -> usize {
        self.left_col
    }

    pub fn save(&mut self) -> Result<()> {
        self.buffer.save()
    }

    /// Line contents without the trailing newline.
    pub fn line_text(&self, line: usize) -> String {
        self.buffer.line_text(line)
    }

    pub fn line_count(&self) -> usize {
        self.buffer.line_count()
    }

    /// Collapse to a single caret at `at`, clearing the goal column.
    ///
    /// Every place the old single-cursor code assigned to `self.cursor` now
    /// goes through here, which is what keeps the selection set the only
    /// source of truth. Task 7 replaces these callers with actions.
    pub(crate) fn set_caret(&mut self, at: Position) {
        // Placing the caret ends the undo run, the same as a motion does. This
        // is the mouse's half of that rule: click away mid-word and the next
        // thing typed is a new undo step.
        self.buffer.undo_boundary();
        self.selections.set_single(Selection::caret(at));
        self.goal_col = None;
    }

    /// The text area inside the panel's border.
    fn text_area(area: Rect) -> Rect {
        Block::bordered().inner(area)
    }

    pub(crate) fn line_grapheme_count(&self, line: usize) -> usize {
        self.buffer.line_grapheme_count(line)
    }

    pub(crate) fn last_line(&self) -> usize {
        self.buffer.line_count().saturating_sub(1)
    }

    /// Keep the cursor inside the viewport after any movement.
    pub(crate) fn scroll_to_cursor(&mut self) {
        let cursor = self.cursor();

        if self.height > 0 {
            if cursor.line < self.top_line {
                self.top_line = cursor.line;
            } else if cursor.line >= self.top_line + self.height {
                self.top_line = cursor.line - self.height + 1;
            }
        }

        if self.width > 0 {
            let col = self.cursor_display_col(cursor);
            if col < self.left_col {
                self.left_col = col;
            } else if col >= self.left_col + self.width {
                // Keep the cursor one column inside the right edge so the
                // character being typed is visible rather than flush against
                // the border.
                self.left_col = col + 1 - self.width;
            }
        }
    }

    /// The display column a cursor sits at, tabs expanded.
    fn cursor_display_col(&self, cursor: Position) -> usize {
        self.buffer.with_line_str(cursor.line, |line| {
            grapheme_to_display_col(line, cursor.col, TAB_WIDTH)
        })
    }

    /// Rows a page motion covers. Before the first frame the height is unknown,
    /// so fall back to a screenful rather than moving nowhere.
    pub(crate) fn page(&self) -> usize {
        self.height.max(1)
    }

    /// Every match in the buffer.
    ///
    /// The app asks through here rather than reaching into `self.buffer`: a
    /// panel's internals are not application state, which is the same rule
    /// `RenderContext` enforces pointing the other way.
    ///
    /// ponytail: this scans the whole buffer, which is ~10 ms on a 50k-line
    /// file — fine for answering Enter, too slow to run on every keystroke.
    /// An incremental search box scans the viewport first and completes off
    /// the render thread; see `typ-buffer/tests/perf.rs`.
    pub fn buffer_find_all(&self, query: &SearchQuery) -> Vec<Selection> {
        self.buffer.find_all(query)
    }

    /// Select a range and scroll it into view.
    pub fn select_range(&mut self, selection: Selection) {
        self.selections.set_single(selection);
        self.goal_col = None;
        self.scroll_to_cursor();
    }

    /// Replace every match, as one undo step. Returns how many.
    pub fn replace_all(&mut self, query: &SearchQuery, replacement: &str) -> usize {
        let hits = self.buffer.find_all(query);
        if hits.is_empty() {
            return 0;
        }

        // `Other`, so a replace-all is always its own undo step and never folds
        // into a run of typing that happened either side of it.
        self.buffer
            .begin_edit_group(EditKind::Other, &self.selections);
        // Backwards, so each replacement leaves the earlier hits' positions
        // untouched — the same reason multi-caret edits run in reverse.
        for hit in hits.iter().rev() {
            let (start, end) = hit.range();
            self.buffer.replace_range(start, end, replacement);
        }
        self.buffer.end_edit_group();

        self.clamp_selections();
        hits.len()
    }

    /// Pull every selection back inside the text.
    ///
    /// Only replace-all needs this. Undo and redo restore selections that were
    /// recorded against the very rope being restored, so they are in range by
    /// construction; a replace rewrites text underneath selections that were
    /// never recorded anywhere.
    fn clamp_selections(&mut self) {
        let last_line = self.last_line();
        let buffer = &self.buffer;
        let clamp = |p: Position| {
            let line = p.line.min(last_line);
            Position {
                line,
                col: p.col.min(buffer.line_grapheme_count(line)),
            }
        };
        let clamped: Vec<Selection> = self
            .selections
            .iter()
            .map(|s| Selection {
                anchor: clamp(s.anchor),
                head: clamp(s.head),
            })
            .collect();
        self.set_selections(clamped);
        self.goal_col = None;
    }
}

impl Panel for EditorPanel {
    fn name(&self) -> &'static str {
        "editor"
    }

    fn title(&self) -> String {
        let name = self
            .buffer
            .path()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("untitled")
            .to_string();
        if self.buffer.is_dirty() {
            format!("{name} *")
        } else {
            name
        }
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &RenderContext) {
        let border = if ctx.is_focused {
            ctx.theme.border_focused
        } else {
            ctx.theme.border
        };
        let block = Block::bordered()
            .border_style(Style::default().fg(border))
            .title(self.title());
        let inner = block.inner(area);
        block.render(area, buf);

        self.height = inner.height as usize;
        self.width = inner.width as usize;
        let end = (self.top_line + self.height).min(self.buffer.line_count());
        let selections: Vec<Selection> = self.selections.iter().copied().collect();
        let left_col = self.left_col;
        let lines: Vec<Line> = (self.top_line..end)
            .map(|i| {
                self.buffer.with_line_str(i, |text| {
                    crate::render::styled_line(text, i, left_col, TAB_WIDTH, &selections, ctx.theme)
                })
            })
            .collect();
        Paragraph::new(lines)
            .style(Style::default().fg(ctx.theme.fg).bg(ctx.theme.bg))
            .render(inner, buf);
    }

    fn apply_action(&mut self, action: typ_core::Action) -> Option<Vec<PanelEvent>> {
        self.perform(action)
    }

    fn cursor_position(&self, panel_area: Rect) -> Option<(u16, u16)> {
        let inner = Self::text_area(panel_area);
        let cursor = self.cursor();
        let row = cursor.line.checked_sub(self.top_line)?;
        if row >= inner.height as usize {
            return None;
        }
        // Scrolled off the left edge is as invisible as scrolled off the right,
        // so both answer None rather than clamping to an edge the cursor is not
        // actually at.
        let col = self.cursor_display_col(cursor).checked_sub(self.left_col)?;
        if col >= inner.width as usize {
            return None;
        }
        Some((inner.x + col as u16, inner.y + row as u16))
    }

    /// The editor has no raw-key behavior left.
    ///
    /// Every key that does anything here is a keymap row resolving to an
    /// `Action`, which is the invariant the whole milestone exists to establish:
    /// a primitive reachable only from a key handler is invisible to the
    /// command palette and to the vim layer. The M1-era arms that used to live
    /// here were the last thing violating it.
    fn handle_key(&mut self, _chord: KeyChord) -> Vec<PanelEvent> {
        Vec::new()
    }

    fn handle_mouse(&mut self, event: MouseEvent, panel_area: Rect) -> Vec<PanelEvent> {
        let at = |panel: &Self, event: &MouseEvent| {
            let inner = Self::text_area(panel_area);
            let row = event.row.saturating_sub(inner.y) as usize;
            // Both offsets apply: a click is at a screen cell, and the text
            // under it is `top_line` rows down and `left_col` columns across.
            let col = event.column.saturating_sub(inner.x) as usize + panel.left_col;
            let line = (panel.top_line + row).min(panel.last_line());
            Position {
                line,
                col: panel
                    .buffer
                    .with_line_str(line, |text| display_to_grapheme_col(text, col, TAB_WIDTH)),
            }
        };

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let position = at(self, &event);

                if event.modifiers.contains(KeyModifiers::ALT) {
                    // Alt+click stacks a cursor: the mouse half of
                    // multi-cursor, with Action::AddCursor as the keyboard half.
                    self.selections.push(Selection::caret(position));
                    self.last_click = Some(position);
                    self.drag_anchor = Some(position);
                    return vec![PanelEvent::NeedsRedraw];
                }

                if self.last_click == Some(position) {
                    // A second click in the same cell selects the word under
                    // it. No timing check: clicking the same cell twice is
                    // deliberate, and a double-click timer would put a clock on
                    // the render path to distinguish two things a user does not
                    // confuse.
                    let text = self.buffer.line_text(position.line);
                    if let Some((start, end)) = typ_buffer::word_at(&text, position.col) {
                        self.selections.set_single(Selection {
                            anchor: Position {
                                line: position.line,
                                col: start,
                            },
                            head: Position {
                                line: position.line,
                                col: end,
                            },
                        });
                        self.drag_anchor = None;
                        self.goal_col = None;
                        return vec![PanelEvent::NeedsRedraw];
                    }
                }

                self.set_caret(position);
                self.drag_anchor = Some(position);
                self.last_click = Some(position);
                vec![PanelEvent::NeedsRedraw]
            }

            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(anchor) = self.drag_anchor else {
                    // A drag with no press behind it is not ours: it belongs to
                    // whatever panel the press landed in.
                    return Vec::new();
                };
                let head = at(self, &event);
                self.selections.set_single(Selection { anchor, head });
                self.goal_col = None;
                vec![PanelEvent::NeedsRedraw]
            }

            MouseEventKind::Up(MouseButton::Left) => {
                self.drag_anchor = None;
                Vec::new()
            }

            // Invariant 8 — mouse and keyboard are peers. A clipboard reachable
            // only from the keyboard is half a feature.
            //
            // Right-click *inside* a selection copies it and leaves it standing.
            // Outside one it does nothing: the alternative is copying whatever
            // happens to be selected elsewhere, which silently replaces the
            // clipboard on a misclick.
            MouseEventKind::Down(MouseButton::Right) => {
                let position = at(self, &event);
                let inside = self
                    .selections
                    .iter()
                    .any(|s| !s.is_empty() && s.range().0 <= position && position < s.range().1);
                if !inside {
                    return Vec::new();
                }
                self.perform(typ_core::Action::Copy).unwrap_or_default()
            }

            // Middle-click pastes at the pointer, the X11 convention every
            // terminal user already has in their hands.
            MouseEventKind::Down(MouseButton::Middle) => {
                let position = at(self, &event);
                self.set_caret(position);
                self.last_click = Some(position);
                self.perform(typ_core::Action::Paste).unwrap_or_default()
            }

            _ => Vec::new(),
        }
    }

    fn handle_scroll(&mut self, delta: i32, _panel_area: Rect) -> Vec<PanelEvent> {
        let max_top = self.buffer.line_count().saturating_sub(self.height.max(1));
        self.top_line = (self.top_line as i64 + delta as i64).clamp(0, max_top as i64) as usize;
        vec![PanelEvent::NeedsRedraw]
    }

    fn needs_close_confirmation(&self) -> Option<String> {
        self.buffer
            .is_dirty()
            .then(|| "Unsaved changes. Close anyway?".to_string())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
