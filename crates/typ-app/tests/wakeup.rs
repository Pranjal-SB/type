//! The loop's input is a channel, not the terminal.
//!
//! Until this milestone `event_loop` blocked inside `event::read()`, so a
//! finished parse or a file-change notification could not reach the app until
//! the user happened to press a key. These tests drive the loop body directly:
//! `run()` owns the terminal and a test has no tty.

use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::run::{Flow, channel, step};
use typ_core::AppEvent;

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-wakeup-test").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hello.rs"), "fn main() {}\n").unwrap();
    dir
}

fn app_with_file(name: &str) -> (App, PathBuf) {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    let file = dir.join("hello.rs");
    app.open_path(&file).unwrap();
    (app, file)
}

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

/// Waiting on the channel with a timeout rather than sleeping: a test that
/// sleeps to synchronise fails on a loaded runner and then gets deleted.
const WAIT: Duration = Duration::from_secs(5);

#[test]
fn a_worker_thread_wakes_the_loop_without_any_terminal_input() {
    let (_app, file) = app_with_file("worker-wakes");
    let (tx, rx) = channel();

    let sent = file.clone();
    std::thread::spawn(move || {
        tx.send(AppEvent::FileChanged(sent)).unwrap();
    });

    let received = rx.recv_timeout(WAIT).expect("the loop was never woken");
    assert!(
        matches!(&received, AppEvent::FileChanged(p) if p == &file),
        "{received:?}"
    );
}

#[test]
fn a_key_delivered_through_the_channel_reaches_the_buffer() {
    let (mut app, _) = app_with_file("key-through-channel");
    let (tx, rx) = channel();

    tx.send(AppEvent::Input(Event::Key(KeyEvent::new(
        KeyCode::Char('x'),
        KeyModifiers::NONE,
    ))))
    .unwrap();

    let event = rx.recv_timeout(WAIT).unwrap();
    let flow = step(&mut app, event, AREA).unwrap();

    assert_eq!(flow, Flow::Continue);
    assert_eq!(app.editor_mut().line_text(0), "xfn main() {}");
}

#[test]
fn the_sender_is_cloneable_so_every_worker_can_hold_one() {
    let (tx, rx) = channel();
    let a = tx.clone();
    let b = tx.clone();
    drop(tx);

    std::thread::spawn(move || a.send(AppEvent::FileChanged(PathBuf::from("a"))).unwrap());
    std::thread::spawn(move || b.send(AppEvent::FileChanged(PathBuf::from("b"))).unwrap());

    let mut seen = [
        rx.recv_timeout(WAIT).unwrap(),
        rx.recv_timeout(WAIT).unwrap(),
    ];
    seen.sort_by_key(|e| match e {
        AppEvent::FileChanged(p) => p.clone(),
        _ => PathBuf::new(),
    });
    let paths: Vec<PathBuf> = seen
        .iter()
        .map(|e| match e {
            AppEvent::FileChanged(p) => p.clone(),
            other => panic!("not a file change: {other:?}"),
        })
        .collect();
    assert_eq!(paths, vec![PathBuf::from("a"), PathBuf::from("b")]);
}

#[test]
fn quitting_stops_the_loop() {
    let (mut app, _) = app_with_file("quit");
    let quit = AppEvent::Input(Event::Key(KeyEvent::new(
        KeyCode::Char('q'),
        KeyModifiers::CONTROL,
    )));

    let flow = step(&mut app, quit, AREA).unwrap();

    assert_eq!(flow, Flow::Quit);
}

#[test]
fn the_loop_wires_the_app_to_its_workers() {
    // **The test that was missing for four releases.** From M2.7 until v0.3.0
    // `event_loop` created the event channel, gave one end to the input pump
    // and never gave the other to the app — so every shipped binary ran with
    // `parse_worker`, `find_worker` and `sender` all `None`. Syntax
    // highlighting, the picker's corpus, project search, external-change
    // reloading and every language server were fully tested and completely
    // dead.
    //
    // Every other test in the suite calls `set_event_sender` itself, which is
    // exactly why none of them could see it. This one asserts the *loop's*
    // wiring rather than its own.
    let dir = std::env::temp_dir().join("typ-wakeup-wired");
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = App::new(&dir).unwrap();
    assert!(!app.is_wired(), "a fresh app has no channel");

    let _rx = typ_app::run::wire(&mut app);
    assert!(
        app.is_wired(),
        "the loop left the app with no way to reach its workers"
    );
}
