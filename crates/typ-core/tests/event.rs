use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_core::{HandlerId, KeyChord, NotifyLevel, PanelEvent, PanelId};

#[test]
fn plain_char_canonicalizes_to_itself() {
    let k = KeyChord::from_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    assert_eq!(k.canonical, "a");
}

#[test]
fn ctrl_modifier_is_prefixed() {
    let k = KeyChord::from_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert_eq!(k.canonical, "ctrl+s");
}

#[test]
fn modifiers_are_ordered_consistently() {
    let mods = KeyModifiers::CONTROL | KeyModifiers::SHIFT | KeyModifiers::ALT;
    let k = KeyChord::from_event(KeyEvent::new(KeyCode::Char('p'), mods));
    assert_eq!(k.canonical, "ctrl+alt+shift+p");
}

#[test]
fn named_keys_use_lowercase_names() {
    let k = KeyChord::from_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(k.canonical, "enter");
    let k = KeyChord::from_event(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE));
    assert_eq!(k.canonical, "f5");
}

#[test]
fn panel_event_stays_small() {
    // This vocabulary is capped deliberately. New panels register a handler
    // and route through OpenWith rather than adding variants.
    let all = [
        PanelEvent::NeedsRedraw,
        PanelEvent::Quit,
        PanelEvent::CloseSelf,
        PanelEvent::Focus(PanelId(0)),
        PanelEvent::OpenFile {
            path: "x".into(),
            line: 0,
            col: 0,
        },
        PanelEvent::OpenWith {
            handler: HandlerId("editor"),
            path: "x".into(),
        },
        PanelEvent::RunCommand {
            command: "ls".into(),
            cwd: None,
        },
        PanelEvent::Notify {
            level: NotifyLevel::Info,
            message: "hi".into(),
        },
    ];
    assert_eq!(all.len(), 8);

    // The assert above only counts what this test constructs, so on its own it
    // would still pass after a 9th variant were added. This match is the part
    // that actually holds the line: it is exhaustive with no wildcard arm, so
    // adding a variant fails to compile here and forces a decision about
    // whether the vocabulary really needed to grow.
    for e in &all {
        match e {
            PanelEvent::NeedsRedraw
            | PanelEvent::Quit
            | PanelEvent::CloseSelf
            | PanelEvent::Focus(_)
            | PanelEvent::OpenFile { .. }
            | PanelEvent::OpenWith { .. }
            | PanelEvent::RunCommand { .. }
            | PanelEvent::Notify { .. } => {}
        }
    }
}
