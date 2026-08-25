//! Searching the project's text, inside the same walk that indexes its files.
//!
//! `grep-searcher` and `grep-regex` rather than `TextBuffer::find_all`, for two
//! things that are specifications rather than code: binary detection, and the
//! handling of encodings and line terminators. A picker that offers forty
//! matches inside a `.png` is worse than one that offers none.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::UTF8;
use grep_searcher::{BinaryDetection, SearcherBuilder};
use ignore::{WalkBuilder, WalkState};
use unicode_segmentation::UnicodeSegmentation;

use crate::walk::relative_to;

/// One matching line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineHit {
    /// Root-relative, `/` separated — the same shape `walk` produces.
    pub path: String,
    /// **0-based.** `grep` reports 1-based and `PanelEvent::OpenFile` wants
    /// 0-based; the subtraction happens here and has a test, because an
    /// off-by-one would put every result one line from where it said.
    pub line: usize,
    /// **Grapheme index**, invariant 4. `grep` reports a byte offset into the
    /// line and this is the boundary that owns the conversion.
    pub col: usize,
    pub text: String,
}

/// What one search found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Search {
    pub hits: Vec<LineHit>,
    /// False when the cap stopped it early, so the picker can say so rather
    /// than implying the project holds exactly `limit` matches.
    pub complete: bool,
}

/// Longest line text kept per hit.
///
/// A minified bundle is one line of two megabytes. The picker renders eighty
/// columns of it, so shipping the rest through the channel is a megabyte
/// allocated per row to be thrown away at the first clip.
const MAX_LINE: usize = 512;

/// Search every file under `root` for `query`, stopping after `limit` hits.
///
/// `overrides` are open buffers: `(path, text)` pairs searched from memory
/// instead of from disk. **This is a correctness feature, not a speed one.** A
/// project search that reports what is on disk while the user is looking at
/// unsaved edits is answering a question nobody asked. Helix does the same and
/// describes it as an optimisation; the reason it matters is the other one.
///
/// An empty query returns nothing rather than everything: an empty regex
/// matches at every position, which here means every line of every file.
/// An unparsable one returns nothing too — the query is whatever has been typed
/// *so far*, so half-written patterns arrive on every keystroke and `[` is not
/// an error worth reporting, just a pattern that is not finished.
pub fn search(root: &Path, query: &str, limit: usize, overrides: &[(PathBuf, String)]) -> Search {
    if query.is_empty() || limit == 0 {
        return Search {
            hits: Vec::new(),
            complete: true,
        };
    }

    // Smart case: the same rule `SearchQuery` and `rank` use. Three parts of
    // "find" disagreeing about what a capital means would be worse than any one
    // of them choosing wrong.
    let Ok(matcher) = RegexMatcherBuilder::new().case_smart(true).build(query) else {
        return Search {
            hits: Vec::new(),
            complete: true,
        };
    };

    let found = Mutex::new(Vec::<LineHit>::new());
    // Set when any worker hits the cap. Checked before starting a file, so the
    // overshoot is bounded by one file rather than by the number of threads
    // times the size of the project.
    let capped = Mutex::new(false);

    WalkBuilder::new(root)
        .require_git(false)
        .build_parallel()
        .run(|| {
            let searcher = SearcherBuilder::new()
                // Stop at the first NUL. This is the binary detection, and it
                // is the main reason this crate is a dependency.
                .binary_detection(BinaryDetection::quit(b'\x00'))
                // **Off, unlike Helix.** They need multi-line for patterns that
                // span lines; one match per line is what makes a line number
                // mean something, and a picker row *is* a line. Revisit when
                // somebody asks for a multi-line pattern.
                .line_number(true)
                .build();
            let mut searcher = searcher;
            let matcher = matcher.clone();
            let found = &found;
            let capped = &capped;

            Box::new(move |entry| {
                let Ok(entry) = entry else {
                    return WalkState::Continue;
                };
                if !entry.file_type().is_some_and(|t| t.is_file()) {
                    return WalkState::Continue;
                }
                if *capped.lock().expect("cap mutex") {
                    return WalkState::Quit;
                }
                let Some(relative) = relative_to(root, entry.path()) else {
                    return WalkState::Continue;
                };

                let mut local = Vec::new();
                let sink = UTF8(|line_number, line| {
                    local.push(hit_of(&relative, line_number, line, &matcher));
                    Ok(true)
                });

                // The open buffer wins over the file on disk.
                let overridden = overrides
                    .iter()
                    .find(|(path, _)| path == entry.path())
                    .map(|(_, text)| text);
                let result = match overridden {
                    Some(text) => searcher.search_slice(&matcher, text.as_bytes(), sink),
                    None => searcher.search_path(&matcher, entry.path(), sink),
                };
                if result.is_err() {
                    // Unreadable, or not text after all. Nothing to report to —
                    // this crate sits below `typ-app` and cannot log — and one
                    // bad file must not empty the results.
                    return WalkState::Continue;
                }

                let mut found = found.lock().expect("search mutex");
                found.append(&mut local);
                if found.len() >= limit {
                    *capped.lock().expect("cap mutex") = true;
                    return WalkState::Quit;
                }
                WalkState::Continue
            })
        });

    let mut hits = found.into_inner().expect("search mutex");
    // A parallel walk finds files in whichever order the workers got there, so
    // without this the same search lists its results differently between runs.
    hits.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));

    let complete = hits.len() <= limit;
    hits.truncate(limit);
    Search { hits, complete }
}

/// Turn one sink callback into a `LineHit`.
fn hit_of(
    relative: &str,
    line_number: u64,
    line: &str,
    matcher: &grep_regex::RegexMatcher,
) -> LineHit {
    use grep_matcher::Matcher as _;

    let stripped = line.trim_end_matches(['\n', '\r']);
    // Byte offset of the first match on this line, or the start of it if the
    // matcher will not say — a `col` that is merely imprecise beats one that
    // panics.
    let byte = matcher
        .find(stripped.as_bytes())
        .ok()
        .flatten()
        .map(|m| m.start())
        .unwrap_or(0);

    LineHit {
        path: relative.to_string(),
        // 1-based in, 0-based out.
        line: (line_number as usize).saturating_sub(1),
        col: grapheme_col(stripped, byte),
        text: clip(stripped),
    }
}

/// A byte offset into `line`, as a grapheme index. Invariant 4.
fn grapheme_col(line: &str, byte: usize) -> usize {
    if line.is_ascii() {
        return byte.min(line.len());
    }
    line.grapheme_indices(true)
        .take_while(|(at, _)| *at < byte)
        .count()
}

/// Keep the line renderable. See `MAX_LINE`.
fn clip(line: &str) -> String {
    if line.len() <= MAX_LINE {
        return line.to_string();
    }
    // On a grapheme boundary — truncating a `String` mid-cluster is a panic,
    // and mid-codepoint is invalid UTF-8.
    let end = line
        .grapheme_indices(true)
        .map(|(at, _)| at)
        .take_while(|at| *at <= MAX_LINE)
        .last()
        .unwrap_or(0);
    line[..end].to_string()
}
