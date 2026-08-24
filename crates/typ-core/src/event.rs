use std::path::PathBuf;

/// Everything the event loop can be woken by.
///
/// The loop blocks on a channel of these rather than on `event::read()`, so a
/// worker thread can deliver a result without waiting for the user to press a
/// key. A terminal event is one variant among several, not the only input.
///
/// M2.7 added `Parsed` and M3 adds an LSP response. That is the point of the
/// type: a new off-thread producer is a variant here and a match arm in the
/// loop, not a change to how the loop waits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    /// Something arrived from the terminal: a key, a mouse report, a paste.
    Input(crossterm::event::Event),
    /// The file at this path changed on disk.
    FileChanged(PathBuf),
    /// A worker finished parsing a snapshot of a buffer.
    ///
    /// The generation inside is what makes an out-of-order result harmless:
    /// the panel keeps the highest it has seen and discards anything older.
    /// Two parses can never be in flight at once by construction, but "can
    /// never" and "cannot, and here is the counter that proves it" are
    /// different claims.
    Parsed(typ_syntax::Parsed),
}

/// So `ParseWorker::spawn` can take the app's own sender.
///
/// The worker sends `typ_syntax::Parsed` rather than an `AppEvent` because
/// `typ-syntax` sits below this crate and must not depend back on it — not
/// even in dev-dependencies, where the cycle would surface as a publish-order
/// failure. This impl is the whole cost of keeping that edge absent.
impl From<typ_syntax::Parsed> for AppEvent {
    fn from(parsed: typ_syntax::Parsed) -> Self {
        AppEvent::Parsed(parsed)
    }
}

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
