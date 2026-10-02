//! The layout tree: how the editor area is divided between panes.
//!
//! Geometry only. What a pane holds (its tabs) lives with the rest of the app's
//! state; this says where each pane is, and render and hit-testing both ask it,
//! so a click and the paint it lands on agree by construction.

use ratatui::layout::Rect;

/// A pane's name for as long as it exists. Never reused within a session, so a
/// stale id names nothing rather than naming whichever pane took its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PaneId(pub u32);

/// Which way a split divides its area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// Side by side, a vertical rule between: `split right`.
    Columns,
    /// One above the other, a horizontal rule between: `split down`.
    Rows,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Layout {
    Leaf(PaneId),
    Split {
        axis: Axis,
        /// The first child's share of the space left after the rule, 0 to 1.
        ratio: f32,
        a: Box<Layout>,
        b: Box<Layout>,
    },
}

impl Layout {
    /// An even split of two layouts.
    pub fn split(axis: Axis, a: Layout, b: Layout) -> Self {
        Layout::Split {
            axis,
            ratio: 0.5,
            a: Box::new(a),
            b: Box::new(b),
        }
    }

    /// Every pane's rect inside `area`, in screen order: left before right, top
    /// before bottom, depth first.
    pub fn rects(&self, area: Rect) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        self.collect(area, &mut out);
        out
    }

    /// Every rule between panes, with the axis of the split it divides.
    pub fn rules(&self, area: Rect) -> Vec<(Axis, Rect)> {
        let mut out = Vec::new();
        self.collect_rules(area, &mut out);
        out
    }

    fn collect_rules(&self, area: Rect, out: &mut Vec<(Axis, Rect)>) {
        if let Layout::Split { axis, ratio, a, b } = self {
            let (first, rule, second) = divide(area, *axis, *ratio);
            out.push((*axis, rule));
            a.collect_rules(first, out);
            b.collect_rules(second, out);
        }
    }

    /// Every pane, in screen order.
    pub fn ids(&self) -> Vec<PaneId> {
        self.rects(Rect::default())
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    }

    /// The first pane in screen order.
    fn first(&self) -> PaneId {
        match self {
            Layout::Leaf(id) => *id,
            Layout::Split { a, .. } => a.first(),
        }
    }

    /// Split the leaf `target`, `new` taking the second half. Returns whether
    /// `target` was found.
    pub fn split_leaf(&mut self, target: PaneId, axis: Axis, new: PaneId) -> bool {
        match self {
            Layout::Leaf(id) if *id == target => {
                *self = Layout::split(axis, Layout::Leaf(target), Layout::Leaf(new));
                true
            }
            Layout::Leaf(_) => false,
            Layout::Split { a, b, .. } => {
                a.split_leaf(target, axis, new) || b.split_leaf(target, axis, new)
            }
        }
    }

    /// Remove the leaf `target`; its sibling takes the whole of the split.
    ///
    /// Returns the pane that should have focus next: the first, in screen
    /// order, of whatever took the space. `None` when `target` is not here or
    /// is the only leaf, which never closes.
    pub fn remove(&mut self, target: PaneId) -> Option<PaneId> {
        let Layout::Split { a, b, .. } = self else {
            return None;
        };
        let survivor = if **a == Layout::Leaf(target) {
            std::mem::replace(&mut **b, Layout::Leaf(target))
        } else if **b == Layout::Leaf(target) {
            std::mem::replace(&mut **a, Layout::Leaf(target))
        } else {
            return a.remove(target).or_else(|| b.remove(target));
        };
        *self = survivor;
        Some(self.first())
    }

    fn collect(&self, area: Rect, out: &mut Vec<(PaneId, Rect)>) {
        match self {
            Layout::Leaf(id) => out.push((*id, area)),
            Layout::Split { axis, ratio, a, b } => {
                let (first, _, second) = divide(area, *axis, *ratio);
                a.collect(first, out);
                b.collect(second, out);
            }
        }
    }
}

/// Split `area` into `(first, rule, second)` along `axis`.
///
/// The rule is one cell. `ratio` is the first child's share of what is left
/// after it, rounded down, so an odd cell goes to the second child.
fn divide(area: Rect, axis: Axis, ratio: f32) -> (Rect, Rect, Rect) {
    let total = match axis {
        Axis::Columns => area.width,
        Axis::Rows => area.height,
    };
    let rule = total.min(1);
    let room = total - rule;
    let first = ((room as f32 * ratio.clamp(0.0, 1.0)) as u16).min(room);
    let second = room - first;
    match axis {
        Axis::Columns => (
            Rect::new(area.x, area.y, first, area.height),
            Rect::new(area.x + first, area.y, rule, area.height),
            Rect::new(area.x + first + rule, area.y, second, area.height),
        ),
        Axis::Rows => (
            Rect::new(area.x, area.y, area.width, first),
            Rect::new(area.x, area.y + first, area.width, rule),
            Rect::new(area.x, area.y + first + rule, area.width, second),
        ),
    }
}
