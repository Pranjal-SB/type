//! A Language Server Protocol client.
//!
//! Bottom of the dependency graph beside `typ-syntax` and `typ-find`, and for
//! the same reason: nothing of TYPE's appears in either dependency table, so
//! there is no cycle to surface as a publish-order failure on release day.
//!
//! **This crate speaks char offsets and never mentions graphemes.** A char is
//! ropey's native unit and the pivot for all three LSP position encodings;
//! `col` is a grapheme index everywhere above here, and `typ-buffer` owns the
//! conversion because that is where grapheme logic already lives.
//!
//! The research this is built on, including why these dependencies and not the
//! obvious ones, is in `docs/design/lsp.md`.

#[doc(hidden)]
pub mod fake;

/// The protocol's own types, so a consumer can deserialise a payload without
/// naming the dependency itself.
///
/// `gen-lsp-types` under its rust-analyzer alias — the version lives in one
/// manifest and a future swap is one line, which is exactly why the alias
/// exists. `typ-app` deserialises `PublishDiagnosticsParams` through this
/// rather than reading fields out of a `serde_json::Value` by hand, because a
/// field name typed wrong in a hand-parse is a diagnostic that silently never
/// appears.
pub use lsp_types;

pub mod client;
pub mod position;
pub mod transport;
pub mod uri;

pub use client::{Client, LspEvent, SyncKind};
pub use position::{Encoding, from_lsp, to_lsp};
pub use transport::{Incoming, ServerId, SpawnError, Transport};
pub use uri::{path_to_uri, uri_to_path};
