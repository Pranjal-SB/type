//! Not a behaviour test. Prints the numbers Task 1 exists to produce.
//! Deleted at Task 2; the numbers live in the plan's "Actual:" line.

use std::time::Instant;

#[test]
#[ignore = "spike: run by hand with --nocapture"]
fn parse_a_large_file() {
    // 50k lines of plausible Rust, the same shape tests/perf.rs uses.
    let source = "fn main() {\n    let x = 1;\n}\n".repeat(16_667);
    let rope = ropey::Rope::from_str(&source);

    // query compilation, isolated: it sits on the startup path, ahead of the
    // first frame, and folding it into the parse number below would misreport
    // a one-time cost as a per-parse one.
    let start = Instant::now();
    let _ = typ_syntax::parse_rust(&ropey::Rope::from_str("fn a() {}\n"));
    println!("query compile (first parse_rust call): {:?}", start.elapsed());

    let start = Instant::now();
    let syntax = typ_syntax::parse_rust(&rope).expect("parses");
    println!(
        "cold parse (query already warm), {} lines: {:?}",
        rope.len_lines(),
        start.elapsed()
    );

    let start = Instant::now();
    let spans = typ_syntax::highlights(&syntax, &rope, 0..2_000);
    println!("highlight 60 lines: {:?}, {} spans", start.elapsed(), spans.len());
}
