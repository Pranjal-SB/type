//! The left dock's posture: open, focused, folded.
//!
//! A child module of `app` beside `panes`: panes are how the editor area is
//! divided, this is the region beside it. Where either is drawn is
//! `crate::dock` and `crate::pane`, which know nothing about focus.

use ratatui::layout::Rect;
use typ_core::Panel;

use crate::app::{App, Focus};

/// What a folded dock shows at the top of its rule.
pub(crate) const FOLDED: &str = "\u{25b8}";

impl App {
    /// Whether the left dock is open rather than folded to its rule.
    pub fn dock_open(&self) -> bool {
        self.dock.open
    }

    /// `ctrl+b` (interface §5): closed opens and focuses the dock, open
    /// without focus focuses it, focused folds it.
    pub(super) fn toggle_left_dock(&mut self) {
        if !self.dock.open {
            self.open_dock();
        } else if self.focus != Focus::Tree {
            self.set_focus(Focus::Tree);
        } else {
            self.fold_dock();
        }
        self.mark_dirty();
    }

    fn open_dock(&mut self) {
        self.dock.open = true;
        self.set_focus(Focus::Tree);
        self.mark_dirty();
    }

    /// Fold the dock to its rule. Focus goes back where it came from, and to
    /// the editor when it came from nowhere: a folded dock cannot hold it.
    fn fold_dock(&mut self) {
        self.dock.open = false;
        if self.focus == Focus::Tree {
            let back = self
                .came_from
                .filter(|f| *f != Focus::Tree)
                .unwrap_or(Focus::Editor);
            self.set_focus(back);
        }
    }

    /// A press on the folded dock's column opens it. Returns whether it was
    /// there.
    pub fn press_dock_at(&mut self, column: u16, row: u16, frame: Rect) -> bool {
        if self.dock.open || !self.regions(frame).rule.contains((column, row).into()) {
            return false;
        }
        self.open_dock();
        true
    }
}

impl App {
    /// The dock and its rule. The app draws the lines; the tree draws its
    /// list inside them. Folded, the rule alone with `▸` at its top.
    pub(super) fn render_dock(
        &mut self,
        regions: &crate::dock::DockAreas,
        buf: &mut ratatui::buffer::Buffer,
        (w, h): (u16, u16),
    ) {
        typ_core::chrome::rule(regions.rule, buf, &self.theme, self.theme.bg);
        if !self.dock.open {
            let top = regions.rule.intersection(buf.area);
            if !top.is_empty() {
                buf[(top.x, top.y)].set_symbol(FOLDED);
            }
            return;
        }
        let focused = self.focus == Focus::Tree;
        typ_core::chrome::labelled_rule(
            regions.heading,
            buf,
            &self.tree.title(),
            focused,
            &self.theme,
            self.theme.chrome_bg,
        );
        draw_zoom(
            buf,
            regions.zoom,
            self.zoomed() == Some(Zoom::Dock),
            &self.theme,
        );
        let ctx = typ_core::RenderContext {
            theme: &self.theme,
            syntax: &self.syntax_theme,
            // The tree draws files, not their contents.
            diagnostics: &[],
            is_focused: focused,
            panel_index: 0,
            terminal_width: w,
            terminal_height: h,
        };
        self.tree.render(regions.content, buf, &ctx);
    }
}

impl App {
    /// A key in resize mode with the dock focused: left and right move its
    /// rule, `=` puts it back at its size, `enter` and `esc` leave. Returns
    /// whether the mode took the key, as `handle_resize_chord` does.
    pub(super) fn resize_dock_with(&mut self, code: crossterm::event::KeyCode) -> bool {
        use crossterm::event::KeyCode;
        let body = self.split_body(self.screen).0;
        match code {
            KeyCode::Left => self.dock.nudge_rule(body, -super::panes::NUDGE),
            KeyCode::Right => self.dock.nudge_rule(body, super::panes::NUDGE),
            KeyCode::Char('=') => self.dock.size = crate::dock::DOCK_SIZE,
            // A left dock has no rule above or below it to move.
            KeyCode::Up | KeyCode::Down => {}
            KeyCode::Esc | KeyCode::Enter => self.resizing = false,
            _ => {
                self.resizing = false;
                return false;
            }
        }
        self.mark_dirty();
        true
    }
}

/// What is zoomed to the whole body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Zoom {
    Dock,
    Pane(crate::pane::PaneId),
}

/// Two presses on the dock's heading this close together are a double-click.
/// What Windows and most desktops default to.
const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(500);

impl App {
    /// What is zoomed, if anything. A zoom holds while its region has the
    /// keys, so the layout it hid is never one key away from being stranded.
    pub(crate) fn zoomed(&self) -> Option<Zoom> {
        match self.focus {
            Focus::Tree if self.dock.zoomed && self.dock.open => Some(Zoom::Dock),
            Focus::Editor => {
                let id = self.panes[self.pane].id;
                (self.zoomed_pane == Some(id)).then_some(Zoom::Pane(id))
            }
            _ => None,
        }
    }

    /// `ctrl+k z`: zoom the focused region, or put the layout back. Nothing
    /// in the layout changes, so the way back is exact.
    pub(super) fn toggle_zoom(&mut self) {
        let was = self.zoomed();
        self.dock.zoomed = false;
        self.zoomed_pane = None;
        if was.is_none() {
            match self.focus {
                Focus::Tree => self.dock.zoomed = self.dock.open,
                Focus::Editor => self.zoomed_pane = Some(self.panes[self.pane].id),
            }
        }
        self.mark_dirty();
    }

    /// A press on the open dock's heading: it focuses the dock, a press on
    /// its ` ◰` zooms it, and so does a second press inside `DOUBLE_CLICK`.
    /// Returns whether the press was there.
    pub fn press_dock_heading(
        &mut self,
        column: u16,
        row: u16,
        frame: Rect,
        now: std::time::Instant,
    ) -> bool {
        let regions = self.regions(frame);
        let at = (column, row).into();
        if !self.dock.open || !regions.heading.contains(at) {
            return false;
        }
        let double = self
            .heading_pressed
            .is_some_and(|then| now.saturating_duration_since(then) <= DOUBLE_CLICK);
        self.set_focus(Focus::Tree);
        if regions.zoom.contains(at) || double {
            self.toggle_zoom();
            self.heading_pressed = None;
        } else {
            self.heading_pressed = Some(now);
        }
        self.mark_dirty();
        true
    }
}

/// The ` ◰` in `area`, which is zero wide when its row is too narrow. Accent
/// and bold on the zoomed region, where it is the way back; the tab row's
/// quiet colour everywhere else.
pub(super) fn draw_zoom(
    buf: &mut ratatui::buffer::Buffer,
    area: Rect,
    zoomed: bool,
    theme: &typ_core::ThemeColors,
) {
    use ratatui::style::{Modifier, Style};
    let area = area.intersection(buf.area);
    if area.width < 2 {
        return;
    }
    let quiet = Style::default()
        .fg(theme.status_bar_inactive_fg)
        .bg(theme.chrome_bg);
    let style = if zoomed {
        quiet.fg(theme.border_focused).add_modifier(Modifier::BOLD)
    } else {
        quiet
    };
    buf[(area.x, area.y)].set_symbol(" ").set_style(quiet);
    buf[(area.x + 1, area.y)]
        .set_symbol(crate::layout::ZOOM)
        .set_style(style);
}
