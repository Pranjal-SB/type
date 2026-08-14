use typ_buffer::{Position, Selection};
use typ_core::{Action, Direction, Motion, Panel};
use typ_panel_editor::EditorPanel;

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

fn heads(p: &EditorPanel) -> Vec<Position> {
    p.selections().iter().map(|s| s.head).collect()
}

#[test]
fn select_all_covers_the_whole_document_as_one_selection() {
    let mut p = EditorPanel::from_str("ab\ncd\n");
    p.apply_action(Action::SelectAll);
    assert_eq!(p.selections().len(), 1);
    let (start, end) = p.selections().primary().range();
    assert_eq!(start, pos(0, 0));
    assert_eq!(
        end,
        pos(2, 0),
        "the trailing newline leaves a final empty line"
    );
}

#[test]
fn select_line_covers_the_current_line_without_its_newline() {
    let mut p = EditorPanel::from_str("abc\ndef\n");
    p.set_selections_for_test(vec![Selection::caret(pos(1, 2))]);
    p.apply_action(Action::SelectLine);
    assert_eq!(p.selections().primary().range(), (pos(1, 0), pos(1, 3)));
}

#[test]
fn adding_a_cursor_below_puts_one_on_the_next_line() {
    let mut p = EditorPanel::from_str("abc\ndef\nghi\n");
    p.apply_action(Action::AddCursor(Direction::Forward));
    assert_eq!(p.selections().len(), 2);
    assert_eq!(heads(&p), vec![pos(0, 0), pos(1, 0)]);
}

#[test]
fn the_added_cursor_becomes_primary_so_repeating_extends_downwards() {
    let mut p = EditorPanel::from_str("a\nb\nc\nd\n");
    p.apply_action(Action::AddCursor(Direction::Forward));
    p.apply_action(Action::AddCursor(Direction::Forward));
    assert_eq!(heads(&p), vec![pos(0, 0), pos(1, 0), pos(2, 0)]);
}

#[test]
fn adding_a_cursor_above_walks_upwards() {
    let mut p = EditorPanel::from_str("abc\ndef\n");
    p.set_selections_for_test(vec![Selection::caret(pos(1, 1))]);
    p.apply_action(Action::AddCursor(Direction::Backward));
    assert_eq!(heads(&p), vec![pos(0, 1), pos(1, 1)]);
}

#[test]
fn adding_a_cursor_past_the_end_of_the_document_adds_nothing() {
    let mut p = EditorPanel::from_str("only\n");
    p.set_selections_for_test(vec![Selection::caret(pos(1, 0))]);
    let events = p.apply_action(Action::AddCursor(Direction::Forward));
    assert_eq!(p.selections().len(), 1);
    assert_eq!(
        events,
        Some(Vec::new()),
        "handled, with nothing to report — not declined"
    );
}

#[test]
fn a_cursor_added_to_a_shorter_line_clamps_to_that_line() {
    let mut p = EditorPanel::from_str("abcdef\nab\n");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 5))]);
    p.apply_action(Action::AddCursor(Direction::Forward));
    assert_eq!(heads(&p), vec![pos(0, 5), pos(1, 2)]);
}

#[test]
fn collapse_leaves_one_caret_at_the_primary_head() {
    let mut p = EditorPanel::from_str("abc\ndef\n");
    p.apply_action(Action::AddCursor(Direction::Forward));
    p.apply_action(Action::CollapseSelections);
    assert_eq!(p.selections().len(), 1);
    assert_eq!(p.cursor(), pos(1, 0));
}

#[test]
fn collapse_also_drops_a_selection_down_to_a_caret() {
    let mut p = EditorPanel::from_str("abcdef\n");
    p.apply_action(Action::SelectAll);
    p.apply_action(Action::CollapseSelections);
    assert!(p.selections().primary().is_empty());
}

#[test]
fn typing_with_several_cursors_then_collapsing_keeps_the_text() {
    let mut p = EditorPanel::from_str("a\na\n");
    p.apply_action(Action::AddCursor(Direction::Forward));
    p.apply_action(Action::InsertChar('!'));
    p.apply_action(Action::CollapseSelections);
    assert_eq!(p.line_text(0), "!a");
    assert_eq!(p.line_text(1), "!a");
    assert_eq!(p.selections().len(), 1);
}

#[test]
fn a_motion_that_merges_two_cursors_leaves_one() {
    let mut p = EditorPanel::from_str("ab\n");
    p.set_selections_for_test(vec![
        Selection::caret(pos(0, 0)),
        Selection::caret(pos(0, 1)),
    ]);
    // Both run into the start of the line and become the same caret.
    p.apply_action(Action::Move {
        motion: Motion::LineStart,
        extend: false,
    });
    assert_eq!(p.selections().len(), 1);
}

#[test]
fn select_all_then_typing_replaces_the_document() {
    let mut p = EditorPanel::from_str("throw away\n");
    p.apply_action(Action::SelectAll);
    p.apply_action(Action::InsertChar('x'));
    assert_eq!(p.line_text(0), "x");
    assert_eq!(p.cursor(), pos(0, 1));
}
