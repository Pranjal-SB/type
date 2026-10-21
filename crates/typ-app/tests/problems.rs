//! The bottom dock and its first panel, problems: every diagnostic of every
//! open document as an excerpt of its buffer, grouped by file, the files with
//! errors first. `ctrl+j` opens, focuses and folds the dock; `enter` or a
//! click jumps to a problem; `f8` walks the same order.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use typ_app::app::targets::TargetKind;
use typ_app::lsp::ServerConfig;
use typ_app::run::{AppReceiver, channel, step_batch};
use typ_app::{App, Focus};
use typ_buffer::Position;
use typ_core::AppEvent;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 80,
    height: 24,
};

const WAIT: Duration = Duration::from_secs(10);

/// Warnings only: listed after the file with an error, though it sorts first.
const A: &str = "meh();\nfn a() {}\n";
/// An error on its second line, a warning on its third.
const B: &str = "fn b() {}\nbad();\nmeh();\n";

fn fake() -> &'static str {
    env!("CARGO_BIN_EXE_typ-app-fake-server")
}

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("typ-problems").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.rs"), A).unwrap();
    std::fs::write(dir.join("b.rs"), B).unwrap();
    dir
}

fn server() -> ServerConfig {
    ServerConfig {
        language_id: "rust".into(),
        extensions: vec!["rs".into()],
        command: fake().into(),
        args: vec!["--lint".into()],
        roots: vec!["Cargo.toml".into()],
    }
}

fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    let event = AppEvent::Input(Event::Key(KeyEvent::new(code, mods)));
    step_batch(app, vec![event], AREA).unwrap();
}

fn ctrl_j(app: &mut App) {
    press(app, KeyCode::Char('j'), KeyModifiers::CONTROL);
}

fn click(app: &mut App, (column, row): (u16, u16)) {
    let event = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    };
    step_batch(app, vec![AppEvent::Input(Event::Mouse(event))], AREA).unwrap();
}

fn pump_until(app: &mut App, rx: &AppReceiver, done: impl Fn(&App) -> bool) -> bool {
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        if done(app) {
            return true;
        }
        if let Ok(event) = rx.recv_timeout(Duration::from_millis(250)) {
            let mut batch = vec![event];
            batch.extend(rx.try_iter());
            step_batch(app, batch, AREA).unwrap();
        }
    }
    done(app)
}

fn draw(app: &mut App) -> Vec<String> {
    draw_at(app, AREA.width, AREA.height)
}

fn draw_at(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| app.render(frame)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn at(path: &Path, line: usize) -> (PathBuf, Position) {
    (path.to_path_buf(), Position { line, col: 0 })
}

/// Both files open, `b.rs` on screen, and all three problems published.
fn ready(name: &str) -> (App, AppReceiver, PathBuf) {
    let dir = fixture(name);
    let (tx, rx) = channel();
    let mut app = App::new(&dir).unwrap();
    app.add_language_server(server());
    app.set_event_sender(tx);
    app.open_path(&dir.join("a.rs")).unwrap();
    app.open_in_new_tab(&dir.join("b.rs")).unwrap();
    app.set_focus(Focus::Editor);
    assert!(
        pump_until(&mut app, &rx, |a| a.problems().len() == 3),
        "the problems never arrived: {:?}",
        app.problems()
    );
    (app, rx, dir)
}

#[test]
fn three_problems_in_two_files_list_as_three_excerpts_errors_first() {
    let (mut app, _rx, dir) = ready("listed");
    let (a, b) = (dir.join("a.rs"), dir.join("b.rs"));
    assert_eq!(app.problems(), [at(&b, 1), at(&b, 2), at(&a, 0)]);

    ctrl_j(&mut app);
    // Thirteen rows of list: a tall frame, and the dock's rule moved up four
    // in resize mode to make room. Read from the rule down, since the tab row
    // above names both files too.
    let tall = Rect::new(0, 0, 80, 40);
    let key = |code, mods| AppEvent::Input(Event::Key(KeyEvent::new(code, mods)));
    let mut resize = vec![
        key(KeyCode::Char('k'), KeyModifiers::CONTROL),
        key(KeyCode::Char('='), KeyModifiers::NONE),
    ];
    resize.extend((0..4).map(|_| key(KeyCode::Up, KeyModifiers::NONE)));
    resize.push(key(KeyCode::Enter, KeyModifiers::NONE));
    step_batch(&mut app, resize, tall).unwrap();
    let rows = draw_at(&mut app, 80, 40);
    let heading = rows
        .iter()
        .position(|row| row.contains("problems 3"))
        .expect("no labelled rule");
    let screen = rows[heading..].join("\n");
    let place = |needle: &str| {
        screen
            .find(needle)
            .unwrap_or_else(|| panic!("{needle:?} is not on screen:\n{screen}"))
    };
    assert!(place("b.rs") < place("a.rs"), "{screen}");
    // Each excerpt is its line and one either side, read from the buffer.
    assert!(place("fn b() {}") < place("bad();"));
    assert!(place("bad();") < place("meh();"));
    assert!(screen.contains("fn a() {}"), "{screen}");
}

#[test]
fn a_click_on_the_second_problem_jumps_to_it() {
    let (mut app, _rx, _dir) = ready("click");
    ctrl_j(&mut app);
    draw(&mut app);
    let second = app
        .targets(AREA)
        .into_iter()
        .find(|t| t.kind == TargetKind::ProblemRow(1))
        .expect("the second problem is not a target");
    click(&mut app, second.point);
    assert_eq!(app.focus(), Focus::Editor);
    assert_eq!(app.editor().cursor(), Position { line: 2, col: 0 });
    assert_eq!(app.editor().line_text(2), "meh();");
}

#[test]
fn enter_in_the_dock_jumps_to_the_chosen_problem() {
    let (mut app, _rx, dir) = ready("enter");
    ctrl_j(&mut app);
    draw(&mut app);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Down, KeyModifiers::NONE);
    press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
    assert_eq!(app.focus(), Focus::Editor);
    assert_eq!(app.editor().path(), Some(dir.join("a.rs").as_path()));
    assert_eq!(app.editor().cursor(), Position { line: 0, col: 0 });
}

#[test]
fn a_fix_takes_its_excerpt_away_on_the_next_publish() {
    let (mut app, rx, dir) = ready("fix");
    let b = dir.join("b.rs");
    app.editor_mut().goto(1, 0);
    for _ in 0..3 {
        press(&mut app, KeyCode::Delete, KeyModifiers::NONE);
    }
    for c in "ok".chars() {
        press(&mut app, KeyCode::Char(c), KeyModifiers::NONE);
    }
    assert_eq!(app.editor().line_text(1), "ok();");
    assert!(
        pump_until(&mut app, &rx, |a| a.problems().len() == 2),
        "{:?}",
        app.problems()
    );
    assert!(!app.problems().contains(&at(&b, 1)));
    ctrl_j(&mut app);
    assert!(!draw(&mut app).join("\n").contains("bad();"));
}

#[test]
fn f8_walks_the_same_order_across_files() {
    let (mut app, _rx, dir) = ready("f8");
    let (a, b) = (dir.join("a.rs"), dir.join("b.rs"));
    app.editor_mut().goto(0, 0);
    let here = |app: &App| {
        (
            app.editor().path().unwrap().to_path_buf(),
            app.editor().cursor(),
        )
    };
    for expected in [at(&b, 1), at(&b, 2), at(&a, 0), at(&b, 1)] {
        press(&mut app, KeyCode::F(8), KeyModifiers::NONE);
        assert_eq!(here(&app), expected);
    }
}

#[test]
fn ctrl_j_opens_and_focuses_then_focuses_then_folds() {
    let (mut app, _rx, _dir) = ready("walk");
    assert!(!app.bottom_dock_open());
    ctrl_j(&mut app);
    assert!(app.bottom_dock_open());
    assert_eq!(app.focus(), Focus::BottomDock);
    press(&mut app, KeyCode::Esc, KeyModifiers::NONE);
    assert_eq!(app.focus(), Focus::Editor);
    assert!(app.bottom_dock_open(), "esc folded the dock");
    ctrl_j(&mut app);
    assert_eq!(app.focus(), Focus::BottomDock);
    ctrl_j(&mut app);
    assert!(!app.bottom_dock_open());
    assert_eq!(app.focus(), Focus::Editor);
}

#[test]
fn the_docks_and_the_panes_tile_the_body() {
    let (mut app, _rx, _dir) = ready("tiling");
    ctrl_j(&mut app);
    app.set_focus(Focus::Editor);
    for (width, height) in [(80, 24), (120, 40), (40, 12), (100, 8)] {
        let frame = Rect::new(0, 0, width, height);
        let body = Rect::new(0, 0, width, height - 1);
        let mut rects = app.dock_rects(frame);
        rects.extend(app.pane_rects(frame));
        let rects: Vec<Rect> = rects.into_iter().filter(|r| !r.is_empty()).collect();
        let covered: u32 = rects.iter().map(|r| r.area()).sum();
        assert_eq!(covered, body.area(), "{width}x{height}: {rects:?}");
        for (i, r) in rects.iter().enumerate() {
            assert_eq!(
                r.intersection(body),
                *r,
                "{width}x{height}: {r:?} leaves the body"
            );
            for other in &rects[i + 1..] {
                assert!(
                    r.intersection(*other).is_empty(),
                    "{width}x{height}: {r:?} overlaps {other:?}"
                );
            }
        }
    }
}
