use std::path::PathBuf;

/// Identifies a live panel instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PanelId(pub u32);

/// Identifies a registered handler in `typ-registry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HandlerId(pub &'static str);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyLevel {
    Info,
    Warn,
    Error,
}

/// The complete vocabulary a panel may emit.
///
/// This set is deliberately closed. Editors that let every viewer add its own
/// variant end up with an enum that each new panel type must edit, turning it
/// into a chokepoint. New panels register a handler in `typ-registry` and route
/// through `OpenWith` instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelEvent {
    /// Panel state changed; the app should repaint.
    NeedsRedraw,
    /// Quit the application.
    Quit,
    /// Close the emitting panel.
    CloseSelf,
    /// Move focus to another panel.
    Focus(PanelId),
    /// Open a path in whichever panel the registry says owns it.
    OpenFile {
        path: PathBuf,
        line: usize,
        col: usize,
    },
    /// Open a path with an explicitly chosen handler.
    OpenWith { handler: HandlerId, path: PathBuf },
    /// Run a shell command, optionally in a given directory.
    RunCommand {
        command: String,
        cwd: Option<PathBuf>,
    },
    /// Surface a message to the user.
    Notify { level: NotifyLevel, message: String },
}
