use typ_buffer::{EditKind, Position, Selection, Selections, TextBuffer};

/// The M1-era helpers take no selection set of their own, so these tests pass a
/// caret at the origin — undo's returned selections are covered in `undo.rs`.
fn origin() -> Selections {
    Selections::single(Selection::caret(Position { line: 0, col: 0 }))
}

#[test]
fn from_str_counts_lines() {
    let b = TextBuffer::from_str("a\nb\nc\n");
    assert_eq!(b.line_count(), 4); // trailing newline yields a final empty line
}

#[test]
fn line_text_excludes_the_newline() {
    let b = TextBuffer::from_str("hello\nworld\n");
    assert_eq!(b.line_text(0), "hello");
}

#[test]
fn insert_char_updates_the_line() {
    let mut b = TextBuffer::from_str("ac\n");
    b.insert_char(Position { line: 0, col: 1 }, 'b');
    assert_eq!(b.line_text(0), "abc");
}

#[test]
fn insert_marks_buffer_dirty() {
    let mut b = TextBuffer::from_str("a\n");
    assert!(!b.is_dirty());
    b.insert_char(Position { line: 0, col: 0 }, 'x');
    assert!(b.is_dirty());
}

#[test]
fn a_range_of_one_wide_grapheme_removes_all_of_it() {
    // Deletion is `replace_range` over grapheme positions. The removed
    // `delete_before`/`delete_after` tested this per helper; one range covers
    // both directions.
    let mut b = TextBuffer::from_str("日本語\n");
    b.replace_range(
        Position { line: 0, col: 1 },
        Position { line: 0, col: 2 },
        "",
    );
    assert_eq!(b.line_text(0), "日語");
}

#[test]
fn undo_restores_the_previous_content() {
    let mut b = TextBuffer::from_str("a\n");
    b.insert_char(Position { line: 0, col: 1 }, 'b');
    assert_eq!(b.line_text(0), "ab");
    b.undo(&origin());
    assert_eq!(b.line_text(0), "a");
}

#[test]
fn redo_reapplies_an_undone_edit() {
    let mut b = TextBuffer::from_str("a\n");
    b.insert_char(Position { line: 0, col: 1 }, 'b');
    b.undo(&origin());
    b.redo(&origin());
    assert_eq!(b.line_text(0), "ab");
}

#[test]
fn save_writes_to_disk_and_clears_dirty() {
    let dir = std::env::temp_dir().join("typ-buffer-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("save.txt");
    std::fs::write(&path, "old\n").unwrap();

    let mut b = TextBuffer::from_path(&path).unwrap();
    b.insert_char(Position { line: 0, col: 3 }, '!');
    b.save().unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old!\n");
    assert!(!b.is_dirty());
}

#[test]
fn undo_history_shares_the_rope_rather_than_copying_the_text() {
    // Ropey clones are O(1) and copy-on-write, so a deep undo stack over a
    // large buffer must not cost one full copy of the text per step. This
    // asserts the behaviour that guarantee buys: many edits stay fast and the
    // content stays exact.
    let big = "abcdefghij".repeat(20_000); // 200 KB
    let mut b = TextBuffer::from_str(&big);
    for i in 0..200 {
        b.insert_char(Position { line: 0, col: i }, 'x');
    }
    for _ in 0..200 {
        b.undo(&origin());
    }
    assert_eq!(b.line_text(0), big);
}

#[test]
fn saving_leaves_no_temporary_file_behind() {
    let dir = std::env::temp_dir().join("typ-buffer-atomic");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("save.txt");
    std::fs::write(&path, "old\n").unwrap();

    let mut b = TextBuffer::from_path(&path).unwrap();
    b.insert_char(Position { line: 0, col: 3 }, '!');
    b.save().unwrap();

    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name != "save.txt")
        .collect();
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old!\n");
}

#[test]
fn a_file_already_at_the_temp_path_is_never_written_through() {
    // The temp name is the file name plus the pid, so anyone who can write the
    // directory can put something there first: on Unix, a symlink to a file
    // of the user's they want overwritten. The save must not open it.
    let dir = std::env::temp_dir().join("typ-buffer-temp-planted");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("save.txt");
    std::fs::write(&path, "old\n").unwrap();
    let planted = dir.join(format!(".save.txt.{}.typ-tmp", std::process::id()));
    std::fs::write(&planted, "not yours\n").unwrap();

    let mut b = TextBuffer::from_path(&path).unwrap();
    b.insert_char(Position { line: 0, col: 3 }, '!');
    b.save().unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old!\n");
    assert_eq!(
        std::fs::read_to_string(&planted).unwrap(),
        "not yours\n",
        "the save opened a file it did not create"
    );
}

#[test]
fn a_save_that_cannot_be_written_leaves_the_original_untouched() {
    let dir = std::env::temp_dir().join("typ-buffer-atomic-fail");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("target.txt");
    std::fs::write(&path, "original\n").unwrap();

    let mut b = TextBuffer::from_path(&path).unwrap();
    b.insert_char(Position { line: 0, col: 0 }, 'X');

    // Point the buffer at a path that cannot be created: a directory of that
    // name already exists there.
    let blocked = dir.join("blocked");
    std::fs::create_dir_all(&blocked).unwrap();
    b.set_path_for_test(blocked.clone());
    assert!(b.save().is_err(), "writing over a directory must fail");

    // The real file never got touched, and the buffer still knows it is dirty.
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
    assert!(b.is_dirty(), "a failed save must not clear the dirty flag");
}

#[test]
fn an_edit_group_is_a_single_undo_step() {
    let mut b = TextBuffer::from_str("abc\n");
    b.begin_edit_group(EditKind::Other, &origin());
    b.insert_char(Position { line: 0, col: 0 }, 'x');
    b.insert_char(Position { line: 0, col: 1 }, 'y');
    b.end_edit_group();
    assert_eq!(b.line_text(0), "xyabc");
    b.undo(&origin());
    assert_eq!(b.line_text(0), "abc", "one undo takes back both edits");
}

#[test]
fn edits_outside_a_group_are_separate_undo_steps() {
    let mut b = TextBuffer::from_str("abc\n");
    b.insert_char(Position { line: 0, col: 0 }, 'x');
    b.insert_char(Position { line: 0, col: 1 }, 'y');
    b.undo(&origin());
    assert_eq!(b.line_text(0), "xabc");
}

#[test]
fn a_new_buffer_at_a_path_starts_empty_and_clean() {
    let dir = std::env::temp_dir().join("typ-buffer-new-at");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("does-not-exist-yet.rs");
    let _ = std::fs::remove_file(&path);

    let buffer = TextBuffer::new_at(&path);

    assert_eq!(buffer.line_count(), 1, "an empty buffer is one empty line");
    assert_eq!(buffer.line_text(0), "");
    assert_eq!(buffer.path(), Some(path.as_path()));
    assert!(
        !buffer.is_dirty(),
        "nothing has been typed, so quitting must not challenge the user"
    );
    assert!(!path.exists(), "opening must not create the file");
}

#[test]
fn saving_a_new_buffer_creates_the_file() {
    let dir = std::env::temp_dir().join("typ-buffer-new-at-save");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("created-on-save.rs");
    let _ = std::fs::remove_file(&path);

    let mut buffer = TextBuffer::new_at(&path);
    buffer.insert_char(Position { line: 0, col: 0 }, 'x');
    buffer.save().expect("save creates the file");

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "x");
}
