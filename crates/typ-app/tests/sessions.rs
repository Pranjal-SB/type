//! Sessions: quit on a project and come back to the same panes, ratios, tabs
//! and cursors. Stored in the state directory; every test here points
//! `TYP_STATE_DIR` at its own temp directory, so none touches a real one.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_app::App;
use typ_app::config::load_settings;
use typ_app::dock::Preset;
use typ_app::session::{self, Session};
use typ_core::{Action, KeyChord};

/// The state directory for this binary. Every test sets the same value, so
/// the process-wide variable never differs between threads.
fn state_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("typ-sessions-state");
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: every test in this binary writes the same value, and nothing in
    // it reads the variable on another thread expecting a different one.
    unsafe { std::env::set_var("TYP_STATE_DIR", &dir) };
    dir
}

fn project(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-sessions").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["a.rs", "b.rs", "c.rs"] {
        let text: String = (0..40).map(|n| format!("line {n}\n")).collect();
        std::fs::write(dir.join(file), text).unwrap();
    }
    dir
}

fn session_file(root: &Path) -> PathBuf {
    state_dir();
    let path = session::file_for(root).expect("TYP_STATE_DIR names a directory");
    let _ = std::fs::remove_file(&path);
    path
}

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    app.handle_chord(KeyChord::from_event(KeyEvent::new(code, mods)))
        .unwrap();
}

/// Two panes, three tabs, cursors moved, the rule off centre.
fn worked_on(root: &Path) -> App {
    let mut app = App::new(root).unwrap();
    app.open_path(&root.join("a.rs")).unwrap();
    app.open_path(&root.join("b.rs")).unwrap();
    app.editor_mut().goto(12, 3);
    app.apply_named_action(Action::SplitRight).unwrap();
    app.close_picker();
    app.open_path(&root.join("c.rs")).unwrap();
    app.editor_mut().goto(30, 2);
    app.apply_named_action(Action::ResizePanes).unwrap();
    for _ in 0..5 {
        press(&mut app, KeyCode::Left, KeyModifiers::NONE);
    }
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    app
}

#[test]
fn quitting_and_starting_again_brings_the_layout_back() {
    let root = project("round-trip");
    let file = session_file(&root);
    let before = worked_on(&root);
    let saved = before.session();
    assert_eq!(saved.panes.len(), 2);
    assert!(
        matches!(saved.layout, typ_app::pane::Layout::Split { ratio, .. } if ratio < 0.45),
        "the rule did not move: {:?}",
        saved.layout
    );
    session::save(&file, &saved).unwrap();

    let mut after = App::new(&root).unwrap();
    let warning = after.restore_session(&file);
    assert_eq!(warning, None);

    assert_eq!(
        after.session(),
        saved,
        "the restored app is not the one that quit"
    );
    assert_eq!(after.pane_count(), 2);
    assert_eq!(after.current_pane(), 1);
    assert_eq!(after.editor().cursor().line, 30);
    assert_eq!(after.editor().cursor().col, 2);
    assert_eq!(after.tab_count(), 1);
    after.apply_named_action(Action::FocusPane(1)).unwrap();
    assert_eq!(after.tab_count(), 2);
    assert_eq!(after.editor().path(), Some(root.join("b.rs").as_path()));
    assert_eq!(after.editor().cursor().line, 12);
}

#[test]
fn the_dock_and_the_preset_come_back_too() {
    let root = project("dock");
    let file = session_file(&root);
    let mut before = App::new(&root).unwrap();
    before.open_path(&root.join("a.rs")).unwrap();
    before.apply_named_action(Action::CycleLayout).unwrap();
    session::save(&file, &before.session()).unwrap();

    let mut after = App::new(&root).unwrap();
    assert_eq!(after.restore_session(&file), None);
    assert_eq!(after.preset(), Preset::Focus);
    assert!(!after.dock_open());
}

#[test]
fn a_file_that_has_gone_is_skipped_and_named() {
    let root = project("deleted");
    let file = session_file(&root);
    let mut before = App::new(&root).unwrap();
    before.open_path(&root.join("a.rs")).unwrap();
    before.open_path(&root.join("b.rs")).unwrap();
    session::save(&file, &before.session()).unwrap();
    std::fs::remove_file(root.join("b.rs")).unwrap();

    let mut after = App::new(&root).unwrap();
    let warning = after
        .restore_session(&file)
        .expect("the missing file went unmentioned");
    assert!(warning.contains("b.rs"), "{warning}");
    assert_eq!(after.tab_count(), 1);
    assert_eq!(after.editor().path(), Some(root.join("a.rs").as_path()));
}

#[test]
fn a_session_that_does_not_parse_starts_clean_with_a_warning() {
    let root = project("garbage");
    let file = session_file(&root);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "this is [[ not toml").unwrap();

    let mut app = App::new(&root).unwrap();
    let warning = app.restore_session(&file).expect("garbage said nothing");
    assert!(warning.contains("session"), "{warning}");
    assert_eq!(app.pane_count(), 1);
    assert_eq!(app.tab_count(), 1);
    assert_eq!(app.editor().path(), None);
}

#[test]
fn no_session_file_is_no_warning() {
    let root = project("none");
    let file = session_file(&root);
    let mut app = App::new(&root).unwrap();
    assert_eq!(app.restore_session(&file), None);
}

#[test]
fn saving_leaves_no_temp_file_behind() {
    let root = project("atomic");
    let file = session_file(&root);
    let app = worked_on(&root);
    session::save(&file, &app.session()).unwrap();
    session::save(&file, &app.session()).unwrap();
    let dir = file.parent().unwrap();
    let names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(names.is_empty(), "{names:?}");
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(Session::from_toml(&text).unwrap(), app.session());
}

#[test]
fn the_session_lives_in_the_state_dir() {
    let state = state_dir();
    let root = project("where");
    let file = session::file_for(&root).unwrap();
    assert!(
        file.starts_with(state.join("sessions")),
        "{}",
        file.display()
    );
    let other = session::file_for(&project("elsewhere")).unwrap();
    assert_ne!(file, other, "two projects shared a session file");
}

#[test]
fn session_false_in_config_turns_it_off() {
    let path = project("config").join("config.toml");
    std::fs::write(&path, "session = false\n").unwrap();
    let (settings, warning) = load_settings(Some(&path));
    assert_eq!(warning, None);
    assert!(!settings.session);
    assert!(typ_app::config::Settings::default().session);
}
