# TYPE

[![CI](https://github.com/Pranjal-SB/type/actions/workflows/ci.yml/badge.svg)](https://github.com/Pranjal-SB/type/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/typ-editor.svg)](https://crates.io/crates/typ-editor)
[![MSRV](https://img.shields.io/badge/rustc-1.96%2B-blue.svg)](rust-toolchain.toml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**T**erminal-**Y**oked **P**rogramming **E**nvironment. A full IDE that runs in your terminal.

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
there. Modal editing is a setting. An opt-in vim layer translates modes, counts and operators into
the same named actions the non-modal path already calls, so it owns no editing primitives of
its own and the core never depends on it.

## Design goals

| Goal | Commitment |
|---|---|
| Fast | Cold start under 100 ms; keystroke to painted glyph under 16 ms at p99 |
| Clean | No chrome without a job; one visual system across every panel |
| Mature | Every action reachable three ways: keybinding, command palette, mouse |
| Complete | LSP, DAP, and tree-sitter clients, so language capability arrives by protocol |

## Status

**v0.2.6, pre-alpha.** Editing works and the editor looks the part: line numbers, current-line
highlight, bracket matching, and multiple cursors with a visibly distinct primary. Search and
replace, clipboard that works over SSH, Tab indent, `Ctrl+D`, goto-line, undo that takes back a
run of typing in one press. It notices when a file changes on disk, reloads it when you have no
unsaved work and says so when you do, and saves without flattening your line endings, your
symlinks or your mode bits.

**Colour is now an artifact rather than a constant.** A theme is a TOML file; six ship and any
number can live in your config directory. The terminal's colour depth is detected at startup and
the palette is brought down to 256 colours when it has to be. Indentation is measured from the
file instead of assumed, whitespace can be shown when you ask for it, and indent guides are
drawn — including through blank lines.

Every shipped theme is checked against a contrast rubric at truecolor **and again after
degradation**, which is the half nobody else checks: quantising moves every colour by a
different amount, and a palette that reads at 24-bit can lose a surface entirely at 8. The
floors depend on the ground a theme declares, because WCAG 2.1's ratio is not perceptually
uniform across polarity — see [`docs/design/themes.md`](docs/design/themes.md).

**Getting it takes one line, and the Linux build now starts on Linux.** v0.2.5 shipped a
single glibc-linked Linux archive that failed on anything older than Ubuntu 24.04; the Linux
builds are static musl now, x86_64 and aarch64. Every release downloads its own archives back,
checks the sums, runs them and asserts the version before it stops being a draft.

No syntax highlighting, no LSP, no tabs or splits yet; see the roadmap. Full history in
[CHANGELOG.md](CHANGELOG.md).

Every editing primitive is a named action and every key binding is a table row, so a command
palette and an opt-in vim layer are configuration rather than a rewrite.

- [Architecture and design rationale](docs/design/architecture.md)
- [Gap analysis: known defects, and how TYPE measures against the field](docs/design/gap-analysis.md)
- [Contributing](CONTRIBUTING.md)

## Install

Linux and macOS:

```sh
curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/Pranjal-SB/type/main/install.sh | sh
```

Windows:

```powershell
irm https://raw.githubusercontent.com/Pranjal-SB/type/main/install.ps1 | iex
```

Both check the published SHA-256 before anything is written outside a temporary directory, and
both install for the current user only — `~/.local/bin`, or `%LOCALAPPDATA%\Programs	yp` — so
neither asks for `sudo` or Administrator. `--bin-dir` / `-BinDir` puts it somewhere else and
`--version` / `-Version` fetches a tag other than the latest — or, since a script piped into
`sh` or `iex` cannot be given arguments, `TYP_BIN_DIR` and `TYP_VERSION` in the environment.

With a Rust toolchain, either of:

```sh
cargo install typ-editor     # compiles it
cargo binstall typ-editor    # fetches the same archive the installers do
```

The crate is `typ-editor` and the binary is `typ`, which is also why the archives are named
`typ-*`: `type` is a POSIX shell builtin and taking that name would shadow it.

Or take an archive from a [release](https://github.com/Pranjal-SB/type/releases) directly:

| Target | |
|---|---|
| `x86_64-unknown-linux-musl` | static; what `install.sh` picks on x86_64 Linux |
| `aarch64-unknown-linux-musl` | static; Graviton, Raspberry Pi, arm64 servers |
| `x86_64-unknown-linux-gnu` | dynamically linked against the build runner's glibc, currently 2.39. Take musl unless you specifically want this one |
| `x86_64-apple-darwin`, `aarch64-apple-darwin` | |
| `x86_64-pc-windows-msvc` | |

Each carries a `.sha256`, the third-party licence notices, and build provenance:

```sh
gh attestation verify typ-v0.2.6-x86_64-unknown-linux-musl.tar.gz --repo Pranjal-SB/type
```

## Build from source

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
| `Ctrl+D` | Select the word under the cursor, then each next occurrence |
| `Ctrl+Shift+L` | Select every occurrence at once |
| `Ctrl+G` | Go to a line |
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
paste and never pass them on. In the legacy key encoding a `Ctrl`+letter chord also carries no
shift bit for the terminal to report. They work on Windows today, and elsewhere once terminal
capability detection lands.

Copying emits OSC 52, so a copy over SSH reaches the clipboard on the machine you are sitting
at rather than the one you are logged into. Multi-cursor copy joins the selections with
newlines, and pasting that many lines back into that many cursors gives one line to each.

Mouse: click to position the cursor, drag to select, click twice in the same place to select
the word, `Alt`+click to stack another cursor, right-click a selection to copy it, middle-click
to paste, click a selected tree entry to open or toggle it, wheel to scroll whichever panel the
pointer is over.

## Appearance

`config.toml`, beside `keys.toml` in your config directory. Every key is optional.

```toml
theme = "slate"          # slate, mocha, latte, dracula, rose-pine, tokyo-night
color_depth = "truecolor" # or "ansi256" — omit to ask the terminal
indent_width = 4          # omit to measure it from the file
whitespace = "selection"  # none | trailing | selection | all
```

Six themes ship. Drop a `<config>/themes/<name>.toml` in to add your own, or copy a shipped one
and edit it — a file of the same name wins over the embedded copy. The format and the contrast
rules every theme is held to are in [`docs/design/themes.md`](docs/design/themes.md), and
`typ_core::audit` is public so you can run the same check against your own palette.

`whitespace` defaults to `selection`: marks appear only inside a selection, which is where they
are diagnostic rather than noise. `trailing` is the one that catches a defect.

**Search**

| Key | Action |
|---|---|
| `Ctrl+F` | Search |
| `F3` `Shift+F3` | Next / previous match, wrapping |
| `Ctrl+H` | Replace every match |

Search is smart-case: a lowercase needle finds everything, one with a capital in it means it.
`Ctrl+D` is not: matching an identifier is a different job from finding prose, so `value` and
`Value` stay two different things.

**The status bar** carries the file name, its type, its line ending, the indent width, the
cursor count when there is more than one, the position and how far through the file you are.
Unsaved changes and a cursor count above one are accented, because both are states you can
forget you are in.

Keybindings are non-modal, and will stay usable that way. Every binding is a row in a table
rather than a branch in a match, so rebinding is configuration. A vim layer (modes, counts,
operators, composable motions) is planned as an opt-in setting.

## Debugging

A TUI owns the screen, so there is nowhere to print. Set `TYP_LOG` to a path and TYPE appends a
line per event — startup, the clipboard provider it detected, config problems, failed saves:

```bash
TYP_LOG=/tmp/typ.log typ .
```

Unset, it costs a branch and writes nothing. The clipboard provider line is the first thing
worth looking at if copy and paste are not doing what you expect.

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
defaults are kept. A typo here never stops the editor opening. One bad line rejects the whole
file rather than half-applying it, because a keymap you cannot tell the state of is worse than
one that plainly did nothing.

## Roadmap

| Version | Milestone | Scope | State |
|---|---|---|---|
| — | M0 | Feel spike: measure input latency, frame timing, unicode correctness | shipped |
| v0.1.0 | M1 | Walking skeleton: event loop, panel contract, editor and file tree | shipped |
| v0.2.0 | M2 | Editing: multi-cursor, selections, word motion, search and replace | shipped |
| v0.2.1 | M2.1 | Correctness: keystroke budgets, undo coalescing, the shift map | shipped |
| v0.2.2 | M2.2 | Usable: clipboard, indent, new files, guarded open | shipped |
| v0.2.3 | M2.3 | Polish: gutter, truecolor theme, current line, brackets, status segments, `Ctrl+D`, goto-line, logging | shipped |
| v0.2.4 | M2.4 | Live: wakeable event loop, file watching, damage-driven redraw, resize, save correctness | shipped |
| v0.2.5 | M2.5 | Colour: themes as files, contrast rubric, capability detection, indent detection, whitespace and indent guides | shipped |
| v0.2.6 | M2.6 | Ship: static musl and aarch64 Linux builds, one-line installers, self-verifying releases | **current** |
| v0.2.7 | M2.7 | Parse: tree-sitter highlighting, grammar distribution, `config.toml`, terminal light/dark, kitty keyboard protocol | next |
| v0.3.0 | M3 | Code intelligence: LSP client | |
| v0.4.0 | M4 | Workspace: splits, tabs, sessions, command palette, project search | |
| v0.5.0 | M5 | Terminal panel and git integration | |
| v1.0.0 | M6 | OS-level file association, performance budgets enforced in CI | |

Then: extension host (v1.1), then debugger (v1.2).

**Versioning.** `0.<milestone>.<patch milestone>`: the minor is the milestone number, the
patch is a correctness milestone landing on top of it. One version for the whole workspace;
every crate inherits it and `typ --version` prints it. v1.0.0 is M6, the point at which the
author stops using another editor. Milestones stay the working unit. Plans, tasks and commits
are still organised by M-number, and the version is what that milestone is called once it
lands.

## Non-goals

Collaborative editing, notebooks, HTML-rendering extensions, an extension marketplace, and a
GPU renderer. Remote development is not a goal either. You already have SSH, and that is a
structural advantage of living in a terminal.

## License

MIT
