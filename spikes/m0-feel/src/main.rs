use std::io::{Write, stdout};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::ExecutableCommand;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseButton, MouseEventKind,
};
use m0_feel::click::click_to_position;
use m0_feel::highlight::Highlighter;
use m0_feel::metrics::FrameTimer;
use m0_feel::viewport::Viewport;
use m0_feel::width::grapheme_to_display_col;
use ratatui::DefaultTerminal;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
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

fn style_for(kind: &str) -> Style {
    let c = match kind {
        "keyword" => Color::Magenta,
        "string" => Color::Green,
        "number" => Color::Yellow,
        "comment" => Color::DarkGray,
        "identifier" => Color::Cyan,
        _ => Color::Reset,
    };
    Style::default().fg(c)
}

fn styled_line(text: &str, spans: &[(std::ops::Range<usize>, &'static str)]) -> Line<'static> {
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut pos = 0usize;
    for (range, kind) in spans {
        if range.start > pos {
            out.push(Span::raw(text[pos..range.start].to_string()));
        }
        out.push(Span::styled(text[range.clone()].to_string(), style_for(kind)));
        pos = range.end;
    }
    if pos < text.len() {
        out.push(Span::raw(text[pos..].to_string()));
    }
    Line::from(out)
}

/// What a run reports once the terminal is back to normal.
struct RunStats {
    timer: FrameTimer,
    /// How long the worker took to parse, once its result arrived.
    parse: Option<Duration>,
    /// Time from process start to the first painted frame.
    first_frame: Duration,
}

fn main() -> Result<()> {
    let boot = Instant::now();
    let path = std::env::args().nth(1).context("usage: m0-feel <file>")?;
    let text: Arc<str> =
        Arc::from(std::fs::read_to_string(&path).with_context(|| format!("reading {path}"))?);
    let rope = Rope::from_str(&text);

    // Parsing 2.2MB of Rust costs ~750ms, and it is linear in file size — there
    // is no constant factor to win. Doing it before the first frame means the
    // editor is visibly frozen for that long on open, so it goes to a worker
    // and comes back as a message. Lines render unhighlighted until it lands.
    //
    // Highlighting is Rust-only in the spike; other extensions never parse.
    let (tx, rx) = mpsc::channel();
    if path.ends_with(".rs") {
        let src = Arc::clone(&text);
        thread::spawn(move || {
            let mut h = match Highlighter::new_rust() {
                Ok(h) => h,
                // The spike has no error channel and a missing grammar is not
                // what it is measuring; plain rendering is a fine outcome.
                Err(_) => return,
            };
            let t0 = Instant::now();
            h.parse(&src);
            let _ = tx.send((h, t0.elapsed()));
        });
    }

    let mut terminal = ratatui::init();
    stdout().execute(EnableMouseCapture)?;

    let result = run(&mut terminal, &rope, &rx, boot);

    stdout().execute(DisableMouseCapture)?;
    ratatui::restore();

    let stats = result?;
    match stats.parse {
        Some(d) => println!("initial parse: {}ms (off-thread)", d.as_millis()),
        None => println!("initial parse: (never arrived / not a .rs file)"),
    }
    println!("first frame at: {}ms after start", stats.first_frame.as_millis());
    println!("frame timing: {}", stats.timer.report());
    Ok(())
}

fn run(
    terminal: &mut DefaultTerminal,
    rope: &Rope,
    parsed: &Receiver<(Highlighter, Duration)>,
    boot: Instant,
) -> Result<RunStats> {
    let total = rope.len_lines();
    let mut vp = Viewport { top_line: 0, height: 0 };
    let mut cursor: (usize, usize) = (0, 0);
    let mut timer = FrameTimer::new();
    let mut sync_output = true;
    let mut highlight_on = true;
    let mut highlighter: Option<Highlighter> = None;
    let mut parse: Option<Duration> = None;
    let mut first_frame: Option<Duration> = None;
    // Only painted frames are timed. Redrawing on every event — mouse moves
    // included — would stuff the histogram with no-op frames and flatter the
    // percentiles.
    let mut dirty = true;

    loop {
        if dirty {
            let frame_start = Instant::now();
            if sync_output {
                sync(SYNC_BEGIN);
            }

            terminal.draw(|frame| {
                let [area, status_area] = Layout::vertical([
                    Constraint::Min(1),
                    Constraint::Length(1),
                ])
                .areas(frame.area());

                vp.height = area.height as usize;
                let range = vp.visible_range(total);
                let hl = highlighter.as_ref().filter(|_| highlight_on);

                // One tree walk for the whole viewport, then split the (sorted)
                // spans across lines as we go. Walking per line was
                // O(lines_above) twice over and made deep scrolling unusable.
                let mut byte = rope.line_to_byte(range.start);
                let spans = match hl {
                    Some(h) => h.spans_in_range(byte, rope.line_to_byte(range.end)),
                    None => Vec::new(),
                };
                let mut next_span = 0usize;

                let mut lines: Vec<Line> = Vec::with_capacity(vp.height);
                for l in rope.lines_at(range.start).take(vp.height) {
                    let s = l.to_string();
                    let line_end = byte + s.len();
                    let s = s.trim_end_matches('\n');

                    let mut local = Vec::new();
                    while let Some((r, kind)) = spans.get(next_span) {
                        if r.start >= line_end {
                            break;
                        }
                        let a = r.start.saturating_sub(byte);
                        let b = (r.end.min(line_end) - byte).min(s.len());
                        if a < b {
                            local.push((a..b, *kind));
                        }
                        // A span crossing the line break (block comment, raw
                        // string) belongs to the next line too.
                        if r.end > line_end {
                            break;
                        }
                        next_span += 1;
                    }

                    lines.push(match hl {
                        Some(_) => styled_line(s, &local),
                        None => Line::raw(s.to_string()),
                    });
                    byte = line_end;
                }
                frame.render_widget(Paragraph::new(lines), area);

                // "wait" is the whole point of this design being visible: the
                // file is open and scrollable while the parse is still running.
                let hl_state = match (highlight_on, highlighter.is_some()) {
                    (false, _) => "off ",
                    (true, true) => "on  ",
                    (true, false) => "wait",
                };
                let status = format!(
                    " sync:{}  hl:{}  line {}/{}  cursor {}:{}  [s] sync  [h] highlight  [q] quit ",
                    if sync_output { "on " } else { "off" },
                    hl_state,
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
                    let display_col =
                        grapheme_to_display_col(text.trim_end_matches('\n'), cursor.1, 4);
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
            first_frame.get_or_insert_with(|| boot.elapsed());
            dirty = false;
        }

        // Poll rather than block: the parse can finish at any moment and must
        // not wait for the user's next keypress to appear on screen.
        //
        // ponytail: this wakes 60x/sec while idle doing nothing. Fine for a
        // spike that runs for 30 seconds. M1 blocks on a single event channel
        // instead, with a thread pumping crossterm events into it, so a
        // finished parse wakes the loop directly and idle costs nothing.
        if event::poll(Duration::from_millis(16))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    let quit = key.code == KeyCode::Char('q')
                        || (key.code == KeyCode::Char('c')
                            && key.modifiers.contains(KeyModifiers::CONTROL));
                    if quit {
                        return Ok(RunStats {
                            timer,
                            parse,
                            first_frame: first_frame.unwrap_or_default(),
                        });
                    }
                    match key.code {
                        KeyCode::Down => vp.scroll(1, total),
                        KeyCode::Up => vp.scroll(-1, total),
                        KeyCode::PageDown => vp.scroll(vp.height as i32, total),
                        KeyCode::PageUp => vp.scroll(-(vp.height as i32), total),
                        KeyCode::Char('s') => sync_output = !sync_output,
                        KeyCode::Char('h') => highlight_on = !highlight_on,
                        _ => {}
                    }
                    dirty = true;
                }
                Event::Mouse(m) => match m.kind {
                    MouseEventKind::ScrollDown => {
                        vp.scroll(3, total);
                        dirty = true;
                    }
                    MouseEventKind::ScrollUp => {
                        vp.scroll(-3, total);
                        dirty = true;
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        cursor = click_to_position(rope, vp, m.column, m.row, 4);
                        dirty = true;
                    }
                    // Moves and drags change nothing on screen. Redrawing on
                    // them is what was padding the frame histogram.
                    _ => {}
                },
                Event::Resize(_, _) => dirty = true,
                _ => {}
            }
        }

        if highlighter.is_none()
            && let Ok((h, elapsed)) = parsed.try_recv()
        {
            highlighter = Some(h);
            parse = Some(elapsed);
            dirty = true;
        }
    }
}
