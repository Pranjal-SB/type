use std::io::{Write, stdout};
use std::sync::mpsc;

use anyhow::Result;
use crossterm::ExecutableCommand;
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyEventKind, MouseEventKind,
};
use ratatui::layout::Rect;
use typ_core::{AppEvent, KeyChord, Panel, PanelEvent};

use crate::app::{App, Focus};

/// The end of the channel a worker holds. Cloneable, and the only way a worker
/// talks to the app — no worker is ever handed a reference to `App`.
pub type AppSender = mpsc::Sender<AppEvent>;

/// The end the loop blocks on.
pub type AppReceiver = mpsc::Receiver<AppEvent>;

pub fn channel() -> (AppSender, AppReceiver) {
    mpsc::channel()
}

/// Whether the loop goes round again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

/// Enter/leave synchronized output (CSI 2026) around a frame so partial
/// repaints are presented atomically. Terminals without support ignore it.
fn begin_frame() {
    let _ = write!(stdout(), "\x1b[?2026h");
}

fn end_frame() {
    let mut out = stdout();
    let _ = write!(out, "\x1b[?2026l");
    let _ = out.flush();
}

pub fn run(mut app: App) -> Result<()> {
    let mut terminal = ratatui::init();
    stdout().execute(EnableMouseCapture)?;
    // Without this a paste arrives as N keypresses, and any chord inside the
    // pasted text runs as a command rather than being inserted.
    stdout().execute(EnableBracketedPaste)?;

    // ratatui's own panic hook leaves raw mode and the alternate screen, but it
    // knows nothing about mouse capture or bracketed paste — which this function
    // turned on. Without this, a panic drops the user back to a shell that keeps
    // emitting mouse escape sequences and wrapping every paste in markers.
    // FINDINGS §6.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = stdout().execute(DisableBracketedPaste);
        let _ = stdout().execute(DisableMouseCapture);
        previous(info);
    }));

    let result = event_loop(&mut terminal, &mut app);

    stdout().execute(DisableBracketedPaste)?;
    stdout().execute(DisableMouseCapture)?;
    ratatui::restore();
    result
}

/// Feed terminal events into the channel from a thread of their own.
///
/// The thread cannot be joined on exit: it is parked inside a blocking
/// `event::read()` that only returns when the user presses something. Detach it
/// and let process exit collect it. Joining is a hang on quit, and it looks
/// exactly like the editor having frozen.
///
/// It ends on its own when the receiver is dropped and the send fails, which is
/// what stops it outliving the editor.
fn spawn_input_pump(tx: AppSender) {
    std::thread::spawn(move || {
        while let Ok(event) = event::read() {
            if tx.send(AppEvent::Input(event)).is_err() {
                return;
            }
        }
    });
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    let (tx, rx) = channel();
    spawn_input_pump(tx);

    loop {
        begin_frame();
        terminal.draw(|frame| app.render(frame))?;
        end_frame();

        if app.should_quit() {
            return Ok(());
        }

        // Every sender is gone only when the pump thread has died, which means
        // the terminal is gone too. Nothing left to wait for.
        let Ok(event) = rx.recv() else {
            return Ok(());
        };

        let size = terminal.size()?;
        let area = Rect::new(0, 0, size.width, size.height);

        if step(app, event, area)? == Flow::Quit {
            return Ok(());
        }
    }
}

/// One turn of the loop, without a terminal.
///
/// `run` owns the screen, so the body lives here where a test can hand it an
/// event and an area and inspect what it did.
pub fn step(app: &mut App, event: AppEvent, area: Rect) -> Result<Flow> {
    let mut events: Vec<PanelEvent> = Vec::new();

    match event {
        AppEvent::FileChanged(path) => app.handle_external_change(&path)?,
        AppEvent::Input(input) => match input {
            // Every binding lives in the keymap now, so there is nothing left
            // here to special-case. The dispatcher owns the order.
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                app.handle_chord(KeyChord::from_event(key))?;
            }
            Event::Paste(text) => app.handle_paste(text)?,
            Event::Mouse(m) => {
                if matches!(m.kind, MouseEventKind::Down(_)) {
                    app.clear_transient();
                }
                let (tree_area, editor_area) = app.areas(area);
                let in_tree = m.column < tree_area.width;

                match m.kind {
                    MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                        // One notch per event for now. The old inline drain
                        // called `event::read()` from here, which raced the pump
                        // thread the moment the terminal stopped being read from
                        // one place. Coalescing comes back in Task 4, reading
                        // the channel instead.
                        let delta: i32 = if matches!(m.kind, MouseEventKind::ScrollDown) {
                            3
                        } else {
                            -3
                        };
                        events = if in_tree {
                            app.tree_mut().handle_scroll(delta, tree_area)
                        } else {
                            app.editor_mut().handle_scroll(delta, editor_area)
                        };
                    }
                    _ => {
                        // A click both focuses the panel and is delivered to it,
                        // so clicking into an unfocused panel takes one click.
                        if in_tree {
                            if app.focus() != Focus::Tree {
                                app.cycle_focus();
                            }
                            events = app.tree_mut().handle_mouse(m, tree_area);
                        } else {
                            if app.focus() != Focus::Editor {
                                app.cycle_focus();
                            }
                            events = app.editor_mut().handle_mouse(m, editor_area);
                        }
                    }
                }
            }
            _ => {}
        },
    }

    app.apply(events)?;

    Ok(if app.should_quit() {
        Flow::Quit
    } else {
        Flow::Continue
    })
}
