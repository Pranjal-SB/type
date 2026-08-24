//! Editor-side halves of the architecture §4 keystroke budget. See
//! `typ-buffer/tests/perf.rs` for why these are `#[ignore]`d.
//!
//!     cargo test --release -p typ-panel-editor --test perf -- --ignored --nocapture

// The perf tests carry the same allocator swap as the binary, so what they
// measure is what ships. Without it the musl column measures mallocng, which
// no user of a released build ever runs. See crates/typ/src/main.rs.
#[cfg(all(target_env = "musl", target_pointer_width = "64"))]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_core::{Action, Motion, Panel, RenderContext, ThemeColors};
use typ_panel_editor::EditorPanel;

fn big_editor() -> EditorPanel {
    let line = "    let editor = Editor::new(); // a representative line of code\n";
    let text: String = std::iter::repeat_n(line, 50_000).collect();
    EditorPanel::from_str(&text)
}

const BUDGET_US: u128 = 16_000;

/// Perf tests run one at a time.
///
/// cargo runs tests in parallel threads inside one process, and a wall-clock
/// measurement taken while a sibling test is saturating another core is not a
/// measurement of anything. This is not hypothetical: adding two render
/// benchmarks here made `InsertChar` read **32 µs** against the 1.9 µs it
/// actually costs, a 20x phantom regression that took a bisect against v0.2.2
/// to disprove.
///
/// A mutex rather than a documented `--test-threads=1`, for the same reason the
/// clipboard tests carry one: an instruction in a doc comment is followed by
/// whoever read it, and the ordinary `cargo test` invocation must not be able to
/// produce a wrong number.
static EXCLUSIVE: Mutex<()> = Mutex::new(());

fn exclusive() -> MutexGuard<'static, ()> {
    // A panicking test poisons the lock; the data is `()` and the next test's
    // measurement is still valid, so recover rather than cascading failures.
    EXCLUSIVE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn typing_a_character_into_a_large_file_fits_in_a_frame() {
    let _guard = exclusive();
    let mut editor = big_editor();
    editor.perform(Action::InsertChar('x')); // warm

    let n = 20;
    let start = Instant::now();
    for _ in 0..n {
        editor.perform(Action::InsertChar('x'));
    }
    let per_key = start.elapsed() / n;
    println!("InsertChar: {per_key:?} per keystroke");
    assert!(
        per_key.as_micros() < BUDGET_US,
        "one keystroke cost {per_key:?}, over the 16ms budget"
    );
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn undo_and_redo_on_a_large_file_fit_in_a_frame() {
    let _guard = exclusive();
    let mut editor = big_editor();
    for _ in 0..20 {
        editor.perform(Action::InsertChar('x'));
        editor.perform(Action::Move {
            motion: Motion::Down,
            extend: false,
        });
    }

    let n = 20;
    let start = Instant::now();
    for _ in 0..n {
        editor.perform(Action::Undo);
        editor.perform(Action::Redo);
    }
    let per_pair = start.elapsed() / n;
    println!("Undo+Redo: {per_pair:?} per pair");
    assert!(
        per_pair.as_micros() < BUDGET_US * 2,
        "an undo/redo pair cost {per_pair:?}, over two frame budgets"
    );
}

/// Draw one frame of a 50k-line buffer, deep enough in that anything scaling
/// with scroll depth shows up.
///
/// M2.3 put three new things on this path — a gutter, a bracket search, and a
/// per-grapheme paint decision — and nothing here measured rendering at all
/// before that. Architecture §4 budgets *keystroke to painted glyph*, so a
/// keystroke measured without its repaint is half a number.
fn draw_frame(editor: &mut EditorPanel, area: Rect) {
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
    editor.render(area, &mut buf, &ctx);
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn drawing_a_frame_deep_in_a_large_file_fits_in_a_frame() {
    let _guard = exclusive();
    let mut editor = big_editor();
    let area = Rect::new(0, 0, 120, 40);

    // Scroll far in. The M0 finding was that the costs which matter here scale
    // with lines *above* the viewport, not with viewport size, so measuring at
    // the top of the file measures nothing.
    editor.perform(Action::Move {
        motion: Motion::DocumentEnd,
        extend: false,
    });
    draw_frame(&mut editor, area); // warm

    let n = 50;
    let start = Instant::now();
    for _ in 0..n {
        draw_frame(&mut editor, area);
    }
    let per_frame = start.elapsed() / n;
    println!("render, deep in a 50k-line file: {per_frame:?} per frame");
    assert!(
        per_frame.as_micros() < BUDGET_US,
        "one frame cost {per_frame:?}, over the 16ms budget"
    );
}

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn an_unmatched_bracket_does_not_walk_the_file() {
    let _guard = exclusive();
    // The pathological case for Task 3: the cursor sits on a bracket whose
    // partner does not exist, so the search runs to its bound every frame. If
    // the bound were ever removed this is the test that would notice.
    let mut editor = EditorPanel::from_str(&format!("(\n{}", "filler\n".repeat(50_000)));
    let area = Rect::new(0, 0, 120, 40);
    draw_frame(&mut editor, area); // warm

    let n = 50;
    let start = Instant::now();
    for _ in 0..n {
        draw_frame(&mut editor, area);
    }
    let per_frame = start.elapsed() / n;
    println!("render with an unmatched bracket at the cursor: {per_frame:?} per frame");
    assert!(
        per_frame.as_micros() < BUDGET_US,
        "one frame cost {per_frame:?}, over the 16ms budget"
    );
}
