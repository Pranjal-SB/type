//! Budgets that spawn a process, in a binary of their own.
//!
//!     cargo test --release -p typ-lsp --test perf_proc -- --ignored --nocapture
//!
//! **Separate from `perf.rs` because the mutex is not enough.** M2.8 measured
//! ranking at 3.7 ms alone and 11.9 ms after a sibling in the same binary had
//! created and deleted ten thousand files: the lock serialises execution, but
//! the page cache and the allocator do not reset when it is released. cargo
//! runs test binaries in fresh processes, and that is what fixes it.

use std::path::Path;
use std::sync::mpsc::channel;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use typ_lsp::{Client, Incoming, ServerId};

static EXCLUSIVE: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    EXCLUSIVE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn fake() -> &'static str {
    env!("CARGO_BIN_EXE_typ-lsp-fake-server")
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn a_server_answers_initialize_promptly() {
    // **Reported, not gated.** How fast a server starts is the server's
    // business — rust-analyzer takes tens of seconds to be *useful* and that is
    // fine, because nothing waits for it. The number is here so a regression in
    // TYPE's own framing is visible against a double that does no work.
    let _guard = exclusive();

    let mut best = u128::MAX;
    for _ in 0..5 {
        let (tx, rx) = channel::<Incoming>();
        let start = Instant::now();
        let mut client =
            Client::start(ServerId(0), fake(), &[], Path::new("."), tx).expect("the double starts");
        while !client.is_initialized() {
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(incoming) => {
                    client.handle(incoming);
                }
                Err(_) => panic!("the handshake never completed"),
            }
        }
        best = best.min(start.elapsed().as_micros());
    }
    println!("spawn to initialized: {best} µs (reported, not gated)");
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn spawning_a_server_never_blocks_the_caller() {
    // **The real gate.** `Client::start` returns before the handshake, so a
    // server configured and slow — or configured and absent — cannot come out
    // of the cold-start budget. The whole of architecture §4's 100 ms is the
    // ceiling and this is supposed to be a rounding error inside it.
    let _guard = exclusive();

    let mut best = u128::MAX;
    let mut clients = Vec::new();
    for _ in 0..5 {
        let (tx, rx) = channel::<Incoming>();
        let start = Instant::now();
        // `--sleep` never answers anything. A client that waited would hang
        // here rather than measure.
        let client = Client::start(ServerId(0), fake(), &["--sleep".into()], Path::new("."), tx);
        best = best.min(start.elapsed().as_micros());
        clients.push((client, rx));
    }
    println!("Client::start returns in: {best} µs");
    assert!(
        best < 100_000,
        "starting a server took {best} µs of a 100 ms cold start"
    );
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn a_server_that_is_not_installed_costs_almost_nothing() {
    // The default state on most machines: the binary named in config is not on
    // PATH. The failure has to be cheap, because it is the common path.
    let _guard = exclusive();

    let mut best = u128::MAX;
    for _ in 0..5 {
        let (tx, _rx) = channel::<Incoming>();
        let start = Instant::now();
        let result = Client::start(
            ServerId(0),
            "definitely-not-a-language-server",
            &[],
            Path::new("."),
            tx,
        );
        best = best.min(start.elapsed().as_micros());
        assert!(result.is_err(), "that binary exists?");
    }
    println!("a missing binary refuses in: {best} µs");
    assert!(best < 50_000, "failing to start took {best} µs");
}
