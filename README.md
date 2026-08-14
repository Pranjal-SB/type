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

**Pre-alpha.** Walking skeleton runs: file tree and editor panels, focus cycling, keyboard and
mouse input as peers, scroll coalescing, save and undo. No syntax highlighting, no LSP, no
splits or tabs yet — see the roadmap.

- [Architecture and design rationale](docs/design/architecture.md)
- [Current implementation plan](docs/plans/m0-m1-foundation.md)

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
