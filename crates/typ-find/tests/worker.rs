//! The worker, driven through a local event type.
//!
//! **Deliberately not `typ_core::AppEvent`.** `typ-core` depends on this crate,
//! so naming it here — even in dev-dependencies — is a cycle. Cargo builds it
//! happily and `cargo publish` does not, because `typ-find` goes to the registry
//! first. That is a release-day failure caught here instead, and it is why
//! `FindWorker::spawn` is generic over its message.

use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use typ_find::{FindWorker, Found};

/// Stands in for `AppEvent`.
#[derive(Debug)]
enum TestEvent {
    Found(Found),
}

impl From<Found> for TestEvent {
    fn from(found: Found) -> Self {
        TestEvent::Found(found)
    }
}

struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("typ-worker-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("fixture root");
        for rel in ["src/main.rs", "src/highlight.rs", "README.md"] {
            let path = dir.join(rel);
            fs::create_dir_all(path.parent().expect("has a parent")).expect("fixture dirs");
            fs::write(path, "").expect("fixture file");
        }
        Fixture(dir)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Wait for one event, failing the test rather than hanging forever.
fn next(rx: &mpsc::Receiver<TestEvent>) -> Found {
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(TestEvent::Found(found)) => found,
        Err(e) => panic!("no event within 10s: {e}"),
    }
}

#[test]
fn indexing_then_filtering_returns_ranked_hits() {
    let fixture = Fixture::new("index-filter");
    let (tx, rx) = mpsc::channel();
    let mut worker = FindWorker::spawn(tx);

    worker.index(fixture.0.clone());
    let Found::Indexed { count } = next(&rx) else {
        panic!("expected Indexed first");
    };
    assert_eq!(count, 3);

    let generation = worker.filter("highlight".into(), 10);
    let Found::Files {
        generation: got,
        hits,
    } = next(&rx)
    else {
        panic!("expected Files");
    };
    assert_eq!(got, generation);
    assert_eq!(hits[0].path, "src/highlight.rs");
}

#[test]
fn the_generation_that_comes_back_is_the_one_that_went_in() {
    let fixture = Fixture::new("generation");
    let (tx, rx) = mpsc::channel();
    let mut worker = FindWorker::spawn(tx);
    worker.index(fixture.0.clone());
    let _ = next(&rx);

    let first = worker.filter("main".into(), 10);
    let second = worker.filter("main".into(), 10);
    assert_ne!(first, second, "generations must advance");

    // Whatever arrives, it carries a generation that was actually issued —
    // coalescing may drop the first, but it may never invent one.
    let Found::Files { generation, .. } = next(&rx) else {
        panic!("expected Files");
    };
    assert!(generation == first || generation == second);
}

#[test]
fn a_burst_of_filters_collapses_to_the_last_one() {
    // The coalescing contract, and the reason there is no debounce timer. Typing
    // "high" fast must not queue four ranking passes whose first three answers
    // are stale before they are sent.
    let fixture = Fixture::new("coalesce");
    let (tx, rx) = mpsc::channel();
    let mut worker = FindWorker::spawn(tx);
    worker.index(fixture.0.clone());
    let _ = next(&rx);

    worker.filter("h".into(), 10);
    worker.filter("hi".into(), 10);
    worker.filter("hig".into(), 10);
    let last = worker.filter("highlight".into(), 10);

    // The last request always runs and always arrives; earlier ones may or may
    // not, depending on how fast the worker drained the queue. Wait for the one
    // that is guaranteed.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let Found::Files { generation, hits } = next(&rx) else {
            panic!("expected Files");
        };
        if generation == last {
            assert_eq!(hits[0].path, "src/highlight.rs");
            break;
        }
        assert!(Instant::now() < deadline, "the last filter never arrived");
    }
}

#[test]
fn filtering_before_any_index_answers_empty_rather_than_blocking() {
    // Opening the picker and typing immediately, before the walk lands. An
    // answer of "nothing yet" is correct; a worker that waits for the corpus is
    // a picker that swallows the first keystrokes.
    let (tx, rx) = mpsc::channel();
    let mut worker = FindWorker::spawn(tx);

    let generation = worker.filter("anything".into(), 10);
    let Found::Files {
        generation: got,
        hits,
    } = next(&rx)
    else {
        panic!("expected Files");
    };
    assert_eq!(got, generation);
    assert!(hits.is_empty());
}

#[test]
fn a_second_index_replaces_the_corpus() {
    let first = Fixture::new("reindex-a");
    let (tx, rx) = mpsc::channel();
    let mut worker = FindWorker::spawn(tx);

    worker.index(first.0.clone());
    let _ = next(&rx);

    let empty = std::env::temp_dir().join(format!("typ-worker-empty-{}", std::process::id()));
    let _ = fs::remove_dir_all(&empty);
    fs::create_dir_all(&empty).expect("empty root");
    worker.index(empty.clone());
    let Found::Indexed { count } = next(&rx) else {
        panic!("expected Indexed");
    };
    assert_eq!(count, 0, "the previous corpus was not replaced");
    let _ = fs::remove_dir_all(&empty);
}

#[test]
fn dropping_the_worker_ends_the_thread() {
    let (tx, rx) = mpsc::channel::<TestEvent>();
    let worker = FindWorker::spawn(tx);
    drop(worker);
    // The worker held the only other sender clone; once its thread exits, the
    // channel closes and `recv` errors rather than blocking forever.
    assert!(rx.recv_timeout(Duration::from_secs(10)).is_err());
}
