//! Completion, from the app's side: when to ask, and what an answer becomes.
//!
//! **Asked at the end of a batch, never from the key.** The key that wants a
//! completion arrives before the `didChange` carrying it, and a question asked
//! there is about text the server does not have yet. So a key only records
//! that it wants one, and the end of the batch asks, after the sync pass has
//! sent the change: one question per batch however many keys were in it, and
//! never more than one in flight. An answer for a version the server has since
//! been sent is dropped, and the word is asked about again.

use crate::completion::{self, List};
use crate::lsp::{Ask, Pending};

use super::{App, Focus};

/// `CompletionTriggerKind`.
const INVOKED: u8 = 1;
const TRIGGER_CHARACTER: u8 = 2;
const INCOMPLETE: u8 = 3;

/// What wants a completion, as the protocol's `context` says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trigger {
    /// A word character.
    Typed,
    /// One of the server's trigger characters.
    Char(char),
    /// The word grew under a list the server said was incomplete.
    Incomplete,
}

/// The list, and whether a key has asked for one.
#[derive(Default)]
pub(crate) struct Completing {
    list: Option<List>,
    wanted: Option<Trigger>,
}

impl App {
    /// The labels the list shows, while it is up.
    pub fn completion_labels(&self) -> Option<Vec<String>> {
        let list = self.completing.list.as_ref()?;
        Some(list.items.iter().map(|item| item.label.clone()).collect())
    }

    /// A character was typed into the editor.
    pub(super) fn completion_typed(&mut self, c: char) {
        let Some(path) = self.shown_editor().and_then(|e| e.path()) else {
            return;
        };
        if self.lsp.triggers(path, "completionProvider").contains(&c) {
            self.completing.list = None;
            self.completing.wanted = Some(Trigger::Char(c));
            return;
        }
        if !completion::is_word(c) {
            self.completing.list = None;
            return;
        }
        self.completing.wanted = match &self.completing.list {
            Some(list) if !list.incomplete => self.completing.wanted,
            Some(_) => Some(Trigger::Incomplete),
            None => Some(Trigger::Typed),
        };
    }

    /// The end of a batch: drop a list the caret has left, and ask if a key
    /// wanted it and nothing is already being asked.
    pub(super) fn settle_completion(&mut self) {
        if self.completing.list.is_some() && self.completion_query().is_none() {
            self.completing.list = None;
        }
        let Some(trigger) = self.completing.wanted else {
            return;
        };
        // The answer to the question in flight decides: shown if it is
        // current, asked again if it is not.
        if self.lsp.in_flight(Ask::Completion) {
            return;
        }
        self.completing.wanted = None;
        let Some(editor) = self.shown_editor() else {
            return;
        };
        let Some(path) = editor.path().map(|p| p.to_path_buf()) else {
            return;
        };
        let cursor = editor.cursor();
        let line = editor.line_text(cursor.line);
        if matches!(trigger, Trigger::Typed | Trigger::Incomplete)
            && completion::word_start(&line, cursor.col) == cursor.col
        {
            // Nothing to complete: the word was typed over before the server
            // got round to it.
            return;
        }
        let char_index = editor.buffer().char_index(cursor);
        let rope = editor.buffer().snapshot();
        let context = match trigger {
            Trigger::Typed => serde_json::json!({ "triggerKind": INVOKED }),
            Trigger::Char(c) => serde_json::json!({
                "triggerKind": TRIGGER_CHARACTER,
                "triggerCharacter": c.to_string(),
            }),
            Trigger::Incomplete => serde_json::json!({ "triggerKind": INCOMPLETE }),
        };
        // Silent when there is no answer to be had. A sentence on the bar for
        // every key typed into a file no server offers completion for is noise.
        if self
            .lsp
            .ask(Ask::Completion, &path, char_index, &rope, 0, Some(context))
            .is_ok()
        {
            self.lsp.stamp_last(cursor);
        }
    }

    /// An answer to a completion. Returns whether the screen changed.
    pub(super) fn completion_answered(
        &mut self,
        pending: Pending,
        result: serde_json::Value,
    ) -> bool {
        let path = pending.asked_at.0;
        let current = self.lsp.version(&path) == Some(pending.version)
            && self.focus == Focus::Editor
            && self.shown_editor().and_then(|e| e.path()) == Some(path.as_path());
        if !current {
            self.complete_again();
            return false;
        }
        let (items, incomplete) = crate::lsp::completion_items(&result);
        let Some(editor) = self.shown_editor() else {
            return false;
        };
        let cursor = editor.cursor();
        let line = editor.line_text(cursor.line);
        let start = typ_buffer::Position {
            line: cursor.line,
            col: completion::word_start(&line, cursor.col),
        };
        self.completing.list = (!items.is_empty()).then_some(List {
            path,
            items,
            incomplete,
            start,
        });
        true
    }

    /// Ask again at the end of the batch, about the word as it is by then,
    /// if it is still a word.
    pub(super) fn complete_again(&mut self) {
        self.completing.wanted.get_or_insert(Trigger::Typed);
    }

    /// What has been typed of the word, or `None` when the caret has left it.
    fn completion_query(&self) -> Option<String> {
        let list = self.completing.list.as_ref()?;
        if self.focus != Focus::Editor {
            return None;
        }
        let editor = self.shown_editor()?;
        if editor.path() != Some(list.path.as_path()) {
            return None;
        }
        let cursor = editor.cursor();
        if cursor.line != list.start.line || cursor.col < list.start.col {
            return None;
        }
        let typed = completion::between(&editor.line_text(cursor.line), list.start.col, cursor.col);
        typed.chars().all(completion::is_word).then_some(typed)
    }
}
