use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_buffer::{Position, Selection};
use typ_core::{Panel, RenderContext, ThemeColors};
use typ_panel_editor::EditorPanel;

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

/// Screen x for a display column of the text.
///
/// One cell of border, then the gutter — one digit and a spacer for the
/// two-line fixtures here. Stated once because these assertions are about
/// *which text* is highlighted, and a raw column literal makes every one of
/// them wrong together the next time the furniture to their left changes.
fn tx(col: u16) -> u16 {
    const BORDER: u16 = 1;
    const GUTTER: u16 = 2;
    BORDER + GUTTER + col
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

#[test]
fn a_new_editor_has_exactly_one_empty_selection() {
    let panel = EditorPanel::from_str("abc\n");
    assert_eq!(panel.selections().len(), 1);
    assert!(panel.selections().primary().is_empty());
    assert_eq!(panel.cursor(), pos(0, 0));
}

#[test]
fn the_cursor_is_the_primary_head() {
    let mut panel = EditorPanel::from_str("abcdef\n");
    panel.set_selections_for_test(vec![Selection {
        anchor: pos(0, 1),
        head: pos(0, 4),
    }]);
    assert_eq!(panel.cursor(), pos(0, 4));
}

#[test]
fn selected_text_is_drawn_in_the_selection_colors() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("abcdef\n");
    panel.set_selections_for_test(vec![Selection {
        anchor: pos(0, 1),
        head: pos(0, 4),
    }]);
    let buf = render(&mut panel, Rect::new(0, 0, 20, 5));

    assert_eq!(
        buf[(tx(0), 1)].bg,
        theme.bg,
        "column 0 is outside the selection"
    );
    for col in 1..4 {
        assert_eq!(
            buf[(tx(col), 1)].bg,
            theme.selection_bg,
            "column {col} should be selected"
        );
    }
    assert_eq!(
        buf[(tx(4), 1)].bg,
        theme.bg,
        "the end of a selection is exclusive"
    );
}

#[test]
fn a_selection_spanning_lines_covers_both_ends() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("abcd\nefgh\n");
    panel.set_selections_for_test(vec![Selection {
        anchor: pos(0, 2),
        head: pos(1, 2),
    }]);
    let buf = render(&mut panel, Rect::new(0, 0, 20, 6));

    assert_eq!(
        buf[(tx(2), 1)].bg,
        theme.selection_bg,
        "tail of the first line"
    );
    assert_eq!(
        buf[(tx(0), 2)].bg,
        theme.selection_bg,
        "head of the second line"
    );
    assert_eq!(
        buf[(tx(3), 2)].bg,
        theme.bg,
        "past the selection on the second line"
    );
}

#[test]
fn every_selection_is_drawn_not_only_the_primary() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("abcdef\n");
    panel.set_selections_for_test(vec![
        Selection {
            anchor: pos(0, 0),
            head: pos(0, 1),
        },
        Selection {
            anchor: pos(0, 4),
            head: pos(0, 5),
        },
    ]);
    let buf = render(&mut panel, Rect::new(0, 0, 20, 5));
    assert_eq!(buf[(tx(0), 1)].bg, theme.selection_bg);
    assert_eq!(buf[(tx(4), 1)].bg, theme.selection_bg);
    assert_eq!(
        buf[(tx(2), 1)].bg,
        theme.bg,
        "the gap between them is not selected"
    );
}

#[test]
fn an_empty_selection_paints_nothing() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("abcdef\n");
    let buf = render(&mut panel, Rect::new(0, 0, 20, 5));
    for col in 0..6 {
        assert_eq!(
            buf[(tx(col), 1)].bg,
            theme.bg,
            "a caret must not highlight column {col}"
        );
    }
}

#[test]
fn selection_highlighting_lands_on_the_right_columns_with_wide_characters() {
    let theme = ThemeColors::default();
    let mut panel = EditorPanel::from_str("日本語\n");
    // Select the second CJK grapheme only.
    panel.set_selections_for_test(vec![Selection {
        anchor: pos(0, 1),
        head: pos(0, 2),
    }]);
    let buf = render(&mut panel, Rect::new(0, 0, 20, 5));

    assert_eq!(
        buf[(tx(0), 1)].bg,
        theme.bg,
        "the first grapheme is not selected"
    );
    assert_eq!(
        buf[(tx(2), 1)].symbol(),
        "本",
        "the selected grapheme starts two display columns in"
    );
    assert_eq!(buf[(tx(2), 1)].bg, theme.selection_bg);
    // A wide grapheme owns its own cell plus a continuation cell holding a
    // space. Only the first cell carries the glyph and its style; the terminal
    // paints the double-width character across both columns from it and never
    // draws the continuation, so the second cell's background is not part of
    // what the user sees.
    assert_eq!(buf[(tx(3), 1)].symbol(), " ", "continuation cell");
    assert_eq!(
        buf[(tx(4), 1)].bg,
        theme.bg,
        "the third grapheme is not selected"
    );
}
