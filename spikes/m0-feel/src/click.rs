use ropey::Rope;

use crate::viewport::Viewport;
use crate::width::display_to_grapheme_col;

/// Map a mouse position in panel-local cells to a `(line, grapheme_col)`
/// position in the buffer.
///
/// Rows below the last line clamp to the last line, and columns past the end
/// of a line clamp to that line's length — matching what every GUI editor does.
pub fn click_to_position(
    rope: &Rope,
    vp: Viewport,
    mouse_col: u16,
    mouse_row: u16,
    tab_width: usize,
) -> (usize, usize) {
    let last_line = rope.len_lines().saturating_sub(1);
    let line = (vp.top_line + mouse_row as usize).min(last_line);

    let text = rope.line(line).to_string();
    let text = text.trim_end_matches('\n');
    let col = display_to_grapheme_col(text, mouse_col as usize, tab_width);

    (line, col)
}
