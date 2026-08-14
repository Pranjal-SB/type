use std::any::Any;
use std::path::Path;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Widget};
use typ_buffer::{
    Position, Selection, Selections, TextBuffer, display_to_grapheme_col, grapheme_to_display_col,
};
use typ_core::{KeyChord, Panel, PanelEvent, RenderContext};
use unicode_segmentation::UnicodeSegmentation;

pub mod render;

pub(crate) const TAB_WIDTH: usize = 4;

pub struct EditorPanel {
    pub(crate) buffer: TextBuffer,
    /// Never a bare cursor: a caret is an empty selection, so every editing
    /// path is written once and works for one cursor or thirty.
    pub(crate) selections: Selections,
    pub(crate) top_line: usize,
    /// Display column the cursor "wants", preserved across vertical movement
    /// so passing through short lines does not permanently lose the column.
    pub(crate) goal_col: Option<usize>,
    pub(crate) height: usize,
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
            goal_col: None,
            height: 0,
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

    pub fn save(&mut self) -> Result<()> {
        self.buffer.save()
    }

    /// Line contents without the trailing newline.
    pub fn line_text(&self, line: usize) -> String {
        self.buffer.line_text(line)
    }

    /// Collapse to a single caret at `at`, clearing the goal column.
    ///
    /// Every place the old single-cursor code assigned to `self.cursor` now
    /// goes through here, which is what keeps the selection set the only
    /// source of truth. Task 7 replaces these callers with actions.
    pub(crate) fn set_caret(&mut self, at: Position) {
        self.selections.set_single(Selection::caret(at));
        self.goal_col = None;
    }

    /// The text area inside the panel's border.
    fn text_area(area: Rect) -> Rect {
        Block::bordered().inner(area)
    }

    pub(crate) fn line_grapheme_count(&self, line: usize) -> usize {
        self.buffer.line_text(line).graphemes(true).count()
    }

    pub(crate) fn last_line(&self) -> usize {
        self.buffer.line_count().saturating_sub(1)
    }

    /// Keep the cursor inside the viewport after any movement.
    pub(crate) fn scroll_to_cursor(&mut self) {
        if self.height == 0 {
            return;
        }
        let cursor = self.cursor();
        if cursor.line < self.top_line {
            self.top_line = cursor.line;
        } else if cursor.line >= self.top_line + self.height {
            self.top_line = cursor.line - self.height + 1;
        }
    }

    /// Rows a page motion covers. Before the first frame the height is unknown,
    /// so fall back to a screenful rather than moving nowhere.
    pub(crate) fn page(&self) -> usize {
        self.height.max(1)
    }

    /// Pull the cursor back inside the text after the buffer changed underneath
    /// it — undo and redo can shrink the content the cursor was sitting in.
    pub(crate) fn clamp_cursor(&mut self) {
        let last_line = self.last_line();
        let line_len: Vec<usize> = (0..=last_line)
            .map(|i| self.line_grapheme_count(i))
            .collect();
        let clamp = |p: Position| {
            let line = p.line.min(last_line);
            Position {
                line,
                col: p.col.min(line_len[line]),
            }
        };
        self.selections.map_in_place(|s| Selection {
            anchor: clamp(s.anchor),
            head: clamp(s.head),
        });
        self.goal_col = None;
    }

    fn move_vertical(&mut self, delta: i32) {
        let cursor = self.cursor();
        let goal = self.goal_col.unwrap_or_else(|| {
            grapheme_to_display_col(&self.buffer.line_text(cursor.line), cursor.col, TAB_WIDTH)
        });
        let next = (cursor.line as i64 + delta as i64).clamp(0, self.last_line() as i64) as usize;
        let col = display_to_grapheme_col(&self.buffer.line_text(next), goal, TAB_WIDTH);
        self.selections
            .set_single(Selection::caret(Position { line: next, col }));
        self.goal_col = Some(goal);
        self.scroll_to_cursor();
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
        let end = (self.top_line + self.height).min(self.buffer.line_count());
        let selections: Vec<Selection> = self.selections.iter().copied().collect();
        let lines: Vec<Line> = (self.top_line..end)
            .map(|i| {
                crate::render::styled_line(&self.buffer.line_text(i), i, &selections, ctx.theme)
            })
            .collect();
        Paragraph::new(lines)
            .style(Style::default().fg(ctx.theme.fg).bg(ctx.theme.bg))
            .render(inner, buf);
    }

    fn cursor_position(&self, panel_area: Rect) -> Option<(u16, u16)> {
        let inner = Self::text_area(panel_area);
        let cursor = self.cursor();
        let row = cursor.line.checked_sub(self.top_line)?;
        if row >= inner.height as usize {
            return None;
        }
        let col =
            grapheme_to_display_col(&self.buffer.line_text(cursor.line), cursor.col, TAB_WIDTH);
        if col >= inner.width as usize {
            return None;
        }
        Some((inner.x + col as u16, inner.y + row as u16))
    }

    fn handle_key(&mut self, chord: KeyChord) -> Vec<PanelEvent> {
        // Chorded bindings are matched first: without this, Ctrl+Z arrives as
        // KeyCode::Char('z') and gets typed into the buffer.
        match chord.canonical.as_str() {
            "ctrl+z" => {
                self.buffer.undo();
                self.clamp_cursor();
                return vec![PanelEvent::NeedsRedraw];
            }
            "ctrl+y" => {
                self.buffer.redo();
                self.clamp_cursor();
                return vec![PanelEvent::NeedsRedraw];
            }
            _ => {}
        }
        if chord
            .raw
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return Vec::new();
        }

        match chord.raw.code {
            KeyCode::Enter => {
                let at = self.cursor();
                self.buffer.insert_char(at, '\n');
                self.set_caret(Position {
                    line: at.line + 1,
                    col: 0,
                });
            }
            KeyCode::Delete => {
                self.buffer.delete_after(self.cursor());
                self.goal_col = None;
            }
            KeyCode::Home => {
                let at = self.cursor();
                self.set_caret(Position {
                    line: at.line,
                    col: 0,
                });
            }
            KeyCode::End => {
                let at = self.cursor();
                let col = self.line_grapheme_count(at.line);
                self.set_caret(Position { line: at.line, col });
            }
            KeyCode::PageDown => self.move_vertical(self.page() as i32),
            KeyCode::PageUp => self.move_vertical(-(self.page() as i32)),
            KeyCode::Char(c) => {
                let at = self.cursor();
                self.buffer.insert_char(at, c);
                self.set_caret(Position {
                    line: at.line,
                    col: at.col + 1,
                });
            }
            KeyCode::Backspace => {
                let at = self.cursor();
                if at.col > 0 {
                    self.buffer.delete_before(at);
                    self.set_caret(Position {
                        line: at.line,
                        col: at.col - 1,
                    });
                } else if at.line > 0 {
                    // Joining lines: the cursor lands where the two now meet.
                    let joined_at = self.line_grapheme_count(at.line - 1);
                    self.buffer.delete_before(at);
                    self.set_caret(Position {
                        line: at.line - 1,
                        col: joined_at,
                    });
                }
            }
            KeyCode::Left => {
                let at = self.cursor();
                let next = if at.col > 0 {
                    Position {
                        line: at.line,
                        col: at.col - 1,
                    }
                } else if at.line > 0 {
                    Position {
                        line: at.line - 1,
                        col: self.line_grapheme_count(at.line - 1),
                    }
                } else {
                    at
                };
                self.set_caret(next);
            }
            KeyCode::Right => {
                let at = self.cursor();
                let next = if at.col < self.line_grapheme_count(at.line) {
                    Position {
                        line: at.line,
                        col: at.col + 1,
                    }
                } else if at.line < self.last_line() {
                    Position {
                        line: at.line + 1,
                        col: 0,
                    }
                } else {
                    at
                };
                self.set_caret(next);
            }
            KeyCode::Up => self.move_vertical(-1),
            KeyCode::Down => self.move_vertical(1),
            _ => {}
        }
        self.scroll_to_cursor();
        vec![PanelEvent::NeedsRedraw]
    }

    fn handle_mouse(&mut self, event: MouseEvent, panel_area: Rect) -> Vec<PanelEvent> {
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return Vec::new();
        }
        let inner = Self::text_area(panel_area);
        let row = event.row.saturating_sub(inner.y) as usize;
        let col = event.column.saturating_sub(inner.x) as usize;
        let line = (self.top_line + row).min(self.last_line());
        self.set_caret(Position {
            line,
            col: display_to_grapheme_col(&self.buffer.line_text(line), col, TAB_WIDTH),
        });
        vec![PanelEvent::NeedsRedraw]
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
