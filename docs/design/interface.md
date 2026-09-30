---
type: design
status: proposal
area: shell
verified: 2026-09-29
verified-against: v0.3.0
---

# TYPE interface spec

**Status: proposal, not built.** What every surface of TYPE looks like and how you reach it. Where
this disagrees with `visual.md`, `ui-ux-study.md` or `controls.md`, this wins. The mockups live in
the Claude Design project "Type terminal IDE mockups" (`TYPE Interface.dc.html`,
`TYPE Interface Earlier.dc.html`, `TYPE Settings.dc.html`), outside the repo. Mock ids (`3b`,
`4c` …) point to its canvas. The mockups are the reference; this file is the rulebook. §12 says
when each part lands.

---

## 0. The rules everything follows

1. **Every action is reachable by keyboard and by mouse.** Every mouse target has a warp label,
   so it has a key. Every action has a row in `ctrl+p`, so it can be clicked. A test walks the
   action table and fails the build if an action is missing one of the two. (3d)
2. **Two surfaces.** `bg` is where you read and edit. `chrome_bg` is everything that serves it:
   status bar, sidebars, tab rows, floats. There is no third surface.
3. **Lines mean docked, boxes mean floating.** Docked regions are separated by 3:1 rules and
   never boxed. Anything drawn *over* other content gets a rounded border. (3a, 3c)
4. **Contrast is measured, not eyeballed.** Text ≥ 4.5:1 against its own ground. Rules and
   borders ≥ 3:1 (WCAG 1.4.11 non-text). A surface change on its own never carries separation.
5. **Accent warns, matches, or invites.** An accented cell always does something when you click
   it or look at it. It never decorates.
6. **Focus is always visible.** The focused region's label is accent and bold, the hardware
   cursor is inside it, and everything unfocused recedes one ramp step.
7. **Width picks the default layout, never the user's.** (3f)
8. **Motion is one-shot or tied to work.** Spinners run only while a worker reports activity.
   Flashes last ≤150 ms. Nothing idles.

---

## 1. Colour and contrast (Slate)

The contrast audit found that rules were 1.3:1 and `chrome_bg` against `bg` only 1.14:1, so the
eye could not tell the editor from a dock. Fixed values:

| Role | Shipped at v0.3.0 | Now | Ratio on bg | Ratio on chrome_bg |
|---|---|---|---|---|
| Rule / border (new slot `rule`) | `#2a3240` (`border`) | `#626e87` | 3.6 | 3.2 |
| Receded text (new slot `receded_fg`) | `#8495ac` (`status_bar_inactive_fg`) | `#9aa5b5` | 7.4 | 6.5 |
| Indent guide (decorative) | `#7889a0` | `#3b4557` | 1.9 | - |
| Active indent guide (new) | - | `#7889a0` | 5.2 | - |
| Error glyph on selection | `#ec767f` | `#ff9aa2` | - | 4.6 on `#2b456f` |
| Focus accent | `#68a6e4` | unchanged | 7.2 | 6.3 |
| Body text | `#cdd5e1` | unchanged | 12.5 | - |
| Line numbers | `#7889a0` | unchanged | 5.2 | - |

Latte check (4f): rule `#7c7f93` (overlay2) = 3.5:1. overlay1 fails at 2.8. Receded `#5c5f77`
= 5.6:1.

**New theme slots**, each with an audit rule at truecolor and at 256:

- `rule`: ≥ 3:1 on `bg` and on `chrome_bg`
- `receded_fg`: `quiet` floor on both grounds
- `indent_guide_active`: `quiet` floor
- `float_border`: defaults to `accent`, ≥ 3:1 on both grounds
- `scrim`: the colour step modal backdrops are repainted to
- `git_added` / `git_modified` / `git_removed`
- `diff_add_bg` / `diff_del_bg` / `diff_add_word_bg` / `diff_del_word_bg`

---

## 2. Separating regions (3a)

Three options were compared in the same state, with the dock focused:

| | How | Verdict |
|---|---|---|
| **A · labeled rule** | One 3:1 rule is the dock's heading. The dock's tabs are cut into it. When focused, the label turns accent + bold and the rule segment before it turns accent. | **Chosen.** One row, no fill, works on any theme and at 256 colours. |
| B · header strip | A filled `chrome_bg` row with an accent `▌` on focus | Fallback for themes with a strong `chrome_bg`. Reads heavy. |
| C · surface | The whole dock on `chrome_bg` under a rule | Rejected. Content looks like chrome, and at 1.14:1 it barely differs. |

**The labeled rule is also the dock's tab bar.** Click a label, or `ctrl+j` then `←→`. Drag the
rule to resize. Double-click it to zoom. The right end holds the count, the posture toggle `◰`
and the panel menu `⋯`.

Vertical boundaries (sidebar | editor | right dock | split panes) are single 3:1 rules. Panes
never get boxes.

---

## 3. Floats (3c, 2c, 2f, 4a, 4b)

Anything drawn over other content (picker, completion, signature, hover, context menu, menu bar
dropdown, floating panel) follows these rules:

- A rounded border in `float_border` (accent) when it has keyboard focus, and `rule` when it
  doesn't (e.g. signature help while completion owns the keys).
- **Its name is cut into the top-left of the border. Its exit (`esc`, or dock / zoom / close for
  panels) is cut into the top-right.**
- **A one-cell clear gutter** in `bg` around the border, so the border never touches code. This
  is what made the 2f terminal readable.
- **Modal floats** (picker, menus, layout picker) also repaint the screen behind them one step
  darker (SGR dim or recolour to `scrim`), but never the status bar. **Non-modal floats**
  (completion, floating terminal) don't dim the screen: you work with both.
- No drop shadows and no animation. A float appears complete in one frame.

---

## 4. Focus

- The focused region's label (active tab, dock rule label, sidebar heading) is **accent +
  bold**. The editor's active tab also gets a 2-pixel top line where the terminal supports it
  (half-block `▀` fallback).
- The hardware cursor is always inside the focused region.
- Unfocused regions recede: text drops to `receded_fg`, syntax colour is removed.
- When the keyboard is in the status bar, the focused segment gets `selection_primary_bg`.
- **A focus stack, not a cycle.** `esc` in any panel returns to the pane you came from.

---

## 5. Navigation: keyboard and mouse (3d)

| To reach | Keyboard | Mouse |
|---|---|---|
| Anything visible | `ctrl+k j`, then its two-letter label | click |
| Region next to this one | `alt+←↑→↓` (spatial) | click inside it |
| Every region in order | `f6` / `shift+f6`, including tab rows and the status bar (screen-reader path) | - |
| Left dock | `ctrl+b`: open + focus → focus → close | click its heading |
| Bottom dock | `ctrl+j`, same three steps | click a label in its rule |
| Right dock | `ctrl+k a` (agent) or `ctrl+k o` (outline) | click |
| Back to the editor | `esc` | click code |
| Tabs | `alt+,` `alt+.` · `alt+1…9` · `ctrl+w` | click · middle-click closes · drag to reorder or split |
| Panes | `ctrl+k 1…9` · `ctrl+k \` split right · `ctrl+k -` split down | click pane number · drag tab to edge |
| Inside any list | `↑↓ pgup pgdn home end` · `enter` · `space` preview · `←→` fold · `/` filter | click · double-click · wheel |
| Context menu | `shift+f10`, menu key, `ctrl+k .` | right-click |
| Status bar segment | `f6` to the bar, `←→`, `enter` opens its picker | click |
| Resize | `ctrl+k r`, then arrows, `=` resets | drag any rule |
| Move a float | `ctrl+k m`, then arrows | drag its top border |
| Menu bar | `f10` or `alt+letter` | click once it's shown |
| Everything else | `ctrl+p` then `>` | menu bar |

**Warp labels:** the first letter is the region (`s` side, `t` tabs, `e` editor, `d` dock, `b` bar,
`r` right dock, `f` float), the second follows screen order. With practice, `ctrl+k j d1` always
means "first problem". The rest of the screen dims while labels are up.

**Force keys in a terminal panel:** everything goes to the shell except `ctrl+k`, `ctrl+p`,
`ctrl+q` and the dock toggles. The floating terminal names them in its footer.

---

## 6. The ctrl+k shortcut menu (3e, 2e)

A terminal can reliably deliver about 50 chords, and an IDE needs hundreds. `ctrl+k` is a door:

1. Press `ctrl+k`. A menu docks above the status bar straight away (no timer), grouped into
   columns (panels, layout, panes, editor, find…). The editor's text never moves sideways.
2. Press a letter, or use arrows + `enter`, or click a row. `esc` cancels.
3. The action runs and the menu is gone. For 2 s the status bar shows `ctrl+k t · terminal`, so
   using the menu teaches the direct chord.

The menu is generated from the keymap: a rebind updates it. It shows only what's reachable from
the focused layer.

---

## 7. Layouts (3f, 2g, 4g)

| Preset | What | Default when |
|---|---|---|
| **Focus** | editor only, docks folded to their labeled rule | **shipped default** |
| Standard | left dock + bottom dock | - |
| Wide | left dock, split panes, right dock | ≥ 160 cols with "adapt" on |
| Review | diff multibuffer + agent dock | opened by the review lane |
| Saved | any layout, stored as a file | per project, if "remember" is on |

- Change it by clicking `▣` in the status bar, with `ctrl+k l`, `>layout`, `View › Layout preset…`,
  or `layout = "focus"` in config.
- Arrowing through presets live-previews them on the real screen.
- **Adapt to width** (on by default) steps down Wide → Standard → Focus as the terminal shrinks
  and back up as it grows. A layout picked by hand always wins.
- Below 100 cols, docks are exclusive. Status segments drop in this order: symbol path →
  branch → LF/indent → progress label (the bar stays) → percentage. Position and error count
  never drop.

---

## 8. Surfaces

**Workspace (3b).** Left dock is a stack: tree, outline, changes, timeline, each collapsible to
its title row. Editor: sticky scope row (only when scrolled inside a scope), inlay hints on
`chrome_bg`, diagnostic at end of line with its fix key, git bars in the gutter, fold markers,
a scrollbar column with match and diagnostic marks. Bottom dock tabs: problems · references ·
search · terminal · output. Problems is a multibuffer: excerpts are real lines, and fixes are
previewed in place.

**Picker (3c).** One modal float for every "choose one of N". Its mode strip shows the prefixes
`> @ # : ' % ?`; clicking a mode types its prefix. Results from other modes appear under the
file results. A preview column is shown only at ≥ 100 cols. The footer shows the keys, and every
row has a menu.

**Code intelligence (2c).** Ghost text for the top candidate (`tab` accepts), the completion
list as a focused float, a docs column at ≥ 100 cols, and signature help as an unfocused float.
Code actions show as `✦` in the gutter and at end of line (`ctrl+.`). While a float is open, the
status bar's left half shows its keys.

**Review lane (2d).** File changes from any process (agent, formatter, git checkout) arrive as a
live diff multibuffer with word-level tint. Per hunk: `a` accept, `r` reject, `e` edit in place,
`c` comment to agent. `]h` / `[h` move between hunks. The agent dock shows steps, checks and the
follow toggle; your keystroke always wins. The history strip is a braille sparkline you can
scrub with `←→`.

**Floating terminal / debug (2f).** A terminal in float posture, with an accent border and a
clear gutter. Debug: the paused line on `amber_deep`, inline values on `chrome_bg`, stepping
keys docked where the ctrl+k menu appears.

**Menu bar (4a).** Hidden and summoned by `f10`/`alt`. Takes zero rows when idle. Generated from
action groups, with real keys on every row. Disabled rows are shown receded.

**Context menu (4b).** A query over the action table filtered by the target's type. Opens at the
cursor when triggered from the keyboard. Type to filter.

**Settings (4c).** A tab in the editor area. Search first, results grouped under labeled rules.
`●` marks a modified value. Each row shows its value control (`‹ 4 ›`, `[x]`), its default and
its source file. Every edit writes to `config.toml` or the project file, keeping comments.
`ctrl+o` opens the raw file.

**Keybindings (4d).** The same list over `keys.toml`: action, key, source, layer. Press keys to
record. `ctrl+r` searches by pressing the key. Conflicts and shadowing are flagged. Chords this
terminal can't send are flagged too, and startup warns about them.

**Empty state (4e).** The wordmark (the product's only gradient) sits over the recent projects
(press 1–3) and six keys taken from the keymap.

---

## 8b. Settings and keybindings in detail (TYPE Settings.dc.html, turn 5)

**Settings (5a–5c).**
- Layout: a search row with a scope toggle (user / project, `ctrl+s`), a section column
  (26 cols, only at ≥ 100 cols), and one flat list grouped under labeled rules (`editor › indentation`).
  Below 100 cols the sections fold into the query as `@section`.
- Query prefixes: `@section`, `:modified`, `#language`, as in the picker.
- Row = two lines: name + control, then description + default + source. `●` marks a value that
  differs from the default. `project` badge when a project file set it.
- Five controls: enum `‹ ›` (enter opens a bordered list with live preview), toggle `[x]`,
  number/text field (red border + message on invalid, esc reverts), colour swatch, chip list
  (`×` removes, `+ add`).
- Shadowing is stated (`▲ your user value is overridden by .typ/config.toml`). Settings that need
  a restart are labelled before you change them. `ctrl+z` undoes the last change.
- Keys: `tab` moves between sections and list · `↑↓` rows · `←→` change · `enter` edit · `space`
  toggle · `r` reset · `?` docs · `ctrl+o` open raw file. The mouse clicks the same controls.

**Keybindings (5d–5e).**
- Table: action · key · layer (global / editor / terminal…) · source (default / user). Filters:
  modified, conflicts, unbound, needs-kitty (`:modified :conflict :unbound :kitty`).
  `ctrl+r` searches by pressing the key itself.
- Recording is a modal float: keycaps appear as you press them, and prefixes wait for the next
  key. It shows a capability check (universal / needs kitty / tty-owned) and a live conflict.
  Resolution: **replace** (default) · keep both in different layers · cancel.
- Keyboard view: choose a modifier or prefix (ctrl, alt, ctrl+k ▸, ctrl+l ▸, f-keys). Each key
  shows its action. Free keys are outlined, `tty` keys (ctrl+i/m/[/h) are shown as taken, and
  `kitty` keys are marked. Arrows or click select a key, `enter` rebinds, `del` unbinds.

## 9. Glyphs added to the symbols table

All must be checked single-width in the common terminal list before shipping; each has an ascii
fallback.

| Purpose | unicode | ascii |
|---|---|---|
| Git gutter bar | `▎` | `\|` |
| Code action | `✦` | `*` |
| Float / dock posture | `◰` `◧` | `[]` `[\|` |
| Layout segment | `▣` | `#` |
| Breakpoint / paused | `●` / `‖` | `o` / `=` |
| Sparkline | braille `⣀⣤⣶⣿` | `_.-^` |
| Menu check / submenu | `✓` `▸` | `x` `>` |

---

## 10. VS Code parity (3g)

**Have. Finish the shell:** explorer, tabs (add preview + pin), quick open / palette, find and
replace (project replace via excerpts), multi-cursor, syntax, LSP basics, clickable status bar.

**Build next (what makes it an IDE):** splits and editor groups (TermIDE and ttt have none);
completion / signature / code actions / rename; problems / references / terminal dock;
source control with diff, hunk staging and merge; settings and keybindings UI; folding / sticky
scroll / outline; generated context menus and the hidden menu bar.

**Later:** debugger (DAP), tasks and test explorer, extension host (limited to panels, actions and
pickers, once the core API is stable), styled markdown and kitty image preview, multi-root
workspaces, screen-reader mode (plan the focus order now).

**Never, or the terminal already does it:** Remote-SSH and dev containers (run `typ` there),
minimap (scrollbar with marks instead), activity bar, webviews / notebooks / Live Share,
floating OS windows (postures cover it), settings sync / accounts / profiles (dotfiles and
`TYP_CONFIG_DIR` do it).

---

## 11. Open

- Git panel + merge conflict screen: not yet mocked.
- Screen-reader mode: focus order defined (f6), glyph policy not yet.
- 256-colour and ascii renders of 3b: to be mocked and audited.
- Warp label collisions above ~200 targets: fall back to three letters or to region-first
  labelling.

---

## 12. Sequencing

What exists at v0.3.0 that this builds on: boxed panels whose corners join where two meet,
`chrome_bg` as the one raised surface, the picker as one overlay with `>` for commands, the
contrast audit in `typ-core`, tabs, and hover and diagnostics from the
LSP client. What does not: docks, splits, postures, the `ctrl+k` prefix mechanism
(`controls.md` §2), any float border, and every slot in §1 marked new.

In the order the dependencies force. The durations are estimates from the pace so far: v0.2.1
to v0.2.10 took eleven days, and M3 took about a week.

| Slice | Spec | Depends on | Estimate |
|---|---|---|---|
| **1. Colour and floats** (shipped in v0.3.2) | §0.2–0.5, §1 slots and values with their audit rules, §3 applied to the picker and the hover box, §4 recede. Actual: `rule` shipped as the existing `border` field (a rename breaks theme files); `indent_guide_active`, the git and diff slots and "error on selection" are left to the slices that draw them | nothing | 2 to 3 days |
| **2. The door** (shipped in v0.3.3) | §6 `ctrl+k` menu generated from the keymap, the status bar teaching line, §4 focus stack, `f6` order. Actual: the focus stack is one deep and `f6` walks two regions, because docks and the status bar are not focusable yet; `esc` in the editor never moves focus, it is the pane the others return to; the menu shows every row under the prefix, since no panel has a layer of its own | `Action` carrying a description and a group (gaps 52, 53) | 3 to 4 days |
| **3. Layout model (M4)** | §2 labeled rules as dock tab bars, left / bottom / right docks, §5 splits and panes, postures (docked, float, zoom), §7 presets and adapt-to-width, sessions | slices 1 and 2 | 2 to 3 weeks |
| **4. Menus and warp** | §5 warp labels, §8 menu bar and context menus, both generated from the action table; the §0.1 parity test | slice 3 for regions to label | 3 to 5 days |
| **5. Settings and keybindings** | §8b both surfaces, writing TOML with comments kept | slice 3 (a tab in the editor area) | about a week |
| **6. Code intelligence UI** | §8 completion, signature help, code actions, rename, the problems multibuffer | slice 3 floats and docks, LSP requests beyond hover | 1 to 2 weeks |
| **7. Terminal, git, review (M5)** | floating terminal, git gutter and panel, §8 review lane and agent dock | slices 3 and 6 | a month or more |
| **Later** | debugger, tasks, extensions, screen reader | M5 | after v1.0 |

So: the look (slices 1 and 2) is about a week away. The shell in the mockups, with docks,
splits, layouts, menus and settings, is slices 1 to 5, roughly five to six weeks. Everything in
this document is two to three months.
