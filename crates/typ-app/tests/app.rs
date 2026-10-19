use std::path::PathBuf;

use ratatui::layout::Rect;
use typ_app::App;
use typ_app::dock::Dock;
use typ_core::{Action, PanelEvent};

/// One directory per test — see the tree panel fixture for why sharing races.
fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-app-test").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hello.rs"), "fn main() {}\n").unwrap();
    dir
}

#[test]
fn a_new_app_focuses_the_tree() {
    let app = App::new(&fixture("focus-tree")).unwrap();
    assert_eq!(app.focused_name(), "tree");
}

#[test]
fn cycling_focus_moves_to_the_editor_the_status_bar_and_back() {
    let mut app = App::new(&fixture("cycle")).unwrap();
    app.cycle_focus();
    assert_eq!(app.focused_name(), "editor");
    app.cycle_focus();
    assert_eq!(app.focused_name(), "status bar");
    app.cycle_focus();
    assert_eq!(app.focused_name(), "tree");
}

#[test]
fn applying_quit_sets_the_quit_flag() {
    let mut app = App::new(&fixture("quit")).unwrap();
    assert!(!app.should_quit());
    app.apply_named_action(Action::Quit).unwrap();
    assert!(app.should_quit());
}

#[test]
fn open_file_event_loads_the_file_into_the_editor() {
    let dir = fixture("open");
    let mut app = App::new(&dir).unwrap();
    app.apply(vec![PanelEvent::OpenFile {
        path: dir.join("hello.rs"),
        at: typ_core::Position::default(),
    }])
    .unwrap();
    assert_eq!(app.editor_title(), "hello.rs");
}

#[test]
fn opening_a_file_moves_focus_to_the_editor() {
    let dir = fixture("open-focus");
    let mut app = App::new(&dir).unwrap();
    app.apply(vec![PanelEvent::OpenFile {
        path: dir.join("hello.rs"),
        at: typ_core::Position::default(),
    }])
    .unwrap();
    assert_eq!(app.focused_name(), "editor");
}

#[test]
fn layout_gives_the_tree_a_fixed_width_sidebar() {
    let body = Dock::left().areas(Rect::new(0, 0, 100, 30));
    // The sidebar and its rule take 30 columns between them: 29 of list, one
    // of line. The editor starts on the column after the rule, so no cell is
    // drawn twice and neither panel has to know what sits beside it.
    assert_eq!(body.content.width, 29);
    assert_eq!(body.rule, Rect::new(29, 0, 1, 30));
    assert_eq!(body.rest.x, 30);
    assert_eq!(body.rest.width, 70);
    assert_eq!(body.heading, Rect::new(0, 0, 29, 1));
    assert_eq!(body.content.y, 1, "the list starts under its heading");
}

#[test]
fn layout_shrinks_the_sidebar_on_narrow_terminals() {
    // Exact, not `< 30` and `> 0`, which a one-column sidebar satisfied.
    // Gap 137. Under 60 columns the sidebar is a third of the width.
    let body = Dock::left().areas(Rect::new(0, 0, 40, 30));
    assert_eq!(body.content.width, 12);
    assert_eq!(body.rule.x, 12);
    assert_eq!(body.rest.x, 13);
    assert_eq!(body.rest.width, 27);
}

#[test]
fn layout_survives_a_frame_too_small_to_split() {
    for width in 0..4 {
        for height in 0..3 {
            let body = Dock::left().areas(Rect::new(0, 0, width, height));
            assert!(body.rest.right() <= width.max(body.rest.x));
            assert!(body.content.height <= height);
        }
    }
}
