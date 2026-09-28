use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_app::config::load_keymap;
use typ_core::{Action, KeyChord, Motion};

fn write(name: &str, contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-config-test").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("keys.toml");
    std::fs::write(&path, contents).unwrap();
    path
}

fn chord(code: KeyCode, mods: KeyModifiers) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, mods))
}

fn move_left() -> Option<Action> {
    Some(Action::Move {
        motion: Motion::Left,
        extend: false,
    })
}

#[test]
fn no_config_file_yields_the_defaults_and_no_complaint() {
    let (keymap, warning) = load_keymap(None);
    assert!(warning.is_none());
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Left, KeyModifiers::NONE)),
        move_left()
    );
}

#[test]
fn a_missing_file_is_not_an_error() {
    let path = PathBuf::from("does/not/exist/keys.toml");
    let (_, warning) = load_keymap(Some(&path));
    assert!(warning.is_none(), "an absent config is the normal case");
}

#[test]
fn a_valid_config_is_applied_over_the_defaults() {
    let path = write("valid", "\"ctrl+e\" = \"move_line_end\"\n");
    let (keymap, warning) = load_keymap(Some(&path));
    assert!(warning.is_none());
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Char('e'), KeyModifiers::CONTROL)),
        Some(Action::Move {
            motion: Motion::LineEnd,
            extend: false
        })
    );
    // Untouched defaults survive.
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Left, KeyModifiers::NONE)),
        move_left()
    );
}

#[test]
fn a_broken_config_warns_and_falls_back_rather_than_refusing_to_start() {
    let path = write("broken", "\"ctrl+e\" = \"summon_daemon\"\n");
    let (keymap, warning) = load_keymap(Some(&path));
    let warning = warning.expect("a broken config must be reported");
    assert!(warning.contains("summon_daemon"), "warning: {warning}");
    // An editor that will not start because of a keybinding typo is a worse
    // editor than one that starts with the defaults and says so.
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Left, KeyModifiers::NONE)),
        move_left()
    );
}

#[test]
fn an_unreadable_config_warns_with_the_path_in_the_message() {
    let path = write("unreadable", "not = = toml");
    let (_, warning) = load_keymap(Some(&path));
    let warning = warning.expect("malformed TOML must be reported");
    assert!(warning.contains("keys.toml"), "warning: {warning}");
}

// Not in the plan.

#[test]
fn a_config_that_only_unbinds_leaves_everything_else_alone() {
    // Freeing a chord the terminal or window manager wants is the reason
    // unbinding exists, and it must not take the rest of the keymap with it.
    let path = write("unbind", "\"ctrl+f\" = \"\"\n");
    let (keymap, warning) = load_keymap(Some(&path));
    assert!(warning.is_none());
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Char('f'), KeyModifiers::CONTROL)),
        None
    );
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Left, KeyModifiers::NONE)),
        move_left()
    );
}

#[test]
fn one_bad_line_rejects_the_whole_file_rather_than_half_applying_it() {
    let path = write(
        "partial",
        "\"ctrl+e\" = \"move_line_end\"\n\"ctrl+r\" = \"nonsense\"\n",
    );
    let (keymap, warning) = load_keymap(Some(&path));
    assert!(warning.is_some());
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Char('e'), KeyModifiers::CONTROL)),
        None,
        "a half-applied keymap is worse than a rejected one: the user cannot \
         tell which half took effect"
    );
}

/// Two config files that exist and cannot be read: a directory where the file
/// should be, and what Notepad writes when told to save as "Unicode".
fn unreadable(name: &str, file: &str) -> Vec<PathBuf> {
    let dir = std::env::temp_dir()
        .join("typ-config-unreadable")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    let as_dir = dir.join("as-dir").join(file);
    std::fs::create_dir_all(&as_dir).unwrap();
    let utf16 = dir.join("utf16").join(file);
    std::fs::create_dir_all(utf16.parent().unwrap()).unwrap();
    let mut bytes = vec![0xFF, 0xFE];
    bytes.extend("theme = \"x\"".encode_utf16().flat_map(u16::to_le_bytes));
    std::fs::write(&utf16, bytes).unwrap();
    vec![as_dir, utf16]
}

#[test]
fn a_keys_file_that_exists_and_cannot_be_read_is_reported() {
    // Gap 97. Only `NotFound` means "there is no config"; everything else was
    // silently read as absent, and the user's bindings did not apply.
    for path in unreadable("keys", "keys.toml") {
        let (_, warning) = load_keymap(Some(&path));
        let warning = warning.unwrap_or_else(|| panic!("{} was read as absent", path.display()));
        assert!(warning.contains("keys.toml"), "{warning}");
    }
}

#[test]
fn a_settings_file_that_exists_and_cannot_be_read_is_reported() {
    for path in unreadable("settings", "config.toml") {
        let (_, warning) = typ_app::config::load_settings(Some(&path));
        let warning = warning.unwrap_or_else(|| panic!("{} was read as absent", path.display()));
        assert!(warning.contains("config.toml"), "{warning}");
    }
}
