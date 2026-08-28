---
type: design
status: living
area: code-intelligence
verified: 2026-08-28
verified-against: v0.3.0-dev (M3 tasks 8, 9)
---

# LSP — what the research found, and what it decided

Researched before M3 was planned, because M3 is the largest milestone since M2 and the
architecture document's LSP row turned out to be a 2025 prediction rather than a decision.

Everything below was checked against crates.io, the LSP specification, or the source of the
editor named. Where a claim is someone else's, the link is in [Sources](#sources).

---

## 1. The stack table was stale in four rows

`architecture.md` §5 names the stack. Three of its rows are contradicted by the tree today and
the fourth is the subject of this document.

| §5 says | The tree does | Since |
|---|---|---|
| Syntax: grammars **dynamically loaded** | compiled in, no runtime directory | M2.7 |
| Fuzzy matching: `nucleo` | `nucleo-matcher`, deliberately without the rayon wrapper | M2.8 |
| Project search: **shell out to** `ripgrep` | `grep-searcher` as a library | M2.8 |
| LSP: `lsp-types` 0.97 + custom **async** client | `gen-lsp-types` 0.11 + threads, no runtime | M3 |

This is the same failure already logged as gap 44, where §5 lists `typ-config` and `typ-ui`,
neither of which was built. A milestone that changes the stack has to change §5 in the same
commit, or §5 becomes a list of things that were once intended.

## 2. Everyone has left `lsp-types`

`lsp-types` 0.97.0 was published on 2024-06-04 and nothing has followed it. The three largest
Rust consumers have each moved, and they moved in three different directions:

- **Helix** vendors a fork, `helix-lsp-types`, as a workspace member. It is not published to
  crates.io.
- **Zed** pins a git fork by revision: `lsp-types = { git = "https://github.com/zed-industries/lsp-types", rev = "f4dfa89…" }`.
- **rust-analyzer** switched to `gen-lsp-types` in PR #22115, merged 2026-06-24. Master now
  reads `lsp-types = { version = "0.11.0", package = "gen-lsp-types", features = ["url"] }`.

The rust-analyzer PR names eight defects in `lsp-types`: missing `SnippetTextEdit`, absent `Eq`
and `Hash` derives, poor interoperability with the `Uri` type added in 0.96, type names that do
not match the specification, missing `Default` on `CodeActionOptions`, incomplete `Hash`
coverage, limited `FoldingRange.kind`, and a typo in the `workspace/diagnostics` capability
field.

**Every one of those is a transcription error.** That is the argument for a generated crate, and
it is a stronger argument than "rust-analyzer did it" — the prior-art rule in `AGENTS.md` does
not accept the second one.

### The fork route is closed to TYPE

Cargo refuses to publish a crate that carries a git dependency. TYPE publishes ten crates, so
Zed's approach is unavailable, and Helix's fork is a workspace member rather than a registry
crate. The choice is genuinely between the dormant crate, the generated one, and writing the
types by hand.

### Dependency weight, measured against the current lockfile

| | new to TYPE | already present |
|---|---|---|
| `gen-lsp-types` 0.11 + `url` | `serde_json`, `url`, `idna` and friends | `serde` |
| `lsp-types` 0.97 | `serde_json`, `fluent-uri` 0.1, `serde_repr` | `serde`, `bitflags` 1 (via `wezterm-input-types`) |
| `lsp-server` 0.10 | `serde_json`, `crossbeam-channel`, `serde_derive` | `serde`, `log` |

`lsp-types` pulling `bitflags` ^1 looked like a duplicate-major problem and is not: 1.3.2 and
2.13.1 are both in `Cargo.lock` already, dragged in by `wezterm-input-types`. The argument
against `lsp-types` is its dormancy, not its dependency list.

### Decision

**`gen-lsp-types` 0.11 with `features = ["url"]`**, pinned exactly and bumped in its own commit
the way the toolchain is.

The reasoning is a risk asymmetry rather than a preference. A breaking change in a *types* crate
surfaces as a compile error: loud, immediate, and bounded by how many types TYPE actually names,
which is about fifteen at v0.3.0 and perhaps sixty across all of M3. A transcription defect in a
dormant crate surfaces as a wire message the server rejects, or worse, silently misreads. Churn
is the cheaper of the two failures.

Runtime cost is identical across all three options — they are serde derives on structs. Binary
cost is close to zero for the same reason M2.8 measured: `lto = "fat"` contributes what the
program *reaches*, and that milestone's predicted +1.2–1.8 MB came in at +0.05 MB.

The case against is real and is recorded here rather than argued away: `gen-lsp-types` has 28
stars, one maintainer, and shipped a breaking `fix!` on 2026-07-27 after going 0.8 to 0.11 in
seven weeks.

### Why `url`, having first decided against it

The first pass said to leave both URI backends off and own the `file://` conversion, on the
grounds that Windows drive letters and UNC paths are where that conversion breaks and TYPE is
developed on Windows.

That was the wrong call. Percent-encoding a path with non-ASCII characters is a **correctness
table, not logic**, and a client that decodes it wrong opens a different file without saying so.
`url` is servo's, it is mature, and the table is already right there.

What does not change: the round-trip tests run on all three platforms regardless, because
`Url::from_file_path` has its own edges — it returns `Err(())` for a relative path and has
documented UNC behaviour.

## 3. No LSP client crate is worth taking

The registry was searched rather than assumed. There is no dominant general-purpose Rust LSP
*client*; Helix, Zed, Neovim and Emacs all wrote their own, and crates.io holds a long tail of
clients built for one project.

| Crate | Version | 90-day downloads | Why not |
|---|---|---|---|
| `lsp-max-client` | 26.6.9 | 4,725 | tower-based, so it brings a runtime |
| `escriba-lsp-client` | 0.1.84 | 1,264 | active, but shaped for one host application |
| `weavatrix-lsp-client` | 0.1.0 | 20 | "bounded, runtime-free" is the right shape; unknown author, first release |
| `tokio-lsp` | 0.1.1 | 472 | runtime |
| `async-lsp-client`, `lsp-client`, `lsp-client-rs` | — | under 250 | small or stale |
| `lspresso` | 0.1.0 | 15 | a *test* client — worth re-examining when the fixture is built |

`weavatrix-lsp-client` is not a dependency TYPE should take, but its one-line description names
a problem the alternatives ignore. See §6.

## 4. Transport: threads, not a runtime

`lsp-server` is described as a server scaffold, and it is, but the two functions that matter are
generic:

```rust
pub fn read(r: &mut impl BufRead) -> Result<Option<Message>>
pub fn write(&self, w: &mut impl Write) -> Result<()>
```

`BufRead` and `Write`, not the process's own stdio. They frame just as well over a child's pipes,
which makes the crate usable client-side despite its framing.

Helix runs on tokio because Helix's editor loop is async. TYPE's blocks on a single `recv()`, and
`ParseWorker` and `FindWorker` are both threads plus channels. A runtime would mean a second
concurrency shape alongside those two, and a startup cost against a 100 ms budget.

**Decision: `lsp-server` 0.10 for framing and correlation, one reader thread and one writer
thread per server.**

What `ReqQueue` buys is the half that gets forgotten. **Servers send requests to the client**,
not only responses — rust-analyzer sends `workspace/configuration`,
`client/registerCapability` and `window/workDoneProgress/create`. Correlation has to work in
both directions or the server hangs waiting for a reply that was never modelled.

The caution: `lsp-server` went 0.7.9 to 0.10.0 between June and July 2026, including two
breaking commits that realigned `Response` to JSON-RPC. Those are conformance corrections rather
than churn for its own sake, but the version gets pinned exactly.

One oddity, checked so it does not get mistaken for a signal: rust-analyzer's own workspace pins
`lsp-server = "0.7.9"` while the crate at `lib/lsp-server` in the same repository is 0.10.0. The
path dependency is commented out at the workspace root, so the registry pin is what their
published build uses. It is a stale pin in their tree, not a warning about 0.10.

## 5. Position encoding is the correctness core

LSP positions are `(line, character)` where `character` counts **UTF-16 code units** by default.
TYPE's `col` is a grapheme index, and invariant 4 says it is a grapheme index everywhere.

Since 3.17 the client may state a preference through `general.positionEncodings`, and
`PositionEncodingKind` defines three:

| Kind | Counts | In TYPE's terms |
|---|---|---|
| `utf-32` | Unicode code points | ropey's native `char` index — **zero conversion** |
| `utf-8` | bytes | one `char_to_byte` |
| `utf-16` | UTF-16 code units | must be counted |

Helix negotiates exactly this order, for the same reason: a rope indexed by chars gets UTF-32
free. But UTF-16 is the only encoding a server is required to support and most implement nothing
else, so **all three get written regardless**. Negotiation is an optimisation; the UTF-16 path is
the one that has to be right.

### The trap is one layer above the encoding

`col` is graphemes, and none of the three encodings counts graphemes. A single `👍🏽` is:

| Unit | Count |
|---|---|
| grapheme cluster | 1 |
| `char` / UTF-32 | 2 |
| UTF-16 code unit | 4 |
| byte / UTF-8 | 8 |

A server may legitimately return a position **inside** that cluster. There is no `Position` in
`Selections` that represents it, so the conversion has to snap to a cluster boundary or the
cursor lands somewhere the buffer cannot express. This is the client-side half of the bug that
made rust-analyzer fail on the bottom emoji.

### The seam that falls out of it

**`typ-lsp` speaks char offsets and never mentions graphemes.** Char is ropey's native unit and
the natural pivot for all three encodings, and grapheme-to-char stays in `typ-buffer` where the
grapheme logic already lives. `typ-lsp` then depends on `ropey` and on nothing of TYPE's, which
is the same bottom-of-graph position `typ-syntax` and `typ-find` hold, for the same
publish-order reason.

## 6. Orphaned server processes

rust-analyzer spawns `cargo` and `rustc` beneath itself. If `typ` dies badly — a panic, a
SIGKILL, a closed terminal window — the server and its entire subtree keep running and keep a
core busy indefinitely.

Neither `lsp-server` nor a hand-rolled framer does anything about this. Killing the child is not
enough; the grandchildren survive it.

What it needs is platform-specific: a **Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`**
on Windows, and a **process group with a group-directed kill** on Unix. This is the problem
`weavatrix-lsp-client` was written to solve, and it is the kind of defect that surfaces three
months later as "why is this laptop hot".

## 7. Undercurl cannot be drawn through ratatui

`gap-analysis.md` lists undercurl for diagnostics as an M3 deliverable. It is not reachable as
the render path is built.

- **crossterm 0.29 has it.** `Attribute::Undercurled = 3`, alongside `DoubleUnderlined`,
  `Underdotted` and `Underdashed`, at `src/style/types/attribute.rs:106-112`.
- **ratatui-core 0.30 does not expose it.** `Modifier` carries nine bits — `BOLD`, `DIM`,
  `ITALIC`, `UNDERLINED`, `SLOW_BLINK`, `RAPID_BLINK`, `REVERSED`, `HIDDEN`, `CROSSED_OUT` — and
  none of them is a curl. The crossterm backend maps `Modifier` to attributes, so with no bit
  there is nothing to map from.
- **Underline colour is separate again**, behind ratatui's `underline-color` feature, supported
  only on the crossterm, termina and termwiz backends.

Writing the escape by hand after `terminal.draw()` is not an option worth trying: ratatui owns
the double-buffer diff, and a cell it did not write is a cell the next frame will not repair.

Three ways out, in the order they should be tried:

1. Ship a **coloured single underline** at v0.3.0 by enabling ratatui's `underline-color`
   feature. That is most of the visual signal and it is available today.
2. **Send a `Modifier::UNDERCURL` upstream to ratatui** in parallel. Nine of the bitflags' bits
   are used and crossterm already has the attribute, so the change is small and TYPE is already
   in that ecosystem.
3. Revisit once (2) lands or is refused.

**Re-checked at v0.3.0-dev, and two of those moved.**

- **(1) is already done, by default.** `underline-color` is in ratatui 0.30's `default` feature
  list, TYPE takes default features, and `ratatui-crossterm` already emits `SetUnderlineColor`
  per cell. Coloured underlines cost nothing and need no manifest change. The step this section
  proposed as work is not work.
- **(2) has not been filed by anyone.** A search of `ratatui/ratatui` for undercurl returns zero
  issues and zero pull requests, open or closed. So there is nothing to wait on, and the custom
  backend M3 Task 10 builds is the only route to the curl itself.
- Bits re-confirmed: `Modifier` is a `u16` with nine bits used, top bit `0b0001_0000_0000`, so
  the free bit the plan takes is genuinely free. crossterm 0.29 still has `Undercurled = 3`
  alongside `DoubleUnderlined`, `Underdotted` and `Underdashed`.
- **One live upstream bug to design around:** ratatui#1346, open since 2024-08-27, reports
  `underline_color()` causing the screen to flicker from that line down. It is the feature Task
  11 paints with, so a repaint problem there is TYPE's problem too.

The gap-analysis row gets amended to say this rather than left to look like a missed deliverable.

## 8. Document sync: full, and once per frame

`undo.rs` stores whole-rope snapshots, not deltas, so incremental `didChange` would need a new
per-buffer delta list built for it.

It should not be built yet, and the reason is not only that it is more code. **Full sync cannot
desync.** The recurring failure of incremental sync is client and server state drifting apart
until an edit arrives that contradicts the document the server believes it holds. Full sync is
simultaneously the smaller change and the safer one.

The cost is sending the document on every change, and there is a coalescing point already in the
tree: the event loop batches input, so **one `didChange` per frame** collapses a ten-key burst
into one notification. That is the same argument `typ-syntax/src/worker.rs` already makes
against Zed's 200 ms reparse debounce — self-tuning to the machine, no latency floor — reused
rather than reinvented.

**The prediction, written down so a miss is visible:** `rope.to_string()` plus serialisation for
a 50k-line file, once per frame, costs under 1 ms and never touches the render thread. If the
measurement disagrees, incremental sync is a follow-up task and not a redesign.

## 9. `Shift` was close, and not enough

`typ-buffer/src/change.rs` maps a position forward through edits already applied. Its own module
comment names the consumers it was extracted for: *"search results, diagnostics, git hunks"*.
Diagnostics arrive describing the document as it was some milliseconds ago, the user keeps
typing, and without a shift the squiggles sit under the wrong words.

**Building it showed `Shift` answers a different question.** `Shift::apply` moves *every*
position, including ones before the edit — correct for its only caller, which asks "where does
the *next* edit in this batch go" and within a batch has no positions before the edit. A
diagnostic sits wherever the server put it, so it needs the edit's own extent to compare
against.

So M3 added `EditSpan` beside it: one applied edit, with `start`, `old_end` and `new_end`, and
`shift_through` folding a position through a slice of them — unmoved before, clamped to the start
inside, shifted after. `TextBuffer::replace_range` records the spans and the app drains them once
per batch, which is what bounds the record.

Its limit is the one `Shift` already had, and it is stated in the doc comment rather than
discovered later: undo, redo and an external reload replace the whole rope without going through
`replace_range`, so nothing is recorded and anything held against the old text keeps stale
coordinates until it is published again. That is a shift map, not an anchor system, and anchors
remain a separate decision.

## 10. Landscape re-check

`landscape.md` asks to be re-verified at the start of any milestone longer than a few weeks.

**Helix's last release is 25.07.1, dated 2025-07-18 — thirteen months ago** — against a push to
master two days before this was written, and 45,941 stars. The gap the landscape document
identified has widened rather than closed.

Its ranked list of what makes an evaluator bounce still puts **LSP first, as an instant bounce**,
with the fuzzy finder and project search second. Those shipped at v0.2.8.

## Decisions, collected

| Question | Decision |
|---|---|
| Milestone shape | Three releases: v0.3.0 client and diagnostics, v0.3.1 completion, v0.3.2 edits |
| Types | `gen-lsp-types` 0.11, `features = ["url"]`, pinned exactly |
| Transport | `lsp-server` 0.10, one reader and one writer thread per server, no runtime |
| Crate position | `typ-lsp` at the bottom beside `typ-syntax` and `typ-find`, depending on nothing of TYPE's |
| `typ-lsp`'s unit | char offsets; graphemes stay in `typ-buffer` |
| Encoding preference | `utf-32`, `utf-8`, `utf-16` — all three implemented |
| Document sync | full, one `didChange` per frame, with the cost measured on 50k lines |
| Process lifecycle | Job Object on Windows, process group on Unix |
| Undercurl | coloured underline is already free — ratatui 0.30 ships `underline-color` by default; the curl needs the custom backend |
| Line breaks | ropey without `unicode_lines`: a line feed and a CRLF pair, matching rust-analyzer, ripgrep and git |
| Sync capability | absent `textDocumentSync` means send nothing, matching vscode-languageclient and Helix |
| Pull diagnostics | deferred past v0.3.0 — declaring `textDocument.diagnostic` turns rust-analyzer's native push off |

## Sources

- [LSP specification 3.17 — `positionEncodings` and `PositionEncodingKind`](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/)
- [LSP specification 3.18 (draft)](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.18/specification/)
- [`PositionEncodingKind` constants](https://docs.rs/lsp-types/latest/lsp_types/struct.PositionEncodingKind.html)
- [helix#5894 — Negotiate LSP Position Encoding](https://github.com/helix-editor/helix/pull/5894)
- [rust-analyzer#22115 — switch out lsp-types for gen-lsp-types](https://github.com/rust-lang/rust-analyzer/pull/22115)
- [`gen-lsp-types`](https://github.com/ribru17/gen-lsp-types)
- [`lsp-server` — `Message::read` / `Message::write`](https://docs.rs/lsp-server/latest/lsp_server/enum.Message.html)
- [rust-analyzer#7453 — implement the utf-8 offsets extension from clangd](https://github.com/rust-lang/rust-analyzer/issues/7453)
- [clangd protocol extensions — `offsetEncoding`, deprecated in clangd-21](https://clangd.llvm.org/extensions)
- [The bottom emoji breaks rust-analyzer](https://fasterthanli.me/articles/the-bottom-emoji-breaks-rust-analyzer)
- [ratatui#308 — enable setting the underline color](https://github.com/ratatui/ratatui/issues/308)
- [Helix language server documentation](https://docs.helix-editor.com/master/lsp.html)
- [Helix releases](https://github.com/helix-editor/helix/releases)
- [microsoft/language-server-protocol#1706 — stronger validation of incremental text sync consistency](https://github.com/microsoft/language-server-protocol/issues/1706)

Added while building tasks 8 and 9, each read at a pinned commit:

- [rust-analyzer `main_loop.rs` — the native-diagnostics gate](https://github.com/rust-lang/rust-analyzer/blob/70d74f4d134c45b073c82167fb7e7d61334bd8f5/crates/rust-analyzer/src/main_loop.rs#L570)
- [rust-analyzer `lsp/capabilities.rs` — `text_document_diagnostic`, `diagnosticProvider`](https://github.com/rust-lang/rust-analyzer/blob/70d74f4d134c45b073c82167fb7e7d61334bd8f5/crates/rust-analyzer/src/lsp/capabilities.rs#L442)
- [rust-analyzer `lib/line-index` — lines break on a line feed and nothing else](https://github.com/rust-lang/rust-analyzer/blob/70d74f4d134c45b073c82167fb7e7d61334bd8f5/lib/line-index/src/lib.rs#L488)
- [Helix `handlers/diagnostics.rs` — the merged pull implementation](https://github.com/helix-editor/helix/blob/079a789e8cb08ead67f19e1971a1b7438b37354b/helix-term/src/handlers/diagnostics.rs)
- [Helix `application.rs` — push tagged `identifier: None`](https://github.com/helix-editor/helix/blob/079a789e8cb08ead67f19e1971a1b7438b37354b/helix-term/src/application.rs#L829)
- [Helix `helix-lsp/client.rs` — `did_change` and `did_save` capability gates](https://github.com/helix-editor/helix/blob/079a789e8cb08ead67f19e1971a1b7438b37354b/helix-lsp/src/client.rs#L1081)
- [Helix workspace manifest — `ropey` without default features](https://github.com/helix-editor/helix/blob/079a789e8cb08ead67f19e1971a1b7438b37354b/Cargo.toml#L48)
- [vscode-languageserver-node `client.ts` — resolving `textDocumentSync`](https://github.com/microsoft/vscode-languageserver-node/blob/3e4039cee8c7d955ee2fb563f81fc1f8ab3f5121/client/src/common/client.ts#L1489)
- [vscode-languageserver-node `textSynchronization.ts` — registering on the resolved options](https://github.com/microsoft/vscode-languageserver-node/blob/3e4039cee8c7d955ee2fb563f81fc1f8ab3f5121/client/src/common/textSynchronization.ts#L100)
- [microsoft/language-server-protocol#288 — the `didSave` table, confirmed by dbaeumer](https://github.com/microsoft/language-server-protocol/issues/288)
- [neovim#37936 — push and pull diagnostics override each other](https://github.com/neovim/neovim/issues/37936)
- [ratatui#1346 — `underline_color()` flickers, still open](https://github.com/ratatui/ratatui/issues/1346)
- [ropey — line break behaviour and the `simd` footgun](https://docs.rs/ropey/1.6.1/ropey/#a-note-about-line-breaks)

Checked directly in the tree or the registry, with no link to give: crossterm 0.29
`src/style/types/attribute.rs:106-112`; ratatui-core 0.30 `src/style.rs:108`; rust-analyzer
master `crates/rust-analyzer/Cargo.toml:32`; Zed master `Cargo.toml:675`; crates.io version,
download and dependency data for every crate named above.

---

## Actual — what building tasks 8 and 9 changed

Four of this document's conclusions moved. Each was checked in a source tree rather than
reasoned about, because three of the four are places the earlier reasoning was wrong.

### The gap-1 premise was false, and its replacement is sharper

The M3 plan argued that rust-analyzer sends its **native** diagnostics — the ones that appear as
you type — only through pull, so a push-only client would get the slow half that arrives on save.
That is not true for a client shaped like TYPE. `crates/rust-analyzer/src/main_loop.rs`:

```rust
if project_or_mem_docs_changed
    && !self.config.text_document_diagnostic()
    && self.config.publish_diagnostics(None)
{
    self.update_diagnostics();
}
```

`text_document_diagnostic()` reads the **client** capability `textDocument.diagnostic`
(`crates/rust-analyzer/src/lsp/capabilities.rs`). `update_diagnostics` is the native set. It runs
only when the client has *not* declared pull support. TYPE does not, so push carries both halves.

**The trap that replaces it is worse than the gap was.** Declaring `textDocument.diagnostic`
turns the native push *off*. A client that declares the capability without a working pull path
loses the fast diagnostics it already had. So the declaration and the implementation ship
together or neither does, and a test asserts the capability stays absent until they do.

Pull is still worth building — servers that implement nothing else exist, `ruby-lsp` being the
one `helix#7757` was filed about, and rust-analyzer states
`diagnosticProvider.identifier = "rust-analyzer"` precisely so the two sets can be kept apart.
It is not worth building *for* rust-analyzer.

### Helix keys the two sets apart; it does not prefer one

Worth recording because a first pass got it backwards from a search summary describing
`helix#7900`, which never merged. What is in the tree at `079a789`:

- `helix-term/src/application.rs` handles `publishDiagnostics` as
  `DiagnosticProvider::Lsp { server_id, identifier: None }`
- `helix-term/src/handlers/diagnostics.rs` tags the pulled set with the server's `identifier`
- 250 ms debounce per document, in-flight request cancelled first
- a **1 s** debounce that re-pulls *every open document* on every edit when a server declares
  `interFileDependencies`, which rust-analyzer does
- `DiagnosticServerCancellationData.retriggerRequest` honoured with a 500 ms retry
- `previousResultId` stored per server, so an `Unchanged` report keeps what is shown

That last-but-two point is the real cost of pull, and it is the reason the debounce is not
optional.

### An absent `textDocumentSync` means silence

TYPE first defaulted to full sync when a server declared no `textDocumentSync` at all, reasoning
that a server which forgot to declare it and gets nothing is worse off. Both mature clients
disagree, and so does the specification's default:

- **vscode-languageserver-node** resolves the number form to explicit options and otherwise
  leaves `resolvedTextDocumentSync` undefined; every sync feature registers only if the resolved
  options say so.
- **Helix** returns `None` from `text_document_did_change` on the same input.

The `save` column is the specification maintainer's own table, confirmed by dbaeumer in
`microsoft/language-server-protocol#288`: absent capabilities and an object with no `save` send
nothing, the number form sends a textless `didSave`, only `includeText: true` carries the
document.

`publishDiagnostics.versionSupport` is declared, because TYPE reads the field — a publish naming
a version older than one already sent is dropped. Reading it without declaring it is as
dishonest as the reverse.

### ropey's default line breaks are the wrong set for an editor

The one that would have been hardest to find later, and it predates M3.

ropey's `unicode_lines` feature is on by default and makes `U+000B`, `U+000C`, `U+0085`,
`U+2028` and `U+2029` line breaks, bringing it into conformance with Unicode Annex #14. That is
right for text layout and wrong for a code editor. rust-analyzer's `lib/line-index` breaks on a
line feed and nothing else; so does ripgrep, which `typ-find` already used; so does git, which
M5 will. A file containing a form feed — legal Rust, and a page separator in older code — put
every diagnostic, every search match and every future hunk below it one line out of step.

Reproduced before fixing: `a\x0Cb\n` was two lines to the buffer and one to everything it talks
to. The workspace now pins
`ropey = { version = "1.6", default-features = false, features = ["simd"] }`, which is the line
Helix pins, for this reason. `simd` has to be restored by hand — ropey's own documentation calls
losing it this way a footgun.

`cr_lines` goes with it, so a bare CR is content. `LineEnding` has only ever modelled LF and
CRLF, so this makes the rope agree with the type that was already beside it.

### Full sync, measured

§8 predicted "under 1 ms" for `rope.to_string()` plus serialisation of a 50k-line file, once per
frame.

| 50k lines | render thread (`to_string` + `Value`) | writer thread (`to_vec`) |
|---|---|---|
| typical, 1.45 MB | 1.269 ms | 1.963 ms |
| wide, 3.2 MB | 2.850 ms | 4.934 ms |

**Missed by 3 to 6x**, and the design survives anyway because of where the cost falls: the writer
thread already paid for serialisation, and `didChange` now hands it a closure over a cloned
`Rope` — an atomic bump — so it pays for the text too. Nothing of full sync is on the render
thread. Incremental sync stays unbuilt, and it stays unbuilt for a measured reason rather than an
assumed one.

### Dependency status, re-checked at v0.3.0-dev

| Crate | Latest | Published | Our pin |
|---|---|---|---|
| `gen-lsp-types` | 0.11.0 | 2026-07-28 | `=0.11.0`, current |
| `lsp-server` | 0.10.0 | 2026-07-16 | `=0.10.0`, current |
| `lsp-types` | 0.97.0 | 2024-06-04 | not used — the dormancy §2 rejected it for is now 26 months |
| `ropey` | 2.0.0-beta.1 | 2025-08-02 | 1.6.1, the newest stable |

Both LSP pins are the newest release; neither needs a bump. `ropey` 1.6.1 dates from 2023 and is
still what Helix pins; the `2.x` branch is actively worked on — commits through August 2026 — but
there is no stable 2.0 and no reason to take a beta for a rope this project is not straining.
Re-check at M4.

