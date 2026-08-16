# Contributing to TYPE

TYPE is pre-alpha and moving fast. Bug reports and small focused PRs are welcome; large ones
are worth an issue first, because the roadmap is opinionated and I would rather not waste your
afternoon.

## Getting set up

```bash
git clone https://github.com/Pranjal-SB/type
cd type
cargo build
cargo run -- .
```

The toolchain is pinned in `rust-toolchain.toml`. `rustup` picks it up automatically.

## Before you open a PR

These three have to be clean. CI runs them on Linux, macOS and Windows.

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

If you touched anything on the editing or render path, run the performance tests too. They are
`#[ignore]`d because a shared CI runner is too noisy to gate a merge on a wall-clock number:

```bash
cargo test --release -p typ-buffer --test perf -- --ignored --nocapture
cargo test --release -p typ-panel-editor --test perf -- --ignored --nocapture
```

Paste the numbers in the PR. A budget with nothing measuring it is a budget nobody has.

## The rules that are not negotiable

The reasoning is in [the architecture doc](docs/design/architecture.md). The ones that catch
people out:

- **Every editing primitive is an `Action`.** No key handler may mutate a buffer directly.
  Three things need to reach editing behaviour — the keymap, the command palette, the planned
  vim layer — and a primitive that only a key handler can call is invisible to all three.
- **Every key binding is a row in a table**, never a match arm. `typ-core/src/keymap.rs`.
- **`col` is a grapheme index.** Never a byte offset, never a char offset, anywhere. This is
  what keeps the cursor correct on CJK and emoji.
- **Panels never see application state.** They get a `RenderContext` and return events.
- **Mouse and keyboard are peers.** If you add an interaction, add both, and test both.

## Tests

Write the failing test first and check it fails for the reason you expect. This is not
ceremony — plans in this project have been wrong, and the failing-test step is what caught it
each time.

Name tests after the behaviour, not the function:

```rust
#[test]
fn a_dirty_buffer_is_never_silently_reloaded() { ... }
```

Integration tests live in `crates/<crate>/tests/`. Most of this codebase is tested from the
outside, through actions and rendered frames, rather than through private functions.

## Commits

Conventional Commits. The subject carries the change; add a body only for the part that is not
obvious from the diff — a decision, a trade-off, a trap someone else would hit.

```
feat(editor): Ctrl+D selects the next occurrence
fix(buffer): cap the undo stack at 1000 steps
```

## Finding something to work on

`docs/design/gap-analysis.md` is the defect register. Anything marked LOW or MED with no
milestone next to it is fair game. Good ones to start with:

- **13** — `last_click` is not cleared by keyboard motion, so click, arrow away, click the same
  cell selects a word instead of placing a caret.
- **11** — dragging past the edge of the viewport does not autoscroll.
- **6** — no tty check, so `typ | cat` writes escape sequences into a pipe.
- **39** — comment density is about double what idiomatic Rust carries. Mechanical, low risk.

## AI-assisted contributions

Use whatever tools you like. The bar is the same either way: you understand the change, the
tests are real, and you ran the commands above and can paste the output. PRs that were clearly
generated and not read do not get reviewed.

## License

MIT. By contributing you agree your work ships under it.
