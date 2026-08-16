//! Copy, cut and paste, across one cursor or thirty.
//!
//! These drive the internal register rather than the system clipboard: a test
//! that reached for the real one would fail on a headless CI runner, race any
//! other test doing the same, and clobber whatever the developer had copied.
//! The register is the source of truth for paste anyway, so this tests the path
//! that actually decides behaviour.

use std::sync::{Mutex, MutexGuard, OnceLock};

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use typ_buffer::{Position, Selection, clipboard};
use typ_core::{Action, Panel};
use typ_panel_editor::EditorPanel;

/// The register is process-wide, which is what a clipboard *is* — and it means
/// these tests cannot run beside each other. Without this they interleave and
/// one test reads the string another just copied, which showed up as a paste
/// producing text that appears nowhere in its own fixture.
///
/// Serialising here rather than demanding `--test-threads=1` keeps the failure
/// impossible to reintroduce by running the suite the normal way.
fn exclusive() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

fn sel(from: (usize, usize), to: (usize, usize)) -> Selection {
    Selection {
        anchor: pos(from.0, from.1),
        head: pos(to.0, to.1),
    }
}

fn text(p: &EditorPanel) -> String {
    (0..p.line_count())
        .map(|i| p.line_text(i))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn copy_leaves_the_buffer_alone_and_fills_the_register() {
    let _guard = exclusive();
    let mut p = EditorPanel::from_str("hello world\n");
    p.set_selections_for_test(vec![sel((0, 0), (0, 5))]);

    p.apply_action(Action::Copy);

    assert_eq!(text(&p), "hello world\n");
    assert_eq!(clipboard::register(), "hello");
}

#[test]
fn cut_removes_the_selection_and_fills_the_register() {
    let _guard = exclusive();
    let mut p = EditorPanel::from_str("hello world\n");
    p.set_selections_for_test(vec![sel((0, 0), (0, 6))]);

    p.apply_action(Action::Cut);

    assert_eq!(text(&p), "world\n");
    assert_eq!(clipboard::register(), "hello ");
}

#[test]
fn cut_is_one_undo_step_even_with_several_cursors() {
    let _guard = exclusive();
    let mut p = EditorPanel::from_str("aXb\naXb\naXb\n");
    p.set_selections_for_test(vec![
        sel((0, 1), (0, 2)),
        sel((1, 1), (1, 2)),
        sel((2, 1), (2, 2)),
    ]);

    p.apply_action(Action::Cut);
    assert_eq!(text(&p), "ab\nab\nab\n");

    p.apply_action(Action::Undo);
    assert_eq!(
        text(&p),
        "aXb\naXb\naXb\n",
        "one cut is one undo, not one per cursor"
    );
}

#[test]
fn paste_at_a_caret_inserts() {
    let _guard = exclusive();
    let mut p = EditorPanel::from_str("ac\n");
    clipboard::set_register("b");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 1))]);

    p.apply_action(Action::Paste);

    assert_eq!(text(&p), "abc\n");
}

#[test]
fn paste_over_a_selection_replaces_it() {
    let _guard = exclusive();
    let mut p = EditorPanel::from_str("hello world\n");
    clipboard::set_register("goodbye");
    p.set_selections_for_test(vec![sel((0, 0), (0, 5))]);

    p.apply_action(Action::Paste);

    assert_eq!(text(&p), "goodbye world\n");
}

#[test]
fn copying_several_selections_joins_them_with_newlines() {
    let _guard = exclusive();
    let mut p = EditorPanel::from_str("one\ntwo\nthree\n");
    p.set_selections_for_test(vec![
        sel((0, 0), (0, 3)),
        sel((1, 0), (1, 3)),
        sel((2, 0), (2, 5)),
    ]);

    p.apply_action(Action::Copy);

    assert_eq!(clipboard::register(), "one\ntwo\nthree");
}

#[test]
fn pasting_a_line_per_cursor_distributes_one_to_each() {
    let _guard = exclusive();
    // The behaviour VS Code and Sublime both have: a multi-cursor copy followed
    // by a multi-cursor paste round-trips, rather than stamping the whole
    // clipboard at every cursor.
    let mut p = EditorPanel::from_str("..\n..\n..\n");
    clipboard::set_register("x\ny\nz");
    p.set_selections_for_test(vec![
        Selection::caret(pos(0, 1)),
        Selection::caret(pos(1, 1)),
        Selection::caret(pos(2, 1)),
    ]);

    p.apply_action(Action::Paste);

    assert_eq!(text(&p), ".x.\n.y.\n.z.\n");
}

#[test]
fn pasting_a_mismatched_line_count_stamps_the_whole_text_everywhere() {
    let _guard = exclusive();
    let mut p = EditorPanel::from_str("..\n..\n");
    clipboard::set_register("x\ny\nz");
    p.set_selections_for_test(vec![
        Selection::caret(pos(0, 1)),
        Selection::caret(pos(1, 1)),
    ]);

    p.apply_action(Action::Paste);

    assert_eq!(
        text(&p),
        ".x\ny\nz.\n.x\ny\nz.\n",
        "two cursors and three lines is not a distribution, so every cursor gets all of it"
    );
}

#[test]
fn pasting_an_empty_register_changes_nothing() {
    let _guard = exclusive();
    let mut p = EditorPanel::from_str("abc\n");
    clipboard::set_register("");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 1))]);

    p.apply_action(Action::Paste);

    assert_eq!(text(&p), "abc\n");
}

#[test]
fn copy_with_an_empty_selection_leaves_the_register_alone() {
    let _guard = exclusive();
    let mut p = EditorPanel::from_str("abc\n");
    clipboard::set_register("previous");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 1))]);

    p.apply_action(Action::Copy);

    assert_eq!(
        clipboard::register(),
        "previous",
        "copying nothing must not wipe what was already there"
    );
}

// Invariant 8: mouse and keyboard are peers, and every interaction gets a test
// both ways. A keyboard-only clipboard breaks it — right-click to copy and
// middle-click to paste are the mouse half.

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 40,
    height: 10,
};

fn click(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

/// Screen cell for a position in the text.
///
/// The text area sits inside the border *and* to the right of the gutter, so
/// this is one cell for the border plus the gutter's width. Every fixture in
/// this file is a two-line buffer, so the gutter is one digit and a spacer.
///
/// It is a helper rather than a literal because the geometry has now moved
/// twice — borders at M1.1, the gutter at M2.3 — and each time it moved, every
/// test that hardcoded a column was wrong in the same way at once.
fn text_cell(col: u16, line: u16) -> (u16, u16) {
    const BORDER: u16 = 1;
    const GUTTER: u16 = 2;
    (col + BORDER + GUTTER, line + BORDER)
}

#[test]
fn right_clicking_inside_a_selection_copies_it() {
    let _guard = exclusive();
    let mut p = EditorPanel::from_str("hello world\n");
    p.set_selections_for_test(vec![sel((0, 0), (0, 5))]);
    clipboard::set_register("previous");

    let (x, y) = text_cell(2, 0);
    p.handle_mouse(click(MouseEventKind::Down(MouseButton::Right), x, y), AREA);

    assert_eq!(clipboard::register(), "hello");
}

#[test]
fn right_clicking_inside_a_selection_keeps_it() {
    let _guard = exclusive();
    let mut p = EditorPanel::from_str("hello world\n");
    p.set_selections_for_test(vec![sel((0, 0), (0, 5))]);

    let (x, y) = text_cell(2, 0);
    p.handle_mouse(click(MouseEventKind::Down(MouseButton::Right), x, y), AREA);

    assert_eq!(
        p.selections().primary().range(),
        (pos(0, 0), pos(0, 5)),
        "copying must not collapse what was copied"
    );
}

#[test]
fn right_clicking_outside_a_selection_copies_nothing() {
    let _guard = exclusive();
    let mut p = EditorPanel::from_str("hello world\n");
    p.set_selections_for_test(vec![sel((0, 0), (0, 5))]);
    clipboard::set_register("previous");

    let (x, y) = text_cell(8, 0);
    p.handle_mouse(click(MouseEventKind::Down(MouseButton::Right), x, y), AREA);

    assert_eq!(
        clipboard::register(),
        "previous",
        "a right-click away from the selection is not a copy of it"
    );
}

#[test]
fn middle_clicking_pastes_at_the_pointer() {
    let _guard = exclusive();
    let mut p = EditorPanel::from_str("ac\n");
    clipboard::set_register("b");

    let (x, y) = text_cell(1, 0);
    p.handle_mouse(click(MouseEventKind::Down(MouseButton::Middle), x, y), AREA);

    assert_eq!(
        text(&p),
        "abc\n",
        "middle-click pastes where the pointer is"
    );
}

#[test]
fn a_cut_selection_pastes_back_identically() {
    let _guard = exclusive();
    let mut p = EditorPanel::from_str("hello world\n");
    p.set_selections_for_test(vec![sel((0, 0), (0, 6))]);

    p.apply_action(Action::Cut);
    p.apply_action(Action::Paste);

    assert_eq!(text(&p), "hello world\n");
}
