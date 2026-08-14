use std::any::Any;
use std::path::Path;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph, Widget};
use typ_buffer::{Position, TextBuffer, display_to_grapheme_col, grapheme_to_display_col};
use typ_core::{KeyChord, Panel, PanelEvent, RenderContext};
use unicode_segmentation::UnicodeSegmentation;

const TAB_WIDTH: usize = 4;

pub struct EditorPanel {
    buffer: TextBuffer,
    cursor: Position,
    top_line: usize,
    /// Display column the cursor "wants", preserved across vertical movement
    /// so passing through short lines does not permanently lose the column.
    goal_col: Option<usize>,
    height: usize,
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
            cursor: Position::default(),
            top_line: 0,
            goal_col: None,
            height: 0,
        }
    }

    pub fn cursor(&self) -> Position {
        self.cursor
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

    /// The text area inside the panel's border.
    fn text_area(area: Rect) -> Rect {
        Block::bordered().inner(area)
    }

    fn line_grapheme_count(&self, line: usize) -> usize {
        self.buffer.line_text(line).graphemes(true).count()
    }

    fn last_line(&self) -> usize {
        self.buffer.line_count().saturating_sub(1)
    }

    /// Keep the cursor inside the viewport after any movement.
    fn scroll_to_cursor(&mut self) {
        if self.height == 0 {
            return;
        }
        if self.cursor.line < self.top_line {
            self.top_line = self.cursor.line;
        } else if self.cursor.line >= self.top_line + self.height {
            self.top_line = self.cursor.line - self.height + 1;
        }
    }

    /// Rows a page motion covers. Before the first frame the height is unknown,
    /// so fall back to a screenful rather than moving nowhere.
    fn page(&self) -> usize {
        self.height.max(1)
    }

    /// Pull the cursor back inside the text after the buffer changed underneath
    /// it — undo and redo can shrink the content the cursor was sitting in.
    fn clamp_cursor(&mut self) {
        self.cursor.line = self.cursor.line.min(self.last_line());
        self.cursor.col = self
            .cursor
            .col
            .min(self.line_grapheme_count(self.cursor.line));
        self.goal_col = None;
    }

    fn move_vertical(&mut self, delta: i32) {
        let goal = self.goal_col.unwrap_or_else(|| {
            grapheme_to_display_col(
                &self.buffer.line_text(self.cursor.line),
                self.cursor.col,
                TAB_WIDTH,
            )
        });
        let next =
            (self.cursor.line as i64 + delta as i64).clamp(0, self.last_line() as i64) as usize;
        self.cursor.line = next;
        self.cursor.col = display_to_grapheme_col(&self.buffer.line_text(next), goal, TAB_WIDTH);
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
        let lines: Vec<Line> = (self.top_line..end)
            .map(|i| Line::raw(self.buffer.line_text(i)))
            .collect();
        Paragraph::new(lines)
            .style(Style::default().fg(ctx.theme.fg).bg(ctx.theme.bg))
            .render(inner, buf);
    }

    fn cursor_position(&self, panel_area: Rect) -> Option<(u16, u16)> {
        let inner = Self::text_area(panel_area);
        let row = self.cursor.line.checked_sub(self.top_line)?;
        if row >= inner.height as usize {
            return None;
        }
        let col = grapheme_to_display_col(
            &self.buffer.line_text(self.cursor.line),
            self.cursor.col,
            TAB_WIDTH,
        );
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
                self.buffer.insert_char(self.cursor, '\n');
                self.cursor.line += 1;
                self.cursor.col = 0;
                self.goal_col = None;
            }
            KeyCode::Delete => {
                self.buffer.delete_after(self.cursor);
                self.goal_col = None;
            }
            KeyCode::Home => {
                self.cursor.col = 0;
                self.goal_col = None;
            }
            KeyCode::End => {
                self.cursor.col = self.line_grapheme_count(self.cursor.line);
                self.goal_col = None;
            }
            KeyCode::PageDown => self.move_vertical(self.page() as i32),
            KeyCode::PageUp => self.move_vertical(-(self.page() as i32)),
            KeyCode::Char(c) => {
                self.buffer.insert_char(self.cursor, c);
                self.cursor.col += 1;
                self.goal_col = None;
            }
            KeyCode::Backspace => {
                if self.cursor.col > 0 {
                    self.buffer.delete_before(self.cursor);
                    self.cursor.col -= 1;
                } else if self.cursor.line > 0 {
                    // Joining lines: the cursor lands where the two now meet.
                    let joined_at = self.line_grapheme_count(self.cursor.line - 1);
                    self.buffer.delete_before(self.cursor);
                    self.cursor.line -= 1;
                    self.cursor.col = joined_at;
                }
                self.goal_col = None;
            }
            KeyCode::Left => {
                if self.cursor.col > 0 {
                    self.cursor.col -= 1;
                } else if self.cursor.line > 0 {
                    self.cursor.line -= 1;
                    self.cursor.col = self.line_grapheme_count(self.cursor.line);
                }
                self.goal_col = None;
            }
            KeyCode::Right => {
                if self.cursor.col < self.line_grapheme_count(self.cursor.line) {
                    self.cursor.col += 1;
                } else if self.cursor.line < self.last_line() {
                    self.cursor.line += 1;
                    self.cursor.col = 0;
                }
                self.goal_col = None;
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
        self.cursor = Position {
            line,
            col: display_to_grapheme_col(&self.buffer.line_text(line), col, TAB_WIDTH),
        };
        self.goal_col = None;
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
