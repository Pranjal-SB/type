use typ_buffer::{Position, Selection};
use typ_core::{Action, Direction, Motion, Panel};
use typ_panel_editor::EditorPanel;

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

fn del(direction: Direction, by_word: bool) -> Action {
    Action::Delete { direction, by_word }
}

fn mv(motion: Motion) -> Action {
    Action::Move {
        motion,
        extend: false,
    }
}

#[test]
fn typing_inserts_at_the_caret_and_advances_it() {
    let mut p = EditorPanel::from_str("ac\n");
    p.apply_action(mv(Motion::Right));
    p.apply_action(Action::InsertChar('b'));
    assert_eq!(p.line_text(0), "abc");
    assert_eq!(p.cursor(), pos(0, 2));
}

#[test]
fn typing_replaces_a_selection() {
    let mut p = EditorPanel::from_str("abcdef\n");
    p.set_selections_for_test(vec![Selection {
        anchor: pos(0, 1),
        head: pos(0, 4),
    }]);
    p.apply_action(Action::InsertChar('X'));
    assert_eq!(p.line_text(0), "aXef");
    assert_eq!(p.cursor(), pos(0, 2));
}

#[test]
fn typing_inserts_at_every_caret() {
    let mut p = EditorPanel::from_str("ab\nab\n");
    p.set_selections_for_test(vec![
        Selection::caret(pos(0, 1)),
        Selection::caret(pos(1, 1)),
    ]);
    p.apply_action(Action::InsertChar('-'));
    assert_eq!(p.line_text(0), "a-b");
    assert_eq!(p.line_text(1), "a-b");
    let heads: Vec<Position> = p.selections().iter().map(|s| s.head).collect();
    assert_eq!(heads, vec![pos(0, 2), pos(1, 2)]);
}

#[test]
fn multi_caret_edits_on_one_line_do_not_corrupt_each_other() {
    let mut p = EditorPanel::from_str("abcdef\n");
    p.set_selections_for_test(vec![
        Selection::caret(pos(0, 1)),
        Selection::caret(pos(0, 3)),
        Selection::caret(pos(0, 5)),
    ]);
    p.apply_action(Action::InsertChar('.'));
    assert_eq!(p.line_text(0), "a.bc.de.f");
    let heads: Vec<Position> = p.selections().iter().map(|s| s.head).collect();
    assert_eq!(heads, vec![pos(0, 2), pos(0, 5), pos(0, 8)]);
}

#[test]
fn enter_splits_at_the_caret() {
    let mut p = EditorPanel::from_str("ab\n");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 1))]);
    p.apply_action(Action::InsertNewline);
    assert_eq!(p.line_text(0), "a");
    assert_eq!(p.line_text(1), "b");
    assert_eq!(p.cursor(), pos(1, 0));
}

#[test]
fn backspace_deletes_one_grapheme_at_the_caret() {
    let mut p = EditorPanel::from_str("abc\n");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 2))]);
    p.apply_action(del(Direction::Backward, false));
    assert_eq!(p.line_text(0), "ac");
    assert_eq!(p.cursor(), pos(0, 1));
}

#[test]
fn backspace_with_a_selection_deletes_the_selection_and_nothing_more() {
    let mut p = EditorPanel::from_str("abcdef\n");
    p.set_selections_for_test(vec![Selection {
        anchor: pos(0, 1),
        head: pos(0, 4),
    }]);
    p.apply_action(del(Direction::Backward, false));
    assert_eq!(p.line_text(0), "aef");
    assert_eq!(p.cursor(), pos(0, 1));
}

#[test]
fn delete_forward_removes_the_grapheme_under_the_caret() {
    let mut p = EditorPanel::from_str("abc\n");
    p.apply_action(del(Direction::Forward, false));
    assert_eq!(p.line_text(0), "bc");
    assert_eq!(p.cursor(), pos(0, 0));
}

#[test]
fn delete_word_backward_removes_a_whole_word() {
    let mut p = EditorPanel::from_str("foo bar\n");
    p.apply_action(mv(Motion::LineEnd));
    p.apply_action(del(Direction::Backward, true));
    assert_eq!(p.line_text(0), "foo ");
}

#[test]
fn delete_word_forward_removes_a_whole_word() {
    let mut p = EditorPanel::from_str("foo bar\n");
    p.apply_action(del(Direction::Forward, true));
    assert_eq!(p.line_text(0), " bar");
}

#[test]
fn backspace_at_the_start_of_a_line_joins_it_to_the_previous() {
    let mut p = EditorPanel::from_str("ab\ncd\n");
    p.set_selections_for_test(vec![Selection::caret(pos(1, 0))]);
    p.apply_action(del(Direction::Backward, false));
    assert_eq!(p.line_text(0), "abcd");
    assert_eq!(p.cursor(), pos(0, 2));
}

#[test]
fn a_multi_caret_edit_undoes_as_one_step() {
    let mut p = EditorPanel::from_str("ab\nab\n");
    p.set_selections_for_test(vec![
        Selection::caret(pos(0, 1)),
        Selection::caret(pos(1, 1)),
    ]);
    p.apply_action(Action::InsertChar('-'));
    p.apply_action(Action::Undo);
    assert_eq!(p.line_text(0), "ab");
    assert_eq!(p.line_text(1), "ab", "both edits belong to one undo step");
}

#[test]
fn undo_then_redo_restores_the_edit() {
    let mut p = EditorPanel::from_str("ab\n");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 1))]);
    p.apply_action(Action::InsertChar('-'));
    p.apply_action(Action::Undo);
    p.apply_action(Action::Redo);
    assert_eq!(p.line_text(0), "a-b");
}

#[test]
fn undo_pulls_the_caret_back_inside_the_text() {
    let mut p = EditorPanel::from_str("ab\n");
    p.apply_action(mv(Motion::LineEnd));
    p.apply_action(Action::InsertChar('c'));
    p.apply_action(Action::InsertChar('d'));
    p.apply_action(Action::Undo);
    let line_len = p.line_text(0).chars().count();
    assert!(
        p.cursor().col <= line_len,
        "cursor at {} in a line of {line_len}",
        p.cursor().col
    );
}

#[test]
fn typing_a_word_undoes_as_one_step() {
    let mut p = EditorPanel::from_str("");
    for ch in "hello".chars() {
        p.apply_action(Action::InsertChar(ch));
    }
    assert_eq!(p.line_text(0), "hello");

    p.apply_action(Action::Undo);
    assert_eq!(
        p.line_text(0),
        "",
        "one press takes back the word, not the last letter"
    );
}

#[test]
fn moving_the_cursor_splits_a_typing_run() {
    let mut p = EditorPanel::from_str("");
    for ch in "ab".chars() {
        p.apply_action(Action::InsertChar(ch));
    }
    p.apply_action(mv(Motion::Left));
    p.apply_action(Action::InsertChar('c'));

    p.apply_action(Action::Undo);
    assert_eq!(p.line_text(0), "ab", "the motion ended the first run");
}

#[test]
fn backspacing_after_typing_is_its_own_step() {
    let mut p = EditorPanel::from_str("");
    for ch in "ab".chars() {
        p.apply_action(Action::InsertChar(ch));
    }
    p.apply_action(del(Direction::Backward, false));
    assert_eq!(p.line_text(0), "a");

    p.apply_action(Action::Undo);
    assert_eq!(
        p.line_text(0),
        "ab",
        "the kind change split the run without needing a motion"
    );
}

#[test]
fn undo_returns_the_cursor_to_where_the_edit_was_made() {
    let mut p = EditorPanel::from_str("one\ntwo\nthree\n");
    p.set_selections_for_test(vec![Selection::caret(pos(1, 3))]);
    p.apply_action(Action::InsertChar('!'));

    // Wander somewhere unrelated, the way a user would before noticing.
    p.apply_action(mv(Motion::DocumentStart));
    p.apply_action(Action::Undo);

    assert_eq!(p.line_text(1), "two");
    assert_eq!(
        p.cursor(),
        pos(1, 3),
        "undo shows what it undid rather than leaving the cursor where it was"
    );
}

#[test]
fn redo_returns_the_cursor_to_where_the_edit_left_it() {
    let mut p = EditorPanel::from_str("one\ntwo\n");
    p.set_selections_for_test(vec![Selection::caret(pos(1, 3))]);
    p.apply_action(Action::InsertChar('!'));
    p.apply_action(Action::Undo);
    p.apply_action(Action::Redo);

    assert_eq!(p.line_text(1), "two!");
    assert_eq!(p.cursor(), pos(1, 4));
}

#[test]
fn an_edit_reports_a_redraw() {
    let mut p = EditorPanel::from_str("ab\n");
    assert_eq!(
        p.apply_action(Action::InsertChar('x')),
        Some(vec![typ_core::PanelEvent::NeedsRedraw])
    );
}
