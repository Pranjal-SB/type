# Distribution

How a build becomes something a person can run. Covers the target matrix, the release
pipeline, the install channels, and the repository tooling around them.

Companion to [`gap-analysis.md`](gap-analysis.md) Part 7, which designs *first launch* — the
setup wizard, symbol presets, the glyph question. This document is the layer underneath: what
gets built, for whom, and how it arrives. Measurements taken 2026-08-23 against v0.2.5.

## 1. The defect that prompted this

The published Linux binary does not run on most Linux.

```
$ gh release download v0.2.5 -p '*linux-gnu.tar.gz'
$ strings typ | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -3
GLIBC_2.33
GLIBC_2.34
GLIBC_2.39
```

`release.yml` builds on `ubuntu-latest`, which has meant Ubuntu 24.04 since January 2025, and
24.04 carries glibc 2.39. A binary linked against 2.39 will not start anywhere older:

| Distribution | glibc | Runs v0.2.5? |
|---|---|---|
| Ubuntu 24.04, Debian 13 | 2.39 / 2.41 | yes |
| Ubuntu 22.04 LTS | 2.35 | **no** |
| Debian 12 (bookworm) | 2.36 | **no** |
| RHEL 9, Rocky 9, Alma 9 | 2.34 | **no** |
| Amazon Linux 2023 | 2.34 | **no** |

The failure is `/lib/x86_64-linux-gnu/libc.so.6: version 'GLIBC_2.39' not found`, which reads
to the person who typed it as a broken editor rather than a wrong build. This is the most
consequential defect in the project right now, because it is the only one that reaches someone
who has never seen the editor work.

It is not an exotic problem. Helix's most recent release, 25.07.1, exists for exactly this
reason: *"a patch release which lowers the GLIBC requirements of the release artifacts."*

## 2. What does work

Verified rather than assumed, because the pipeline had never been exercised end to end.

```
$ gh release download v0.2.5 -p '*windows-msvc*'
$ sha256sum -c typ-v0.2.5-x86_64-pc-windows-msvc.zip.sha256
typ-v0.2.5-x86_64-pc-windows-msvc.zip: OK
$ unzip -q *.zip && ./typ-v0.2.5-x86_64-pc-windows-msvc/typ.exe --version
typ 0.2.5
```

Archive layout, checksum, and binary are all correct. The three other archives come out of the
same steps. The pipeline is sound; the target list is not.

## 3. The field, measured

Read from release pages and install scripts on 2026-08-23.

| Project | Linux targets | One-line installer | Checksums |
|---|---|---|---|
| **Helix** | x86_64, aarch64, AppImage, `.deb` | none | none |
| **Neovim** | x86_64, arm64, AppImage | none | none |
| **Kakoune** | source only | none | — |
| **micro** | 5 arches + 5 BSDs | `curl https://getmic.ro \| bash` | `.sha` per asset |
| **TermIDE** | — | `curl -fsSL .../install.sh \| sh` | — |
| **Zed** | x86_64, aarch64 | `curl https://zed.dev/install.sh \| sh` | **none** |
| **ripgrep** | gnu + musl per arch, 8 arches | none | `.sha256` per asset |
| **bat** | gnu + musl per arch, 13 targets | none | none |
| **starship** | musl by default | `curl -sS https://starship.rs/install.sh \| sh` | none |
| **atuin** | via cargo-dist | `curl --proto '=https' --tlsv1.2 -LsSf https://setup.atuin.sh \| sh` | via dist |
| **oh-my-pi** | — | `install.sh` + `install.ps1`, under 200 lines | sha256 **and cosign** |
| **TYPE v0.2.5** | x86_64 gnu only | none | `.sha256` per asset |

Two things fall out of that table.

**The one-line installer is a minority among editors.** Helix and Neovim, the two that matter
most to the same people TYPE wants, ship none. The tools that do ship one are mostly single
binaries with no runtime directory to place. So the installer is a differentiator here rather
than table stakes — worth building, but not the thing that is currently broken.

**Target coverage is where every serious project spends the effort instead.** ripgrep and bat
both ship gnu *and* musl for every architecture. TYPE ships one target for one architecture on
the platform with the most fragmentation. Nobody in the table has narrower Linux coverage.

Archive naming is the one thing already right, and it was right by following the obvious
convention:

```
typ-v0.2.5-x86_64-unknown-linux-gnu.tar.gz
ripgrep-15.2.0-x86_64-unknown-linux-gnu.tar.gz
bat-v0.26.1-x86_64-unknown-linux-gnu.tar.gz
```

It is *not*, however, enough for `cargo-binstall`. Its default template interpolates the
**crate** name, and ours is `typ-editor` while every archive is named for the **binary**,
`typ`. So binstall looks for `typ-editor-v0.2.5-…` and finds nothing. A
`[package.metadata.binstall]` block naming the real layout is required rather than a nicety.

## 4. The target matrix

**Static musl is the answer to distro fragmentation, and it is cheap right now.**

A musl build links libc statically. There is no glibc version to be too new, no
`GLIBC_2.39 not found`, and one file covers Ubuntu, Debian, RHEL, Alpine, Void, NixOS and
everything else. This is what starship defaults to and what ripgrep and bat ship alongside gnu.

**Why "right now" is load-bearing.** Nothing in the graph compiles C — no `cc`, `cmake`,
`bindgen` or `pkg-config` — and none of TYPE's own crates carries a `build.rs`. So
`rustup target add x86_64-unknown-linux-musl` plus a `--target` flag is the entire change: no
`musl-tools`, no `musl-gcc`, no `cross`, no container.

Two qualifications, both learned by doing it on a clean box rather than by reading the
lockfile. Several dependencies *do* have build scripts — `libc`, `signal-hook`,
`parking_lot_core` — but they are ordinary Rust programs, not C. And building any Rust binary
for Linux still needs a linker driver:

```
error: linker `cc` not found
```

That is true of the gnu target as much as musl, it is one `apt-get install gcc`, and GitHub's
runners ship it. It is a fact about Linux, not a cost of this change.

**M2.7 ends the easy version.** Tree-sitter grammars are C; taking them puts a real C
toolchain in the path of every musl and every cross build, and the matrix then needs `cross` or
a hand-built sysroot. Doing this before M2.7 costs a matrix entry. Doing it after costs a build
system.

aarch64 Linux was left out of `release.yml` with the note that it needs a cross linker. That
is now out of date: GitHub's `ubuntu-24.04-arm` runners have been free for public repositories
since January 2025, so aarch64 is a native build on a native runner and needs no linker at all.

Proposed matrix — six targets, two more runners, no new tooling:

| Target | Runner | Covers |
|---|---|---|
| `x86_64-unknown-linux-musl` | `ubuntu-latest` | every x86_64 Linux, any age |
| `aarch64-unknown-linux-musl` | `ubuntu-24.04-arm` | Graviton, Pi, Asahi, arm64 servers |
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` | a dynamically linked build, current glibc only |
| `x86_64-apple-darwin` | `macos-latest` | Intel Macs, cross-compiled from the arm runner |
| `aarch64-apple-darwin` | `macos-latest` | Apple Silicon |
| `x86_64-pc-windows-msvc` | `windows-latest` | Windows |

**musl is the compatible build; gnu is not, and should stop pretending to be.** The obvious
alternative was to pin the gnu row to `ubuntu-22.04` and buy a glibc 2.35 floor for a one-word
diff. That is a fix with an expiry date: 22.04 begins deprecation on 2026-09-17 and is
unsupported by 2027-04-17, and it still excludes RHEL 9 and Amazon Linux 2023 at glibc 2.34.
Chasing an old runner is the wrong shape of answer to a problem static linking solves outright.
So gnu stays on `ubuntu-latest`, documented as the current-glibc build, and musl is what the
installer reaches for.

The Apple rows need no Intel runner. `macos-13` was retired in December 2025 and
`macos-15-intel` — the last x86_64 image Actions will offer — goes in August 2027. Neither
matters here: the arm64 macOS runner cross-compiles `x86_64-apple-darwin` with the stock
toolchain, which is already what produces the current artifact.

Deliberately not included: 32-bit anything, ARM32, the BSDs, `.deb` and `.rpm`. Community
territory once demand exists, per Part 7's channel table.

### musl's allocator, measured

The one thing that could have made this whole section the wrong answer, now settled with
numbers. Best of five per column, one 12-core Linux host, nothing else running:

| metric | gnu | musl | musl + jemalloc | budget |
|---|---|---|---|---|
| `find_all`, rare needle | 4.11 ms | **10.17 ms** | 4.28 ms | 16 ms |
| `find_all`, match on every line | 97.6 ms | 165.3 ms | 98.6 ms | — |
| render, unmatched bracket | 161 µs | 366 µs | 248 µs | 16 ms |
| `InsertChar` | 1.10 µs | 1.54 µs | 1.25 µs | 16 ms |
| `Undo+Redo` | 146 ns | 275 ns | 137 ns | 32 ms |

Plain musl was worse than glibc's *worst* run on every one of its five `find_all` runs —
10.17, 10.46, 11.86, 11.99 and 13.66 ms against a 16 ms gate. The test still passed. That is
the point: a budget with no margin left passes right up until it does not, and `find_all` is
the one budget architecture §4 already flags as having less than an order of magnitude of
headroom.

**The cause was confirmed, not assumed.** `find_all`'s hot path is `line.contains(needle)`,
which is pure Rust and never enters libc, so "musl is slower" was not on its own an
explanation. A throwaway probe declaring jemalloc in `tests/perf.rs` and changing nothing else
took it from 10.17 ms to 4.05 ms. It is the allocator, and jemalloc lands marginally ahead of
glibc rather than merely level with it. ripgrep carries the same swap for the same reason:

```rust
#[cfg(all(target_env = "musl", target_pointer_width = "64"))]
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;
// "musl's allocator appears to slow down ripgrep quite a bit"
```

**What it costs.** `tikv-jemalloc-sys` is C with an autotools build, and it wants a
musl-targeting compiler rather than the host one:

```
ToolNotFound: failed to find tool "x86_64-linux-musl-gcc"
configure: error: C compiler cannot create executables
```

So `release.yml` installs `musl-tools` on the musl rows only. This is the first C in the graph,
and the "pure Rust, so this is free" argument above stops applying to musl builds — still far
cheaper than doing it after tree-sitter, but no longer free.

The swap is declared in `crates/typ/src/main.rs` for the shipped binary **and in both
`tests/perf.rs` files**, so the perf suite measures what users run. Without the second half the
musl column would measure `mallocng`, which no released build ever uses.

## 5. The installer

Two scripts, `install.sh` and `install.ps1`, both consuming release assets that already exist.
Neither needs a domain to work; `raw.githubusercontent.com` serves them today and a short URL
is a cosmetic upgrade later.

What they do, in order: detect OS and architecture, prefer musl on Linux, resolve the latest
tag through the GitHub API, download the archive **and its `.sha256`**, verify, unpack to a
temporary directory, move the binary into place, and print the PATH line if the destination is
not already on `PATH`.

Default destination `~/.local/bin` on Unix and `%LOCALAPPDATA%\Programs\typ` on Windows.
Neither needs `sudo`. starship escalates to `sudo` for `/usr/local/bin`, and that is the wrong
trade for an editor: a per-user install that never asks for a password beats a system-wide one
that does.

Three implementation details that are not obvious, and are the reason to write these by hand
rather than adopt a generator:

- **Wrap the body in `main()` and call it on the last line.** `curl | sh` starts executing
  before the response has finished arriving, so a dropped connection can run half a script. A
  script whose only top-level statement is the final call either runs completely or does
  nothing.
- **Verify the checksum.** Zed, starship and bat all ship installers or archives with no
  integrity check at all. TYPE already publishes a `.sha256` beside every asset, so verifying
  costs four lines and puts the installer above every editor in the table.
- **`--proto '=https' --tlsv1.2` on the curl invocation**, as atuin does, so a redirect cannot
  downgrade the transport.

`set -euo pipefail`, and a `--version` / `--bin-dir` flag pair so the script is testable
against a fixture rather than only against the live release.

**Not adopted: `cargo-dist`.** It generates these two scripts plus Homebrew and winget, and it
is the right answer at M6 when Windows file association makes those channels earn their keep.
Today it would replace 120 readable lines with several hundred generated ones to solve a
problem that is two files wide.

## 6. Repository tooling

Surveyed `.github/` across helix, ripgrep, starship, bat, zellij, uv and ruff.

| | helix | ripgrep | starship | bat | zellij | uv / ruff | TYPE |
|---|---|---|---|---|---|---|---|
| Dependency bot | dependabot | — | renovate | dependabot | dependabot | renovate | **none** |
| MSRV job | — | yes | — | yes | — | yes | implicit |
| Advisory scan | — | — | `security-audit.yml` | — | — | yes | `cargo deny` |
| Issue templates | yes | yes | yes | yes | yes | yes | yes |
| PR template | — | — | yes | — | — | yes | yes |
| `FUNDING.yml` | yes | yes | yes | yes | yes | — | — |
| `CODEOWNERS` | — | — | — | — | — | yes | — |
| Spell check | — | — | `spell-check.yml` | — | — | yes | — |
| Workflow linting | — | — | — | — | — | `zizmor` | — |
| Build provenance | — | — | — | — | — | yes | — |

The CI that exists is good and does not need rework: three platforms, fmt, clippy at
`-D warnings`, the whole test suite, and `cargo deny` over advisories, licenses, bans and
sources with every license in the graph named individually rather than wildcarded. That is
already ahead of most of the table.

What is missing, in the order it is worth adding:

1. **`dependabot.yml`** — five lines, two ecosystems, `cargo` and `github-actions`. Every
   project in the table has this or renovate. The `github-actions` half matters more than it
   looks: `actions/checkout@v4` and `Swatinem/rust-cache@v2` are floating major tags that
   nothing currently watches.
2. **Perf tests in CI** — gap-analysis defect #18, still open, and the one genuinely unusual
   gap. The budgets are the project's stated identity and nothing enforces them. A weekly
   `schedule:` job running the `#[ignore]`d perf tests, best-of-five as the test files already
   do, is the smallest version that closes it.
3. **Build provenance** — `actions/attest-build-provenance` signs the release archives through
   Sigstore and makes `gh attestation verify` work against them. Around ten lines and a
   `permissions:` block. oh-my-pi does this with cosign; GitHub now does it natively.
4. **Release verification** — the pipeline had never been checked end to end until §2 of this
   document. A job that downloads its own artifacts, verifies the checksums and runs
   `typ --version` on the runners that can execute their own output would have caught the
   glibc defect at the tag rather than at the install.
5. **`typos`** — cheap, and this repository is documentation-heavy.

Not worth adopting: `zizmor` and `CODEOWNERS` answer problems a many-contributor repo has.
`FUNDING.yml` is a decision about the project rather than about tooling.

## 7. Sequencing

This should land **before** M2.6 rather than inside or after it.

Three reasons, in decreasing weight:

1. The glibc defect is live on a published release. Every day it stands, someone who tries
   TYPE on a stock server distribution concludes it does not work.
2. Pure-Rust musl is a matrix entry today and a build system after tree-sitter arrives. The
   cost of this work only goes up.
3. Everything here is verifiable by the same means — download the artifact, check the sum, run
   the binary — and none of it depends on a design decision M2.6 might change.

Against: it is a milestone spent on nothing a current user can see. True, and the wrong frame.
The people it reaches are the ones who do not exist yet because the binary did not start.

## Sources

- [Helix 25.07.1 — the GLIBC patch release](https://github.com/helix-editor/helix/releases)
- [`ubuntu-latest` moved to 24.04](https://github.com/actions/runner-images/issues/10636)
- [Linux arm64 runners, free for public repositories](https://github.blog/changelog/2025-01-16-linux-arm64-hosted-runners-now-available-for-free-in-public-repositories-public-preview/)
- [starship `install.sh`](https://github.com/starship/starship/blob/master/install/install.sh)
- [Zed `install.sh`](https://zed.dev/install.sh)
- [micro — `getmic.ro`](https://github.com/zyedidia/micro)
- [bat `CICD.yml` — the 13-target matrix](https://github.com/sharkdp/bat/blob/master/.github/workflows/CICD.yml)
- [ripgrep `ci.yml` — cross, musl, MSRV](https://github.com/BurntSushi/ripgrep/blob/master/.github/workflows/ci.yml)
- [cargo-binstall — discovery and `[package.metadata.binstall]`](https://github.com/cargo-bins/cargo-binstall)
- [cargo-dist](https://axodotdev.github.io/cargo-dist/)
- [atuin installation](https://docs.atuin.sh/guide/installation/)
- [Curl to shell is not so bad — the partial-download hazard](https://www.arp242.net/curl-to-sh.html)
- [`actions/attest-build-provenance`](https://github.com/actions/attest-build-provenance)
