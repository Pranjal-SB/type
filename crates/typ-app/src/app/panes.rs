//! Panes: splitting the editor area, closing a split, and which pane is which.
//!
//! A child module of `app` for the reason `tabs` is. Tabs are what one pane
//! holds; this is how many panes there are and where. The geometry itself is
//! `crate::pane::Layout`, which knows nothing about tabs.

use ratatui::layout::Rect;
use typ_core::Panel;

use crate::app::{App, Focus, Pane};
use crate::pane::{Axis, PaneId};

impl App {
    /// Split the current pane, the new one on the second side of `axis`.
    ///
    /// The new pane opens empty with the file picker up (decision 1 of the M4
    /// plan): one file is in one pane, so there is nothing to copy into it.
    pub(super) fn split(&mut self, axis: Axis) {
        let id = PaneId(self.next_pane);
        self.next_pane += 1;
        let current = self.panes[self.pane].id;
        if !self.layout.split_leaf(current, axis, id) {
            crate::log_error!("pane {current:?} is not in the layout; split refused");
            return;
        }
        self.panes.push(Pane::new(id));
        self.pane = self.panes.len() - 1;
        self.settle_active_tab();
        self.mark_dirty();
        self.open_picker();
    }

    /// Close the current pane, asking first if a tab in it has unsaved work.
    ///
    /// The same confirmation closing those tabs one at a time would give: the
    /// next press of the same thing is the answer. The last pane never closes;
    /// it holds the "never empty" promise `tabs` makes for a pane.
    pub(super) fn request_close_pane(&mut self) {
        if self.panes.len() == 1 {
            self.status = Some("The last pane stays open. Ctrl+W closes its tabs.".to_string());
            return;
        }
        let id = self.panes[self.pane].id;
        if self.close_pane_pending != Some(id)
            && let Some(message) = self.panes[self.pane]
                .tabs
                .iter()
                .find_map(|tab| tab.panel.needs_close_confirmation())
        {
            self.status = Some(format!(
                "{message}  Close the pane again to discard, Ctrl+S to save."
            ));
            self.close_pane_pending = Some(id);
            return;
        }
        self.close_pane_pending = None;
        let Some(next) = self.layout.remove(id) else {
            return;
        };
        self.panes.remove(self.pane);
        self.pane = self.index_of(next).unwrap_or(0);
        self.set_focus(Focus::Editor);
        self.warn_if_changed_on_disk();
        self.mark_dirty();
    }

    /// Where `id` is in `panes`.
    pub(super) fn index_of(&self, id: PaneId) -> Option<usize> {
        self.panes.iter().position(|pane| pane.id == id)
    }

    /// How many panes there are. Never zero.
    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    /// The current pane's place in screen order, counting from zero.
    pub fn current_pane(&self) -> usize {
        let id = self.panes[self.pane].id;
        self.layout
            .ids()
            .iter()
            .position(|&each| each == id)
            .unwrap_or(0)
    }

    /// Every pane's whole rect, tab row included, in screen order.
    pub fn pane_rects(&self, frame: Rect) -> Vec<Rect> {
        let (body, _) = self.split_body(frame);
        self.layout
            .rects(crate::layout::split(body).editor)
            .into_iter()
            .map(|(_, rect)| rect)
            .collect()
    }
}
