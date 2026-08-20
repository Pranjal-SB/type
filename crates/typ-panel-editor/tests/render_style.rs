//! Current line, primary selection, matching brackets.
//!
//! Three small render changes that together are most of what "designed" looks
//! like — and all three are invisible to every other test in the crate, because
//! every other test asks what the editor *did* rather than what it drew.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_buffer::{Position, Selection};
use typ_core::{Panel, RenderContext, ThemeColors};
use typ_panel_editor::EditorPanel;

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

fn render(panel: &mut EditorPanel, area: Rect) -> Buffer {
    let theme = ThemeColors::default();
    let ctx = RenderContext {
        theme: &theme,
        is_focused: true,
        panel_index: 0,
        terminal_width: area.width,
        terminal_height: area.height,
    };
    let mut buf = Buffer::empty(area);
    panel.render(area, &mut buf, &ctx);
    buf
}

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 24,
    height: 6,
};

/// Screen x for a display column of text: border, then a two-cell gutter for
/// the small fixtures here.
fn tx(col: u16) -> u16 {
    3 + col
}

/// Screen y for a buffer line at the top of the viewport.
fn ty(line: u16) -> u16 {
    1 + line
}

// --- the current line ----------------------------------------------------

#[test]
fn the_cursors_line_is_tinted_across_the_whole_text_width() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("ab\ncd\n");
    let buf = render(&mut panel, AREA);

    // Past the end of "ab" as well as under it. A highlight that stops at the
    // last character reads as a rendering bug rather than as a feature — it is
    // the ragged right edge that gives it away.
    for col in 0..20 {
        assert_eq!(
            buf[(tx(col), ty(0))].bg,
            theme.cursor_line_bg,
            "column {col} of the cursor's line should be tinted"
        );
    }
}

#[test]
fn other_lines_are_left_alone() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("ab\ncd\n");
    let buf = render(&mut panel, AREA);
    assert_eq!(buf[(tx(0), ty(1))].bg, theme.bg);
}

#[test]
fn the_highlight_runs_the_whole_row_including_the_gutter() {
    // **This reverses an earlier decision, deliberately.** The rule used to be
    // that the tint stopped at the gutter, on the argument that the current
    // line's number is already brightened and tinting its background too is
    // "two answers to one question".
    //
    // That argument is about redundant *signals*, and it holds. What it misses
    // is that a background tint is not a signal, it is a shape: its job is to
    // say which row the caret is on, and a band that begins three cells in does
    // not draw a row, it draws a rectangle floating inside one. Seen on a real
    // terminal rather than in an assertion, that reads as a rendering fault —
    // the same ragged-edge complaint `the_cursors_line_is_tinted_across_the_
    // whole_text_width` already fixed at the right-hand end, arriving from the
    // left.
    //
    // The number keeps its brighter foreground. Two answers, but to two
    // different questions: the colour says "this line", the shape says "this
    // row".
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("ab\ncd\n");
    let buf = render(&mut panel, AREA);

    // Every cell from the frame's edge to the end of the text.
    for x in 1..AREA.width - 1 {
        assert_eq!(
            buf[(x, ty(0))].bg,
            theme.cursor_line_bg,
            "column {x} of the cursor's row should be tinted"
        );
    }
    // And the row below, which has no caret, keeps the gutter's own background
    // — otherwise the tint says nothing.
    assert_eq!(buf[(1, ty(1))].bg, theme.gutter_bg);
}

#[test]
fn every_cursors_line_is_highlighted_not_just_the_primarys() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("ab\ncd\nef\n");
    panel.set_selections_for_test(vec![
        Selection::caret(pos(0, 0)),
        Selection::caret(pos(2, 0)),
    ]);
    let buf = render(&mut panel, AREA);

    // Picking one and leaving the other twenty-nine invisible is the failure
    // mode here.
    assert_eq!(buf[(tx(0), ty(0))].bg, theme.cursor_line_bg);
    assert_eq!(buf[(tx(0), ty(2))].bg, theme.cursor_line_bg);
    assert_eq!(buf[(tx(0), ty(1))].bg, theme.bg, "the gap is not a cursor");
}

#[test]
fn a_line_with_a_real_selection_on_it_loses_the_stripe() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("abcdef\nghi\n");
    panel.set_selections_for_test(vec![Selection {
        anchor: pos(0, 0),
        head: pos(0, 3),
    }]);
    let buf = render(&mut panel, AREA);

    // The selection is already saying where the user is. Painting a stripe
    // behind it as well is noise, and it is why VS Code drops the current-line
    // highlight the moment a selection exists.
    assert_eq!(
        buf[(tx(5), ty(0))].bg,
        theme.bg,
        "past the selection, on a line with a non-empty selection, is plain"
    );
}

// --- primary versus secondary --------------------------------------------

#[test]
fn the_primary_selection_is_drawn_in_its_own_colour() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("abc\ndef\n");
    // set_selections_for_test makes the last one primary.
    panel.set_selections_for_test(vec![
        Selection {
            anchor: pos(0, 0),
            head: pos(0, 2),
        },
        Selection {
            anchor: pos(1, 0),
            head: pos(1, 2),
        },
    ]);
    let buf = render(&mut panel, AREA);

    assert_eq!(
        buf[(tx(0), ty(1))].bg,
        theme.selection_primary_bg,
        "the primary is the one every motion is relative to"
    );
    assert_eq!(buf[(tx(0), ty(0))].bg, theme.selection_bg);
}

#[test]
fn a_lone_selection_is_the_primary_one() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("abc\n");
    panel.set_selections_for_test(vec![Selection {
        anchor: pos(0, 0),
        head: pos(0, 2),
    }]);
    let buf = render(&mut panel, AREA);
    assert_eq!(buf[(tx(0), ty(0))].bg, theme.selection_primary_bg);
}

// --- brackets ------------------------------------------------------------

#[test]
fn both_halves_of_a_matched_pair_are_highlighted() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("fn f() {\n}\n");
    panel.set_selections_for_test(vec![Selection::caret(pos(0, 7))]);
    let buf = render(&mut panel, AREA);

    assert_eq!(buf[(tx(7), ty(0))].bg, theme.bracket_match_bg, "the open");
    assert_eq!(buf[(tx(7), ty(0))].fg, theme.bracket_match_fg);
    assert_eq!(buf[(tx(0), ty(1))].bg, theme.bracket_match_bg, "the close");
}

#[test]
fn an_unmatched_bracket_is_drawn_plainly_and_silently() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("fn f( {\n");
    panel.set_selections_for_test(vec![Selection::caret(pos(0, 4))]);
    let buf = render(&mut panel, AREA);
    // No match, no complaint, no highlight — it is a hint, and a hint that
    // reports failure is worse than one that stays quiet.
    assert_ne!(buf[(tx(4), ty(0))].bg, theme.bracket_match_bg);
}

#[test]
fn a_selection_wins_over_a_bracket_highlight() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("(abc)\n");
    panel.set_selections_for_test(vec![Selection {
        anchor: pos(0, 0),
        head: pos(0, 5),
    }]);
    let buf = render(&mut panel, AREA);
    // Both are "where you are", and the selection is the one the next keystroke
    // acts on.
    assert_eq!(buf[(tx(0), ty(0))].bg, theme.selection_primary_bg);
}
