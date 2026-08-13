use m0_feel::click::click_to_position;
use m0_feel::viewport::Viewport;
use ropey::Rope;

fn rope() -> Rope {
    Rope::from_str("hello world\n日本語です\nshort\n")
}

#[test]
fn click_on_first_line_maps_to_that_column() {
    let vp = Viewport { top_line: 0, height: 10 };
    assert_eq!(click_to_position(&rope(), vp, 6, 0, 4), (0, 6));
}

#[test]
fn click_accounts_for_scroll_offset() {
    let vp = Viewport { top_line: 2, height: 10 };
    // Row 0 on screen is buffer line 2 when scrolled by 2.
    assert_eq!(click_to_position(&rope(), vp, 0, 0, 4), (2, 0));
}

#[test]
fn click_inside_a_wide_char_selects_that_char() {
    let vp = Viewport { top_line: 1, height: 10 };
    // Column 1 is the right half of the first CJK grapheme.
    assert_eq!(click_to_position(&rope(), vp, 1, 0, 4), (1, 0));
}

#[test]
fn click_past_end_of_line_clamps_to_line_end() {
    let vp = Viewport { top_line: 2, height: 10 };
    assert_eq!(click_to_position(&rope(), vp, 99, 0, 4), (2, 5));
}

#[test]
fn click_below_last_line_clamps_to_last_line() {
    let vp = Viewport { top_line: 0, height: 10 };
    let r = rope();
    let (line, _) = click_to_position(&r, vp, 0, 90, 4);
    assert_eq!(line, r.len_lines() - 1);
}
