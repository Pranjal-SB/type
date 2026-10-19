//! The left dock: where it is, and that it shares the body with the panes
//! without a gap or an overlap.

use ratatui::layout::Rect;
use typ_app::dock::{Dock, DockAreas};

fn tiles(body: Rect, areas: &DockAreas) {
    let parts = [areas.heading, areas.content, areas.rule, areas.rest];
    let cells: u32 = parts.iter().map(|r| r.width as u32 * r.height as u32).sum();
    assert_eq!(
        cells,
        body.width as u32 * body.height as u32,
        "{body:?}: {areas:?} does not cover the body"
    );
    for (i, a) in parts.iter().enumerate() {
        assert_eq!(a.intersection(body), *a, "{body:?}: {a:?} leaves the body");
        for b in &parts[i + 1..] {
            assert!(
                a.is_empty() || b.is_empty() || !a.intersects(*b),
                "{body:?}: {a:?} overlaps {b:?}"
            );
        }
    }
}

#[test]
fn the_dock_and_the_panes_tile_the_body() {
    let dock = Dock::left();
    for width in 0..130 {
        for height in [0, 1, 2, 24] {
            let body = Rect::new(3, 2, width, height);
            tiles(body, &dock.areas(body));
        }
    }
}

#[test]
fn an_open_left_dock_is_thirty_columns_with_its_rule() {
    let areas = Dock::left().areas(Rect::new(0, 0, 100, 30));
    assert_eq!(areas.heading, Rect::new(0, 0, 29, 1));
    assert_eq!(areas.content, Rect::new(0, 1, 29, 29));
    assert_eq!(areas.rule, Rect::new(29, 0, 1, 30));
    assert_eq!(areas.rest, Rect::new(30, 0, 70, 30));
}

#[test]
fn a_folded_dock_is_its_rule_and_the_panes_take_the_rest() {
    let folded = Dock {
        open: false,
        ..Dock::left()
    };
    for width in 0..130 {
        for height in [0, 1, 2, 24] {
            let body = Rect::new(3, 2, width, height);
            tiles(body, &folded.areas(body));
        }
    }
    let areas = folded.areas(Rect::new(0, 0, 100, 30));
    assert_eq!(areas.rule, Rect::new(0, 0, 1, 30));
    assert_eq!(areas.rest, Rect::new(1, 0, 99, 30));
    assert_eq!(areas.content.width, 0);
}
