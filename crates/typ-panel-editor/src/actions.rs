//! `Action` → editor behavior.
//!
//! Every mutation of the editor lives here or is called from here. Nothing in
//! `handle_key` touches the buffer, which is what keeps the keymap, the future
//! command palette, and the future vim layer able to reach the same behavior.

use typ_buffer::{
    Position, Selection, display_to_grapheme_col, grapheme_to_display_col, next_word_boundary,
    previous_word_boundary,
};
use typ_core::{Action, Motion, PanelEvent};

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

    /// Replace the selection set, preserving order and the primary.
    pub(crate) fn set_selections(&mut self, list: Vec<Selection>) {
        let mut iter = list.into_iter();
        let first = iter.next().expect("selections are never empty");
        self.selections.set_single(first);
        for selection in iter {
            self.selections.push(selection);
        }
    }

    /// The entry point every consumer uses. `None` means this panel does not
    /// handle the action, so the app should try it.
    pub fn perform(&mut self, action: Action) -> Option<Vec<PanelEvent>> {
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
            // Not this panel's business. The app tries it next.
            _ => None,
        }
    }
}
