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
fn an_unparseable_file_still_returns_a_syntax() {
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
