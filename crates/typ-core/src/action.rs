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

/// Where a motion lands.
///
/// Motions carry no "extend" flag themselves — that is an argument of
/// `Action::Move`, so every motion is automatically available in both forms
/// rather than being listed twice and drifting apart.
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
    Copy,
    Cut,
    Paste,
    Indent,
    Outdent,
}

impl Action {
    /// Every action a config file may name, in a stable order.
    pub const ALL: &'static [Action] = &[
        Action::Move {
            motion: Motion::Left,
            extend: false,
        },
        Action::Move {
            motion: Motion::Left,
            extend: true,
        },
        Action::Move {
            motion: Motion::Right,
            extend: false,
        },
        Action::Move {
            motion: Motion::Right,
            extend: true,
        },
        Action::Move {
            motion: Motion::Up,
            extend: false,
        },
        Action::Move {
            motion: Motion::Up,
            extend: true,
        },
        Action::Move {
            motion: Motion::Down,
            extend: false,
        },
        Action::Move {
            motion: Motion::Down,
            extend: true,
        },
        Action::Move {
            motion: Motion::WordLeft,
            extend: false,
        },
        Action::Move {
            motion: Motion::WordLeft,
            extend: true,
        },
        Action::Move {
            motion: Motion::WordRight,
            extend: false,
        },
        Action::Move {
            motion: Motion::WordRight,
            extend: true,
        },
        Action::Move {
            motion: Motion::LineStart,
            extend: false,
        },
        Action::Move {
            motion: Motion::LineStart,
            extend: true,
        },
        Action::Move {
            motion: Motion::LineEnd,
            extend: false,
        },
        Action::Move {
            motion: Motion::LineEnd,
            extend: true,
        },
        Action::Move {
            motion: Motion::PageUp,
            extend: false,
        },
        Action::Move {
            motion: Motion::PageUp,
            extend: true,
        },
        Action::Move {
            motion: Motion::PageDown,
            extend: false,
        },
        Action::Move {
            motion: Motion::PageDown,
            extend: true,
        },
        Action::Move {
            motion: Motion::DocumentStart,
            extend: false,
        },
        Action::Move {
            motion: Motion::DocumentStart,
            extend: true,
        },
        Action::Move {
            motion: Motion::DocumentEnd,
            extend: false,
        },
        Action::Move {
            motion: Motion::DocumentEnd,
            extend: true,
        },
        Action::Delete {
            direction: Direction::Backward,
            by_word: false,
        },
        Action::Delete {
            direction: Direction::Backward,
            by_word: true,
        },
        Action::Delete {
            direction: Direction::Forward,
            by_word: false,
        },
        Action::Delete {
            direction: Direction::Forward,
            by_word: true,
        },
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
        Action::Copy,
        Action::Cut,
        Action::Paste,
        Action::Indent,
        Action::Outdent,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            // The name is a compile-time pairing rather than a runtime
            // `format!`, so it returns `&'static str` and compares without
            // allocating on every keymap lookup.
            Action::Move { motion, extend } => match (motion, extend) {
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
            },
            Action::Delete {
                direction: Direction::Backward,
                by_word: false,
            } => "delete_backward",
            Action::Delete {
                direction: Direction::Backward,
                by_word: true,
            } => "delete_word_backward",
            Action::Delete {
                direction: Direction::Forward,
                by_word: false,
            } => "delete_forward",
            Action::Delete {
                direction: Direction::Forward,
                by_word: true,
            } => "delete_word_forward",
            Action::InsertNewline => "insert_newline",
            // Never returned by `from_name`; see the type docs.
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
            Action::Copy => "copy",
            Action::Cut => "cut",
            Action::Paste => "paste",
            Action::Indent => "indent",
            Action::Outdent => "outdent",
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
