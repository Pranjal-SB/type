---
type: design
status: draft
area: strategy
verified: 2026-08-30
verified-against: v0.3.0
---

# UI/UX study: what VS Code and Zed get right, translated to a terminal

**Status: study, not a plan.** Input to the visual milestone that `visual.md` already calls
for, and to M4's workspace work. Nothing here is a licence to build; everything here is a
candidate that still has to survive mockups and the 80-column floor.

Sources: VS Code's published UX guidelines and layout docs, Zed's settings-UI and hidden-gems
posts, and source-derived documentation of Zed's `workspace`/`Dock`/`Panel` system. Both
editors are open source; where a claim mattered it was checked against how the thing actually
behaves, not how it is marketed. Companions: `visual.md` (what the chrome looks like),
`controls.md` (the keyboard model), `landscape.md` (why anyone would switch).

---

## 1. The anatomy both editors converge on

Strip the branding and VS Code and Zed are the same machine:

| Region | VS Code | Zed |
|---|---|---|
| Center | Editor Groups (splits, tabs) | Panes (splits, tabs) |
| Left dock | Activity Bar + Primary Sidebar | Left dock (project, outline, git…) |
| Right dock | Secondary Sidebar | Right dock (collab, agent…) |
| Bottom dock | Panel (terminal, problems, output) | Bottom dock (terminal, diagnostics) |
| Bottom edge | Status Bar | Status bar |
| Overlay | Quick Pick / Command Palette | Command palette, file finder, outline modal |
| Ephemeral | Notifications (toasts, bottom-right) | Toasts |

Two independent teams, a decade apart, one with a browser engine and one with a bespoke GPU
framework, arrived at: **an editor center, three docks, a status bar, one fuzzy overlay, and
transient toasts.** That is not fashion; it is the shape users have learned, and it is the
shape TYPE's own `Panel` trait and layout are already growing toward. The anatomy is settled.
The work is deciding which parts earn cells in a terminal.

Zed's `Panel` trait is worth copying almost verbatim in spirit: `persistent_name()` (state
serialization key), `position()` (which dock), `toggle_action()` (the action that shows/hides
it), `activation_priority()` (ordering), plus zoom. Every panel answering those five questions
is what makes "learning one panel teaches all of them" (architecture §4) mechanically true
instead of aspirational.

## 2. Why they feel good: the principles that transfer

### 2.1 One fuzzy overlay is the whole discoverability story

VS Code's real UI primitive is not the sidebar, it is **Quick Pick**: one widget that is the
file finder, the command palette, goto-line, goto-symbol, branch picker, everything. A prefix
selects the mode: `>` commands, `@` symbols, `:` line, `#` workspace symbols. Every feature
that ships as a Quick Pick inherits fuzzy matching, keyboard/mouse parity and muscle memory
for free.

TYPE already discovered this independently: the v0.2.9 palette is "a mode on an existing
widget." The study confirms the direction and says **go further: the picker is a platform,
not a feature.** Adopt the prefix language wholesale (`>` is already shipped; `:` should fold
goto-line in; `@` arrives with M3 symbols) and route every future "choose one of N" interaction
through it: theme picker, language picker, branch picker at M5. A second list widget is a
defect.

One Zed detail worth stealing: in the outline modal, adding a space widens the query to match
qualifiers (`pub fn`), so filtering by *kind* needs no syntax. Cheap in nucleo, memorable in use.

### 2.2 Toggle-with-focus is the entire focus model

The single best-feeling mechanic in both editors is one key per panel with idempotent
semantics. Zed's `toggle_panel_focus::<T>()` states it exactly:

- panel hidden → **open it and focus it**
- panel open but unfocused → **focus it**
- panel focused → **hide it, return focus to the editor**

`Ctrl+B` (sidebar) and `` Ctrl+` `` (terminal) in VS Code carry those semantics, and they are
why panels feel weightless there: the same key is "give me it" and "get rid of it," and you
never think about where focus is because closing always hands it back.

`controls.md` leaves "the focus model" open and calls `F6` cycling not an answer. **This is
the answer.** Toggle-with-focus per panel, `Esc` in any dock panel returns to the editor, and
closing a panel restores focus to the last-focused pane. A focus *stack* rather than a cycle:
"back to where I was" falls out of it. `F6` survives as the accessibility fallback, not the
model. Bindings hang off `ctrl+k` where the universal tier is spent (`ctrl+b` for the tree is
worth its single chord; the rest are `ctrl+k g`, `ctrl+k o`, …).

### 2.3 The editor is primary; everything else must be dismissible to zero

Zed's most-cited quality is chrome that is "out of your face"; VS Code ships Zen mode and
centered layout because its own default chrome is too loud. Both agree on the direction:
**every non-editor surface must be togglable to nothing, and the empty state is legitimate.**

TYPE is structurally ahead here (the one-rule layout in `visual.md` already spends ~0% on
chrome against VS Code's icon strips) but the principle adds a requirement: *every* dock,
bar and gutter needs an off switch, and `typ file.txt` with everything off must be a
first-class state, because that is the `$EDITOR` invariant wearing a UX hat.

### 2.4 Progressive disclosure: chrome appears when it has a job

VS Code's breadcrumbs, minimap and sticky scroll all default on and each costs screen real
estate whether or not it is earning it: the accepted critique of its UX. Zed inverts the
default. TYPE already has the right instinct (tab bar appears at two files) and should make
it a stated rule: **a surface defaults hidden until the state it describes exists.** Problems
dock appears when there are problems; git panel when there is a repo; hint box when a chord is
pending. This is "no chrome without a job" extended from *cells* to *time*.

### 2.5 Nothing modal, nothing blocking; progress is ambient

Neither editor ever puts a blocking dialog in front of background work. Long work reports
through the status bar (VS Code's spinner segments) or a toast that expires. The only modal
moments are ones the user initiated (a picker, a confirm on destructive close).

Terminal translation: toasts are a one-line surface directly above the status bar (no box:
one-rule styling), stacking count shown as `(+2)`, dismiss on any key that isn't consumed,
readable later in a log view. rust-analyzer indexing at M3 is the first real customer:
"rust-analyzer: indexing 3/117" belongs in a status segment, not a toast, because it is state,
not an event. **Events toast; states get a segment.** That sentence resolves most future
"where does this message go" questions.

This is also where `visual.md`'s motion tension lands: a progress spinner needs a timed wake.
M2.4 built the wakeable loop; grant progress indication a timer (spinner braille frames at
~8 fps, only while a worker reports activity) without conceding animation anywhere else.
Damage-driven stays the rule; a running worker *is* damage.

### 2.6 The status bar is an instrument panel, both directions

VS Code's status bar is clickable end to end: line:col opens goto, language opens the language
picker, branch opens the branch picker, the error count opens Problems. TYPE's status bar
already carries the right *data* (file, type, line ending, indent, cursor count, position);
mouse parity (invariant 8) says every one of those segments becomes a click target for the
picker or action that edits it. This is nearly free (segments already exist, the pickers
mostly exist) and it is exactly the kind of parity the pitch sentence promises.

### 2.7 Settings: the file is truth, the UI is a view

The user-visible half of VS Code's settings editor is the model to copy: **search-first**, flat
results over a category tree, each row showing name, description, current value and an
affordance; a **modified indicator** with "reset to default" and "copy setting id"; a toggle to
the raw JSON that never goes away. The file remains the source of truth and the GUI is a
projection of it.

Zed's rebuild adds the two architectural lessons, learned expensively: their macro-generated
first attempt failed because settings lived scattered across crates with "no unified,
strongly-typed model," and the fix was **centralizing settings into one strongly-typed model
first, UI second.** And they put settings in its own window because settings' scope (user vs
project) is not the project the workspace is focused on: the organizing principle of the UI
is the *files*.

Translation for TYPE, in order:

1. **Centralize now, while config is small.** One typed `Settings` model in one place, every
   consumer reads it, before M4/M5 sprawl it across crates. This is the cheap moment Zed
   missed.
2. **Settings UI is a panel in the editor area** (a tab, like VS Code: TYPE has no second
   window to spend). Rendered as a searchable list: name, one-line description, value, default,
   and *source* (default / user file). Enter edits in place; the write goes to `config.toml`:
   comments and unknown keys preserved. `>Preferences: Open Settings (file)` keeps the raw
   path one command away, always.
3. **Keybindings get the identical treatment** over `keys.toml`: searchable table of action,
   description, chord, source; conflicts shown. `bindings_for` is already the data source:
   this is the third surface of the same table `controls.md` demands, beside the palette and
   the hint box, and it is why `Action` growing a description pays three times.

### 2.8 Persistence: the project remembers itself

Both editors restore layout, open tabs, dock sizes and visibility per project. Zed serializes
per-panel state under `persistent_name()`, throttled. Users read this as the editor
"respecting their work." This is M4's sessions task; the study adds only the scope: it is not
just open files: it is dock visibility, dock sizes, and focus. The test is: quit, reopen,
and the screen is indistinguishable.

### 2.9 Zed's actual innovation: the multibuffer

The one idea in either editor that is *new* rather than well-executed: Zed renders project
search results, diagnostics and diffs as a **multibuffer**: excerpts from many files
concatenated into one scrollable, *editable* buffer. You do not visit results; you edit them
where they stand. It collapses the loop (results list → jump → edit → back → next) that every
other editor, VS Code included, still runs.

This is the highest-leverage candidate in this document and the most expensive. TYPE's
architecture is unusually well-placed for a read-only version (spans render per-line already;
excerpts are viewports), and a read-only grouped-excerpt results panel with in-context
preview is most of the felt value. Editable excerpts touch selections, undo and the buffer
model and should not be attempted before the workspace settles. **Recommendation: read-only
excerpt view for project search and diagnostics as a post-M4 candidate; editable multibuffer
recorded as the long-term bet, unscheduled.**

### 2.10 Speed is the aesthetic

Zed's minimalism reads as beautiful because nothing ever stutters; latency is the design
language. TYPE's budgets (sub-100 ms start, sub-16 ms keystroke) are already the same bet.
The study's only addition: **hold the budgets through the UI build-out.** Every surface this
document proposes is a paint cost, and the visual milestone must extend the perf files to
cover the shell (dock layout, toast paint, settings list) the way editing is covered now.

## 3. Translation table: every GUI surface, and TYPE's answer

| Surface | VS Code / Zed | TYPE's translation | When |
|---|---|---|---|
| Activity bar | Icon strip | **None.** Costs a column, needs icons the font may not have. Panel toggles + palette + status hints do its job | - |
| Primary sidebar | Explorer, outline, git… one at a time | Left dock, one panel at a time, heading names it; `ctrl+b` toggle-with-focus | tree shipped; model M4 |
| Secondary sidebar | Overflow dock | Skip until a panel actually competes for the left dock | - |
| Bottom panel | Terminal, problems, output | Bottom dock: terminal (M5), problems (M3/M4), search results | M4 skeleton, M5 fill |
| Editor groups | Splits, grid, floating windows | Splits (M4). No grid, no floating, terminal owns the window | M4 |
| Tabs | Pinned, preview-italic, drag | Shipped. Preview-tab semantics (single-click reuses tab, edit promotes) worth adopting; pinning later | v0.2.9 + |
| Status bar | Clickable segments both ends | Clickable segments → pickers/actions (§2.6) | visual milestone |
| Quick Pick / palette | One overlay, prefix modes | Shipped as picker; extend prefixes `:` `@`, route all choosers through it | M3+ |
| Settings GUI | Search-first view over JSON | Settings panel over `config.toml` (§2.7) | after centralization |
| Keybindings GUI | Searchable table over JSON | Same widget over `keys.toml` + `bindings_for` | with hint box work |
| Notifications | Toast stack + center | One-line toast above status bar; events toast, states get segments | visual milestone |
| Progress | Status spinner | Status segment + timed-wake spinner (§2.5) | M3 (LSP indexing) |
| Breadcrumbs | Path/symbol row | No row. Symbol path is a status segment candidate at M3 | maybe |
| Sticky scroll | Pinned scope headers | Genuinely good in a TUI; candidate, costs rows only when scrolled into deep scope | post-M4 |
| Minimap | Pixel overview | No. A scrollbar column with match/diagnostic marks gives 80% at 1 column | visual milestone |
| Zen/centered | Chrome off | Falls out of "everything togglable to zero" (§2.3) | free |
| Walkthroughs | Onboarding checklists | Empty-state screens that teach: no file open → the six keys that matter | visual milestone |
| Multibuffer | Zed only | Read-only excerpt view first; editable is the long bet (§2.9) | post-M4 |
| Peek / hover | Inline popup | M3 hover exists; one-rule styling, no box borders | M3 |
| Webviews, marketplace, collab, remote | - | Non-goals, unchanged | - |

## 4. The shell, mocked at 80×24

Everything above, drawn with `visual.md`'s one-rule system. Editor focused, tree receded,
LSP indexing, one toast, bottom dock open on problems:

```
project      │ main.rs ×  lib.rs
▸ src        │  12  fn main() {
  main.rs    │  13      let e = Editor::new();
  lib.rs     │  14      e.run()░
Cargo.toml   │  15  }
             │
             ├──────────────────────────────────────────────
             │ problems                              3 in 2 files
             │ main.rs:14  E0308 mismatched types
             │ lib.rs:3    unused import `std::fmt`
 ✓ saved · config reloaded
 main.rs  Rust  LF  4  ⠋ rust-analyzer 41/117  ✗3   14:11  62%
```

Points the mock decides: the bottom dock splits the *editor column only*: the sidebar runs
full height (both VS Code and Zed default this way; the tree staying tall is worth more than
a wider problems list). The toast line exists only while a toast lives. The spinner and the
diagnostic count are status segments; the count is a click target opening the dock.

## 5. What deliberately does not transfer

Named so they are decisions rather than omissions: the activity bar (§3), minimap, floating
editor windows, notification center, menu bar, drag-to-rearrange docks (rebinding is
configuration; rearrange later if ever), secondary sidebar, walkthrough *documents* (empty
states teach instead), and everything already in architecture §3's non-goals.

## 6. Sequencing against the roadmap

- **M3 (current):** no new scope. Two things land here anyway because M3 needs them: progress
  segment for indexing (§2.5) and the diagnostics count as a segment.
- **M4 (workspace):** the structural half: splits, sessions (§2.8 scope), the focus model
  (§2.2), bottom-dock skeleton, `Panel` trait growing Zed's five answers (§1). Settings
  centralization (§2.7 step 1) belongs here, before it gets harder.
- **M5 (terminal + git):** fills the docks the shell defined.
- **Visual milestone** (`visual.md` is the other input): one-rule build-out, receded focus,
  hint box, toasts, clickable status segments, scrollbar column, empty states, settings and
  keybindings panels.
- **Unscheduled, recorded:** sticky scroll, read-only excerpt view, editable multibuffer.

The through-line: TYPE's invariants (actions, table-row bindings, `Panel`/`RenderContext`,
mouse-keyboard parity) are precisely the substrate these UX patterns need: the palette
already proved it by being a mode instead of a feature. The gap is not architecture; it is
that ten milestones went to correctness and none yet to the shell. That milestone now has its
input.

---

# Part 2. The UI: a concrete visual language

Part 1 is structure and flow. This part is what the surfaces *look like*. It builds on
`visual.md`'s two settled decisions (one rule, no boxes; recede the unfocused) and answers
its three open questions (density, the hint box's home, glyphs). When the visual milestone is
planned, this part merges into `visual.md`; until then it is one study, one file.

Constraints inherited, restated once: the 27 theme slots are the entire palette budget
(`themes.md`); `chrome_bg` is the *only* raised surface: two levels, not three; every colour
choice survives the contrast rubric at truecolor and at 256; the layout works at 80×24; no
glyph outside the symbols table below is ever assumed.

## 7. Density: the numbers, decided

`visual.md` asked; these are the answers, and they are deliberately few:

| Question | Answer |
|---|---|
| Around the rule | one space each side; no panel's text ever touches `│` |
| Gutter | line numbers right-aligned, width = digits of last line + 1, one space before text |
| Tree indent | 2 columns per depth; `▸`/`▾` occupies the first |
| Panel heading | row 0 of the panel, no blank row under it, vertical space is the scarcest resource |
| Overlay width | `min(64, cols − 8)`, centered, top edge at row 2 |
| Overlay height | `min(12, rows − 6)` results; the picker never covers the status bar |

That is the whole density spec. Anything not listed inherits "no chrome without a job": zero
blank separator rows, zero decorative padding.

## 8. The symbols table

One config key, `symbols = "unicode" | "ascii"`, defaulting to unicode. Nerd-font glyphs are
never assumed, anywhere, including in user themes' examples. Every glyph below is single-width
and was chosen for that: `⚠` is rejected because it renders double-width in enough terminals
to break layout.

| Purpose | unicode | ascii |
|---|---|---|
| Directory closed / open | `▸` `▾` | `>` `v` |
| Panel rule | `│` | `\|` |
| Modified (tab, tree) | `●` | `*` |
| Close (tab) | `×` | `x` |
| Error / warning / info / hint | `✗` `▲` `●` `○` | `X` `!` `i` `?` |
| Spinner | `⠋⠙⠹⠸⠼⠴⠦⠧` | `-` `\` `\|` `/` |
| Truncation | `…` | `..` |
| Scrollbar thumb | `▐` | `#` |
| Picker prompt | `›` | `>` |

This table is the *entire* glyph vocabulary of the shell. A new surface picks from it or adds
a row here first: that is how TermIDE and Fresh ended up with coherent `unicode|ascii`
presets, and how symbol sprawl is prevented rather than cleaned up.

## 9. Per-surface specification

Each surface: what it is drawn with, using existing slots. **Part 2 adds one theme slot in
total** (`receded_fg`, the one `visual.md` already costed) everything else reuses the 27.

**Tab bar.** Active tab: `fg`, bold. Inactive: `receded_fg`. Modified: `●` in
`status_bar_accent` before the name: colour carries the signal, position keeps it at
`ascii`. Close `×` on the active tab only (click target; keyboard has `Ctrl+W`). Two spaces
between tabs, no separators, no boxes. Preview-tab semantics (§3) render italic where the
terminal supports it, `receded_fg` where it does not. Underline is never used here:
underline is reserved editor-wide for diagnostics, so it can only ever mean one thing.

**Picker / palette overlay.** Ground: `chrome_bg`, no border: the surface change *is* the
edge, same trick the status bar already uses. Row 1: `›` prompt + query. Results: matched
characters in `status_bar_accent`, selected row on `selection_bg`, result count right-aligned
in `receded_fg`. Appears instantly, no animation. The overlay floats over the editor's `bg`,
so the two-level rule holds: page below, chrome above.

**The `Ctrl+K` hint box** (`visual.md`'s homeless widget): docked full-width directly above
the status bar, on `chrome_bg`, grouped into columns by the binding's declared group, max 8
rows then `… n more` (scrollable: it is a menu, per `controls.md`). Keys in
`status_bar_accent`, descriptions in `fg`, group headings in `receded_fg`. Above the status
bar because that is where the eye already checks state, because it leaves the editor's text
unmoved (the cardinal sin: reflow while mid-chord), and because at 80 columns there is
nowhere else. It pushes the viewport up at most 8 rows for at most one chord's duration.

**Toasts.** One line above the status bar (below the hint box, if both: they cannot
co-occur: a pending chord suppresses toasts). Severity glyph in the matching `diagnostic_*`
colour, message in `fg`, info-level in `receded_fg`. Stacked toasts show newest plus `(+2)`
in `receded_fg`. Expire on timer or any consumed keypress; history reachable via
`>Notifications: Show Log`.

**Status bar.** As shipped, plus: segments become click targets (§2.6), two spaces apart,
and the accent budget stays exactly where it is: `status_bar_accent` means "a state you can
forget you are in" (unsaved, multi-cursor) and now also the error count when nonzero. Nothing
else on the bar is ever accented; an accent that decorates is an accent that no longer warns.

**Problems dock.** Heading row: `problems` left, `3 in 2 files` right in `receded_fg`. Rows:
severity glyph in its `diagnostic_*` colour, `path:line` in `receded_fg`, message in `fg`,
selected row on `selection_bg`. The dock is `bg`, not `chrome_bg`: it holds content you read
and edit against, not chrome; the rule above it is its only frame.

**Scrollbar column.** Rightmost column of the editor, only when the buffer overflows the
viewport (progressive disclosure). Thumb `▐` in `receded_fg`; overview marks at proportional
rows: search matches in `status_bar_accent`, diagnostics in severity colour. This is the
minimap's 80% at 1/119th the cost (§3).

**Empty state.** No file open: centered block, `typ` in `fg`, then five bindings (open,
find file, search project, palette, quit) keys in `fg`, labels in `receded_fg`. Rendered
from the keymap, never hardcoded, so a rebind updates the welcome screen: same one-source
rule as the hint box.

**Hover / diagnostic popup (M3).** `chrome_bg`, anchored to the symbol, max width 60,
wrapped, no border. Inline diagnostics: undercurl where the terminal supports it (the custom
backend's first job), plain underline elsewhere, in severity colour; the gutter carries the
glyph in the same colour.

## 10. Colour discipline: the rules that keep it one system

1. **Two levels.** `bg` is where you read and edit; `chrome_bg` is everything that serves it
   (status bar, overlays, hint box, sidebar). No third surface, ever: `themes.md` already
   states why.
2. **Accent warns, matches, or invites: never decorates.** If a screen has accent on it,
   something wants attention or is interactive. The test: any accented cell, clicked or
   attended to, does something.
3. **Severity colours mean severity.** `diagnostic_*` never moonlights as decoration, so a
   red cell is always a problem.
4. **Weight before colour.** Focus, tab activity and recede are carried by the fg ramp
   (`fg` / `receded_fg`), not by hue, which is what keeps the system alive at 256 colours
   and in themes a user writes badly.
5. **One new slot per capability, audited.** `receded_fg` lands with a `quiet`-floor audit
   rule (`visual.md` already specifies it). Any future slot arrives the same way or not at
   all.

## 11. Motion: the whole policy

The spinner (§2.5) is the only thing that animates: braille frames at ~100 ms, only while a
worker reports activity, drawn as a status segment. No fades, no easing, no slide-ins: the
picker and hint box appear complete in one frame, because appearing *instantly* is the
terminal's native transition and the one thing a browser-based editor cannot do faster.
Damage-driven rendering survives intact: a running worker is damage; nothing else is.

## 12. The shell, drawn

96 columns, editor focused (tree receded), two tabs, LSP indexing, problems dock open,
scrollbar with one error mark:

```
project        │ main.rs ● ×  lib.rs
▸ src          │  11  impl Editor {
  main.rs      │  12      fn main() {
  lib.rs       │  13          let e = Editor::new();
Cargo.toml     │  14          e.run()░                                                  ▐
               │  15      }                                                             ✗
               │  16  }
               ├────────────────────────────────────────────────────────────────────────
               │ problems                                                  3 in 2 files
               │ ✗ main.rs:14   E0308: mismatched types: expected `()`, found `Result`
               │ ▲ lib.rs:3     unused import: `std::fmt`
 main.rs ●  Rust  LF  4  ⠹ rust-analyzer 41/117  ✗ 3   14:11  62%
```

The picker, over the same screen:

```
               │        › src ma
               │        ▌ src/main.rs           ● src ma
               │          src/panel/mask.rs
               │          crates/typ-app/src/main.rs                          37,586
```

And `Ctrl+K` pending:

```
 panels                    files                     view
 e   focus the file tree   n   new file              z   toggle the sidebar
 g   open the git panel    r   rename this file      =   zoom the panel
 ─ press a key, Esc cancels ──────────────────────────────────────── … 9 more
 main.rs ●  Rust  LF  4   ctrl+k                                       14:11  62%
```

Three mocks, one system: the same rule, the same two surfaces, the same accent doing the same
job in each. That uniformity (not any individual widget) is what reads as "designed" in
Zed and what the default ratatui look never achieves, and it is enforceable here because
every one of these surfaces draws from the same 28 slots under the same audit.

---

# Part 3. Delight: pretty, cool, and featureful, without breaking the system

Part 2 keeps the shell honest; Part 3 makes it desirable. Surveyed for this part: Claude
Code's spinner (the most-discussed piece of terminal UI personality in years), the Charm
ecosystem (glow, crush), btop, lazygit, yazi, and the cli-spinners catalogue. One background
finding first: the current consensus method for beautiful TUIs is **design in layers**:
monochrome must be usable, 16-colour must carry hierarchy, truecolor adds the beauty. TYPE
already owns the hard half of that (depth detection, quantisation, a contrast rubric audited
at both depths); nobody surveyed audits their degraded tier. The delight layer lands on a
substrate the pretty apps don't have.

## 13. Where prettiness actually comes from, app by app

| App | The trick | The lesson |
|---|---|---|
| Claude Code | Spinner = glyph pulse + rotating whimsical verbs + live counter; verbs user-configurable | A loading state is a *personality slot*, and users love configuring it |
| btop | Sub-cell graphs: braille (2×4 dots/cell) and eighth-blocks | The character grid has 8× the resolution it appears to |
| lazygit / btop | Spatial consistency, the same info always lives in the same region | Eyes learn coordinates; never reflow the map |
| glow / crush | Gradient text, generous alignment, one accent family | Gradients read as premium *only* against restraint |
| yazi | Instant previews, image protocol where available | Speed + progressive capability = perceived polish |

## 14. The spinner: a signature, not a stock part

Claude Code proved the spinner is brand surface. TYPE should have its own, and the design
space is well mapped (cli-spinners): braille `dots` (80 ms) is the tasteful default the whole
field uses; `star ✶✸✹✺` (70 ms), `arc ◜◠◝◞◡◟`, `point ∙∙∙ ●∙∙ ∙●∙ ∙∙●`, `growHorizontal
▏▎▍▌▋▊▉` are the distinctive ones.

**The signature: animate colour, not just glyph.** Claude Code's shimmer is colour-cycling,
and TYPE has something no stock spinner has: an audited ramp. The `typ` spinner: glyph
sequence `∙ ∙ ●` (the `point` family) with the active dot **breathing through the fg ramp**
(accent → fg → accent) at truecolor, falling back to plain glyph animation at 256, to `-\|/`
at ascii. Three dots because three is the brand quantity: three ways to reach every action.

Shipped as config, because Claude Code proved people *want* this knob:

```toml
spinner = "typ"        # typ | dots | star | line
spinner_verbs = false  # true cycles verbs beside async work, Claude Code-style
```

Verbs off by default (TYPE's voice is drier than Claude Code's) but the slot exists, and a
user's verb list in config is a five-line feature that buys disproportionate affection.
Placement per §2.5 stands: the spinner is a status segment. It never floats over content.

## 15. The sub-cell toolkit

Adopted as shared vocabulary (a `typ-app` paint helper, one implementation):

- **Eighth-blocks `▏▎▍▌▋▊▉█`**: determinate progress at 8× horizontal resolution. The
  indexing segment becomes `rust-analyzer ▋░░ 41/117` in 5 cells, smoother than any GUI
  progress bar of the same width. Vertical eighths `▁▂▃▄▅▆▇█` do the same for the scrollbar
  thumb: sub-row thumb positioning, which makes a 40-row scrollbar track a 100k-line file
  perceptibly.
- **Braille pairs (2×4 dots per cell)**: sparklines and graphs. First customers: commit
  activity sparkline in the git panel heading (M5), CPU/mem in the terminal panel's status
  row if ever wanted.
- **`▰▱` bars**: batch operations with known length (project-wide replace).

Rule: sub-cell glyphs appear inside a surface's existing budget (a smoother thumb, a richer
segment), never as new chrome.

## 16. Micro-interactions: one-shot feedback

The class of delight both VS Code and Zed underuse, and Neovim's most-loved plugin behavior
(`highlight-on-yank`) proves out. All are ≤150 ms one-shots on the timed wake, all skippable:

- **Yank flash**: `Ctrl+C` flashes the copied range (`selection_bg`, 120 ms, one pulse).
  Confirms *what* was copied without a toast. The single highest affection-per-line feature
  in this document.
- **Save pulse**: on write, the filename segment blips accent and fades. Pairs with the
  modified `●` disappearing: two signals, one event, zero toasts.
- **Wrap blip**: `F3` wrapping past EOF blips the position segment once, so "why am I back
  at the top" never gets asked.
- **Undo grouping flash**: undo of a coalesced run flashes the restored range, showing the
  *extent* of what came back. Nobody in the field does this; TYPE's undo coalescing makes it
  honest.

```toml
motion = "full"   # full | minimal | off: minimal keeps spinners, drops flashes
```

## 17. Pretty features that are also features

Where "pretty" and "featureful" are the same line item: each cheap *because* of the
architecture, each a demo moment:

1. **Bracket-pair colourization**: nesting depth coloured from the syntax ramp. VS Code
   shipped it natively after the extension hit 6M installs; tree-sitter makes it a render
   detail here. Opt-in theme table, same audit.
2. **Colour swatches on colour literals**: `#a3be8c` in a buffer gets its actual colour
   painted as a 1-cell swatch beside it. A TUI paints a bg cell; VS Code needs a decoration
   API. Direct payoff in TYPE's own theme files: *editing a theme shows the theme.*
3. **Live theme preview**: arrowing through the theme picker repaints the whole screen per
   selection. `RenderContext` carries the palette, so this is state, not machinery. The
   single best screenshot/GIF the project can produce.
4. **In-editor markdown styling**: headings bold/scaled-by-colour, `code` on `chrome_bg`,
   links underlined + OSC 8 hyperlinked. The md grammar is already compiled in.
5. **OSC 8 everywhere paths appear**: problems dock, picker, status filename: real
   hyperlinks in terminals that support them, invisible elsewhere.
6. **Ghost text blame** (M5): current line's blame in `receded_fg` at EOL, toggleable,
   `receded` discipline keeps it whisper-quiet.
7. **Empty-state wordmark**: the one sanctioned gradient: `typ` glyph-art on the welcome
   screen, accent ramp across it at truecolor, flat accent at 256, plain text at 16. The
   layered-design tier table made visible in the first second of use.
8. **Image preview in the picker** (kitty graphics protocol, tier-gated): far future,
   recorded because yazi proves the wow and the protocol degrades cleanly to "no preview."

## 18. Motion policy v3: supersedes §11

Three classes, one rule each:

| Class | What | Clock behavior |
|---|---|---|
| A, async state | spinner, progress segments | loops **only while a worker reports activity** |
| B, one-shot feedback | yank/save/wrap/undo flashes | single ≤150 ms timer, fires, dies |
| C, everything else | easing, slides, idle loops, cursor trails | **never** |

Damage-driven survives exactly as before: class A's damage is the worker, class B's is the
event. An idle TYPE still draws nothing, costs nothing, and wakes for nothing, which is
itself a feature the GUI editors cannot match, and the reason `motion = "off"` over a
metered SSH connection costs zero capability.

## 19. Delight, sequenced

- **M3 (current):** nothing new. The spinner segment (§2.5) ships with `typ` frames since a
  spinner is being written anyway.
- **Visual milestone:** §14 config, §15 helper + progress/scrollbar, §16 all four flashes +
  `motion`, §17.3 live theme preview, §17.7 wordmark. This is the "TYPE got pretty" release
  and it should be *one* release: delight dripped across six releases reads as noise, landed
  at once it reads as a statement.
- **Rides existing milestones:** §17.1/2/4 with syntax work post-M3; §17.5 with the custom
  backend; §17.6 with M5 git.
- **Recorded, unscheduled:** §17.8 images, braille graphs beyond the git sparkline.

Everything in this part obeys the earlier law: accent still warns/matches/invites, severity
still means severity, two surfaces, one audit: delight is spent *inside* the system, which
is the difference between pretty and decorated.

---

# Part 4: Completeness, postures, and the ideas that are ours

Parts 1–3 translated the shell. This part answers the harder question: why VS Code *feels
complete* and TYPE's plan didn't yet, and it corrects the study's own gaps, found by doing
the census Part 1 skipped.

## 20. The complete feeling has a name: no dead ends

Sit in VS Code and try to get stuck. You can't. Every noun on screen (a tab, a file, a
breadcrumb, a status segment, a problem row) answers a right-click with a menu of what you
can do to it. Every surface has an overflow. Every capability has at least one visible entry
point. **Completeness is not the feature count; it is the absence of dead ends.** A user who
never opens the docs and never hits a wall concludes "this is complete" long before they've
used a tenth of it.

VS Code buys this with thousands of hand-wired menu contributions. TYPE can *derive* it,
because of invariant 2: every primitive is a named `Action`, and `controls.md` already gives
actions a description and a group. Three laws, each generated from the same table:

1. **Every noun has a menu.** Right-click anything → a context menu listing the actions whose
   target is that noun's type (tab → close/close others/pin/reveal in tree; problem row →
   go to/copy message/quick fix; status segment → its picker). The menu is a *query over the
   action table filtered by target type*, the same query the palette and hint box already
   run. VS Code maintains its menus by hand and they drift; TYPE's cannot drift. This is the
   single highest-leverage sentence in this document: **the context menu becomes the fourth
   reachable way, and it costs a query, not a subsystem.**
2. **Every surface names its exits.** A panel's heading row ends with `⋯`: click or `ctrl+k .`
   opens that panel's menu (posture, close, settings that scope to it). No surface is a
   dead end because every surface carries its own door.
3. **The menu bar exists: hidden.** One row, `File  Edit  Selection  View  Go  Help`,
   summoned by `Alt`/`F10` (VS Code's own toggle convention), dropped as palette-backed
   menus, generated from action groups. Zero rows when idle; full GUI-refugee comfort when
   summoned. This reverses Part 1's "no menu bar": the right call was never *no* menu bar,
   it was *no resident* menu bar.

## 21. The census: every VS Code surface, honestly accounted

The audit Part 1 should have contained. Legend: ✔ shipped · ◆ planned/named earlier ·
**bold** = gap this census found · ✕ declined with reason.

| Surface | Status | Where |
|---|---|---|
| Explorer / file tree | ✔ | v0.1 |
| Open-editors list | **gap** | fold into tree heading section, M4 |
| Outline view, goto-symbol | ◆ | M3 `@`, outline panel later |
| Search view + replace UI | ✔ / ◆ | project-wide replace UI is M4-adjacent |
| Source control view | ◆ | M5 |
| Run & Debug | ◆ | v1.2 DAP |
| Extensions view | ◆ | v1.1 host |
| Tabs, splits, palette, quick pick | ✔ / ◆ | shipped / M4 |
| In-editor find widget | ✔ | v0.2 |
| **Code folding** | **gap** | tree-sitter gives fold ranges free; gutter `▾`; unowned until now, assign M4 |
| **Completion UI** | **gap** | v0.3.1 is *next* and had no UI design; see §23.1 |
| Signature help, inlay hints, code actions | ◆ | M3.x LSP surfaces; lightbulb = gutter `✦` + `ctrl+.` |
| Rename inline box | ◆ | M3.x, inline edit at the symbol, not a prompt |
| Peek definition/references | ◆ | the *peek posture*, §22 |
| **Diff view** | **gap** | M5 needs side-by-side and inline diff; design owed before M5 |
| **Merge conflict UI** | **gap** | M5; conflict blocks with pick-left/right actions |
| **Local history / timeline** | **gap → adopted** | §23.2, beloved safety net, cheap for us |
| Breadcrumbs, minimap, sticky scroll | ✕/✕/◆ | Part 2 decisions stand |
| Notifications, progress | ◆ | Part 2 toasts/segments |
| Settings & keybindings editors | ◆ | §2.7 |
| Menu bar | **reversed** | §20.3, hidden, summonable |
| Context menus everywhere | **reversed** | §20.1, generated, invariant-grade |
| Layout drag/drop, floating OS windows | ✕ → **answered better** | §22 postures |
| Welcome/walkthroughs | ◆ | empty states |
| Zen/centered, panel maximize | ◆ | §22 zoom posture |
| **Screencast mode** | **gap → adopted** | keypress overlay; trivial in a TUI, gold for demos/teaching |
| Profiles | ✕ | `TYP_CONFIG_DIR` already is one |
| Remote, ports, notebooks, marketplace | ✕ | architecture §3 non-goals |

Eight real gaps found, five adopted, the completion UI urgent (it's the next milestone).
That table is the honest answer to "are you planning for everything": now, yes, and the
declines are written down beside the adoptions.

## 22. Postures: every panel is popable, hideable, and more

The user-shaped idea in "what if we make it popable or hideable," taken seriously and made
systemic. Neovim proved floating windows in a character grid feel *magical*, and no terminal
IDE has generalized them. TYPE does: **posture is a property of every panel, not a feature
of any panel.**

```
docked    the Part 1 layout: tree left, dock bottom
floating  a rect over the editor, chrome-surfaced, title row, moved/resized
          by mouse-drag or ctrl+k arrows, position persisted per session
zoomed    the panel takes the whole frame (Zed's zoom); same key returns it
peek      a transient float that exists until Esc: goto-definition peek,
          a problem's context, a git hunk: look, decide, dismiss
hidden    gone entirely; state preserved
```

One action cycles posture (`ctrl+k space`), each posture also directly addressable, all five
persisted by the session machinery M4 already owes. The floating terminal over your code, the
problems list floated next to the line it names, a peeked definition that never disturbs your
layout: this is a *capability VS Code physically lacks* (its floats are OS windows, outside
the grid, off the keyboard). The renderer already composites overlays for the picker; a
floating panel is the same paint with a different owner. Invariant 5 unthreatened: a posture
is layout state, owned by the layout, invisible to the panel.

This also retroactively simplifies Part 2: the picker, the hint box and the hover popup stop
being three bespoke overlays and become three panels in `peek` posture. One mechanism, four
surfaces, day one.

## 23. Ideas that are ours: invention, not translation

Where the study stops porting and starts designing. Each of these is possible *because* of
TYPE's architecture and hard elsewhere:

1. **Completion that respects the frame budget** (v0.3.1, urgent). Ghost text for the top
   candidate in `receded_fg`: visible, never blocking, `Tab` accepts (it is unbound mid-word
   today, so it is free). The full list is a `peek` panel below the cursor: filtered as you type,
   documentation for the selected item in a side column *only when the terminal is ≥100
   cols*. Every candidate row shows its source glyph and its *edit consequence* (`→ imports
   std::fmt`), which VS Code buries in a detail pane. Non-modal to its bones: typing never
   waits for it, `Esc` costs nothing.
2. **Local history as a scrubbable timeline.** TYPE already has coalesced undo; persist it.
   `ctrl+k z` opens a peek strip: a braille sparkline of edit activity over time, `←`/`→`
   scrubs the buffer live through its own history: the whole file animates back through
   time under your cursor. Every editor's local history is a list of files; nobody's is a
   *scrubber*. The damage-driven renderer makes each scrub step one cheap repaint.
3. **The agent lane.** `landscape.md` names the AI-agent workflow as TYPE's opening;
   nothing in the plan serves it yet. When a watched file changes on disk (machinery shipped
   in M2.4), don't just reload: **flash the changed ranges** (§16's one-shot class) and mark
   them in the gutter until touched, so a human watching an agent edit sees *what moved*,
   live, like a quiet inline diff. TYPE becomes the best glass cockpit for Claude Code
   sessions: a position neither VS Code nor Zed is even aiming at.
4. **Screencast mode.** `>Toggle Screencast` overlays each keypress + fired action name in
   a corner chip. In a GUI this is a plugin; in a TUI it is ten lines past the keymap. Every
   demo GIF, every bug report, every teaching moment improves.
5. **Warp-to-anything.** One binding scatters two-letter labels over every visible
   interactive noun (tabs, tree entries, problem rows, status segments, panel headings);
   type the label, focus lands there. The whole screen becomes keyboard-addressable in two
   keystrokes: mouse parity's inverse, closing the loop from the other side.

## 24. Sequencing corrections

- **v0.3.1 completion UI (§23.1) is now the most urgent design debt**: it was scheduled
  with no design; §23.1 is its input.
- Folding: assign to M4 (tree-sitter ranges + gutter affordance).
- Postures (§22) land in M4 as the *generalization* of the overlay work the picker already
  did; peek posture arrives first since M3.x's LSP surfaces want it.
- Generated context menus (§20.1) ride the same `Action` metadata work the hint box needs:
  one milestone pays for palette-descriptions, hint box, menus, and menu bar together.
- Diff/merge design owed before M5 starts. Screencast and warp are visual-milestone stuffing;
  the agent lane (§23.3) deserves its own small milestone and its own README paragraph,
  because it is the pitch `landscape.md` says wins the era.

---

# Part 5. Panels: the layout model, measured against TermIDE and ttt

Parts 1–4 took the dock anatomy from the GUI editors. This part checks it against the two
terminal competitors that actually ship multi-panel layouts, read from their docs on
2026-08-30, and then commits TYPE to a layout model. `gap-analysis.md` already lists their
*features*; this is specifically their *layout machinery*, which it never covered.

## 25. What they actually built

**TermIDE: panel groups with adaptive stacking.** The layout is vertical panel groups;
within a group, panels *stack* in one column with adjustable per-panel heights. The layout is
**responsive to terminal width**: ≥160 columns gets sidebar + two file-manager panels, below
that the sidebar stacks Git Status, File Manager and Operations in one column. Navigation is
spatial: `Alt+←/→` between groups, `Alt+↑/↓` within a group, `Alt+1–9` jumps to a panel,
`Alt+K` opens a per-panel action menu. `Alt+F11` is the finding of the section:
fullscreen-current-panel is a *preset* where **every other panel collapses to its title row**:
the map survives, only the territory shrinks. `Ctrl+Alt+=/-` resizes by 3 lines. Sessions
auto-save and restore layouts; bookmarks live in `bookmarks.toml`.

**ttt: fixed regions, rich contents.** One sidebar (`ctrl+k e`) holding explorer, search,
git changes, outline and plugins as switchable panels; one bottom panel (`ctrl+k b`) with
**Diagnostics, References and Terminal as tabs**. The terminal is the deep end: its own tabs
(`ctrl+k t` spawns one, vertical inner tab bar), fullscreen toggle (`Alt+T`), and (the
detail that matters for M5) a **named force-key list**: when the PTY has focus, *every* key
goes to the shell except `Ctrl+T`, `Alt+T`, `Ctrl+Q`, `Ctrl+P`. File tabs are the VS Code
preview model (open replaces the unpinned tab; re-click pins), drag-reorderable, with
Close/Close Others/Close All on right-click. Dividers drag with the mouse. Folding is
`ctrl+k [`.

**And the striking absence: neither ships real editor splits.** ttt has no side-by-side
editor columns at all; TermIDE's groups hold *different panels*, not two views of one buffer.
Among non-modal terminal IDEs, **M4's splits are a differentiator, not table stakes**, only
Helix has them in the terminal field, and it is modal. Worth knowing before M4 treats splits
as a checkbox.

## 26. What this corrects in Parts 1–4

1. **"One panel at a time per dock" was wrong.** Part 1 §3 assumed VS Code's
   one-view-visible sidebar; both VS Code (collapsible Explorer sections) and TermIDE
   (stacked panels, adjustable heights) actually *stack*. A dock holds an ordered stack of
   panels, each collapsible to its **title row**, heights adjustable by drag or
   `ctrl+k =`/`ctrl+k -`. A collapsed panel's title row is its reminder and its click
   target: the dock becomes an accordion, and "tree + outline + git status visible at once"
   is a layout, not three toggles fighting for one slot.
2. **Zoom posture adopts TermIDE's `Alt+F11` semantics**, not VS Code's maximize: the zoomed
   panel takes the frame and every other visible panel collapses to a title row. You never
   lose the map. Un-zoom restores exact heights, which session persistence must therefore
   store.
3. **Responsive layout presets are a real, unowned question.** TYPE has an 80-column floor
   and no idea what 200 columns means. Adopt TermIDE's move as a rule: **width thresholds
   change the *default* layout, never the user's explicit one.** Below ~100 columns docks
   are exclusive (opening one hides the other); at ≥160 a right dock becomes available by
   default. Numbers to be tuned in mockups; the rule lands now.
4. **The force-key list becomes part of the layer design.** `controls.md` §3 already makes
   the terminal panel "a keymap that is nearly empty"; ttt names what stays in it. TYPE's
   equivalent: the panel-toggle keys, `ctrl+q`, `ctrl+p`, and `ctrl+k`: everything else
   reaches the PTY. Four names instead of a policy sentence.
5. **The References surface was missing from every part of this study and every TYPE plan.**
   Find-all-references lands with LSP and its results need a home; ttt gives it a bottom-dock
   tab beside diagnostics. Same excerpt-list widget as the problems dock (§2.9's read-only
   excerpt view serves both). Added to the census as found-by-field.
6. **Bookmarks: adopt as a picker mode, not a panel.** TermIDE ships them as a file;
   TYPE's version is a `'` prefix in the picker (marks named, ranked, persisted with the
   session): the platform rule of §2.1 instead of a fifteenth sidebar panel.

## 27. TYPE's layout model, committed

One paragraph, so M4 has a spec instead of a vibe:

> The window is an **editor area** plus up to three **docks** (left, bottom, right) and a
> status bar. The editor area is a **binary split tree of panes**; each pane shows tabs;
> tabs are views onto a central buffer set (one buffer may appear in two panes). Each dock
> holds an **ordered stack of panels**, each panel collapsed-to-title or expanded with a
> stored height. Every panel has a posture (§22): docked, floating, zoomed, peek, or hidden.
> Focus follows toggle-with-focus (§2.2); `Esc` returns to the editor; the terminal panel's
> keymap passes everything but the force keys to the PTY. Width thresholds pick default
> layouts; the session stores the whole tree (splits, stacks, heights, postures, focus)
> and restores it indistinguishably.

Everything earlier in the study hangs off this paragraph unchanged: the one-rule visual
system draws it, postures move things through it, the census fills it, and the `Panel` trait's
five answers (§1) are exactly the data it needs from each panel.

## 28. Panel roster: who lives where, through v1

| Dock | Panels, in likely stack order | Arrives |
|---|---|---|
| Left | tree · outline · git changes | shipped · M3.x · M5 |
| Bottom | problems · references · search results · terminal (own tabs) | M3.x · M3.x · M4 · M5 |
| Right | (empty by default; available ≥160 cols for whatever the user drags there) | M4 |
| Editor area | buffers · settings panel · keybindings panel · diff view | - · §2.7 · §2.7 · M5 |
| Peek | hover · definition peek · completion list · hint box · picker | M3 · M3.x · v0.3.1 · M4 · shipped |

TermIDE's viewer zoo (hex, image, markdown, mermaid, database) stays post-v1 behind
`OpenWith`/`typ-registry`, exactly as architecture already bets: fourteen of its
forty-five crates are panels, and every one of them is a registration in this model, not a
change to it.

## 29. Native to the terminal: why the app feels detached, and the fixes

Diagnosed from a live screenshot (Windows Terminal, v0.3.x): TYPE reads as *a program drawn
inside a terminal* rather than *the terminal being an editor*. Four causes, each with a fix,
ordered by how much of the feeling each one carries:

1. **TYPE paints an opaque ground that is not the terminal's.** Slate's `#10141b` sits inside
   Windows Terminal's own darker ground, and the terminal's padding frames it: the app is
   visibly a rectangle floating on someone else's colour. The existence proof that this is
   the cause and not a fact of TUI life: opencode and oh-my-pi in the same terminal feel
   native, and both get it the same way: **they never stamp a background; they colour
   elements and leave every other cell on SGR default**, so the terminal's ground (padding,
   acrylic, all of it) shows through and there is no rectangle to see. omp additionally runs
   inline rather than on the alt screen; an editor cannot take that half, but the ground
   half transfers whole. Fix: `bg = "terminal"` (the page ground is *no background at all*)
   and it is **the default experience, not an option**: first launch puts TYPE on the user's
   own ground, following their light/dark, chrome painted only where chrome earns it. Opaque
   grounds remain what a *deliberately chosen* theme (Slate, mocha…) ships. Rubric
   consequence, solved: an adaptive theme is audited against the ground OSC 11 reports at
   startup, and falls back to its declared colour when the terminal will not answer or the
   report clashes with the theme's `kind`.
2. **The editor is still in a box.** The screenshot shows the full `┌─ app.rs ─┐` frame that
   `visual.md` rejected: an app-in-a-box is the *strongest* "detached" signal on screen, and
   the tree not being boxed makes it worse. Already decided; the screenshot is the evidence
   of what it costs to not have built it.
3. **TYPE never speaks to the terminal about itself.** The tab still says "PowerShell".
   Fix: OSC 0/2 window title (`app.rs · typ`) updated on tab switch, restored on exit; and
   DECSCUSR so the cursor shape is deliberate. An app that names the terminal's tab *is* the
   terminal in a way no amount of internal paint achieves. Cost: a few lines in the backend.
   In no plan until now: census addition.
4. **TYPE ignores the user's own palette.** A person who configured their terminal's sixteen
   ANSI colours sees TYPE override every one of them. Fix: ship a `terminal` theme whose
   `[ui]` maps to the sixteen ANSI slots and whose ground is `bg = "terminal"`: TYPE then
   wears whatever the user already chose, the way lazygit and a base16 Helix do. Their
   terminal, their colours, zero configuration. Pairs with the light/dark following that
   `gap-analysis.md` Part 7 already designed (OSC 11 + DEC 2031): between them, the terminal
   changes and TYPE changes *with* it, which is the definition of not-detached.

Sequencing: 2 is the visual milestone. 1 and 4 land with the OSC reply parser that
light/dark following already requires (and that defect 41 requires for *correctness*): one
input-layer investment, four native-feel features. 3 is small enough to ride any release
after the custom backend exists.
