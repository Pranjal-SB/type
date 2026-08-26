---
type: design
status: living
area: spec
verified: 2026-08-22
verified-against: v0.2.4
---

# TYPE — Terminal-Yoked Programming Environment

**Status:** approved; M0–M2.4 built against it
**Date:** 2026-08-10, last verified against the tree 2026-08-22, on the unreleased M2.5 branch
above v0.2.4
**Binary:** `typ` · **Crate:** `typ-editor` · **Repo:** `type`

---

## 1. Goal

A full IDE that runs in the terminal. Capability comparable to VS Code and Zed, delivered
through a terminal UI: non-modal, mouse and keyboard as equal peers, panel-rich,
extensible.

The bet is that a terminal IDE can carry the full feature surface of a GUI IDE while
starting faster and weighing less, because the renderer is already running and there is no
browser engine underneath.

### What "done" looks like

A developer opens `typ` in a project and never needs VS Code for that project. Code
intelligence, debugging, git, terminal, search, and extensions are all present. It starts
in under a tenth of a second and never stutters while scrolling a large file.

---

## 2. Why build this

- GUI IDEs are slow to start and heavy to run.
- Agentic and coding work already lives in the terminal; leaving it to edit is friction.
- Existing terminal editors each miss the mark in a different way:
  - **vim / nano** — no file tree, no run/debug surface, no IDE affordances without deep configuration.
  - **Helix** — ~85% there, but modal (Kakoune-style), mouse support is an afterthought, no integrated
    terminal, git is gutter-only, and the plugin system (Steel, PR #8675) has been open ~2 years.
  - **Fresh** — non-modal but sprawling (366k LOC src) and GPL-2.0, so nothing is reusable.
  - **TermIDE** — excellent panel architecture, but no plugin system and no debugger.
  - **ttt** — leanest and well-built, but regex highlighting rather than tree-sitter, and Go.

**The gap: non-modal, full mouse parity, panel-rich, plugin-first.** No project occupies that square.

### Feasibility

TermIDE went `0.1.0` (2025-11-25) to `0.29.7` (2026-08-01) — 98 releases, 145k lines of Rust,
44 crates, 20+ panel types, LSP, git, PTY terminal, tree-sitter across 21 languages — in
**eight months, essentially one author.**

For scale, full parity on the features that matter lands in the same range three independent
ways: a bottom-up estimate, TermIDE's actual 145k, and Helix's ~150k. This is a tractability
check, not a target — see §8.

That is not the 1.3M lines Zed carries. Zed's mass is a GPU renderer, a CRDT collab stack,
remote-dev servers, notebooks, a webview extension host, a marketplace, and a 100k-line
agent crate. None of that is frontier *editing* capability, and all of it is out of scope here.

### The leverage: it is protocols, not features

VS Code does not implement "go to definition." It implements an **LSP client** and receives
goto-def, hover, completion, rename, references, code actions, diagnostics, formatting,
signature help, symbols, inlay hints, semantic tokens, code lens, and call/type hierarchy —
for every language, permanently. **DAP** does the same for breakpoints, stepping, call
stacks, watches, and the debug REPL.

So "all the frontier IDE features" is mostly **three clients**: LSP, DAP, tree-sitter. Write
them once and write them well; that is roughly 40k of the 150k and it buys the large
majority of what people mean by "IDE."

---

## 3. Non-goals

Explicitly out of scope, permanently unless revisited:

- Collaborative editing / CRDT infrastructure
- Notebook (Jupyter) support
- Webview-style extensions rendering arbitrary HTML
- An extension marketplace
- A GPU renderer — the terminal is the renderer
- Remote development servers — SSH into the box and run `typ`; this is free in a terminal
  and is a structural advantage over GUI IDEs, not a gap

---

## 4. Product principles

The stated ideal is **clean, responsive, and mature, without giving up features.** Each is
made testable so it can be verified rather than asserted.

### Responsive — budgets, not vibes

| Metric | Budget |
|---|---|
| Cold start to interactive, mid-size repo | < 100 ms |
| Keystroke to painted glyph (p99) | < 16 ms |
| Scroll of a 100k-line file | no dropped frames at terminal refresh |
| Any blocking operation on the render thread | zero |

LSP, git, file watching, and syntax parsing all run on worker threads and deliver results as
events. The render thread only renders.

**These budgets have tests behind them** — `crates/typ-buffer/tests/perf.rs` and
`crates/typ-panel-editor/tests/perf.rs`, measured against a 50k-line file. They are
`#[ignore]`d, because a shared CI runner cannot hold a 16 ms number steadily enough to gate a
merge on it and a flaky perf gate gets disabled within a week; they are run by hand with
`--release --ignored`. A budget stated in prose with nothing measuring it is how M2 shipped a
keystroke costing 33 ms against this table, for ten tasks, with 215 tests green.

One thing the tests establish that the budget alone does not: a whole-buffer scan returning a
match on every line of a large file does not fit in a frame at any constant factor. Search is
therefore viewport-first with the remainder completed off-thread — a design constraint, not a
number to optimise toward.

**The budget is keystroke to painted glyph, so the paint is measured too.** Until v0.2.3 every
perf test measured edits and none measured rendering, which is half a number — and M2.3 put a
gutter, a bracket search and a per-grapheme paint decision on that path. A frame drawn deep in
a 50k-line file cost 439 µs at v0.2.3 and **482 µs on the M2.5 branch**, best of five — the
chrome surfaces added about 10%, which is 3% of the budget. Two further cautions were learned by
getting them wrong: these
tests take a mutex, because cargo's parallel threads made one read 32 µs against the 1.9 µs it
actually cost, and the `find_all` budget takes best-of-five, because it is the one number here
with less than an order of magnitude of headroom and a single sample of it measures the
scheduler.

### Clean

- No chrome without a job. Every border, gutter, and status segment justifies its cells.
- One visual system applied uniformly: focus, borders, selection, and status look the same
  in every panel.
- Every panel obeys the same affordances — focus, move, close, resize — so learning one
  panel teaches all of them.

### Mature

- **Mouse and keyboard are peers.** Neither is bolted on. Click to position the cursor, drag
  to select, click status chips, click the tree, scroll anywhere — and every one of those has
  a keyboard equivalent.
- **Every action is reachable three ways**: keybinding, command palette, mouse.
- Nothing is modal-only and nothing is mouse-only.
- Non-modal by default. Familiar to anyone arriving from VS Code.

**Modal editing is a setting, not a fork.** The core stays non-modal and always usable
without it; a vim layer sits above the editor as a toggle (`editing.mode = "vim"`), the way
Zed does it — a mode state machine that intercepts keys and translates them into the same
actions the non-modal path already calls. That is the whole reason it can be optional: the
layer owns modes, counts, operators and pending motions, and owns no editing primitives of
its own.

What modal editors actually have that non-modal ones lack is not modes, it is a **composable
grammar** — operator × count × motion, so `d3w` is three orthogonal pieces rather than a
memorised command. That grammar is the thing worth taking; modes are the price it charges,
and a toggle is how someone declines to pay it.

The obligation this creates on the core: every editing primitive must be reachable as a
named action taking explicit arguments, never only as a key handler. A `handle_key` arm that
mutates the buffer inline is unreachable from the vim layer, from the command palette, and
from a plugin — three consumers, one rule.

### Without giving up features

Guaranteed by the protocol leverage in §2, not by grinding out features one at a time.

---

## 5. Architecture

### Stack

| Concern | Choice |
|---|---|
| Language | Rust |
| TUI | ratatui 0.30 + crossterm 0.29 |
| Text buffer | `ropey` |
| Syntax | `tree-house` + five grammars, **compiled in** — reversed, see below |
| LSP | not built; the choice and its reasoning are in [`lsp.md`](lsp.md) |
| Terminal panel | `portable-pty` + `vte` — not built |
| Git | `gitoxide` — not built |
| Fuzzy matching | `nucleo-matcher`, without the `nucleo` wrapper |
| Project search | `grep-searcher` **as a library**, not a subprocess |

**Rows in this table that have been overtaken by the code say so.** Three of them were read as
decisions for months after the tree stopped agreeing with them, which is the same failure as the
crate list in §5 — a 2025 prediction being read as a specification. A milestone that changes the
stack changes this table in the same commit.

**Grammars are compiled in, and the dynamic-loading argument was answered rather than accepted.**
The original reasoning was TermIDE's six-line comment about tree-sitter ABI-14 versus ABI-15
colliding the exported `tree_sitter_php` C symbol at link time and silently disabling PHP
highlighting. Real, and it is a hazard of loading *many* grammars from a directory at runtime.
M2.7 took the other side: a closed set of five, linked statically, cannot collide at link time
because the linker sees all of them at once and would fail loudly rather than silently. What
compiled-in buys is the thing Helix's `--grammar fetch` costs its users — no C compiler, no
runtime directory to find, no partial install. Measured cost was 4.87 MB against a 1.19 MB
baseline. Adding a sixth language is a recompile, which is the trade.

**Fuzzy matching is `nucleo-matcher`, not `nucleo`.** The wrapper adds a rayon pool and streaming
injection; measured at M2.8, one thread ranks 50k paths against a six-character needle in
4.51 ms, and it already runs on a worker. The pool would have been a second thread pool beside
`ignore`'s crossbeam-deque one, for no measured gain.

**Project search is `grep-searcher` linked in, not a `ripgrep` subprocess.** Shelling out makes
the binary depend on something the user may not have installed, and gives up the two things that
are specifications rather than code — binary detection, and encoding and line-terminator
handling. A search that offers forty matches inside a `.png` is worse than one that offers none.

**Syntax queries are viewport-scoped, never per-line.** Measured at M0: asking a tree-sitter
tree for one line's spans costs O(lines above it), twice over — once resolving the line's
byte offset, once descending from the root past every top-level item to prune it. Done per
visible line per frame on a 50k-line file that is a **p99 of 1144ms** against a 16ms budget,
while `p50` stays at 1.1ms because the cost scales with scroll depth rather than viewport
size. `typ-syntax` therefore exposes a range query, and the fix is traversal, not caching:
line offsets come from the rope, and the walk seeks in with
`TreeCursor::goto_first_child_for_byte` rather than starting at the root. 18.7ms → 0.4ms per
viewport, flat with depth. `Node::descendant_for_byte_range` looks like the answer and is
not — a multi-line viewport's smallest containing node is the root.

**Tree-sitter parses at ~2 MB/s and that is not improvable.** Linear in file size,
independent of tree shape, so 50k lines of Rust costs ~750ms of wall clock. Every editor
surveyed hides it rather than reducing it: vim never builds a whole-file model at all
(approximate, and visibly wrong after a fast scroll), Neovim slices one parse across
event-loop iterations via the parse timeout because Lua gives it no threads. TYPE has
threads, so the parse runs on a worker and the tree arrives as an event — the §4 "no blocking
work on the render thread" rule is load-bearing here, not decorative. The file opens and
scrolls immediately, unhighlighted, and recolors when the tree lands.

**Unicode width must be handled from day one.** Column drift on CJK and emoji is not an edge
case in an editor, it is a daily correctness bug, and it is what mouse-click-to-cursor
depends on.

TermIDE carries a forked `unicode-width` via `[patch.crates-io]`, which suggested TYPE would
need one too. Measured instead: stock `unicode-width` 0.2 passes all nine width cases,
including emoji and combining marks. **No fork needed.** Their patch predates fixes upstream.

The general rule this establishes: prior art is evidence, not authority. Where a surveyed
project's choice is load-bearing here, it is because it was tested or because the failure it
avoids was observed — never because they did it.

### Crates — 14

TermIDE's 44 crates is over-split for one author. Fresh's 10 crates hiding 366k lines is
under-split. The middle:

```
typ-core/            Panel trait, events, commands, keychord, terminal capabilities
typ-buffer/          ropey wrapper, undo, multi-cursor, selections
typ-syntax/          tree-sitter: highlight, injections (folds and indents unbuilt)
typ-find/            gitignore-aware parallel walk, fuzzy ranking, project search
typ-lsp/             LSP client — async, multi-server, per-language
typ-git/             status, diff, blame, hunks
typ-registry/        filetype -> handler mapping
typ-ui/              shared ratatui widgets, theme, render helpers   [not built — see below]
typ-config/          config, keybindings, theme loading              [not built — see below]
typ-panel-editor/
typ-panel-tree/
typ-picker/          the file-picker and project-search overlay
typ-panel-terminal/
typ-panel-git/
typ-app/             event loop, layout, session, palette
typ/                 thin binary
```

**Two of the fourteen were decided against rather than deferred.** `typ-syntax` arrived at M2.7
carrying highlighting and injections; folds and indents are still forward-looking, and the crate
is named here with the contents it was predicted to have rather than the contents it has.

`typ-find` and `typ-picker` arrived at M2.8 and are **not** on the original list of fourteen —
this plan put fuzzy find inside `typ-app`, which the dependency graph does not allow. `typ-core`
names the worker's result type on `AppEvent`, so the walking and ranking half has to sit below
`typ-core`; the widget implements `Panel`, so it has to sit above. One crate cannot do both, and
neither half belongs in `typ-app`. The same split, for the same reason, as `typ-syntax` and the
highlighting inside `typ-panel-editor`.
`typ-lsp`, `typ-git` and the two remaining panel crates are forward-looking entire — they arrive
with the milestone that needs them. `typ-ui` and `typ-config` are different: they were reached for at
M2.5 and the seam turned out to fall somewhere else.

`Keymap::merge_toml` lives in `typ-core` beside the type it produces, and theme parsing followed
the same shape for the same reason — a parser belongs with the type it parses into, not in a
crate that exists to hold parsers. What would have been left for `typ-config` is three
path-finding functions, which is a crate boundary separating nothing. The theme vocabulary
(`Theme`, `ThemeColors`, `audit`, the degradation) sits in `typ-core` on the same argument, so
`typ-ui` has no contents either; the shared render helpers it was meant to hold are still one
`RenderContext` and a `Paint` enum.

Revisit when the plugin host at v1.1 needs config registration, which is the first thing that
would sit on the other side of that boundary. Cost of being wrong is a `git mv`.

**800 lines per file is where you go looking for a seam.** Fresh and TermIDE both broke this
badly and it shows — `plugin_dispatch.rs` at 6.7k lines, `panel-editor` at 22k, `modal` at
15k. Files that size stop being contributable, including by their own author.

**Revised at v0.2.3, after the rule misfired.** It was written as a hard cap, and a hard
numeric gate has a failure of its own: it forces a cut at an arithmetic boundary rather than
at a seam. `actions.rs` crossed 800 by eleven lines and was split twice — once into
`occurrence.rs`, which shares a needle, a case rule and a stop condition with nothing else in
the editor and was worth doing at any length, and once into a 56-line `edit.rs` that cohered
around nothing and was merged straight back. The second split existed to satisfy a number.

So the number is a **trigger to look**, not a threshold to satisfy. At 800 a file has almost
always grown a second responsibility; find it and split there. If there genuinely is not one,
the file stays long and the reason is recorded. The failure this rule exists to prevent is a
6,700-line file, and nothing about 850 resembles that.

### The Panel contract

TermIDE's `Panel` trait is the best single artifact in any of the three projects surveyed.
TYPE takes its *shape* — return events, never touch state — but not its size.

- **Starts at five methods**: `name`, `title`, `render`, `handle_key`, `as_any`. Nothing else
  until a second panel needs it. First growth came at M1.1: `cursor_position(panel_area)`,
  defaulted to `None`. The app draws the terminal's real cursor from the focused panel rather
  than styling a cell, so it blinks and reshapes like every other terminal program's; panels
  with nothing to edit ignore the method entirely. TermIDE's ~30 methods are the endpoint of years of real
  panels; adopting that surface up front would be guessing at generality we have not earned.
  The trait grows when a concrete panel forces it, and each addition is defaulted so existing
  panels do not break.
- Panels return `Vec<PanelEvent>` rather than mutating application state — decoupled and
  independently testable.
- `RenderContext` is a narrow struct (theme colors, focus flag, dimensions), **not**
  `&AppState`. A panel cannot reach into the world.
Two of TermIDE's methods are worth adopting when their milestone arrives, not before:
`status_segments()` (focused panel contributes clickable status-bar chips, clicks route back
by id via `handle_status_action`) at M4 with the status bar, and `to_session()` (session
persistence as a per-panel concern rather than a central one) at M4 with sessions.

**M2 added `apply_action`**, and it is the load-bearing one. It is the single entry point
through which the keymap, the future command palette, and the future vim layer all reach a
panel's behavior — which is why no `handle_key` arm may mutate a buffer. A primitive
reachable only from a key handler is invisible to all three consumers. `EditorPanel` now has
no raw-key behavior at all; every key that does anything is a keymap row.

Its return type is `Option<Vec<PanelEvent>>`, not `Vec<PanelEvent>`, because "I do not handle
this action" and "handled, nothing to report" are different answers and conflating them is a
silent bug. Adding a cursor at the edge of the document is a real instance of the second.

The app's dispatch order follows from that: action → panel → app → **panel's raw
`handle_key`**. The last tier exists because the file tree navigates on raw arrows and Enter,
all of which the keymap binds to editor actions; without it the tree goes dead while every
test still passes. Naming the tree's own primitives as actions is the honest fix.

That was written as landing "with the command palette at M4". The palette shipped at M2.9 and
this did not, because the two turned out to be independent: the palette lists whatever is in
`Action::ALL`, so it covers the editor and the app, and the raw-key tier is untouched. What
`apply_action` actually bought was the palette costing a `Mode` on an existing widget instead of
a feature — 53 names already had a `name()` and a binding lookup. The tree's vocabulary waits for
a second consumer to want it, which is the only thing invariant 2 requires.

### The selection model

There is no single-cursor type, and there never was one to remove. A caret is an empty
selection, and the editor always holds a `Selections` — non-empty, document-ordered,
non-overlapping, with one entry designated primary. Every mutating method restores those
invariants, so no editing path defends against an out-of-order or overlapping set.

This was decided before the first editing code was written, on the grounds that adding
multi-cursor later means rewriting every editing path twice: once to add the concept, once to
undo what the single-cursor assumption baked in. It held — every action written since works
for one cursor or thirty without branching on which.

**Edits are described, not performed.** A multi-cursor action produces one `Edit { start, end,
text }` per selection, and a single pass applies them while carrying an accumulated `Shift`.
This is not a stylistic choice: an edit moves every position after it, so a caret returned by
an earlier closure is stale the moment a later edit lands to its left. The first attempt had
each closure perform its own edit and return a caret, running selections last-to-first so
offsets stayed valid; three cursors typing on one line produced the correct text and put two
of three carets in the wrong column. `Shift` lives in `typ-buffer` because search results,
diagnostics and git hunks will each need to map a position across an edit. It is a shift map
over one batch, not an anchor system — anchors are a separate decision.

**Undo groups by edit kind, not by time.** Consecutive edits of the same kind fold into the
open run; a motion, a click, or a save ends it. VS Code and Zed break runs on an idle timer,
which would mean the buffer needs a clock and tests need to inject one. The structural rule is
deterministic and matches what a user means by "undo what I just typed": the run ends when
they moved. Snapshots carry the selections the edit was made from, so undo restores the cursor
to where the edit happened rather than wherever clamping left it.

### Event model — the one deliberate fix

TermIDE's `PanelEvent` grew to **61 variants** because every viewer added its own
(`ViewMermaid`, `SwapActiveToHex`, `ViewDatabase`, …). That enum became a chokepoint: every
new panel type edits core.

TYPE keeps roughly **12 universal variants** — `NeedsRedraw`, `Quit`, `Focus`,
`OpenFile { path, line, col }`, `RunCommand`, `CloseSelf`, `Notify`, and similar — and routes
everything else through one:

```rust
OpenWith { handler: HandlerId, path: PathBuf }
```

resolved by `typ-registry` against an extension/mime table.

This is load-bearing in three directions at once:

1. **It is the filetype association** (§6).
2. **Adding a panel type never edits core** — register a handler instead.
3. **It is the seam the plugin host plugs into** in v1.1 — a plugin registers a handler
   through the same path a built-in panel does.

---

## 6. Becoming the default editor

Three distinct mechanisms, all in v1. They matter in roughly inverse order to how obvious
they are.

### `$EDITOR` — the one that actually matters most

For terminal tooling, the dominant "default editor" mechanism is not the OS filetype table,
it is `$EDITOR` / `$VISUAL` and `git config core.editor`. That is what opens for
`git commit`, `crontab -e`, `kubectl edit`, `gh pr create`, and every CLI that shells out.

Zero lines of feature code, but it imposes real constraints on the binary:

- `typ <file>` opens exactly that file, blocks until closed, and exits cleanly.
- Honest exit codes — a non-zero exit must abort the calling operation.
- No daemon detach in this mode, and no session restore stomping a commit buffer.

Treat these as invariants from M1, not as an afterthought. They are cheap to hold and
expensive to retrofit.

### In-editor: extension → panel

`typ-registry` maps extension/mime to a handler. Text falls through to the editor panel;
images, binaries, databases, and markdown route to their own viewers as those panels land.
Handlers are registered, never hardcoded, so this is also the plugin extension point.

### OS-level: double-click a file → it opens in TYPE

| OS | Mechanism | Shim needed? |
|---|---|---|
| Linux | `.desktop` with `MimeType=` **and `Terminal=true`** | **No** — the DE spawns the terminal |
| Windows | `HKCU\Software\Classes\<ext>` + `shell\open\command` | Yes |
| macOS | `.app` bundle with `CFBundleDocumentTypes` | Yes |

`Terminal=true` is a freedesktop-spec flag telling the desktop environment to run the
application inside a terminal emulator of its own choosing. It works across GNOME, KDE, and
XFCE, and it removes the entire terminal-spawning problem on Linux. Verified in Fresh's
shipped `fresh.desktop`.

### This is a genuine differentiator

Verified against the field, not assumed:

| | Windows Explorer | Linux | macOS |
|---|---|---|---|
| Neovim | none — feature request open since 2017 | partial, via distro packages | none |
| Helix | none | none | none |
| TermIDE | none | none | none |
| ttt | none | none | none |
| Fresh | none — no Windows code in-tree | **yes**, proper `.desktop` + 22 MIME types | `CFBundleDocumentTypes`, but on the **GUI** crate only |

**No TUI editor ships Windows Explorer association.** Neovim's issue has been open roughly
nine years and the ecosystem routes around it with hand-written `.reg` gists. Fresh — the
most feature-maximalist project surveyed — wrote no Windows association code at all, and on
macOS pointed the handler at its GUI mode rather than its terminal mode.

That last detail is the trap appearing in the wild: even a project that wanted this punted
rather than solve terminal-spawning on macOS. The difficulty is exactly why the square is
empty, which is what makes it worth occupying.

**Opt-in, never automatic.** Association is installed by an explicit `typ --install-associations`
command, never as a side effect of installing. Silently taking `.txt` from Notepad or `.md`
from whatever currently owns it is how software gets uninstalled.

**The wrinkle:** a GUI double-click has no terminal to run in, so one must be spawned. TYPE
ships a launcher shim that reads a configured terminal emulator (`wt.exe`, `$TERMINAL`,
`x-terminal-emulator`) and execs `typ` inside it, plus per-OS install scripts. Windows and
macOS only — Linux is covered by `Terminal=true`.

**The shim is the risk, not the registration.** Registering a handler is trivial; the fragile
part is the terminal that gets spawned — its dimensions, font, whether it closes on exit,
whether it inherits the right shell and working directory. A double-click is a single shot at
a first impression, and an 80×24 window with a fallback font squanders it. Polish budget goes
here, not into the registry writes.

**Who this is for.** The target is not the vim user — they are already served, and they enter
via `typ .` and `$EDITOR` regardless. The target is the person currently on VS Code who wants
speed and a clean interface without giving up capability. For them, double-clicking a file is
a normal, daily entry path, and an editor that cannot be set as the default is not a real
replacement. That is why this ships in v1 rather than later.

**Single-instance routing** ships alongside it, because without it double-clicking five files
spawns five editors. A named pipe on Windows, a unix socket elsewhere: if an instance already
owns that workspace, hand it the path; otherwise cold start. VS Code and Zed both do exactly
this, and it is unpleasant to retrofit.

---

## 7. Render and input model

This section is what makes "responsive and mature" real rather than aspirational.

- **Synchronized output (CSI 2026)** wraps every frame, eliminating tearing on partial
  repaints. Borrowed from pi-tui, which is what gives pi and oh-my-pi their visual polish.
- **Damage-driven redraw.** Repaint on dirty state, never on a timer tick. ratatui's
  double-buffer diff then emits only changed cells. This is a measurement concern as well as
  a performance one: at M0 every mouse-move was repainting and being recorded as a frame,
  which quietly flattered both `p50` and `p99` until the dirty flag landed.
- **The event loop blocks on one channel**, with worker threads and a crossterm-pumping
  thread feeding it. Blocking directly on terminal input instead means an off-thread result
  — a finished parse, an LSP response — does not appear until the user's next keypress;
  polling instead of blocking fixes that but burns a wakeup per tick forever.
- **Input coalescing.** Batch scroll deltas into a single `handle_scroll`; drop stale resize
  events. Prevents the scroll-lag that makes TUIs feel cheap.
- **Terminal capability detection** at startup: truecolor, the **kitty keyboard protocol**,
  image protocols (Kitty/iTerm2/Ghostty), synchronized output. Graceful degradation for each.

The kitty keyboard protocol matters more than it looks. Without it, a terminal cannot
distinguish `Ctrl+I` from `Tab`, or `Ctrl+M` from `Enter`, and key-release events are
unavailable. Full modifier fidelity is a prerequisite for VS Code-comparable keybindings.

---

## 8. v1 scope

**The bar: the point at which the author stops using their current editor.**

### In

- Editor panel — tree-sitter highlighting, multi-cursor, selections, undo, search/replace
- LSP — completion, hover, goto-definition, references, rename, diagnostics, code actions,
  formatting, document symbols, inlay hints
- File tree panel
- Integrated terminal panel (PTY)
- Git — gutter, status, diff, blame, stage hunks
- Splits, layout, session restore
- Tabs, command palette, fuzzy file finder, project-wide search
- Filetype registry, `$EDITOR` invariants, OS-level association, single-instance routing
- Config, theming, keybindings

**On line counts.** Sizes quoted anywhere in this document are scale signals, not budgets or
targets. They exist to answer "is this tractable for one person" — the answer is yes — and
nothing is scoped in or out because of a line count. The only structural rule that stays is
the per-file cap in §5, which is about keeping code readable, not about keeping it small.

### Out (post-v1)

- Plugin host (JSON-RPC over stdio) — v1.1
- DAP / debugger — v1.2
- Minimap, sticky scroll, breadcrumbs — polish pass
- Additional viewer panels (image, hex, database, markdown preview, mermaid)

### The plugin host, when it lands

Extensions are **subprocesses speaking JSON-RPC over stdio** — the same shape as LSP, reusing
transport that already exists.

- No embedded runtime, so no sandbox to get wrong; the OS process boundary does that work.
- No `oxc`/`rquickjs` dependency weight and no effect on startup time.
- Extensions in any language, so oh-my-pi or pi integrate as-is rather than being rewritten
  in Lua — which matters, because async HTTP and JSON in Lua is miserable and that is exactly
  what an agent extension needs.
- Four verbs are enough to host an agent harness: **spawn a panel, read/write buffers, run a
  process and stream its output, subscribe to editor events.**

Trade-off: an IPC round-trip per call, so nothing on the keystroke hot path (custom
highlighters, input transforms) can be an extension. Those are core concerns regardless.

An embedded Lua runtime gets added later **only if** real demand for hot-path extensions
appears. Not both up front.

---

## 9. Milestones

**Versions map onto milestones, they do not replace them.** The scheme is
`0.<milestone>.<patch milestone>`: M1 shipped as v0.1.0, M2 as v0.2.0, the M2.1 correctness
pass as v0.2.1, and M6 ships as v1.0.0. One version for the whole workspace, set once in
`[workspace.package]` and inherited by every crate, so `typ --version` and the crate metadata
cannot disagree. Milestones remain the unit of work — plans, task lists and commits are
organised by M-number; the version is the public name for a milestone that has landed, and it
is bumped in the close-out task alongside the README.

Post-v1 follows the same shape: the plugin host is v1.1, DAP is v1.2.

The list below is the **shape** of the plan — the six numbered milestones and what each is for.
It is deliberately not the schedule. Patch milestones (M2.1 through M2.7 so far) are inserted
between them as defects and gaps are found, and keeping their list here as well as in two other
places is how three copies disagree. The live roadmap is the README's table, and the reasoning
behind each insertion is [`gap-analysis.md`](gap-analysis.md) Part 6.


**M0 — Feel spike (throwaway, ~1 week).**
The riskiest unknown is not "can an editor be written," it is "will the terminal feel good
enough." Answer before any real architecture exists. Open one file, highlight it, scroll it,
click in it, save it. Then delete it.

Must answer:
- Does click-to-position via crossterm mouse feel native or laggy?
- Does synchronized output actually eliminate flicker on this terminal?
- Can tree-sitter incrementally re-highlight a 50k-line file at scroll speed?
- Do CJK and emoji hold their columns in the viewport?

If it does not feel better than nvim after a week, nothing downstream matters.

**M1 — Walking skeleton.** Event loop, `Panel` trait, `typ-core`, one editor panel, one file
tree panel. A vertical slice through the real architecture. The `$EDITOR` invariants from §6
hold from here on — `typ <file>` opens, blocks, exits clean, reports honest exit codes.

**M2 — Editing is real.** `typ-buffer` complete: multi-cursor, selections, undo, search and
replace.

*Recorded after the fact:* two of this milestone's claims moved. `typ-syntax` highlighting did
not ship at M2. It was rescheduled to M2.5 on the grounds that it needs a theme to map capture
names onto (see [`gap-analysis.md`](gap-analysis.md) Part 5) — **right about the dependency,
wrong about the direction.** The mapping *is* a theme, so the theme system has to exist first,
and it turned out to be a milestone's worth of work on its own. M2.5 is the theme system;
highlighting is **M2.6**. Self-hosting was declared to
begin here and did not: the clipboard, Tab indent and the guard on opening over a dirty buffer
were all absent, and it began at **M2.2** once those landed. The correction is left visible
rather than edited out, because "the mechanism meant to keep every later milestone honest was
declared on and was off" is the most expensive thing this document has been wrong about.

**M3 — Code intelligence.** `typ-lsp`. Completion, diagnostics, goto-definition, rename,
code actions. This is the milestone where it becomes an IDE.

**M4 — Workspace.** Splits, sessions, workspace-wide file watching.

Tabs and the command palette came forward to **M2.9**, and the fuzzy finder and project search to
M2.8, because M2.8 made moving between files the primary interaction while `App` still held one
buffer — every `Ctrl+P` discarded the file you were on. What is left for M4 is the part that
needs a layout tree and a second answer to "which panel is active", plus a watcher that follows a
workspace rather than a file: a tab switch currently rebuilds an OS file watch on the render
thread, measured at 640 µs of a 16 ms budget.

**M5 — Terminal and git.** PTY panel, git gutter/status/diff/blame.

**M6 — Association and polish.** OS-level filetype association behind
`typ --install-associations`, launcher shim (with the terminal-spawn polish that §6 flags as
the real risk), single-instance routing. Performance budgets from §4 verified and enforced
in CI.

**v1 ships.** Then: plugin host (v1.1), DAP (v1.2).

Self-hosting from M2 onward is the forcing function. Every bug gets found by the author
using it daily, and the project stays alive because it is useful before it is finished.

---

## 10. Open questions

- **Layout model.** TermIDE uses an accordion with smart stacking; VS Code uses fixed
  sidebar plus editor group splits; Zed uses tiled panes. Decide at M4, ideally against
  mockups rather than in prose.
- ~~**Config format.**~~ **Closed at M2: TOML.** Decided by shipping rather than by argument —
  `keys.toml` landed in Task 14 and the `toml` crate was already in the tree. JSONC's
  advantage was easing migration from VS Code, which matters for a keybinding file someone
  ports and for nothing else; TOML is idiomatic in Rust, comments cleanly, and is what the
  theme files will use too. Not revisited without a concrete migration complaint.
- **Keybinding defaults.** VS Code-compatible out of the box is the non-modal thesis, but a
  vim mode will be requested early. Ship the keymap layer with enough indirection that a vim
  mode is a config, not a fork.
- **Minimum supported terminal.** Which capabilities degrade gracefully versus which are
  hard requirements.

---

## 11. Prior art, measured

Field re-measured at v0.2.1 against VS Code, Zed, Sublime, Helix and TermIDE, alongside a full
defect audit of the tree: [`gap-analysis.md`](gap-analysis.md). Its findings that change this
document are folded in above; the ones that change the schedule are in that document's Part 6.


Measured from source, not from READMEs.

| | Fresh | TermIDE | ttt |
|---|---|---|---|
| Language | Rust | Rust | Go |
| License | GPL-2.0 | MIT | MIT |
| TUI layer | crossterm + own renderer | **ratatui 0.30** | tcell v3 + own diff renderer |
| Buffer | custom | `ropey` | custom |
| Highlighting | tree-sitter 0.26 + syntect | tree-sitter 0.24, static-linked | chroma (regex) |
| Plugins | QuickJS + oxc TS transpile | none | gopher-lua, sandboxed |
| LSP | lsp-types 0.97 | lsp-types 0.97 | hand-rolled |
| PTY | — | portable-pty + vte | go-pty + vt10x |
| **src LOC** | **366k** (+284k tests) | **145k** | **80k** |

Taken from each:

- **TermIDE** — the `Panel` trait's shape (not its size), `RenderContext` narrowing,
  panel-per-crate, and later, status segments with click routing and `to_session`. Its
  `unicode-width` patch was evaluated and rejected: stock 0.2 passes every case we tested.
- **ttt** — its 87-line diff renderer, as the standing reminder of how little this actually
  needs to be. Plus the "plugins can create panels" surface.
- **Fresh** — typed plugin API generation: emit type stubs from the Rust API so extension
  authors get real autocomplete.
- **pi / oh-my-pi** — CSI 2026 synchronized output, image protocol detection, and the
  render-strategy split.

Deliberately not taken: Fresh's scope sprawl (webui, server, client, GPU window all in one
tree) and its `opt-level = "z"` release profile, which optimizes for binary size while
claiming speed.
