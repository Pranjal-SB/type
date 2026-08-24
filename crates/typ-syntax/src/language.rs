use tree_house::LanguageConfig;

/// A language TYPE has a grammar compiled in for.
///
/// An enum rather than a registry of trait objects: the set is closed at
/// compile time because the grammars are, and a `match` is what makes adding
/// one a compiler error at every site that must learn about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
}

impl Language {
    /// Every language, in the order their scope indices are interned.
    ///
    /// The one place the set is spelled out. `ALL` and the `match`es below are
    /// what a new grammar has to touch, and forgetting either is a failing test
    /// rather than a language that silently never highlights.
    pub const ALL: &'static [Language] = &[Language::Rust];

    /// The extension a file carries, lowercased by the caller or not.
    ///
    /// Extension rather than content sniffing, matching `typ-registry`, which
    /// already answers the neighbouring question the same way. Shebang and
    /// modeline detection are a later, additive question.
    pub fn for_extension(ext: &str) -> Option<Language> {
        match ext.to_ascii_lowercase().as_str() {
            "rs" => Some(Language::Rust),
            _ => None,
        }
    }

    /// The name a grammar's injection query uses for this language.
    ///
    /// `(#set! injection.language "rust")` in a query, or the info string on a
    /// markdown fence, both arrive as this name. Without it an injection
    /// resolves to nothing and the injected region renders in its parent's
    /// colours.
    pub fn from_name(name: &str) -> Option<Language> {
        match name.to_ascii_lowercase().as_str() {
            "rust" | "rs" => Some(Language::Rust),
            _ => None,
        }
    }

    /// Load this language's grammar and compile its queries.
    ///
    /// Called once per language, behind the `OnceLock` in `lib.rs`. Panics
    /// rather than returning an error: a compiled-in grammar that does not load
    /// is a build that should not have linked, not a runtime condition any
    /// caller can do anything about.
    pub(crate) fn config(self) -> LanguageConfig {
        let (grammar, highlights, injections) = match self {
            Language::Rust => (
                tree_sitter_rust::LANGUAGE,
                tree_sitter_rust::HIGHLIGHTS_QUERY,
                tree_sitter_rust::INJECTIONS_QUERY,
            ),
        };

        let grammar = tree_house::tree_sitter::Grammar::try_from(grammar)
            .unwrap_or_else(|e| panic!("compiled-in grammar for {self:?} did not load: {e}"));

        // No locals query. Locals track variable definitions and references so
        // a name can be coloured by what it binds to rather than by its shape;
        // that is a correctness improvement over the highlights query, not a
        // prerequisite for one, and it costs a second query compile on the
        // startup path. Revisit with a measurement, not with an intention.
        LanguageConfig::new(grammar, highlights, injections, "")
            .unwrap_or_else(|e| panic!("queries for {self:?} did not compile: {e}"))
    }
}
