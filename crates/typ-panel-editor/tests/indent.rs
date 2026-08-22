//! Tab indents, Shift+Tab outdents.
//!
//! Two behaviours behind one key, which is what every editor in the field does:
//! with nothing selected Tab inserts to the next tab stop, and with a selection
//! it shifts every line the selection touches. Conflating them — always
//! inserting, or always shifting whole lines — is wrong in one direction or the
//! other on every keypress.

use typ_buffer::{Position, Selection};
use typ_core::{Action, Panel};
use typ_panel_editor::EditorPanel;

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
fn tab_at_a_caret_goes_to_the_next_tab_stop() {
    let mut p = EditorPanel::from_str("ab\n");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 2))]);

    p.apply_action(Action::Indent);

    assert_eq!(
        text(&p),
        "ab  \n",
        "column 2 with a width of 4 needs two spaces, not four"
    );
    assert_eq!(p.cursor(), pos(0, 4));
}

#[test]
fn tab_at_the_start_of_a_line_inserts_a_full_level() {
    let mut p = EditorPanel::from_str("ab\n");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 0))]);

    p.apply_action(Action::Indent);

    assert_eq!(text(&p), "    ab\n");
}

#[test]
fn tab_with_a_selection_indents_every_line_it_touches() {
    let mut p = EditorPanel::from_str("one\ntwo\nthree\n");
    p.set_selections_for_test(vec![sel((0, 1), (1, 2))]);

    p.apply_action(Action::Indent);

    assert_eq!(
        text(&p),
        "    one\n    two\nthree\n",
        "the selection spans two lines, so two lines move"
    );
}

#[test]
fn indenting_a_selection_does_not_replace_it() {
    let mut p = EditorPanel::from_str("one\ntwo\n");
    p.set_selections_for_test(vec![sel((0, 0), (1, 3))]);

    p.apply_action(Action::Indent);

    assert_eq!(
        text(&p),
        "    one\n    two\n",
        "a selection is the target of an indent, not text to be overwritten"
    );
}

#[test]
fn the_selection_survives_an_indent_and_follows_the_text() {
    let mut p = EditorPanel::from_str("one\ntwo\n");
    p.set_selections_for_test(vec![sel((0, 1), (1, 2))]);

    p.apply_action(Action::Indent);

    assert_eq!(
        p.selections().primary().range(),
        (pos(0, 5), pos(1, 6)),
        "both ends shift by the four spaces added to their own line"
    );
}

#[test]
fn a_selection_ending_at_column_zero_leaves_that_line_alone() {
    let mut p = EditorPanel::from_str("one\ntwo\n");
    p.set_selections_for_test(vec![sel((0, 0), (1, 0))]);

    p.apply_action(Action::Indent);

    assert_eq!(
        text(&p),
        "    one\ntwo\n",
        "nothing on the second line is selected, so it is not part of the block"
    );
}

#[test]
fn indenting_skips_blank_lines() {
    let mut p = EditorPanel::from_str("one\n\ntwo\n");
    p.set_selections_for_test(vec![sel((0, 0), (2, 3))]);

    p.apply_action(Action::Indent);

    assert_eq!(
        text(&p),
        "    one\n\n    two\n",
        "indenting a blank line leaves trailing whitespace and nothing else"
    );
}

#[test]
fn outdent_removes_one_level() {
    // Three lines rather than one, so the file says four rather than leaving
    // an eight-column jump as the only evidence there is.
    let mut p = EditorPanel::from_str("a\n    b\n        deep\n");
    p.set_selections_for_test(vec![Selection::caret(pos(2, 8))]);

    p.apply_action(Action::Outdent);

    assert_eq!(text(&p), "a\n    b\n    deep\n");
}

#[test]
fn outdent_takes_a_partial_level_to_zero() {
    let mut p = EditorPanel::from_str("   three\n");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 3))]);

    p.apply_action(Action::Outdent);

    assert_eq!(
        text(&p),
        "three\n",
        "three spaces is less than a level, so it goes to zero rather than minus one"
    );
}

#[test]
fn outdent_on_an_unindented_line_does_nothing() {
    let mut p = EditorPanel::from_str("flush\n");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 2))]);

    p.apply_action(Action::Outdent);

    assert_eq!(
        text(&p),
        "flush\n",
        "there is no indentation to remove, so no character may be eaten"
    );
    assert_eq!(p.cursor(), pos(0, 2));
}

#[test]
fn outdent_removes_a_leading_tab_whole() {
    let mut p = EditorPanel::from_str("\ttabbed\n");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 1))]);

    p.apply_action(Action::Outdent);

    assert_eq!(text(&p), "tabbed\n");
}

#[test]
fn indent_across_several_cursors_is_one_undo_step() {
    let mut p = EditorPanel::from_str("a\nb\nc\n");
    p.set_selections_for_test(vec![
        Selection::caret(pos(0, 0)),
        Selection::caret(pos(1, 0)),
        Selection::caret(pos(2, 0)),
    ]);

    p.apply_action(Action::Indent);
    assert_eq!(text(&p), "    a\n    b\n    c\n");

    p.apply_action(Action::Undo);
    assert_eq!(
        text(&p),
        "a\nb\nc\n",
        "one indent is one undo, not one per cursor"
    );
}

#[test]
fn two_cursors_on_one_line_indent_it_once() {
    let mut p = EditorPanel::from_str("shared\n");
    p.set_selections_for_test(vec![sel((0, 0), (0, 2)), sel((0, 3), (0, 5))]);

    p.apply_action(Action::Indent);

    assert_eq!(
        text(&p),
        "    shared\n",
        "the line is indented once however many cursors are sitting on it"
    );
}

/// The width is measured from the buffer at load, so Tab inserts what the rest
/// of the file uses rather than what the editor happens to prefer.
#[test]
fn the_width_comes_from_the_file() {
    let source = "fn main() {\n  let a = 1;\n  if a {\n    b();\n  }\n}\n";
    let mut p = EditorPanel::from_str(source);
    assert_eq!(p.tab_width(), 2);

    p.set_selections_for_test(vec![Selection::caret(pos(0, 0))]);
    p.apply_action(Action::Indent);
    assert_eq!(p.line_text(0), "  fn main() {");
}

#[test]
fn a_file_that_says_nothing_keeps_the_default() {
    assert_eq!(EditorPanel::from_str("one\ntwo\n").tab_width(), 4);
    assert_eq!(EditorPanel::from_str("").tab_width(), 4);
}

#[test]
fn the_override_beats_the_measurement() {
    let mut p = EditorPanel::from_str("fn main() {\n  let a = 1;\n  if a {\n    b();\n  }\n}\n");
    assert_eq!(p.tab_width(), 2);
    p.set_tab_width(8);
    assert_eq!(p.tab_width(), 8);

    p.set_selections_for_test(vec![Selection::caret(pos(0, 0))]);
    p.apply_action(Action::Indent);
    assert_eq!(p.line_text(0), "        fn main() {");
}
