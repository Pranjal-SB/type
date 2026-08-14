//! Directory expansion — the walking skeleton listed one flat directory and
//! refused to open the folders it drew.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_core::{KeyChord, Panel};
use typ_panel_tree::TreePanel;

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-tree-expand").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub/deeper")).unwrap();
    std::fs::write(dir.join("a.rs"), "").unwrap();
    std::fs::write(dir.join("sub/c.rs"), "").unwrap();
    std::fs::write(dir.join("sub/deeper/d.rs"), "").unwrap();
    dir
}

fn chord(code: KeyCode) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn enter_on_a_directory_reveals_its_children() {
    let mut t = TreePanel::new(&fixture("expand")).unwrap();
    assert_eq!(t.entry_count(), 2); // sub/, a.rs
    t.handle_key(chord(KeyCode::Enter)); // sub/
    assert_eq!(t.entry_count(), 4); // sub/, deeper/, c.rs, a.rs
}

#[test]
fn enter_on_an_expanded_directory_collapses_it() {
    let mut t = TreePanel::new(&fixture("collapse")).unwrap();
    t.handle_key(chord(KeyCode::Enter));
    t.handle_key(chord(KeyCode::Enter));
    assert_eq!(t.entry_count(), 2);
}

#[test]
fn children_are_nested_under_their_parent() {
    let mut t = TreePanel::new(&fixture("nesting")).unwrap();
    t.handle_key(chord(KeyCode::Enter));
    t.handle_key(chord(KeyCode::Down)); // deeper/
    assert_eq!(t.depth_of_selection(), 1);
    assert_eq!(t.selected().unwrap().file_name().unwrap(), "deeper");
}

#[test]
fn nesting_goes_deeper_than_one_level() {
    let mut t = TreePanel::new(&fixture("deep")).unwrap();
    t.handle_key(chord(KeyCode::Enter)); // expand sub/
    t.handle_key(chord(KeyCode::Down)); // deeper/
    t.handle_key(chord(KeyCode::Enter)); // expand deeper/
    assert_eq!(t.entry_count(), 5); // sub/, deeper/, d.rs, c.rs, a.rs
    t.handle_key(chord(KeyCode::Down)); // d.rs
    assert_eq!(t.depth_of_selection(), 2);
}

#[test]
fn collapsing_a_parent_hides_grandchildren() {
    let mut t = TreePanel::new(&fixture("grandchildren")).unwrap();
    t.handle_key(chord(KeyCode::Enter));
    t.handle_key(chord(KeyCode::Down));
    t.handle_key(chord(KeyCode::Enter)); // deeper/ expanded
    t.handle_key(chord(KeyCode::Up)); // back to sub/
    t.handle_key(chord(KeyCode::Enter)); // collapse sub/
    assert_eq!(t.entry_count(), 2);
}

#[test]
fn left_collapses_and_right_expands() {
    let mut t = TreePanel::new(&fixture("arrows")).unwrap();
    t.handle_key(chord(KeyCode::Right));
    assert_eq!(t.entry_count(), 4);
    t.handle_key(chord(KeyCode::Left));
    assert_eq!(t.entry_count(), 2);
}

#[test]
fn the_selection_survives_a_collapse_above_it() {
    let mut t = TreePanel::new(&fixture("selection")).unwrap();
    t.handle_key(chord(KeyCode::Enter)); // expand sub/
    t.handle_key(chord(KeyCode::Down)); // deeper/
    t.handle_key(chord(KeyCode::Down)); // c.rs
    t.handle_key(chord(KeyCode::Down)); // a.rs
    assert_eq!(t.selected().unwrap().file_name().unwrap(), "a.rs");
    t.handle_key(chord(KeyCode::Up));
    t.handle_key(chord(KeyCode::Up));
    t.handle_key(chord(KeyCode::Up));
    t.handle_key(chord(KeyCode::Enter)); // collapse sub/
    assert_eq!(t.selected().unwrap().file_name().unwrap(), "sub");
}
