//! Indent-width detection over the shapes real files actually take.

use typ_buffer::detect_indent_width;

fn detect(source: &str) -> Option<usize> {
    detect_indent_width(source.lines())
}

#[test]
fn two_space_file_reads_as_two() {
    assert_eq!(
        detect("fn main() {\n  let a = 1;\n  if a {\n    b();\n  }\n}\n"),
        Some(2)
    );
}

#[test]
fn four_space_file_reads_as_four() {
    assert_eq!(
        detect("fn main() {\n    let a = 1;\n    if a {\n        b();\n    }\n}\n"),
        Some(4)
    );
}

#[test]
fn tab_indented_file_offers_no_space_width() {
    // A tab's display width is a preference, not a property of the file, so
    // there is nothing here to measure and the caller keeps its default.
    assert_eq!(
        detect("fn main() {\n\tlet a = 1;\n\tif a {\n\t\tb();\n\t}\n}\n"),
        None
    );
}

#[test]
fn a_line_mixing_tabs_and_spaces_is_rejected_without_losing_the_file() {
    // Line 5 indents with a tab *and* spaces. It contributes nothing, and the
    // four-space lines around it still carry the answer.
    assert_eq!(
        detect("fn a() {\n    x();\n}\nfn b() {\n\t   y();\n}\n"),
        Some(4)
    );
}

#[test]
fn an_unindented_file_measures_nothing() {
    assert_eq!(detect("one\ntwo\nthree\n"), None);
    assert_eq!(detect(""), None);
}

/// ttt's `DetectIndent` histograms into a Go map and picks the maximum with a
/// strict `>`, so a tie resolves to whichever key iteration reached first —
/// and Go randomises that. Same file, same binary, different answer. Scoring
/// in a fixed preference order is what makes this test hold.
#[test]
fn a_tie_resolves_the_same_way_every_time() {
    // Two 2-deltas against two 4-deltas.
    let two_against_four = "a\n  b\nc\n    d\ne\n";
    assert_eq!(detect(two_against_four), Some(2));
    assert_eq!(detect(two_against_four), Some(2));

    // Two 4-deltas against two 6-deltas: the earlier entry in the preference
    // order wins, so the tie-break is the order and not merely "smallest".
    let four_against_six = "a\n    b\nc\n      d\ne\n";
    assert_eq!(detect(four_against_six), Some(4));
    assert_eq!(detect(four_against_six), Some(4));
}

/// ```text
/// const a = b + c,
///       d = b - c;
/// ```
/// Six spaces, and not a six-wide indent. The previous line ends in a comma
/// and has a space immediately before the column where the two diverge.
#[test]
fn an_aligned_continuation_is_not_an_indent() {
    assert_eq!(detect("const a = b + c,\n      d = b - c;\n"), None);
}

/// When a file changes indent unit part-way the width falls out of the ratio:
/// two tabs where eight spaces stood is four spaces to the tab.
#[test]
fn a_tab_to_space_transition_gives_the_ratio() {
    assert_eq!(detect("start\n\t\ta\n        b\n"), Some(4));
}

/// VS Code's one special case, kept because YAML earns it: deep two-space
/// nesting throws off spurious four-deltas, so 2 takes 4's crown when it has
/// at least two thirds of 4's count.
#[test]
fn two_beats_four_when_it_has_two_thirds_of_the_count() {
    // Two 2-deltas against three 4-deltas: 2 * 3 >= 3 * 2, so 2 wins.
    assert_eq!(detect("a\n  b\nc\n    d\ne\n    f\n"), Some(2));
    // Two 2-deltas against four 4-deltas is not enough.
    assert_eq!(detect("a\n  b\n    c\nd\n    e\nf\n    g\n"), Some(4));
}
