//! One parse in flight, no timer.
//!
//! Zed debounces reparses at 200 ms. That is a fixed cost on the common case —
//! type one character, stop — paid to solve a problem this shape does not have:
//! the worker takes one job at a time, so a burst of edits collapses into
//! however many parses fit in the time the burst took. Self-tuning to the
//! machine and the file, and no latency floor.
//!
//! Helix's alternative is a 500 ms timeout on the *main* thread. Invariant 7
//! rules that out before it is a trade-off.

use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread;

use ropey::Rope;

use crate::{Language, Syntax};

/// A completed parse, on its way back to whoever asked.
///
/// `typ-syntax`'s own type rather than `typ_core::AppEvent`: sending the app's
/// event type from here would put `typ-core` in this crate's dev-dependencies
/// while `typ-core` already depends on this crate, and a dev-dependency cycle
/// is a publish-order failure waiting for release day. `typ-core` writes one
/// `From` impl instead and this crate depends on nothing of TYPE's.
pub struct Parsed {
    /// Which request this answers. The panel keeps the highest it has seen.
    pub generation: u64,
    pub syntax: Arc<Syntax>,
}

/// By hand because `Syntax` implements neither, and `AppEvent` — which gains a
/// variant holding this — derives both. Two results are the same result when
/// they answer the same request with the same allocation.
impl PartialEq for Parsed {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && Arc::ptr_eq(&self.syntax, &other.syntax)
    }
}

impl Eq for Parsed {}

impl std::fmt::Debug for Parsed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parsed")
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl Clone for Parsed {
    fn clone(&self) -> Self {
        Self {
            generation: self.generation,
            syntax: Arc::clone(&self.syntax),
        }
    }
}

/// A parse request that has not started yet. At most one is ever pending.
struct Pending {
    language: Language,
    rope: Rope,
    generation: u64,
}

/// A thread that parses snapshots and sends the results back.
///
/// Dropping it stops the thread.
pub struct ParseWorker {
    /// `None` once the thread has been told to stop.
    jobs: Option<Sender<Pending>>,
    generation: u64,
}

impl ParseWorker {
    /// Start the thread.
    ///
    /// Generic over the message so the caller's event type is the caller's
    /// business: `typ-app` passes a `Sender<AppEvent>`, a test passes a
    /// `Sender<Parsed>` through the reflexive `From<T> for T`.
    pub fn spawn<E>(results: Sender<E>) -> ParseWorker
    where
        E: From<Parsed> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<Pending>();

        thread::Builder::new()
            .name("typ-parse".into())
            .spawn(move || {
                // `recv` fails when the last `ParseWorker` holding the sender
                // is dropped, which is how the thread learns to exit.
                while let Ok(job) = rx.recv() {
                    // Coalesce: anything already queued behind this job
                    // describes newer text, so parsing this one would be work
                    // whose result is stale before it is sent. Same shape as
                    // `step_batch`'s scroll folding.
                    let job = std::iter::successors(Some(job), |_| rx.try_recv().ok())
                        .last()
                        .expect("the iterator starts from a value");

                    let Ok(syntax) = Syntax::parse(job.language, &job.rope) else {
                        // A file that will not parse renders as plain text.
                        // The reason is in the `ParseError` the caller could
                        // have inspected; here there is nobody to tell, and a
                        // missing highlight is the documented floor.
                        continue;
                    };

                    let parsed = Parsed {
                        generation: job.generation,
                        syntax: Arc::new(syntax),
                    };
                    if results.send(E::from(parsed)).is_err() {
                        // The app is gone. So is the reason to keep parsing.
                        break;
                    }
                }
            })
            .expect("the OS can start a thread");

        ParseWorker {
            jobs: Some(tx),
            generation: 0,
        }
    }

    /// Ask for a parse. Never blocks.
    ///
    /// The rope is a snapshot: ropey's nodes are reference-counted and shared,
    /// so the clone is cheap and the worker reads a consistent tree while the
    /// user keeps typing into the original.
    pub fn request(&mut self, language: Language, rope: Rope) {
        let Some(jobs) = &self.jobs else {
            return;
        };

        self.generation += 1;
        let pending = Pending {
            language,
            rope,
            generation: self.generation,
        };

        if jobs.send(pending).is_err() {
            // The thread is gone; stop pretending otherwise.
            self.jobs = None;
        }
    }

    /// The generation the most recent [`request`](Self::request) was given.
    pub fn generation(&self) -> u64 {
        self.generation
    }
}
