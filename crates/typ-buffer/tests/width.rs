use typ_buffer::{display_to_grapheme_col, display_width, grapheme_to_display_col};

#[test]
fn ascii_width_is_one_per_char() {
    assert_eq!(display_width("hello"), 5);
}

#[test]
fn cjk_chars_are_two_columns_wide() {
    assert_eq!(display_width("日本語"), 6);
}

#[test]
fn emoji_is_two_columns_wide() {
    assert_eq!(display_width("🦀"), 2);
}

#[test]
fn combining_marks_do_not_add_width() {
    // "e" + combining acute accent renders as one column.
    assert_eq!(display_width("e\u{0301}"), 1);
}

#[test]
fn grapheme_to_display_col_accounts_for_wide_chars() {
    // Before "語" there are two CJK graphemes, each 2 columns wide.
    assert_eq!(grapheme_to_display_col("日本語", 2, 4), 4);
}

#[test]
fn display_to_grapheme_col_is_inverse_for_wide_chars() {
    assert_eq!(display_to_grapheme_col("日本語", 4, 4), 2);
}

#[test]
fn display_to_grapheme_col_snaps_into_a_wide_char() {
    // Clicking the right half of "日" must land on grapheme 0, not 1.
    assert_eq!(display_to_grapheme_col("日本語", 1, 4), 0);
}

#[test]
fn tabs_expand_to_the_next_tab_stop() {
    assert_eq!(display_width("\t"), 4);
    assert_eq!(grapheme_to_display_col("a\tb", 2, 4), 4);
}

#[test]
fn clicking_past_end_of_line_clamps_to_line_length() {
    assert_eq!(display_to_grapheme_col("abc", 99, 4), 3);
}
