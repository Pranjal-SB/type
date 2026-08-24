//! Time to first highlight — the cost that binary size was standing in for.
//!
//! **This has to be its own test binary.** Every grammar and its queries load
//! together behind a process-wide `OnceLock` on the first parse, so any test
//! that ran earlier in the same binary has already paid the cost and this one
//! measures a warm cache. Written first inside `tests/perf.rs`, it reported
//! 60 µs for work that takes three orders of magnitude longer — the same
//! "measuring the path that early-returns" mistake this file exists to avoid.
//! Cargo runs test binaries one at a time in separate processes, so a separate
//! file is a fresh `OnceLock`.
//!
//!     cargo test --release -p typ-panel-editor --test perf_startup -- --ignored --nocapture
//!
//! There is no assertion and no budget yet. This never touches cold start —
//! the load happens on the parse worker, and the first frame paints
//! unhighlighted regardless — so what it bounds is how long a freshly opened
//! file waits before it colours. Architecture §4 has no line for that; this is
//! the number a line would be drawn from.

#[cfg(all(target_env = "musl", target_pointer_width = "64"))]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::time::Instant;

#[test]
#[ignore = "wall-clock; run with --release --ignored --nocapture"]
fn loading_every_grammar_is_recorded() {
    // One sample, and it cannot be otherwise: a `OnceLock` has no reset, so
    // the first call is the only chance to measure the load. Best-of-five is
    // impossible here rather than skipped.
    let rope = ropey::Rope::from_str("fn a() {}\n");

    let start = Instant::now();
    let _ = typ_syntax::Syntax::parse(typ_syntax::Language::Rust, &rope);
    let cold = start.elapsed();

    let start = Instant::now();
    let _ = typ_syntax::Syntax::parse(typ_syntax::Language::Rust, &rope);
    let warm = start.elapsed();

    println!("first parse — loads six grammars and compiles their queries: {cold:?}");
    println!("second parse — everything already loaded: {warm:?}");
    println!("attributable to loading: {:?}", cold.saturating_sub(warm));
}
