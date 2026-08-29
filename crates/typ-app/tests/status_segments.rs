use std::path::Path;

use typ_app::status::{Emphasis, SegmentId, StatusFacts, file_type_of, segments};

fn facts() -> StatusFacts<'static> {
    StatusFacts {
        progress: &[],
        errors: 0,
        warnings: 0,
        file_name: "main.rs",
        modified: false,
        file_type: Some("rs"),
        line_ending: "LF",
        indent_width: 4,
        selection_count: 1,
        line: 0,
        col: 0,
        total_lines: 100,
    }
}

fn text_of(id: SegmentId, facts: &StatusFacts) -> Option<String> {
    segments(facts)
        .into_iter()
        .find(|s| s.id == id)
        .map(|s| s.text)
}

#[test]
fn the_file_name_comes_first() {
    let built = segments(&facts());
    assert_eq!(built[0].id, SegmentId::FileName);
    assert_eq!(built[0].text, "main.rs");
}

#[test]
fn an_unsaved_file_is_marked_and_accented() {
    let mut facts = facts();
    facts.modified = true;
    let built = segments(&facts);
    assert_eq!(built[0].text, "main.rs *");
    assert_eq!(
        built[0].emphasis,
        Emphasis::Accent,
        "unsaved work is the one thing here a user must notice without looking"
    );
}

#[test]
fn position_counts_from_one() {
    let mut facts = facts();
    facts.line = 11;
    facts.col = 4;
    assert_eq!(text_of(SegmentId::Position, &facts).unwrap(), "12:5");
}

#[test]
fn the_line_ending_is_shown_rather_than_assumed() {
    let mut facts = facts();
    facts.line_ending = "CRLF";
    assert_eq!(text_of(SegmentId::LineEnding, &facts).unwrap(), "CRLF");
}

#[test]
fn the_indent_is_stated_even_though_it_is_hardcoded() {
    // Saying "Spaces: 4" out loud is the first step to it being configurable.
    assert_eq!(text_of(SegmentId::Indent, &facts()).unwrap(), "Spaces: 4");
}

#[test]
fn one_cursor_says_nothing_about_cursors() {
    assert_eq!(text_of(SegmentId::Selections, &facts()), None);
}

#[test]
fn several_cursors_are_counted_and_accented() {
    let mut facts = facts();
    facts.selection_count = 30;
    let built = segments(&facts);
    let seg = built
        .iter()
        .find(|s| s.id == SegmentId::Selections)
        .expect("a cursor count");
    assert_eq!(seg.text, "30 cursors");
    assert_eq!(seg.emphasis, Emphasis::Accent);
}

#[test]
fn a_file_with_no_extension_omits_the_filetype_rather_than_faking_one() {
    let mut facts = facts();
    facts.file_type = None;
    assert_eq!(text_of(SegmentId::FileType, &facts), None);
    // And nothing else shifts: omitting is not the same as blanking.
    assert_eq!(text_of(SegmentId::Position, &facts).unwrap(), "1:1");
}

#[test]
fn the_percentage_walks_the_file() {
    let mut facts = facts();
    facts.total_lines = 100;

    facts.line = 0;
    assert_eq!(text_of(SegmentId::Percentage, &facts).unwrap(), "1%");
    facts.line = 49;
    assert_eq!(text_of(SegmentId::Percentage, &facts).unwrap(), "50%");
    facts.line = 99;
    assert_eq!(text_of(SegmentId::Percentage, &facts).unwrap(), "100%");
}

#[test]
fn a_one_line_file_is_entirely_on_screen() {
    let mut facts = facts();
    facts.total_lines = 1;
    facts.line = 0;
    assert_eq!(text_of(SegmentId::Percentage, &facts).unwrap(), "100%");
}

#[test]
fn quiet_segments_are_the_ones_that_only_matter_when_wrong() {
    let built = segments(&facts());
    for id in [
        SegmentId::FileType,
        SegmentId::LineEnding,
        SegmentId::Indent,
        SegmentId::Percentage,
    ] {
        let seg = built.iter().find(|s| s.id == id).expect("segment");
        assert_eq!(seg.emphasis, Emphasis::Quiet, "{id:?} should be quiet");
    }
}

#[test]
fn the_filetype_is_the_extension_not_an_invented_language_name() {
    assert_eq!(
        file_type_of(Some(Path::new("src/main.rs"))).as_deref(),
        Some("rs")
    );
    assert_eq!(
        file_type_of(Some(Path::new("README.MD"))).as_deref(),
        Some("md"),
        "case is normalised so MD and md are one filetype"
    );
    assert_eq!(file_type_of(Some(Path::new("Makefile"))), None);
    assert_eq!(file_type_of(None), None);
}
