#!/usr/bin/env bash
# Fetch one pinned, checksummed binary. No `curl | sh`, no "latest", no
# fallback: a supply-chain step whose failure mode is "carry on with whatever
# arrived" is not a supply-chain step.
set -euo pipefail

: "${TL_VERSION:?the action must pass a pinned version}"
: "${TL_CHECKSUM:?the action must pass the expected sha256}"

# Linux is static musl. See ../install.sh for the measured reason; on a GitHub
# runner it also means the same artifact works on `ubuntu-latest` and on a
# container job with a different base image.
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)   TARGET=x86_64-unknown-linux-musl ;;
  Linux-aarch64)  TARGET=aarch64-unknown-linux-musl ;;
  Darwin-arm64)   TARGET=aarch64-apple-darwin ;;
  Darwin-x86_64)  TARGET=x86_64-apple-darwin ;;
  *) echo "thruline: no published binary for $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

# cargo-dist's name, pinned by crates/cli/tests/m6_release_paths.rs.
ARCHIVE="thruline-cli-${TARGET}.tar.gz"
URL="https://github.com/thrulinehq/thruline/releases/download/${TL_VERSION}/${ARCHIVE}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

curl -fsSL --retry 3 -o "$TMP/$ARCHIVE" "$URL"
HAVE="$(shasum -a 256 "$TMP/$ARCHIVE" | cut -d' ' -f1)"
if [ "$HAVE" != "$TL_CHECKSUM" ]; then
  echo "thruline: checksum mismatch for $URL" >&2
  echo "  expected $TL_CHECKSUM" >&2
  echo "  got      $HAVE" >&2
  exit 1
fi
tar -xzf "$TMP/$ARCHIVE" -C "$TMP"
# One directory named after the archive, binary inside.
BIN="$TMP/thruline-cli-${TARGET}/thruline"
[ -f "$BIN" ] || { echo "thruline: the archive does not contain thruline-cli-${TARGET}/thruline" >&2; exit 1; }
install -m 0755 "$BIN" /usr/local/bin/thruline
thruline --version
