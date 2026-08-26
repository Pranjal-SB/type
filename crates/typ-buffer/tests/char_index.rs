//! Grapheme positions to char offsets and back.
//!
//! The one place the two units meet. `col` is a grapheme index everywhere in
//! TYPE (invariant 4) and `typ-lsp` speaks char offsets, because a char is
//! ropey's native unit and the pivot for every LSP position encoding. The
//! conversion lives here rather than there, because this is where grapheme
//! logic already is.

use typ_buffer::{Position, TextBuffer};

/// One grapheme, two chars, four UTF-16 code units, eight bytes.
const THUMB: &str = "\u{1F44D}\u{1F3FD}";

fn at(line: usize, col: usize) -> Position {
    Position { line, col }
}

#[test]
fn a_position_becomes_a_char_index() {
    let buf = TextBuffer::from_str(&format!("a{THUMB}b\nsecond\n"));
    // col 2 is 'b': 'a' is one grapheme, the thumb is one grapheme and two chars.
    assert_eq!(buf.char_index(at(0, 2)), 3);
}

#[test]
fn a_char_index_becomes_a_position() {
    let buf = TextBuffer::from_str(&format!("a{THUMB}b\nsecond\n"));
    assert_eq!(buf.position(3), at(0, 2));
}

#[test]
fn a_char_index_inside_a_cluster_snaps_to_its_start() {
    // A server may hand back a position that lands between the thumb and its
    // skin-tone modifier. There is no `Position` for that, and `Selections`
    // could not hold one, so it snaps down to the grapheme it is inside.
    let buf = TextBuffer::from_str(&format!("a{THUMB}b\nsecond\n"));
    assert_eq!(buf.position(2), at(0, 1));
}

#[test]
fn a_position_on_a_later_line_counts_from_that_line() {
    let buf = TextBuffer::from_str("ab\ncd\n");
    assert_eq!(buf.char_index(at(1, 1)), 4);
    assert_eq!(buf.position(4), at(1, 1));
}

#[test]
fn a_char_index_past_the_end_clamps() {
    let buf = TextBuffer::from_str("ab\n");
    assert_eq!(buf.position(9_999), buf.position(buf.rope().len_chars()));
}

#[test]
fn a_line_past_the_end_clamps() {
    let buf = TextBuffer::from_str("ab\n");
    let last = buf.rope().len_lines() - 1;
    assert_eq!(buf.char_index(at(9_999, 0)), buf.char_index(at(last, 0)));
}

#[test]
fn the_end_of_a_line_is_before_its_break() {
    // "ab\n": char 2 is the newline, and col 2 means "after b". The two must
    // agree, or a cursor at end of line round-trips onto the next one.
    let buf = TextBuffer::from_str("ab\ncd\n");
    assert_eq!(buf.char_index(at(0, 2)), 2);
    assert_eq!(buf.position(2), at(0, 2));
}

#[test]
fn a_crlf_line_ending_does_not_shift_the_column() {
    // TYPE preserves line endings, so a CRLF buffer is ordinary rather than an
    // edge case, and \r must not be counted as a grapheme on the line.
    let buf = TextBuffer::from_str("ab\r\ncd\r\n");
    assert_eq!(buf.char_index(at(0, 2)), 2);
    assert_eq!(buf.char_index(at(1, 0)), 4);
    assert_eq!(buf.position(4), at(1, 0));
}

#[test]
fn every_position_in_a_nasty_buffer_round_trips() {
    let buf = TextBuffer::from_str(
        "let x = \"\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}\";\n\
         let y = \u{65E5}\u{672C}\u{8A9E};\n\
         // e\u{301}cole\ttail\n",
    );
    for line in 0..buf.rope().len_lines() {
        for col in 0..=buf.line_grapheme_count(line) {
            let pos = at(line, col);
            assert_eq!(buf.position(buf.char_index(pos)), pos, "{pos:?}");
        }
    }
}

#[test]
fn an_empty_buffer_answers_rather_than_panicking() {
    let buf = TextBuffer::from_str("");
    assert_eq!(buf.char_index(at(0, 0)), 0);
    assert_eq!(buf.position(0), at(0, 0));
    assert_eq!(buf.char_index(at(9, 9)), 0);
    assert_eq!(buf.position(9), at(0, 0));
}
