#!/usr/bin/env sh
# ThruLine installer — BUILD-SPEC §15's `curl | sh` path.
#
# **Not published by this repository.** This script is built here and served
# from thrulinehq.com by a separate, deliberate act; nothing in this repository
# deploys it.
#
# Two properties are worth more than convenience in a script people pipe into a
# shell, and both are here:
#
#   1. It verifies before it executes. The checksum comes from the public
#      release's `SHA256SUMS`; an installer that trusts its download without
#      checking it is only a delivery mechanism.
#   2. It fails closed. `set -eu`, no `|| true`, no "carry on without the
#      checksum". Every path that cannot complete stops and says why.
set -eu

REPO="thrulinehq/thruline"
VERSION="${THRULINE_VERSION:-}"
PREFIX="${THRULINE_PREFIX:-/usr/local/bin}"

say() { printf '%s\n' "$*" >&2; }
die() { say "thruline: $*"; exit 1; }

command -v curl >/dev/null 2>&1 || die "curl is required"
command -v tar  >/dev/null 2>&1 || die "tar is required"

# **Linux is musl, and statically linked.** This is not a preference.
#
# The bench measured the glibc failure directly: a binary built against glibc
# 2.41 will not start on 2.36, and the error is
# `libc.so.6: version 'GLIBC_2.39' not found`. glibc is backward compatible and
# not forward, so a dynamically linked release has to be built on the oldest
# libc anyone will ever run it on — which means guessing, forever, about
# machines nobody has seen. The bench hit it directly, and Terminal-Bench
# reported it as "asciinema is not installed in the container" — a string that
# sends a reader looking for a missing recording tool rather than a binary that
# could not run. (Static linking closes the libc half of that failure class and
# not the arch half; both Linux architectures are published for the other half.)
#
# A static musl binary has no libc to be wrong about. It runs on Alpine, on
# Debian oldstable, in a distroless container, and on whatever the next base
# image is. The cost is that `thruline` cannot dlopen anything at runtime, and
# it does not: the release build is `--no-default-features`, so the ONNX
# annotation path — the only thing that would have wanted a dynamic loader — is
# already out.
#
# `uname -m` reports the kernel's architecture, which on a 64-bit kernel running
# a 32-bit userland is not the userland's. There is no 32-bit build to get
# wrong here, so the mismatch cannot arise; if a 32-bit target is ever added,
# this is where it breaks.
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)          TARGET=x86_64-unknown-linux-musl ;;
  Linux-aarch64|Linux-arm64) TARGET=aarch64-unknown-linux-musl ;;
  Darwin-arm64)          TARGET=aarch64-apple-darwin ;;
  Darwin-x86_64)         TARGET=x86_64-apple-darwin ;;
  *) die "no published binary for $(uname -s)-$(uname -m)" ;;
esac

if [ -z "$VERSION" ]; then
  VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -1)"
  [ -n "$VERSION" ] || die "could not determine the latest release; set THRULINE_VERSION"
  say "thruline: installing $VERSION (set THRULINE_VERSION to pin)"
fi

# The artifact names are cargo-dist's, not ours, and they are pinned by
# `crates/cli/tests/m6_release_paths.rs` against `dist plan`'s own output. The
# first version of this script fetched `thruline-$TARGET.tar.gz` from a release
# that planned `thruline-cli-$TARGET.tar.gz`. The public publisher deliberately
# creates `SHA256SUMS` from the four final archive hashes.
ARCHIVE="thruline-cli-$TARGET.tar.gz"
# `THRULINE_BASE_URL` overrides where the archives are fetched from: a mirror,
# a GitHub Enterprise host, or the acceptance harness's local server. It does
# **not** relax any check — the checksum file is fetched from the same base and
# compared exactly as before, so a mirror can serve different bytes only by
# serving a matching checksum, which is the same trust boundary as the default.
# What it cannot do is skip verification, because there is no path here that
# skips verification.
BASE="${THRULINE_BASE_URL:-https://github.com/$REPO/releases/download/$VERSION}"
if [ -n "${THRULINE_BASE_URL:-}" ]; then
  say "thruline: WARNING: downloading from non-official source $BASE"
fi
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

curl -fsSL --retry 3 -o "$TMP/$ARCHIVE" "$BASE/$ARCHIVE" \
  || die "download failed: $BASE/$ARCHIVE"
curl -fsSL --retry 3 -o "$TMP/SHA256SUMS" "$BASE/SHA256SUMS" \
  || die "no SHA256SUMS in $VERSION; refusing to install an unverified binary"

# cargo-dist writes `<hash> *<name>` — the `*` is sha256sum's binary-mode
# marker and is part of the field, so it is stripped rather than matched
# around. Anchored on both sides: an unanchored match would let
# `thruline-cli-x86_64-apple-darwin.tar.gz` satisfy a request for the aarch64
# one, and the wrong-architecture binary would then pass its checksum.
WANT="$(awk -v n="$ARCHIVE" '{ f = $2; sub(/^\*/, "", f); if (f == n) print $1 }' "$TMP/SHA256SUMS")"
[ -n "$WANT" ] || die "SHA256SUMS does not list $ARCHIVE"

# **Refuses rather than skipping.** cargo-dist's own generated installer says
# "skipping sha256 checksum verification (it requires the 'sha256sum' command)"
# and installs anyway — and macOS has `shasum`, not `sha256sum`, so that path
# is every Mac. An installer that verifies when it happens to be able to is not
# an installer that verifies.
if command -v shasum >/dev/null 2>&1; then
  HAVE="$(shasum -a 256 "$TMP/$ARCHIVE" | cut -d' ' -f1)"
elif command -v sha256sum >/dev/null 2>&1; then
  HAVE="$(sha256sum "$TMP/$ARCHIVE" | cut -d' ' -f1)"
else
  die "neither shasum nor sha256sum is available; refusing to install unverified"
fi
# `HAVE` is the output of a pipe, and this is POSIX `sh`: `pipefail` does not
# exist here (it would break dash), so a failing `shasum` leaves `HAVE` empty
# with a zero status. Today that still refuses, but only because `WANT` was
# proven non-empty at the top — an empty `HAVE` cannot equal a non-empty `WANT`.
# That makes the safety of this comparison depend on a check twenty lines away,
# and reordering or relaxing that one silently opens an empty-equals-empty
# bypass of the only integrity check in the install path. Stated locally instead.
[ -n "$HAVE" ] || die "could not compute the archive's sha256; refusing to install unverified"
[ "$HAVE" = "$WANT" ] || die "checksum mismatch (want $WANT, got $HAVE)"

tar -xzf "$TMP/$ARCHIVE" -C "$TMP"
# The archive holds one directory named after itself, with the binary inside.
BIN="$TMP/thruline-cli-$TARGET/thruline"
[ -f "$BIN" ] || die "the archive does not contain thruline-cli-$TARGET/thruline"

if [ -w "$PREFIX" ]; then
  install -m 0755 "$BIN" "$PREFIX/thruline"
else
  say "thruline: $PREFIX is not writable; using sudo"
  sudo install -m 0755 "$BIN" "$PREFIX/thruline"
fi

say "thruline: installed to $PREFIX/thruline"
say ""
# **What the next line says was measured on a fresh machine, not assumed.**
# It used to say `thruline demo` flatly. On a clean `debian:bookworm-slim` with
# ThruLine just installed from this script, that command refuses:
#
#   thruline demo needs a real test runner, and found none. Looked for: pytest
#
# The refusal is right — the demo runs a real check and will not simulate one —
# but being told to run something that then declines is the worst possible
# first thirty seconds, and §15's whole point is that the value is visible
# before the user runs anything. So the requirement is stated here, where it
# costs one line, instead of being discovered there.
if command -v pytest >/dev/null 2>&1 || command -v cargo >/dev/null 2>&1; then
  say "Next: \`thruline demo\` shows what it does in about ten seconds."
else
  say "Next: \`thruline demo\` shows what it does in about ten seconds — it runs a"
  say "      real test command, so it needs pytest or cargo on PATH. Without one,"
  say "      \`thruline autopsy\` reads what this machine already has."
fi
say "Then: \`thruline init\` sets up capture for your harness."
