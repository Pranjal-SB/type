---
type: design
status: draft
area: spec
verified: 2026-08-27
verified-against: v0.2.10
---

# Visual direction

**Status: design, not built.** Nothing here is in the tree. It is written down because
`architecture.md` §4 commits to "no chrome without a job" and "one visual system applied
uniformly" without ever saying what that looks like, and because two tasks in M2.5 (whitespace,
indent guides) are paint decisions that were about to be invented per-task instead of derived
from a direction.

Companion: [`controls.md`](controls.md), the keyboard model. The two constrain each other — a
grouped `Ctrl+K` hint box has to have somewhere to live, and that is a layout question.

## The decision: one rule, no boxes

Three densities were mocked at the same moment — file tree on the left, a file open, cursor on
line three — and rendered with Slate's real values rather than approximations.

**Rejected — full boxes**, which is what the tree ships today:

```
┌─ project ───┬─ main.rs ────────────────────┐
│ ▸ src       │  1  fn main() {              │
│   main.rs   │  2      let e = Editor::new();│
│ Cargo.toml  │  3      e.run()              │
└─────────────┴──────────────────────────────┘
```

Structure is unmissable and it costs two rows and two columns of every panel. On an 80×24
terminal that is roughly **15% of the screen spent on lines that carry no information**. It fails
"no chrome without a job" on its own terms.

**Rejected — editorial**, no rules at all, separation by whitespace and small-caps labels. It
reads beautifully above about 100 columns and falls apart below it, and TYPE has to work at 80.

**Chosen — one rule:**

```
project      │ main.rs
▸ src        │  1  fn main() {
  main.rs    │  2      let e = Editor::new();
  lib.rs     │  3      e.run()
Cargo.toml   │
 main.rs  Rust  LF  Spaces: 4               3:5
```

One vertical rule between panels, one status bar, no boxes. Panel identity comes from a heading,
not a frame. It buys back the 15% and keeps a hard edge between regions, which is the thing the
editorial version gave up.

**What it gives up, and how that is paid for:** a box has a border, and a border is the obvious
place to show focus. Removing boxes removes that. See below.

## Focus: recede the unfocused

Four treatments were compared, each drawn twice — tree focused, then editor focused.

**Chosen: the unfocused panel's content drops a ramp step.** The whole half changes weight, which
is unmissable in peripheral vision without anything moving or resizing.

The obvious objection is that dimming code you are reading is hostile. It does not apply, and the
reason is the useful part: **the receded panel is never the thing you are reading.** If you are
reading code, the editor has focus and the tree is the thing that dimmed. If you are picking a
file, the reverse. The dimmed half is reference material at that moment, not body text.

That resolves the rubric conflict too. A dimmed `fg` would fail the **body** floor — but it should
never have been measured against it. Receded content is recessive text and belongs to **`quiet`**
(5.0 on a dark ground, 2.0 on a light one), which the rubric already has.

Measured on Slate: focused body text is 12.48 against the page. `#828a95` sits at 5.29 truecolor
and 5.43 at 256 — clearing `quiet` at both depths while reading as a **2.4× drop in weight**.
That lands within a step of `base05`, which is already in the ramp.

**So it costs one theme slot pointing at an existing ramp step, not a new colour.** That matters:
Slate's rule is that a widget names a step and never mixes its own, and a palette assembled
colour-by-colour is how one visual system gets broken quietly.

### The three rejected treatments, and why

| Treatment | Why not |
|---|---|
| **The rule brightens** | The rule is *shared* between two panels, so it can only be one colour. It says focus exists somewhere, not which side of it. The heading ends up doing the real work anyway. |
| **Accent bar on the focused edge** | Unambiguous about which side — and it eats a column, so text reflows every time focus moves. That alone disqualifies it. |
| **Heading only** | The quietest option and defensible, since the terminal's real cursor is already drawn from the focused panel. Rejected as *too* quiet: at a glance across the screen nothing distinguishes the halves. |

**Worth keeping in mind regardless:** TYPE already draws the terminal's hardware cursor from the
focused panel (`architecture.md` §5). That is the strongest focus signal a terminal has, it is
already correct, and it blinks exactly where the reader is looking. Everything above is redundancy
on top of a signal that already works — which is why the argument was about degree, not about
whether focus is visible at all.

## What this implies, and what it does not

**Implies:**

- One new theme slot for receded content, pointing at an existing ramp step in each of the six
  themes, with a `quiet`-floor audit rule so it cannot rot.
- `RenderContext` already carries a focus flag, so the render path has what it needs. Panels do
  not learn about each other — a panel asks "am I focused" and picks a ramp step. That keeps
  invariant 5 intact.
- Removing the boxes touches `chrome.rs` and `layout.rs`, which currently overlap panel rects by
  a column so two borders share a cell. With one rule that machinery gets simpler, not harder.

**Does not imply, and must not be smuggled into M2.5:** none of this is in the M2.5 plan. The
milestone owes themes, capability detection, indent detection, whitespace and guides. Boxes and
focus receding are a separate body of work and want their own tasks, written down before they are
built. **This document is the input to that, not a licence to start.**

## Is ratatui the ceiling?

Asked at v0.2.10, because the terminal applications that currently look best — opencode,
lavalamp, oh-my-pi — do not use it. Checked rather than argued, and the answer is no.

**The three of them are one renderer.** `lavalamp` depends on `@opentui/core` and
`@opentui/react`; opencode's TUI is `opentui`, from the same organisation. OpenTUI is a
TypeScript API over a **Zig** native renderer, and it ships a `three` package, so it composites
rather than filling a cell grid. It was never a choice TYPE declined — it is not a Rust library,
and no Rust project could have picked it. opencode's `packages/app`, separately, is SolidJS and
Vite: a browser UI, not a terminal one.

Nor is there a Rust alternative to weigh. `tui-realm`, `widgetui`, `soft_ratatui` and
`raclettui` are all built **on** ratatui. The real second option is what Helix and TermIDE did,
hand-roll on crossterm, and Helix does not look better for it.

**ratatui does not cap what can be drawn**, which was checked directly:

| Wanted | Available |
|---|---|
| Undercurl, styled underlines | `Modifier` is a `u16` with 9 of 16 bits used; `bitflags` 2 has `from_bits_retain`, so a custom bit survives the buffer and the diff |
| Arbitrary escapes per cell | `Cell::symbol` is a string, and ratatui's own tests set an OSC 8 hyperlink into one (`buffer.rs:1220`) |
| Synchronized output (CSI 2026) | already built — `run.rs` writes `?2026h`/`?2026l` around every frame by hand, and a backend would own it instead |
| Full control of the bytes emitted | `Backend` is fourteen methods and `draw()` yields `(x, y, &Cell)` |

What produces the interchangeable ratatui look is `Block::bordered()` and the default widget set,
which this document already rejected on its first page.

**So the constraint is this document being a draft, not the renderer.** Ten milestones have gone
to correctness and none to visual design. That is the gap, and it is a milestone rather than a
task.

**The one structural tension is motion, and it is TYPE's own.** The loop repaints on dirty state
and never on a timer. Easing, transitions and a spinner while a language server indexes all need
frames on a clock. M2.4 made the loop wakeable so a timed wake is possible, but nothing has asked
for one, and "damage-driven" as written forecloses movement. Beauty in 2026 is substantially
movement. Deciding that is part of the milestone this document is the input to.

**Acted on now, and only this:** a custom `Backend` in `typ-app`, roughly 250 lines wrapping
crossterm. M3 needs it for the diagnostics underline regardless, and building it there means the
visual milestone starts with the capability already present instead of blocked behind it.

## Open

**Density inside a panel.** Nothing here says how much padding a panel carries, whether the gutter
is flush with the rule, or how the tree indents. Those are the next questions and they want
mockups rather than prose.

**Where the `Ctrl+K` hint box lives.** `controls.md` specifies it as grouped, described and
navigable. At 80 columns, in a layout with a sidebar, there is not obviously room. Overlay it on
the editor, dock it above the status bar, or replace the status bar while pending — undecided,
and it is a visual question, so it wants mockups.

**Typography.** TYPE does not get a vote on the font — `gap-analysis.md` Part 5 settles that. What
it does get a vote on is glyph *choice*: box-drawing weight, whether marks like `·` and `→` come
from a Unicode set with an ASCII fallback, and whether nerd-font icons are ever assumed. TermIDE
and Fresh both ship symbol presets (`unicode | nerd | ascii`); TYPE has no answer.
