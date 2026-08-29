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
    Move {
        motion: Motion,
        extend: bool,
    },
    Delete {
        direction: Direction,
        by_word: bool,
    },
    InsertNewline,
    InsertChar(char),
    Undo,
    Redo,
    SelectAll,
    SelectLine,
    /// Select the word under the cursor, then each next occurrence of it.
    SelectNextOccurrence,
    /// Select every occurrence at once.
    SelectAllOccurrences,
    CollapseSelections,
    AddCursor(Direction),
    Save,
    Quit,
    FocusNext,
    GotoLine,
    SearchOpen,
    SearchNext,
    SearchPrevious,
    ReplaceOpen,
    /// Open the fuzzy file picker over the body.
    OpenFilePicker,
    /// Open the project-search picker over the body.
    OpenProjectSearch,
    /// Open the picker over every named action.
    OpenCommandPalette,
    /// The next open file, wrapping at the end.
    NextTab,
    /// The previous open file, wrapping at the start.
    PrevTab,
    /// Close the active tab, after asking if it holds unsaved work.
    CloseTab,
    /// Jump to the nth open file, counting from one — every tabbed application
    /// counts these from one, including the terminals this runs inside.
    GoToTab(u8),
    Copy,
    Cut,
    Paste,
    Indent,
    Outdent,
    /// Jump to where the thing under the cursor is defined.
    ///
    /// App-owned, like the tab actions: it can open a file, and a panel that
    /// could open a file would need to know it sits in a list of them.
    GotoDefinition,
    /// Show what the server knows about the thing under the cursor.
    Hover,
    /// Start every stopped language server again.
    ///
    /// The other half of a crash-loop guard: something has to be able to say
    /// "I fixed it". Helix spells it `:lsp-restart` and Zed has a command for
    /// it; TYPE reaches it through the palette, which every named action is in
    /// for free.
    RestartLanguageServers,
}

/// The `go_to_tab_N` names, indexed by `n - 1`.
///
/// Nine of them, because `Alt+digit` is the universal binding for this and
/// there are nine non-zero digits. A tenth would need a chord no terminal is
/// guaranteed to deliver — see `docs/design/controls.md` §1.
const GO_TO_TAB_NAMES: [&str; 9] = [
    "go_to_tab_1",
    "go_to_tab_2",
    "go_to_tab_3",
    "go_to_tab_4",
    "go_to_tab_5",
    "go_to_tab_6",
    "go_to_tab_7",
    "go_to_tab_8",
    "go_to_tab_9",
];

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
        Action::SelectNextOccurrence,
        Action::SelectAllOccurrences,
        Action::CollapseSelections,
        Action::AddCursor(Direction::Backward),
        Action::AddCursor(Direction::Forward),
        Action::Save,
        Action::Quit,
        Action::FocusNext,
        Action::GotoLine,
        Action::SearchOpen,
        Action::SearchNext,
        Action::SearchPrevious,
        Action::ReplaceOpen,
        Action::OpenFilePicker,
        Action::OpenProjectSearch,
        Action::OpenCommandPalette,
        Action::NextTab,
        Action::PrevTab,
        Action::CloseTab,
        Action::GoToTab(1),
        Action::GoToTab(2),
        Action::GoToTab(3),
        Action::GoToTab(4),
        Action::GoToTab(5),
        Action::GoToTab(6),
        Action::GoToTab(7),
        Action::GoToTab(8),
        Action::GoToTab(9),
        Action::Copy,
        Action::Cut,
        Action::Paste,
        Action::Indent,
        Action::Outdent,
        Action::GotoDefinition,
        Action::Hover,
        Action::RestartLanguageServers,
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
            Action::SelectNextOccurrence => "select_next_occurrence",
            Action::SelectAllOccurrences => "select_all_occurrences",
            Action::CollapseSelections => "collapse_selections",
            Action::AddCursor(Direction::Backward) => "add_cursor_above",
            Action::AddCursor(Direction::Forward) => "add_cursor_below",
            Action::Save => "save",
            Action::Quit => "quit",
            Action::FocusNext => "focus_next",
            Action::GotoLine => "goto_line",
            Action::SearchOpen => "search_open",
            Action::SearchNext => "search_next",
            Action::SearchPrevious => "search_previous",
            Action::ReplaceOpen => "replace_open",
            Action::OpenFilePicker => "open_file_picker",
            Action::OpenProjectSearch => "open_project_search",
            Action::OpenCommandPalette => "open_command_palette",
            Action::NextTab => "next_tab",
            Action::PrevTab => "prev_tab",
            Action::CloseTab => "close_tab",
            Action::GotoDefinition => "goto_definition",
            Action::Hover => "hover",
            Action::RestartLanguageServers => "restart_language_servers",
            Action::GoToTab(n) => GO_TO_TAB_NAMES
                .get((*n as usize).saturating_sub(1))
                .copied()
                // Unreachable through `ALL` or the keymap, which build 1..=9
                // and nothing else. A name that round-trips to nothing is
                // better than one that round-trips to the wrong tab.
                .unwrap_or("go_to_tab_none"),
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
