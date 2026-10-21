//! What a server's answers mean in TYPE's terms: diagnostics, a definition's
//! target, a hover's text.
//!
//! Pure functions over JSON, split from `mod.rs` at M4.6. Nothing here holds
//! state or sends anything.

use std::path::PathBuf;

use typ_core::Diagnostic;
use typ_lsp::Encoding;

/// One LSP diagnostic, in TYPE's coordinates.
///
/// Two conversions, in this order and no other: the server's encoding to a char
/// offset, which `typ-lsp` owns because char is ropey's native unit; then the
/// char offset to a grapheme position, which `typ-buffer` owns because that is
/// where grapheme logic lives. A server may legitimately name a position inside
/// a cluster and `TextBuffer::position` snaps down to its start, because there
/// is no `Position` for the middle of one and `Selections` could not hold it.
pub(crate) fn to_diagnostic(
    buffer: &typ_buffer::TextBuffer,
    encoding: Encoding,
    diagnostic: &typ_lsp::lsp_types::Diagnostic,
) -> Diagnostic {
    let rope = buffer.rope().slice(..);
    let at = |pos| buffer.position(typ_lsp::from_lsp(encoding, rope, pos));
    Diagnostic {
        range: (at(diagnostic.range.start), at(diagnostic.range.end)),
        severity: severity(diagnostic.severity),
        message: message(&diagnostic.message),
        source: diagnostic.source.clone(),
    }
}

/// A diagnostic's message, whichever of the two shapes it arrived in.
///
/// 3.18 lets a message be `MarkupContent` rather than a string. Painting the
/// markup's backticks is worse than painting nothing, so the value is taken and
/// the kind is dropped; rendering markdown is Task 12's problem, and it has a
/// box to do it in.
fn message(message: &typ_lsp::lsp_types::Message) -> String {
    match message {
        typ_lsp::lsp_types::Message::String(text) => text.clone(),
        typ_lsp::lsp_types::Message::MarkupContent(markup) => markup.value.clone(),
    }
}

/// The protocol's number as one of TYPE's four.
///
/// **Anything unrecognised is a warning, not a discard.** `DiagnosticSeverity`
/// has a `Custom(u32)` variant for numbers the protocol does not define, and
/// one of those is still the server saying something is wrong with that range.
/// Dropping it loses information to make a match tidy.
fn severity(severity: Option<typ_lsp::lsp_types::DiagnosticSeverity>) -> typ_core::Severity {
    use typ_core::Severity;
    use typ_lsp::lsp_types::DiagnosticSeverity as Lsp;
    match severity {
        Some(Lsp::Error) => Severity::Error,
        Some(Lsp::Information) => Severity::Information,
        Some(Lsp::Hint) => Severity::Hint,
        // `Warning`, an absent severity, and `Custom(n)` for an `n` the
        // protocol does not define.
        _ => Severity::Warning,
    }
}

/// The path and position a `textDocument/definition` answer names.
///
/// **Four shapes, all legal.** The protocol allows `Location`, `Location[]`,
/// `LocationLink[]` and null, and a client that handles only the first works
/// against exactly one server. The first entry of a list wins; a definition
/// with several answers is a `goto` that has to pick one, and picking the first
/// is what every editor in the field does before it grows a picker for them.
pub(crate) fn definition_target(
    result: &serde_json::Value,
) -> Option<(PathBuf, typ_lsp::lsp_types::Position)> {
    let one = match result {
        serde_json::Value::Array(items) => items.first()?,
        serde_json::Value::Null => return None,
        object => object,
    };

    // `LocationLink` names the target differently, and a server that returns
    // them returns nothing this would otherwise recognise.
    let (uri, range) = match one.get("targetUri") {
        Some(uri) => (
            uri,
            one.get("targetSelectionRange").or(one.get("targetRange"))?,
        ),
        None => (one.get("uri")?, one.get("range")?),
    };

    let uri: typ_lsp::lsp_types::Uri = uri.as_str()?.parse().ok()?;
    let path = typ_lsp::uri_to_path(&uri)?;
    let start = range.get("start")?;
    Some((
        path,
        typ_lsp::lsp_types::Position {
            line: start.get("line")?.as_u64()? as u32,
            character: start.get("character")?.as_u64()? as u32,
        },
    ))
}

/// One thing a server offered to do at the caret: an edit, a command for it
/// to run, or both, the edit first.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CodeAction {
    pub title: String,
    pub edit: Option<serde_json::Value>,
    /// A `Command`: its `command` and `arguments`.
    pub command: Option<serde_json::Value>,
}

/// A `textDocument/codeAction` answer, in the order the server gave.
///
/// **Two shapes in one list.** A `CodeAction` carries its command as an
/// object; a bare `Command` is the object, its `command` a string. One the
/// server marked `disabled` is left out: TYPE has no receded row to show it
/// in, and choosing it would do nothing.
pub(crate) fn code_actions(result: &serde_json::Value) -> Vec<CodeAction> {
    let Some(items) = result.as_array() else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|item| item.get("disabled").is_none())
        .filter_map(|item| {
            let title = item.get("title")?.as_str()?.to_string();
            let command = match item.get("command") {
                Some(serde_json::Value::String(_)) => Some(item.clone()),
                Some(command @ serde_json::Value::Object(_)) => Some(command.clone()),
                _ => None,
            };
            Some(CodeAction {
                title,
                edit: item.get("edit").cloned(),
                command,
            })
        })
        .collect()
}

/// The text of a `textDocument/hover` answer, as text.
///
/// **Three shapes here too**, and the markup is flattened rather than rendered:
/// painting the backticks of a fenced block is worse than painting nothing, and
/// a terminal box is not where a markdown renderer belongs. `MarkedString` is
/// deprecated and still what several servers send.
pub(crate) fn hover_text(result: &serde_json::Value) -> Option<String> {
    let contents = result.get("contents")?;
    let text = match contents {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(items) => items
            .iter()
            .filter_map(marked_string)
            .collect::<Vec<_>>()
            .join("\n"),
        object => marked_string(object)?,
    };
    let text = flatten_markdown(&text);
    (!text.is_empty()).then_some(text)
}

/// One `MarkedString` or `MarkupContent`, as its text.
fn marked_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => Some(text.clone()),
        object => object.get("value")?.as_str().map(str::to_string),
    }
}

/// Markdown as the words in it.
///
/// Deliberately not a renderer. Fences, inline code and emphasis are the three
/// things every server's hover uses and the three whose punctuation reads as
/// noise in a one-line box; anything else markdown can do arrives as its own
/// text, which is the honest floor.
fn flatten_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            continue;
        }
        let line = line.replace("**", "").replace('`', "");
        if line.trim().is_empty() {
            if !out.ends_with('\n') && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out.trim().to_string()
}

/// The items of a `textDocument/completion` answer, in the server's order,
/// and whether it said the list is incomplete.
///
/// **Three shapes**: `CompletionItem[]`, `CompletionList` and null. The order
/// is `sortText`, falling back to the label, which is the order the server
/// meant the list read in before anything is typed.
///
/// `at` turns a position in the server's encoding into the buffer's, which is
/// where a `textEdit`'s start is converted: once, on arrival, against the text
/// the server was answering about.
pub(crate) fn completion_items(
    result: &serde_json::Value,
    at: impl Fn(typ_lsp::lsp_types::Position) -> typ_buffer::Position,
) -> (Vec<crate::completion::Item>, bool) {
    let (items, incomplete) = match result {
        serde_json::Value::Array(items) => (items.as_slice(), false),
        serde_json::Value::Object(_) => (
            result
                .get("items")
                .and_then(|i| i.as_array())
                .map_or(&[][..], Vec::as_slice),
            result
                .get("isIncomplete")
                .and_then(|i| i.as_bool())
                .unwrap_or(false),
        ),
        _ => (&[][..], false),
    };
    let mut keyed: Vec<(String, crate::completion::Item)> = items
        .iter()
        .filter_map(|item| {
            let decoded = completion_item(item, &at)?;
            let sort = text_of(item, "sortText").unwrap_or_else(|| decoded.label.clone());
            Some((sort, decoded))
        })
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    (
        keyed.into_iter().map(|(_, item)| item).collect(),
        incomplete,
    )
}

fn text_of(value: &serde_json::Value, field: &str) -> Option<String> {
    value.get(field)?.as_str().map(str::to_string)
}

/// One item, or `None` when it has no label to show.
///
/// The text it puts in is the `textEdit`'s, then `insertText`, then the label:
/// the specification's own order. An `InsertReplaceEdit` is taken by its
/// insert range, which is what typing over the rest of a word would keep.
fn completion_item(
    item: &serde_json::Value,
    at: impl Fn(typ_lsp::lsp_types::Position) -> typ_buffer::Position,
) -> Option<crate::completion::Item> {
    let label = text_of(item, "label")?;
    let edit = item.get("textEdit");
    let edit_text = edit.and_then(|e| text_of(e, "newText"));
    let edit_start = edit
        .and_then(|e| e.get("range").or_else(|| e.get("insert")))
        .and_then(|range| range.get("start"))
        .and_then(|start| {
            Some(typ_lsp::lsp_types::Position {
                line: start.get("line")?.as_u64()? as u32,
                character: start.get("character")?.as_u64()? as u32,
            })
        });
    let docs = item
        .get("documentation")
        .and_then(marked_string)
        .map(|text| flatten_markdown(&text))
        .filter(|text| !text.is_empty());
    Some(crate::completion::Item {
        filter: text_of(item, "filterText").unwrap_or_else(|| label.clone()),
        text: edit_text
            .or_else(|| text_of(item, "insertText"))
            .unwrap_or_else(|| label.clone()),
        kind: item.get("kind").and_then(|k| k.as_u64()),
        detail: text_of(item, "detail"),
        docs,
        edit_start: edit_start.map(at),
        label,
    })
}

/// The signature a `textDocument/signatureHelp` answer makes active, with its
/// active parameter as a char range into the label.
///
/// `activeParameter` on the signature wins over the one beside the list, as
/// the specification says. A parameter's label is either its own text, found
/// in the signature's, or a pair of UTF-16 offsets into it.
pub(crate) fn signature_help(result: &serde_json::Value) -> Option<crate::signature::Signature> {
    let signatures = result.get("signatures")?.as_array()?;
    let index = result
        .get("activeSignature")
        .and_then(|i| i.as_u64())
        .unwrap_or(0);
    let one = signatures
        .get(usize::try_from(index).ok()?)
        .or(signatures.first())?;
    let label = text_of(one, "label")?;
    let active = one
        .get("activeParameter")
        .or_else(|| result.get("activeParameter"))
        .and_then(|i| i.as_u64())
        .and_then(|i| {
            one.get("parameters")?
                .as_array()?
                .get(usize::try_from(i).ok()?)
        })
        .and_then(|parameter| parameter_range(&label, parameter.get("label")?));
    Some(crate::signature::Signature { label, active })
}

/// A parameter's place in its signature's label, in chars.
fn parameter_range(label: &str, parameter: &serde_json::Value) -> Option<(usize, usize)> {
    match parameter {
        serde_json::Value::String(text) => {
            let byte = label.find(text.as_str())?;
            let start = label[..byte].chars().count();
            Some((start, start + text.chars().count()))
        }
        serde_json::Value::Array(pair) => {
            let unit = |at: &serde_json::Value| -> Option<usize> {
                let mut units = usize::try_from(at.as_u64()?).ok()?;
                let mut chars = 0;
                for c in label.chars() {
                    if units == 0 {
                        break;
                    }
                    units = units.saturating_sub(c.len_utf16());
                    chars += 1;
                }
                Some(chars)
            };
            Some((unit(pair.first()?)?, unit(pair.get(1)?)?))
        }
        _ => None,
    }
}
