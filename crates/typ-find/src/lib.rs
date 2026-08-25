//! Finding things in a project: which files exist, which of them a query means,
//! and which lines contain a string.
//!
//! The bottom of the dependency graph, beside `typ-syntax` and for the same
//! reason: `typ_core::AppEvent` names this crate's result type, so this crate
//! must not depend on `typ-core` — not even in dev-dependencies, where the
//! cycle surfaces as a publish-order failure rather than a build failure.

mod rank;
mod search;
mod walk;
mod worker;

pub use rank::{FileHit, rank};
pub use search::{LineHit, Search, search};
pub use walk::walk;
pub use worker::{FindWorker, Found};
