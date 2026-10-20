//! Every mouse target on screen, in one list (interface §5).
//!
//! Built from the geometry render and hit-testing already ask: `regions` for
//! the dock, `pane_areas` and `tabbar::cells` for the tab rows, the tree's
//! `visible`, `status_layout` and the menu's `row_rects`. Nothing here
//! measures a region a second time, so a warp label that clicks a target's
//! point lands where the mouse would.
//!
//! The editor's text is not a list of targets and contributes none until
//! diagnostics and code actions are (interface §12, slice 6).

use ratatui::layout::Rect;

use typ_core::Panel;

use crate::app::App;
use crate::status::SegmentId;

/// What a target is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    /// A tab in a pane's tab row. `pane` indexes `App::panes`, `tab` that
    /// pane's tabs.
    Tab { pane: usize, tab: usize },
    /// The number at the left of a pane's tab row.
    PaneNumber(usize),
    /// The ` ◰` at the right of a pane's tab row.
    PaneZoom(usize),
    /// The open dock's labelled rule.
    DockHeading,
    /// The ` ◰` cut into the dock's heading.
    DockZoom,
    /// The folded dock's column.
    DockFolded,
    /// A row of the tree, by entry index.
    TreeRow(usize),
    /// A status segment that does something.
    Segment(SegmentId),
    /// A row of the `ctrl+k` menu, by its index in the menu.
    MenuRow(usize),
    /// A row of the open float menu (the context menu, or the menu bar's
    /// dropdown), by its place among the rows it shows.
    FloatRow(usize),
    /// A group's name on the menu bar, by its place in `Group::ALL`.
    BarGroup(usize),
}

impl TargetKind {
    /// The first letter of its warp label (interface §5): `s` side, `t` tabs,
    /// `b` bar, `f` anything summoned over the rest.
    pub fn region(self) -> char {
        match self {
            TargetKind::Tab { .. } | TargetKind::PaneNumber(_) | TargetKind::PaneZoom(_) => 't',
            TargetKind::DockHeading
            | TargetKind::DockZoom
            | TargetKind::DockFolded
            | TargetKind::TreeRow(_) => 's',
            TargetKind::Segment(_) => 'b',
            TargetKind::MenuRow(_) | TargetKind::FloatRow(_) | TargetKind::BarGroup(_) => 'f',
        }
    }
}

/// One thing on screen a click does something to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    /// The cells it takes.
    pub rect: Rect,
    /// Where a click on it lands: inside `rect`, on a cell that does what
    /// clicking the thing does.
    pub point: (u16, u16),
    pub kind: TargetKind,
}

impl Target {
    fn at(rect: Rect, point: (u16, u16), kind: TargetKind) -> Self {
        Target { rect, point, kind }
    }

    /// The region letter its warp label starts with.
    pub fn region(&self) -> char {
        self.kind.region()
    }
}

impl App {
    /// Every target in `frame`, region by region and in screen order inside
    /// each: the dock, the tab rows, the status bar, then the menu.
    ///
    /// While the context menu or the menu bar is up, only theirs: a press
    /// anywhere else closes them and reaches nothing under them, so nothing
    /// under them is a target.
    pub fn targets(&self, frame: Rect) -> Vec<Target> {
        let float_rows = |rows: Vec<(usize, Rect)>| {
            rows.into_iter().map(|(index, rect)| {
                Target::at(rect, (rect.x, rect.y), TargetKind::FloatRow(index))
            })
        };
        if let Some(menu) = &self.context {
            let (body, _) = crate::layout::split_frame(frame);
            return float_rows(menu.row_rects(body)).collect();
        }
        if let Some(bar) = &self.bar {
            let mut out: Vec<Target> = crate::app::menu_bar::cells(frame)
                .into_iter()
                .map(|(index, rect)| {
                    Target::at(rect, (rect.x + 1, rect.y), TargetKind::BarGroup(index))
                })
                .collect();
            out.extend(float_rows(bar.dropdown_rows(frame)));
            return out;
        }
        let mut out = self.dock_targets(frame);
        out.extend(self.pane_targets(frame));
        out.extend(self.status_targets(frame));
        let (_, menu_area) = self.split_body(frame);
        if let (Some(menu), Some(area)) = (&self.menu, menu_area) {
            out.extend(menu.row_rects(area).into_iter().map(|(index, rect)| {
                Target::at(rect, (rect.x, rect.y), TargetKind::MenuRow(index))
            }));
        }
        out
    }

    fn dock_targets(&self, frame: Rect) -> Vec<Target> {
        let regions = self.regions(frame);
        if !self.dock.open {
            let rule = regions.rule;
            return match rule.is_empty() {
                true => Vec::new(),
                false => vec![Target::at(rule, (rule.x, rule.y), TargetKind::DockFolded)],
            };
        }
        let mut out = Vec::new();
        let heading = regions.heading;
        if !heading.is_empty() {
            // The heading less its zoom button, which is a target of its own.
            let zoom = regions.zoom;
            let width = match zoom.width {
                0 => heading.width,
                _ => zoom.x - heading.x,
            };
            let rect = Rect { width, ..heading };
            out.push(Target::at(rect, (rect.x, rect.y), TargetKind::DockHeading));
            if zoom.width > 0 {
                out.push(Target::at(zoom, (zoom.x + 1, zoom.y), TargetKind::DockZoom));
            }
        }
        let content = regions.content;
        for (row, entry) in (content.y..).zip(self.tree.visible(content)) {
            let rect = Rect::new(content.x, row, content.width, 1);
            out.push(Target::at(
                rect,
                (rect.x, rect.y),
                TargetKind::TreeRow(entry),
            ));
        }
        out
    }

    fn pane_targets(&self, frame: Rect) -> Vec<Target> {
        let mut out = Vec::new();
        for area in self.pane_areas(self.regions(frame).rest) {
            let number = area.number;
            if number.width > 0 {
                let point = (number.x + number.width / 2, number.y);
                out.push(Target::at(
                    number,
                    point,
                    TargetKind::PaneNumber(area.index),
                ));
            }
            let bar = area.bar;
            if bar.height > 0 {
                let pane = &self.panes[area.index];
                let labels: Vec<String> = pane.tabs.iter().map(|tab| tab.panel.title()).collect();
                for cell in crate::tabbar::cells(&labels, pane.active, bar.width) {
                    let rect = Rect::new(bar.x + cell.x, bar.y, cell.width, 1);
                    let kind = TargetKind::Tab {
                        pane: area.index,
                        tab: cell.index,
                    };
                    // The cell's leading space: never its close box.
                    out.push(Target::at(rect, (rect.x, rect.y), kind));
                }
            }
            let zoom = area.zoom;
            if zoom.width > 0 {
                let point = (zoom.x + 1, zoom.y);
                out.push(Target::at(zoom, point, TargetKind::PaneZoom(area.index)));
            }
        }
        out
    }

    fn status_targets(&self, frame: Rect) -> Vec<Target> {
        let (_, area) = crate::layout::split_frame(frame);
        self.status_layout(area)
            .segments
            .into_iter()
            .filter(|(segment, rect)| segment.id.action().is_some() && !rect.is_empty())
            .map(|(segment, rect)| {
                Target::at(rect, (rect.x, rect.y), TargetKind::Segment(segment.id))
            })
            .collect()
    }
}
