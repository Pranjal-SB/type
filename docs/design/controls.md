---
type: design
status: draft
area: spec
verified: 2026-08-22
---

# Controls — the keyboard model

**Status: half built as of v0.2.9.** The tier analysis in §1 is load-bearing and has been used —
it is why the command palette is reached by typing `>` into `Ctrl+P` rather than by
`Ctrl+Shift+P`, and why tab switching is bound to `Alt+,`/`Alt+.` as well as the page keys. The
*mechanism* in §2 — sequence bindings, `Resolved::Pending`, the generated hint, `Action` carrying
a description and a group — is not built. Gap analysis 52 and 53 track what that leaves.

The decisions still constrain M2.6 (kitty protocol), M4 (splits) and M5 (terminal panel), and two
of them were arrived at by measuring the field rather than by taste.

The companion question — what TYPE *looks* like — is deliberately absent. See
[Open](#open-questions).

Spec: [`architecture.md`](architecture.md) §4 "Mature", which commits to mouse and keyboard as
peers and to every action being reachable three ways.

## The problem

The spec says peers. The tree says otherwise. Panel focus is two bindings:

```
f6         FocusNext
ctrl+tab   FocusNext
```

Both cycle. There is no "go to the tree", no close, no resize, no way to discover what exists.
`Keymap` is `BTreeMap<String, Action>` and `lookup` returns `Option<Action>` — a flat, global,
single-chord map with no notion of a prefix and no notion of context.

That is enough for two panels and an editor. It does not reach an IDE, and the failure is not
gradual: at M5 the terminal panel needs `Ctrl+C` to reach the PTY rather than quit the editor,
and the current design **cannot express that at all**.

## 1. Two tiers, and it is forced

Not a preference. Terminals cannot deliver the full VS Code chord set, and TermIDE's keybinding
documentation is the clearest statement of which ones survive:

**Universal — every VT100+ terminal:** `Ctrl+letter`, `Alt+letter`, `Alt+digit`,
`Alt+punctuation`, `F1`–`F12` with one modifier, arrows / `Home` / `End` / `Tab` / `Enter` /
`Esc` with one modifier.

**Enhanced — requires the kitty keyboard protocol:** `Ctrl+punctuation`, **`Ctrl+Shift+letter`**,
`Ctrl+Alt+anything`, `Alt+Shift+letter`, `Super`/`Meta`.

`Ctrl+Shift+letter` being Enhanced is the finding that matters: **`Ctrl+Shift+P` and
`Ctrl+Shift+E` — VS Code's command palette and explorer — cannot be defaults in a terminal.**
An early draft of this design recommended them and was wrong.

The universal budget is roughly 26 `Ctrl+letter` plus 26 `Alt+letter` plus digits and
punctuation, minus everything editing already spends on cut, copy, paste, save, undo and find.
An IDE does not fit. **A prefix is not a stylistic choice, it is the only way to reach the
rest.**

ttt reached the same conclusion independently, and it reads as VS Code because VS Code does the
same thing:

```
ctrl+b        sidebar.toggle        direct
ctrl+p        command.palette       direct — note: not ctrl+shift+p
ctrl+0        sidebar.focus
ctrl+k e      sidebar.explorer      prefix
ctrl+k t      terminal.new
ctrl+l f      editor.formatDocument second prefix, language actions
ctrl+l r      editor.findReferences
```

**Decided:** frequent actions get universal single chords; everything else hangs off `ctrl+k`.
Enhanced-tier bindings may ship where they are de-facto standards — TermIDE keeps `Ctrl+/` for
comment-toggle on exactly this reasoning — but each one is a deliberate, documented exception
and the startup path warns when the terminal cannot deliver a configured Enhanced binding.

**Any number of prefixes; one named so far.** Resolution treats a prefix as a range bound, so
`ctrl+l` costs nothing structurally once `ctrl+k` works — nothing in the mechanism knows how many
there are. What is deferred is *naming* them, because a prefix's identity comes from its
contents and the language actions that would fill `ctrl+l` do not exist until M3. Build for many,
ship one.

**This gives M2.6's kitty-protocol work a reason it did not have.** It was scoped as
`Ctrl+I`-versus-`Tab` disambiguation. It is actually the difference between half the keymap
existing and not.

## 2. Prefix resolution, and the hint

`BTreeMap` turns out to be exactly the right container, for a reason unrelated to why it was
chosen. It is ordered, so every binding under a prefix is a range scan —
`range("ctrl+k ".."ctrl+k!")` — in O(log n + k). Helix builds a trie for this; TYPE gets the
same query free from a container already present so that help listings come out in a stable
order.

Three changes:

1. **Binding keys become sequences.** `"ctrl+k e"` beside `"ctrl+b"`. Still a table row, so
   **invariant 3 is untouched** — rows stay the source of truth and the prefix index is a query
   over them, not a second representation.

2. **`lookup` gains a third outcome:**

   ```rust
   enum Resolved {
       Matched(Action),
       Pending(Vec<(String, Action)>),   // from the range scan
       NotFound,
   }
   ```

3. **The hint renders from `Pending`'s payload.** Generated from the bindings, never authored.
   This is the property worth protecting: a hand-written menu drifts from the keymap the first
   time somebody rebinds something, and nothing catches it.

### The hint is a surface, not a reminder

A flat thirty-row dump of `key → ActionName` is a debug view. What it has to be:

- **Grouped.** `ctrl+k` shows sections — panels, files, view — not one alphabetised list. A
  binding declares its group in the same table row that declares its key, so grouping stays
  generated rather than authored.
- **Described.** A human sentence per binding, not the action's identifier. `Action` needs a
  description alongside its name, and that description is the same string the command palette
  shows at M4 — one source, two surfaces.
- **Navigable.** Arrows move, `Enter` runs, `Esc` cancels. That makes it a menu as well as a
  hint, which is what closes the loop on "every action reachable three ways": the same box is
  the keyboard path and, being drawn, the mouse path.

The cost is that `Action` grows a description and a group, and every binding row gains two
fields. That is the right cost — it is what makes the palette, the hint and the help listing one
thing instead of three that drift.

### No timer

Helix and Neovim disagree here and the disagreement is instructive. `which-key.nvim` (7.3k
stars) delays 200 ms. Helix shows the box immediately:

```rust
KeymapResult::Pending(node) => cxt.editor.autoinfo = Some(node.infobox()),
```

The difference is modality. In vim `d` is both an operator and a prefix, so an immediate popup
flashes on every delete and the delay is a debounce. TYPE is non-modal with an explicit `ctrl+k`
prefix — **there is no way to be accidentally pending**, so there is nothing to debounce.

Immediate. No timer to tune, no test for "did the user pause", no configuration.

which-key's Hydra mode — popup stays until `Esc` — is declined. It is a mode, and modes are the
thing this editor chose not to have.

### Nobody else in the terminal does this

Searched ttt, TermIDE and Fresh for a pending-chord hint: zero hits. ttt has a
`KeybindingsWidget`, but it is a 671-line sorted reference you open deliberately, not something
that appears while your fingers are mid-chord. Helix has it; no terminal *IDE* does.

## 3. Context: layers and predicates, in that order

VS Code and Zed both scope every binding by context. VS Code has 56 context keys in
`editorContextKeys.ts` alone and `when` clauses like `editorTextFocus && !editorReadonly`. Zed
is the same idea, smaller: `"context": "Picker || menu"`.

TYPE will need this. The question is which mechanism, and the answer is **both, on different
axes**:

| Mechanism | Answers | Example |
|---|---|---|
| **Layer** | which panel is focused | a binding in the tree's keymap only applies when the tree has focus |
| **Predicate** | what state that panel is in | `Escape` clears a selection only when there is one |

VS Code collapses both into one language, which is why its clauses are long — the first term is
almost always focus and the rest is state. **Layering makes that first term free.** A binding
written in the editor's keymap does not need `when: editorFocus`; it says so by living there.
That deletes the most-repeated clause in the system and leaves the predicate language much
smaller than VS Code's.

**The rule, and it is mechanical:**

> If the condition is *which panel*, it is a layer. If it is *what state*, it is a predicate.
> Focus is never expressed in a predicate.

Without that rule there are two ways to say the same thing, which is worse than either alone.

### Sequenced, not simultaneous

**Layers now.** Forced by M5: the terminal panel is one whose keymap is nearly empty, so almost
everything falls through to the PTY. That reframes the hardest case as the simplest one instead
of as a special case in the dispatcher.

Resolution order becomes focused panel's keymap → global keymap. Panel bindings shadow global
ones at the same key. Pending resolution range-scans both layers and merges, so the hint shows
what is actually reachable from where you are.

This is also less new machinery than it sounds: `app.rs:386` already tries
`focused_mut().apply_action(action)` first and falls through when it returns `None`. Layers turn
an implicit convention into an explicit, inspectable lookup order — and make it visible to the
palette and the hint, which the convention never was.

**Predicates later, slot left open.** No forcing case exists yet. The first is likely M3 —
`Escape` dismissing a diagnostic popup versus clearing a selection. The boundary is decided now
so that adding them is an addition rather than a retrofit.

## What this buys elsewhere

- `bindings_for`, which existed for help text, **became the command palette's data source at
  v0.2.9** rather than a second list to maintain — the palette shows what key runs each command
  and nothing had to be written down twice. It is the one prediction in this document that has
  been paid out.

  What has not: §2 says that description is "the same string the command palette shows — one
  source, two surfaces". `Action` has no description, so the palette lists bare `name()`s like
  `select_next_occurrence`. The hint that was to share it does not exist, so there is no second
  surface to keep honest yet.
- The vim layer gets pending-state support for nothing: `d3w` is the same shape of problem as
  `ctrl+k e`.
- Every binding becomes introspectable, which is what "every action reachable three ways"
  requires in order to be checkable rather than asserted.

## Open questions

**What TYPE looks like.** Untouched, and the largest gap in the design. There is no density
target, no layout intent, no typographic idea beyond §4's "no chrome without a job". The hint
box above assumes there is room for it; that assumption is unverified because nothing describes
the screen it appears on.

**The focus model.** Deeper than bindings. How many things can hold focus at once; what happens
to the editor's cursor while the tree has focus; whether there is a "back to where I was".
Today's answer is `F6` cycling, which is not an answer.

**Second prefix.** Deferred to M3, see above.

## Sources

Read from source on 2026-08-22, not from documentation about them:

- `termide/doc/en/keybindings.md` — the universal/enhanced tiering and the startup warning
- `ttt/internal/config/keybindings.go` — `DefaultKeybindings`, the `ctrl+k` / `ctrl+l` prefixes
- `helix-editor/helix` — `helix-view/src/info.rs`, `helix-term/src/ui/editor.rs` infobox trigger
- `folke/which-key.nvim` — the 200 ms delay and Hydra mode
- `zed-industries/zed` — `assets/keymaps/default-linux.json` context predicates
- `microsoft/vscode` — `src/vs/editor/common/editorContextKeys.ts`, 56 context keys

Prior-art rule applies throughout: each choice above is taken because a failure was measured or
observed, not because the field does it. The two places TYPE deliberately diverges are the
immediate hint (Neovim delays; the reason for the delay does not exist here) and layered
keymaps (the field uses predicates alone; layering removes the clause the field repeats most).
