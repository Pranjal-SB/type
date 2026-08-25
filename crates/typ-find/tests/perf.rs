//! Permanent budgets for ranking. **CPU only — no filesystem.**
//!
//! `#[ignore]`d, so an ordinary `cargo test` skips them:
//!
//! ```text
//! cargo test --release -p typ-find --test perf -- --ignored --nocapture
//! ```
//!
//! **The walk and the search live in `perf_fs.rs`, in a separate binary, and
//! that split is load-bearing.** With all five in one binary the mutex
//! serialises them correctly and the numbers are still wrong: ranking 50k paths
//! measured 3.68 ms alone and 11.9 ms after a sibling had created and deleted
//! ten thousand files. Creating and destroying a tree leaves the page cache and
//! the allocator in a state that outlives the lock, so exclusivity is necessary
//! and not sufficient. Cargo runs test binaries one at a time in fresh
//! processes, which is what makes the separation work.
//!
//! Debug timings are meaningless here — `nucleo-matcher` and `ignore` are both
//! heavily inlined, and an unoptimised build measures the inlining that did not
//! happen.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use typ_find::rank;

/// Perf tests run one at a time.
///
/// cargo runs tests in parallel threads inside one process, and a wall-clock
/// measurement taken while a sibling saturates another core is not a
/// measurement of anything. M2.7 spent a bisect against a previous tag proving
/// a 20x was a phantom caused by exactly this omission — and it matters more
/// here than anywhere else in the workspace, because `walk` and `search` are
/// *themselves* parallel and will contend with any sibling for every core.
static EXCLUSIVE: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    EXCLUSIVE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The keystroke budget. Architecture §4.
const BUDGET_US: u128 = 16_000;

/// Best of five.
///
/// Noise on a wall clock is additive, so the fastest run is the one least
/// contaminated by things that are not the code. `find_all` taught this by
/// giving 6.9, 9.0, 14.9 and 18.7 ms on consecutive runs against a 16 ms gate.
fn best_of_five(mut f: impl FnMut()) -> Duration {
    (0..5)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed()
        })
        .min()
        .expect("five samples")
}

/// A corpus shaped like a real repository rather than `file0..fileN`.
///
/// A flat list of identical stems matches far too uniformly and flatters the
/// prefilter, which is the difference between measuring the matcher and
/// measuring a best case nobody has.
fn corpus(n: usize) -> Vec<String> {
    let dirs = [
        "crates/typ-core/src",
        "crates/typ-buffer/src",
        "crates/typ-app/src/config/themes",
        "docs/design",
        "target/debug/build/serde-1a2b3c/out",
        "node_modules/@scope/pkg/dist/esm",
        "src/components/hero",
        "tests/fixtures/golden",
    ];
    let stems = [
        "buffer",
        "render",
        "highlight",
        "selection",
        "keymap",
        "theme",
        "index",
        "mod",
        "utils",
        "handler",
        "component",
        "config",
    ];
    let exts = ["rs", "toml", "md", "json", "ts", "tsx", "yaml"];
    let mut all: Vec<String> = (0..n)
        .map(|i| {
            format!(
                "{}/{}_{}.{}",
                dirs[i % dirs.len()],
                stems[(i / 3) % stems.len()],
                i,
                exts[(i / 7) % exts.len()]
            )
        })
        .collect();
    // `walk` returns a sorted corpus and `rank`'s tie-break assumes one.
    all.sort();
    all
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn ranking_fifty_thousand_paths_fits_in_a_keystroke() {
    let _guard = exclusive();
    let candidates = corpus(50_000);

    // A one-character needle is the worst case: the prefilter rejects almost
    // nothing, so nearly every candidate is scored in full.
    let worst = best_of_five(|| {
        std::hint::black_box(rank("r", &candidates, 100));
    });
    let typical = best_of_five(|| {
        std::hint::black_box(rank("cthigh", &candidates, 100));
    });

    println!("rank 50k, 1 char:    {worst:?} best of 5");
    println!("rank 50k, 6 chars:   {typical:?} best of 5");

    assert!(
        worst.as_micros() < BUDGET_US,
        "ranking a 50k corpus took {worst:?}, over the {BUDGET_US} µs keystroke budget"
    );
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn an_empty_query_does_not_rank_the_whole_corpus() {
    // The opening screen. It must cost the visible page, not the project: an
    // empty query has nothing to rank, so anything above microseconds means it
    // is scoring 50,000 candidates against nothing.
    let _guard = exclusive();
    let candidates = corpus(50_000);

    let elapsed = best_of_five(|| {
        std::hint::black_box(rank("", &candidates, 100));
    });
    println!("rank 50k, empty:     {elapsed:?} best of 5");

    assert!(
        elapsed.as_micros() < 1_000,
        "an empty query took {elapsed:?}; it should be a clone of 100 strings"
    );
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn ranking_stays_proportional_to_the_query_not_the_corpus() {
    // A longer needle prefilters harder, so it must not cost more than a short
    // one. The failure this guards is a ranking pass that walks every candidate
    // in full regardless of how much the query rules out.
    let _guard = exclusive();
    let small = corpus(10_000);
    let large = corpus(50_000);

    let a = best_of_five(|| {
        std::hint::black_box(rank("cthigh", &small, 100));
    });
    let b = best_of_five(|| {
        std::hint::black_box(rank("cthigh", &large, 100));
    });
    println!("rank 10k, 6 chars:   {a:?} best of 5");
    println!("rank 50k, 6 chars:   {b:?} best of 5");

    // Five times the corpus for well under five times the work would be a
    // pleasant surprise; more than eight times is a superlinear pass.
    assert!(
        b.as_nanos() < a.as_nanos().saturating_mul(8),
        "5x the corpus cost {:.1}x the time",
        b.as_nanos() as f64 / a.as_nanos().max(1) as f64
    );
}
