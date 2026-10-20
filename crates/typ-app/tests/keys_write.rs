//! Writing one binding into `keys.toml` and keeping everything else (M4.5
//! Task 1). Every test writes in its own temp directory.

use std::path::PathBuf;

use typ_app::config::{load_keymap, write_binding};
use typ_core::Action;

fn path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-keys-write").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("keys.toml")
}

const COMMENTED: &str = "\
# My keys. Keep this note.
\"ctrl+e\" = \"open_file_picker\"   # like the old editor

# Panes.
\"alt+s\" = \"split_right\"
";

fn bound(path: &std::path::Path, chord: &str) -> Option<Action> {
    let (keymap, warning) = load_keymap(Some(path));
    assert!(warning.is_none(), "{warning:?}");
    keymap
        .rows()
        .find(|(c, _)| *c == chord)
        .map(|(_, action)| action)
}

#[test]
fn a_new_binding_adds_one_line_and_keeps_every_other_byte() {
    let path = path("new");
    std::fs::write(&path, COMMENTED).unwrap();

    write_binding(&path, "ctrl+k s", Some("save")).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(after, format!("{COMMENTED}\"ctrl+k s\" = \"save\"\n"));
    assert_eq!(bound(&path, "ctrl+k s"), Some(Action::Save));
}

#[test]
fn rebinding_a_row_changes_its_value_and_keeps_the_comment_after_it() {
    let path = path("rebind");
    std::fs::write(&path, COMMENTED).unwrap();

    write_binding(&path, "ctrl+e", Some("open_project_search")).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        after,
        COMMENTED.replace(
            "\"ctrl+e\" = \"open_file_picker\"",
            "\"ctrl+e\" = \"open_project_search\""
        )
    );
}

#[test]
fn an_empty_action_unbinds_as_keys_toml_already_means() {
    let path = path("unbind");
    std::fs::write(&path, COMMENTED).unwrap();

    write_binding(&path, "ctrl+d", Some("")).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.ends_with("\"ctrl+d\" = \"\"\n"), "{after}");
    assert_eq!(bound(&path, "ctrl+d"), None);
}

#[test]
fn none_takes_the_row_out_and_the_default_comes_back() {
    let path = path("reset");
    std::fs::write(
        &path,
        "# Keep me.\n\"ctrl+d\" = \"\"\n\"alt+s\" = \"split_right\"\n",
    )
    .unwrap();

    write_binding(&path, "ctrl+d", None).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(after, "# Keep me.\n\"alt+s\" = \"split_right\"\n");
    assert_eq!(bound(&path, "ctrl+d"), Some(Action::SelectNextOccurrence));
}

#[test]
fn the_row_is_found_however_the_file_spells_the_chord() {
    let path = path("spelling");
    std::fs::write(&path, "\"Shift+Ctrl+E\" = \"save\"\n").unwrap();

    write_binding(&path, "ctrl+shift+e", Some("quit")).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(after, "\"Shift+Ctrl+E\" = \"quit\"\n");
}

#[test]
fn a_missing_file_is_created_with_just_that_binding() {
    let path = path("missing");

    write_binding(&path, "alt+s", Some("split_down")).unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(after, "\"alt+s\" = \"split_down\"\n");
    assert_eq!(bound(&path, "alt+s"), Some(Action::SplitDown));
}
