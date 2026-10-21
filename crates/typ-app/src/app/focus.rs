//! Which region has the keys, and how they move: `f6` over every region,
//! `esc` back to where they came from.
//!
//! A child module of `app` because focus stopped being one of two fixed
//! panels at M4.1: the walk now visits the dock, each pane and the status bar,
//! and what "the region before this one" means is its own question.

use crate::app::{App, Focus};
use crate::dock::Dock;

/// A region focus can go back to. A pane by its id: an index stops naming
/// the same pane once one before it closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    Dock,
    Pane(crate::pane::PaneId),
    Status,
    BottomDock,
}

/// How far back `esc` remembers.
const STACK_DEPTH: usize = 8;

/// One stop on the `f6` walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    Dock,
    /// An index into `App::panes`.
    Pane(usize),
    BottomDock,
    Status,
}

impl App {
    /// The next region in the walk, wrapping.
    pub fn cycle_focus(&mut self) {
        self.step_focus(1);
    }

    /// `f6` and `shift+f6` (interface §5): the dock, then the panes in
    /// reading order, then the status bar. Fixed, so a screen-reader user can
    /// learn it. A folded dock holds no focus, so the walk steps over it.
    pub(super) fn step_focus(&mut self, delta: isize) {
        let mut order = Vec::new();
        if self.dock.open {
            order.push(Region::Dock);
        }
        order.extend(self.reading_order().into_iter().map(Region::Pane));
        if self.bottom.open {
            order.push(Region::BottomDock);
        }
        order.push(Region::Status);

        let here = match self.focus {
            Focus::Tree => Region::Dock,
            Focus::Editor => Region::Pane(self.pane),
            Focus::Status => Region::Status,
            Focus::BottomDock => Region::BottomDock,
        };
        let at = order.iter().position(|r| *r == here).unwrap_or(0);
        let next = (at as isize + delta).rem_euclid(order.len() as isize);
        match order[next as usize] {
            Region::Dock => self.set_focus(Focus::Tree),
            Region::Pane(index) => self.make_current(index),
            Region::BottomDock => self.set_focus(Focus::BottomDock),
            Region::Status => self.focus_status(),
        }
        self.mark_dirty();
    }

    /// Every pane's index in reading order, as the walk visits them.
    ///
    /// Measured on the layout as it is without a zoom, so the walk is the
    /// same whatever is zoomed; moving off a zoomed region ends the zoom.
    fn reading_order(&self) -> Vec<usize> {
        let body = self.split_body(self.screen).0;
        let rest = Dock {
            zoomed: false,
            ..self.dock
        }
        .areas(body)
        .rest;
        let rest = self.bottom.bottom_areas(rest).rest;
        let mut rects = self.layout.rects(rest);
        rects.sort_by_key(|(_, rect)| (rect.y, rect.x));
        rects
            .into_iter()
            .filter_map(|(id, _)| self.index_of(id))
            .collect()
    }

    /// Where focus is now, as the way back names it.
    fn place(&self) -> Place {
        match self.focus {
            Focus::Tree => Place::Dock,
            Focus::Editor => Place::Pane(self.panes[self.pane].id),
            Focus::Status => Place::Status,
            Focus::BottomDock => Place::BottomDock,
        }
    }

    /// Put where focus is now on the way back, unless it is already on top.
    pub(super) fn remember(&mut self) {
        let here = self.place();
        if self.came_from.last() != Some(&here) {
            self.came_from.push(here);
        }
        if self.came_from.len() > STACK_DEPTH {
            self.came_from.remove(0);
        }
    }

    /// Take `place` off the way back: a pane that closed, a dock that folded.
    pub(super) fn forget(&mut self, place: Place) {
        self.came_from.retain(|p| *p != place);
    }

    /// Move focus, remembering where it came from.
    pub fn set_focus(&mut self, focus: Focus) {
        if focus != self.focus {
            self.remember();
            self.take_focus(focus);
        }
    }

    /// Move focus without remembering: the way back itself.
    fn take_focus(&mut self, focus: Focus) {
        if focus != self.focus {
            // A zoom belongs to the region it was asked from, and ends
            // when the keys leave it.
            self.dock.zoomed = false;
            self.zoomed_pane = None;
            self.focus = focus;
        }
    }

    /// Go to `place` without remembering where focus was. False when it is
    /// not there to go to.
    fn arrive(&mut self, place: Place) -> bool {
        match place {
            Place::Dock if !self.dock.open => return false,
            Place::Dock => self.take_focus(Focus::Tree),
            Place::Pane(id) => {
                let Some(index) = self.index_of(id) else {
                    return false;
                };
                if self.zoomed_pane != Some(id) {
                    self.zoomed_pane = None;
                }
                self.pane = index;
                self.take_focus(Focus::Editor);
                self.warn_if_changed_on_disk();
            }
            Place::BottomDock if !self.bottom.open => return false,
            Place::BottomDock => self.take_focus(Focus::BottomDock),
            Place::Status => {
                self.take_focus(Focus::Status);
                self.segment = crate::status::SegmentId::Position;
            }
        }
        self.mark_dirty();
        true
    }

    /// Back to the most recent place on the way back that still exists and
    /// is not here. False when there is none.
    pub(super) fn go_back(&mut self) -> bool {
        let here = self.place();
        while let Some(place) = self.came_from.pop() {
            if place != here && self.arrive(place) {
                return true;
            }
        }
        false
    }

    /// `esc` with nothing of the region's own to cancel: back the way focus
    /// came (interface §4, a focus stack), or to the current pane when there
    /// is no way back. The editor is home, so `esc` in a pane never moves
    /// focus (§5, "back to the editor"): it is pressed there to clear things,
    /// and a habit that throws focus into the tree is a habit that types into
    /// it. Returns whether focus moved.
    pub(super) fn focus_back(&mut self) -> bool {
        if self.focus == Focus::Editor
            || (self.focus != Focus::Status && self.focused().captures_escape())
        {
            return false;
        }
        if self.go_back() {
            return true;
        }
        self.arrive(Place::Pane(self.panes[self.pane].id))
    }
}
