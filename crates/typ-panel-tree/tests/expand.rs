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

/// Make a directory unreadable, and readable again on drop so it can be
/// cleaned up.
struct Unreadable(PathBuf);

impl Unreadable {
    fn new(dir: PathBuf) -> Self {
        #[cfg(windows)]
        {
            // Everyone, by SID, so the locale cannot rename it.
            let status = std::process::Command::new("icacls")
                .arg(&dir)
                .args(["/deny", "*S-1-1-0:(RX)"])
                .output()
                .expect("icacls runs");
            assert!(status.status.success(), "icacls: {status:?}");
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        assert!(
            std::fs::read_dir(&dir).is_err(),
            "the fixture is still readable, so this test would prove nothing"
        );
        Unreadable(dir)
    }
}

impl Drop for Unreadable {
    fn drop(&mut self) {
        #[cfg(windows)]
        let _ = std::process::Command::new("icacls")
            .arg(&self.0)
            .args(["/remove:d", "*S-1-1-0"])
            .output();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }
}

#[test]
fn one_unreadable_expanded_directory_does_not_freeze_the_tree() {
    // `collect` used `?` on every `read_dir` and `rebuild` propagated before
    // assigning, so an expanded directory that became unreadable made every
    // later expand or collapse of anything fail too. Gap 96.
    let dir = fixture("unreadable");
    std::fs::create_dir_all(dir.join("zother")).unwrap();
    std::fs::write(dir.join("zother/e.rs"), "").unwrap();
    let mut t = TreePanel::new(&dir).unwrap();
    t.handle_key(chord(KeyCode::Enter)); // expand sub/
    assert_eq!(t.entry_count(), 5); // sub/, deeper/, c.rs, zother/, a.rs

    let _lock = Unreadable::new(dir.join("sub"));
    t.handle_key(chord(KeyCode::Down));
    t.handle_key(chord(KeyCode::Down));
    t.handle_key(chord(KeyCode::Down)); // zother/
    assert_eq!(t.selected().unwrap().file_name().unwrap(), "zother");
    let events = t.handle_key(chord(KeyCode::Enter)); // expand zother/

    // sub/ (now shut), zother/, e.rs, a.rs.
    assert_eq!(
        t.entry_count(),
        4,
        "expanding a readable directory failed because another is not"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, typ_core::PanelEvent::Notify { .. })),
        "the unreadable directory was dropped without saying so"
    );
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
