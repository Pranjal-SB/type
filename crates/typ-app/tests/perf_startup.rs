//! Cold start — architecture §4's first budget, and the one M2.7 could have
//! broken without noticing.
//!
//! **Its own test binary, and it has to be.** This measures the first run of
//! work that is cached process-wide afterwards; anything that ran before it in
//! the same binary would leave it measuring a warm cache. The same mistake was
//! made once already this milestone in `typ-panel-editor`, where a
//! grammar-load benchmark sharing a binary reported 60 µs for something that
//! takes 102 ms.
//!
//!     cargo test --release -p typ-app --test perf_startup -- --ignored --nocapture
//!
//! What this covers is `main`'s real sequence up to the first frame: build the
//! app over a repo root, load settings, keymap and theme, then open a file.
//! What it cannot cover is terminal setup and the draw itself, which need a
//! terminal. That makes this a floor rather than the whole number, and the
//! floor is where a regression would show.

#[cfg(all(target_env = "musl", target_pointer_width = "64"))]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::path::PathBuf;
use std::time::Instant;

use typ_app::App;

/// The repo this test is compiled from — a real tree with real directories,
/// rather than a synthetic one whose shape flatters the file tree.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/typ-app has two ancestors")
        .to_path_buf()
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored --nocapture"]
fn cold_start_stays_under_a_tenth_of_a_second() {
    let root = repo_root();
    let file = root.join("crates/typ-app/src/app.rs");
    assert!(file.exists(), "fixture moved: {}", file.display());

    // One sample. A second run in this process would measure a warm page
    // cache and a warm allocator, which is the opposite of what "cold" means.
    let start = Instant::now();

    let mut app = App::new(&root).expect("the app builds");
    let (settings, _) = typ_app::config::load_settings(None);
    app.set_indent_width(settings.indent_width);
    app.set_whitespace(settings.whitespace);
    let (keymap, _) = typ_app::config::load_keymap(None);
    app.set_keymap(keymap);
    let (colors, syntax, _) =
        typ_app::config::load_theme(None, &settings.theme, typ_core::Depth::TrueColor);
    app.set_theme(colors);
    app.set_syntax_theme(syntax);
    app.open_path(&file).expect("the file opens");

    let elapsed = start.elapsed();

    println!("cold start over {}: {elapsed:?}", root.display());
    println!("  (no terminal setup and no first draw — see the module comment)");
    assert!(
        elapsed.as_millis() < 100,
        "cold start took {elapsed:?}, over the 100 ms budget"
    );
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored --nocapture"]
fn opening_a_file_does_not_wait_for_its_parse() {
    // The claim M2.7 rests on: grammar loading is 102 ms, and it stays off the
    // startup path because it happens lazily on the worker. If `open_path`
    // ever came to block on a parse, cold start would inherit that 102 ms and
    // this is the test that would say so.
    let root = repo_root();
    let file = root.join("crates/typ-app/src/app.rs");

    let (tx, _rx) = typ_app::run::channel();
    let mut app = App::new(&root).expect("the app builds");
    app.set_event_sender(tx);

    let start = Instant::now();
    app.open_path(&file).expect("the file opens");
    let elapsed = start.elapsed();

    println!("open_path on a .rs file, worker running: {elapsed:?}");
    assert!(
        elapsed.as_millis() < 50,
        "opening a file took {elapsed:?} — it is waiting for the parse"
    );
}

#[test]
#[ignore = "wall-clock budget; run alone with --release --ignored --nocapture --exact"]
fn cold_start_with_a_session_to_restore_stays_under_a_tenth_of_a_second() {
    // Run it on its own (`--exact` and its name): sharing a process with the
    // cold start above would hand it that test's warm page cache.
    use typ_app::pane::{Axis, Layout, PaneId};
    use typ_app::session::{SavedPane, SavedTab, Session};

    let root = repo_root();
    let tab = |path: &str, line| SavedTab {
        path: root.join(path),
        line,
        col: 0,
        top: line.saturating_sub(10),
    };
    // Written by hand rather than by an app that opened the files, which
    // would warm the very cache this is meant to find cold.
    let session = Session {
        layout: Layout::split(
            Axis::Columns,
            Layout::Leaf(PaneId(0)),
            Layout::Leaf(PaneId(1)),
        ),
        panes: vec![
            SavedPane {
                id: PaneId(0),
                tabs: vec![
                    tab("crates/typ-app/src/app.rs", 400),
                    tab("crates/typ-app/src/run.rs", 200),
                    tab("crates/typ-panel-editor/src/lib.rs", 900),
                ],
                active: 0,
            },
            SavedPane {
                id: PaneId(1),
                tabs: vec![
                    tab("crates/typ-app/src/app/panes.rs", 100),
                    tab("crates/typ-buffer/src/lib.rs", 50),
                    tab("docs/design/interface.md", 150),
                ],
                active: 2,
            },
        ],
        current: PaneId(0),
        preset: typ_app::dock::Preset::Standard,
        picked: false,
        dock_open: true,
        dock_size: 30,
    };
    let file = std::env::temp_dir()
        .join("typ-perf-startup-session")
        .join("session.toml");
    typ_app::session::save(&file, &session).expect("the session saves");

    let start = Instant::now();

    let mut app = App::new(&root).expect("the app builds");
    let (settings, _) = typ_app::config::load_settings(None);
    app.set_indent_width(settings.indent_width);
    app.set_whitespace(settings.whitespace);
    app.set_preset(settings.layout);
    let (keymap, _) = typ_app::config::load_keymap(None);
    app.set_keymap(keymap);
    let (colors, syntax, _) =
        typ_app::config::load_theme(None, &settings.theme, typ_core::Depth::TrueColor);
    app.set_theme(colors);
    app.set_syntax_theme(syntax);
    let warning = app.restore_session(&file);

    let elapsed = start.elapsed();

    assert_eq!(warning, None);
    assert_eq!(app.pane_count(), 2);
    println!("cold start restoring 2 panes, 6 tabs: {elapsed:?}");
    assert!(
        elapsed.as_millis() < 100,
        "cold start with a session took {elapsed:?}, over the 100 ms budget"
    );
}
