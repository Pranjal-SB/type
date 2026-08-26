//! A parse result belongs to a buffer, and with tabs there is more than one.
//!
//! Every piece of parse state was app-global when exactly one buffer existed,
//! and each one is wrong in a different way once there are two. These are the
//! three failures, written before the fix so each is confirmed to be real.

use std::path::PathBuf;
use std::time::Duration;

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

/// Wait on the channel, never on the clock.
const WAIT: Duration = Duration::from_secs(10);

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-tab-parse").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("first.rs"), "fn first() {}\n").unwrap();
    std::fs::write(dir.join("second.rs"), "fn second() {}\n").unwrap();
    dir
}

/// Pump until the tab at `index` holds a tree, or the channel goes quiet.
///
/// Named by tab rather than by "the editor" because the whole point of these
/// tests is that the two can differ.
fn pump_until_tab_parsed(app: &mut App, rx: &typ_app::run::AppReceiver, index: usize) -> bool {
    loop {
        match rx.recv_timeout(WAIT) {
            Ok(event) => {
                let was_parse = matches!(event, AppEvent::Parsed(_));
                step(app, event, AREA).unwrap();
                if was_parse && app.tab(index).syntax().is_some() {
                    return true;
                }
            }
            Err(_) => return false,
        }
    }
}

#[test]
fn a_second_tab_at_the_same_revision_as_the_first_still_gets_parsed() {
    // `parsed_revision` was one number on the app, and a freshly opened buffer
    // is at revision 0. So the second file opened is compared against the
    // first's revision, matches, and is never parsed — which is not an edge
    // case, it is what happens the first time anyone opens a second file.
    let dir = fixture("same-revision");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);

    app.open_path(&dir.join("first.rs")).unwrap();
    assert!(
        pump_until_tab_parsed(&mut app, &rx, 0),
        "first.rs never parsed"
    );

    app.open_in_new_tab(&dir.join("second.rs")).unwrap();
    assert_eq!(app.tab_count(), 2);
    assert!(
        pump_until_tab_parsed(&mut app, &rx, 1),
        "the second tab never got a tree"
    );
}

#[test]
fn a_parse_that_lands_after_a_switch_goes_to_the_tab_that_asked_for_it() {
    // `awaited_generation` was one slot on the app, so asking for a second
    // parse forgot the first. Switch away while a parse is in flight and its
    // result is dropped on arrival — but the tab's revision was already
    // recorded as requested, so nothing ever asks again and that buffer stays
    // unhighlighted for as long as it is open.
    let dir = fixture("switch-midflight");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);

    app.open_path(&dir.join("first.rs")).unwrap();
    assert!(
        pump_until_tab_parsed(&mut app, &rx, 0),
        "first.rs never parsed"
    );

    // Second tab asks for a parse, then loses focus before the answer lands.
    app.open_in_new_tab(&dir.join("second.rs")).unwrap();
    app.activate_tab(0);
    assert_eq!(app.active_tab(), 0);

    assert!(
        pump_until_tab_parsed(&mut app, &rx, 1),
        "the backgrounded tab's own parse was thrown away"
    );
}

#[test]
fn switching_back_to_an_unchanged_tab_does_not_ask_for_another_parse() {
    // The other half: per-tab state must not turn every switch into a re-parse,
    // which would put the cost of the whole workspace on one keystroke.
    let dir = fixture("no-rework");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);

    app.open_path(&dir.join("first.rs")).unwrap();
    assert!(
        pump_until_tab_parsed(&mut app, &rx, 0),
        "first.rs never parsed"
    );
    app.open_in_new_tab(&dir.join("second.rs")).unwrap();
    assert!(
        pump_until_tab_parsed(&mut app, &rx, 1),
        "second.rs never parsed"
    );

    app.activate_tab(0);
    let quiet = rx.recv_timeout(Duration::from_millis(500));
    assert!(
        !matches!(quiet, Ok(AppEvent::Parsed(_))),
        "switching to an unedited buffer re-parsed it"
    );
}

#[test]
fn each_tab_keeps_its_own_tree() {
    let dir = fixture("own-tree");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);

    app.open_path(&dir.join("first.rs")).unwrap();
    assert!(
        pump_until_tab_parsed(&mut app, &rx, 0),
        "first.rs never parsed"
    );
    let first = app.tab(0).syntax().cloned().expect("a tree for first.rs");

    app.open_in_new_tab(&dir.join("second.rs")).unwrap();
    assert!(
        pump_until_tab_parsed(&mut app, &rx, 1),
        "second.rs never parsed"
    );
    let second = app.tab(1).syntax().cloned().expect("a tree for second.rs");

    assert!(
        !std::sync::Arc::ptr_eq(&first, &second),
        "both tabs are sharing one syntax tree"
    );
}
