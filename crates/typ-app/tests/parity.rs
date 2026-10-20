//! Interface §0.1 as a build failure: every action has a keyboard path and a
//! mouse path.
//!
//! The keyboard path is a binding, or a row in the palette, which is typed.
//! The mouse path is a row in the palette, which is clicked. The palette's
//! rows are read off the palette as it opens, not off `Action::ALL`, so an
//! action the palette stops listing fails here.

use std::path::PathBuf;

use typ_app::App;
use typ_core::Action;

fn app() -> App {
    let dir: PathBuf = std::env::temp_dir().join("typ-parity");
    std::fs::create_dir_all(&dir).unwrap();
    App::new(&dir).unwrap()
}

/// Every action named by a row of the command palette.
fn palette_rows(app: &mut App) -> Vec<String> {
    app.apply_named_action(Action::OpenCommandPalette).unwrap();
    let rows = app
        .picker()
        .expect("the palette did not open")
        .commands()
        .iter()
        .map(|row| row.name.clone())
        .collect();
    app.close_picker();
    rows
}

/// Actions whose mouse path is not a palette row, each with the one it has.
fn exceptions(app: &App) -> Vec<(Action, &'static str, bool)> {
    let door = app.keymap().under("ctrl+k");
    vec![(
        // A palette row that opens the palette would reopen what is already
        // up. It is clicked as the door's row instead, and the door is a
        // palette row of its own (`show the ctrl+k menu`).
        Action::OpenCommandPalette,
        "a row of the ctrl+k menu",
        door.iter().any(|(_, a)| *a == Action::OpenCommandPalette),
    )]
}

#[test]
fn every_action_has_a_keyboard_path_and_a_mouse_path() {
    let mut app = app();
    let palette = palette_rows(&mut app);
    let excepted = exceptions(&app);

    let mut missing = Vec::new();
    for action in Action::ALL {
        let in_palette = palette.iter().any(|name| name == action.name());
        let bound = !app.keymap().bindings_for(*action).is_empty();
        if !(bound || in_palette) {
            missing.push(format!("{}: no binding and no palette row", action.name()));
        }
        if in_palette {
            continue;
        }
        match excepted.iter().find(|(a, _, _)| a == action) {
            Some((_, _, true)) => {}
            Some((_, path, false)) => missing.push(format!(
                "{}: its mouse path, {path}, is gone",
                action.name()
            )),
            None => missing.push(format!("{}: no palette row to click", action.name())),
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}
