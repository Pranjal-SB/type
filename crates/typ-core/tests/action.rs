use typ_core::{Action, Direction, Motion};

#[test]
fn actions_round_trip_through_their_names() {
    for action in Action::ALL {
        assert_eq!(
            Action::from_name(action.name()),
            Some(*action),
            "{} did not round-trip",
            action.name()
        );
    }
}

#[test]
fn names_are_snake_case_and_unique() {
    // Digits are allowed, but not as the first character: `go_to_tab_3` is a
    // name a config file can write and an identifier a reader can pronounce,
    // and `3_tab` is neither. The rule read lowercase-or-underscore until
    // `GoToTab` arrived, which was right for the actions that existed rather
    // than right about snake_case.
    let mut seen = std::collections::HashSet::new();
    for action in Action::ALL {
        let name = action.name();
        assert!(
            name.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                && !name.starts_with(|c: char| c.is_ascii_digit()),
            "{name} is not snake_case"
        );
        assert!(seen.insert(name), "{name} is used twice");
    }
}

#[test]
fn an_unknown_name_is_rejected_rather_than_guessed() {
    assert_eq!(Action::from_name("move_sideways"), None);
    assert_eq!(Action::from_name(""), None);
}

#[test]
fn every_motion_exists_in_both_moving_and_extending_form() {
    for motion in Motion::ALL {
        let moving = Action::Move {
            motion: *motion,
            extend: false,
        };
        let extending = Action::Move {
            motion: *motion,
            extend: true,
        };
        assert_ne!(moving.name(), extending.name());
        assert_eq!(Action::from_name(moving.name()), Some(moving));
        assert_eq!(Action::from_name(extending.name()), Some(extending));
    }
}

#[test]
fn insert_char_is_not_nameable() {
    // Typed text arrives as a key event, not as a binding. If it were
    // nameable, a config file could bind a key to inserting a different
    // character, which is a text-substitution feature, not a keybinding.
    assert_eq!(Action::from_name("insert_char"), None);
}

#[test]
fn directions_are_explicit_arguments_not_separate_actions() {
    let back = Action::Delete {
        direction: Direction::Backward,
        by_word: false,
    };
    let forward = Action::Delete {
        direction: Direction::Forward,
        by_word: false,
    };
    assert_ne!(back, forward);
    assert_eq!(Action::from_name("delete_backward"), Some(back));
    assert_eq!(Action::from_name("delete_forward"), Some(forward));
}
