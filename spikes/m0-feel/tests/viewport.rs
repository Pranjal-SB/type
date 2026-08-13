use m0_feel::viewport::Viewport;

#[test]
fn visible_range_starts_at_top_line() {
    let vp = Viewport { top_line: 10, height: 5 };
    assert_eq!(vp.visible_range(100), 10..15);
}

#[test]
fn visible_range_clamps_to_total_lines() {
    let vp = Viewport { top_line: 98, height: 5 };
    assert_eq!(vp.visible_range(100), 98..100);
}

#[test]
fn scroll_down_advances_top_line() {
    let mut vp = Viewport { top_line: 0, height: 10 };
    vp.scroll(3, 100);
    assert_eq!(vp.top_line, 3);
}

#[test]
fn scroll_up_past_start_clamps_to_zero() {
    let mut vp = Viewport { top_line: 2, height: 10 };
    vp.scroll(-10, 100);
    assert_eq!(vp.top_line, 0);
}

#[test]
fn scroll_down_past_end_keeps_last_screen_visible() {
    let mut vp = Viewport { top_line: 0, height: 10 };
    vp.scroll(1000, 100);
    assert_eq!(vp.top_line, 90);
}

#[test]
fn scroll_does_not_underflow_when_file_is_shorter_than_viewport() {
    let mut vp = Viewport { top_line: 0, height: 50 };
    vp.scroll(10, 3);
    assert_eq!(vp.top_line, 0);
}
