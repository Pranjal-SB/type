use typ_buffer::{Position, TextBuffer};

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
fn delete_before_removes_the_preceding_grapheme() {
    let mut b = TextBuffer::from_str("abc\n");
    b.delete_before(Position { line: 0, col: 2 });
    assert_eq!(b.line_text(0), "ac");
}

#[test]
fn delete_before_at_start_of_buffer_is_a_noop() {
    let mut b = TextBuffer::from_str("abc\n");
    b.delete_before(Position { line: 0, col: 0 });
    assert_eq!(b.line_text(0), "abc");
}

#[test]
fn delete_before_wide_char_removes_whole_grapheme() {
    let mut b = TextBuffer::from_str("日本語\n");
    b.delete_before(Position { line: 0, col: 1 });
    assert_eq!(b.line_text(0), "本語");
}

#[test]
fn delete_after_removes_the_grapheme_under_the_cursor() {
    let mut b = TextBuffer::from_str("abc\n");
    b.delete_after(Position { line: 0, col: 1 });
    assert_eq!(b.line_text(0), "ac");
}

#[test]
fn delete_after_at_end_of_line_joins_the_next_line() {
    let mut b = TextBuffer::from_str("ab\ncd\n");
    b.delete_after(Position { line: 0, col: 2 });
    assert_eq!(b.line_text(0), "abcd");
}

#[test]
fn delete_after_at_end_of_buffer_is_a_noop() {
    let mut b = TextBuffer::from_str("ab");
    b.delete_after(Position { line: 0, col: 2 });
    assert_eq!(b.line_text(0), "ab");
}

#[test]
fn delete_after_removes_a_whole_wide_grapheme() {
    let mut b = TextBuffer::from_str("日本語\n");
    b.delete_after(Position { line: 0, col: 0 });
    assert_eq!(b.line_text(0), "本語");
}

#[test]
fn undo_restores_the_previous_content() {
    let mut b = TextBuffer::from_str("a\n");
    b.insert_char(Position { line: 0, col: 1 }, 'b');
    assert_eq!(b.line_text(0), "ab");
    b.undo();
    assert_eq!(b.line_text(0), "a");
}

#[test]
fn redo_reapplies_an_undone_edit() {
    let mut b = TextBuffer::from_str("a\n");
    b.insert_char(Position { line: 0, col: 1 }, 'b');
    b.undo();
    b.redo();
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
