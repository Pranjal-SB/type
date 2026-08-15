use typ_buffer::{Position, Shift};

fn at(line: usize, col: usize) -> Position {
    Position { line, col }
}

#[test]
fn a_position_before_every_edit_is_untouched() {
    let mut shift = Shift::default();
    shift.record(5, at(5, 2), at(5, 4));
    assert_eq!(shift.apply(at(1, 0)), at(1, 0));
}

#[test]
fn inserting_on_a_line_moves_later_positions_on_that_line() {
    let mut shift = Shift::default();
    // Two characters landed where one column stood.
    shift.record(0, at(0, 1), at(0, 3));
    assert_eq!(shift.apply(at(0, 5)), at(0, 7));
}

#[test]
fn a_column_shift_does_not_leak_onto_another_line() {
    let mut shift = Shift::default();
    shift.record(0, at(0, 1), at(0, 3));
    assert_eq!(shift.apply(at(1, 5)), at(1, 5));
}

#[test]
fn inserting_a_newline_moves_every_later_line_down() {
    let mut shift = Shift::default();
    shift.record(0, at(0, 1), at(1, 0));
    assert_eq!(shift.apply(at(4, 2)).line, 5);
}

#[test]
fn shifts_on_one_line_accumulate() {
    let mut shift = Shift::default();
    shift.record(0, at(0, 1), at(0, 2));
    shift.record(0, at(0, 4), at(0, 5));
    // Two single-character inserts on the same line: a later position moves by
    // both, which is the multi-cursor case that made this type necessary.
    assert_eq!(shift.apply(at(0, 9)), at(0, 11));
}

#[test]
fn a_position_never_shifts_below_zero() {
    let mut shift = Shift::default();
    shift.record(0, at(0, 5), at(0, 0));
    assert_eq!(shift.apply(at(0, 1)), at(0, 0));
}
