# M0 Feel Spike — Findings

**Date:** 2026-08-13
**Terminal:** Windows Terminal — 1.24.11911.0
**Build:** release, opt-level 3, thin LTO
**Test file:** 50,000 generated Rust lines (`big.rs`, 2.2 MB)

## 1. Mouse click-to-position

- Feel while clicking around: native — no perceptible delay between click and cursor move
- Wide-character accuracy (CJK): exact
- Emoji accuracy: exact
- Tab accuracy: exact
- Click past end of line: clamps correctly

Backed by 5 click tests and 9 width tests, all passing:
`click_inside_a_wide_char_selects_that_char`, `click_past_end_of_line_clamps_to_line_end`,
`click_below_last_line_clamps_to_last_line`, `click_accounts_for_scroll_offset`,
`display_to_grapheme_col_snaps_into_a_wide_char`, `tabs_expand_to_the_next_tab_stop`.

## 2. Synchronized output (CSI 2026)

- Supported by this terminal: yes (sequence accepted, no stray output)
- Visible tearing with sync ON: none
- Visible tearing with sync OFF: none
- Verdict: **keep — but unproven here**

Windows Terminal 1.24 shows no tearing either way, so this terminal gave the flag nothing to
prove. Kept anyway: measured cost is 157us mean / 543us p99 (§3), which is 3.4% of the frame
budget, and terminals that *do* tear without it are common enough that paying 157us as
insurance is the cheaper mistake. **Do not cite this run as evidence CSI 2026 works.** The
next terminal that tears is the real test.

## 3. Frame timing

Runs are **not** of equal length — 1389 / 756 / 629 frames. Per-frame statistics still
compare (they are per-frame, not per-run), but `p99` on 629 samples is the 6th-slowest frame
and carries less weight than `p99` on 1389. Read `p50` as the solid number and `p99` as
directional. Numbers below are post-fix; see §4.

| Scenario | n | mean | p50 | p99 | max | max_at_frame |
|---|---|---|---|---|---|---|
| Scroll, highlight on, sync on | 1389 | 1421us | 1271us | 3657us | 6351us | 1204 |
| Scroll, no highlight, sync on | 756 | 1030us | 873us | 2654us | 5022us | 502 |
| Scroll, no highlight, sync off | 629 | 873us | 775us | 2111us | 3001us | 2 |

Budget: p99 < 16000us. Met: **yes** — worst p99 is 3657us, 23% of budget. Worst single frame
across all three runs is 6351us, 40% of budget.

Cost attribution (differences between adjacent rows):

| Feature | mean | p50 | p99 | % of 16ms budget (p99) |
|---|---|---|---|---|
| Highlighting | +391us | +398us | +1003us | 6.3% |
| Synchronized output | +157us | +98us | +543us | 3.4% |

Was the worst frame the startup paint (`max_at_frame` 0 or 1) or a real stall: **real stalls**
in runs 1 and 2 — frames 1204 and 502, both mid-scroll. Run 3's max landed at frame 2, so its
3001us max *is* startup paint and should not be read as a scroll number. The mid-run maxes are
2.2–4.4x their run's p99, which is the shape of an occasional stall rather than a systematic
one; at 6351us worst case there is no reason to chase it now. Note where it would come from if
it grows: the recolor-all-at-once path in §4.

`first=` was 2998 / 2756 / 2771us across the three runs — the first painted frame costs ~2x a
steady-state frame and is consistent, so it is layout warm-up, not a highlighting cost.

## 4. Tree-sitter under scroll

- Initial parse of 50k lines: 718 / 814 / 714 ms, off-thread
- Time from process start to first painted frame: 47ms (run 1), 9ms (runs 2, 3)
- Parse throughput: ~2 MB/s, linear in file size and independent of tree shape
- Cost concentrated in: parse (one-time, off-thread). `spans_for_line` is 391us mean of a
  1421us frame after the traversal fix — real but not dominant.
- Needs per-line caching in M1: **no** — it needed viewport-scoped traversal instead.
  Two O(lines-above-viewport) costs, both fixed by construction rather than by cache:
  `rope.line_to_byte()` for the line offset, and `TreeCursor::goto_first_child_for_byte()`
  to seek to the first top-level item instead of descending from the root. Measured
  18.7ms → 0.4ms per viewport, flat with scroll depth. See Task 6 Step 6.
- Highlighting p99 before the traversal fix: 1144011us (recorded because "tree-sitter is
  too slow to scroll" would have been the wrong conclusion to draw from it)

The 700-800ms parse never blocked a paint: first frame landed at 47ms and 9ms, roughly an
order of magnitude before the parse finished. That is the whole point of Task 6a. The run-1
gap (47ms vs 9ms) is almost certainly cold page cache on the 2.2 MB file — run 1 was the first
launch of the session — not a highlighting cost; a 38ms one-time delta is not worth chasing to
confirm.

Two known ceilings carried into M1 (both from Task 6a):

1. **16ms idle poll.** Moving the parse off-thread forced `event::poll` + a dirty flag in
   place of blocking `event::read()`. The loop wakes 60x/second doing nothing. M1 needs a
   blocking event channel that both input and parse-completion can wake — recorded in
   architecture §7.
2. **Recolor-all-at-once.** When a parse lands, the entire viewport restyles in one frame.
   Invisible at 700ms/50k lines on this machine. On a slower parse or a larger file it is the
   most likely source of a visible hitch, and it is where a growing `max_at_frame` would come
   from.

## 5. Unicode width

- `unicode-width` 0.2.2 passed all 9 width tests: **yes**
- If no, which failed and how: n/a
- Does M1 need a `[patch.crates-io]` fork: **no**

## 6. API surprises

- `tree_sitter_rust` language accessor used: `LANGUAGE`
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
  - `max_at_frame` added to the metrics output. Without it, run 3's 3001us max reads as a
    scroll stall when it is the startup paint.

**Mouse capture leaks on panic.** `ratatui::init()` installs a panic hook that restores raw
mode and leaves the alternate screen, but it knows nothing about `EnableMouseCapture`, which
this spike issues separately (`main.rs:103`) and disables only on the normal return path
(`main.rs:107`). A panic therefore exits to a shell still emitting mouse escape sequences.
Not exercised in these runs — found by reading the exit paths, not by panicking — and not a
gate, since both real exits (`q`, `Ctrl+C`) route through `run`'s normal return. M1 must own
one teardown that covers every terminal mode it turned on, including the panic path.

---

## Verdict

**GO / NO-GO:** **GO**

Reasoning:

All three hard gates pass:

| Gate | Result |
|---|---|
| Click-to-position drifts on wide characters | No — exact by hand and across 14 tests |
| p99 > 16000us scrolling with highlighting on | No — 3657us, 23% of budget |
| Terminal left broken on exit | No — clean teardown on `q` and `Ctrl+C` (both routed through `run`'s normal return, `main.rs:105-108`) |

The spike answered the one question it existed to answer: a rope + tree-sitter + ratatui stack
paints a 50k-line file at 1.3ms per frame with syntax highlighting on, which is 12x headroom.
Highlighting costs 6.3% of the budget and synchronized output 3.4%; neither is a reason to
design around.

The single largest risk this spike found was not the stack — it was a traversal written the
obvious way, which cost 1144011us p99 and would have been misread as "tree-sitter can't do
this." The fix was structural, not a cache. M1 inherits the range-query requirement in
architecture §5 so the same hole isn't dug twice.

Carry into M1:
- `src/width.rs` → `crates/typ-buffer/src/position.rs`, with its 9 tests
- Viewport-scoped traversal shape (`goto_first_child_for_byte`, not root descent) →
  `typ-syntax`, specified in architecture §5
- Off-thread parse + blocking event channel → architecture §7 (the spike's 16ms poll is the
  shortcut M1 must not inherit)
- `max_at_frame` in whatever metrics M1 keeps — a p99 without it is ambiguous about startup
- One teardown owning every terminal mode, panic path included (§6, mouse capture leak)

Discard:
- everything else in this spike
