use std::any::Any;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};
use typ_core::{KeyChord, Panel, PanelEvent, RenderContext};

pub struct TreePanel {
    root: PathBuf,
    entries: Vec<PathBuf>,
    selected: usize,
    top_line: usize,
    height: usize,
}

impl TreePanel {
    pub fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            root: root.to_path_buf(),
            entries: read_dir_sorted(root)?,
            selected: 0,
            top_line: 0,
            height: 0,
        })
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn selected(&self) -> Option<&Path> {
        self.entries.get(self.selected).map(PathBuf::as_path)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn move_selection(&mut self, delta: i32) {
        let last = self.entries.len().saturating_sub(1) as i64;
        self.selected = (self.selected as i64 + delta as i64).clamp(0, last) as usize;
        if self.height > 0 {
            if self.selected < self.top_line {
                self.top_line = self.selected;
            } else if self.selected >= self.top_line + self.height {
                self.top_line = self.selected - self.height + 1;
            }
        }
    }

    /// Emit an open event when the selection is a file.
    fn activate(&self) -> Vec<PanelEvent> {
        match self.selected() {
            Some(p) if p.is_file() => vec![PanelEvent::OpenFile {
                path: p.to_path_buf(),
                line: 0,
                col: 0,
            }],
            // Directory expansion is not part of the walking skeleton.
            _ => vec![PanelEvent::NeedsRedraw],
        }
    }
}

/// Directories first, then files, each alphabetical.
fn read_dir_sorted(root: &Path) -> Result<Vec<PathBuf>> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(root)
        .with_context(|| format!("reading {}", root.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort_by_key(|p| {
        (
            !p.is_dir(),
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase(),
        )
    });
    Ok(entries)
}

impl Panel for TreePanel {
    fn name(&self) -> &'static str {
        "tree"
    }

    fn title(&self) -> String {
        self.root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("/")
            .to_string()
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &RenderContext) {
        self.height = area.height as usize;
        let end = (self.top_line + self.height).min(self.entries.len());
        let lines: Vec<Line> = (self.top_line..end)
            .map(|i| {
                let p = &self.entries[i];
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                let label = if p.is_dir() {
                    format!("{name}/")
                } else {
                    name.to_string()
                };
                let style = if i == self.selected {
                    Style::default()
                        .fg(ctx.theme.selection_fg)
                        .bg(ctx.theme.selection_bg)
                } else {
                    Style::default().fg(ctx.theme.fg)
                };
                Line::styled(label, style)
            })
            .collect();
        Paragraph::new(lines)
            .style(Style::default().bg(ctx.theme.bg))
            .render(area, buf);
    }

    fn handle_key(&mut self, chord: KeyChord) -> Vec<PanelEvent> {
        match chord.raw.code {
            KeyCode::Down => self.move_selection(1),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Enter => return self.activate(),
            _ => return Vec::new(),
        }
        vec![PanelEvent::NeedsRedraw]
    }

    fn handle_mouse(&mut self, event: MouseEvent, panel_area: Rect) -> Vec<PanelEvent> {
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return Vec::new();
        }
        let row = event.row.saturating_sub(panel_area.y) as usize;
        let idx = self.top_line + row;
        if idx >= self.entries.len() {
            return Vec::new();
        }
        // Click selects; clicking the already-selected entry activates it,
        // matching how GUI file trees behave.
        if idx == self.selected {
            return self.activate();
        }
        self.selected = idx;
        vec![PanelEvent::NeedsRedraw]
    }

    fn handle_scroll(&mut self, delta: i32, _panel_area: Rect) -> Vec<PanelEvent> {
        let max_top = self.entries.len().saturating_sub(self.height.max(1));
        self.top_line = (self.top_line as i64 + delta as i64).clamp(0, max_top as i64) as usize;
        vec![PanelEvent::NeedsRedraw]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
