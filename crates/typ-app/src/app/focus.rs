//! Which region has the keys, and how they move: `f6` over every region,
//! `esc` back to where they came from.
//!
//! A child module of `app` because focus stopped being one of two fixed
//! panels at M4.1: the walk now visits the dock, each pane and the status bar,
//! and what "the region before this one" means is its own question.

use crate::app::{App, Focus};
use crate::dock::Dock;

/// One stop on the `f6` walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    Dock,
    /// An index into `App::panes`.
    Pane(usize),
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
        order.push(Region::Status);

        let here = match self.focus {
            Focus::Tree => Region::Dock,
            Focus::Editor => Region::Pane(self.pane),
            Focus::Status => Region::Status,
        };
        let at = order.iter().position(|r| *r == here).unwrap_or(0);
        let next = (at as isize + delta).rem_euclid(order.len() as isize);
        match order[next as usize] {
            Region::Dock => self.set_focus(Focus::Tree),
            Region::Pane(index) => self.make_current(index),
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
        let mut rects = self.layout.rects(rest);
        rects.sort_by_key(|(_, rect)| (rect.y, rect.x));
        rects
            .into_iter()
            .filter_map(|(id, _)| self.index_of(id))
            .collect()
    }

    /// Move focus, remembering where it came from.
    pub fn set_focus(&mut self, focus: Focus) {
        if focus != self.focus {
            // A zoom belongs to the region it was asked from, and ends
            // when the keys leave it.
            self.dock.zoomed = false;
            self.zoomed_pane = None;
            self.came_from = Some(self.focus);
            self.focus = focus;
        }
    }

    /// `esc` with nothing of the panel's own to cancel: back to where focus
    /// came from. The editor is home, so `esc` there never moves focus: it is
    /// the pane every other one returns to (interface §5, "back to the
    /// editor"). Returns whether focus moved.
    pub(super) fn focus_back(&mut self) -> bool {
        if self.focus == Focus::Editor
            || (self.focus != Focus::Status && self.focused().captures_escape())
        {
            return false;
        }
        let back = self
            .came_from
            .filter(|f| *f != self.focus)
            .unwrap_or(Focus::Editor);
        self.set_focus(back);
        true
    }
}
