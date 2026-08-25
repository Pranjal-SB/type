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
    let mut indices = Vec::new();
    scored
        .into_iter()
        .map(|(path, _score)| {
            indices.clear();
            let haystack = haystack_of(path, &mut buffer);
            pattern.indices(haystack, &mut matcher, &mut indices);
            // `indices` can report the same position twice and out of order once
            // normalisation is involved; the picker wants each cell named once,
            // ascending.
            indices.sort_unstable();
            indices.dedup();
            FileHit {
                path: path.clone(),
                indices: indices.clone(),
            }
        })
        .collect()
}

/// Build the haystack so that `Pattern::indices` reports **grapheme** offsets.
///
/// **Not `Utf32Str::new`, and that is the whole point.** Its two variants are
/// indexed in different units — `Unicode(&[char])` holds one char per grapheme
/// cluster, so indices into it are grapheme indices, while `Ascii(&[u8])` holds
/// UTF-8 bytes. That would be harmless if the variant tracked whether the string
/// was ASCII, but it does not: `Utf32Str::new` collapses the graphemes, tests
/// whether the *collapsed* chars are all ASCII, and if so returns
/// `Ascii(str.as_bytes())` — the uncollapsed bytes. `"e\u{301}x.rs"` is 5
/// graphemes, 6 chars and 7 bytes, takes that path, and reports 3 for the `x`,
/// which is neither its char nor its grapheme index.
///
/// Established by probe against nucleo-matcher 0.3.1, then confirmed against the
/// source. The owned `Utf32String::from` does not have the extra branch, which
/// is why Helix — which pre-builds owned haystacks per candidate — can treat
/// nucleo's indices as grapheme indices unconditionally and be right.
///
/// So the rule is theirs and the reason is local: split on `is_ascii` and
/// nothing else. Then ASCII is one byte per grapheme and everything else is one
/// char per grapheme, and both are grapheme indices with no conversion to get
/// wrong. Borrowed rather than owned because this runs per visible row per
/// keystroke and the buffer is reused.
fn haystack_of<'a>(text: &'a str, buffer: &'a mut Vec<char>) -> Utf32Str<'a> {
    if text.is_ascii() {
        return Utf32Str::Ascii(text.as_bytes());
    }
    buffer.clear();
    // One char per grapheme cluster — the first, discarding the rest. Exactly
    // what nucleo's own (private) `chars::graphemes` does, and what makes an
    // index into this slice a grapheme index.
    buffer.extend(
        text.graphemes(true)
            .filter_map(|cluster| cluster.chars().next()),
    );
    Utf32Str::Unicode(buffer)
}

