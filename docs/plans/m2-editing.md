# M2 (Editing) — Implementation Plan

**How to use this plan:** tasks are ordered and each ends with a commit. Work them in
sequence; each one leaves the tree in a working, testable state. Checkboxes track progress.

**Goal:** Turn the walking skeleton into an editor someone would choose to use — selections,
multiple cursors, word-wise motion, search and replace — with every editing primitive
reachable as a **named action**, and every key bound through a **data table** rather than a
match arm.

**Architecture:** Two structural pieces land before any feature. `Action` is a closed enum of
named editing operations with explicit arguments; `Keymap` maps a canonical chord string to
an `Action` and loads from TOML. Everything else in this milestone is expressed through them.
The cursor becomes `Selections` — a non-empty, sorted, non-overlapping list — from the first
task that touches it, so no editing path is ever written single-cursor and rewritten later.

**Tech stack:** Rust 1.96 (edition 2024) · ratatui 0.30.2 · crossterm 0.29 · ropey 1.6 ·
unicode-width 0.2 · unicode-segmentation 1.11 · anyhow 1.0 · toml (new)

**Spec:** [`docs/design/architecture.md`](../design/architecture.md) — §4 product principles
(mouse/keyboard parity, modal editing as a setting), §5 the `Panel` contract, §7 render and
input model.

**Prior plan:** [`m0-m1-foundation.md`](m0-m1-foundation.md). M1, M1.1 and M1.2 are complete
and merged; `main` builds an editor that opens, edits and saves files with a single cursor.

## Global constraints

Every task's requirements implicitly include this section.

- **Binary name is `typ`**, crate name is `typ-editor`, project name is TYPE. Never ship a
  binary named `type` — it collides with the POSIX shell builtin.
- **Per-file cap: 800 lines.** If a file approaches it, split by responsibility first.
- **Nothing blocks the render thread.** I/O, parsing, and subprocess work happen off-thread
  and return as events.
- **Panels never receive `&AppState`.** They get `RenderContext` and return `Vec<PanelEvent>`.
- **`PanelEvent` stays small.** New viewers register a handler in `typ-registry` and route
  through `OpenWith`; they do not add enum variants.
- **Mouse and keyboard are peers.** Every interaction works both ways.
- **Every editing primitive is an `Action`.** No `handle_key` arm may mutate a buffer
  directly. Three consumers depend on this — the keymap, the future command palette, and the
  future vim layer — and a primitive reachable only from a key handler is invisible to all
  three.
- **`col` is always a grapheme index**, never a byte or char offset.
- **Conventional Commits.** Single author — no co-author trailers.

## What M2 does not include

Tree-sitter highlighting and the command palette move to **M2.5**, planned separately. This
milestone stops at editing. No LSP (M3), no splits or tabs or sessions (M4), no terminal panel
or git (M5). The vim layer is **not** built here — M2 only guarantees it is buildable without
touching the editor core, by making every primitive an `Action` and every binding a table row.

---

## File structure

### `typ-core` — vocabulary

| File | Responsibility |
|---|---|
| `src/action.rs` | The `Action` enum and its name parsing. No behavior. |
| `src/keymap.rs` | Chord string → `Action`, defaults, TOML merge. |

### `typ-buffer` — text primitives

| File | Responsibility |
|---|---|
| `src/selection.rs` | `Selection`, `Selections` — ordering, merging, the primary index. |
| `src/word.rs` | Word-boundary classification and scanning. |
| `src/search.rs` | Literal search over the rope, returning positions. |

### `typ-panel-editor` — the editor

| File | Responsibility |
|---|---|
| `src/lib.rs` | `EditorPanel` — state, `Panel` impl, render. |
| `src/actions.rs` | `EditorPanel::apply_action` — motions, edits, cursor management. |
| `src/render.rs` | Line-to-`Line` conversion, selection spans, horizontal windowing. |

### `typ-app` — routing

| File | Responsibility |
|---|---|
| `src/app.rs` | Focus, dispatch, status bar, prompt state. |
| `src/prompt.rs` | The status-bar prompt: search and replace input. |
| `src/config.rs` | Locating and loading `keys.toml`. |

---

### Task 1: The `Action` vocabulary

**Files:**
- Create: `crates/typ-core/src/action.rs`, `crates/typ-core/tests/action.rs`
- Modify: `crates/typ-core/src/lib.rs`

**Interfaces:**
- Consumes: nothing
- Produces:
  - `typ_core::Action` — a `Copy` enum, every editing primitive TYPE has
  - `typ_core::Action::from_name(&str) -> Option<Action>`
  - `typ_core::Action::name(&self) -> &'static str`
  - `typ_core::Motion`, `typ_core::Direction`

- [ ] **Step 1: Write the failing test**

`crates/typ-core/tests/action.rs`:

```rust
use typ_core::{Action, Direction, Motion};

#[test]
fn actions_round_trip_through_their_names() {
    for action in Action::ALL {
        assert_eq!(
            Action::from_name(action.name()),
            Some(*action),
            "{} did not round-trip",
            action.name()
        );
    }
}

#[test]
fn names_are_snake_case_and_unique() {
    let mut seen = std::collections::HashSet::new();
    for action in Action::ALL {
        let name = action.name();
        assert!(
            name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{name} is not snake_case"
        );
        assert!(seen.insert(name), "{name} is used twice");
    }
}

#[test]
fn an_unknown_name_is_rejected_rather_than_guessed() {
    assert_eq!(Action::from_name("move_sideways"), None);
    assert_eq!(Action::from_name(""), None);
}

#[test]
fn every_motion_exists_in_both_moving_and_extending_form() {
    for motion in Motion::ALL {
        let moving = Action::Move {
            motion: *motion,
            extend: false,
        };
        let extending = Action::Move {
            motion: *motion,
            extend: true,
        };
        assert_ne!(moving.name(), extending.name());
        assert_eq!(Action::from_name(moving.name()), Some(moving));
        assert_eq!(Action::from_name(extending.name()), Some(extending));
    }
}

#[test]
fn insert_char_is_not_nameable() {
    // Typed text arrives as a key event, not as a binding. If it were
    // nameable, a config file could bind a key to inserting a different
    // character, which is a text-substitution feature, not a keybinding.
    assert_eq!(Action::from_name("insert_char"), None);
}

#[test]
fn directions_are_explicit_arguments_not_separate_actions() {
    let back = Action::Delete {
        direction: Direction::Backward,
        by_word: false,
    };
    let forward = Action::Delete {
        direction: Direction::Forward,
        by_word: false,
    };
    assert_ne!(back, forward);
    assert_eq!(Action::from_name("delete_backward"), Some(back));
    assert_eq!(Action::from_name("delete_forward"), Some(forward));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p typ-core --test action`

Expected: FAIL — `unresolved imports typ_core::Action, typ_core::Direction, typ_core::Motion`.

- [ ] **Step 3: Write the implementation**

`crates/typ-core/src/action.rs`:

```rust
//! The named vocabulary of editing operations.
//!
//! Every editing primitive in TYPE is an `Action` with explicit arguments.
//! Three consumers depend on that: the keymap, the command palette, and the
//! opt-in vim layer. A primitive reachable only from a `handle_key` arm is
//! invisible to all three, so no key handler may mutate a buffer directly.

/// Which way an operation runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direction {
    Backward,
    Forward,
}

/// Where a motion lands. Motions carry no "extend" flag themselves — that is
/// an argument of `Action::Move`, so every motion is automatically available
/// in both forms rather than being listed twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    PageUp,
    PageDown,
    DocumentStart,
    DocumentEnd,
}

impl Motion {
    pub const ALL: &'static [Motion] = &[
        Motion::Left,
        Motion::Right,
        Motion::Up,
        Motion::Down,
        Motion::WordLeft,
        Motion::WordRight,
        Motion::LineStart,
        Motion::LineEnd,
        Motion::PageUp,
        Motion::PageDown,
        Motion::DocumentStart,
        Motion::DocumentEnd,
    ];

    const fn stem(self) -> &'static str {
        match self {
            Motion::Left => "left",
            Motion::Right => "right",
            Motion::Up => "up",
            Motion::Down => "down",
            Motion::WordLeft => "word_left",
            Motion::WordRight => "word_right",
            Motion::LineStart => "line_start",
            Motion::LineEnd => "line_end",
            Motion::PageUp => "page_up",
            Motion::PageDown => "page_down",
            Motion::DocumentStart => "document_start",
            Motion::DocumentEnd => "document_end",
        }
    }
}

/// A named editing operation.
///
/// `InsertChar` is deliberately absent from `ALL` and unnameable: typed text
/// arrives as a key event, not as a binding, and a bindable "insert this
/// character" would be a text-substitution feature wearing a keybinding's
/// clothes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Move { motion: Motion, extend: bool },
    Delete { direction: Direction, by_word: bool },
    InsertNewline,
    InsertChar(char),
    Undo,
    Redo,
    SelectAll,
    SelectLine,
    CollapseSelections,
    AddCursor(Direction),
    Save,
    Quit,
    FocusNext,
    SearchOpen,
    SearchNext,
    SearchPrevious,
    ReplaceOpen,
}

impl Action {
    /// Every action a config file may name, in a stable order.
    pub const ALL: &'static [Action] = &[
        Action::Move { motion: Motion::Left, extend: false },
        Action::Move { motion: Motion::Left, extend: true },
        Action::Move { motion: Motion::Right, extend: false },
        Action::Move { motion: Motion::Right, extend: true },
        Action::Move { motion: Motion::Up, extend: false },
        Action::Move { motion: Motion::Up, extend: true },
        Action::Move { motion: Motion::Down, extend: false },
        Action::Move { motion: Motion::Down, extend: true },
        Action::Move { motion: Motion::WordLeft, extend: false },
        Action::Move { motion: Motion::WordLeft, extend: true },
        Action::Move { motion: Motion::WordRight, extend: false },
        Action::Move { motion: Motion::WordRight, extend: true },
        Action::Move { motion: Motion::LineStart, extend: false },
        Action::Move { motion: Motion::LineStart, extend: true },
        Action::Move { motion: Motion::LineEnd, extend: false },
        Action::Move { motion: Motion::LineEnd, extend: true },
        Action::Move { motion: Motion::PageUp, extend: false },
        Action::Move { motion: Motion::PageUp, extend: true },
        Action::Move { motion: Motion::PageDown, extend: false },
        Action::Move { motion: Motion::PageDown, extend: true },
        Action::Move { motion: Motion::DocumentStart, extend: false },
        Action::Move { motion: Motion::DocumentStart, extend: true },
        Action::Move { motion: Motion::DocumentEnd, extend: false },
        Action::Move { motion: Motion::DocumentEnd, extend: true },
        Action::Delete { direction: Direction::Backward, by_word: false },
        Action::Delete { direction: Direction::Backward, by_word: true },
        Action::Delete { direction: Direction::Forward, by_word: false },
        Action::Delete { direction: Direction::Forward, by_word: true },
        Action::InsertNewline,
        Action::Undo,
        Action::Redo,
        Action::SelectAll,
        Action::SelectLine,
        Action::CollapseSelections,
        Action::AddCursor(Direction::Backward),
        Action::AddCursor(Direction::Forward),
        Action::Save,
        Action::Quit,
        Action::FocusNext,
        Action::SearchOpen,
        Action::SearchNext,
        Action::SearchPrevious,
        Action::ReplaceOpen,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Action::Move { motion, extend } => {
                // The name is a compile-time pairing rather than a runtime
                // format!, so it can be returned as &'static str and compared
                // without allocating on every keypress.
                match (motion, extend) {
                    (Motion::Left, false) => "move_left",
                    (Motion::Left, true) => "extend_left",
                    (Motion::Right, false) => "move_right",
                    (Motion::Right, true) => "extend_right",
                    (Motion::Up, false) => "move_up",
                    (Motion::Up, true) => "extend_up",
                    (Motion::Down, false) => "move_down",
                    (Motion::Down, true) => "extend_down",
                    (Motion::WordLeft, false) => "move_word_left",
                    (Motion::WordLeft, true) => "extend_word_left",
                    (Motion::WordRight, false) => "move_word_right",
                    (Motion::WordRight, true) => "extend_word_right",
                    (Motion::LineStart, false) => "move_line_start",
                    (Motion::LineStart, true) => "extend_line_start",
                    (Motion::LineEnd, false) => "move_line_end",
                    (Motion::LineEnd, true) => "extend_line_end",
                    (Motion::PageUp, false) => "move_page_up",
                    (Motion::PageUp, true) => "extend_page_up",
                    (Motion::PageDown, false) => "move_page_down",
                    (Motion::PageDown, true) => "extend_page_down",
                    (Motion::DocumentStart, false) => "move_document_start",
                    (Motion::DocumentStart, true) => "extend_document_start",
                    (Motion::DocumentEnd, false) => "move_document_end",
                    (Motion::DocumentEnd, true) => "extend_document_end",
                }
            }
            Action::Delete { direction: Direction::Backward, by_word: false } => "delete_backward",
            Action::Delete { direction: Direction::Backward, by_word: true } => {
                "delete_word_backward"
            }
            Action::Delete { direction: Direction::Forward, by_word: false } => "delete_forward",
            Action::Delete { direction: Direction::Forward, by_word: true } => {
                "delete_word_forward"
            }
            Action::InsertNewline => "insert_newline",
            // Unreachable through from_name; see the type docs.
            Action::InsertChar(_) => "insert_char_literal",
            Action::Undo => "undo",
            Action::Redo => "redo",
            Action::SelectAll => "select_all",
            Action::SelectLine => "select_line",
            Action::CollapseSelections => "collapse_selections",
            Action::AddCursor(Direction::Backward) => "add_cursor_above",
            Action::AddCursor(Direction::Forward) => "add_cursor_below",
            Action::Save => "save",
            Action::Quit => "quit",
            Action::FocusNext => "focus_next",
            Action::SearchOpen => "search_open",
            Action::SearchNext => "search_next",
            Action::SearchPrevious => "search_previous",
            Action::ReplaceOpen => "replace_open",
        }
    }

    /// Look an action up by the name a config file uses.
    ///
    /// Linear over ~40 entries and called once per keymap load, never per
    /// keypress — a map would be more code for no measurable gain.
    pub fn from_name(name: &str) -> Option<Action> {
        Action::ALL.iter().copied().find(|a| a.name() == name)
    }
}
```

Then in `crates/typ-core/src/lib.rs`, add the module and re-export beside the existing ones:

```rust
pub mod action;

pub use action::{Action, Direction, Motion};
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p typ-core --test action`

Expected: PASS, 6 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/typ-core/src/action.rs crates/typ-core/src/lib.rs crates/typ-core/tests/action.rs
git commit -m "feat(core): name every editing primitive as an Action"
```

---

### Task 2: The keymap table

**Files:**
- Create: `crates/typ-core/src/keymap.rs`, `crates/typ-core/tests/keymap.rs`
- Modify: `crates/typ-core/src/lib.rs`, `crates/typ-core/Cargo.toml`

**Interfaces:**
- Consumes: `typ_core::{Action, KeyChord}`
- Produces:
  - `typ_core::Keymap::default_bindings() -> Keymap`
  - `Keymap::lookup(&self, chord: &KeyChord) -> Option<Action>`
  - `Keymap::merge_toml(&mut self, src: &str) -> anyhow::Result<()>`
  - `Keymap::bindings_for(&self, action: Action) -> Vec<&str>`

- [ ] **Step 1: Add the dependencies**

`typ-core` currently depends on only `crossterm` and `ratatui`. `merge_toml` needs both a TOML
parser and `anyhow`, which the crate does not yet have:

```bash
cargo add toml --package typ-core
```

`anyhow` is already in `[workspace.dependencies]`, so add it by hand to
`crates/typ-core/Cargo.toml` as `anyhow.workspace = true`. Then move the `toml` version cargo
picked into `[workspace.dependencies]` in the root `Cargo.toml` and replace the crate entry
with `toml.workspace = true`, matching how every other dependency here is declared.

The result:

```toml
[dependencies]
anyhow.workspace = true
crossterm.workspace = true
ratatui.workspace = true
toml.workspace = true
```

- [ ] **Step 2: Write the failing test**

`crates/typ-core/tests/keymap.rs`:

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_core::{Action, Direction, KeyChord, Keymap, Motion};

fn chord(code: KeyCode, mods: KeyModifiers) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, mods))
}

#[test]
fn the_defaults_bind_the_arrows() {
    let keymap = Keymap::default_bindings();
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Left, KeyModifiers::NONE)),
        Some(Action::Move { motion: Motion::Left, extend: false })
    );
}

#[test]
fn shift_extends_the_selection_rather_than_moving() {
    let keymap = Keymap::default_bindings();
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Left, KeyModifiers::SHIFT)),
        Some(Action::Move { motion: Motion::Left, extend: true })
    );
}

#[test]
fn ctrl_arrows_move_by_word() {
    let keymap = Keymap::default_bindings();
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Right, KeyModifiers::CONTROL)),
        Some(Action::Move { motion: Motion::WordRight, extend: false })
    );
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Right, KeyModifiers::CONTROL | KeyModifiers::SHIFT)),
        Some(Action::Move { motion: Motion::WordRight, extend: true })
    );
}

#[test]
fn an_unbound_chord_returns_nothing() {
    let keymap = Keymap::default_bindings();
    assert_eq!(keymap.lookup(&chord(KeyCode::F(12), KeyModifiers::NONE)), None);
}

#[test]
fn config_overrides_a_default_binding() {
    let mut keymap = Keymap::default_bindings();
    keymap.merge_toml("\"ctrl+d\" = \"delete_forward\"").unwrap();
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        Some(Action::Delete { direction: Direction::Forward, by_word: false })
    );
}

#[test]
fn config_can_unbind_a_key_with_an_empty_action() {
    let mut keymap = Keymap::default_bindings();
    keymap.merge_toml("\"ctrl+z\" = \"\"").unwrap();
    assert_eq!(keymap.lookup(&chord(KeyCode::Char('z'), KeyModifiers::CONTROL)), None);
}

#[test]
fn an_unknown_action_name_is_an_error_naming_the_action() {
    let mut keymap = Keymap::default_bindings();
    let err = keymap.merge_toml("\"ctrl+k\" = \"summon_daemon\"").unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("summon_daemon"), "error was: {text}");
    assert!(text.contains("ctrl+k"), "error was: {text}");
}

#[test]
fn malformed_toml_is_an_error_not_a_panic() {
    let mut keymap = Keymap::default_bindings();
    assert!(keymap.merge_toml("this is not toml = = =").is_err());
}

#[test]
fn a_rejected_config_leaves_the_previous_bindings_intact() {
    let mut keymap = Keymap::default_bindings();
    let _ = keymap.merge_toml("\"ctrl+k\" = \"summon_daemon\"");
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Left, KeyModifiers::NONE)),
        Some(Action::Move { motion: Motion::Left, extend: false })
    );
}

#[test]
fn bindings_can_be_looked_up_backwards_for_help_text() {
    let keymap = Keymap::default_bindings();
    let bindings = keymap.bindings_for(Action::Save);
    assert!(bindings.contains(&"ctrl+s"), "bindings were: {bindings:?}");
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p typ-core --test keymap`

Expected: FAIL — `unresolved import typ_core::Keymap`.

- [ ] **Step 4: Write the implementation**

`crates/typ-core/src/keymap.rs`:

```rust
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

/// The non-modal defaults. Shape borrowed from what someone arriving from a
/// GUI editor already has in their fingers.
const DEFAULTS: &[(&str, Action)] = &[
    ("left", Action::Move { motion: Motion::Left, extend: false }),
    ("shift+left", Action::Move { motion: Motion::Left, extend: true }),
    ("right", Action::Move { motion: Motion::Right, extend: false }),
    ("shift+right", Action::Move { motion: Motion::Right, extend: true }),
    ("up", Action::Move { motion: Motion::Up, extend: false }),
    ("shift+up", Action::Move { motion: Motion::Up, extend: true }),
    ("down", Action::Move { motion: Motion::Down, extend: false }),
    ("shift+down", Action::Move { motion: Motion::Down, extend: true }),
    ("ctrl+left", Action::Move { motion: Motion::WordLeft, extend: false }),
    ("ctrl+shift+left", Action::Move { motion: Motion::WordLeft, extend: true }),
    ("ctrl+right", Action::Move { motion: Motion::WordRight, extend: false }),
    ("ctrl+shift+right", Action::Move { motion: Motion::WordRight, extend: true }),
    ("home", Action::Move { motion: Motion::LineStart, extend: false }),
    ("shift+home", Action::Move { motion: Motion::LineStart, extend: true }),
    ("end", Action::Move { motion: Motion::LineEnd, extend: false }),
    ("shift+end", Action::Move { motion: Motion::LineEnd, extend: true }),
    ("pageup", Action::Move { motion: Motion::PageUp, extend: false }),
    ("shift+pageup", Action::Move { motion: Motion::PageUp, extend: true }),
    ("pagedown", Action::Move { motion: Motion::PageDown, extend: false }),
    ("shift+pagedown", Action::Move { motion: Motion::PageDown, extend: true }),
    ("ctrl+home", Action::Move { motion: Motion::DocumentStart, extend: false }),
    ("ctrl+shift+home", Action::Move { motion: Motion::DocumentStart, extend: true }),
    ("ctrl+end", Action::Move { motion: Motion::DocumentEnd, extend: false }),
    ("ctrl+shift+end", Action::Move { motion: Motion::DocumentEnd, extend: true }),
    ("backspace", Action::Delete { direction: Direction::Backward, by_word: false }),
    ("ctrl+backspace", Action::Delete { direction: Direction::Backward, by_word: true }),
    ("delete", Action::Delete { direction: Direction::Forward, by_word: false }),
    ("ctrl+delete", Action::Delete { direction: Direction::Forward, by_word: true }),
    ("enter", Action::InsertNewline),
    ("ctrl+z", Action::Undo),
    ("ctrl+y", Action::Redo),
    ("ctrl+a", Action::SelectAll),
    ("ctrl+l", Action::SelectLine),
    ("esc", Action::CollapseSelections),
    ("ctrl+alt+up", Action::AddCursor(Direction::Backward)),
    ("ctrl+alt+down", Action::AddCursor(Direction::Forward)),
    ("ctrl+s", Action::Save),
    ("ctrl+q", Action::Quit),
    ("tab", Action::FocusNext),
    ("ctrl+f", Action::SearchOpen),
    ("f3", Action::SearchNext),
    ("shift+f3", Action::SearchPrevious),
    ("ctrl+h", Action::ReplaceOpen),
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
    /// Parsed into a staging map first so a config with one bad line changes
    /// nothing — a half-applied keymap is worse than a rejected one, because
    /// the user cannot tell which half took effect.
    pub fn merge_toml(&mut self, src: &str) -> Result<()> {
        let table: BTreeMap<String, String> =
            toml::from_str(src).context("parsing the keybinding table")?;

        let mut staged: Vec<(String, Option<Action>)> = Vec::new();
        for (chord, action_name) in table {
            if action_name.is_empty() {
                // An empty action unbinds, which a user needs in order to free
                // a chord their terminal or window manager wants.
                staged.push((chord, None));
                continue;
            }
            let action = Action::from_name(&action_name).ok_or_else(|| {
                anyhow!("{chord} is bound to an unknown action: {action_name}")
            })?;
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
```

Add to `crates/typ-core/src/lib.rs`:

```rust
pub mod keymap;

pub use keymap::Keymap;
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p typ-core --test keymap`

Expected: PASS, 10 tests.

If `shift+left` fails to look up, check `KeyChord::from_event` — crossterm reports
`KeyModifiers::SHIFT` on arrow keys but folds shift into the character for letters, which is
why the default table has no `shift+<letter>` entries.

- [ ] **Step 6: Commit**

```bash
git add crates/typ-core/src/keymap.rs crates/typ-core/src/lib.rs \
  crates/typ-core/tests/keymap.rs crates/typ-core/Cargo.toml Cargo.toml Cargo.lock
git commit -m "feat(core): keybindings as a data table with TOML overrides"
```

---

### Task 3: `Selection` and `Selections`

**Files:**
- Create: `crates/typ-buffer/src/selection.rs`, `crates/typ-buffer/tests/selection.rs`
- Modify: `crates/typ-buffer/src/lib.rs`

**Interfaces:**
- Consumes: `typ_buffer::Position`
- Produces:
  - `typ_buffer::Selection { anchor, head }` with `caret`, `is_empty`, `range`, `contains`
  - `typ_buffer::Selections` with `primary`, `iter`, `len`, `push`, `set_single`,
    `map_in_place`, `collapse_to_heads`

- [ ] **Step 1: Write the failing test**

`crates/typ-buffer/tests/selection.rs`:

```rust
use typ_buffer::{Position, Selection, Selections};

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

#[test]
fn a_caret_is_an_empty_selection() {
    let s = Selection::caret(pos(1, 4));
    assert!(s.is_empty());
    assert_eq!(s.anchor, s.head);
}

#[test]
fn range_returns_the_endpoints_in_document_order() {
    // Selected leftwards: the head is before the anchor.
    let s = Selection { anchor: pos(2, 5), head: pos(1, 0) };
    assert_eq!(s.range(), (pos(1, 0), pos(2, 5)));
}

#[test]
fn contains_is_half_open_so_touching_selections_do_not_overlap() {
    let s = Selection { anchor: pos(0, 2), head: pos(0, 5) };
    assert!(s.contains(pos(0, 2)));
    assert!(s.contains(pos(0, 4)));
    assert!(!s.contains(pos(0, 5)), "the end is exclusive");
}

#[test]
fn selections_always_hold_at_least_one() {
    let s = Selections::default();
    assert_eq!(s.len(), 1);
    assert_eq!(s.primary().head, pos(0, 0));
}

#[test]
fn the_primary_is_the_one_most_recently_added() {
    let mut s = Selections::default();
    s.push(Selection::caret(pos(5, 0)));
    assert_eq!(s.primary().head, pos(5, 0));
}

#[test]
fn selections_are_kept_in_document_order() {
    let mut s = Selections::default();
    s.push(Selection::caret(pos(9, 0)));
    s.push(Selection::caret(pos(4, 0)));
    let lines: Vec<usize> = s.iter().map(|sel| sel.head.line).collect();
    assert_eq!(lines, vec![0, 4, 9]);
}

#[test]
fn the_primary_survives_reordering() {
    let mut s = Selections::default();
    s.push(Selection::caret(pos(9, 0)));
    s.push(Selection::caret(pos(4, 0)));
    // Added last, so still primary even though it sorted into the middle.
    assert_eq!(s.primary().head, pos(4, 0));
}

#[test]
fn overlapping_selections_merge_into_one() {
    let mut s = Selections::default();
    s.set_single(Selection { anchor: pos(0, 0), head: pos(0, 6) });
    s.push(Selection { anchor: pos(0, 4), head: pos(0, 9) });
    assert_eq!(s.len(), 1);
    assert_eq!(s.iter().next().unwrap().range(), (pos(0, 0), pos(0, 9)));
}

#[test]
fn adjacent_selections_stay_separate() {
    let mut s = Selections::default();
    s.set_single(Selection { anchor: pos(0, 0), head: pos(0, 3) });
    s.push(Selection { anchor: pos(0, 3), head: pos(0, 6) });
    assert_eq!(s.len(), 2, "touching is not overlapping");
}

#[test]
fn collapse_to_heads_drops_everything_but_the_primary_caret() {
    let mut s = Selections::default();
    s.push(Selection { anchor: pos(2, 0), head: pos(2, 4) });
    s.collapse_to_heads();
    assert_eq!(s.len(), 1);
    assert_eq!(s.primary().head, pos(2, 4));
    assert!(s.primary().is_empty());
}

#[test]
fn map_in_place_rewrites_every_selection_then_restores_the_invariants() {
    let mut s = Selections::default();
    s.push(Selection::caret(pos(2, 0)));
    // Move everything to the same place; they must merge rather than pile up.
    s.map_in_place(|_| Selection::caret(pos(1, 1)));
    assert_eq!(s.len(), 1);
    assert_eq!(s.primary().head, pos(1, 1));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p typ-buffer --test selection`

Expected: FAIL — `unresolved imports typ_buffer::Selection, typ_buffer::Selections`.

- [ ] **Step 3: Write the implementation**

`crates/typ-buffer/src/selection.rs`:

```rust
//! Cursors and selections.
//!
//! There is no single-cursor type. A caret is an empty selection, and the
//! editor always holds a `Selections` — with one entry in the common case.
//! Adding multi-cursor later would mean rewriting every editing path twice:
//! once to add the concept, once to undo what the single-cursor assumption
//! baked in.

use crate::position::Position;

/// A range of text with a fixed `anchor` and a moving `head`.
///
/// The head is where the cursor is drawn and where typing happens. Extending
/// moves the head and leaves the anchor, which is what makes shift+arrow grow
/// and shrink from the end the user expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: Position,
    pub head: Position,
}

impl Selection {
    pub fn caret(at: Position) -> Self {
        Self { anchor: at, head: at }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    /// The endpoints in document order, regardless of which way it was made.
    pub fn range(&self) -> (Position, Position) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// Half-open: the start is inside, the end is not.
    ///
    /// That is what makes two selections which merely touch — one ending where
    /// the next begins — stay separate instead of merging.
    pub fn contains(&self, pos: Position) -> bool {
        let (start, end) = self.range();
        pos >= start && pos < end
    }
}

impl Default for Selection {
    fn default() -> Self {
        Self::caret(Position::default())
    }
}

/// A non-empty, document-ordered, non-overlapping set of selections.
///
/// Every mutating method ends by restoring those invariants, so no editing
/// code has to defend against an out-of-order or overlapping set.
#[derive(Debug, Clone)]
pub struct Selections {
    list: Vec<Selection>,
    /// Index into `list`, retargeted after each sort so the selection the user
    /// is steering stays the one they added.
    primary: usize,
}

impl Default for Selections {
    fn default() -> Self {
        Self { list: vec![Selection::default()], primary: 0 }
    }
}

impl Selections {
    pub fn single(selection: Selection) -> Self {
        Self { list: vec![selection], primary: 0 }
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Always false — the type's invariant. Present because clippy expects it
    /// next to `len`, and because a caller reading the API should see the
    /// guarantee stated rather than inferred.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn primary(&self) -> Selection {
        self.list[self.primary]
    }

    pub fn iter(&self) -> impl Iterator<Item = &Selection> {
        self.list.iter()
    }

    /// Replace everything with one selection.
    pub fn set_single(&mut self, selection: Selection) {
        self.list = vec![selection];
        self.primary = 0;
    }

    /// Add a selection and make it primary.
    pub fn push(&mut self, selection: Selection) {
        self.list.push(selection);
        self.primary = self.list.len() - 1;
        self.normalize();
    }

    /// Rewrite every selection, then restore the invariants.
    pub fn map_in_place(&mut self, mut f: impl FnMut(Selection) -> Selection) {
        for selection in &mut self.list {
            *selection = f(*selection);
        }
        self.normalize();
    }

    /// Drop every selection but the primary, and reduce it to its head.
    pub fn collapse_to_heads(&mut self) {
        let head = self.primary().head;
        self.set_single(Selection::caret(head));
    }

    fn normalize(&mut self) {
        let primary = self.list[self.primary];
        self.list.sort_by_key(|s| s.range());

        let mut merged: Vec<Selection> = Vec::with_capacity(self.list.len());
        for selection in self.list.drain(..) {
            match merged.last_mut() {
                Some(previous) if overlaps(*previous, selection) => {
                    *previous = union(*previous, selection);
                }
                _ => merged.push(selection),
            }
        }
        self.list = merged;

        // The primary may have been merged into a larger selection, so look for
        // whichever one now covers where it was rather than trusting an index.
        self.primary = self
            .list
            .iter()
            .position(|s| *s == primary || covers(*s, primary))
            .unwrap_or(0);
    }
}

fn overlaps(a: Selection, b: Selection) -> bool {
    let (_, a_end) = a.range();
    let (b_start, _) = b.range();
    // Strictly greater, so selections that only touch stay separate — the same
    // rule as `Selection::contains` being half-open.
    a_end > b_start
}

fn union(a: Selection, b: Selection) -> Selection {
    let (a_start, a_end) = a.range();
    let (b_start, b_end) = b.range();
    Selection { anchor: a_start.min(b_start), head: a_end.max(b_end) }
}

fn covers(outer: Selection, inner: Selection) -> bool {
    let (o_start, o_end) = outer.range();
    let (i_start, i_end) = inner.range();
    o_start <= i_start && i_end <= o_end
}
```

Add to `crates/typ-buffer/src/lib.rs`:

```rust
pub mod selection;

pub use selection::{Selection, Selections};
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p typ-buffer --test selection`

Expected: PASS, 11 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/typ-buffer/src/selection.rs crates/typ-buffer/src/lib.rs crates/typ-buffer/tests/selection.rs
git commit -m "feat(buffer): selections with a primary, ordered and non-overlapping"
```

---

### Task 4: Word boundaries

**Files:**
- Create: `crates/typ-buffer/src/word.rs`, `crates/typ-buffer/tests/word.rs`
- Modify: `crates/typ-buffer/src/lib.rs`

**Interfaces:**
- Consumes: nothing
- Produces:
  - `typ_buffer::next_word_boundary(line: &str, col: usize) -> usize`
  - `typ_buffer::previous_word_boundary(line: &str, col: usize) -> usize`
  - `typ_buffer::word_at(line: &str, col: usize) -> Option<(usize, usize)>`

- [ ] **Step 1: Write the failing test**

`crates/typ-buffer/tests/word.rs`:

```rust
use typ_buffer::{next_word_boundary, previous_word_boundary, word_at};

#[test]
fn next_boundary_stops_at_the_end_of_a_word() {
    assert_eq!(next_word_boundary("hello world", 0), 5);
}

#[test]
fn next_boundary_skips_leading_whitespace_then_the_word() {
    assert_eq!(next_word_boundary("hello world", 5), 11);
}

#[test]
fn punctuation_is_its_own_run() {
    // Moving off "foo" lands between word and punctuation, not past both.
    assert_eq!(next_word_boundary("foo::bar", 0), 3);
    assert_eq!(next_word_boundary("foo::bar", 3), 5);
    assert_eq!(next_word_boundary("foo::bar", 5), 8);
}

#[test]
fn next_boundary_clamps_at_the_end_of_the_line() {
    assert_eq!(next_word_boundary("abc", 3), 3);
    assert_eq!(next_word_boundary("", 0), 0);
}

#[test]
fn previous_boundary_stops_at_the_start_of_a_word() {
    assert_eq!(previous_word_boundary("hello world", 11), 6);
    assert_eq!(previous_word_boundary("hello world", 6), 0);
}

#[test]
fn previous_boundary_clamps_at_the_start_of_the_line() {
    assert_eq!(previous_word_boundary("abc", 0), 0);
}

#[test]
fn boundaries_count_graphemes_not_bytes() {
    // Three CJK graphemes, a space, then a word.
    assert_eq!(next_word_boundary("日本語 ok", 0), 3);
    assert_eq!(previous_word_boundary("日本語 ok", 4), 4);
}

#[test]
fn word_at_returns_the_run_under_the_cursor() {
    assert_eq!(word_at("let value = 1;", 4), Some((4, 9)));
}

#[test]
fn word_at_returns_nothing_in_whitespace() {
    assert_eq!(word_at("a  b", 1), None);
}

#[test]
fn a_cursor_just_past_a_word_is_still_on_it() {
    assert_eq!(word_at("abc", 3), Some((0, 3)));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p typ-buffer --test word`

Expected: FAIL — unresolved imports.

- [ ] **Step 3: Write the implementation**

`crates/typ-buffer/src/word.rs`:

```rust
//! Word-wise motion.
//!
//! Everything here indexes graphemes, never bytes or chars, so `Ctrl+Left`
//! through CJK or emoji moves in the same units the cursor does.

use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Whitespace,
    Word,
    Punctuation,
}

/// Punctuation is its own class rather than being lumped in with words, so
/// `foo::bar` is four stops instead of one — which is what makes word motion
/// useful in code rather than only in prose.
fn class(grapheme: &str) -> Class {
    let Some(c) = grapheme.chars().next() else {
        return Class::Whitespace;
    };
    if c.is_whitespace() {
        Class::Whitespace
    } else if c.is_alphanumeric() || c == '_' {
        Class::Word
    } else {
        Class::Punctuation
    }
}

fn classes(line: &str) -> Vec<Class> {
    line.graphemes(true).map(class).collect()
}

/// The next boundary at or after `col`: skip whitespace, then consume one run
/// of like-classed graphemes.
pub fn next_word_boundary(line: &str, col: usize) -> usize {
    let classes = classes(line);
    let len = classes.len();
    let mut i = col.min(len);

    while i < len && classes[i] == Class::Whitespace {
        i += 1;
    }
    if i >= len {
        return len;
    }
    let run = classes[i];
    while i < len && classes[i] == run {
        i += 1;
    }
    i
}

/// The previous boundary at or before `col`, mirroring `next_word_boundary`.
pub fn previous_word_boundary(line: &str, col: usize) -> usize {
    let classes = classes(line);
    let mut i = col.min(classes.len());

    while i > 0 && classes[i - 1] == Class::Whitespace {
        i -= 1;
    }
    if i == 0 {
        return 0;
    }
    let run = classes[i - 1];
    while i > 0 && classes[i - 1] == run {
        i -= 1;
    }
    i
}

/// The run containing `col`, as `(start, end)` grapheme indices.
///
/// A cursor sitting immediately after a word counts as being on it, which is
/// what makes double-click-at-the-end select what the user meant.
pub fn word_at(line: &str, col: usize) -> Option<(usize, usize)> {
    let classes = classes(line);
    let len = classes.len();
    if len == 0 {
        return None;
    }
    let probe = if col < len { col } else { len - 1 };
    let target = classes[probe];
    if target == Class::Whitespace {
        return None;
    }

    let mut start = probe;
    while start > 0 && classes[start - 1] == target {
        start -= 1;
    }
    let mut end = probe;
    while end < len && classes[end] == target {
        end += 1;
    }
    Some((start, end))
}
```

Add to `crates/typ-buffer/src/lib.rs`:

```rust
pub mod word;

pub use word::{next_word_boundary, previous_word_boundary, word_at};
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p typ-buffer --test word`

Expected: PASS, 10 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/typ-buffer/src/word.rs crates/typ-buffer/src/lib.rs crates/typ-buffer/tests/word.rs
git commit -m "feat(buffer): grapheme-indexed word boundaries with punctuation runs"
```

---

### Task 5: Literal search and range replacement

**Files:**
- Create: `crates/typ-buffer/src/search.rs`, `crates/typ-buffer/tests/search.rs`
- Modify: `crates/typ-buffer/src/lib.rs`, `crates/typ-buffer/src/buffer.rs`

**Interfaces:**
- Consumes: `typ_buffer::{Position, TextBuffer, Selection}`
- Produces:
  - `typ_buffer::SearchQuery { needle: String, case_sensitive: bool }`
  - `typ_buffer::find_in_line(line: &str, query: &SearchQuery) -> Vec<(usize, usize)>`
  - `TextBuffer::find_all(&self, query: &SearchQuery) -> Vec<Selection>`
  - `TextBuffer::replace_range(&mut self, start: Position, end: Position, text: &str)`

- [ ] **Step 1: Write the failing test**

`crates/typ-buffer/tests/search.rs`:

```rust
use typ_buffer::{Position, SearchQuery, TextBuffer};

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

fn query(needle: &str, case_sensitive: bool) -> SearchQuery {
    SearchQuery { needle: needle.to_string(), case_sensitive }
}

#[test]
fn find_all_returns_every_match_in_document_order() {
    let b = TextBuffer::from_str("one two one\nrepeat one\n");
    let hits = b.find_all(&query("one", true));
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].range(), (pos(0, 0), pos(0, 3)));
    assert_eq!(hits[1].range(), (pos(0, 8), pos(0, 11)));
    assert_eq!(hits[2].range(), (pos(1, 7), pos(1, 10)));
}

#[test]
fn a_case_insensitive_search_matches_regardless_of_case() {
    let b = TextBuffer::from_str("Rust rust RUST\n");
    assert_eq!(b.find_all(&query("rust", false)).len(), 3);
    assert_eq!(b.find_all(&query("rust", true)).len(), 1);
}

#[test]
fn an_empty_needle_matches_nothing() {
    let b = TextBuffer::from_str("anything\n");
    assert!(b.find_all(&query("", true)).is_empty());
}

#[test]
fn matches_are_measured_in_graphemes_not_bytes() {
    let b = TextBuffer::from_str("日本語 ok\n");
    let hits = b.find_all(&query("ok", true));
    assert_eq!(hits[0].range(), (pos(0, 4), pos(0, 6)));
}

#[test]
fn repeated_text_yields_non_overlapping_matches() {
    let b = TextBuffer::from_str("aaaa\n");
    let hits = b.find_all(&query("aa", true));
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].range(), (pos(0, 0), pos(0, 2)));
    assert_eq!(hits[1].range(), (pos(0, 2), pos(0, 4)));
}

#[test]
fn a_match_never_spans_a_line_break() {
    let b = TextBuffer::from_str("ab\ncd\n");
    assert!(b.find_all(&query("bc", true)).is_empty());
}

#[test]
fn replace_range_swaps_the_text_and_marks_the_buffer_dirty() {
    let mut b = TextBuffer::from_str("hello world\n");
    b.replace_range(pos(0, 6), pos(0, 11), "there");
    assert_eq!(b.line_text(0), "hello there");
    assert!(b.is_dirty());
}

#[test]
fn replace_range_is_undoable_as_one_step() {
    let mut b = TextBuffer::from_str("hello world\n");
    b.replace_range(pos(0, 6), pos(0, 11), "there");
    b.undo();
    assert_eq!(b.line_text(0), "hello world");
}

#[test]
fn replace_range_handles_a_replacement_of_a_different_length() {
    let mut b = TextBuffer::from_str("a-b\n");
    b.replace_range(pos(0, 1), pos(0, 2), "===");
    assert_eq!(b.line_text(0), "a===b");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p typ-buffer --test search`

Expected: FAIL — `unresolved import typ_buffer::SearchQuery`, no method `find_all`.

- [ ] **Step 3: Write the search module**

`crates/typ-buffer/src/search.rs`:

```rust
//! Literal, line-scoped search.
//!
//! Line-scoped on purpose: a match never spans a line break, so every result
//! is expressible as `(line, grapheme)` without a second coordinate system,
//! and that is what a user typing into a search box means anyway. Regex
//! belongs behind this same `SearchQuery` type later, not beside it.

use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub needle: String,
    pub case_sensitive: bool,
}

impl SearchQuery {
    pub fn new(needle: impl Into<String>, case_sensitive: bool) -> Self {
        Self { needle: needle.into(), case_sensitive }
    }
}

/// Grapheme index pairs of every non-overlapping match in one line.
pub fn find_in_line(line: &str, query: &SearchQuery) -> Vec<(usize, usize)> {
    if query.needle.is_empty() {
        return Vec::new();
    }

    let fold = |s: &str| {
        if query.case_sensitive {
            s.to_string()
        } else {
            s.to_lowercase()
        }
    };

    let haystack: Vec<String> = line.graphemes(true).map(&fold).collect();
    let needle: Vec<String> = query.needle.graphemes(true).map(&fold).collect();

    let mut hits = Vec::new();
    let mut i = 0usize;
    while i + needle.len() <= haystack.len() {
        if haystack[i..i + needle.len()] == needle[..] {
            hits.push((i, i + needle.len()));
            // Advance past the match. Overlapping hits would let a replace-all
            // rewrite text it had already rewritten.
            i += needle.len();
        } else {
            i += 1;
        }
    }
    hits
}
```

- [ ] **Step 4: Add the buffer methods**

In `crates/typ-buffer/src/buffer.rs`, add these to `impl TextBuffer`, and add
`use crate::search::SearchQuery;` and `use crate::selection::Selection;` to the imports:

```rust
    /// Every match in the buffer, in document order, as selections whose head
    /// sits at the end of the match — so jumping to one leaves the cursor
    /// where typing would naturally continue.
    pub fn find_all(&self, query: &SearchQuery) -> Vec<Selection> {
        let mut hits = Vec::new();
        for line in 0..self.line_count() {
            let text = self.line_text(line);
            for (start, end) in crate::search::find_in_line(&text, query) {
                hits.push(Selection {
                    anchor: Position { line, col: start },
                    head: Position { line, col: end },
                });
            }
        }
        hits
    }

    /// Replace the text between two positions as a single undo step.
    pub fn replace_range(&mut self, start: Position, end: Position, text: &str) {
        let from = self.char_offset(start);
        let to = self.char_offset(end);
        if from >= to {
            return;
        }
        self.history.record(self.rope.to_string());
        self.rope.remove(from..to);
        self.rope.insert(from, text);
        self.dirty = true;
    }
```

Add to `crates/typ-buffer/src/lib.rs`:

```rust
pub mod search;

pub use search::{SearchQuery, find_in_line};
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p typ-buffer --test search`

Expected: PASS, 9 tests.

- [ ] **Step 6: Run the whole buffer suite**

Run: `cargo test -p typ-buffer`

Expected: PASS — the earlier buffer, width, selection and word tests still pass. `find_all`
and `replace_range` are additive; if `delete_before` or `insert_char` broke, the cause is
`char_offset` having been changed rather than reused.

- [ ] **Step 7: Commit**

```bash
git add crates/typ-buffer/src/search.rs crates/typ-buffer/src/buffer.rs crates/typ-buffer/src/lib.rs crates/typ-buffer/tests/search.rs
git commit -m "feat(buffer): literal search and single-step range replacement"
```

---

### Task 6: The editor holds selections, and draws them

**Files:**
- Modify: `crates/typ-core/src/panel.rs`, `crates/typ-panel-editor/src/lib.rs`,
  `crates/typ-panel-editor/Cargo.toml`
- Create: `crates/typ-panel-editor/src/render.rs`,
  `crates/typ-panel-editor/tests/selection_render.rs`

**Interfaces:**
- Consumes: `typ_buffer::{Selection, Selections}`, `typ_core::Action`
- Produces:
  - `Panel::apply_action(&mut self, action: Action) -> Vec<PanelEvent>` — defaulted to empty
  - `EditorPanel::selections(&self) -> &Selections`
  - `EditorPanel::cursor(&self) -> Position` — now the primary head, unchanged signature
  - `typ_panel_editor::render::styled_line(...) -> ratatui::text::Line`

- [ ] **Step 1: Add the trait method**

In `crates/typ-core/src/panel.rs`, add to `trait Panel`, beside the other defaulted methods:

```rust
    /// Perform a named action.
    ///
    /// This is the only way a binding, the command palette, or the vim layer
    /// reaches a panel's behavior. A panel that ignores an action returns no
    /// events, which is how the app knows to try the action itself.
    fn apply_action(&mut self, action: crate::Action) -> Vec<PanelEvent> {
        let _ = action;
        Vec::new()
    }
```

- [ ] **Step 2: Write the failing test**

`crates/typ-panel-editor/tests/selection_render.rs`:

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_buffer::{Position, Selection};
use typ_core::{Panel, RenderContext, ThemeColors};
use typ_panel_editor::EditorPanel;

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

fn render(panel: &mut EditorPanel, area: Rect) -> Buffer {
    let theme = ThemeColors::default();
    let ctx = RenderContext {
        theme: &theme,
        is_focused: true,
        panel_index: 0,
        terminal_width: area.width,
        terminal_height: area.height,
    };
    let mut buf = Buffer::empty(area);
    panel.render(area, &mut buf, &ctx);
    buf
}

#[test]
fn a_new_editor_has_exactly_one_empty_selection() {
    let panel = EditorPanel::from_str("abc\n");
    assert_eq!(panel.selections().len(), 1);
    assert!(panel.selections().primary().is_empty());
    assert_eq!(panel.cursor(), pos(0, 0));
}

#[test]
fn the_cursor_is_the_primary_head() {
    let mut panel = EditorPanel::from_str("abcdef\n");
    panel.set_selections_for_test(vec![Selection { anchor: pos(0, 1), head: pos(0, 4) }]);
    assert_eq!(panel.cursor(), pos(0, 4));
}

#[test]
fn selected_text_is_drawn_in_the_selection_colors() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("abcdef\n");
    panel.set_selections_for_test(vec![Selection { anchor: pos(0, 1), head: pos(0, 4) }]);
    let buf = render(&mut panel, Rect::new(0, 0, 20, 5));

    // Text starts at column 1, row 1, inside the border.
    assert_eq!(buf[(1, 1)].bg, theme.bg, "column 0 is outside the selection");
    for x in 2..5 {
        assert_eq!(buf[(x, 1)].bg, theme.selection_bg, "column {x} should be selected");
    }
    assert_eq!(buf[(5, 1)].bg, theme.bg, "the end of a selection is exclusive");
}

#[test]
fn a_selection_spanning_lines_covers_both_ends() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("abcd\nefgh\n");
    panel.set_selections_for_test(vec![Selection { anchor: pos(0, 2), head: pos(1, 2) }]);
    let buf = render(&mut panel, Rect::new(0, 0, 20, 6));

    assert_eq!(buf[(3, 1)].bg, theme.selection_bg, "tail of the first line");
    assert_eq!(buf[(1, 2)].bg, theme.selection_bg, "head of the second line");
    assert_eq!(buf[(4, 2)].bg, theme.bg, "past the selection on the second line");
}

#[test]
fn every_selection_is_drawn_not_only_the_primary() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("abcdef\n");
    panel.set_selections_for_test(vec![
        Selection { anchor: pos(0, 0), head: pos(0, 1) },
        Selection { anchor: pos(0, 4), head: pos(0, 5) },
    ]);
    let buf = render(&mut panel, Rect::new(0, 0, 20, 5));
    assert_eq!(buf[(1, 1)].bg, theme.selection_bg);
    assert_eq!(buf[(5, 1)].bg, theme.selection_bg);
    assert_eq!(buf[(3, 1)].bg, theme.bg, "the gap between them is not selected");
}

#[test]
fn an_empty_selection_paints_nothing() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("abcdef\n");
    let buf = render(&mut panel, Rect::new(0, 0, 20, 5));
    for x in 1..7 {
        assert_eq!(buf[(x, 1)].bg, theme.bg, "a caret must not highlight column {x}");
    }
}

#[test]
fn selection_highlighting_lands_on_the_right_columns_with_wide_characters() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("日本語\n");
    // Select the second CJK grapheme only.
    panel.set_selections_for_test(vec![Selection { anchor: pos(0, 1), head: pos(0, 2) }]);
    let buf = render(&mut panel, Rect::new(0, 0, 20, 5));

    assert_eq!(buf[(1, 1)].bg, theme.bg, "the first grapheme is not selected");
    assert_eq!(buf[(3, 1)].bg, theme.selection_bg, "two display columns in");
    assert_eq!(buf[(4, 1)].bg, theme.selection_bg, "and its second column");
    assert_eq!(buf[(5, 1)].bg, theme.bg, "the third grapheme is not selected");
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p typ-panel-editor --test selection_render`

Expected: FAIL — no method `selections`, no method `set_selections_for_test`.

- [ ] **Step 4: Replace the cursor field with selections**

In `crates/typ-panel-editor/src/lib.rs`, change the struct and add the accessors. The
`cursor: Position` field goes away entirely — leaving it beside `Selections` would create two
sources of truth that drift the first time one is updated without the other.

```rust
use typ_buffer::{Selection, Selections};

pub struct EditorPanel {
    buffer: TextBuffer,
    selections: Selections,
    top_line: usize,
    goal_col: Option<usize>,
    height: usize,
}
```

```rust
    pub fn selections(&self) -> &Selections {
        &self.selections
    }

    /// The primary head — where the terminal cursor is drawn.
    pub fn cursor(&self) -> Position {
        self.selections.primary().head
    }

    /// Set selections directly. Test-only: production code goes through
    /// actions, so that every path a user can take is one a test can take.
    #[doc(hidden)]
    pub fn set_selections_for_test(&mut self, list: Vec<Selection>) {
        assert!(!list.is_empty(), "selections are never empty");
        let mut selections = Selections::single(list[0]);
        for selection in &list[1..] {
            selections.push(*selection);
        }
        self.selections = selections;
    }
```

Every existing use of `self.cursor` inside the file becomes
`self.selections.primary().head`, and every assignment becomes
`self.selections.set_single(Selection::caret(new_position))`. The existing `handle_key` arms
stay working for now; Task 7 replaces them.

- [ ] **Step 5: Write the selection-aware line renderer**

`crates/typ-panel-editor/src/render.rs`:

```rust
//! Turning a line of text plus the selections covering it into styled spans.
//!
//! Split out of `lib.rs` because this is where display-column arithmetic
//! lives, and it is the part most likely to grow: highlighting arrives in M2.5
//! and has to compose with selection styling rather than fight it.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use typ_buffer::{Position, Selection};
use typ_core::ThemeColors;
use unicode_segmentation::UnicodeSegmentation;

/// Build one rendered line, splitting it into spans wherever the selection
/// state changes.
///
/// Spans are cut at grapheme boundaries and styled per grapheme run, so a wide
/// character is highlighted as one unit and never half-painted.
pub fn styled_line(
    text: &str,
    line_index: usize,
    selections: &[Selection],
    theme: &ThemeColors,
) -> Line<'static> {
    let plain = Style::default().fg(theme.fg).bg(theme.bg);
    let selected = Style::default().fg(theme.selection_fg).bg(theme.selection_bg);

    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut current = String::new();
    let mut current_selected: Option<bool> = None;

    for (col, grapheme) in text.graphemes(true).enumerate() {
        let position = Position { line: line_index, col };
        let is_selected = selections.iter().any(|s| s.contains(position));

        if current_selected != Some(is_selected) && !current.is_empty() {
            let style = if current_selected == Some(true) { selected } else { plain };
            spans.push(Span::styled(std::mem::take(&mut current), style));
        }
        current_selected = Some(is_selected);
        current.push_str(grapheme);
    }

    if !current.is_empty() {
        let style = if current_selected == Some(true) { selected } else { plain };
        spans.push(Span::styled(current, style));
    }

    Line::from(spans)
}
```

- [ ] **Step 6: Use it from `render`**

In `crates/typ-panel-editor/src/lib.rs`, replace the `Line::raw(...)` construction inside
`Panel::render` with:

```rust
        let selections: Vec<Selection> = self.selections.iter().copied().collect();
        let lines: Vec<Line> = (self.top_line..end)
            .map(|i| {
                crate::render::styled_line(&self.buffer.line_text(i), i, &selections, ctx.theme)
            })
            .collect();
```

and add `pub mod render;` at the top of the file. Add `unicode-segmentation.workspace = true`
to `crates/typ-panel-editor/Cargo.toml` if it is not already there.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -p typ-panel-editor`

Expected: PASS — 7 new selection-render tests plus the existing editor and keys suites.

If the wide-character test fails by one column, the cause is spans being cut on `char`
boundaries rather than graphemes; `styled_line` must iterate `graphemes(true)`.

- [ ] **Step 8: Commit**

```bash
git add crates/typ-core/src/panel.rs crates/typ-panel-editor/src crates/typ-panel-editor/tests crates/typ-panel-editor/Cargo.toml
git commit -m "feat(editor): hold selections rather than a bare cursor, and draw them"
```

---

### Task 7: Motions as actions

**Files:**
- Create: `crates/typ-panel-editor/src/actions.rs`, `crates/typ-panel-editor/tests/motion.rs`
- Modify: `crates/typ-panel-editor/src/lib.rs`

**Interfaces:**
- Consumes: `typ_core::{Action, Motion}`, `typ_buffer::{next_word_boundary, previous_word_boundary}`
- Produces: `EditorPanel::apply_action` covering every `Action::Move`

- [ ] **Step 1: Write the failing test**

`crates/typ-panel-editor/tests/motion.rs`:

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_buffer::{Position, Selection};
use typ_core::{Action, Motion, Panel, RenderContext, ThemeColors};
use typ_panel_editor::EditorPanel;

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

fn mv(motion: Motion) -> Action {
    Action::Move { motion, extend: false }
}

fn extend(motion: Motion) -> Action {
    Action::Move { motion, extend: true }
}

/// Panels learn their height at render time; page motions need one frame.
fn render(panel: &mut EditorPanel, area: Rect) {
    let theme = ThemeColors::default();
    let ctx = RenderContext {
        theme: &theme,
        is_focused: true,
        panel_index: 0,
        terminal_width: area.width,
        terminal_height: area.height,
    };
    let mut buf = Buffer::empty(area);
    panel.render(area, &mut buf, &ctx);
}

#[test]
fn moving_right_advances_the_caret() {
    let mut p = EditorPanel::from_str("abc\n");
    p.apply_action(mv(Motion::Right));
    assert_eq!(p.cursor(), pos(0, 1));
    assert!(p.selections().primary().is_empty());
}

#[test]
fn extending_right_leaves_the_anchor_behind() {
    let mut p = EditorPanel::from_str("abc\n");
    p.apply_action(extend(Motion::Right));
    let s = p.selections().primary();
    assert_eq!(s.anchor, pos(0, 0));
    assert_eq!(s.head, pos(0, 1));
}

#[test]
fn a_plain_move_collapses_an_existing_selection_to_its_far_end() {
    let mut p = EditorPanel::from_str("abcdef\n");
    p.set_selections_for_test(vec![Selection { anchor: pos(0, 1), head: pos(0, 4) }]);
    p.apply_action(mv(Motion::Right));
    // Collapsing to the end of the selection, then moving, is what a GUI
    // editor does: the arrow key does not jump back to the anchor.
    assert_eq!(p.cursor(), pos(0, 5));
    assert!(p.selections().primary().is_empty());
}

#[test]
fn moving_left_out_of_a_selection_collapses_to_its_near_end() {
    let mut p = EditorPanel::from_str("abcdef\n");
    p.set_selections_for_test(vec![Selection { anchor: pos(0, 1), head: pos(0, 4) }]);
    p.apply_action(mv(Motion::Left));
    assert_eq!(p.cursor(), pos(0, 0));
}

#[test]
fn moving_right_at_the_end_of_a_line_wraps_to_the_next() {
    let mut p = EditorPanel::from_str("ab\ncd\n");
    p.apply_action(mv(Motion::LineEnd));
    p.apply_action(mv(Motion::Right));
    assert_eq!(p.cursor(), pos(1, 0));
}

#[test]
fn word_motion_stops_at_punctuation_runs() {
    let mut p = EditorPanel::from_str("foo::bar\n");
    p.apply_action(mv(Motion::WordRight));
    assert_eq!(p.cursor(), pos(0, 3));
    p.apply_action(mv(Motion::WordRight));
    assert_eq!(p.cursor(), pos(0, 5));
}

#[test]
fn word_motion_crosses_a_line_when_the_line_is_exhausted() {
    let mut p = EditorPanel::from_str("foo\nbar\n");
    p.apply_action(mv(Motion::LineEnd));
    p.apply_action(mv(Motion::WordRight));
    assert_eq!(p.cursor(), pos(1, 0));
}

#[test]
fn document_motions_reach_both_ends() {
    let mut p = EditorPanel::from_str("a\nb\nc\n");
    p.apply_action(mv(Motion::DocumentEnd));
    assert_eq!(p.cursor().line, 3, "the trailing newline makes a final empty line");
    p.apply_action(mv(Motion::DocumentStart));
    assert_eq!(p.cursor(), pos(0, 0));
}

#[test]
fn vertical_motion_remembers_the_goal_column() {
    let mut p = EditorPanel::from_str("abcdef\nab\nabcdef\n");
    p.apply_action(mv(Motion::LineEnd));
    assert_eq!(p.cursor(), pos(0, 6));
    p.apply_action(mv(Motion::Down));
    assert_eq!(p.cursor(), pos(1, 2), "clamped to the short line");
    p.apply_action(mv(Motion::Down));
    assert_eq!(p.cursor(), pos(2, 6), "the goal column is restored");
}

#[test]
fn page_motions_move_by_the_visible_height() {
    let text = (0..100).map(|i| format!("line {i}\n")).collect::<String>();
    let mut p = EditorPanel::from_str(&text);
    render(&mut p, Rect::new(0, 0, 40, 12)); // 12 rows minus the border = 10
    p.apply_action(mv(Motion::PageDown));
    assert_eq!(p.cursor().line, 10);
}

#[test]
fn a_motion_applies_to_every_selection() {
    let mut p = EditorPanel::from_str("abc\ndef\n");
    p.set_selections_for_test(vec![
        Selection::caret(pos(0, 0)),
        Selection::caret(pos(1, 0)),
    ]);
    p.apply_action(mv(Motion::Right));
    let heads: Vec<Position> = p.selections().iter().map(|s| s.head).collect();
    assert_eq!(heads, vec![pos(0, 1), pos(1, 1)]);
}

#[test]
fn a_motion_requests_a_redraw() {
    let mut p = EditorPanel::from_str("abc\n");
    assert_eq!(
        p.apply_action(mv(Motion::Right)),
        vec![typ_core::PanelEvent::NeedsRedraw]
    );
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p typ-panel-editor --test motion`

Expected: FAIL — `apply_action` is the defaulted trait method, so every assertion about
movement fails while the redraw assertion fails on an empty vector.

- [ ] **Step 3: Write the motion implementation**

`crates/typ-panel-editor/src/actions.rs`:

```rust
//! `Action` → editor behavior.
//!
//! Every mutation of the editor lives here or is called from here. Nothing in
//! `handle_key` touches the buffer, which is what keeps the keymap, the future
//! command palette, and the future vim layer able to reach the same behavior.

use typ_buffer::{Position, Selection, next_word_boundary, previous_word_boundary};
use typ_core::{Action, Motion, PanelEvent};

use crate::{EditorPanel, TAB_WIDTH};

impl EditorPanel {
    /// Move one selection according to a motion.
    ///
    /// `extend` decides whether the anchor follows. A plain move from a
    /// non-empty selection collapses toward the direction of travel rather
    /// than from the head, which is the behavior everyone arriving from a GUI
    /// editor has in their fingers.
    fn move_selection(&self, selection: Selection, motion: Motion, extend: bool) -> Selection {
        let collapse_target = match (selection.is_empty(), motion) {
            (false, Motion::Left | Motion::WordLeft | Motion::LineStart | Motion::DocumentStart) => {
                Some(selection.range().0)
            }
            (false, Motion::Right | Motion::WordRight | Motion::LineEnd | Motion::DocumentEnd) => {
                Some(selection.range().1)
            }
            _ => None,
        };
        if !extend && let Some(target) = collapse_target {
            return Selection::caret(target);
        }

        let head = self.moved_position(selection.head, motion);
        Selection {
            anchor: if extend { selection.anchor } else { head },
            head,
        }
    }

    fn moved_position(&self, from: Position, motion: Motion) -> Position {
        let line_len = |line: usize| self.line_grapheme_count(line);
        let last_line = self.last_line();

        match motion {
            Motion::Left => {
                if from.col > 0 {
                    Position { line: from.line, col: from.col - 1 }
                } else if from.line > 0 {
                    Position { line: from.line - 1, col: line_len(from.line - 1) }
                } else {
                    from
                }
            }
            Motion::Right => {
                if from.col < line_len(from.line) {
                    Position { line: from.line, col: from.col + 1 }
                } else if from.line < last_line {
                    Position { line: from.line + 1, col: 0 }
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
                        Position { line: from.line - 1, col: line_len(from.line - 1) }
                    }
                } else {
                    let text = self.buffer.line_text(from.line);
                    Position { line: from.line, col: previous_word_boundary(&text, from.col) }
                }
            }
            Motion::WordRight => {
                let text = self.buffer.line_text(from.line);
                if from.col >= line_len(from.line) {
                    if from.line >= last_line {
                        from
                    } else {
                        Position { line: from.line + 1, col: 0 }
                    }
                } else {
                    Position { line: from.line, col: next_word_boundary(&text, from.col) }
                }
            }
            Motion::LineStart => Position { line: from.line, col: 0 },
            Motion::LineEnd => Position { line: from.line, col: line_len(from.line) },
            Motion::DocumentStart => Position { line: 0, col: 0 },
            Motion::DocumentEnd => Position { line: last_line, col: line_len(last_line) },
        }
    }

    /// Vertical movement, preserving the goal column through short lines.
    fn vertical(&self, from: Position, delta: i64) -> Position {
        let goal = self.goal_col.unwrap_or_else(|| {
            typ_buffer::grapheme_to_display_col(
                &self.buffer.line_text(from.line),
                from.col,
                TAB_WIDTH,
            )
        });
        let line = (from.line as i64 + delta).clamp(0, self.last_line() as i64) as usize;
        let col = typ_buffer::display_to_grapheme_col(&self.buffer.line_text(line), goal, TAB_WIDTH);
        Position { line, col }
    }

    /// The entry point every consumer uses.
    pub fn perform(&mut self, action: Action) -> Vec<PanelEvent> {
        match action {
            Action::Move { motion, extend } => {
                // The goal column survives vertical motion and is cleared by
                // anything else, so passing through a short line does not
                // permanently narrow the column.
                let vertical = matches!(
                    motion,
                    Motion::Up | Motion::Down | Motion::PageUp | Motion::PageDown
                );
                if vertical {
                    // Latch the goal from where the cursor is *now*, before
                    // moving. Recomputing it afterwards would store the column
                    // the motion just clamped to, so passing through one short
                    // line would narrow the goal permanently — which is the
                    // exact bug this field exists to prevent.
                    if self.goal_col.is_none() {
                        let cursor = self.cursor();
                        self.goal_col = Some(typ_buffer::grapheme_to_display_col(
                            &self.buffer.line_text(cursor.line),
                            cursor.col,
                            TAB_WIDTH,
                        ));
                    }
                } else {
                    self.goal_col = None;
                }
                let mut moved: Vec<Selection> = Vec::new();
                for selection in self.selections.iter() {
                    moved.push(self.move_selection(*selection, motion, extend));
                }
                let mut iter = moved.into_iter();
                let first = iter.next().expect("selections are never empty");
                self.selections.set_single(first);
                for selection in iter {
                    self.selections.push(selection);
                }
                self.scroll_to_cursor();
                vec![PanelEvent::NeedsRedraw]
            }
            _ => Vec::new(),
        }
    }
}
```

Note the borrow shape: selections are read into a `Vec` before being written back, because
`move_selection` borrows `self` immutably while `self.selections` needs a mutable borrow. A
`Vec` of at most a few dozen selections is the cheap way out; cloning the whole `Selections`
each keystroke is not.

- [ ] **Step 4: Wire it to the trait**

In `crates/typ-panel-editor/src/lib.rs`, add `pub mod actions;`, make `goal_col`,
`selections`, `buffer`, `top_line`, `height`, `page`, `line_grapheme_count`, `last_line` and
`scroll_to_cursor` visible to the sibling module by marking them `pub(crate)`, and in
`impl Panel for EditorPanel` add:

```rust
    fn apply_action(&mut self, action: Action) -> Vec<PanelEvent> {
        self.perform(action)
    }
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p typ-panel-editor --test motion`

Expected: PASS, 12 tests.

- [ ] **Step 6: Commit**

```bash
git add crates/typ-panel-editor/src crates/typ-panel-editor/tests/motion.rs
git commit -m "feat(editor): every motion as an action, applied to every selection"
```

---

### Task 8: Edits across every selection

**Files:**
- Modify: `crates/typ-panel-editor/src/actions.rs`
- Create: `crates/typ-panel-editor/tests/edit.rs`

**Interfaces:**
- Consumes: `typ_buffer::TextBuffer`, `typ_core::{Action, Direction}`
- Produces: `Action::{InsertChar, InsertNewline, Delete, Undo, Redo}` handled in `perform`

- [ ] **Step 1: Write the failing test**

`crates/typ-panel-editor/tests/edit.rs`:

```rust
use typ_buffer::{Position, Selection};
use typ_core::{Action, Direction, Motion, Panel};
use typ_panel_editor::EditorPanel;

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

fn del(direction: Direction, by_word: bool) -> Action {
    Action::Delete { direction, by_word }
}

#[test]
fn typing_inserts_at_the_caret_and_advances_it() {
    let mut p = EditorPanel::from_str("ac\n");
    p.apply_action(Action::Move { motion: Motion::Right, extend: false });
    p.apply_action(Action::InsertChar('b'));
    assert_eq!(p.line_text(0), "abc");
    assert_eq!(p.cursor(), pos(0, 2));
}

#[test]
fn typing_replaces_a_selection() {
    let mut p = EditorPanel::from_str("abcdef\n");
    p.set_selections_for_test(vec![Selection { anchor: pos(0, 1), head: pos(0, 4) }]);
    p.apply_action(Action::InsertChar('X'));
    assert_eq!(p.line_text(0), "aXef");
    assert_eq!(p.cursor(), pos(0, 2));
}

#[test]
fn typing_inserts_at_every_caret() {
    let mut p = EditorPanel::from_str("ab\nab\n");
    p.set_selections_for_test(vec![
        Selection::caret(pos(0, 1)),
        Selection::caret(pos(1, 1)),
    ]);
    p.apply_action(Action::InsertChar('-'));
    assert_eq!(p.line_text(0), "a-b");
    assert_eq!(p.line_text(1), "a-b");
    let heads: Vec<Position> = p.selections().iter().map(|s| s.head).collect();
    assert_eq!(heads, vec![pos(0, 2), pos(1, 2)]);
}

#[test]
fn multi_caret_edits_on_one_line_do_not_corrupt_each_other() {
    let mut p = EditorPanel::from_str("abcdef\n");
    p.set_selections_for_test(vec![
        Selection::caret(pos(0, 1)),
        Selection::caret(pos(0, 3)),
        Selection::caret(pos(0, 5)),
    ]);
    p.apply_action(Action::InsertChar('.'));
    assert_eq!(p.line_text(0), "a.bc.de.f");
    let heads: Vec<Position> = p.selections().iter().map(|s| s.head).collect();
    assert_eq!(heads, vec![pos(0, 2), pos(0, 5), pos(0, 8)]);
}

#[test]
fn enter_splits_at_every_caret() {
    let mut p = EditorPanel::from_str("ab\n");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 1))]);
    p.apply_action(Action::InsertNewline);
    assert_eq!(p.line_text(0), "a");
    assert_eq!(p.line_text(1), "b");
    assert_eq!(p.cursor(), pos(1, 0));
}

#[test]
fn backspace_deletes_one_grapheme_at_each_caret() {
    let mut p = EditorPanel::from_str("abc\n");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 2))]);
    p.apply_action(del(Direction::Backward, false));
    assert_eq!(p.line_text(0), "ac");
    assert_eq!(p.cursor(), pos(0, 1));
}

#[test]
fn backspace_with_a_selection_deletes_the_selection_and_nothing_more() {
    let mut p = EditorPanel::from_str("abcdef\n");
    p.set_selections_for_test(vec![Selection { anchor: pos(0, 1), head: pos(0, 4) }]);
    p.apply_action(del(Direction::Backward, false));
    assert_eq!(p.line_text(0), "aef");
    assert_eq!(p.cursor(), pos(0, 1));
}

#[test]
fn delete_forward_removes_the_grapheme_under_the_caret() {
    let mut p = EditorPanel::from_str("abc\n");
    p.apply_action(del(Direction::Forward, false));
    assert_eq!(p.line_text(0), "bc");
    assert_eq!(p.cursor(), pos(0, 0));
}

#[test]
fn delete_word_backward_removes_a_whole_word() {
    let mut p = EditorPanel::from_str("foo bar\n");
    p.apply_action(Action::Move { motion: Motion::LineEnd, extend: false });
    p.apply_action(del(Direction::Backward, true));
    assert_eq!(p.line_text(0), "foo ");
}

#[test]
fn delete_word_forward_removes_a_whole_word() {
    let mut p = EditorPanel::from_str("foo bar\n");
    p.apply_action(del(Direction::Forward, true));
    assert_eq!(p.line_text(0), " bar");
}

#[test]
fn backspace_at_the_start_of_a_line_joins_it_to_the_previous() {
    let mut p = EditorPanel::from_str("ab\ncd\n");
    p.set_selections_for_test(vec![Selection::caret(pos(1, 0))]);
    p.apply_action(del(Direction::Backward, false));
    assert_eq!(p.line_text(0), "abcd");
    assert_eq!(p.cursor(), pos(0, 2));
}

#[test]
fn a_multi_caret_edit_undoes_as_one_step() {
    let mut p = EditorPanel::from_str("ab\nab\n");
    p.set_selections_for_test(vec![
        Selection::caret(pos(0, 1)),
        Selection::caret(pos(1, 1)),
    ]);
    p.apply_action(Action::InsertChar('-'));
    p.apply_action(Action::Undo);
    assert_eq!(p.line_text(0), "ab");
    assert_eq!(p.line_text(1), "ab", "both edits belong to one undo step");
}

#[test]
fn undo_then_redo_restores_the_edit() {
    let mut p = EditorPanel::from_str("ab\n");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 1))]);
    p.apply_action(Action::InsertChar('-'));
    p.apply_action(Action::Undo);
    p.apply_action(Action::Redo);
    assert_eq!(p.line_text(0), "a-b");
}

#[test]
fn undo_pulls_the_caret_back_inside_the_text() {
    let mut p = EditorPanel::from_str("ab\n");
    p.apply_action(Action::Move { motion: Motion::LineEnd, extend: false });
    p.apply_action(Action::InsertChar('c'));
    p.apply_action(Action::InsertChar('d'));
    p.apply_action(Action::Undo);
    assert!(p.cursor().col <= p.line_text(0).chars().count());
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p typ-panel-editor --test edit`

Expected: FAIL — `perform` returns nothing for edit actions, so the buffer never changes.

- [ ] **Step 3: Write the implementation**

Add to `impl EditorPanel` in `crates/typ-panel-editor/src/actions.rs`:

```rust
    /// Apply one text edit at every selection.
    ///
    /// Selections are processed **last to first**. An edit changes the
    /// positions of everything after it, so working backwards means every
    /// selection still points at the text it was aimed at when its turn comes.
    /// Working forwards would require re-mapping the remaining selections after
    /// every single edit, which is the same computation done N times.
    fn edit_at_each_selection(
        &mut self,
        mut edit: impl FnMut(&mut typ_buffer::TextBuffer, Selection) -> Position,
    ) -> Vec<PanelEvent> {
        let mut selections: Vec<Selection> = self.selections.iter().copied().collect();
        // The buffer records one undo snapshot per call, so a multi-caret edit
        // is one undo step. Without this, undoing a 30-caret edit would take
        // 30 presses.
        self.buffer.begin_edit_group();
        let mut new_heads: Vec<Position> = Vec::with_capacity(selections.len());
        for selection in selections.drain(..).rev() {
            new_heads.push(edit(&mut self.buffer, selection));
        }
        self.buffer.end_edit_group();

        new_heads.reverse();
        let mut iter = new_heads.into_iter().map(Selection::caret);
        let first = iter.next().expect("selections are never empty");
        self.selections.set_single(first);
        for selection in iter {
            self.selections.push(selection);
        }
        self.goal_col = None;
        self.scroll_to_cursor();
        vec![PanelEvent::NeedsRedraw]
    }
```

and extend `perform` with these arms, above the catch-all:

```rust
            Action::InsertChar(c) => self.edit_at_each_selection(move |buffer, selection| {
                let (start, end) = selection.range();
                if !selection.is_empty() {
                    buffer.replace_range(start, end, &c.to_string());
                } else {
                    buffer.insert_char(start, c);
                }
                Position { line: start.line, col: start.col + 1 }
            }),

            Action::InsertNewline => self.edit_at_each_selection(|buffer, selection| {
                let (start, end) = selection.range();
                if !selection.is_empty() {
                    buffer.replace_range(start, end, "\n");
                } else {
                    buffer.insert_char(start, '\n');
                }
                Position { line: start.line + 1, col: 0 }
            }),

            Action::Delete { direction, by_word } => {
                self.delete_at_each_selection(direction, by_word)
            }

            Action::Undo => {
                self.buffer.undo();
                self.clamp_selections();
                vec![PanelEvent::NeedsRedraw]
            }

            Action::Redo => {
                self.buffer.redo();
                self.clamp_selections();
                vec![PanelEvent::NeedsRedraw]
            }
```

Deletion needs the line text, so it reads what it needs before mutating:

```rust
    fn delete_at_each_selection(
        &mut self,
        direction: typ_core::Direction,
        by_word: bool,
    ) -> Vec<PanelEvent> {
        use typ_core::Direction;

        // Line texts are captured up front: the closure below cannot borrow
        // `self` while `self.buffer` is borrowed mutably.
        let lines: Vec<String> = (0..self.buffer.line_count())
            .map(|i| self.buffer.line_text(i))
            .collect();

        self.edit_at_each_selection(move |buffer, selection| {
            // A non-empty selection is the target, whichever key was pressed.
            if !selection.is_empty() {
                let (start, end) = selection.range();
                buffer.replace_range(start, end, "");
                return start;
            }
            let head = selection.head;
            let line = lines.get(head.line).cloned().unwrap_or_default();
            match direction {
                Direction::Backward => {
                    let target = if by_word {
                        typ_buffer::previous_word_boundary(&line, head.col)
                    } else {
                        head.col.saturating_sub(1)
                    };
                    if head.col == 0 {
                        // Joining with the previous line.
                        let previous = head.line.saturating_sub(1);
                        let col = lines.get(previous).map_or(0, |l| {
                            unicode_segmentation::UnicodeSegmentation::graphemes(l.as_str(), true)
                                .count()
                        });
                        buffer.delete_before(head);
                        return Position { line: previous, col };
                    }
                    buffer.replace_range(
                        Position { line: head.line, col: target },
                        head,
                        "",
                    );
                    Position { line: head.line, col: target }
                }
                Direction::Forward => {
                    if by_word {
                        let target = typ_buffer::next_word_boundary(&line, head.col);
                        buffer.replace_range(head, Position { line: head.line, col: target }, "");
                    } else {
                        buffer.delete_after(head);
                    }
                    head
                }
            }
        })
    }

    /// Pull every selection back inside the text after the buffer changed
    /// underneath it — undo and redo can shrink what a selection covered.
    fn clamp_selections(&mut self) {
        let last_line = self.last_line();
        let line_len: Vec<usize> = (0..=last_line).map(|i| self.line_grapheme_count(i)).collect();
        let clamp = |p: Position| {
            let line = p.line.min(last_line);
            Position { line, col: p.col.min(line_len[line]) }
        };
        self.selections.map_in_place(|s| Selection {
            anchor: clamp(s.anchor),
            head: clamp(s.head),
        });
        self.goal_col = None;
    }
```

- [ ] **Step 4: Add edit grouping to the buffer**

`TextBuffer` currently snapshots on every mutation. Add grouping in
`crates/typ-buffer/src/buffer.rs`:

```rust
    /// Suppress per-edit snapshots until `end_edit_group`, so a multi-caret
    /// edit is a single undo step.
    pub fn begin_edit_group(&mut self) {
        if self.group_depth == 0 {
            self.history.record(self.rope.to_string());
        }
        self.group_depth += 1;
    }

    pub fn end_edit_group(&mut self) {
        self.group_depth = self.group_depth.saturating_sub(1);
    }
```

Add `group_depth: usize` to the struct, initialised to `0` in both constructors, and guard
every existing `self.history.record(...)` call with:

```rust
        if self.group_depth == 0 {
            self.history.record(self.rope.to_string());
        }
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p typ-buffer -p typ-panel-editor`

Expected: PASS — 14 new edit tests, and the existing buffer suite unchanged.

If `multi_caret_edits_on_one_line_do_not_corrupt_each_other` fails with the insertions
landing at the wrong columns, the loop is running forwards; it must be `.rev()`.

- [ ] **Step 6: Commit**

```bash
git add crates/typ-panel-editor/src/actions.rs crates/typ-panel-editor/tests/edit.rs crates/typ-buffer/src/buffer.rs
git commit -m "feat(editor): edits apply at every selection as one undo step"
```

---

### Task 9: Multi-cursor and selection commands

**Files:**
- Modify: `crates/typ-panel-editor/src/actions.rs`
- Create: `crates/typ-panel-editor/tests/multicursor.rs`

**Interfaces:**
- Consumes: `typ_core::{Action, Direction}`, `typ_buffer::word_at`
- Produces: `Action::{SelectAll, SelectLine, CollapseSelections, AddCursor}` handled

- [ ] **Step 1: Write the failing test**

`crates/typ-panel-editor/tests/multicursor.rs`:

```rust
use typ_buffer::{Position, Selection};
use typ_core::{Action, Direction, Motion, Panel};
use typ_panel_editor::EditorPanel;

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

#[test]
fn select_all_covers_the_whole_document_as_one_selection() {
    let mut p = EditorPanel::from_str("ab\ncd\n");
    p.apply_action(Action::SelectAll);
    assert_eq!(p.selections().len(), 1);
    let (start, end) = p.selections().primary().range();
    assert_eq!(start, pos(0, 0));
    assert_eq!(end, pos(2, 0), "the trailing newline leaves a final empty line");
}

#[test]
fn select_line_covers_the_current_line_without_its_newline() {
    let mut p = EditorPanel::from_str("abc\ndef\n");
    p.set_selections_for_test(vec![Selection::caret(pos(1, 2))]);
    p.apply_action(Action::SelectLine);
    assert_eq!(p.selections().primary().range(), (pos(1, 0), pos(1, 3)));
}

#[test]
fn adding_a_cursor_below_puts_one_on_the_next_line() {
    let mut p = EditorPanel::from_str("abc\ndef\nghi\n");
    p.apply_action(Action::AddCursor(Direction::Forward));
    assert_eq!(p.selections().len(), 2);
    let heads: Vec<Position> = p.selections().iter().map(|s| s.head).collect();
    assert_eq!(heads, vec![pos(0, 0), pos(1, 0)]);
}

#[test]
fn the_added_cursor_becomes_primary_so_repeating_extends_downwards() {
    let mut p = EditorPanel::from_str("a\nb\nc\nd\n");
    p.apply_action(Action::AddCursor(Direction::Forward));
    p.apply_action(Action::AddCursor(Direction::Forward));
    let heads: Vec<Position> = p.selections().iter().map(|s| s.head).collect();
    assert_eq!(heads, vec![pos(0, 0), pos(1, 0), pos(2, 0)]);
}

#[test]
fn adding_a_cursor_above_walks_upwards() {
    let mut p = EditorPanel::from_str("abc\ndef\n");
    p.set_selections_for_test(vec![Selection::caret(pos(1, 1))]);
    p.apply_action(Action::AddCursor(Direction::Backward));
    let heads: Vec<Position> = p.selections().iter().map(|s| s.head).collect();
    assert_eq!(heads, vec![pos(0, 1), pos(1, 1)]);
}

#[test]
fn adding_a_cursor_past_the_end_of_the_document_adds_nothing() {
    let mut p = EditorPanel::from_str("only\n");
    p.set_selections_for_test(vec![Selection::caret(pos(1, 0))]);
    p.apply_action(Action::AddCursor(Direction::Forward));
    assert_eq!(p.selections().len(), 1);
}

#[test]
fn a_cursor_added_to_a_shorter_line_clamps_to_that_line() {
    let mut p = EditorPanel::from_str("abcdef\nab\n");
    p.set_selections_for_test(vec![Selection::caret(pos(0, 5))]);
    p.apply_action(Action::AddCursor(Direction::Forward));
    let heads: Vec<Position> = p.selections().iter().map(|s| s.head).collect();
    assert_eq!(heads, vec![pos(0, 5), pos(1, 2)]);
}

#[test]
fn collapse_leaves_one_caret_at_the_primary_head() {
    let mut p = EditorPanel::from_str("abc\ndef\n");
    p.apply_action(Action::AddCursor(Direction::Forward));
    p.apply_action(Action::CollapseSelections);
    assert_eq!(p.selections().len(), 1);
    assert_eq!(p.cursor(), pos(1, 0));
}

#[test]
fn collapse_also_drops_a_selection_down_to_a_caret() {
    let mut p = EditorPanel::from_str("abcdef\n");
    p.apply_action(Action::SelectAll);
    p.apply_action(Action::CollapseSelections);
    assert!(p.selections().primary().is_empty());
}

#[test]
fn typing_with_several_cursors_then_collapsing_keeps_the_text() {
    let mut p = EditorPanel::from_str("a\na\n");
    p.apply_action(Action::AddCursor(Direction::Forward));
    p.apply_action(Action::InsertChar('!'));
    p.apply_action(Action::CollapseSelections);
    assert_eq!(p.line_text(0), "!a");
    assert_eq!(p.line_text(1), "!a");
    assert_eq!(p.selections().len(), 1);
}

#[test]
fn a_motion_that_merges_two_cursors_leaves_one() {
    let mut p = EditorPanel::from_str("ab\n");
    p.set_selections_for_test(vec![
        Selection::caret(pos(0, 0)),
        Selection::caret(pos(0, 1)),
    ]);
    // Both run into the start of the line and become the same caret.
    p.apply_action(Action::Move { motion: Motion::LineStart, extend: false });
    assert_eq!(p.selections().len(), 1);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p typ-panel-editor --test multicursor`

Expected: FAIL — these actions fall through `perform`'s catch-all and do nothing.

- [ ] **Step 3: Write the implementation**

Add these arms to `perform` in `crates/typ-panel-editor/src/actions.rs`:

```rust
            Action::SelectAll => {
                let last = self.last_line();
                self.selections.set_single(Selection {
                    anchor: Position { line: 0, col: 0 },
                    head: Position { line: last, col: self.line_grapheme_count(last) },
                });
                self.goal_col = None;
                vec![PanelEvent::NeedsRedraw]
            }

            Action::SelectLine => {
                let line = self.cursor().line;
                // Without the newline: selecting it would make the next
                // keystroke eat the line break, which is not what "select this
                // line" means to anyone.
                self.selections.set_single(Selection {
                    anchor: Position { line, col: 0 },
                    head: Position { line, col: self.line_grapheme_count(line) },
                });
                self.goal_col = None;
                vec![PanelEvent::NeedsRedraw]
            }

            Action::CollapseSelections => {
                self.selections.collapse_to_heads();
                self.goal_col = None;
                self.scroll_to_cursor();
                vec![PanelEvent::NeedsRedraw]
            }

            Action::AddCursor(direction) => {
                let from = self.selections.primary().head;
                let target_line = match direction {
                    typ_core::Direction::Backward => from.line.checked_sub(1),
                    typ_core::Direction::Forward => {
                        let next = from.line + 1;
                        (next <= self.last_line()).then_some(next)
                    }
                };
                let Some(line) = target_line else {
                    // At the edge of the document there is nowhere to add one.
                    // Silently doing nothing is right: the alternative is
                    // stacking a duplicate cursor on the line already held.
                    return Vec::new();
                };
                let col = from.col.min(self.line_grapheme_count(line));
                self.selections.push(Selection::caret(Position { line, col }));
                self.scroll_to_cursor();
                vec![PanelEvent::NeedsRedraw]
            }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p typ-panel-editor --test multicursor`

Expected: PASS, 11 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/typ-panel-editor/src/actions.rs crates/typ-panel-editor/tests/multicursor.rs
git commit -m "feat(editor): select all, select line, collapse, and stacked cursors"
```

---

### Task 10: Mouse selection

**Files:**
- Modify: `crates/typ-panel-editor/src/lib.rs`
- Create: `crates/typ-panel-editor/tests/mouse.rs`

**Interfaces:**
- Consumes: `crossterm::event::{MouseEvent, MouseEventKind}`, `typ_buffer::word_at`
- Produces: drag-to-select, alt-click to add a cursor, double-click to select a word

- [ ] **Step 1: Write the failing test**

`crates/typ-panel-editor/tests/mouse.rs`:

```rust
use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use typ_buffer::Position;
use typ_core::Panel;
use typ_panel_editor::EditorPanel;

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

const AREA: Rect = Rect { x: 0, y: 0, width: 40, height: 10 };

fn at(kind: MouseEventKind, column: u16, row: u16, modifiers: KeyModifiers) -> MouseEvent {
    MouseEvent { kind, column, row, modifiers }
}

fn down(column: u16, row: u16) -> MouseEvent {
    at(
        MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        KeyModifiers::NONE,
    )
}

fn drag(column: u16, row: u16) -> MouseEvent {
    at(
        MouseEventKind::Drag(MouseButton::Left),
        column,
        row,
        KeyModifiers::NONE,
    )
}

#[test]
fn a_click_places_a_caret_and_clears_any_selection() {
    let mut p = EditorPanel::from_str("hello\nworld\n");
    p.handle_mouse(down(3, 2), AREA);
    assert_eq!(p.cursor(), pos(1, 2));
    assert!(p.selections().primary().is_empty());
    assert_eq!(p.selections().len(), 1);
}

#[test]
fn dragging_extends_from_where_the_press_landed() {
    let mut p = EditorPanel::from_str("hello world\n");
    p.handle_mouse(down(1, 1), AREA);
    p.handle_mouse(drag(6, 1), AREA);
    let s = p.selections().primary();
    assert_eq!(s.anchor, pos(0, 0));
    assert_eq!(s.head, pos(0, 5));
}

#[test]
fn dragging_backwards_selects_the_same_text() {
    let mut p = EditorPanel::from_str("hello world\n");
    p.handle_mouse(down(6, 1), AREA);
    p.handle_mouse(drag(1, 1), AREA);
    assert_eq!(p.selections().primary().range(), (pos(0, 0), pos(0, 5)));
}

#[test]
fn dragging_across_lines_selects_across_them() {
    let mut p = EditorPanel::from_str("abc\ndef\n");
    p.handle_mouse(down(2, 1), AREA);
    p.handle_mouse(drag(2, 2), AREA);
    assert_eq!(p.selections().primary().range(), (pos(0, 1), pos(1, 1)));
}

#[test]
fn a_drag_without_a_press_does_not_start_a_selection() {
    let mut p = EditorPanel::from_str("hello\n");
    p.handle_mouse(drag(4, 1), AREA);
    assert!(p.selections().primary().is_empty());
}

#[test]
fn alt_click_adds_a_cursor_instead_of_replacing_the_one_there() {
    let mut p = EditorPanel::from_str("abc\ndef\n");
    p.handle_mouse(down(1, 1), AREA);
    p.handle_mouse(
        at(
            MouseEventKind::Down(MouseButton::Left),
            2,
            2,
            KeyModifiers::ALT,
        ),
        AREA,
    );
    assert_eq!(p.selections().len(), 2);
    let heads: Vec<Position> = p.selections().iter().map(|s| s.head).collect();
    assert_eq!(heads, vec![pos(0, 0), pos(1, 1)]);
}

#[test]
fn a_second_click_in_the_same_place_selects_the_word() {
    let mut p = EditorPanel::from_str("let value = 1;\n");
    p.handle_mouse(down(6, 1), AREA);
    p.handle_mouse(down(6, 1), AREA);
    assert_eq!(p.selections().primary().range(), (pos(0, 4), pos(0, 9)));
}

#[test]
fn a_second_click_somewhere_else_is_just_another_click() {
    let mut p = EditorPanel::from_str("let value = 1;\n");
    p.handle_mouse(down(6, 1), AREA);
    p.handle_mouse(down(2, 1), AREA);
    assert!(p.selections().primary().is_empty());
    assert_eq!(p.cursor(), pos(0, 1));
}

#[test]
fn releasing_the_button_ends_the_drag() {
    let mut p = EditorPanel::from_str("hello world\n");
    p.handle_mouse(down(1, 1), AREA);
    p.handle_mouse(
        at(
            MouseEventKind::Up(MouseButton::Left),
            6,
            1,
            KeyModifiers::NONE,
        ),
        AREA,
    );
    p.handle_mouse(drag(9, 1), AREA);
    // The drag after the release must not keep extending.
    assert_eq!(p.selections().primary().head, pos(0, 5));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p typ-panel-editor --test mouse`

Expected: FAIL — `handle_mouse` currently ignores drags and modifiers.

- [ ] **Step 3: Write the implementation**

Add drag state to `EditorPanel` in `crates/typ-panel-editor/src/lib.rs`:

```rust
    /// Where the current drag began, and what was last clicked, so a second
    /// click in the same cell can mean "select the word".
    drag_anchor: Option<Position>,
    last_click: Option<Position>,
```

Both start as `None`. Replace `handle_mouse` with:

```rust
    fn handle_mouse(&mut self, event: MouseEvent, panel_area: Rect) -> Vec<PanelEvent> {
        let inner = Self::text_area(panel_area);
        let position = |panel: &Self, event: &MouseEvent| {
            let row = event.row.saturating_sub(inner.y) as usize;
            let col = event.column.saturating_sub(inner.x) as usize;
            let line = (panel.top_line + row).min(panel.last_line());
            Position {
                line,
                col: display_to_grapheme_col(&panel.buffer.line_text(line), col, TAB_WIDTH),
            }
        };

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let at = position(self, &event);

                if event.modifiers.contains(KeyModifiers::ALT) {
                    // Alt+click stacks a cursor. This is the mouse half of
                    // multi-cursor; the keyboard half is Action::AddCursor.
                    self.selections.push(Selection::caret(at));
                    self.last_click = Some(at);
                    self.drag_anchor = Some(at);
                    return vec![PanelEvent::NeedsRedraw];
                }

                if self.last_click == Some(at) {
                    // Second click in the same cell: select the word under it.
                    // No timing check — a click in the same cell is a
                    // deliberate second click, and timing would need a clock on
                    // the render path.
                    let text = self.buffer.line_text(at.line);
                    if let Some((start, end)) = typ_buffer::word_at(&text, at.col) {
                        self.selections.set_single(Selection {
                            anchor: Position { line: at.line, col: start },
                            head: Position { line: at.line, col: end },
                        });
                        self.drag_anchor = None;
                        return vec![PanelEvent::NeedsRedraw];
                    }
                }

                self.selections.set_single(Selection::caret(at));
                self.drag_anchor = Some(at);
                self.last_click = Some(at);
                self.goal_col = None;
                vec![PanelEvent::NeedsRedraw]
            }

            MouseEventKind::Drag(MouseButton::Left) => {
                let Some(anchor) = self.drag_anchor else {
                    return Vec::new();
                };
                let head = position(self, &event);
                self.selections.set_single(Selection { anchor, head });
                self.goal_col = None;
                vec![PanelEvent::NeedsRedraw]
            }

            MouseEventKind::Up(MouseButton::Left) => {
                self.drag_anchor = None;
                Vec::new()
            }

            _ => Vec::new(),
        }
    }
```

Add `KeyModifiers` to the crossterm import.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p typ-panel-editor`

Expected: PASS — 9 new mouse tests, and the existing click tests from M1 still pass because a
plain press still places a caret.

- [ ] **Step 5: Commit**

```bash
git add crates/typ-panel-editor/src/lib.rs crates/typ-panel-editor/tests/mouse.rs
git commit -m "feat(editor): drag to select, alt-click to stack cursors, click twice for a word"
```

---

### Task 11: Horizontal scrolling

**Files:**
- Modify: `crates/typ-panel-editor/src/lib.rs`, `crates/typ-panel-editor/src/render.rs`
- Create: `crates/typ-panel-editor/tests/horizontal.rs`

**Interfaces:**
- Consumes: `typ_buffer::{display_width_with_tabs, grapheme_to_display_col}`
- Produces: `EditorPanel::left_col(&self) -> usize`, horizontal windowing in `render`

- [ ] **Step 1: Write the failing test**

`crates/typ-panel-editor/tests/horizontal.rs`:

```rust
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_core::{Action, Motion, Panel, RenderContext, ThemeColors};
use typ_panel_editor::EditorPanel;

fn render(panel: &mut EditorPanel, area: Rect) -> Buffer {
    let theme = ThemeColors::default();
    let ctx = RenderContext {
        theme: &theme,
        is_focused: true,
        panel_index: 0,
        terminal_width: area.width,
        terminal_height: area.height,
    };
    let mut buf = Buffer::empty(area);
    panel.render(area, &mut buf, &ctx);
    buf
}

fn row(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
}

const AREA: Rect = Rect { x: 0, y: 0, width: 12, height: 4 };

#[test]
fn a_short_line_is_not_scrolled() {
    let mut p = EditorPanel::from_str("abc\n");
    let buf = render(&mut p, AREA);
    assert_eq!(p.left_col(), 0);
    assert_eq!(row(&buf, 1), "│abc       │");
}

#[test]
fn moving_past_the_right_edge_scrolls_the_view() {
    let mut p = EditorPanel::from_str("abcdefghijklmnop\n");
    render(&mut p, AREA); // learn the width: 12 minus borders = 10 columns
    p.apply_action(Action::Move { motion: Motion::LineEnd, extend: false });
    let buf = render(&mut p, AREA);
    assert!(p.left_col() > 0, "the view must follow the cursor");
    assert!(
        row(&buf, 1).contains('p'),
        "the end of the line must be visible: {}",
        row(&buf, 1)
    );
}

#[test]
fn coming_back_left_scrolls_the_view_back() {
    let mut p = EditorPanel::from_str("abcdefghijklmnop\n");
    render(&mut p, AREA);
    p.apply_action(Action::Move { motion: Motion::LineEnd, extend: false });
    render(&mut p, AREA);
    p.apply_action(Action::Move { motion: Motion::LineStart, extend: false });
    let buf = render(&mut p, AREA);
    assert_eq!(p.left_col(), 0);
    assert!(row(&buf, 1).contains('a'));
}

#[test]
fn the_cursor_is_reported_within_the_visible_window() {
    let mut p = EditorPanel::from_str("abcdefghijklmnop\n");
    render(&mut p, AREA);
    p.apply_action(Action::Move { motion: Motion::LineEnd, extend: false });
    render(&mut p, AREA);
    let (x, _) = p.cursor_position(AREA).expect("the cursor is on screen");
    assert!((1..11).contains(&x), "cursor x was {x}");
}

#[test]
fn a_wide_character_is_not_split_across_the_left_edge() {
    let mut p = EditorPanel::from_str("日本語日本語日本語\n");
    render(&mut p, AREA);
    p.apply_action(Action::Move { motion: Motion::LineEnd, extend: false });
    let buf = render(&mut p, AREA);
    let text = row(&buf, 1);
    // A half-drawn CJK cell would show as a stray blank at the left edge.
    assert!(
        !text.starts_with("│ "),
        "a wide grapheme was cut in half: {text}"
    );
}

#[test]
fn vertical_scrolling_still_works_alongside_it() {
    let text = (0..50).map(|i| format!("line {i}\n")).collect::<String>();
    let mut p = EditorPanel::from_str(&text);
    render(&mut p, AREA);
    p.handle_scroll(5, AREA);
    let buf = render(&mut p, AREA);
    assert!(row(&buf, 1).contains("line 5"));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p typ-panel-editor --test horizontal`

Expected: FAIL — no method `left_col`.

- [ ] **Step 3: Write the implementation**

Add `left_col: usize` to `EditorPanel` (initialised to `0`) and its accessor:

```rust
    pub fn left_col(&self) -> usize {
        self.left_col
    }
```

Add `width: usize` to `EditorPanel` beside the existing `height`, initialised to `0`, and set
it in `Panel::render` from the inner area:

```rust
        self.height = inner.height as usize;
        self.width = inner.width as usize;
```

Both are learned at render time for the same reason: a panel does not know its size until it
is asked to draw. Extend `scroll_to_cursor` to handle the horizontal axis:

```rust
    pub(crate) fn scroll_to_cursor(&mut self) {
        let cursor = self.cursor();
        if self.height > 0 {
            if cursor.line < self.top_line {
                self.top_line = cursor.line;
            } else if cursor.line >= self.top_line + self.height {
                self.top_line = cursor.line - self.height + 1;
            }
        }
        if self.width > 0 {
            let col = grapheme_to_display_col(
                &self.buffer.line_text(cursor.line),
                cursor.col,
                TAB_WIDTH,
            );
            if col < self.left_col {
                self.left_col = col;
            } else if col >= self.left_col + self.width {
                // Keep the cursor one column inside the right edge so the
                // character being typed is visible rather than flush against
                // the border.
                self.left_col = col + 1 - self.width;
            }
        }
    }
```

In `render.rs`, add horizontal windowing. Slicing by display column rather than by grapheme
is what keeps a wide character from being cut in half at the left edge:

```rust
/// Drop `left_col` display columns from the front of a line.
///
/// A wide grapheme straddling the boundary is dropped entirely rather than
/// half-drawn: terminals cannot render half a cell, and the alternative is a
/// row that is one column narrower than every other row.
pub fn window(text: &str, left_col: usize, tab_width: usize) -> (String, usize) {
    if left_col == 0 {
        return (text.to_string(), 0);
    }
    let mut column = 0usize;
    let mut out = String::new();
    let mut skipped = 0usize;
    for grapheme in text.graphemes(true) {
        let width = typ_buffer::display_width_with_tabs(grapheme, tab_width).max(1);
        if column + width <= left_col {
            column += width;
            skipped += 1;
            continue;
        }
        if column < left_col {
            // Straddles the edge: skip it and note the gap.
            column += width;
            skipped += 1;
            continue;
        }
        out.push_str(grapheme);
        column += width;
    }
    (out, skipped)
}
```

`styled_line` takes `left_col`, calls `window` first, and offsets the grapheme index it
compares against selections by the number of skipped graphemes, so highlighting stays aligned
with the text after scrolling. `cursor_position` subtracts `left_col` from the display column
and returns `None` when the result is negative or past the width.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p typ-panel-editor`

Expected: PASS — 6 new horizontal tests plus everything before.

- [ ] **Step 5: Commit**

```bash
git add crates/typ-panel-editor/src crates/typ-panel-editor/tests/horizontal.rs
git commit -m "feat(editor): horizontal scrolling that never splits a wide grapheme"
```

---

### Task 12: The app routes keys through the keymap

**Files:**
- Modify: `crates/typ-app/src/app.rs`, `crates/typ-app/src/run.rs`, `crates/typ-app/Cargo.toml`
- Create: `crates/typ-app/tests/dispatch.rs`

**Interfaces:**
- Consumes: `typ_core::{Action, Keymap}`
- Produces:
  - `App::keymap(&self) -> &Keymap`, `App::set_keymap(&mut self, keymap: Keymap)`
  - `App::handle_chord(&mut self, chord: KeyChord) -> anyhow::Result<()>`

- [ ] **Step 1: Write the failing test**

`crates/typ-app/tests/dispatch.rs`:

```rust
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_app::App;
use typ_core::{Action, KeyChord, Keymap, Motion};

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-dispatch-test").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hello.rs"), "fn main() {}\n").unwrap();
    dir
}

fn chord(code: KeyCode, mods: KeyModifiers) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, mods))
}

fn app_with_file(name: &str) -> App {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("hello.rs")).unwrap();
    app
}

#[test]
fn a_bound_chord_reaches_the_focused_panel() {
    let mut app = app_with_file("bound");
    app.handle_chord(chord(KeyCode::Right, KeyModifiers::NONE)).unwrap();
    assert_eq!(app.editor_mut().cursor().col, 1);
}

#[test]
fn an_unbound_printable_character_is_typed() {
    let mut app = app_with_file("typing");
    app.handle_chord(chord(KeyCode::Char('x'), KeyModifiers::NONE)).unwrap();
    assert_eq!(app.editor_mut().line_text(0), "xfn main() {}");
}

#[test]
fn a_control_chord_with_no_binding_types_nothing() {
    let mut app = app_with_file("unbound-ctrl");
    app.handle_chord(chord(KeyCode::Char('j'), KeyModifiers::CONTROL)).unwrap();
    assert_eq!(app.editor_mut().line_text(0), "fn main() {}");
}

#[test]
fn tab_cycles_focus_rather_than_reaching_the_panel() {
    let mut app = app_with_file("focus");
    assert_eq!(app.focused_name(), "editor");
    app.handle_chord(chord(KeyCode::Tab, KeyModifiers::NONE)).unwrap();
    assert_eq!(app.focused_name(), "tree");
}

#[test]
fn quit_is_handled_by_the_app_and_still_guards_unsaved_work() {
    let mut app = app_with_file("quit");
    app.handle_chord(chord(KeyCode::Char('x'), KeyModifiers::NONE)).unwrap();
    app.handle_chord(chord(KeyCode::Char('q'), KeyModifiers::CONTROL)).unwrap();
    assert!(!app.should_quit(), "unsaved changes must still prompt");
    app.handle_chord(chord(KeyCode::Char('q'), KeyModifiers::CONTROL)).unwrap();
    assert!(app.should_quit());
}

#[test]
fn save_reports_through_the_status_bar() {
    let mut app = app_with_file("save");
    app.handle_chord(chord(KeyCode::Char('x'), KeyModifiers::NONE)).unwrap();
    app.handle_chord(chord(KeyCode::Char('s'), KeyModifiers::CONTROL)).unwrap();
    assert_eq!(app.status(), Some("Saved."));
}

#[test]
fn a_rebound_key_takes_effect() {
    let mut app = app_with_file("rebind");
    let mut keymap = Keymap::default_bindings();
    keymap.merge_toml("\"ctrl+e\" = \"move_line_end\"").unwrap();
    app.set_keymap(keymap);
    app.handle_chord(chord(KeyCode::Char('e'), KeyModifiers::CONTROL)).unwrap();
    assert_eq!(app.editor_mut().cursor().col, 12);
}

#[test]
fn an_action_the_panel_ignores_falls_through_to_the_app() {
    let mut app = app_with_file("fallthrough");
    // The tree has no Undo, so it must not swallow it.
    app.cycle_focus();
    assert_eq!(app.focused_name(), "tree");
    app.handle_chord(chord(KeyCode::Char('z'), KeyModifiers::CONTROL)).unwrap();
    assert!(app.status().is_none(), "a no-op must not report an error");
}

#[test]
fn the_keymap_is_readable_for_help_text() {
    let app = app_with_file("help");
    assert!(app.keymap().bindings_for(Action::Save).contains(&"ctrl+s"));
    assert!(
        app.keymap()
            .bindings_for(Action::Move { motion: Motion::Left, extend: false })
            .contains(&"left")
    );
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p typ-app --test dispatch`

Expected: FAIL — no method `handle_chord`, `set_keymap`, or `keymap`.

- [ ] **Step 3: Write the implementation**

Add `keymap: Keymap` to `App`, initialised with `Keymap::default_bindings()`, plus the
accessors. Then the dispatcher, which is the piece that replaces the `match key.code` block
in `run.rs`:

```rust
    /// Route one keypress.
    ///
    /// Order matters and is deliberate: a bound chord becomes an action, the
    /// focused panel gets first refusal on it, and only then does the app try
    /// it. Anything unbound and printable is text. A chord carrying Ctrl or Alt
    /// is never text — that is what stops an unbound Ctrl+J from typing a `j`.
    pub fn handle_chord(&mut self, chord: KeyChord) -> Result<()> {
        if !(chord.canonical == "ctrl+q") {
            self.clear_transient();
        }

        if let Some(action) = self.keymap.lookup(&chord) {
            let events = self.focused_mut().apply_action(action);
            if events.is_empty() {
                return self.perform_app_action(action);
            }
            return self.apply(events);
        }

        let is_chorded = chord
            .raw
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
        if let KeyCode::Char(c) = chord.raw.code
            && !is_chorded
        {
            let events = self.focused_mut().apply_action(Action::InsertChar(c));
            return self.apply(events);
        }
        Ok(())
    }

    /// Actions no panel claimed.
    fn perform_app_action(&mut self, action: Action) -> Result<()> {
        match action {
            Action::FocusNext => self.cycle_focus(),
            Action::Quit => self.request_quit(),
            Action::Save => match self.editor.save() {
                Ok(()) => self.status = Some("Saved.".to_string()),
                // A save that fails silently is how work gets lost.
                Err(e) => self.status = Some(format!("Save failed: {e:#}")),
            },
            // Everything else is a panel's business; a panel that ignored it
            // meant to ignore it.
            _ => {}
        }
        Ok(())
    }
```

`Focus::Tree` must not swallow `Action::Save`: `TreePanel` leaves `apply_action` defaulted, so
it returns no events and the app handles it. That is the whole reason the default returns an
empty vector rather than a redraw.

In `run.rs`, the key branch collapses to:

```rust
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                app.handle_chord(KeyChord::from_event(key))?;
            }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p typ-app`

Expected: PASS — 9 new dispatch tests plus the existing app, status and frame suites.

The frame tests may need their expected status hint updated if `HINT` changed; it should not
have. If `save_reports_through_the_status_bar` fails with `None`, the editor panel is
claiming `Action::Save` — it must not implement that arm.

- [ ] **Step 5: Commit**

```bash
git add crates/typ-app/src crates/typ-app/tests/dispatch.rs
git commit -m "feat(app): dispatch keys through the keymap into named actions"
```

---

### Task 13: The status-bar prompt

**Files:**
- Create: `crates/typ-app/src/prompt.rs`, `crates/typ-app/tests/prompt.rs`
- Modify: `crates/typ-app/src/app.rs`, `crates/typ-app/src/lib.rs`

**Interfaces:**
- Consumes: `typ_core::KeyChord`
- Produces:
  - `typ_app::prompt::{Prompt, PromptKind}`
  - `Prompt::{new, input, kind, label, insert_char, delete_backward, take_input,
    set_pending_needle, pending_needle, become_replace_after_needle, become_replace,
    is_replace_flow}`
  - `App::prompt(&self) -> Option<&Prompt>`

- [ ] **Step 1: Write the failing test**

`crates/typ-app/tests/prompt.rs`:

```rust
use typ_app::prompt::{Prompt, PromptKind};

#[test]
fn a_new_prompt_starts_empty_and_knows_what_it_is_for() {
    let prompt = Prompt::new(PromptKind::Search);
    assert_eq!(prompt.input(), "");
    assert_eq!(prompt.kind(), PromptKind::Search);
}

#[test]
fn typing_accumulates_into_the_input() {
    let mut prompt = Prompt::new(PromptKind::Search);
    prompt.insert_char('f');
    prompt.insert_char('n');
    assert_eq!(prompt.input(), "fn");
}

#[test]
fn backspace_removes_a_whole_grapheme() {
    let mut prompt = Prompt::new(PromptKind::Search);
    for c in "日本".chars() {
        prompt.insert_char(c);
    }
    prompt.delete_backward();
    assert_eq!(prompt.input(), "日");
}

#[test]
fn backspace_on_an_empty_prompt_is_harmless() {
    let mut prompt = Prompt::new(PromptKind::Search);
    prompt.delete_backward();
    assert_eq!(prompt.input(), "");
}

#[test]
fn the_label_says_which_prompt_this_is() {
    assert_eq!(Prompt::new(PromptKind::Search).label(), "Search:");
    assert_eq!(Prompt::new(PromptKind::Replace).label(), "Replace with:");
}

#[test]
fn a_replace_prompt_asks_the_second_question_in_place() {
    let mut prompt = Prompt::new(PromptKind::Search);
    prompt.become_replace_after_needle();
    assert!(prompt.is_replace_flow());
    prompt.set_pending_needle(prompt.take_input());
    prompt.become_replace();
    assert_eq!(prompt.kind(), PromptKind::Replace);
    assert_eq!(prompt.label(), "Replace with:");
    assert!(!prompt.is_replace_flow(), "the flow is spent once the needle is banked");
}

#[test]
fn taking_the_input_leaves_the_prompt_empty() {
    let mut prompt = Prompt::new(PromptKind::Search);
    prompt.insert_char('a');
    assert_eq!(prompt.take_input(), "a");
    assert_eq!(prompt.input(), "");
}
```

`crates/typ-app/tests/search_flow.rs`:

```rust
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_app::App;
use typ_core::KeyChord;

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-search-flow").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hits.txt"), "alpha\nbeta alpha\ngamma\n").unwrap();
    dir
}

fn chord(code: KeyCode, mods: KeyModifiers) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, mods))
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        app.handle_chord(chord(KeyCode::Char(c), KeyModifiers::NONE)).unwrap();
    }
}

fn app_with_hits(name: &str) -> App {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("hits.txt")).unwrap();
    app
}

#[test]
fn ctrl_f_opens_a_search_prompt() {
    let mut app = app_with_hits("open");
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL)).unwrap();
    assert!(app.prompt().is_some());
}

#[test]
fn typing_in_the_prompt_does_not_reach_the_buffer() {
    let mut app = app_with_hits("capture");
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL)).unwrap();
    typed(&mut app, "alpha");
    assert_eq!(app.editor_mut().line_text(0), "alpha", "the file is unchanged");
    assert_eq!(app.prompt().unwrap().input(), "alpha");
}

#[test]
fn enter_jumps_to_the_first_match_after_the_cursor() {
    let mut app = app_with_hits("jump");
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL)).unwrap();
    typed(&mut app, "alpha");
    app.handle_chord(chord(KeyCode::Enter, KeyModifiers::NONE)).unwrap();
    assert!(app.prompt().is_none(), "the prompt closes on Enter");
    assert_eq!(app.editor_mut().cursor().line, 0);
    assert!(!app.editor_mut().selections().primary().is_empty(), "the match is selected");
}

#[test]
fn search_next_walks_through_the_matches_and_wraps() {
    let mut app = app_with_hits("walk");
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL)).unwrap();
    typed(&mut app, "alpha");
    app.handle_chord(chord(KeyCode::Enter, KeyModifiers::NONE)).unwrap();
    app.handle_chord(chord(KeyCode::F(3), KeyModifiers::NONE)).unwrap();
    assert_eq!(app.editor_mut().cursor().line, 1);
    app.handle_chord(chord(KeyCode::F(3), KeyModifiers::NONE)).unwrap();
    assert_eq!(app.editor_mut().cursor().line, 0, "wraps to the top");
}

#[test]
fn a_search_with_no_matches_says_so_and_moves_nothing() {
    let mut app = app_with_hits("miss");
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL)).unwrap();
    typed(&mut app, "zeta");
    app.handle_chord(chord(KeyCode::Enter, KeyModifiers::NONE)).unwrap();
    assert_eq!(app.editor_mut().cursor().line, 0);
    assert!(app.status().unwrap().contains("No matches"), "status: {:?}", app.status());
}

#[test]
fn escape_abandons_the_prompt_without_moving_the_cursor() {
    let mut app = app_with_hits("escape");
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL)).unwrap();
    typed(&mut app, "gamma");
    app.handle_chord(chord(KeyCode::Esc, KeyModifiers::NONE)).unwrap();
    assert!(app.prompt().is_none());
    assert_eq!(app.editor_mut().cursor().line, 0);
}

#[test]
fn replace_swaps_every_match_in_one_undo_step() {
    let mut app = app_with_hits("replace");
    app.handle_chord(chord(KeyCode::Char('h'), KeyModifiers::CONTROL)).unwrap();
    typed(&mut app, "alpha");
    app.handle_chord(chord(KeyCode::Enter, KeyModifiers::NONE)).unwrap();
    typed(&mut app, "ALPHA");
    app.handle_chord(chord(KeyCode::Enter, KeyModifiers::NONE)).unwrap();

    assert_eq!(app.editor_mut().line_text(0), "ALPHA");
    assert_eq!(app.editor_mut().line_text(1), "beta ALPHA");

    app.handle_chord(chord(KeyCode::Char('z'), KeyModifiers::CONTROL)).unwrap();
    assert_eq!(app.editor_mut().line_text(0), "alpha");
    assert_eq!(app.editor_mut().line_text(1), "beta alpha", "one undo, both lines");
}

#[test]
fn the_status_bar_shows_the_prompt_while_it_is_open() {
    let mut app = app_with_hits("status");
    app.handle_chord(chord(KeyCode::Char('f'), KeyModifiers::CONTROL)).unwrap();
    typed(&mut app, "al");
    assert_eq!(app.status_left(), "Search: al");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p typ-app --test prompt --test search_flow`

Expected: FAIL — `typ_app::prompt` does not exist.

- [ ] **Step 3: Write the prompt**

`crates/typ-app/src/prompt.rs`:

```rust
//! The status-bar prompt.
//!
//! One line, one purpose at a time. It exists because M1.2 proved the editor
//! needs somewhere to ask a question; search and replace are the second and
//! third questions it asks.

use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    Search,
    /// The needle has been entered; this is collecting the replacement.
    Replace,
}

#[derive(Debug, Clone)]
pub struct Prompt {
    kind: PromptKind,
    input: String,
    /// Set while a replace is collecting its second answer.
    pending_needle: Option<String>,
    /// True when this prompt was opened by Ctrl+H, so answering the needle
    /// leads to a second question rather than to a jump.
    replace_flow: bool,
}

impl Prompt {
    pub fn new(kind: PromptKind) -> Self {
        Self {
            kind,
            input: String::new(),
            pending_needle: None,
            replace_flow: false,
        }
    }

    pub fn kind(&self) -> PromptKind {
        self.kind
    }

    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn label(&self) -> &'static str {
        match self.kind {
            PromptKind::Search => "Search:",
            PromptKind::Replace => "Replace with:",
        }
    }

    pub fn insert_char(&mut self, c: char) {
        self.input.push(c);
    }

    /// Remove one grapheme, not one byte or char — the prompt accepts the same
    /// text the buffer does, including CJK and combining sequences.
    pub fn delete_backward(&mut self) {
        let mut graphemes: Vec<&str> = self.input.graphemes(true).collect();
        graphemes.pop();
        self.input = graphemes.concat();
    }

    pub fn take_input(&mut self) -> String {
        std::mem::take(&mut self.input)
    }

    pub fn set_pending_needle(&mut self, needle: String) {
        self.pending_needle = Some(needle);
    }

    pub fn pending_needle(&self) -> Option<&str> {
        self.pending_needle.as_deref()
    }

    /// Mark this as the first half of a replace, so Enter collects the needle
    /// and asks for the replacement instead of jumping to the match.
    pub fn become_replace_after_needle(&mut self) {
        self.replace_flow = true;
    }

    pub fn is_replace_flow(&self) -> bool {
        self.replace_flow
    }

    /// Move to the second question, keeping the prompt open.
    pub fn become_replace(&mut self) {
        self.kind = PromptKind::Replace;
        self.replace_flow = false;
    }
}
```

- [ ] **Step 4: Wire it into the app**

Add to `App`: `prompt: Option<Prompt>`, `last_query: Option<SearchQuery>`. Add the accessor
`pub fn prompt(&self) -> Option<&Prompt>`.

`handle_chord` grows one branch at the very top, because a prompt captures all input:

```rust
        // An open prompt owns the keyboard. Routing through the keymap first
        // would let a chord bound to an editing action fire while the user is
        // typing a search term.
        if self.prompt.is_some() {
            return self.handle_prompt_chord(chord);
        }
```

```rust
    fn handle_prompt_chord(&mut self, chord: KeyChord) -> Result<()> {
        // Decide first, mutate second. Holding `self.prompt.as_mut()` across an
        // assignment to `self.prompt` does not compile, and threading the
        // borrow through every arm is worse than naming the outcome.
        enum Outcome {
            Stay,
            Close,
            Search(String),
            AskReplacement(String),
            Replace { needle: String, replacement: String },
        }

        let Some(prompt) = self.prompt.as_mut() else {
            return Ok(());
        };

        let outcome = match chord.raw.code {
            KeyCode::Esc => Outcome::Close,
            KeyCode::Backspace => {
                prompt.delete_backward();
                Outcome::Stay
            }
            KeyCode::Char(c) => {
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
            Outcome::Replace { needle, replacement } => {
                self.prompt = None;
                self.run_replace_all(&needle, &replacement);
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

    fn jump_to_match(&mut self, query: &SearchQuery, direction: Direction) {
        let hits = self.editor.buffer_find_all(query);
        if hits.is_empty() {
            self.status = Some(format!("No matches for {}", query.needle));
            return;
        }
        let from = self.editor.cursor();
        let next = match direction {
            Direction::Forward => hits
                .iter()
                .find(|hit| hit.range().0 > from)
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
```

`perform_app_action` gains the search arms:

```rust
            Action::SearchOpen => self.prompt = Some(Prompt::new(PromptKind::Search)),
            Action::ReplaceOpen => {
                let mut prompt = Prompt::new(PromptKind::Search);
                prompt.become_replace_after_needle();
                self.prompt = Some(prompt);
            }
            Action::SearchNext | Action::SearchPrevious => {
                let Some(query) = self.last_query.clone() else {
                    self.status = Some("Nothing to search for yet".to_string());
                    return Ok(());
                };
                let direction = if action == Action::SearchNext {
                    Direction::Forward
                } else {
                    Direction::Backward
                };
                self.jump_to_match(&query, direction);
            }
```

`status_left` shows the prompt when one is open, ahead of any message:

```rust
    pub fn status_left(&self) -> String {
        if let Some(prompt) = &self.prompt {
            return format!("{} {}", prompt.label(), prompt.input());
        }
        self.status.clone().unwrap_or_else(|| HINT.to_string())
    }
```

- [ ] **Step 5: Add the editor methods the app needs**

The app must not reach into `EditorPanel`'s buffer directly — that is the `RenderContext`
rule pointing the other way. Add to `EditorPanel`:

```rust
    pub fn buffer_find_all(&self, query: &SearchQuery) -> Vec<Selection> {
        self.buffer.find_all(query)
    }

    /// Select a range and scroll it into view.
    pub fn select_range(&mut self, selection: Selection) {
        self.selections.set_single(selection);
        self.goal_col = None;
        self.scroll_to_cursor();
    }

    /// Replace every match, as one undo step. Returns how many.
    pub fn replace_all(&mut self, query: &SearchQuery, replacement: &str) -> usize {
        let hits = self.buffer.find_all(query);
        if hits.is_empty() {
            return 0;
        }
        self.buffer.begin_edit_group();
        // Backwards, so each replacement leaves the earlier hits' positions
        // untouched — the same reason multi-caret edits run in reverse.
        for hit in hits.iter().rev() {
            let (start, end) = hit.range();
            self.buffer.replace_range(start, end, replacement);
        }
        self.buffer.end_edit_group();
        self.clamp_selections();
        hits.len()
    }
```

`become_replace_after_needle` is what makes `Ctrl+H` two questions in one prompt: it sets a
flag, and the `PromptKind::Search if prompt.is_replace_flow()` arm above banks the needle,
switches the label to `Replace with:`, and leaves the prompt open. A separate prompt type per
question would double the state for no gain.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p typ-app`

Expected: PASS — 7 prompt tests, 8 search-flow tests, and everything before.

If `typing_in_the_prompt_does_not_reach_the_buffer` fails with the text in the file, the
prompt branch is below the keymap lookup in `handle_chord` instead of above it.

- [ ] **Step 7: Commit**

```bash
git add crates/typ-app/src crates/typ-app/tests/prompt.rs crates/typ-app/tests/search_flow.rs crates/typ-panel-editor/src
git commit -m "feat(app): search and replace through a status-bar prompt"
```

---

### Task 14: Loading `keys.toml`

**Files:**
- Create: `crates/typ-app/src/config.rs`, `crates/typ-app/tests/config.rs`
- Modify: `crates/typ-app/src/lib.rs`, `crates/typ/src/main.rs`

**Interfaces:**
- Consumes: `typ_core::Keymap`
- Produces:
  - `typ_app::config::config_path() -> Option<PathBuf>`
  - `typ_app::config::load_keymap(path: Option<&Path>) -> (Keymap, Option<String>)`

- [ ] **Step 1: Write the failing test**

`crates/typ-app/tests/config.rs`:

```rust
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_app::config::load_keymap;
use typ_core::{Action, KeyChord, Motion};

fn write(name: &str, contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-config-test").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("keys.toml");
    std::fs::write(&path, contents).unwrap();
    path
}

fn chord(code: KeyCode, mods: KeyModifiers) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, mods))
}

#[test]
fn no_config_file_yields_the_defaults_and_no_complaint() {
    let (keymap, warning) = load_keymap(None);
    assert!(warning.is_none());
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Left, KeyModifiers::NONE)),
        Some(Action::Move { motion: Motion::Left, extend: false })
    );
}

#[test]
fn a_missing_file_is_not_an_error() {
    let path = PathBuf::from("does/not/exist/keys.toml");
    let (_, warning) = load_keymap(Some(&path));
    assert!(warning.is_none(), "an absent config is the normal case");
}

#[test]
fn a_valid_config_is_applied_over_the_defaults() {
    let path = write("valid", "\"ctrl+e\" = \"move_line_end\"\n");
    let (keymap, warning) = load_keymap(Some(&path));
    assert!(warning.is_none());
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Char('e'), KeyModifiers::CONTROL)),
        Some(Action::Move { motion: Motion::LineEnd, extend: false })
    );
    // Untouched defaults survive.
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Left, KeyModifiers::NONE)),
        Some(Action::Move { motion: Motion::Left, extend: false })
    );
}

#[test]
fn a_broken_config_warns_and_falls_back_rather_than_refusing_to_start() {
    let path = write("broken", "\"ctrl+e\" = \"summon_daemon\"\n");
    let (keymap, warning) = load_keymap(Some(&path));
    let warning = warning.expect("a broken config must be reported");
    assert!(warning.contains("summon_daemon"), "warning: {warning}");
    // An editor that will not start because of a keybinding typo is a worse
    // editor than one that starts with the defaults and says so.
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Left, KeyModifiers::NONE)),
        Some(Action::Move { motion: Motion::Left, extend: false })
    );
}

#[test]
fn an_unreadable_config_warns_with_the_path_in_the_message() {
    let path = write("unreadable", "not = = toml");
    let (_, warning) = load_keymap(Some(&path));
    let warning = warning.expect("malformed TOML must be reported");
    assert!(warning.contains("keys.toml"), "warning: {warning}");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p typ-app --test config`

Expected: FAIL — `typ_app::config` does not exist.

- [ ] **Step 3: Write the implementation**

`crates/typ-app/src/config.rs`:

```rust
//! Finding and loading user config.
//!
//! Config problems are warnings, never startup failures. An editor that
//! refuses to open because of a typo in a keybinding is an editor you cannot
//! use to fix the typo.

use std::path::{Path, PathBuf};

use typ_core::Keymap;

/// `$TYP_CONFIG_DIR/keys.toml` if set, else the platform config directory.
///
/// The environment variable exists so tests and `$EDITOR` invocations can be
/// isolated from whatever the developer has in their real config.
pub fn config_path() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("TYP_CONFIG_DIR") {
        return Some(PathBuf::from(dir).join("keys.toml"));
    }
    let base = if cfg!(windows) {
        std::env::var("APPDATA").ok()?
    } else if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        xdg
    } else {
        format!("{}/.config", std::env::var("HOME").ok()?)
    };
    Some(PathBuf::from(base).join("typ").join("keys.toml"))
}

/// The keymap, plus a warning if the config existed and could not be used.
pub fn load_keymap(path: Option<&Path>) -> (Keymap, Option<String>) {
    let mut keymap = Keymap::default_bindings();
    let Some(path) = path else {
        return (keymap, None);
    };
    let Ok(source) = std::fs::read_to_string(path) else {
        // No config is the normal case, not a problem worth a message.
        return (keymap, None);
    };
    match keymap.merge_toml(&source) {
        Ok(()) => (keymap, None),
        Err(e) => {
            // merge_toml is all-or-nothing, so the returned keymap is still
            // the untouched defaults rather than a half-applied mixture.
            let warning = format!("{}: {e:#}", path.display());
            (Keymap::default_bindings(), Some(warning))
        }
    }
}
```

- [ ] **Step 4: Load it at startup**

In `crates/typ/src/main.rs`, after building the `App`:

```rust
    let (keymap, warning) = typ_app::config::load_keymap(
        typ_app::config::config_path().as_deref(),
    );
    app.set_keymap(keymap);
    if let Some(warning) = warning {
        // Surfaced in the status bar rather than on stderr: stderr is invisible
        // once the alternate screen is up.
        app.notify(warning);
    }
```

Add `App::notify(&mut self, message: String)` setting `self.status`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p typ-app --test config`

Expected: PASS, 5 tests.

- [ ] **Step 6: Document the file format**

Add to `README.md`, after the key tables:

````markdown
## Configuring keys

Bindings live in `keys.toml`, in `$XDG_CONFIG_HOME/typ/` (or `%APPDATA%\typ\` on Windows).
Set `TYP_CONFIG_DIR` to point somewhere else. Anything you set overrides the default of the
same name; everything else keeps its default.

```toml
# chord = action
"ctrl+e" = "move_line_end"
"ctrl+shift+k" = "delete_word_forward"

# an empty action unbinds a key
"ctrl+l" = ""
```

A binding whose action name is unknown is reported in the status bar at startup, and the
defaults are kept — a typo here never stops the editor opening.
````

- [ ] **Step 7: Commit**

```bash
git add crates/typ-app/src/config.rs crates/typ-app/src/lib.rs crates/typ-app/tests/config.rs crates/typ/src/main.rs README.md
git commit -m "feat(config): load keys.toml, warning rather than failing on a bad one"
```

---

### Task 15: Golden frames for the new surface

**Files:**
- Modify: `crates/typ-app/tests/frame.rs`

**Interfaces:**
- Consumes: everything above
- Produces: assertions covering selection, multi-cursor and prompt rendering

- [ ] **Step 1: Write the failing test**

Add to `crates/typ-app/tests/frame.rs`:

```rust
#[test]
fn a_selection_is_visible_in_the_rendered_frame() {
    let dir = fixture("selection-frame");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("main.rs")).unwrap();
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Right,
        KeyModifiers::SHIFT,
    )))
    .unwrap();
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Right,
        KeyModifiers::SHIFT,
    )))
    .unwrap();

    let terminal = draw(&mut app, 60, 8);
    let buffer = terminal.backend().buffer();
    let theme = ThemeColors::default();
    // Editor text begins at column 31.
    assert_eq!(buffer[(31, 1)].bg, theme.selection_bg);
    assert_eq!(buffer[(32, 1)].bg, theme.selection_bg);
    assert_eq!(buffer[(33, 1)].bg, theme.bg);
}

#[test]
fn several_cursors_are_all_visible_as_the_frame_is_drawn() {
    let dir = fixture("multicursor-frame");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("main.rs")).unwrap();
    app.editor_mut().apply_action(typ_core::Action::AddCursor(typ_core::Direction::Forward));
    app.editor_mut().apply_action(typ_core::Action::InsertChar('#'));

    let terminal = draw(&mut app, 60, 8);
    let rows = rows(&terminal);
    assert!(rows[1].contains("#fn main"), "row 1: {}", rows[1]);
    assert!(rows[2].contains("#let x"), "row 2: {}", rows[2]);
}

#[test]
fn an_open_prompt_takes_over_the_left_of_the_status_bar() {
    let dir = fixture("prompt-frame");
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("main.rs")).unwrap();
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('f'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    for c in "main".chars() {
        app.handle_chord(KeyChord::from_event(KeyEvent::new(
            KeyCode::Char(c),
            KeyModifiers::NONE,
        )))
        .unwrap();
    }

    let terminal = draw(&mut app, 60, 8);
    let rows = rows(&terminal);
    assert!(rows[7].starts_with("Search: main"), "status: {}", rows[7]);
    assert!(rows[7].ends_with("main.rs  1:1"), "status: {}", rows[7]);
}

#[test]
fn a_long_line_scrolled_right_keeps_its_borders() {
    let dir = fixture("horizontal-frame");
    std::fs::write(dir.join("wide.rs"), "x".repeat(200) + "\n").unwrap();
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("wide.rs")).unwrap();
    app.editor_mut()
        .apply_action(typ_core::Action::Move { motion: typ_core::Motion::LineEnd, extend: false });

    let terminal = draw(&mut app, 60, 8);
    let rows = rows(&terminal);
    assert_eq!(rows[1].chars().count(), 60);
    assert!(rows[1].ends_with('│'), "row 1: {}", rows[1]);
}
```

The fixture helper needs `main.rs` to have a second line for the multi-cursor test — it
already writes `"fn main() {}\nlet x = 1;\n"`.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p typ-app --test frame`

Expected: FAIL on the four new tests; the ten existing ones still pass.

- [ ] **Step 3: Fix whatever they catch**

These tests assert behavior built in Tasks 6–13. If they fail, the bug is in that code, not
in the tests — check the failure against the equivalent unit test first, since a golden frame
failing while a unit test passes usually means a coordinate is being computed twice in two
places rather than once.

- [ ] **Step 4: Run the whole suite**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all -- --check`

Expected: all three clean.

- [ ] **Step 5: Commit**

```bash
git add crates/typ-app/tests/frame.rs
git commit -m "test(app): golden frames for selections, cursors and the prompt"
```

---

### Task 16: Milestone close-out

**Files:**
- Modify: `README.md`, `docs/design/architecture.md`, `docs/plans/m2-editing.md`

- [ ] **Step 1: Update the README key tables**

Add the new editor bindings — word motion, selection, multi-cursor, search and replace — and
update Status to say what the editor now does. Keep the `⚠️` line only if something in it is
still true.

- [ ] **Step 2: Record the architecture decisions**

In `docs/design/architecture.md` §5, note that the `Panel` trait grew `apply_action`, and why:
it is the single entry point through which the keymap, the command palette and the vim layer
all reach a panel, and the reason no `handle_key` arm may mutate a buffer.

Add a short subsection on the selection model: a caret is an empty selection, `Selections` is
always non-empty and non-overlapping, and edits run last-to-first so earlier positions stay
valid.

- [ ] **Step 3: Verify the milestone by hand**

```bash
cargo build --release
./target/release/typ .
```

Check all of:
- Shift+arrows select; the selection is visible and the cursor stays at the moving end.
- Ctrl+arrows move by word, stopping at punctuation.
- Drag selects; alt-click adds a cursor; clicking twice in one spot selects a word.
- Typing with several cursors edits every one, and a single `Ctrl+Z` undoes all of it.
- `Ctrl+F` searches, `F3` walks the matches and wraps, `Esc` abandons the prompt.
- `Ctrl+H` replaces every match, undoable in one step.
- A long line scrolls horizontally without breaking the right border.
- `Ctrl+Q` still guards unsaved work, and the terminal is left working on exit.

- [ ] **Step 4: Tick this plan's checkboxes and record deviations**

Every task above records what actually happened next to what was expected, the way
`m0-m1-foundation.md` does. Any deviation gets a sentence explaining why — that record is what
made the M1.1 and M1.2 patches diagnosable rather than archaeological.

- [ ] **Step 5: Commit and open the PR**

```bash
git add README.md docs
git commit -m "docs: close out M2 — editing, selections, multi-cursor, search"
gh pr create --base main --title "M2 — editing" --body-file docs/plans/m2-pr-body.md
```

---

## Self-review

Run twice. The second pass is the one that found things — the first checked coverage, the
second checked whether the code in the plan would actually compile and pass its own tests.

**Spec coverage.** §4's mouse/keyboard parity: every capability here is reachable both ways —
selection by shift+arrow and by drag, multi-cursor by `ctrl+alt+arrow` and by alt-click, word
selection by motion and by clicking twice. §4's "modal editing is a setting": Tasks 1, 2 and
12 are the seam that makes it possible, and nothing in Tasks 6–13 bypasses `Action`. §5's
`Panel` contract: the trait grows one defaulted method, and panels still return events rather
than touching state. §7's input model: scroll coalescing and synchronized output are untouched.

**Defects found on the second pass, all fixed in place:**

1. **`goal_col` was overwritten with the clamped column** after every vertical motion, so
   `vertical_motion_remembers_the_goal_column` in Task 7 would have failed — passing through
   one short line would have narrowed the goal permanently, which is the exact bug the field
   exists to prevent. The goal is now latched *before* moving.
2. **`handle_prompt_chord` would not have compiled.** It held `self.prompt.as_mut()` across
   `self.prompt = None`. Rewritten to decide an `Outcome` first and mutate afterwards.
3. **`self.width` was used by horizontal scrolling and never declared.** Task 11 now adds the
   field and sets it in `render` beside `height`.
4. **`typ-core` had no `anyhow` dependency** while Task 2's `merge_toml` returns
   `anyhow::Result`. Found on a third pass that checked the plan against the actual crate
   manifests rather than against itself.
5. **`become_replace_after_needle` was called but never defined**, and the two-stage replace
   closed its prompt after the first answer (found on the first pass, fixed then).
6. `gh pr create --body-file <(...)` was a placeholder — now names a real file.
7. `set_selections_for_test` indexed `list[0]` without checking; it now asserts.

**Type consistency.** `Selection`/`Selections` match across Tasks 3 and 6–13. `perform` is the
inherent method, `apply_action` the trait method that calls it — consistent from Task 7 on.
`begin_edit_group`/`end_edit_group` are introduced in Task 8 and reused by `replace_all` in
Task 13. `line_text` exists on both `TextBuffer` and `EditorPanel` (the panel delegates), which
is deliberate and used in tests of both. `page()`, `last_line()`, `line_grapheme_count()` and
`scroll_to_cursor()` all predate this plan and are made `pub(crate)` in Task 7.

**Known gaps, deliberate.** Tree-sitter highlighting and the command palette are M2.5. Undo is
per-action with no time grouping — typing ten characters is ten undo steps, which is worse
than most editors and needs a timer on the edit path; M2.5 alongside highlighting. Search is
literal, behind a `SearchQuery` type shaped to admit regex later. There is no replace-one,
only replace-all.

**One behavior worth watching during execution.** Task 12 treats an empty event vector as "the
panel declined this action", and `Action::AddCursor` at the edge of the document deliberately
returns empty. The app then tries it as an app action and finds no arm, so it is a harmless
no-op — but if a future action both belongs to a panel and needs a fallback, that convention
will need a real "handled" signal rather than an empty vector.

**One execution risk.** Task 6 changes `EditorPanel`'s cursor field to `Selections`, and Tasks
7–8 add the action path while the old `handle_key` arms still exist. Task 12 deletes them.
Stopping between 6 and 12 leaves the tree green but with two input paths; the cheap recovery is
to finish Task 12, not to revert.
