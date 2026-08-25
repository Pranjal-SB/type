use std::fs;
use std::path::{Path, PathBuf};

use typ_find::{LineHit, Search, search};

struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("typ-search-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("fixture root");
        Fixture(dir)
    }

    fn file(&self, rel: &str, contents: &str) -> &Self {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().expect("has a parent")).expect("fixture dirs");
        fs::write(path, contents).expect("fixture file");
        self
    }

    fn bytes(&self, rel: &str, contents: &[u8]) -> &Self {
        fs::write(self.0.join(rel), contents).expect("fixture file");
        self
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(root: &Path, query: &str) -> Search {
    search(root, query, 100, &[])
}

fn at<'a>(hits: &'a [LineHit], path: &str) -> Vec<&'a LineHit> {
    hits.iter().filter(|hit| hit.path == path).collect()
}

#[test]
fn a_literal_query_finds_its_lines() {
    let fixture = Fixture::new("literal");
    fixture
        .file("a.rs", "fn one() {}\nfn needle() {}\nfn three() {}\n")
        .file("b.rs", "nothing here\n");

    let found = run(fixture.path(), "needle");

    assert_eq!(found.hits.len(), 1, "got {:?}", found.hits);
    assert_eq!(found.hits[0].path, "a.rs");
    assert_eq!(found.hits[0].text.trim(), "fn needle() {}");
}

#[test]
fn line_numbers_are_zero_based() {
    // grep reports 1-based; `PanelEvent::OpenFile.line` is 0-based. An
    // off-by-one here puts every result one line from where it said it was, on
    // every search, forever.
    let fixture = Fixture::new("lines");
    fixture.file("a.rs", "first\nsecond\nthird\n");

    let found = run(fixture.path(), "first");
    assert_eq!(found.hits[0].line, 0, "line 1 should report as 0");

    let found = run(fixture.path(), "third");
    assert_eq!(found.hits[0].line, 2);
}

#[test]
fn col_is_a_grapheme_index() {
    // Invariant 4. grep reports a byte offset into the line; a line with a
    // multi-byte character before the match has more bytes than graphemes, and
    // handing the byte straight through puts the cursor past the match.
    let fixture = Fixture::new("col");
    // "e" + combining acute, then "xy": the match at "xy" is grapheme 1.
    fixture.file("a.rs", "e\u{301}xy\n");

    let found = run(fixture.path(), "xy");

    assert_eq!(found.hits.len(), 1, "got {:?}", found.hits);
    assert_eq!(
        found.hits[0].col, 1,
        "byte offset would say 3, char offset 2"
    );
}

#[test]
fn a_binary_file_is_skipped() {
    // The main reason this is grep-searcher and not `find_all`. A picker
    // offering forty matches inside a .png is worse than one offering none.
    let fixture = Fixture::new("binary");
    fixture.file("a.rs", "needle\n");
    fixture.bytes("blob.bin", b"needle\x00needle\x00needle");

    let found = run(fixture.path(), "needle");

    assert!(
        at(&found.hits, "blob.bin").is_empty(),
        "got {:?}",
        found.hits
    );
    assert_eq!(at(&found.hits, "a.rs").len(), 1);
}

#[test]
fn gitignored_files_are_skipped() {
    let fixture = Fixture::new("ignored");
    fixture
        .file(".gitignore", "target/\n")
        .file("a.rs", "needle\n")
        .file("target/generated.rs", "needle\n");

    let found = run(fixture.path(), "needle");

    assert_eq!(found.hits.len(), 1, "got {:?}", found.hits);
    assert_eq!(found.hits[0].path, "a.rs");
}

#[test]
fn matching_is_smart_case() {
    // The same rule `SearchQuery` and `rank` use. Three components of "find"
    // disagreeing about what a capital means would be worse than any of them
    // choosing wrong.
    let fixture = Fixture::new("case");
    fixture.file("a.rs", "Needle\nneedle\n");

    assert_eq!(run(fixture.path(), "needle").hits.len(), 2);
    assert_eq!(run(fixture.path(), "Needle").hits.len(), 1);
}

#[test]
fn the_cap_truncates_and_says_so() {
    // An unbounded result list is an unbounded allocation on a worker, driven
    // by whatever the user typed.
    let fixture = Fixture::new("cap");
    fixture.file("a.rs", &"needle\n".repeat(50));

    let found = search(fixture.path(), "needle", 10, &[]);

    assert_eq!(found.hits.len(), 10);
    assert!(!found.complete, "a truncated search claimed to be complete");
}

#[test]
fn an_uncapped_search_reports_complete() {
    let fixture = Fixture::new("complete");
    fixture.file("a.rs", "needle\n");

    let found = search(fixture.path(), "needle", 10, &[]);

    assert_eq!(found.hits.len(), 1);
    assert!(found.complete);
}

#[test]
fn a_query_matching_nothing_is_empty_and_complete() {
    let fixture = Fixture::new("nothing");
    fixture.file("a.rs", "hay\n");

    let found = run(fixture.path(), "needle");

    assert!(found.hits.is_empty());
    assert!(found.complete);
}

#[test]
fn an_invalid_regex_is_empty_rather_than_a_panic() {
    // The query is whatever the user has typed *so far*, so half-written
    // patterns arrive on every keystroke. `[` is not a pattern yet.
    let fixture = Fixture::new("badregex");
    fixture.file("a.rs", "needle\n");

    let found = run(fixture.path(), "[");

    assert!(found.hits.is_empty());
}

#[test]
fn an_empty_query_matches_nothing_rather_than_every_line() {
    // An empty regex matches at every position, which for a project search is
    // every line of every file — the exact shape of an accidental fork bomb.
    let fixture = Fixture::new("emptyquery");
    fixture.file("a.rs", "one\ntwo\n");

    let found = run(fixture.path(), "");

    assert!(found.hits.is_empty(), "got {:?}", found.hits);
}

#[test]
fn results_are_ordered_by_path_then_line() {
    // A parallel walk finds files in whichever order the workers got there, so
    // without an explicit sort the same search lists its results differently
    // between runs.
    let fixture = Fixture::new("order");
    fixture
        .file("b.rs", "needle\nneedle\n")
        .file("a.rs", "x\nneedle\n");

    let found = run(fixture.path(), "needle");
    let seen: Vec<(&str, usize)> = found
        .hits
        .iter()
        .map(|hit| (hit.path.as_str(), hit.line))
        .collect();

    assert_eq!(seen, vec![("a.rs", 1), ("b.rs", 0), ("b.rs", 1)]);
}

#[test]
fn an_open_buffer_is_searched_instead_of_the_file_on_disk() {
    // **Unsaved edits.** A project search that reports what is on disk while
    // the user is looking at something else is answering a question nobody
    // asked. Helix does the same and calls it a speed optimisation; the reason
    // that matters is correctness.
    let fixture = Fixture::new("overlay");
    fixture.file("a.rs", "saved\n");

    let overrides = [(fixture.path().join("a.rs"), "unsaved\n".to_string())];

    let found = search(fixture.path(), "unsaved", 100, &overrides);
    assert_eq!(found.hits.len(), 1, "the buffer was not searched");
    assert_eq!(found.hits[0].text.trim(), "unsaved");

    let found = search(fixture.path(), "saved", 100, &overrides);
    assert_eq!(
        found.hits.len(),
        1,
        "the on-disk text was searched as well as the buffer"
    );
}

#[test]
fn paths_are_relative_with_forward_slashes() {
    let fixture = Fixture::new("relpaths");
    fixture.file("deep/nested/a.rs", "needle\n");

    let found = run(fixture.path(), "needle");

    assert_eq!(found.hits[0].path, "deep/nested/a.rs");
}

#[test]
fn a_very_long_line_is_truncated_rather_than_shipped_whole() {
    // A minified bundle is one line of two megabytes. Sending it through the
    // channel to render eighty columns of it is the picker allocating a
    // megabyte per row.
    let fixture = Fixture::new("longline");
    fixture.file("min.js", &format!("{}needle\n", "x".repeat(10_000)));

    let found = run(fixture.path(), "needle");

    assert_eq!(found.hits.len(), 1);
    assert!(
        found.hits[0].text.len() < 1_000,
        "line was {} bytes",
        found.hits[0].text.len()
    );
}
