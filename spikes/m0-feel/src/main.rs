use std::io::{Write, stdout};
use std::time::Instant;

use anyhow::{Context, Result};
use crossterm::ExecutableCommand;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseButton, MouseEventKind,
};
use m0_feel::click::click_to_position;
use m0_feel::metrics::FrameTimer;
use m0_feel::viewport::Viewport;
use m0_feel::width::grapheme_to_display_col;
use ratatui::DefaultTerminal;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ropey::Rope;

/// CSI ?2026h / l — synchronized output. Tells the terminal to buffer the
/// frame and present it atomically, which removes tearing on partial repaints.
const SYNC_BEGIN: &[u8] = b"\x1b[?2026h";
const SYNC_END: &[u8] = b"\x1b[?2026l";

fn sync(seq: &[u8]) {
    let mut out = stdout();
    let _ = out.write_all(seq);
    let _ = out.flush();
}

fn main() -> Result<()> {
    let path = std::env::args().nth(1).context("usage: m0-feel <file>")?;
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {path}"))?;
    let rope = Rope::from_str(&text);

    let mut terminal = ratatui::init();
    stdout().execute(EnableMouseCapture)?;

    let result = run(&mut terminal, &rope);

    stdout().execute(DisableMouseCapture)?;
    ratatui::restore();

    let timer = result?;
    println!("frame timing: {}", timer.report());
    Ok(())
}

fn run(terminal: &mut DefaultTerminal, rope: &Rope) -> Result<FrameTimer> {
    let total = rope.len_lines();
    let mut vp = Viewport { top_line: 0, height: 0 };
    let mut cursor: (usize, usize) = (0, 0);
    let mut timer = FrameTimer::new();
    let mut sync_output = true;

    loop {
        let frame_start = Instant::now();
        if sync_output {
            sync(SYNC_BEGIN);
        }

        terminal.draw(|frame| {
            let [area, status_area] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(frame.area());

            vp.height = area.height as usize;
            let lines: Vec<Line> = rope
                .lines_at(vp.visible_range(total).start)
                .take(vp.height)
                .map(|l| Line::raw(l.to_string().trim_end_matches('\n').to_string()))
                .collect();
            frame.render_widget(Paragraph::new(lines), area);

            let status = format!(
                " sync:{}  line {}/{}  cursor {}:{}  [s] sync  [q] quit ",
                if sync_output { "on " } else { "off" },
                vp.top_line + 1,
                total,
                cursor.0 + 1,
                cursor.1,
            );
            frame.render_widget(
                Paragraph::new(status).style(Style::new().fg(Color::Black).bg(Color::Gray)),
                status_area,
            );

            if cursor.0 >= vp.top_line && cursor.0 < vp.top_line + vp.height {
                let text = rope.line(cursor.0).to_string();
                let display_col = grapheme_to_display_col(text.trim_end_matches('\n'), cursor.1, 4);
                frame.set_cursor_position((
                    area.x + display_col as u16,
                    area.y + (cursor.0 - vp.top_line) as u16,
                ));
            }
        })?;

        if sync_output {
            sync(SYNC_END);
        }
        timer.record(frame_start.elapsed());

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let quit = key.code == KeyCode::Char('q')
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL));
                if quit {
                    return Ok(timer);
                }
                match key.code {
                    KeyCode::Down => vp.scroll(1, total),
                    KeyCode::Up => vp.scroll(-1, total),
                    KeyCode::PageDown => vp.scroll(vp.height as i32, total),
                    KeyCode::PageUp => vp.scroll(-(vp.height as i32), total),
                    KeyCode::Char('s') => sync_output = !sync_output,
                    _ => {}
                }
            }
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollDown => vp.scroll(3, total),
                MouseEventKind::ScrollUp => vp.scroll(-3, total),
                MouseEventKind::Down(MouseButton::Left) => {
                    cursor = click_to_position(rope, vp, m.column, m.row, 4);
                }
                _ => {}
            },
            _ => {}
        }
    }
}
