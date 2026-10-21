//! The bottom dock (interface §2, §5) and its first panel, problems.
//!
//! A child module of `app` beside `docking`, which is the left dock: the same
//! three steps on `ctrl+j`, its labelled rule dragged or nudged like the left
//! dock's, one geometry (`body_regions`) for paint, clicks and `targets`.
//!
//! The list is built here, from the servers' diagnostics and the buffers'
//! lines, and handed to the panel; `f8` walks the same list.

use std::path::{Path, PathBuf};

use anyhow::Result;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use typ_buffer::Position;
use typ_core::{PanelEvent, Severity};

use super::panes::Drag;
use super::targets::{Target, TargetKind};
use super::{App, Focus};
use crate::app::docking::Zoom;
use crate::dock::{BottomAreas, DockAreas};
use crate::problems::{Problem, ProblemsPanel};

/// Lines either side of a problem's own in its excerpt.
const CONTEXT: usize = 1;

impl App {
    /// Whether the bottom dock is open.
    pub fn bottom_dock_open(&self) -> bool {
        self.bottom.open
    }

    /// Every problem in the order the dock lists them and `f8` walks them.
    pub fn problems(&self) -> Vec<(PathBuf, Position)> {
        self.collect_problems()
            .into_iter()
            .map(|p| (p.path, p.at))
            .collect()
    }

    /// Every dock region in `frame`: the left dock's heading, list and rule,
    /// and the bottom dock's heading and panel. With the panes, they tile
    /// the body.
    pub fn dock_rects(&self, frame: Rect) -> Vec<Rect> {
        let (left, bottom) = self.body_regions(frame);
        vec![
            left.heading,
            left.content,
            left.rule,
            bottom.heading,
            bottom.content,
        ]
    }

    /// The left dock's regions and the bottom dock's, the panes in what is
    /// left: the one place the body is divided.
    pub(crate) fn body_regions(&self, frame: Rect) -> (DockAreas, BottomAreas) {
        let body = self.split_body(frame).0;
        if let Some(Zoom::Pane(_)) = self.zoomed() {
            return (DockAreas::hidden(body), BottomAreas::hidden(body));
        }
        let mut left = crate::dock::Dock {
            zoomed: self.zoomed() == Some(Zoom::Dock),
            ..self.dock
        }
        .areas(body);
        let bottom = self.bottom.bottom_areas(left.rest);
        left.rest = bottom.rest;
        (left, bottom)
    }

    /// The panes' columns before the bottom dock takes its rows: what its
    /// rule is measured against.
    fn bottom_base(&self, frame: Rect) -> Rect {
        let (left, bottom) = self.body_regions(frame);
        left.rest.union(bottom.heading).union(bottom.content)
    }

    /// `ctrl+j` (interface §5): closed opens and focuses the dock, open
    /// without focus focuses it, focused folds it.
    pub(super) fn toggle_bottom_dock(&mut self) {
        if !self.bottom.open {
            self.bottom.open = true;
            self.refresh_problems();
            self.set_focus(Focus::BottomDock);
        } else if self.focus != Focus::BottomDock {
            self.set_focus(Focus::BottomDock);
        } else {
            self.fold_bottom();
        }
        self.mark_dirty();
    }

    /// Fold the dock. Focus goes back where it came from, or to the editor.
    fn fold_bottom(&mut self) {
        self.bottom.open = false;
        self.forget(super::focus::Place::BottomDock);
        if self.focus == Focus::BottomDock && !self.go_back() {
            self.set_focus(Focus::Editor);
            self.forget(super::focus::Place::BottomDock);
        }
    }

    /// Every diagnostic of every open document, grouped by file, the files
    /// with an error first and then by path, each file's in line order.
    pub(super) fn collect_problems(&self) -> Vec<Problem> {
        let mut seen: Vec<&Path> = Vec::new();
        let mut files: Vec<Vec<Problem>> = Vec::new();
        for editor in self.all_tabs().filter_map(super::Tab::editor) {
            let Some(path) = editor.path() else {
                continue;
            };
            if seen.contains(&path) {
                continue;
            }
            seen.push(path);
            let mut found: Vec<Problem> = self
                .lsp
                .diagnostics(path)
                .iter()
                .map(|d| {
                    let line = d.range.0.line;
                    let last = editor.line_count().saturating_sub(1);
                    let from = line.saturating_sub(CONTEXT);
                    let to = (line + CONTEXT).min(last);
                    Problem {
                        path: path.to_path_buf(),
                        name: editor.file_name(),
                        severity: d.severity,
                        message: d.message.lines().next().unwrap_or_default().to_string(),
                        at: d.range.0,
                        excerpt: (from..=to).map(|n| (n, editor.line_text(n))).collect(),
                    }
                })
                .collect();
            found.sort_by_key(|p| (p.at, p.severity));
            if !found.is_empty() {
                files.push(found);
            }
        }
        let has_error = |file: &[Problem]| file.iter().any(|p| p.severity == Severity::Error);
        files.sort_by(|a, b| (!has_error(a), &a[0].path).cmp(&(!has_error(b), &b[0].path)));
        files.into_iter().flatten().collect()
    }

    /// Hand the panel the list as it is now.
    pub(super) fn refresh_problems(&mut self) {
        let problems = self.collect_problems();
        if let Some(panel) = self
            .bottom_panel
            .as_any_mut()
            .downcast_mut::<ProblemsPanel>()
        {
            panel.set(problems);
        }
    }

    /// `f8`: the next problem after the caret in the dock's order, into the
    /// next file after this file's last, wrapping to the first of all.
    pub(super) fn next_problem(&mut self) {
        let list = self.problems();
        if list.is_empty() {
            self.status = Some("No problems.".to_string());
            return;
        }
        let here = self
            .shown_editor()
            .and_then(|e| Some((e.path()?.to_path_buf(), e.cursor())));
        let next = match here {
            Some((path, cursor)) => list
                .iter()
                .position(|(p, at)| *p == path && *at > cursor)
                .or_else(|| {
                    let last = list.iter().rposition(|(p, _)| *p == path)?;
                    Some((last + 1) % list.len())
                })
                .unwrap_or(0),
            None => 0,
        };
        let (path, at) = list[next].clone();
        self.jump_to(&path, at);
    }

    /// Open `path` and put the caret on `at`, the editor focused: where a
    /// problem is, whether or not its file was open.
    pub(super) fn jump_to(&mut self, path: &Path, at: Position) {
        if !self.open_or_report(path) {
            return;
        }
        if let Some(editor) = self.shown_editor_mut() {
            editor.goto(at.line, at.col);
        }
        self.set_focus(Focus::Editor);
        self.mark_dirty();
    }

    /// A mouse event on the bottom dock. A press on its rule focuses it and
    /// picks the rule up to drag; a press on the list focuses it and goes to
    /// the panel, which jumps. Returns whether the dock took it.
    pub fn route_bottom_mouse(&mut self, event: MouseEvent, frame: Rect) -> Result<bool> {
        let MouseEventKind::Down(button) = event.kind else {
            return Ok(false);
        };
        let (_, bottom) = self.body_regions(frame);
        let at = (event.column, event.row).into();
        if bottom.heading.contains(at) {
            self.clear_transient();
            self.set_focus(Focus::BottomDock);
            if button == MouseButton::Left {
                self.dragging = Some(Drag::Bottom);
            }
            self.mark_dirty();
            return Ok(true);
        }
        if !bottom.content.contains(at) {
            return Ok(false);
        }
        self.clear_transient();
        self.set_focus(Focus::BottomDock);
        self.mark_dirty();
        let events = self.bottom_panel.handle_mouse(event, bottom.content);
        self.apply(events)?;
        Ok(true)
    }

    /// The pointer moved with the dock's rule held.
    pub(super) fn drag_bottom_to(&mut self, row: u16, frame: Rect) {
        let base = self.bottom_base(frame);
        self.bottom.place_bottom_rule(base, row);
        self.mark_dirty();
    }

    /// A wheel over the dock's list scrolls it. `None` when not over it.
    pub fn scroll_bottom_at(
        &mut self,
        event: MouseEvent,
        delta: i32,
        frame: Rect,
    ) -> Option<Vec<PanelEvent>> {
        let (_, bottom) = self.body_regions(frame);
        bottom
            .content
            .contains((event.column, event.row).into())
            .then(|| self.bottom_panel.handle_scroll(delta, bottom.content))
    }

    /// A key in resize mode with the dock focused: up and down move its rule.
    pub(super) fn resize_bottom_with(&mut self, code: crossterm::event::KeyCode) -> bool {
        use crossterm::event::KeyCode;
        let base = self.bottom_base(self.screen);
        match code {
            KeyCode::Up => self.bottom.nudge_bottom_rule(base, -super::panes::NUDGE),
            KeyCode::Down => self.bottom.nudge_bottom_rule(base, super::panes::NUDGE),
            KeyCode::Char('=') => self.bottom.size = crate::dock::BOTTOM_SIZE,
            KeyCode::Left | KeyCode::Right => {}
            KeyCode::Esc | KeyCode::Enter => self.resizing = false,
            _ => {
                self.resizing = false;
                return false;
            }
        }
        self.mark_dirty();
        true
    }

    /// The dock's problems, then its rule, as targets: problems first, so
    /// `d1` is always the first problem (interface §5).
    pub(super) fn bottom_targets(&self, frame: Rect) -> Vec<Target> {
        let (_, bottom) = self.body_regions(frame);
        if bottom.heading.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<Target> = self
            .problems_panel()
            .map(|panel| panel.blocks(bottom.content))
            .unwrap_or_default()
            .into_iter()
            .map(|(i, rect)| Target {
                rect,
                point: (rect.x, rect.y),
                kind: TargetKind::ProblemRow(i),
            })
            .collect();
        let heading = bottom.heading;
        out.push(Target {
            rect: heading,
            point: (heading.x + 1, heading.y),
            kind: TargetKind::BottomHeading,
        });
        out
    }

    fn problems_panel(&self) -> Option<&ProblemsPanel> {
        self.bottom_panel.as_any().downcast_ref::<ProblemsPanel>()
    }

    /// The dock's rule, labelled with its panel, and the panel under it.
    pub(super) fn render_bottom(
        &mut self,
        bottom: &BottomAreas,
        buf: &mut ratatui::buffer::Buffer,
        (w, h): (u16, u16),
    ) {
        if bottom.heading.is_empty() {
            return;
        }
        self.refresh_problems();
        let focused = self.focus == Focus::BottomDock;
        typ_core::chrome::labelled_rule(
            bottom.heading,
            buf,
            &self.bottom_panel.title(),
            focused,
            &self.theme,
            self.theme.chrome_bg,
        );
        let ctx = typ_core::RenderContext {
            theme: &self.theme,
            syntax: &self.syntax_theme,
            diagnostics: &[],
            is_focused: focused,
            panel_index: 4,
            terminal_width: w,
            terminal_height: h,
        };
        self.bottom_panel.render(bottom.content, buf, &ctx);
    }
}
