//! Editor-side halves of the architecture §4 keystroke budget. See
//! `typ-buffer/tests/perf.rs` for why these are `#[ignore]`d.
//!
//!     cargo test --release -p typ-panel-editor --test perf -- --ignored --nocapture

use std::time::Instant;

use typ_core::{Action, Motion};
use typ_panel_editor::EditorPanel;

fn big_editor() -> EditorPanel {
    let line = "    let editor = Editor::new(); // a representative line of code\n";
    let text: String = std::iter::repeat_n(line, 50_000).collect();
    EditorPanel::from_str(&text)
}

const BUDGET_US: u128 = 16_000;

#[test]
#[ignore = "wall-clock budget; run with --release --ignored"]
fn typing_a_character_into_a_large_file_fits_in_a_frame() {
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
