//! Select-next-occurrence and select-all-occurrences.
//!
//! Split out of `actions.rs` when that file reached the 800-line cap in
//! AGENTS.md. The seam is a real one rather than a convenient byte count: these
//! two share a needle, a case rule and a stop condition with each other and
//! with nothing else in the editor.

use typ_buffer::{Position, SearchQuery, Selection};
use typ_core::PanelEvent;

use crate::EditorPanel;

impl EditorPanel {
    /// Select the word under a bare caret, if there is one.
    ///
    /// Returns whether it did. `Ctrl+D` stops there — the first press means
    /// "this word" and the second means "and the next one", which is the
    /// two-stage shape that makes the key feel like one gesture. `Ctrl+Shift+L`
    /// carries straight on, because "select every occurrence" in two presses is
    /// not what anyone means by it.
    fn select_word_under_caret(&mut self) -> bool {
        let primary = self.selections.primary();
        if !primary.is_empty() {
            return false;
        }
        let cursor = primary.head;
        let word = self
            .buffer
            .with_line_str(cursor.line, |line| typ_buffer::word_at(line, cursor.col));
        let Some((start, end)) = word else {
            // A caret in whitespace has no word to take.
            return false;
        };
        self.selections.set_single(Selection {
            anchor: Position {
                line: cursor.line,
                col: start,
            },
            head: Position {
                line: cursor.line,
                col: end,
            },
        });
        self.goal_col = None;
        true
    }

    /// The primary selection's text, if it has any.
    fn occurrence_needle(&self) -> Option<String> {
        let primary = self.selections.primary();
        if primary.is_empty() {
            return None;
        }
        let (start, end) = primary.range();
        Some(self.buffer.text_in_range(start, end))
    }

    /// Case-sensitive, unlike `Ctrl+F`.
    ///
    /// Smart-case is right for a search box, where the job is finding prose.
    /// Matching an identifier is a different job: `value` and `Value` are two
    /// different things, and every editor in the field draws that line here.
    fn occurrence_query(needle: String) -> SearchQuery {
        SearchQuery::new(needle, true)
    }

    pub(crate) fn select_next_occurrence(&mut self) -> Option<Vec<PanelEvent>> {
        if self.select_word_under_caret() {
            return Some(vec![PanelEvent::NeedsRedraw]);
        }
        let Some(needle) = self.occurrence_needle() else {
            return Some(Vec::new());
        };

        let from = self.selections.primary().range().0;
        let query = Self::occurrence_query(needle);
        // From the cursor, not `find_all` filtered: the whole-buffer scan is
        // ~7 ms on 50k lines and this is a key people hold down. See
        // `TextBuffer::find_next`.
        let Some(hit) = self.buffer.find_next(&query, from) else {
            return Some(Vec::new());
        };

        // Wrapping brings the search back round to a match already held once
        // every occurrence is selected. That is the stop condition, and without
        // it this either loops forever or stacks duplicate cursors on one word.
        if self.selections.iter().any(|s| s.range() == hit.range()) {
            return Some(Vec::new());
        }

        self.selections.push(hit);
        self.scroll_to_cursor();
        Some(vec![PanelEvent::NeedsRedraw])
    }

    pub(crate) fn select_all_occurrences(&mut self) -> Option<Vec<PanelEvent>> {
        self.select_word_under_caret();
        let Some(needle) = self.occurrence_needle() else {
            return Some(Vec::new());
        };

        // One scan, once, for an action nobody holds down — the opposite
        // trade-off from Ctrl+D and the right one here.
        let query = Self::occurrence_query(needle);
        let hits = self.buffer.find_all(&query);
        let Some((first, rest)) = hits.split_first() else {
            return Some(Vec::new());
        };
        self.selections.set_single(*first);
        for hit in rest {
            self.selections.push(*hit);
        }
        self.scroll_to_cursor();
        Some(vec![PanelEvent::NeedsRedraw])
    }
}
