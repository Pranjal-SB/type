//! The layout tree: which pane gets which part of the editor area.

use ratatui::layout::Rect;
use typ_app::pane::{Axis, Layout, PaneId};

const AREA: Rect = Rect {
    x: 30,
    y: 0,
    width: 70,
    height: 29,
};

#[test]
fn one_leaf_is_the_whole_editor_area() {
    let layout = Layout::Leaf(PaneId(0));
    assert_eq!(layout.rects(AREA), vec![(PaneId(0), AREA)]);
}

#[test]
fn a_split_side_by_side_leaves_one_column_between_its_halves() {
    let layout = Layout::split(
        Axis::Columns,
        Layout::Leaf(PaneId(0)),
        Layout::Leaf(PaneId(1)),
    );
    let rects = layout.rects(AREA);
    // 70 columns: 34, the rule, 35. The odd column goes to the second.
    assert_eq!(
        rects,
        vec![
            (PaneId(0), Rect::new(30, 0, 34, 29)),
            (PaneId(1), Rect::new(65, 0, 35, 29)),
        ]
    );
}

#[test]
fn a_split_one_above_the_other_leaves_one_row_between_its_halves() {
    let layout = Layout::split(Axis::Rows, Layout::Leaf(PaneId(0)), Layout::Leaf(PaneId(1)));
    let rects = layout.rects(AREA);
    assert_eq!(
        rects,
        vec![
            (PaneId(0), Rect::new(30, 0, 70, 14)),
            (PaneId(1), Rect::new(30, 15, 70, 14)),
        ]
    );
}

#[test]
fn a_split_too_small_to_hold_its_rule_does_not_underflow() {
    let layout = Layout::split(
        Axis::Columns,
        Layout::Leaf(PaneId(0)),
        Layout::Leaf(PaneId(1)),
    );
    for width in 0..3 {
        let rects = layout.rects(Rect::new(0, 0, width, 5));
        assert_eq!(rects.len(), 2);
        assert!(rects.iter().all(|(_, r)| r.right() <= width));
    }
}
