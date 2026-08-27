//! Moving a position forward through edits that have already been applied.
//!
//! `Shift` answers where the *next* edit in a batch goes, and moves every
//! position because within one batch there are none before the edit. A
//! diagnostic can sit anywhere, so `shift_through` compares against each edit's
//! own extent instead.

use typ_buffer::{EditSpan, Position, TextBuffer, shift_through};

fn at(line: usize, col: usize) -> Position {
    Position { line, col }
}

/// Inserting `text` at `at`, as a span.
fn insert(pos: Position, new_end: Position) -> EditSpan {
    EditSpan {
        start: pos,
        old_end: pos,
        new_end,
    }
}

#[test]
fn a_position_before_the_edit_does_not_move() {
    let edits = [insert(at(4, 0), at(5, 0))];
    assert_eq!(shift_through(at(1, 3), &edits), at(1, 3));
}

#[test]
fn a_position_after_a_newline_moves_down_a_line() {
    let edits = [insert(at(0, 0), at(1, 0))];
    assert_eq!(shift_through(at(5, 2), &edits), at(6, 2));
}

#[test]
fn a_position_on_the_edits_own_line_moves_by_columns_too() {
    // Two characters typed at the start of line 3.
    let edits = [insert(at(3, 0), at(3, 2))];
    assert_eq!(shift_through(at(3, 7), &edits), at(3, 9));
}

#[test]
fn a_position_on_a_later_line_keeps_its_column() {
    let edits = [insert(at(3, 0), at(3, 2))];
    assert_eq!(shift_through(at(4, 7), &edits), at(4, 7));
}

#[test]
fn a_position_inside_the_replaced_range_clamps_to_its_start() {
    // The text it named is gone. There is nowhere else honest for it to go.
    let edits = [EditSpan {
        start: at(2, 0),
        old_end: at(4, 0),
        new_end: at(2, 0),
    }];
    assert_eq!(shift_through(at(3, 5), &edits), at(2, 0));
}

#[test]
fn a_deletion_pulls_everything_after_it_up() {
    let edits = [EditSpan {
        start: at(2, 0),
        old_end: at(4, 0),
        new_end: at(2, 0),
    }];
    assert_eq!(shift_through(at(9, 1), &edits), at(7, 1));
}

#[test]
fn edits_fold_left_to_right() {
    // Each span is stated in the coordinates current when it ran, which is what
    // makes applying them in order the right answer.
    let edits = [insert(at(0, 0), at(1, 0)), insert(at(0, 0), at(1, 0))];
    assert_eq!(shift_through(at(5, 4), &edits), at(7, 4));
}

#[test]
fn nothing_moves_when_nothing_was_edited() {
    assert_eq!(shift_through(at(3, 3), &[]), at(3, 3));
}

#[test]
fn the_buffer_records_what_an_edit_did() {
    let mut buffer = TextBuffer::from_str("one\ntwo\nthree\n");
    buffer.replace_range(at(0, 0), at(0, 0), "\n");
    let edits = buffer.take_edits();
    assert_eq!(edits.len(), 1);
    assert_eq!(shift_through(at(2, 0), &edits), at(3, 0));
}

#[test]
fn taking_the_edits_empties_the_record() {
    // Draining is what bounds it. A consumer that stops draining grows a `Vec`
    // for the length of the session.
    let mut buffer = TextBuffer::from_str("one\n");
    buffer.replace_range(at(0, 0), at(0, 0), "x");
    assert_eq!(buffer.take_edits().len(), 1);
    assert!(buffer.take_edits().is_empty());
}

#[test]
fn an_edit_that_changes_nothing_records_nothing() {
    let mut buffer = TextBuffer::from_str("one\n");
    buffer.replace_range(at(0, 1), at(0, 1), "");
    assert!(buffer.take_edits().is_empty());
}
