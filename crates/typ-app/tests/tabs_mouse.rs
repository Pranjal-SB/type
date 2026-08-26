//! Invariant 8: every tab interaction works with a mouse too.
//!
//! The hit test asks `tabbar::cells` for the same rectangles the renderer draws
//! into, which is what makes "the tab under the pointer" true by construction
//! rather than by two pieces of arithmetic agreeing.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::tabbar;
use typ_core::{KeyChord, Panel};

/// Deliberately not the whole terminal: a hit test that forgets the bar's own
/// origin passes every test anchored at column zero.
const FRAME: Rect = Rect {
    x: 0,
    y: 0,
    width: 100,
    height: 30,
};

fn fixture(name: &str, files: &[&str]) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-tabs-mouse").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for file in files {
        std::fs::write(dir.join(file), "fn x() {}\n").unwrap();
    }
    dir
}

fn open(name: &str, files: &[&str]) -> (App, PathBuf) {
    let dir = fixture(name, files);
    let mut app = App::new(&dir).unwrap();
    for file in files {
        app.open_path(&dir.join(file)).unwrap();
    }
    (app, dir)
}

fn press(button: MouseButton, x: u16, y: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(button),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }
}

/// The labels the app would hand the bar, so a test can lay out the same cells.
fn labels(app: &App) -> Vec<String> {
    (0..app.tab_count()).map(|i| app.tab(i).title()).collect()
}

#[test]
fn clicking_a_tab_activates_it() {
    let (mut app, _dir) = open("activate", &["a.rs", "b.rs", "c.rs"]);
    assert_eq!(app.active_tab(), 2);

    let bar = app.tab_bar_area(FRAME);
    let cells = tabbar::cells(&labels(&app), app.active_tab(), bar.width);
    let first = cells[0];

    let handled =
        app.route_tab_bar_mouse(press(MouseButton::Left, bar.x + first.x + 1, bar.y), FRAME);

    assert!(handled, "the click did not reach the bar");
    assert_eq!(app.active_tab(), 0);
}

#[test]
fn clicking_the_close_box_closes_that_tab_not_the_active_one() {
    // The distinction the whole feature turns on. Closing "the active tab"
    // whichever box was clicked is the bug that looks like it works, because
    // the active tab is usually the one under the pointer.
    let (mut app, _dir) = open("close-box", &["a.rs", "b.rs", "c.rs"]);
    assert_eq!(app.editor_title(), "c.rs");

    let bar = app.tab_bar_area(FRAME);
    let names = labels(&app);
    let cells = tabbar::cells(&names, app.active_tab(), bar.width);
    let first = cells[0];
    let close_x = tabbar::close_box_x(&first, &names[first.index]).expect("a full cell has one");

    app.route_tab_bar_mouse(press(MouseButton::Left, bar.x + close_x, bar.y), FRAME);

    assert_eq!(app.tab_count(), 2);
    assert_eq!(app.editor_title(), "c.rs", "the wrong tab was closed");
}

#[test]
fn a_middle_click_closes_the_tab_under_the_pointer() {
    // The convention every browser and terminal already carries, and it needs
    // no column of its own to be discoverable to anyone who has it.
    let (mut app, _dir) = open("middle", &["a.rs", "b.rs", "c.rs"]);

    let bar = app.tab_bar_area(FRAME);
    let cells = tabbar::cells(&labels(&app), app.active_tab(), bar.width);
    let second = cells[1];

    app.route_tab_bar_mouse(
        press(MouseButton::Middle, bar.x + second.x + 1, bar.y),
        FRAME,
    );

    assert_eq!(app.tab_count(), 2);
    assert_eq!(app.editor_title(), "c.rs", "the active tab moved");
}

#[test]
fn clicking_the_close_box_on_a_dirty_tab_asks_first() {
    // Invariant 8 says the mouse and the keyboard are peers, and Ctrl+W asks
    // before discarding unsaved work. A close box that does not is not a
    // difference in style — it is the one path in the editor that loses work
    // without saying anything, which is what the open guard used to be.
    let (mut app, _dir) = open("dirty-close-box", &["a.rs", "b.rs"]);
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('X'),
        KeyModifiers::NONE,
    )))
    .unwrap();
    assert_eq!(app.editor_title(), "b.rs *", "the fixture must be dirty");

    let bar = app.tab_bar_area(FRAME);
    let names = labels(&app);
    let cells = tabbar::cells(&names, app.active_tab(), bar.width);
    let dirty = cells[1];
    let close_x = tabbar::close_box_x(&dirty, &names[dirty.index]).expect("a full cell has one");

    app.route_tab_bar_mouse(press(MouseButton::Left, bar.x + close_x, bar.y), FRAME);

    assert_eq!(
        app.tab_count(),
        2,
        "unsaved work was discarded by one click"
    );
    assert!(
        app.status().unwrap_or_default().contains("Unsaved changes"),
        "a refusal nobody can see is indistinguishable from a broken click"
    );
}

#[test]
fn clicking_the_close_box_again_goes_through() {
    let (mut app, _dir) = open("dirty-close-twice", &["a.rs", "b.rs"]);
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('X'),
        KeyModifiers::NONE,
    )))
    .unwrap();

    let bar = app.tab_bar_area(FRAME);
    let names = labels(&app);
    let cells = tabbar::cells(&names, app.active_tab(), bar.width);
    let dirty = cells[1];
    let close_x = tabbar::close_box_x(&dirty, &names[dirty.index]).expect("a full cell has one");
    let at = press(MouseButton::Left, bar.x + close_x, bar.y);

    app.route_tab_bar_mouse(at, FRAME);
    app.route_tab_bar_mouse(at, FRAME);

    assert_eq!(app.tab_count(), 1, "the second click did not go through");
}

#[test]
fn a_middle_click_on_a_dirty_tab_asks_too() {
    let (mut app, _dir) = open("dirty-middle", &["a.rs", "b.rs"]);
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('X'),
        KeyModifiers::NONE,
    )))
    .unwrap();

    let bar = app.tab_bar_area(FRAME);
    let cells = tabbar::cells(&labels(&app), app.active_tab(), bar.width);
    let dirty = cells[1];

    app.route_tab_bar_mouse(
        press(MouseButton::Middle, bar.x + dirty.x + 1, bar.y),
        FRAME,
    );

    assert_eq!(
        app.tab_count(),
        2,
        "unsaved work was discarded by one click"
    );
}

#[test]
fn confirming_one_tab_does_not_arm_the_close_box_of_another() {
    // The trap the old open guard carried a path *and* an event counter to
    // avoid: an answer to one question must not answer a different one.
    let (mut app, _dir) = open("wrong-tab", &["a.rs", "b.rs", "c.rs"]);
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('X'),
        KeyModifiers::NONE,
    )))
    .unwrap();

    let bar = app.tab_bar_area(FRAME);
    let names = labels(&app);
    let cells = tabbar::cells(&names, app.active_tab(), bar.width);
    let dirty = cells[2];
    let clean = cells[0];
    let dirty_x = tabbar::close_box_x(&dirty, &names[dirty.index]).expect("full");
    let clean_x = tabbar::close_box_x(&clean, &names[clean.index]).expect("full");

    // Ask about the dirty tab, then click a different tab's close box.
    app.route_tab_bar_mouse(press(MouseButton::Left, bar.x + dirty_x, bar.y), FRAME);
    app.route_tab_bar_mouse(press(MouseButton::Left, bar.x + clean_x, bar.y), FRAME);

    assert_eq!(app.tab_count(), 2, "the clean tab should have closed");
    assert!(
        labels(&app).iter().any(|t| t == "c.rs *"),
        "the dirty tab was closed by an answer meant for another question"
    );
}

#[test]
fn clicking_the_empty_space_past_the_last_tab_does_nothing() {
    let (mut app, _dir) = open("empty", &["a.rs", "b.rs"]);
    let before = app.active_tab();

    let bar = app.tab_bar_area(FRAME);
    let handled = app.route_tab_bar_mouse(press(MouseButton::Left, bar.right() - 1, bar.y), FRAME);

    assert!(handled, "the bar let a click fall through to the editor");
    assert_eq!(app.active_tab(), before);
    assert_eq!(app.tab_count(), 2);
}

#[test]
fn a_click_below_the_bar_is_not_the_bars() {
    let (mut app, _dir) = open("below", &["a.rs", "b.rs"]);

    let bar = app.tab_bar_area(FRAME);
    let handled = app.route_tab_bar_mouse(press(MouseButton::Left, bar.x + 1, bar.y + 1), FRAME);

    assert!(!handled, "the bar claimed a click on the editor");
}

#[test]
fn there_is_no_bar_to_click_with_one_tab_open() {
    let (mut app, _dir) = open("one-tab", &["a.rs"]);

    let handled = app.route_tab_bar_mouse(press(MouseButton::Left, 40, 0), FRAME);

    assert!(!handled, "a bar that is not drawn answered a click");
}

#[test]
fn a_click_resolves_against_the_scroll_offset() {
    // The bug this guards activates the third file you opened when you click
    // the third *visible* tab after the bar has scrolled — right exactly once,
    // before anyone opens enough files to scroll it.
    let names: Vec<String> = (0..20).map(|i| format!("file{i}.rs")).collect();
    let borrowed: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
    let (mut app, _dir) = open("scrolled", &borrowed);
    assert_eq!(app.active_tab(), 19);

    let bar = app.tab_bar_area(FRAME);
    let cells = tabbar::cells(&labels(&app), app.active_tab(), bar.width);
    let leftmost = cells[0];
    assert!(leftmost.index > 0, "the bar did not scroll");

    app.route_tab_bar_mouse(
        press(MouseButton::Left, bar.x + leftmost.x + 1, bar.y),
        FRAME,
    );

    assert_eq!(
        app.active_tab(),
        leftmost.index,
        "the click resolved against the whole list instead of the visible page"
    );
}

#[test]
fn the_last_column_of_a_clipped_tab_is_not_a_close_box() {
    // A cell cut off by the bar's edge has no × in it, so the column that would
    // have held one is part of the name. Closing a file because its name ran
    // long is not a thing anyone should be able to do by accident.
    let long = "an-extremely-long-generated-file-name.rs".to_string();
    let cell = tabbar::cells(std::slice::from_ref(&long), 0, 12)[0];

    assert_eq!(cell.width, 12, "the fixture must produce a clipped cell");
    assert_eq!(tabbar::close_box_x(&cell, &long), None);
}
