//! Recording a key, and every write the tab makes.
//!
//! Every change goes through `config::write_binding`, so `keys.toml` keeps its
//! comments, and leaves the old file on an undo stack. The tab then loads the
//! file back as `main` would and hands that keymap to the app, so the keymap,
//! the `ctrl+k` menu, the hint and the palette all move together: they all
//! read the one the app holds.

use typ_core::{Action, KeyChord, Keymap, NotifyLevel, PanelEvent, Position, Resolved};

use super::{Binding, KeysView};
use crate::config::{load_keymap, restore_config, write_binding};

/// The float's state: which row is being rebound, and what has been pressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Recording {
    pub row: Binding,
    /// The steps pressed so far, canonical.
    pub steps: Vec<String>,
    /// The last step is a prefix, so the next key finishes the chord.
    pub waiting: bool,
}

impl Recording {
    /// The chord pressed, once it is whole.
    pub fn chord(&self) -> Option<String> {
        (!self.steps.is_empty() && !self.waiting).then(|| self.steps.join(" "))
    }
}

/// A change to one row of `keys.toml`: the chord, and the action's name, `""`
/// to unbind, or `None` to take the row out.
type Write = (String, Option<&'static str>);

impl KeysView {
    /// `enter` on a row, or a click on its key: the float goes up.
    pub(super) fn record(&mut self, row: Binding) -> Vec<PanelEvent> {
        self.selected = Some(row.clone());
        self.recording = Some(Recording {
            row,
            steps: Vec::new(),
            waiting: false,
        });
        vec![PanelEvent::NeedsRedraw]
    }

    /// A key while the float is up. `esc` cancels, `enter` on a whole chord
    /// replaces, and anything else is the chord: a prefix waits for one more
    /// key, and a key after a whole chord starts again.
    pub(super) fn recording_key(&mut self, chord: &KeyChord) -> Vec<PanelEvent> {
        let Some(mut recording) = self.recording.take() else {
            return Vec::new();
        };
        match chord.canonical.as_str() {
            "esc" => return vec![PanelEvent::NeedsRedraw],
            "enter" if recording.chord().is_some() => {
                self.recording = Some(recording);
                return self.replace();
            }
            _ => {}
        }
        if recording.waiting {
            recording.steps.push(chord.canonical.clone());
            recording.waiting = false;
        } else {
            recording.steps = vec![chord.canonical.clone()];
            recording.waiting = matches!(self.keymap.resolve(None, chord), Resolved::Pending(_));
        }
        self.recording = Some(recording);
        vec![PanelEvent::NeedsRedraw]
    }

    /// Put the recorded chord on the row's action. The chord the row had is
    /// unbound, and whatever the new chord ran loses it, which is what
    /// replacing it means in a table with one action per chord.
    pub(super) fn replace(&mut self) -> Vec<PanelEvent> {
        let Some(recording) = self.recording.take() else {
            return Vec::new();
        };
        let Some(new) = recording.chord() else {
            return vec![PanelEvent::NeedsRedraw];
        };
        let (old, action) = recording.row;
        if old.as_deref() == Some(new.as_str()) {
            return vec![PanelEvent::NeedsRedraw];
        }
        // Back on a default, the row comes out rather than restating it.
        let set = if self.defaults.action_for(&new) == Some(action) {
            None
        } else {
            Some(action.name())
        };
        let mut writes = vec![(new.clone(), set)];
        if let Some(old) = old {
            writes.push((old.clone(), self.unbinding(&old)));
        }
        self.write(writes, (Some(new), action))
    }

    /// `del`: the row's chord runs nothing.
    pub(super) fn unbind(&mut self) -> Vec<PanelEvent> {
        let Some((Some(chord), action)) = self.selected.clone() else {
            return notify(NotifyLevel::Info, "Select a binding to unbind.");
        };
        let unbound = self.unbinding(&chord);
        self.write(vec![(chord, unbound)], (None, action))
    }

    /// `r`: the action's chords as shipped. Every row of `keys.toml` that
    /// gives it a chord, or takes one of its own away, comes out.
    pub(super) fn reset(&mut self) -> Vec<PanelEvent> {
        let Some((_, action)) = self.selected.clone() else {
            return notify(NotifyLevel::Info, "Select a binding to reset.");
        };
        let writes: Vec<Write> = self
            .user
            .iter()
            .filter(|chord| {
                self.keymap.action_for(chord) == Some(action)
                    || self.defaults.action_for(chord) == Some(action)
            })
            .map(|chord| (chord.clone(), None))
            .collect();
        if writes.is_empty() {
            return notify(
                NotifyLevel::Info,
                &format!("{} is already as shipped.", action.description()),
            );
        }
        let first = self
            .defaults
            .bindings_for(action)
            .first()
            .map(|c| c.to_string());
        self.write(writes, (first, action))
    }

    /// How to unbind `chord`: an empty row over a default, or no row at all
    /// when only `keys.toml` ever bound it.
    fn unbinding(&self, chord: &str) -> Option<&'static str> {
        self.defaults.action_for(chord).map(|_| "")
    }

    /// Write every change, check the file still loads, and keep the old file
    /// for undo. A file that would no longer load is put back.
    fn write(&mut self, writes: Vec<Write>, select: Binding) -> Vec<PanelEvent> {
        let before = std::fs::read_to_string(&self.path).ok();
        for (chord, value) in &writes {
            if let Err(e) = write_binding(&self.path, chord, *value) {
                return self.put_back(before, &format!("Could not write {chord}: {e}"));
            }
        }
        if let (_, Some(warning)) = load_keymap(Some(&self.path)) {
            return self.put_back(before, &warning);
        }
        self.undo.push(before);
        self.reload();
        self.changed = true;
        self.selected = Some(select);
        vec![PanelEvent::NeedsRedraw]
    }

    fn put_back(&mut self, before: Option<String>, why: &str) -> Vec<PanelEvent> {
        let _ = restore_config(&self.path, before.as_deref());
        notify(NotifyLevel::Error, why)
    }

    /// `ctrl+z`: the file as it was before the last change.
    pub(super) fn undo(&mut self) -> Vec<PanelEvent> {
        let Some(before) = self.undo.pop() else {
            return notify(NotifyLevel::Info, "Nothing to undo in the keybindings.");
        };
        if let Err(e) = restore_config(&self.path, before.as_deref()) {
            self.undo.push(before);
            return notify(NotifyLevel::Error, &format!("Could not undo: {e}"));
        }
        self.reload();
        self.changed = true;
        vec![PanelEvent::NeedsRedraw]
    }

    /// `ctrl+o`: `keys.toml` in an editor tab.
    pub(super) fn open_raw(&self) -> Vec<PanelEvent> {
        vec![PanelEvent::OpenFile {
            path: self.path.clone(),
            at: Position::default(),
        }]
    }

    /// The keymap as the file now says, once, after a change. The app takes
    /// it and every surface that reads the keymap moves with it.
    pub fn take_changed(&mut self) -> Option<Keymap> {
        std::mem::take(&mut self.changed).then(|| self.keymap.clone())
    }

    /// What the float's capability line says about `chord`.
    pub(super) fn capability(chord: &str) -> &'static str {
        if crate::capability::tty_owned(chord) {
            "tty-owned: a terminal sends it as tab, enter, esc or backspace"
        } else if crate::capability::needs_protocol(chord) {
            "needs kitty: only a terminal with its keyboard protocol sends it"
        } else {
            "universal: every terminal sends it"
        }
    }

    /// What the float's conflict line says: what `chord` runs now.
    pub(super) fn conflict_line(&self, chord: &str, action: Action) -> String {
        match self.keymap.action_for(chord) {
            Some(now) if now == action => "already runs this".to_string(),
            Some(now) => format!("\u{25b2} now: {}, which loses it", now.description()),
            None => "free".to_string(),
        }
    }
}

fn notify(level: NotifyLevel, message: &str) -> Vec<PanelEvent> {
    vec![PanelEvent::Notify {
        level,
        message: message.to_string(),
    }]
}
