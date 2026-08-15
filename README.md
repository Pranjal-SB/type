# TYPE

**T**erminal-**Y**oked **P**rogramming **E**nvironment — a full IDE that runs in your terminal.

Not a terminal text editor with some IDE features bolted on. The full surface — code
intelligence, debugging, git, integrated terminal, project search — delivered through a
terminal UI that treats the mouse and the keyboard as equal peers.

```
┌─ project ───┬─ main.rs ──────────────────────────────┐
│ src/        │  1  fn main() {                        │
│  ▸ core/    │  2      let editor = Editor::new();    │
│    lib.rs   │  3      editor.run()                   │
│  ▸ ui/      │  4  }                                  │
│ Cargo.toml  │                                        │
├─────────────┴────────────────────────────────────────┤
│ $ cargo test                                         │
└──────────────────────────────────────────────────────┘
```

## Why

Modern GUI editors are slow to start and heavy to run. Existing terminal editors either lack
the IDE surface entirely, or require deep configuration to approximate it, or are built
keyboard-purist in a way that never made room for a mouse.

TYPE targets the gap: **non-modal by default, full mouse parity, panel-rich, extensible.**
Familiar immediately if you're arriving from a GUI editor, without giving up capability to get
there. Modal editing is a setting rather than a fork — an opt-in vim layer translates modes,
counts and operators into the same named actions the non-modal path already calls, so it owns
no editing primitives of its own and the core never depends on it.

## Design goals

| Goal | Commitment |
|---|---|
| Fast | Cold start under 100 ms; keystroke to painted glyph under 16 ms at p99 |
| Clean | No chrome without a job; one visual system across every panel |
| Mature | Every action reachable three ways — keybinding, command palette, mouse |
| Complete | LSP, DAP, and tree-sitter clients, so language capability arrives by protocol |

## Status

**v0.2.2 — pre-alpha.** Selections, multiple cursors, word-wise motion, drag to select,
horizontal scrolling, literal search and replace, clipboard with OSC 52 so it works over SSH,
Tab indent, and creating new files — alongside the file tree, focus cycling, undo/redo, save
and rebindable keys. Every editing primitive is a named action and every key is a table row, so
a command palette and an opt-in vim layer are configuration rather than a rewrite.

This is the release at which TYPE should be able to edit its own source: the gaps that made a
real editing session impossible — no clipboard, no indent, opening a file discarding unsaved
work — are closed. Whether it is actually pleasant to use it all day is the next thing to find
out, and the answer is expected to produce a defect list rather than a victory lap.

No syntax highlighting, no LSP, no splits or tabs yet — see the roadmap.

A status bar carries messages, the open file, and the cursor position. Quitting with unsaved
changes asks before discarding them.

Undo takes back a run of typing in one press rather than one character at a time, and puts
the cursor back where the edit was made.

- [Architecture and design rationale](docs/design/architecture.md)
- [Gap analysis — known defects and how TYPE measures against the field](docs/design/gap-analysis.md)
- [Current implementation plan](docs/plans/m2.2-usable.md)

## Build

```bash
cargo build --release
./target/release/typ .
```

## Keys

| Key | Action |
|---|---|
| `F6` | Cycle focus between tree and editor (`Ctrl+Tab` too, where the terminal reports it) |
| `Ctrl+S` | Save |
| `Ctrl+Q` | Quit |

**Tree**

| Key | Action |
|---|---|
| `↑` `↓` | Move the selection |
| `Enter` | Open a file, or expand/collapse a directory |
| `→` `←` | Expand / collapse a directory |

**Editor**

| Key | Action |
|---|---|
| Arrows | Move the cursor |
| `Shift`+arrows | Extend the selection |
| `Ctrl+←` `Ctrl+→` | Move by word |
| `Home` `End` | Start / end of line |
| `Ctrl+Home` `Ctrl+End` | Start / end of document |
| `PageUp` `PageDown` | Move by a screen |
| `Ctrl+A` | Select all |
| `Ctrl+L` | Select the line |
| `Esc` | Collapse to a single cursor |
| `Ctrl+Alt+↑` `Ctrl+Alt+↓` | Add a cursor above / below |
| `Enter` | Split the line |
| `Backspace` `Delete` | Delete before / under the cursor |
| `Ctrl+Backspace` `Ctrl+Delete` | Delete a word |
| `Tab` `Shift+Tab` | Indent / outdent |
| `Ctrl+Z` `Ctrl+Y` | Undo / redo |

Every one of those works at every cursor at once, and holding `Shift` with any motion
extends instead of moving.

`Tab` with nothing selected moves to the next tab stop; with a selection it shifts every line
the selection touches, and the selection survives so you can press it again.

**Clipboard**

| Key | Also | Mouse | Action |
|---|---|---|---|
| `Ctrl+C` | `Ctrl+Insert` | right-click a selection | Copy |
| `Ctrl+X` | `Shift+Delete` | — | Cut |
| `Ctrl+V` | `Shift+Insert` | middle-click | Paste |

`Ctrl+Shift+C`/`X`/`V` are bound too, but most terminals claim them for their own copy and
paste and never pass them on — and in the legacy key encoding a `Ctrl`+letter chord carries no
shift bit for the terminal to report. They work on Windows today, and elsewhere once terminal
capability detection lands.

Copying emits OSC 52, so a copy over SSH reaches the clipboard on the machine you are sitting
at rather than the one you are logged into. Multi-cursor copy joins the selections with
newlines, and pasting that many lines back into that many cursors gives one line to each.

Mouse: click to position the cursor, drag to select, click twice in the same place to select
the word, `Alt`+click to stack another cursor, right-click a selection to copy it, middle-click
to paste, click a selected tree entry to open or toggle it, wheel to scroll whichever panel the
pointer is over.

**Search**

| Key | Action |
|---|---|
| `Ctrl+F` | Search |
| `F3` `Shift+F3` | Next / previous match, wrapping |
| `Ctrl+H` | Replace every match |

Search is smart-case: a lowercase needle finds everything, one with a capital in it means it.

Keybindings are non-modal, and will stay usable that way. Every binding is a row in a table
rather than a branch in a match, so rebinding is configuration. A vim layer — modes, counts,
operators, composable motions — is planned as an opt-in setting, not as the default and not
as a fork of the editor core.

## Configuring keys

Bindings live in `keys.toml`, in `$XDG_CONFIG_HOME/typ/` (or `%APPDATA%\typ\` on Windows).
Set `TYP_CONFIG_DIR` to point somewhere else. Anything you set overrides the default of the
same name; everything else keeps its default.

```toml
# chord = action
"ctrl+e" = "move_line_end"
"ctrl+shift+k" = "delete_word_forward"

# an empty action unbinds a key
"ctrl+l" = ""
```

A binding whose action name is unknown is reported in the status bar at startup and the
defaults are kept — a typo here never stops the editor opening. One bad line rejects the
whole file rather than half-applying it, because a keymap you cannot tell the state of is
worse than one that plainly did nothing.

## Roadmap

| Version | Milestone | Scope | State |
|---|---|---|---|
| — | M0 | Feel spike — measure input latency, frame timing, unicode correctness | shipped |
| v0.1.0 | M1 | Walking skeleton — event loop, panel contract, editor and file tree | shipped |
| v0.2.0 | M2 | Editing — multi-cursor, selections, word motion, search and replace | shipped |
| v0.2.1 | M2.1 | Correctness — keystroke budgets, undo coalescing, the shift map | shipped |
| v0.2.2 | M2.2 | Usable — clipboard, indent, new files, guarded open. Self-hosting starts here | **current** |
| v0.2.5 | M2.5 | Damage-driven redraw, wakeable event loop, tree-sitter highlighting, themes | next |
| v0.3.0 | M3 | Code intelligence — LSP client | |
| v0.4.0 | M4 | Workspace — splits, tabs, sessions, command palette, project search | |
| v0.5.0 | M5 | Terminal panel and git integration | |
| v1.0.0 | M6 | OS-level file association, performance budgets enforced in CI | |

Then: extension host (v1.1), then debugger (v1.2).

**Versioning.** `0.<milestone>.<patch milestone>` — the minor is the milestone number, the
patch is a correctness milestone landing on top of it. One version for the whole workspace;
every crate inherits it and `typ --version` prints it. v1.0.0 is M6, the point at which the
author stops using another editor. Milestones stay the working unit — plans, tasks and
commits are still organised by M-number; the version is what that milestone is called once it
lands.

## Non-goals

Collaborative editing, notebooks, HTML-rendering extensions, an extension marketplace, and a
GPU renderer. Remote development is not a goal either — you already have SSH, and that is a
structural advantage of living in a terminal rather than a gap to close.

## License

MIT
