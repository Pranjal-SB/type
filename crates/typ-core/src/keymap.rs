//! Chord string → `Action`, as data rather than control flow.
//!
//! Bindings live in a table because three things need to read them: the input
//! loop, help text, and — once the vim layer lands — a second table swapped in
//! wholesale. A `match` on `KeyCode` can be read by exactly one of those.

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow};

use crate::{Action, Direction, KeyChord, Motion};

#[derive(Debug, Clone)]
pub struct Keymap {
    /// Canonical chord string → action. `BTreeMap` so `bindings_for` and any
    /// help listing come out in a stable order rather than a hash order that
    /// changes between runs.
    bindings: BTreeMap<String, Action>,
}

/// The non-modal defaults, shaped like what someone arriving from a GUI editor
/// already has in their fingers.
const DEFAULTS: &[(&str, Action)] = &[
    (
        "left",
        Action::Move {
            motion: Motion::Left,
            extend: false,
        },
    ),
    (
        "shift+left",
        Action::Move {
            motion: Motion::Left,
            extend: true,
        },
    ),
    (
        "right",
        Action::Move {
            motion: Motion::Right,
            extend: false,
        },
    ),
    (
        "shift+right",
        Action::Move {
            motion: Motion::Right,
            extend: true,
        },
    ),
    (
        "up",
        Action::Move {
            motion: Motion::Up,
            extend: false,
        },
    ),
    (
        "shift+up",
        Action::Move {
            motion: Motion::Up,
            extend: true,
        },
    ),
    (
        "down",
        Action::Move {
            motion: Motion::Down,
            extend: false,
        },
    ),
    (
        "shift+down",
        Action::Move {
            motion: Motion::Down,
            extend: true,
        },
    ),
    (
        "ctrl+left",
        Action::Move {
            motion: Motion::WordLeft,
            extend: false,
        },
    ),
    (
        "ctrl+shift+left",
        Action::Move {
            motion: Motion::WordLeft,
            extend: true,
        },
    ),
    (
        "ctrl+right",
        Action::Move {
            motion: Motion::WordRight,
            extend: false,
        },
    ),
    (
        "ctrl+shift+right",
        Action::Move {
            motion: Motion::WordRight,
            extend: true,
        },
    ),
    (
        "home",
        Action::Move {
            motion: Motion::LineStart,
            extend: false,
        },
    ),
    (
        "shift+home",
        Action::Move {
            motion: Motion::LineStart,
            extend: true,
        },
    ),
    (
        "end",
        Action::Move {
            motion: Motion::LineEnd,
            extend: false,
        },
    ),
    (
        "shift+end",
        Action::Move {
            motion: Motion::LineEnd,
            extend: true,
        },
    ),
    (
        "pageup",
        Action::Move {
            motion: Motion::PageUp,
            extend: false,
        },
    ),
    (
        "shift+pageup",
        Action::Move {
            motion: Motion::PageUp,
            extend: true,
        },
    ),
    (
        "pagedown",
        Action::Move {
            motion: Motion::PageDown,
            extend: false,
        },
    ),
    (
        "shift+pagedown",
        Action::Move {
            motion: Motion::PageDown,
            extend: true,
        },
    ),
    (
        "ctrl+home",
        Action::Move {
            motion: Motion::DocumentStart,
            extend: false,
        },
    ),
    (
        "ctrl+shift+home",
        Action::Move {
            motion: Motion::DocumentStart,
            extend: true,
        },
    ),
    (
        "ctrl+end",
        Action::Move {
            motion: Motion::DocumentEnd,
            extend: false,
        },
    ),
    (
        "ctrl+shift+end",
        Action::Move {
            motion: Motion::DocumentEnd,
            extend: true,
        },
    ),
    (
        "backspace",
        Action::Delete {
            direction: Direction::Backward,
            by_word: false,
        },
    ),
    (
        "ctrl+backspace",
        Action::Delete {
            direction: Direction::Backward,
            by_word: true,
        },
    ),
    (
        "delete",
        Action::Delete {
            direction: Direction::Forward,
            by_word: false,
        },
    ),
    (
        "ctrl+delete",
        Action::Delete {
            direction: Direction::Forward,
            by_word: true,
        },
    ),
    ("enter", Action::InsertNewline),
    ("ctrl+z", Action::Undo),
    ("ctrl+y", Action::Redo),
    ("ctrl+a", Action::SelectAll),
    ("ctrl+l", Action::SelectLine),
    // VS Code, Sublime and ttt all put select-next-occurrence on Ctrl+D. TYPE
    // has no chord *sequences*, so Ctrl+K L for select-all is unavailable and
    // this takes VS Code's other binding for it.
    ("ctrl+d", Action::SelectNextOccurrence),
    ("ctrl+shift+l", Action::SelectAllOccurrences),
    ("esc", Action::CollapseSelections),
    ("ctrl+alt+up", Action::AddCursor(Direction::Backward)),
    ("ctrl+alt+down", Action::AddCursor(Direction::Forward)),
    ("ctrl+s", Action::Save),
    ("ctrl+q", Action::Quit),
    // Tab indents, because no code editor is usable otherwise. Focus moves to
    // F6, which browsers and IDEs already use for pane cycling and which
    // survives every terminal — Ctrl+Tab is bound too, but a terminal without
    // the kitty keyboard protocol cannot tell it apart from a bare Tab.
    //
    // Consequence, accepted: Tab does nothing in the file tree, which has no
    // indent concept and no named actions of its own yet. F6 works from both
    // panels, so nothing became unreachable. Architecture §5 already records
    // naming the tree's primitives as M4 work.
    ("tab", Action::Indent),
    ("shift+tab", Action::Outdent),
    ("f6", Action::FocusNext),
    ("ctrl+tab", Action::FocusNext),
    ("ctrl+g", Action::GotoLine),
    ("ctrl+f", Action::SearchOpen),
    // Ctrl+P is where every GUI editor puts "go to file", and it was the one
    // chord in that family still unbound.
    ("ctrl+p", Action::OpenFilePicker),
    // Ctrl+Shift+F beside Ctrl+F, the same relationship VS Code uses: the
    // buffer, then the project.
    ("ctrl+shift+f", Action::OpenProjectSearch),
    // An Enhanced-tier chord, and a documented exception rather than an
    // oversight: `controls.md` §1 says Ctrl+Shift+P cannot be the *only* way to
    // the palette in a terminal. Typing `>` into Ctrl+P is the path that always
    // works; this is here for the terminals that deliver it, the way the
    // Ctrl+Shift clipboard chords are.
    ("ctrl+shift+p", Action::OpenCommandPalette),
    // Tabs, bound twice on purpose. Ctrl+PageUp/PageDown is the spelling every
    // tabbed application uses and most terminals deliver; `controls.md` §1 does
    // not list the page keys among the universally deliverable ones, and
    // `Alt+punctuation` is. So the familiar chord and the guaranteed one both
    // ship, and neither is the only way in.
    ("ctrl+pagedown", Action::NextTab),
    ("ctrl+pageup", Action::PrevTab),
    ("alt+.", Action::NextTab),
    ("alt+,", Action::PrevTab),
    // Ctrl+W is close-tab in VS Code, Zed and every browser. It is also
    // readline's delete-word-backwards, which is the cost of the choice — the
    // prompt line is where that habit lives, and the prompt owns the keyboard
    // ahead of the keymap while it is open.
    ("ctrl+w", Action::CloseTab),
    ("alt+1", Action::GoToTab(1)),
    ("alt+2", Action::GoToTab(2)),
    ("alt+3", Action::GoToTab(3)),
    ("alt+4", Action::GoToTab(4)),
    ("alt+5", Action::GoToTab(5)),
    ("alt+6", Action::GoToTab(6)),
    ("alt+7", Action::GoToTab(7)),
    ("alt+8", Action::GoToTab(8)),
    ("alt+9", Action::GoToTab(9)),
    ("f3", Action::SearchNext),
    ("shift+f3", Action::SearchPrevious),
    ("ctrl+h", Action::ReplaceOpen),
    ("ctrl+c", Action::Copy),
    ("ctrl+x", Action::Cut),
    ("ctrl+v", Action::Paste),
    // The Insert trio, because a terminal may swallow Ctrl+C before TYPE ever
    // sees it and a user who cannot copy has no way to discover why.
    ("ctrl+insert", Action::Copy),
    ("shift+delete", Action::Cut),
    ("shift+insert", Action::Paste),
    // Bound because people reach for them, though whether they ever arrive is
    // the terminal's decision on two counts. Most emulators bind Ctrl+Shift+C/V
    // to their *own* copy and paste and never forward the key. And in the legacy
    // encoding a Ctrl+letter chord collapses to one control byte that carries no
    // shift bit at all, so the terminal could not report the difference even if
    // it wanted to — that needs the kitty keyboard protocol, which arrives with
    // capability detection at M2.5. Windows is the exception: its console API
    // reports full modifier state, so these work there today.
    //
    // Harmless where they are swallowed: the chord that does arrive is plain
    // ctrl+c, which is already bound to the same action.
    ("ctrl+shift+c", Action::Copy),
    ("ctrl+shift+x", Action::Cut),
    ("ctrl+shift+v", Action::Paste),
];

impl Keymap {
    pub fn default_bindings() -> Self {
        Self {
            bindings: DEFAULTS
                .iter()
                .map(|(chord, action)| ((*chord).to_string(), *action))
                .collect(),
        }
    }

    pub fn lookup(&self, chord: &KeyChord) -> Option<Action> {
        self.bindings.get(&chord.canonical).copied()
    }

    /// Chords bound to an action, for help text and the future palette.
    pub fn bindings_for(&self, action: Action) -> Vec<&str> {
        self.bindings
            .iter()
            .filter(|(_, a)| **a == action)
            .map(|(chord, _)| chord.as_str())
            .collect()
    }

    /// Apply a user config over the current bindings.
    ///
    /// Parsed into a staging list first, so a config with one bad line changes
    /// nothing. A half-applied keymap is worse than a rejected one: the user
    /// cannot tell which half took effect.
    pub fn merge_toml(&mut self, src: &str) -> Result<()> {
        let table: BTreeMap<String, String> =
            toml::from_str(src).context("parsing the keybinding table")?;

        let mut staged: Vec<(String, Option<Action>)> = Vec::new();
        for (chord, action_name) in table {
            if action_name.is_empty() {
                // An empty action unbinds, which a user needs in order to free
                // a chord their terminal or window manager wants for itself.
                staged.push((chord, None));
                continue;
            }
            let action = Action::from_name(&action_name)
                .ok_or_else(|| anyhow!("{chord} is bound to an unknown action: {action_name}"))?;
            staged.push((chord, Some(action)));
        }

        for (chord, action) in staged {
            match action {
                Some(action) => {
                    self.bindings.insert(chord, action);
                }
                None => {
                    self.bindings.remove(&chord);
                }
            }
        }
        Ok(())
    }
}

impl Default for Keymap {
    fn default() -> Self {
        Self::default_bindings()
    }
}
