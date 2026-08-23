#!/bin/sh
# TYPE installer for macOS and Linux.
#
#     curl --proto '=https' --tlsv1.2 -fsSL https://raw.githubusercontent.com/Pranjal-SB/type/main/install.sh | sh
#
# Downloads the release archive for this machine, checks it against the SHA-256
# published beside it, and puts `typ` in ~/.local/bin. No sudo, ever: a
# per-user install that never asks for a password beats a system-wide one that
# does, and an editor has no business wanting root.
#
# Two structural decisions worth knowing before editing:
#
#   * The whole body is a function, called on the last line. `curl | sh` starts
#     executing before the response has finished arriving, so a connection that
#     drops mid-transfer can otherwise run half a script. A script whose only
#     top-level statement is the final call either runs completely or does
#     nothing at all. Do not move work out of a function to "simplify" it.
#
#   * POSIX sh, and `set -eu` rather than `set -euo pipefail`. `pipefail` is not
#     POSIX; on Debian and Ubuntu `sh` is dash, where `set -o pipefail` is an
#     error — and this script is piped into exactly that shell by the command
#     above. Pipelines whose failure matters are checked explicitly instead.
#
# Tested by tests/install_test.sh.

set -eu

REPO="Pranjal-SB/type"
BASE_URL="${TYP_BASE_URL:-https://github.com/$REPO/releases}"

say() { printf '%s\n' "$*"; }
err() { printf '%s\n' "$*" >&2; }

# Which release archive this machine wants.
#
# Linux resolves to musl, never gnu. The gnu build tracks whatever glibc the
# release runner had — 2.39 at the time of writing — and does not start on
# Ubuntu 22.04, Debian 12, RHEL 9 or Amazon Linux 2023. musl is statically
# linked and runs on all of them. Picking gnu here would reintroduce the exact
# defect this installer was written alongside.
detect_target() {
    detect_os=$1
    detect_arch=$2
    case "$detect_os" in
    Linux)
        case "$detect_arch" in
        x86_64 | amd64) say "x86_64-unknown-linux-musl" ;;
        aarch64 | arm64) say "aarch64-unknown-linux-musl" ;;
        *)
            err "unsupported architecture: $detect_arch"
            err "Linux builds are published for x86_64 and aarch64."
            return 1
            ;;
        esac
        ;;
    Darwin)
        case "$detect_arch" in
        x86_64) say "x86_64-apple-darwin" ;;
        arm64 | aarch64) say "aarch64-apple-darwin" ;;
        *)
            err "unsupported architecture: $detect_arch"
            return 1
            ;;
        esac
        ;;
    *)
        err "unsupported operating system: $detect_os"
        err "Windows has its own installer: install.ps1"
        return 1
        ;;
    esac
}

# The only function that touches the network.
#
# --proto '=https' so a redirect cannot downgrade the transport, --tlsv1.2 as a
# floor, -f so an HTTP error is a failure rather than a saved error page. It is
# also why the tests do not drive this end: a file:// fixture cannot be served
# over a connection pinned to https, and weakening the flag to make a test pass
# would be testing a script nobody ships.
download() {
    download_url=$1
    download_dest=$2
    if command -v curl >/dev/null 2>&1; then
        curl --proto '=https' --tlsv1.2 -fsSL "$download_url" -o "$download_dest"
    elif command -v wget >/dev/null 2>&1; then
        wget --https-only -qO "$download_dest" "$download_url"
    else
        err "need curl or wget"
        return 1
    fi
}

# `sha256sum` on Linux, `shasum` on macOS. Both read the filename out of the
# sum file, so this has to run in the directory holding the archive.
checksum_ok() {
    checksum_file=$1
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum -c "$checksum_file" >/dev/null 2>&1
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 -c "$checksum_file" >/dev/null 2>&1
    else
        err "no sha256 tool found (looked for sha256sum and shasum)"
        return 1
    fi
}

# Everything after the bytes arrive. Split out from the download so the tests
# can drive it against a local fixture.
#
# Nothing is written outside the temporary directory until the checksum has
# passed. A tampered or truncated archive must not leave a half-installed
# binary behind, and the cheapest way to guarantee that is to do the risky part
# somewhere disposable.
verify_and_install() {
    archive=$1
    sumfile=$2
    bindir=$3

    archive_dir=$(dirname "$archive")
    if ! (cd "$archive_dir" && checksum_ok "$(basename "$sumfile")"); then
        err "checksum mismatch for $(basename "$archive") — refusing to install"
        err "The download was corrupted, interrupted, or tampered with."
        return 1
    fi

    unpack_dir=$(mktemp -d)
    if ! tar xzf "$archive" -C "$unpack_dir"; then
        rm -rf "$unpack_dir"
        err "could not unpack $archive"
        return 1
    fi

    found=""
    for candidate in "$unpack_dir"/*/typ "$unpack_dir"/typ; do
        [ -f "$candidate" ] && found=$candidate
    done
    if [ -z "$found" ]; then
        rm -rf "$unpack_dir"
        err "no typ binary inside $archive"
        return 1
    fi

    mkdir -p "$bindir"
    cp "$found" "$bindir/typ"
    chmod 755 "$bindir/typ"
    rm -rf "$unpack_dir"
    say "installed $bindir/typ"
}

# A trailing note only when it is useful. Telling someone their PATH is already
# correct is noise; not telling them when it is not is a broken install.
path_hint() {
    hint_dir=$1
    case ":$PATH:" in
    *":$hint_dir:"*) ;;
    *)
        say ""
        say "$hint_dir is not on your PATH. Add it:"
        say ""
        say "    export PATH=\"\$PATH:$hint_dir\""
        ;;
    esac
}

usage() {
    cat <<'EOF'
Install TYPE, the terminal IDE.

USAGE:
    install.sh [OPTIONS]

OPTIONS:
    --version <TAG>   Install a specific release, e.g. v0.2.6. Default: latest.
    --bin-dir <DIR>   Where to put the binary. Default: $TYP_BIN_DIR, or
                      ~/.local/bin.
    -h, --help        Print this help.
EOF
}

main() {
    version="latest"
    bindir="${TYP_BIN_DIR:-$HOME/.local/bin}"

    while [ $# -gt 0 ]; do
        case "$1" in
        --version)
            [ $# -ge 2 ] || {
                err "--version needs a tag"
                return 1
            }
            version=$2
            shift 2
            ;;
        --bin-dir)
            [ $# -ge 2 ] || {
                err "--bin-dir needs a directory"
                return 1
            }
            bindir=$2
            shift 2
            ;;
        -h | --help)
            usage
            return 0
            ;;
        *)
            err "unknown option: $1"
            usage >&2
            return 1
            ;;
        esac
    done

    target=$(detect_target "$(uname -s)" "$(uname -m)")

    if [ "$version" = "latest" ]; then
        url_base="$BASE_URL/latest/download"
        # The archive name carries the tag, which "latest" does not tell us, so
        # ask the redirect where it lands rather than guessing.
        resolved=$(
            curl --proto '=https' --tlsv1.2 -fsSLI -o /dev/null \
                -w '%{url_effective}' "$BASE_URL/latest" 2>/dev/null || true
        )
        version=${resolved##*/}
        [ -n "$version" ] || {
            err "could not work out the latest release; pass --version"
            return 1
        }
        url_base="$BASE_URL/download/$version"
    else
        url_base="$BASE_URL/download/$version"
    fi

    name="typ-$version-$target"
    workdir=$(mktemp -d)
    # Clean up whatever happens next, including a failed download.
    trap 'rm -rf "$workdir"' EXIT INT TERM

    say "downloading $name"
    download "$url_base/$name.tar.gz" "$workdir/$name.tar.gz"
    download "$url_base/$name.tar.gz.sha256" "$workdir/$name.tar.gz.sha256"

    verify_and_install "$workdir/$name.tar.gz" "$workdir/$name.tar.gz.sha256" "$bindir"
    path_hint "$bindir"
}

# Sourced by the tests to get the functions without running anything; the guard
# is also what keeps a truncated download from doing work, so it stays last.
[ "${TYP_INSTALL_LIB:-}" = "1" ] || main "$@"
