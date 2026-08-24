//! Tree-sitter parsing and highlighting, behind an API that does not name
//! `tree-house`.
//!
//! The crate boundary is the point: the highlighter underneath was chosen for
//! its rope support and its injections (see the milestone plan's decisions),
//! and swapping it is a change to this crate rather than to the editor.

use std::borrow::Cow;
use std::ops::Range;
use std::sync::OnceLock;
use std::time::Duration;

use ropey::Rope;
use tree_house::Syntax as TreeHouseSyntax;
use tree_house::highlighter::{Highlight, HighlightEvent, Highlighter};
use tree_house::{InjectionLanguageMarker, LanguageConfig, LanguageLoader};

mod language;
mod worker;

pub use language::Language;
pub use worker::{ParseWorker, Parsed};

/// How long a single parse may take before it is abandoned.
///
/// tree-house requires a timeout; there is no "wait forever" option. Generous
/// on purpose: the parse runs on a worker, so a slow one costs a late
/// highlight rather than a dropped frame, and the failure this guards against
/// is a pathological grammar looping, not a large file. Task 1 measured a cold
/// parse of 50k lines at 192 ms, so this is roughly two orders of magnitude of
/// headroom over the largest file the editor will hand it once Task 7's size
/// guard is in.
pub const PARSE_TIMEOUT: Duration = Duration::from_secs(10);

/// Why a parse produced no tree.
///
/// Callers that only need "colour it or don't" use `.ok()`; the variants exist
/// so the reason survives to somewhere it can be reported. `typ-syntax` cannot
/// log — `log` lives in `typ-app`, which sits above `typ-core`, which sits
/// above this crate — so the reason travels as data or not at all.
///
/// There is no "no grammar for this language" variant because there cannot be
/// one: every [`Language`] has a grammar compiled in, so that case is
/// [`Language::for_extension`] returning `None` before `parse` is reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    /// The parse ran longer than [`PARSE_TIMEOUT`].
    Timeout { bytes: usize },
    /// The grammar declined the input for a reason that is not a timeout.
    Failed(&'static str),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout { bytes } => {
                write!(f, "parse of {bytes} bytes exceeded {PARSE_TIMEOUT:?}")
            }
            Self::Failed(why) => write!(f, "parse failed: {why}"),
        }
    }
}

impl std::error::Error for ParseError {}

/// A syntax capture name, as an index rather than a string.
///
/// The index is what the render loop compares on the per-cell path; the name
/// is what a theme keys on. [`scope_name`] converts one to the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Scope(pub u32);

/// A run of bytes that share one capture.
///
/// Byte offsets into the rope, not columns: the conversion to grapheme columns
/// happens at the panel boundary, because invariant 4 says `col` is a grapheme
/// index and a render loop taking byte offsets is how that gets broken quietly
/// on the first non-ASCII file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub scope: Scope,
}

/// Every grammar, its compiled queries, and the one shared scope-name table.
///
/// **The scope index space is global across languages.** tree-house numbers
/// captures per `Query`, so Rust's `Capture(3)` and YAML's `Capture(3)` are
/// unrelated. [`scope_name`] takes no language and the render path's style
/// cache is a flat vector, so every language's `configure` interns into one
/// table here and the same capture *name* gets the same index whichever
/// grammar emitted it.
struct Grammars {
    /// Indexed by position in [`Language::ALL`], which is what a
    /// `tree_house::Language` is in this crate.
    configs: Vec<LanguageConfig>,
    scope_names: Vec<String>,
}

impl Grammars {
    fn load() -> Self {
        let configs: Vec<LanguageConfig> = Language::ALL.iter().map(|l| l.config()).collect();

        let mut scope_names: Vec<String> = Vec::new();
        for config in &configs {
            config.configure(|name| {
                let idx = scope_names
                    .iter()
                    .position(|n| n == name)
                    .unwrap_or_else(|| {
                        scope_names.push(name.to_string());
                        scope_names.len() - 1
                    });
                Some(Highlight::new(idx as u32))
            });
        }

        Self {
            configs,
            scope_names,
        }
    }
}

impl LanguageLoader for Grammars {
    fn language_for_marker(&self, marker: InjectionLanguageMarker) -> Option<tree_house::Language> {
        let name: Cow<str> = match marker {
            InjectionLanguageMarker::Name(name) => Cow::Borrowed(name),
            InjectionLanguageMarker::Match(text) | InjectionLanguageMarker::Filename(text) => {
                Cow::Owned(text.to_string())
            }
            _ => return None,
        };
        Language::from_name(name.trim()).map(th_language)
    }

    fn get_config(&self, lang: tree_house::Language) -> Option<&LanguageConfig> {
        self.configs.get(lang.idx())
    }
}

/// Loaded once per process, on the first parse.
///
/// Grammar loading and query compilation cost 58 ms for one language (Task 1),
/// which is a startup-path cost worth paying once and never again — not per
/// parse, and not per buffer.
static GRAMMARS: OnceLock<Grammars> = OnceLock::new();

fn grammars() -> &'static Grammars {
    GRAMMARS.get_or_init(Grammars::load)
}

fn th_language(language: Language) -> tree_house::Language {
    let idx = Language::ALL
        .iter()
        .position(|l| *l == language)
        .expect("every Language is in Language::ALL");
    tree_house::Language::new(idx as u32)
}

/// The name a theme keys on, for a scope the render loop compares by index.
///
/// Empty for an index no grammar produced, which no caller should be able to
/// construct — `Scope` values come from [`Syntax::highlights`].
pub fn scope_name(scope: Scope) -> &'static str {
    grammars()
        .scope_names
        .get(scope.0 as usize)
        .map(String::as_str)
        .unwrap_or("")
}

/// A parsed buffer.
///
/// Shared across threads as an `Arc`: the worker parses, the panel paints.
pub struct Syntax {
    inner: TreeHouseSyntax,
}

// The worker in Task 3 hands this to the render thread inside an `Arc`. If it
// were ever not `Send + Sync` that would surface as a wall of errors in
// another crate; here it is one line and it names the requirement.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Syntax>();
};

impl Syntax {
    /// Parse a rope.
    ///
    /// Driven from the rope's chunks — never `rope.to_string()`, which is the
    /// trap AGENTS.md names about `line_text`: cheap once, catastrophic in a
    /// loop.
    pub fn parse(language: Language, rope: &Rope) -> Result<Syntax, ParseError> {
        let inner = TreeHouseSyntax::new(
            rope.slice(..),
            th_language(language),
            PARSE_TIMEOUT,
            grammars(),
        )
        .map_err(|e| match e {
            tree_house::Error::Timeout => ParseError::Timeout {
                bytes: rope.len_bytes(),
            },
            tree_house::Error::ExceededMaximumSize => ParseError::Failed("input too large"),
            tree_house::Error::InvalidRanges => ParseError::Failed("invalid ranges"),
            tree_house::Error::NoRootConfig => ParseError::Failed("no grammar for the root layer"),
            tree_house::Error::IncompatibleGrammar(..) => {
                ParseError::Failed("grammar ABI is incompatible with the runtime")
            }
            tree_house::Error::Unknown => ParseError::Failed("unknown"),
        })?;

        Ok(Syntax { inner })
    }

    /// Styled ranges covering `lines`, ascending and non-overlapping.
    ///
    /// The line range becomes a byte range before the highlighter is asked, so
    /// the cost is the viewport's rather than the file's.
    pub fn highlights(&self, rope: &Rope, lines: Range<usize>) -> Vec<Span> {
        let (start, end) = byte_range_of_lines(rope, lines);
        if start >= end {
            return Vec::new();
        }

        let src = rope.slice(..);
        let mut highlighter = Highlighter::new(&self.inner, src, grammars(), start..end);

        let mut pos = highlighter.next_event_offset();
        let mut stack: Vec<Highlight> = Vec::new();
        let mut spans: Vec<Span> = Vec::new();

        while pos < end {
            let (event, new_highlights) = highlighter.advance();
            if event == HighlightEvent::Refresh {
                stack.clear();
            }
            stack.extend(new_highlights);

            let span_start = pos;
            pos = highlighter.next_event_offset().min(end);

            // The innermost capture wins: tree-house pushes outermost first, so
            // the top of the stack is the most specific thing covering these
            // bytes. `string` inside a `macro_invocation` should paint as a
            // string.
            if let Some(top) = stack.last()
                && span_start < pos
            {
                spans.push(Span {
                    start: span_start as usize,
                    end: pos as usize,
                    scope: Scope(top.get()),
                });
            }
        }

        spans
    }

    /// Top-level items the parse found.
    ///
    /// One number proving a parse happened, without depending on what any
    /// particular grammar captures — which is what Task 3's worker test needs.
    /// `named_child_count` on the root is O(1); counting captures would mean
    /// walking the whole file, roughly 700 ms on 50k lines by Task 1's
    /// measurement, to produce a number only a test reads.
    pub fn top_level_items(&self) -> usize {
        self.inner.tree().root_node().named_child_count() as usize
    }
}

impl std::fmt::Debug for Syntax {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Syntax")
            .field("top_level_items", &self.top_level_items())
            .finish_non_exhaustive()
    }
}

/// Byte offsets spanning `lines`, clamped to the rope.
///
/// A viewport near the end of the file asks for lines past it every time the
/// cursor is on the last screen, so clamping is the common path rather than a
/// guard against misuse.
fn byte_range_of_lines(rope: &Rope, lines: Range<usize>) -> (u32, u32) {
    let len_lines = rope.len_lines();
    let start_line = lines.start.min(len_lines);
    let end_line = lines.end.clamp(start_line, len_lines);

    let start = rope.line_to_byte(start_line);
    let end = if end_line == len_lines {
        rope.len_bytes()
    } else {
        rope.line_to_byte(end_line)
    };

    (start as u32, end as u32)
}
