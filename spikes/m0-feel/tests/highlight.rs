use m0_feel::highlight::Highlighter;

#[test]
fn keywords_are_highlighted_on_their_line() {
    let src = "fn main() {}\nlet x = 1;\n";
    let mut h = Highlighter::new_rust().expect("rust grammar loads");
    h.parse(src);
    let spans = h.spans_for_line(src, 0);
    assert!(!spans.is_empty(), "expected highlight spans on line 0");
}

#[test]
fn parsing_a_large_buffer_completes() {
    let src = "fn f() { let x = 1; }\n".repeat(20_000);
    let mut h = Highlighter::new_rust().expect("rust grammar loads");
    h.parse(&src);
    assert!(!h.spans_for_line(&src, 0).is_empty());
}
