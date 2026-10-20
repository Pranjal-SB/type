//! The keyboard view (interface §8b 5e): pick a modifier or a prefix, and
//! every key on a drawn keyboard says what it runs under it. Free keys are
//! outlined, keys the terminal owns are taken, keys that need the kitty
//! protocol are marked.
//!
//! The same tab, the same keymap and the same writes as the list: `enter`
//! records over the selected key's binding and `del` unbinds it, through
//! `record.rs`. Its targets join the tab's one geometry.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use typ_core::{KeyChord, NotifyLevel, PanelEvent, RenderContext};

use super::KeysView;
use super::layout::Target;
use crate::capability::{needs_protocol, tty_owned};

/// The strip: what each chip is called, and what it puts before a key.
const STRIP: &[(&str, &str)] = &[
    ("ctrl", "ctrl+"),
    ("ctrl+shift", "ctrl+shift+"),
    ("alt", "alt+"),
    ("ctrl+k \u{25b8}", "ctrl+k "),
    ("f-keys", ""),
];
/// The index of the chip whose keys are the function keys.
const F_KEYS: usize = 4;

const LETTERS: &[&[&str]] = &[
    &["1", "2", "3", "4", "5", "6", "7", "8", "9", "0", "-", "="],
    &["q", "w", "e", "r", "t", "y", "u", "i", "o", "p", "[", "]"],
    &["a", "s", "d", "f", "g", "h", "j", "k", "l", ";", "'"],
    &["z", "x", "c", "v", "b", "n", "m", ",", ".", "/"],
];
const FUNCTION: &[&[&str]] = &[
    &["f1", "f2", "f3", "f4", "f5", "f6"],
    &["f7", "f8", "f9", "f10", "f11", "f12"],
];

/// A key cell's width and height, and how far each row steps in.
const CELL: u16 = 6;
const CELL_HEIGHT: u16 = 2;
const ROW_STEP: u16 = 2;
/// The strip's line and the board's first, below the search row.
const STRIP_LINE: u16 = 2;
const BOARD_LINE: u16 = 4;

const BOARD_HINT: &str = "arrows pick a key \u{b7} tab the modifier \u{b7} enter records";

/// Which chip, and which key on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct Board {
    pub strip: usize,
    pub row: usize,
    pub col: usize,
}

/// What the view's own keys do on the board, read from [`BOARD_KEYS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoardKey {
    Up,
    Down,
    Left,
    Right,
    Next,
    Previous,
    Record,
    Unbind,
    Reset,
    Undo,
    OpenRaw,
    /// Back to the list.
    List,
}

const BOARD_KEYS: &[(&str, BoardKey)] = &[
    ("up", BoardKey::Up),
    ("down", BoardKey::Down),
    ("left", BoardKey::Left),
    ("right", BoardKey::Right),
    ("tab", BoardKey::Next),
    ("backtab", BoardKey::Previous),
    ("shift+backtab", BoardKey::Previous),
    ("enter", BoardKey::Record),
    ("delete", BoardKey::Unbind),
    ("r", BoardKey::Reset),
    ("ctrl+z", BoardKey::Undo),
    ("ctrl+o", BoardKey::OpenRaw),
    ("ctrl+t", BoardKey::List),
    ("esc", BoardKey::List),
];

fn keys(strip: usize) -> &'static [&'static [&'static str]] {
    if strip == F_KEYS { FUNCTION } else { LETTERS }
}

/// The chord a key on the board is under a chip.
fn chord(strip: usize, key: &str) -> String {
    format!("{}{key}", STRIP[strip].1)
}

impl Board {
    /// The same row and column, kept on the board `strip` draws.
    fn clamped(self) -> Board {
        let rows = keys(self.strip);
        let row = self.row.min(rows.len() - 1);
        let col = self.col.min(rows[row].len() - 1);
        Board { row, col, ..self }
    }

    fn chord(self) -> String {
        chord(self.strip, keys(self.strip)[self.row][self.col])
    }
}

impl KeysView {
    /// `ctrl+t` or the toggle: the keyboard, or back to the list.
    pub(super) fn toggle_board(&mut self) -> Vec<PanelEvent> {
        self.board = match self.board {
            Some(_) => None,
            None => Some(Board::default()),
        };
        self.top = 0;
        self.sync_board();
        vec![PanelEvent::NeedsRedraw]
    }

    /// The selected binding follows the selected key, so `del`, `r` and
    /// `enter` act on it as they do on a row.
    fn sync_board(&mut self) {
        let Some(board) = self.board else {
            return;
        };
        let chord = board.chord();
        self.selected = self.keymap.action_for(&chord).map(|a| (Some(chord), a));
    }

    fn set_board(&mut self, board: Board) {
        self.board = Some(board.clamped());
        self.sync_board();
    }

    pub(super) fn board_key(&mut self, chord: &KeyChord) -> Vec<PanelEvent> {
        let Some(board) = self.board else {
            return Vec::new();
        };
        let Some(key) = BOARD_KEYS
            .iter()
            .find(|(name, _)| *name == chord.canonical)
            .map(|(_, key)| *key)
        else {
            return Vec::new();
        };
        let step = |n: usize, by: isize, len: usize| (n as isize + by).clamp(0, len as isize - 1);
        let rows = keys(board.strip);
        let moved = match key {
            BoardKey::Up => Board {
                row: step(board.row, -1, rows.len()) as usize,
                ..board
            },
            BoardKey::Down => Board {
                row: step(board.row, 1, rows.len()) as usize,
                ..board
            },
            BoardKey::Left => Board {
                col: step(board.col, -1, rows[board.row].len()) as usize,
                ..board
            },
            BoardKey::Right => Board {
                col: step(board.col, 1, rows[board.row].len()) as usize,
                ..board
            },
            BoardKey::Next => Board {
                strip: (board.strip + 1) % STRIP.len(),
                ..board
            },
            BoardKey::Previous => Board {
                strip: (board.strip + STRIP.len() - 1) % STRIP.len(),
                ..board
            },
            BoardKey::Record => return self.record_key(),
            BoardKey::Unbind => return self.unbind(),
            BoardKey::Reset => return self.reset(),
            BoardKey::Undo => return self.undo(),
            BoardKey::OpenRaw => return self.open_raw(),
            BoardKey::List => return self.toggle_board(),
        };
        self.set_board(moved);
        vec![PanelEvent::NeedsRedraw]
    }

    /// `enter` on a key: record over its binding. A free key has none to
    /// record over; an action gets a key from its row in the list.
    fn record_key(&mut self) -> Vec<PanelEvent> {
        match (self.selected.clone(), self.board) {
            (Some(row), _) => self.record(row),
            (None, Some(board)) => vec![PanelEvent::Notify {
                level: NotifyLevel::Info,
                message: format!(
                    "{} is free: give it to an action from that action's row in the list.",
                    crate::app::spelled(&board.chord())
                ),
            }],
            (None, None) => Vec::new(),
        }
    }

    /// A click on a chip or a key. A key already selected records, as
    /// `enter` does.
    pub(super) fn board_click(&mut self, target: &Target) -> Vec<PanelEvent> {
        let Some(board) = self.board else {
            return Vec::new();
        };
        match *target {
            Target::Strip(strip) => self.set_board(Board { strip, ..board }),
            Target::BoardKey(row, col) if (row, col) == (board.row, board.col) => {
                return self.record_key();
            }
            Target::BoardKey(row, col) => self.set_board(Board { row, col, ..board }),
            _ => return Vec::new(),
        }
        vec![PanelEvent::NeedsRedraw]
    }

    /// The strip's chips and the board's keys, as click targets.
    pub(super) fn board_targets(area: Rect, board: Board) -> Vec<(Rect, Target)> {
        let mut out = Vec::new();
        let y = area.y + STRIP_LINE;
        let mut x = area.x + 1;
        for (n, (name, _)) in STRIP.iter().enumerate() {
            let width = typ_buffer::display_width(name) as u16 + 2;
            if x + width > area.right() {
                break;
            }
            out.push((Rect::new(x, y, width, 1), Target::Strip(n)));
            x += width + 1;
        }
        // A cell in, three rows stepped in, twelve keys across.
        let cell = CELL
            .min(area.width.saturating_sub(1 + 3 * ROW_STEP) / 12)
            .max(3);
        for (r, row) in keys(board.strip).iter().enumerate() {
            let y = area.y + BOARD_LINE + r as u16 * CELL_HEIGHT;
            let start = area.x + 1 + r as u16 * ROW_STEP;
            for c in 0..row.len() {
                let x = start + c as u16 * cell;
                if x + cell > area.right() || y + CELL_HEIGHT > area.bottom().saturating_sub(1) {
                    break;
                }
                out.push((Rect::new(x, y, cell, CELL_HEIGHT), Target::BoardKey(r, c)));
            }
        }
        out
    }

    /// The search row's hint, the chips, the keys, and the line that says the
    /// selected key in full.
    pub(super) fn draw_board(
        &self,
        area: Rect,
        targets: &[(Rect, Target)],
        buf: &mut Buffer,
        ctx: &RenderContext,
    ) {
        let Some(board) = self.board else {
            return;
        };
        let theme = ctx.theme;
        crate::settings::search_row(
            Rect::new(area.x, area.y, area.width, 1),
            buf,
            ctx,
            false,
            "",
            BOARD_HINT,
        );
        for (rect, target) in targets {
            match *target {
                Target::Strip(n) => {
                    let style = if n == board.strip {
                        Style::default()
                            .fg(theme.border_focused)
                            .bg(theme.chrome_bg)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.tree_file_fg).bg(theme.chrome_bg)
                    };
                    let text = format!(" {} ", STRIP[n].0);
                    buf.set_stringn(rect.x, rect.y, text, rect.width as usize, style);
                }
                Target::BoardKey(r, c) => {
                    let selected = (r, c) == (board.row, board.col);
                    self.draw_key(
                        *rect,
                        buf,
                        board.strip,
                        keys(board.strip)[r][c],
                        selected,
                        ctx,
                    );
                }
                _ => {}
            }
        }
        let rows = keys(board.strip).len() as u16;
        let y = area.y + BOARD_LINE + rows * CELL_HEIGHT + 1;
        if y < area.bottom().saturating_sub(1) {
            let chord = board.chord();
            let what = match self.keymap.action_for(&chord) {
                Some(action) => action.description(),
                None => "free",
            };
            let text = format!(
                "{}  {what}  \u{b7}  {}",
                crate::app::spelled(&chord),
                Self::capability(&chord)
            );
            let style = Style::default().fg(theme.fg).bg(theme.bg);
            buf.set_stringn(
                area.x + 1,
                y,
                text,
                area.width.saturating_sub(2) as usize,
                style,
            );
        }
    }

    /// One key: taken keys on the raised ground with what they run under the
    /// label, free keys outlined, a `k` in the corner of a kitty-only one.
    fn draw_key(
        &self,
        rect: Rect,
        buf: &mut Buffer,
        strip: usize,
        key: &str,
        selected: bool,
        ctx: &RenderContext,
    ) {
        let theme = ctx.theme;
        let chord = chord(strip, key);
        let action = self.keymap.action_for(&chord);
        let tty = tty_owned(&chord);
        let room = rect.width.saturating_sub(1) as usize;
        let ground = if selected {
            theme.selection_primary_bg
        } else if action.is_some() || tty {
            theme.chrome_bg
        } else {
            theme.bg
        };
        let cell = Rect::new(rect.x, rect.y, rect.width.saturating_sub(1), rect.height);
        buf.set_style(cell, Style::default().fg(theme.fg).bg(ground));
        let quiet = Style::default().fg(theme.line_number_fg).bg(ground);
        match (action, tty) {
            (_, true) => {
                buf.set_stringn(rect.x + 1, rect.y, key, room, quiet);
                buf.set_stringn(rect.x, rect.y + 1, "tty", room, quiet);
            }
            (Some(action), false) => {
                let label = Style::default()
                    .fg(theme.fg)
                    .bg(ground)
                    .add_modifier(Modifier::BOLD);
                buf.set_stringn(rect.x + 1, rect.y, key, room, label);
                buf.set_stringn(rect.x, rect.y + 1, action.name(), room, quiet);
            }
            (None, false) => {
                let outline = if key.chars().count() == 1 {
                    format!("[ {key} ]")
                } else {
                    format!("[{key}]")
                };
                let style = Style::default().fg(theme.border).bg(ground);
                buf.set_stringn(rect.x, rect.y, outline, room, style);
            }
        }
        if needs_protocol(&chord) && room > 0 {
            let mark = Style::default().fg(theme.diagnostic_warning).bg(ground);
            buf.set_stringn(rect.x + room as u16 - 1, rect.y, "k", 1, mark);
        }
    }
}
