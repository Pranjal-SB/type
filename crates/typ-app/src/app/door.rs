//! The `ctrl+k` door: a prefix pressed, and the key that follows it.
//!
//! A child module of `app` for the reason `picker` is: it reaches `App`'s
//! private fields without widening them.

use anyhow::Result;
use crossterm::event::KeyCode;
use typ_core::{KeyChord, Resolved};

use super::App;

impl App {
    /// The first key of a sequence, if one is waiting for its second.
    pub fn pending_prefix(&self) -> Option<&str> {
        self.pending.as_deref()
    }

    /// The key after a prefix. The prefix is already taken: whatever this key
    /// is, the sequence is over.
    pub(super) fn finish_sequence(&mut self, prefix: &str, chord: &KeyChord) -> Result<()> {
        if chord.raw.code == KeyCode::Esc {
            return Ok(());
        }
        match self.keymap.resolve(Some(prefix), chord) {
            // The named-action path: panel first, then the app, and the same
            // exemption for the two actions that confirm themselves.
            Resolved::Matched(action) => self.apply_named_action(action),
            Resolved::Pending(_) | Resolved::NotFound => {
                self.clear_transient();
                self.status = Some(format!("{prefix} {} is not bound", chord.canonical));
                Ok(())
            }
        }
    }
}
