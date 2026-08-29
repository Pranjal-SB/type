//! The one text attribute ratatui does not have.
//!
//! `Modifier` carries nine bits — bold, dim, italic, underlined, two blinks,
//! reversed, hidden, crossed out — and none of them is a curl. crossterm has
//! had the attribute since 0.29 (`Undercurled = 3`), so the gap is in the
//! middle layer rather than at either end, and there is no upstream proposal to
//! wait for: a search of `ratatui/ratatui` for undercurl returns nothing, open
//! or closed.
//!
//! `Modifier` is a `u16` whose highest used bit is `0b0001_0000_0000`, so the
//! bit above it is free and `from_bits_retain` claims it without a fork. It
//! survives `Style`, the `Buffer` and the frame diff untouched, because none of
//! those inspects which bits are set — they compare and combine. Only a backend
//! looks, and TYPE owns the one it draws through.
//!
//! The risk this takes is named rather than hidden: if ratatui ever spends this
//! bit on an attribute of its own, TYPE's undercurl becomes that attribute.
//! `a_free_bit_is_still_free` is the test that fails on the day it happens.

use ratatui::style::{Modifier, Style};

/// A curly underline, in the first bit ratatui has not spent.
pub const UNDERCURL: Modifier = Modifier::from_bits_retain(0b0010_0000_0000);

/// Sugar matching `Style::underlined`, so a caller never writes the bit.
pub trait Undercurl {
    /// Draw a curly underline under this text.
    ///
    /// On a terminal without styled underlines the backend draws a plain one,
    /// because a diagnostic that is invisible is worse than one that is drawn
    /// with the wrong shape.
    fn undercurl(self) -> Self;
}

impl Undercurl for Style {
    fn undercurl(self) -> Self {
        self.add_modifier(UNDERCURL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_free_bit_is_still_free() {
        // The day ratatui spends this bit, TYPE's undercurl silently becomes
        // whatever they spent it on. This test is how that day announces
        // itself.
        assert!(
            !Modifier::all().contains(UNDERCURL),
            "ratatui has claimed the bit UNDERCURL uses; pick another or fork"
        );
    }

    #[test]
    fn undercurl_is_not_underlined() {
        // Two different attributes. A cell can carry both, and the backend
        // draws the curl for one and a line for the other.
        let style = Style::new().undercurl();
        assert!(style.add_modifier.contains(UNDERCURL));
        assert!(!style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn undercurl_survives_being_patched_onto_another_style() {
        // `Buffer::set_style` patches, and a style that lost the bit on the way
        // through would be a diagnostic that renders everywhere except where it
        // was combined with something.
        let combined = Style::new()
            .add_modifier(Modifier::BOLD)
            .patch(Style::new().undercurl());
        assert!(combined.add_modifier.contains(UNDERCURL));
        assert!(combined.add_modifier.contains(Modifier::BOLD));
    }
}
