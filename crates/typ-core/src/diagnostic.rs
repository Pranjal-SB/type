//! A problem a language server found, in TYPE's own coordinates.
//!
//! **Grapheme positions, not LSP ones.** Invariant 4 says `col` is a grapheme
//! index everywhere, and `RenderContext` is as far as anything protocol-shaped
//! is allowed to travel. `typ-app` converts on the way in: the server's
//! encoding to a char offset in `typ-lsp`, the char offset to a grapheme
//! position in `typ-buffer`.

use typ_buffer::Position;

/// How bad it is.
///
/// TYPE's own four rather than the protocol's numbers, so a match is exhaustive
/// and a fifth would be a compiler error. A severity a server sends that is not
/// one of these maps to `Warning` rather than being dropped — an unrecognised
/// number is still the server saying something is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Ordered worst first, so the gutter can take the maximum of a line by
    /// taking the minimum of this.
    Error,
    Warning,
    Information,
    Hint,
}

/// One diagnostic, anchored to the buffer it describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Half-open, in grapheme coordinates. A zero-width range is legal and
    /// means "here", which servers use for a missing token.
    pub range: (Position, Position),
    pub severity: Severity,
    pub message: String,
    /// Which tool said so — `rustc`, `clippy`, `rust-analyzer`. Shown beside
    /// the message when there is room, and the reason two sources can disagree
    /// about a line without either looking wrong.
    pub source: Option<String>,
}

impl Diagnostic {
    /// Whether this diagnostic touches a line.
    pub fn covers_line(&self, line: usize) -> bool {
        (self.range.0.line..=self.range.1.line).contains(&line)
    }
}
