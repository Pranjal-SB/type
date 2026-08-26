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

/// An app with `n` files open, each a real buffer, and the workers wired up.
///
/// **The sender is not optional here.** Without it `rewatch` returns on its
/// first line and `request_parse_if_stale` on its third, so a tab switch
/// measures an index assignment and reports 27 ns — which is what the first
/// draft of this benchmark did. Both are the expensive halves of a switch, and
/// `rewatch` is the one that talks to the OS.
///
/// The receiver comes back so the caller can hold it: dropping it makes every
/// worker send fail, which is a different early return in the same place.
///
/// Files are written before anything is timed. Twenty small writes is not the
/// ten thousand that contaminated M2.8's ranking benchmark through the page
/// cache, but the fixture stays outside every measured region regardless.
fn app_with_tabs(name: &str, n: usize) -> (App, typ_app::run::AppReceiver) {
    let dir = std::env::temp_dir().join("typ-app-perf-tabs").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let line = "    let editor = Editor::new(); // a representative line of code\n";
    let text: String = std::iter::repeat_n(line, 400).collect();
    let (tx, rx) = typ_app::run::channel();
    let mut app = App::new(&dir).unwrap();
    app.set_event_sender(tx);
    for i in 0..n {
        let file = dir.join(format!("file{i}.rs"));
        std::fs::write(&file, &text).unwrap();
        app.open_path(&file).unwrap();
    }
    assert_eq!(app.tab_count(), n);
    (app, rx)
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored --nocapture"]
fn switching_tabs_stays_inside_a_keystroke() {
    // Twenty open files, and the switch does more than move an index: it
    // restamps the MRU, reapplies the config, rewatches the file and asks
    // whether the buffer needs parsing. Invariant 7 says none of that may block
    // the render thread, and `rewatch` is the one that talks to the OS.
    let _guard = exclusive();
    let (mut app, _rx) = app_with_tabs("switch", 20);

    let n = 100;
    let start = Instant::now();
    for _ in 0..n {
        app.next_tab();
    }
    let per_switch = start.elapsed() / n;

    println!("tab switch (20 open): {per_switch:?}");
    assert!(
        per_switch.as_micros() < BUDGET_US,
        "a tab switch costs {per_switch:?}, over the {BUDGET_US} µs keystroke budget"
    );
}

// **What that number is, measured rather than guessed.** 640 µs a switch, and a
// throwaway probe put `watch_file` plus its drop at 909 µs on its own — so the
// whole cost is the OS file watcher being torn down and rebuilt, synchronously,
// on the render thread.
//
// It passes at four percent of the keystroke budget, and it is left alone: the
// milestone that watches a workspace rather than a file replaces this path
// entirely, so tuning the single-file version now is work thrown away. Recorded
// because "switching tabs does filesystem I/O on the render thread" is exactly
// the sentence invariant 7 is written against, and nobody should have to
// rediscover it with a profiler.

#[test]
#[ignore = "wall-clock budget; run with --release --ignored --nocapture"]
fn drawing_the_tab_bar_stays_inside_a_frame_however_many_are_open() {
    // **Not "proportional to the visible cells", which is what an earlier
    // version of this test was named.** It is not: `cells` measures every label
    // to find its window and `first_visible` rescans from each candidate start,
    // so the cost does grow with the number of files open — 4.5 µs at two tabs
    // against 128 µs at two hundred, which is 28x for 100x the tabs.
    //
    // Left alone deliberately. 128 µs is under one percent of a frame, nobody
    // has two hundred files open, and caching the widths would put a second
    // source of truth about the labels beside the labels. The number is the
    // budget; the shape is recorded so the next person does not have to
    // rediscover it.
    let _guard = exclusive();
    let theme = typ_core::ThemeColors::default();
    let area = Rect::new(0, 0, 120, 1);

    let measure = |count: usize| {
        let labels: Vec<String> = (0..count).map(|i| format!("file{i}.rs")).collect();
        let mut buf = ratatui::buffer::Buffer::empty(area);
        let n = 2_000;
        let start = Instant::now();
        for _ in 0..n {
            typ_app::tabbar::draw(&mut buf, area, &labels, count - 1, &theme);
        }
        start.elapsed() / n
    };

    let few = measure(2);
    let many = measure(200);
    println!("bar,   2 tabs: {few:?}");
    println!("bar, 200 tabs: {many:?}");
    println!(
        "ratio:         {:.1}x",
        many.as_secs_f64() / few.as_secs_f64().max(f64::EPSILON)
    );

    assert!(
        many.as_micros() < BUDGET_US,
        "drawing the bar with 200 tabs costs {many:?}, over the frame budget"
    );
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored --nocapture"]
fn ranking_the_palette_stays_inside_a_keystroke() {
    // The one corpus deliberately ranked on the render thread rather than on
    // the worker, on the grounds that sixty static names are cheaper than the
    // round trip. That is a claim, so it gets a number.
    let _guard = exclusive();
    let mut app = big_app("palette");

    // Every prefix of a real command name, which is what typing one looks
    // like. Built rather than written out: the intermediate prefixes are
    // misspelled words to a spell checker, and CI runs one.
    const WORD: &str = "select";
    let queries: Vec<&str> = (0..=WORD.len()).map(|n| &WORD[..n]).collect();

    let n = 200;
    let start = Instant::now();
    for _ in 0..n {
        for query in &queries {
            app.open_command_palette();
            for c in query.chars() {
                app.handle_chord(typ_core::KeyChord::from_event(KeyEvent::new(
                    KeyCode::Char(c),
                    KeyModifiers::NONE,
                )))
                .unwrap();
            }
        }
    }
    let per_keystroke = start.elapsed() / (n * queries.len() as u32);

    println!("palette open + query: {per_keystroke:?}");
    assert!(
        per_keystroke.as_micros() < BUDGET_US,
        "ranking the palette costs {per_keystroke:?}, over the {BUDGET_US} µs keystroke budget"
    );
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
