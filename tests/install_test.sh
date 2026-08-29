#!/bin/sh
# Tests for install.sh. Plain POSIX sh and asserts rather than a framework:
# the thing under test is a shell script that has to run on whatever `sh` a
# stranger's machine provides, and testing it with a tool that machine may not
# have is testing the wrong thing.
#
#     sh tests/install_test.sh
#
# The network half is deliberately not covered here. install.sh pins
# `--proto '=https'` so a redirect cannot downgrade the transport, which means
# a `file://` fixture cannot exercise the download path — and weakening the
# flag to make a test pass would be testing a script nobody ships. What is
# covered is everything after the bytes arrive, which is where the failure
# modes that matter live.

set -eu

# shellcheck disable=SC1007  # `CDPATH=` is a deliberate empty assignment: an
# exported CDPATH makes `cd` print and jump elsewhere, which would silently
# resolve the repo root to the wrong directory.
here=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
script="$here/install.sh"
pass=0
fail=0

ok() {
    pass=$((pass + 1))
    printf 'ok   %s\n' "$1"
}

no() {
    fail=$((fail + 1))
    printf 'FAIL %s\n   %s\n' "$1" "${2:-}"
}

# A throwaway release: an archive laid out exactly as release.yml builds one,
# plus the .sha256 that ships beside it.
make_fixture() {
    root=$1
    name=typ-v9.9.9-x86_64-unknown-linux-musl
    mkdir -p "$root/$name"
    printf '#!/bin/sh\necho "typ 9.9.9"\n' > "$root/$name/typ"
    chmod +x "$root/$name/typ"
    echo 'notices' > "$root/$name/THIRD-PARTY-LICENSES.md"
    (cd "$root" && tar czf "$name.tar.gz" "$name" && rm -rf "$name")
    if command -v sha256sum >/dev/null 2>&1; then
        (cd "$root" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
    else
        (cd "$root" && shasum -a 256 "$name.tar.gz" > "$name.tar.gz.sha256")
    fi
    echo "$root/$name.tar.gz"
}

# Sourcing gives the tests the functions without running main. The guard is the
# last line of install.sh, so a truncated download still reaches nothing.
TYP_INSTALL_LIB=1
export TYP_INSTALL_LIB
# shellcheck source=/dev/null
. "$script"

# --- a good archive installs the binary ---------------------------------------
t=$(mktemp -d)
archive=$(make_fixture "$t")
bin="$t/bin"
if verify_and_install "$archive" "$archive.sha256" "$bin" >/dev/null 2>&1 &&
    [ -x "$bin/typ" ] && [ "$("$bin/typ")" = "typ 9.9.9" ]; then
    ok "a good archive installs an executable binary"
else
    no "a good archive installs an executable binary" "not installed, or not runnable"
fi
rm -rf "$t"

# --- a bad checksum installs nothing ------------------------------------------
# The case that matters. Everything else is convenience; this is the one that
# decides whether a tampered or truncated download reaches the filesystem.
t=$(mktemp -d)
archive=$(make_fixture "$t")
# **Not `s/^[0-9a-f]/0/`.** That rewrote the first hex digit to `0`, which is a
# no-op one time in sixteen — and `tar czf` stamps an mtime into the gzip
# header, so the digest is different on every run and the dice are rolled
# every time. The result was a security-relevant test going red at random on
# unrelated pull requests, which is how a check gets ignored. `t` branches out
# once the first substitution has fired, so exactly one of the two applies.
sed 's/^0/1/;t;s/^[0-9a-f]/0/' "$archive.sha256" > "$archive.sha256.bad"
bin="$t/bin"
if verify_and_install "$archive" "$archive.sha256.bad" "$bin" >/dev/null 2>&1; then
    no "a mismatched checksum aborts" "install reported success"
elif [ -e "$bin/typ" ]; then
    no "a mismatched checksum aborts" "it failed but still wrote $bin/typ"
else
    ok "a mismatched checksum aborts and installs nothing"
fi
rm -rf "$t"

# --- an unknown architecture is named, not guessed ----------------------------
if out=$(detect_target Linux sparc64 2>&1); then
    no "an unknown architecture fails" "returned '$out' instead of failing"
elif printf '%s' "$out" | grep -q sparc64; then
    ok "an unknown architecture fails, and says which one"
else
    no "an unknown architecture fails" "message does not name the arch: $out"
fi

# --- Linux resolves to musl, not gnu ------------------------------------------
# The whole point of the milestone: gnu is the current-glibc build and musl is
# the one that runs anywhere, so the installer must not reach for gnu.
if [ "$(detect_target Linux x86_64)" = "x86_64-unknown-linux-musl" ]; then
    ok "Linux resolves to the musl target"
else
    no "Linux resolves to the musl target" "got $(detect_target Linux x86_64)"
fi

# --- a truncated script does nothing ------------------------------------------
# The reason the whole body is a function called on the last line. `curl | sh`
# begins executing before the response has finished arriving, so a dropped
# connection can otherwise run half a script.
t=$(mktemp -d)
head -c 400 "$script" > "$t/partial.sh"
if sh "$t/partial.sh" --bin-dir "$t/bin" >/dev/null 2>&1 && [ -e "$t/bin/typ" ]; then
    no "a truncated script is a no-op" "it installed something"
elif [ -e "$t/bin" ]; then
    no "a truncated script is a no-op" "it created $t/bin"
else
    ok "a truncated script installs nothing and creates nothing"
fi
rm -rf "$t"

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
