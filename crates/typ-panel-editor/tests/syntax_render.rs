//! Painting a syntax capture, and everything that must keep outranking it.

use std::ops::Range;

use ratatui::style::{Color, Style};
use ratatui::text::Line;
use typ_buffer::{Position, Selection};
use typ_core::ThemeColors;
use typ_panel_editor::render::{LineStyle, Whitespace, styled_line};

fn keyword() -> Style {
    Style::default().fg(Color::Rgb(0xc0, 0x78, 0xdd))
}

fn string() -> Style {
    Style::default().fg(Color::Rgb(0x98, 0xc3, 0x79))
}

/// A `LineStyle` with nothing switched on, to spread with `..`.
fn plain(theme: &ThemeColors) -> LineStyle<'_> {
    LineStyle {
        line: 0,
        left_col: 0,
        width: 40,
        tab_width: 4,
        selections: &[],
        primary: Selection::caret(Position { line: 0, col: 0 }),
        cursor_line: false,
        brackets: None,
        whitespace: Whitespace::None,
        indent_guides: 0,
        syntax: &[],
        theme,
    }
}

fn render(
    text: &str,
    syntax: &[(Range<usize>, Style)],
    f: impl FnOnce(&mut LineStyle),
) -> Line<'static> {
    let theme = ThemeColors::default();
    let mut ctx = LineStyle {
        syntax,
        ..plain(&theme)
    };
    f(&mut ctx);
    styled_line(text, &ctx)
}

#[test]
fn a_keyword_takes_the_syntax_colour() {
    let line = render("fn main() {}", &[(0..2, keyword())], |_| {});

    let first = &line.spans[0];
    assert_eq!(first.content, "fn");
    assert_eq!(first.style.fg, keyword().fg);
}

#[test]
fn a_selected_keyword_keeps_the_selection_background() {
    // The reason syntax is an Overlay and not a Paint. If it chose the
    // background, selecting a keyword would hide the selection — and the
    // selection is what the next keystroke acts on.
    let theme = ThemeColors::default();
    let selections = [Selection {
        anchor: Position { line: 0, col: 0 },
        head: Position { line: 0, col: 2 },
    }];
    let ctx = LineStyle {
        syntax: &[(0..2, keyword())],
        selections: &selections,
        primary: selections[0],
        ..plain(&theme)
    };
    let line = styled_line("fn main() {}", &ctx);

    let first = &line.spans[0];
    assert_eq!(first.content, "fn");
    assert_eq!(first.style.fg, keyword().fg, "syntax lost to the selection");
    assert_eq!(first.style.bg, Some(theme.selection_primary_bg));
}

#[test]
fn a_whitespace_mark_outranks_syntax() {
    // Precedence, stated once so it cannot drift: the mark is on screen only
    // because the user asked to see that character.
    let theme = ThemeColors::default();
    let ctx = LineStyle {
        syntax: &[(0..5, string())],
        whitespace: Whitespace::All,
        ..plain(&theme)
    };
    let line = styled_line("\"a b\"", &ctx);

    let space = line
        .spans
        .iter()
        .find(|s| s.content.contains('·'))
        .expect("a mark");
    assert_eq!(space.style.fg, Some(theme.whitespace));
}

#[test]
fn an_indent_guide_outranks_syntax() {
    // The other half of the precedence. A guide stands only in indentation,
    // which no grammar captures — but a span covering the whole line would
    // still reach those cells if syntax were checked first.
    let theme = ThemeColors::default();
    let ctx = LineStyle {
        syntax: &[(0..12, string())],
        indent_guides: 1,
        ..plain(&theme)
    };
    let line = styled_line("    let x = 1", &ctx);

    let guide = line
        .spans
        .iter()
        .find(|s| s.content.contains('│'))
        .expect("a guide");
    assert_eq!(guide.style.fg, Some(theme.indent_guide));
}

#[test]
fn a_line_with_no_syntax_renders_exactly_as_before() {
    // The regression gate for every buffer without a grammar, which is most of
    // them on day one.
    let theme = ThemeColors::default();
    let with = styled_line("let x = 1;", &plain(&theme));
    let without = styled_line("let x = 1;", &plain(&theme));
    assert_eq!(with.spans.len(), without.spans.len());
    assert_eq!(with.spans[0].style, without.spans[0].style);
}

#[test]
fn a_span_across_a_wide_grapheme_does_not_split_it() {
    let line = render("\"日本\"", &[(0..4, string())], |_| {});
    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(text.contains('日'), "a wide grapheme was cut in half");
    assert!(text.contains('本'), "a wide grapheme was cut in half");
}

#[test]
fn two_adjacent_scopes_become_two_spans() {
    // Runs break where the capture changes, or a keyword and the string after
    // it would be painted as one colour — whichever arrived first.
    let line = render("fn\"a\"", &[(0..2, keyword()), (2..5, string())], |_| {});

    assert_eq!(line.spans[0].content, "fn");
    assert_eq!(line.spans[0].style.fg, keyword().fg);
    assert_eq!(line.spans[1].style.fg, string().fg);
}

#[test]
fn a_span_is_read_in_grapheme_columns_not_bytes() {
    // Invariant 4. The panel converts byte offsets to grapheme columns before
    // building this slice; if it ever stops, a line with a multi-byte
    // character ahead of the capture paints the wrong cells — and every
    // ASCII test in this file still passes.
    let line = render("é fn", &[(2..4, keyword())], |_| {});

    let coloured: String = line
        .spans
        .iter()
        .filter(|s| s.style.fg == keyword().fg)
        .map(|s| s.content.as_ref())
        .collect();
    assert_eq!(coloured, "fn", "columns were read as bytes");
}
