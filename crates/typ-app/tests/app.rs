use std::path::PathBuf;

use ratatui::layout::Rect;
use typ_app::App;
use typ_app::layout::split;
use typ_core::PanelEvent;

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
fn cycling_focus_moves_to_the_editor_and_back() {
    let mut app = App::new(&fixture("cycle")).unwrap();
    app.cycle_focus();
    assert_eq!(app.focused_name(), "editor");
    app.cycle_focus();
    assert_eq!(app.focused_name(), "tree");
}

#[test]
fn applying_quit_sets_the_quit_flag() {
    let mut app = App::new(&fixture("quit")).unwrap();
    assert!(!app.should_quit());
    app.apply(vec![PanelEvent::Quit]).unwrap();
    assert!(app.should_quit());
}

#[test]
fn open_file_event_loads_the_file_into_the_editor() {
    let dir = fixture("open");
    let mut app = App::new(&dir).unwrap();
    app.apply(vec![PanelEvent::OpenFile {
        path: dir.join("hello.rs"),
        line: 0,
        col: 0,
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
        line: 0,
        col: 0,
    }])
    .unwrap();
    assert_eq!(app.focused_name(), "editor");
}

#[test]
fn layout_gives_the_tree_a_fixed_width_sidebar() {
    let (tree, editor) = split(Rect::new(0, 0, 100, 30));
    assert_eq!(tree.width, 30);
    assert_eq!(editor.x, 30);
    assert_eq!(editor.width, 70);
}

#[test]
fn layout_shrinks_the_sidebar_on_narrow_terminals() {
    let (tree, editor) = split(Rect::new(0, 0, 40, 30));
    assert!(tree.width < 30);
    assert!(editor.width > 0);
}
