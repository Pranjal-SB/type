# M0 (Feel Spike) + M1 (Walking Skeleton) — Implementation Plan

**How to use this plan:** tasks are ordered and each ends with a commit. Work them in
sequence; each one leaves the tree in a working, testable state. Checkboxes track progress.

**Goal:** Prove the terminal can deliver the feel TYPE requires (M0), then build a vertical
slice through the real architecture — event loop, `Panel` trait, editor panel, file tree
panel (M1).

**Architecture:** M0 is a deliberately throwaway single-crate spike under `spikes/m0-feel/`,
existing only to produce measurements and a go/no-go decision. M1 starts the real Cargo
workspace: `typ-core` defines the `Panel` trait and event vocabulary, `typ-buffer` owns text,
`typ-app` owns the event loop and dispatch, and two panels prove the contract works for more
than one implementation.

**Tech stack:** Rust 1.96 (edition 2024) · ratatui 0.30.2 · crossterm 0.29 · ropey 1.6 ·
tree-sitter 0.26 + tree-sitter-rust 0.24 · unicode-width 0.2 · unicode-segmentation 1.11 ·
anyhow 1.0

## Global constraints

- **Binary name is `typ`**, crate name is `typ-editor`, project name is TYPE. Never ship a
  binary named `type` — it collides with the POSIX shell builtin and would be unrunnable.
- **Per-file cap: 800 lines.** If a file approaches it, split by responsibility first.
- **Nothing blocks the render thread.** I/O, parsing, and subprocess work happen off-thread
  and return as events.
- **Panels never receive `&AppState`.** They get `RenderContext` and return `Vec<PanelEvent>`.
- **`PanelEvent` stays small.** New viewers register a handler in `typ-registry` and route
  through `OpenWith`; they do not add enum variants.
- **`$EDITOR` invariants hold from M1 onward:** `typ <file>` opens exactly that file, blocks
  until closed, exits clean, and returns honest exit codes. No daemon detach in this mode.
- **Mouse and keyboard are peers.** Every interaction works both ways.
- **Conventional Commits.** Single author — no co-author trailers.

---

## File structure

### M0 — throwaway spike (`spikes/m0-feel/`)

| File | Responsibility |
|---|---|
| `Cargo.toml` | Standalone crate, not a workspace member |
| `src/main.rs` | Terminal setup/teardown, event loop, wiring only |
| `src/width.rs` | Grapheme ↔ display-column mapping (carries into M1) |
| `src/viewport.rs` | Scroll state and visible-line calculation |
| `src/highlight.rs` | tree-sitter parse + per-line highlight spans |
| `src/metrics.rs` | Frame timing histogram, synchronized-output toggle |
| `tests/width.rs` | CJK/emoji/tab column correctness |
| `FINDINGS.md` | Measurements and go/no-go verdict — the actual deliverable |

### M1 — real workspace

| File | Responsibility |
|---|---|
| `Cargo.toml` | Workspace root, shared dependency versions |
| `crates/typ-core/src/panel.rs` | `Panel` trait, `RenderContext`, `ThemeColors` |
| `crates/typ-core/src/event.rs` | `PanelEvent`, `PanelId`, `HandlerId`, `NotifyLevel` |
| `crates/typ-core/src/key.rs` | `KeyChord` and canonical key naming |
| `crates/typ-buffer/src/buffer.rs` | Rope-backed text buffer, load/save/edit |
| `crates/typ-buffer/src/position.rs` | `Position`, grapheme ↔ display column |
| `crates/typ-buffer/src/undo.rs` | Undo/redo stack |
| `crates/typ-registry/src/lib.rs` | Extension → `HandlerId` table |
| `crates/typ-panel-editor/src/lib.rs` | Editor panel |
| `crates/typ-panel-tree/src/lib.rs` | File tree panel |
| `crates/typ-app/src/app.rs` | Panel registry, focus, event dispatch |
| `crates/typ-app/src/layout.rs` | Splits panel area between tree and editor |
| `crates/typ-app/src/run.rs` | Event loop, terminal lifecycle, input coalescing |
| `crates/typ/src/main.rs` | CLI parsing, `$EDITOR` invariants, exit codes |

---

# Milestone 0 — Feel Spike

**This code is deleted after M0.** Its only durable outputs are `FINDINGS.md`, the go/no-go
decision, and `src/width.rs` (promoted into `typ-buffer` at M1). Do not build architecture here.

---

### Task 1: Spike scaffold with mouse capture and clean teardown

**Files:**
- Create: `spikes/m0-feel/Cargo.toml`, `spikes/m0-feel/src/main.rs`, `spikes/m0-feel/.gitignore`

**Interfaces:**
- Consumes: nothing
- Produces: a binary that enters the alternate screen with mouse capture on, redraws on
  events, and restores the terminal on `q` or `Ctrl+C`

- [x] **Step 1: Create the spike crate manifest**

`spikes/m0-feel/Cargo.toml`:

```toml
[package]
name = "m0-feel"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
ratatui = "0.30.2"
crossterm = "0.29"
ropey = "1.6"
unicode-width = "0.2"
unicode-segmentation = "1.11"
anyhow = "1.0"
tree-sitter = "0.26"
tree-sitter-rust = "0.24"

[profile.release]
opt-level = 3
lto = "thin"
debug = 1
```

`spikes/m0-feel/.gitignore`:

```
target/
big.rs
wide.txt
```

`opt-level = 3` and `debug = 1`, not `opt-level = "z"`. Measurements must reflect a
speed-optimized build, and symbols are kept so profiling works.

- [x] **Step 2: Write main.rs with terminal lifecycle and mouse capture**

`spikes/m0-feel/src/main.rs`:

```rust
use std::io::stdout;

use anyhow::Result;
use crossterm::ExecutableCommand;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
};
use ratatui::DefaultTerminal;

fn main() -> Result<()> {
    // ratatui::init() enables raw mode + alternate screen and installs a panic
    // hook, but it does NOT enable mouse capture. That is on us.
    let mut terminal = ratatui::init();
    stdout().execute(EnableMouseCapture)?;

    let result = run(&mut terminal);

    stdout().execute(DisableMouseCapture)?;
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut last_event = String::from("(none)");

    loop {
        terminal.draw(|frame| {
            let text = format!("m0-feel spike\nlast event: {last_event}\nq to quit");
            frame.render_widget(text.as_str(), frame.area());
        })?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let quit = key.code == KeyCode::Char('q')
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL));
                if quit {
                    return Ok(());
                }
                last_event = format!("{:?} {:?}", key.code, key.modifiers);
            }
            Event::Mouse(m) => {
                last_event = format!("{:?} at ({}, {})", m.kind, m.column, m.row);
            }
            Event::Resize(w, h) => {
                last_event = format!("resize {w}x{h}");
            }
            _ => {}
        }
    }
}
```

- [x] **Step 3: Verify it builds and runs**

Run: `cargo run --release --manifest-path spikes/m0-feel/Cargo.toml`

Expected: alternate screen appears. Moving and clicking the mouse updates `last event` with
mouse kinds and coordinates. Pressing `q` exits and **the terminal is left in a working
state** — the prompt behaves normally, typing echoes, no stray escape sequences.

If the terminal is left broken after exit, stop and fix teardown before continuing.
Everything downstream depends on this being correct.

- [x] **Step 4: Commit**

```bash
git add spikes/m0-feel
git commit -m "feat(spike): m0 scaffold with mouse capture and clean teardown"
```

---

### Task 2: Grapheme ↔ display-column mapping, with tests

The highest-risk pure function in the project. Column drift on CJK and emoji is a daily
correctness bug in an editor, not an edge case, and mouse-click-to-cursor depends on it. So
it gets tested first and properly.

**Result: 9/9 pass on stock `unicode-width` 0.2.** TermIDE ships a `[patch.crates-io]` fork,
which made a fork look likely here. It is not needed — their patch predates upstream fixes.

**Files:**
- Create: `spikes/m0-feel/src/width.rs`, `spikes/m0-feel/tests/width.rs`

**Interfaces:**
- Consumes: nothing
- Produces:
  - `width::display_width(s: &str) -> usize`
  - `width::grapheme_to_display_col(line: &str, grapheme_idx: usize, tab_width: usize) -> usize`
  - `width::display_to_grapheme_col(line: &str, display_col: usize, tab_width: usize) -> usize`

- [x] **Step 1: Write the failing tests**

`spikes/m0-feel/tests/width.rs`:

```rust
use m0_feel::width::{display_to_grapheme_col, display_width, grapheme_to_display_col};

#[test]
fn ascii_width_is_one_per_char() {
    assert_eq!(display_width("hello"), 5);
}

#[test]
fn cjk_chars_are_two_columns_wide() {
    assert_eq!(display_width("日本語"), 6);
}

#[test]
fn emoji_is_two_columns_wide() {
    assert_eq!(display_width("🦀"), 2);
}

#[test]
fn combining_marks_do_not_add_width() {
    // "e" + combining acute accent renders as one column.
    assert_eq!(display_width("e\u{0301}"), 1);
}

#[test]
fn grapheme_to_display_col_accounts_for_wide_chars() {
    // Before "語" there are two CJK graphemes, each 2 columns wide.
    assert_eq!(grapheme_to_display_col("日本語", 2, 4), 4);
}

#[test]
fn display_to_grapheme_col_is_inverse_for_wide_chars() {
    assert_eq!(display_to_grapheme_col("日本語", 4, 4), 2);
}

#[test]
fn display_to_grapheme_col_snaps_into_a_wide_char() {
    // Clicking the right half of "日" must land on grapheme 0, not 1.
    assert_eq!(display_to_grapheme_col("日本語", 1, 4), 0);
}

#[test]
fn tabs_expand_to_the_next_tab_stop() {
    assert_eq!(display_width("\t"), 4);
    assert_eq!(grapheme_to_display_col("a\tb", 2, 4), 4);
}

#[test]
fn clicking_past_end_of_line_clamps_to_line_length() {
    assert_eq!(display_to_grapheme_col("abc", 99, 4), 3);
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path spikes/m0-feel/Cargo.toml --test width`

Expected: FAIL — the crate has no library target and `m0_feel::width` does not exist.

- [x] **Step 3: Add a library target and implement width.rs**

Add to `spikes/m0-feel/Cargo.toml` after `[package]`:

```toml
[lib]
name = "m0_feel"
path = "src/lib.rs"

[[bin]]
name = "m0-feel"
path = "src/main.rs"
```

`spikes/m0-feel/src/lib.rs`:

```rust
pub mod width;
```

`spikes/m0-feel/src/width.rs`:

```rust
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Display columns occupied by a single grapheme cluster.
///
/// Tabs are handled by callers that know the current column, so this reports
/// a tab as 0 and lets them add the tab-stop padding.
fn grapheme_width(g: &str) -> usize {
    if g == "\t" {
        0
    } else {
        // Zero-width and combining sequences report 0 here, which is correct.
        UnicodeWidthStr::width(g)
    }
}

/// Total display columns a string occupies, expanding tabs to `tab_width` stops.
pub fn display_width_with_tabs(s: &str, tab_width: usize) -> usize {
    let mut col = 0usize;
    for g in s.graphemes(true) {
        if g == "\t" {
            col += tab_width - (col % tab_width);
        } else {
            col += grapheme_width(g);
        }
    }
    col
}

/// Total display columns, using the default tab width of 4.
pub fn display_width(s: &str) -> usize {
    display_width_with_tabs(s, 4)
}

/// Display column at which the grapheme at `grapheme_idx` begins.
pub fn grapheme_to_display_col(line: &str, grapheme_idx: usize, tab_width: usize) -> usize {
    let mut col = 0usize;
    for (i, g) in line.graphemes(true).enumerate() {
        if i == grapheme_idx {
            return col;
        }
        if g == "\t" {
            col += tab_width - (col % tab_width);
        } else {
            col += grapheme_width(g);
        }
    }
    col
}

/// Grapheme index containing `display_col`.
///
/// Clicking anywhere inside a wide grapheme selects that grapheme, so the
/// right half of a CJK character does not land on the following one. Clicks
/// past the end of the line clamp to the line length.
pub fn display_to_grapheme_col(line: &str, display_col: usize, tab_width: usize) -> usize {
    let mut col = 0usize;
    for (i, g) in line.graphemes(true).enumerate() {
        let w = if g == "\t" {
            tab_width - (col % tab_width)
        } else {
            grapheme_width(g)
        };
        if display_col < col + w.max(1) {
            return i;
        }
        col += w;
    }
    line.graphemes(true).count()
}
```

- [x] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path spikes/m0-feel/Cargo.toml --test width`

Expected: PASS, 9 tests.

Actual: 9 passed, 0 failed, on `unicode-width` 0.2.2. No `[patch.crates-io]` override needed
for M1. `width.rs` promotes into `typ-buffer` unchanged at Task 11.

- [x] **Step 5: Commit**

```bash
git add spikes/m0-feel
git commit -m "feat(spike): grapheme to display column mapping with unicode tests"
```

---

### Task 3: Load and scroll a large file

**Files:**
- Create: `spikes/m0-feel/src/viewport.rs`, `spikes/m0-feel/tests/viewport.rs`
- Modify: `spikes/m0-feel/src/lib.rs`, `spikes/m0-feel/src/main.rs`

**Interfaces:**
- Consumes: `width::display_width`
- Produces:
  - `viewport::Viewport { pub top_line: usize, pub height: usize }`
  - `Viewport::scroll(&mut self, delta: i32, total_lines: usize)`
  - `Viewport::visible_range(&self, total_lines: usize) -> std::ops::Range<usize>`

- [x] **Step 1: Write the failing tests**

`spikes/m0-feel/tests/viewport.rs`:

```rust
use m0_feel::viewport::Viewport;

#[test]
fn visible_range_starts_at_top_line() {
    let vp = Viewport { top_line: 10, height: 5 };
    assert_eq!(vp.visible_range(100), 10..15);
}

#[test]
fn visible_range_clamps_to_total_lines() {
    let vp = Viewport { top_line: 98, height: 5 };
    assert_eq!(vp.visible_range(100), 98..100);
}

#[test]
fn scroll_down_advances_top_line() {
    let mut vp = Viewport { top_line: 0, height: 10 };
    vp.scroll(3, 100);
    assert_eq!(vp.top_line, 3);
}

#[test]
fn scroll_up_past_start_clamps_to_zero() {
    let mut vp = Viewport { top_line: 2, height: 10 };
    vp.scroll(-10, 100);
    assert_eq!(vp.top_line, 0);
}

#[test]
fn scroll_down_past_end_keeps_last_screen_visible() {
    let mut vp = Viewport { top_line: 0, height: 10 };
    vp.scroll(1000, 100);
    assert_eq!(vp.top_line, 90);
}

#[test]
fn scroll_does_not_underflow_when_file_is_shorter_than_viewport() {
    let mut vp = Viewport { top_line: 0, height: 50 };
    vp.scroll(10, 3);
    assert_eq!(vp.top_line, 0);
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path spikes/m0-feel/Cargo.toml --test viewport`

Expected: FAIL — `m0_feel::viewport` does not exist.

- [x] **Step 3: Implement viewport.rs**

`spikes/m0-feel/src/viewport.rs`:

```rust
use std::ops::Range;

#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub top_line: usize,
    pub height: usize,
}

impl Viewport {
    /// Lines currently visible, clamped to the end of the buffer.
    pub fn visible_range(&self, total_lines: usize) -> Range<usize> {
        let start = self.top_line.min(total_lines);
        let end = (start + self.height).min(total_lines);
        start..end
    }

    /// Scroll by `delta` lines. Positive scrolls down.
    ///
    /// The last screenful stays visible rather than scrolling into empty
    /// space, and a buffer shorter than the viewport never scrolls at all.
    pub fn scroll(&mut self, delta: i32, total_lines: usize) {
        let max_top = total_lines.saturating_sub(self.height);
        let next = self.top_line as i64 + delta as i64;
        self.top_line = next.clamp(0, max_top as i64) as usize;
    }
}
```

Add `pub mod viewport;` to `spikes/m0-feel/src/lib.rs`.

- [x] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path spikes/m0-feel/Cargo.toml --test viewport`

Expected: PASS, 6 tests.

- [x] **Step 5: Render a real file in main.rs**

Replace `spikes/m0-feel/src/main.rs`:

```rust
use std::io::stdout;

use anyhow::{Context, Result};
use crossterm::ExecutableCommand;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseEventKind,
};
use m0_feel::viewport::Viewport;
use ratatui::DefaultTerminal;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ropey::Rope;

fn main() -> Result<()> {
    let path = std::env::args().nth(1).context("usage: m0-feel <file>")?;
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {path}"))?;
    let rope = Rope::from_str(&text);

    let mut terminal = ratatui::init();
    stdout().execute(EnableMouseCapture)?;

    let result = run(&mut terminal, &rope);

    stdout().execute(DisableMouseCapture)?;
    ratatui::restore();
    result
}

fn run(terminal: &mut DefaultTerminal, rope: &Rope) -> Result<()> {
    let total = rope.len_lines();
    let mut vp = Viewport { top_line: 0, height: 0 };

    loop {
        terminal.draw(|frame| {
            let area = frame.area();
            vp.height = area.height as usize;
            let lines: Vec<Line> = rope
                .lines_at(vp.visible_range(total).start)
                .take(vp.height)
                .map(|l| Line::raw(l.to_string().trim_end_matches('\n').to_string()))
                .collect();
            frame.render_widget(Paragraph::new(lines), area);
        })?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let quit = key.code == KeyCode::Char('q')
                    || (key.code == KeyCode::Char('c')
                        && key.modifiers.contains(KeyModifiers::CONTROL));
                if quit {
                    return Ok(());
                }
                match key.code {
                    KeyCode::Down => vp.scroll(1, total),
                    KeyCode::Up => vp.scroll(-1, total),
                    KeyCode::PageDown => vp.scroll(vp.height as i32, total),
                    KeyCode::PageUp => vp.scroll(-(vp.height as i32), total),
                    _ => {}
                }
            }
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollDown => vp.scroll(3, total),
                MouseEventKind::ScrollUp => vp.scroll(-3, total),
                _ => {}
            },
            _ => {}
        }
    }
}
```

- [x] **Step 6: Generate a large test file and scroll it**

```bash
cargo build --release --manifest-path spikes/m0-feel/Cargo.toml
```

Generate a 50k-line file (PowerShell):

```powershell
1..50000 | ForEach-Object { "fn f$_() -> usize { let x = $_; x * 2 }" } |
  Set-Content -Path spikes/m0-feel/big.rs -Encoding utf8
```

Run: `./target/release/m0-feel.exe spikes/m0-feel/big.rs`

Expected: renders, arrow keys and wheel scroll it, `q` exits cleanly. Hold `Down` and scroll
aggressively — note subjectively whether it feels smooth. Measurement comes in Task 5.

- [x] **Step 7: Commit**

```bash
git add spikes/m0-feel
git commit -m "feat(spike): load and scroll large files with keyboard and wheel"
```

---

### Task 4: Click-to-position the cursor

The core "does this feel native" question. A click must land on the exact grapheme under the
pointer, including inside wide characters and past line ends.

**Files:**
- Create: `spikes/m0-feel/src/click.rs`, `spikes/m0-feel/tests/click.rs`
- Modify: `spikes/m0-feel/src/lib.rs`, `spikes/m0-feel/src/main.rs`

**Interfaces:**
- Consumes: `width::display_to_grapheme_col`, `viewport::Viewport`
- Produces: `click::click_to_position(&Rope, Viewport, u16, u16, usize) -> (usize, usize)`
  returning `(line, grapheme_col)`

- [x] **Step 1: Write the failing tests**

`spikes/m0-feel/tests/click.rs`:

```rust
use m0_feel::click::click_to_position;
use m0_feel::viewport::Viewport;
use ropey::Rope;

fn rope() -> Rope {
    Rope::from_str("hello world\n日本語です\nshort\n")
}

#[test]
fn click_on_first_line_maps_to_that_column() {
    let vp = Viewport { top_line: 0, height: 10 };
    assert_eq!(click_to_position(&rope(), vp, 6, 0, 4), (0, 6));
}

#[test]
fn click_accounts_for_scroll_offset() {
    let vp = Viewport { top_line: 2, height: 10 };
    // Row 0 on screen is buffer line 2 when scrolled by 2.
    assert_eq!(click_to_position(&rope(), vp, 0, 0, 4), (2, 0));
}

#[test]
fn click_inside_a_wide_char_selects_that_char() {
    let vp = Viewport { top_line: 1, height: 10 };
    // Column 1 is the right half of the first CJK grapheme.
    assert_eq!(click_to_position(&rope(), vp, 1, 0, 4), (1, 0));
}

#[test]
fn click_past_end_of_line_clamps_to_line_end() {
    let vp = Viewport { top_line: 2, height: 10 };
    assert_eq!(click_to_position(&rope(), vp, 99, 0, 4), (2, 5));
}

#[test]
fn click_below_last_line_clamps_to_last_line() {
    let vp = Viewport { top_line: 0, height: 10 };
    let r = rope();
    let (line, _) = click_to_position(&r, vp, 0, 90, 4);
    assert_eq!(line, r.len_lines() - 1);
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path spikes/m0-feel/Cargo.toml --test click`

Expected: FAIL — `m0_feel::click` does not exist.

- [x] **Step 3: Implement click.rs**

`spikes/m0-feel/src/click.rs`:

```rust
use ropey::Rope;

use crate::viewport::Viewport;
use crate::width::display_to_grapheme_col;

/// Map a mouse position in panel-local cells to a `(line, grapheme_col)`
/// position in the buffer.
///
/// Rows below the last line clamp to the last line, and columns past the end
/// of a line clamp to that line's length — matching what every GUI editor does.
pub fn click_to_position(
    rope: &Rope,
    vp: Viewport,
    mouse_col: u16,
    mouse_row: u16,
    tab_width: usize,
) -> (usize, usize) {
    let last_line = rope.len_lines().saturating_sub(1);
    let line = (vp.top_line + mouse_row as usize).min(last_line);

    let text = rope.line(line).to_string();
    let text = text.trim_end_matches('\n');
    let col = display_to_grapheme_col(text, mouse_col as usize, tab_width);

    (line, col)
}
```

Add `pub mod click;` to `spikes/m0-feel/src/lib.rs`.

- [x] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path spikes/m0-feel/Cargo.toml --test click`

Expected: PASS, 5 tests.

- [x] **Step 5: Wire click into the spike and render a cursor**

In `spikes/m0-feel/src/main.rs`, add `use m0_feel::click::click_to_position;` and a
`cursor: (usize, usize)` initialized to `(0, 0)`.

In the mouse match arm:

```rust
MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
    cursor = click_to_position(rope, vp, m.column, m.row, 4);
}
```

In the draw closure, after rendering the paragraph:

```rust
use m0_feel::width::grapheme_to_display_col;

if cursor.0 >= vp.top_line && cursor.0 < vp.top_line + vp.height {
    let text = rope.line(cursor.0).to_string();
    let display_col = grapheme_to_display_col(text.trim_end_matches('\n'), cursor.1, 4);
    frame.set_cursor_position((
        area.x + display_col as u16,
        area.y + (cursor.0 - vp.top_line) as u16,
    ));
}
```

- [x] **Step 6: Verify click feel by hand**

Run: `./target/release/m0-feel.exe spikes/m0-feel/big.rs`

Then a mixed-width file:

```powershell
"hello world`n日本語です mixed 🦀 emoji`n`ttabbed line" |
  Set-Content spikes/m0-feel/wide.txt -Encoding utf8
```

Run: `./target/release/m0-feel.exe spikes/m0-feel/wide.txt`

Expected: the cursor lands exactly under the pointer on every line, including the CJK, emoji,
and tab lines. **Any visible drift is a blocking finding** — record it and resolve before M1.

- [x] **Step 7: Commit**

```bash
git add spikes/m0-feel
git commit -m "feat(spike): click-to-position cursor with wide-char correctness"
```

---

### Task 5: Frame timing and synchronized output

**Files:**
- Create: `spikes/m0-feel/src/metrics.rs`, `spikes/m0-feel/tests/metrics.rs`
- Modify: `spikes/m0-feel/src/lib.rs`, `spikes/m0-feel/src/main.rs`

**Interfaces:**
- Consumes: nothing
- Produces:
  - `metrics::FrameTimer::new() -> FrameTimer`
  - `FrameTimer::record(&mut self, d: std::time::Duration)`
  - `FrameTimer::report(&self) -> String` — count, mean, p50, p99, max in microseconds

- [x] **Step 1: Write the failing tests**

`spikes/m0-feel/tests/metrics.rs`:

```rust
use std::time::Duration;

use m0_feel::metrics::FrameTimer;

#[test]
fn p99_reflects_the_slow_tail() {
    let mut t = FrameTimer::new();
    for _ in 0..99 {
        t.record(Duration::from_micros(100));
    }
    t.record(Duration::from_micros(50_000));
    let report = t.report();
    assert!(report.contains("n=100"), "report was: {report}");
    assert!(report.contains("max=50000us"), "report was: {report}");
}

#[test]
fn empty_timer_reports_without_panicking() {
    assert!(FrameTimer::new().report().contains("n=0"));
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path spikes/m0-feel/Cargo.toml --test metrics`

Expected: FAIL — `m0_feel::metrics` does not exist.

- [x] **Step 3: Implement metrics.rs**

`spikes/m0-feel/src/metrics.rs`:

```rust
use std::time::Duration;

/// Collects frame durations and reports percentiles.
///
/// Stores every sample rather than bucketing. A spike run is short and this
/// keeps the percentile math exact.
#[derive(Default)]
pub struct FrameTimer {
    samples: Vec<u128>,
}

impl FrameTimer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, d: Duration) {
        self.samples.push(d.as_micros());
    }

    pub fn report(&self) -> String {
        if self.samples.is_empty() {
            return "n=0".to_string();
        }
        let mut s = self.samples.clone();
        s.sort_unstable();
        let n = s.len();
        let mean = s.iter().sum::<u128>() / n as u128;
        let pick = |q: f64| s[((n as f64 * q) as usize).min(n - 1)];
        format!(
            "n={} mean={}us p50={}us p99={}us max={}us",
            n,
            mean,
            pick(0.50),
            pick(0.99),
            s[n - 1]
        )
    }
}
```

Add `pub mod metrics;` to `spikes/m0-feel/src/lib.rs`.

- [x] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path spikes/m0-feel/Cargo.toml --test metrics`

Expected: PASS, 2 tests.

- [x] **Step 5: Instrument the draw loop and add a sync-output toggle**

In `spikes/m0-feel/src/main.rs`, change `run`'s signature to `-> Result<FrameTimer>` and
return `Ok(timer)` at the quit branch. Before the loop:

```rust
use std::time::Instant;
use m0_feel::metrics::FrameTimer;

let mut timer = FrameTimer::new();
let mut sync_output = true;
```

Wrap the draw call:

```rust
let frame_start = Instant::now();
if sync_output {
    // CSI ?2026h / l — synchronized output. Tells the terminal to buffer this
    // frame and present it atomically, which removes tearing on partial repaints.
    print!("\x1b[?2026h");
}
terminal.draw(|frame| { /* existing body unchanged */ })?;
if sync_output {
    print!("\x1b[?2026l");
}
timer.record(frame_start.elapsed());
```

Add to the key match:

```rust
KeyCode::Char('s') => sync_output = !sync_output,
```

And in `main`, after `ratatui::restore()`:

```rust
println!("frame timing: {}", timer.report());
```

- [x] **Step 6: Measure**

Run: `./target/release/m0-feel.exe spikes/m0-feel/big.rs`

Scroll hard for about 30 seconds with sync output on, quit, record the numbers. Repeat with
sync output toggled off (`s`).

Expected: p99 well under 16000us. Compare visual tearing between modes and record which
terminal was used — this is a **Windows Terminal** measurement and other emulators may differ.

- [x] **Step 7: Commit**

```bash
git add spikes/m0-feel
git commit -m "feat(spike): frame timing metrics and synchronized output toggle"
```

---

### Task 6: Tree-sitter highlighting on the visible viewport

**Files:**
- Create: `spikes/m0-feel/src/highlight.rs`, `spikes/m0-feel/tests/highlight.rs`
- Modify: `spikes/m0-feel/src/lib.rs`, `spikes/m0-feel/src/main.rs`

**Interfaces:**
- Consumes: nothing
- Produces:
  - `highlight::Highlighter::new_rust() -> anyhow::Result<Highlighter>`
  - `Highlighter::parse(&mut self, text: &str)`
  - `Highlighter::spans_for_line(&self, text: &str, line: usize) -> Vec<(Range<usize>, &'static str)>`

- [x] **Step 1: Write the failing tests**

`spikes/m0-feel/tests/highlight.rs`:

```rust
use m0_feel::highlight::Highlighter;

#[test]
fn keywords_are_highlighted_on_their_line() {
    let src = "fn main() {}\nlet x = 1;\n";
    let mut h = Highlighter::new_rust().expect("rust grammar loads");
    h.parse(src);
    let spans = h.spans_for_line(src, 0);
    assert!(!spans.is_empty(), "expected highlight spans on line 0");
}

#[test]
fn parsing_a_large_buffer_completes() {
    let src = "fn f() { let x = 1; }\n".repeat(20_000);
    let mut h = Highlighter::new_rust().expect("rust grammar loads");
    h.parse(&src);
    assert!(!h.spans_for_line(&src, 0).is_empty());
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path spikes/m0-feel/Cargo.toml --test highlight`

Expected: FAIL — `m0_feel::highlight` does not exist.

- [x] **Step 3: Implement highlight.rs**

`spikes/m0-feel/src/highlight.rs`:

```rust
use std::ops::Range;

use anyhow::Result;
use tree_sitter::{Language, Node, Parser, Tree};

/// Minimal highlighter: parses with tree-sitter and classifies leaf nodes by
/// their grammar node kind.
///
/// A real implementation uses highlight queries and captures. The spike only
/// needs to answer "can tree-sitter keep up while scrolling", so node-kind
/// classification is enough and avoids pulling in the query machinery.
pub struct Highlighter {
    parser: Parser,
    tree: Option<Tree>,
}

impl Highlighter {
    pub fn new_rust() -> Result<Self> {
        let mut parser = Parser::new();
        let lang: Language = tree_sitter_rust::LANGUAGE.into();
        parser.set_language(&lang)?;
        Ok(Self { parser, tree: None })
    }

    /// Reparse `text`, reusing the previous tree so edits are incremental.
    pub fn parse(&mut self, text: &str) {
        self.tree = self.parser.parse(text, self.tree.as_ref());
    }

    /// Highlight spans for one line, as byte ranges relative to that line.
    pub fn spans_for_line(&self, text: &str, line: usize) -> Vec<(Range<usize>, &'static str)> {
        let Some(tree) = &self.tree else {
            return Vec::new();
        };

        let line_start: usize = text.split_inclusive('\n').take(line).map(str::len).sum();
        if line_start > text.len() {
            return Vec::new();
        }
        let line_len = text[line_start..]
            .split_inclusive('\n')
            .next()
            .map_or(0, str::len);
        let line_end = line_start + line_len;

        let mut out = Vec::new();
        collect_leaves(tree.root_node(), line_start, line_end, &mut out);
        out
    }
}

fn collect_leaves(
    node: Node,
    line_start: usize,
    line_end: usize,
    out: &mut Vec<(Range<usize>, &'static str)>,
) {
    if node.end_byte() <= line_start || node.start_byte() >= line_end {
        return;
    }
    if node.child_count() == 0 {
        if let Some(kind) = classify(node.kind()) {
            let s = node.start_byte().max(line_start) - line_start;
            let e = node.end_byte().min(line_end) - line_start;
            if s < e {
                out.push((s..e, kind));
            }
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_leaves(child, line_start, line_end, out);
    }
}

fn classify(kind: &str) -> Option<&'static str> {
    match kind {
        "fn" | "let" | "if" | "else" | "match" | "struct" | "enum" | "impl" | "pub" | "use"
        | "mod" | "return" | "for" | "while" | "loop" => Some("keyword"),
        "string_literal" | "raw_string_literal" | "char_literal" => Some("string"),
        "integer_literal" | "float_literal" => Some("number"),
        "line_comment" | "block_comment" => Some("comment"),
        "identifier" | "type_identifier" | "field_identifier" => Some("identifier"),
        _ => None,
    }
}
```

Add `pub mod highlight;` to `spikes/m0-feel/src/lib.rs`.

- [x] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path spikes/m0-feel/Cargo.toml --test highlight`

Expected: PASS, 2 tests.

If `tree_sitter_rust::LANGUAGE` does not resolve, older grammar crates expose `language()`
instead. Check `cargo doc -p tree-sitter-rust --open` and use whichever the pinned 0.24
exposes. Record which in `FINDINGS.md`.

- [x] **Step 5: Colorize the rendered lines**

In `spikes/m0-feel/src/main.rs`:

```rust
use ratatui::style::{Color, Style};
use ratatui::text::Span;

fn style_for(kind: &str) -> Style {
    let c = match kind {
        "keyword" => Color::Magenta,
        "string" => Color::Green,
        "number" => Color::Yellow,
        "comment" => Color::DarkGray,
        "identifier" => Color::Cyan,
        _ => Color::Reset,
    };
    Style::default().fg(c)
}

fn styled_line(text: &str, spans: &[(std::ops::Range<usize>, &'static str)]) -> Line<'static> {
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut pos = 0usize;
    for (range, kind) in spans {
        if range.start > pos {
            out.push(Span::raw(text[pos..range.start].to_string()));
        }
        out.push(Span::styled(text[range.clone()].to_string(), style_for(kind)));
        pos = range.end;
    }
    if pos < text.len() {
        out.push(Span::raw(text[pos..].to_string()));
    }
    Line::from(out)
}
```

Parse once after loading the file, then call `styled_line` per visible line in the draw closure.

- [x] **Step 6: Measure highlighting under scroll**

Run: `./target/release/m0-feel.exe spikes/m0-feel/big.rs`

Scroll hard for 30 seconds and record the timing report.

Expected: p99 still under 16000us with highlighting on. If not, note whether the cost is in
`parse` (should be near zero when text is unchanged) or in `spans_for_line` — the latter walks
the tree per line per frame and is the obvious thing M1 caches.

**Result: first measurement failed the budget badly — `p99=1144011us`, `max=1144830us`
against a `p50` of `1108us`.** Highlighting was subjectively unusable; turning it off with
`h` restored normal scrolling. The prediction above was half right and its proposed fix was
wrong, which matters for M1, so both are recorded.

Right: the cost was in `spans_for_line`, not `parse`. Wrong: **caching is not the fix.**
There were two independent O(lines-above-the-viewport) costs, and the bimodal timing —
fast `p50`, catastrophic `p99` — is their signature, since both scale with scroll depth
rather than with viewport size:

1. `spans_for_line` recomputed the line's byte offset with
   `text.split_inclusive('\n').take(line).map(str::len).sum()`, rescanning the file from
   byte 0 **per visible line, per frame**. At line 40,000 that is a 1.7 MB scan × ~40 lines.
   The rope already knows this: `rope.line_to_byte(line)`.
2. `collect_leaves` descended from `root_node()`. Pruning by byte range still has to *visit*
   every sibling to prune it, and a 50k-line file's root has 50k top-level items. Measured
   at 18.7ms for a single 40-line viewport — and that was paid 40 times per frame.

`Node::descendant_for_byte_range` looks like the fix and is not: a 40-line viewport spans 40
sibling `function_item`s, so the smallest node containing it *is* the root. Measured at
2.8us and it changed nothing. What works is seeking at the sibling level —
`TreeCursor::goto_first_child_for_byte(start)`, then walking siblings forward until past
`end`. **18.7ms → 0.4ms, and flat with scroll depth.**

Combined with hoisting the walk out of the per-line loop — one walk per viewport instead of
one per line, spans then split across lines as they are consumed — this is a pure traversal
fix. No cache, no invalidation logic.

**Carry into M1:** `typ-syntax` asks the tree for a *viewport* (`spans_in_range`), never for
a line. A per-line cache would have papered over an O(offset) traversal and inherited a
cache-invalidation problem on every edit for no reason. Guarded by
`tests/highlight.rs::viewport_spans_deep_in_a_large_buffer_stay_cheap`, which asserts a
40-line viewport at line 39,000 of a 40k-line file costs under one 16ms frame.

- [x] **Step 7: Commit**

```bash
git add spikes/m0-feel
git commit -m "feat(spike): tree-sitter highlighting on the visible viewport"
```

---

### Task 6a: Move the initial parse off the render thread — *added, not in the original plan*

Task 6 measured the initial parse at **723–761ms for 50k lines** and the plan treated that as
a number to record. It is also a number to act on: the whole of it lands before the first
frame, so the editor is visibly frozen for three quarters of a second on open — against a §4
budget of **cold start to interactive < 100ms.**

**First: is it fixable by parsing faster?** No. Measured across sizes and shapes:

| Input | Lines | Size | Parse | Throughput |
|---|---|---|---|---|
| flat ×6,250 | 6,250 | 225 KB | 120.4ms | 1.9 MB/s |
| flat ×12,500 | 12,500 | 451 KB | 241.2ms | 1.9 MB/s |
| flat ×25,000 | 25,000 | 903 KB | 441.6ms | 2.1 MB/s |
| flat ×50,000 | 50,000 | 1806 KB | 798.5ms | 2.3 MB/s |
| `big.rs` | 50,000 | 2273 KB | 923.3ms | 2.5 MB/s |
| nested (mods/impls/matches) | 45,000 | 1469 KB | 765.7ms | 2.0 MB/s |

Linear, and flat at ~2 MB/s regardless of tree shape. `big.rs`'s 50k flat top-level items are
not a pathological input — a generated file with realistic nesting parses at the same rate.
There is no constant factor to win, so the cost has to be *hidden* rather than reduced.

**How the field handles it — three different answers, and none of them is "parse faster":**

- **Vim never builds a whole-file model.** Regex rules over visible lines only, with
  `syntax sync minlines` guessing the syntax state at the top of the screen and `synmaxcol`
  abandoning long lines. Fast because it is approximate; the cost is highlighting that is
  visibly wrong after a fast scroll until forced to redraw.
- **Neovim pays the same tree-sitter cost and slices it.** Their treesitter tracking issue
  (#22426) lists "initial parse blocks the event loop" as a named bug, fixed in #22420 by
  using tree-sitter's parse timeout to spread one parse across event-loop iterations. They do
  that because Lua/libuv gives them no threads.
- **Helix** parses on load and carries the same cost.

**TYPE has threads, so it takes the direct version** — which §4 already mandates ("syntax
parsing runs on worker threads and delivers results as events"). This task is that constraint
proven on real numbers rather than asserted.

- [x] **Step 1: Parse on a worker, deliver the tree as a message**

`Arc<str>` for the source so the worker owns a share of it; `mpsc::channel` carrying
`(Highlighter, Duration)` back. Both `Parser` and `Tree` are `Send`, so the whole
`Highlighter` moves across cleanly and no lock is needed.

- [x] **Step 2: Poll the event loop instead of blocking on it**

`event::read()` blocks until input, so a parse finishing during an idle moment would not
appear until the user's next keypress. Replaced with `event::poll(16ms)` plus a dirty flag,
redrawing only on state change.

The dirty flag also fixes a **measurement** bug: every mouse-move event was previously
triggering a redraw *and* recording a frame, padding the histogram with cheap no-op frames
and flattering both `p50` and `p99`. Frame counts before and after this change are not
comparable.

- [x] **Step 3: Report time-to-first-frame**

The binary now prints `initial parse: Nms (off-thread)` and `first frame at: Nms after
start`, and the status bar shows `hl:wait` → `hl:on` so the handoff is visible rather than
inferred.

- [x] **Step 4: Measure — folded into Task 7**

**Known ceiling.** The 16ms poll wakes 60×/sec while idle doing nothing, which is acceptable
for a 30-second spike and not for an editor. **M1 blocks on a single event channel** with a
thread pumping crossterm events into it, so a finished parse wakes the loop directly and idle
costs nothing. Marked `ponytail:` at the call site.

**Second known ceiling.** Scrolling into not-yet-parsed territory shows plain text and then
recolors all at once when the tree lands. Fine at 0.7s. If a file ever takes ~10s, the answer
is vim's — approximate highlighting immediately, exact once parsed. Recorded so that
"off-thread" is not mistaken for something that scales without limit.

- [x] **Step 5: Commit**

```bash
git add spikes/m0-feel
git commit -m "perf(spike): parse off-thread so the first frame does not wait"
```

---

### Task 7: Write FINDINGS.md and make the go/no-go call

**Files:**
- Create: `spikes/m0-feel/FINDINGS.md`

**Interfaces:**
- Consumes: measurements from Tasks 4, 5, 6
- Produces: the M0 decision that gates M1

- [x] **Step 1: Write FINDINGS.md**

Fill in every measured value. Do not write "good" or "fine" — write numbers.

```markdown
# M0 Feel Spike — Findings

**Date:** <date>
**Terminal:** Windows Terminal — <version>
**Build:** release, opt-level 3, thin LTO
**Test file:** 50,000 generated Rust lines

## 1. Mouse click-to-position

- Feel while clicking around: <native / slight lag / laggy>
- Wide-character accuracy (CJK): <exact / drifts by N columns>
- Emoji accuracy: <exact / drifts by N columns>
- Tab accuracy: <exact / drifts by N columns>
- Click past end of line: <clamps correctly / wrong>

## 2. Synchronized output (CSI 2026)

- Supported by this terminal: <yes / no>
- Visible tearing with sync ON: <none / some / bad>
- Visible tearing with sync OFF: <none / some / bad>
- Verdict: <keep / drop>

## 3. Frame timing

Runs must be of comparable length — a 90-frame run and a 1100-frame run do not compare, and
`p99` on 90 samples is just the 90th sample. Numbers below are post-fix; see §4.

| Scenario | n | mean | p50 | p99 | max | max_at_frame |
|---|---|---|---|---|---|---|
| Scroll, highlight on, sync on | | | | | | |
| Scroll, no highlight, sync on | | | | | | |
| Scroll, no highlight, sync off | | | | | | |

Budget: p99 < 16000us. Met: <yes / no>

Was the worst frame the startup paint (`max_at_frame` 0 or 1) or a real stall: <which>

## 4. Tree-sitter under scroll

- Initial parse of 50k lines: <N>ms, off-thread
- Time from process start to first painted frame: <N>ms
- Parse throughput: ~2 MB/s, linear in file size and independent of tree shape
- Cost concentrated in: <parse / spans_for_line / rendering>
- Needs per-line caching in M1: **no** — it needed viewport-scoped traversal instead.
  Two O(lines-above-viewport) costs, both fixed by construction rather than by cache:
  `rope.line_to_byte()` for the line offset, and `TreeCursor::goto_first_child_for_byte()`
  to seek to the first top-level item instead of descending from the root. Measured
  18.7ms → 0.4ms per viewport, flat with scroll depth. See Task 6 Step 6.
- Highlighting p99 before the traversal fix: 1144011us (recorded because "tree-sitter is
  too slow to scroll" would have been the wrong conclusion to draw from it)

## 5. Unicode width

- `unicode-width` 0.2.2 passed all 9 width tests: <yes / no>
- If no, which failed and how:
- Does M1 need a `[patch.crates-io]` fork: <yes / no>

## 6. API surprises

- `tree_sitter_rust` language accessor used: <LANGUAGE / language()>
- `Node::descendant_for_byte_range` does not help viewport queries — a multi-line viewport's
  smallest containing node is the root. `TreeCursor::goto_first_child_for_byte` is the one
  that works.
- Other deviations from the plan:
  - `h` runtime toggle for highlighting, so the on/off comparison is one process and one
    scroll pattern rather than two runs.
  - Initial parse timed and printed — the FINDINGS template asked for the number and nothing
    was measuring it.
  - Parse moved off-thread (Task 6a), which forced `event::poll` + a dirty flag in place of
    blocking `event::read()`.

---

## Verdict

**GO / NO-GO:** <decision>

Reasoning:

Carry into M1:
- `src/width.rs` -> `crates/typ-buffer/src/position.rs`
- <anything else worth keeping>

Discard:
- everything else in this spike
```

- [x] **Step 2: Commit the findings**

```bash
git add spikes/m0-feel/FINDINGS.md
git commit -m "docs(spike): record m0 feel measurements and go/no-go verdict"
```

- [x] **Step 3: Stop and review**

**Hard gate.** Do not begin M1 until `FINDINGS.md` is read and GO is confirmed.

M1 does not start while any of these is true:
- Click-to-position drifts on wide characters
- p99 frame time exceeds 16000us while scrolling with highlighting on
- The terminal is left broken on exit under any condition

**GO confirmed 2026-08-13.** None of the three blockers hold: click-to-position is exact on
CJK, emoji and tabs; p99 with highlighting on is 3657us against a 16000us budget; the terminal
restores on both `q` and `Ctrl+C`. One non-blocking defect is carried into M1 rather than
fixed here — mouse capture leaks on panic, see FINDINGS §6. M0 code is now frozen; the spike
is deleted once `src/width.rs` is promoted at Task 11.

---

# Milestone 1 — Walking Skeleton

Real architecture starts here. Everything below is production code held to the global
constraints.

---

### Task 8: Workspace scaffold and `typ-core` event vocabulary

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `.gitattributes`
- Create: `crates/typ-core/{Cargo.toml,src/lib.rs,src/event.rs,src/key.rs,tests/event.rs}`

**Interfaces:**
- Consumes: nothing
- Produces:
  - `typ_core::PanelId(pub u32)`, `typ_core::HandlerId(pub &'static str)`
  - `typ_core::NotifyLevel { Info, Warn, Error }`
  - `typ_core::PanelEvent` — the closed set of 8 variants below
  - `typ_core::KeyChord { pub raw: crossterm::event::KeyEvent, pub canonical: String }`
  - `KeyChord::from_event(KeyEvent) -> KeyChord`

- [x] **Step 1: Write the failing tests**

`crates/typ-core/tests/event.rs`:

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_core::{HandlerId, KeyChord, NotifyLevel, PanelEvent, PanelId};

#[test]
fn plain_char_canonicalizes_to_itself() {
    let k = KeyChord::from_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    assert_eq!(k.canonical, "a");
}

#[test]
fn ctrl_modifier_is_prefixed() {
    let k = KeyChord::from_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert_eq!(k.canonical, "ctrl+s");
}

#[test]
fn modifiers_are_ordered_consistently() {
    let mods = KeyModifiers::CONTROL | KeyModifiers::SHIFT | KeyModifiers::ALT;
    let k = KeyChord::from_event(KeyEvent::new(KeyCode::Char('p'), mods));
    assert_eq!(k.canonical, "ctrl+alt+shift+p");
}

#[test]
fn named_keys_use_lowercase_names() {
    let k = KeyChord::from_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert_eq!(k.canonical, "enter");
    let k = KeyChord::from_event(KeyEvent::new(KeyCode::F(5), KeyModifiers::NONE));
    assert_eq!(k.canonical, "f5");
}

#[test]
fn panel_event_stays_small() {
    // This vocabulary is capped deliberately. New panels register a handler
    // and route through OpenWith rather than adding variants.
    let all = [
        PanelEvent::NeedsRedraw,
        PanelEvent::Quit,
        PanelEvent::CloseSelf,
        PanelEvent::Focus(PanelId(0)),
        PanelEvent::OpenFile { path: "x".into(), line: 0, col: 0 },
        PanelEvent::OpenWith { handler: HandlerId("editor"), path: "x".into() },
        PanelEvent::RunCommand { command: "ls".into(), cwd: None },
        PanelEvent::Notify { level: NotifyLevel::Info, message: "hi".into() },
    ];
    assert_eq!(all.len(), 8);
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p typ-core`

Expected: FAIL — no workspace or crate exists yet.

- [x] **Step 3: Create the workspace**

Root `Cargo.toml`:

```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"

[workspace.dependencies]
anyhow = "1.0"
crossterm = "0.29"
ratatui = "0.30.2"
ropey = "1.6"
unicode-segmentation = "1.11"
unicode-width = "0.2"

typ-core = { path = "crates/typ-core" }
typ-buffer = { path = "crates/typ-buffer" }
typ-registry = { path = "crates/typ-registry" }
typ-panel-editor = { path = "crates/typ-panel-editor" }
typ-panel-tree = { path = "crates/typ-panel-tree" }
typ-app = { path = "crates/typ-app" }

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
strip = true
```

Root `.gitignore`:

```
target/
```

Root `.gitattributes` — keeps line endings consistent for contributors on any OS:

```
* text=auto eol=lf
*.rs text eol=lf
*.toml text eol=lf
*.md text eol=lf
```

`crates/typ-core/Cargo.toml`:

```toml
[package]
name = "typ-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
crossterm.workspace = true
ratatui.workspace = true
```

- [x] **Step 4: Implement event.rs, key.rs, lib.rs**

`crates/typ-core/src/event.rs`:

```rust
use std::path::PathBuf;

/// Identifies a live panel instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PanelId(pub u32);

/// Identifies a registered handler in `typ-registry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HandlerId(pub &'static str);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyLevel {
    Info,
    Warn,
    Error,
}

/// The complete vocabulary a panel may emit.
///
/// This set is deliberately closed. Editors that let every viewer add its own
/// variant end up with an enum that each new panel type must edit, turning it
/// into a chokepoint. New panels register a handler in `typ-registry` and route
/// through `OpenWith` instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PanelEvent {
    /// Panel state changed; the app should repaint.
    NeedsRedraw,
    /// Quit the application.
    Quit,
    /// Close the emitting panel.
    CloseSelf,
    /// Move focus to another panel.
    Focus(PanelId),
    /// Open a path in whichever panel the registry says owns it.
    OpenFile { path: PathBuf, line: usize, col: usize },
    /// Open a path with an explicitly chosen handler.
    OpenWith { handler: HandlerId, path: PathBuf },
    /// Run a shell command, optionally in a given directory.
    RunCommand { command: String, cwd: Option<PathBuf> },
    /// Surface a message to the user.
    Notify { level: NotifyLevel, message: String },
}
```

`crates/typ-core/src/key.rs`:

```rust
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// A key press in both raw and canonical form.
///
/// `raw` is used for text insertion and PTY passthrough, where the exact event
/// matters. `canonical` is used for keybinding lookup, where a stable string
/// form matters. Keeping both avoids the bug where a binding table and a
/// text-input path disagree about what was pressed.
#[derive(Debug, Clone)]
pub struct KeyChord {
    pub raw: KeyEvent,
    pub canonical: String,
}

impl KeyChord {
    pub fn from_event(raw: KeyEvent) -> Self {
        let mut s = String::new();
        // Fixed order so a binding table never has to guess.
        if raw.modifiers.contains(KeyModifiers::CONTROL) {
            s.push_str("ctrl+");
        }
        if raw.modifiers.contains(KeyModifiers::ALT) {
            s.push_str("alt+");
        }
        if raw.modifiers.contains(KeyModifiers::SHIFT) {
            s.push_str("shift+");
        }
        s.push_str(&key_name(raw.code));
        Self { raw, canonical: s }
    }
}

fn key_name(code: KeyCode) -> String {
    match code {
        KeyCode::Char(c) => c.to_lowercase().to_string(),
        KeyCode::F(n) => format!("f{n}"),
        KeyCode::Enter => "enter".into(),
        KeyCode::Esc => "esc".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::BackTab => "backtab".into(),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Delete => "delete".into(),
        KeyCode::Insert => "insert".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pageup".into(),
        KeyCode::PageDown => "pagedown".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        other => format!("{other:?}").to_lowercase(),
    }
}
```

`crates/typ-core/src/lib.rs`:

```rust
pub mod event;
pub mod key;
pub mod panel;

pub use event::{HandlerId, NotifyLevel, PanelEvent, PanelId};
pub use key::KeyChord;
pub use panel::{Panel, RenderContext, ThemeColors};
```

`panel` is filled in at Task 9. To keep this task compiling, create an empty
`crates/typ-core/src/panel.rs` and comment out the `pub use panel::...` line until then.

- [x] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p typ-core`

Expected: PASS, 5 tests.

**Result: 5 passed.** `cargo clippy -p typ-core --all-targets -- -D warnings` is also clean.

Two deviations from this task as written:

1. **`exclude` needs a literal path.** The workspace section above did not mention
   `spikes/m0-feel`, and without excluding it cargo refuses to build the spike at all — a
   manifest under the workspace root must be a member or explicitly excluded. `exclude` does
   not glob the way `members` does, so `exclude = ["spikes/*"]` silently fails to match and
   `exclude = ["spikes/m0-feel"]` is required. Verified both workspaces build independently
   afterwards: 5 tests in `typ-core`, 25 in the spike.

2. **`panel_event_stays_small` needed teeth.** As specified the test built an 8-element array
   and asserted its length was 8, which is true by construction and stays true after a 9th
   variant is added. An exhaustive `match` with no wildcard arm was added, so growing
   `PanelEvent` breaks the build here and forces the decision to be deliberate. That is the
   behaviour the test's name and comment already claimed.

- [x] **Step 6: Commit**

```bash
git add Cargo.toml .gitignore .gitattributes crates/typ-core
git commit -m "feat(core): workspace scaffold with panel event vocabulary and key chords"
```

---

### Task 9: The `Panel` trait

**Files:**
- Modify: `crates/typ-core/src/panel.rs`, `crates/typ-core/src/lib.rs`
- Create: `crates/typ-core/tests/panel.rs`

**Interfaces:**
- Consumes: `PanelEvent`, `KeyChord`
- Produces:
  - `typ_core::ThemeColors` with fields `fg, bg, selection_bg, selection_fg, border,
    border_focused, line_numbers, cursor, status_bar_bg, status_bar_fg` (all
    `ratatui::style::Color`)
  - `typ_core::RenderContext<'a> { theme, is_focused, panel_index, terminal_width,
    terminal_height }`
  - `typ_core::Panel` — five required methods, everything else defaulted

- [x] **Step 1: Write the failing tests**

`crates/typ-core/tests/panel.rs`:

```rust
use std::any::Any;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use typ_core::{KeyChord, Panel, PanelEvent, RenderContext, ThemeColors};

/// A panel implementing only the required methods proves the defaults work.
struct Minimal;

impl Panel for Minimal {
    fn name(&self) -> &'static str {
        "minimal"
    }
    fn title(&self) -> String {
        "Minimal".into()
    }
    fn render(&mut self, _area: Rect, _buf: &mut Buffer, _ctx: &RenderContext) {}
    fn handle_key(&mut self, _chord: KeyChord) -> Vec<PanelEvent> {
        vec![PanelEvent::NeedsRedraw]
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[test]
fn a_panel_needs_only_the_required_methods() {
    let mut p = Minimal;
    let chord = KeyChord::from_event(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
    assert_eq!(p.handle_key(chord), vec![PanelEvent::NeedsRedraw]);
}

#[test]
fn defaulted_methods_return_empty() {
    let mut p = Minimal;
    assert!(p.handle_scroll(3, Rect::new(0, 0, 10, 10)).is_empty());
    assert!(p.tick().is_empty());
    assert!(!p.captures_escape());
    assert!(p.needs_close_confirmation().is_none());
}

#[test]
fn panels_are_dispatchable_as_trait_objects() {
    let panels: Vec<Box<dyn Panel>> = vec![Box::new(Minimal)];
    assert_eq!(panels[0].name(), "minimal");
    let _ = ThemeColors::default();
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p typ-core --test panel`

Expected: FAIL — `Panel`, `RenderContext`, `ThemeColors` are not defined.

- [x] **Step 3: Implement panel.rs**

`crates/typ-core/src/panel.rs`:

```rust
use std::any::Any;

use crossterm::event::MouseEvent;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::{KeyChord, PanelEvent};

/// The colors a panel is allowed to know about.
///
/// Deliberately a small copy rather than a reference to a full theme: panels
/// should not be able to reach into application state through their theme.
#[derive(Debug, Clone, Copy)]
pub struct ThemeColors {
    pub fg: Color,
    pub bg: Color,
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub border: Color,
    pub border_focused: Color,
    pub line_numbers: Color,
    pub cursor: Color,
    pub status_bar_bg: Color,
    pub status_bar_fg: Color,
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            fg: Color::White,
            bg: Color::Black,
            selection_bg: Color::Blue,
            selection_fg: Color::White,
            border: Color::DarkGray,
            border_focused: Color::Cyan,
            line_numbers: Color::DarkGray,
            cursor: Color::Yellow,
            status_bar_bg: Color::DarkGray,
            status_bar_fg: Color::White,
        }
    }
}

/// Everything a panel may see at render time.
///
/// This is the whole surface — a panel never receives `&AppState`.
pub struct RenderContext<'a> {
    pub theme: &'a ThemeColors,
    pub is_focused: bool,
    pub panel_index: usize,
    pub terminal_width: u16,
    pub terminal_height: u16,
}

/// A rectangular, focusable unit of UI.
///
/// Implementors provide five methods; everything else has a default. Panels
/// communicate outward by returning events, never by mutating shared state.
pub trait Panel: Any {
    /// Stable type name, used for registry lookup and session records.
    fn name(&self) -> &'static str;

    /// Dynamic title shown in the panel header.
    fn title(&self) -> String;

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &RenderContext);

    fn handle_key(&mut self, chord: KeyChord) -> Vec<PanelEvent>;

    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;

    /// `panel_area` is supplied so the panel can translate to local coordinates.
    fn handle_mouse(&mut self, event: MouseEvent, panel_area: Rect) -> Vec<PanelEvent> {
        let _ = (event, panel_area);
        Vec::new()
    }

    /// Coalesced scroll. Positive is down.
    fn handle_scroll(&mut self, delta: i32, panel_area: Rect) -> Vec<PanelEvent> {
        let _ = (delta, panel_area);
        Vec::new()
    }

    /// Periodic hook for background work.
    fn tick(&mut self) -> Vec<PanelEvent> {
        Vec::new()
    }

    /// True when the panel consumes Escape itself (e.g. an open search box).
    fn captures_escape(&self) -> bool {
        false
    }

    /// `Some(message)` blocks closing until confirmed.
    fn needs_close_confirmation(&self) -> Option<String> {
        None
    }
}
```

Uncomment the `pub use panel::{Panel, RenderContext, ThemeColors};` line in `lib.rs`.

- [x] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p typ-core`

Expected: PASS, 8 tests across both files.

**Result: 8 passed** (5 event, 3 panel), clippy clean under `-D warnings`. No deviations
from the task as written.

One constraint this task locks in, worth knowing before Task 12 writes a real panel:
`trait Panel: Any` implies `'static`, so no panel may borrow — a panel that wants to read a
buffer owns it or shares it behind `Rc`/`Arc`. That is the price of downcasting through
`as_any`, and it is the right trade here, but it is a real constraint rather than an
accident.

- [x] **Step 5: Commit**

```bash
git add crates/typ-core
git commit -m "feat(core): add Panel trait with defaulted optional methods"
```

---

### Task 10: `typ-buffer` — rope-backed text with positions and undo

**Files:**
- Create: `crates/typ-buffer/{Cargo.toml,src/lib.rs,src/position.rs,src/buffer.rs,src/undo.rs,
  tests/buffer.rs,tests/width.rs}`

**Interfaces:**
- Consumes: nothing
- Produces:
  - `typ_buffer::Position { pub line: usize, pub col: usize }` — `col` is a grapheme index
  - `typ_buffer::{display_width, grapheme_to_display_col, display_to_grapheme_col}`
  - `typ_buffer::TextBuffer::{from_path, from_str, line_count, line_text, insert_char,
    delete_before, save, is_dirty, path, undo, redo}`

- [x] **Step 1: Write the failing tests**

`crates/typ-buffer/tests/buffer.rs`:

```rust
use typ_buffer::{Position, TextBuffer};

#[test]
fn from_str_counts_lines() {
    let b = TextBuffer::from_str("a\nb\nc\n");
    assert_eq!(b.line_count(), 4); // trailing newline yields a final empty line
}

#[test]
fn line_text_excludes_the_newline() {
    let b = TextBuffer::from_str("hello\nworld\n");
    assert_eq!(b.line_text(0), "hello");
}

#[test]
fn insert_char_updates_the_line() {
    let mut b = TextBuffer::from_str("ac\n");
    b.insert_char(Position { line: 0, col: 1 }, 'b');
    assert_eq!(b.line_text(0), "abc");
}

#[test]
fn insert_marks_buffer_dirty() {
    let mut b = TextBuffer::from_str("a\n");
    assert!(!b.is_dirty());
    b.insert_char(Position { line: 0, col: 0 }, 'x');
    assert!(b.is_dirty());
}

#[test]
fn delete_before_removes_the_preceding_grapheme() {
    let mut b = TextBuffer::from_str("abc\n");
    b.delete_before(Position { line: 0, col: 2 });
    assert_eq!(b.line_text(0), "ac");
}

#[test]
fn delete_before_at_start_of_buffer_is_a_noop() {
    let mut b = TextBuffer::from_str("abc\n");
    b.delete_before(Position { line: 0, col: 0 });
    assert_eq!(b.line_text(0), "abc");
}

#[test]
fn delete_before_wide_char_removes_whole_grapheme() {
    let mut b = TextBuffer::from_str("日本語\n");
    b.delete_before(Position { line: 0, col: 1 });
    assert_eq!(b.line_text(0), "本語");
}

#[test]
fn undo_restores_the_previous_content() {
    let mut b = TextBuffer::from_str("a\n");
    b.insert_char(Position { line: 0, col: 1 }, 'b');
    assert_eq!(b.line_text(0), "ab");
    b.undo();
    assert_eq!(b.line_text(0), "a");
}

#[test]
fn redo_reapplies_an_undone_edit() {
    let mut b = TextBuffer::from_str("a\n");
    b.insert_char(Position { line: 0, col: 1 }, 'b');
    b.undo();
    b.redo();
    assert_eq!(b.line_text(0), "ab");
}

#[test]
fn save_writes_to_disk_and_clears_dirty() {
    let dir = std::env::temp_dir().join("typ-buffer-test");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("save.txt");
    std::fs::write(&path, "old\n").unwrap();

    let mut b = TextBuffer::from_path(&path).unwrap();
    b.insert_char(Position { line: 0, col: 3 }, '!');
    b.save().unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "old!\n");
    assert!(!b.is_dirty());
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p typ-buffer`

Expected: FAIL — the crate does not exist.

- [x] **Step 3: Create the crate**

`crates/typ-buffer/Cargo.toml`:

```toml
[package]
name = "typ-buffer"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
ropey.workspace = true
unicode-segmentation.workspace = true
unicode-width.workspace = true
```

`crates/typ-buffer/src/position.rs` — copy `spikes/m0-feel/src/width.rs` verbatim, then append:

```rust
/// A cursor location. `col` is a grapheme index, never a byte or char offset.
///
/// Using grapheme indices throughout means a cursor never lands inside a
/// multi-byte character or splits a combining sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}
```

`crates/typ-buffer/src/undo.rs`:

```rust
/// Whole-content undo history.
///
/// Snapshotting entire buffer content is the simplest thing that is correct
/// for any edit shape. If memory shows up in profiling on large files, switch
/// to per-edit deltas.
#[derive(Default)]
pub struct History {
    undo: Vec<String>,
    redo: Vec<String>,
}

impl History {
    pub fn record(&mut self, before: String) {
        self.undo.push(before);
        self.redo.clear();
    }

    /// Returns the content to restore, banking `current` for redo.
    pub fn undo(&mut self, current: String) -> Option<String> {
        let prev = self.undo.pop()?;
        self.redo.push(current);
        Some(prev)
    }

    pub fn redo(&mut self, current: String) -> Option<String> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }
}
```

`crates/typ-buffer/src/buffer.rs`:

```rust
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ropey::Rope;
use unicode_segmentation::UnicodeSegmentation;

use crate::position::Position;
use crate::undo::History;

pub struct TextBuffer {
    rope: Rope,
    path: Option<PathBuf>,
    dirty: bool,
    history: History,
}

impl TextBuffer {
    pub fn from_str(s: &str) -> Self {
        Self {
            rope: Rope::from_str(s),
            path: None,
            dirty: false,
            history: History::default(),
        }
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading {}", path.display()))?;
        Ok(Self {
            rope: Rope::from_str(&text),
            path: Some(path.to_path_buf()),
            dirty: false,
            history: History::default(),
        })
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Line contents without the trailing newline.
    pub fn line_text(&self, line: usize) -> String {
        if line >= self.rope.len_lines() {
            return String::new();
        }
        self.rope
            .line(line)
            .to_string()
            .trim_end_matches('\n')
            .trim_end_matches('\r')
            .to_string()
    }

    /// Absolute char offset of a `Position`, clamping out-of-range input.
    fn char_offset(&self, pos: Position) -> usize {
        let line = pos.line.min(self.rope.len_lines().saturating_sub(1));
        let line_start = self.rope.line_to_char(line);
        let text = self.line_text(line);
        let chars_before: usize = text
            .graphemes(true)
            .take(pos.col)
            .map(|g| g.chars().count())
            .sum();
        line_start + chars_before
    }

    pub fn insert_char(&mut self, pos: Position, ch: char) {
        self.history.record(self.rope.to_string());
        let offset = self.char_offset(pos);
        self.rope.insert_char(offset, ch);
        self.dirty = true;
    }

    /// Delete the grapheme immediately before `pos` (backspace).
    pub fn delete_before(&mut self, pos: Position) {
        let offset = self.char_offset(pos);
        if offset == 0 {
            return;
        }
        let text = self.line_text(pos.line);
        let n = if pos.col == 0 {
            1 // joining with the previous line: remove the newline
        } else {
            text.graphemes(true)
                .nth(pos.col - 1)
                .map_or(1, |g| g.chars().count())
        };
        self.history.record(self.rope.to_string());
        self.rope.remove(offset - n..offset);
        self.dirty = true;
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.history.undo(self.rope.to_string()) {
            self.rope = Rope::from_str(&prev);
            self.dirty = true;
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.history.redo(self.rope.to_string()) {
            self.rope = Rope::from_str(&next);
            self.dirty = true;
        }
    }

    pub fn save(&mut self) -> Result<()> {
        let path = self.path.as_ref().context("buffer has no path to save to")?;
        std::fs::write(path, self.rope.to_string())
            .with_context(|| format!("writing {}", path.display()))?;
        self.dirty = false;
        Ok(())
    }
}
```

`crates/typ-buffer/src/lib.rs`:

```rust
pub mod buffer;
pub mod position;
pub mod undo;

pub use buffer::TextBuffer;
pub use position::{
    Position, display_to_grapheme_col, display_width, display_width_with_tabs,
    grapheme_to_display_col,
};
```

- [x] **Step 4: Carry the width tests across**

Copy `spikes/m0-feel/tests/width.rs` to `crates/typ-buffer/tests/width.rs`, changing the
import to:

```rust
use typ_buffer::{display_to_grapheme_col, display_width, grapheme_to_display_col};
```

- [x] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p typ-buffer`

Expected: PASS — 10 buffer tests plus the 9 width tests carried over.

Actual: PASS — 10 + 9. One deviation: `TextBuffer::from_str` trips
`clippy::should_implement_trait`, allowed at the method with a comment. The name
matches `Rope::from_str` and construction is infallible, so the `FromStr` trait's
`Result` shape would be wrong here.

- [x] **Step 6: Commit**

```bash
git add crates/typ-buffer
git commit -m "feat(buffer): rope-backed text buffer with grapheme positions and undo"
```

---

### Task 11: `typ-registry` — filetype to handler

**Files:**
- Create: `crates/typ-registry/{Cargo.toml,src/lib.rs,tests/registry.rs}`

**Interfaces:**
- Consumes: `typ_core::HandlerId`
- Produces:
  - `typ_registry::Registry::with_builtins() -> Registry`
  - `Registry::register(&mut self, ext: &'static str, handler: HandlerId)`
  - `Registry::handler_for(&self, path: &Path) -> HandlerId` — falls back to `HandlerId("editor")`

- [x] **Step 1: Write the failing tests**

`crates/typ-registry/tests/registry.rs`:

```rust
use std::path::Path;

use typ_core::HandlerId;
use typ_registry::Registry;

#[test]
fn unknown_extensions_fall_back_to_the_editor() {
    let r = Registry::with_builtins();
    assert_eq!(r.handler_for(Path::new("a.zzz")), HandlerId("editor"));
}

#[test]
fn files_without_an_extension_fall_back_to_the_editor() {
    let r = Registry::with_builtins();
    assert_eq!(r.handler_for(Path::new("Makefile")), HandlerId("editor"));
}

#[test]
fn known_text_extensions_route_to_the_editor() {
    let r = Registry::with_builtins();
    assert_eq!(r.handler_for(Path::new("main.rs")), HandlerId("editor"));
}

#[test]
fn registering_a_handler_overrides_the_fallback() {
    let mut r = Registry::with_builtins();
    r.register("png", HandlerId("image"));
    assert_eq!(r.handler_for(Path::new("logo.png")), HandlerId("image"));
}

#[test]
fn extension_matching_is_case_insensitive() {
    let mut r = Registry::with_builtins();
    r.register("png", HandlerId("image"));
    assert_eq!(r.handler_for(Path::new("LOGO.PNG")), HandlerId("image"));
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p typ-registry`

Expected: FAIL — the crate does not exist.

- [x] **Step 3: Implement the crate**

`crates/typ-registry/Cargo.toml`:

```toml
[package]
name = "typ-registry"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
typ-core.workspace = true
```

`crates/typ-registry/src/lib.rs`:

```rust
use std::collections::HashMap;
use std::path::Path;

use typ_core::HandlerId;

/// The fallback used when no handler claims a path.
pub const EDITOR: HandlerId = HandlerId("editor");

/// Maps file extensions to the panel type that opens them.
///
/// This is the seam that keeps `PanelEvent` small: a new viewer registers here
/// rather than adding an enum variant, and the same path will later admit
/// externally provided handlers without any core change.
pub struct Registry {
    by_extension: HashMap<String, HandlerId>,
}

impl Registry {
    pub fn with_builtins() -> Self {
        // One content panel ships today. Entries exist so the mechanism is
        // exercised from day one rather than bolted on later.
        let mut by_extension = HashMap::new();
        for ext in ["rs", "toml", "md", "txt", "json", "yaml", "yml"] {
            by_extension.insert(ext.to_string(), EDITOR);
        }
        Self { by_extension }
    }

    pub fn register(&mut self, ext: &'static str, handler: HandlerId) {
        self.by_extension.insert(ext.to_lowercase(), handler);
    }

    pub fn handler_for(&self, path: &Path) -> HandlerId {
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_lowercase)
            .and_then(|e| self.by_extension.get(&e).copied())
            .unwrap_or(EDITOR)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::with_builtins()
    }
}
```

- [x] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p typ-registry`

Expected: PASS, 5 tests.

Actual: PASS, 5 tests. Clippy clean. No deviations.

- [x] **Step 5: Commit**

```bash
git add crates/typ-registry
git commit -m "feat(registry): map file extensions to panel handlers"
```

---

### Task 12: `typ-panel-editor` — the editor panel

**Files:**
- Create: `crates/typ-panel-editor/{Cargo.toml,src/lib.rs,tests/editor.rs}`

**Interfaces:**
- Consumes: `typ_core::{Panel, RenderContext, PanelEvent, KeyChord}`, `typ_buffer::*`
- Produces:
  - `typ_panel_editor::EditorPanel::{from_path, from_str, cursor, top_line}`

- [x] **Step 1: Write the failing tests**

`crates/typ-panel-editor/tests/editor.rs`:

```rust
use crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use typ_buffer::Position;
use typ_core::{KeyChord, Panel, PanelEvent};
use typ_panel_editor::EditorPanel;

fn chord(code: KeyCode) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn typing_inserts_text_and_advances_the_cursor() {
    let mut p = EditorPanel::from_str("\n");
    p.handle_key(chord(KeyCode::Char('h')));
    p.handle_key(chord(KeyCode::Char('i')));
    assert_eq!(p.cursor(), Position { line: 0, col: 2 });
}

#[test]
fn arrow_keys_move_the_cursor() {
    let mut p = EditorPanel::from_str("abc\ndef\n");
    p.handle_key(chord(KeyCode::Right));
    p.handle_key(chord(KeyCode::Down));
    assert_eq!(p.cursor(), Position { line: 1, col: 1 });
}

#[test]
fn cursor_cannot_move_left_past_the_start() {
    let mut p = EditorPanel::from_str("abc\n");
    p.handle_key(chord(KeyCode::Left));
    assert_eq!(p.cursor(), Position { line: 0, col: 0 });
}

#[test]
fn moving_down_clamps_the_column_to_a_shorter_line() {
    let mut p = EditorPanel::from_str("abcdef\nab\n");
    for _ in 0..5 {
        p.handle_key(chord(KeyCode::Right));
    }
    p.handle_key(chord(KeyCode::Down));
    assert_eq!(p.cursor(), Position { line: 1, col: 2 });
}

#[test]
fn backspace_deletes_the_previous_grapheme() {
    let mut p = EditorPanel::from_str("\n");
    p.handle_key(chord(KeyCode::Char('a')));
    p.handle_key(chord(KeyCode::Char('b')));
    p.handle_key(chord(KeyCode::Backspace));
    assert_eq!(p.cursor(), Position { line: 0, col: 1 });
}

#[test]
fn every_key_press_requests_a_redraw() {
    let mut p = EditorPanel::from_str("\n");
    assert_eq!(p.handle_key(chord(KeyCode::Char('a'))), vec![PanelEvent::NeedsRedraw]);
}

#[test]
fn clicking_places_the_cursor_at_that_position() {
    let mut p = EditorPanel::from_str("hello\nworld\n");
    let area = Rect::new(0, 0, 40, 10);
    let ev = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 3,
        row: 1,
        modifiers: KeyModifiers::NONE,
    };
    p.handle_mouse(ev, area);
    assert_eq!(p.cursor(), Position { line: 1, col: 3 });
}

#[test]
fn clicking_inside_a_wide_char_selects_that_char() {
    let mut p = EditorPanel::from_str("日本語\n");
    let area = Rect::new(0, 0, 40, 10);
    let ev = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 1, // right half of the first CJK grapheme
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    p.handle_mouse(ev, area);
    assert_eq!(p.cursor(), Position { line: 0, col: 0 });
}

#[test]
fn scrolling_moves_the_viewport_not_the_cursor() {
    let text = (0..100).map(|i| format!("line {i}\n")).collect::<String>();
    let mut p = EditorPanel::from_str(&text);
    p.handle_scroll(5, Rect::new(0, 0, 40, 10));
    assert_eq!(p.top_line(), 5);
    assert_eq!(p.cursor(), Position { line: 0, col: 0 });
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p typ-panel-editor`

Expected: FAIL — the crate does not exist.

- [x] **Step 3: Implement the panel**

`crates/typ-panel-editor/Cargo.toml`:

```toml
[package]
name = "typ-panel-editor"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
crossterm.workspace = true
ratatui.workspace = true
unicode-segmentation.workspace = true
typ-buffer.workspace = true
typ-core.workspace = true
```

`crates/typ-panel-editor/src/lib.rs`:

```rust
use std::any::Any;
use std::path::Path;

use anyhow::Result;
use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};
use unicode_segmentation::UnicodeSegmentation;
use typ_buffer::{Position, TextBuffer, display_to_grapheme_col, grapheme_to_display_col};
use typ_core::{KeyChord, Panel, PanelEvent, RenderContext};

const TAB_WIDTH: usize = 4;

pub struct EditorPanel {
    buffer: TextBuffer,
    cursor: Position,
    top_line: usize,
    /// Display column the cursor "wants", preserved across vertical movement
    /// so passing through short lines does not permanently lose the column.
    goal_col: Option<usize>,
    height: usize,
}

impl EditorPanel {
    pub fn from_str(s: &str) -> Self {
        Self::new(TextBuffer::from_str(s))
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        Ok(Self::new(TextBuffer::from_path(path)?))
    }

    fn new(buffer: TextBuffer) -> Self {
        Self {
            buffer,
            cursor: Position::default(),
            top_line: 0,
            goal_col: None,
            height: 0,
        }
    }

    pub fn cursor(&self) -> Position {
        self.cursor
    }

    pub fn top_line(&self) -> usize {
        self.top_line
    }

    pub fn save(&mut self) -> Result<()> {
        self.buffer.save()
    }

    fn line_grapheme_count(&self, line: usize) -> usize {
        self.buffer.line_text(line).graphemes(true).count()
    }

    fn last_line(&self) -> usize {
        self.buffer.line_count().saturating_sub(1)
    }

    /// Keep the cursor inside the viewport after any movement.
    fn scroll_to_cursor(&mut self) {
        if self.height == 0 {
            return;
        }
        if self.cursor.line < self.top_line {
            self.top_line = self.cursor.line;
        } else if self.cursor.line >= self.top_line + self.height {
            self.top_line = self.cursor.line - self.height + 1;
        }
    }

    fn move_vertical(&mut self, delta: i32) {
        let goal = self.goal_col.unwrap_or_else(|| {
            grapheme_to_display_col(
                &self.buffer.line_text(self.cursor.line),
                self.cursor.col,
                TAB_WIDTH,
            )
        });
        let next =
            (self.cursor.line as i64 + delta as i64).clamp(0, self.last_line() as i64) as usize;
        self.cursor.line = next;
        self.cursor.col = display_to_grapheme_col(&self.buffer.line_text(next), goal, TAB_WIDTH);
        self.goal_col = Some(goal);
        self.scroll_to_cursor();
    }
}

impl Panel for EditorPanel {
    fn name(&self) -> &'static str {
        "editor"
    }

    fn title(&self) -> String {
        let name = self
            .buffer
            .path()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("untitled")
            .to_string();
        if self.buffer.is_dirty() {
            format!("{name} *")
        } else {
            name
        }
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &RenderContext) {
        self.height = area.height as usize;
        let end = (self.top_line + self.height).min(self.buffer.line_count());
        let lines: Vec<Line> = (self.top_line..end)
            .map(|i| Line::raw(self.buffer.line_text(i)))
            .collect();
        Paragraph::new(lines)
            .style(Style::default().fg(ctx.theme.fg).bg(ctx.theme.bg))
            .render(area, buf);
    }

    fn handle_key(&mut self, chord: KeyChord) -> Vec<PanelEvent> {
        match chord.raw.code {
            KeyCode::Char(c) => {
                self.buffer.insert_char(self.cursor, c);
                self.cursor.col += 1;
                self.goal_col = None;
            }
            KeyCode::Backspace => {
                if self.cursor.col > 0 {
                    self.buffer.delete_before(self.cursor);
                    self.cursor.col -= 1;
                }
                self.goal_col = None;
            }
            KeyCode::Left => {
                if self.cursor.col > 0 {
                    self.cursor.col -= 1;
                } else if self.cursor.line > 0 {
                    self.cursor.line -= 1;
                    self.cursor.col = self.line_grapheme_count(self.cursor.line);
                }
                self.goal_col = None;
            }
            KeyCode::Right => {
                if self.cursor.col < self.line_grapheme_count(self.cursor.line) {
                    self.cursor.col += 1;
                } else if self.cursor.line < self.last_line() {
                    self.cursor.line += 1;
                    self.cursor.col = 0;
                }
                self.goal_col = None;
            }
            KeyCode::Up => self.move_vertical(-1),
            KeyCode::Down => self.move_vertical(1),
            _ => {}
        }
        self.scroll_to_cursor();
        vec![PanelEvent::NeedsRedraw]
    }

    fn handle_mouse(&mut self, event: MouseEvent, panel_area: Rect) -> Vec<PanelEvent> {
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return Vec::new();
        }
        let row = event.row.saturating_sub(panel_area.y) as usize;
        let col = event.column.saturating_sub(panel_area.x) as usize;
        let line = (self.top_line + row).min(self.last_line());
        self.cursor = Position {
            line,
            col: display_to_grapheme_col(&self.buffer.line_text(line), col, TAB_WIDTH),
        };
        self.goal_col = None;
        vec![PanelEvent::NeedsRedraw]
    }

    fn handle_scroll(&mut self, delta: i32, _panel_area: Rect) -> Vec<PanelEvent> {
        let max_top = self.buffer.line_count().saturating_sub(self.height.max(1));
        self.top_line = (self.top_line as i64 + delta as i64).clamp(0, max_top as i64) as usize;
        vec![PanelEvent::NeedsRedraw]
    }

    fn needs_close_confirmation(&self) -> Option<String> {
        self.buffer
            .is_dirty()
            .then(|| "Unsaved changes. Close anyway?".to_string())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
```

- [x] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p typ-panel-editor`

Expected: PASS, 9 tests.

Actual: PASS, 9 tests. Clippy clean. One addition: `EditorPanel::from_str`
carries the same `allow(clippy::should_implement_trait)` as `TextBuffer::from_str`,
for the same reason.

`scrolling_moves_the_viewport_not_the_cursor` runs without a render pass, so `height` is 0.
The `self.height.max(1)` in `handle_scroll` is what makes that case work.

- [x] **Step 5: Commit**

```bash
git add crates/typ-panel-editor
git commit -m "feat(editor): editor panel with keyboard, mouse, and scroll handling"
```

---

### Task 13: `typ-panel-tree` — the file tree panel

**Files:**
- Create: `crates/typ-panel-tree/{Cargo.toml,src/lib.rs,tests/tree.rs}`

**Interfaces:**
- Consumes: `typ_core::{Panel, RenderContext, PanelEvent, KeyChord}`
- Produces: `typ_panel_tree::TreePanel::{new, selected, entry_count, root}`

- [x] **Step 1: Write the failing tests**

`crates/typ-panel-tree/tests/tree.rs`:

```rust
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use typ_core::{KeyChord, Panel, PanelEvent};
use typ_panel_tree::TreePanel;

fn fixture() -> PathBuf {
    let dir = std::env::temp_dir().join("typ-tree-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("a.rs"), "").unwrap();
    std::fs::write(dir.join("b.rs"), "").unwrap();
    std::fs::write(dir.join("sub/c.rs"), "").unwrap();
    dir
}

fn chord(code: KeyCode) -> KeyChord {
    KeyChord::from_event(KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn lists_entries_in_the_root_directory() {
    let t = TreePanel::new(&fixture()).unwrap();
    // sub/, a.rs, b.rs — directories sort first.
    assert_eq!(t.entry_count(), 3);
}

#[test]
fn directories_sort_before_files() {
    let t = TreePanel::new(&fixture()).unwrap();
    assert!(t.selected().unwrap().is_dir());
}

#[test]
fn arrow_keys_move_the_selection() {
    let mut t = TreePanel::new(&fixture()).unwrap();
    t.handle_key(chord(KeyCode::Down));
    assert_eq!(t.selected().unwrap().file_name().unwrap(), "a.rs");
}

#[test]
fn selection_clamps_at_the_end_of_the_list() {
    let mut t = TreePanel::new(&fixture()).unwrap();
    for _ in 0..50 {
        t.handle_key(chord(KeyCode::Down));
    }
    assert_eq!(t.selected().unwrap().file_name().unwrap(), "b.rs");
}

#[test]
fn pressing_enter_on_a_file_emits_open_file() {
    let mut t = TreePanel::new(&fixture()).unwrap();
    t.handle_key(chord(KeyCode::Down)); // a.rs
    let events = t.handle_key(chord(KeyCode::Enter));
    assert!(matches!(
        events.first(),
        Some(PanelEvent::OpenFile { line: 0, col: 0, .. })
    ));
}

#[test]
fn pressing_enter_on_a_directory_does_not_emit_open_file() {
    let mut t = TreePanel::new(&fixture()).unwrap();
    let events = t.handle_key(chord(KeyCode::Enter));
    assert!(!events.iter().any(|e| matches!(e, PanelEvent::OpenFile { .. })));
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p typ-panel-tree`

Expected: FAIL — the crate does not exist.

- [x] **Step 3: Implement the panel**

`crates/typ-panel-tree/Cargo.toml`:

```toml
[package]
name = "typ-panel-tree"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
crossterm.workspace = true
ratatui.workspace = true
typ-core.workspace = true
```

`crates/typ-panel-tree/src/lib.rs`:

```rust
use std::any::Any;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, MouseButton, MouseEvent, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Widget};
use typ_core::{KeyChord, Panel, PanelEvent, RenderContext};

pub struct TreePanel {
    root: PathBuf,
    entries: Vec<PathBuf>,
    selected: usize,
    top_line: usize,
    height: usize,
}

impl TreePanel {
    pub fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            root: root.to_path_buf(),
            entries: read_dir_sorted(root)?,
            selected: 0,
            top_line: 0,
            height: 0,
        })
    }

    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }

    pub fn selected(&self) -> Option<&Path> {
        self.entries.get(self.selected).map(PathBuf::as_path)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn move_selection(&mut self, delta: i32) {
        let last = self.entries.len().saturating_sub(1) as i64;
        self.selected = (self.selected as i64 + delta as i64).clamp(0, last) as usize;
        if self.height > 0 {
            if self.selected < self.top_line {
                self.top_line = self.selected;
            } else if self.selected >= self.top_line + self.height {
                self.top_line = self.selected - self.height + 1;
            }
        }
    }

    /// Emit an open event when the selection is a file.
    fn activate(&self) -> Vec<PanelEvent> {
        match self.selected() {
            Some(p) if p.is_file() => vec![PanelEvent::OpenFile {
                path: p.to_path_buf(),
                line: 0,
                col: 0,
            }],
            // Directory expansion is not part of the walking skeleton.
            _ => vec![PanelEvent::NeedsRedraw],
        }
    }
}

/// Directories first, then files, each alphabetical.
fn read_dir_sorted(root: &Path) -> Result<Vec<PathBuf>> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(root)
        .with_context(|| format!("reading {}", root.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort_by_key(|p| {
        (
            !p.is_dir(),
            p.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_lowercase(),
        )
    });
    Ok(entries)
}

impl Panel for TreePanel {
    fn name(&self) -> &'static str {
        "tree"
    }

    fn title(&self) -> String {
        self.root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("/")
            .to_string()
    }

    fn render(&mut self, area: Rect, buf: &mut Buffer, ctx: &RenderContext) {
        self.height = area.height as usize;
        let end = (self.top_line + self.height).min(self.entries.len());
        let lines: Vec<Line> = (self.top_line..end)
            .map(|i| {
                let p = &self.entries[i];
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("?");
                let label = if p.is_dir() {
                    format!("{name}/")
                } else {
                    name.to_string()
                };
                let style = if i == self.selected {
                    Style::default()
                        .fg(ctx.theme.selection_fg)
                        .bg(ctx.theme.selection_bg)
                } else {
                    Style::default().fg(ctx.theme.fg)
                };
                Line::styled(label, style)
            })
            .collect();
        Paragraph::new(lines)
            .style(Style::default().bg(ctx.theme.bg))
            .render(area, buf);
    }

    fn handle_key(&mut self, chord: KeyChord) -> Vec<PanelEvent> {
        match chord.raw.code {
            KeyCode::Down => self.move_selection(1),
            KeyCode::Up => self.move_selection(-1),
            KeyCode::Enter => return self.activate(),
            _ => return Vec::new(),
        }
        vec![PanelEvent::NeedsRedraw]
    }

    fn handle_mouse(&mut self, event: MouseEvent, panel_area: Rect) -> Vec<PanelEvent> {
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return Vec::new();
        }
        let row = event.row.saturating_sub(panel_area.y) as usize;
        let idx = self.top_line + row;
        if idx >= self.entries.len() {
            return Vec::new();
        }
        // Click selects; clicking the already-selected entry activates it,
        // matching how GUI file trees behave.
        if idx == self.selected {
            return self.activate();
        }
        self.selected = idx;
        vec![PanelEvent::NeedsRedraw]
    }

    fn handle_scroll(&mut self, delta: i32, _panel_area: Rect) -> Vec<PanelEvent> {
        let max_top = self.entries.len().saturating_sub(self.height.max(1));
        self.top_line = (self.top_line as i64 + delta as i64).clamp(0, max_top as i64) as usize;
        vec![PanelEvent::NeedsRedraw]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
```

- [x] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p typ-panel-tree`

Expected: PASS, 6 tests.

Actual: PASS, 6 tests. Clippy clean. One deviation from the spec above: the test
`fixture()` takes a name and builds one directory per test. As written it was a
single shared path that each test deleted and recreated, which races under
cargo's test threads.

- [x] **Step 5: Commit**

```bash
git add crates/typ-panel-tree
git commit -m "feat(tree): file tree panel with selection and open events"
```

---

### Task 14: `typ-app` — event loop, focus, and dispatch

**Files:**
- Create: `crates/typ-app/{Cargo.toml,src/lib.rs,src/app.rs,src/layout.rs,src/run.rs,tests/app.rs}`

**Interfaces:**
- Consumes: everything above
- Produces:
  - `typ_app::App::{new, open_path, apply, should_quit, focused_name, editor_title,
    cycle_focus, render, areas, tree_mut, editor_mut, focused_mut}`
  - `typ_app::layout::split(area: Rect) -> (Rect, Rect)` — `(tree_area, editor_area)`
  - `typ_app::run::run(app: App) -> anyhow::Result<()>`

- [x] **Step 1: Write the failing tests**

`crates/typ-app/tests/app.rs`:

```rust
use std::path::PathBuf;

use ratatui::layout::Rect;
use typ_app::App;
use typ_app::layout::split;
use typ_core::PanelEvent;

fn fixture() -> PathBuf {
    let dir = std::env::temp_dir().join("typ-app-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hello.rs"), "fn main() {}\n").unwrap();
    dir
}

#[test]
fn a_new_app_focuses_the_tree() {
    let app = App::new(&fixture()).unwrap();
    assert_eq!(app.focused_name(), "tree");
}

#[test]
fn cycling_focus_moves_to_the_editor_and_back() {
    let mut app = App::new(&fixture()).unwrap();
    app.cycle_focus();
    assert_eq!(app.focused_name(), "editor");
    app.cycle_focus();
    assert_eq!(app.focused_name(), "tree");
}

#[test]
fn applying_quit_sets_the_quit_flag() {
    let mut app = App::new(&fixture()).unwrap();
    assert!(!app.should_quit());
    app.apply(vec![PanelEvent::Quit]).unwrap();
    assert!(app.should_quit());
}

#[test]
fn open_file_event_loads_the_file_into_the_editor() {
    let dir = fixture();
    let mut app = App::new(&dir).unwrap();
    app.apply(vec![PanelEvent::OpenFile {
        path: dir.join("hello.rs"),
        line: 0,
        col: 0,
    }])
    .unwrap();
    assert_eq!(app.editor_title(), "hello.rs");
}

#[test]
fn opening_a_file_moves_focus_to_the_editor() {
    let dir = fixture();
    let mut app = App::new(&dir).unwrap();
    app.apply(vec![PanelEvent::OpenFile {
        path: dir.join("hello.rs"),
        line: 0,
        col: 0,
    }])
    .unwrap();
    assert_eq!(app.focused_name(), "editor");
}

#[test]
fn layout_gives_the_tree_a_fixed_width_sidebar() {
    let (tree, editor) = split(Rect::new(0, 0, 100, 30));
    assert_eq!(tree.width, 30);
    assert_eq!(editor.x, 30);
    assert_eq!(editor.width, 70);
}

#[test]
fn layout_shrinks_the_sidebar_on_narrow_terminals() {
    let (tree, editor) = split(Rect::new(0, 0, 40, 30));
    assert!(tree.width < 30);
    assert!(editor.width > 0);
}
```

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p typ-app`

Expected: FAIL — the crate does not exist.

- [x] **Step 3: Create the crate and implement layout.rs**

`crates/typ-app/Cargo.toml`:

```toml
[package]
name = "typ-app"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
anyhow.workspace = true
crossterm.workspace = true
ratatui.workspace = true
typ-core.workspace = true
typ-panel-editor.workspace = true
typ-panel-tree.workspace = true
typ-registry.workspace = true
```

`crates/typ-app/src/layout.rs`:

```rust
use ratatui::layout::Rect;

/// Preferred sidebar width in columns.
const SIDEBAR_WIDTH: u16 = 30;
/// Below this total width the sidebar takes a share instead of a fixed size.
const NARROW_THRESHOLD: u16 = 60;

/// Split the frame into `(tree_area, editor_area)`.
///
/// A fixed sidebar matches what people arriving from GUI editors expect. On
/// narrow terminals a fixed 30 columns would leave nothing for the editor, so
/// it degrades to a third of the width.
pub fn split(area: Rect) -> (Rect, Rect) {
    let sidebar = if area.width < NARROW_THRESHOLD {
        (area.width / 3).max(1)
    } else {
        SIDEBAR_WIDTH
    };
    let tree = Rect::new(area.x, area.y, sidebar, area.height);
    let editor = Rect::new(
        area.x + sidebar,
        area.y,
        area.width.saturating_sub(sidebar),
        area.height,
    );
    (tree, editor)
}
```

- [x] **Step 4: Implement app.rs and lib.rs**

`crates/typ-app/src/app.rs`:

```rust
use std::path::Path;

use anyhow::Result;
use ratatui::layout::Rect;
use typ_core::{Panel, PanelEvent, RenderContext, ThemeColors};
use typ_panel_editor::EditorPanel;
use typ_panel_tree::TreePanel;
use typ_registry::Registry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tree,
    Editor,
}

pub struct App {
    tree: TreePanel,
    editor: EditorPanel,
    registry: Registry,
    theme: ThemeColors,
    focus: Focus,
    quit: bool,
}

impl App {
    pub fn new(root: &Path) -> Result<Self> {
        Ok(Self {
            tree: TreePanel::new(root)?,
            editor: EditorPanel::from_str(""),
            registry: Registry::with_builtins(),
            theme: ThemeColors::default(),
            focus: Focus::Tree,
            quit: false,
        })
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn focused_name(&self) -> &'static str {
        match self.focus {
            Focus::Tree => self.tree.name(),
            Focus::Editor => self.editor.name(),
        }
    }

    pub fn editor_title(&self) -> String {
        self.editor.title()
    }

    pub fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Tree => Focus::Editor,
            Focus::Editor => Focus::Tree,
        };
    }

    pub fn open_path(&mut self, path: &Path) -> Result<()> {
        // The registry decides the handler. There is one content panel today,
        // but the lookup runs from day one so adding viewers never touches this.
        let _handler = self.registry.handler_for(path);
        self.editor = EditorPanel::from_path(path)?;
        self.focus = Focus::Editor;
        Ok(())
    }

    /// Process events emitted by panels.
    pub fn apply(&mut self, events: Vec<PanelEvent>) -> Result<()> {
        for event in events {
            match event {
                PanelEvent::Quit => self.quit = true,
                PanelEvent::OpenFile { path, .. } | PanelEvent::OpenWith { path, .. } => {
                    self.open_path(&path)?;
                }
                // Redraw happens every loop pass in the walking skeleton.
                PanelEvent::NeedsRedraw => {}
                // Two fixed panels, so these are no-ops until the layout
                // system lands.
                PanelEvent::CloseSelf | PanelEvent::Focus(_) => {}
                PanelEvent::RunCommand { .. } | PanelEvent::Notify { .. } => {}
            }
        }
        Ok(())
    }

    pub fn render(&mut self, frame: &mut ratatui::Frame) {
        let (tree_area, editor_area) = crate::layout::split(frame.area());
        let (w, h) = (frame.area().width, frame.area().height);

        let tree_ctx = RenderContext {
            theme: &self.theme,
            is_focused: self.focus == Focus::Tree,
            panel_index: 0,
            terminal_width: w,
            terminal_height: h,
        };
        self.tree.render(tree_area, frame.buffer_mut(), &tree_ctx);

        let editor_ctx = RenderContext {
            theme: &self.theme,
            is_focused: self.focus == Focus::Editor,
            panel_index: 1,
            terminal_width: w,
            terminal_height: h,
        };
        self.editor.render(editor_area, frame.buffer_mut(), &editor_ctx);
    }

    /// Areas for hit-testing mouse events, in the same order as `render`.
    pub fn areas(&self, area: Rect) -> (Rect, Rect) {
        crate::layout::split(area)
    }

    pub fn tree_mut(&mut self) -> &mut TreePanel {
        &mut self.tree
    }

    pub fn editor_mut(&mut self) -> &mut EditorPanel {
        &mut self.editor
    }

    pub fn focused_mut(&mut self) -> &mut dyn Panel {
        match self.focus {
            Focus::Tree => &mut self.tree,
            Focus::Editor => &mut self.editor,
        }
    }
}
```

`crates/typ-app/src/lib.rs`:

```rust
pub mod app;
pub mod layout;
pub mod run;

pub use app::{App, Focus};
```

- [x] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p typ-app`

Expected: PASS, 7 tests.

Actual: PASS, 7 tests. Same fixture change as Task 13 — one temp directory per
test rather than a shared path, which races under cargo's test threads.

- [x] **Step 6: Implement run.rs — the event loop**

`crates/typ-app/src/run.rs`:

```rust
use std::io::{Write, stdout};
use std::time::Duration;

use anyhow::Result;
use crossterm::ExecutableCommand;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseEventKind,
};
use ratatui::layout::Rect;
use typ_core::{KeyChord, Panel, PanelEvent};

use crate::app::{App, Focus};

/// Enter/leave synchronized output (CSI 2026) around a frame so partial
/// repaints are presented atomically. Terminals without support ignore it.
fn begin_frame() {
    let _ = write!(stdout(), "\x1b[?2026h");
}

fn end_frame() {
    let mut out = stdout();
    let _ = write!(out, "\x1b[?2026l");
    let _ = out.flush();
}

pub fn run(mut app: App) -> Result<()> {
    let mut terminal = ratatui::init();
    stdout().execute(EnableMouseCapture)?;

    let result = event_loop(&mut terminal, &mut app);

    stdout().execute(DisableMouseCapture)?;
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    loop {
        begin_frame();
        terminal.draw(|frame| app.render(frame))?;
        end_frame();

        if app.should_quit() {
            return Ok(());
        }

        let mut events: Vec<PanelEvent> = Vec::new();

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                // Application bindings win before panel dispatch.
                let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                match key.code {
                    KeyCode::Char('q') if ctrl => events.push(PanelEvent::Quit),
                    KeyCode::Char('s') if ctrl => {
                        if app.focus() == Focus::Editor {
                            app.editor_mut().save()?;
                        }
                    }
                    KeyCode::Tab => app.cycle_focus(),
                    _ => {
                        events = app.focused_mut().handle_key(KeyChord::from_event(key));
                    }
                }
            }
            Event::Mouse(m) => {
                let size = terminal.size()?;
                let full = Rect::new(0, 0, size.width, size.height);
                let (tree_area, editor_area) = app.areas(full);
                let in_tree = m.column < tree_area.width;

                match m.kind {
                    // Coalesce wheel events into a single scroll call so a fast
                    // wheel does not queue one repaint per notch.
                    MouseEventKind::ScrollDown | MouseEventKind::ScrollUp => {
                        let mut delta: i32 =
                            if matches!(m.kind, MouseEventKind::ScrollDown) { 3 } else { -3 };
                        while event::poll(Duration::from_millis(0))? {
                            match event::read()? {
                                Event::Mouse(next) => match next.kind {
                                    MouseEventKind::ScrollDown => delta += 3,
                                    MouseEventKind::ScrollUp => delta -= 3,
                                    _ => break,
                                },
                                _ => break,
                            }
                        }
                        events = if in_tree {
                            app.tree_mut().handle_scroll(delta, tree_area)
                        } else {
                            app.editor_mut().handle_scroll(delta, editor_area)
                        };
                    }
                    _ => {
                        // A click both focuses the panel and is delivered to it,
                        // so clicking into an unfocused panel takes one click.
                        if in_tree {
                            if app.focus() != Focus::Tree {
                                app.cycle_focus();
                            }
                            events = app.tree_mut().handle_mouse(m, tree_area);
                        } else {
                            if app.focus() != Focus::Editor {
                                app.cycle_focus();
                            }
                            events = app.editor_mut().handle_mouse(m, editor_area);
                        }
                    }
                }
            }
            _ => {}
        }

        app.apply(events)?;
    }
}
```

- [x] **Step 7: Verify the workspace builds**

Run: `cargo build --workspace`

Expected: builds with no errors.

Actual: `cargo build --workspace` and `cargo clippy --workspace --all-targets
-- -D warnings` both clean.

- [x] **Step 8: Commit**

```bash
git add crates/typ-app
git commit -m "feat(app): event loop with focus, dispatch, and scroll coalescing"
```

---

### Task 15: The `typ` binary and `$EDITOR` invariants

**Files:**
- Create: `crates/typ/{Cargo.toml,src/main.rs,tests/cli.rs}`

**Interfaces:**
- Consumes: `typ_app::{App, run}`
- Produces: the `typ` binary — `typ` (current directory), `typ <dir>`, `typ <file>`

- [x] **Step 1: Write the failing tests**

`crates/typ/tests/cli.rs`:

```rust
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_typ")
}

#[test]
fn missing_path_exits_nonzero_with_a_message_on_stderr() {
    let out = Command::new(bin())
        .arg("definitely/does/not/exist.rs")
        .output()
        .expect("binary runs");
    assert!(!out.status.success(), "expected a non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("does not exist"),
        "stderr was: {stderr}"
    );
}

#[test]
fn version_flag_prints_and_exits_zero() {
    let out = Command::new(bin()).arg("--version").output().expect("binary runs");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("typ"));
}

#[test]
fn help_flag_names_the_binary() {
    let out = Command::new(bin()).arg("--help").output().expect("binary runs");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("typ"));
}
```

These are the `$EDITOR` invariants under test: a failure must exit non-zero so a calling
`git commit` aborts rather than committing an empty message.

- [x] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p typ-editor`

Expected: FAIL — the crate does not exist.

- [x] **Step 3: Implement the binary**

`crates/typ/Cargo.toml`:

```toml
[package]
name = "typ-editor"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "TYPE — Terminal-Yoked Programming Environment"

[[bin]]
name = "typ"
path = "src/main.rs"

[dependencies]
anyhow.workspace = true
typ-app.workspace = true
```

The crate is `typ-editor` (publishable — `typ` on crates.io is held by an abandoned 2020
crate) while the binary is `typ`.

`crates/typ/src/main.rs`:

```rust
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Result, bail};
use typ_app::{App, run::run};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
typ — TYPE, the Terminal-Yoked Programming Environment

USAGE:
    typ [PATH]

ARGS:
    PATH    File to open, or directory to open as a workspace.
            Defaults to the current directory.

OPTIONS:
    -h, --help       Print this help
    -V, --version    Print version
";

/// Exit codes are load-bearing: TYPE is usable as `$EDITOR`, and a caller such
/// as `git commit` must abort when the editor fails rather than proceeding with
/// an empty message.
fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("typ: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn real_main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    for a in &args {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("typ {VERSION}");
                return Ok(());
            }
            _ => {}
        }
    }

    let target: PathBuf = args
        .first()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    if !target.exists() {
        bail!("{} does not exist", target.display());
    }

    let (root, file) = if target.is_dir() {
        (target.clone(), None)
    } else {
        let parent = target.parent().unwrap_or(Path::new(".")).to_path_buf();
        (parent, Some(target.clone()))
    };

    let mut app = App::new(&root)?;
    if let Some(f) = file {
        app.open_path(&f)?;
    }

    // Blocks until the user exits. No daemon detach — a caller waiting on
    // $EDITOR must see this process end when editing ends.
    run(app)
}
```

- [x] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p typ-editor`

Expected: PASS, 3 tests. Note the crate is `typ-editor` while `CARGO_BIN_EXE_typ` refers to
the binary name.

Actual: PASS, 3 tests.

- [ ] **Step 5: Verify the walking skeleton by hand**

```bash
cargo build --release
./target/release/typ.exe .
```

Verify all of:
- File tree renders left, editor right.
- `Tab` cycles focus.
- Arrow keys move the tree selection; `Enter` on a file opens it.
- Clicking a tree entry selects it; clicking it again opens it.
- Clicking in the editor places the cursor exactly under the pointer.
- The wheel scrolls whichever panel the pointer is over.
- Typing inserts text; `Backspace` deletes; `Ctrl+S` saves.
- `Ctrl+Q` exits and **the terminal is left working**.

Then the `$EDITOR` contract:

```bash
./target/release/typ.exe no-such-file.rs; echo "exit=$?"
```

Expected: `typ: no-such-file.rs does not exist` on stderr, `exit=1`.

Actual: exactly that, verified. The interactive checklist above it is still
outstanding — it needs a human at a real terminal.

- [x] **Step 6: Commit**

```bash
git add crates/typ
git commit -m "feat(cli): typ binary with workspace and single-file entry points"
```

---

### Task 16: CI, README, and milestone close-out

**Files:**
- Create: `.github/workflows/ci.yml`, `README.md`
- Delete: `spikes/m0-feel/{src,tests,Cargo.toml}`

**Interfaces:**
- Consumes: the whole workspace
- Produces: a green CI run gating future work

- [x] **Step 1: Write the CI workflow**

`.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, windows-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - name: Format
        run: cargo fmt --all -- --check
      - name: Clippy
        run: cargo clippy --workspace --all-targets -- -D warnings
      - name: Test
        run: cargo test --workspace
```

The spike is excluded automatically — it is a standalone crate, not a workspace member.

- [x] **Step 2: Run the same checks locally**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected: formatting clean, no clippy warnings, all tests passing.

Actual: all three clean. 57 tests across the workspace.

Fix warnings rather than allowing them. The per-file cap and the render-thread constraint are
not machine-checkable yet; clippy is the part that is, so keep it at zero.

- [x] **Step 3: Write the README**

The existing README already carried the why, the design goals, the roadmap and the
non-goals. Rather than replace it with the shorter text below, the Status section was
rewritten and Build and Keys sections were added.

`README.md`:

````markdown
# TYPE

**T**erminal-**Y**oked **P**rogramming **E**nvironment — a full IDE that runs in your terminal.

Capability comparable to a modern GUI editor, delivered through a terminal UI: non-modal,
mouse and keyboard as equal peers, panel-rich, extensible.

## Status

Pre-alpha. Walking skeleton complete: file tree, editor panel, focus cycling, mouse and
keyboard input, scroll coalescing.

## Build

```bash
cargo build --release
./target/release/typ .
```

## Keys

| Key | Action |
|---|---|
| `Tab` | Cycle focus between tree and editor |
| `Enter` | Open the selected file (tree) |
| Arrows | Move selection or cursor |
| `Ctrl+S` | Save |
| `Ctrl+Q` | Quit |

Mouse: click to select or position the cursor, wheel to scroll the panel under the pointer.

## Design

See [`docs/design/architecture.md`](docs/design/architecture.md).

## License

MIT
````

- [x] **Step 4: Delete the spike**

M0's job is done. Its measurements live in `FINDINGS.md` (kept), and `width.rs` was promoted
into `typ-buffer/src/position.rs` (kept). The rest is finished.

```bash
git rm -r spikes/m0-feel/src spikes/m0-feel/tests spikes/m0-feel/Cargo.toml
```

Keep `spikes/m0-feel/FINDINGS.md` — it is the record of what was measured and why the design
choices hold.

One fix landed here that the plan did not schedule: `run()` now installs a panic hook that
disables mouse capture before delegating to ratatui's. FINDINGS §6 recorded this as the one
M0 defect carried into M1 — ratatui's hook restores raw mode and the alternate screen but
knows nothing about mouse capture, so a panic returned the user to a shell still emitting
mouse escapes.

- [x] **Step 5: Commit**

```bash
git add .github README.md
git commit -m "ci: run fmt, clippy, and tests on three platforms"
git commit -m "chore: remove m0 spike, findings and width logic retained"
```

---

## What M1 does not include

Stated so nobody goes looking: no syntax highlighting in the real editor panel (M2), no LSP
(M3), no splits, tabs, or command palette (M4), no terminal panel or git (M5), no OS-level
file association (M6), no plugin host, no debugger. Selections, multi-cursor, and search also
land at M2 — M1 proves the architecture carries input, render, and events correctly across two
panels, and nothing more.
