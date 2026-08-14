use typ_buffer::{Position, SearchQuery, TextBuffer};

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

fn query(needle: &str, case_sensitive: bool) -> SearchQuery {
    SearchQuery {
        needle: needle.to_string(),
        case_sensitive,
    }
}

#[test]
fn find_all_returns_every_match_in_document_order() {
    let b = TextBuffer::from_str("one two one\nrepeat one\n");
    let hits = b.find_all(&query("one", true));
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].range(), (pos(0, 0), pos(0, 3)));
    assert_eq!(hits[1].range(), (pos(0, 8), pos(0, 11)));
    assert_eq!(hits[2].range(), (pos(1, 7), pos(1, 10)));
}

#[test]
fn a_case_insensitive_search_matches_regardless_of_case() {
    let b = TextBuffer::from_str("Rust rust RUST\n");
    assert_eq!(b.find_all(&query("rust", false)).len(), 3);
    assert_eq!(b.find_all(&query("rust", true)).len(), 1);
}

#[test]
fn an_empty_needle_matches_nothing() {
    let b = TextBuffer::from_str("anything\n");
    assert!(b.find_all(&query("", true)).is_empty());
}

#[test]
fn matches_are_measured_in_graphemes_not_bytes() {
    let b = TextBuffer::from_str("日本語 ok\n");
    let hits = b.find_all(&query("ok", true));
    assert_eq!(hits[0].range(), (pos(0, 4), pos(0, 6)));
}

#[test]
fn a_match_made_of_wide_characters_reports_grapheme_bounds() {
    let b = TextBuffer::from_str("aa日本語bb\n");
    let hits = b.find_all(&query("日本", true));
    assert_eq!(hits[0].range(), (pos(0, 2), pos(0, 4)));
}

#[test]
fn repeated_text_yields_non_overlapping_matches() {
    let b = TextBuffer::from_str("aaaa\n");
    let hits = b.find_all(&query("aa", true));
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].range(), (pos(0, 0), pos(0, 2)));
    assert_eq!(hits[1].range(), (pos(0, 2), pos(0, 4)));
}

#[test]
fn a_match_at_the_end_of_a_line_is_still_found() {
    let b = TextBuffer::from_str("prefix end\n");
    let hits = b.find_all(&query("end", true));
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].range(), (pos(0, 7), pos(0, 10)));
}

#[test]
fn a_match_never_spans_a_line_break() {
    let b = TextBuffer::from_str("ab\ncd\n");
    assert!(b.find_all(&query("bc", true)).is_empty());
}

#[test]
fn replace_range_swaps_the_text_and_marks_the_buffer_dirty() {
    let mut b = TextBuffer::from_str("hello world\n");
    b.replace_range(pos(0, 6), pos(0, 11), "there");
    assert_eq!(b.line_text(0), "hello there");
    assert!(b.is_dirty());
}

#[test]
fn replace_range_is_undoable_as_one_step() {
    let mut b = TextBuffer::from_str("hello world\n");
    b.replace_range(pos(0, 6), pos(0, 11), "there");
    b.undo();
    assert_eq!(b.line_text(0), "hello world");
}

#[test]
fn replace_range_handles_a_replacement_of_a_different_length() {
    let mut b = TextBuffer::from_str("a-b\n");
    b.replace_range(pos(0, 1), pos(0, 2), "===");
    assert_eq!(b.line_text(0), "a===b");
}

#[test]
fn replace_range_with_an_empty_replacement_deletes() {
    let mut b = TextBuffer::from_str("abcdef\n");
    b.replace_range(pos(0, 1), pos(0, 4), "");
    assert_eq!(b.line_text(0), "aef");
}

#[test]
fn replace_range_with_an_empty_range_inserts() {
    let mut b = TextBuffer::from_str("ac\n");
    b.replace_range(pos(0, 1), pos(0, 1), "b");
    assert_eq!(b.line_text(0), "abc");
}

#[test]
fn replace_range_with_an_empty_range_and_no_text_does_nothing() {
    let mut b = TextBuffer::from_str("ab\n");
    b.replace_range(pos(0, 1), pos(0, 1), "");
    assert_eq!(b.line_text(0), "ab");
    assert!(!b.is_dirty(), "a no-op must not dirty the buffer");
}
