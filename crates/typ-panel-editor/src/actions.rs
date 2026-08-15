//! `Action` → editor behavior.
//!
//! Every mutation of the editor lives here or is called from here. Nothing in
//! `handle_key` touches the buffer, which is what keeps the keymap, the future
//! command palette, and the future vim layer able to reach the same behavior.

use typ_buffer::{
    EditKind, Position, Selection, Shift, TextBuffer, clipboard, display_to_grapheme_col,
    grapheme_to_display_col, next_word_boundary, previous_word_boundary,
};
use typ_core::{Action, Direction, Motion, PanelEvent};
use unicode_segmentation::UnicodeSegmentation;

use crate::{EditorPanel, TAB_WIDTH};

impl EditorPanel {
    /// Move one selection according to a motion.
    ///
    /// `extend` decides whether the anchor follows. A plain move from a
    /// non-empty selection collapses toward the direction of travel rather
    /// than moving from the head, which is the behavior everyone arriving from
    /// a GUI editor has in their fingers.
    fn move_selection(&self, selection: Selection, motion: Motion, extend: bool) -> Selection {
        if !extend && !selection.is_empty() {
            let collapse_to = match motion {
                Motion::Left | Motion::WordLeft | Motion::LineStart | Motion::DocumentStart => {
                    Some(selection.range().0)
                }
                Motion::Right | Motion::WordRight | Motion::LineEnd | Motion::DocumentEnd => {
                    Some(selection.range().1)
                }
                // Vertical motions move from the head rather than collapsing to
                // an end: up and down have no "direction of travel" along the
                // selection to collapse toward.
                _ => None,
            };
            if let Some(target) = collapse_to {
                return Selection::caret(target);
            }
        }

        let head = self.moved_position(selection.head, motion);
        Selection {
            anchor: if extend { selection.anchor } else { head },
            head,
        }
    }

    fn moved_position(&self, from: Position, motion: Motion) -> Position {
        let last_line = self.last_line();

        match motion {
            Motion::Left => {
                if from.col > 0 {
                    Position {
                        line: from.line,
                        col: from.col - 1,
                    }
                } else if from.line > 0 {
                    Position {
                        line: from.line - 1,
                        col: self.line_grapheme_count(from.line - 1),
                    }
                } else {
                    from
                }
            }
            Motion::Right => {
                if from.col < self.line_grapheme_count(from.line) {
                    Position {
                        line: from.line,
                        col: from.col + 1,
                    }
                } else if from.line < last_line {
                    Position {
                        line: from.line + 1,
                        col: 0,
                    }
                } else {
                    from
                }
            }
            Motion::Up => self.vertical(from, -1),
            Motion::Down => self.vertical(from, 1),
            Motion::PageUp => self.vertical(from, -(self.page() as i64)),
            Motion::PageDown => self.vertical(from, self.page() as i64),
            Motion::WordLeft => {
                if from.col == 0 {
                    if from.line == 0 {
                        from
                    } else {
                        Position {
                            line: from.line - 1,
                            col: self.line_grapheme_count(from.line - 1),
                        }
                    }
                } else {
                    let text = self.buffer.line_text(from.line);
                    Position {
                        line: from.line,
                        col: previous_word_boundary(&text, from.col),
                    }
                }
            }
            Motion::WordRight => {
                if from.col >= self.line_grapheme_count(from.line) {
                    if from.line >= last_line {
                        from
                    } else {
                        Position {
                            line: from.line + 1,
                            col: 0,
                        }
                    }
                } else {
                    let text = self.buffer.line_text(from.line);
                    Position {
                        line: from.line,
                        col: next_word_boundary(&text, from.col),
                    }
                }
            }
            Motion::LineStart => Position {
                line: from.line,
                col: 0,
            },
            Motion::LineEnd => Position {
                line: from.line,
                col: self.line_grapheme_count(from.line),
            },
            Motion::DocumentStart => Position { line: 0, col: 0 },
            Motion::DocumentEnd => Position {
                line: last_line,
                col: self.line_grapheme_count(last_line),
            },
        }
    }

    /// Vertical movement, preserving the goal column through short lines.
    fn vertical(&self, from: Position, delta: i64) -> Position {
        let goal = self.goal_col.unwrap_or_else(|| {
            grapheme_to_display_col(&self.buffer.line_text(from.line), from.col, TAB_WIDTH)
        });
        let line = (from.line as i64 + delta).clamp(0, self.last_line() as i64) as usize;
        let col = display_to_grapheme_col(&self.buffer.line_text(line), goal, TAB_WIDTH);
        Position { line, col }
    }

    /// Every selection's text, joined by newlines.
    ///
    /// Newlines rather than nothing, because the counterpart paste splits on
    /// them to hand one line back to each cursor. Joining with the empty string
    /// would make a three-cursor copy indistinguishable from one long word.
    fn selected_text(&self) -> String {
        self.selections
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| {
                let (start, end) = s.range();
                self.buffer.text_in_range(start, end)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Replace the selection set, preserving order and the primary.
    pub(crate) fn set_selections(&mut self, list: Vec<Selection>) {
        let mut iter = list.into_iter();
        let first = iter.next().expect("selections are never empty");
        self.selections.set_single(first);
        for selection in iter {
            self.selections.push(selection);
        }
    }

    /// Apply one described edit per selection, keeping every other selection
    /// pointing at the text it was aimed at.
    ///
    /// The closure *describes* an edit as a range plus its replacement rather
    /// than performing it. That is what makes multi-cursor correct: an edit
    /// shifts every position after it, so the positions a later selection was
    /// built from are stale the moment an earlier edit lands. Describing first
    /// lets this function apply the edits in order and carry the accumulated
    /// shift forward, which is the same job a text editor's change-mapping does
    /// and is not something each action should reimplement.
    fn edit_at_each_selection(
        &mut self,
        kind: EditKind,
        describe: impl Fn(Selection, &TextBuffer) -> Edit,
    ) -> Option<Vec<PanelEvent>> {
        // Describing happens entirely before the first mutation, so the closure
        // can borrow the buffer directly. The previous version copied every line
        // in the file into a Vec<String> to dodge a borrow that was never a
        // conflict — 50k allocations per keystroke to avoid a compile error that
        // does not occur.
        let described: Vec<Edit> = self
            .selections
            .iter()
            .map(|s| describe(*s, &self.buffer))
            .collect();

        // One snapshot for the whole group, so a thirty-caret edit is one undo
        // step rather than thirty — and consecutive edits of the same kind fold
        // into the run already open, so typing a word is one step too.
        self.buffer.begin_edit_group(kind, &self.selections);

        let mut shift = Shift::default();
        let mut heads: Vec<Position> = Vec::with_capacity(described.len());
        for edit in described {
            let start = shift.apply(edit.start);
            let end = shift.apply(edit.end);
            self.buffer.replace_range(start, end, &edit.text);

            let after = position_after(start, &edit.text);
            shift.record(edit.end.line, end, after);
            heads.push(after);
        }

        self.buffer.end_edit_group();

        self.set_selections(heads.into_iter().map(Selection::caret).collect());
        self.goal_col = None;
        self.scroll_to_cursor();
        Some(vec![PanelEvent::NeedsRedraw])
    }

    /// The entry point every consumer uses. `None` means this panel does not
    /// handle the action, so the app should try it.
    pub fn perform(&mut self, action: Action) -> Option<Vec<PanelEvent>> {
        // Anything that is not an edit ends the undo run. "Undo what I just
        // typed" means the text typed since the cursor last moved, so the
        // boundary belongs on every action that is not itself an edit — one
        // place, rather than remembered at each of them.
        if !matches!(
            action,
            Action::InsertChar(_) | Action::InsertNewline | Action::Delete { .. }
        ) {
            self.buffer.undo_boundary();
        }

        match action {
            Action::Move { motion, extend } => {
                let vertical = matches!(
                    motion,
                    Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown
                );
                if vertical {
                    // Latch the goal from where the cursor is *now*, before
                    // moving. Recomputing it afterwards would store the column
                    // the motion just clamped to, so one pass through a short
                    // line would narrow the goal permanently — the exact bug
                    // this field exists to prevent.
                    if self.goal_col.is_none() {
                        let cursor = self.cursor();
                        self.goal_col = Some(grapheme_to_display_col(
                            &self.buffer.line_text(cursor.line),
                            cursor.col,
                            TAB_WIDTH,
                        ));
                    }
                } else {
                    self.goal_col = None;
                }

                // Read every selection before writing any: `move_selection`
                // borrows self immutably, and the write needs it mutably.
                let moved: Vec<Selection> = self
                    .selections
                    .iter()
                    .map(|s| self.move_selection(*s, motion, extend))
                    .collect();
                self.set_selections(moved);
                self.scroll_to_cursor();
                Some(vec![PanelEvent::NeedsRedraw])
            }
            Action::InsertChar(c) => {
                let text = c.to_string();
                self.edit_at_each_selection(EditKind::Insert, move |selection, _buffer| {
                    let (start, end) = selection.range();
                    Edit {
                        start,
                        end,
                        text: text.clone(),
                    }
                })
            }

            // `Other`, not `Insert`: a newline ends the typing run, so undo
            // after Enter takes back the line rather than the paragraph.
            Action::InsertNewline => {
                self.edit_at_each_selection(EditKind::Other, |selection, _buffer| {
                    let (start, end) = selection.range();
                    Edit {
                        start,
                        end,
                        text: "\n".to_string(),
                    }
                })
            }

            Action::Delete { direction, by_word } => {
                self.edit_at_each_selection(EditKind::Delete, move |selection, buffer| {
                    // A non-empty selection is the target, whichever key was
                    // pressed.
                    if !selection.is_empty() {
                        let (start, end) = selection.range();
                        return Edit::delete(start, end);
                    }

                    let head = selection.head;
                    // One line, not every line: a word boundary never reaches
                    // past the line it is on.
                    let line_len = buffer.line_grapheme_count(head.line);

                    match direction {
                        Direction::Backward => {
                            if head.col > 0 {
                                let target = if by_word {
                                    buffer.with_line_str(head.line, |line| {
                                        previous_word_boundary(line, head.col)
                                    })
                                } else {
                                    head.col - 1
                                };
                                Edit::delete(
                                    Position {
                                        line: head.line,
                                        col: target,
                                    },
                                    head,
                                )
                            } else if head.line > 0 {
                                // Join with the previous line: delete the
                                // newline between them.
                                let previous = head.line - 1;
                                let col = buffer.line_grapheme_count(previous);
                                Edit::delete(
                                    Position {
                                        line: previous,
                                        col,
                                    },
                                    head,
                                )
                            } else {
                                Edit::nothing(head)
                            }
                        }
                        Direction::Forward => {
                            if head.col < line_len {
                                let target = if by_word {
                                    buffer.with_line_str(head.line, |line| {
                                        next_word_boundary(line, head.col)
                                    })
                                } else {
                                    head.col + 1
                                };
                                Edit::delete(
                                    head,
                                    Position {
                                        line: head.line,
                                        col: target,
                                    },
                                )
                            } else if head.line + 1 < buffer.line_count() {
                                // At the end of a line, pull the next one up.
                                Edit::delete(
                                    head,
                                    Position {
                                        line: head.line + 1,
                                        col: 0,
                                    },
                                )
                            } else {
                                Edit::nothing(head)
                            }
                        }
                    }
                })
            }

            Action::Undo => {
                // No clamping: these selections were valid against this exact
                // rope when they were recorded, which is also why undo puts the
                // cursor back where the edit was made rather than wherever the
                // clamp happened to land it.
                if let Some(restored) = self.buffer.undo(&self.selections) {
                    self.selections = restored;
                    self.goal_col = None;
                }
                self.scroll_to_cursor();
                Some(vec![PanelEvent::NeedsRedraw])
            }

            Action::Redo => {
                if let Some(restored) = self.buffer.redo(&self.selections) {
                    self.selections = restored;
                    self.goal_col = None;
                }
                self.scroll_to_cursor();
                Some(vec![PanelEvent::NeedsRedraw])
            }

            Action::SelectAll => {
                let last = self.last_line();
                self.selections.set_single(Selection {
                    anchor: Position { line: 0, col: 0 },
                    head: Position {
                        line: last,
                        col: self.line_grapheme_count(last),
                    },
                });
                self.goal_col = None;
                Some(vec![PanelEvent::NeedsRedraw])
            }

            Action::SelectLine => {
                let line = self.cursor().line;
                // Without the newline: selecting it would make the next
                // keystroke eat the line break, which is not what "select this
                // line" means to anyone.
                self.selections.set_single(Selection {
                    anchor: Position { line, col: 0 },
                    head: Position {
                        line,
                        col: self.line_grapheme_count(line),
                    },
                });
                self.goal_col = None;
                Some(vec![PanelEvent::NeedsRedraw])
            }

            Action::CollapseSelections => {
                self.selections.collapse_to_heads();
                self.goal_col = None;
                self.scroll_to_cursor();
                Some(vec![PanelEvent::NeedsRedraw])
            }

            Action::AddCursor(direction) => {
                let from = self.selections.primary().head;
                let target_line = match direction {
                    Direction::Backward => from.line.checked_sub(1),
                    Direction::Forward => {
                        let next = from.line + 1;
                        (next <= self.last_line()).then_some(next)
                    }
                };
                let Some(line) = target_line else {
                    // At the edge of the document there is nowhere to add one.
                    // Some(vec![]) rather than None: the action was handled and
                    // simply had nothing to do, so the app must not retry it as
                    // an app action.
                    return Some(Vec::new());
                };
                let col = from.col.min(self.line_grapheme_count(line));
                self.selections
                    .push(Selection::caret(Position { line, col }));
                self.scroll_to_cursor();
                Some(vec![PanelEvent::NeedsRedraw])
            }

            Action::Copy => {
                let text = self.selected_text();
                // Copying nothing must not wipe what is already held. Every
                // editor in the field either copies the whole line or does
                // nothing; doing nothing is the one that never surprises.
                if !text.is_empty() {
                    clipboard::set(&text);
                }
                Some(vec![PanelEvent::NeedsRedraw])
            }

            Action::Cut => {
                let text = self.selected_text();
                if text.is_empty() {
                    return Some(Vec::new());
                }
                clipboard::set(&text);
                // `Other`, so a cut always stands alone in the undo history
                // rather than folding into a run of typing on either side.
                self.edit_at_each_selection(EditKind::Other, |selection, _buffer| {
                    let (start, end) = selection.range();
                    Edit::delete(start, end)
                })
            }

            Action::Paste => {
                let text = clipboard::get();
                if text.is_empty() {
                    return Some(Vec::new());
                }

                // One clipboard line per cursor when the counts match, which is
                // what makes a multi-cursor copy round-trip through a paste.
                // VS Code and Sublime both do this, and without it a three-line
                // copy stamps all three lines at all three cursors.
                let lines: Vec<String> = text.lines().map(str::to_string).collect();
                let distribute = lines.len() == self.selections.len() && self.selections.len() > 1;

                let index = std::cell::Cell::new(0usize);
                self.edit_at_each_selection(EditKind::Other, move |selection, _buffer| {
                    let (start, end) = selection.range();
                    let piece = if distribute {
                        let i = index.get();
                        index.set(i + 1);
                        lines[i].clone()
                    } else {
                        text.clone()
                    };
                    Edit {
                        start,
                        end,
                        text: piece,
                    }
                })
            }

            // Not this panel's business. The app tries it next.
            _ => None,
        }
    }
}

/// One edit, described rather than performed: replace `start..end` with `text`.
///
/// An empty range inserts and an empty text deletes, so every editing action
/// reduces to this one shape and the position mapping only has to understand
/// one thing.
struct Edit {
    start: Position,
    end: Position,
    text: String,
}

impl Edit {
    fn delete(start: Position, end: Position) -> Self {
        Self {
            start,
            end,
            text: String::new(),
        }
    }

    /// An edit that changes nothing, for a caret with nowhere to go — the
    /// start of the buffer for backspace, the end for delete.
    fn nothing(at: Position) -> Self {
        Self {
            start: at,
            end: at,
            text: String::new(),
        }
    }
}

/// Where a position ends up once `text` has been inserted at `start`.
fn position_after(start: Position, text: &str) -> Position {
    let mut line = start.line;
    let mut col = start.col;
    for grapheme in text.graphemes(true) {
        if grapheme == "\n" || grapheme == "\r\n" {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    Position { line, col }
}
