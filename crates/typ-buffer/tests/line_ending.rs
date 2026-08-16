//! Detection only.
//!
//! `m2.1-correctness.md` deferred *preserving* line endings to M2.5 — writing
//! back what was found rather than always writing `\n`. Displaying `LF` or
//! `CRLF` in the status bar needs only the detection half, which is independent
//! and ten lines, so it lands here with a test M2.5 can build the preservation
//! half against.

use typ_buffer::{LineEnding, TextBuffer};

#[test]
fn unix_endings_are_detected() {
    assert_eq!(TextBuffer::from_str("a\nb\n").line_ending(), LineEnding::Lf);
}

#[test]
fn windows_endings_are_detected() {
    assert_eq!(
        TextBuffer::from_str("a\r\nb\r\n").line_ending(),
        LineEnding::Crlf
    );
}

#[test]
fn the_first_ending_in_the_file_decides() {
    // A mixed file is not a third kind of file. Whatever the first line did is
    // what the file "is", and it is what M2.5 will write back — the alternative
    // is a save that silently normalises someone's whole file because the
    // majority went the other way.
    assert_eq!(
        TextBuffer::from_str("a\r\nb\nc\n").line_ending(),
        LineEnding::Crlf
    );
    assert_eq!(
        TextBuffer::from_str("a\nb\r\nc\n").line_ending(),
        LineEnding::Lf
    );
}

#[test]
fn a_file_with_no_newline_at_all_defaults_to_lf() {
    assert_eq!(
        TextBuffer::from_str("single line").line_ending(),
        LineEnding::Lf
    );
}

#[test]
fn an_empty_buffer_defaults_to_lf() {
    assert_eq!(TextBuffer::from_str("").line_ending(), LineEnding::Lf);
}

#[test]
fn a_lone_carriage_return_is_not_a_line_ending() {
    // Classic Mac endings died with Mac OS 9, and a stray `\r` inside a line is
    // far likelier than a file that uses them. Treating it as an ending would
    // mean a save that rewrites every line break in the file.
    assert_eq!(
        TextBuffer::from_str("a\rb\nc\n").line_ending(),
        LineEnding::Lf
    );
}

#[test]
fn the_label_is_what_a_status_bar_should_show() {
    assert_eq!(LineEnding::Lf.label(), "LF");
    assert_eq!(LineEnding::Crlf.label(), "CRLF");
}

#[test]
fn a_new_file_that_does_not_exist_yet_defaults_to_lf() {
    let dir = std::env::temp_dir().join("typ-line-ending-new");
    std::fs::create_dir_all(&dir).unwrap();
    let buffer = TextBuffer::new_at(&dir.join("unwritten.txt"));
    assert_eq!(buffer.line_ending(), LineEnding::Lf);
}
