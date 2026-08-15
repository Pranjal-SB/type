use typ_buffer::{next_word_boundary, previous_word_boundary, word_at};

#[test]
fn next_boundary_stops_at_the_end_of_a_word() {
    assert_eq!(next_word_boundary("hello world", 0), 5);
}

#[test]
fn next_boundary_skips_leading_whitespace_then_the_word() {
    assert_eq!(next_word_boundary("hello world", 5), 11);
}

#[test]
fn punctuation_is_its_own_run() {
    // Moving off "foo" lands between word and punctuation, not past both.
    assert_eq!(next_word_boundary("foo::bar", 0), 3);
    assert_eq!(next_word_boundary("foo::bar", 3), 5);
    assert_eq!(next_word_boundary("foo::bar", 5), 8);
}

#[test]
fn next_boundary_clamps_at_the_end_of_the_line() {
    assert_eq!(next_word_boundary("abc", 3), 3);
    assert_eq!(next_word_boundary("", 0), 0);
}

#[test]
fn previous_boundary_stops_at_the_start_of_a_word() {
    assert_eq!(previous_word_boundary("hello world", 11), 6);
    assert_eq!(previous_word_boundary("hello world", 6), 0);
}

#[test]
fn previous_boundary_clamps_at_the_start_of_the_line() {
    assert_eq!(previous_word_boundary("abc", 0), 0);
}

#[test]
fn boundaries_count_graphemes_not_bytes() {
    // Graphemes: 日(0) 本(1) 語(2) space(3) o(4) k(5) — nine bytes in the CJK
    // run alone, so anything counting bytes lands somewhere else entirely.
    assert_eq!(next_word_boundary("日本語 ok", 0), 3);
    // From the end of "ok", back to its start.
    assert_eq!(previous_word_boundary("日本語 ok", 6), 4);
    // From the start of "ok", back past the space to the start of the CJK run.
    assert_eq!(previous_word_boundary("日本語 ok", 4), 0);
}

#[test]
fn word_at_returns_the_run_under_the_cursor() {
    assert_eq!(word_at("let value = 1;", 4), Some((4, 9)));
}

#[test]
fn word_at_returns_nothing_in_whitespace() {
    assert_eq!(word_at("a  b", 1), None);
}

#[test]
fn a_cursor_just_past_a_word_is_still_on_it() {
    assert_eq!(word_at("abc", 3), Some((0, 3)));
}

#[test]
fn word_at_on_an_empty_line_finds_nothing() {
    assert_eq!(word_at("", 0), None);
}
