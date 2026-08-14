use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_core::{Action, Direction, KeyChord, Keymap, Motion};

fn chord(code: KeyCode, mods: KeyModifiers) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, mods))
}

#[test]
fn the_defaults_bind_the_arrows() {
    let keymap = Keymap::default_bindings();
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Left, KeyModifiers::NONE)),
        Some(Action::Move {
            motion: Motion::Left,
            extend: false
        })
    );
}

#[test]
fn shift_extends_the_selection_rather_than_moving() {
    let keymap = Keymap::default_bindings();
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Left, KeyModifiers::SHIFT)),
        Some(Action::Move {
            motion: Motion::Left,
            extend: true
        })
    );
}

#[test]
fn ctrl_arrows_move_by_word() {
    let keymap = Keymap::default_bindings();
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Right, KeyModifiers::CONTROL)),
        Some(Action::Move {
            motion: Motion::WordRight,
            extend: false
        })
    );
    assert_eq!(
        keymap.lookup(&chord(
            KeyCode::Right,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT
        )),
        Some(Action::Move {
            motion: Motion::WordRight,
            extend: true
        })
    );
}

#[test]
fn an_unbound_chord_returns_nothing() {
    let keymap = Keymap::default_bindings();
    assert_eq!(
        keymap.lookup(&chord(KeyCode::F(12), KeyModifiers::NONE)),
        None
    );
}

#[test]
fn config_overrides_a_default_binding() {
    let mut keymap = Keymap::default_bindings();
    keymap
        .merge_toml("\"ctrl+d\" = \"delete_forward\"")
        .unwrap();
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Char('d'), KeyModifiers::CONTROL)),
        Some(Action::Delete {
            direction: Direction::Forward,
            by_word: false
        })
    );
}

#[test]
fn config_can_unbind_a_key_with_an_empty_action() {
    let mut keymap = Keymap::default_bindings();
    keymap.merge_toml("\"ctrl+z\" = \"\"").unwrap();
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Char('z'), KeyModifiers::CONTROL)),
        None
    );
}

#[test]
fn an_unknown_action_name_is_an_error_naming_the_action() {
    let mut keymap = Keymap::default_bindings();
    let err = keymap
        .merge_toml("\"ctrl+k\" = \"summon_daemon\"")
        .unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("summon_daemon"), "error was: {text}");
    assert!(text.contains("ctrl+k"), "error was: {text}");
}

#[test]
fn malformed_toml_is_an_error_not_a_panic() {
    let mut keymap = Keymap::default_bindings();
    assert!(keymap.merge_toml("this is not toml = = =").is_err());
}

#[test]
fn a_rejected_config_leaves_the_previous_bindings_intact() {
    let mut keymap = Keymap::default_bindings();
    let _ = keymap.merge_toml("\"ctrl+k\" = \"summon_daemon\"");
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Left, KeyModifiers::NONE)),
        Some(Action::Move {
            motion: Motion::Left,
            extend: false
        })
    );
}

#[test]
fn bindings_can_be_looked_up_backwards_for_help_text() {
    let keymap = Keymap::default_bindings();
    let bindings = keymap.bindings_for(Action::Save);
    assert!(bindings.contains(&"ctrl+s"), "bindings were: {bindings:?}");
}
