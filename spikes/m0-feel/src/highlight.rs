use std::ops::Range;

use anyhow::Result;
use tree_sitter::{Language, Node, Parser, Tree};

/// Minimal highlighter: parses with tree-sitter and classifies leaf nodes by
/// their grammar node kind.
///
/// A real implementation uses highlight queries and captures. The spike only
/// needs to answer "can tree-sitter keep up while scrolling", so node-kind
/// classification is enough and avoids pulling in the query machinery.
pub struct Highlighter {
    parser: Parser,
    tree: Option<Tree>,
}

impl Highlighter {
    pub fn new_rust() -> Result<Self> {
        let mut parser = Parser::new();
        let lang: Language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&lang)?;
        Ok(Self { parser, tree: None })
    }

    /// Reparse `text`, reusing the previous tree so edits are incremental.
    pub fn parse(&mut self, text: &str) {
        self.tree = self.parser.parse(text, self.tree.as_ref());
    }

    /// Highlight spans for one line, as byte ranges relative to that line.
    pub fn spans_for_line(&self, text: &str, line: usize) -> Vec<(Range<usize>, &'static str)> {
        let Some(tree) = &self.tree else {
            return Vec::new();
        };

        let line_start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
        if line_start > text.len() {
            return Vec::new();
        }
        let line_len = text[line_start..]
            .split_inclusive('\n')
            .next()
            .map_or(0, str::len);
        let line_end = line_start + line_len;

        let mut out = Vec::new();
        collect_leaves(tree.root_node(), line_start, line_end, &mut out);
        out
    }
}

fn collect_leaves(
    node: Node,
    line_start: usize,
    line_end: usize,
    out: &mut Vec<(Range<usize>, &'static str)>,
) {
    if node.end_byte() <= line_start || node.start_byte() >= line_end {
        return;
    }
    if node.child_count() == 0 {
        if let Some(kind) = classify(node.kind()) {
            let s = node.start_byte().max(line_start) - line_start;
            let e = node.end_byte().min(line_end) - line_start;
            if s < e {
                out.push((s..e, kind));
            }
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_leaves(child, line_start, line_end, out);
    }
}

fn classify(kind: &str) -> Option<&'static str> {
    match kind {
        "fn" | "let" | "if" | "else" | "match" | "struct" | "enum" | "impl" | "pub" | "use"
        | "mod" | "return" | "for" | "while" | "loop" => Some("keyword"),
        "string_literal" | "raw_string_literal" | "char_literal" => Some("string"),
        "integer_literal" | "float_literal" => Some("number"),
        "line_comment" | "block_comment" => Some("comment"),
        "identifier" | "type_identifier" | "field_identifier" => Some("identifier"),
        _ => None,
    }
}
