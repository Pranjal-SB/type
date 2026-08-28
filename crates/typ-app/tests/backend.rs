//! The backend, and the two things it exists to draw.
//!
//! ratatui owns the double-buffer diff, so a cell it did not write is a cell
//! the next frame will not repair — which is why the undercurl cannot be an
//! escape written after `terminal.draw()`. It has to come from a `Backend`.
//!
//! The other half is the frame boundary. `run.rs` wrapped every draw in
//! synchronized-output sequences by hand; the backend absorbs those, so the
//! boundary lives in one place instead of two.

use ratatui::backend::Backend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use typ_app::backend::{TypBackend, Underlines};
use typ_core::style::Undercurl;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 4,
    height: 1,
};

/// Draw one frame into a byte buffer and return what was written.
fn draw_to_vec(paint: impl FnOnce(&mut Buffer)) -> String {
    draw_to_vec_with(Underlines::Styled, paint)
}

fn draw_to_vec_with(underlines: Underlines, paint: impl FnOnce(&mut Buffer)) -> String {
    let previous = Buffer::empty(AREA);
    let mut next = Buffer::empty(AREA);
    paint(&mut next);

    let mut out: Vec<u8> = Vec::new();
    {
        let mut backend = TypBackend::new(&mut out, underlines);
        // The real path: ratatui hands the backend the diff, not the buffer.
        backend.draw(previous.diff(&next).into_iter()).unwrap();
        backend.flush().unwrap();
    }
    String::from_utf8(out).expect("the backend writes utf-8")
}

#[test]
fn an_undercurl_cell_emits_the_styled_underline_sequence() {
    let out = draw_to_vec(|buf| {
        buf[(0, 0)]
            .set_symbol("x")
            .set_style(Style::new().undercurl());
    });
    assert!(out.contains("\x1b[4:3m"), "no undercurl in: {out:?}");
}

#[test]
fn an_undercurl_cell_emits_its_underline_colour_in_the_colon_form() {
    // **The colon form, not `58;2;r;g;b`.** Windows Terminal's conpty accepts
    // only `58:2::r:g:b` and misbehaves on the semicolon form
    // (microsoft/terminal#17426, still open), which is what crossterm's own
    // `SetUnderlineColor` writes. The colon form is also what kitty documents
    // and what the `Setulc` terminfo string vim and neovim use emits.
    let out = draw_to_vec(|buf| {
        buf[(0, 0)].set_symbol("x").set_style(
            Style::new()
                .undercurl()
                .underline_color(Color::Rgb(255, 0, 0)),
        );
    });
    assert!(out.contains("\x1b[58:2::255:0:0m"), "no colour in: {out:?}");
}

#[test]
fn a_plain_cell_emits_no_underline_sequence() {
    let out = draw_to_vec(|buf| {
        buf[(0, 0)].set_symbol("x");
    });
    assert!(!out.contains("4:3"), "an unstyled cell paid for undercurl");
    assert!(!out.contains("\x1b[4m"), "an unstyled cell was underlined");
}

#[test]
fn an_undercurl_that_ends_is_turned_off() {
    // Left on, it runs to the end of the line and looks like a rendering bug
    // rather than a diagnostic.
    let out = draw_to_vec(|buf| {
        buf[(0, 0)]
            .set_symbol("x")
            .set_style(Style::new().undercurl());
        buf[(1, 0)].set_symbol("y");
    });
    assert!(out.contains("\x1b[4:0m"), "never turned off: {out:?}");
}

#[test]
fn a_frame_is_wrapped_in_synchronized_output() {
    let out = draw_to_vec(|buf| {
        buf[(0, 0)].set_symbol("x");
    });
    assert!(out.starts_with("\x1b[?2026h"), "no frame start: {out:?}");
    assert!(out.ends_with("\x1b[?2026l"), "no frame end: {out:?}");
}

#[test]
fn a_terminal_without_styled_underlines_falls_back_to_a_plain_one() {
    // There is no reliable query for this, so the fallback has to be something
    // rather than nothing: a plain underline still says "look here".
    let out = draw_to_vec_with(Underlines::Plain, |buf| {
        buf[(0, 0)]
            .set_symbol("x")
            .set_style(Style::new().undercurl());
    });
    assert!(out.contains("\x1b[4m"), "no plain underline in: {out:?}");
    assert!(!out.contains("4:3"), "styled underline on a plain terminal");
}

#[test]
fn a_plain_terminal_still_gets_the_underline_colour() {
    // Colour and shape are separate capabilities, and the colour is the older
    // and better supported of the two.
    let out = draw_to_vec_with(Underlines::Plain, |buf| {
        buf[(0, 0)].set_symbol("x").set_style(
            Style::new()
                .undercurl()
                .underline_color(Color::Rgb(0, 255, 0)),
        );
    });
    assert!(out.contains("\x1b[58:2::0:255:0m"), "{out:?}");
}

#[test]
fn ordinary_modifiers_still_work() {
    let out = draw_to_vec(|buf| {
        buf[(0, 0)]
            .set_symbol("x")
            .set_style(Style::new().bold().italic());
    });
    assert!(out.contains("\x1b[1m"), "no bold in: {out:?}");
    assert!(out.contains("\x1b[3m"), "no italic in: {out:?}");
}

#[test]
fn a_modifier_that_goes_away_is_turned_off() {
    let out = draw_to_vec(|buf| {
        buf[(0, 0)].set_symbol("x").set_style(Style::new().bold());
        buf[(1, 0)].set_symbol("y");
    });
    assert!(out.contains("\x1b[22m"), "bold was never cleared: {out:?}");
}

#[test]
fn a_contiguous_run_moves_the_cursor_once() {
    // One `MoveTo` for the run, not one per cell. The diff hands cells in
    // order, and re-addressing every one of them is most of a frame's bytes.
    let out = draw_to_vec(|buf| {
        for (i, c) in "abcd".chars().enumerate() {
            buf[(i as u16, 0)].set_symbol(&c.to_string());
        }
    });
    assert_eq!(out.matches("\x1b[1;1H").count(), 1, "{out:?}");
    assert!(!out.contains("\x1b[1;2H"), "re-addressed mid-run: {out:?}");
}

#[test]
fn the_frame_ends_with_everything_reset() {
    // A style left set leaks into whatever the shell prints next.
    let out = draw_to_vec(|buf| {
        buf[(0, 0)].set_symbol("x").set_style(
            Style::new()
                .undercurl()
                .underline_color(Color::Rgb(1, 2, 3)),
        );
    });
    let tail = out.rsplit_once('x').expect("the symbol was printed").1;
    assert!(
        tail.contains("\x1b[0m"),
        "no reset before the end: {tail:?}"
    );
}
