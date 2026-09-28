//! Where the hover box goes, given text a server controls.

use ratatui::layout::Rect;
use typ_app::layout::{HOVER_MAX_HEIGHT, HOVER_MAX_WIDTH, hover_area};

const FRAME: Rect = Rect {
    x: 0,
    y: 0,
    width: 120,
    height: 40,
};

#[test]
fn a_line_longer_than_a_u16_still_gets_the_widest_box() {
    // Gap 106. rust-analyzer emits very long single lines on macro-heavy code.
    // Cast to `u16` first, 65534 overflowed the `+ 2` and 65536 wrapped to a
    // 3-cell box.
    for longest in [65_534, 65_535, 65_536, 1_000_000] {
        let area = hover_area(FRAME, (5, 5), &"x".repeat(longest));
        assert_eq!(area.width, HOVER_MAX_WIDTH, "longest = {longest}");
    }
}

#[test]
fn more_lines_than_a_u16_still_gets_the_tallest_box() {
    for lines in [65_534, 65_536] {
        let area = hover_area(FRAME, (5, 5), &"x\n".repeat(lines));
        assert_eq!(area.height, HOVER_MAX_HEIGHT, "lines = {lines}");
    }
}

#[test]
fn a_wide_line_is_measured_in_cells() {
    // Gap 105. Three CJK characters are six cells; counted as three, the box
    // was too narrow and the text was clipped inside it.
    let area = hover_area(FRAME, (5, 5), "日本語");
    assert_eq!(area.width, 6 + 2);
}
