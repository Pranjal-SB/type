//! Panes: splitting the editor area, closing a split, and moving between them.
//!
//! A child module of `app` for the reason `tabs` is. Tabs are what one pane
//! holds; this is how many panes there are, where, and which has the keys. The
//! geometry of the splits is `crate::pane::Layout`, which knows nothing about
//! tabs.

use crossterm::event::MouseEvent;
use ratatui::layout::Rect;
use typ_core::{Edge, Panel, PanelEvent};

use crate::app::docking::Zoom;
use crate::app::{App, Focus, Pane};
use crate::pane::{Axis, PaneId};

/// Cells one arrow moves a rule in resize mode.
pub(super) const NUDGE: i32 = 1;

/// What a held mouse button is dragging.
#[derive(Debug, Clone)]
pub(crate) enum Drag {
    /// The rule of the split at this path.
    Split(crate::pane::SplitPath),
    /// The rule between the dock and the panes.
    Dock,
}

/// Where one pane is drawn: the regions inside its rect.
///
/// One function computes these (`App::pane_areas`) and both the render and
/// every hit-test ask it, which is what keeps a click on the cell it was
/// aimed at.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PaneArea {
    /// Index into `App::panes`.
    pub index: usize,
    /// The whole pane, tab row included.
    pub whole: Rect,
    /// The pane's number at the left of its tab row. Zero wide with one pane,
    /// where there is nothing to tell apart.
    pub number: Rect,
    /// The tab row, right of the number.
    pub bar: Rect,
    /// The visible tab's text.
    pub content: Rect,
    /// The ` ◰` at the right end of the tab row.
    pub zoom: Rect,
}

impl App {
    /// Every pane's areas inside the editor region, in reading order: by row,
    /// then by column. The order the numbers in the tab rows count in.
    pub(crate) fn pane_areas(&self, editor: Rect) -> Vec<PaneArea> {
        let numbered = self.panes.len() > 1;
        let mut areas: Vec<PaneArea> = self
            .visible_panes(editor)
            .into_iter()
            .filter(|(_, whole)| !whole.is_empty())
            .filter_map(|(id, whole)| {
                let index = self.index_of(id)?;
                let (row, content) = crate::layout::split_tabs(whole);
                // " 9 ", one cell either side of the digit, as a tab pads its
                // name. Clamped so a sliver of a pane is not overrun.
                let width = if numbered { 3.min(row.width) } else { 0 };
                let number = Rect { width, ..row };
                let zoom = crate::layout::zoom_button(row, 0);
                let bar = Rect {
                    x: row.x + width,
                    width: zoom.x.saturating_sub(row.x + width),
                    ..row
                };
                Some(PaneArea {
                    index,
                    whole,
                    number,
                    bar,
                    content,
                    zoom,
                })
            })
            .collect();
        areas.sort_by_key(|area| (area.whole.y, area.whole.x));
        areas
    }

    /// The panes on screen and where: every leaf, or the zoomed pane alone
    /// with the whole region.
    fn visible_panes(&self, editor: Rect) -> Vec<(PaneId, Rect)> {
        match self.zoomed() {
            Some(Zoom::Pane(id)) => vec![(id, editor)],
            _ => self.layout.rects(editor),
        }
    }

    /// The current pane's areas inside the editor region.
    pub(crate) fn current_pane_area(&self, editor: Rect) -> PaneArea {
        self.pane_areas(editor)
            .into_iter()
            .find(|area| area.index == self.pane)
            .unwrap_or(PaneArea {
                index: self.pane,
                whole: Rect::default(),
                number: Rect::default(),
                bar: Rect::default(),
                content: Rect::default(),
                zoom: Rect::default(),
            })
    }

    /// The editor region of `frame`.
    fn editor_region(&self, frame: Rect) -> Rect {
        self.regions(frame).rest
    }

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
        self.make_current(self.index_of(next).unwrap_or(0));
    }

    /// Make the pane at `index` in `panes` the one with the keys.
    pub(super) fn make_current(&mut self, index: usize) {
        self.pane = index;
        if self.zoomed_pane != Some(self.panes[index].id) {
            self.zoomed_pane = None;
        }
        self.set_focus(Focus::Editor);
        self.warn_if_changed_on_disk();
        self.mark_dirty();
    }

    /// Focus the pane `n` places into reading order, counting from zero. A
    /// number past the last pane does nothing, as `alt+9` does for tabs.
    pub(super) fn focus_pane(&mut self, n: usize) {
        if let Some(area) = self.pane_areas(self.editor_region(self.screen)).get(n) {
            self.make_current(area.index);
        }
    }

    /// Focus the pane beside the current one on `edge`.
    ///
    /// Beside means across the one-cell rule from it and overlapping it. Of
    /// those, the one level with the cursor wins, then the one sharing the
    /// most of the current pane's side: `alt+right` from a tall pane goes to
    /// the pane next to the line being edited, not to whichever was split
    /// first.
    pub(super) fn focus_neighbour(&mut self, edge: Edge) {
        let areas = self.pane_areas(self.editor_region(self.screen));
        let Some(current) = areas.iter().find(|area| area.index == self.pane) else {
            return;
        };
        let here = current.whole;
        let (cx, cy) = self.panes[self.pane]
            .shown()
            .panel
            .cursor_position(current.content)
            .unwrap_or((here.x + here.width / 2, here.y + here.height / 2));

        let best = areas
            .iter()
            .filter_map(|area| {
                let there = area.whole;
                let (adjacent, along, from, to) = match edge {
                    Edge::Right => (there.x == here.right() + 1, cy, there.y, there.bottom()),
                    Edge::Left => (there.right() + 1 == here.x, cy, there.y, there.bottom()),
                    Edge::Bottom => (there.y == here.bottom() + 1, cx, there.x, there.right()),
                    Edge::Top => (there.bottom() + 1 == here.y, cx, there.x, there.right()),
                };
                let (mine_from, mine_to) = match edge {
                    Edge::Right | Edge::Left => (here.y, here.bottom()),
                    Edge::Top | Edge::Bottom => (here.x, here.right()),
                };
                let overlap = to.min(mine_to).saturating_sub(from.max(mine_from));
                (adjacent && overlap > 0).then_some((
                    (from..to).contains(&along),
                    overlap,
                    area.index,
                ))
            })
            .max_by_key(|&(level, overlap, _)| (level, overlap));
        if let Some((_, _, index)) = best {
            self.make_current(index);
        }
    }

    /// A press somewhere in the editor region: the pane under it becomes
    /// current, so the tab row and the text that route the press next are that
    /// pane's. Returns true when the press was on the pane's number, which has
    /// nothing else to do with it.
    pub fn press_pane_at(&mut self, column: u16, row: u16, frame: Rect) -> bool {
        let areas = self.pane_areas(self.editor_region(frame));
        let Some(area) = areas
            .iter()
            .find(|area| area.whole.contains((column, row).into()))
        else {
            return false;
        };
        if area.index != self.pane {
            self.pane = area.index;
            self.warn_if_changed_on_disk();
            self.mark_dirty();
        }
        if area.zoom.contains((column, row).into()) {
            self.set_focus(Focus::Editor);
            self.toggle_zoom();
            return true;
        }
        area.number.contains((column, row).into())
    }

    /// Scroll whichever pane is under the pointer, focused or not, the way
    /// every editor's wheel works. `None` when the pointer is over no pane.
    pub fn scroll_pane_at(
        &mut self,
        event: MouseEvent,
        delta: i32,
        frame: Rect,
    ) -> Option<Vec<PanelEvent>> {
        let areas = self.pane_areas(self.editor_region(frame));
        let area = areas
            .iter()
            .find(|area| area.whole.contains((event.column, event.row).into()))?;
        Some(
            self.panes[area.index]
                .shown_mut()
                .panel
                .handle_scroll(delta, area.content),
        )
    }

    /// A key in resize mode. Returns whether the mode took it; a key it does
    /// not take has already ended the mode.
    pub(super) fn handle_resize_chord(&mut self, chord: &typ_core::KeyChord) -> bool {
        use crossterm::event::KeyCode;
        if !chord.raw.modifiers.is_empty() {
            self.resizing = false;
            return false;
        }
        // The dock's rule, when the keys are in the dock.
        if self.focus == Focus::Tree {
            return self.resize_dock_with(chord.raw.code);
        }
        let editor = self.editor_region(self.screen);
        let id = self.panes[self.pane].id;
        let (axis, delta) = match chord.raw.code {
            KeyCode::Left => (Axis::Columns, -NUDGE),
            KeyCode::Right => (Axis::Columns, NUDGE),
            KeyCode::Up => (Axis::Rows, -NUDGE),
            KeyCode::Down => (Axis::Rows, NUDGE),
            KeyCode::Char('=') => {
                for axis in [Axis::Columns, Axis::Rows] {
                    if let Some(path) = self.layout.nearest_split(id, axis) {
                        self.layout.even(editor, &path);
                    }
                }
                self.mark_dirty();
                return true;
            }
            KeyCode::Esc | KeyCode::Enter => {
                self.resizing = false;
                self.mark_dirty();
                return true;
            }
            _ => {
                self.resizing = false;
                return false;
            }
        };
        if let Some(path) = self.layout.nearest_split(id, axis) {
            self.layout.nudge_rule(editor, &path, delta);
            self.mark_dirty();
        }
        true
    }

    /// A press on a rule, between panes or beside the open dock, starts
    /// dragging it. Returns whether there was a rule there.
    pub fn press_rule_at(&mut self, column: u16, row: u16, frame: Rect) -> bool {
        let regions = self.regions(frame);
        self.dragging = if self.dock.open && regions.rule.contains((column, row).into()) {
            Some(Drag::Dock)
        } else {
            // A zoomed pane hides every rule between panes.
            self.zoomed()
                .is_none()
                .then(|| self.layout.rule_at(regions.rest, column, row))
                .flatten()
                .map(Drag::Split)
        };
        self.dragging.is_some()
    }

    /// The pointer moved with the button held. Returns whether a rule moved.
    pub fn drag_rule_to(&mut self, column: u16, row: u16, frame: Rect) -> bool {
        let path = match self.dragging.clone() {
            Some(Drag::Split(path)) => path,
            Some(Drag::Dock) => {
                self.dock.place_rule(self.split_body(frame).0, column);
                self.mark_dirty();
                return true;
            }
            None => return false,
        };
        let cell = match self.layout.axis_at(&path) {
            Some(Axis::Columns) => column,
            Some(Axis::Rows) => row,
            None => return false,
        };
        self.layout
            .place_rule(self.editor_region(frame), &path, cell);
        self.mark_dirty();
        true
    }

    /// The button came up. Returns whether a drag ended.
    pub fn release_rule(&mut self) -> bool {
        self.dragging.take().is_some()
    }

    /// The frame the next key is about. `step` says so on every event, and
    /// `render` on every frame.
    pub fn set_screen(&mut self, frame: Rect) {
        self.screen = frame;
    }

    /// Where `id` is in `panes`.
    pub(super) fn index_of(&self, id: PaneId) -> Option<usize> {
        self.panes.iter().position(|pane| pane.id == id)
    }

    /// How many panes there are. Never zero.
    pub fn pane_count(&self) -> usize {
        self.panes.len()
    }

    /// The current pane's place in reading order, counting from zero: one less
    /// than the number in its tab row.
    pub fn current_pane(&self) -> usize {
        self.pane_areas(self.editor_region(self.screen))
            .iter()
            .position(|area| area.index == self.pane)
            .unwrap_or(0)
    }

    /// Every pane's whole rect, tab row included, in reading order.
    pub fn pane_rects(&self, frame: Rect) -> Vec<Rect> {
        self.pane_areas(self.editor_region(frame))
            .into_iter()
            .map(|area| area.whole)
            .collect()
    }
}
