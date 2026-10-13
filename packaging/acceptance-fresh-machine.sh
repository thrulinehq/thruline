#!/usr/bin/env bash
# THE ACCEPTANCE: a fresh machine, from `curl … | sh` to a PR check, using only
# the built artifacts.
#
# "Fresh machine" is a clean `debian:bookworm-slim` container. It has no Rust
# toolchain, no cargo, no node, no ThruLine source tree and no ThruLine binary.
# Everything it ends up with arrives through the published install path. That
# is a stronger claim than a script run on this laptop, where the thing being
# tested is already installed and on PATH.
#
# WHAT IT PROVES, IN ORDER
#   1. `curl … | sh` installs a working `thruline` on a machine that had none.
#   2. The checksum is verified — and the installer REFUSES a corrupted archive
#      rather than installing it. Proved by corrupting one and observing the
#      refusal, not by reading the script.
#   3. `thruline demo` runs, offline, on that machine.
#
#      **Step 3b currently FAILS on Linux, and that is a real defect, not a
#      harness fault.** `capture.rs::execute_shell` runs captured commands under
#      `/bin/zsh -lc`, which a stock Linux image does not have, so
#      `thruline demo --continuity` dies with
#      `i/o error at /bin/zsh: No such file or directory`. The one-line fix was
#      made on this branch and REVERTED: `crates/core/**` is not this track's
#      under the ownership ruling. Filed as docs/tracks/C/INTERFACE-REQUESTS.md R4 with the
#      fix and its two deliberate parts, and **the core has since applied it at
#      `e00f31c`** — so this passes after the merge and fails until then, on
#      this branch only. Left failing on purpose — this script
#      is the only thing that runs the install path on a machine that has never
#      seen ThruLine, and making it pass by avoiding the defect would remove the
#      only evidence that the defect is there.
#   4. A real session is captured and a real receipt is signed there.
#   5. The Action's own step bodies produce a **check run payload** from that
#      receipt — the pull-request surface, reached with nothing but the
#      installed binary.
#
# WHAT IT DOES NOT PROVE
#   Nothing about GitHub. No release has been published, so the archives are
#   served from a local HTTP server in the same shape a release would serve
#   them, and no check run is posted to any pull request. The payload is
#   produced and inspected; GitHub accepting it is unverified.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STAGE="${THRULINE_STAGE:-$REPO_ROOT/target/fresh-machine}"
case "$(uname -m)" in
  arm64|aarch64) PLATFORM=linux/arm64; TARGET=aarch64-unknown-linux-musl ;;
  *)             PLATFORM=linux/amd64; TARGET=x86_64-unknown-linux-musl ;;
esac
BIN="$REPO_ROOT/target/musl/thruline-$TARGET"

[ -f "$BIN" ] || {
  echo "no $BIN — run ./packaging/build-musl.sh first" >&2
  exit 1
}

# ── stage a release, in the public download shape ────────────────────────────
# `thruline-cli-<target>.tar.gz` holding one directory named after itself, plus
# `SHA256SUMS` in sha256sum's binary-marker format. Getting this shape wrong is
# the failure this whole harness exists to catch, so it is built from the same
# names the install path reads rather than from a second list.
rm -rf "$STAGE"; mkdir -p "$STAGE/release" "$STAGE/pkg/thruline-cli-$TARGET"
cp "$BIN" "$STAGE/pkg/thruline-cli-$TARGET/thruline"
cp "$REPO_ROOT/LICENSE.md" "$REPO_ROOT/THIRD_PARTY_NOTICES.md" "$REPO_ROOT/README.md" "$REPO_ROOT/CHANGELOG.md" \
   "$STAGE/pkg/thruline-cli-$TARGET/"
tar -czf "$STAGE/release/thruline-cli-$TARGET.tar.gz" -C "$STAGE/pkg" "thruline-cli-$TARGET"
( cd "$STAGE/release" && shasum -a 256 -b "thruline-cli-$TARGET.tar.gz" \
    | sed 's/ \(.*\)/ *\1/' | sed 's/\*\*/*/' > SHA256SUMS )
# `shasum -b` already emits `<hash> *<name>` on macOS; normalise either form.
python3 - "$STAGE/release/SHA256SUMS" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1])
out = []
for line in p.read_text().splitlines():
    h, _, n = line.partition(" ")
    out.append(f"{h} *{n.strip().lstrip('*')}")
p.write_text("\n".join(out) + "\n")
print(p.read_text().strip())
PY
cp "$REPO_ROOT/packaging/install.sh" "$STAGE/release/install.sh"
cp -r "$REPO_ROOT/packaging/action" "$STAGE/action"

echo "=== staged release ==="
ls -l "$STAGE/release"

# ── the fresh machine ───────────────────────────────────────────────────────
docker run --rm --platform "$PLATFORM" \
  -v "$STAGE:/stage:ro" -w /work \
  debian:bookworm-slim bash -euo pipefail -c '
    echo "=== a machine with no thruline and no toolchain ==="
    command -v thruline && { echo "thruline was already installed; this proves nothing" >&2; exit 1; }
    command -v cargo    && { echo "a Rust toolchain is present; this is not a fresh machine" >&2; exit 1; }
    apt-get -qq update >/dev/null && apt-get -qq install -y curl ca-certificates git jq python3 >/dev/null
    mkdir -p /srv && cp -r /stage/release/. /srv/
    ( cd /srv && python3 -m http.server 8000 >/dev/null 2>&1 & )
    for _ in $(seq 1 40); do curl -fsS http://127.0.0.1:8000/SHA256SUMS >/dev/null 2>&1 && break; sleep 0.25; done

    echo
    echo "=== 1. curl … | sh ==="
    curl -fsSL http://127.0.0.1:8000/install.sh \
      | THRULINE_BASE_URL=http://127.0.0.1:8000 THRULINE_VERSION=v0.0.0-acceptance sh
    thruline --version

    echo
    echo "=== 2. the installer REFUSES a corrupted archive ==="
    # Corrupt the served bytes and leave the checksum file alone. An installer
    # that verifies must now refuse; one that only says it verifies will not.
    printf "corrupted" >> /srv/thruline-cli-'"$TARGET"'.tar.gz
    set +e
    out="$(curl -fsSL http://127.0.0.1:8000/install.sh \
      | THRULINE_BASE_URL=http://127.0.0.1:8000 THRULINE_VERSION=v0.0.0-acceptance \
        THRULINE_PREFIX=/tmp/badinstall sh 2>&1)"
    rc=$?
    set -e
    echo "$out"
    [ $rc -ne 0 ] || { echo "REFUSAL DID NOT HAPPEN: the installer accepted a corrupted archive" >&2; exit 1; }
    case "$out" in *"checksum mismatch"*) ;; *) echo "refused, but not for the checksum: $out" >&2; exit 1 ;; esac
    [ ! -e /tmp/badinstall/thruline ] || { echo "a binary was installed anyway" >&2; exit 1; }
    echo "refused, and installed nothing."

    echo
    echo "=== 3a. on a machine with no test runner, the demo REFUSES clearly ==="
    export THRULINE_HOME=/work/home
    git config --global user.email a@e.invalid
    git config --global user.name A
    # This is a finding, pinned rather than papered over: a bare machine has no
    # pytest, and the demo declines instead of simulating a test run. The
    # refusal has to name what is missing, or the first thirty seconds of using
    # this product are spent guessing.
    set +e
    out="$(thruline demo 2>&1)"; rc=$?
    set -e
    echo "$out" | tail -5
    [ $rc -ne 0 ] || { echo "the demo claimed to run with no test runner present" >&2; exit 1; }
    case "$out" in *pytest*) ;; *) echo "the refusal does not name what is missing" >&2; exit 1 ;; esac

    echo
    echo "=== 3b. with a test runner present, the demo runs ==="
    apt-get -qq install -y python3-pytest >/dev/null 2>&1 || pip3 install --quiet --break-system-packages pytest >/dev/null 2>&1 || true
    command -v pytest >/dev/null 2>&1 || { echo "could not install pytest; skipping 3b LOUDLY" >&2; DEMO_SKIPPED=1; }
    if [ "${DEMO_SKIPPED:-0}" = "0" ]; then
      thruline demo 2>&1 | tail -12
    fi

    echo
    echo "=== 4. a real session, a real signed receipt ==="
    mkdir -p /work/repo && cd /work/repo
    git init -q -b main
    printf "def rate(w):\n    return 2.5 * w\n" > app.py
    git add -A && git commit -qm first
    sid=77777777-8888-4999-8aaa-bbbbbbbbbbbb
    printf %s "{\"hook_event_name\":\"SessionStart\",\"session_id\":\"$sid\",\"transcript_path\":\"/t\",\"cwd\":\"/work/repo\",\"source\":\"startup\"}" \
      | thruline hook claude-code
    printf %s "{\"hook_event_name\":\"Stop\",\"session_id\":\"$sid\",\"transcript_path\":\"/t\",\"cwd\":\"/work/repo\",\"last_assistant_message\":\"All tests pass.\",\"stop_hook_active\":false}" \
      | thruline hook claude-code
    thruline fold >/dev/null
    mkdir -p .thruline/receipts
    thruline receipt --out .thruline/receipts/latest.json
    git add -A && git commit -qm receipt
    test -s .thruline/receipts/latest.json

    echo
    echo "=== 5. the PR check, from the Action’s own step bodies ==="
    export GITHUB_SHA="$(git rev-parse HEAD)"
    export GITHUB_STEP_SUMMARY=/work/summary.md
    : > "$GITHUB_STEP_SUMMARY"
    # These are the two commands packaging/action/action.yml runs. The payload
    # they produce is what the Action POSTs to /repos/{owner}/{repo}/check-runs.
    thruline verify-receipt .thruline/receipts/latest.json \
      --key "$THRULINE_HOME/signing.pub" --offline \
      --format check-run --head-sha "$GITHUB_SHA" > /work/check-run.json || true
    thruline verify-receipt .thruline/receipts/latest.json \
      --key "$THRULINE_HOME/signing.pub" --offline --format md > /work/table.md || true
    thruline verify-receipt .thruline/receipts/latest.json \
      --key "$THRULINE_HOME/signing.pub" --offline --format badge > /work/badge.svg || true

    conclusion="$(jq -r .conclusion /work/check-run.json)"
    title="$(jq -r .output.title /work/check-run.json)"
    echo "check run conclusion: $conclusion"
    echo "check run title:      $title"
    echo
    sed -n "1,12p" /work/table.md

    # The one rule, checked on the bytes: this receipt claims "All tests pass."
    # with no test check in the record, so the claim is unproved and the check
    # run must NOT be green.
    case "$conclusion" in
      success) echo "GREEN over an unproved claim — the whole point, violated" >&2; exit 1 ;;
      neutral|failure) ;;
      *) echo "unrecognised conclusion $conclusion" >&2; exit 1 ;;
    esac
    grep -q ">verified<" /work/badge.svg && { echo "badge says verified" >&2; exit 1; }
    echo
    echo "PASS — installed from curl|sh, refused a corrupted archive, ran the demo,"
    echo "       signed a receipt, and produced a PR check payload that is not green"
    echo "       over an unproved claim. Nothing but the built artifact was used."
  '
