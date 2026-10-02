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

use crate::app::panes::PaneArea;
use crate::app::{App, Focus, SEGMENT_GAP};
use crate::pane::Axis;

impl App {
    pub fn render(&mut self, frame: &mut ratatui::Frame) {
        let (_, status_area) = crate::layout::split_frame(frame.area());
        let (_, menu_area) = self.split_body(frame.area());
        let regions = self.regions(frame.area());
        let tree_area = regions.content;
        let (w, h) = (frame.area().width, frame.area().height);

        let tree_ctx = RenderContext {
            theme: &self.theme,
            syntax: &self.syntax_theme,
            // The tree draws files, not their contents.
            diagnostics: &[],
            is_focused: self.focus == Focus::Tree,
            panel_index: 0,
            terminal_width: w,
            terminal_height: h,
        };

        // The app draws the lines; each panel draws its content inside them.
        typ_core::chrome::labelled_rule(
            regions.heading,
            frame.buffer_mut(),
            &self.tree.title(),
            self.focus == Focus::Tree,
            &self.theme,
            self.theme.chrome_bg,
        );
        self.tree.render(tree_area, frame.buffer_mut(), &tree_ctx);
        typ_core::chrome::rule(regions.rule, frame.buffer_mut(), &self.theme, self.theme.bg);

        self.screen = frame.area();
        let panes = self.pane_areas(regions.rest);
        for (number, area) in panes.iter().enumerate() {
            self.render_pane(number + 1, area, frame.buffer_mut(), (w, h));
        }
        for (axis, rule) in self.layout.rules(regions.rest) {
            let buf = frame.buffer_mut();
            match axis {
                Axis::Columns => typ_core::chrome::rule(rule, buf, &self.theme, self.theme.bg),
                Axis::Rows => {
                    typ_core::chrome::horizontal_rule(rule, buf, &self.theme, self.theme.bg)
                }
            }
        }
        let editor_area = panes
            .iter()
            .find(|area| area.index == self.pane)
            .map_or(Rect::default(), |area| area.content);

        self.render_status(status_area, frame.buffer_mut());
        if let (Some(menu), Some(area)) = (&self.menu, menu_area) {
            menu.render(area, frame.buffer_mut(), &self.theme);
        }

        // Over the body, under the picker. A hover and the overlay are never up
        // together — opening the picker clears the transient state the hover is
        // part of — but the order says which would win if they were.
        self.render_hover(frame, editor_area);

        // The overlay draws last, over the body — after the status bar too, so a
        // tall picker on a short terminal covers the bar rather than being
        // clipped by it. `chrome::float` clears every cell of its rect, which is
        // what stops the editor showing through.
        if self.picker.is_some() {
            // Modal, so everything behind it steps back: the frame minus the
            // status bar, which stays as it is.
            let (behind, _) = crate::layout::split_frame(frame.area());
            typ_core::chrome::scrim(frame.buffer_mut(), behind, self.theme.scrim);
            let area = crate::layout::picker_area(frame.area());
            let ctx = RenderContext {
                theme: &self.theme,
                syntax: &self.syntax_theme,
                diagnostics: &[],
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

        // The keyboard is the menu's while it is up, so no panel shows a caret.
        if self.menu.is_some() {
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
        // Measured in cells, not chars. A CJK file name is one char and two
        // cells per character, and counting chars pushed the right half's
        // tail off the end of the bar. Gap 105.
        let right_width: usize = right_segments
            .iter()
            .map(|s| typ_buffer::display_width(&s.text))
            .sum::<usize>()
            + SEGMENT_GAP.len() * right_segments.len().saturating_sub(1);

        // The right half is the fixed cost; the left is truncated to whatever
        // is left over, so a long message never pushes the position off-screen.
        let width = area.width as usize;
        let room = width.saturating_sub(right_width + 2);
        let left = clip_to_cells(&left, room);
        let gap = width.saturating_sub(typ_buffer::display_width(&left) + right_width);

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

    pub(super) fn focused(&self) -> &dyn Panel {
        match self.focus {
            Focus::Tree => &self.tree,
            Focus::Editor => &self.panes[self.pane].shown().panel,
        }
    }

    /// Areas for hit-testing mouse events, in the same order as `render`.
    /// Excludes the status bar row. That alone does not stop a click there
    /// reaching a panel (their hit tests clamp) so `run::step` drops a press
    /// below these rects before routing it.
    ///
    /// The editor's rect is the one *below* the tab bar. It has to come from
    /// `split_tabs` rather than from `split`, because the bar moves every
    /// coordinate inside the editor down a row and a hit-test that missed that
    /// would land every click one line above the pointer.
    ///
    /// With panes, "the editor" is the current pane's text.
    pub fn areas(&self, area: Rect) -> (Rect, Rect) {
        let regions = self.regions(area);
        (
            regions.content,
            self.current_pane_area(regions.rest).content,
        )
    }

    /// The dock's regions and the panes' area in `frame`: the one place the
    /// body is divided, for render and every hit-test.
    pub(crate) fn regions(&self, frame: Rect) -> crate::dock::DockAreas {
        self.dock.areas(self.split_body(frame).0)
    }

    /// One pane: the visible tab's text, then its tab row over the top, led by
    /// the pane's number when there is more than one.
    fn render_pane(
        &mut self,
        number: usize,
        area: &PaneArea,
        buf: &mut ratatui::buffer::Buffer,
        (w, h): (u16, u16),
    ) {
        let PaneArea {
            index,
            number: number_area,
            bar,
            content,
            ..
        } = *area;
        let focused = self.focus == Focus::Editor && index == self.pane;
        // Resolved before the pane is borrowed mutably: the result borrows
        // `self.lsp` alone, so the tab's borrow ends with the call.
        let diagnostics =
            crate::app::servers::diagnostics_for(&self.lsp, self.panes[index].shown());
        let ctx = RenderContext {
            theme: &self.theme,
            syntax: &self.syntax_theme,
            diagnostics,
            is_focused: focused,
            panel_index: 1,
            terminal_width: w,
            terminal_height: h,
        };
        let pane = &mut self.panes[index];
        pane.shown_mut().panel.render(content, buf, &ctx);

        if bar.height > 0 {
            let labels: Vec<String> = pane.tabs.iter().map(|tab| tab.panel.title()).collect();
            crate::tabbar::draw(buf, bar, &labels, pane.active, &self.theme, focused);
        }
        if number_area.width > 0 {
            // The pane's label (interface §4): accent and bold on the pane
            // with the keys, the tab row's own quiet colour on the rest.
            let style = if focused {
                Style::default()
                    .fg(self.theme.border_focused)
                    .bg(self.theme.chrome_bg)
                    .add_modifier(ratatui::style::Modifier::BOLD)
            } else {
                Style::default()
                    .fg(self.theme.status_bar_inactive_fg)
                    .bg(self.theme.chrome_bg)
            };
            buf.set_stringn(
                number_area.x,
                number_area.y,
                format!(" {number} "),
                number_area.width as usize,
                style,
            );
        }
    }
}

/// The longest prefix of `text` that fits in `room` cells, whole graphemes only.
fn clip_to_cells(text: &str, room: usize) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    let mut used = 0;
    text.graphemes(true)
        .take_while(|g| {
            used += typ_buffer::display_width(g);
            used <= room
        })
        .collect()
}

impl App {
    /// Draw the hover box, if a server has said something about the cursor.
    ///
    /// Anchored to the cursor rather than to the frame: it is about the thing
    /// under it, and a centred box would make the reader look away from the
    /// word they asked about.
    fn render_hover(&mut self, frame: &mut ratatui::Frame, editor_area: Rect) {
        let Some(text) = self.hover.clone() else {
            return;
        };
        let Some(cursor) = self.panes[self.pane]
            .shown()
            .panel
            .cursor_position(editor_area)
        else {
            return;
        };

        let lines: Vec<&str> = text.lines().collect();
        let area = crate::layout::hover_area(frame.area(), cursor, &text);

        let ctx = RenderContext {
            theme: &self.theme,
            syntax: &self.syntax_theme,
            diagnostics: &[],
            // Never focused: it takes no keys, and a bright border would say it
            // does. So the float is bordered in the rule, not `float_border`.
            is_focused: false,
            panel_index: 3,
            terminal_width: frame.area().width,
            terminal_height: frame.area().height,
        };
        // Non-modal, so nothing behind it dims. `esc` is honest: any key or
        // click clears it, see `clear_transient`.
        typ_core::chrome::float(
            area,
            frame.buffer_mut(),
            crate::layout::HOVER_NAME,
            crate::layout::HOVER_EXIT,
            &ctx,
        );

        let inner = typ_core::chrome::inner(area);
        let body: Vec<Line> = lines
            .iter()
            .map(|line| Line::from(Span::raw(line.to_string())))
            .collect();
        Paragraph::new(body)
            .style(Style::default().fg(self.theme.fg).bg(self.theme.chrome_bg))
            .render(inner, frame.buffer_mut());
    }
}
