//! Permanent budgets for the walk and the project search. **Filesystem-heavy.**
//!
//! ```text
//! cargo test --release -p typ-find --test perf_fs -- --ignored --nocapture
//! ```
//!
//! A separate binary from `perf.rs` on purpose — see the note there. Building
//! and tearing down a ten-thousand-file tree distorts every CPU measurement
//! that runs after it in the same process, mutex or no mutex.

use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use typ_find::{search, walk};

/// Perf tests run one at a time. See `perf.rs` for why.
static EXCLUSIVE: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    EXCLUSIVE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Best of five. Noise on a wall clock is additive, so the fastest run is the
/// one least contaminated by things that are not the code.
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

/// A tree on disk, removed on drop.
struct Tree(PathBuf);

impl Tree {
    fn new(name: &str, files: usize) -> Self {
        let dir = std::env::temp_dir().join(format!("typ-find-perf-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        for i in 0..files {
            let path = dir.join(format!("d{}", i % 64)).join(format!("f{i}.rs"));
            fs::create_dir_all(path.parent().expect("has a parent")).expect("perf dirs");
            fs::write(
                path,
                "fn a() {}
fn b() {}
// a representative line of code
let needle = 1;
",
            )
            .expect("perf file");
        }
        Tree(dir)
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn walking_ten_thousand_files_stays_under_a_second() {
    // Not a keystroke budget — the walk is on a worker and the picker shows the
    // previous list while it runs. The gate is "fast enough that the list is
    // never visibly stale", and the measurement that matters is the ratio to a
    // serial walk: 2596 ms against 94.7 ms at 37.6k files on this machine, which
    // is what makes `build_parallel` non-negotiable.
    let _guard = exclusive();
    let tree = Tree::new("walk", 10_000);

    let elapsed = best_of_five(|| {
        std::hint::black_box(walk(&tree.0));
    });
    println!("walk 10k files:      {elapsed:?} best of 5");

    assert_eq!(walk(&tree.0).len(), 10_000, "the walk lost files");
    assert!(
        elapsed.as_millis() < 1_000,
        "walking 10k files took {elapsed:?}"
    );
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn searching_ten_thousand_files_stays_under_a_second() {
    let _guard = exclusive();
    let tree = Tree::new("search", 10_000);

    let elapsed = best_of_five(|| {
        std::hint::black_box(search(&tree.0, "representative", 500, &[]));
    });
    println!("search 10k files:    {elapsed:?} best of 5");

    assert!(
        elapsed.as_millis() < 1_000,
        "searching 10k files took {elapsed:?}"
    );
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn a_capped_search_stops_early_rather_than_finishing() {
    // The cap is what bounds a search the user is still typing. If it only
    // truncated the result *after* walking everything, a one-character query on
    // a large project would cost a full scan per keystroke.
    let _guard = exclusive();
    let tree = Tree::new("cap", 10_000);

    let capped = best_of_five(|| {
        std::hint::black_box(search(&tree.0, "needle", 20, &[]));
    });
    let full = best_of_five(|| {
        std::hint::black_box(search(&tree.0, "needle", 100_000, &[]));
    });
    println!("search capped at 20: {capped:?} best of 5");
    println!("search uncapped:     {full:?} best of 5");

    assert!(
        capped < full,
        "the cap saved nothing: {capped:?} capped against {full:?} uncapped"
    );
}
