//! What damage-driven redraw is worth, measured rather than asserted.
//!
//! See `typ-panel-editor/tests/perf.rs` for why these are `#[ignore]`d and why
//! they take a mutex.
//!
//!     cargo test --release -p typ-app --test perf -- --ignored --nocapture

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use typ_app::App;
use typ_app::run::{step, step_batch};
use typ_core::AppEvent;

const BUDGET_US: u128 = 16_000;

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 120,
    height: 40,
};

/// Perf tests run one at a time. A wall-clock number taken while a sibling
/// saturates another core measures the scheduler.
static EXCLUSIVE: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    EXCLUSIVE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// An app over a 50k-line file, which is the size every budget in
/// architecture §4 is stated against.
fn big_app(name: &str) -> App {
    let dir = std::env::temp_dir().join("typ-app-perf").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let line = "    let editor = Editor::new(); // a representative line of code\n";
    let text: String = std::iter::repeat_n(line, 50_000).collect();
    let file = dir.join("big.rs");
    std::fs::write(&file, text).unwrap();

    let mut app = App::new(&dir).unwrap();
    app.open_path(&file).unwrap();
    app
}

fn draw(app: &mut App, terminal: &mut Terminal<TestBackend>) {
    terminal.draw(|frame| app.render(frame)).unwrap();
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn an_idle_wakeup_costs_nothing_against_a_frame() {
    let _guard = exclusive();
    let mut app = big_app("idle");
    let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).unwrap();
    draw(&mut app, &mut terminal); // warm
    app.take_dirty(); // opening the file marked it; that frame is now painted

    // A watcher reporting a file this app does not have open: the shape of
    // every wakeup that turns out to have nothing behind it.
    let idle = AppEvent::FileChanged(PathBuf::from("/somewhere/else.rs"));

    let n = 200;
    let start = Instant::now();
    for _ in 0..n {
        step(&mut app, idle.clone(), AREA).unwrap();
    }
    let per_wakeup = start.elapsed() / n;

    // Before the frame loop below, which marks dirty on purpose: asking after
    // it would be asking about that, not about the wakeups.
    assert!(
        !app.take_dirty(),
        "an idle wakeup asked for a repaint, so the ratio below is a lie"
    );

    let frames = 20;
    let start = Instant::now();
    for _ in 0..frames {
        app.mark_dirty();
        draw(&mut app, &mut terminal);
    }
    let per_frame = start.elapsed() / frames;

    println!("idle wakeup: {per_wakeup:?}");
    println!("frame:       {per_frame:?}");
    println!(
        "ratio:       {:.0}x",
        per_frame.as_secs_f64() / per_wakeup.as_secs_f64().max(f64::EPSILON)
    );

    assert!(
        per_wakeup.as_micros() < BUDGET_US,
        "idle wakeup {per_wakeup:?} exceeds the {BUDGET_US} µs frame budget"
    );
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn a_burst_of_events_costs_one_frame() {
    let _guard = exclusive();
    let mut app = big_app("burst");
    let mut terminal = Terminal::new(TestBackend::new(AREA.width, AREA.height)).unwrap();
    draw(&mut app, &mut terminal); // warm
    app.take_dirty();

    // A paste, a held key, or a run of parse completions at M2.5: thirty
    // events queued behind one wakeup.
    let burst: Vec<AppEvent> = (0..30)
        .map(|_| {
            AppEvent::Input(Event::Key(KeyEvent::new(
                KeyCode::Char('x'),
                KeyModifiers::NONE,
            )))
        })
        .collect();

    let start = Instant::now();
    step_batch(&mut app, burst, AREA).unwrap();
    let dispatch = start.elapsed();

    let start = Instant::now();
    let drew = app.take_dirty();
    if drew {
        draw(&mut app, &mut terminal);
    }
    let frame = start.elapsed();

    println!("30 events dispatched: {dispatch:?}");
    println!("frames drawn for it:  {}", u8::from(drew));
    println!("that frame:           {frame:?}");

    assert!(drew, "a burst of edits drew nothing");
    assert!(!app.take_dirty(), "the batch asked for more than one frame");
    assert!(
        (dispatch + frame).as_micros() < BUDGET_US * 30,
        "a 30-event burst cost more than 30 frames' worth of budget"
    );
}
