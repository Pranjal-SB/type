//! Invariant 8: every picker interaction works with a mouse too.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use typ_core::{KeyChord, Panel, PanelEvent};
use typ_find::FileHit;
use typ_picker::Picker;

/// The overlay's rect. Deliberately not at the origin: a hit-test that forgets
/// to subtract the panel's own position passes every test anchored at (0, 0).
const AREA: Rect = Rect {
    x: 10,
    y: 4,
    width: 40,
    height: 10,
};

fn hits(n: usize) -> Vec<FileHit> {
    (0..n)
        .map(|i| FileHit {
            path: format!("file{i}.rs"),
            indices: Vec::new(),
        })
        .collect()
}

fn picker(n: usize) -> Picker {
    let mut picker = Picker::new();
    picker.set_hits(hits(n));
    picker
}

fn click(x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

/// Screen row of the nth visible list row.
///
/// The overlay's border takes one row, the query line the next, the rule the
/// one after; the list starts at offset 3 from the rect's top.
fn row_y(row: u16) -> u16 {
    AREA.y + 3 + row
}

#[test]
fn clicking_a_row_opens_it() {
    let mut picker = picker(5);
    let events = picker.handle_mouse(click(AREA.x + 5, row_y(2)), AREA);

    let opened = events.iter().find_map(|event| match event {
        PanelEvent::OpenFile { path, .. } => Some(path.to_string_lossy().to_string()),
        _ => None,
    });
    assert_eq!(opened.as_deref(), Some("file2.rs"), "got {events:?}");
}

#[test]
fn clicking_resolves_against_the_scroll_offset_not_the_whole_list() {
    // The bug this guards opens the third file in the project when you click
    // the third *visible* row after scrolling — which is right exactly once,
    // before anyone scrolls.
    let mut picker = picker(50);
    for _ in 0..20 {
        picker.handle_key(KeyChord::from_event(KeyEvent::new(
            KeyCode::Down,
            KeyModifiers::NONE,
        )));
    }
    // Settle the offset against the same height the render uses.
    let rows = (AREA.height - 4) as usize;
    let first_visible = picker.visible(rows)[0].path.clone();
    assert_ne!(first_visible, "file0.rs", "the list did not scroll");

    let events = picker.handle_mouse(click(AREA.x + 5, row_y(0)), AREA);
    let opened = events.iter().find_map(|event| match event {
        PanelEvent::OpenFile { path, .. } => Some(path.to_string_lossy().to_string()),
        _ => None,
    });
    assert_eq!(opened.as_deref(), Some(first_visible.as_str()));
}

#[test]
fn clicking_the_query_line_does_not_open_anything() {
    let mut picker = picker(5);
    let events = picker.handle_mouse(click(AREA.x + 5, AREA.y + 1), AREA);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, PanelEvent::OpenFile { .. })),
        "got {events:?}"
    );
}

#[test]
fn clicking_past_the_last_row_opens_nothing() {
    // Three hits in a ten-row box leaves blank rows. Clicking one must not
    // resolve to the last hit, and must certainly not index past the end.
    let mut picker = picker(3);
    let events = picker.handle_mouse(click(AREA.x + 5, row_y(5)), AREA);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, PanelEvent::OpenFile { .. })),
        "got {events:?}"
    );
}

#[test]
fn clicking_outside_the_overlay_dismisses_it() {
    // Every GUI picker closes on a click away from it, and a modal with no way
    // out but the keyboard is the thing invariant 8 exists to prevent.
    let mut picker = picker(5);
    let events = picker.handle_mouse(click(0, 0), AREA);
    assert!(events.contains(&PanelEvent::CloseSelf), "got {events:?}");
}

#[test]
fn clicking_the_border_neither_opens_nor_dismisses() {
    // The border belongs to the overlay, so a click on it is not "outside" —
    // but there is no row there either. Doing nothing is the honest answer.
    let mut picker = picker(5);
    let events = picker.handle_mouse(click(AREA.x, AREA.y), AREA);
    assert!(
        !events.contains(&PanelEvent::CloseSelf),
        "a click on its own border dismissed it: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, PanelEvent::OpenFile { .. })),
        "got {events:?}"
    );
}

#[test]
fn scrolling_moves_the_list_without_opening_anything() {
    let mut picker = picker(50);
    let events = picker.handle_scroll(3, AREA);
    assert!(picker.offset() > 0, "the list did not scroll");
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, PanelEvent::OpenFile { .. })),
        "a scroll opened a file"
    );
}

#[test]
fn scrolling_up_stops_at_the_top() {
    let mut picker = picker(50);
    picker.handle_scroll(-5, AREA);
    assert_eq!(picker.offset(), 0);
}

#[test]
fn scrolling_down_stops_at_the_end() {
    let mut picker = picker(8);
    picker.handle_scroll(1_000, AREA);
    let rows = (AREA.height - 4) as usize;
    assert!(
        picker.offset() <= 8usize.saturating_sub(rows),
        "scrolled past the end: offset {}",
        picker.offset()
    );
}

#[test]
fn a_scroll_on_an_empty_list_is_not_a_panic() {
    let mut picker = Picker::new();
    picker.handle_scroll(5, AREA);
    assert_eq!(picker.offset(), 0);
}

#[test]
fn a_click_on_a_degenerate_area_is_not_a_panic() {
    let mut picker = picker(5);
    let tiny = Rect::new(0, 0, 1, 1);
    picker.handle_mouse(click(0, 0), tiny);
    picker.handle_scroll(1, tiny);
}
