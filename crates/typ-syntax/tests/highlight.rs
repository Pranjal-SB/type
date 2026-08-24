use ropey::Rope;
use typ_syntax::{Language, Syntax};

fn scopes_of(source: &str) -> Vec<(String, String)> {
    let rope = Rope::from_str(source);
    let syntax = Syntax::parse(Language::Rust, &rope).expect("rust grammar parses");
    syntax
        .highlights(&rope, 0..rope.len_lines())
        .into_iter()
        .map(|span| {
            (
                typ_syntax::scope_name(span.scope).to_string(),
                rope.byte_slice(span.start..span.end).to_string(),
            )
        })
        .collect()
}

#[test]
fn a_keyword_is_captured_as_a_keyword() {
    let scopes = scopes_of("fn main() {}\n");
    assert!(
        scopes
            .iter()
            .any(|(scope, text)| scope.starts_with("keyword") && text == "fn"),
        "expected `fn` captured under keyword, got {scopes:?}"
    );
}

#[test]
fn spans_are_ascending_and_do_not_overlap() {
    let rope = Rope::from_str("fn main() { let x: u32 = 1; }\n");
    let syntax = Syntax::parse(Language::Rust, &rope).unwrap();
    let spans = syntax.highlights(&rope, 0..rope.len_lines());

    // The render loop walks a line once and consumes spans in order. Overlapping
    // or unsorted spans would make it either quadratic or wrong, and which one
    // is not obvious from the output.
    for pair in spans.windows(2) {
        assert!(
            pair[0].end <= pair[1].start,
            "spans overlap or are unsorted: {:?} then {:?}",
            pair[0],
            pair[1]
        );
    }
}

#[test]
fn a_viewport_is_cheaper_than_the_file() {
    // Not a timing assertion — a correctness one. Asking for three lines must
    // not return the whole file's spans, or every scroll costs a full walk.
    let source = "fn a() {}\n".repeat(1_000);
    let rope = Rope::from_str(&source);
    let syntax = Syntax::parse(Language::Rust, &rope).unwrap();

    let window = syntax.highlights(&rope, 0..3);
    let whole = syntax.highlights(&rope, 0..rope.len_lines());
    assert!(
        window.len() < whole.len() / 10,
        "viewport query walked the file"
    );
}

#[test]
fn an_unparsable_file_still_returns_a_syntax() {
    // Tree-sitter recovers from errors rather than failing: half a file of
    // valid Rust must still colour. A parser that gave up on the first typo
    // would switch highlighting off exactly while you are typing.
    let rope = Rope::from_str("fn main() { let x = ;;; }\n");
    let syntax = Syntax::parse(Language::Rust, &rope).expect("recovers from a syntax error");
    assert!(!syntax.highlights(&rope, 0..rope.len_lines()).is_empty());
}

#[test]
fn extensions_map_to_languages() {
    assert_eq!(Language::for_extension("rs"), Some(Language::Rust));
    assert_eq!(Language::for_extension("RS"), Some(Language::Rust));
    assert_eq!(Language::for_extension("txt"), None);
}

#[test]
fn a_scope_index_means_the_same_name_everywhere() {
    // The global interner. tree-house numbers captures per `Query`, so two
    // grammars each have their own `Capture(3)`. `scope_name` takes no
    // language and Task 5's style cache is a flat vector, so an index must
    // mean one name across every grammar. With one language this is trivially
    // true; the test exists so that adding the other four in Task 6 cannot
    // quietly make it false.
    let rope = Rope::from_str("fn main() { let x: u32 = 1; }\n");
    let syntax = Syntax::parse(Language::Rust, &rope).unwrap();

    for span in syntax.highlights(&rope, 0..rope.len_lines()) {
        let name = typ_syntax::scope_name(span.scope);
        assert!(!name.is_empty(), "scope {:?} has no name", span.scope);
        assert_eq!(
            typ_syntax::scope_name(span.scope),
            name,
            "scope_name is not a function of the index alone"
        );
    }
}

#[test]
fn a_parsed_tree_reports_its_top_level_items() {
    // Task 3's worker test needs one number proving a parse happened without
    // depending on what any particular grammar captures.
    let rope = Rope::from_str("fn main() { let x: u32 = 1; }\n");
    let syntax = Syntax::parse(Language::Rust, &rope).unwrap();
    assert!(syntax.top_level_items() > 0);

    let empty = Rope::from_str("");
    let syntax = Syntax::parse(Language::Rust, &empty).unwrap();
    assert_eq!(syntax.top_level_items(), 0);
}

#[test]
fn every_claimed_extension_has_a_grammar_or_is_deliberately_plain() {
    // typ-registry claims these seven. Six should highlight; `txt` should not,
    // and saying so here is what stops "we forgot one" reading like a decision.
    for (ext, expected) in [
        ("rs", Some(Language::Rust)),
        ("toml", Some(Language::Toml)),
        ("json", Some(Language::Json)),
        ("md", Some(Language::Markdown)),
        ("yaml", Some(Language::Yaml)),
        ("yml", Some(Language::Yaml)),
        ("txt", None),
    ] {
        assert_eq!(Language::for_extension(ext), expected, "extension {ext}");
    }
}

#[test]
fn no_extension_reaches_an_injection_only_language() {
    // `MarkdownInline` exists so the block grammar can inject into it. It is
    // not a language you open a file in, and an extension resolving to it
    // would parse a whole document with a grammar that only understands the
    // inside of a paragraph.
    for ext in ["md", "markdown", "mdi", "inline", "rs", "txt"] {
        assert_ne!(
            Language::for_extension(ext),
            Some(Language::MarkdownInline),
            "extension {ext} reached the inline grammar"
        );
    }
}

#[test]
fn every_language_parses_its_own_sample() {
    for (language, sample) in [
        (Language::Rust, "fn main() {}\n"),
        (Language::Toml, "[package]\nname = \"typ\"\n"),
        (Language::Json, "{\"a\": 1}\n"),
        (Language::Markdown, "# Title\n\ntext\n"),
        (Language::Yaml, "a: 1\nb: [2, 3]\n"),
    ] {
        let rope = Rope::from_str(sample);
        let syntax = Syntax::parse(language, &rope)
            .unwrap_or_else(|e| panic!("{language:?} did not parse: {e}"));
        assert!(
            !syntax.highlights(&rope, 0..rope.len_lines()).is_empty(),
            "{language:?} parsed but captured nothing — check the highlights query loaded"
        );
    }
}

#[test]
fn a_fenced_code_block_is_highlighted_as_its_language() {
    // The injection case, and the reason tree-house was taken over a
    // hand-rolled highlighter. Without it every README is one flat colour.
    let rope = Rope::from_str("# T\n\n```rust\nfn main() {}\n```\n");
    let syntax = Syntax::parse(Language::Markdown, &rope).unwrap();
    let names: Vec<&str> = syntax
        .highlights(&rope, 0..rope.len_lines())
        .iter()
        .map(|span| typ_syntax::scope_name(span.scope))
        .collect();
    assert!(
        names.iter().any(|n| n.starts_with("keyword")),
        "no Rust keyword inside the fence: injection did not fire, got {names:?}"
    );
}

#[test]
fn markdown_reaches_its_own_inline_grammar() {
    // Markdown ships as two grammars, and the block one injects into the
    // inline one for every paragraph. Without that a paragraph is one
    // undifferentiated span — no emphasis, no links, no inline code.
    //
    // This needs a directive the crate's own query does not carry; see
    // `Language::config`. The failure it guards against is silent: the
    // injection layer is created and simply parses an empty range.
    let rope = Rope::from_str("# Title\n\nSome *emphasis* here.\n");
    let syntax = Syntax::parse(Language::Markdown, &rope).unwrap();
    let names: Vec<&str> = syntax
        .highlights(&rope, 0..rope.len_lines())
        .iter()
        .map(|span| typ_syntax::scope_name(span.scope))
        .collect();

    assert!(
        names.iter().any(|n| n.starts_with("text.title")),
        "the block grammar stopped capturing headings: {names:?}"
    );
    assert!(
        names.iter().any(|n| n.starts_with("text.emphasis")),
        "the inline grammar captured no emphasis: {names:?}"
    );
}
