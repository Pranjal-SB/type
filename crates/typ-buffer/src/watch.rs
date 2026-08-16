//! Notice when a file changes on disk.
//!
//! A rebase, a formatter, or another editor writes the file while it is open.
//! Without this the editor neither reloads nor warns, and the next save
//! silently overwrites whatever the other writer did.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// A live watch. Dropping it stops the watching, which is how opening another
/// file replaces the old watch rather than accumulating them.
pub struct FileWatch {
    _watcher: RecommendedWatcher,
}

/// Report changes to `path` by calling `on_change` from the watcher's thread.
///
/// **Watches the parent directory, not the file.** Editors and formatters write
/// by rename-over, which destroys the inode a file watch is pinned to and
/// leaves that watch pointed at nothing — the file keeps changing and the
/// watcher keeps saying nothing. Watching the directory and filtering by name
/// survives it, and also sees the file being deleted and recreated.
///
/// `on_change` is handed the path as it was given here, not the path the OS
/// reported, so a caller can compare it against what it has open without
/// worrying about how each platform spells it.
pub fn watch_file(path: &Path, on_change: impl Fn(PathBuf) + Send + 'static) -> Result<FileWatch> {
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
        let Ok(event) = event else { return };
        // Access events fire on every read, including our own. Only creation,
        // modification and removal change what is on disk.
        if !(event.kind.is_create() || event.kind.is_modify() || event.kind.is_remove()) {
            return;
        }
        if event.paths.iter().any(|p| p.file_name() == Some(&name)) {
            on_change(reported.clone());
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
