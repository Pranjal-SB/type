//! Which panics give the terminal back. Gap 90.
//!
//! Its own binary with one test: a panic hook is process-wide, so any other
//! test panicking in parallel would run through the one installed here.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn only_a_panic_on_the_loop_thread_restores_the_terminal() {
    let restored = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&restored);
    typ_app::run::install_panic_hook(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });

    // A worker dying must not take raw mode and the alternate screen with it
    // while the loop carries on drawing into a cooked terminal.
    let worker = std::thread::spawn(|| panic!("a parse worker died"));
    assert!(worker.join().is_err());
    assert_eq!(
        restored.load(Ordering::SeqCst),
        0,
        "a worker-thread panic tore down the terminal under a running editor"
    );

    // The thread that installed the hook is the one running the loop, and a
    // panic there ends the editor: that one still has to hand the terminal back.
    let on_loop = std::panic::catch_unwind(|| panic!("the loop died"));
    assert!(on_loop.is_err());
    assert_eq!(restored.load(Ordering::SeqCst), 1);
}
