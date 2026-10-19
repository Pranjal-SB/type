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

/// The smallest a pane may be dragged or nudged to: wide enough for a gutter
/// and a few characters, tall enough for its tab row and two lines.
pub const MIN_COLUMNS: u16 = 10;
pub const MIN_ROWS: u16 = 3;

/// Which split a rule belongs to: the turns from the root, `false` into the
/// first child and `true` into the second. Valid until the layout next
/// changes shape, which is why nothing holds one past a drag.
pub type SplitPath = Vec<bool>;

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

    /// The split whose rule is under `(column, row)`, if any.
    pub fn rule_at(&self, area: Rect, column: u16, row: u16) -> Option<SplitPath> {
        let Layout::Split { axis, ratio, a, b } = self else {
            return None;
        };
        let (first, rule, second) = divide(area, *axis, *ratio);
        if rule.contains((column, row).into()) {
            return Some(Vec::new());
        }
        let (turn, child, child_area) = if first.contains((column, row).into()) {
            (false, a, first)
        } else {
            (true, b, second)
        };
        let mut path = child.rule_at(child_area, column, row)?;
        path.insert(0, turn);
        Some(path)
    }

    /// The split nearest `leaf` along `axis`: its closest ancestor that
    /// divides that way. The rule an arrow in resize mode moves.
    pub fn nearest_split(&self, leaf: PaneId, axis: Axis) -> Option<SplitPath> {
        let Layout::Split {
            axis: here, a, b, ..
        } = self
        else {
            return None;
        };
        let (turn, child) = if a.contains(leaf) {
            (false, a)
        } else if b.contains(leaf) {
            (true, b)
        } else {
            return None;
        };
        if let Some(mut path) = child.nearest_split(leaf, axis) {
            path.insert(0, turn);
            return Some(path);
        }
        (*here == axis).then(Vec::new)
    }

    fn contains(&self, leaf: PaneId) -> bool {
        match self {
            Layout::Leaf(id) => *id == leaf,
            Layout::Split { a, b, .. } => a.contains(leaf) || b.contains(leaf),
        }
    }

    /// The axis of the split at `path`, if `path` names a split.
    pub fn axis_at(&self, path: &[bool]) -> Option<Axis> {
        match (self, path.split_first()) {
            (Layout::Split { axis, .. }, None) => Some(*axis),
            (Layout::Split { a, .. }, Some((false, rest))) => a.axis_at(rest),
            (Layout::Split { b, .. }, Some((true, rest))) => b.axis_at(rest),
            (Layout::Leaf(_), _) => None,
        }
    }

    /// Put the rule of the split at `path` at `cell`, an absolute column for a
    /// side-by-side split or row for a stacked one, clamped so neither side
    /// goes under its minimum.
    pub fn place_rule(&mut self, area: Rect, path: &[bool], cell: u16) {
        self.with_split(area, path, &mut |area, axis, ratio, a, b| {
            let (origin, total) = match axis {
                Axis::Columns => (area.x, area.width),
                Axis::Rows => (area.y, area.height),
            };
            let room = total.saturating_sub(1);
            if room == 0 {
                return;
            }
            let low = a.min_extent(axis);
            let high = room.saturating_sub(b.min_extent(axis));
            let first = cell.saturating_sub(origin);
            // When both minimums cannot fit, the split stays where it is
            // rather than favouring one side.
            if low > high {
                return;
            }
            let first = first.clamp(low, high);
            // Half a cell over, so `divide`'s floor lands on `first` exactly
            // whatever the float makes of the division.
            *ratio = (first as f32 + 0.5) / room as f32;
        });
    }

    /// Move the rule of the split at `path` by `delta` cells, clamped.
    pub fn nudge_rule(&mut self, area: Rect, path: &[bool], delta: i32) {
        let Some(axis) = self.axis_at(path) else {
            return;
        };
        let Some(rule) = self.rule_rect(area, path) else {
            return;
        };
        let at = match axis {
            Axis::Columns => rule.x,
            Axis::Rows => rule.y,
        };
        let to = (at as i32 + delta).clamp(0, u16::MAX as i32) as u16;
        self.place_rule(area, path, to);
    }

    /// Even the split at `path` again.
    pub fn even(&mut self, area: Rect, path: &[bool]) {
        self.with_split(area, path, &mut |_, _, ratio, _, _| *ratio = 0.5);
    }

    /// The rule rect of the split at `path`.
    fn rule_rect(&self, area: Rect, path: &[bool]) -> Option<Rect> {
        let Layout::Split { axis, ratio, a, b } = self else {
            return None;
        };
        let (first, rule, second) = divide(area, *axis, *ratio);
        match path.split_first() {
            None => Some(rule),
            Some((false, rest)) => a.rule_rect(first, rest),
            Some((true, rest)) => b.rule_rect(second, rest),
        }
    }

    /// Run `change` on the split at `path`, given the area it divides.
    fn with_split(
        &mut self,
        area: Rect,
        path: &[bool],
        change: &mut dyn FnMut(Rect, Axis, &mut f32, &Layout, &Layout),
    ) {
        let Layout::Split { axis, ratio, a, b } = self else {
            return;
        };
        let (first, _, second) = divide(area, *axis, *ratio);
        match path.split_first() {
            None => change(area, *axis, ratio, a, b),
            Some((false, rest)) => a.with_split(first, rest, change),
            Some((true, rest)) => b.with_split(second, rest, change),
        }
    }

    /// The fewest cells this layout can be along `axis` without a pane in it
    /// going under the minimum.
    fn min_extent(&self, axis: Axis) -> u16 {
        match self {
            Layout::Leaf(_) => match axis {
                Axis::Columns => MIN_COLUMNS,
                Axis::Rows => MIN_ROWS,
            },
            Layout::Split {
                axis: here, a, b, ..
            } if *here == axis => a.min_extent(axis) + 1 + b.min_extent(axis),
            Layout::Split { a, b, .. } => a.min_extent(axis).max(b.min_extent(axis)),
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
