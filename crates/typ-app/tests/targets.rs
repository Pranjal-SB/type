//! One list of mouse targets (interface §5): every clickable thing on screen,
//! with the region letter its warp label starts with and the cell a click on
//! it lands on. A click at each target's point does what clicking that thing
//! does, so a warp label that clicks there cannot drift from the mouse.

use std::path::PathBuf;

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use typ_app::app::targets::{Target, TargetKind};
use typ_app::prompt::PromptKind;
use typ_app::run::step;
use typ_app::status::SegmentId;
use typ_app::{App, Focus};
use typ_core::{Action, AppEvent, KeyChord};

const FRAME: Rect = Rect {
    x: 0,
    y: 0,
    width: 120,
    height: 30,
};

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-targets").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    for file in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(dir.join(file), "fn main() {}\n").unwrap();
    }
    dir
}

fn draw(app: &mut App) {
    let mut terminal = Terminal::new(TestBackend::new(FRAME.width, FRAME.height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
}

/// The dock open, two panes side by side: a.rs and b.rs in the left one,
/// b.rs active, and c.rs in the right one. The left pane current.
fn two_panes(name: &str) -> App {
    let dir = dir(name);
    let mut app = App::new(&dir).unwrap();
    app.open_path(&dir.join("a.rs")).unwrap();
    app.open_path(&dir.join("b.rs")).unwrap();
    app.apply_named_action(Action::SplitRight).unwrap();
    app.close_picker();
    app.open_path(&dir.join("c.rs")).unwrap();
    app.apply_named_action(Action::FocusPane(1)).unwrap();
    draw(&mut app);
    app
}

fn click(app: &mut App, (column, row): (u16, u16)) {
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    step(app, AppEvent::Input(Event::Mouse(event)), FRAME).unwrap();
}

/// The first target `wanted` picks, in screen order.
fn find(app: &App, wanted: impl Fn(&TargetKind) -> bool) -> Target {
    *app.targets(FRAME)
        .iter()
        .find(|t| wanted(&t.kind))
        .unwrap_or_else(|| panic!("no such target in {:#?}", app.targets(FRAME)))
}

#[test]
fn no_two_targets_overlap_and_each_point_is_inside_its_own() {
    let app = two_panes("overlap");
    let targets = app.targets(FRAME);
    assert!(targets.len() > 10, "{targets:#?}");
    for (i, a) in targets.iter().enumerate() {
        assert!(a.rect.contains(a.point.into()), "{a:?}");
        for b in &targets[i + 1..] {
            assert!(
                a.rect.intersection(b.rect).is_empty(),
                "{a:?} overlaps {b:?}"
            );
        }
    }
}

#[test]
fn each_kind_has_its_region_letter() {
    let app = two_panes("regions");
    for target in app.targets(FRAME) {
        let expected = match target.kind {
            TargetKind::Tab { .. } | TargetKind::PaneNumber(_) | TargetKind::PaneZoom(_) => 't',
            TargetKind::DockHeading
            | TargetKind::DockZoom
            | TargetKind::DockFolded
            | TargetKind::TreeRow(_) => 's',
            TargetKind::Segment(_) => 'b',
            TargetKind::MenuRow(_)
            | TargetKind::FloatRow(_)
            | TargetKind::BarGroup(_)
            | TargetKind::CompletionRow(_) => 'f',
        };
        assert_eq!(target.region(), expected, "{target:?}");
    }
}

#[test]
fn a_click_on_a_background_tab_shows_it() {
    let mut app = two_panes("tab");
    assert_eq!(app.active_tab(), 1);
    let tab = find(&app, |k| matches!(k, TargetKind::Tab { tab: 0, .. }));
    click(&mut app, tab.point);
    assert_eq!(app.current_pane(), 0);
    assert_eq!(app.active_tab(), 0);
}

#[test]
fn a_click_on_the_second_pane_number_focuses_it() {
    let mut app = two_panes("number");
    let numbers: Vec<Target> = app
        .targets(FRAME)
        .into_iter()
        .filter(|t| matches!(t.kind, TargetKind::PaneNumber(_)))
        .collect();
    assert_eq!(numbers.len(), 2);
    click(&mut app, numbers[1].point);
    assert_eq!(app.current_pane(), 1);
    assert_eq!(app.focus(), Focus::Editor);
}

#[test]
fn a_click_on_a_pane_zoom_zooms_it() {
    let mut app = two_panes("pane-zoom");
    let zoom = find(&app, |k| matches!(k, TargetKind::PaneZoom(_)));
    click(&mut app, zoom.point);
    assert!(
        app.status_right().contains("zoomed"),
        "{}",
        app.status_right()
    );
}

#[test]
fn a_click_on_the_dock_heading_focuses_the_dock_and_its_zoom_zooms_it() {
    let mut app = two_panes("heading");
    let heading = find(&app, |k| *k == TargetKind::DockHeading);
    click(&mut app, heading.point);
    assert_eq!(app.focus(), Focus::Tree);

    let mut app = two_panes("dock-zoom");
    let zoom = find(&app, |k| *k == TargetKind::DockZoom);
    click(&mut app, zoom.point);
    assert_eq!(app.focus(), Focus::Tree);
    assert!(
        app.status_right().contains("zoomed"),
        "{}",
        app.status_right()
    );
}

#[test]
fn a_click_on_a_tree_row_selects_it() {
    let mut app = two_panes("tree");
    let rows: Vec<Target> = app
        .targets(FRAME)
        .into_iter()
        .filter(|t| matches!(t.kind, TargetKind::TreeRow(_)))
        .collect();
    // sub/, a.rs, b.rs, c.rs
    assert_eq!(rows.len(), 4, "{rows:#?}");
    click(&mut app, rows[2].point);
    assert_eq!(app.focus(), Focus::Tree);
    assert!(
        app.tree_mut().selected().unwrap().ends_with("b.rs"),
        "{:?}",
        app.tree_mut().selected()
    );
}

#[test]
fn a_click_on_the_position_segment_opens_goto_line() {
    let mut app = two_panes("segment");
    let position = find(&app, |k| *k == TargetKind::Segment(SegmentId::Position));
    click(&mut app, position.point);
    assert_eq!(app.prompt().map(|p| p.kind()), Some(PromptKind::GotoLine));
}

#[test]
fn only_segments_that_do_something_are_targets() {
    let app = two_panes("quiet-segments");
    for target in app.targets(FRAME) {
        if let TargetKind::Segment(id) = target.kind {
            assert!(id.action().is_some(), "{id:?}");
        }
    }
}

#[test]
fn a_click_on_the_folded_dock_opens_it() {
    let mut app = two_panes("folded");
    app.apply_named_action(Action::ToggleLeftDock).unwrap();
    app.apply_named_action(Action::ToggleLeftDock).unwrap();
    assert!(!app.dock_open());
    assert!(
        !app.targets(FRAME)
            .iter()
            .any(|t| matches!(t.kind, TargetKind::TreeRow(_) | TargetKind::DockHeading)),
        "a folded dock shows no rows"
    );
    let folded = find(&app, |k| *k == TargetKind::DockFolded);
    click(&mut app, folded.point);
    assert!(app.dock_open());
}

#[test]
fn the_ctrl_k_menu_rows_are_targets_while_it_is_up() {
    let mut app = two_panes("menu");
    assert!(
        !app.targets(FRAME)
            .iter()
            .any(|t| matches!(t.kind, TargetKind::MenuRow(_)))
    );
    app.handle_chord(KeyChord::from_event(KeyEvent::new(
        KeyCode::Char('k'),
        KeyModifiers::CONTROL,
    )))
    .unwrap();
    draw(&mut app);
    let rows: Vec<Target> = app
        .targets(FRAME)
        .into_iter()
        .filter(|t| matches!(t.kind, TargetKind::MenuRow(_)))
        .collect();
    assert_eq!(
        rows.len(),
        app.keymap().under("ctrl+k").len() - 8,
        "nine digits are one row"
    );

    click(&mut app, rows[0].point);
    assert_eq!(app.pending_prefix(), None, "the click ran nothing");
}
