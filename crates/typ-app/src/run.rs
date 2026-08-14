use std::io::{Write, stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::ExecutableCommand;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseEventKind,
};
use ratatui::layout::Rect;
use typ_core::{KeyChord, Panel, PanelEvent};

use crate::app::{App, Focus};

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

    let result = event_loop(&mut terminal, &mut app);

    stdout().execute(DisableMouseCapture)?;
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        begin_frame();
        terminal.draw(|frame| app.render(frame))?;
        end_frame();

        if app.should_quit() {
            return Ok(());
        }

        let mut events: Vec<PanelEvent> = Vec::new();

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                // Application bindings win before panel dispatch.
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Char('q') if ctrl => events.push(PanelEvent::Quit),
                    KeyCode::Char('s') if ctrl => {
                        if app.focus() == Focus::Editor {
                            app.editor_mut().save()?;
                        }
                    }
                    KeyCode::Tab => app.cycle_focus(),
                    _ => {
                        events = app.focused_mut().handle_key(KeyChord::from_event(key));
                    }
                }
            }
            Event::Mouse(m) => {
                let size = terminal.size()?;
                let full = Rect::new(0, 0, size.width, size.height);
                let (tree_area, editor_area) = app.areas(full);
                let in_tree = m.column < tree_area.width;

                match m.kind {
                    // Coalesce wheel events into a single scroll call so a fast
                    // wheel does not queue one repaint per notch.
                    MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                        let mut delta: i32 = if matches!(m.kind, MouseEventKind::ScrollDown) {
                            3
                        } else {
                            -3
                        };
                        while event::poll(Duration::from_millis(0))? {
                            match event::read()? {
                                Event::Mouse(next) => match next.kind {
                                    MouseEventKind::ScrollDown => delta += 3,
                                    MouseEventKind::ScrollUp => delta -= 3,
                                    _ => break,
                                },
                                _ => break,
                            }
                        }
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
        }

        app.apply(events)?;
    }
}
