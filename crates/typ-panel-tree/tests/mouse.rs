//! Clicking the file tree.
//!
//! Invariant 8 makes the mouse and the keyboard peers, and until this file the
//! tree had no mouse test at all: `handle_mouse` and `handle_scroll` were the
//! only panel entry points in the workspace with none. That is how gap 75
//! survived: `event.row.saturating_sub(inner.y)` turns every row *above* the
//! list into row 0, so clicking the panel's own title bar acted on the first
//! visible entry. The picker has had a test for exactly this shape since M2.8.

use std::path::PathBuf;

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use typ_core::{Panel, PanelEvent};
use typ_panel_tree::TreePanel;

/// The panel occupies rows 0..12. Row 0 is the frame's top border, row 11 the
/// bottom one, so the list is rows 1..11 and entry 0 is drawn at row 1.
const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 30,
    height: 12,
};

/// `sub/` (with a file inside, so expanding it is visible), then `a.rs`.
/// Directories sort first, so entry 0 is `sub/` and entry 1 is `a.rs`.
fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-tree-mouse").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/c.rs"), "").unwrap();
    std::fs::write(dir.join("a.rs"), "").unwrap();
    dir
}

fn tree(name: &str) -> TreePanel {
    let t = TreePanel::new(&fixture(name)).unwrap();
    assert_eq!(t.entry_count(), 2, "fixture: sub/ collapsed, and a.rs");
    t
}

fn click_at(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// A click on the list row drawn for `index`.
fn click_row(index: u16) -> MouseEvent {
    click_at(2, AREA.y + 1 + index)
}

#[test]
fn clicking_the_title_bar_does_nothing() {
    // A fresh tree has selected == top_line == 0, so a row that saturates to 0
    // lands on the already-selected entry and *activates* it: expanding the
    // first directory, or opening the first file. Gap 75.
    let mut t = tree("title-bar");
    let events = t.handle_mouse(click_at(2, AREA.y), AREA);

    assert!(events.is_empty(), "got {events:?}");
    assert_eq!(t.entry_count(), 2, "the title bar expanded sub/");
}

#[test]
fn clicking_the_bottom_border_does_nothing() {
    // The other end of the same missing guard: `top_line + inner.height` is one
    // past the last visible row, which selects an entry that is off screen.
    let mut t = tree("bottom-border");
    let events = t.handle_mouse(click_at(2, AREA.bottom() - 1), AREA);
    assert!(events.is_empty(), "got {events:?}");
}

#[test]
fn clicking_outside_the_panel_does_nothing() {
    let mut t = tree("outside");
    assert!(t.handle_mouse(click_at(AREA.right(), 3), AREA).is_empty());
    assert!(
        t.handle_mouse(click_at(2, AREA.bottom() + 4), AREA)
            .is_empty()
    );
}

#[test]
fn clicking_a_row_selects_it() {
    let mut t = tree("select");
    t.handle_mouse(click_row(1), AREA);
    assert_eq!(t.selected().unwrap().file_name().unwrap(), "a.rs");
}

#[test]
fn clicking_the_selected_row_activates_it() {
    // Two clicks, matching how a GUI file tree behaves: the first selects, the
    // second acts. On a file that is an open; on a directory, an expand.
    let mut t = tree("activate");
    t.handle_mouse(click_row(1), AREA);
    let events = t.handle_mouse(click_row(1), AREA);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, PanelEvent::OpenFile { .. })),
        "got {events:?}"
    );
}

#[test]
fn clicking_a_directory_expands_it() {
    // Select `a.rs` first, so `sub/` is genuinely being selected and then acted
    // on rather than activated by the first click: a fresh tree already has
    // entry 0 selected, which is the state the title-bar bug above exploits.
    let mut t = tree("expand");
    t.handle_mouse(click_row(1), AREA);
    t.handle_mouse(click_row(0), AREA);
    assert_eq!(t.entry_count(), 2, "the first click should only select");
    t.handle_mouse(click_row(0), AREA);
    assert_eq!(t.entry_count(), 3, "sub/c.rs did not appear");
}
