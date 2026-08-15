use typ_buffer::{EditKind, Position, Selection, Selections, TextBuffer};

fn caret(line: usize, col: usize) -> Selections {
    Selections::single(Selection::caret(Position { line, col }))
}

fn text(buffer: &TextBuffer) -> String {
    (0..buffer.line_count())
        .map(|i| buffer.line_text(i))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn a_run_of_typing_undoes_as_one_step() {
    let mut buffer = TextBuffer::from_str("");
    for (i, ch) in "hello".chars().enumerate() {
        let at = Position { line: 0, col: i };
        buffer.begin_edit_group(EditKind::Insert, &caret(0, i));
        buffer.replace_range(at, at, &ch.to_string());
        buffer.end_edit_group();
    }
    assert_eq!(text(&buffer), "hello");

    buffer.undo(&caret(0, 5));
    assert_eq!(
        text(&buffer),
        "",
        "five characters typed in a row are one undo step"
    );
}

#[test]
fn moving_between_edits_ends_the_run() {
    let mut buffer = TextBuffer::from_str("");
    let at = Position { line: 0, col: 0 };
    buffer.begin_edit_group(EditKind::Insert, &caret(0, 0));
    buffer.replace_range(at, at, "ab");
    buffer.end_edit_group();

    buffer.undo_boundary();

    let at = Position { line: 0, col: 2 };
    buffer.begin_edit_group(EditKind::Insert, &caret(0, 2));
    buffer.replace_range(at, at, "cd");
    buffer.end_edit_group();

    buffer.undo(&caret(0, 4));
    assert_eq!(text(&buffer), "ab", "the boundary split the two runs");
}

#[test]
fn changing_what_the_edit_does_ends_the_run() {
    let mut buffer = TextBuffer::from_str("");
    let at = Position { line: 0, col: 0 };
    buffer.begin_edit_group(EditKind::Insert, &caret(0, 0));
    buffer.replace_range(at, at, "ab");
    buffer.end_edit_group();

    // No boundary call: the kind change alone must split the run, or
    // backspacing over a word would undo the word and the typing together.
    let from = Position { line: 0, col: 1 };
    let to = Position { line: 0, col: 2 };
    buffer.begin_edit_group(EditKind::Delete, &caret(0, 2));
    buffer.replace_range(from, to, "");
    buffer.end_edit_group();

    buffer.undo(&caret(0, 1));
    assert_eq!(text(&buffer), "ab");
}

#[test]
fn undo_returns_the_selections_the_edit_was_made_from() {
    let mut buffer = TextBuffer::from_str("one\ntwo\nthree");
    let before = caret(2, 4);
    let at = Position { line: 2, col: 4 };
    buffer.begin_edit_group(EditKind::Insert, &before);
    buffer.replace_range(at, at, "X");
    buffer.end_edit_group();

    let restored = buffer.undo(&caret(2, 5)).expect("something to undo");
    assert_eq!(
        restored.primary(),
        before.primary(),
        "undo puts the cursor back where the edit was made, not wherever clamping left it"
    );
}

#[test]
fn redo_returns_the_selections_from_after_the_edit() {
    let mut buffer = TextBuffer::from_str("abc");
    let at = Position { line: 0, col: 3 };
    buffer.begin_edit_group(EditKind::Insert, &caret(0, 3));
    buffer.replace_range(at, at, "d");
    buffer.end_edit_group();

    let after = caret(0, 4);
    buffer.undo(&after);
    let restored = buffer.redo(&caret(0, 3)).expect("something to redo");
    assert_eq!(restored.primary(), after.primary());
}

#[test]
fn undoing_an_untouched_buffer_reports_nothing_to_do() {
    let mut buffer = TextBuffer::from_str("abc");
    assert!(buffer.undo(&caret(0, 0)).is_none());
    assert!(buffer.redo(&caret(0, 0)).is_none());
}

#[test]
fn typing_after_an_undo_does_not_fold_into_the_step_it_undid() {
    let mut buffer = TextBuffer::from_str("");
    let at = Position { line: 0, col: 0 };
    buffer.begin_edit_group(EditKind::Insert, &caret(0, 0));
    buffer.replace_range(at, at, "ab");
    buffer.end_edit_group();

    buffer.undo(&caret(0, 2));
    assert_eq!(text(&buffer), "");

    buffer.begin_edit_group(EditKind::Insert, &caret(0, 0));
    buffer.replace_range(at, at, "cd");
    buffer.end_edit_group();

    buffer.undo(&caret(0, 2));
    assert_eq!(
        text(&buffer),
        "",
        "the undo ended the run before 'cd' began"
    );
}

#[test]
fn a_multi_caret_edit_stays_one_step() {
    let mut buffer = TextBuffer::from_str("a\na\na");
    let mut selections = Selections::single(Selection::caret(Position { line: 0, col: 1 }));
    selections.push(Selection::caret(Position { line: 1, col: 1 }));
    selections.push(Selection::caret(Position { line: 2, col: 1 }));

    buffer.begin_edit_group(EditKind::Insert, &selections);
    for line in (0..3).rev() {
        let at = Position { line, col: 1 };
        buffer.replace_range(at, at, "!");
    }
    buffer.end_edit_group();

    buffer.undo(&selections);
    assert_eq!(text(&buffer), "a\na\na");
}
