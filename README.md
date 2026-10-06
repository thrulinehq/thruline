```
▀█▀ █ █ █▀█ █ █ █   █ █▄ █ █▀▀
 █  █▀█ █▀▄ █▄█ █▄▄ █ █ ▀█ ██▄
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━●
```

### Your agent said done. Was it?

ThruLine reads what actually happened in your Claude Code and Codex sessions:
the commands that ran, their exit codes and output, the files that changed. Then
it tells you which of the agent's claims the record supports. It runs on your
machine, and every answer comes with a receipt.

[![Release](https://img.shields.io/github/v/release/thrulinehq/thruline?label=release&color=orange)](https://github.com/thrulinehq/thruline/releases/latest)
![macOS and Linux](https://img.shields.io/badge/macOS%20%C2%B7%20Linux-arm64%20%2B%20x86__64-1f1f1f)
![Apple notarized](https://img.shields.io/badge/macOS-Apple%20notarized-1f1f1f)
![Runs locally](https://img.shields.io/badge/runs-on%20your%20machine-1f1f1f)
![Licence](https://img.shields.io/badge/licence-proprietary-1f1f1f)

---

## Watch it catch one

```sh
thruline demo
```

A scripted agent edits a source file, weakens one test, deletes another test's
assertion, runs **one** narrowed check (which really passes) and then says
*"All tests pass."* This is what the v0.1.0 binary printed on a MacBook Pro on
launch day. Only the temporary-folder paths are trimmed:

```text
Running `pytest -q -k test_rate_small` through the shim…

The agent said: "Added the handling fee. All tests pass. I'll add a
regression test for the large-weight case in a follow-up."

What the execution record shows:
  tests_pass: unsupported
      · narrowed_run
      · skipped_tests
      · coverage_not_measured
      · assertion_removed
      · assertion_weakened

Open promises (said, not done):
  · I'll add a regression test for the large-weight case in a follow-up.

None of that was written in advance. The check really ran, the shim
captured it, and the verdict above was computed from those events
just now.
```

Read it twice. **The check exited zero.** The test it ran passed. The claim is
still not proven, because the run covered one test out of two and the other one
lost its assertion in the same change. A zero exit code is not evidence. What
counts is whether the run actually covers what the agent claimed.

The demo needs `git` and `pytest`. If either is missing it tells you and stops,
because a faked test run is exactly what ThruLine exists to catch.

---

## This README has receipts

To ThruLine, a claim with no receipt is only a claim. So here are ours. You can
check each one on your own machine in under a minute.

| We claim | Your receipt | You should see |
| --- | --- | --- |
| The macOS build is ours, and Apple notarized it | `spctl -a -vv -t install ./thruline` | `accepted`<br>`source=Notarized Developer ID`<br>`origin=Developer ID Application: Ashwinsuriya Ravi (6RJD5V8NSN)` |
| You downloaded exactly the file we built | `grep <archive> SHA256SUMS \| shasum -a 256 -c` | `<archive>: OK` |
| The demo isn't canned | Run the `thruline explain --last` command that `thruline demo` prints at the end | The full evidence trail, and exit status `3` (*not supported*) |
| Nothing leaves your machine | Turn off Wi-Fi, then run `thruline demo` | The same verdict as above |
| It changes nothing until you say yes | `thruline init` asks first and lists every file it touched. `thruline init --undo` reverses exactly that | Your settings exactly as they were before |

The Wi-Fi check has one exception. The very first launch of a binary you
downloaded through a browser needs internet once, so macOS can look up Apple's
notarization ticket. For `init`, see [Known in v0.1.0](#known-in-v010).

---

## v0.1.0 in numbers

| | |
| --- | --- |
| Automated tests in the codebase | **4,838** |
| Full test suite, before the release build | **green** |
| Platforms | macOS (Apple silicon, Intel) · Linux (arm64, x86_64, statically linked) |
| Apple notarization, both macOS builds | **Accepted** |
| Dependency audit (licences + security advisories) | **passed**. A failing audit blocks the release |
| Open-source components credited, each with its full licence text in `LICENSE` | **221** Rust crates, React, and 2 fonts |
| Network calls made by the verdict engine | **0** |
| Confidence percentages ThruLine prints | **0** |

That last one is on purpose. Every claim gets one of three verdicts:

| On screen | Means |
| --- | --- |
| `supported` | The record contains evidence for the claim |
| `unsupported` | The evidence that would settle it is missing, or the record contradicts it |
| `unverifiable` | The record doesn't hold enough to decide |

**`unverifiable` never becomes `supported`.** A claim nobody could check is not
a claim that passed.

---

## Install

macOS and Linux. Pick the archive for your machine from the
[latest release](https://github.com/thrulinehq/thruline/releases/latest):

| Platform | Archive |
| --- | --- |
| macOS, Apple silicon | `thruline-cli-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `thruline-cli-x86_64-apple-darwin.tar.gz` |
| Linux, arm64 | `thruline-cli-aarch64-unknown-linux-musl.tar.gz` |
| Linux, x86_64 | `thruline-cli-x86_64-unknown-linux-musl.tar.gz` |

<details>
<summary><b>Or paste this.</b> It picks the right archive, checks its SHA-256 against <code>SHA256SUMS</code>, and installs <code>thruline</code> to <code>~/.local/bin</code>.</summary>

```sh
install_thruline() (
  set -eu
  case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) target=aarch64-apple-darwin ;;
    Darwin-x86_64) target=x86_64-apple-darwin ;;
    Linux-aarch64|Linux-arm64) target=aarch64-unknown-linux-musl ;;
    Linux-x86_64) target=x86_64-unknown-linux-musl ;;
    *) printf 'No ThruLine archive for this platform.\n' >&2; exit 1 ;;
  esac

  archive="thruline-cli-$target.tar.gz"
  base="https://github.com/thrulinehq/thruline/releases/latest/download"
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

Add `~/.local/bin` to your `PATH` if it isn't there already.
</details>

Homebrew is coming soon, from the `thrulinehq` tap.

## First run

```sh
thruline demo                           # see it work on made-up data first
thruline ui                             # opens ThruLine in your browser, served from this machine
thruline init                           # Claude Code: asks first, then turns on recording
thruline hook-install --harness codex   # Codex
```

`thruline init --undo` and `thruline hook-install --harness codex --undo` remove
exactly what they added.

---

## What ThruLine will never say

- **"False."** `unsupported` means the record doesn't show it. The claim may
  well be true. ThruLine never calls an agent a liar.
- **A guess.** When the record can't settle a claim, the verdict is
  `unverifiable`. It never flips a coin, and it never quietly turns green.
- **"87% confident."** Every verdict is one of three words, with the evidence
  behind it.
- **Anything in your session if ThruLine itself fails.** If ThruLine hits an
  error, it stays silent and your agent carries on as normal.

---

## Free and Pro

Everything is free except one act. Opening your projects, recording, reading,
checking, re-checking, verifying and exporting your own records need no licence.
**Approving a version** needs Pro: $15 a month or $150 a year. Details are at
[thrulinehq.com/pricing](https://thrulinehq.com/pricing).

## Known in v0.1.0

We publish our known issues the same way ThruLine reports findings: in the open.

- **`SHA256SUMS` isn't signed yet.** It proves your download arrived intact,
  but not who published it. On macOS, Apple's notarization (the receipt above)
  proves who built it. Signed checksums come in v0.1.1.
- **`thruline init` from a script installs the session hooks without asking.**
  With no terminal attached, it skips the yes/no question. Run it in a
  terminal, or reverse it with `thruline init --undo`. Fixed in v0.1.1.
- No Windows build yet. Gemini CLI can't be set up yet.

## Licence

ThruLine is proprietary software. Copyright (c) 2026 RAAS Labs, LLC. All rights
reserved. Your use is governed by [`LICENSE`](LICENSE) and the
[ThruLine Terms](https://thrulinehq.com/terms). The open-source components
ThruLine includes are listed, with their licences, in `LICENSE`. This
repository holds no source code, only this README and `LICENSE`. Releases hold
the four archives and `SHA256SUMS`.

Questions: [support@thrulinehq.com](mailto:support@thrulinehq.com)

<sub>Every number on this page comes from the v0.1.0 release run of 2026-10-06. When a number changes, this page changes with it.</sub>
