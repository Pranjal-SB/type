//! A diagnostic is visible without being read.
//!
//! Two marks, because one is not enough on its own. The undercurl says *which
//! text*, and it is invisible on a line scrolled off the screen or past the
//! right edge; the gutter sign says *which line*, and it cannot say which word.
//! Every editor in the field draws both.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use typ_buffer::{Position, Selection};
use typ_core::{Diagnostic, Panel, RenderContext, Severity, ThemeColors, UNDERCURL};
use typ_panel_editor::EditorPanel;
use typ_panel_editor::gutter::{Gutter, GutterComponent};

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 30,
    height: 6,
};

fn at(line: usize, col: usize) -> Position {
    Position { line, col }
}

fn diagnostic(severity: Severity, from: Position, to: Position) -> Diagnostic {
    Diagnostic {
        range: (from, to),
        severity,
        message: "something".into(),
        source: Some("test".into()),
    }
}

fn render(panel: &mut EditorPanel, diagnostics: &[Diagnostic]) -> Buffer {
    let theme = ThemeColors::default();
    let ctx = RenderContext {
        theme: &theme,
        syntax: typ_core::SyntaxTheme::empty(),
        diagnostics,
        is_focused: true,
        panel_index: 0,
        terminal_width: AREA.width,
        terminal_height: AREA.height,
    };
    let mut buf = Buffer::empty(AREA);
    panel.render(AREA, &mut buf, &ctx);
    buf
}

/// A panel with a gutter that has a diagnostics column, and no line numbers —
/// so the sign is at a known x and the test is not counting digits.
fn panel(text: &str) -> EditorPanel {
    let mut panel = EditorPanel::from_str(text);
    panel.set_gutter(Gutter::new(vec![
        GutterComponent::Diagnostics,
        GutterComponent::Spacer,
    ]));
    panel
}

/// `chrome::frame` draws a border, so the panel's first content cell is
/// (1, 1). The fixture's gutter is then a sign and a spacer.
const GUTTER_X: u16 = 1;
const TEXT_X: u16 = 3;

/// Buffer line `line` is drawn on row `line + 1`, inside the border.
fn y_of(line: u16) -> u16 {
    line + 1
}

/// The gutter's diagnostic cell for a buffer line.
fn sign(buf: &Buffer, line: u16) -> String {
    buf[(GUTTER_X, y_of(line))].symbol().to_string()
}

#[test]
fn an_error_range_is_undercurled_in_the_error_colour() {
    let mut panel = panel("let x = wrong;\nfn ok() {}\n");
    let theme = ThemeColors::default();
    let buf = render(
        &mut panel,
        &[diagnostic(Severity::Error, at(0, 8), at(0, 13))],
    );

    for col in 8..13u16 {
        let cell = &buf[(TEXT_X + col, y_of(0))];
        assert!(
            cell.modifier.contains(UNDERCURL),
            "column {col} was not undercurled"
        );
        assert_eq!(cell.underline_color, theme.diagnostic_error, "column {col}");
    }
}

#[test]
fn only_the_range_is_underlined() {
    let mut panel = panel("let x = wrong;\n");
    let buf = render(
        &mut panel,
        &[diagnostic(Severity::Error, at(0, 8), at(0, 13))],
    );
    assert!(
        !buf[(TEXT_X, y_of(0))].modifier.contains(UNDERCURL),
        "before"
    );
    assert!(
        !buf[(TEXT_X + 13, y_of(0))].modifier.contains(UNDERCURL),
        "after"
    );
}

#[test]
fn a_zero_width_range_still_marks_a_cell() {
    // Servers use an empty range for a missing token — a semicolon that should
    // be there and is not. Underlining nothing is the same as saying nothing.
    let mut panel = panel("let x = 1\n");
    let buf = render(
        &mut panel,
        &[diagnostic(Severity::Error, at(0, 9), at(0, 9))],
    );
    assert!(buf[(TEXT_X + 9, y_of(0))].modifier.contains(UNDERCURL));
}

#[test]
fn a_warning_uses_the_warning_colour() {
    let mut panel = panel("let x = 1;\n");
    let theme = ThemeColors::default();
    let buf = render(
        &mut panel,
        &[diagnostic(Severity::Warning, at(0, 4), at(0, 5))],
    );
    assert_eq!(
        buf[(TEXT_X + 4, y_of(0))].underline_color,
        theme.diagnostic_warning
    );
}

#[test]
fn a_diagnostic_spanning_lines_marks_both_of_them() {
    let mut panel = panel("fn a() {\n    body\n}\n");
    let buf = render(
        &mut panel,
        &[diagnostic(Severity::Error, at(0, 3), at(2, 1))],
    );
    assert!(
        buf[(TEXT_X + 3, y_of(0))].modifier.contains(UNDERCURL),
        "first"
    );
    assert!(
        buf[(TEXT_X + 2, y_of(1))].modifier.contains(UNDERCURL),
        "middle"
    );
    assert!(buf[(TEXT_X, y_of(2))].modifier.contains(UNDERCURL), "last");
}

#[test]
fn a_wide_grapheme_carries_the_underline_on_the_cell_that_holds_it() {
    // A CJK character is one grapheme and two cells, and the terminal draws the
    // underline under the whole glyph because the escape precedes it.
    //
    // In the buffer only the first cell holds anything: `Buffer::set_stringn`
    // calls `reset()` on the cells a wide grapheme covers, since they are
    // hidden by it. This test asserted on the second cell first time round,
    // which was asserting about ratatui's bookkeeping rather than about what
    // is drawn.
    let mut panel = panel("let s = 日本;\n");
    let buf = render(
        &mut panel,
        &[diagnostic(Severity::Error, at(0, 8), at(0, 9))],
    );
    let cell = &buf[(TEXT_X + 8, y_of(0))];
    assert_eq!(cell.symbol(), "日");
    assert!(cell.modifier.contains(UNDERCURL));
    assert!(
        !buf[(TEXT_X + 10, y_of(0))].modifier.contains(UNDERCURL),
        "the grapheme after it must be clean"
    );
}

#[test]
fn the_gutter_shows_a_sign_on_a_line_with_a_diagnostic() {
    let mut panel = panel("one\ntwo\nthree\n");
    let buf = render(
        &mut panel,
        &[diagnostic(Severity::Error, at(1, 0), at(1, 3))],
    );
    assert_eq!(sign(&buf, 0), " ", "line 0 has nothing");
    assert_ne!(sign(&buf, 1), " ", "line 1 has a diagnostic");
    assert_eq!(sign(&buf, 2), " ", "line 2 has nothing");
}

#[test]
fn the_gutter_sign_is_one_cell_wide() {
    // The gutter's width is fixed for the frame. A two-cell glyph would push
    // every line of text one column right of where the mouse thinks it is.
    let mut panel = panel("one\ntwo\n");
    let buf = render(
        &mut panel,
        &[diagnostic(Severity::Error, at(0, 0), at(0, 3))],
    );
    assert_ne!(sign(&buf, 0), " ", "no sign to measure");
    assert_eq!(
        buf[(GUTTER_X + 1, y_of(0))].symbol(),
        " ",
        "the spacer was overwritten, so the glyph took two cells"
    );
}

#[test]
fn the_most_severe_diagnostic_on_a_line_wins_the_gutter() {
    let mut panel = panel("one\ntwo\n");
    let theme = ThemeColors::default();
    let buf = render(
        &mut panel,
        &[
            diagnostic(Severity::Hint, at(0, 0), at(0, 1)),
            diagnostic(Severity::Error, at(0, 1), at(0, 2)),
            diagnostic(Severity::Warning, at(0, 2), at(0, 3)),
        ],
    );
    assert_eq!(buf[(GUTTER_X, y_of(0))].fg, theme.diagnostic_error);
}

#[test]
fn a_diagnostic_outside_the_viewport_draws_nothing() {
    // `for_viewport`, not `for_buffer`. Correctness here; the cost is measured
    // in `tests/perf.rs`.
    let text: String = (0..200).map(|i| format!("line {i}\n")).collect();
    let mut panel = panel(&text);
    let buf = render(
        &mut panel,
        &[diagnostic(Severity::Error, at(150, 0), at(150, 4))],
    );
    for line in 0..AREA.height - 2 {
        assert_eq!(sign(&buf, line), " ", "line {line} drew a sign");
    }
}

#[test]
fn a_line_with_no_diagnostic_is_not_underlined() {
    let mut panel = panel("clean\ndirty\n");
    let buf = render(
        &mut panel,
        &[diagnostic(Severity::Error, at(1, 0), at(1, 5))],
    );
    for x in 0..5u16 {
        assert!(
            !buf[(TEXT_X + x, y_of(0))].modifier.contains(UNDERCURL),
            "column {x} of a clean line"
        );
    }
}

#[test]
fn an_undercurl_is_not_a_plain_underline() {
    // The backend draws them differently and a terminal that cannot do one can
    // still do the other, so they must not be the same bit.
    let mut panel = panel("let x = 1;\n");
    let buf = render(
        &mut panel,
        &[diagnostic(Severity::Error, at(0, 4), at(0, 5))],
    );
    let cell = &buf[(TEXT_X + 4, y_of(0))];
    assert!(cell.modifier.contains(UNDERCURL));
    assert!(!cell.modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn a_selection_over_a_diagnostic_keeps_both() {
    // Two axes, not one. The selection sets a background and the diagnostic an
    // underline, so neither has to win.
    let mut panel = panel("let x = 1;\n");
    panel.select_range(Selection {
        anchor: at(0, 0),
        head: at(0, 10),
    });
    let theme = ThemeColors::default();
    let buf = render(
        &mut panel,
        &[diagnostic(Severity::Error, at(0, 4), at(0, 5))],
    );
    let cell = &buf[(TEXT_X + 4, y_of(0))];
    assert!(cell.modifier.contains(UNDERCURL), "lost the diagnostic");
    assert_eq!(cell.bg, theme.selection_primary_bg, "lost the selection");
}
