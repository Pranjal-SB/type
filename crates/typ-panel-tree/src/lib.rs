use std::any::Any;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};
use typ_core::{KeyChord, Panel, PanelEvent, RenderContext};

/// One visible row of the tree.
#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    /// Nesting level below the root, used for indentation.
    pub depth: usize,
    pub is_dir: bool,
}

pub struct TreePanel {
    root: PathBuf,
    /// The flattened visible rows. Rebuilt whenever expansion changes, which is
    /// cheap because only expanded directories are ever read.
    entries: Vec<Entry>,
    expanded: HashSet<PathBuf>,
    selected: usize,
    top_line: usize,
    height: usize,
}

impl TreePanel {
    pub fn new(root: &Path) -> Result<Self> {
        let mut panel = Self {
            root: root.to_path_buf(),
            entries: Vec::new(),
            expanded: HashSet::new(),
            selected: 0,
            top_line: 0,
            height: 0,
        };
        panel.rebuild()?;
        Ok(panel)
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn selected(&self) -> Option<&Path> {
        self.entries.get(self.selected).map(|e| e.path.as_path())
    }

    pub fn depth_of_selection(&self) -> usize {
        self.entries.get(self.selected).map_or(0, |e| e.depth)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Rebuild the visible rows, keeping the selection on the same path where
    /// that path is still visible.
    fn rebuild(&mut self) -> Result<()> {
        let previous = self.selected().map(Path::to_path_buf);
        let mut entries = Vec::new();
        collect(&self.root, 0, &self.expanded, &mut entries)?;
        self.entries = entries;
        self.selected = previous
            .and_then(|p| self.entries.iter().position(|e| e.path == p))
            .unwrap_or(self.selected)
            .min(self.entries.len().saturating_sub(1));
        Ok(())
    }

    fn move_selection(&mut self, delta: i32) {
        let last = self.entries.len().saturating_sub(1) as i64;
        self.selected = (self.selected as i64 + delta as i64).clamp(0, last) as usize;
        self.scroll_to_selection();
    }

    fn scroll_to_selection(&mut self) {
        if self.height == 0 {
            return;
        }
        if self.selected < self.top_line {
            self.top_line = self.selected;
        } else if self.selected >= self.top_line + self.height {
            self.top_line = self.selected - self.height + 1;
        }
    }

    fn set_expanded(&mut self, path: PathBuf, expand: bool) -> Vec<PanelEvent> {
        if expand {
            self.expanded.insert(path);
        } else {
            // Collapsing a directory also hides everything under it, so drop
            // the descendants' expansion state rather than leaving it to
            // resurface the next time this directory is opened.
            self.expanded.retain(|p| !p.starts_with(&path));
        }
        match self.rebuild() {
            Ok(()) => vec![PanelEvent::NeedsRedraw],
            Err(e) => vec![PanelEvent::Notify {
                level: typ_core::NotifyLevel::Error,
                message: format!("{e:#}"),
            }],
        }
    }

    /// Open a file, or toggle a directory.
    fn activate(&mut self) -> Vec<PanelEvent> {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return Vec::new();
        };
        if entry.is_dir {
            let expand = !self.expanded.contains(&entry.path);
            self.set_expanded(entry.path, expand)
        } else {
            vec![PanelEvent::OpenFile {
                path: entry.path,
                line: 0,
                col: 0,
            }]
        }
    }

    /// The list area inside the panel's frame.
    fn list_area(area: Rect) -> Rect {
        typ_core::chrome::inner(area)
    }
}

/// Depth-first walk that descends only into expanded directories.
/// Directories sort before files, each alphabetically.
fn collect(
    dir: &Path,
    depth: usize,
    expanded: &HashSet<PathBuf>,
    out: &mut Vec<Entry>,
) -> Result<()> {
    let mut children: Vec<Entry> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| Entry {
            is_dir: e.path().is_dir(),
            path: e.path(),
            depth,
        })
        .collect();
    children.sort_by_key(|e| {
        (
            !e.is_dir,
            e.path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase(),
        )
    });

    for child in children {
        let descend = child.is_dir && expanded.contains(&child.path);
        let path = child.path.clone();
        out.push(child);
        if descend {
            collect(&path, depth + 1, expanded, out)?;
        }
    }
    Ok(())
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
        typ_core::chrome::frame(area, buf, &self.title(), ctx, ctx.theme.bg);
        let inner = Self::list_area(area);

        self.height = inner.height as usize;
        let end = (self.top_line + self.height).min(self.entries.len());
        let lines: Vec<Line> = (self.top_line..end)
            .map(|i| {
                let entry = &self.entries[i];
                let name = entry
                    .path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?");
                // A caret rather than a folder glyph: it says both "this is a
                // directory" and "this is its state" in one cell, and needs no
                // font support.
                let marker = if !entry.is_dir {
                    "  "
                } else if self.expanded.contains(&entry.path) {
                    "v "
                } else {
                    "> "
                };
                let label = format!("{}{marker}{name}", "  ".repeat(entry.depth));
                // Directories carry the accent, files the quieter secondary
                // step. This is what turns a tree from a list into information:
                // the shape of a project is readable without reading the names.
                let style = if i == self.selected {
                    Style::default()
                        .fg(ctx.theme.selection_fg)
                        .bg(ctx.theme.selection_primary_bg)
                } else if entry.is_dir {
                    Style::default().fg(ctx.theme.tree_directory_fg)
                } else {
                    Style::default().fg(ctx.theme.tree_file_fg)
                };
                Line::styled(label, style)
            })
            .collect();
        Paragraph::new(lines)
            .style(Style::default().bg(ctx.theme.bg))
            .render(inner, buf);
    }

    fn handle_key(&mut self, chord: KeyChord) -> Vec<PanelEvent> {
        match chord.raw.code {
            KeyCode::Down => self.move_selection(1),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Enter => return self.activate(),
            KeyCode::Right => {
                if let Some(e) = self.entries.get(self.selected).cloned()
                    && e.is_dir
                    && !self.expanded.contains(&e.path)
                {
                    return self.set_expanded(e.path, true);
                }
            }
            KeyCode::Left => {
                if let Some(e) = self.entries.get(self.selected).cloned()
                    && e.is_dir
                    && self.expanded.contains(&e.path)
                {
                    return self.set_expanded(e.path, false);
                }
            }
            _ => return Vec::new(),
        }
        vec![PanelEvent::NeedsRedraw]
    }

    fn handle_mouse(&mut self, event: MouseEvent, panel_area: Rect) -> Vec<PanelEvent> {
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return Vec::new();
        }
        let inner = Self::list_area(panel_area);
        let row = event.row.saturating_sub(inner.y) as usize;
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
