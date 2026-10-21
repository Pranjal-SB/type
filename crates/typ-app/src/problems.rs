//! The problems panel (interface §8, workspace): every diagnostic of every
//! open document as a multibuffer of excerpts, grouped by file.
//!
//! The bottom dock's first panel. It registers as a view (`PROBLEMS`, M4.4's
//! `register_view`) and the dock builds it through the registry. It never
//! sees the app: the app hands it the list (`set`) and it hands back
//! `OpenFile` for the problem chosen, which is how a panel jumps.

use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use typ_buffer::Position;
use typ_core::{HandlerId, KeyChord, Panel, PanelEvent, RenderContext, Severity};

/// The view's handler.
pub const PROBLEMS: HandlerId = HandlerId("problems");

/// What the registry builds for `PROBLEMS`. The path says nothing: there is
/// one list, of every open document.
pub fn open(_: &Path) -> Box<dyn Panel> {
    Box::new(ProblemsPanel::default())
}

/// One diagnostic and the lines around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub path: PathBuf,
    /// What the file's heading says.
    pub name: String,
    pub severity: Severity,
    pub message: String,
    pub at: Position,
    /// Its line and one either side, as `(line, text)`: real lines of the
    /// buffer.
    pub excerpt: Vec<(usize, String)>,
}

/// The list, the problem chosen and the first row on show.
#[derive(Default)]
pub struct ProblemsPanel {
    problems: Vec<Problem>,
    selected: usize,
    top: usize,
}

/// One row of the multibuffer.
enum Row {
    /// A file's heading, before its first problem.
    File(usize),
    /// A problem's message.
    Message(usize),
    /// A line of a problem's excerpt: the problem and the line's place in it.
    Line(usize, usize),
}

impl Row {
    fn problem(&self) -> Option<usize> {
        match self {
            Row::File(_) => None,
            Row::Message(i) | Row::Line(i, _) => Some(*i),
        }
    }
}

impl ProblemsPanel {
    /// Replace the list, keeping the choice in range.
    pub fn set(&mut self, problems: Vec<Problem>) {
        if self.problems != problems {
            self.problems = problems;
            self.selected = self.selected.min(self.problems.len().saturating_sub(1));
        }
    }

    pub fn len(&self) -> usize {
        self.problems.len()
    }

    pub fn is_empty(&self) -> bool {
        self.problems.is_empty()
    }

    fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for (i, problem) in self.problems.iter().enumerate() {
            if i == 0 || self.problems[i - 1].path != problem.path {
                rows.push(Row::File(i));
            }
            rows.push(Row::Message(i));
            rows.extend((0..problem.excerpt.len()).map(|k| Row::Line(i, k)));
        }
        rows
    }

    /// Every problem on show and the rows it takes in `area`, its message
    /// first. Render, the click and `App::targets` all ask this.
    pub fn blocks(&self, area: Rect) -> Vec<(usize, Rect)> {
        let mut out: Vec<(usize, Rect)> = Vec::new();
        for (y, row) in (area.y..area.bottom()).zip(self.rows().iter().skip(self.top)) {
            let Some(i) = row.problem() else {
                continue;
            };
            match out.last_mut() {
                Some((last, rect)) if *last == i => rect.height += 1,
                _ => out.push((i, Rect::new(area.x, y, area.width, 1))),
            }
        }
        out
    }

    /// Scroll so the chosen problem's message row is on show.
    fn reveal(&mut self, height: usize) {
        let Some(at) = self
            .rows()
            .iter()
            .position(|row| matches!(row, Row::Message(i) if *i == self.selected))
        else {
            return;
        };
        // Its file's heading too, when it is the file's first problem.
        let from = at.saturating_sub(1);
        if from < self.top {
            self.top = from;
        } else if at >= self.top + height {
            self.top = at + 1 - height.max(1);
        }
    }

    fn jump(&self, i: usize) -> Vec<PanelEvent> {
        self.problems
            .get(i)
            .map(|problem| PanelEvent::OpenFile {
                path: problem.path.clone(),
                at: problem.at,
            })
            .into_iter()
            .collect()
    }
}

impl Panel for ProblemsPanel {
    fn name(&self) -> &'static str {
        "problems"
    }

    fn title(&self) -> String {
        match self.problems.len() {
            0 => "problems".to_string(),
            n => format!("problems {n}"),
        }
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &RenderContext) {
        let theme = ctx.theme;
        let ground = Style::default().fg(theme.fg).bg(theme.bg);
        buf.set_style(area, ground);
        if self.problems.is_empty() {
            let quiet = ground.fg(theme.receded_fg);
            buf.set_stringn(
                area.x + 1,
                area.y,
                "No problems.",
                usize::from(area.width),
                quiet,
            );
            return;
        }
        self.reveal(usize::from(area.height));
        let width = self
            .problems
            .iter()
            .flat_map(|p| p.excerpt.iter().map(|(line, _)| line + 1))
            .max()
            .unwrap_or(1)
            .to_string()
            .len();
        let room = usize::from(area.width.saturating_sub(1));
        let rows = self.rows();
        for (y, row) in (area.y..area.bottom()).zip(rows.iter().skip(self.top)) {
            match *row {
                Row::File(i) => {
                    let style = ground.add_modifier(Modifier::BOLD);
                    buf.set_stringn(area.x + 1, y, &self.problems[i].name, room, style);
                }
                Row::Message(i) => {
                    let problem = &self.problems[i];
                    let row_style = match i == self.selected && ctx.is_focused {
                        true => ground.bg(theme.selection_primary_bg),
                        false => ground,
                    };
                    buf.set_style(Rect::new(area.x, y, area.width, 1), row_style);
                    let mark = row_style.fg(severity_colour(theme, problem.severity));
                    buf.set_stringn(area.x + 2, y, "\u{25cf}", 1, mark);
                    let text = format!(
                        "{}  {}:{}",
                        problem.message,
                        problem.at.line + 1,
                        problem.at.col + 1
                    );
                    buf.set_stringn(area.x + 4, y, text, room.saturating_sub(3), row_style);
                }
                Row::Line(i, k) => {
                    let problem = &self.problems[i];
                    let (line, text) = &problem.excerpt[k];
                    let own = *line == problem.at.line;
                    let style = match own {
                        true => ground,
                        false => ground.fg(theme.receded_fg),
                    };
                    let number = format!("{:>width$} \u{2502} ", line + 1);
                    let quiet = ground.fg(theme.line_number_fg);
                    let (x, _) =
                        buf.set_stringn(area.x + 4, y, &number, room.saturating_sub(3), quiet);
                    let left = usize::from(area.right().saturating_sub(x));
                    buf.set_stringn(x, y, text.replace('\t', "    "), left, style);
                }
            }
        }
    }

    /// `up` and `down` choose, `enter` jumps.
    fn handle_key(&mut self, chord: KeyChord) -> Vec<PanelEvent> {
        let last = self.problems.len().saturating_sub(1);
        match chord.raw.code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => self.selected = (self.selected + 1).min(last),
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = last,
            KeyCode::Enter => return self.jump(self.selected),
            _ => return Vec::new(),
        }
        vec![PanelEvent::NeedsRedraw]
    }

    /// A press on a problem's rows chooses it and jumps.
    fn handle_mouse(&mut self, event: MouseEvent, panel_area: Rect) -> Vec<PanelEvent> {
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return Vec::new();
        }
        let hit = self
            .blocks(panel_area)
            .into_iter()
            .find(|(_, rect)| rect.contains((event.column, event.row).into()));
        match hit {
            Some((i, _)) => {
                self.selected = i;
                self.jump(i)
            }
            None => Vec::new(),
        }
    }

    fn handle_scroll(&mut self, delta: i32, panel_area: Rect) -> Vec<PanelEvent> {
        let rows = self.rows().len();
        let last = rows.saturating_sub(usize::from(panel_area.height));
        self.top = self.top.saturating_add_signed(delta as isize).min(last);
        vec![PanelEvent::NeedsRedraw]
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

fn severity_colour(theme: &typ_core::ThemeColors, severity: Severity) -> ratatui::style::Color {
    match severity {
        Severity::Error => theme.diagnostic_error,
        Severity::Warning => theme.diagnostic_warning,
        Severity::Information => theme.diagnostic_info,
        Severity::Hint => theme.diagnostic_hint,
    }
}
