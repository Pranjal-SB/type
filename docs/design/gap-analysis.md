---
type: design
status: living
area: audit
verified: 2026-08-28
verified-against: v0.3.0-dev (M3 tasks 8, 9)
---

# Gap analysis — TYPE against itself and against the field

**Status:** living document · **Written at:** v0.2.1 · **Re-verified at:** v0.2.5 · **Date:** 2026-08-23

Two questions, answered together because they turn out to be the same question:

1. What is wrong or missing in TYPE as it stands?
2. What do mature editors have that TYPE has not planned for?

Everything here was found by reading the tree at `1691dcf` or by measuring the field, not by
re-reading the plans. That distinction is the point — see [Why the plans could not catch
these](#why-the-plans-could-not-catch-these).

---

## Part 1 — Defects in v0.2.1

Severity is about consequence to a user, not about effort to fix. **A struck-through number
means a later release fixed it** — the version is named in the row — and the row stays so the
record of what was wrong survives the fix. A row that is only *partly* fixed keeps its number
and says which half remains, because striking it would lose the rest.

### Data loss and correctness

| # | Sev | Defect | Where | Lands |
|---|---|---|---|---|
| ~~1~~ | **CRITICAL** | **Opening a file discards unsaved changes with no prompt.** `open_path` replaces the editor unconditionally. `needs_close_confirmation` has exactly one caller — `request_quit` — so Ctrl+Q guards your work and Enter on a tree entry throws it away. | `typ-app/src/app.rs:148`, caller at `:109` | v0.2.2 |
| ~~2~~ | HIGH | **Undo stack is unbounded.** `History.undo: Vec<Snapshot>` has no cap and no eviction. Ropey's structural sharing makes each step cheap, not free — every snapshot pins the nodes it replaced. A long session on a large file grows without limit. vim caps at 1000 steps; VS Code caps by total bytes. | `typ-buffer/src/undo.rs:55` | v0.2.2 |
| ~~3~~ | HIGH | **`typ newfile.md` refuses to start** — `bail!("does not exist")`. There is no way to create a file. Every editor in the field opens an empty buffer at that path and creates it on save. | `typ/src/main.rs:59` | v0.2.2 |
| ~~4~~ | LOW | Save temp file uses a fixed name, `.{name}.typ-tmp`. Two instances saving the same file race each other, and a kill mid-save leaves the file behind. Wants a pid or nonce. | `typ-buffer/src/buffer.rs:320` | v0.2.2 |
| 5 | **MED** | **`typ a.rs b.rs` silently ignores everything after the first path.** `args.first()`, and everything after it is dropped without a word. This entry said "honest until tabs exist, a real bug the moment they do" and predicted its own promotion: **tabs landed at v0.2.9 and this did not**, so the severity moves from LOW and the milestone moves from v0.4.0 to unowned-and-next. `open_path` already appends a tab and dedupes by canonical path, so the fix is the argument loop, not the opening. Opening several should leave the *first* active, which is what vim and VS Code both do. | `typ/src/main.rs:131` | next |
| 6 | LOW | No tty check. `typ | cat` renders escape sequences into a pipe. | `typ/src/main.rs` | v1.0.0 (M6) |
| 40 | MED | **An atomic save gives the file the saving user's ownership.** `rename` puts a new inode at the path, and only root or the owner can `chown` it back, so editing a file you have write access to but do not own — a root-owned config, a shared file in a group-writable directory — silently transfers it to you. Found by reading Fresh, the only project in the field that handles it: `should_use_inplace_write` writes in place when `!fs.is_owner(dest_path)`, and because an in-place write is not crash-safe it carries a recovery temp file plus recovery metadata, with a `SudoSaveRequired { temp_path, dest_path, uid, gid, mode }` escalation path behind it. Unix-only, and the recovery machinery is larger than the rest of M2.4 put together, which is why v0.2.4 preserves mode bits and symlinks and leaves this. | `typ-buffer/src/buffer.rs` `save` | unowned |

| 41 | MED | **An OSC reply on stdin is typed into the buffer.** crossterm 0.29's `parse_event` has branches for `ESC [` and `ESC ESC` and a catch-all that re-parses the remainder as Alt+key. There is no `ESC ]` branch, so `ESC ] 11 ; rgb:2e2e/3434/3636 BEL` becomes `Alt+]` followed by every remaining byte as an ordinary character — into the file. Same for DCS, APC, PM and SOS strings. Found by reading Fresh, whose test for it is named `osc_replies_are_swallowed_not_emitted_as_text` and whose comment calls the alternative "the pre-fix behaviour", so they shipped it and fixed it. Latent in TYPE because nothing queries OSC yet; **live the moment terminal light/dark detection lands**, and reachable before that from any terminal that volunteers a reply. **Not fixable in the dispatcher**: TYPE consumes cooked `Event`s, so the sequence is already `Alt+]` plus `Char`s before it arrives. The fix is reading raw bytes and parsing escapes in TYPE, which is the same change M2.6 makes for the kitty protocol. | crossterm `event/sys/unix/parse.rs:77`, TYPE `typ-app/src/run.rs:83` | M2.7, with the input layer |
| ~~42~~ | MED | **Fixed at v0.2.5.** ~~**The contrast rubric mis-ranks its own themes.**~~ `audit` computes WCAG 2.1 ratios, which overrate dark-on-dark and underrate dark-on-light. Measured across the six shipped themes: Catppuccin Latte's line numbers fail at 2.83 while every dark theme passes at 3.0–3.4 — and in APCA terms Latte's are Lc 50.8 against the dark themes' Lc 23–26, so the rubric rejects the colour that is twice as legible. Slate's error (5.77 WCAG / Lc 42.4) passes where Latte's warning (2.31 / Lc 42.3) fails, at identical perceptual contrast. Zed ships APCA for this reason, user-facing, at a default of Lc 45. The consequence is not cosmetic: it drove finding 2 of the M2.5 plan, which concluded light palettes cannot reach the floor when the floor was the thing that was wrong. | `typ-core/src/audit.rs` | v0.2.5 |
| ~~43~~ | MED | **Fixed at M2.7.** ~~**`COLORTERM` is the only truecolor signal read.**~~ `depth_from` checks `COLORTERM` and a `-direct` terminfo entry. Windows Terminal sets `WT_SESSION` and has historically not set `COLORTERM`, so a stock Windows Terminal falls to the 256-colour path and every theme is quantised for no reason. oh-my-pi treats `WT_SESSION` as an unconditional truecolor claim. One line, no new dependency, and the pure-function shape already in place takes it as a third argument. | `typ-app/src/capability.rs` `depth_from` | M2.7 Task 7.5 |
| ~~50~~ | LOW | **Fixed at M2.7.** ~~**Markdown's inline grammar produces no highlights when injected.**~~ The cause was not the grammar and not the language name — it was the injection *range*. `tree-sitter-md` ships nvim-treesitter's injection query verbatim, and that query omits `injection.include-unnamed-children`. tree-house defaults to `IncludedChildren::None`, which subtracts every child from an injection's range; `(inline)` is an alias over hidden `_line` rules whose unnamed children cover the whole node, so the range came out empty and the layer parsed nothing while appearing to be entered. Fences and frontmatter were unaffected because `code_fence_content` and `minus_metadata` have no children covering their text — which is what made it look markdown-inline-specific rather than range-specific. Helix carries the directive on every markdown injection; TYPE now appends the corrected pattern to the crate's query, plus one for `pipe_table_cell`, which the crate's query never covered at all. | `typ-syntax/src/language.rs` `Language::config` | M2.7 |
| 49 | ~~LOW~~ | ~~**`app.rs` has grown a second responsibility, and it is search and replace.**~~ **Fixed at M2.8 Task 0.** 948 lines by then. `handle_prompt_chord`, `run_search`, `jump_to_match`, `run_replace_all` and `parse_line_number` moved to `typ-app/src/app/search.rs` — a child module of `app` rather than a sibling, so the four methods reach `App`'s private fields without any of them widening to `pub(crate)`. 793 lines afterward, and no test changed, which is the proof the move was a move. |
| 51 | MED | **A tab switch rebuilds an OS file watch on the render thread.** `settle_active_tab` calls `rewatch`, which drops the old `FileWatch` and creates a new one; measured at 640 µs of the 16 ms keystroke budget, and a probe put `watch_file` plus its drop at 909 µs on its own, so the switch cost is entirely this. Invariant 7 says I/O goes off-thread and this is I/O on the render thread. Under budget, and the honest fix is not a faster watch — it is watching the workspace once instead of the active file N times, which is the milestone below. | `typ-app/src/app.rs` `rewatch`, budget in `typ-app/tests/perf.rs` | M4, with workspace watching |
| 52 | MED | **Five Enhanced-tier bindings ship with no startup warning.** `controls.md` §1 puts `Ctrl+Shift+letter` behind the kitty keyboard protocol and requires that "the startup path warns when the terminal cannot deliver a configured Enhanced binding". Nothing warns. `ctrl+shift+c`/`x`/`v` are the de-facto terminal clipboard chords and `ctrl+shift+p` and `ctrl+shift+l` match VS Code, so all five are defensible as documented exceptions — but `ctrl+shift+f` is neither a de-facto standard nor documented as one, and on a terminal without the protocol project search has no binding at all. The palette's `>` prefix is the pattern that fixes this class: a second path that needs no chord. | `typ-core/src/keymap.rs`, `docs/design/controls.md` §1 | unowned |
| 53 | LOW | **The two-tier keymap `controls.md` §1 specifies is unbuilt.** Sequence bindings (`ctrl+k e`), `Resolved::Pending`, the generated grouped hint, and `Action` carrying a description and a group. §1 calls a prefix "not a stylistic choice, the only way to reach the rest" of an IDE's command surface, and §2 says the hint and the palette share one description string. v0.2.9 shipped the palette against `name()`, so half the surface exists and the half that teaches the keymap does not. | `typ-core/src/keymap.rs`, `typ-core/src/action.rs` | unowned |
| 54 | MED | **The weekly perf tripwire runs two of the seven perf test files.** `.github/workflows/perf.yml` runs `typ-buffer/tests/perf.rs` and `typ-panel-editor/tests/perf.rs`. It does not run `typ-app/tests/perf.rs`, `typ-app/tests/perf_startup.rs`, `typ-find/tests/perf.rs`, `typ-find/tests/perf_fs.rs` or `typ-panel-editor/tests/perf_startup.rs`. The workflow's own header names "sub-100 ms cold start" as the identity it exists to protect, and `cold_start_stays_under_a_tenth_of_a_second` lives in `perf_startup.rs` — one of the five it skips. The tripwire does not cover the number it was written for, and a file added since is invisible to it by default rather than by decision. | `.github/workflows/perf.yml` | M3, Task 15 |
| 55 | LOW | **`AGENTS.md` says the perf tests are not scheduled, and they are.** "CI does **not** run the perf tests — they are `#[ignore]`d and nothing is scheduled, so a budget regression is caught only when a human remembers to look." `perf.yml` has run on a weekly cron since it was added for defect 18. The sentence is the kind of stale instruction that makes a reader distrust the rest of the file, and it sits in the document that is meant to be the source of truth. | `AGENTS.md`, budgets section | M3, Task 16 |
| 56 | MED | **`architecture.md`'s verified marker is six releases behind the tree.** Frontmatter reads `verified: 2026-08-22`, `verified-against: v0.2.4`, and the dateline says "last verified against the tree 2026-08-22, on the unreleased M2.5 branch". The tree is v0.2.10. `docs/README.md` states the rule this breaks — "Kept true against the tree… Each carries a `verified` date saying when that was last checked" — and this is the document that rule matters most for, since every other doc defers to it as the spec. §5's stack table was corrected at v0.2.10 against the code, but a partial check cannot honestly move a whole-document marker, so the marker stays wrong until someone reads the whole thing. | `docs/design/architecture.md:5-12` | unowned |
| 44 | LOW | **`architecture.md` §5 lists crates that do not exist and will not.** `typ-config` and `typ-ui` are in the 14-crate layout; neither was built, and the reasoning for `typ-config` — that the seam falls elsewhere, parsing sits with its type — lives only in `docs/plans/`, which is gitignored. A reader of the published spec sees a layout the tree contradicts with no explanation. The other absent crates (`typ-syntax`, `typ-lsp`, `typ-git`, the two panel crates) are forward-looking and fine; these two are decisions. | `docs/design/architecture.md:234` | unowned |
| ~~57~~ | **HIGH** | **Fixed at M3.** ~~**A form feed makes every line number below it wrong.**~~ ropey's `unicode_lines` feature is on by default and makes `U+000B`, `U+000C`, `U+0085`, `U+2028` and `U+2029` line breaks, per Unicode Annex #14 — right for text layout, wrong for a code editor. rust-analyzer's `lib/line-index` breaks on a line feed and nothing else; so does ripgrep, which `typ-find` has used since M2.8; so does git, which M5 needs. Reproduced: `a\x0Cb\n` was two lines to the buffer and one to everything it talks to, so a project-search hit below a form feed already jumped to the wrong line **before M3 existed** — this is an LSP-shaped bug that was never only about LSP. Fixed by pinning `ropey = { default-features = false, features = ["simd"] }`, the line Helix pins for the same reason; `simd` has to be restored by hand, which ropey's own docs call a footgun. `cr_lines` goes with it, so a bare CR is content — `LineEnding` has only ever modelled LF and CRLF, so the rope now agrees with the type beside it. | `Cargo.toml`, `typ-lsp/src/position.rs` `content_len` | M3 |
| ~~58~~ | MED | **Fixed at M3.** ~~**The client read `publishDiagnostics.version` without declaring `versionSupport`.**~~ TYPE drops a publish describing a version older than one already sent, which is what the spec defines the capability as meaning. Declaring it is not decoration: reading the field without declaring it is the same class of lie as declaring it and ignoring the field. | `typ-lsp/src/client.rs` `initialize_params` | M3 |
| 59 | MED | **Declaring `textDocument.diagnostic` would turn rust-analyzer's fast diagnostics off.** `main_loop.rs` guards `update_diagnostics` — the **native** set, the errors that appear as you type — on `!config.text_document_diagnostic()`, which reads the *client* capability. So a client that declares pull support and does not implement it well loses the fast half it already had by push. TYPE does not declare it, and a test asserts the absence with the reason attached, so the tripwire exists. The row stays open because it is a live constraint on M3.1 rather than a defect: the capability and a working pull path land in the same commit or neither does. | `typ-lsp/src/client.rs`, `typ-lsp/tests/client.rs` | M3.1 |
| 60 | LOW | **`underline_color()` flickers upstream.** ratatui#1346, open since 2024-08-27 with nine comments, reports the screen repainting from the styled line to the bottom of the terminal. That is the exact feature M3 Task 11 paints diagnostics with, and TYPE is about to own a custom `Backend` anyway (Task 10), so the repaint path is reachable from here — but nothing is known about the cause yet and guessing at one before the renderer exists would be inventing a fix for a symptom. Measure it once diagnostics are drawn. | `typ-app/src/backend.rs` (Task 10), upstream ratatui#1346 | M3, Task 11 |

Recorded as deliberate deferrals in `m2.1-correctness.md`. ~~Line endings not preserved (`\n`
written into a CRLF file), `save` drops POSIX mode bits and replaces symlinks, no parent-dir
fsync~~ — **all four closed at v0.2.4**: CRLF is normalized to LF in the rope and written back
on save, the mode is carried onto the temp file before the rename, a symlink is resolved and
written through, and the parent directory is fsynced after the rename, which none of ttt,
TermIDE or Fresh does. Still true: non-UTF-8 files fail to open, and the new #40.

### Table stakes that are simply absent

| # | Sev | Defect | Evidence |
|---|---|---|---|
| ~~7~~ | **CRITICAL** | **No clipboard. At all.** No `Copy`, `Cut` or `Paste` in `Action`, no OS clipboard dependency, zero matches in the tree. `Ctrl+C` currently does nothing at all. | `typ-core/src/action.rs:59-75` |
| ~~8~~ | **HIGH** | **Tab cannot indent.** `("tab", Action::FocusNext)` is the only Tab binding, and no `Indent`/`Outdent` action exists. A code editor where Tab does not indent is not yet a code editor. | `typ-core/src/keymap.rs:230` |
| ~~9~~ | HIGH | **Bracketed paste is not enabled.** A terminal paste arrives as N separate key events: N loop passes, N repaints, and any chord inside the pasted text executes as a command rather than being inserted. `Event::Paste` is unhandled. | `typ-app/src/run.rs:59` |
| ~~10~~ | MED | ~~**`Event::Resize` is unhandled.**~~ **Fixed at v0.2.4**, and the prediction held exactly: it stayed harmless until damage-driven redraw landed in the same milestone, at which point the test went red as a frozen screen. The fix is one match arm marking the frame dirty — ratatui's `draw` autoresizes a fullscreen viewport and TYPE's panels learn their size at render time, so there was no plumbing to add. | `typ-app/src/run.rs` |
| 11 | MED | Drag past the viewport edge does not autoscroll; the selection stops at the last visible row. | `typ-panel-editor/src/lib.rs:388` |
| 12 | MED | **Partly fixed at v0.2.3**: ~~no goto-line~~ shipped. **Still absent: move-line, duplicate-line, comment toggle** — all cheap, all unowned. | — |
| 13 | LOW | `last_click` is never cleared by keyboard motion, so click → arrow away → click the same cell selects a word rather than placing a caret. | `typ-panel-editor/src/lib.rs:358` |
| 14 | LOW | No horizontal wheel scroll, no Shift+wheel. | `typ-app/src/run.rs:77` |

### Reads as unfinished

**A defect class the first audit had no rows for**, added at v0.2.2 after the author used TYPE
and reported it "feels very prototypey compared to ttt and TermIDE".

That first audit compared *capability* — does it have tree-sitter, LSP, git, tabs — and every
row was a feature. None asked whether the thing looks like a finished program. This is the same
failure as the missing clipboard, one level up: the clipboard was absent because no plan
imagined it, and the furniture below is absent because the **audit** only imagined features.

| # | Sev | Defect | Where |
|---|---|---|---|
| ~~23~~ | **CRITICAL to feel** | ~~**There is no gutter. No line numbers at all.**~~ **Fixed at v0.2.3**, and as a component list rather than a column, so M3's diagnostics and M5's diff markers fill in a renderer instead of restructuring a module. Original: `styled_line` draws text and selection spans, nothing else. Worse than unplanned: `ThemeColors` has carried a `line_numbers: Color::DarkGray` field since M1 that **nothing has ever read**. An earlier session modelled the intent, wired the colour and never drew the digits. The README's ASCII art shows line numbers that do not exist. | `render.rs:52`, `panel.rs:22` |
| ~~24~~ | **HIGH** | ~~**The palette is 16-colour ANSI.**~~ **Fixed at v0.2.3** — one named ramp at one hue, contrast checked by test rather than by eye. Original: `Color::White`, `Color::Blue`, `Color::DarkGray` — not one `Color::Rgb` in the tree. TYPE inherits whatever the terminal's palette defines, cannot be tuned, and cannot look designed. TermIDE ships 38 themes; we ship one, in someone else's colours. | `panel.rs:28-43` |
| ~~25~~ | HIGH | ~~**`ThemeColors` is 10 flat fields.**~~ **Fixed at v0.2.3** — twenty-four scopes including cursorline, gutter, and a statusline that differs when inactive. Menu, popup, picker and bufferline wait for the panels that need them. Original: Helix's theme surface is 40+ `ui.*` scopes with inheritance, modifiers and underline styles. Ours has no concept of cursorline, gutter, menu, popup, picker, bufferline, virtual text, or a statusline that differs when inactive. | `panel.rs:15` |
| ~~26~~ | HIGH | ~~**The primary selection is not visually distinct.**~~ **Fixed at v0.2.3.** Original: Helix themes `ui.selection.primary` separately from `ui.selection`, and `ui.cursor.primary` from `ui.cursor`. With thirty cursors TYPE gives no way to tell which one is primary — which is the one every motion is relative to. | `render.rs:60` |
| 27 | MED | **Partly fixed at v0.2.3.** ~~No current-line highlight~~ and ~~no matching-bracket highlight~~ both shipped. **Still absent: indent guides, whitespace rendering, a scroll position indicator.** The first two need the syntax tree to look right around continuation lines and are M2.5; the third is cheap and unowned. | — |
| ~~28~~ | MED | ~~**The status bar carries 3 things.**~~ **Fixed at v0.2.3** — seven, each with an emphasis. Reorderability and click routing wait for `status_segments()` at M4. Original: Message, filename, `line:col`. Helix's statusline is **24 named, reorderable elements**: mode, LSP spinner, file encoding, line ending, indent style, filetype, diagnostics counts, workspace diagnostics, selection count, primary selection length, position percentage, total lines, version control, register, cwd, read-only indicator. ttt puts git blame and an indent picker there. | `app.rs:470` |
| 29 | MED | **The sidebar is a fixed 30 columns and cannot be resized.** ttt drags its dividers with the mouse. Invariant 8 says mouse and keyboard are peers; a layout that cannot be adjusted by either is not yet a layout. | `layout.rs:4` |
| 30 | LOW | **Partly fixed at v0.2.3**: directories and files are now coloured apart, so the shape of a project is readable without reading the names. **Still absent: icons, and git status colouring** — the latter needs `typ-git` and is M5, the former is a Nerd Font question that belongs with the symbol presets at M2.5. | `typ-panel-tree/src/lib.rs:188` |

**The structural cause underneath all of it, and its removal.** As written at v0.2.2: the
render path drew every cell on every loop pass and the loop blocked on `event::read()`, so
there was no headroom to *add* visual richness without making an already-unconditional redraw
more expensive. **v0.2.4 removed both halves.** The loop blocks on a channel, drains what is
queued and draws only when something changed, so an idle wakeup costs 425 ns against a 513 µs
frame. The headroom this list was waiting on now exists.

**Architecture, not just appearance.** Helix's gutter is a *list* of components —
`GutterType::{LineNumbers, Diagnostics, Diff, Spacer, CodeActionHint}` — each with a `width()`
and a renderer, in configurable order. Its statusline is the same shape. Building a hardcoded
line-number column would land the feature and miss the design: diagnostics and git-diff markers
arrive at M3 and M5 and need the same column.

### Missing capability the first audit also missed

Found by reading TermIDE's 45 crate names and ttt's feature list rather than their prose.

| # | Sev | Defect |
|---|---|---|
| ~~31~~ | **HIGH — data loss** | ~~**No file watching.**~~ **Fixed at v0.2.4.** Clean buffer reloads silently, dirty buffer warns and is left alone, deleted file leaves the buffer standing as the only copy. `notify` 8.2, watching the **parent directory** rather than the file, because writing by rename-over destroys the inode a file watch is pinned to and leaves it silent while the file keeps changing. Our own save is filtered by comparing the file against the buffer rather than by remembering an mtime — nothing to keep in sync, and no window where the remembered value is stale. |
| ~~32~~ | HIGH | ~~**No logging, anywhere.**~~ **Fixed at v0.2.3** — `TYP_LOG` names a file, off otherwise. A file and a mutex rather than `tracing`, which earns its weight when there are spans to correlate across the worker threads arriving at M2.4. Original: No `log`, no `tracing`, no log file. A TUI owns the screen, so `println!` debugging is unavailable by construction — the one place logging is not optional is the one place we have none. TermIDE has a `logger` crate. |
| ~~33~~ | HIGH | ~~**No select-next-occurrence.**~~ **Fixed at v0.2.3**, and searching from the cursor rather than filtering `find_all` — 3.89 µs per press on a 50k-line file. Original: `Ctrl+D` in VS Code, Sublime and ttt; `Ctrl+K L` for all occurrences. TYPE has add-cursor-above/below only, which is the *rarer* half of multi-cursor. This is the idiom people mean when they say multi-cursor. |
| 34 | MED | **Partly fixed at v0.2.5** — indent detection landed, `.editorconfig` did not. ~~**No `.editorconfig`, no indent detection.**~~ `TAB_WIDTH` is a hardcoded `const` and indentation is always spaces. ttt reads `.editorconfig` and auto-detects indent from content, with a status-bar override. TYPE will silently reformat a tab-indented project. |
| 35 | MED | **No file operations in the tree.** No new file, new folder, rename, delete. ttt puts them on a right-click context menu. The tree is currently a viewer, not a manager. |
| ~~36~~ | MED | ~~No goto-line (`Ctrl+G`)~~ **Fixed at v0.2.3**, centring the target line. Was also listed as #12, whose move-line, duplicate-line and comment-toggle remain. |
| 37 | LOW | No multi-root workspaces. ttt has Add Folder to Workspace and switches the status-bar git branch by which root the active file belongs to. Ours is one root. |
| 38 | MED | **`find_all` sits at half the keystroke budget and no milestone owns fixing it.** Measured at v0.2.3 on a 50k-line file: 5.4–8.7 ms best-of-five against 16 ms, and single samples on an idle laptop ranged 6.9–18.7 ms. Architecture §4 already states the answer — "search is viewport-first with the remainder completed off-thread, a design constraint, not a number to optimise toward" — but the constraint is written down in the spec and owned by no task. M2's search box calls `find_all` on Enter, which is fine; M4's project search and any highlight-as-you-type is where it stops being fine. **The number to watch, watched:** M2.1 recorded 10.5 ms and said so in as many words. |

### Project and process gaps

Not defects in the editor — defects in the things around it: documents that no longer describe
the tree, work no milestone owns, and the entire surface between "the code is correct" and
"someone can install it". They are numbered in the same sequence because they compete for the
same time.

| # | Finding |
|---|---|
| ~~15~~ | ~~**Architecture §10 still lists the config format as an open question.**~~ **Closed in the doc at v0.2.2** — §10 now records TOML as decided-by-shipping, and §9 carries the milestone corrections this document forced. |
| 16 | **§7 capability detection does not exist.** Truecolor, the kitty keyboard protocol, image protocols — none are probed. Synchronized output is emitted unconditionally rather than detected. No plan document owns this work. The kitty protocol is a stated prerequisite for VS Code-grade bindings, and without it `Ctrl+I` and `Tab` are literally the same byte — which is half of why defect #8 is awkward to fix cleanly. |
| ~~17~~ | **Fixed at v0.2.5.** ~~**Theming is hardcoded.** `ThemeColors::default()` is constructed inline in `App::new`.~~ A theme is a TOML file with a named `[palette]` and a typed `[ui]` table, parsed in `typ-core` and loaded by name from the config directory or from the binary, degraded to the terminal's colour depth at load. `ThemeColors` is 27 typed slots and the default is now the fallback for when no theme loads rather than the definition of the theme. Format and rubric documented in [`themes.md`](themes.md). **The other half of the row was decided against**: `typ-ui` and `typ-config` were not built and will not be — the seam falls elsewhere, see `architecture.md` §5. All six ship, each audited at both colour depths, and the rubric they are held to was itself corrected first — see #42. |
| ~~18~~ | ~~**CI never runs the perf tests.**~~ **Closed at v0.2.6.** `.github/workflows/perf.yml` runs both suites weekly and on demand. It is a tripwire rather than a gate, and deliberately so: the perf tests already take a mutex because parallel threads made `InsertChar` read 32 µs against the 1.9 µs it costs, and a hosted runner adds a noisy neighbour on top of that, so a red check firing on unrelated pull requests would train everyone to ignore it. What M6 still owes is the gate. Original text:  They are `#[ignore]`d with no scheduled job, so a budget regression is caught only when a human remembers to look. M6 promises "budgets enforced in CI" and nothing is currently walking toward it. |
| 39 | **Comment density is 22.7% of source lines** — 1,454 of 6,412 at v0.2.3, which is roughly double what idiomatic Rust carries. The rationale-carrying ones earn their place: *why* the first line terminator decides, *why* `find_next` exists instead of filtering `find_all`, *why* the gutter is a component list. The rest restate the code beneath them or argue a point already settled, and every one of those is a line that can rot out of step with what it describes. Trim toward ~12%, keeping the *why* and cutting the *what*. Mechanical, low risk, no milestone — good filler work between tasks. |
| ~~19~~ | ~~No `cargo deny` / `cargo audit`, and no MSRV job.~~ **Closed.** `cargo deny check advisories licenses bans sources` runs in CI against a `deny.toml` that lists every license currently in the graph by name rather than by wildcard, so a dependency arriving with something unexpected fails the build instead of sliding in. The MSRV half was **already covered and the row was wrong about it**: `rust-toolchain.toml` pins `1.96.0`, CI installs exactly that with `rustup show`, and `rust-version = "1.96"` names the same compiler — every CI run *is* the MSRV build. A separate job would have tested the same toolchain twice. |
| ~~20~~ | **Closed at v0.2.6.** ~~No release pipeline.~~ `.github/workflows/release.yml` builds four targets on a tag — Linux x86_64, macOS x86_64 and aarch64, Windows x86_64 — packages each with checksums and opens a draft release, and `docs/releasing.md` records the crate publish order that the manual half kept getting wrong. Hand-written rather than `cargo-dist`: readable at 120 lines, and the installer, Homebrew and winget channels cargo-dist adds are worth adopting when those channels matter rather than before. **Updated at v0.2.5.** The pipeline has now run: v0.2.4 and v0.2.5 both carry four archives with checksums, and crates.io serves 0.2.5 against a tree at 0.2.5, so the two channels agree. **What the first run exposed is #45** — the artifact it produced for Linux does not start on most Linux. **The rest closed at v0.2.6**: six targets including static musl on both Linux architectures, `install.sh` and `install.ps1`, `[package.metadata.binstall]`, and a `verify` job that gates publishing. What is left of Part 7 is first run, not installation — see #22. |
| ~~21~~ | ~~**Crate metadata is too thin to publish.**~~ **Fixed at v0.2.2.** `repository`, `homepage`, `keywords`, `categories`, `readme` and `rust-version` are all inherited from `[workspace.package]`, and every internal dependency carries a version alongside its path — which cargo requires before it will publish at all. `typ-editor` is on crates.io. The row stays because #20 does not: metadata made `cargo install` possible, and a **release pipeline is still the missing half** — nothing produces a binary for anyone who does not already have a Rust toolchain. |
| 22 | **There is no first run.** No config directory is created, no `keys.toml` is scaffolded, no capability report, no `--doctor`, no welcome state. `load_keymap` treats a missing config as "the normal case, not a problem worth a message" — correct for the config, but it means the first launch and the thousandth are indistinguishable. |
| ~~45~~ | **Fixed at v0.2.6.** Linux ships `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`, statically linked, with no glibc version to be too new for. The gnu row stays, dynamically linked and labelled as such, rather than being pinned to an older runner: `ubuntu-22.04` begins deprecation in September 2026 and its glibc 2.35 still excludes RHEL 9 and Amazon Linux 2023 at 2.34, so pinning buys a shrinking window and musl buys all of it. The static build is what `install.sh` picks, what the README points at, and what `cargo binstall` is overridden onto. It cost an allocator: musl's `mallocng` took `find_all` from 4.11 ms to 10.17 ms against a 16 ms budget, and mimalloc returns it to 4.23 ms. Original text: ~~**The published Linux binary does not start on most Linux.**~~ `release.yml` builds on `ubuntu-latest`, which has been 24.04 since January 2025, so the v0.2.5 artifact carries `GLIBC_2.39` and fails on Ubuntu 22.04, Debian 12, RHEL 9 and Amazon Linux 2023 with `version 'GLIBC_2.39' not found`. **This is the highest-severity open row in the document**, because it is the only defect that reaches a person who has never seen the editor work — and it reads to them as a broken editor, not a wrong build. It is also the cheapest to fix while the tree is still pure Rust: static musl is a matrix entry now and a build system once tree-sitter's C grammars arrive. Design in [`distribution.md`](distribution.md) §1 and §4. |
| ~~46~~ | **Fixed at v0.2.6.** `aarch64-unknown-linux-musl` builds natively on `ubuntu-24.04-arm` and runs `typ --version` on the runner that produced it. The stale comment claiming a cross linker was needed is gone. One trap on the way, found by checking a guess rather than shipping it: jemalloc's `build.rs` asks cc-rs for a target-prefixed compiler that `musl-tools` does not provide on either architecture, which is part of why the allocator is mimalloc. Original text: ~~**No aarch64 Linux target, and the reason given no longer holds.**~~ `release.yml` leaves it out because it needs a cross linker. GitHub's `ubuntu-24.04-arm` runners have been free for public repositories since January 2025, so it is a native build on a native runner. Helix, Neovim, bat and ripgrep all ship it; TYPE has the narrowest Linux coverage of anything in the surveyed field. |
| ~~47~~ | **Fixed at v0.2.5.** `.github/dependabot.yml` covers cargo and github-actions, grouped so a quiet week is one pull request per ecosystem, with a seven-day cooldown. Every action is pinned to a commit SHA and `zizmor` lints the workflows. Original text: ~~**Nothing watches the dependency graph or the action versions.**~~ No `dependabot.yml`, no renovate. Every project surveyed — helix, starship, bat, zellij, uv, ruff — has one or the other. `cargo deny` catches an advisory once it is published against a version already in the lockfile, which is a different job from keeping the lockfile current. The `github-actions` half is the quieter risk: `actions/checkout@v4` and `Swatinem/rust-cache@v2` are floating major tags nothing is tracking. |
| ~~48~~ | **Fixed at v0.2.6.** A `verify` job downloads each archive back off the release, checks the `.sha256`, unpacks it, runs the binary and asserts the version matches the tag, on a runner of the target's own architecture. Only then does `publish` flip the draft, so a release that fails verification is a draft nobody saw. The one archive nothing executes is `x86_64-apple-darwin` — the arm64 macOS runner would need Rosetta, and a check that depends on an emulator fails for reasons unrelated to the artifact — so it is checksummed and attested only. Original text: ~~**The release pipeline had never been verified end to end.**~~ Four tags, two releases, and until 2026-08-23 nobody had downloaded an artifact, checked its sum and run it. The Windows archive turned out correct; the Linux one turned out to be #45. A release job that verifies its own output would have caught it at the tag. |

### Why the plans could not catch these

Defects 1, 7 and 8 survived 292 tests, sixteen plan tasks and four self-review passes. Not
because the review was sloppy — the self-review in `m2-editing.md` caught seven real
compile-and-logic defects before a line was written, which is a better hit rate than most code
review achieves.

They survived because **the tests assert what the plan asked for, and the plan asked for what
the plan imagined.** No plan mentioned a clipboard, so no test missed one. This is the class of
defect a written spec structurally cannot catch, and there is exactly one known remedy: use the
thing.

Which brings us to the real finding.

### The strategic hole

**Self-hosting is declared the forcing function and has never been engaged.**

> *"M2 — Editing is real. … Self-hosting begins — TYPE edits TYPE."* — architecture §9
>
> *"Self-hosting from M2 onward is the forcing function. Every bug gets found by the author
> using it daily, and the project stays alive because it is useful before it is finished."*

TYPE cannot edit TYPE today. No clipboard, no Tab indent, no syntax highlighting, no file
finder, no second file without losing the first. M2 is checked complete and the mechanism meant
to keep every later milestone honest never switched on.

**This is the highest-value finding in the document.** Not because any single defect is fatal,
but because the process that was supposed to surface defects like these is not running. Every
milestone after this one inherits the same blindness until it is.

---

## Part 2 — The field, measured

Two classes, because they fail differently. GUI editors set the *capability* bar. Terminal
editors set the *achievable* bar and show which capabilities survive the translation.

### GUI class — the capability bar

| | VS Code | Zed | Sublime Text |
|---|---|---|---|
| Renderer | Electron / DOM | GPUI, custom GPU | custom GPU |
| Cold start | ~1.2 s | ~0.12 s | ~0.1 s |
| Input latency | 12–25 ms | ~2 ms | ~5 ms |
| Idle RAM | 300–650 MB | 150–250 MB | ~100 MB |
| Extensions | marketplace, webview host | WASM, Tree-sitter grammars | Python plugin host |
| Market share | 75.9% of developers | 1.0 shipped 2026-04-29 | long-tail loyal |

**What each one is actually loved for**, which is not the same as what it ships:

- **VS Code** — the command palette ("access nearly every feature without touching menus"),
  multi-cursor, peek-definition (view a definition inline without leaving the file), and the
  extension ecosystem. Note that two of those four are *navigation and discovery*, not editing.
- **Zed** — raw speed, and **multibuffer**: editing fragments from many files in one buffer, as
  a single editable surface. This is the one genuinely novel editing primitive to appear in the
  last decade and nothing in TYPE's plan has an equivalent.
- **Sublime** — startup speed, **Goto Anything** (`Ctrl+P`, then `@symbol`, `#text`, `:line`
  composed in one input), the minimap, and originating multi-cursor. Its reputation for feeling
  good is mostly *responsiveness plus smooth scrolling*, both of which are latency stories.

### Terminal class — the achievable bar

| | Helix | TermIDE | Fresh | ttt | **TYPE (now)** |
|---|---|---|---|---|---|
| Language | Rust | Rust | Rust | Go | Rust |
| Modal | yes (Kakoune) | optional vim | no | no | **no** |
| Mouse parity | afterthought | good | partial | partial | **peer, by rule** |
| Highlighting | tree-sitter | tree-sitter, 22 langs | tree-sitter + syntect | regex (chroma) | **tree-sitter, 5 langs** |
| Fuzzy file picker | yes (nucleo) | yes | yes | yes | **yes, nucleo-matcher** |
| Project search | yes (ripgrep libs) | yes | yes | yes | **yes, ripgrep libs, searches unsaved buffers** |
| LSP | yes | yes | yes | hand-rolled | no |
| DAP | no | no | partial | no | planned v1.2 |
| Terminal panel | **no** | yes | yes | yes | planned M5 |
| Git | gutter only | status, log, diff, stage | yes | yes | planned M5 |
| Plugins | Steel, **PR open ~2 yrs, unmerged** | none | QuickJS | Lua | planned v1.1 |
| **Themes** | many | **38, custom TOML** | many | few | **6, TOML, every one audited** |
| Tabs / splits | splits, no tabs | yes | yes | yes | planned M4 |
| $EDITOR | yes | yes | yes | yes | **yes, from M1** |
| OS file association | none | none | Linux only | none | **planned v1, differentiator** |

TermIDE is the closest competitor and worth naming precisely. Beyond the editor it ships: a
database viewer (SQLite/Postgres/MySQL), a hex editor, a Mermaid diagram viewer, a markdown and
HTML viewer, a text-mode web browser, a resource monitor, SFTP/FTP/SMB remote browsing, code
outline, diagnostics panel, sessions and bookmarks, **38 themes**, and **15 UI languages**.

That is the bar for "mature terminal IDE" as of 2026, set by one author in eight months.

### Design and usability, read from source

Added at v0.2.2. The first pass compared feature lists; this one compares *how the thing is
built and how it feels*, which is what the feature lists were hiding.

#### Helix — the gutter and statusline are composable, not hardcoded

`helix-view/src/gutter.rs` does not draw a line-number column. It draws a **list of gutter
components**:

```rust
GutterType::{ LineNumbers, Diagnostics, Diff, Spacer, CodeActionHint }
```

Each has a `width()` and a render function, and the order is configuration. `LineNumbers`
computes its width from the digit count and supports relative numbering
(`current_line.abs_diff(line)`). `Diff` colours from `diff.plus.gutter` / `diff.minus.gutter` /
`diff.delta.gutter`. `Diagnostics` shares its column with breakpoints.

**This is the lesson for TYPE**: diagnostics arrive at M3 and git markers at M5, and both want
that column. A hardcoded line-number gutter lands the feature and loses the design.

`helix-term/src/ui/statusline.rs` is the same shape — **24 named elements**, reorderable across
left/centre/right:

> Mode · Spinner · FileBaseName · FileName · FileAbsolutePath · FileModificationIndicator ·
> ReadOnlyIndicator · FileEncoding · FileLineEnding · FileIndentStyle · FileType · Diagnostics ·
> WorkspaceDiagnostics · Selections · PrimarySelectionLength · Position · PositionPercentage ·
> TotalLineNumbers · Separator · Spacer · VersionControl · Register · CurrentWorkingDirectory ·
> CodeActionHint

TYPE's status bar carries three of those. Architecture §5 already plans `status_segments()` for
M4 as *clickable chips contributed by the focused panel* — a better design than Helix's central
list, since a panel owns what it can say about itself. The gap is content, not mechanism.

#### Helix — what a theme actually has to cover

40+ `ui.*` scopes, before a single syntax scope: `ui.background`, `ui.cursor{,.primary,.match,
.insert,.normal,.select}`, `ui.cursorline{.primary,.secondary}`, `ui.cursorcolumn.*`,
`ui.linenr{,.selected}`, `ui.gutter{,.selected}`, `ui.selection{,.primary}`,
`ui.statusline{,.inactive,.normal,.insert}`, `ui.bufferline{,.active,.background}`, `ui.menu{,
.selected,.scroll}`, `ui.popup{,.info}`, `ui.picker.header{,.column,.column.active}`,
`ui.help`, `ui.highlight{,.frameline}`, `ui.debug.{breakpoint,active}`,
`ui.background.separator`, `ui.virtual.*`. Plus inheritance between themes, text modifiers, and
**underline styles** — which is what makes coloured undercurl a theme decision rather than a
special case.

~~TYPE's `ThemeColors` is ten flat `Color` fields.~~ **Written at v0.2.1; both named gaps closed
at v0.2.3** — the gutter and `line_number_fg` landed with it, and `selection_primary_bg` is
audited to differ from `selection_bg` by at least 1.3:1 rather than merely existing.
`ThemeColors` is **25** typed slots as of M2.5, loaded from a file.

The count is the less interesting half of the comparison and the shape is the more interesting
one. Helix uses **one flat namespace** where `ui.linenr` and `keyword` are the same kind of key,
which means a typo in `ui.linenr` is silently ignored and the theme just renders wrong. TYPE
splits them: `[ui]` is a closed record known at compile time, so an unknown key is a load error
with a did-you-mean, and `[syntax]` is an open map of capture names because that set genuinely is
open. That is the one place TYPE deliberately does not follow the field, and the reason is that
the two halves are different kinds of thing wearing the same syntax.

Where Helix is still ahead and TYPE has no answer: **inheritance between themes**, **text
modifiers**, and **underline styles** — the last being what makes coloured undercurl a theme
decision rather than a special case, which M3's diagnostics will want.

#### ttt — 182 stars, Go, and a feature list that reads as a gap list

Everything below ships in ttt and is absent from TYPE **and from every TYPE plan**:

`.editorconfig` support · indent auto-detection from file content with a status-bar override ·
bracket matching with highlighted pairs · goto-line · **`Ctrl+D` select next occurrence and
`Ctrl+K L` select all occurrences** · right-click context menus · draggable sidebar and panel
dividers · signature help · inline curly-underline diagnostics plus a problems panel plus hover
plus status-bar counts · format-on-save · inline git blame in the status bar · line numbers with
current-line highlight · a tabbed bottom panel · multi-root workspaces where the status-bar git
branch follows the active file's root · tree context menu with New File, New Folder, Rename and
Delete · directories sorted before files.

It also ships an `install.sh`, a `flake.nix`, and a `community-plugins.json`.

**The single most important item there is `Ctrl+D`.** TYPE has add-cursor-above and
add-cursor-below, which is the *rarer* half of multi-cursor. Select-next-occurrence is the
idiom people mean by the word, and it is what the `Selections` model was built to make cheap.

#### TermIDE — 45 crates, and the names are the architecture

```
app-core app buffer clipboard config core db fetch file-ops git highlight html i18n keyboard
layout logger lsp mermaid modal panel-binary panel-db panel-diagnostics panel-editor
panel-file-manager panel-git-diff panel-git-log panel-git-status panel-html panel-image
panel-markdown panel-mermaid panel-misc panel-operations panel-outline panel-terminal richtext
session state system-monitor theme ui-render ui unicode-width-fix vfs watcher
```

Fourteen panel crates, which is the `OpenWith`/registry bet vindicated — every one of those is a
handler registration in TYPE's design rather than a core change.

The non-panel crates are the more useful signal, because they name concerns TYPE has no home
for: **`watcher`** (external file changes — defect 31, a data-loss bug), **`logger`** (defect
32), `vfs`, `file-ops` (defect 35), `session`, `i18n`, `keyboard` as its own crate, and
`ui-render` split from `ui`. They still carry `unicode-width-fix`, which is the fork TYPE
tested and rejected — that decision still holds.

#### What none of them do, which is still TYPE's opening

No OS-level file association on any platform. No non-modal terminal IDE with full mouse parity
and a plugin story. Helix's plugin PR is ~2 years open; TermIDE has none; ttt's is Lua.

### What the terminal can now do that it could not in 2015

The 256-color ncurses ceiling is gone. Modern emulators — Kitty, WezTerm, Ghostty, Alacritty —
are GPU-accelerated and support truecolor, styled and colored **undercurl**, ligatures, mouse
tracking, the kitty keyboard protocol, synchronized output, and pixel-accurate image protocols
(Kitty graphics, iTerm2, Sixel). Nerd Fonts have made icon glyphs and powerline separators a
safe default. `ratatui-image` unifies the three image protocols behind one widget with a
halfblock fallback.

**The gap between a pretty TUI and a pretty GUI in 2026 is much smaller than it looks, and it
is almost entirely a matter of whether the application bothers.**

---

## Part 3 — What the field has that TYPE has not planned

Sorted by how badly the absence would be felt. Items already in a milestone are omitted.

| Feature | Who has it | Why it matters | Proposed home |
|---|---|---|---|
| **Clipboard** | everyone | defect #7 | v0.2.2 |
| **Indent / outdent** | everyone | defect #8 | v0.2.2 |
| **Theme system + shipped themes** | TermIDE (38), all GUI editors | see Part 5; also a hard dependency of tree-sitter highlighting | **v0.2.5** |
| **Goto Anything–style composed finder** | Sublime, VS Code, Zed | one input that does files, `@symbols`, `#text`, `:line`. TYPE plans a "fuzzy file finder" and a separate palette; composing them into one is strictly better and no harder | M4 |
| **Minimap** | Sublime, VS Code, Zed | listed in TYPE's post-v1 polish. In a terminal it is cheap — a column of half-blocks — and it is one of the most recognisable "this looks like a real editor" signals | M4 |
| **Peek definition** | VS Code, Zed | inline definition without leaving the file. Pure LSP data TYPE will already have; costs a floating panel | M3 |
| **Multibuffer** | Zed only | edit fragments from many files as one surface. Genuinely novel. Falls out almost free from project-search results + the existing `Selections` model | post-v1, but design M4 so as not to preclude it |
| **Undercurl for diagnostics** | Helix, Neovim | squiggles under errors, not just colored text. Terminal-supported since ~2020, still rare | M3 |
| **Bracketed paste** | everyone | defect #9 | v0.2.2 |
| **Sticky scroll / breadcrumbs** | VS Code, Zed | already listed post-v1 polish; tree-sitter makes it nearly free once highlighting exists | M4 |
| **Session restore** | TermIDE, all GUI | already M4 | M4 |
| **Remote file browsing (SFTP/SMB)** | TermIDE | TYPE's answer is "SSH in and run typ", which is defensible and stated in §3 | declined, deliberately |
| **UI localisation** | TermIDE (15 languages) | real work, no user asking for it yet | declined until asked |
| **Database / hex / mermaid viewers** | TermIDE | exactly what `OpenWith` + `typ-registry` exist to enable; correctly post-v1 | post-v1 |

Two conclusions worth stating outright:

- **TYPE's plan is not missing much at the capability level.** The protocol bet (LSP + DAP +
  tree-sitter) covers most of it, and the registry covers most of the rest. The misses are
  concentrated in the small, unglamorous, daily-use layer — clipboard, indent, tabs, themes —
  which is precisely the layer a spec-driven process under-weights.
- **The finder should be one composed input, not two features.** Deciding that at M4 rather
  than after shipping two half-features is worth more than anything else in this table.

---

## Part 4 — Tabs

**Already planned:** architecture §8 lists "splits, tabs, layout, session restore" in v1 scope,
and §9 puts them in **M4 — Workspace**. So tabs are in, at v0.4.0, and nothing needs adding to
the roadmap for them.

### Do tabs fix the data-loss defect?

Partly, and the part they do not fix is the dangerous part.

Tabs change what *opening* means: a second file gets a second buffer rather than replacing the
first, so the specific path in defect #1 — click a file in the tree, lose your edits — stops
existing. That is real and it is most of the daily exposure.

But the guard is still required, because the question only moves:

- **Closing a dirty tab** needs a prompt. Same question, new trigger.
- **Quitting with several dirty tabs** needs to ask per tab, or ask once and list them. VS Code
  shows a modal per file; Sublime cycles through them.
- **M4 is three milestones away.** Between now and then every tree click is a live data-loss
  path, and #1 costs about fifteen lines using the `needs_close_confirmation` machinery that
  already exists.

So: **fix #1 now as a guard on replace, and let M4 turn that guard into a per-tab guard.** The
work is not wasted — the confirmation logic is the same, only the trigger changes. What would
be wasted is designing a tab system early to avoid writing fifteen lines.

### What tabs must get right, from the field

- **Tabs are a view over buffers, not the buffer list itself.** VS Code's split between "open
  editors" and "tabs" exists because one buffer can appear in two panes. Model the buffer set
  centrally and let tabs and splits both be views onto it, or the second split rewrites the
  first design.
- **Preview tabs** (VS Code's italic single-click tab, replaced by the next preview) are why a
  tree-heavy workflow does not end with forty tabs open. Cheap, and users notice its absence.
- **Tab overflow in a terminal is a real constraint** that GUI editors solve with scrolling
  chrome. Consider numbered tabs with `Alt+1..9` — keyboard-first, no chrome, and it matches
  the mouse/keyboard parity rule.
- The buffer set is exactly what `to_session()` (architecture §5, adopted at M4) serialises.
  Design them together.

---

## Part 5 — "Pretty as fuck": what a terminal can and cannot do

One correction first, because it changes what gets built.

### Fonts: TYPE does not get a vote

**A terminal application cannot choose, load, size, or fall back a font.** The terminal emulator
owns the font entirely. There is no escape sequence to request one, and there will not be. This
is not a limitation to engineer around — it is the boundary of the medium.

What that means concretely:

| Want | Possible? | Actually available |
|---|---|---|
| Ship a font with the editor | ❌ | — |
| Set font family / size / weight | ❌ | the user's terminal config |
| Ligatures (`->`, `=>`, `!=`) | ❌ TYPE's call | works if their font has them; TYPE just must not corrupt the columns |
| **Bold, italic, underline, strikethrough** | ✅ | SGR attributes, universally supported |
| **Styled + colored undercurl** | ✅ | modern terminals; the right way to draw a diagnostic squiggle |
| **Truecolor, 24-bit** | ✅ | universal in modern emulators, needs a fallback path |
| **Nerd Font icon glyphs** | ⚠️ config | must be a user setting, not detection — a missing glyph renders as tofu |
| **Box drawing, blocks, braille** | ✅ | borders, minimap columns, sparklines, progress |
| **Real pixel images** | ✅ | Kitty / iTerm2 / Sixel via `ratatui-image`, halfblock fallback |

**The one place TYPE does pick a font** is the M6 launcher shim — when a double-click spawns a
terminal, TYPE chooses that terminal and its config. Architecture §6 already flags the shim as
"the real risk" and says the polish budget goes there. This is why: it is the only pixel of
typography the project will ever control, and it is a first impression.

**Therefore the deliverable is not font support. It is a documented recommended setup** —
terminal, Nerd Font, truecolor — shipped in the README with the same care as the config docs,
plus a first-run status-bar hint when capability detection (#16) finds no truecolor.

### So what does "pretty" actually consist of?

Ranked by visual return per unit of work:

1. **Syntax highlighting** (v0.2.5, planned). Monochrome → colored is the single biggest jump
   the project will ever make. Nothing else is close.
2. **A theme system with several good themes shipped** (see below). Colors chosen by a palette,
   not picked ad hoc per widget.
3. **Diagnostic undercurl and inline hints** (M3). What makes an editor look *intelligent*
   rather than merely colored.
4. **Minimap** (M4). Half-block column, ~100 lines, instantly recognisable.
5. **Sticky scroll and breadcrumbs** (M4). Nearly free once the tree-sitter tree exists.
6. **Border, focus and status polish.** One visual system, per architecture §4 — already a
   stated principle, currently one hardcoded palette.
7. **Motion.** Smooth scrolling and a cursor that eases between positions. Sublime's reputation
   for feeling good is substantially this. In a terminal it is bounded by refresh and by the
   damage-driven redraw landing first (v0.2.5).
8. **Images** (post-v1, with the image viewer panel).

### The theme system belongs in v0.2.5, not M4

This is the non-obvious scheduling call in this document, and it is forced by a dependency
rather than chosen for polish:

**Tree-sitter highlighting cannot be written without a theme.** A highlighter produces capture
names — `keyword`, `function`, `string`, `type.builtin` — and something must map those names to
colors. That mapping *is* a theme. Writing v0.2.5 with the mapping hardcoded means writing it
twice, and the second pass touches every call site.

~~So `typ-config` and `typ-ui` land at v0.2.5:~~ **The theme system landed at v0.2.5; the two
crates did not** — the seam falls elsewhere and both were decided against, see `architecture.md`
§5. What shipped:

- TOML themes loaded from the config dir beside `keys.toml`, or from the binary
- Truecolor with a 256-colour degradation path (needs #16), **and the audit re-run on the
  degraded palette** — quantising moves every colour by a different amount, and nothing else in
  the field checks whether a theme survives it
- The existing `ThemeColors` moves out of `App::new` and becomes the loaded artifact

**Two things this section got wrong, worth keeping visible.** The dependency argument above is
right that a highlighter needs a theme, but it concluded both belong in one milestone; they did
not fit, and tree-sitter moved to M2.6 while the theme system took M2.5 alone. And "3–4 shipped
themes chosen deliberately rather than ported at random" understated the cost of *porting* —
across 97 published palettes measured against this project's rubric, 13 clear it and every one
of the 46 light ones fails. A port is an adaptation, and that has to be said in the theme file
rather than discovered per contributor.

TermIDE ships 38 themes. TYPE does not need 38 — it needs the *system*, plus enough themes to
prove the system is real. Community themes are how a count like 38 happens, and they need a
documented format, not a bigger initial commit.

---

## Part 6 — Revised roadmap

Changes from the roadmap in the README, with reasons.

| Version | Milestone | Scope | Change |
|---|---|---|---|
| ~~v0.2.2~~ | **M2.2 — Usable** | clipboard, indent/outdent, dirty guard on open, new-file creation, undo cap, bracketed paste, temp-file nonce | **shipped** — turned on self-hosting |
| ~~v0.2.3~~ | **M2.3 — Polish** | the gutter and line numbers, truecolor theme surface, current-line highlight, distinguishable primary selection, bracket matching, status-bar segments, `Ctrl+D` select-next-occurrence, goto-line, logging | **shipped** — all eight tasks, plus line-ending detection and the first measurement of the render path |
| v0.2.4 | M2.4 — Live | wakeable channel, **file watching — a data-loss bug**, damage-driven redraw, resize handling, dropped-keystroke fix, line-ending preservation, save metadata | **split from M2.5 at v0.2.3** |
| v0.2.5 | M2.5 — Colour | ~~tree-sitter highlighting, `typ-config`,~~ themes as files, capability detection, ~~`.editorconfig` and~~ indent detection, indent guides and whitespace rendering | **split again** — see below |
| v0.2.6 | M2.6 — Parse | tree-sitter highlighting, grammar distribution, off-thread parse, `config.toml`, terminal light/dark, kitty keyboard protocol | carved out of M2.5 |
| v0.3.0 | M3 — Code intelligence | LSP: completion, diagnostics, goto-def, rename, code actions, **+ undercurl, + peek definition** | two additions |
| v0.4.0 | M4 — Workspace | splits, **tabs** (with per-tab dirty guard), sessions, **one composed Goto-Anything finder**, project search, **+ minimap, + sticky scroll**, capability detection | finder composed, polish pulled in |
| v0.5.0 | M5 — Terminal and git | PTY panel, git gutter/status/diff/blame | unchanged |
| v1.0.0 | M6 — Association and polish | OS association, launcher shim **and its font/terminal choice**, single-instance routing, perf budgets in CI | shim's typography role named |

**Why M2.5 was split.** As scoped above it was three milestones wearing one number: the event
loop, file and save correctness, and syntax plus theming. The bundling is actively harmful —
the loop rework is the riskiest change in the project and tying it to the largest new subsystem
means neither half ships if either goes badly. The seam was already there: everything in M2.4
is about the editor being *live and correct* and none of it needs a theme, while every item in
M2.5 wants the worker channel M2.4 builds. Split at v0.2.3.

**And split again at v0.2.5, along the same joint.** The row above still bundled tree-sitter with
themes, on the correct observation that a highlighter needs a theme to map capture names onto.
Correct about the dependency, wrong about the direction: the mapping *is* a theme, so the theme
system has to exist **first**, and once it does the highlighter is a separate body of work that
shares nothing with it but a file format. Everything in M2.5 is **config and paint**; the
highlighter is **parse**.

M2.6 also inherits a real unscheduled problem that no milestone had ever owned: `cargo install
typ-editor` produces a binary with **no grammars**, and fetching or building them is first-launch
UX ([Part 7](#part-7--install-and-first-launch)). It deserves its own risk budget rather than
being discovered halfway through a milestone that also owes a theme system.

Two items moved off M2.5 for unrelated reasons. `.editorconfig` is a file-format spec with globs,
`root = true` and tree-walking inheritance — unrelated to colour, and it belongs beside
`.gitignore` parsing. Terminal light/dark moved to M2.6 because it needs the `theme = { dark,
light }` config plumbing that lands there, and because it is the one thing in this document no
editor in the field does properly, which is a reason to give it room rather than the last slot in
a full milestone.

Post-v1 unchanged: plugin host v1.1, DAP v1.2, viewer panels. Add **multibuffer** as a design
constraint on M4 rather than a feature: do not build tabs in a way that precludes it.

### The one process change

**Self-host from v0.2.2 onward, for real.** The audit above is what a spec-driven process
misses, and no amount of additional planning finds it. Use TYPE to write TYPE's own next
milestone plan, and every defect in Part 1 that survived sixteen tasks and 292 tests would have
surfaced in an hour.

---

## Part 7 — Install and first launch

The most-used surface in any editor is the one every single user touches exactly once, and it
is the only one TYPE has never designed. Nothing in the architecture, the milestones, or the
plans covers installation or first run. Defects #20–#22 are the symptoms; this is the design.

### What the field does, read from source

**oh-my-pi** — already cited in architecture §7 and §11 for synchronized output and image
protocol detection — is the reference implementation for this entire section. It ships a real
setup wizard, and the way it handles the font question is better than the obvious design.

#### The wizard

`packages/coding-agent/src/modes/setup-wizard/` is a **scene-based wizard**, and three things
about its structure are worth taking before any of its content:

- **Scenes are versioned.** Each declares a `minVersion`; a stored `CURRENT_SETUP_VERSION`
  decides which run. A new user runs every scene; an upgrading user runs *only the scenes added
  since they last set up*. Nobody is ever re-asked a question they already answered, and adding
  a scene in a later release is a supported operation rather than a re-onboarding event.
- **Hard environment gates**, checked before any scene: it requires a TTY, and `--force`
  overrides the version and skip gates but still cannot override the TTY requirement. This is
  exactly the mechanism that keeps a wizard from ever appearing inside `git commit`.
- **Mouse and keyboard are peers inside the wizard.** Wheel moves the highlight with live
  preview, hover lights the row under the pointer, click confirms. The same rule TYPE holds for
  panels, applied to setup. It is also tested at a 24-row terminal, which is the size that
  breaks these things.

#### The font question, answered by eye

The theme scene (`scenes/theme.ts`, title *"Pick a theme"*, subtitle *"Move through the list to
preview; Enter saves the highlighted choice"*) offers six curated options:

| Option | Description, verbatim |
|---|---|
| Match terminal | "Titanium in dark terminals, Light in light terminals" |
| Titanium | "Default dark theme" |
| Light | "Default light theme" |
| Colorblind colors | "Adjust red/green contrast" |
| **ANSI-safe** | **"ASCII glyphs with the dark terminal theme"** |
| Browse all… | "Show every built-in and custom theme" |

Every option **previews live** against a mock status line and mock editor — "Theme changes
preview live. Nothing is saved until you press Enter" — and cancelling restores what was there
before.

**This is the insight, and it is not the design I proposed above.** Glyph support cannot be
detected, but the answer is not to *ask* the user whether they have a Nerd Font — most people
do not know, and the question is jargon at the exact moment a user has the least context. The
answer is to **render the glyphs and let their eyes be the sensor.** If the preview looks like
boxes, they pick ANSI-safe. No terminology, no detection, no lying. The font question arrives
disguised as a theme choice, which is also what it actually is.

It is also why the glyph setting is bundled *into* the theme scene rather than standing alone:
"ANSI-safe" is a presentation choice, and presentation is one decision to a user even when it
is two settings underneath.

#### The one anti-pattern

The same project also ships this, in `welcome.ts`:

```ts
if (theme.getSymbolPreset() === "unicode" && Math.random() < 0.1) {
	this.#selectedTip = "Please use nerdfont 😭.";
}
```

A 10% random welcome tip that names no font, links nothing, and offers no action — and its own
discussion tracker carries a user asking what a Nerd Font even is and what they were supposed
to do about it. The wizard is the good design; the tip is the residue of not having had one.
**Take the wizard, not the nag.**

For contrast on the *install* half specifically: `oh-my-posh font install` opens an interactive
font selector and downloads and installs the chosen Nerd Font — system-wide with privileges,
user directory otherwise, across all three OSes. Worth knowing that a tool can just do this,
if TYPE's wizard ever wants an "install one for me" branch rather than only a preview.

#### The symbol table underneath

`packages/coding-agent/src/modes/theme/symbols.ts` is what makes the ANSI-safe option a real
option rather than a degraded one:

```ts
export type SymbolPreset = "unicode" | "nerd" | "ascii";
export const SYMBOL_PRESETS: Record<SymbolPreset, SymbolMap> = {
	unicode: UNICODE_SYMBOLS, nerd: NERD_SYMBOLS, ascii: ASCII_SYMBOLS,
};
```

Every glyph in the UI is a named `SymbolKey` resolved through the active preset's table —
tree connectors, box drawing in rounded and sharp variants, separators, language icons, even
**per-preset spinner frames**. Not a `has_nerd_font: bool` sprinkled through render code: one
named table swapped wholesale, with ASCII as a real floor rather than a degraded afterthought.

Preset and theme are separate settings that the wizard presents as one choice — which is only
possible because the preset is a table swap. A boolean scattered through render sites could not
be previewed live, and the whole design collapses back into asking a jargon question.

**Summary of what to take:**

| Take | Why |
|---|---|
| Scene-based wizard, versioned per scene | new scenes in later releases run alone; nobody is re-onboarded |
| TTY as a hard gate, unoverridable by `--force` | the mechanism that keeps setup out of `git commit` |
| Mouse and keyboard peers inside the wizard, tested at 24 rows | same rule TYPE already holds for panels |
| **Glyph choice as a live preview, not a question** | the user's eyes are the only working sensor for font support |
| Symbol presets as a swappable named table, ASCII a real floor | makes "no Nerd Font" a supported configuration, not a broken one |
| Policy: recommended, never required, degrade honestly | "display quality, not a required dependency" |
| ~~Random nagging tip~~ **avoid** | fires 1 in 10 launches, says nothing actionable, generated its own support thread |

### Terminal light/dark, and following it live

The other thing worth taking from `packages/tui/src/terminal.ts`, and it is the highest-value
"pretty" finding in this document: **the terminal will tell you whether it is light or dark,
and tell you again when that changes.**

- **OSC 11** queries the background colour; luminance decides dark vs light.
- **DEC mode 2031** asks the terminal to *notify* on colour-scheme change. It is the trigger;
  OSC 11 is the query. omp disables it explicitly on teardown (`\x1b[?2031l`).
- Detection is **tiered with named real-world breakage**, which is the part that would take
  months to rediscover: Tier 1 OSC 11, Tier 2 the `COLORFGBG` env var, Tier 3 native macOS
  appearance *only* where the terminal path is known-broken — "Zellij currently breaks OSC 11
  passthrough on macOS, so terminal-derived appearance cannot be trusted there." tmux needs a
  repeated query where a direct terminal needs one, and Windows Terminal has no end-to-end 2031
  at all, so it is polled every 30 s. Default when everything fails: dark.

omp pairs this with an `autoDarkTheme` / `autoLightTheme` setting, a filesystem watcher for
live theme reload, and a colour-blind mode.

**TYPE should follow the terminal's theme automatically.** No terminal editor in the surveyed
field does this well, it is a visible, unmistakably-polished behaviour, and the hard part —
which terminals lie, and how — is documented above from a working implementation.

### Can TYPE detect what the user has?

This corrects [Part 5](#fonts-type-does-not-get-a-vote), which said font capability "must be a
config setting, not detection." That is right about icons and wrong about widths, and the two
questions are worth separating because only one of them is answerable.

| Question | Detectable? | How |
|---|---|---|
| Does the terminal support truecolor? | ✅ | `COLORTERM`, plus a DECRQSS colour query |
| Which Unicode version does it use for widths? | ✅ | **CPR probing** — print a glyph, request the cursor position, compare where it landed against where it should have. This is exactly what `ucs-detect` does |
| Does it support kitty keyboard, sync output, images? | ✅ | documented query sequences, all with timeouts |
| **Is the terminal currently light or dark, and did that just change?** | ✅ | **OSC 11** queries the background colour; **DEC mode 2031** asks the terminal to *notify* the application when the colour scheme changes. `pi-tui` implements both, plus native macOS dark/light detection. See below |
| **Does the user's font actually contain a Nerd Font glyph?** | ❌ | **No.** The Nerd Fonts maintainers state there is no general programmatic way. A missing glyph still advances the cursor — the terminal's width table has no idea the font will draw tofu. Any answer would be per-terminal and fragile |

So the split is clean, and it is the whole design:

- **Widths and protocol capabilities are probed at startup**, silently, with timeouts and safe
  fallbacks. This is architecture §7's capability detection, still unbuilt (#16). It is also
  load-bearing for correctness, not just for looks: `col` is a grapheme index and the whole
  mouse-click-to-cursor path depends on TYPE and the terminal agreeing on how wide a glyph is.
- **Glyphs are shown, not asked about.** Because the answer cannot be measured and the user
  cannot reliably self-report it either, the working instrument is a **live preview** of the
  glyphs themselves. It ships as a **symbol preset** (`ascii` / `unicode` / `nerd`), defaulting
  to `unicode`, which is safe everywhere and needs no font.

**So the instinct that prompted this section is right on both counts:** a setup step that
handles the font question is the correct design, and oh-my-pi is where to take it from. The
refinement the source adds is that the good version does not ask a question at all.

### What first launch should be

Design constraints, in priority order:

1. **`typ` must open instantly on a cold machine with no ceremony.** Architecture §4 promises
   sub-100 ms cold start. A wizard that runs before the editor appears breaks the single most
   important first impression the project has. **The setup is not a gate.**
2. **`$EDITOR` mode is sacred.** `git commit` invoking `typ` must never show onboarding. Under
   `$EDITOR` the process is not the user's focus, it is in the middle of someone else's
   workflow, and this is an M1 invariant (§6), not a preference.
3. **One line, dismissible, actionable.** First launch opens normally and the status bar shows
   a single hint — with the exact command that acts on it, per the oh-my-pi lesson. Not a modal,
   not a splash screen, not an emoji plea.
4. **`typ --setup` does the work when asked**, as versioned scenes. Probe capabilities and
   report what the terminal can and cannot do; **pick a theme by live preview, with the glyph
   preset folded into that choice** as oh-my-pi does; write a starter `keys.toml`. Re-runnable,
   never automatic, TTY-gated, mouse and keyboard both. Scene versioning from the start — it is
   nearly free on day one and impossible to retrofit without re-onboarding everyone.
5. **`typ --doctor` prints the same probe non-interactively.** One screen: terminal, truecolor,
   kitty keyboard, sync output, image protocol, Unicode width version, config path, theme.
   This is the first thing anyone will be asked to paste into a bug report, and it costs
   almost nothing once the probe exists for §7's sake.
6. **Degrade honestly and silently.** No Nerd Font means ASCII fallbacks for every glyph, not
   tofu, and never a nag on each launch. Follow oh-my-pi's policy — display quality, not a
   dependency.

### What install should be

The bar, from omp's README — **five channels, none of them "clone and build"**:

```
curl -fsSL https://omp.sh/install | sh      # macOS, Linux
irm https://omp.sh/install.ps1 | iex        # Windows
brew install can1357/tap/omp                # Homebrew
bun install -g @oh-my-pi/pi-coding-agent    # native package manager
nix run github:can1357/oh-my-pi             # Nix
```

It also **generates shell completions from live command metadata** for bash, zsh and fish, so
flags and enum values complete without a hand-maintained completion file. TYPE ships no
completions and has no mechanism for them; the keymap and `Action::ALL` are already the
metadata that would generate them.

| Channel | Priority | Notes |
|---|---|---|
| `cargo install typ-editor` | **1** | **Done at v0.2.2** — metadata filled in, name held, published. The one channel that works today, and it reaches only people who already have a Rust toolchain |
| GitHub Releases, prebuilt binaries | **1** | **Workflow built at v0.2.3**, hand-written: Linux x86_64, macOS x86_64 and aarch64, Windows x86_64, with SHA-256s, on a tag. aarch64 Linux is left out because it needs a cross linker. Nothing has been released through it yet |
| Shell / PowerShell one-liner | 2 | The one thing `cargo-dist` would add for free that the hand-written workflow does not |
| Homebrew, Scoop, winget | 3 | Generated by `cargo-dist`; matters most on Windows, where the OS-association differentiator lives — which is the point at which adopting it pays for itself |
| AUR, nixpkgs, Debian | 4 | Community territory once there is a tagged release to package |

The versioning scheme adopted at v0.2.1 is the precondition for all of this, and it now has a
tag with nothing to release. **The gap is the pipeline, not the decision.**

### Where this lands

Install and first launch are **M6**, alongside OS association and the launcher shim — they are
the same concern (how a person first meets this program) and the shim already owns the
terminal-and-font choice that a double-click implies. Architecture §6 says the polish budget
goes there; this is more of what "there" means.

Three exceptions pulled earlier, because they are cheap and they compound:

- ~~**Crate metadata and the crates.io name reservation: now.**~~ **Done at v0.2.2.**
- ~~**`cargo-dist` release workflow: slipped past v0.2.2, still unowned.**~~ **Built at v0.2.3,
  by hand rather than generated.** `release.yml` turns a tag into four platform archives with
  checksums and a draft release; `docs/releasing.md` carries the close-out and the publish
  order. cargo-dist remains the upgrade path for the installer, Homebrew and winget channels —
  taken when Windows association at M6 makes them earn their keep. **What is still owed is the
  first run**: three tags exist with no release behind any of them.
- **Capability probing: v0.2.5**, because tree-sitter highlighting needs to know whether it can
  emit truecolor, and `--doctor` is then nearly free.
- **Symbol presets and terminal light/dark following: v0.2.5**, with the theme system. Both are
  theme concerns, and retrofitting a preset table through render code that already hardcodes
  glyphs is the expensive order to do it in.

Shell completions ride along with the release pipeline — generated from `Action::ALL` and the
keymap rather than hand-written, the same way omp generates its own.

---

## Sources

Field measurements taken 2026-08-15.

- [Zed 1.0 review — GPUI, multibuffer, latency](https://chatforest.com/reviews/zed-1-0-ai-code-editor-parallel-agents-rust-review/)
- [Zed editor guide 2026 — startup and memory figures](https://baeseokjae.github.io/posts/zed-ai-guide-2026/)
- [Sublime Text — Goto Anything, minimap, multiple selections](https://docs.sublimetext.io/guide/usage/editing.html)
- [VS Code tips and tricks — command palette, multi-cursor, peek](https://code.visualstudio.com/docs/editing/tips-and-tricks)
- [Most used IDEs 2026 — VS Code at 75.9%](https://www.secondtalent.com/resources/most-used-ides/)
- [Helix plugin system PR #8675 — Steel, still unmerged](https://github.com/helix-editor/helix/pull/8675)
- [TermIDE — feature surface, 38 themes, 22 languages](https://termide.github.io/)
- [The TUI renaissance 2026 — terminal capability baseline](https://www.youngju.dev/blog/culture/2026-05-14-tui-development-ratatui-bubbletea-ink-textual-terminal-ui-renaissance-deep-dive-2026.en)
- [ratatui-image — Kitty, iTerm2, Sixel, halfblock fallback](https://github.com/ratatui/ratatui-image)
- [Neovim #7479 — styled and colored undercurl in terminals](https://github.com/neovim/neovim/issues/7479)
- [oh-my-posh — `font install`, interactive selector, per-privilege install location](https://ohmyposh.dev/docs/installation/fonts)
- [oh-my-posh font management internals](https://deepwiki.com/JanDeDobbeleer/oh-my-posh/6.5-font-management)
- [oh-my-pi discussion #7808 — what a Nerd Font is, and why it is a tip rather than a dependency](https://github.com/can1357/oh-my-pi/discussions/7808)
- [Nerd Fonts discussion #829 — no general way to detect glyph support programmatically](https://github.com/ryanoasis/nerd-fonts/discussions/829)
- [ucs-detect — Unicode width detection by cursor-position report](https://pypi.org/project/ucs-detect/1.0.1)
- oh-my-pi source, read at `main`: `packages/coding-agent/src/modes/theme/symbols.ts` (symbol presets), `packages/coding-agent/src/modes/theme/theme.ts` (tiered light/dark detection), `packages/tui/src/terminal.ts` (OSC 11, mode 2031, per-terminal workarounds), `packages/coding-agent/src/modes/components/welcome.ts` (the tip), [`can1357/oh-my-pi`](https://github.com/can1357/oh-my-pi)
