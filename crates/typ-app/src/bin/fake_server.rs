//! `typ-lsp`'s test double, as a bin target of this package.
//!
//! `CARGO_BIN_EXE_` resolves only bins of the package under test, so these
//! three lines are what let `typ-app`'s tests spawn the same server
//! `typ-lsp`'s own tests do rather than keeping a second copy of one.

fn main() {
    typ_lsp::fake::run();
}
