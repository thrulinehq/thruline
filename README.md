# ThruLine v0.1.0

ThruLine checks local Claude Code and Codex session records and shows receipts
for what an agent claimed and what the record supports. It does not require you
to upload your source code.

## Download and verify

The [v0.1.0 GitHub Release](https://github.com/thrulinehq/thruline/releases/tag/v0.1.0)
provides four CLI archives and `SHA256SUMS`. Choose the archive for your machine:

| Platform | Archive |
| --- | --- |
| macOS, Apple silicon | `thruline-cli-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `thruline-cli-x86_64-apple-darwin.tar.gz` |
| Linux, arm64 | `thruline-cli-aarch64-unknown-linux-musl.tar.gz` |
| Linux, x86_64 | `thruline-cli-x86_64-unknown-linux-musl.tar.gz` |

This command downloads the matching archive, verifies its SHA-256 against
`SHA256SUMS`, and installs `thruline` to `$HOME/.local/bin`. It requires `curl`,
`tar`, and either `shasum` or `sha256sum`.

```sh
install_thruline() (
  set -eu
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) target=aarch64-apple-darwin ;;
    Darwin-x86_64) target=x86_64-apple-darwin ;;
    Linux-aarch64|Linux-arm64) target=aarch64-unknown-linux-musl ;;
    Linux-x86_64) target=x86_64-unknown-linux-musl ;;
    *) printf 'No v0.1.0 archive for this platform.\n' >&2; exit 1 ;;
  esac

  archive="thruline-cli-$target.tar.gz"
  base="https://github.com/thrulinehq/thruline/releases/download/v0.1.0"
  work=$(mktemp -d)
  trap 'rm -rf "$work"' 0
  cd "$work"
  curl -fsSLO "$base/$archive"
  curl -fsSLO "$base/SHA256SUMS"

  expected=$(awk -v name="$archive" '$2 == name { print $1 }' SHA256SUMS)
  [ -n "$expected" ] || { printf 'Missing checksum entry.\n' >&2; exit 1; }
  if command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$archive" | awk '{ print $1 }')
  elif command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$archive" | awk '{ print $1 }')
  else
    printf 'Install shasum or sha256sum to verify the download.\n' >&2
    exit 1
  fi
  [ "$expected" = "$actual" ] || { printf 'Checksum mismatch.\n' >&2; exit 1; }

  tar -xzf "$archive"
  mkdir -p "$HOME/.local/bin"
  install -m 0755 "thruline-cli-$target/thruline" "$HOME/.local/bin/thruline"
)
install_thruline
```

Add `$HOME/.local/bin` to your `PATH` if it is not already there. Then run
`thruline --version`, `thruline ui`, or `thruline init`. `init` asks before it
installs capture hooks and supports `init --undo`.

A Homebrew formula will be available from the separate `thrulinehq` tap once
that tap is published. The v0.1.0 download format is a CLI in `.tar.gz`; there
is no DMG or PKG.

## macOS first run

The macOS executables are released only after Developer ID signing with
hardened runtime and a secure timestamp, followed by Apple notarization.
Apple cannot staple a notarization ticket to a standalone CLI. A binary
downloaded through a browser needs internet access on its first run so
Gatekeeper can look up that ticket. Homebrew and curl installs are unaffected
by this first-run requirement.

## Licence

ThruLine is proprietary software. Copyright (c) 2026 RAAS Labs, LLC. All rights reserved. Your use is governed by `LICENSE` and the ThruLine Terms at https://thrulinehq.com/terms. The open-source components ThruLine includes are listed, with their licences, in `LICENSE`.

The release contains the four archives, `SHA256SUMS`, this README, and
`LICENSE`. No product source is included in the release assets.
