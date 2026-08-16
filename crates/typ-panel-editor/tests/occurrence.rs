//! `Ctrl+D` and `Ctrl+Shift+L`.
//!
//! TYPE shipped add-cursor-above and add-cursor-below, which is the *rarer*
//! half of multi-cursor. Select-next-occurrence is the idiom people mean when
//! they say the word, and it is what the `Selections` model was built to make
//! cheap.

use typ_buffer::{Position, Selection};
use typ_core::{Action, Panel};
use typ_panel_editor::EditorPanel;

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

fn ranges(panel: &EditorPanel) -> Vec<(Position, Position)> {
    panel.selections().iter().map(|s| s.range()).collect()
}

#[test]
fn the_first_press_selects_the_word_under_the_cursor() {
    let mut panel = EditorPanel::from_str("let value = value + 1;\n");
    panel.set_selections_for_test(vec![Selection::caret(pos(0, 5))]);

    panel.apply_action(Action::SelectNextOccurrence);

    // One press, one selection: "select this word". The second press is what
    // means "and the next one", and that two-stage shape is what makes the key
    // feel like a single gesture.
    assert_eq!(ranges(&panel), vec![(pos(0, 4), pos(0, 9))]);
}

#[test]
fn the_second_press_adds_a_cursor_at_the_next_occurrence() {
    let mut panel = EditorPanel::from_str("let value = value + 1;\n");
    panel.set_selections_for_test(vec![Selection::caret(pos(0, 5))]);

    panel.apply_action(Action::SelectNextOccurrence);
    panel.apply_action(Action::SelectNextOccurrence);

    assert_eq!(
        ranges(&panel),
        vec![(pos(0, 4), pos(0, 9)), (pos(0, 12), pos(0, 17))]
    );
}

#[test]
fn the_newly_added_occurrence_becomes_the_primary() {
    let mut panel = EditorPanel::from_str("value value\n");
    panel.set_selections_for_test(vec![Selection::caret(pos(0, 0))]);
    panel.apply_action(Action::SelectNextOccurrence);
    panel.apply_action(Action::SelectNextOccurrence);

    // The primary is what the next press searches from, and what every motion
    // is relative to. It has to be the one just added or the third press finds
    // the second occurrence all over again.
    assert_eq!(
        panel.selections().primary().range(),
        (pos(0, 6), pos(0, 11))
    );
}

#[test]
fn it_wraps_to_occurrences_above_the_cursor() {
    let mut panel = EditorPanel::from_str("value\nother\nvalue\n");
    panel.set_selections_for_test(vec![Selection::caret(pos(2, 0))]);

    panel.apply_action(Action::SelectNextOccurrence);
    panel.apply_action(Action::SelectNextOccurrence);

    assert_eq!(
        ranges(&panel),
        vec![(pos(0, 0), pos(0, 5)), (pos(2, 0), pos(2, 5))]
    );
}

#[test]
fn it_stops_once_every_occurrence_is_selected() {
    let mut panel = EditorPanel::from_str("value value\n");
    panel.set_selections_for_test(vec![Selection::caret(pos(0, 0))]);

    for _ in 0..10 {
        panel.apply_action(Action::SelectNextOccurrence);
    }

    // Ten presses, two occurrences. Looping forever — or worse, stacking
    // duplicate cursors on the same text — is the failure this guards.
    assert_eq!(panel.selections().len(), 2);
}

#[test]
fn a_word_with_only_one_occurrence_selects_once_and_stays_put() {
    let mut panel = EditorPanel::from_str("unique thing\n");
    panel.set_selections_for_test(vec![Selection::caret(pos(0, 0))]);

    panel.apply_action(Action::SelectNextOccurrence);
    panel.apply_action(Action::SelectNextOccurrence);

    assert_eq!(ranges(&panel), vec![(pos(0, 0), pos(0, 6))]);
}

#[test]
fn matching_is_case_sensitive_unlike_the_search_box() {
    // Ctrl+F is smart-case because finding prose is a different job. Matching
    // an identifier is not: `value` and `Value` are two different things and
    // every editor in the field draws that line the same way.
    let mut panel = EditorPanel::from_str("value Value value\n");
    panel.set_selections_for_test(vec![Selection::caret(pos(0, 0))]);

    panel.apply_action(Action::SelectNextOccurrence);
    panel.apply_action(Action::SelectNextOccurrence);

    assert_eq!(
        ranges(&panel),
        vec![(pos(0, 0), pos(0, 5)), (pos(0, 12), pos(0, 17))]
    );
}

#[test]
fn an_existing_selection_is_used_verbatim_rather_than_its_word() {
    let mut panel = EditorPanel::from_str("ab_cd xx ab_cd\n");
    // Select just "b_c", which is not a word by any boundary rule.
    panel.set_selections_for_test(vec![Selection {
        anchor: pos(0, 1),
        head: pos(0, 4),
    }]);

    panel.apply_action(Action::SelectNextOccurrence);

    assert_eq!(
        ranges(&panel),
        vec![(pos(0, 1), pos(0, 4)), (pos(0, 10), pos(0, 13))]
    );
}

#[test]
fn a_cursor_in_whitespace_has_no_word_to_select() {
    let mut panel = EditorPanel::from_str("a    b\n");
    panel.set_selections_for_test(vec![Selection::caret(pos(0, 2))]);

    // Handled, and nothing to do — not "unhandled", which would let the app try
    // it as an app action.
    let events = panel.apply_action(Action::SelectNextOccurrence);
    assert!(events.is_some());
    assert_eq!(ranges(&panel), vec![(pos(0, 2), pos(0, 2))]);
}

#[test]
fn select_all_occurrences_takes_them_in_one_press() {
    let mut panel = EditorPanel::from_str("v\nv\nv\nv\n");
    panel.set_selections_for_test(vec![Selection::caret(pos(0, 0))]);

    panel.apply_action(Action::SelectAllOccurrences);

    assert_eq!(panel.selections().len(), 4);
    assert_eq!(ranges(&panel)[3], (pos(3, 0), pos(3, 1)));
}

#[test]
fn select_all_occurrences_starts_from_the_word_under_the_cursor_too() {
    let mut panel = EditorPanel::from_str("let value = value;\n");
    panel.set_selections_for_test(vec![Selection::caret(pos(0, 6))]);

    panel.apply_action(Action::SelectAllOccurrences);

    assert_eq!(
        ranges(&panel),
        vec![(pos(0, 4), pos(0, 9)), (pos(0, 12), pos(0, 17))]
    );
}

#[test]
fn typing_after_selecting_occurrences_edits_every_one() {
    // The whole point of the feature: this is what people mean by multi-cursor.
    let mut panel = EditorPanel::from_str("foo\nfoo\n");
    panel.set_selections_for_test(vec![Selection::caret(pos(0, 0))]);

    panel.apply_action(Action::SelectAllOccurrences);
    panel.apply_action(Action::InsertChar('X'));

    assert_eq!(panel.line_text(0), "X");
    assert_eq!(panel.line_text(1), "X");
}
