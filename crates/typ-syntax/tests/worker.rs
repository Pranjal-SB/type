use std::sync::mpsc;
use std::time::Duration;

use ropey::Rope;
use typ_syntax::{Language, ParseWorker, Parsed};

fn recv_parsed(rx: &mpsc::Receiver<Parsed>) -> (u64, usize) {
    let parsed = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("a parse arrives");
    (parsed.generation, parsed.syntax.top_level_items())
}

#[test]
fn a_request_comes_back_as_an_event() {
    let (tx, rx) = mpsc::channel();
    let mut worker = ParseWorker::spawn(tx);
    worker.request(Language::Rust, Rope::from_str("fn main() {}\n"));

    let (generation, items) = recv_parsed(&rx);
    assert_eq!(generation, 1);
    assert!(items > 0);
}

#[test]
fn a_burst_of_requests_yields_the_last_one() {
    // The throttle. Ten edits in a row must not cost ten parses, and whatever
    // does come back must describe the newest text — never an earlier snapshot
    // arriving late and painting the file as it used to be.
    let (tx, rx) = mpsc::channel::<Parsed>();
    let mut worker = ParseWorker::spawn(tx);

    for i in 1..=10 {
        worker.request(Language::Rust, Rope::from_str(&"fn a() {}\n".repeat(i)));
    }

    // Dropping the worker closes the job channel, so the thread finishes what
    // it has and exits — which makes draining bounded by the work rather than
    // by a timeout nobody can pick correctly on a loaded CI machine.
    drop(worker);
    let last = rx.into_iter().last().expect("nothing came back");
    assert!(
        last.generation <= 10,
        "generation ran ahead of the requests"
    );

    // The last result must describe the last request. Ten items is the tenth
    // rope; anything fewer means a stale snapshot won the race and the buffer
    // would paint as it used to be.
    assert_eq!(
        last.syntax.top_level_items(),
        last.generation as usize,
        "the newest result does not describe the newest request"
    );
}

#[test]
fn a_burst_costs_fewer_parses_than_it_has_requests() {
    // The other half of coalescing: not merely that the last one wins, but
    // that the ones in between were skipped rather than queued. Without this
    // a worker that parsed all ten in order would still pass the test above.
    let (tx, rx) = mpsc::channel::<Parsed>();
    let mut worker = ParseWorker::spawn(tx);

    // Big enough that a parse outlasts the loop queueing the next request,
    // which is the condition coalescing exists for.
    let big = Rope::from_str(&"fn a() {}\n".repeat(5_000));
    for _ in 0..10 {
        worker.request(Language::Rust, big.clone());
    }

    drop(worker);
    let count = rx.into_iter().count();
    assert!(count > 0, "nothing came back");
    assert!(
        count < 10,
        "no coalescing happened: {count} parses for 10 requests"
    );
}

#[test]
fn dropping_the_worker_stops_the_thread() {
    // A worker parked on a dead channel is a thread leak, and the editor opens
    // a new buffer every time you click a file in the tree.
    let (tx, rx) = mpsc::channel::<Parsed>();
    let worker = ParseWorker::spawn(tx);
    drop(worker);
    // The worker's own sender is gone with it, so the receiver hangs up rather
    // than blocking forever. Disconnected specifically: a timeout would also be
    // an `Err` and would mean the thread is still sitting there.
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(2)),
        Err(mpsc::RecvTimeoutError::Disconnected)
    );
}
