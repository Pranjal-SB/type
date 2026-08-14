//! Literal, line-scoped search.
//!
//! Line-scoped on purpose: a match never spans a line break, so every result
//! is expressible as `(line, grapheme)` without a second coordinate system,
//! and that is what a user typing into a search box means anyway. Regex
//! belongs behind this same `SearchQuery` type later, not beside it.

use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub needle: String,
    pub case_sensitive: bool,
}

impl SearchQuery {
    pub fn new(needle: impl Into<String>, case_sensitive: bool) -> Self {
        Self {
            needle: needle.into(),
            case_sensitive,
        }
    }
}

/// Compare two graphemes, optionally folding case, without allocating.
///
/// `to_lowercase` on a `char` yields an iterator precisely so this can be done
/// lazily — folding into `String`s first would allocate twice per comparison,
/// which on a long line is thousands of allocations for one keystroke.
fn grapheme_eq(a: &str, b: &str, case_sensitive: bool) -> bool {
    if case_sensitive {
        return a == b;
    }
    a.chars()
        .flat_map(char::to_lowercase)
        .eq(b.chars().flat_map(char::to_lowercase))
}

/// Grapheme index pairs of every non-overlapping match in one line.
///
/// The line and the needle are each split into a `Vec<&str>` of graphemes —
/// two allocations of borrowed slices, not one allocation per character. That
/// distinction matters: an incremental search over a 100k-line file runs this
/// on every line for every keystroke, and a `Vec<String>` there would be
/// millions of allocations per keypress.
///
/// Indices come out in graphemes directly, so nothing has to map byte offsets
/// back afterwards — and case folding, which can change a string's byte length,
/// never gets the chance to shift them.
pub fn find_in_line(line: &str, query: &SearchQuery) -> Vec<(usize, usize)> {
    if query.needle.is_empty() {
        return Vec::new();
    }

    let haystack: Vec<&str> = line.graphemes(true).collect();
    let needle: Vec<&str> = query.needle.graphemes(true).collect();
    if needle.len() > haystack.len() {
        return Vec::new();
    }

    let mut hits = Vec::new();
    let mut i = 0usize;
    while i + needle.len() <= haystack.len() {
        let matched = haystack[i..i + needle.len()]
            .iter()
            .zip(&needle)
            .all(|(h, n)| grapheme_eq(h, n, query.case_sensitive));
        if matched {
            hits.push((i, i + needle.len()));
            // Advance past the match. Overlapping hits would let a replace-all
            // rewrite text it had already rewritten.
            i += needle.len();
        } else {
            i += 1;
        }
    }
    hits
}
