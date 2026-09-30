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
    /// `FocusNext` the other way round.
    FocusPrevious,
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
    /// Put the `ctrl+k` menu up without pressing `ctrl+k`.
    ///
    /// For a terminal or a multiplexer that eats the chord: the palette
    /// reaches this by name, so the menu is never only one key away.
    OpenMenu,
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

/// The `go_to_tab_N` descriptions, indexed like `GO_TO_TAB_NAMES`.
const GO_TO_TAB_DESCRIPTIONS: [&str; 9] = [
    "switch to tab 1",
    "switch to tab 2",
    "switch to tab 3",
    "switch to tab 4",
    "switch to tab 5",
    "switch to tab 6",
    "switch to tab 7",
    "switch to tab 8",
    "switch to tab 9",
];

/// A column of the `ctrl+k` menu. Closed: a new group is a design decision,
/// not something an action invents for itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    Edit,
    Selection,
    Find,
    Files,
    Tabs,
    Panels,
    Code,
    App,
}

impl Group {
    /// In the order the menu draws them.
    pub const ALL: &'static [Group] = &[
        Group::Edit,
        Group::Selection,
        Group::Find,
        Group::Files,
        Group::Tabs,
        Group::Panels,
        Group::Code,
        Group::App,
    ];

    /// The column's heading.
    pub fn label(self) -> &'static str {
        match self {
            Group::Edit => "edit",
            Group::Selection => "selection",
            Group::Find => "find",
            Group::Files => "files",
            Group::Tabs => "tabs",
            Group::Panels => "panels",
            Group::Code => "code",
            Group::App => "app",
        }
    }
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
        Action::SelectNextOccurrence,
        Action::SelectAllOccurrences,
        Action::CollapseSelections,
        Action::AddCursor(Direction::Backward),
        Action::AddCursor(Direction::Forward),
        Action::Save,
        Action::Quit,
        Action::FocusNext,
        Action::FocusPrevious,
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
        Action::OpenMenu,
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
            Action::FocusPrevious => "focus_previous",
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
            Action::OpenMenu => "open_menu",
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

    /// What the action does, as a menu row says it.
    ///
    /// The `ctrl+k` menu and the command palette both show this, so it is one
    /// string for two surfaces. `name` stays what a config file writes.
    /// Exhaustive, so a new action cannot compile without saying what it does.
    pub fn description(&self) -> &'static str {
        match self {
            Action::Move { motion, extend } => match (motion, extend) {
                (Motion::Left, false) => "move the cursor left",
                (Motion::Left, true) => "extend the selection left",
                (Motion::Right, false) => "move the cursor right",
                (Motion::Right, true) => "extend the selection right",
                (Motion::Up, false) => "move the cursor up a line",
                (Motion::Up, true) => "extend the selection up a line",
                (Motion::Down, false) => "move the cursor down a line",
                (Motion::Down, true) => "extend the selection down a line",
                (Motion::WordLeft, false) => "move the cursor to the previous word",
                (Motion::WordLeft, true) => "extend the selection to the previous word",
                (Motion::WordRight, false) => "move the cursor to the next word",
                (Motion::WordRight, true) => "extend the selection to the next word",
                (Motion::LineStart, false) => "move the cursor to the start of the line",
                (Motion::LineStart, true) => "extend the selection to the start of the line",
                (Motion::LineEnd, false) => "move the cursor to the end of the line",
                (Motion::LineEnd, true) => "extend the selection to the end of the line",
                (Motion::PageUp, false) => "move the cursor up a page",
                (Motion::PageUp, true) => "extend the selection up a page",
                (Motion::PageDown, false) => "move the cursor down a page",
                (Motion::PageDown, true) => "extend the selection down a page",
                (Motion::DocumentStart, false) => "move the cursor to the start of the file",
                (Motion::DocumentStart, true) => "extend the selection to the start of the file",
                (Motion::DocumentEnd, false) => "move the cursor to the end of the file",
                (Motion::DocumentEnd, true) => "extend the selection to the end of the file",
            },
            Action::Delete {
                direction: Direction::Backward,
                by_word: false,
            } => "delete the character before the cursor",
            Action::Delete {
                direction: Direction::Backward,
                by_word: true,
            } => "delete the word before the cursor",
            Action::Delete {
                direction: Direction::Forward,
                by_word: false,
            } => "delete the character after the cursor",
            Action::Delete {
                direction: Direction::Forward,
                by_word: true,
            } => "delete the word after the cursor",
            Action::InsertNewline => "insert a line break",
            Action::InsertChar(_) => "type a character",
            Action::Undo => "undo the last edit",
            Action::Redo => "redo the last undone edit",
            Action::SelectAll => "select the whole file",
            Action::SelectLine => "select the line",
            Action::SelectNextOccurrence => "add the next match of the selection",
            Action::SelectAllOccurrences => "select every match of the selection",
            Action::CollapseSelections => "collapse each selection to its cursor",
            Action::AddCursor(Direction::Backward) => "add a cursor on the line above",
            Action::AddCursor(Direction::Forward) => "add a cursor on the line below",
            Action::Save => "save the file",
            Action::Quit => "quit the editor",
            Action::FocusNext => "move focus to the next panel",
            Action::FocusPrevious => "move focus to the previous panel",
            Action::GotoLine => "go to a line number",
            Action::SearchOpen => "search the file",
            Action::SearchNext => "jump to the next match",
            Action::SearchPrevious => "jump to the previous match",
            Action::ReplaceOpen => "replace in the file",
            Action::OpenFilePicker => "open a file by name",
            Action::OpenProjectSearch => "search the project",
            Action::OpenCommandPalette => "run a command by name",
            Action::NextTab => "switch to the next tab",
            Action::PrevTab => "switch to the previous tab",
            Action::CloseTab => "close the tab",
            Action::GoToTab(n) => GO_TO_TAB_DESCRIPTIONS
                .get((*n as usize).saturating_sub(1))
                .copied()
                .unwrap_or("switch to no tab"),
            Action::Copy => "copy the selection",
            Action::Cut => "cut the selection",
            Action::Paste => "paste the clipboard",
            Action::Indent => "indent the selected lines",
            Action::Outdent => "outdent the selected lines",
            Action::GotoDefinition => "go to the definition",
            Action::Hover => "describe what is under the cursor",
            Action::RestartLanguageServers => "restart the language servers",
            Action::OpenMenu => "show the ctrl+k menu",
        }
    }

    /// Which column of the `ctrl+k` menu the action sits in.
    pub fn group(&self) -> Group {
        match self {
            Action::Move { extend: false, .. }
            | Action::Delete { .. }
            | Action::InsertNewline
            | Action::InsertChar(_)
            | Action::Undo
            | Action::Redo
            | Action::Copy
            | Action::Cut
            | Action::Paste
            | Action::Indent
            | Action::Outdent => Group::Edit,
            Action::Move { extend: true, .. }
            | Action::SelectAll
            | Action::SelectLine
            | Action::SelectNextOccurrence
            | Action::SelectAllOccurrences
            | Action::CollapseSelections
            | Action::AddCursor(_) => Group::Selection,
            Action::GotoLine
            | Action::SearchOpen
            | Action::SearchNext
            | Action::SearchPrevious
            | Action::ReplaceOpen
            | Action::OpenProjectSearch => Group::Find,
            Action::Save | Action::OpenFilePicker => Group::Files,
            Action::NextTab | Action::PrevTab | Action::CloseTab | Action::GoToTab(_) => {
                Group::Tabs
            }
            Action::FocusNext | Action::FocusPrevious => Group::Panels,
            Action::GotoDefinition | Action::Hover | Action::RestartLanguageServers => Group::Code,
            Action::Quit | Action::OpenCommandPalette | Action::OpenMenu => Group::App,
        }
    }
}
