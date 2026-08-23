//! Searching forward from a position, stopping at the first hit.
//!
//! `find_all` scans the whole buffer — ~7 ms on 50k lines, measured. That is
//! fine for answering Enter once and it is not fine for `Ctrl+D` held down,
//! which is one scan per press. Searching from the cursor and stopping at the
//! first match is both the faster thing and the simpler one.

use typ_buffer::{Position, SearchQuery, TextBuffer};

fn at(line: usize, col: usize) -> Position {
    Position { line, col }
}

fn query(needle: &str) -> SearchQuery {
    SearchQuery::new(needle, true)
}

#[test]
fn finds_the_next_match_after_a_position() {
    let buffer = TextBuffer::from_str("foo bar foo\n");
    let hit = buffer.find_next(&query("foo"), at(0, 0)).expect("a match");
    assert_eq!(hit.range(), (at(0, 8), at(0, 11)));
}

#[test]
fn a_match_starting_exactly_at_the_position_is_skipped() {
    // The caller has just selected that one; returning it again would make
    // Ctrl+D a no-op that looks like a freeze.
    let buffer = TextBuffer::from_str("foo foo\n");
    let hit = buffer.find_next(&query("foo"), at(0, 0)).expect("a match");
    assert_eq!(hit.range().0, at(0, 4));
}

#[test]
fn the_search_crosses_lines() {
    let buffer = TextBuffer::from_str("alpha\nbeta\nalpha\n");
    let hit = buffer
        .find_next(&query("alpha"), at(0, 0))
        .expect("a match");
    assert_eq!(hit.range(), (at(2, 0), at(2, 5)));
}

#[test]
fn it_wraps_to_the_top_of_the_buffer() {
    let buffer = TextBuffer::from_str("foo\nbar\n");
    let hit = buffer.find_next(&query("foo"), at(1, 0)).expect("a match");
    assert_eq!(
        hit.range(),
        (at(0, 0), at(0, 3)),
        "past the last match, the next one is the first one"
    );
}

#[test]
fn a_lone_match_is_found_again_by_wrapping() {
    // This is what tells Ctrl+D that every occurrence is already selected: the
    // search comes back round to one the caller already has.
    let buffer = TextBuffer::from_str("only foo here\n");
    let hit = buffer.find_next(&query("foo"), at(0, 5)).expect("a match");
    assert_eq!(hit.range(), (at(0, 5), at(0, 8)));
}

#[test]
fn a_needle_that_is_not_there_reports_nothing() {
    let buffer = TextBuffer::from_str("alpha beta\n");
    assert_eq!(buffer.find_next(&query("gamma"), at(0, 0)), None);
}

#[test]
fn an_empty_needle_matches_nothing() {
    let buffer = TextBuffer::from_str("anything\n");
    assert_eq!(buffer.find_next(&query(""), at(0, 0)), None);
}

#[test]
fn case_sensitivity_belongs_to_the_query() {
    let buffer = TextBuffer::from_str("Foo foo\n");
    let sensitive = buffer.find_next(&SearchQuery::new("foo", true), at(0, 0));
    assert_eq!(sensitive.map(|s| s.range().0), Some(at(0, 4)));

    let insensitive = buffer.find_next(&SearchQuery::new("foo", false), at(0, 0));
    assert_eq!(insensitive.map(|s| s.range().0), Some(at(0, 4)));
}

#[test]
fn columns_are_grapheme_indices() {
    let buffer = TextBuffer::from_str("日本語 ok 日本語 ok\n");
    let hit = buffer.find_next(&query("ok"), at(0, 0)).expect("a match");
    assert_eq!(hit.range(), (at(0, 4), at(0, 6)));
}

#[test]
fn the_head_sits_at_the_end_of_the_match() {
    // Same shape `find_all` returns, so a caret left behind after jumping is
    // where typing would naturally continue.
    let buffer = TextBuffer::from_str("foo bar\n");
    let hit = buffer.find_next(&query("bar"), at(0, 0)).expect("a match");
    assert_eq!(hit.head, at(0, 7));
    assert_eq!(hit.anchor, at(0, 4));
}
