use typ_buffer::{Position, TextBuffer, brackets};

fn at(line: usize, col: usize) -> Position {
    Position { line, col }
}

/// A generous bound for the small fixtures here. The editor passes its viewport
/// height plus a margin; see `gives_up_rather_than_scanning_the_whole_file`.
const BOUND: usize = 64;

#[test]
fn an_opening_bracket_matches_the_closing_one_after_it() {
    let buffer = TextBuffer::from_str("foo(bar)\n");
    assert_eq!(
        brackets::match_at(&buffer, at(0, 3), BOUND),
        Some((at(0, 3), at(0, 7)))
    );
}

#[test]
fn a_closing_bracket_matches_the_opening_one_before_it() {
    let buffer = TextBuffer::from_str("foo(bar)\n");
    assert_eq!(
        brackets::match_at(&buffer, at(0, 7), BOUND),
        Some((at(0, 3), at(0, 7)))
    );
}

#[test]
fn a_cursor_just_past_a_bracket_still_matches_it() {
    // Typing `)` leaves the caret after it, which is the moment the match is
    // most useful. Every editor in the field probes both sides for this reason.
    let buffer = TextBuffer::from_str("foo(bar)\n");
    assert_eq!(
        brackets::match_at(&buffer, at(0, 8), BOUND),
        Some((at(0, 3), at(0, 7)))
    );
}

#[test]
fn the_bracket_under_the_cursor_wins_over_the_one_before_it() {
    // `)(` with the caret on the `(`: both sides hold a bracket, and the one
    // the cursor is actually on is the one the user is asking about.
    let buffer = TextBuffer::from_str("a)(b)\n");
    assert_eq!(
        brackets::match_at(&buffer, at(0, 2), BOUND),
        Some((at(0, 2), at(0, 4)))
    );
}

#[test]
fn nesting_counts_so_the_outer_bracket_finds_the_outer_partner() {
    let buffer = TextBuffer::from_str("((a))\n");
    assert_eq!(
        brackets::match_at(&buffer, at(0, 0), BOUND),
        Some((at(0, 0), at(0, 4))),
        "the outer ( must not match the first ) it meets"
    );
    assert_eq!(
        brackets::match_at(&buffer, at(0, 1), BOUND),
        Some((at(0, 1), at(0, 3)))
    );
}

#[test]
fn a_pair_can_span_lines() {
    let buffer = TextBuffer::from_str("fn main() {\n    body\n}\n");
    assert_eq!(
        brackets::match_at(&buffer, at(0, 10), BOUND),
        Some((at(0, 10), at(2, 0)))
    );
}

#[test]
fn scanning_backwards_also_spans_lines() {
    let buffer = TextBuffer::from_str("fn main() {\n    body\n}\n");
    assert_eq!(
        brackets::match_at(&buffer, at(2, 0), BOUND),
        Some((at(0, 10), at(2, 0)))
    );
}

#[test]
fn all_three_pairs_are_matched() {
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}')] {
        let buffer = TextBuffer::from_str(&format!("x{open}y{close}z\n"));
        assert_eq!(
            brackets::match_at(&buffer, at(0, 1), BOUND),
            Some((at(0, 1), at(0, 3))),
            "{open}{close} did not match"
        );
    }
}

#[test]
fn brackets_of_different_kinds_do_not_match_each_other() {
    let buffer = TextBuffer::from_str("(a]\n");
    assert_eq!(brackets::match_at(&buffer, at(0, 0), BOUND), None);
}

#[test]
fn an_unbalanced_bracket_reports_nothing_rather_than_guessing() {
    let buffer = TextBuffer::from_str("foo(bar\n");
    assert_eq!(brackets::match_at(&buffer, at(0, 3), BOUND), None);
}

#[test]
fn a_position_with_no_bracket_near_it_reports_nothing() {
    let buffer = TextBuffer::from_str("plain text\n");
    assert_eq!(brackets::match_at(&buffer, at(0, 4), BOUND), None);
}

#[test]
fn gives_up_rather_than_scanning_the_whole_file() {
    // The partner exists, 500 lines away, and the bound is 4. Finding it would
    // mean a keystroke that walks the buffer — architecture §4 forbids exactly
    // that, and an un-highlighted bracket is a far smaller cost than a dropped
    // frame.
    let mut text = String::from("(\n");
    text.push_str(&"filler\n".repeat(500));
    text.push_str(")\n");
    let buffer = TextBuffer::from_str(&text);
    assert_eq!(brackets::match_at(&buffer, at(0, 0), 4), None);
    // Same buffer, a bound that reaches: the limit is the only thing stopping it.
    assert_eq!(
        brackets::match_at(&buffer, at(0, 0), 600),
        Some((at(0, 0), at(501, 0)))
    );
}

#[test]
fn columns_are_grapheme_indices_even_with_wide_characters() {
    // `col` is a grapheme index everywhere in TYPE, and a bracket match that
    // reported byte offsets would put the highlight on the wrong cell the
    // moment a line contains anything outside ASCII.
    let buffer = TextBuffer::from_str("日本語(x)\n");
    assert_eq!(
        brackets::match_at(&buffer, at(0, 3), BOUND),
        Some((at(0, 3), at(0, 5)))
    );
}

#[test]
fn a_bracket_at_the_very_start_of_the_buffer_scans_backwards_safely() {
    let buffer = TextBuffer::from_str(")\n");
    assert_eq!(brackets::match_at(&buffer, at(0, 0), BOUND), None);
}
