//! Word-wise motion.
//!
//! Everything here indexes graphemes, never bytes or chars, so `Ctrl+Left`
//! through CJK or emoji moves in the same units the cursor does.

use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Whitespace,
    Word,
    Punctuation,
}

/// Punctuation is its own class rather than being lumped in with words, so
/// `foo::bar` is four stops instead of one — which is what makes word motion
/// useful in code rather than only in prose.
fn class(grapheme: &str) -> Class {
    let Some(c) = grapheme.chars().next() else {
        return Class::Whitespace;
    };
    if c.is_whitespace() {
        Class::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        Class::Word
    } else {
        Class::Punctuation
    }
}

fn classes(line: &str) -> Vec<Class> {
    line.graphemes(true).map(class).collect()
}

/// The next boundary at or after `col`: skip whitespace, then consume one run
/// of like-classed graphemes.
pub fn next_word_boundary(line: &str, col: usize) -> usize {
    let classes = classes(line);
    let len = classes.len();
    let mut i = col.min(len);

    while i < len && classes[i] == Class::Whitespace {
        i += 1;
    }
    if i >= len {
        return len;
    }
    let run = classes[i];
    while i < len && classes[i] == run {
        i += 1;
    }
    i
}

/// The previous boundary at or before `col`, mirroring `next_word_boundary`.
pub fn previous_word_boundary(line: &str, col: usize) -> usize {
    let classes = classes(line);
    let mut i = col.min(classes.len());

    while i > 0 && classes[i - 1] == Class::Whitespace {
        i -= 1;
    }
    if i == 0 {
        return 0;
    }
    let run = classes[i - 1];
    while i > 0 && classes[i - 1] == run {
        i -= 1;
    }
    i
}

/// The run containing `col`, as `(start, end)` grapheme indices.
///
/// A cursor sitting immediately after a word counts as being on it, which is
/// what makes double-click-at-the-end select what the user meant.
pub fn word_at(line: &str, col: usize) -> Option<(usize, usize)> {
    let classes = classes(line);
    let len = classes.len();
    if len == 0 {
        return None;
    }
    let probe = if col < len { col } else { len - 1 };
    let target = classes[probe];
    if target == Class::Whitespace {
        return None;
    }

    let mut start = probe;
    while start > 0 && classes[start - 1] == target {
        start -= 1;
    }
    let mut end = probe;
    while end < len && classes[end] == target {
        end += 1;
    }
    Some((start, end))
}
