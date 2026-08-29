//! Servers come from config, and the defaults need none.
//!
//! Three decisions here were checked against the editors that had to make them
//! rather than reasoned from the specification, which says nothing about any of
//! them: where a server is rooted, what to do when the binary is not really
//! there, and what to do when one keeps dying.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::lsp::ServerConfig;
use typ_app::run::{AppReceiver, channel, step_batch};
use typ_core::AppEvent;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 60,
    height: 12,
};

const WAIT: Duration = Duration::from_secs(10);

fn fake() -> &'static str {
    env!("CARGO_BIN_EXE_typ-app-fake-server")
}

fn root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-lsp-config").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &Path, contents: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn server(command: &str, flags: &[&str]) -> ServerConfig {
    ServerConfig {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: command.into(),
        args: flags.iter().map(|f| f.to_string()).collect(),
        roots: vec!["Cargo.toml".into()],
    }
}

fn key(c: char) -> AppEvent {
    AppEvent::Input(Event::Key(KeyEvent::new(
        KeyCode::Char(c),
        KeyModifiers::NONE,
    )))
}

fn pump_until(app: &mut App, rx: &AppReceiver, done: impl Fn(&App) -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if done(app) {
            return true;
        }
        if let Ok(event) = rx.recv_timeout(Duration::from_millis(250)) {
            let mut batch = vec![event];
            batch.extend(rx.try_iter());
            step_batch(app, batch, AREA).unwrap();
        }
    }
    done(app)
}

fn settle(app: &mut App, rx: &AppReceiver) {
    let deadline = Instant::now() + Duration::from_millis(600);
    while Instant::now() < deadline {
        if let Ok(event) = rx.recv_timeout(Duration::from_millis(50)) {
            let mut batch = vec![event];
            batch.extend(rx.try_iter());
            step_batch(app, batch, AREA).unwrap();
        }
    }
}

fn app_at(dir: &Path, config: ServerConfig) -> (App, AppReceiver) {
    let (tx, rx) = channel();
    let mut app = App::new(dir).unwrap();
    app.set_language_servers(vec![config]);
    app.set_event_sender(tx);
    (app, rx)
}

// --- the defaults ---------------------------------------------------------

#[test]
fn rust_and_toml_are_configured_without_a_config_file() {
    // The whole point of a default: a machine with the toolchain installed and
    // no `config.toml` still gets diagnostics.
    let settings = typ_app::config::Settings::default();
    let names: Vec<&str> = settings
        .language_servers
        .iter()
        .map(|s| s.language_id.as_str())
        .collect();
    assert!(names.contains(&"rust"), "{names:?}");
    assert!(names.contains(&"toml"), "{names:?}");
}

// --- roots ----------------------------------------------------------------

#[test]
fn a_workspace_gets_one_server_not_one_per_member() {
    // **The defect the plan would have shipped.** With "nearest ancestor wins"
    // these two files have different roots — `crates/a` and `crates/b`, each
    // holding a `Cargo.toml` — and one server per root means one rust-analyzer
    // per open crate, each indexing the whole workspace. TYPE is an
    // eleven-crate workspace, so TYPE would have been the first thing to melt.
    let dir = root("workspace");
    write(&dir.join("Cargo.toml"), "[workspace]\n");
    write(&dir.join("crates/a/Cargo.toml"), "[package]\n");
    write(&dir.join("crates/b/Cargo.toml"), "[package]\n");
    write(&dir.join("crates/a/src/lib.rs"), "fn a() {}\n");
    write(&dir.join("crates/b/src/lib.rs"), "fn b() {}\n");

    let (mut app, rx) = app_at(&dir, server(fake(), &[]));
    app.open_path(&dir.join("crates/a/src/lib.rs")).unwrap();
    app.open_path(&dir.join("crates/b/src/lib.rs")).unwrap();
    assert!(pump_until(&mut app, &rx, |a| a
        .lsp_notifications_of("textDocument/didOpen")
        == 2));

    assert_eq!(
        app.language_servers_running(),
        1,
        "one workspace, one server"
    );
}

#[test]
fn two_projects_get_two_servers() {
    let dir = root("two-projects");
    write(&dir.join("one/Cargo.toml"), "[package]\n");
    write(&dir.join("two/Cargo.toml"), "[package]\n");
    write(&dir.join("one/src/lib.rs"), "fn a() {}\n");
    write(&dir.join("two/src/lib.rs"), "fn b() {}\n");

    let (mut app, rx) = app_at(&dir, server(fake(), &[]));
    app.open_path(&dir.join("one/src/lib.rs")).unwrap();
    app.open_path(&dir.join("two/src/lib.rs")).unwrap();
    assert!(pump_until(&mut app, &rx, |a| a
        .lsp_notifications_of("textDocument/didOpen")
        == 2));

    assert_eq!(app.language_servers_running(), 2);
}

// --- what happens when it is not really there -----------------------------

#[test]
fn a_binary_that_is_not_on_path_is_silent_and_editing_continues() {
    // The default state on most machines. It must be boring.
    let dir = root("absent");
    write(&dir.join("a.rs"), "fn main() {}\n");
    let (mut app, rx) = app_at(&dir, server("definitely-not-a-language-server", &[]));
    app.open_path(&dir.join("a.rs")).unwrap();
    step_batch(&mut app, vec![key('x')], AREA).unwrap();
    settle(&mut app, &rx);

    assert_eq!(app.editor().buffer().line_text(0), "xfn main() {}");
    assert_eq!(app.language_servers_running(), 0);
}

#[test]
fn a_server_that_dies_before_the_handshake_says_why_in_its_own_words() {
    // **Measured, not guessed.** rustup puts a `rust-analyzer` shim on `PATH`
    // whether the component is installed or not: the spawn succeeds, nothing
    // arrives, and seconds later the stream closes with one line of stderr
    // saying "Unknown binary 'rust-analyzer.exe' in official toolchain". So
    // `NotFound` is *not* the path most machines take, and the only thing that
    // explains this one is the server's own last words. Zed reports a failed
    // start the same way, error plus captured stderr.
    let dir = root("dies");
    write(&dir.join("a.rs"), "fn main() {}\n");
    let (mut app, rx) = app_at(&dir, server(fake(), &["--exit-now"]));
    app.open_path(&dir.join("a.rs")).unwrap();

    assert!(
        pump_until(&mut app, &rx, |a| a.status().is_some()),
        "a server that died said nothing"
    );
    let status = app.status().unwrap().to_string();
    assert!(
        status.contains("fake-server") || status.contains("did not start"),
        "status was: {status}"
    );
    // And the editor is still an editor.
    step_batch(&mut app, vec![key('x')], AREA).unwrap();
    assert_eq!(app.editor().buffer().line_text(0), "xfn main() {}");
}

#[test]
fn a_server_that_never_started_is_not_restarted_into_a_loop() {
    let dir = root("no-loop");
    write(&dir.join("a.rs"), "fn main() {}\n");
    let (mut app, rx) = app_at(&dir, server(fake(), &["--exit-now"]));
    app.open_path(&dir.join("a.rs")).unwrap();
    assert!(pump_until(&mut app, &rx, |a| a.status().is_some()));
    settle(&mut app, &rx);
    assert_eq!(app.language_servers_running(), 0, "it kept respawning");
}

// --- restarting -----------------------------------------------------------

#[test]
fn a_stopped_server_can_be_started_again_by_asking() {
    // The other half of a crash-loop guard: something has to be able to say "I
    // fixed it". Helix spells it `:lsp-restart`; TYPE reaches it through the
    // palette like every other named action.
    let dir = root("restart");
    write(&dir.join("a.rs"), "fn main() {}\n");
    let (mut app, rx) = app_at(&dir, server(fake(), &["--exit-now"]));
    app.open_path(&dir.join("a.rs")).unwrap();
    assert!(pump_until(&mut app, &rx, |a| a.status().is_some()));

    app.apply_named_action(typ_core::Action::RestartLanguageServers)
        .unwrap();
    assert!(
        app.status().unwrap_or_default().contains("Restart"),
        "status was: {:?}",
        app.status()
    );
}

#[test]
fn restarting_with_nothing_stopped_says_so() {
    let dir = root("restart-none");
    write(&dir.join("a.rs"), "fn main() {}\n");
    let (mut app, _rx) = app_at(&dir, server(fake(), &[]));
    app.apply_named_action(typ_core::Action::RestartLanguageServers)
        .unwrap();
    assert_eq!(app.status(), Some("No language server to restart."));
}

#[test]
fn the_restart_action_is_reachable_by_name() {
    assert!(typ_core::Action::ALL.contains(&typ_core::Action::RestartLanguageServers));
    assert_eq!(
        typ_core::Action::from_name("restart_language_servers"),
        Some(typ_core::Action::RestartLanguageServers)
    );
}
