use std::path::Path;
use std::sync::Mutex;

use ignore::{WalkBuilder, WalkState};

/// Every file under `root` that a picker should offer, relative to it.
///
/// **Parallel, and that is not an optimisation.** Measured on this machine
/// against a 37,586-file tree: `build()` took 2596 ms and `build_parallel()`
/// took 94.7 ms. The serial walk is not a slower version of the right answer,
/// it is a picker that reads as hung — most of that cost is `stat`, which is
/// latency rather than work, and latency is what parallelism is for.
///
/// A missing or unreadable root is an empty list rather than an error. This
/// crate sits below `typ-app` and cannot log (see AGENTS.md), so a `Result`
/// here would travel two crates to reach somewhere it could be reported, and
/// "no candidates" is the true answer for a directory that is not there.
/// Individual unreadable entries are skipped for the same reason: one
/// permission-denied subdirectory must not empty the picker.
pub fn walk(root: &Path) -> Vec<String> {
    let found = Mutex::new(Vec::new());

    WalkBuilder::new(root)
        // `.gitignore`, `.ignore`, nested ignore files, the global one and
        // negations — all on by default, and all the reason this is a
        // dependency rather than a `read_dir` loop.
        //
        // `require_git(false)` because the default is to apply `.gitignore`
        // only inside a git repository, and an editor is opened on directories
        // that are not one: a vendored source tree, an unpacked archive, a
        // project whose `.git` lives elsewhere through a worktree. A
        // `.gitignore` sitting in a directory means what it says whether or not
        // git is watching, and ripgrep grew `--no-require-git` for the same
        // complaint.
        .require_git(false)
        .build_parallel()
        .run(|| {
            let found = &found;
            Box::new(move |entry| {
                // One lock per file, deliberately. Batching into a per-worker
                // vector needs a flush when the worker ends, which the callback
                // signature gives no hook for — the first draft of this function
                // batched and silently dropped every tail under 4096. The lock
                // is nanoseconds against a `stat` that is microseconds; if that
                // ever stops being true it is a measurement away from being
                // found, and `tests/perf.rs` takes it.
                if let Ok(entry) = entry
                    && entry.file_type().is_some_and(|t| t.is_file())
                    && let Some(relative) = relative_to(root, entry.path())
                {
                    found.lock().expect("walk mutex").push(relative);
                }
                WalkState::Continue
            })
        });

    let mut all = found.into_inner().expect("walk mutex");
    // Sorted so the corpus has one order. An empty query lists candidates in
    // this order, and a parallel walk's natural order is whichever worker got
    // there first — which changes between runs and would make the picker's
    // opening screen shuffle for no reason.
    all.sort_unstable();
    all
}

/// `root`-relative, with `/` separators on every platform.
///
/// The candidate string is what the matcher scores, and `crates\typ-core`
/// scores differently from `crates/typ-core` under path-aware scoring. A picker
/// that ranks differently on Windows is a picker with two behaviours, and the
/// one nobody tests is the one that is wrong.
fn relative_to(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?;
    let text = relative.to_str()?;
    if text.is_empty() {
        return None;
    }
    Some(if std::path::MAIN_SEPARATOR == '/' {
        text.to_string()
    } else {
        text.replace(std::path::MAIN_SEPARATOR, "/")
    })
}
