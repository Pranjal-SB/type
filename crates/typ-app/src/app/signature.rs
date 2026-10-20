//! Signature help, from the app's side: asked the way completion is (at the
//! end of the batch, one question in flight, an answer checked by the version
//! it describes), and shown while the caret stays inside the call.
//!
//! **It never takes a key.** `)` and `esc` put it away on their way to the
//! editor, and every other key goes on as though it were not there.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::lsp::{Ask, Pending};
use crate::signature::Signature;

use super::{App, Focus};

/// What closes it, typed.
const CLOSE: char = ')';

/// The signature on screen, and whether a key has asked for one.
#[derive(Default)]
pub(crate) struct Signing {
    shown: Option<Shown>,
    wanted: bool,
}

struct Shown {
    path: std::path::PathBuf,
    signature: Signature,
    /// Where the caret was when it first appeared: just inside the call. A
    /// caret left of it has left the call.
    start: typ_buffer::Position,
}

impl App {
    /// The signature on screen and its active parameter's text, if one is up.
    pub fn signature_help(&self) -> Option<(String, Option<String>)> {
        let shown = self.signing.shown.as_ref()?;
        Some((shown.signature.label.clone(), shown.signature.active_text()))
    }

    /// A character was typed into the editor.
    pub(super) fn signature_typed(&mut self, c: char) {
        if c == CLOSE {
            self.close_signature();
            return;
        }
        let Some(path) = self.shown_editor().and_then(|e| e.path()) else {
            return;
        };
        if self
            .lsp
            .triggers(path, "signatureHelpProvider")
            .contains(&c)
        {
            self.signing.wanted = true;
        }
    }

    /// `esc`, or `)`: put it away and stop asking.
    pub(super) fn close_signature(&mut self) {
        self.signing.shown = None;
        self.signing.wanted = false;
        self.lsp.cancel(Ask::Signature);
    }

    /// The end of a batch: drop it if the caret has left the call, and ask if
    /// a key wanted it and nothing is already being asked.
    pub(super) fn settle_signature(&mut self) {
        let left = self.signing.shown.as_ref().is_some_and(|shown| {
            self.focus != Focus::Editor
                || self.shown_editor().is_none_or(|editor| {
                    let cursor = editor.cursor();
                    editor.path() != Some(shown.path.as_path())
                        || cursor.line != shown.start.line
                        || cursor.col < shown.start.col
                })
        });
        if left {
            self.signing.shown = None;
        }
        if !self.signing.wanted || self.lsp.in_flight(Ask::Signature) {
            return;
        }
        self.signing.wanted = false;
        let Some(editor) = self.shown_editor() else {
            return;
        };
        let Some(path) = editor.path().map(|p| p.to_path_buf()) else {
            return;
        };
        let cursor = editor.cursor();
        let char_index = editor.buffer().char_index(cursor);
        let rope = editor.buffer().snapshot();
        if self
            .lsp
            .ask(Ask::Signature, &path, char_index, &rope, 0, None)
            .is_ok()
        {
            self.lsp.stamp_last(cursor);
        }
    }

    /// An answer to signature help. Returns whether the screen changed.
    pub(super) fn signature_answered(
        &mut self,
        pending: Pending,
        result: serde_json::Value,
    ) -> bool {
        let path = pending.asked_at.0;
        let current = self.lsp.version(&path) == Some(pending.version)
            && self.focus == Focus::Editor
            && self.shown_editor().and_then(|e| e.path()) == Some(path.as_path());
        if !current {
            self.signing.wanted = true;
            return false;
        }
        let had = self.signing.shown.is_some();
        let Some(signature) = crate::lsp::signature_help(&result) else {
            self.signing.shown = None;
            return had;
        };
        let start = match &self.signing.shown {
            Some(shown) => shown.start,
            None => pending.asked_at.1,
        };
        self.signing.shown = Some(Shown {
            path,
            signature,
            start,
        });
        true
    }

    /// Ask again at the end of the batch.
    pub(super) fn sign_again(&mut self) {
        self.signing.wanted = true;
    }

    /// Draw it above the caret, if it is up.
    pub(super) fn render_signature(&self, frame: Rect, buf: &mut Buffer) {
        let Some(shown) = self.signing.shown.as_ref() else {
            return;
        };
        let content = self.current_pane_area(self.editor_region(frame)).content;
        let Some(caret) = self.panes[self.pane]
            .shown()
            .panel()
            .cursor_position(content)
        else {
            return;
        };
        let (body, _) = crate::layout::split_frame(frame);
        let area = shown.signature.area(body, caret);
        shown
            .signature
            .render(area, buf, &self.float_context(frame));
    }
}
