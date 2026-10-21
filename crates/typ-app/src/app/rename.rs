//! Rename: `f2` or `>rename`.
//!
//! `textDocument/prepareRename` first when the server offers it, so a symbol
//! that cannot be renamed says why before a name is typed for it; then the
//! status bar prompt, seeded with the symbol; then `textDocument/rename`,
//! whose `WorkspaceEdit` lands through `apply_workspace_edit`: every file in
//! its buffer, one undo each, nothing saved (M4.7 decisions 2 and 3).

use std::path::PathBuf;

use typ_buffer::Position;

use super::App;
use crate::lsp::{Ask, NoAnswer, Pending};
use crate::prompt::{Prompt, PromptKind};

/// Where a rename started and the name it started from.
pub(crate) struct Renaming {
    path: PathBuf,
    at: Position,
    from: String,
}

impl App {
    /// `Action::Rename`: ask whether the symbol at the caret can be renamed,
    /// or go straight to the prompt when the server does not say.
    pub(super) fn start_rename(&mut self) {
        let Some((path, at)) = self
            .shown_editor()
            .and_then(|e| Some((e.path()?.to_path_buf(), e.cursor())))
        else {
            self.status = Some(NoAnswer::NoServer.message().to_string());
            return;
        };
        let prepares = self
            .lsp
            .capability(&path, "renameProvider")
            .and_then(|provider| provider.get("prepareProvider"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if prepares {
            self.ask_server(Ask::PrepareRename);
            return;
        }
        match self.lsp.able(Ask::Rename, &path) {
            Ok(()) => {
                let word = self.word_at_caret();
                self.open_rename_prompt(path, at, word);
            }
            Err(reason) => self.status = Some(reason.message().to_string()),
        }
    }

    /// The answer to `prepareRename`: a range, a range and a placeholder,
    /// `defaultBehavior`, or null for nothing renameable here. A refusal
    /// arrives as an error and the bar says the server's reason.
    pub(super) fn rename_prepared(
        &mut self,
        server: typ_lsp::ServerId,
        result: &serde_json::Value,
    ) -> bool {
        let Some((path, at)) = self
            .shown_editor()
            .and_then(|e| Some((e.path()?.to_path_buf(), e.cursor())))
        else {
            return false;
        };
        if result.is_null() {
            self.status = Some(NoAnswer::Empty(Ask::PrepareRename).message().to_string());
            return true;
        }
        let range = result.get("range").unwrap_or(result);
        let seed = match result.get("placeholder").and_then(|p| p.as_str()) {
            Some(placeholder) => placeholder.to_string(),
            None => self
                .text_in_lsp_range(server, range)
                .unwrap_or_else(|| self.word_at_caret()),
        };
        self.open_rename_prompt(path, at, seed);
        true
    }

    fn open_rename_prompt(&mut self, path: PathBuf, at: Position, from: String) {
        let mut prompt = Prompt::new(PromptKind::Rename);
        prompt.restore_input(from.clone());
        self.prompt = Some(prompt);
        self.renaming = Some(Renaming { path, at, from });
        self.mark_dirty();
    }

    /// The prompt's answer: ask the server to rename to `name`.
    pub(super) fn rename_to(&mut self, name: String) {
        let Some(renaming) = self.renaming.take() else {
            return;
        };
        if name.is_empty() || name == renaming.from {
            return;
        }
        let Some((pane, index)) = self.tab_for(&renaming.path) else {
            return;
        };
        let Some(editor) = self.panes[pane].tabs[index].editor() else {
            return;
        };
        let rope = editor.buffer().snapshot();
        let char_index = editor.buffer().char_index(renaming.at);
        let asked = self
            .lsp
            .ask_with(Ask::Rename, &renaming.path, 0, |uri, encoding| {
                let p = typ_lsp::to_lsp(encoding, rope.slice(..), char_index);
                serde_json::json!({
                    "textDocument": { "uri": uri },
                    "position": { "line": p.line, "character": p.character },
                    "newName": name,
                })
            });
        match asked {
            Ok(()) => self.lsp.stamp_last(renaming.at),
            Err(reason) => self.status = Some(reason.message().to_string()),
        }
    }

    /// The answer to `rename`: applied when it describes the document as it
    /// still is, refused when the file changed while the server worked.
    pub(super) fn renamed(&mut self, pending: Pending, result: &serde_json::Value) -> bool {
        if self.lsp.version(&pending.asked_at.0) != Some(pending.version) {
            self.status =
                Some("The file changed before the rename came back; nothing renamed.".into());
            return true;
        }
        if result.is_null() {
            self.status = Some(NoAnswer::Empty(Ask::Rename).message().to_string());
            return true;
        }
        let encoding = self.lsp.encoding(pending.server);
        self.apply_workspace_edit(result, encoding);
        true
    }

    /// The text a server's range covers in the buffer on screen.
    fn text_in_lsp_range(
        &self,
        server: typ_lsp::ServerId,
        range: &serde_json::Value,
    ) -> Option<String> {
        let encoding = self.lsp.encoding(server);
        let buffer = self.shown_editor()?.buffer();
        let rope = buffer.rope().slice(..);
        let at = |key: &str| {
            let p = range.get(key)?;
            let p = typ_lsp::lsp_types::Position {
                line: u32::try_from(p.get("line")?.as_u64()?).ok()?,
                character: u32::try_from(p.get("character")?.as_u64()?).ok()?,
            };
            Some(buffer.position(typ_lsp::from_lsp(encoding, rope, p)))
        };
        let text = buffer.text_in_range(at("start")?, at("end")?);
        (!text.is_empty()).then_some(text)
    }

    /// The word around the caret, or nothing.
    fn word_at_caret(&self) -> String {
        let Some(editor) = self.shown_editor() else {
            return String::new();
        };
        let cursor = editor.cursor();
        let line = editor.line_text(cursor.line);
        let start = crate::completion::word_start(&line, cursor.col);
        let rest = crate::completion::between(&line, cursor.col, usize::MAX);
        let tail: String = rest
            .chars()
            .take_while(|c| crate::completion::is_word(*c))
            .collect();
        crate::completion::between(&line, start, cursor.col) + &tail
    }
}
