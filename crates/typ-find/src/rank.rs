use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use unicode_segmentation::UnicodeSegmentation;

/// One ranked candidate, and which of its graphemes the query matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHit {
    pub path: String,
    /// Grapheme indices into `path`, ascending. Empty for an empty query.
    ///
    /// **Graphemes, not chars.** Invariant 4, converted here because this is
    /// the boundary that owns it: the matcher works in chars and the picker
    /// paints cells, and a path carrying a combining sequence has more of the
    /// former than the latter.
    pub indices: Vec<u32>,
}

/// The `limit` best matches for `query`, best first.
///
/// An empty query is the picker's opening screen and returns the corpus in its
/// own order — the walk already sorted it, and asking the matcher to rank
/// nothing by nothing produces whatever its tie-break happens to give.
///
/// Two passes, deliberately. `match_list` scores every candidate and is the
/// expensive one — 4.51 ms against 50,000 paths, measured. `indices` then runs
/// only on the survivors, which is the visible page: fifty calls rather than
/// fifty thousand. Computing indices up front would be roughly doubling the
/// cost of every keystroke to produce highlighting for rows nobody will see.
pub fn rank(query: &str, candidates: &[String], limit: usize) -> Vec<FileHit> {
    if limit == 0 {
        return Vec::new();
    }

    if query.is_empty() {
        return candidates
            .iter()
            .take(limit)
            .map(|path| FileHit {
                path: path.clone(),
                indices: Vec::new(),
            })
            .collect();
    }

    // `match_paths` scores the final path segment higher, which is what makes
    // typing a filename find the file rather than every sibling of a directory
    // with the same letters.
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());

    // Smart case: lowercase matches anything, a capital means it. The same rule
    // `SearchQuery` uses for buffer search, so the two halves of "find" do not
    // disagree about what a capital is for.
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);

    let mut scored = pattern.match_list(candidates.iter(), &mut matcher);

    // `match_list` sorts by score, but ties keep input order, and input order is
    // a total order only because `walk` sorted it. Making the tie-break explicit
    // means ranking cannot start shuffling if a caller ever hands over an
    // unsorted corpus.
    scored.sort_by(|(a_path, a_score), (b_path, b_score)| {
        b_score.cmp(a_score).then_with(|| a_path.cmp(b_path))
    });
    scored.truncate(limit);

    let mut buffer = Vec::new();
    let mut raw = Vec::new();
    scored
        .into_iter()
        .map(|(path, _score)| {
            raw.clear();
            buffer.clear();
            let haystack = Utf32Str::new(path, &mut buffer);
            // What `raw` is indexed in depends on which variant nucleo chose,
            // which is the whole reason this is not a one-liner. See
            // `to_graphemes`.
            let byte_indexed = matches!(haystack, Utf32Str::Ascii(_));
            pattern.indices(haystack, &mut matcher, &mut raw);
            // `indices` can report the same position twice and out of order once
            // normalisation is involved; the picker wants each cell named once,
            // ascending.
            raw.sort_unstable();
            raw.dedup();
            FileHit {
                path: path.clone(),
                indices: to_graphemes(path, &raw, byte_indexed),
            }
        })
        .collect()
}

/// Whatever `Pattern::indices` just reported, as grapheme offsets.
///
/// **The unit depends on the variant `Utf32Str::new` picked, and it is not the
/// same unit in both cases.** Established by probe against nucleo-matcher 0.3.1
/// rather than assumed:
///
/// - `Utf32Str::Unicode(&[char])` holds *one char per grapheme cluster* —
///   `chars::graphemes` keeps each cluster's first codepoint and drops the rest.
///   Indices into it are already grapheme indices, so there is nothing to do.
///   (That collapse is nucleo's `unicode-segmentation` feature, which is on by
///   default and which this crate names explicitly in `Cargo.toml` so a future
///   change to their defaults is a compile-time decision rather than a silent
///   re-indexing.)
/// - `Utf32Str::Ascii(&[u8])` holds the **original UTF-8 bytes**, so indices
///   into it are byte offsets.
///
/// The trap is that the second case is reachable for a string that is *not*
/// ASCII. `Utf32Str::new` collapses graphemes, checks whether the collapsed
/// chars are all ASCII, and if so returns `Ascii(str.as_bytes())` — the
/// uncollapsed bytes. `"e\u{301}x.rs"` is 5 graphemes, 6 chars and 7 bytes, and
/// takes that path: nucleo reports index 3 for the `x`, which is its byte
/// offset and neither its char nor its grapheme index.
fn to_graphemes(text: &str, indices: &[u32], byte_indexed: bool) -> Vec<u32> {
    if !byte_indexed {
        return indices.to_vec();
    }
    // A genuinely ASCII string has one byte per grapheme, which is most paths
    // and every path in most projects.
    if text.is_ascii() {
        return indices.to_vec();
    }

    // Byte offset of each grapheme's start, ascending — so a reported byte lands
    // on the grapheme containing it.
    let starts: Vec<usize> = text.grapheme_indices(true).map(|(at, _)| at).collect();
    let mut out: Vec<u32> = indices
        .iter()
        .map(|&byte| match starts.binary_search(&(byte as usize)) {
            Ok(grapheme) => grapheme as u32,
            // Inside a cluster rather than at its start: `partition_point`
            // semantics — the grapheme before the insertion point.
            Err(next) => next.saturating_sub(1) as u32,
        })
        .collect();
    // Two bytes of one cluster collapse to one grapheme, so the mapping is not
    // injective and the result can repeat.
    out.dedup();
    out
}
