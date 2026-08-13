use std::time::Instant;

use m0_feel::highlight::Highlighter;

#[test]
fn keywords_are_highlighted_on_their_line() {
    let src = "fn main() {}\nlet x = 1;\n";
    let mut h = Highlighter::new_rust().expect("rust grammar loads");
    h.parse(src);
    let spans = h.spans_for_line(src, 0);
    assert!(!spans.is_empty(), "expected highlight spans on line 0");
}

#[test]
fn parsing_a_large_buffer_completes() {
    let src = "fn f() { let x = 1; }\n".repeat(20_000);
    let mut h = Highlighter::new_rust().expect("rust grammar loads");
    h.parse(&src);
    assert!(!h.spans_for_line(&src, 0).is_empty());
}

#[test]
fn viewport_spans_deep_in_a_large_buffer_stay_cheap() {
    // The bug this guards: walking from the root and rescanning the text to
    // find a line start made highlight cost scale with how far you had
    // scrolled. A whole viewport near the end of a 40k-line file must cost
    // far less than one 16ms frame budget.
    let line = "fn f() { let x = 1; }\n";
    let src = line.repeat(40_000);
    let mut h = Highlighter::new_rust().expect("rust grammar loads");
    h.parse(&src);

    let start = line.len() * 39_000;
    let end = start + line.len() * 40;

    let t0 = Instant::now();
    let spans = h.spans_in_range(start, end);
    let elapsed = t0.elapsed();

    assert!(!spans.is_empty(), "expected spans in the visible range");
    assert!(
        elapsed.as_micros() < 16_000,
        "viewport highlight took {elapsed:?}, budget is 16ms/frame"
    );
}
