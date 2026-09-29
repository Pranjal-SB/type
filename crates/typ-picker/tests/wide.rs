//! Wide characters in the overlay's rows. Gap 105.
//!
//! A CJK grapheme is one grapheme and two cells. Counted as one cell, every
//! glyph after it was drawn a column early, over the first one's second half.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_core::{Panel, RenderContext, ThemeColors};
use typ_find::{FileHit, LineHit};
use typ_picker::Picker;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 40,
    height: 10,
};

/// The first list row: border, query, rule.
const FIRST_ROW: u16 = 3;
/// The inner rect's left edge, inside the border.
const LEFT: u16 = 1;

fn render(picker: &mut Picker) -> Buffer {
    let theme = ThemeColors::default();
    let ctx = RenderContext {
        theme: &theme,
        syntax: typ_core::SyntaxTheme::empty(),
        diagnostics: &[],
        is_focused: true,
        panel_index: 0,
        terminal_width: 40,
        terminal_height: 10,
    };
    let mut buf = Buffer::empty(AREA);
    picker.render(AREA, &mut buf, &ctx);
    buf
}

#[test]
fn a_file_row_puts_each_glyph_in_its_own_cells() {
    let mut picker = Picker::new();
    picker.set_hits(vec![FileHit {
        path: "日本/a.rs".to_string(),
        indices: Vec::new(),
    }]);

    let buf = render(&mut picker);

    assert_eq!(buf[(LEFT, FIRST_ROW)].symbol(), "日");
    assert_eq!(buf[(LEFT + 2, FIRST_ROW)].symbol(), "本");
    assert_eq!(
        buf[(LEFT + 4, FIRST_ROW)].symbol(),
        "/",
        "the separator was drawn over the second half of a wide character"
    );
}

#[test]
fn a_search_row_starts_its_text_after_the_location_s_cells() {
    let mut picker = Picker::search();
    picker.set_lines(
        vec![LineHit {
            path: "日本.rs".to_string(),
            line: 0,
            col: 0,
            text: "needle".to_string(),
        }],
        true,
    );

    let buf = render(&mut picker);

    // `日本.rs:1` is nine cells, then the two-space gap.
    assert_eq!(
        buf[(LEFT + 9 + 2, FIRST_ROW)].symbol(),
        "n",
        "the match text was placed by counting graphemes"
    );
}
