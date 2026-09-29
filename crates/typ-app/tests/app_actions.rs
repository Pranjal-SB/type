//! Every action the app owns, run end to end. Gap 127.
//!
//! The check this replaces was "handled by `EditorPanel` *or* present in a
//! hand-written 27-entry list": listed rather than run, so no test executed an
//! app-owned action except two, and `perform_app_action` ends in
//! `_ => return false`, which made deleting any arm silent. That is what let
//! gap 68 ship.
//!
//! Now every action the editor does not claim has to be named below with an
//! effect it must produce, and is run through `apply_named_action`: the
//! palette's path, which is the one gap 68 broke. A new app action with no
//! expectation fails `every_app_action_has_an_expectation`.

use std::path::PathBuf;

use typ_app::prompt::PromptKind;
use typ_app::{App, Focus};
use typ_core::{Action, Panel};
use typ_picker::Mode;

/// Enough tabs that every `GoToTab(n)` names a real one.
const TABS: usize = 10;

fn app(name: &str) -> App {
    // Created, never removed first: on Windows `remove_dir_all` can return
    // before the directory is gone. Every file is rewritten below anyway.
    let dir: PathBuf = std::env::temp_dir().join("typ-app-actions").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = App::new(&dir).unwrap();
    for i in 0..TABS {
        let path = dir.join(format!("f{i}.rs"));
        std::fs::write(&path, format!("fn f{i}() {{}}\n")).unwrap();
        app.open_path(&path).unwrap();
    }
    assert_eq!(app.active_tab(), TABS - 1);
    app
}

fn run(app: &mut App, action: Action) {
    app.apply_named_action(action).unwrap();
}

fn make_dirty(app: &mut App) {
    app.editor_mut().apply_action(Action::InsertChar('x'));
    assert!(app.editor().is_dirty());
}

fn prompt_kind(app: &App) -> Option<PromptKind> {
    app.prompt().map(|p| p.kind())
}

fn picker_mode(app: &App) -> Option<Mode> {
    app.picker().map(|p| p.mode())
}

/// Run `action` on a fresh app and assert what it must have done.
///
/// `None` for an action with no expectation here, which the completeness test
/// turns into a failure for any action the editor does not handle.
fn exercise(action: Action) -> Option<fn(&mut App, Action)> {
    let check: fn(&mut App, Action) = match action {
        Action::Save => |app, a| {
            make_dirty(app);
            run(app, a);
            assert!(!app.editor().is_dirty(), "save left the buffer dirty");
        },
        Action::Quit => |app, a| {
            // Gap 68: on a dirty buffer the palette path cleared the flag it
            // then set, so the second quit never went through.
            make_dirty(app);
            run(app, a);
            assert!(!app.should_quit(), "quit discarded unsaved work unasked");
            run(app, a);
            assert!(app.should_quit(), "the confirming quit did not quit");
        },
        Action::FocusNext => |app, a| {
            assert_eq!(app.focus(), Focus::Editor);
            run(app, a);
            assert_eq!(app.focus(), Focus::Tree);
        },
        Action::GotoLine => |app, a| {
            run(app, a);
            assert_eq!(prompt_kind(app), Some(PromptKind::GotoLine));
        },
        Action::SearchOpen => |app, a| {
            run(app, a);
            assert_eq!(prompt_kind(app), Some(PromptKind::Search));
        },
        Action::ReplaceOpen => |app, a| {
            run(app, a);
            let prompt = app.prompt().expect("replace opened no prompt");
            assert!(prompt.is_replace_flow());
        },
        Action::SearchNext | Action::SearchPrevious => |app, a| {
            // Nothing searched yet, which the app has to say rather than
            // ignore the key.
            run(app, a);
            assert!(app.status().is_some(), "{} said nothing", a.name());
        },
        Action::OpenFilePicker => |app, a| {
            run(app, a);
            assert_eq!(picker_mode(app), Some(Mode::Files));
        },
        Action::OpenProjectSearch => |app, a| {
            run(app, a);
            assert_eq!(picker_mode(app), Some(Mode::Search));
        },
        Action::OpenCommandPalette => |app, a| {
            run(app, a);
            assert_eq!(picker_mode(app), Some(Mode::Commands));
        },
        Action::NextTab => |app, a| {
            run(app, a);
            assert_eq!(app.active_tab(), 0, "next from the last tab wraps");
        },
        Action::PrevTab => |app, a| {
            run(app, a);
            assert_eq!(app.active_tab(), TABS - 2);
        },
        Action::CloseTab => |app, a| {
            make_dirty(app);
            run(app, a);
            assert_eq!(app.tab_count(), TABS, "closed unsaved work unasked");
            run(app, a);
            assert_eq!(
                app.tab_count(),
                TABS - 1,
                "the confirming close did not close"
            );
        },
        Action::GoToTab(n) if (1..=9).contains(&n) => |app, a| {
            let Action::GoToTab(n) = a else {
                unreachable!()
            };
            run(app, a);
            assert_eq!(app.active_tab(), n as usize - 1);
        },
        // No server is configured, and each of these has to say so.
        Action::GotoDefinition | Action::Hover | Action::RestartLanguageServers => |app, a| {
            run(app, a);
            assert!(app.status().is_some(), "{} said nothing", a.name());
        },
        _ => return None,
    };
    Some(check)
}

/// What the editor does not claim. Asked of a throwaway app, because asking
/// means running the action.
fn app_owned() -> Vec<Action> {
    let mut probe = app("probe");
    Action::ALL
        .iter()
        .copied()
        .filter(|action| probe.editor_mut().apply_action(*action).is_none())
        .collect()
}

#[test]
fn every_app_action_has_an_expectation() {
    let missing: Vec<&str> = app_owned()
        .into_iter()
        .filter(|action| exercise(*action).is_none())
        .map(|action| action.name())
        .collect();
    assert!(
        missing.is_empty(),
        "these actions reach neither the editor nor a test of what the app does with them: \
         {missing:?}"
    );
}

#[test]
fn every_app_action_does_what_it_says() {
    for action in app_owned() {
        let Some(check) = exercise(action) else {
            continue;
        };
        let mut app = app(action.name());
        check(&mut app, action);
    }
}
