//! Painting a frame, and the geometry that decides where each panel goes.
//!
//! Split out of `app.rs` for the same reason `tabs.rs` was: the file had three
//! responsibilities and told you about none of them. This is the one that has
//! nothing to do with editor state — it reads `App` and writes cells.
//!
//! `SEGMENT_GAP` stays in `app.rs` because the status *strings* are built there
//! too, and a constant shared by two modules belongs to neither exclusively. A
//! child module can read its parent's private items, which is what makes that
//! work without widening anything.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use typ_core::{Panel, RenderContext};

use crate::app::{App, Focus, SEGMENT_GAP};

impl App {
    pub fn render(&mut self, frame: &mut ratatui::Frame) {
        let (body, status_area) = crate::layout::split_frame(frame.area());
        let (tree_area, pane) = crate::layout::split(body);
        let (bar_area, editor_area) = crate::layout::split_tabs(pane, self.tabs.len());
        let (w, h) = (frame.area().width, frame.area().height);

        let tree_ctx = RenderContext {
            theme: &self.theme,
            syntax: &self.syntax_theme,
            is_focused: self.focus == Focus::Tree,
            panel_index: 0,
            terminal_width: w,
            terminal_height: h,
        };
        let editor_ctx = RenderContext {
            theme: &self.theme,
            syntax: &self.syntax_theme,
            is_focused: self.focus == Focus::Editor,
            panel_index: 1,
            terminal_width: w,
            terminal_height: h,
        };

        // The focused panel draws last.
        //
        // The two rects share a column — see `layout::split` — so one cell
        // carries both panels' border, and a shared border cannot be two
        // colours. Drawing the focused panel second gives that cell its colour,
        // which is the right answer: the focused panel's box is the complete
        // one, and the unfocused panel is the one that gives ground.
        match self.focus {
            Focus::Editor => {
                self.tree.render(tree_area, frame.buffer_mut(), &tree_ctx);
                self.tabs[self.active]
                    .panel
                    .render(editor_area, frame.buffer_mut(), &editor_ctx);
            }
            Focus::Tree => {
                self.tabs[self.active]
                    .panel
                    .render(editor_area, frame.buffer_mut(), &editor_ctx);
                self.tree.render(tree_area, frame.buffer_mut(), &tree_ctx);
            }
        }

        if bar_area.height > 0 {
            let labels: Vec<String> = self.tabs.iter().map(|tab| tab.panel.title()).collect();
            crate::tabbar::draw(
                frame.buffer_mut(),
                bar_area,
                &labels,
                self.active,
                &self.theme,
            );
        }

        self.render_status(status_area, frame.buffer_mut());

        // The overlay draws last, over the body — after the status bar too, so a
        // tall picker on a short terminal covers the bar rather than being
        // clipped by it. `chrome::frame` fills every cell of its rect, which is
        // what stops the editor showing through.
        if self.picker.is_some() {
            let area = crate::layout::picker_area(frame.area());
            let ctx = RenderContext {
                theme: &self.theme,
                syntax: &self.syntax_theme,
                // Always focused: it owns the keyboard for as long as it is up,
                // so a dimmed border would be lying about where keys go.
                is_focused: true,
                panel_index: 2,
                terminal_width: w,
                terminal_height: h,
            };
            if let Some(picker) = self.picker.as_mut() {
                picker.render(area, frame.buffer_mut(), &ctx);
            }
            // The overlay has its own text cursor at the end of the query, and
            // the panel underneath must not also claim one.
            return;
        }

        // Only the focused panel gets a cursor, and it is the terminal's real
        // one — set after drawing, so it lands on top of the frame. Panels with
        // nothing to edit return None and the cursor stays hidden.
        let focused_area = match self.focus {
            Focus::Tree => tree_area,
            Focus::Editor => editor_area,
        };
        if let Some((x, y)) = self.focused().cursor_position(focused_area) {
            frame.set_cursor_position((x, y));
        }
    }

    fn render_status(&self, area: Rect, buf: &mut ratatui::buffer::Buffer) {
        let background = Style::default()
            .fg(self.theme.status_bar_fg)
            .bg(self.theme.status_bar_bg);
        let left = self.status_left();
        let right_segments = self.status_segments();
        let right_width: usize = right_segments
            .iter()
            .map(|s| s.text.chars().count())
            .sum::<usize>()
            + SEGMENT_GAP.len() * right_segments.len().saturating_sub(1);

        // The right half is the fixed cost; the left is truncated to whatever
        // is left over, so a long message never pushes the position off-screen.
        let width = area.width as usize;
        let room = width.saturating_sub(right_width + 2);
        let left: String = left.chars().take(room).collect();
        let gap = width.saturating_sub(left.chars().count() + right_width);

        // Each segment carries its own emphasis. This is where
        // `status_bar_inactive_fg` and `status_bar_accent` earn their place:
        // without them the bar is one weight of text and a reader has to parse
        // it rather than glance at it.
        let mut spans = vec![
            Span::styled(left, background),
            Span::styled(" ".repeat(gap), background),
        ];
        for (index, segment) in right_segments.iter().enumerate() {
            if index > 0 {
                spans.push(Span::styled(SEGMENT_GAP, background));
            }
            spans.push(Span::styled(
                segment.text.clone(),
                background.fg(segment.emphasis.colour(&self.theme)),
            ));
        }

        Paragraph::new(Line::from(spans))
            .style(background)
            .render(area, buf);
    }

    fn focused(&self) -> &dyn Panel {
        match self.focus {
            Focus::Tree => &self.tree,
            Focus::Editor => &self.tabs[self.active].panel,
        }
    }

    /// Areas for hit-testing mouse events, in the same order as `render`.
    /// Excludes the status bar row, so a click on it hits neither panel.
    ///
    /// The editor's rect is the one *below* the tab bar. It has to come from
    /// `split_tabs` rather than from `split`, because the bar moves every
    /// coordinate inside the editor down a row and a hit-test that missed that
    /// would land every click one line above the pointer.
    pub fn areas(&self, area: Rect) -> (Rect, Rect) {
        let (body, _) = crate::layout::split_frame(area);
        let (tree, pane) = crate::layout::split(body);
        let (_, editor) = crate::layout::split_tabs(pane, self.tabs.len());
        (tree, editor)
    }
}
