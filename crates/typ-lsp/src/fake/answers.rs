//! What the fake answers each request with: completion, signature help,
//! code actions, rename, and the `--lint` diagnostics.
//!
//! Split from `fake.rs` at M4.7, when the answers outgrew the loop that
//! sends them. Pure functions of the request and the document's text.

use super::Flags;

/// What `textDocument/completion` answers: two functions, one with docs, and
/// under `--complete-edit` a CJK word placed by a `textEdit`.
pub(super) fn completion(
    flags: &Flags,
    text: &str,
    params: &serde_json::Value,
) -> serde_json::Value {
    let mut items = vec![
        serde_json::json!({ "label": "foo", "kind": 3, "detail": "fn foo()", "documentation": "Does foo." }),
        serde_json::json!({ "label": "format", "kind": 3, "detail": "fn format(s: &str)" }),
    ];
    if flags.complete_edit {
        let line = params
            .pointer("/position/line")
            .and_then(|l| l.as_u64())
            .unwrap_or(0);
        let at = params
            .pointer("/position/character")
            .and_then(|c| c.as_u64())
            .unwrap_or(0);
        let start = word_start(text, line as usize, at as usize, flags.utf8);
        items.push(serde_json::json!({
            "label": "日本語",
            "kind": 6,
            "textEdit": {
                "range": {
                    "start": { "line": line, "character": start },
                    "end": { "line": line, "character": at },
                },
                "newText": "日本語",
            },
        }));
    }
    serde_json::json!({ "isIncomplete": false, "items": items })
}

/// The command the fake's second code action runs.
pub(super) const FIX_COMMAND: &str = "fake.fix";

/// The id the fake gives `workspace/applyEdit`.
pub(super) const APPLY_ID: i32 = 8;

/// The line the fake offers code actions on.
pub(super) const FIX_LINE: u64 = 2;

/// An edit replacing the first three characters of `FIX_LINE` with `text`.
pub(super) fn fix_edit(uri: &str, text: &str) -> serde_json::Value {
    serde_json::json!({ "changes": { uri: [{
        "range": {
            "start": { "line": FIX_LINE, "character": 0 },
            "end": { "line": FIX_LINE, "character": 3 },
        },
        "newText": text,
    }]}})
}

/// What `textDocument/codeAction` answers: on `FIX_LINE`, a quick fix with an
/// edit and a bare command; anywhere else, nothing.
pub(super) fn code_actions(params: &serde_json::Value) -> serde_json::Value {
    let line = params.pointer("/range/start/line").and_then(|l| l.as_u64());
    if line != Some(FIX_LINE) {
        return serde_json::json!([]);
    }
    let uri = params
        .pointer("/textDocument/uri")
        .and_then(|u| u.as_str())
        .unwrap_or_default();
    serde_json::json!([
        { "title": "Replace with fix", "kind": "quickfix", "edit": fix_edit(uri, "fix") },
        { "title": "Run the fake fix", "command": FIX_COMMAND, "arguments": [uri] },
    ])
}

/// The word at `character` on `line` of `text`, as its char range and text.
/// Chars stand in for the encoding, which is exact for the ASCII the rename
/// tests use.
pub(super) fn word_at(text: &str, line: u64, character: u64) -> Option<(usize, usize, String)> {
    let chars: Vec<char> = text.split('\n').nth(line as usize)?.chars().collect();
    let at = (character as usize).min(chars.len());
    let is_word = |c: &char| c.is_alphanumeric() || *c == '_';
    let start = at - chars[..at].iter().rev().take_while(|c| is_word(c)).count();
    let end = at + chars[at..].iter().take_while(|c| is_word(c)).count();
    (start < end).then(|| (start, end, chars[start..end].iter().collect()))
}

/// Every whole-word `word` in `text`, as edits to `new_name`.
pub(super) fn occurrences(text: &str, word: &str, new_name: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for (line, row) in text.split('\n').enumerate() {
        let chars: Vec<char> = row.chars().collect();
        let width = word.chars().count();
        for start in 0..chars.len() {
            let end = start + width;
            let is_word = |i: usize| {
                chars
                    .get(i)
                    .is_some_and(|c| c.is_alphanumeric() || *c == '_')
            };
            let here: String = chars[start..end.min(chars.len())].iter().collect();
            if here == word && !(start > 0 && is_word(start - 1)) && !is_word(end) {
                out.push(serde_json::json!({
                    "range": {
                        "start": { "line": line, "character": start },
                        "end": { "line": line, "character": end },
                    },
                    "newText": new_name,
                }));
            }
        }
    }
    out
}

/// What `textDocument/prepareRename` answers: the word at the position and
/// its range, or null off a word.
pub(super) fn prepare_rename(text: &str, params: &serde_json::Value) -> serde_json::Value {
    let line = params
        .pointer("/position/line")
        .and_then(|l| l.as_u64())
        .unwrap_or(0);
    let at = params
        .pointer("/position/character")
        .and_then(|c| c.as_u64())
        .unwrap_or(0);
    let Some((start, end, word)) = word_at(text, line, at) else {
        return serde_json::Value::Null;
    };
    serde_json::json!({
        "range": {
            "start": { "line": line, "character": start },
            "end": { "line": line, "character": end },
        },
        "placeholder": word,
    })
}

/// What `textDocument/rename` answers: the word at the position renamed in
/// the document, and in every other `.rs` file beside it, read from disk as
/// a server reads files nobody has open.
pub(super) fn rename(text: &str, params: &serde_json::Value) -> serde_json::Value {
    let line = params
        .pointer("/position/line")
        .and_then(|l| l.as_u64())
        .unwrap_or(0);
    let at = params
        .pointer("/position/character")
        .and_then(|c| c.as_u64())
        .unwrap_or(0);
    let new_name = params
        .get("newName")
        .and_then(|n| n.as_str())
        .unwrap_or_default();
    let uri = params
        .pointer("/textDocument/uri")
        .and_then(|u| u.as_str())
        .unwrap_or_default();
    let Some((_, _, word)) = word_at(text, line, at) else {
        return serde_json::Value::Null;
    };
    let mut changes = serde_json::Map::new();
    changes.insert(uri.to_string(), occurrences(text, &word, new_name).into());
    let here = uri
        .parse::<lsp_types::Uri>()
        .ok()
        .and_then(|u| crate::uri::uri_to_path(&u));
    let siblings = here
        .as_ref()
        .and_then(|p| std::fs::read_dir(p.parent()?).ok())
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|e| e == "rs") && Some(p) != here.as_ref());
    for path in siblings {
        let (Ok(disk), Some(other)) = (
            std::fs::read_to_string(&path),
            crate::uri::path_to_uri(&path),
        ) else {
            continue;
        };
        let edits = occurrences(&disk, &word, new_name);
        if !edits.is_empty() {
            changes.insert(other.as_str().to_string(), edits.into());
        }
    }
    serde_json::json!({ "changes": changes })
}

/// What `textDocument/signatureHelp` answers: one signature, its parameter
/// the number of commas since the last open paren before the position. Null
/// outside any parens. The position is read as chars, which is exact for the
/// ASCII the tests type. The second parameter's label is an offset pair, the
/// other shape the protocol allows.
pub(super) fn signature(text: &str, params: &serde_json::Value) -> serde_json::Value {
    let line = params
        .pointer("/position/line")
        .and_then(|l| l.as_u64())
        .unwrap_or(0);
    let at = params
        .pointer("/position/character")
        .and_then(|c| c.as_u64())
        .unwrap_or(0);
    let line = text.split('\n').nth(line as usize).unwrap_or("");
    let before: String = line.chars().take(at as usize).collect();
    let Some(open) = before.rfind('(') else {
        return serde_json::Value::Null;
    };
    let active = before[open..].matches(',').count();
    serde_json::json!({
        "signatures": [{
            "label": "fn f(a: i32, b: &str)",
            "parameters": [{ "label": "a: i32" }, { "label": [13, 20] }],
        }],
        "activeSignature": 0,
        "activeParameter": active,
    })
}

/// Where the word ending at `at` on `line` starts, in UTF-8 bytes or UTF-16
/// units: the server's own count, which is the thing under test.
pub(super) fn word_start(text: &str, line: usize, at: usize, utf8: bool) -> usize {
    let line = text.split('\n').nth(line).unwrap_or("");
    let width = |c: char| if utf8 { c.len_utf8() } else { c.len_utf16() };
    let mut seen = 0;
    let before: Vec<char> = line
        .chars()
        .take_while(|c| {
            seen += width(*c);
            seen <= at
        })
        .collect();
    let word: usize = before
        .iter()
        .rev()
        .take_while(|c| c.is_alphanumeric() || **c == '_')
        .map(|c| width(*c))
        .sum();
    before.iter().map(|c| width(*c)).sum::<usize>() - word
}

/// What `--lint` finds: an error per line holding `bad`, a warning per line
/// holding `meh`.
pub(super) fn lint(text: &str) -> Vec<(u32, i64, &'static str)> {
    text.split('\n')
        .enumerate()
        .filter_map(|(line, row)| {
            let line = u32::try_from(line).ok()?;
            if row.contains("bad") {
                Some((line, 1, "fake: bad"))
            } else if row.contains("meh") {
                Some((line, 2, "fake: meh"))
            } else {
                None
            }
        })
        .collect()
}
