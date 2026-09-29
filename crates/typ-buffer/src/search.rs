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
    /// Match only where no word grapheme touches either end, so `value` does
    /// not match inside `other_value`.
    pub whole_word: bool,
}

impl SearchQuery {
    pub fn new(needle: impl Into<String>, case_sensitive: bool) -> Self {
        Self {
            needle: needle.into(),
            case_sensitive,
            whole_word: false,
        }
    }

    /// The same query, matching whole words only.
    pub fn whole_word(self) -> Self {
        Self {
            whole_word: true,
            ..self
        }
    }
}

/// `find_in_line_with` for an ASCII line and needle, where a byte is a grapheme.
///
/// Same walk as the grapheme loop: a rejected candidate moves on by one, an
/// accepted one by the needle's length, so the two cannot disagree.
fn find_in_ascii_line(line: &str, needle: &str, whole_word: bool) -> Vec<(usize, usize)> {
    let word_at = |i: usize| line.get(i..i + 1).is_some_and(crate::is_word_grapheme);
    let mut hits = Vec::new();
    let mut i = 0usize;
    while let Some(found) = line[i..].find(needle) {
        let start = i + found;
        let end = start + needle.len();
        let inside_a_word = whole_word && ((start > 0 && word_at(start - 1)) || word_at(end));
        if inside_a_word {
            i = start + 1;
        } else {
            hits.push((start, end));
            i = end;
        }
    }
    hits
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
/// Indices come out in graphemes directly, so nothing has to map byte offsets
/// back afterwards — and case folding, which can change a string's byte length,
/// never gets the chance to shift them.
pub fn find_in_line(line: &str, query: &SearchQuery) -> Vec<(usize, usize)> {
    if query.needle.is_empty() {
        return Vec::new();
    }
    let needle: Vec<&str> = query.needle.graphemes(true).collect();
    find_in_line_with(line, &needle, query)
}

/// `find_in_line` with the needle already split.
///
/// Splitting it is per-search work, not per-line work: a whole-buffer scan calls
/// this once per line, and rebuilding the needle each time was one allocation
/// per line for a value that never changes. That, plus collecting the haystack
/// into a `Vec<&str>`, was what put a 50k-line search at 141 ms against a 16 ms
/// keystroke budget. Neither allocation survives here.
pub(crate) fn find_in_line_with(
    line: &str,
    needle: &[&str],
    query: &SearchQuery,
) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }

    // A byte-level containment check, memchr-backed and allocation-free. Most
    // lines in a real search hold no match at all, and this retires them before
    // any grapheme segmentation happens. Case-sensitive only: folding can change
    // a string's byte length, so the bytes of a case-insensitive needle are not
    // a sound precondition for its matches.
    if query.case_sensitive && !line.contains(query.needle.as_str()) {
        return Vec::new();
    }

    // An ASCII line is one grapheme per byte, so byte offsets are columns and a
    // byte search is the grapheme search. Segmenting it anyway cost about 2 µs a
    // line, which on Ctrl+Shift+L's 4167 lines was most of the frame.
    if query.case_sensitive && line.is_ascii() && query.needle.is_ascii() {
        return find_in_ascii_line(line, &query.needle, query.whole_word);
    }

    // Segmenting the line once and indexing the result beats re-segmenting from
    // each candidate position: building a `Graphemes` iterator per position cost
    // more than the single `Vec` of borrowed slices it was meant to avoid,
    // measured at 190 ms against 141 ms on a 50k-line scan.
    let haystack: Vec<&str> = line.graphemes(true).collect();
    if needle.len() > haystack.len() {
        return Vec::new();
    }

    let mut hits = Vec::new();
    let mut i = 0usize;
    while i + needle.len() <= haystack.len() {
        let end = i + needle.len();
        // A word grapheme on either side means this is the inside of a longer
        // word, which a whole-word query does not want.
        let inside_a_word = || {
            (i > 0 && crate::is_word_grapheme(haystack[i - 1]))
                || haystack
                    .get(end)
                    .is_some_and(|g| crate::is_word_grapheme(g))
        };
        let matched = haystack[i..end]
            .iter()
            .zip(needle)
            .all(|(h, n)| grapheme_eq(h, n, query.case_sensitive))
            && !(query.whole_word && inside_a_word());
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
