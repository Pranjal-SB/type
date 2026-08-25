---
type: process
status: living
area: release
verified: 2026-08-25
verified-against: v0.2.8
---

# Releasing

Written because the manual half of this drifted: crates.io served 0.2.1 while the tree and the
tag were at 0.2.3. A release that depends on someone remembering an order is a release that
lags by however long they forget.

Binaries are automated, and so is publishing the GitHub release — it publishes itself once it
has verified itself. crates.io is the one step that stays manual and cannot be otherwise: it
needs a token this repository deliberately does not hold.

## 1. Close the milestone out

Part of the milestone's last commit, not a separate chore:

- `version` in `[workspace.package]`, and the nine `typ-*` path dependencies beside it.
  `crates/typ/tests/manifests.rs` checks both this and the metadata crates.io requires, because
  the checklist below is what failed at v0.2.7 and again at v0.2.8. They
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

The perf tests do not run on a pull request — a shared runner's wall clock is not a gate. They
run weekly in `.github/workflows/perf.yml` and by hand here. A budget regression that lands on
a Tuesday should not wait until a release to be noticed, and before a release is still the last
place to check.

The installer tests are cheap and worth running before a tag, because they are the one thing
whose failure a user hits before they ever get a binary:

```
sh tests/install_test.sh
powershell -NoProfile -File tests\install_test.ps1
```

## 3. Tag

```
git tag v0.2.4
git push origin main --tags
```

The tag is the trigger. `.github/workflows/release.yml` builds six targets — Linux x86_64 and
aarch64 on musl, Linux x86_64 on glibc, macOS x86_64 and aarch64, Windows x86_64 — packages
each with the README, LICENSE, CHANGELOG and a generated `THIRD-PARTY-LICENSES.md`, writes a
SHA-256 beside it, attests build provenance, and opens a **draft** release.

Then it checks its own work. A `verify` job downloads each archive back off the release,
confirms the checksum, unpacks it, runs the binary, and asserts the version it prints matches
the tag. Only if that passes does a `publish` job flip the draft. A tag containing a hyphen
(`v0.2.6-rc.1`) is published as a prerelease, so it does not become what `install.sh` resolves
as latest.

That sequence replaced a human clicking publish after no checks at all, which is how four tags
shipped before anyone downloaded an artifact and ran it — and how the Linux build turned out
not to start on most Linux. See gap-analysis #45 and #48.

`workflow_dispatch` runs the same pipeline against an existing tag, which is how to exercise it
without cutting a release. **Do that first for anything structural**: immutable releases are
enabled, so a published release's assets can no longer be corrected in place. Cut a release
candidate rather than rewriting a tag someone may already have installed.

Nothing to do by hand here beyond reading the notes. If `verify` fails, the release stays a
draft nobody saw.

**Rehearse anything structural on a candidate, and do not skip it because the change looks
small.** v0.2.6 took four: `rc.1` failed building on Windows (`$TARGET` is undefined in a pwsh
step and expands to nothing), `rc.2` failed on every verify row because a draft release is
invisible to a token with only `contents: read`, `rc.3` proved the version assertion actually
fires by being pointed at a wrong string on purpose, and `rc.4` was the first clean run. The
first two would each have landed on a permanent tag.

## 4. Publish to crates.io

Ten crates, and **the order is a dependency order** — cargo will not accept a crate whose
dependencies are not already on the registry at the version it names:

```
cargo publish -p typ-syntax
cargo publish -p typ-find
cargo publish -p typ-core
cargo publish -p typ-buffer
cargo publish -p typ-registry
cargo publish -p typ-panel-tree
cargo publish -p typ-panel-editor
cargo publish -p typ-picker
cargo publish -p typ-app
cargo publish -p typ-editor
```

`typ-syntax` and `typ-find` go **first**, ahead of `typ-core`, which is the position that is easy
to get wrong: both look like leaves, but `typ-core`'s `AppEvent` carries a `typ_syntax::Parsed`
and a `typ_find::Found`, so they are the bottom of the graph rather than the top. Neither depends
on anything of TYPE's in either dependency table — deliberately, because a dev-dependency back
onto `typ-core` would build locally and fail here. `typ-buffer` has no internal dependencies
either and can swap with `typ-core`; every line after them depends on something above it.
`typ-picker` needs `typ-core` and `typ-find`, so it sits below `typ-app`. The registry takes a
moment to index each one, so a failure on the next line usually means "wait and retry", not
"wrong order".

`typ-editor` is the package that carries the `typ` binary, and it goes last.

## 5. Run the installers against the release you just cut

The pipeline verifies the archives. It does not verify the scripts that fetch them, and those
resolve "latest" through two different mechanisms — a redirect on Unix, the API on Windows —
either of which can break without a single archive being wrong.

```
sh install.sh --bin-dir /tmp/typ-check && /tmp/typ-check/typ --version
powershell -NoProfile -File install.ps1 -BinDir $env:TEMP\typ-check
```

Best done somewhere that has never built the project, which is the only way to find out whether
the thing a stranger runs works. A container is enough:

```
docker run --rm -it debian:bullseye sh -c \
  'apt-get update -qq && apt-get install -y -qq curl >/dev/null &&
   curl --proto "=https" --tlsv1.2 -fsSL https://raw.githubusercontent.com/Pranjal-SB/type/main/install.sh | sh &&
   ~/.local/bin/typ --version'
```

bullseye is glibc 2.31 — old enough that the gnu build cannot start on it, so this also
confirms the installer is reaching for musl rather than merely reaching for something.

## Regenerating the demo

```
cargo build --release
vhs assets/demo.tape
```

[VHS](https://github.com/charmbracelet/vhs) renders `assets/demo.gif` from a script, so the
README's demo is rebuilt per release rather than re-recorded by hand. Worth doing whenever the
milestone changed something visible — the gutter, the theme, the status bar.
