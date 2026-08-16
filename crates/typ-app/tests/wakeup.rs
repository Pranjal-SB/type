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
    assert_eq!(received, AppEvent::FileChanged(file));
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

    let mut seen = vec![
        rx.recv_timeout(WAIT).unwrap(),
        rx.recv_timeout(WAIT).unwrap(),
    ];
    seen.sort_by_key(|e| match e {
        AppEvent::FileChanged(p) => p.clone(),
        _ => PathBuf::new(),
    });
    assert_eq!(
        seen,
        vec![
            AppEvent::FileChanged(PathBuf::from("a")),
            AppEvent::FileChanged(PathBuf::from("b")),
        ]
    );
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
