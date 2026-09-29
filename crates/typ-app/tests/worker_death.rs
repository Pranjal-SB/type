//! A worker thread that dies is noticed and said, not waited on.
//!
//! Both workers exit once nobody receives their results, which is how these
//! tests kill them: drop the receiving end, then keep asking. A panic on the
//! worker (a grammar whose queries do not compile) is the way it happens for
//! real. Before, `is_wired` kept answering true and the app stamped a
//! generation that would never arrive. Gap 91.

use std::time::{Duration, Instant};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::run::{channel, step_batch};
use typ_core::AppEvent;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

fn fixture(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("typ-worker-death").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "fn a() {}\n").unwrap();
    dir
}

/// Keep doing `poke` until the app stops claiming to be wired, or give up.
fn until_unwired(app: &mut App, mut poke: impl FnMut(&mut App)) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if !app.is_wired() {
            return true;
        }
        poke(app);
        std::thread::sleep(Duration::from_millis(10));
    }
    !app.is_wired()
}

#[test]
fn a_dead_parse_worker_is_noticed_and_said() {
    let dir = fixture("parse");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);
    drop(rx);
    app.open_path(&dir.join("a.rs")).unwrap();

    let key = AppEvent::Input(Event::Key(KeyEvent::new(
        KeyCode::Char('x'),
        KeyModifiers::NONE,
    )));
    assert!(
        until_unwired(&mut app, |a| {
            step_batch(a, vec![key.clone()], AREA).unwrap();
        }),
        "the parse worker died and the app still says it is wired"
    );
    assert!(
        app.status().is_some_and(|s| s.contains("highlighting")),
        "status was: {:?}",
        app.status()
    );
}

#[test]
fn a_dead_find_worker_is_noticed_and_said() {
    let dir = fixture("find");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);
    drop(rx);

    assert!(
        until_unwired(&mut app, |a| {
            a.request_filter("a".into(), 10);
        }),
        "the find worker died and the app still says it is wired"
    );
    assert!(
        app.status().is_some_and(|s| s.contains("search")),
        "status was: {:?}",
        app.status()
    );
}
