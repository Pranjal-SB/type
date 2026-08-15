use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_app::App;
use typ_core::{Action, KeyChord, Keymap, Motion};

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-dispatch-test").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hello.rs"), "fn main() {}\n").unwrap();
    // A second entry, so "move the selection down" has somewhere to go. Sorted
    // after hello.rs, so the first entry — the one Enter activates — is
    // unchanged.
    std::fs::write(dir.join("zz.rs"), "fn other() {}\n").unwrap();
    dir
}

fn chord(code: KeyCode, mods: KeyModifiers) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, mods))
}

fn app_with_file(name: &str) -> App {
    let dir = fixture(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("hello.rs")).unwrap();
    app
}

#[test]
fn a_bound_chord_reaches_the_focused_panel() {
    let mut app = app_with_file("bound");
    app.handle_chord(chord(KeyCode::Right, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.editor_mut().cursor().col, 1);
}

#[test]
fn an_unbound_printable_character_is_typed() {
    let mut app = app_with_file("typing");
    app.handle_chord(chord(KeyCode::Char('x'), KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.editor_mut().line_text(0), "xfn main() {}");
}

#[test]
fn a_control_chord_with_no_binding_types_nothing() {
    let mut app = app_with_file("unbound-ctrl");
    app.handle_chord(chord(KeyCode::Char('j'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.editor_mut().line_text(0), "fn main() {}");
}

#[test]
fn tab_cycles_focus_rather_than_reaching_the_panel() {
    let mut app = app_with_file("focus");
    assert_eq!(app.focused_name(), "editor");
    app.handle_chord(chord(KeyCode::Tab, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(app.focused_name(), "tree");
}

#[test]
fn quit_is_handled_by_the_app_and_still_guards_unsaved_work() {
    let mut app = app_with_file("quit");
    app.handle_chord(chord(KeyCode::Char('x'), KeyModifiers::NONE))
        .unwrap();
    app.handle_chord(chord(KeyCode::Char('q'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(!app.should_quit(), "unsaved changes must still prompt");
    app.handle_chord(chord(KeyCode::Char('q'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.should_quit());
}

#[test]
fn save_reports_through_the_status_bar() {
    let mut app = app_with_file("save");
    app.handle_chord(chord(KeyCode::Char('x'), KeyModifiers::NONE))
        .unwrap();
    app.handle_chord(chord(KeyCode::Char('s'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.status(), Some("Saved."));
}

#[test]
fn a_rebound_key_takes_effect() {
    let mut app = app_with_file("rebind");
    let mut keymap = Keymap::default_bindings();
    keymap.merge_toml("\"ctrl+e\" = \"move_line_end\"").unwrap();
    app.set_keymap(keymap);
    app.handle_chord(chord(KeyCode::Char('e'), KeyModifiers::CONTROL))
        .unwrap();
    assert_eq!(app.editor_mut().cursor().col, 12);
}

#[test]
fn an_action_the_panel_ignores_falls_through_to_the_app() {
    let mut app = app_with_file("fallthrough");
    // The tree has no Undo, so it must not swallow it.
    app.cycle_focus();
    assert_eq!(app.focused_name(), "tree");
    app.handle_chord(chord(KeyCode::Char('z'), KeyModifiers::CONTROL))
        .unwrap();
    assert!(app.status().is_none(), "a no-op must not report an error");
}

#[test]
fn the_keymap_is_readable_for_help_text() {
    let app = app_with_file("help");
    assert!(app.keymap().bindings_for(Action::Save).contains(&"ctrl+s"));
    assert!(
        app.keymap()
            .bindings_for(Action::Move {
                motion: Motion::Left,
                extend: false
            })
            .contains(&"left")
    );
}

#[test]
fn multi_cursor_reaches_the_keyboard() {
    let mut app = app_with_file("multicursor");
    // The whole point of the milestone: this chord did nothing before the
    // dispatcher existed, because the editor's legacy key arms had no idea what
    // an action was.
    app.handle_chord(chord(
        KeyCode::Down,
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    ))
    .unwrap();
    assert_eq!(
        app.editor_mut().selections().len(),
        2,
        "ctrl+alt+down must stack a cursor"
    );
}

#[test]
fn shift_arrow_extends_a_selection_from_the_keyboard() {
    let mut app = app_with_file("extend");
    app.handle_chord(chord(KeyCode::Right, KeyModifiers::SHIFT))
        .unwrap();
    app.handle_chord(chord(KeyCode::Right, KeyModifiers::SHIFT))
        .unwrap();
    let primary = app.editor_mut().selections().primary();
    assert!(
        !primary.is_empty(),
        "shift+right must select, not just move"
    );
    assert_eq!(primary.anchor.col, 0);
    assert_eq!(primary.head.col, 2);
}

// The tree navigates on raw keys, and every one of them — up, down, enter,
// left, right — is bound in the keymap to an editor action. A dispatcher that
// stops at "the app did not want it either" swallows all five and the file tree
// goes dead. These are the regression tests for that.

#[test]
fn arrow_keys_still_move_the_tree_selection() {
    let mut app = app_with_file("tree-arrows");
    app.cycle_focus();
    assert_eq!(app.focused_name(), "tree");
    let before = app.tree_mut().selected().map(|p| p.to_path_buf());
    app.handle_chord(chord(KeyCode::Down, KeyModifiers::NONE))
        .unwrap();
    assert_ne!(
        app.tree_mut().selected().map(|p| p.to_path_buf()),
        before,
        "down is bound to an editor motion, but the tree must still receive it"
    );
}

#[test]
fn enter_still_opens_a_file_from_the_tree() {
    let dir = fixture("tree-enter");
    let mut app = App::new(&dir).unwrap();
    assert_eq!(app.focused_name(), "tree");
    assert_eq!(app.editor_title(), "untitled");

    app.handle_chord(chord(KeyCode::Enter, KeyModifiers::NONE))
        .unwrap();
    assert_eq!(
        app.editor_title(),
        "hello.rs",
        "enter is bound to insert_newline, but the tree must still open the file"
    );
}
