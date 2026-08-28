//! Error and warning counts in the status bar.
//!
//! The undercurl says which word and the gutter sign says which line; neither
//! says whether the file is clean. A count does, and it is the one piece of
//! diagnostic state worth knowing without looking at any particular line.

use typ_app::status::{Emphasis, SegmentId, StatusFacts, segments};

fn facts(errors: usize, warnings: usize) -> StatusFacts<'static> {
    StatusFacts {
        progress: &[],
        file_name: "main.rs",
        modified: false,
        file_type: Some("rs"),
        line_ending: "LF",
        indent_width: 4,
        selection_count: 1,
        line: 0,
        col: 0,
        total_lines: 10,
        errors,
        warnings,
    }
}

fn diagnostics_segment(errors: usize, warnings: usize) -> Option<String> {
    segments(&facts(errors, warnings))
        .into_iter()
        .find(|segment| segment.id == SegmentId::Diagnostics)
        .map(|segment| segment.text)
}

#[test]
fn a_clean_file_says_nothing() {
    // A segment that reads "0 errors" spends a cell to tell you what silence
    // already said. Every other segment here is omitted when it has no answer;
    // this one is omitted when the answer is "nothing is wrong".
    assert_eq!(diagnostics_segment(0, 0), None);
}

#[test]
fn errors_and_warnings_are_counted_separately() {
    assert_eq!(diagnostics_segment(2, 3).as_deref(), Some("2E 3W"));
}

#[test]
fn a_kind_with_no_instances_is_left_out_of_the_text() {
    assert_eq!(diagnostics_segment(1, 0).as_deref(), Some("1E"));
    assert_eq!(diagnostics_segment(0, 4).as_deref(), Some("4W"));
}

#[test]
fn an_error_is_worth_noticing_and_a_warning_alone_is_not() {
    // The bar's three levels are a ranking, not a palette. An error is the one
    // diagnostic state that changes what a user does next.
    let with_error = segments(&facts(1, 0))
        .into_iter()
        .find(|s| s.id == SegmentId::Diagnostics)
        .unwrap();
    assert_eq!(with_error.emphasis, Emphasis::Accent);

    let warnings_only = segments(&facts(0, 2))
        .into_iter()
        .find(|s| s.id == SegmentId::Diagnostics)
        .unwrap();
    assert_eq!(warnings_only.emphasis, Emphasis::Quiet);
}

#[test]
fn the_count_sits_before_the_position() {
    // Read right to left, the bar goes from "where am I" outwards. The counts
    // belong with the file's state, not with the cursor's.
    let ids: Vec<SegmentId> = segments(&facts(1, 1)).into_iter().map(|s| s.id).collect();
    let diagnostics = ids.iter().position(|id| *id == SegmentId::Diagnostics);
    let position = ids.iter().position(|id| *id == SegmentId::Position);
    assert!(diagnostics < position, "{ids:?}");
}
