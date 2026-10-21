//! A server's `WorkspaceEdit`: the one way a code action and a rename change
//! text (M4.7 decisions 2 and 3).
//!
//! Per document, converted from the server's position encoding against the
//! buffer it lands in, applied by the editor as one undo step per buffer. A
//! file that is not open opens in a background tab first and stays unsaved,
//! so nothing reaches the disk without the user saving it. An edit that
//! creates, renames or deletes a file is refused whole.

use typ_lsp::Encoding;

use super::{App, Tab};
use crate::lsp::FileEdit;

impl App {
    /// Apply `edit` as a server speaking UTF-16 sent it: the protocol's
    /// default, and what a test without a server has.
    pub fn apply_workspace_edit_for_test(&mut self, edit: &serde_json::Value) {
        self.apply_workspace_edit(edit, Encoding::Utf16);
    }

    /// Apply a `WorkspaceEdit`, saying on the bar what it did or why not.
    /// Returns how many files it changed.
    pub(super) fn apply_workspace_edit(
        &mut self,
        edit: &serde_json::Value,
        encoding: Encoding,
    ) -> usize {
        let files = match crate::lsp::workspace_edit(edit) {
            Ok(files) => files,
            Err(refusal) => {
                self.status = Some(refusal.message().to_string());
                return 0;
            }
        };
        let mut changed = 0;
        for file in files {
            match self.edit_file(file, encoding) {
                Ok(()) => changed += 1,
                Err(message) => self.status = Some(message),
            }
        }
        self.mark_dirty();
        changed
    }

    /// One file's edits, in its tab, opened in the background if need be.
    fn edit_file(&mut self, file: FileEdit, encoding: Encoding) -> Result<(), String> {
        let (pane, index) = match self.tab_for(&file.path) {
            Some(found) => found,
            None => self.open_in_background(&file.path)?,
        };
        let Some(editor) = self.panes[pane].tabs[index].editor_mut() else {
            return Err(format!("{} is not open as text.", file.path.display()));
        };
        let buffer = editor.buffer();
        let rope = buffer.rope().slice(..);
        let at = |pos| buffer.position(typ_lsp::from_lsp(encoding, rope, pos));
        let edits = file
            .edits
            .into_iter()
            .map(|edit| (at(edit.start), at(edit.end), edit.text))
            .collect();
        editor.apply_edits(edits);
        Ok(())
    }

    /// Open `path` as a tab in the current pane without showing it.
    fn open_in_background(&mut self, path: &std::path::Path) -> Result<(usize, usize), String> {
        let panel = self
            .panel_for(path)
            .map_err(|e| format!("Could not open {}: {e:#}", path.display()))?;
        let pane = self.pane;
        self.panes[pane].tabs.push(Tab::new(panel));
        let index = self.panes[pane].tabs.len() - 1;
        self.watch_tab(pane, index);
        Ok((pane, index))
    }
}
