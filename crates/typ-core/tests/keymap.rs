use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_core::keymap::Resolved;
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
    // F12 was the unbound key this reached for until M3 gave it
    // goto-definition. F9 is what is left: nothing claims it, and a test that
    // asserts a miss has to name a key nothing hits.
    assert_eq!(
        keymap.lookup(&chord(KeyCode::F(9), KeyModifiers::NONE)),
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
fn a_config_chord_matches_however_it_is_capitalised_or_ordered() {
    let mut keymap = Keymap::default_bindings();
    keymap
        .merge_toml(
            "\"Ctrl+Shift+K\" = \"save\"\n\"shift+alt+J\" = \"save\"\n\"PageUp\" = \"save\"",
        )
        .unwrap();
    for (code, mods) in [
        (
            KeyCode::Char('k'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ),
        (KeyCode::Char('j'), KeyModifiers::ALT | KeyModifiers::SHIFT),
        (KeyCode::PageUp, KeyModifiers::NONE),
    ] {
        assert_eq!(
            keymap.lookup(&chord(code, mods)),
            Some(Action::Save),
            "{code:?} with {mods:?} did not reach the binding"
        );
    }
}

#[test]
fn a_config_chord_that_names_no_key_is_an_error_naming_the_chord() {
    for spelling in ["C-s", "pgup", "ctrl+", "hyper+x", "ctrl+shift"] {
        let mut keymap = Keymap::default_bindings();
        let err = keymap
            .merge_toml(&format!("\"{spelling}\" = \"save\""))
            .expect_err(spelling);
        let text = format!("{err:#}");
        assert!(text.contains(spelling), "error was: {text}");
    }
}

#[test]
fn every_default_chord_is_one_a_config_may_spell() {
    let defaults = Keymap::default_bindings();
    for action in Action::ALL {
        for chord in defaults.bindings_for(*action) {
            let mut keymap = Keymap::default_bindings();
            keymap
                .merge_toml(&format!("\"{chord}\" = \"{}\"", action.name()))
                .unwrap_or_else(|e| panic!("{chord} was refused: {e:#}"));
        }
    }
}

#[test]
fn bindings_can_be_looked_up_backwards_for_help_text() {
    let keymap = Keymap::default_bindings();
    let bindings = keymap.bindings_for(Action::Save);
    assert!(bindings.contains(&"ctrl+s"), "bindings were: {bindings:?}");
}

// --- sequences: `ctrl+k f` is a row like any other (controls.md §2) --------

#[test]
fn a_sequence_in_a_config_is_canonicalised_step_by_step() {
    let mut keymap = Keymap::default_bindings();
    keymap
        .merge_toml("\"Ctrl+K  F\" = \"open_project_search\"")
        .unwrap();
    let bindings = keymap.bindings_for(Action::OpenProjectSearch);
    assert!(
        bindings.contains(&"ctrl+k f"),
        "bindings were: {bindings:?}"
    );
}

#[test]
fn a_chord_bound_alone_and_as_a_prefix_is_an_error_naming_both() {
    let mut keymap = Keymap::default_bindings();
    let err = keymap
        .merge_toml("\"ctrl+k\" = \"save\"\n\"ctrl+k f\" = \"open_project_search\"")
        .unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("\"ctrl+k\""), "error was: {text}");
    // Whichever sequence it found first: every one under ctrl+k is a clash.
    assert!(text.contains("\"ctrl+k "), "error was: {text}");
    // Rejected whole, like any other bad config.
    assert_eq!(
        keymap.lookup(&chord(KeyCode::Char('k'), KeyModifiers::CONTROL)),
        None
    );
}

#[test]
fn a_sequence_of_three_keys_is_an_error() {
    // Nothing needs one, and the menu has one level.
    let mut keymap = Keymap::default_bindings();
    let err = keymap.merge_toml("\"ctrl+k f g\" = \"save\"").unwrap_err();
    let text = format!("{err:#}");
    assert!(text.contains("ctrl+k f g"), "error was: {text}");
}

#[test]
fn the_defaults_bind_no_chord_both_alone_and_as_a_prefix() {
    // An empty config still runs the check over the whole table, defaults
    // included.
    Keymap::default_bindings().merge_toml("").unwrap();
}

// --- resolving a prefix ------------------------------------------------------

#[test]
fn a_prefix_resolves_to_pending_with_every_row_under_it() {
    let keymap = Keymap::default_bindings();
    let ctrl_k = chord(KeyCode::Char('k'), KeyModifiers::CONTROL);
    match keymap.resolve(None, &ctrl_k) {
        Resolved::Pending(rows) => assert!(
            rows.contains(&("f".to_string(), Action::OpenProjectSearch)),
            "rows were: {rows:?}"
        ),
        other => panic!("ctrl+k resolved to {other:?}"),
    }
}

#[test]
fn the_second_step_resolves_against_the_prefix() {
    let keymap = Keymap::default_bindings();
    let f = chord(KeyCode::Char('f'), KeyModifiers::NONE);
    assert_eq!(
        keymap.resolve(Some("ctrl+k"), &f),
        Resolved::Matched(Action::OpenProjectSearch)
    );
    let q = chord(KeyCode::Char('q'), KeyModifiers::NONE);
    assert_eq!(keymap.resolve(Some("ctrl+k"), &q), Resolved::NotFound);
}

#[test]
fn an_ordinary_chord_resolves_as_it_always_looked_up() {
    let keymap = Keymap::default_bindings();
    assert_eq!(
        keymap.resolve(None, &chord(KeyCode::Char('s'), KeyModifiers::CONTROL)),
        Resolved::Matched(Action::Save)
    );
    // Typed text: bound to nothing and the start of nothing.
    assert_eq!(
        keymap.resolve(None, &chord(KeyCode::Char('f'), KeyModifiers::NONE)),
        Resolved::NotFound
    );
}

#[test]
fn the_door_holds_what_has_no_chord_every_terminal_sends() {
    let rows = Keymap::default_bindings().under("ctrl+k");
    for (key, action) in [
        ("f", Action::OpenProjectSearch),
        ("p", Action::OpenCommandPalette),
        ("g", Action::GotoLine),
        ("w", Action::CloseTab),
        ("r", Action::RestartLanguageServers),
        ("h", Action::Hover),
        ("d", Action::GotoDefinition),
    ] {
        assert!(
            rows.contains(&(key.to_string(), action)),
            "ctrl+k {key} is not {}; rows were {rows:?}",
            action.name()
        );
    }
    // The Enhanced-tier chords stay as second bindings.
    let keymap = Keymap::default_bindings();
    assert!(
        keymap
            .bindings_for(Action::OpenProjectSearch)
            .contains(&"ctrl+shift+f")
    );
    assert!(
        keymap
            .bindings_for(Action::OpenCommandPalette)
            .contains(&"ctrl+shift+p")
    );
}
