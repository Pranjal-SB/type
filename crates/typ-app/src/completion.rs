//! The completion list (interface §8 2c): what a server offered for the word
//! under the caret.
//!
//! Filtered here, as the word grows, rather than asked again: a keystroke never
//! waits on the server (plan decision 1). The server is asked once per word,
//! and again only when it said its list was incomplete.

use std::path::PathBuf;

use typ_buffer::Position;
use unicode_segmentation::UnicodeSegmentation;

/// One thing a server offered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub label: String,
    /// `CompletionItemKind`, as the protocol numbers it.
    pub kind: Option<u64>,
    pub detail: Option<String>,
    pub docs: Option<String>,
    /// What typing is matched against: `filterText`, or the label.
    pub filter: String,
    /// What accepting puts in.
    pub text: String,
    /// Where the server's `textEdit` starts, in its own encoding. `None`
    /// replaces the word before the caret.
    pub edit_start: Option<typ_lsp::lsp_types::Position>,
}

/// The list while it is up.
pub struct List {
    /// The document it was asked about.
    pub path: PathBuf,
    pub items: Vec<Item>,
    /// The server said there is more than it sent, so a longer word asks again.
    pub incomplete: bool,
    /// Where the word being completed starts.
    pub start: Position,
}

/// Whether a character is part of a word, for completion's purposes.
///
/// Letters, digits and `_` in every script, which is what an identifier is in
/// every language TYPE ships a grammar for.
pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The column the word ending at `col` starts at, in graphemes.
pub fn word_start(line: &str, col: usize) -> usize {
    let before: Vec<&str> = line.graphemes(true).take(col).collect();
    let word = before
        .iter()
        .rev()
        .take_while(|g| g.chars().all(is_word))
        .count();
    before.len() - word
}

/// The graphemes of `line` from `from` up to `to`.
pub fn between(line: &str, from: usize, to: usize) -> String {
    line.graphemes(true)
        .skip(from)
        .take(to.saturating_sub(from))
        .collect()
}
