//! Wall-clock budgets from architecture §4.
//!
//! `#[ignore]`d on purpose: a shared CI runner cannot hold a 16ms number
//! steadily enough to gate a merge on it, and a flaky perf gate gets disabled
//! within a week. These are run by hand:
//!
//!     cargo test --release -p typ-buffer --test perf -- --ignored --nocapture
//!
//! A budget nobody re-measures is a budget nobody has, so the numbers go into
//! the plan's "Actual:" line each time this changes.

use std::time::Instant;

use typ_buffer::{SearchQuery, TextBuffer};

/// Representative of the file this editor will be used on: 50k lines of code.
fn big_buffer() -> TextBuffer {
    let line = "    let editor = Editor::new(); // a representative line of code\n";
    let text: String = std::iter::repeat_n(line, 50_000).collect();
    TextBuffer::from_str(&text)
}

const BUDGET_US: u128 = 16_000;

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn counting_graphemes_on_a_line_does_not_walk_the_buffer() {
    let buffer = big_buffer();

    // Touch a line near the end, where an O(buffer) implementation is most
    // visible, and do it enough times that a per-call allocation shows up.
    let last = buffer.line_count() - 1;
    let start = Instant::now();
    for _ in 0..1_000 {
        std::hint::black_box(buffer.line_grapheme_count(last));
    }
    let per_call = start.elapsed() / 1_000;
    println!("line_grapheme_count: {per_call:?}");
    assert!(
        per_call.as_micros() < 100,
        "one line lookup cost {per_call:?}, which is a whole-buffer walk, not a line"
    );
}

/// The shape a search box actually produces: a needle that most lines do not
/// contain. Every keystroke narrows the result, so this is the case that has to
/// fit in a frame.
#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn searching_a_large_file_for_a_rare_needle_fits_in_a_keystroke() {
    let buffer = big_buffer();
    let query = SearchQuery::new("Editor::from_pieces", true);

    let start = Instant::now();
    let hits = buffer.find_all(&query);
    let elapsed = start.elapsed();
    println!("find_all, rare needle: {elapsed:?} ({} hits)", hits.len());

    assert!(
        elapsed.as_micros() < BUDGET_US,
        "find_all cost {elapsed:?}, over the 16ms keystroke budget"
    );
}

/// The pathological end: a needle on every one of 50k lines. Recorded, not
/// gated.
///
/// No amount of constant-factor work makes a whole-buffer scan that returns 50k
/// matches fit in a frame, and pretending otherwise would push the number down
/// by shrinking the file rather than by fixing anything. What this actually says
/// is that a search box must not run `find_all` on every keystroke — it scans
/// the viewport first and completes the buffer off-thread. That is a design
/// constraint on the search task, recorded here so it is not rediscovered by
/// shipping a laggy search box.
#[test]
#[ignore = "measurement, not a gate; run with --release --ignored"]
fn a_match_on_every_line_is_measured_not_budgeted() {
    let buffer = big_buffer();
    let query = SearchQuery::new("Editor", true);

    let start = Instant::now();
    let hits = buffer.find_all(&query);
    println!(
        "find_all, match on every line: {:?} ({} hits)",
        start.elapsed(),
        hits.len()
    );
}
