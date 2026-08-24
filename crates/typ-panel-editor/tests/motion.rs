use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_buffer::{Position, Selection};
use typ_core::{Action, Motion, Panel, PanelEvent, RenderContext, ThemeColors};
use typ_panel_editor::EditorPanel;

fn pos(line: usize, col: usize) -> Position {
    Position { line, col }
}

fn mv(motion: Motion) -> Action {
    Action::Move {
        motion,
        extend: false,
    }
}

fn extend(motion: Motion) -> Action {
    Action::Move {
        motion,
        extend: true,
    }
}

/// Panels learn their height at render time; page motions need one frame.
fn render(panel: &mut EditorPanel, area: Rect) {
    let theme = ThemeColors::default();
    let ctx = RenderContext {
        theme: &theme,
        syntax: typ_core::SyntaxTheme::empty(),
        is_focused: true,
        panel_index: 0,
        terminal_width: area.width,
        terminal_height: area.height,
    };
    let mut buf = Buffer::empty(area);
    panel.render(area, &mut buf, &ctx);
}

#[test]
fn moving_right_advances_the_caret() {
    let mut p = EditorPanel::from_str("abc\n");
    p.apply_action(mv(Motion::Right));
    assert_eq!(p.cursor(), pos(0, 1));
    assert!(p.selections().primary().is_empty());
}

#[test]
fn extending_right_leaves_the_anchor_behind() {
    let mut p = EditorPanel::from_str("abc\n");
    p.apply_action(extend(Motion::Right));
    let s = p.selections().primary();
    assert_eq!(s.anchor, pos(0, 0));
    assert_eq!(s.head, pos(0, 1));
}

#[test]
fn a_plain_move_collapses_an_existing_selection_to_its_far_end() {
    let mut p = EditorPanel::from_str("abcdef\n");
    p.set_selections_for_test(vec![Selection {
        anchor: pos(0, 1),
        head: pos(0, 4),
    }]);
    p.apply_action(mv(Motion::Right));
    // Collapse to the far edge and stop there — the keypress is spent
    // dismissing the selection. Moving on to column 5 as well would skip a
    // character, and every GUI editor collapses without advancing.
    assert_eq!(p.cursor(), pos(0, 4));
    assert!(p.selections().primary().is_empty());
}

#[test]
fn moving_left_out_of_a_selection_collapses_to_its_near_end() {
    let mut p = EditorPanel::from_str("abcdef\n");
    p.set_selections_for_test(vec![Selection {
        anchor: pos(0, 1),
        head: pos(0, 4),
    }]);
    p.apply_action(mv(Motion::Left));
    assert_eq!(
        p.cursor(),
        pos(0, 1),
        "the near edge, without moving further"
    );
    assert!(p.selections().primary().is_empty());
}

#[test]
fn moving_right_at_the_end_of_a_line_wraps_to_the_next() {
    let mut p = EditorPanel::from_str("ab\ncd\n");
    p.apply_action(mv(Motion::LineEnd));
    p.apply_action(mv(Motion::Right));
    assert_eq!(p.cursor(), pos(1, 0));
}

#[test]
fn word_motion_stops_at_punctuation_runs() {
    let mut p = EditorPanel::from_str("foo::bar\n");
    p.apply_action(mv(Motion::WordRight));
    assert_eq!(p.cursor(), pos(0, 3));
    p.apply_action(mv(Motion::WordRight));
    assert_eq!(p.cursor(), pos(0, 5));
}

#[test]
fn word_motion_crosses_a_line_when_the_line_is_exhausted() {
    let mut p = EditorPanel::from_str("foo\nbar\n");
    p.apply_action(mv(Motion::LineEnd));
    p.apply_action(mv(Motion::WordRight));
    assert_eq!(p.cursor(), pos(1, 0));
}

#[test]
fn document_motions_reach_both_ends() {
    let mut p = EditorPanel::from_str("a\nb\nc\n");
    p.apply_action(mv(Motion::DocumentEnd));
    assert_eq!(
        p.cursor().line,
        3,
        "the trailing newline makes a final empty line"
    );
    p.apply_action(mv(Motion::DocumentStart));
    assert_eq!(p.cursor(), pos(0, 0));
}

#[test]
fn vertical_motion_remembers_the_goal_column() {
    let mut p = EditorPanel::from_str("abcdef\nab\nabcdef\n");
    p.apply_action(mv(Motion::LineEnd));
    assert_eq!(p.cursor(), pos(0, 6));
    p.apply_action(mv(Motion::Down));
    assert_eq!(p.cursor(), pos(1, 2), "clamped to the short line");
    p.apply_action(mv(Motion::Down));
    assert_eq!(p.cursor(), pos(2, 6), "the goal column is restored");
}

#[test]
fn a_horizontal_motion_forgets_the_goal_column() {
    let mut p = EditorPanel::from_str("abcdef\nab\nabcdef\n");
    p.apply_action(mv(Motion::LineEnd));
    p.apply_action(mv(Motion::Down)); // (1, 2), goal 6
    p.apply_action(mv(Motion::Left)); // (1, 1), goal cleared
    p.apply_action(mv(Motion::Down));
    assert_eq!(p.cursor(), pos(2, 1), "the new column is the goal now");
}

#[test]
fn page_motions_move_by_the_visible_height() {
    let text = (0..100).map(|i| format!("line {i}\n")).collect::<String>();
    let mut p = EditorPanel::from_str(&text);
    render(&mut p, Rect::new(0, 0, 40, 12)); // 12 rows minus the border = 10
    p.apply_action(mv(Motion::PageDown));
    assert_eq!(p.cursor().line, 10);
    p.apply_action(mv(Motion::PageUp));
    assert_eq!(p.cursor().line, 0);
}

#[test]
fn a_motion_applies_to_every_selection() {
    let mut p = EditorPanel::from_str("abc\ndef\n");
    p.set_selections_for_test(vec![
        Selection::caret(pos(0, 0)),
        Selection::caret(pos(1, 0)),
    ]);
    p.apply_action(mv(Motion::Right));
    let heads: Vec<Position> = p.selections().iter().map(|s| s.head).collect();
    assert_eq!(heads, vec![pos(0, 1), pos(1, 1)]);
}

#[test]
fn a_motion_requests_a_redraw() {
    let mut p = EditorPanel::from_str("abc\n");
    assert_eq!(
        p.apply_action(mv(Motion::Right)),
        Some(vec![PanelEvent::NeedsRedraw])
    );
}

#[test]
fn an_action_the_editor_does_not_handle_is_declined() {
    let mut p = EditorPanel::from_str("abc\n");
    // Save belongs to the app, not the panel.
    assert_eq!(p.apply_action(Action::Save), None);
}
