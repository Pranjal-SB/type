---
type: design
status: living
area: audit
verified: 2026-09-03
verified-against: v0.3.0
---

# Gap analysis — TYPE against itself and against the field

**Status:** living document · **Written at:** v0.2.1 · **Re-verified at:** v0.3.0 · **Date:** 2026-09-03

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
| ~~54~~ | MED | **Fixed at v0.3.0 — it runs all nine.** ~~**The weekly perf tripwire runs two of the seven perf test files.**~~ `.github/workflows/perf.yml` runs `typ-buffer/tests/perf.rs` and `typ-panel-editor/tests/perf.rs`. It does not run `typ-app/tests/perf.rs`, `typ-app/tests/perf_startup.rs`, `typ-find/tests/perf.rs`, `typ-find/tests/perf_fs.rs` or `typ-panel-editor/tests/perf_startup.rs`. The workflow's own header names "sub-100 ms cold start" as the identity it exists to protect, and `cold_start_stays_under_a_tenth_of_a_second` lives in `perf_startup.rs` — one of the five it skips. The tripwire does not cover the number it was written for, and a file added since is invisible to it by default rather than by decision. | `.github/workflows/perf.yml` | M3, Task 15 |
| 55 | LOW | **`AGENTS.md` says the perf tests are not scheduled, and they are.** "CI does **not** run the perf tests — they are `#[ignore]`d and nothing is scheduled, so a budget regression is caught only when a human remembers to look." `perf.yml` has run on a weekly cron since it was added for defect 18. The sentence is the kind of stale instruction that makes a reader distrust the rest of the file, and it sits in the document that is meant to be the source of truth. | `AGENTS.md`, budgets section | M3, Task 16 |
| 56 | MED | **`architecture.md`'s verified marker is six releases behind the tree.** Frontmatter reads `verified: 2026-08-22`, `verified-against: v0.2.4`, and the dateline says "last verified against the tree 2026-08-22, on the unreleased M2.5 branch". The tree is v0.2.10. `docs/README.md` states the rule this breaks — "Kept true against the tree… Each carries a `verified` date saying when that was last checked" — and this is the document that rule matters most for, since every other doc defers to it as the spec. §5's stack table was corrected at v0.2.10 against the code, but a partial check cannot honestly move a whole-document marker, so the marker stays wrong until someone reads the whole thing. | `docs/design/architecture.md:5-12` | unowned |
| 44 | LOW | **`architecture.md` §5 lists crates that do not exist and will not.** `typ-config` and `typ-ui` are in the 14-crate layout; neither was built, and the reasoning for `typ-config` — that the seam falls elsewhere, parsing sits with its type — lives only in `docs/plans/`, which is gitignored. A reader of the published spec sees a layout the tree contradicts with no explanation. The other absent crates (`typ-syntax`, `typ-lsp`, `typ-git`, the two panel crates) are forward-looking and fine; these two are decisions. | `docs/design/architecture.md:234` | unowned |
| 61 | MED | **Starting a language server costs 8 ms on the thread that asks for it.** `Client::start` does `CreateProcess`, a job object and three thread spawns synchronously; measured best-of-five at 8.1 ms on Windows in `typ-lsp/tests/perf_proc.rs`. Invariant 7 says subprocess work goes off-thread, and this is subprocess work — so this is an invariant violation and not merely a slow path. What keeps it out of v0.3.0's blocking list: it happens once per server per session, and it lands on the frame that *opens a file* rather than on a keystroke, which is a frame already paying for a read, a rope build and a watch. The fix is an asynchronous `Client::start` — spawn on a thread, deliver the transport as an event — which is a transport API change and not a thing to do in a release commit. | `typ-lsp/src/transport.rs` `spawn`, `typ-app/src/lsp/mod.rs` `start` | v0.3.1 |
| 62 | LOW | **`App::notify_server_for_test` is test-shaped production API.** Task 13 needed a way to end a progress token at a chosen moment rather than racing the fake server. It is `pub` on `App` and named for the only thing that calls it. Either the fake server grows a flag that ends the token on a notification it already receives, or the method becomes a `pub(crate)` seam a test module reaches. | `typ-app/src/app.rs` | v0.3.1 |
| 63 | LOW | **No `workspaceFolders` in the handshake.** TYPE sends the deprecated `rootUri` only. rust-analyzer reads `workspaceFolders` and falls back to `rootUri` (`crates/rust-analyzer/src/session.rs`), and `Transport::spawn` sets the child's `current_dir` to the root as well, so nothing is broken today — but `rootUri` has been deprecated since LSP 3.6 and a server that drops it is within its rights. One field, best added when multi-root arrives with splits. | `typ-lsp/src/client.rs` `initialize_params` | M4 |
| ~~64~~ | MED | **Fixed at v0.3.0.** ~~**The installer's checksum test fails one run in sixteen.**~~ `tests/install_test.sh` corrupted a digest with `sed 's/^[0-9a-f]/0/'`, which is a no-op when the digest already starts with `0` — and `tar czf` stamps an mtime into the gzip header, so the digest is different on every run and the dice were rolled every time. The test then saw a *correct* install succeed against what it believed was a bad checksum and reported failure. Found because it fired on the M3 pull request, whose diff does not touch the installer. A security-relevant check going red at random on unrelated work is how a check gets ignored, which is the argument `perf.yml`'s own header makes about gates. `sed 's/^0/1/;t;s/^[0-9a-f]/0/'`, verified over 200 random digests. | `tests/install_test.sh` | v0.3.0 |
| ~~65~~ | **CRITICAL** | **Fixed at v0.3.1.** ~~**Every shipped binary ran with its workers disconnected.**~~ `event_loop` created the event channel, gave one end to the input pump and never gave the other to `App`, so `parse_worker`, `find_worker` and `sender` were all `None` in production — `set_event_sender` was called only by tests. From M2.7 to v0.3.0 that meant **no syntax highlighting, no picker corpus, no project search, no external-change reload and no language servers** in the binary, all of them fully tested and completely dead. Found by opening `typ` on this repository and seeing unhighlighted code. The tests could not see it because every one of them wires the app by hand, which is the one thing the binary did not do — `parse_wiring.rs`'s own header says it exists because "a wiring mistake passes all of them", and it was one layer too low. The wiring is now `run::wire`, a named function with a test that asserts an app comes out of it connected. | `typ-app/src/run.rs` `event_loop` | v0.3.1 |
| ~~66~~ | HIGH | **Fixed at v0.3.1.** ~~**A file opened from the command line reached no language server until a key was pressed.**~~ `sync_language_servers` runs at the end of `step_batch`, and the loop draws one frame and then blocks on `recv()` — so with the channel wired but no input, nothing was ever announced. `run::wire` now syncs once before the loop blocks, which is what `set_event_sender` already did for the parse. | `typ-app/src/run.rs` | v0.3.1 |
| ~~57~~ | **HIGH** | **Fixed at M3.** ~~**A form feed makes every line number below it wrong.**~~ ropey's `unicode_lines` feature is on by default and makes `U+000B`, `U+000C`, `U+0085`, `U+2028` and `U+2029` line breaks, per Unicode Annex #14 — right for text layout, wrong for a code editor. rust-analyzer's `lib/line-index` breaks on a line feed and nothing else; so does ripgrep, which `typ-find` has used since M2.8; so does git, which M5 needs. Reproduced: `a\x0Cb\n` was two lines to the buffer and one to everything it talks to, so a project-search hit below a form feed already jumped to the wrong line **before M3 existed** — this is an LSP-shaped bug that was never only about LSP. Fixed by pinning `ropey = { default-features = false, features = ["simd"] }`, the line Helix pins for the same reason; `simd` has to be restored by hand, which ropey's own docs call a footgun. `cr_lines` goes with it, so a bare CR is content — `LineEnding` has only ever modelled LF and CRLF, so the rope now agrees with the type beside it. | `Cargo.toml`, `typ-lsp/src/position.rs` `content_len` | M3 |
| ~~58~~ | MED | **Fixed at M3.** ~~**The client read `publishDiagnostics.version` without declaring `versionSupport`.**~~ TYPE drops a publish describing a version older than one already sent, which is what the spec defines the capability as meaning. Declaring it is not decoration: reading the field without declaring it is the same class of lie as declaring it and ignoring the field. | `typ-lsp/src/client.rs` `initialize_params` | M3 |
| 59 | MED | **Declaring `textDocument.diagnostic` would turn rust-analyzer's fast diagnostics off.** `main_loop.rs` guards `update_diagnostics` — the **native** set, the errors that appear as you type — on `!config.text_document_diagnostic()`, which reads the *client* capability. So a client that declares pull support and does not implement it well loses the fast half it already had by push. TYPE does not declare it, and a test asserts the absence with the reason attached, so the tripwire exists. The row stays open because it is a live constraint rather than a defect: the capability and a working pull path land in the same commit or neither does. **Pull diagnostics were cut from v0.3.0 on the strength of this** — push already carries rust-analyzer's fast half, and the capability cannot ship ahead of the implementation — so this row is what v0.3.1 has to satisfy before it declares anything. | `typ-lsp/src/client.rs`, `typ-lsp/tests/client.rs` | v0.3.1 |
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

## Part 8: The v0.3.0 audit

**Date:** 2026-09-03 · **Read at:** `e1998d8` · **Method:** nine parallel readers over the whole
tree, then every claim re-checked by hand. Where a defect could be demonstrated rather than
argued, a throwaway test was written, run, and the failure pasted into the row; those rows say
**proven**. The rest were confirmed by reading the code and its callers.

Two things about the method are worth keeping, because both changed a conclusion:

- **A repro that passes is not an absent bug.** Gap 67 first came back green because the repro
  closed the *last* tab, whose index nobody reuses. The defect needs a middle tab. A failed
  reproduction disproves the reproduction before it disproves the claim.
- **Shell semantics get tested, not reasoned about.** `install.sh`'s `set -e` inside a `for`
  loop with an `&&` body looked like an early exit and is not: POSIX exempts a failure that is
  not the last command of an AND-list. Two minutes of `dash` settled what an afternoon of
  argument would not have.

### Data loss

| # | Sev | Defect | Where | Lands |
|---|---|---|---|---|
| 67 | **CRITICAL** | **A confirmed tab close arms the tab that takes its index.** `close_pending` is a raw `usize` and `close_tab` never clears it; `handle_chord` deliberately skips `clear_transient` for `CloseTab`, so nothing else does either. Close a dirty *middle* tab properly, ask, confirm, and `most_recently_used` slides another tab into that index while `close_pending` still names it. One more Ctrl+W discards it with no prompt. The status bar even still names the tab that is already gone. The mouse path needs one fewer gesture: two clicks on a close box, then one on the tab that replaced it. **Proven:** `one Ctrl+W discarded unsaved work in c.rs * without asking; status = Some("… Close b.rs again to discard …")`. | `typ-app/src/app/tabs.rs:139` `close_tab`, guard at `:194` | v0.3.1 |
| 68 | **CRITICAL** | **`quit` and `close_tab` can never complete from the command palette.** `apply_named_action` calls `clear_transient()`, which sets `quit_pending = false` and `close_pending = None`, and *then* falls through to `request_quit()`/`request_close_tab()`, which arm exactly those flags. Every invocation clears the confirmation it is about to set, so on a dirty buffer the palette offers "Ctrl+Q again to discard" forever and Enter never quits. **Proven:** `quit never completes from the palette; status = Some("Unsaved changes. Close anyway? …")`. Masked by gap 128. | `typ-app/src/app.rs:797` `apply_named_action` | v0.3.1 |
| 69 | HIGH | **A cloned repository can drive the user's terminal.** `typ-find`'s `hit_of` trims a trailing newline and clips length and filters nothing; `BinaryDetection::quit(b'\x00')` quits on NUL only, and `0x1b` is valid UTF-8. The bytes reach `Cell::set_symbol` at two sites and `backend.rs` `Print`s them verbatim. ratatui strips control characters in `Span::styled_graphemes` and `Buffer::set_stringn` and **not** in `set_symbol`, its own source says "those are low-level APIs where the caller is responsible", and these two call sites are the only non-literal callers in the workspace. A file named `README\x1b]52;c;<base64>\x07.md` is a legal POSIX filename that git checks out; the victim clones, runs `typ .`, presses the picker key, and OSC 52 writes the attacker's text into their clipboard. No query typed, no file opened. Default-enabled in kitty, foot, WezTerm, Alacritty and tmux with `set-clipboard on`. The grep variant reaches Windows, where filenames cannot hold `0x1b` but file *contents* can. **Proven:** `raw control characters landed in 2 cells: [(7, 3), (22, 3)]`. ratatui's `debug_assert!` in `cell_width` does **not** fire on this path, so there is no safety net in debug builds either. | `typ-picker/src/render.rs:289` and `typ-app/src/tabbar.rs:144`, source at `typ-find/src/search.rs:187`, sink at `typ-app/src/backend.rs:257` | v0.3.1 |
| 70 | HIGH | **An external write of non-UTF-8 over an open file exits the editor and takes every other tab's unsaved work.** `handle_external_change` ends in `panel.reload()?`; `TextBuffer::from_path` uses `read_to_string`. The `?` runs to `step_batch(…)?` → `event_loop` → `run` → `ExitCode::FAILURE`. `git checkout` of a branch where that path is a binary is enough, and the trigger is a watcher event rather than anything the user did. | `typ-app/src/app.rs:702` | v0.3.1 |
| 71 | HIGH | **Pressing Enter on a binary file in the tree does the same.** `apply` propagates `open_path`'s error for `PanelEvent::OpenFile`, and `TreePanel::activate` emits that event for any non-directory entry. Also reachable from the picker. `jump_to_definition` at `app.rs:560` already handles this correctly with a status message, `apply` never got the same treatment. | `typ-app/src/app.rs:1109`, `:1120` | v0.3.1 |
| 72 | HIGH | **`Selections`' non-overlapping invariant is false, and the consequence is a silent wrong-range edit.** `overlaps` tests `a_end > b_start` strictly, then falls back to a clause requiring *both* sides empty. A caret at `P` sorts before the selection `[P, Q)` it sits at the start of, `range()` gives `(P,P) < (P,Q)`, so `a` is the caret, `b` is non-empty, and neither clause fires. Reachable today: double-click a word, Alt+click its first cell (`typ-panel-editor/src/lib.rs:818` is an unconditional `push`). `edit_at_each_selection` then applies ascending through a `Shift`: the caret inserts, records `cols += 1`, and the selection is shifted to `[P+1, Q+1)`, replacing text one grapheme right of what was selected. **Proven:** `two selections both cover column 5`. | `typ-buffer/src/selection.rs:158` `overlaps` | v0.3.1 |
| 73 | HIGH | **Merging discards selection direction.** `union` is hardcoded `anchor: min(start), head: max(end)`, so two backwards selections come out forwards. Alt+click two carets, hold Shift+Left: at the press where they merge the head teleports to the far end and the next press shrinks the selection instead of extending it. **Proven:** `merged forwards: anchor=col 1 head=col 8`. | `typ-buffer/src/selection.rs:173` `union` | v0.3.1 |

### Broken in ordinary use

| # | Sev | Defect | Where | Lands |
|---|---|---|---|---|
| 74 | HIGH | **The picker's mouse wheel does nothing.** `scroll()` moves `offset` and deliberately leaves `selected` alone; `render::draw` calls `visible()` on every frame, which calls `scroll_into_view()`, which snaps the offset back to keep the selection on screen. One wheel notch is undone by the paint it triggers. `TreePanel` does not have this bug because its render does not force the selection into view, which is the inconsistency that gives it away. **Proven:** `wheel put offset at 3; the next paint reset it to 0`. Masked by gap 130. | `typ-picker/src/lib.rs:287` `scroll`, undone at `:250` `visible` | v0.3.1 |
| 75 | HIGH | **Clicking the file tree's title bar opens the first file.** `handle_mouse` computes `event.row.saturating_sub(inner.y)` with no check that the click is inside the list, so the frame's top border saturates to row 0. On a fresh tree `selected == top_line == 0`, so the branch that activates the already-selected entry runs. A click on the bottom border selects one row past the last visible entry. The column is never checked either. `Picker::row_at` does exactly the guard this lacks. **Proven:** `clicking the title bar expanded sub/, it acted as a click on list row 0`. Untestable until gap 129 is fixed. | `typ-panel-tree/src/lib.rs:263` | v0.3.1 |
| 76 | HIGH | **One language server's answer can run another's handler.** `take_pending` finds a `Pending` by `RequestId` alone, ignoring the `ServerId` it stores. Each `Client` numbers from `next_id = 0`, so rust-analyzer and taplo in one repository both issue `RequestId(2)`. Alt+D in a `.rs` file and Alt+H in `Cargo.toml` collide: taplo's hover payload is handed to the definition handler. `cancel()` ten lines above *does* carry `p.server`, which makes this an omission rather than a decision. A server that exits with a request in flight leaves its `Pending` behind and widens the window. | `typ-app/src/lsp/mod.rs:751` | v0.3.1 |
| 77 | HIGH | **A reopened file is announced to the server with `didChange` for a document it has closed.** `close_absent` sends `didClose` and sets `doc.synced = None` but leaves the `Doc` in `docs`; the only removal anywhere is `docs.retain` on server exit. On reopen `sync_one` therefore takes the `Some(doc)` arm, because `None != Some(revision)`. rust-analyzer logs "unexpected DidChangeTextDocument" and drops it, so that file has no pushed diagnostics for the rest of the session and nothing will re-announce it. `did_open` is only ever called from the `None` branch. | `typ-app/src/lsp/mod.rs:507`, closed at `:564` | v0.3.1 |
| 78 | HIGH | **The client advertises `window.workDoneProgress` and then answers none of the requests that capability invites.** `Lsp::handle` answers `workspace/configuration` and passes everything else up to `app.rs`, where `ServerRequest { .. } => false` drops it. `Client::respond_error` has zero callers in the workspace. The spec forbids leaving a request unanswered, and the server's outgoing queue grows one entry per progress token. Masked by gap 132. | `typ-app/src/app.rs:396`, capability at `typ-lsp/src/client.rs:433` | v0.3.1 |
| 79 | HIGH | **A terminal paste while the picker is open edits the buffer behind the overlay.** `handle_paste` guards against an open prompt and not against an open picker, so the text goes to `Action::Paste` on the focused panel. Pasting a path into a fuzzy finder is an ordinary gesture. This is the exact failure `handle_chord` documents at `app.rs:934` for keys, "otherwise typing a filename fires every editing action bound to a letter", and the paste path never got the same guard. | `typ-app/src/app.rs:1006` | v0.3.1 |
| 80 | HIGH | **Undo marks a buffer dirty even when it restores the file on disk.** `undo()` and `redo()` both call `touch()` unconditionally. Undo back to the saved state and Ctrl+Q still challenges you over a file you have not changed. No save-point revision is tracked; every mature editor tracks one, which by the prior-art rule makes its absence a gap rather than a simplification. | `typ-buffer/src/buffer.rs:521`, `:527` | v0.3.1 |
| 81 | HIGH | **A no-op edit destroys the redo stack.** `begin_edit_group` takes its snapshot before knowing whether the group mutates anything, and `History::record` opens with `self.redo.clear()`. `replace_range` early-returns on a no-op, but the group has already fired. So Backspace at (0,0), which visibly does nothing, throws away a pending redo and pushes a phantom undo step that restores an identical rope. Delete at end-of-buffer is the same. Every other group call site guards first: `shift_lines` on `deltas.is_empty()`, `replace_all` on `hits.is_empty()`, Cut and Paste on empty text. `edit_at_each_selection` is the only unguarded one. | `typ-buffer/src/buffer.rs:494`, `typ-buffer/src/undo.rs:77`, unguarded caller at `typ-panel-editor/src/actions.rs:206` | v0.3.1 |
| 82 | MED | **`set_selections` is documented "preserving order and the primary" and preserves neither.** It calls `set_single(first)` then `push`es the rest, and `push` sets `primary = len - 1` before normalizing, so the primary always ends as the document-last selection. Every `Action::Move` routes through it. `add_cursor_above` makes the new upper caret primary; the first arrow press moves the terminal cursor to the bottom-most caret instead, and `scroll_to_cursor` follows it. `Selections::map_in_place` already preserves the primary by identity and has zero production callers. | `typ-panel-editor/src/actions.rs:167` | v0.3.1 |
| 83 | MED | **The event loop cannot exit when the input pump dies.** `run.rs`'s comment says "every sender is gone only when the pump thread has died", but `wire` gives `App` a sender clone and `App` outlives `event_loop`, so `rx` never disconnects. `spawn_input_pump` exits when `event::read()` returns `Err`, a closed tty, EOF on the input source, and at that point no sender will ever fire again and `recv()` blocks forever. Invariant 10's "exits with an honest code" fails in exactly the case where the terminal has gone away. | `typ-app/src/run.rs:145` | v0.3.1 |
| 84 | MED | **Clicking the status bar moves the caret.** Mouse routing tests only `m.column < tree_area.width` and never bounds-checks `m.row` against either panel rect. `render.rs` claims the split "excludes the status bar row, so a click on it hits neither panel"; nothing enforces that claim, and `EditorPanel`'s hit test has no upper guard either. Measured on a 100×30 frame: a click at row 29 moved the cursor to line 28. `route_tab_bar_mouse` *does* check its row, which is why the tab-bar row is safe and this one is not. Same class as gap 75, one layer up. | `typ-app/src/run.rs:345` | v0.3.1 |
| 85 | MED | **A background tab is never re-checked against disk.** `rewatch` holds one `FileWatch` for the active tab, and `handle_external_change` returns early for any path that is not the active tab's. `settle_active_tab` re-watches on a switch but never consults `matches_disk()`. Change a file in another program, switch back to its tab, and it shows stale contents with no warning, and Ctrl+S overwrites the other writer silently. The whole external-change guard predates tabs. | `typ-app/src/app/tabs.rs:228`, `typ-app/src/app.rs:675` | v0.3.1 |
| 86 | MED | **A clipped tab paints a close box the hit test says does not exist.** `write_cell` breaks the name at `x + 2 >= end` and then writes the trailing space and `×`, so a tab clipped to its full width still gets a close glyph, while `close_box_x` returns `None` for it. `route_tab_bar_mouse` therefore falls to the `else` and *activates* the tab. No test binds `draw`'s output columns to the `cells()` rectangles the hit test uses; the file's own header calls the agreement "true by construction". | `typ-app/src/tabbar.rs:138`, hit test at `typ-app/src/app/tabs.rs:284` | v0.3.1 |

### Performance budgets

Measured `--release`, best-of-five, against the 16 ms keystroke-to-painted-glyph budget.

| # | Sev | Defect | Where | Lands |
|---|---|---|---|---|
| 87 | HIGH | **Every motion and every edit is O(N² log N) in the cursor count.** `set_selections` rebuilds the set with N calls to `Selections::push`, and each `push` runs a full `normalize()`, sort, merge, `Vec` rebuild. `Action::Move` costs 3 µs / 331 µs / 8.5 ms / 28 ms / **110 ms** at 1 / 100 / 1000 / 2000 / 4000 cursors. Real trigger: Ctrl+Shift+L on a 50k-line file is 73 ms, and one arrow key afterwards is 95 ms. Not the rope, `Move` touches no text and costs the same as `InsertChar`. Not `find_all`, which is 7 ms of the 73. `Selections::map_in_place` does the same job with one normalize, has a test, and has no production callers. | `typ-panel-editor/src/actions.rs:168`, four call sites | v0.3.1 |
| 88 | HIGH | **A frame costs O(visible cells × *total* selections).** `paint_for` scans the whole selection set for every drawn cell with no viewport pre-filter, so cursors that are nowhere near the screen still cost paint time: 575 µs / 1.4 ms / 6.0 ms / 11.7 ms / **24 ms** at 1 / 100 / 1000 / 2000 / 4000. Over budget on paint alone past ~2500 cursors, before any keystroke work. The selections are document-ordered, so the filter is a binary search and a slice. | `typ-panel-editor/src/render.rs:479` | v0.3.1 |
| 89 | MED | **`styled_line` builds spans for the whole line, not the visible part.** The grapheme loop has no `ctx.width` bound, so a long line is walked and turned into `Span`s in full and then clipped by `Paragraph` at draw time. Per frame: 140 µs at 1k chars, 557 µs at 10k, 8.6 ms at 100k, **30 ms at 500k**. A minified JS or CSS bundle is one such line and `MAX_HIGHLIGHTED_BYTES` does not exclude it, so every keystroke drops frames. Every perf fixture in the tree uses 65-character lines. | `typ-panel-editor/src/render.rs:238` | v0.3.1 |

### Silent failure

The rule this section is measured against is AGENTS.md's own: **nothing below `typ-app` can log**,
so a lower crate that hits a problem returns the reason as data or the reason is gone for good.

| # | Sev | Defect | Where | Lands |
|---|---|---|---|---|
| 90 | HIGH | **A worker-thread panic tears down the terminal while the editor keeps running.** `std::panic::set_hook` is process-wide and runs on whichever thread panicked. TYPE spawns six, parse, find, LSP read/write/stderr, and the input pump. A panic on any of them leaves raw mode, leaves the alternate screen, and then the main loop carries on drawing into a cooked terminal: keys no longer reach the app and every dirty buffer is unreachable. The hook's own comment reasons only about the main thread, which is the case it does fix. | `typ-app/src/run.rs:56` | v0.3.1 |
| 91 | HIGH | **A dead parse or find worker is never noticed.** `ParseWorker::request` does `if jobs.send(pending).is_err() { self.jobs = None; }` and returns nothing, and the app then records `parsed_revision` and `awaited_generation` for a job that was never delivered. On the next edit `request()` returns early *without incrementing the generation*, so the awaited number is re-stamped and will never arrive. Highlighting is dead for the session and `App::is_wired()` still answers true, because it only checks `parse_worker.is_some()`. The find worker has the identical shape and leaves the picker empty forever. The two `panic!`s in `language.rs` are how the thread dies: query compilation is not linker-checked, and a `tree-sitter-*` bump shipping a predicate tree-house does not know is exactly the case. `ParseError` already exists for returning this as data. | `typ-syntax/src/worker.rs:146`, `typ-syntax/src/language.rs:163`, `:171`, `typ-find/src/worker.rs:225` | v0.3.1 |
| 92 | MED | **The LSP writer thread exits silently and every later notification is dropped.** When the write breaks the thread breaks out of its loop, but `outgoing` stays `Some`, so `send_deferred`'s `let _ = tx.send(…)` fails forever into nothing. If the server's stdin breaks while its stdout stays open the reader never sends `Closed`, so the client believes the server is healthy: the user types, `didChange` goes nowhere, and diagnostics describe the file as it was minutes ago while the notification counter still climbs. | `typ-lsp/src/transport.rs:166`, `:236` | v0.3.1 |
| 93 | MED | **A server that refuses `initialize` is recorded as having completed one.** `finish_initialize(response_result.ok())` throws away the `ResponseError`, and `awaiting_initialize` was already cleared, so `is_initialized()` answers true with `capabilities: None`. `ever_ready` is set from exactly that, so `after_exit` classifies the eventual death as "exited" rather than "did not start" and burns all four restarts. Meanwhile `ask` passes the initialized check and fails the capability check, so the status bar says "This language server does not offer that" about a server that actually refused the workspace. | `typ-lsp/src/client.rs:137` | v0.3.1 |
| 94 | MED | **File-watch failures are discarded.** `notify` delivers watch errors through the same channel as events, inotify limit exhaustion, the watched directory removed, a rescan-required overflow, and `let Ok(event) = event else { return }` drops every one. The module's own header says the watch exists so that "the next save silently overwrites whatever the other writer did"; after this branch fires that is exactly what happens, and `typ-buffer` cannot log. | `typ-buffer/src/watch.rs:43` | v0.3.1 |
| 95 | MED | **Two discarded Win32 return values can leave an orphaned `rust-analyzer` tree.** `SetInformationJobObject` and `AssignProcessToJobObject` both return `BOOL` and both are ignored. If the assignment fails, the child is already in a job that refuses nesting, which is the normal state under some CI runners and container hosts, the job handle is still non-null, so `kill_tree` calls `TerminateJobObject` on an empty job and **never reaches the `child.kill()` fallback**. TYPE exits; rust-analyzer and its `cargo`/`rustc` subtree keep running. That is the precise failure the job object exists to prevent, arriving silently. Found independently by two readers. | `typ-lsp/src/transport.rs:375-394` | v0.3.1 |
| 96 | MED | **One unreadable expanded directory stops the whole tree from ever refreshing.** `collect` uses `?` on each `read_dir` and `rebuild` propagates before assigning `self.entries`. Individual entries are tolerated with `filter_map(|e| e.ok())`; a directory is not. Expand a folder you cannot read, routine on Windows under `C:\Users`, and because the path is now permanently in `expanded`, every later expand or collapse of any other directory fails too. | `typ-panel-tree/src/lib.rs:69` | v0.3.1 |
| 97 | MED | **Every config read error is treated as "there is no config".** The comment is right about `NotFound` and wrong about everything else: a permission problem, a directory where a file should be, or a file Notepad saved as UTF-16 all read as absent. The user's bindings silently do not apply and the status bar, which exists to collect exactly these complaints, and is already wired for it, says nothing. | `typ-app/src/config/keys.rs:13`, `typ-app/src/config/settings.rs:85` | v0.3.1 |
| 98 | MED | **An unparsable search pattern is reported as a complete search with no matches.** `complete: true` is an affirmative claim that the project contains none. The query is a regex and nothing on screen says so, so searching for a literal `foo(` returns a confident "nothing here". `Search` already carries a flag for qualifying its own answer. | `typ-find/src/search.rs:74` | v0.3.1 |
| 99 | MED | **A failed save leaks its temp file.** The write-and-sync step returns through `?` with no cleanup, while the two error paths after it both remove the temp first. Disk full or a read-only directory leaves `.main.rs.4212.typ-tmp` beside the source; it is not gitignored and a dot-prefix does not hide it from `git status`. The buffer stays dirty and the error does reach the status bar, so this is hygiene rather than lost work. | `typ-buffer/src/buffer.rs:559` | v0.3.1 |
| 100 | MED | **A mis-spelled key chord in `keys.toml` is accepted and never fires.** `merge_toml` validates the action name loudly and never validates or canonicalizes the chord string, while `lookup` matches against `KeyChord::canonical`, lowercase, in a fixed `ctrl+alt+shift+` order. So `"Ctrl+S"`, `"shift+ctrl+p"`, `"C-s"` and `"pgup"` all load without complaint and do nothing. There is no chord parser anywhere in the workspace. | `typ-core/src/keymap.rs:351` | v0.3.1 |
| 101 | MED | **A refused LSP request costs a keypress and says nothing.** Every neighbouring arm sets `self.status`; this one logs and returns. The error's code and message are discarded before the log line, and `TYP_LOG` is unset by default, so in the shipped configuration it writes nowhere at all. From the keyboard it is indistinguishable from an unbound key. | `typ-app/src/app.rs:510` | v0.3.1 |
| 102 | LOW | **A `publishDiagnostics` payload that will not parse loses both the reason and the sender.** `server` is in scope and not logged; the serde error is dropped entirely. If a server's payload shape is one field off, every diagnostic it will ever publish disappears and the log line names neither the server nor the field. | `typ-app/src/app.rs:422` | v0.3.1 |

### The save path

| # | Sev | Defect | Where | Lands |
|---|---|---|---|---|
| 103 | MED | **The comment says the file is never briefly world-readable, and the code makes it so.** Order is `File::create`, mode `0o666 & ~umask`, so typically 0644, then write the entire rope, then `sync_all`, and only *then* `copy_permissions`. For the whole duration of the write a `0600` file's full contents sit on disk readable by anyone. Editing `~/.ssh/id_rsa` or a `.env` on a shared box leaks it in that window. `save_fidelity.rs` asserts the *final* mode is `0o600`, which is true, and is why this was never caught. | `typ-buffer/src/buffer.rs:559-567` | v0.3.1 |
| 104 | MED | **The temp file is created with no `O_EXCL` and no `O_NOFOLLOW`, at a fully predictable path.** `File::create` opens `O_CREAT|O_WRONLY|O_TRUNC`, and the name is the filename plus the pid, with `pid_max` at 32768 by default, an attacker with write access to the directory can pre-create all of them. Pre-planted as a symlink to `~/.ssh/authorized_keys`, the save writes through it, `copy_permissions` chmods the real target, and `rename` (which does not follow symlinks) leaves the user's file a broken link. Needs concurrent directory write access, so not reachable from a clone alone. | `typ-buffer/src/buffer.rs:640`, name at `:629` | v0.3.1 |

### Display width

| # | Sev | Defect | Where | Lands |
|---|---|---|---|---|
| 105 | MED | **Display width is computed by counting characters at five sites, while a correct helper exists and is used at two others.** `typ_buffer::display_width` is `UnicodeWidthStr`-based and handles tabs; `tabbar.rs` and `panel-editor/render.rs` use it. The status bar sums `s.text.chars().count()`, the hover box measures its longest line the same way, the picker uses grapheme counts as cell widths, and `chrome.rs` measures a panel title that way. A CJK filename is one grapheme and two cells, so the status bar's right segments are under-measured and `1:1  33%` is pushed off the end. Untestable as written: `frame.rs:246` asserts `status.chars().count() == 60`, measuring with the same wrong ruler, and `typ-picker/tests/` has no wide-character case at all. | `typ-app/src/app/render.rs:136`, `:145`, `:209`; `typ-picker/src/render.rs:174`, `:223`; `typ-core/src/chrome.rs:102` | v0.3.1 |
| 106 | MED | **The hover box's width arithmetic overflows on server-controlled input.** `(longest as u16 + 2)` where `longest` is the longest line of the server's hover text, which is unbounded. At ≥65534 characters this is an overflow panic in a debug build and wraps to a 3-cell box in a release one. rust-analyzer emits very long single lines on macro-heavy code. | `typ-app/src/layout.rs:69` | v0.3.1 |

### Find and rank

| # | Sev | Defect | Where | Lands |
|---|---|---|---|---|
| 107 | MED | **A search stopped by the cap at exactly the limit reports itself complete.** The cap fires at `found.len() >= limit` and quits the whole walk, but `complete` is derived as `hits.len() <= limit`, so only an *overshoot* is reported. Ten matches in the first file and hundreds after it gives "Search  10 matches" with no `+`, which is the exact lie the suffix exists to prevent. | `typ-find/src/search.rs:157` | v0.3.1 |
| 108 | MED | **The per-file sink has no cap, so one large file materialises every matching line before the limit is consulted.** `capped` is checked before a file starts and the sink always returns `Ok(true)`. Any repository with a large generated file or lockfile: open project search, type `e`, and that file alone produces hundreds of thousands of `LineHit`s, each with an owned path plus up to 512 bytes, on the find worker, per keystroke, with several files in flight. The header's claim that "the overshoot is bounded by one file" is true and is not a bound. | `typ-find/src/search.rs:120` | v0.3.1 |
| 109 | MED | **The open-buffer override never fires in the `$EDITOR` case.** Overrides are matched by raw `PathBuf` equality against the walker's entries. `typ notes.md` sets the root to `.`, so `ignore` yields `./notes.md` while the tab holds `notes.md`, and `Path`'s `Eq` compares components and keeps a leading `CurDir`. Edit without saving, search for what you just typed, and the file is read from disk instead. Silent, the override simply does not fire. Both existing tests use an absolute root, where the two spellings always agree. `tab_for` already canonicalises for its own comparison, so the codebase knows about this. | `typ-find/src/search.rs:127` | v0.3.1 |
| 110 | LOW | **Scoring still goes through `Utf32Str::new`, which is the trap the file's own doc comment documents and fixes for `indices` only.** `match_list` builds its haystacks internally, so for a decomposed path the score is computed over raw UTF-8 bytes while `indices` uses the corrected grapheme haystack. A query containing a non-ASCII character can fail to rank a path whose accent is decomposed. Highlighting itself can never be wrong, because `indices` re-matches against the haystack it is handed. | `typ-find/src/rank.rs:55` | unowned |

### Threading and lifecycle

| # | Sev | Defect | Where | Lands |
|---|---|---|---|---|
| 111 | MED | **The parse worker coalesces queued jobs unconditionally, but jobs from different tabs are not supersessions.** The comment, "anything already queued behind this job describes newer text", held when there was one buffer. With tabs each records its own `awaited_generation`, so two requests queued while the worker is mid-parse collapse to one, the dropped generation never arrives, and the losing tab keeps `parsed_revision = Some(rev)` so `request_parse_if_stale` returns early forever. That tab renders unhighlighted for the rest of its life. All four tests for this state pump the previous parse to completion first, so two jobs are never queued together. | `typ-syntax/src/worker.rs:99` | v0.3.1 |
| 112 | MED | **`did_open` serialises the whole rope on the render thread, on the cold-start path.** `wire()` calls `sync_language_servers()` before the loop reaches its first draw, and `sync_one`'s `None` branch does `snapshot.rope.to_string()`, the 1.269 ms/50k-line cost `lsp.md` says was moved off the render thread. `did_change` and `did_save` use `send_deferred`; `did_open` does not. Companion to gap 61, which covers the spawn. | `typ-app/src/lsp/mod.rs:530`, `typ-lsp/src/client.rs:223` | v0.3.1 |
| 113 | LOW | **`exit` is sent without waiting for the `shutdown` response.** The spec's sequence is shutdown, wait for the response, then exit; a server still flushing state on `shutdown` is entitled to treat the early `exit` as abnormal. The doc comment claims this is "the sequence the specification asks for". Bounded by `wait_for_exit(500 ms)` and the tree kill, so a conformance defect rather than a hang, reported because rust-analyzer writing state on shutdown is the stated reason the call exists. | `typ-lsp/src/client.rs:366` | unowned |
| 114 | LOW | **Diagnostics are found through a canonicalising lookup and stored through a raw-path map.** `tab_for` runs `fs::canonicalize` on both sides; `synced_version` and `set_pushed` index `HashMap<PathBuf, Doc>` by the tab's un-canonicalised path. A URI that decodes to a different spelling of the same file, a lowercase drive letter, a symlinked directory, an 8.3 name, passes the first and misses the second, and the whole publish is discarded with no message while the version guard is skipped too. Latent: no server is confirmed to re-spell the path, but the asymmetry is real and the fix is cheap. | `typ-app/src/app.rs:432-448` | v0.3.1 |

### Dead and unreachable

| # | Sev | Defect | Where | Lands |
|---|---|---|---|---|
| 115 | LOW | **Four `PanelEvent` variants have no producer.** Counted across `crates/*/src/`: `RunCommand`, `Quit`, `OpenWith` and `Focus` appear only as match arms. `OpenWith` is reserved by invariant 6 and `Focus` by the M4 split work, both defensible; `PanelEvent::Quit` → `request_quit` is reserved by nothing, so no panel can currently ask the app to quit. `NotifyLevel::Warn` is constructed nowhere at all. Invariant 6 budgets this enum at ~12 variants, so a variant nothing can emit is spending that budget. | `typ-core/src/event.rs:92-113` | v0.3.1 |
| 116 | LOW | **The registry's answer is computed and thrown away.** `let _handler = self.registry.handler_for(path);` and then an `EditorPanel` is built regardless. `Registry::register` exists and its test asserts `logo.png → HandlerId("image")`. Harmless today because nothing calls `register`, but the mechanism the crate exists to prove is not wired: the first registered viewer opens as garbage text with no error. | `typ-app/src/app/tabs.rs:215` | v0.3.1 |
| 117 | LOW | **Three `TextBuffer` editing methods have no production caller and record no `EditSpan`.** `insert_char`, `delete_before` and `delete_after` are called only from `typ-buffer`'s own tests; `record_snapshot`'s doc says "M2 Task 12 deletes their last callers", which it did. Unlike `replace_range` none of them pushes to `self.edits`, so a future caller would silently strand every diagnostic held against the buffer, and each takes its own undo snapshot rather than joining a group. | `typ-buffer/src/buffer.rs:300`, `:308`, `:330` | v0.3.1 |
| 118 | LOW | **`Diagnostic::covers_line` has no callers and its logic is reimplemented one crate away.** `for_viewport` open-codes the same range test. | `typ-core/src/diagnostic.rs:43` | v0.3.1 |
| 119 | LOW | **`Theme::write_toml` emits only `[ui]`, dropping `[palette]` and `[syntax]`.** Its doc says "this is how the shipped default becomes a file rather than a private path", but a theme serialized through it loses every syntax colour, and the only caller is a test. | `typ-core/src/theme.rs:210` | v0.3.1 |
| 120 | LOW | **`anyhow` is a dependency of `typ-lsp` and `typ-syntax` and appears nowhere in either crate.** Both publish to crates.io, so consumers inherit it. (`tree-house-bindings` looks unused in `typ-syntax` too and is not, it is there for feature unification, which the workspace manifest documents.) | `crates/typ-lsp/Cargo.toml:14`, `crates/typ-syntax/Cargo.toml:14` | v0.3.1 |
| 121 | LOW | **`Action::ALL` is hand-maintained and nothing forces a new variant into it.** `name()` is an exhaustive match, so a new variant is compiler-forced to get a name; `ALL` is not. A variant missing from it is unbindable from a config file, absent from the command palette, and `tests/action.rs` still passes because it iterates `ALL`. The repo already compensates with hand-written `ALL.contains(...)` assertions in two test files, a guard that itself has to be remembered. | `typ-core/src/action.rs:144` | v0.3.1 |
| 122 | LOW | **Typing in the picker does not reset the selection.** `insert` only appends to the query; the arriving hits merely clamp. Type `mai`, press Down three times, type `n`, and the list re-ranks completely while `selected` stays at 3, Enter opens whatever now sits there. fzf, telescope and VS Code all reset to the top on a query change. | `typ-picker/src/lib.rs:330` | v0.3.1 |
| 123 | LOW | **Ctrl+D and Ctrl+Shift+L match the needle as a bare substring.** With the caret in `value`, Ctrl+Shift+L also selects inside `other_value_here` and `valueless`. The module comment reasons about case sensitivity and says nothing about word boundaries, so this reads as an omission rather than a decision, but it is a behaviour judgement, not a provable defect. | `typ-panel-editor/src/occurrence.rs:80`, `:105` | v0.3.1 |
| 124 | LOW | **Every pointer-motion report while the picker is open costs a full render pass.** `route_picker_mouse` sets `dirty = true` before `finish` can decline, while `Picker::handle_mouse` returns immediately for anything but `Down(Left)`. `run.rs:314` documents this exact anti-pattern for the non-picker path, "at M0 these were being counted as frames, which quietly flattered both p50 and p99". | `typ-app/src/app/picker.rs:196` | v0.3.1 |
| 125 | LOW | **Terminal teardown uses `?`, so a failure mid-cleanup skips `ratatui::restore()` and swallows the loop's own error.** On the way in, three setup calls `?` after `EnterAlternateScreen` has already succeeded, with no path that undoes it. | `typ-app/src/run.rs:41-48`, `:71-73` | v0.3.1 |
| 126 | LOW | **The goto-line rejection message can never be seen.** `status_left` returns the prompt when one is open and the prompt is deliberately kept open on a rejection, so "Not a line number: abc" is written to a field nothing is reading. It surfaces after Esc, describing something the user has moved on from. The test asserts on `app.status()` rather than on `status_left()`, which is why it passes. | `typ-app/src/app.rs:716`, written at `typ-app/src/app/search.rs:106` | v0.3.1 |

### The suite is softer than 971 suggests

Every row here is a test that passes whether or not the thing it names works. Four of them
masked a defect above.

| # | Sev | Defect | Where | Lands |
|---|---|---|---|---|
| 127 | HIGH | **App-owned actions are asserted against a hand-written allowlist instead of being run.** The check is "handled by `EditorPanel` *or* present in this 27-entry list", and the comment says why, "listed rather than probed because probing them means running them". So no test runs an app-owned action end to end except `goto_line` and `restart_language_servers`, and `perform_app_action` ends in `_ => return false`, which makes deleting any arm silent. This is what let gap 68 ship. | `typ-app/tests/milestone.rs:140`, `typ-app/tests/palette.rs:153` | v0.3.1 |
| 128 | HIGH | **`typ-panel-tree` has no mouse test at all.** `tree.rs` and `expand.rs` are keyboard-only, so `handle_mouse` and `handle_scroll` are entirely uncovered, which is how gap 75 survived, and it is an invariant-8 violation in its own right. The picker has an explicit test for exactly this shape. `handle_scroll` also reads `self.height`, which is only ever written inside `render`, so a wheel notch before the first draw clamps against the wrong bound. | `typ-panel-tree/tests/` | v0.3.1 |
| 129 | MED | **The picker's scroll test asserts before the render that undoes it.** `assert!(offset() > 0)` immediately after `handle_scroll`, and no frame is drawn. Green for gap 74's entire life. Its sibling at `:161` asserts `offset() <= 2` after scrolling 1000 rows, which a `handle_scroll` that does nothing also satisfies. | `typ-picker/tests/mouse.rs:141`, `:161` | v0.3.1 |
| 130 | MED | **The status-bar test measures with the same wrong ruler as the implementation.** `assert_eq!(status.chars().count(), 60)` cannot catch gap 105 by construction. | `typ-app/tests/frame.rs:246` | v0.3.1 |
| 131 | MED | **Two tests claim a server request is answered; neither asserts a reply.** The progress test asserts only that a segment appears, and the fake sends its follow-up notifications unconditionally without waiting, so it passes against a client that answers nothing, which is what gap 78 is. The `workspace/configuration` reply is likewise never asserted anywhere: delete the whole arm and all 971 tests pass while rust-analyzer blocks forever. | `typ-app/tests/progress.rs:133`, `typ-lsp/src/fake.rs:330` | v0.3.1 |
| 132 | MED | **The test named for an error path never produces an error.** `a_real_error_is_not_retried` passes `&[]`, the well-behaved fake, and `fake.rs` has no flag producing any error response other than `-32801`. It asserts exactly what an existing hover test already asserts. A client that retried on every error code, or swallowed real errors, would stay green. | `typ-app/tests/navigate.rs:290` | v0.3.1 |
| 133 | MED | **A render test that never reads the render.** The drawn buffer is dropped unread and the assertion re-checks what the setup already asserted. `App::render` builds four `RenderContext`s, three with `diagnostics: &[]`; wiring the editor to the wrong one still passes. | `typ-app/tests/diagnostics.rs:183` | v0.3.1 |
| 134 | MED | **A test with no assertion.** `shutdown_lets_the_server_stop_on_its_own` calls a method returning `()` which itself discards `wait_for_exit`'s bool. It cannot fail, not for a shutdown that hangs the full 10 s on every quit, nor for a server left running after exit. | `typ-lsp/tests/client.rs:176` | v0.3.1 |
| 135 | MED | **Two tautologies.** `syntax_render.rs:127` compares two identical expressions built from the same `plain()` helper. `palette.rs:217` asserts the palette's binding column against `Keymap::default_bindings()`, which is what built it, and if no action is unbound the loop body never runs. | `typ-panel-editor/tests/syntax_render.rs:127`, `typ-app/tests/palette.rs:217` | v0.3.1 |
| 136 | MED | **`lsp_sync.rs` counts notifications and never inspects a payload.** The header says "the document the server sees is the document on screen", and every assertion is a count or a version number. A `didChange` carrying the pre-edit rope, an empty string, or the wrong tab's buffer passes the whole file. | `typ-app/tests/lsp_sync.rs` | v0.3.1 |
| 137 | LOW | **Assertions that pass on absence.** `status_diagnostics.rs:70` compares two `Option<usize>` where `None < Some(_)`, so a missing segment passes. `app.rs:85` asserts `tree.width < 30` and `editor.width > 0` on a 40-column split, which a one-column sidebar satisfies. `grep_overrides.rs:96` asserts de-duplication, not that a clean tab was withheld. | various | v0.3.1 |
| 138 | LOW | **Invariant 10 is tested only through its error paths.** `cli.rs` covers `--version`, `--help` and two missing-path failures. Nothing covers "opens that file, blocks until closed, exits honestly, never detaches", the three clauses that make `typ` usable as `$EDITOR`. A regression that backgrounds the process or exits 0 on a failed save is invisible. | `typ/tests/cli.rs` | v0.3.1 |
| 139 | LOW | **Negatives asserted after a fixed sleep.** Two `navigate.rs` tests assert something did *not* happen after a 400 ms `settle()`; on a loaded runner the answer has merely not arrived, and they pass identically if the feature is broken end to end. `client.rs:155` drains with a 5 s `recv_timeout` and terminates on the timeout rather than on a state, costing ≥5 s every run. | `typ-app/tests/navigate.rs:170`, `:274`; `typ-lsp/tests/client.rs:155` | unowned |

### Type design: where an invariant rests on discipline

Invariant 4 (`col` is always a grapheme index) and the `Selections` contract are both real and
both currently held. Neither is held *by the compiler*, and this section is the list of places
where a wrong-unit value would compile. Gaps 72 and 73 are what that costs when the discipline
slips, so these are the same finding at one level up.

| # | Sev | Defect | Where | Lands |
|---|---|---|---|---|
| 140 | MED | **`Shift::record` takes three unlabelled positional arguments in two different coordinate spaces.** `original_end_line` is in *original* coordinates while `applied_end` and `after` are in current ones, and the two `Position`s are interchangeable to the compiler, `shift.record(edit.end.line, after, end)` compiles and makes every subsequent multi-caret edit land wrong. Invisible on a single cursor, because the shift is only consulted for the *next* selection, so only a multi-caret same-line test would catch a swap. `EditSpan` already holds `start`/`old_end`/`new_end` and is already produced by `replace_range`, so the fix is one named argument. | `typ-buffer/src/change.rs:54`, sole caller `typ-panel-editor/src/actions.rs:216` | v0.3.1 |
| 141 | MED | **`PanelEvent::OpenFile` re-flattens a `Position` into two bare `usize`, across a crate boundary.** Every boundary that converts *into* a `col` is currently correct, `typ-find`'s `grapheme_col`, `typ-lsp`'s `from_lsp` then `TextBuffer::position`, the mouse's `display_to_grapheme_col`, and every one of them is correct by convention. This event is where the convention has no type to lean on: a future producer writing `col: pos.character as usize` compiles, lands in `panel.goto`, and is *clamped* to the line's grapheme count, so a wrong unit becomes a plausible wrong column rather than a failure. `typ-core` already depends on `typ-buffer`, so `at: Position` costs nothing. Same family: `typ_lsp::from_lsp` returns a bare `usize` char offset that is assignment-compatible with a grapheme `col`; every caller pipes it through `TextBuffer::position`, but nothing makes them. | `typ-core/src/event.rs:100`, `typ-lsp/src/position.rs` | v0.3.1 |
| 142 | LOW | **The `Vec<Selection>` → `Selections` conversion is a runtime panic rather than a type.** `Selections` guarantees non-emptiness internally, and then every caller rebuilding one from a `Vec` re-establishes it with `.expect("selections are never empty")` or an `assert!`. All four callers currently derive their `Vec` from an existing `Selections`, so it cannot fire, until one filters, e.g. dropping empty selections, which panics on a buffer holding only carets. Folds into gap 82's fix: one `replace_preserving_primary(first, rest)` that is non-empty by signature. | `typ-panel-editor/src/actions.rs:170`, `typ-panel-editor/src/lib.rs:215` | v0.3.1 |
| 143 | LOW | **`goal_col` is a display column stored in the same type as `Position::col`, which is a grapheme index.** Both current writers convert correctly. `self.goal_col = Some(cursor.col)` compiles and is wrong only on lines containing tabs or wide graphemes, so it survives every ASCII fixture. `left_col` sits beside grapheme-column comparisons in `render.rs` with the same shape. Low because between them there are three writers, all `pub(crate)`. | `typ-panel-editor/src/lib.rs:96`, `:93` | unowned |
| 144 | LOW | **`ThemeColors` holds `ratatui::style::Color`, which is wider than a theme file can express.** All 27 fields are public on a public struct, and `Theme::write_toml` ends in `unreachable!("{other:?} cannot be written to a theme file")`, so a `ThemeColors` built by hand with `Color::Reset` panics the writer. Both real producers (`parse_hex`, `downgrade`) return `Color::Rgb` unconditionally, so this is theoretical today. Not worth a newtype until a second producer exists; worth knowing before someone adds one. | `typ-core/src/panel.rs:82-154`, `typ-core/src/theme.rs:223` | unowned |
| 145 | LOW | **The theme's `ui_pairs` and `assign` can drift, and a runtime assert is what catches it.** The destructure at `theme.rs:289` is exhaustive and compiler-enforced; `assign` at `:351` matches on string keys and is not. The two are bridged by an `assert!` and a round-trip test. This is the right trade and should stay, recorded only because it is the one place in the theme system where the compiler is not the enforcer. | `typ-core/src/theme.rs:289`, `:351`, `:452` | won't fix |
| 146 | LOW | **The OSC 52 write is unbounded in length.** Copying a 10 MB selection emits a ~13 MB escape sequence. Some terminals cap and truncate; the worst outcome is a partial clipboard, and there is no breakout because the payload is base64 first. A length cap with a status message would be honest about what happened. | `typ-buffer/src/clipboard.rs:114` | unowned |

### What was checked and is sound

Worth recording, because a clean bill on a surface someone will worry about later is as useful
as a defect, and because two of these are one design decision away from becoming critical.
Everything below was read rather than assumed; where a reader traced an argument by hand rather
than running it, that is said.

- **Language-server spawn cannot be hijacked by a cloned repository.** The command comes from
  `<config_dir>/config.toml` and nothing else: `TYP_CONFIG_DIR`, else `APPDATA` on Windows,
  else `XDG_CONFIG_HOME`, else `~/.config`. There is no project-local config read anywhere in
  the tree. No trust prompt is needed *because* of that, which is the point: **a future
  `.typ/config.toml` would turn this into remote code execution on clone.** No CWD hijack on
  Windows either: `current_dir` sets the child's working directory, not the search path, and
  Rust's own search order does not include it.
- **A malicious server cannot make the editor write files.** There is no `workspace/applyEdit`,
  no `WorkspaceEdit` and no `workspace/executeCommand` handler.
- **OSC 52 has no breakout.** The payload is base64 before it enters the sequence and the
  alphabet cannot terminate it. There is deliberately no OSC 52 *read*, which is the half that
  leaks a clipboard to a remote host.
- **The editor pane, file tree, status bar, hover box and panel chrome are all filtered**, via
  `Paragraph`/`Span::styled_graphemes` and `set_stringn`. That protection is ratatui's, not
  TYPE's, which is what makes gap 69's two `set_symbol` sites the whole of the exposure.
- **Out-of-workspace jumps are correct, not a traversal.** rust-analyzer routinely sends you to
  `~/.cargo/registry` and to stdlib sources; `url`'s parser normalises `..` before
  `to_file_path`.
- **Invariant 5 holds with no back-channel.** `RenderContext` carries a palette, syntax scopes,
  a diagnostics slice and four scalars. Grepped for `Rc`, `Arc`, `OnceLock`, `thread_local!`,
  `static mut`, `Mutex` and `RefCell` across the panel crates: the only hits are inbound parse
  results and the stderr tail below the app. `as_any_mut` is used app→panel and never the
  reverse.
- **One source of truth for the cursor.** No stored `cursor` field anywhere; `EditorPanel::cursor()`
  derives from `selections.primary().head`.
- **`PanelEvent` is at 8 variants** against invariant 6's ~12.
- **Position encoding is honoured in both directions.** All three encodings are handled, every
  caller passes the negotiated one rather than a default, clamping precedes every ropey call
  that could panic, and the round-trip test covers a ZWJ/skin-tone/CJK/CRLF corpus.
- **Zero raw `usize` subtraction in the whole workspace**: every one is `saturating_` or
  `checked_`.
- **All nine perf files are in `perf.yml`.** Gap 54 stayed closed.
- **The manifests test covers what crates.io requires**, on every member.
- **`install.sh`, `install.ps1` and `release.yml` are sound**, including the `set -e` loop
  described at the head of this part.
- **`typ-syntax` has no incremental parse**, so there is no `InputEdit` offset to corrupt, and
  grammars load lazily on the worker exactly as documented: the startup path never touches
  them.
- **Layout arithmetic survives a 1×1, 0-width and 2-row terminal**, and `main.rs` satisfies all
  four clauses of invariant 10 in the code even though the tests only cover the error ones
  (gap 138).

Named individually, because "no underflow found" is worth nothing without the list of what was
looked at:

- **`typ-buffer` arithmetic.** `brackets.rs:111` `*depth -= 1` cannot underflow: `depth` starts
  at 1 in both scanners and the function returns the instant it reaches 0. `indent.rs:87-93`
  `ab[ab.len() - 1]` cannot panic: `b_spaces >= 1` is checked first, so the index is guarded by
  a short-circuit. `word.rs:83` `len - 1` sits behind a `len == 0` early return.
  `position.rs:22,43,60` `tab_width - (col % tab_width)` cannot divide by zero: `set_tab_width`
  clamps `.max(1)`, `detect_indent_width` only returns 2..=8, and the fallback is 4.
  `line_ending.rs:49` `text.as_bytes()[index - 1]` is UTF-8 safe (it compares against `b'\r'`
  and continuation bytes are ≥ 0x80) with `Some(0)` handled before it.
- **`change.rs`'s `Shift` was traced by hand**, not run: multi-cursor insert on one line,
  multi-cursor Enter, and multi-cursor line-join via backspace at column 0. The `cols`/`col_line`
  reset is safe because `Selections::normalize` guarantees document order, so a later edit can
  never target an earlier line. Gap 140 is about the *call*, not this logic.
- **`buffer.rs` grapheme↔char conversion** snaps down correctly at cluster boundaries and clamps
  at line end. No byte or char offset leaks into a `col` anywhere in `typ-buffer` or `typ-core`.
- **`find_next` wraparound** is correct: the second loop is inclusive of the cursor line with
  `min_col: None`, so a lone match at the cursor's own column is still found.
- **The hand-rolled base64** in `clipboard.rs` is covered against the RFC 4648 vectors including
  the padding cases, which is where a hand-rolled encoder normally goes wrong.
- **Undo eviction** drops the oldest and keeps the newest, and is tested. The CRLF mixed-ending
  rewrite on save is a documented decision, not a defect.
- **The find worker's generation discipline holds.** The coalescing drain keeps `Index`, `Filter`
  and `Grep` separate and orders index before filter; `awaited_filter != Some(generation)` drops
  every superseded result; a late `Found::Files` in `Search`/`Commands` mode is a no-op; `send`
  on a dead receiver sets `jobs = None` rather than panicking. (The *parse* worker's coalescing
  is gap 111: different code, different answer.)
- **`typ-find`'s mutex lock order is consistently `found → capped`**, so no deadlock. Symlinks
  are not followed, matching ripgrep's default. `relative_to` handles the root itself and
  non-UTF-8. `require_git(false)` and the default ignore rules match between `walk` and `search`.
- **Fuzzy match indices map correctly to what is painted.** The ASCII/Unicode split in
  `haystack_of` and the walk in `write_clipped` are both grapheme-indexed and in step; indices
  past the end are ignored rather than panicking. Highlighting cannot land on the wrong
  characters. Gap 110 is about *scoring*, which is a different call.
- **Picker page-boundary arithmetic agrees across all three consumers**: `list_rows`, `row_at`
  and `draw_rows` put the last row on `inner.bottom() - 1`. Enter or a click on an empty list
  returns `None` rather than an `OpenFile` with an empty path.
- **`typ-registry` extension matching is case-insensitive and its no-extension fallback is
  correct**, both tested.
- **LSP capability gates are real.** `did_open`/`did_change`/`did_save`/`did_close` and `ask` all
  check the relevant capability before sending, with the spec's absent-`textDocumentSync`
  semantics, behind nine unit tests.
- **`pending` cannot grow without bound**: `cancel(kind)` prunes on every new ask of that kind,
  so at most one stale entry per `Ask` variant survives a server death.
- **All three LSP threads exit cleanly** on channel disconnect or EOF; stderr is drained so the
  pipe cannot fill and fake a hang, and is capped at 32 lines; the malformed-frame budget is
  bounded and tested.
- **Document versions are monotonic** within a server's life, and `docs.retain` on exit is
  correct because a restarted server is a fresh document set.
- **The process-lifetime handling is better than most editors manage**: a Unix process group
  with `SIGTERM`/`SIGKILL`, a Windows job object with `KILL_ON_JOB_CLOSE`. Gap 95 is about the
  two return values it discards, not about the design.
- **Writing through a symlink is deliberate, and correct.** `resolve_symlink` canonicalises so
  the rename replaces the *target* rather than the link: a repo containing `notes.md ->
  ~/.bashrc` means saving that file writes `~/.bashrc`. vim, emacs and ttt all do this, the user
  sees the target's contents on open, and replacing the link instead breaks every dotfiles repo.
  Recorded so it stays a decision rather than becoming an accident.

**On the perf figures in this part.** Each is best-of-five, `--release`, taken on a machine that
may have had other work on it: the absolute values are worth ±20–30%, and were not re-taken on
a quiet machine per the budgets section above. What is not in doubt is the *shape*: `Action::Move`
going 3 µs → 331 µs → 8.5 ms → 28 ms → 110 ms across 1 → 100 → 1000 → 2000 → 4000 cursors is
quadratic at any scaling factor, and paint growing linearly with cursors that are off screen is
linear at any scaling factor. Treat the numbers as "which order of magnitude and which curve",
not as measurements to regress against; the budget tests are the place for that.

### Who found what, and what that is worth

Nine readers ran over the tree in parallel, each with one scope and no sight of the others. That
independence is the only reason this section means anything: **a defect two readers reached from
different directions is worth more than one reader's confident report**, and there is no way to
tell them apart later unless it is written down now.

| Scope | Gaps it produced |
|---|---|
| `typ-buffer`, `typ-core` | 72, 73, 80, 81, 100, 115, 117, 118, 119 |
| `typ-panel-editor` | 81, 87, 88, 89, 123 |
| `typ-app` wiring, `main.rs` | 67, 71, 79, 83, 84, 85, 86, 115, 124, 125, 126 |
| `typ-lsp`, `typ-app/lsp`, `typ-syntax` | 76, 77, 78, 93, 95, 111, 112, 113, 114 |
| `typ-find`, `typ-picker`, `typ-panel-tree`, `typ-registry` | 74, 75, 96, 107, 108, 109, 110, 116, 122 |
| Silent failure, whole tree | 70, 90, 91, 92, 93, 94, 95, 97, 98, 99, 101, 102 |
| Type design | 82, 140–145 |
| Security | 69, 103, 104, 146 |
| The test suite itself | 127–139 |
| Read directly rather than delegated | 105, 106, 120, and the clean bills on the perf workflow, `PanelEvent`'s variant count, `Action`'s coverage, raw subtraction, the manifests test and both installers |

**Four were found twice, independently.** Treat these as the highest-confidence rows in the part:

- **Gap 95** (the two discarded Win32 `BOOL`s) came from the LSP reader and the silent-failure
  reader separately, by different routes: one auditing process lifetime, one auditing discarded
  return values.
- **Gap 93** (a refused `initialize` recorded as a successful one) likewise.
- **Gaps 72 and 73** were found by the buffer reader as two bugs in `overlaps` and `union`, and
  by the type-design reader as one question: "what actually enforces the `Selections`
  invariants?", with those two as its evidence. The second framing is why gaps 140–145 exist.
- **Gap 81** arrived as two halves that had to be put together: the buffer reader found
  `begin_edit_group` snapshotting unconditionally, and the panel-editor reader independently
  found `edit_at_each_selection` calling it without a guard while every sibling call site guards.
  Neither half is the whole defect.

Everything else is a single reader's finding, re-checked by hand before it was written down. That
re-check is not nothing, but it is one person agreeing with one report, which is a weaker thing
than the four rows above.

### Dead ends

Eleven claims were investigated and turned out to be wrong, or right for the wrong reason. They
are here because a wrong answer that is not written down gets re-derived at full price, and
because six of them are the same mistake: **believing a measurement without checking what it
measured.**

**Wrong theories about the code:**

- **`install.sh`'s `set -e` loop.** The `for candidate in …; do [ -f "$candidate" ] && found=… ; done`
  body looked like it must exit early when the last iteration's test fails. It does not: POSIX
  exempts a failure that is not the last command of an AND-list, so the list's status is 1 but
  the shell does not exit. Settled in two minutes under `dash` after an argument that would have
  gone on much longer. The installer is fine.
- **The `overlaps` fix.** The proposal was `a_end == b_start && (a.is_empty() || b.is_empty())`.
  Wrong: `a` is always the earlier of the two, and a caret at a *preceding* selection's end is a
  genuine second cursor because `[P,Q)` does not contain `Q`. The correct condition is
  `a.is_empty()` alone. Caught by writing the second boundary test before the fix.
- **ratatui's `debug_assert!` on control characters.** Reported as "a debug build panics on this
  input, so there is at least a signal". It does not fire on the render path: the assert is in
  `cell_width`, which that path does not reach. Gap 69 was silent in debug and release alike.

**Reproductions that passed, and did not mean what that looked like:**

- **Gap 67's first repro** closed the *last* tab. Its index is never reused, so `close_pending`
  named nothing and the test went green. The defect needs a middle tab. A failed reproduction
  disproves the reproduction before it disproves the claim: this one nearly buried a CRITICAL.
- **Gap 75's first probe** used an empty `sub/` directory, so expanding it added no rows and the
  assertion could not see the bug it was written for.
- **Gap 75's second probe** asserted that clicking a directory twice expands it. It does not: a
  fresh tree already has entry 0 selected, and clicking an already-selected row activates on the
  *first* click, so the second collapsed it again. The test was wrong, not the code.
- **Gap 77's test** passed on its first run against the fixed code, but would have passed against
  the broken code too: `close_tab` pushes no event, so `step_batch` never ran and the
  reconciliation the test asserts against had not happened.
- **Gap 78's first disable-and-recheck** reported the test still passing without the fix. Removing
  the specific match arm falls through to the `MethodNotFound` catch-all, and the fake sent its
  progress burst on *any* reply. The check was measuring nothing.

**Measurements of the wrong quantity:**

- **"Six tests disappeared."** The suite reported 971 before the branch and 970 after five tests
  were added. The two numbers came from different counters: `rtk`'s summary line includes six
  doctests, the per-suite sum does not. Apples to apples it is 965 → 970, exactly +5. Nothing was
  lost, and half an hour went into proving it.
- **`awk -F'[ ;]'` over `test result:` lines.** Consecutive delimiters produce empty fields, so
  the field taken for "failed" was the empty string and the field taken for "ignored" was the
  literal word `failed`. Every "0 failed" printed that way was meaningless. The counts were
  re-taken with a `sed` capture, and the failure count is now read from `grep -c` on the failure
  lines instead: a different measurement, not a better parse of the same one.

The last two are worth more than they look. The budgets section of `AGENTS.md` already carries
five ways a wall-clock number lies; these are two ways a *pass/fail* number lies, which is the
one everybody trusts without looking.

### Where this stands

**A snapshot, not a living list: delete this subsection when the branch lands.** It exists
because the state below is the one thing in this part that is not recoverable by reading the
tree, and the audit's own lesson is that unrecorded context is context that is gone.

Branch `fix/audit-v0.3.0`, off `e1998d8`. Each fix was written test-first: the failing test was
run and its output read *before* the fix, and the suite plus `clippy -D warnings` was green
before each commit.

| Commit | Gaps | |
|---|---|---|
| `c59255e` | 67–139 | This part of this document |
| `dce7fbb` | 67 | `close_pending = None` in `close_tab` |
| `64403bf` | 68 | `answers_its_own_confirmation`, shared by both dispatch paths |
| `0f83fdc` | 69 | `typ_core::printable`, used by the picker and the tab bar |
| `eb735a6` | 70, 71 | `App::open_or_report`, and `reload` no longer propagates |
| `6c50614` | 72 | `overlaps` merges on `a.is_empty()` |
| `08ce332` | 73 | `union` keeps a direction both inputs agree on |
| `45a9b36` | 74, 129 | `scroll` carries the selection; the test paints before asserting |
| `4c062b9` | 75, 128 | The tree's hit test is bounded, and the crate has mouse tests |
| `192a417` | 76, 77, 78, 131 | Answers keyed on `ServerId`; a reopen sends `didOpen`; server requests are answered. Carries gaps 140–146 and this subsection, which `git add -A` swept in, they belong to the doc commit and are noted here rather than rewritten |

Two fixes deliberately differ from what the audit proposed, because a test said so, and both are
worth knowing before someone "corrects" them back:

- **Gap 72.** The proposal was `a.is_empty() || b.is_empty()`. That is wrong in the other
  direction: a caret at a *preceding* selection's end is a genuine second cursor, because
  `[P,Q)` does not contain `Q`. The boundary is one-sided: `a` is the earlier of the two, so it
  turns on `a.is_empty()` alone. There is now a test pinning each side.
- **Gap 74.** The proposal offered two options, one of which was to make `visible` stop
  correcting the offset. That would break the guarantee the mouse hit-test depends on, so the
  selection travels with the viewport instead.

**Suite at this point:** 986 passed, 0 failed, 31 ignored; `clippy --workspace --all-targets`
clean. The runnable count was 965 before this branch. Take the failure count from `grep -c` on
the failure lines rather than from a field split: see the dead ends above for why.

**Three of the RED steps were taken after the fix rather than before**, which is not the
workflow and is recorded because two of them were nearly wrong. Each was checked by disabling
the fix and re-running:

- Gap 77's test passed on the first run because `close_tab` marks the frame dirty but pushes no
  event, so nothing drove `step_batch` and the reconciliation pass never ran. The test was
  asserting against a sync that had not happened. It calls `sync_language_servers` directly now.
- Gap 78's first disable-and-check said the test still passed, because removing the specific
  arm just falls through to the `MethodNotFound` catch-all, and the fake sent its burst on *any*
  reply. The fake now requires an accepting one, which is both the correct semantic and what
  makes the test able to fail.
- Gap 76's two unit tests were confirmed red by dropping `p.server == server` from the
  predicate.

**There is a stash.** `stash@{0}`, "wip: LSP ContentModified retry + empty-answer status
(pre-audit)": work that predates this audit, set aside so the branch started clean. It touches
`app.rs` and `lsp/mod.rs`, both of which gaps 76–78 also edit, and it changes `handle_answer`,
whose signature gap 76 changes. It will not pop cleanly. It is also *not* redundant with gap 101:
it adds a message for an empty answer and a retry for `ContentModified`, and leaves the
`Err(_) => log_warn!` arm (which is gap 101) exactly as it was.

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
