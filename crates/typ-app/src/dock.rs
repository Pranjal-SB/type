//! A dock: a region against one edge of the body, headed by a labelled rule
//! and divided from the panes by a plain one (interface §2).
//!
//! Geometry and posture only. What the left dock shows is the tree, which the
//! app holds; this says where it goes, and render and hit-testing both ask
//! [`Dock::areas`], so a click lands on the cell that was painted there.

use ratatui::layout::Rect;
use typ_core::Edge;

/// Preferred dock width in columns, its rule included: what the boxed sidebar
/// took before M4.
pub const DOCK_SIZE: u16 = 30;
/// Below this body width the dock takes a third instead of its size.
const NARROW_THRESHOLD: u16 = 60;
/// The narrowest an open dock may be dragged or nudged to, its rule included.
pub const MIN_DOCK: u16 = 12;
/// Under this many terminal columns a Standard layout renders as Focus, unless
/// the user chose the layout by hand (interface §7).
pub const ADAPT_WIDTH: u16 = 100;

/// A layout preset (interface §7). Presets govern the dock and never panes: a
/// preset that closed or opened panes would discard the user's split.
///
/// Wide waits for a right dock and Review for the review lane, so two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preset {
    /// The editor only, the dock folded to its rule. The shipped default
    /// (interface §7): a file gets the whole screen until the user asks.
    #[default]
    Focus,
    /// The left dock open beside the panes.
    Standard,
}

impl Preset {
    /// The name `config.toml` and the status bar use.
    pub fn name(self) -> &'static str {
        match self {
            Preset::Focus => "focus",
            Preset::Standard => "standard",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Preset::Focus, Preset::Standard]
            .into_iter()
            .find(|preset| preset.name() == name)
    }

    /// The one after this, wrapping.
    pub fn next(self) -> Self {
        match self {
            Preset::Focus => Preset::Standard,
            Preset::Standard => Preset::Focus,
        }
    }
}

/// One dock's state.
///
/// `edge` is data for the docks to come (decision 2 of the M4 plan): only a
/// left dock exists, and [`Dock::areas`] lays out that one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dock {
    pub edge: Edge,
    /// Columns, rule included, while open.
    pub size: u16,
    pub open: bool,
    pub zoomed: bool,
}

/// Where a dock and the panes beside it are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DockAreas {
    /// The labelled rule along the dock's top row.
    pub heading: Rect,
    /// The panel under the heading.
    pub content: Rect,
    /// The one column between the dock and the panes.
    pub rule: Rect,
    /// Everything else: where the panes go.
    pub rest: Rect,
    /// The ` ◰` cut into the heading's right end, one cell of rule after it.
    /// Zero wide when the heading is too narrow to carry it.
    pub zoom: Rect,
}

impl Dock {
    /// The left dock as the app starts: open, at its preferred size.
    pub fn left() -> Self {
        Dock {
            edge: Edge::Left,
            size: DOCK_SIZE,
            open: true,
            zoomed: false,
        }
    }

    /// Split `body` into the dock's regions and the rest.
    ///
    /// On a narrow terminal a fixed width would leave the panes nothing, so the
    /// dock degrades to a third of the body. The width counts the rule.
    ///
    /// ponytail: left edge only; a right or bottom dock mirrors this when its
    /// first panel lands.
    pub fn areas(&self, body: Rect) -> DockAreas {
        let width = if !self.open {
            // Folded: the rule alone, `▸` at its top.
            1
        } else if body.width < NARROW_THRESHOLD {
            (body.width / 3).max(1)
        } else {
            // Never over half, so a terminal that shrinks under a wide dock
            // still leaves the panes as much as the dock has.
            self.size.min(body.width / 2)
        }
        .min(body.width);
        // Zoomed, the dock is the whole body and the panes get nothing.
        let (width, list) = if self.zoomed && self.open {
            (body.width, body.width)
        } else {
            (width, width.saturating_sub(1))
        };
        let heading_rows = body.height.min(1);
        let heading = Rect::new(body.x, body.y, list, heading_rows);
        DockAreas {
            heading,
            content: Rect::new(
                body.x,
                body.y + heading_rows,
                list,
                body.height - heading_rows,
            ),
            rule: Rect::new(body.x + list, body.y, width - list, body.height),
            rest: Rect::new(body.x + width, body.y, body.width - width, body.height),
            zoom: crate::layout::zoom_button(heading, 1),
        }
    }

    /// Put the rule at `column`, clamped so the dock is at least
    /// [`MIN_DOCK`] and at most half of `body`.
    pub fn place_rule(&mut self, body: Rect, column: u16) {
        let high = body.width / 2;
        if MIN_DOCK > high {
            return;
        }
        self.size = (column.saturating_sub(body.x) + 1).clamp(MIN_DOCK, high);
    }

    /// Move the rule `delta` columns, clamped as [`Dock::place_rule`] is.
    pub fn nudge_rule(&mut self, body: Rect, delta: i32) {
        let at = self.areas(body).rule.x as i32 + delta;
        self.place_rule(body, at.clamp(0, u16::MAX as i32) as u16);
    }
}

impl DockAreas {
    /// The dock hidden: no heading, list or rule, the panes the whole body.
    /// What a zoomed pane is given.
    pub fn hidden(body: Rect) -> Self {
        let nothing = Rect::new(body.x, body.y, 0, 0);
        DockAreas {
            heading: nothing,
            content: nothing,
            rule: nothing,
            rest: body,
            zoom: nothing,
        }
    }
}
