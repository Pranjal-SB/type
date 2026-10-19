//! One watcher for the whole workspace, off the render thread.
//!
//! Waits on the channel, never on the clock, for the reason
//! `external_change.rs` gives.

use std::path::{Path, PathBuf};
use std::time::Duration;

use ratatui::layout::Rect;
use typ_app::App;
use typ_app::run::{AppReceiver, channel, step};
use typ_core::AppEvent;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

const WAIT: Duration = Duration::from_secs(10);

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-workspace-watch").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("target/debug")).unwrap();
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join(".gitignore"), "target/\n").unwrap();
    std::fs::write(dir.join("src/a.rs"), "alpha\n").unwrap();
    std::fs::write(dir.join("src/b.rs"), "beta\n").unwrap();
    dir
}

fn watching(name: &str) -> (App, PathBuf, AppReceiver) {
    let dir = fixture(name);
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);
    assert!(
        app.await_workspace_watch(WAIT),
        "the workspace watch never started"
    );
    app.open_path(&dir.join("src/a.rs")).unwrap();
    app.open_path(&dir.join("src/b.rs")).unwrap();
    (app, dir, rx)
}

/// Every change reported until one names `file`, which is returned last.
fn changes_until(rx: &AppReceiver, file: &Path) -> Vec<PathBuf> {
    let mut seen = Vec::new();
    loop {
        match rx.recv_timeout(WAIT) {
            Ok(AppEvent::FileChanged(path)) => {
                let done = path == file;
                seen.push(path);
                if done {
                    return seen;
                }
            }
            Ok(_) => continue,
            Err(e) => panic!("never heard about {}: {e}; saw {seen:?}", file.display()),
        }
    }
}

#[test]
fn a_change_to_a_file_open_in_a_background_tab_reaches_that_tab() {
    let (mut app, dir, rx) = watching("background");
    let a = dir.join("src/a.rs");
    assert_eq!(app.active_tab(), 1, "b.rs is in front, a.rs behind it");

    std::fs::write(&a, "changed\n").unwrap();
    for path in changes_until(&rx, &a) {
        step(&mut app, AppEvent::FileChanged(path), AREA).unwrap();
    }

    assert_eq!(app.tab(0).line_text(0), "changed");
}

#[test]
fn a_tab_switch_does_no_watcher_work() {
    let (mut app, _dir, _rx) = watching("switch");
    // One watch, the workspace's: neither open file needed its own.
    assert_eq!(app.watches_started(), 1);

    for _ in 0..5 {
        app.next_tab();
    }

    assert_eq!(app.watches_started(), 1, "a switch started a watch");
}

#[test]
fn a_change_inside_target_or_git_is_not_reported() {
    let (_app, dir, rx) = watching("ignored");
    std::fs::write(dir.join("target/debug/build.log"), "noise\n").unwrap();
    std::fs::write(dir.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    // A sentinel after the noise: the watcher reports in order, so anything
    // from the two writes above would arrive before it.
    let a = dir.join("src/a.rs");
    std::fs::write(&a, "sentinel\n").unwrap();

    let seen = changes_until(&rx, &a);

    assert!(
        seen.iter().all(
            |path| !path.starts_with(dir.join("target")) && !path.starts_with(dir.join(".git"))
        ),
        "ignored paths were reported: {seen:?}"
    );
}

#[test]
fn a_file_outside_the_workspace_still_gets_a_watch_of_its_own() {
    let (mut app, _dir, rx) = watching("outside");
    let elsewhere = fixture("outside-file").join("src/a.rs");
    app.open_path(&elsewhere).unwrap();
    assert_eq!(
        app.watches_started(),
        2,
        "nothing watches a file outside the root"
    );

    std::fs::write(&elsewhere, "moved\n").unwrap();
    for path in changes_until(&rx, &elsewhere) {
        step(&mut app, AppEvent::FileChanged(path), AREA).unwrap();
    }
    assert_eq!(app.editor().line_text(0), "moved");
}
