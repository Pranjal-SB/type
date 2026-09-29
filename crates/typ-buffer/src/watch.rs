//! Notice when a file changes on disk.
//!
//! A rebase, a formatter, or another editor writes the file while it is open.
//! Without this the editor neither reloads nor warns, and the next save
//! silently overwrites whatever the other writer did.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// What one notification from the OS means for the watched file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    /// The file may differ from what was last read.
    Changed,
    /// The watch reported a failure, and may have stopped.
    Failed(String),
}

fn interpret(event: notify::Result<notify::Event>, name: &std::ffi::OsStr) -> Option<WatchEvent> {
    // An error arrives on the same channel as the events: the watch limit
    // exhausted, the watched directory removed. Dropping it left the editor
    // believing it was still watching, and the next save overwrote whatever
    // changed unseen. `typ-buffer` cannot log, so it is handed up as data.
    // Gap 94.
    let event = match event {
        Ok(event) => event,
        Err(error) => return Some(WatchEvent::Failed(error.to_string())),
    };
    // The OS lost events and says so: the file may have changed with nothing
    // naming it. Checking costs one comparison against disk.
    if event.need_rescan() {
        return Some(WatchEvent::Changed);
    }
    // Access events fire on every read, including our own. Only creation,
    // modification and removal change what is on disk.
    if !(event.kind.is_create() || event.kind.is_modify() || event.kind.is_remove()) {
        return None;
    }
    event
        .paths
        .iter()
        .any(|p| p.file_name() == Some(name))
        .then_some(WatchEvent::Changed)
}

/// A live watch. Dropping it stops the watching, which is how opening another
/// file replaces the old watch rather than accumulating them.
pub struct FileWatch {
    _watcher: RecommendedWatcher,
}

/// Report changes to `path` by calling `on_event` from the watcher's thread.
///
/// **Watches the parent directory, not the file.** Editors and formatters write
/// by rename-over, which destroys the inode a file watch is pinned to and
/// leaves that watch pointed at nothing — the file keeps changing and the
/// watcher keeps saying nothing. Watching the directory and filtering by name
/// survives it, and also sees the file being deleted and recreated.
///
/// `on_event` is handed the path as it was given here, not the path the OS
/// reported, so a caller can compare it against what it has open without
/// worrying about how each platform spells it.
pub fn watch_file(
    path: &Path,
    on_event: impl Fn(PathBuf, WatchEvent) + Send + 'static,
) -> Result<FileWatch> {
    let path = path.to_path_buf();
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let name = path
        .file_name()
        .context("watching a path with no file name")?
        .to_os_string();

    let reported = path.clone();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if let Some(what) = interpret(event, &name) {
            on_event(reported.clone(), what);
        }
    })
    .context("creating a file watcher")?;

    // ponytail: no debouncing. One save produces several events on every
    // platform, and the handler on the other end is idempotent — it compares
    // the file against the buffer and does nothing when they agree. A
    // debouncer earns its place when an event costs more than that comparison.
    watcher
        .watch(&dir, RecursiveMode::NonRecursive)
        .with_context(|| format!("watching {}", dir.display()))?;

    Ok(FileWatch { _watcher: watcher })
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{EventKind, Flag};

    #[test]
    fn a_watch_error_is_reported_rather_than_dropped() {
        let error = notify::Error::generic("inotify watch limit reached");
        assert!(
            matches!(
                interpret(Err(error), "f.rs".as_ref()),
                Some(WatchEvent::Failed(reason)) if reason.contains("watch limit")
            ),
            "the next save would silently overwrite whatever changed unseen"
        );
    }

    #[test]
    fn a_rescan_notice_counts_as_a_change() {
        // The OS dropped events and says so. The file may have changed with no
        // event naming it, so it has to be checked.
        let event = notify::Event::new(EventKind::Other).set_flag(Flag::Rescan);
        assert_eq!(
            interpret(Ok(event), "f.rs".as_ref()),
            Some(WatchEvent::Changed)
        );
    }

    #[test]
    fn an_event_for_another_file_is_ignored() {
        let event = notify::Event::new(EventKind::Modify(notify::event::ModifyKind::Any))
            .add_path(PathBuf::from("other.rs"));
        assert_eq!(interpret(Ok(event), "f.rs".as_ref()), None);
    }
}
