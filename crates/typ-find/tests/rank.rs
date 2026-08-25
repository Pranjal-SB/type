use typ_find::{FileHit, rank};

fn paths(hits: &[FileHit]) -> Vec<&str> {
    hits.iter().map(|hit| hit.path.as_str()).collect()
}

const CORPUS: &[&str] = &[
    "crates/typ-core/src/theme.rs",
    "crates/typ-panel-editor/src/highlight.rs",
    "crates/typ-buffer/src/buffer.rs",
    "docs/design/architecture.md",
    "README.md",
    "highlight/notes.txt",
];

fn corpus() -> Vec<String> {
    CORPUS.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_filename_match_outranks_a_directory_match() {
    // The whole reason for `Config::match_paths`. Typing a filename must find
    // the file, not everything living in a directory of that name — without
    // this the picker is a substring search with extra steps.
    let hits = rank("highlight", &corpus(), 10);
    assert_eq!(
        paths(&hits)[0],
        "crates/typ-panel-editor/src/highlight.rs",
        "got {:?}",
        paths(&hits)
    );
}

#[test]
fn an_empty_query_returns_everything_in_corpus_order() {
    // Opening the picker shows the project, not a blank list. Corpus order,
    // because the walk already sorted it and re-ranking nothing by nothing
    // would produce whatever order the matcher's tie-break happens to give.
    let hits = rank("", &corpus(), 10);
    assert_eq!(paths(&hits), CORPUS);
}

#[test]
fn an_empty_query_still_respects_the_limit() {
    let hits = rank("", &corpus(), 3);
    assert_eq!(paths(&hits), &CORPUS[..3]);
}

#[test]
fn a_query_matching_nothing_returns_nothing() {
    // Not "everything, unranked". A picker that falls back to the whole corpus
    // on a typo hides the fact that you typed one.
    let hits = rank("zzqqxx", &corpus(), 10);
    assert!(hits.is_empty(), "got {:?}", paths(&hits));
}

#[test]
fn ranking_is_stable_across_runs() {
    // A picker whose rows reshuffle between identical keystrokes is one you
    // cannot build muscle memory against. Ties break on the path, which is
    // total, rather than on the matcher's internal ordering, which is not.
    let first = rank("rs", &corpus(), 10);
    for _ in 0..5 {
        assert_eq!(paths(&rank("rs", &corpus(), 10)), paths(&first));
    }
}

#[test]
fn the_limit_keeps_the_best_not_the_first() {
    // Truncating before sorting is the bug this guards: it produces a list that
    // is correctly ranked among the wrong candidates.
    let all = rank("highlight", &corpus(), 10);
    let capped = rank("highlight", &corpus(), 1);
    assert_eq!(paths(&capped), vec![paths(&all)[0]]);
}

#[test]
fn indices_are_grapheme_columns_not_char_or_byte_offsets() {
    // Invariant 4, at the boundary this crate owns. The matcher works in chars;
    // the picker paints cells. A path with a combining sequence in it has more
    // chars than graphemes, and styling by char index would colour the wrong
    // cells from the first accent onward.
    //
    // "e\u{301}" is one grapheme, two chars. The `x` after it is grapheme 1.
    let corpus = vec!["e\u{301}x.rs".to_string()];
    let hits = rank("x", &corpus, 10);
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].indices,
        vec![1],
        "expected grapheme 1, got {:?} — char indexing would say 2",
        hits[0].indices
    );
}

#[test]
fn indices_mark_every_matched_grapheme() {
    let corpus = vec!["abc.rs".to_string()];
    let hits = rank("ac", &corpus, 10);
    assert_eq!(hits[0].indices, vec![0, 2]);
}

#[test]
fn an_empty_query_marks_nothing() {
    let hits = rank("", &corpus(), 1);
    assert!(hits[0].indices.is_empty(), "got {:?}", hits[0].indices);
}

#[test]
fn a_zero_limit_returns_nothing_rather_than_everything() {
    assert!(rank("rs", &corpus(), 0).is_empty());
    assert!(rank("", &corpus(), 0).is_empty());
}

#[test]
fn matching_is_smart_case() {
    // Lowercase finds everything; a capital means it. The same rule the buffer
    // search already uses, so the two do not disagree about what a capital is
    // for.
    let corpus = vec!["README.md".to_string(), "readme_draft.md".to_string()];

    let lower = rank("readme", &corpus, 10);
    assert_eq!(
        lower.len(),
        2,
        "lowercase should match both: {:?}",
        paths(&lower)
    );

    let upper = rank("README", &corpus, 10);
    assert_eq!(paths(&upper), vec!["README.md"], "a capital should mean it");
}
