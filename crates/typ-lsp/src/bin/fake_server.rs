//! The fake server as a binary. The behaviour lives in `typ_lsp::fake` so
//! `typ-app`'s tests can spawn the same double from a bin target of their own —
//! `CARGO_BIN_EXE_` is only set for bin targets of the package being tested.

fn main() {
    typ_lsp::fake::run();
}
