//! What counts as a line, and why it has to be what everything else counts.
//!
//! A buffer's idea of a line is not private. A language server reports a
//! diagnostic on line 42, ripgrep reports a match on line 42, git reports a
//! hunk at line 42 — and every one of those tools splits on `\n` and nothing
//! else. A rope configured to also break on `U+000C` renders a file with a form
//! feed in it one line out of step with all three, from that character down.
//!
//! ropey's `unicode_lines` feature, on by default, does exactly that:
//! `U+000B`, `U+000C`, `U+0085`, `U+2028` and `U+2029` all become line breaks,
//! bringing it into conformance with Unicode Annex #14 — which is right for
//! text layout and wrong for a code editor. Helix disables it for the same
//! reason.

use typ_buffer::TextBuffer;

/// Lines the way every other tool in the pipeline counts them.
fn lf_lines(text: &str) -> usize {
    text.split('\n').count()
}

fn check(name: &str, text: &str) {
    let buffer = TextBuffer::from_str(text);
    assert_eq!(
        buffer.rope().len_lines(),
        lf_lines(text),
        "{name}: the buffer disagrees with a plain LF split"
    );
}

#[test]
fn a_form_feed_is_not_a_line_break() {
    // `\x0c` is legal in Rust source and appears in real code as a page
    // separator. rust-analyzer's line index counts `b'\n'` and nothing else.
    check("form feed", "fn a() {}\n\u{000C}\nfn b() {}\n");
}

#[test]
fn a_vertical_tab_is_not_a_line_break() {
    check("vertical tab", "a\u{000B}b\n");
}

#[test]
fn next_line_is_not_a_line_break() {
    check("NEL", "a\u{0085}b\n");
}

#[test]
fn a_unicode_line_separator_is_not_a_line_break() {
    // Inside a string literal this is ordinary content, and a server counting
    // LF will not have moved to the next line.
    check("U+2028", "let s = \"a\u{2028}b\";\n");
}

#[test]
fn a_paragraph_separator_is_not_a_line_break() {
    check("U+2029", "let s = \"a\u{2029}b\";\n");
}

#[test]
fn a_line_feed_still_is_one() {
    check("lf", "one\ntwo\nthree\n");
}

#[test]
fn crlf_is_still_one_break_rather_than_two() {
    // Loaded files are normalised to LF, but a rope built from raw text has to
    // agree anyway — CRLF is one break, never two.
    let buffer = TextBuffer::from_str("one\r\ntwo\r\n");
    assert_eq!(buffer.rope().len_lines(), 3);
}
