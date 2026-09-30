use typ_core::{Action, Direction, Group, Motion};

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

/// Every name `Action::name` can return, read from its source.
///
/// `name()` is an exhaustive match, so the compiler makes a new variant give
/// itself a name there. `ALL` is a hand-written list the compiler knows nothing
/// about. Reading the match's string literals is what ties the two together: a
/// variant with a name and no place in `ALL` fails here, rather than shipping
/// unbindable and missing from the palette while every test that iterates
/// `ALL` stays green. Gap 121.
fn names_in_the_source() -> Vec<&'static str> {
    let source = include_str!("../src/action.rs");
    let start = source
        .find("pub fn name(&self)")
        .expect("Action::name moved");
    let end = source[start..]
        .find("pub fn from_name")
        .expect("Action::from_name moved")
        + start;
    source[start..end]
        .split("=> \"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .collect()
}

#[test]
fn every_name_the_match_can_return_is_in_all() {
    // Typed text is deliberately unbindable; see the `Action` docs. The
    // `go_to_tab_N` names come from a table rather than a literal arm, so the
    // scan does not see them and `actions_round_trip_through_their_names`
    // covers them instead.
    let unreachable = ["insert_char_literal"];
    let names = names_in_the_source();
    assert!(names.len() > 40, "the scan found only {names:?}");
    for name in names {
        if unreachable.contains(&name) {
            continue;
        }
        assert!(
            Action::from_name(name).is_some(),
            "{name} has a name and is missing from Action::ALL"
        );
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

#[test]
fn every_action_says_what_it_does_in_a_lowercase_phrase() {
    // The menu and the palette show this string, not the snake_case name, so
    // it reads as a phrase: a lowercase verb first and no full stop, the way a
    // menu row reads in every editor that has one.
    let mut seen = std::collections::HashSet::new();
    for action in Action::ALL {
        let description = action.description();
        let name = action.name();
        assert!(!description.is_empty(), "{name} has no description");
        assert!(
            description.starts_with(|c: char| c.is_ascii_lowercase()),
            "{name}: {description:?} does not start with a lowercase verb"
        );
        assert!(
            !description.ends_with('.'),
            "{name}: {description:?} ends in a full stop"
        );
        assert!(
            seen.insert(description),
            "{name}: {description:?} is used twice"
        );
    }
}

#[test]
fn every_group_holds_something() {
    // A group nothing belongs to is a heading the menu can never draw.
    for group in Group::ALL {
        assert!(
            Action::ALL.iter().any(|a| a.group() == *group),
            "{group:?} is empty"
        );
    }
}
