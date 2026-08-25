//! Search, replace and goto-line: the prompt's three questions and what they do.
//!
//! Lifted out of `app.rs` at M2.8. It was one contiguous block answering to
//! nothing else in the file, and the milestone that adds a picker was about to
//! push `app.rs` further past the point where invariant 9 says to go looking.
//!
//! A second `impl App` rather than a `Search` struct: the four methods reach
//! `self.editor`, `self.prompt` and `self.status`, and threading those through a
//! new type would be a rewrite wearing a refactor's clothes. The seam here is
//! the file boundary, which is the one that was actually costing something.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};
use typ_buffer::SearchQuery;
use typ_core::{Direction, KeyChord};

use super::App;
use crate::prompt::PromptKind;

impl App {
    /// Keys while a prompt is open.
    pub(crate) fn handle_prompt_chord(&mut self, chord: KeyChord) -> Result<()> {
        // Decide first, mutate second. Holding `self.prompt.as_mut()` across an
        // assignment to `self.prompt` does not compile, and threading the
        // borrow through every arm is worse than naming the outcome.
        enum Outcome {
            Stay,
            Close,
            Search(String),
            AskReplacement(String),
            Replace { needle: String, replacement: String },
            Goto(String),
        }

        // A chord is never text, in the prompt exactly as in the buffer —
        // otherwise Ctrl+F while searching types an "f" into the needle.
        let is_chorded = chord
            .raw
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);

        let Some(prompt) = self.prompt.as_mut() else {
            return Ok(());
        };

        let outcome = match chord.raw.code {
            KeyCode::Esc => Outcome::Close,
            KeyCode::Backspace if !is_chorded => {
                prompt.delete_backward();
                Outcome::Stay
            }
            KeyCode::Char(c) if !is_chorded => {
                prompt.insert_char(c);
                Outcome::Stay
            }
            KeyCode::Enter => {
                let input = prompt.take_input();
                match prompt.kind() {
                    // Ctrl+H's first Enter banks the needle and asks the second
                    // question; the prompt stays open across both.
                    PromptKind::Search if prompt.is_replace_flow() => {
                        Outcome::AskReplacement(input)
                    }
                    PromptKind::Search => Outcome::Search(input),
                    PromptKind::Replace => Outcome::Replace {
                        needle: prompt.pending_needle().unwrap_or_default().to_string(),
                        replacement: input,
                    },
                    PromptKind::GotoLine => Outcome::Goto(input),
                }
            }
            _ => Outcome::Stay,
        };

        match outcome {
            Outcome::Stay => {}
            Outcome::Close => self.prompt = None,
            Outcome::Search(needle) => {
                self.prompt = None;
                self.run_search(needle);
            }
            Outcome::AskReplacement(needle) => {
                if let Some(prompt) = self.prompt.as_mut() {
                    prompt.set_pending_needle(needle);
                    prompt.become_replace();
                }
            }
            Outcome::Replace {
                needle,
                replacement,
            } => {
                self.prompt = None;
                self.run_replace_all(&needle, &replacement);
            }
            Outcome::Goto(input) => {
                if input.is_empty() {
                    // Answering nothing is answering "never mind".
                    self.prompt = None;
                } else if let Some(line) = parse_line_number(&input) {
                    self.prompt = None;
                    self.editor.goto_line(line);
                } else {
                    // Rejected, and the prompt stays open with the input still
                    // in it: closing on a typo throws the answer away and makes
                    // the user reopen and retype it.
                    self.status = Some(format!("Not a line number: {input}"));
                    if let Some(prompt) = self.prompt.as_mut() {
                        prompt.restore_input(input);
                    }
                }
            }
        }
        Ok(())
    }

    /// Select the first match at or after the cursor, wrapping.
    fn run_search(&mut self, needle: String) {
        if needle.is_empty() {
            return;
        }
        // Case-insensitive unless the user typed a capital — "smart case",
        // which is what makes a lowercase search find everything without a
        // setting, and a capitalised one mean it.
        let case_sensitive = needle.chars().any(char::is_uppercase);
        let query = SearchQuery::new(needle, case_sensitive);
        self.last_query = Some(query.clone());
        self.jump_to_match(&query, Direction::Forward);
    }

    pub(crate) fn jump_to_match(&mut self, query: &SearchQuery, direction: Direction) {
        let hits = self.editor.buffer_find_all(query);
        if hits.is_empty() {
            self.status = Some(format!("No matches for {}", query.needle));
            return;
        }
        let from = self.editor.cursor();
        let next = match direction {
            // `>=`, not `>`: opening a search with the cursor at the top of the
            // file must find a match that starts there. Jumping leaves the
            // cursor at the match's *end*, so repeating never re-finds the one
            // it is sitting on.
            Direction::Forward => hits
                .iter()
                .find(|hit| hit.range().0 >= from)
                .or_else(|| hits.first()),
            Direction::Backward => hits
                .iter()
                .rev()
                .find(|hit| hit.range().1 < from)
                .or_else(|| hits.last()),
        };
        if let Some(hit) = next.copied() {
            self.editor.select_range(hit);
            self.status = Some(format!("{} matches", hits.len()));
        }
    }

    fn run_replace_all(&mut self, needle: &str, replacement: &str) {
        if needle.is_empty() {
            return;
        }
        let case_sensitive = needle.chars().any(char::is_uppercase);
        let query = SearchQuery::new(needle.to_string(), case_sensitive);
        let count = self.editor.replace_all(&query, replacement);
        self.status = Some(match count {
            0 => format!("No matches for {needle}"),
            1 => "1 replacement".to_string(),
            n => format!("{n} replacements"),
        });
    }
}

/// A 1-based line number typed into the goto prompt, as a 0-based index.
///
/// Line 0 is line 1: a user who types `0` means the top of the file, and there
/// is no other thing they could have meant.
fn parse_line_number(input: &str) -> Option<usize> {
    let n: usize = input.trim().parse().ok()?;
    Some(n.saturating_sub(1))
}
