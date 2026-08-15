# Changelog

Versions map onto milestones: `0.<milestone>.<patch milestone>`. See
[`docs/design/architecture.md`](docs/design/architecture.md) §9.

## [Unreleased]

### Added
- Gap analysis: a defect audit of the tree and a measurement of TYPE against VS Code, Zed,
  Sublime, Helix, TermIDE and oh-my-pi, plus the install and first-launch design.

### Fixed
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

[Unreleased]: https://github.com/Pranjal-SB/type/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/Pranjal-SB/type/releases/tag/v0.2.1
