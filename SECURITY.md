# Security

## Reporting a vulnerability

Use GitHub's [private vulnerability reporting](https://github.com/Pranjal-SB/type/security/advisories/new)
rather than a public issue. It goes to the maintainer only, and it gives us a place to fix the
thing before it is described in public.

Expect a first reply within a week. This is a one-person project, so that is a realistic number
rather than an SLA.

## What is in scope

TYPE is an editor: it reads files you point it at, writes them back, watches them for external
changes, and talks to your terminal. The interesting boundaries are:

- **File reading and writing.** Path handling, symlink and hard-link preservation, mode bits,
  the temp-file-and-rename save path.
- **Configuration and theme loading.** TOML parsed out of the user's config directory, and
  themes loaded by name from that directory.
- **Terminal escape sequences.** Both directions — what TYPE emits, and what it parses from
  capability queries and pasted input. Bracketed paste is the classic place an editor gets
  used as a shell-injection gadget.
- **The release artifacts.** Archives published on the releases page, their checksums, and
  their build provenance.

Out of scope: anything that requires an attacker to already be able to run code as your user,
and denial of service by opening a pathologically large or malformed file — that is a
[performance budget](AGENTS.md) question and belongs in an issue.

## Verifying a release

Every archive ships a `.sha256` beside it:

```
sha256sum -c typ-v0.2.5-x86_64-unknown-linux-gnu.tar.gz.sha256
```

From v0.2.6 the archives also carry GitHub build provenance, signed through Sigstore, which
proves the binary came out of this repository's release workflow rather than off someone's
laptop:

```
gh attestation verify typ-v0.2.6-x86_64-unknown-linux-musl.tar.gz --repo Pranjal-SB/type
```

## Supply chain

`cargo deny check advisories licenses bans sources` runs on every push and pull request, with
every licence in the dependency graph named individually rather than matched by wildcard, so a
dependency arriving with something unexpected fails the build instead of sliding in.

GitHub Actions are pinned to commit SHAs rather than tags, and Dependabot updates them weekly.
