# Changelog

Versions map onto milestones: `0.<milestone>.<patch milestone>`. See
[`docs/design/architecture.md`](docs/design/architecture.md) §9.

## [0.2.7] - 2026-08-25 (M2.7, parse)

The editor understands the file it is showing. A tree-sitter grammar parses the buffer on a
worker thread and the result is painted through the theme, so code is parsed rather than
pattern-matched.

Scoped larger than it shipped. Terminal capability work — the kitty keyboard protocol, following
the terminal's own light and dark — was cut: it needs an escape parser that reads replies from
the terminal, which is a backend question still open, and it buys polish rather than capability.

### Added
- Syntax highlighting for Rust, TOML, JSON, YAML and Markdown, by tree-sitter.
- Grammars are compiled into the binary. No runtime directory to locate, no C compiler to
  install, nothing to fetch — the two failure modes the field's other approaches carry.
- Parsing runs on a worker thread and coalesces: a burst of typing costs one parse per parse,
  not one per keystroke, with no debounce timer and so no latency floor on the common case of
  typing one character and stopping.
- Injections. A fenced code block in a Markdown file is highlighted as the language its fence
  names, YAML and TOML frontmatter as themselves, and a paragraph's emphasis and inline code
  through Markdown's second grammar.
- A `[syntax]` table in all six shipped themes, thirteen capture names each. Names resolve
  through their dotted prefixes, so `keyword.control` finds `keyword` and a theme colours a
  grammar it was never written for.
- `WT_SESSION` is read as a truecolor claim. Windows Terminal does not set `COLORTERM`, so a
  stock install had been quantising every theme for nothing.
- Cold start and time-to-first-highlight are measured, alongside the frame and keystroke budgets
  that already were.

### Fixed
- A parse belonging to a buffer that has since been closed is no longer applied to the one that
  replaced it. Opening a second file before the first finished parsing painted it in the
  previous file's colours until the next parse landed.
- Markdown paragraphs reach the inline grammar. The injection query shipped by the grammar crate
  is written for another editor's defaults, and under this highlighter it produced an empty
  range: the layer was created and parsed nothing, silently.

### Performance
- A highlighted frame deep in a 50k-line file: 864 µs, against a 16 ms budget and 511 µs for the
  same frame unhighlighted.
- Cold start over a real repository: 7.2 ms, against 100 ms.
- Loading six grammars and compiling their queries: 102 ms, once per process, on the worker —
  so it costs latency to the first highlight and nothing at startup.
- The binary grows from 1.19 MB to 4.87 MB, which is what five compiled-in languages cost.

## [0.2.6] - 2026-08-24 (M2.6, ship)

The milestone that makes the previous five reachable. v0.2.5 published one Linux archive and it
did not start on most Linux; getting the editor at all meant having a Rust toolchain. Linux is
static musl now on two architectures, there is a one-line installer for every platform, and a
release cannot publish itself until it has downloaded its own artifacts and run them.

Nothing in the editor changed, with one exception the measurement forced: the allocator on musl.

### Added
- **Static Linux builds, x86_64 and aarch64.** `x86_64-unknown-linux-musl` and
  `aarch64-unknown-linux-musl`, statically linked, with no glibc version to be too new for. One
  file covers Ubuntu, Debian, RHEL, Alpine, Void and NixOS regardless of age, and aarch64 covers
  Graviton, Raspberry Pi, Asahi and arm64 servers — a platform TYPE did not build for at all.
- **`install.sh` and `install.ps1`.** One line each, POSIX `sh` and Windows PowerShell 5.1. Both
  verify the published SHA-256 before anything is written outside a temporary directory, and
  neither asks for `sudo` or Administrator. Tested under dash, busybox ash, PowerShell 5.1 and 7.
- **Releases verify themselves before they publish.** Every archive is downloaded back off the
  release, checksummed, unpacked, executed, and asserted to report the version its tag claims.
  A release that fails stays a draft.
- **Build provenance** on every archive, so `gh attestation verify <file> --repo Pranjal-SB/type`
  proves it came out of this workflow.
- **`THIRD-PARTY-LICENSES.md` ships inside every archive** — the notices MIT and Apache-2.0
  require to travel with a binary containing their code. 106 crates, generated at package time.
- **Weekly perf runs.** The budgets in `tests/perf.rs` were enforced only when someone
  remembered to look, which had been true since v0.2.1.
- **`cargo binstall typ-editor`** fetches the release archive instead of compiling it. It was
  broken by default — binstall interpolates the crate name, `typ-editor`, and the archives are
  named for the binary — and on Linux it is pointed at the static build rather than the glibc
  one it would otherwise prefer.
- Dependabot on cargo and GitHub Actions, `SECURITY.md`, typo checking, zizmor on the workflows,
  and PSScriptAnalyzer on the PowerShell.

### Fixed
- **The published Linux binary did not start on most Linux.** v0.2.5 shipped one Linux archive,
  linked against glibc 2.39, which fails with `version 'GLIBC_2.39' not found` on Ubuntu 22.04,
  Debian 12, RHEL 9 and Amazon Linux 2023. The static musl build is now the one the installer
  and the README point at.

### Removed
- **v0.2.5's Linux archive was deleted from its release page**, with its `.sha256`, and the
  notes say why. It could not start on most Linux, and a missing asset is a better outcome than
  one that fails with a loader error. Immutable releases do not apply retroactively, so it could
  still be removed; releases published from here on cannot be corrected this way.

### Changed
- **mimalloc is the allocator on 64-bit musl.** musl's own `mallocng` cost `find_all` 4.11 ms →
  10.17 ms on a 50k-line file, against a 16 ms budget with the least headroom in the project.
  mimalloc returns it to 4.23 ms. Measured, best of five, on one host — and mimalloc rather than
  the jemalloc ripgrep uses because jemalloc's autotools build misdetects C11 atomics when it
  cross-compiles to aarch64-musl on an arm64 runner. The trade is resident memory: mimalloc uses
  more than either alternative, which for an editor is the cheaper side.

## [0.2.5] - 2026-08-23 (M2.5, colour)

The milestone that turns the palette into an artifact. A theme is a file, six of them ship, the
terminal's colour depth is detected and degraded to, and indentation stops being a number the
status bar asserts without measuring.

### Added
- **Themes are TOML files.** A named `[palette]` and a typed `[ui]` table of 27 slots. Six ship
  embedded — Slate, Catppuccin Mocha and Latte, Dracula, Rosé Pine, Tokyo Night Storm — and a
  file in `<config>/themes/<name>.toml` wins over the embedded copy of the same name. Every key
  is optional; an unset one keeps the shipped default. An unknown key is a load error with a
  did-you-mean rather than a silently wrong colour.
- **A contrast rubric, public as `typ_core::audit`.** Every shipped theme is checked at truecolor
  **and again after degradation to 256 colours** — the half nobody else checks. Quantising moves
  every colour by a different amount, and three of the ported palettes turned out to lose their
  second surface entirely at 8-bit while reading fine at 24.
- **Terminal colour-depth detection**, with `color_depth` in `config.toml` as the escape hatch.
  Nothing in the environment separates a tmux that forwards truecolor from one that mangles it,
  so that one is a setting rather than a cleverer guess.
- **Indent width is measured, not assumed.** VS Code's `guessIndentation` ported, including the
  alignment rule that stops `const a = b + c,` reading as a 6-wide indent, and a deterministic
  tie-break. The status bar now reports what was measured.
- **Whitespace rendering** — `none | trailing | selection | all`, defaulting to `selection`.
- **Indent guides**, including through blank lines, spaced by the detected width.

### Changed
- **Contrast floors depend on the ground a theme declares.** WCAG 2.1's ratio is not
  perceptually uniform across polarity: measured over 1,066 colour pairs from 97 published
  palettes, a dark ground returns about 2.5x the ratio of a light one at equal legibility. Under
  a single flat floor the rubric passed Slate's gutter at 3.35 and rejected Catppuccin Latte's at
  2.83 — while Latte's is roughly twice as legible. It was rejecting the better colour, for five
  dark themes against one light one.
- **Slate is retuned.** Its gutter and inactive status text moved most, which is the finding
  rather than a side effect: they had been tuned against a floor that was measuring the wrong
  thing.
- Panels draw as full boxes sharing one edge, and chrome sits on its own surface, so the sidebar
  and the editor stop reading as one space.

### Fixed
- The current line's tint now covers its line number instead of stopping at the gutter.

## [0.2.4] - 2026-08-16 (M2.4, live)

The milestone that makes the editor live and correct: able to be woken by something other than
a keypress, able to notice the file changing underneath it, and able to save without quietly
destroying what it did not write.

### Added
- **The event loop blocks on one channel** rather than on the terminal. A worker thread can
  now deliver a result without waiting for the user to press a key, which is the prerequisite
  for tree-sitter at v0.2.5 and LSP at v0.3.0. A detached thread pumps crossterm into the same
  channel.
- **File watching.** A file changed on disk while open was a data-loss bug: TYPE neither
  reloaded nor warned, and the next save overwrote the other writer. Now a clean buffer
  reloads silently, a dirty one warns and is left alone, and a deleted file leaves the buffer
  standing as the only copy. The watch is on the parent directory, because a rename-over
  destroys the inode a file watch is pinned to.

### Changed
- **A frame is drawn because state changed, not because the loop went round.** The loop drains
  everything queued behind the event it woke on and draws once. On a 50k-line file an idle
  wakeup costs 425 ns against a 513 µs frame, and a 30-event burst dispatches in 268 µs and
  draws a single frame.
- Scroll coalescing folds only consecutive wheel events over the same panel, in the batch. The
  version it replaces read ahead in the queue and dropped anything that was not a scroll, so a
  key pressed mid-flick vanished.

### Fixed
- **Saving preserves line endings, symlinks and mode bits.** CRLF is normalized to LF in the
  rope and written back on save, so a typed newline no longer puts an LF into a Windows file.
  A symlink is resolved and written through rather than replaced by a regular file. The
  original mode is carried onto the temp file before the rename, so an executable script stays
  executable and a `0600` file is never briefly world-readable.
- **The parent directory is fsynced after the rename.** A rename is not durable until the
  directory entry naming it is. None of ttt, TermIDE or Fresh does this.
- Resize is handled. It was harmless only while the loop repainted unconditionally; once
  redraw became damage-driven it was a frozen screen.

## [0.2.3] - 2026-08-16 (M2.3, polish)

The milestone that makes TYPE *look* like a finished program. M2.2 made it usable and did
nothing for how it reads; this is the answer to "feels very prototypey compared to ttt and
TermIDE". Every item closes a defect from the gap analysis's "Reads as unfinished" class, the
class the first audit had no rows for, because it compared feature lists and never asked
whether the thing looks finished.

### Added
- **A gutter, with line numbers.** Built as an ordered list of components rather than a
  line-number column: `LineNumbers`, `Spacer`, and `Diagnostics`/`Diff` reserving their cell
  and drawing nothing until M3 and M5 fill them in. Right-aligned, 1-based, width taken from
  the whole buffer so the text never shifts sideways at line 100. Relative numbering exists
  behind a field for the vim layer, off by default.
- **A truecolor theme.** `ThemeColors` goes from ten flat fields to twenty-four, modelled on
  Helix's `ui.*` scopes and drawn from one named ramp at one hue. Contrast is checked by test
  against WCAG ratios rather than by eye: body text holds 7:1, the gutter 3:1, every
  diagnostic 4.5:1, and error and warning are separated by lightness so they survive
  deuteranopia. The diagnostic colours ship unused so a theme file written at M2.5 does not
  get a breaking change at M3.
- **Current-line highlight, distinguishable primary selection, matching brackets.** Only
  empty selections tint their line. The bracket search is bounded by the viewport plus a
  margin and gives up rather than exceeding it, because it runs on the render path.
- **`Ctrl+D` selects the next occurrence**, `Ctrl+Shift+L` selects all of them. Case
  sensitive, unlike `Ctrl+F`: matching an identifier is a different job from finding prose.
- **`Ctrl+G` jumps to a line**, centring it rather than merely scrolling it into view.
- **Seven status segments** instead of three: name, filetype, line ending, indent, cursor
  count, position, percentage, each with an emphasis, so unsaved changes and a cursor count
  above one are accented rather than lost in a strip of even text.
- **A log file.** `TYP_LOG` names a path; unset, logging costs a branch. A TUI owns the
  screen, so `println!` debugging is unavailable by construction.
- **Line-ending detection.** The status bar stops claiming every file is LF. Preserving it on
  save remains M2.5's half of the job.
- **The tree colours directories apart from files**, so the shape of a project is readable
  without reading the names.

### Changed
- The tree's selected row uses the primary selection colour: it is the one thing being
  steered, the same job the editor's primary does.
- Perf tests take a mutex and the `find_all` budget takes best-of-five. Adding two render
  benchmarks made `InsertChar` read 32 µs against the 1.9 µs it actually costs. Cargo runs
  tests in parallel threads, so the older tests were being timed while a sibling saturated a
  core. It took a bisect against v0.2.2 to establish that the 20x was a phantom.

### Fixed
- The render path is measured for the first time. Architecture §4 budgets keystroke *to
  painted glyph*, and every perf test measured edits only. A frame deep in a 50k-line file
  costs 439 µs against 16 ms.

## [0.2.2] - 2026-08-15 (M2.2, usable)

The milestone that makes TYPE able to edit TYPE. Every item here closes a defect found by
reading the tree rather than by planning. None was named by any earlier plan, which is the
finding underneath the findings.

### Added
- **Clipboard.** Copy, cut and paste, keyboard and mouse. `Ctrl+C`/`X`/`V`, the `Insert` trio,
  right-click a selection to copy, middle-click to paste. Copying emits OSC 52, so a copy over
  SSH reaches the machine you are sitting at rather than the one you are logged into.
  Multi-cursor copy joins selections with newlines and pasting that many lines back gives one
  to each cursor.
- **Tab indents, Shift+Tab outdents.** At a caret Tab goes to the next tab stop; with a
  selection it shifts every line the selection touches and leaves the selection standing.
  Blank lines are skipped, and outdent takes a partial level to zero rather than to minus one.
- **`typ newfile.md`** opens an empty buffer that `save` creates. A missing parent directory is
  still an error, and still fails before the screen is taken.
- **Bracketed paste.** A paste is one edit and one undo step rather than one per character, and
  a chord inside pasted text is inserted rather than executed.
- Gap analysis: a defect audit of the tree and a measurement of TYPE against VS Code, Zed,
  Sublime, Helix, TermIDE and oh-my-pi, plus the install and first-launch design.

### Changed
- **Focus cycling moves from `Tab` to `F6`**, with `Ctrl+Tab` bound alongside. Tab had to be
  freed for indent. `F6` rather than `Ctrl+Tab` as the primary because without the kitty
  keyboard protocol a terminal cannot tell `Ctrl+Tab` from `Tab`. Tab now does nothing in the
  file tree until M4 names the tree's own actions; `F6` works from both panels.

### Fixed
- **Opening a file no longer discards unsaved changes.** `needs_close_confirmation` had one
  caller, so `Ctrl+Q` guarded your work and clicking a file in the tree threw it away.
- **The undo stack is capped at 1000 steps.** It had no cap and no eviction, so every version
  of the file was retained for as long as the editor was open.
- Architecture §10 no longer lists the config format as an open question. It is TOML, decided
  when `keys.toml` shipped.

## [0.2.1] - 2026-08-15 (M2.1, correctness)

### Fixed
- `InsertChar` cost 33.8 ms on a 50k-line file against a 16 ms budget. `TextBuffer::line_text`
  returned an owned `String` and three callers looped it over the whole buffer; a borrowing
  accessor replaced it. Undo and redo, and whole-buffer search, were the other two.
- Undo took back one character at a time. Consecutive edits of the same kind now coalesce, and
  anything that is not an edit ends the run — structural, not on a timer.
- Undo restores the selections the edit was made from rather than wherever clamping left them.

### Added
- Performance tests behind the budgets in architecture §4, run by hand with `--release
  --ignored`. A budget stated in prose with nothing measuring it is how the above shipped.

## [0.2.0] - 2026-08-15 (M2, editing is real)

### Added
- Selections as the only cursor model — a caret is an empty selection, and every editing path
  works for one cursor or thirty without branching.
- Multiple cursors: `Ctrl+Alt+↑/↓`, `Alt`+click.
- Word-wise motion and deletion, select-all, select-line, collapse-to-one.
- Mouse selection: drag to select, click twice to select a word.
- Horizontal scrolling that never splits a wide grapheme.
- Literal search and replace-all through a status-bar prompt, smart-case.
- Every editing primitive is a named `Action`; every key binding is a table row.
- `keys.toml` rebinding, from the platform config directory or `TYP_CONFIG_DIR`. A bad file
  warns and keeps the defaults rather than refusing to start.

## [0.1.0] - 2026-08-14 (M1, walking skeleton)

### Added
- Event loop, `Panel` trait, editor panel, file tree panel, status bar.
- `$EDITOR` invariants: `typ <file>` opens that file, blocks until closed, exits with an honest
  code, never detaches.
- Atomic save: write to a sibling temp file, fsync, rename over the target.
- The terminal's real cursor is drawn from the focused panel, so it blinks and reshapes like
  every other terminal program's.

[Unreleased]: https://github.com/Pranjal-SB/type/compare/v0.2.7...HEAD
[0.2.7]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.7
[0.2.6]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.6
[0.2.5]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.5
[0.2.4]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.4
[0.2.3]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.3
[0.2.2]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.2
[0.2.1]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.1

<!-- 0.2.0 and 0.1.0 have no link because they have no tag: tagging began at
     v0.2.1, when the versioning scheme was adopted. -->

