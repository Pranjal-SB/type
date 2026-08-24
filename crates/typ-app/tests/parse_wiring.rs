//! A parse actually reaches the editor.
//!
//! The unit tests in `typ-syntax` and `typ-panel-editor` cover the worker and
//! the panel's accessors; neither touches `App`, so a wiring mistake — a
//! request never made, a result never routed — passes all of them. This is the
//! test that fails when the two halves are not connected.

use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::run::{channel, step};
use typ_core::AppEvent;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

/// Wait on the channel, never on the clock.
const WAIT: Duration = Duration::from_secs(10);

fn fixture(name: &str, file: &str, contents: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-parse-wiring").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(file), contents).unwrap();
    dir
}

/// Pump events until a parse lands in the editor, applying each one.
///
/// Returns whether it arrived. Everything else on the channel is stepped too,
/// because a watcher event arriving first must not swallow the parse.
fn pump_until_parsed(app: &mut App, rx: &typ_app::run::AppReceiver) -> bool {
    loop {
        match rx.recv_timeout(WAIT) {
            Ok(event) => {
                let was_parse = matches!(event, AppEvent::Parsed(_));
                step(app, event, AREA).unwrap();
                if was_parse && app.editor().syntax().is_some() {
                    return true;
                }
            }
            Err(_) => return false,
        }
    }
}

#[test]
fn opening_a_rust_file_ends_with_a_tree_in_the_editor() {
    let dir = fixture("open", "hello.rs", "fn main() {}\n");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);
    app.open_path(&dir.join("hello.rs")).unwrap();

    assert!(
        pump_until_parsed(&mut app, &rx),
        "no parse ever reached the editor"
    );
}

#[test]
fn a_file_with_no_grammar_never_asks_for_a_parse() {
    // The floor. A `.txt` file renders exactly as it did before this
    // milestone: no worker traffic, no tree, no message.
    let dir = fixture("plain", "notes.txt", "just words\n");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);
    app.open_path(&dir.join("notes.txt")).unwrap();

    // Long enough that a parse would have arrived if one had been asked for;
    // the file is two words.
    let quiet = rx.recv_timeout(Duration::from_millis(500));
    assert!(
        !matches!(quiet, Ok(AppEvent::Parsed(_))),
        "a buffer with no grammar asked for a parse"
    );
    assert!(app.editor().syntax().is_none());
}

#[test]
fn typing_asks_for_a_fresh_parse() {
    // The edit trigger, through the real key path rather than by calling the
    // request directly — which is the half that a hook on the wrong funnel
    // would break.
    let dir = fixture("edit", "hello.rs", "fn main() {}\n");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);
    app.open_path(&dir.join("hello.rs")).unwrap();
    assert!(pump_until_parsed(&mut app, &rx), "no parse after open");

    let before = app.editor().syntax().cloned();
    step(
        &mut app,
        AppEvent::Input(Event::Key(KeyEvent::new(
            KeyCode::Char('x'),
            KeyModifiers::NONE,
        ))),
        AREA,
    )
    .unwrap();

    assert!(
        pump_until_parsed(&mut app, &rx),
        "typing did not produce a new parse"
    );
    let after = app.editor().syntax().cloned();
    assert!(
        !std::sync::Arc::ptr_eq(&before.unwrap(), &after.unwrap()),
        "the tree after typing is the same allocation as before it"
    );
}

#[test]
fn a_keyword_reaches_the_screen_in_the_themes_colour() {
    // The milestone's goal, end to end and in one test: a real file, a real
    // grammar, a real shipped theme, and a cell on screen holding the colour
    // that theme gives keywords. Everything else in this milestone is a link
    // in this chain — if any of them is wrong, this is the test that says so.
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;
    use typ_core::{Panel, RenderContext, Theme};

    let dir = fixture("painted", "hello.rs", "fn main() {}\n");
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);
    app.open_path(&dir.join("hello.rs")).unwrap();
    assert!(
        pump_until_parsed(&mut app, &rx),
        "no parse reached the editor"
    );

    let slate = typ_app::config::theme::embedded()
        .find(|(name, _)| *name == "slate")
        .map(|(_, source)| Theme::from_toml(source).unwrap())
        .expect("slate ships");
    let keyword = slate
        .syntax
        .get("keyword")
        .and_then(|style| style.fg)
        .expect("slate gives keywords a colour");

    let area = Rect::new(0, 0, 40, 6);
    let ctx = RenderContext {
        theme: &slate.colors,
        syntax: &slate.syntax,
        is_focused: true,
        panel_index: 0,
        terminal_width: area.width,
        terminal_height: area.height,
    };
    let mut buf = Buffer::empty(area);
    app.editor_mut().render(area, &mut buf, &ctx);

    let painted = buf
        .content()
        .iter()
        .find(|cell| cell.symbol() == "f" && cell.fg == keyword);
    assert!(
        painted.is_some(),
        "no cell holds `f` in the keyword colour {keyword:?} — the chain from \
         theme file to painted glyph is broken somewhere"
    );
}
