//! Lines mean docked, boxes mean floating (interface §0.3, §2).
//!
//! The panels draw their content and nothing around it; the app draws one rule
//! between the tree and the editor, and the tree's heading as a labelled rule.

use std::path::PathBuf;

use crossterm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_app::run::step;
use typ_app::{App, Focus};
use typ_core::{AppEvent, ThemeColors};

const FRAME: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

fn app(name: &str) -> App {
    let dir: PathBuf = std::env::temp_dir().join("typ-rules").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), "alpha\n").unwrap();
    std::fs::write(dir.join("b.rs"), "first\nsecond\nthird\n").unwrap();
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("b.rs")).unwrap();
    app
}

fn draw(app: &mut App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(FRAME.width, FRAME.height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    terminal.backend().buffer().clone()
}

fn row(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
}

fn click(x: u16, y: u16) -> AppEvent {
    AppEvent::Input(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    }))
}

#[test]
fn no_panel_draws_a_box() {
    let mut app = app("no-box");
    let buf = draw(&mut app);
    for y in 0..FRAME.height - 1 {
        let text = row(&buf, y);
        assert!(
            !text.contains(['┌', '┐', '└', '┘', '┬', '┴']),
            "row {y} has a box corner: {text}"
        );
    }
}

#[test]
fn one_rule_in_border_divides_the_tree_from_the_editor() {
    let theme = ThemeColors::default();
    let mut app = app("rule");
    let buf = draw(&mut app);
    let (tree, editor) = app.areas(FRAME);
    let rule = tree.right();
    assert_eq!(
        rule + 1,
        editor.x,
        "one column between the two, and only one"
    );
    for y in 0..FRAME.height - 1 {
        assert_eq!(buf[(rule, y)].symbol(), "│", "row {y}: {}", row(&buf, y));
        assert_eq!(buf[(rule, y)].fg, theme.border, "row {y}");
    }
}

#[test]
fn the_tree_heading_is_a_labelled_rule() {
    let theme = ThemeColors::default();
    let mut app = app("heading");
    let buf = draw(&mut app);
    let (tree, _) = app.areas(FRAME);
    let heading: String = (0..tree.width).map(|x| buf[(x, 0)].symbol()).collect();
    assert!(
        heading.starts_with("── heading ──"),
        "the heading reads {heading:?}"
    );
    assert!(
        heading.ends_with('─'),
        "the rule runs to the edge: {heading:?}"
    );
    assert_eq!(tree.y, 1, "the list starts under its heading");

    // Unfocused: the rule in `border`, the label receded.
    assert_eq!(buf[(0, 0)].fg, theme.border);
    assert_eq!(buf[(3, 0)].fg, theme.receded_fg);

    // Focused: the label and the rule before it are accent, the label bold.
    step(&mut app, click(2, 2), FRAME).unwrap();
    assert_eq!(app.focus(), Focus::Tree);
    let buf = draw(&mut app);
    assert_eq!(buf[(0, 0)].fg, theme.border_focused);
    assert_eq!(buf[(3, 0)].fg, theme.border_focused);
    assert!(
        buf[(3, 0)]
            .modifier
            .contains(ratatui::style::Modifier::BOLD)
    );
    let after = 3 + "heading".len() as u16 + 1;
    assert_eq!(buf[(after, 0)].fg, theme.border, "only the rule before it");
}

#[test]
fn the_first_content_cell_of_each_panel_is_line_zero_col_zero() {
    let mut app = app("first-cell");
    // Somewhere else first, so a click that lands nowhere is visible.
    let (tree, editor) = app.areas(FRAME);
    step(&mut app, click(editor.x + 30, editor.y + 2), FRAME).unwrap();
    assert_eq!(app.editor().cursor().line, 2);

    let buf = draw(&mut app);
    // The first text column is the first cell after the gutter.
    let text_x = (editor.x..editor.right())
        .find(|&x| buf[(x, editor.y)].symbol() == "f")
        .expect("line 0 is drawn on the editor's first row");
    step(&mut app, click(text_x, editor.y), FRAME).unwrap();
    let cursor = app.editor().cursor();
    assert_eq!((cursor.line, cursor.col), (0, 0));

    // The tree's first row is its first entry. The second row first: a click on
    // the row already selected opens it, and a fresh tree has entry 0 selected.
    step(&mut app, click(tree.x, tree.y + 1), FRAME).unwrap();
    assert!(app.tree_mut().selected().unwrap().ends_with("b.rs"));
    step(&mut app, click(tree.x, tree.y), FRAME).unwrap();
    assert_eq!(app.focus(), Focus::Tree);
    assert!(app.tree_mut().selected().unwrap().ends_with("a.rs"));
    let buf = draw(&mut app);
    assert!(
        row(&buf, tree.y).contains("a.rs"),
        "the first entry sits on the tree's first row: {}",
        row(&buf, tree.y)
    );
}
