# Changelog

Versions map onto milestones: `0.<milestone>.<patch milestone>`. See
[`docs/design/architecture.md`](docs/design/architecture.md) §9.

## [Unreleased]

## [0.2.2] — 2026-08-15 — M2.2, usable

The milestone that makes TYPE able to edit TYPE. Every item here closes a defect found by
reading the tree rather than by planning — none was named by any earlier plan, which is the
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

## [0.2.1] — 2026-08-15 — M2.1, correctness

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

## [0.2.0] — 2026-08-15 — M2, editing is real

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

## [0.1.0] — 2026-08-14 — M1, walking skeleton

### Added
- Event loop, `Panel` trait, editor panel, file tree panel, status bar.
- `$EDITOR` invariants: `typ <file>` opens that file, blocks until closed, exits with an honest
  code, never detaches.
- Atomic save — write to a sibling temp file, fsync, rename over the target.
- The terminal's real cursor is drawn from the focused panel, so it blinks and reshapes like
  every other terminal program's.

[Unreleased]: https://github.com/Pranjal-SB/type/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.2
[0.2.1]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.1
