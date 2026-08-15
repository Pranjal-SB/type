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

TYPE targets the gap: **non-modal, full mouse parity, panel-rich, extensible.** Familiar
immediately if you're arriving from a GUI editor, without giving up capability to get there.

## Design goals

| Goal | Commitment |
|---|---|
| Fast | Cold start under 100 ms; keystroke to painted glyph under 16 ms at p99 |
| Clean | No chrome without a job; one visual system across every panel |
| Mature | Every action reachable three ways — keybinding, command palette, mouse |
| Complete | LSP, DAP, and tree-sitter clients, so language capability arrives by protocol |

## Status

**Pre-alpha.** Walking skeleton runs: file tree with expandable directories, editor panel,
focus cycling with visible focus, mouse and keyboard as peers, scroll coalescing, undo/redo
and save. No syntax highlighting, no LSP, no selections, no splits or tabs yet — see the
roadmap.

A status bar carries messages, the open file, and the cursor position. Quitting with unsaved
changes asks before discarding them.

Undo takes back a run of typing in one press rather than one character at a time, and puts
the cursor back where the edit was made.

- [Architecture and design rationale](docs/design/architecture.md)
- [Current implementation plan](docs/plans/m2-editing.md)

## Build

```bash
cargo build --release
./target/release/typ .
```

## Keys

| Key | Action |
|---|---|
| `Tab` | Cycle focus between tree and editor |
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
| `Home` `End` | Start / end of line |
| `PageUp` `PageDown` | Move by a screen |
| `Enter` | Split the line |
| `Backspace` `Delete` | Delete before / under the cursor |
| `Ctrl+Z` `Ctrl+Y` | Undo / redo |

Mouse: click to select or position the cursor, click a selected tree entry to open or toggle
it, wheel to scroll whichever panel the pointer is over.

Keybindings are non-modal, and will stay usable that way. A vim layer — modes, counts,
operators, composable motions — is planned as an opt-in setting, not as the default and not
as a fork of the editor core.

## Roadmap

| Milestone | Scope |
|---|---|
| M0 | Feel spike — measure input latency, frame timing, unicode correctness |
| M1 | Walking skeleton — event loop, panel contract, editor and file tree |
| M2 | Editing — multi-cursor, selections, search, syntax highlighting |
| M3 | Code intelligence — LSP client |
| M4 | Workspace — splits, tabs, sessions, command palette, project search |
| M5 | Terminal panel and git integration |
| M6 | OS-level file association, performance budgets enforced in CI |

Then: extension host, then debugger.

## Non-goals

Collaborative editing, notebooks, HTML-rendering extensions, an extension marketplace, and a
GPU renderer. Remote development is not a goal either — you already have SSH, and that is a
structural advantage of living in a terminal rather than a gap to close.

## License

MIT
