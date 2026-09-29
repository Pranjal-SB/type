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
    // Gap 105. Ten CJK characters are twenty cells; counted as ten, the box
    // was too narrow and the text was clipped inside it.
    let area = hover_area(FRAME, (5, 5), &"日".repeat(10));
    assert_eq!(area.width, 20 + 2);
}

#[test]
fn the_box_leaves_a_row_of_page_between_itself_and_the_cursor() {
    // Interface §3: a float keeps one clear cell around its border. The row
    // under the cursor is that gutter, so the box starts one lower, and the
    // gutter never covers the line being asked about.
    let below = hover_area(FRAME, (5, 5), "fn fake()");
    assert_eq!(below.y, 5 + 2);

    // Near the bottom it goes above, with the same row of page in between.
    let above = hover_area(FRAME, (5, 38), "fn fake()");
    assert_eq!(above.bottom() + 1, 38);
}

#[test]
fn a_short_answer_still_gets_a_box_wide_enough_to_name_itself() {
    // `╭─ hover ─ esc ─╮`: under this the exit would give way.
    let area = hover_area(FRAME, (5, 5), "u8");
    assert_eq!(area.width, typ_app::layout::HOVER_MIN_WIDTH);
    assert_eq!(typ_app::layout::HOVER_MIN_WIDTH, 17);
}
