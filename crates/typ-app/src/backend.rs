//! The terminal, written to directly.
//!
//! **Why this exists rather than `CrosstermBackend`.** ratatui owns the
//! double-buffer diff: a cell it did not write is a cell the next frame will
//! not repair, so an escape sequence emitted after `terminal.draw()` is a cell
//! ratatui believes is clean and will never redraw. The undercurl a diagnostic
//! needs has to come out of the backend or not at all — `Modifier` has no curl
//! bit, so `CrosstermBackend` has nothing to map from.
//!
//! It absorbs the frame boundary on the way past. `run.rs` wrote
//! synchronized-output sequences around every `terminal.draw()` by hand; they
//! belong here, where the frame actually begins and ends, and that is a
//! deletion rather than a feature.
//!
//! **Two sequences are written by hand rather than through crossterm**, and
//! both for the same reason — crossterm writes the legacy semicolon form and at
//! least one terminal in wide use accepts only the colon form. See
//! [`Underlines`] and [`underline_colour`].

use std::io::{self, Write};

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::style::{
    Attribute as CtAttribute, Color as CtColor, Colors as CtColors, Print, SetAttribute, SetColors,
};
use crossterm::terminal::{self, Clear};
use crossterm::{execute, queue};
use ratatui::backend::{Backend, ClearType, WindowSize};
use ratatui::buffer::Cell;
use ratatui::layout::{Position, Size};
use ratatui::style::{Color, Modifier};

/// Whether this terminal can draw a shape other than a straight line.
///
/// **There is no reliable query for it**, and this is the honest floor rather
/// than a good answer. Terminfo's `Smulx` is the right signal and TYPE has no
/// terminfo reader; adding one for a single capability whose failure mode is a
/// straight underline is not a trade worth making. So: an allowlist, and a
/// fallback that still says "look here".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Underlines {
    /// `CSI 4:3 m`. Kitty's extension, since adopted by VTE, WezTerm, foot,
    /// Ghostty, iTerm2, Konsole, Alacritty, contour, mintty, Windows Terminal
    /// and xterm.js, and passed through by tmux 2.9+ and Zellij.
    Styled,
    /// `CSI 4 m`. xterm, PuTTY, rxvt-unicode, st and GNU screen. The colour
    /// still lands — `CSI 58` is older and better supported than the shape —
    /// so a diagnostic on one of these is a coloured straight underline.
    Plain,
}

/// A backend over any writer, with an undercurl.
pub struct TypBackend<W: Write> {
    writer: W,
    underlines: Underlines,
    /// Whether a frame is open, so the closing sequence is never written
    /// unpaired. `Terminal::draw` calls `draw` then `flush`, but nothing in the
    /// trait promises it.
    in_frame: bool,
}

impl<W: Write> TypBackend<W> {
    pub fn new(writer: W, underlines: Underlines) -> Self {
        TypBackend {
            writer,
            underlines,
            in_frame: false,
        }
    }

    /// What this backend decided the terminal can draw.
    pub fn underlines(&self) -> Underlines {
        self.underlines
    }
}

/// Underline colour, in the colon form.
///
/// **Not `CSI 58 ; 2 ; r ; g ; b m`.** Windows Terminal's conpty accepts only
/// `CSI 58 : 2 : : r : g : b m` and mangles the semicolon form
/// (microsoft/terminal#17426, open since 2024), which is exactly what
/// crossterm's `SetUnderlineColor` writes. The colon form is what kitty
/// documents and what the `Setulc` terminfo string vim and neovim rely on
/// emits, so it is the compatible spelling as well as the required one.
fn underline_colour(w: &mut impl Write, colour: Color) -> io::Result<()> {
    match colour {
        Color::Reset => write!(w, "\x1b[59m"),
        Color::Rgb(r, g, b) => write!(w, "\x1b[58:2::{r}:{g}:{b}m"),
        Color::Indexed(i) => write!(w, "\x1b[58:5:{i}m"),
        // The sixteen. Written as palette indices, which is what they are.
        other => {
            let index = ansi_index(other);
            write!(w, "\x1b[58:5:{index}m")
        }
    }
}

/// A named ANSI colour as its palette index.
fn ansi_index(colour: Color) -> u8 {
    match colour {
        Color::Black => 0,
        Color::Red => 1,
        Color::Green => 2,
        Color::Yellow => 3,
        Color::Blue => 4,
        Color::Magenta => 5,
        Color::Cyan => 6,
        Color::Gray => 7,
        Color::DarkGray => 8,
        Color::LightRed => 9,
        Color::LightGreen => 10,
        Color::LightYellow => 11,
        Color::LightBlue => 12,
        Color::LightMagenta => 13,
        Color::LightCyan => 14,
        Color::White => 15,
        Color::Indexed(i) => i,
        // `Reset` and `Rgb` are handled by the caller; nothing else exists.
        _ => 7,
    }
}

/// Emit the attributes that changed between two cells.
///
/// Removals first, then additions, because the sequence that clears bold also
/// clears dim and the one that clears any underline clears the curl — so an
/// attribute that survives the change has to be re-stated after the clear that
/// took it away.
fn modifier_diff(
    w: &mut impl Write,
    from: Modifier,
    to: Modifier,
    underlines: Underlines,
) -> io::Result<()> {
    let removed = from - to;
    let added = to - from;

    if removed.contains(Modifier::REVERSED) {
        queue!(w, SetAttribute(CtAttribute::NoReverse))?;
    }
    // One sequence clears both intensities, so losing either means restating
    // whichever is left.
    let intensity_reset = removed.intersects(Modifier::BOLD | Modifier::DIM);
    if intensity_reset {
        queue!(w, SetAttribute(CtAttribute::NormalIntensity))?;
    }
    if removed.contains(Modifier::ITALIC) {
        queue!(w, SetAttribute(CtAttribute::NoItalic))?;
    }
    if removed.contains(Modifier::CROSSED_OUT) {
        queue!(w, SetAttribute(CtAttribute::NotCrossedOut))?;
    }
    if removed.contains(Modifier::HIDDEN) {
        queue!(w, SetAttribute(CtAttribute::NoHidden))?;
    }
    if removed.intersects(Modifier::SLOW_BLINK | Modifier::RAPID_BLINK) {
        queue!(w, SetAttribute(CtAttribute::NoBlink))?;
    }
    // `CSI 4:0 m` turns off every underline style, the curl included. The plain
    // `CSI 24 m` does too, but on a terminal that understands the styled form
    // it is better to speak one language for the whole attribute.
    let underline_lost = removed.intersects(Modifier::UNDERLINED | typ_core::UNDERCURL);
    if underline_lost {
        match underlines {
            Underlines::Styled => write!(w, "\x1b[4:0m")?,
            Underlines::Plain => queue!(w, SetAttribute(CtAttribute::NoUnderline))?,
        }
    }

    if added.contains(Modifier::REVERSED) {
        queue!(w, SetAttribute(CtAttribute::Reverse))?;
    }
    if to.contains(Modifier::BOLD) && (added.contains(Modifier::BOLD) || intensity_reset) {
        queue!(w, SetAttribute(CtAttribute::Bold))?;
    }
    if to.contains(Modifier::DIM) && (added.contains(Modifier::DIM) || intensity_reset) {
        queue!(w, SetAttribute(CtAttribute::Dim))?;
    }
    if added.contains(Modifier::ITALIC) {
        queue!(w, SetAttribute(CtAttribute::Italic))?;
    }
    if added.contains(Modifier::SLOW_BLINK) {
        queue!(w, SetAttribute(CtAttribute::SlowBlink))?;
    }
    if added.contains(Modifier::RAPID_BLINK) {
        queue!(w, SetAttribute(CtAttribute::RapidBlink))?;
    }
    if added.contains(Modifier::HIDDEN) {
        queue!(w, SetAttribute(CtAttribute::Hidden))?;
    }
    if added.contains(Modifier::CROSSED_OUT) {
        queue!(w, SetAttribute(CtAttribute::CrossedOut))?;
    }

    // Restated whenever the clear above ran, even if the bit did not change.
    let restate = underline_lost;
    if to.contains(typ_core::UNDERCURL) && (added.contains(typ_core::UNDERCURL) || restate) {
        match underlines {
            Underlines::Styled => write!(w, "\x1b[4:3m")?,
            Underlines::Plain => queue!(w, SetAttribute(CtAttribute::Underlined))?,
        }
    } else if to.contains(Modifier::UNDERLINED) && (added.contains(Modifier::UNDERLINED) || restate)
    {
        queue!(w, SetAttribute(CtAttribute::Underlined))?;
    }
    Ok(())
}

impl<W: Write> Backend for TypBackend<W> {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> io::Result<()>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        if !self.in_frame {
            // CSI ?2026h. A terminal that does not know it ignores it, and one
            // that does presents the whole frame at once instead of tearing.
            write!(self.writer, "\x1b[?2026h")?;
            self.in_frame = true;
        }

        let mut fg = Color::Reset;
        let mut bg = Color::Reset;
        let mut underline = Color::Reset;
        let mut modifier = Modifier::empty();
        let mut last: Option<Position> = None;

        for (x, y, cell) in content {
            // Only when the run breaks. Re-addressing every cell is most of a
            // frame's bytes on a screen that is mostly text.
            if !matches!(last, Some(p) if y == p.y && x == p.x + 1) {
                queue!(self.writer, MoveTo(x, y))?;
            }
            last = Some(Position { x, y });

            if cell.modifier != modifier {
                modifier_diff(&mut self.writer, modifier, cell.modifier, self.underlines)?;
                modifier = cell.modifier;
            }
            if cell.fg != fg || cell.bg != bg {
                queue!(
                    self.writer,
                    SetColors(CtColors::new(
                        into_crossterm(cell.fg),
                        into_crossterm(cell.bg)
                    ))
                )?;
                fg = cell.fg;
                bg = cell.bg;
            }
            if cell.underline_color != underline {
                underline_colour(&mut self.writer, cell.underline_color)?;
                underline = cell.underline_color;
            }

            queue!(self.writer, Print(cell.symbol()))?;
        }

        // Everything off. A style left set leaks into whatever prints next,
        // which after a quit is the user's shell prompt.
        write!(self.writer, "\x1b[59m")?;
        queue!(self.writer, SetAttribute(CtAttribute::Reset))
    }

    fn hide_cursor(&mut self) -> io::Result<()> {
        execute!(self.writer, Hide)
    }

    fn show_cursor(&mut self) -> io::Result<()> {
        execute!(self.writer, Show)
    }

    fn get_cursor_position(&mut self) -> io::Result<Position> {
        crossterm::cursor::position()
            .map(|(x, y)| Position { x, y })
            .map_err(io::Error::other)
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> io::Result<()> {
        let Position { x, y } = position.into();
        execute!(self.writer, MoveTo(x, y))
    }

    fn clear(&mut self) -> io::Result<()> {
        self.clear_region(ClearType::All)
    }

    fn clear_region(&mut self, clear_type: ClearType) -> io::Result<()> {
        execute!(
            self.writer,
            Clear(match clear_type {
                ClearType::All => terminal::ClearType::All,
                ClearType::AfterCursor => terminal::ClearType::FromCursorDown,
                ClearType::BeforeCursor => terminal::ClearType::FromCursorUp,
                ClearType::CurrentLine => terminal::ClearType::CurrentLine,
                ClearType::UntilNewLine => terminal::ClearType::UntilNewLine,
            })
        )
    }

    fn append_lines(&mut self, n: u16) -> io::Result<()> {
        for _ in 0..n {
            queue!(self.writer, Print("\n"))?;
        }
        self.writer.flush()
    }

    fn size(&self) -> io::Result<Size> {
        let (width, height) = terminal::size()?;
        Ok(Size { width, height })
    }

    fn window_size(&mut self) -> io::Result<WindowSize> {
        let terminal::WindowSize {
            columns,
            rows,
            width,
            height,
        } = terminal::window_size()?;
        Ok(WindowSize {
            columns_rows: Size {
                width: columns,
                height: rows,
            },
            pixels: Size { width, height },
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.in_frame {
            // CSI ?2026l, and only if a frame was actually opened.
            write!(self.writer, "\x1b[?2026l")?;
            self.in_frame = false;
        }
        self.writer.flush()
    }
}

/// ratatui's colour as crossterm's.
///
/// Hand-written because `IntoCrossterm` lives in `ratatui-crossterm` for the
/// orphan rule and is not something this crate can implement.
fn into_crossterm(colour: Color) -> CtColor {
    match colour {
        Color::Reset => CtColor::Reset,
        Color::Black => CtColor::Black,
        Color::Red => CtColor::DarkRed,
        Color::Green => CtColor::DarkGreen,
        Color::Yellow => CtColor::DarkYellow,
        Color::Blue => CtColor::DarkBlue,
        Color::Magenta => CtColor::DarkMagenta,
        Color::Cyan => CtColor::DarkCyan,
        Color::Gray => CtColor::Grey,
        Color::DarkGray => CtColor::DarkGrey,
        Color::LightRed => CtColor::Red,
        Color::LightGreen => CtColor::Green,
        Color::LightYellow => CtColor::Yellow,
        Color::LightBlue => CtColor::Blue,
        Color::LightMagenta => CtColor::Magenta,
        Color::LightCyan => CtColor::Cyan,
        Color::White => CtColor::White,
        Color::Rgb(r, g, b) => CtColor::Rgb { r, g, b },
        Color::Indexed(i) => CtColor::AnsiValue(i),
    }
}
