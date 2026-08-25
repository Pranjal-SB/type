//! The thread that owns the corpus.
//!
//! **The corpus lives here, not in the panel.** A 37,000-entry `Vec<String>` in
//! the picker would mean the panel re-ranks on the render thread and holds an
//! allocation proportional to the repository. Here, the panel holds only the
//! visible page and the render thread's work is proportional to the screen —
//! which is what invariant 7 is for.
//!
//! Coalescing rather than debouncing, the shape `ParseWorker` proved at M2.7: a
//! burst of keystrokes collapses into however many rankings fit in the time the
//! burst took, which self-tunes to the machine and adds no latency floor to the
//! common case of typing one character and stopping.

use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::thread;

use crate::{FileHit, rank, walk};

/// A result on its way back to whoever asked.
///
/// `typ-find`'s own type rather than `typ_core::AppEvent`, for the reason
/// `Parsed` is: `typ-core` depends on this crate, so naming its event type here
/// — even in dev-dependencies — is a cycle that builds locally and fails at
/// `cargo publish`, because this crate goes to the registry first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// A walk finished and the corpus is now this many candidates.
    ///
    /// Carries the count rather than the paths: the app wants to know the index
    /// is ready, and shipping 37,000 strings through the channel to be counted
    /// and dropped is work with no reader.
    Indexed { count: usize },
    /// Ranked candidates for one query.
    Files { generation: u64, hits: Vec<FileHit> },
}

/// A unit of work the thread has not started yet.
enum Job {
    Index {
        root: PathBuf,
    },
    Filter {
        generation: u64,
        query: String,
        limit: usize,
    },
}

/// A thread that walks a project and ranks queries against it.
///
/// Dropping it stops the thread.
pub struct FindWorker {
    /// `None` once the thread is gone.
    jobs: Option<Sender<Job>>,
    generation: u64,
}

impl FindWorker {
    /// Start the thread.
    ///
    /// Generic over the message for the same reason `ParseWorker::spawn` is:
    /// `typ-app` passes a `Sender<AppEvent>`, a test passes a `Sender<Found>`
    /// through the reflexive `From<T> for T`, and this crate names neither.
    pub fn spawn<E>(results: Sender<E>) -> FindWorker
    where
        E: From<Found> + Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<Job>();

        thread::Builder::new()
            .name("typ-find".into())
            .spawn(move || {
                let mut corpus: Vec<String> = Vec::new();

                // `recv` fails when the last `FindWorker` is dropped, which is
                // how the thread learns to exit.
                while let Ok(job) = rx.recv() {
                    // Drain everything queued behind this one and keep the last
                    // of each kind. An `Index` is not interchangeable with a
                    // `Filter`, so the two coalesce separately: a burst of
                    // keystrokes must not swallow the walk that was queued
                    // before it, and a re-index must not be answered with the
                    // previous corpus.
                    let mut index: Option<PathBuf> = None;
                    let mut filter: Option<(u64, String, usize)> = None;
                    for job in std::iter::once(job).chain(std::iter::from_fn(|| rx.try_recv().ok()))
                    {
                        match job {
                            Job::Index { root } => index = Some(root),
                            Job::Filter {
                                generation,
                                query,
                                limit,
                            } => filter = Some((generation, query, limit)),
                        }
                    }

                    // Index first: a filter queued behind a walk means the user
                    // typed while it ran, and answering from the old corpus
                    // would be answering a question about the new project with
                    // the previous one's files.
                    if let Some(root) = index {
                        corpus = walk(&root);
                        if results
                            .send(E::from(Found::Indexed {
                                count: corpus.len(),
                            }))
                            .is_err()
                        {
                            break;
                        }
                    }

                    if let Some((generation, query, limit)) = filter {
                        let hits = rank(&query, &corpus, limit);
                        if results
                            .send(E::from(Found::Files { generation, hits }))
                            .is_err()
                        {
                            // The app is gone. So is the reason to keep ranking.
                            break;
                        }
                    }
                }
            })
            .expect("the OS can start a thread");

        FindWorker {
            jobs: Some(tx),
            generation: 0,
        }
    }

    /// Walk `root` and make it the corpus. Never blocks.
    pub fn index(&mut self, root: PathBuf) {
        self.send(Job::Index { root });
    }

    /// Ask for the `limit` best matches. Never blocks.
    ///
    /// Returns the generation this request was given, so the caller can discard
    /// everything that is not the answer to it. A filter before any index is
    /// answered with an empty list rather than queued — the picker must not
    /// swallow the keystrokes typed while the walk is still running.
    pub fn filter(&mut self, query: String, limit: usize) -> u64 {
        self.generation += 1;
        let generation = self.generation;
        self.send(Job::Filter {
            generation,
            query,
            limit,
        });
        generation
    }

    /// The generation the most recent [`filter`](Self::filter) was given.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    fn send(&mut self, job: Job) {
        let Some(jobs) = &self.jobs else {
            return;
        };
        if jobs.send(job).is_err() {
            // The thread is gone; stop pretending otherwise.
            self.jobs = None;
        }
    }
}
