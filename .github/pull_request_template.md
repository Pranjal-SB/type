<!--
CONTRIBUTING.md has the reasoning. This is the checklist version.
-->

## What this changes



## Checks

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

<!-- Paste the real output. A claim without it is a guess. -->

- [ ] Tests, clippy and fmt are clean, and the output is pasted above
- [ ] New behaviour has a test named after the behaviour, written before the code
- [ ] Any new editing primitive is an `Action`; no `handle_key` arm mutates a buffer
- [ ] Any new interaction works from both the mouse and the keyboard, and both are tested

If this touches the editing or render path, the perf numbers too — `--release
--ignored`, and best-of-five for anything near the 16 ms budget:

```
cargo test --release -p typ-buffer --test perf -- --ignored --nocapture
cargo test --release -p typ-panel-editor --test perf -- --ignored --nocapture
```
