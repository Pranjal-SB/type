//! LSP positions to char offsets and back, in all three encodings.
//!
//! `👍🏽` is one grapheme, two chars, four UTF-16 code units and eight bytes.
//! Every disagreement between an editor and a language server about where the
//! cursor is comes from one of those four numbers being used where another was
//! meant.

use ropey::Rope;
use typ_lsp::{Encoding, from_lsp, to_lsp};

const THUMB: &str = "\u{1F44D}\u{1F3FD}";

fn pos(line: u32, character: u32) -> lsp_types::Position {
    lsp_types::Position { line, character }
}

#[test]
fn utf32_is_the_char_index_unchanged() {
    let rope = Rope::from_str(&format!("a{THUMB}b\n"));
    // char 3 is 'b': 'a' is one char, the thumb is two.
    assert_eq!(to_lsp(Encoding::Utf32, rope.slice(..), 3), pos(0, 3));
}

#[test]
fn utf16_counts_surrogate_pairs() {
    let rope = Rope::from_str(&format!("a{THUMB}b\n"));
    assert_eq!(
        to_lsp(Encoding::Utf16, rope.slice(..), 3),
        pos(0, 5),
        "1 for 'a' plus 4 surrogate halves"
    );
}

#[test]
fn utf8_counts_bytes() {
    let rope = Rope::from_str(&format!("a{THUMB}b\n"));
    assert_eq!(
        to_lsp(Encoding::Utf8, rope.slice(..), 3),
        pos(0, 9),
        "1 for 'a' plus 8 bytes"
    );
}

#[test]
fn a_position_on_a_later_line_is_relative_to_that_line() {
    let rope = Rope::from_str("fn a() {}\nfn b() {}\n");
    assert_eq!(to_lsp(Encoding::Utf8, rope.slice(..), 13), pos(1, 3));
}

/// Char indices that sit strictly inside a line break, which is only ever the
/// `\n` of a CRLF pair. No cursor can be there, so no `Position` names one.
fn inside_a_break(rope: &Rope, idx: usize) -> bool {
    idx > 0 && idx < rope.len_chars() && rope.char(idx) == '\n' && rope.char(idx - 1) == '\r'
}

#[test]
fn every_encoding_round_trips_across_a_nasty_corpus() {
    // A ZWJ family, a skin tone, a combining acute, CJK, CRLF and a tab.
    let text = "fn a() {}\r\n\
                let x = \"\u{1F469}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}\";\r\n\
                // e\u{301}cole \u{65E5}\u{672C}\u{8A9E}\ttail\r\n";
    let rope = Rope::from_str(text);
    for enc in [Encoding::Utf8, Encoding::Utf16, Encoding::Utf32] {
        for idx in 0..=rope.len_chars() {
            if inside_a_break(&rope, idx) {
                continue;
            }
            let there = to_lsp(enc, rope.slice(..), idx);
            let back = from_lsp(enc, rope.slice(..), there);
            assert_eq!(back, idx, "{enc:?} lost char {idx} via {there:?}");
        }
    }
}

#[test]
fn a_position_inside_a_crlf_pair_clamps_to_the_start_of_the_break() {
    // The one index the round-trip above skips, asserted directly rather than
    // left as a gap. In "ab\r\n" char 3 is the '\n'; a cursor cannot sit
    // between the two, so the answer is char 2, the end of the line's content.
    let rope = Rope::from_str("ab\r\ncd\r\n");
    assert_eq!(rope.char(3), '\n');
    for enc in [Encoding::Utf8, Encoding::Utf16, Encoding::Utf32] {
        let there = to_lsp(enc, rope.slice(..), 3);
        assert_eq!(from_lsp(enc, rope.slice(..), there), 2, "{enc:?}");
    }
}

#[test]
fn a_character_past_the_line_end_clamps_to_it() {
    // The specification requires this rather than permitting it.
    let rope = Rope::from_str("ab\ncd\n");
    assert_eq!(from_lsp(Encoding::Utf16, rope.slice(..), pos(0, 99)), 2);
}

#[test]
fn a_line_past_the_end_clamps_to_the_last_char() {
    let rope = Rope::from_str("ab\ncd\n");
    let last = rope.len_chars();
    assert_eq!(from_lsp(Encoding::Utf16, rope.slice(..), pos(99, 0)), last);
}

#[test]
fn a_position_inside_a_surrogate_pair_does_not_panic() {
    // A server may hand back character 2 here, which is the middle of the
    // pair. There is no char index for it; not crashing is the requirement,
    // and ropey's own utf16 accessors panic out of bounds.
    let rope = Rope::from_str(&format!("a{THUMB}b\n"));
    let idx = from_lsp(Encoding::Utf16, rope.slice(..), pos(0, 2));
    assert!(idx <= rope.len_chars());
}

#[test]
fn an_empty_rope_answers_rather_than_panicking() {
    let rope = Rope::from_str("");
    assert_eq!(to_lsp(Encoding::Utf8, rope.slice(..), 0), pos(0, 0));
    assert_eq!(from_lsp(Encoding::Utf8, rope.slice(..), pos(0, 0)), 0);
    assert_eq!(from_lsp(Encoding::Utf8, rope.slice(..), pos(9, 9)), 0);
}

#[test]
fn the_end_of_a_line_is_before_its_break_not_after() {
    // "ab\ncd\n": char 2 is the newline. Position (0, 2) must mean "after b",
    // which is char 2 — not char 3, the start of the next line.
    let rope = Rope::from_str("ab\ncd\n");
    assert_eq!(from_lsp(Encoding::Utf8, rope.slice(..), pos(0, 2)), 2);
    assert_eq!(to_lsp(Encoding::Utf8, rope.slice(..), 2), pos(0, 2));
}

#[test]
fn a_crlf_break_is_one_break_not_two() {
    // TYPE preserves line endings, so CRLF buffers are ordinary here.
    let rope = Rope::from_str("ab\r\ncd\r\n");
    assert_eq!(to_lsp(Encoding::Utf8, rope.slice(..), 4), pos(1, 0));
    assert_eq!(from_lsp(Encoding::Utf8, rope.slice(..), pos(1, 0)), 4);
}
