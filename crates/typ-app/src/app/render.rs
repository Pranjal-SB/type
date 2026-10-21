//! Painting a frame, and the geometry that decides where each panel goes.
//!
//! Split out of `app.rs` for the same reason `tabs.rs` was: the file had three
//! responsibilities and told you about none of them. This is the one that has
//! nothing to do with editor state — it reads `App` and writes cells.
//!
//! `SEGMENT_GAP` stays in `app.rs` because the status *strings* are built there
//! and drawn in `status_bar`, and a constant shared by two modules belongs to
//! neither exclusively. A child module can read its parent's private items,
//! which is what makes that work without widening anything.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};
use typ_core::{Panel, RenderContext};

use crate::app::docking::Zoom;
use crate::app::panes::PaneArea;
use crate::app::{App, Focus};
use crate::pane::Axis;

impl App {
    pub fn render(&mut self, frame: &mut ratatui::Frame) {
        let (_, status_area) = crate::layout::split_frame(frame.area());
        let (_, menu_area) = self.split_body(frame.area());
        let regions = self.regions(frame.area());
        let tree_area = regions.content;
        let (w, h) = (frame.area().width, frame.area().height);

        self.render_dock(&regions, frame.buffer_mut(), (w, h));

        self.screen = frame.area();
        let panes = self.pane_areas(regions.rest);
        for (number, area) in panes.iter().enumerate() {
            self.render_pane(number + 1, area, frame.buffer_mut(), (w, h));
        }
        // A zoom hides every rule between panes with the panes it hid.
        let rules = match self.zoomed() {
            None => self.layout.rules(regions.rest),
            Some(_) => Vec::new(),
        };
        for (axis, rule) in rules {
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

        // Over the body, under the picker. A hover and the overlay are never up
        // together — opening the picker clears the transient state the hover is
        // part of — but the order says which would win if they were.
        self.render_completion(frame.area(), frame.buffer_mut());
        self.render_signature(frame.area(), frame.buffer_mut());
        self.render_hover(frame, editor_area);
        self.render_context(frame.area(), frame.buffer_mut());
        self.render_bar(frame.area(), frame.buffer_mut());

        // After the floats: a float near the bottom paints its gutter ring one
        // row past its box, which is the status bar's row, and wiped the keys
        // the bar was showing for that very float.
        self.render_status(status_area, frame.buffer_mut());
        if let (Some(menu), Some(area)) = (&self.menu, menu_area) {
            menu.render(area, frame.buffer_mut(), &self.theme);
        }
        // Last of all but the picker: warp labels sit on status segments too.
        self.render_warp(frame.area(), frame.buffer_mut());

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
        // The same for the warp labels, the context menu and the menu bar.
        if self.menu.is_some()
            || self.warp.is_some()
            || self.context.is_some()
            || self.bar.is_some()
        {
            return;
        }

        // Only the focused panel gets a cursor, and it is the terminal's real
        // one — set after drawing, so it lands on top of the frame. Panels with
        // nothing to edit return None and the cursor stays hidden.
        let focused_area = match self.focus {
            Focus::Tree => tree_area,
            Focus::Editor => editor_area,
            Focus::Status => {
                if let Some(at) = self.status_cursor(status_area) {
                    frame.set_cursor_position(at);
                }
                return;
            }
        };
        if let Some((x, y)) = self.focused().cursor_position(focused_area) {
            frame.set_cursor_position((x, y));
        }
    }

    pub(super) fn focused(&self) -> &dyn Panel {
        match self.focus {
            Focus::Tree => &self.tree,
            // The bar has no panel; see `focused_mut`.
            Focus::Editor | Focus::Status => self.panes[self.pane].shown().panel(),
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
        let body = self.split_body(frame).0;
        match self.zoomed() {
            Some(Zoom::Pane(_)) => crate::dock::DockAreas::hidden(body),
            zoom => crate::dock::Dock {
                zoomed: zoom == Some(Zoom::Dock),
                ..self.dock
            }
            .areas(body),
        }
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
            zoom,
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
        let zoomed = self.zoomed() == Some(Zoom::Pane(self.panes[index].id));
        let pane = &mut self.panes[index];
        pane.shown_mut().panel_mut().render(content, buf, &ctx);

        if bar.height > 0 {
            let labels: Vec<String> = pane.tabs.iter().map(|tab| tab.panel().title()).collect();
            crate::tabbar::draw(buf, bar, &labels, pane.active, &self.theme, focused);
        }
        crate::app::docking::draw_zoom(buf, zoom, zoomed, &self.theme);
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
            .panel()
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
