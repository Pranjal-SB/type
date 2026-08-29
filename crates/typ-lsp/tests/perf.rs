//! The LSP client's CPU budgets. See `typ-buffer/tests/perf.rs` for why these
//! are `#[ignore]`d.
//!
//!     cargo test --release -p typ-lsp --test perf -- --ignored --nocapture
//!
//! **CPU only.** Anything that spawns a process is in `perf_proc.rs` and runs
//! in its own binary: M2.8 measured ranking at 3.7 ms alone and 11.9 ms after a
//! sibling test in the same binary had touched ten thousand files. The mutex
//! serialises execution; it does not reset the page cache or the allocator.

use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use ropey::Rope;
use typ_lsp::{Encoding, from_lsp, to_lsp};

/// Perf tests run one at a time. Same reasoning as every other `perf.rs` here:
/// a wall-clock reading taken while a sibling saturates another core measures
/// the scheduler.
static EXCLUSIVE: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    EXCLUSIVE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A representative 50k-line file, with enough non-ASCII to make UTF-16 do
/// real work rather than the arithmetic it does on plain text.
fn big_rope() -> Rope {
    let line = "    let editor = Editor::new(); // représentative ligne de code\n";
    let text: String = std::iter::repeat_n(line, 50_000).collect();
    Rope::from_str(&text)
}

/// Best of five. Noise on a wall clock is additive, so the fastest run is the
/// one least contaminated by things that are not the code.
fn best_of_five(mut run: impl FnMut() -> u128) -> u128 {
    (0..5).map(|_| run()).min().unwrap_or(u128::MAX)
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn a_position_round_trip_is_cheap_in_every_encoding() {
    let _guard = exclusive();
    let rope = big_rope();
    let slice = rope.slice(..);
    // Deep into the file: the whole point of ropey's surrogate index is that
    // this is not a walk from the start.
    let target = rope.line_to_char(45_000) + 20;

    for encoding in [Encoding::Utf32, Encoding::Utf8, Encoding::Utf16] {
        let nanos = best_of_five(|| {
            let start = Instant::now();
            for _ in 0..1_000 {
                let position = to_lsp(encoding, slice, target);
                std::hint::black_box(from_lsp(encoding, slice, position));
            }
            start.elapsed().as_nanos() / 1_000
        });
        println!("{encoding:?} round trip: {nanos} ns per call");
        assert!(
            nanos < 10_000,
            "{encoding:?} round trip took {nanos} ns against a 10 µs budget"
        );
    }
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn a_full_document_notification_costs_the_render_thread_almost_nothing() {
    // **The measurement this milestone's design rests on.** Full sync sends the
    // whole document on every change; the prediction in `lsp.md` §8 was under
    // 1 ms for `to_string` plus serialisation together, and it missed — 1.3 ms
    // and 2.0 ms on a typical file. The design survived because of *where* the
    // cost falls: `did_change` hands the writer thread a closure over a cloned
    // rope, so the render thread pays for the clone and nothing else.
    let _guard = exclusive();
    let rope = big_rope();

    let nanos = best_of_five(|| {
        let start = Instant::now();
        for _ in 0..100 {
            // What the render thread actually does per frame.
            std::hint::black_box(rope.clone());
        }
        start.elapsed().as_nanos() / 100
    });
    println!("render thread per didChange: {nanos} ns");
    assert!(
        nanos < 10_000,
        "a rope snapshot took {nanos} ns; it is supposed to be an atomic bump"
    );

    // And what the writer thread pays, off the hot path. Reported, not gated:
    // it is nobody's latency.
    let millis = best_of_five(|| {
        let start = Instant::now();
        let text = rope.to_string();
        let params = serde_json::json!({
            "textDocument": { "uri": "file:///x.rs", "version": 1 },
            "contentChanges": [ { "text": text } ],
        });
        std::hint::black_box(serde_json::to_vec(&params).unwrap());
        start.elapsed().as_micros()
    });
    println!("writer thread per didChange: {millis} µs (reported, not gated)");
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn four_hundred_diagnostics_convert_in_under_a_millisecond() {
    // A file with four hundred problems is a file mid-refactor, and every one
    // of them is converted from the server's encoding on arrival.
    let _guard = exclusive();
    let rope = big_rope();
    let slice = rope.slice(..);

    let positions: Vec<lsp_types::Position> = (0..400)
        .map(|i| lsp_types::Position {
            line: (i * 100) as u32,
            character: 12,
        })
        .collect();

    let micros = best_of_five(|| {
        let start = Instant::now();
        for position in &positions {
            std::hint::black_box(from_lsp(Encoding::Utf16, slice, *position));
        }
        start.elapsed().as_micros()
    });
    println!("400 diagnostics converted: {micros} µs");
    assert!(
        micros < 1_000,
        "converting 400 diagnostics took {micros} µs"
    );
}
