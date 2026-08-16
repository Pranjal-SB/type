---
type: process
status: living
area: release
verified: 2026-08-16
verified-against: v0.2.3
---

# Releasing

Written because the manual half of this drifted: crates.io served 0.2.1 while the tree and the
tag were at 0.2.3. A release that depends on someone remembering an order is a release that
lags by however long they forget.

Binaries are automated. Publishing is not, and cannot be — crates.io needs a token this
repository deliberately does not hold.

## 1. Close the milestone out

Part of the milestone's last commit, not a separate chore:

- `version` in `[workspace.package]`, and the six `typ-*` path dependencies beside it. They
  carry an explicit version because cargo refuses to publish a crate whose dependencies are
  path-only, and a stale one there publishes a package that cannot resolve.
- The README's **Status** line and its **Roadmap** table.
- `CHANGELOG.md`: a dated section, and the link block at the bottom — the `[Unreleased]`
  compare link points at the new tag.

## 2. Verify

```
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
cargo test --release -p typ-buffer --test perf -- --ignored --nocapture
cargo test --release -p typ-panel-editor --test perf -- --ignored --nocapture
```

The perf tests are the ones CI does not run. Nobody else is going to notice a budget
regression.

## 3. Tag

```
git tag v0.2.4
git push origin main --tags
```

The tag is the trigger. `.github/workflows/release.yml` builds four targets — Linux x86_64,
macOS x86_64 and aarch64, Windows x86_64 — packages each with the README, LICENSE and
CHANGELOG, writes a SHA-256 beside it, and opens a **draft** release.

Draft on purpose: check the four archives are attached and the notes read correctly, then
publish by hand. `workflow_dispatch` runs the same pipeline against an existing tag, which is
how to exercise it without cutting a release.

## 4. Publish to crates.io

Seven crates, and **the order is a dependency order** — cargo will not accept a crate whose
dependencies are not already on the registry at the version it names:

```
cargo publish -p typ-core
cargo publish -p typ-buffer
cargo publish -p typ-registry
cargo publish -p typ-panel-tree
cargo publish -p typ-panel-editor
cargo publish -p typ-app
cargo publish -p typ-editor
```

`typ-core` and `typ-buffer` have no internal dependencies and can go in either order; every
line after them depends on something above it. The registry takes a moment to index each one,
so a failure on the next line usually means "wait and retry", not "wrong order".

`typ-editor` is the package that carries the `typ` binary, and it goes last.

## Regenerating the demo

```
cargo build --release
vhs assets/demo.tape
```

[VHS](https://github.com/charmbracelet/vhs) renders `assets/demo.gif` from a script, so the
README's demo is rebuilt per release rather than re-recorded by hand. Worth doing whenever the
milestone changed something visible — the gutter, the theme, the status bar.
