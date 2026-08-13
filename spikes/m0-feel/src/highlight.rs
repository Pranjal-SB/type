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

    /// Highlight spans overlapping `start..end`, as absolute byte ranges,
    /// ordered by start byte.
    ///
    /// Callers pass a whole viewport, not a line. Walking the tree once per
    /// frame instead of once per visible line is the difference between a
    /// 1.1s frame and a 1ms one on a 50k-line file: the walk prunes by byte
    /// range, but pruning still has to visit every sibling, and the root of a
    /// 50k-line file has 50k of them.
    pub fn spans_in_range(&self, start: usize, end: usize) -> Vec<(Range<usize>, &'static str)> {
        let Some(tree) = &self.tree else {
            return Vec::new();
        };
        let mut out = Vec::new();

        // Seek straight to the top-level item containing `start`, then walk
        // siblings forward until past `end`. Starting from the root instead
        // would visit every one of the file's top-level items just to prune
        // them — 40k of them on a 40k-line file, which is the whole cost.
        let mut cursor = tree.walk();
        if cursor.goto_first_child_for_byte(start).is_none() {
            collect_leaves(tree.root_node(), start, end, &mut out);
            return out;
        }
        loop {
            let node = cursor.node();
            if node.start_byte() >= end {
                break;
            }
            collect_leaves(node, start, end, &mut out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        out
    }

    /// Highlight spans for one line, as byte ranges relative to that line.
    pub fn spans_for_line(&self, text: &str, line: usize) -> Vec<(Range<usize>, &'static str)> {
        let line_start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
        if line_start > text.len() {
            return Vec::new();
        }
        let line_len = text[line_start..]
            .split_inclusive('\n')
            .next()
            .map_or(0, str::len);
        let line_end = line_start + line_len;

        self.spans_in_range(line_start, line_end)
            .into_iter()
            .map(|(r, kind)| {
                let s = r.start.saturating_sub(line_start);
                let e = r.end.min(line_end) - line_start;
                (s..e, kind)
            })
            .collect()
    }
}

fn collect_leaves(
    node: Node,
    start: usize,
    end: usize,
    out: &mut Vec<(Range<usize>, &'static str)>,
) {
    if node.end_byte() <= start || node.start_byte() >= end {
        return;
    }
    if node.child_count() == 0 {
        if let Some(kind) = classify(node.kind()) {
            out.push((node.start_byte()..node.end_byte(), kind));
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_leaves(child, start, end, out);
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
