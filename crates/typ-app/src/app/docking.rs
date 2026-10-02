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
