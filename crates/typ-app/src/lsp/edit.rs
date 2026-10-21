//! A `WorkspaceEdit` as the files it names and the text edits for each.
//!
//! Pure decoding, beside `reply.rs`. Positions stay in the server's encoding:
//! they are converted against the buffer they land in, which may not be open
//! yet when this runs.

use std::path::PathBuf;

use serde_json::Value;
use typ_lsp::lsp_types::Position;

/// One `TextEdit`, in the server's coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextEdit {
    pub start: Position,
    pub end: Position,
    pub text: String,
}

/// Every edit for one file, in the order the server sent them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileEdit {
    pub path: PathBuf,
    pub edits: Vec<TextEdit>,
}

/// Why a `WorkspaceEdit` is not applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// It creates, renames or deletes a file. Refused whole: applying the
    /// text half of an edit whose file half was skipped leaves a project
    /// that is neither before nor after.
    FileOperation,
    /// It does not have the protocol's shape, or names a URI that is not a
    /// file.
    Malformed,
}

impl Refusal {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Refusal::FileOperation => {
                "This change creates, renames or deletes files, which TYPE does not do yet."
            }
            Refusal::Malformed => "The language server sent a change TYPE could not read.",
        }
    }
}

/// The files a `WorkspaceEdit` changes. `documentChanges` wins over
/// `changes` when both are there, as the protocol says.
pub(crate) fn workspace_edit(edit: &Value) -> Result<Vec<FileEdit>, Refusal> {
    if let Some(changes) = edit.get("documentChanges") {
        let changes = changes.as_array().ok_or(Refusal::Malformed)?;
        if changes.iter().any(|change| change.get("kind").is_some()) {
            return Err(Refusal::FileOperation);
        }
        return changes
            .iter()
            .map(|change| {
                let uri = change
                    .pointer("/textDocument/uri")
                    .ok_or(Refusal::Malformed)?;
                file_edit(uri, change.get("edits").ok_or(Refusal::Malformed)?)
            })
            .collect();
    }
    let Some(changes) = edit.get("changes") else {
        return Ok(Vec::new());
    };
    let mut files = changes
        .as_object()
        .ok_or(Refusal::Malformed)?
        .iter()
        .map(|(uri, edits)| file_edit(&Value::String(uri.clone()), edits))
        .collect::<Result<Vec<_>, _>>()?;
    // A map has no order, and the tabs opened for it should have one.
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn file_edit(uri: &Value, edits: &Value) -> Result<FileEdit, Refusal> {
    let uri: typ_lsp::lsp_types::Uri = uri
        .as_str()
        .and_then(|uri| uri.parse().ok())
        .ok_or(Refusal::Malformed)?;
    let path = typ_lsp::uri_to_path(&uri).ok_or(Refusal::Malformed)?;
    let edits = edits
        .as_array()
        .ok_or(Refusal::Malformed)?
        .iter()
        .map(text_edit)
        .collect::<Option<Vec<_>>>()
        .ok_or(Refusal::Malformed)?;
    Ok(FileEdit { path, edits })
}

/// A `TextEdit` or an `AnnotatedTextEdit`, which only adds a field.
fn text_edit(edit: &Value) -> Option<TextEdit> {
    let position = |at: &str| {
        let at = edit.pointer(at)?;
        Some(Position {
            line: u32::try_from(at.get("line")?.as_u64()?).ok()?,
            character: u32::try_from(at.get("character")?.as_u64()?).ok()?,
        })
    };
    Some(TextEdit {
        start: position("/range/start")?,
        end: position("/range/end")?,
        text: edit.get("newText")?.as_str()?.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A real path's URI: a `file:///tmp` one names no file on Windows.
    fn uri(name: &str) -> String {
        let path = std::env::temp_dir().join(name);
        typ_lsp::path_to_uri(&path).unwrap().as_str().to_string()
    }

    fn one(line: u32) -> Value {
        json!({
            "range": {
                "start": { "line": line, "character": 0 },
                "end": { "line": line, "character": 3 },
            },
            "newText": "x",
        })
    }

    #[test]
    fn changes_and_document_changes_read_the_same() {
        let a = uri("a.rs");
        let from_changes = workspace_edit(&json!({ "changes": { a.clone(): [one(2)] } })).unwrap();
        let from_documents = workspace_edit(&json!({
            "documentChanges": [{ "textDocument": { "uri": a, "version": 3 }, "edits": [one(2)] }]
        }))
        .unwrap();
        assert_eq!(from_changes, from_documents);
        assert_eq!(from_changes[0].edits[0].start.line, 2);
    }

    #[test]
    fn a_file_operation_anywhere_refuses_the_whole_edit() {
        let a = uri("a.rs");
        let edit = json!({ "documentChanges": [
            { "textDocument": { "uri": a, "version": 1 }, "edits": [one(0)] },
            { "kind": "rename", "oldUri": a, "newUri": uri("b.rs") },
        ]});
        assert_eq!(workspace_edit(&edit), Err(Refusal::FileOperation));
    }

    #[test]
    fn an_edit_without_a_range_is_malformed() {
        let edit = json!({ "changes": { uri("a.rs"): [{ "newText": "x" }] } });
        assert_eq!(workspace_edit(&edit), Err(Refusal::Malformed));
    }

    #[test]
    fn an_empty_edit_changes_nothing() {
        assert_eq!(workspace_edit(&json!({})), Ok(Vec::new()));
    }
}
