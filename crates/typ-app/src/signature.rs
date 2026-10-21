//! Signature help (interface §8 2c): the call being typed, above the caret,
//! with the argument under it marked.
//!
//! An unfocused float (interface §3): bordered in the rule, never given a key.
//! Typing goes on into the buffer under it.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use typ_buffer::display_width;
use typ_core::RenderContext;

pub const NAME: &str = "signature";
pub const EXIT: &str = "esc";
/// Wide enough for the border to carry its name and its exit.
const MIN_WIDTH: u16 = 23;
/// A border and a row.
const HEIGHT: u16 = 3;

/// One signature, as a server sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub label: String,
    /// The active parameter, as a char range into `label`.
    pub active: Option<(usize, usize)>,
}

impl Signature {
    /// The active parameter's text, if there is one.
    pub fn active_text(&self) -> Option<String> {
        let (start, end) = self.active?;
        Some(self.label.chars().skip(start).take(end - start).collect())
    }

    /// Where it goes in `body` for a caret at `caret`: above the caret's row
    /// with a row of page between, below when above is short. Its label lines
    /// up with the caret's column, less the border and a space.
    pub fn area(&self, body: Rect, caret: (u16, u16)) -> Rect {
        let width = u16::try_from(display_width(&self.label) + 4)
            .unwrap_or(u16::MAX)
            .max(MIN_WIDTH)
            .min(body.width);
        let (cx, cy) = caret;
        let x = cx
            .saturating_sub(2)
            .min(body.right().saturating_sub(width))
            .max(body.x);
        let y = if cy > body.y + HEIGHT {
            cy - HEIGHT - 1
        } else {
            cy.saturating_add(2)
        };
        Rect::new(x, y, width, HEIGHT).intersection(body)
    }

    /// Draw it, the active parameter in the accent.
    pub fn render(&self, area: Rect, buf: &mut Buffer, ctx: &RenderContext) {
        let unfocused = RenderContext {
            is_focused: false,
            ..*ctx
        };
        typ_core::chrome::float(area, buf, NAME, EXIT, &unfocused);
        let inner = typ_core::chrome::inner(area);
        if inner.height == 0 || inner.width < 2 {
            return;
        }
        let theme = ctx.theme;
        let plain = Style::default().fg(theme.fg).bg(theme.chrome_bg);
        let accent = plain
            .fg(theme.status_bar_accent)
            .add_modifier(Modifier::BOLD);
        let (start, end) = self.active.unwrap_or((0, 0));
        let mut x = inner.x + 1;
        let right = inner.right().saturating_sub(1);
        for (index, c) in self.label.chars().enumerate() {
            if x >= right {
                break;
            }
            let style = if (start..end).contains(&index) {
                accent
            } else {
                plain
            };
            let (next, _) =
                buf.set_stringn(x, inner.y, c.to_string(), usize::from(right - x), style);
            x = next;
        }
    }
}
