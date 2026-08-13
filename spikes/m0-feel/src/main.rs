use std::io::stdout;

use anyhow::Result;
use crossterm::ExecutableCommand;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use ratatui::DefaultTerminal;

fn main() -> Result<()> {
    // ratatui::init() enables raw mode + alternate screen and installs a panic
    // hook, but it does NOT enable mouse capture. That is on us.
    let mut terminal = ratatui::init();
    stdout().execute(EnableMouseCapture)?;

    let result = run(&mut terminal);

    stdout().execute(DisableMouseCapture)?;
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut last_event = String::from("(none)");

    loop {
        terminal.draw(|frame| {
            let text = format!("m0-feel spike\nlast event: {last_event}\nq to quit");
            frame.render_widget(text.as_str(), frame.area());
        })?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let quit = key.code == KeyCode::Char('q')
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL));
                if quit {
                    return Ok(());
                }
                last_event = format!("{:?} {:?}", key.code, key.modifiers);
            }
            Event::Mouse(m) => {
                last_event = format!("{:?} at ({}, {})", m.kind, m.column, m.row);
            }
            Event::Resize(w, h) => {
                last_event = format!("resize {w}x{h}");
            }
            _ => {}
        }
    }
}
