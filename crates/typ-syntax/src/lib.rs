//! Spike shape — Task 1 only. Proves `tree-house` compiles against a
//! compiled-in grammar and produces the three numbers the plan's decision
//! rests on. Replaced by Task 2's real API; nothing outside this crate names
//! `tree-house` after that task lands.

use std::ops::Range;
use std::sync::OnceLock;
use std::time::Duration;

use ropey::Rope;
use tree_house::Syntax as TreeHouseSyntax;
use tree_house::highlighter::{Highlight, HighlightEvent, Highlighter};
use tree_house::{InjectionLanguageMarker, Language, LanguageConfig, LanguageLoader};

const SPIKE_TIMEOUT: Duration = Duration::from_millis(500);

struct RustLoader {
    config: LanguageConfig,
}

impl RustLoader {
    fn new() -> Self {
        let grammar = tree_house::tree_sitter::Grammar::try_from(tree_sitter_rust::LANGUAGE)
            .expect("the compiled-in rust grammar loads");
        let config = LanguageConfig::new(
            grammar,
            tree_sitter_rust::HIGHLIGHTS_QUERY,
            tree_sitter_rust::INJECTIONS_QUERY,
            "",
        )
        .expect("the rust highlights query compiles");

        // Every distinct capture name gets its own index, in first-seen order.
        // Real theme resolution and the longest-prefix fallback are Task 2's
        // job; the spike only needs a stable index per capture to prove a
        // highlight can be produced at all.
        let mut names: Vec<String> = Vec::new();
        config.configure(|name| {
            let idx = names.iter().position(|n| n == name).unwrap_or_else(|| {
                names.push(name.to_string());
                names.len() - 1
            });
            Some(Highlight::new(idx as u32))
        });

        Self { config }
    }
}

impl LanguageLoader for RustLoader {
    fn language_for_marker(&self, _marker: InjectionLanguageMarker) -> Option<Language> {
        None
    }

    fn get_config(&self, _lang: Language) -> Option<&LanguageConfig> {
        Some(&self.config)
    }
}

// The grammar and its compiled query are process-wide and built once — the
// real design (Task 2) loads a grammar's query on first use, not per parse.
// Rebuilding it per call would fold query-compile time into every "cold
// parse" number the spike exists to produce.
static LOADER: OnceLock<RustLoader> = OnceLock::new();

pub struct Syntax {
    inner: TreeHouseSyntax,
}

/// Parse a rope as Rust. `None` when the grammar declines the input.
pub fn parse_rust(rope: &Rope) -> Option<Syntax> {
    let loader = LOADER.get_or_init(RustLoader::new);
    let inner =
        TreeHouseSyntax::new(rope.slice(..), Language::new(0), SPIKE_TIMEOUT, loader).ok()?;
    Some(Syntax { inner })
}

/// Styled ranges covering `bytes`, in ascending order, non-overlapping.
///
/// The topmost active highlight per event boundary — good enough for the
/// spike's numbers. Precedence among stacked captures is Task 2's job.
pub fn highlights(syntax: &Syntax, rope: &Rope, bytes: Range<usize>) -> Vec<(Range<usize>, u32)> {
    let loader = LOADER
        .get()
        .expect("a Syntax exists only after parse_rust ran");
    let src = rope.slice(..);
    let mut highlighter = Highlighter::new(&syntax.inner, src, loader, bytes.start as u32..);
    let end = bytes.end as u32;

    let mut pos = highlighter.next_event_offset();
    let mut stack: Vec<Highlight> = Vec::new();
    let mut spans = Vec::new();

    while pos < end {
        let (event, new_highlights) = highlighter.advance();
        if event == HighlightEvent::Refresh {
            stack.clear();
        }
        stack.extend(new_highlights);

        let start = pos;
        pos = highlighter.next_event_offset().min(end);

        if let Some(top) = stack.last()
            && start < pos
        {
            spans.push((start as usize..pos as usize, top.get()));
        }
    }

    spans
}
