//! The file changed underneath the editor.
//!
//! Gap analysis defect 31, and a data-loss bug: a rebase, a formatter or
//! another editor writes the file while it is open, and before this the next
//! save silently overwrote the lot.
//!
//! Three states, one of them automatic:
//!
//! | Buffer | On external change |
//! |---|---|
//! | Clean | reload silently |
//! | Dirty | warn, change nothing |
//! | Deleted | warn, keep the buffer |

use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::run::{channel, step};
use typ_core::AppEvent;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

/// Wait on the channel, never on the clock. A watcher test that sleeps to
/// synchronise fails on a loaded runner and then gets deleted.
const WAIT: Duration = Duration::from_secs(10);

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-external-change").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hello.rs"), "fn main() {}\n").unwrap();
    dir
}

/// An app with the file open and the watcher running, plus the receiver the
/// watcher reports through.
fn watching(name: &str) -> (App, PathBuf, typ_app::run::AppReceiver) {
    let dir = fixture(name);
    let file = dir.join("hello.rs");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);
    app.open_path(&file).unwrap();
    (app, file, rx)
}

/// Drain until the watcher reports our file, or fail on the timeout.
fn await_change(rx: &typ_app::run::AppReceiver, file: &PathBuf) -> AppEvent {
    loop {
        match rx.recv_timeout(WAIT) {
            Ok(AppEvent::FileChanged(p)) if &p == file => return AppEvent::FileChanged(p),
            Ok(_) => continue,
            Err(e) => panic!("the watcher never reported {}: {e}", file.display()),
        }
    }
}

fn type_char(app: &mut App, ch: char) {
    let event = AppEvent::Input(Event::Key(KeyEvent::new(
        KeyCode::Char(ch),
        KeyModifiers::NONE,
    )));
    step(app, event, AREA).unwrap();
}

#[test]
fn an_external_write_to_a_clean_buffer_is_picked_up() {
    let (mut app, file, rx) = watching("clean-reload");

    std::fs::write(&file, "fn main() { changed_on_disk() }\n").unwrap();

    let event = await_change(&rx, &file);
    step(&mut app, event, AREA).unwrap();

    assert_eq!(
        app.editor_mut().line_text(0),
        "fn main() { changed_on_disk() }"
    );
}

#[test]
fn a_dirty_buffer_is_never_silently_reloaded() {
    let (mut app, file, rx) = watching("dirty-kept");
    type_char(&mut app, 'x');
    assert!(app.editor_mut().is_dirty());

    std::fs::write(&file, "fn main() { changed_on_disk() }\n").unwrap();

    let event = await_change(&rx, &file);
    step(&mut app, event, AREA).unwrap();

    assert_eq!(app.editor_mut().line_text(0), "xfn main() {}");
    let status = app.status().unwrap_or_default().to_string();
    assert!(
        status.contains("changed on disk"),
        "the user was not told; status was {status:?}"
    );
}

#[test]
fn a_deleted_file_leaves_the_buffer_standing() {
    let (mut app, file, rx) = watching("deleted");

    std::fs::remove_file(&file).unwrap();

    let event = await_change(&rx, &file);
    step(&mut app, event, AREA).unwrap();

    assert_eq!(app.editor_mut().line_text(0), "fn main() {}");
    let status = app.status().unwrap_or_default().to_string();
    assert!(
        status.contains("deleted"),
        "the user was not told; status was {status:?}"
    );
}

/// Our own save writes the file, the watcher reports it, and a naive handler
/// reloads the buffer from the file it just wrote. Harmless when it works and a
/// race when it does not, so it is decided here rather than found as a flake.
#[test]
fn our_own_save_does_not_come_back_as_an_external_change() {
    let (mut app, file, rx) = watching("own-save");
    type_char(&mut app, 'x');

    app.editor_mut().save().unwrap();

    let event = await_change(&rx, &file);
    step(&mut app, event, AREA).unwrap();

    assert_eq!(app.editor_mut().line_text(0), "xfn main() {}");
    assert_eq!(
        app.status(),
        None,
        "a save reported back as an external change"
    );
}
