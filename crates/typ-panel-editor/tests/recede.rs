//! An editor without focus recedes: one colour, no syntax (interface §4).

use std::sync::Arc;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use typ_core::{Panel, RenderContext, SyntaxTheme, ThemeColors};
use typ_panel_editor::EditorPanel;
use typ_syntax::{Language, Syntax};

const KEYWORD: Color = Color::Rgb(0xc0, 0x78, 0xdd);

fn paint(is_focused: bool) -> (Buffer, ThemeColors) {
    let theme = ThemeColors::default();
    // A real scope, so the focused frame takes the slow path it measures.
    let syntax: SyntaxTheme = [("keyword".to_string(), Style::default().fg(KEYWORD))]
        .into_iter()
        .collect();
    let text = "fn main() {}\n";
    let mut panel = EditorPanel::from_str(text);
    let rope = ropey::Rope::from_str(text);
    panel.set_syntax(1, Arc::new(Syntax::parse(Language::Rust, &rope).unwrap()));

    let area = Rect::new(0, 0, 30, 5);
    let ctx = RenderContext {
        theme: &theme,
        syntax: &syntax,
        diagnostics: &[],
        is_focused,
        panel_index: 1,
        terminal_width: 30,
        terminal_height: 5,
    };
    let mut buf = Buffer::empty(area);
    panel.render(area, &mut buf, &ctx);
    (buf, theme)
}

/// The column of the first `symbol` on row 1, the first line of text.
fn column_of(buf: &Buffer, symbol: &str) -> u16 {
    (0..buf.area.width)
        .find(|&x| buf[(x, 1)].symbol() == symbol)
        .expect("the symbol is on the first line")
}

#[test]
fn a_focused_editor_paints_syntax() {
    let (buf, theme) = paint(true);
    let f = column_of(&buf, "f");
    let m = column_of(&buf, "m");
    assert_eq!(buf[(f, 1)].fg, KEYWORD);
    assert_eq!(buf[(m, 1)].fg, theme.fg);
}

#[test]
fn an_unfocused_editor_drops_to_one_receded_colour() {
    let (buf, theme) = paint(false);
    let f = column_of(&buf, "f");
    let m = column_of(&buf, "m");
    assert_eq!(buf[(f, 1)].fg, theme.receded_fg, "syntax colour survived");
    assert_eq!(buf[(m, 1)].fg, theme.receded_fg, "body text did not recede");
}

#[test]
fn receding_keeps_the_cursor_line() {
    // Where the caret is stays said, focused or not.
    let (focused, _) = paint(true);
    let (unfocused, _) = paint(false);
    let m = column_of(&focused, "m");
    assert_eq!(unfocused[(m, 1)].bg, focused[(m, 1)].bg);
}
