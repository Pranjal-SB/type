//! A log file, for when the screen cannot be printed to.
//!
//! A TUI owns the screen, so `println!` debugging is unavailable *by
//! construction* — the one kind of program where logging is not optional is the
//! one kind that had none. Gap analysis defect 32.
//!
//! # Deliberately not `tracing`
//!
//! A file and a mutex. No subscriber, no spans, no async, no dependency. What
//! is actually needed today is "what happened before it broke", and the whole
//! of that is a timestamp, a level, a module and a line. `tracing` earns its
//! weight when there are spans worth correlating across threads, which arrives
//! with the worker threads at M2.5 — and swapping this for it then is a change
//! of one file, because every call site goes through the macros.
//!
//! # Off unless asked for
//!
//! Set `TYP_LOG` to a path. Unset, `is_enabled()` is a relaxed atomic read and
//! the macros never format their arguments, so a disabled log costs a branch.
//!
//! # Failures are silent, on purpose
//!
//! An unwritable log path disables logging and does not report it. A logger
//! that takes the editor down with it, or that spends the status bar on its own
//! problems, is worse than no logger.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

/// Read without locking, so the macros can skip formatting when logging is off.
static ENABLED: AtomicBool = AtomicBool::new(false);
static SINK: Mutex<Option<File>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Padded so the module column lines up down the file, which is what
        // makes a log skimmable rather than merely searchable.
        let name = match self {
            Level::Info => "INFO ",
            Level::Warn => "WARN ",
            Level::Error => "ERROR",
        };
        f.write_str(name)
    }
}

fn sink() -> MutexGuard<'static, Option<File>> {
    SINK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Log to `path`, appending. `None` turns logging off.
///
/// Appending rather than truncating: the run that needs explaining is often the
/// one before the one you are watching.
pub fn init(path: Option<&Path>) {
    let mut guard = sink();
    match path {
        Some(path) => {
            let opened = OpenOptions::new().create(true).append(true).open(path).ok();
            ENABLED.store(opened.is_some(), Ordering::Relaxed);
            *guard = opened;
        }
        None => {
            ENABLED.store(false, Ordering::Relaxed);
            *guard = None;
        }
    }
}

/// Log to whatever `TYP_LOG` names, or nowhere.
pub fn init_from_env() {
    match std::env::var_os("TYP_LOG") {
        Some(path) if !path.is_empty() => init(Some(Path::new(&path))),
        _ => init(None),
    }
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Write one line. Called by the macros; prefer those.
pub fn write(level: Level, module: &str, message: &str) {
    let mut guard = sink();
    let Some(file) = guard.as_mut() else {
        return;
    };
    // A failed write disables nothing and reports nothing: the disk filling up
    // mid-session must not become an editor problem.
    let _ = writeln!(file, "{} {level} {module}: {message}", timestamp());
    let _ = file.flush();
}

/// Wall-clock time of day, `HH:MM:SS.mmm`, UTC.
///
/// Hand-rolled rather than pulling in `chrono` or `time`: the date is the log
/// file's own mtime and a session does not span days, so time of day is the
/// whole of what a reader needs to line an entry up against "when it broke".
fn timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let (h, m, s) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    format!("{h:02}:{m:02}:{s:02}.{:03}", now.subsec_millis())
}

/// Write a log line, formatting the arguments only if logging is on.
#[macro_export]
macro_rules! log_at {
    ($level:expr, $($arg:tt)*) => {
        if $crate::log::is_enabled() {
            $crate::log::write($level, module_path!(), &format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => { $crate::log_at!($crate::log::Level::Info, $($arg)*) };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => { $crate::log_at!($crate::log::Level::Warn, $($arg)*) };
}

#[macro_export]
macro_rules! log_error {
    ($($arg:tt)*) => { $crate::log_at!($crate::log::Level::Error, $($arg)*) };
}
