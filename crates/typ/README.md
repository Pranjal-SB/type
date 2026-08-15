# typ-editor

**TYPE** — Terminal-Yoked Programming Environment. A full IDE that runs in your terminal:
non-modal by default, mouse and keyboard as equal peers, panel-rich.

Modal editing is a setting, not a fork. The core stays non-modal and always usable without it;
an opt-in vim layer sits above the editor and translates modes, counts and operators into the
same named actions the non-modal path already calls. That is why it can be optional — the layer
owns no editing primitives of its own.

Installs the binary **`typ`** (never `type` — that collides with the POSIX shell builtin).

```bash
cargo install typ-editor
typ .
```

## Status: pre-alpha, v0.2.1

Genuinely working: multiple cursors, selections, word-wise motion, drag to select, horizontal
scrolling, literal search and replace, undo that takes back a run of typing in one press, a
file tree, rebindable keys via `keys.toml`, and atomic saves.

Not there yet: syntax highlighting, clipboard, LSP, splits and tabs, integrated terminal, git.
See the [roadmap](https://github.com/Pranjal-SB/type#roadmap) — and the
[gap analysis](https://github.com/Pranjal-SB/type/blob/main/docs/design/gap-analysis.md), which
lists the known defects honestly rather than waiting for you to find them.

**Do not depend on the `typ-*` library crates yet.** They are published so the binary can be,
and their APIs will change without ceremony until v1.

## Why

GUI editors are slow to start and heavy to run. Terminal editors either lack the IDE surface,
need deep configuration to approximate it, or were built keyboard-purist in a way that never
made room for a mouse. TYPE targets the gap: non-modal by default, full mouse parity, panel-rich,
plugin-first.

The bet is that "all the frontier IDE features" is mostly three protocol clients — LSP, DAP and
tree-sitter — written once and written well.

Full design rationale: [`docs/design/architecture.md`](https://github.com/Pranjal-SB/type/blob/main/docs/design/architecture.md)

## License

MIT
