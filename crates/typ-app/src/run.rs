use std::io::{Write, stdout};
use std::sync::mpsc;

use anyhow::Result;
use crossterm::ExecutableCommand;
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyEventKind, MouseEvent, MouseEventKind,
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

    // `shutdown` then `exit`, before the tree kill that `Drop` would do anyway.
    // rust-analyzer writes state on shutdown, and killing it every time is how
    // a stale lock outlives the editor.
    app.shutdown_language_servers();

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
        // `Terminal::draw` diffs against the previous buffer and emits only the
        // cells that changed, so a redundant draw costs no terminal traffic —
        // but it still costs the whole render pass, which is 439 µs deep in a
        // 50k-line file. That is what the flag is for.
        if app.take_dirty() {
            begin_frame();
            terminal.draw(|frame| app.render(frame))?;
            end_frame();
        }

        if app.should_quit() {
            return Ok(());
        }

        // Every sender is gone only when the pump thread has died, which means
        // the terminal is gone too. Nothing left to wait for.
        let Ok(first) = rx.recv() else {
            return Ok(());
        };

        // Block for one, then take everything already queued behind it. One
        // frame for the batch rather than one per event.
        let mut batch = vec![first];
        batch.extend(rx.try_iter());

        let size = terminal.size()?;
        let area = Rect::new(0, 0, size.width, size.height);

        if step_batch(app, batch, area)? == Flow::Quit {
            return Ok(());
        }
    }
}

/// Dispatch a batch of events, then answer once.
///
/// Taken from yazi, which blocks for one event and then drains everything else
/// already queued before rendering. The alternative — draw per event — costs a
/// full render pass for every notch of a scroll, every character of a paste and
/// every event of a watcher burst.
///
/// Not taken from yazi: its 10 ms minimum between frames. That bounds the frame
/// rate under a burst, which draining already does, and pays up to 10 ms of
/// latency on every keystroke to do it. Against a 16 ms keystroke-to-glyph
/// budget that is most of the budget spent on a problem already solved.
///
/// Takes a `Vec` rather than the receiver so a test can hand it a batch without
/// threads.
pub fn step_batch(app: &mut App, events: Vec<AppEvent>, area: Rect) -> Result<Flow> {
    let mut events = events.into_iter().peekable();

    while let Some(event) = events.next() {
        // Fold a run of wheel events into one scroll. Only a *consecutive*
        // run, and only over the same panel, so nothing is reordered and
        // nothing is discarded — the batch is a vector being folded, not a
        // queue being drained past. The old coalescing read ahead and `break`ed
        // on the first non-scroll, which dropped it: flick the wheel while
        // typing and a character vanished.
        if let Some((m, mut delta)) = as_scroll(&event) {
            let side = app.areas(area).0.width;
            let same_panel = |n: &MouseEvent| (n.column < side) == (m.column < side);
            while let Some(d) = events
                .peek()
                .and_then(as_scroll)
                .filter(|(n, _)| same_panel(n))
                .map(|(_, d)| d)
            {
                delta += d;
                events.next();
            }
            if scroll_step(app, m, delta, area)? == Flow::Quit {
                return Ok(Flow::Quit);
            }
            continue;
        }

        if step(app, event, area)? == Flow::Quit {
            return Ok(Flow::Quit);
        }
    }

    // The coalescing point. Once per batch, after everything in it has been
    // applied, so a ten-key burst is one `didChange` and a document opened
    // before its server finished the handshake is announced on the pass after
    // the response lands.
    app.sync_language_servers();
    Ok(Flow::Continue)
}

/// A wheel event and the rows it asks for, or `None` for anything else.
fn as_scroll(event: &AppEvent) -> Option<(MouseEvent, i32)> {
    let AppEvent::Input(Event::Mouse(m)) = event else {
        return None;
    };
    match m.kind {
        MouseEventKind::ScrollDown => Some((*m, NOTCH)),
        MouseEventKind::ScrollUp => Some((*m, -NOTCH)),
        _ => None,
    }
}

/// Rows one notch of the wheel moves.
const NOTCH: i32 = 3;

fn scroll_step(app: &mut App, m: MouseEvent, delta: i32, area: Rect) -> Result<Flow> {
    let events = route_scroll(app, m, delta, area);
    finish(app, events, true)
}

/// Send a scroll to whichever panel the pointer is over.
fn route_scroll(app: &mut App, m: MouseEvent, delta: i32, area: Rect) -> Vec<PanelEvent> {
    // The overlay first, wherever the pointer is. A wheel while the picker is
    // up scrolls the picker — scrolling the file underneath a modal is what
    // every other program treats as a bug.
    if app.picker().is_some() {
        return app.route_picker_scroll(delta, area);
    }
    let (tree_area, editor_area) = app.areas(area);
    if m.column < tree_area.width {
        app.tree_mut().handle_scroll(delta, tree_area)
    } else {
        app.editor_mut().handle_scroll(delta, editor_area)
    }
}

/// Apply what the panels asked for, settle the dirty flag, and answer.
fn finish(app: &mut App, events: Vec<PanelEvent>, mut changed: bool) -> Result<Flow> {
    // A panel that asked for a repaint gets one even if the caller decided
    // otherwise: the panel is the one that knows.
    if !events.is_empty() {
        changed = true;
    }
    app.apply(events)?;

    // Every path that can edit the buffer arrives here — chord, paste, mouse,
    // prompt, and whatever the next one turns out to be. Hooking each call
    // site instead would work until somebody adds a call site, and the symptom
    // would be highlighting that silently stops updating for one kind of edit.
    app.request_parse_if_stale();

    if changed {
        app.mark_dirty();
    }

    Ok(if app.should_quit() {
        Flow::Quit
    } else {
        Flow::Continue
    })
}

/// One turn of the loop, without a terminal.
///
/// `run` owns the screen, so the body lives here where a test can hand it an
/// event and an area and inspect what it did.
pub fn step(app: &mut App, event: AppEvent, area: Rect) -> Result<Flow> {
    let mut events: Vec<PanelEvent> = Vec::new();

    // Default to marking the frame dirty, and be explicit about the few paths
    // that changed nothing. A path that forgets to mark itself is an invisible
    // missing repaint; a path that marks itself needlessly costs one frame
    // nobody sees. Helix draws the same line, returning false for key releases
    // and for escape sequences it did not understand.
    let mut changed = true;

    match event {
        AppEvent::FileChanged(path) => changed = app.handle_external_change(&path)?,
        AppEvent::Parsed(parsed) => changed = app.handle_parsed(parsed),
        AppEvent::Found(found) => changed = app.handle_found(found),
        AppEvent::Lsp(incoming) => changed = app.handle_lsp(incoming),
        AppEvent::Input(input) => match input {
            // Every binding lives in the keymap now, so there is nothing left
            // here to special-case. The dispatcher owns the order.
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                app.handle_chord(KeyChord::from_event(key))?;
            }
            // A release is the other half of a press already handled. Under the
            // kitty keyboard protocol these arrive for every key, so repainting
            // on them doubles the frame count for nothing.
            Event::Key(_) => changed = false,
            Event::Paste(text) => app.handle_paste(text)?,
            Event::Mouse(m) => {
                // Motion with no button held changes nothing and arrives on
                // every cell the pointer crosses. At M0 these were being
                // counted as frames, which quietly flattered both p50 and p99.
                if matches!(m.kind, MouseEventKind::Moved) {
                    changed = false;
                }
                // The overlay is ahead of both panels, because it is drawn
                // over them: a click that lands on the picker must not also
                // reach whatever it is floating above.
                if app.picker().is_some() {
                    if matches!(m.kind, MouseEventKind::Down(_)) {
                        app.clear_transient();
                    }
                    events = app.route_picker_mouse(m, area);
                    return finish(app, events, changed);
                }

                // The tab bar sits inside the editor's columns but above its
                // rect, so without this a click on it is hit-tested against a
                // row the editor does not own.
                //
                // **Ahead of `clear_transient`, unlike every other click.** A
                // close box on a dirty tab arms a confirmation that the next
                // click has to be able to see, and clearing on the way in would
                // erase the very thing that click is answering. The bar retires
                // the status itself for the gestures that are not an answer.
                if app.route_tab_bar_mouse(m, area) {
                    return finish(app, events, changed);
                }

                if matches!(m.kind, MouseEventKind::Down(_)) {
                    app.clear_transient();
                }

                let (tree_area, editor_area) = app.areas(area);
                let in_tree = m.column < tree_area.width;

                match m.kind {
                    MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                        // One notch. A run of them arriving together is folded
                        // by `step_batch` before it gets here.
                        let delta = if matches!(m.kind, MouseEventKind::ScrollDown) {
                            NOTCH
                        } else {
                            -NOTCH
                        };
                        events = route_scroll(app, m, delta, area);
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
            // Defect 10. ratatui's `draw` autoresizes a fullscreen viewport,
            // querying the backend and clearing into the new area, and panels
            // learn their size at render time — so the whole fix is getting to
            // a draw. Harmless until Task 3, and a frozen screen after it.
            Event::Resize(..) => {}
            // An escape sequence nobody claimed, a focus report, anything the
            // terminal invented. Nothing read it, so nothing changed.
            _ => changed = false,
        },
    }

    finish(app, events, changed)
}
