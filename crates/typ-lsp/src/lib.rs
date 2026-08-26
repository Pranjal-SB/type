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

pub mod position;
pub mod uri;

pub use position::{Encoding, from_lsp, to_lsp};
pub use uri::{path_to_uri, uri_to_path};
