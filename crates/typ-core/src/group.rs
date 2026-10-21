//! The columns of the `ctrl+k` menu, which the context menu, the menu bar
//! and the keybindings tab group actions by.
//!
//! Split from `action.rs` at M4.7: which column an action sits in is
//! `Action::group`, beside the vocabulary; the columns themselves are the
//! menus' business.

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
    Panes,
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
        Group::Panes,
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
            Group::Panes => "panes",
            Group::Code => "code",
            Group::App => "app",
        }
    }
}
