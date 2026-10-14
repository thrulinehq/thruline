#!/usr/bin/env bash
#
# The M3 acceptance suite — the `#[ignore]`d, Docker-requiring tests that prove
# re-execution works — run for real, counted, and never skipped.
#
# ---------------------------------------------------------------------------
# WHY THIS FILE EXISTS, AND WHY IT IS A SCRIPT RATHER THAN INLINE YAML
# ---------------------------------------------------------------------------
#
# Measured before it was written: `.github/workflows/ci.yml` ran
# `cargo test --workspace` and contained ZERO occurrences of `--ignored` or
# `--include-ignored`. Every end-to-end acceptance test for M3.1, M3.2 and M3.3
# — including the ones proving the laundering path is closed and that a flaky
# re-execution never reaches `supported` — therefore ran on no CI at all, while
# a green check appeared on every commit. Under this program's standing rule an
# acceptance test that never runs is worse than none, because it certifies.
#
# It is a script, not sixty lines of `run:` in the workflow, for one reason:
# **the property "these tests actually ran" has to be witnessable before a
# push.** A guard only a CI provider can evaluate is a guard nobody can watch
# fail on their own machine, which is the shape of the defect above.
# `bash .github/workflows/acceptance-docker.sh` is the same code path CI takes,
# and the same code path a reviewer can drive with a stubbed `docker`/`cargo` on
# `PATH` to watch each assertion refuse. (GitHub treats only `.yml`/`.yaml`
# under `.github/workflows/` as workflows; other files there are inert.)
#
# ---------------------------------------------------------------------------
# THREE PROPERTIES, EACH OF WHICH FAILS LOUDLY
# ---------------------------------------------------------------------------
#
#  1. **The inventory is derived, then pinned.** The test binaries are derived
#     from the source tree (below), the `#[ignore]`d tests are listed out of the
#     compiled binaries (`--list --ignored`), and the total is compared with
#     EXPECTED_IGNORED. That number is a plain constant, deliberately NOT
#     readable from the environment: a count a caller can set is a count a
#     caller can set to zero.
#
#  2. **No environment skip.** If Docker is absent, unresponsive, or missing an
#     image the tests refuse to pull, this exits NON-ZERO and says which. A skip
#     that reads as a pass is the exact failure this program keeps finding, and
#     it is the failure these tests were already suffering.
#
#  3. **The executed count is asserted.** `passed + failed` summed across every
#     test binary must equal the inventory, and `failed` must be 0. A test that
#     quietly stops being run — renamed, `#[cfg]`'d out, filtered away — moves
#     the executed count and turns this red rather than passing with one fewer.
#     cargo's own exit code cannot see that: tests which do not run do not fail.
#
# The cheap, Docker-independent checks run FIRST, so a machine with no daemon
# still gets the inventory answer rather than only "no Docker".
#
# ---------------------------------------------------------------------------
# THE NUMBER
# ---------------------------------------------------------------------------
#
# Eleven. Counted from the four files this job derives, on 2026-09-04:
#   crates/cli/tests/m3_reexec.rs                2
#   crates/cli/tests/m3_reexec_rerun.rs          3
#   crates/cli/tests/m3_reexec_laundering.rs     1
#   crates/reexec/tests/docker_sandbox.rs        5
#
# It was NINE until the M3 gap-closing pass, and moved deliberately, which is
# what this pin is for. The two added, both of which need a daemon:
#
#   * `two_runs_with_one_exit_code_and_two_tallies_are_flaky` — the tally half
#     of `Attempt::outcome()`, which a doc comment asserted and no test held.
#   * `a_symlink_in_the_dockerfile_build_context_cannot_carry_a_host_file_into_the_image`
#     — the build-context symlink route, which is closed by the BUILDER and by
#     nothing of ours, and which nothing would have reddened if that changed.
#
# The defect report that commissioned this job said twelve. Twelve was the count
# of the string `#[ignore` in those files, three of which sit inside `//!` module
# doc comments explaining why the tests are ignored. A job pinned to a string
# count would have failed forever, which is the better failure but still the
# wrong number.
set -euo pipefail

EXPECTED_IGNORED=11

# The globs that define this job's scope. A file matching one of these and
# carrying an `#[ignore]`d test is RUN by this job — nothing has to be added to
# a list. Its tests then change the derived count, and the pin above turns that
# into a red that has to be resolved deliberately.
SCOPE_GLOBS=(crates/cli/tests/m3_*.rs crates/reexec/tests/*.rs)

# The image the reruns and the laundering test need: anything with `pytest` on
# its PATH. Built here by default (no stock image carries one); overridable
# because that is a fact about the machine, not about what is being asserted.
PYTEST_IMAGE="${THRULINE_TEST_PYTEST_IMAGE:-thruline-acceptance-pytest:local}"
# Named by `crates/cli/tests/m3_reexec.rs` and, as DECLARED/OTHER, by
# `crates/reexec/tests/docker_sandbox.rs` — which deliberately REFUSES to pull,
# so they have to be present before the tests run.
BASE_IMAGE="python:3.13.7-slim-bookworm"
OTHER_IMAGE="python:3-slim"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

die() {
  echo >&2
  echo "ACCEPTANCE SUITE DID NOT PASS: $*" >&2
  echo >&2
  echo "This job fails rather than skipping. The tests it runs are the only" >&2
  echo "end-to-end evidence that M3's re-execution works; a green check over an" >&2
  echo "environment that could not run them would certify nothing while looking" >&2
  echo "exactly like one that did." >&2
  exit 1
}

section() {
  echo
  echo "=== $* ==="
}

# `timeout(1)` is not on macOS, and this script has to run there too.
#
# A Docker daemon that is *installed but wedged* answers nothing at all — the
# state this machine was in the day this file was written, where `docker
# version` never returned and the socket replied "Docker Desktop is unable to
# start". Without this, the job would hang until the runner's own timeout killed
# it, and "cancelled after 45 minutes" is a worse signal than "the daemon did
# not answer": it reads as an infrastructure hiccup rather than as an acceptance
# suite that did not run.
run_with_timeout() {
  local secs="$1"
  shift
  "$@" >"$TIMEOUT_OUT" 2>&1 &
  local pid=$! waited=0
  while kill -0 "$pid" 2>/dev/null; do
    if [ "$waited" -ge "$secs" ]; then
      kill -9 "$pid" 2>/dev/null || true
      wait "$pid" 2>/dev/null || true
      return 124
    fi
    sleep 1
    waited=$((waited + 1))
  done
  wait "$pid"
}
TIMEOUT_OUT="$(mktemp)"
LOG="$(mktemp)"
trap 'rm -f "$TIMEOUT_OUT" "$LOG"' EXIT

# ---------------------------------------------------------------------------
# 1. Which test binaries carry acceptance tests — derived, not listed.
# ---------------------------------------------------------------------------
# `mapfile` is bash 4; macOS ships 3.2, so this stays POSIX-shaped.
section "inventory: files in scope carrying #[ignore]d tests"
found_files="$(grep -l -E '^[[:space:]]*#\[ignore' "${SCOPE_GLOBS[@]}" 2>/dev/null | sort || true)"
[ -n "$found_files" ] || die "no file in scope carries an #[ignore]d test. Either the acceptance
     suite was deleted or SCOPE_GLOBS no longer matches where it lives; both are
     the same failure — nothing is being run."

CARGO="${CARGO:-cargo}"
CLI_ARGS=()
REEXEC_ARGS=()
while IFS= read -r file; do
  echo "  $file"
  target="$(basename "$file" .rs)"
  case "$file" in
    crates/cli/tests/*) CLI_ARGS+=(--test "$target") ;;
    crates/reexec/tests/*) REEXEC_ARGS+=(--test "$target") ;;
    *) die "$file is in scope but this script does not know which package owns it." ;;
  esac
done <<EOF
$found_files
EOF

# `--no-default-features` drops the optional `ort` (ONNX Runtime) feature. None
# of these tests touches the annotation model, and linking it would make a
# Docker acceptance job depend on a CDN fetch at build time — a second way for
# the job to go red for a reason unrelated to what it measures. Default-feature
# compilation is covered by the workspace `test` job.
# `--locked` matches `.github/workflows/receipt.yml`: a CI run that silently
# updates Cargo.lock is testing a dependency set nobody chose.
CARGO_CLI=("$CARGO" test --locked -p thruline-cli --no-default-features "${CLI_ARGS[@]}")
CARGO_REEXEC=("$CARGO" test --locked -p thruline-reexec "${REEXEC_ARGS[@]}")

# ---------------------------------------------------------------------------
# 2. The tests themselves, listed out of the compiled binaries.
# ---------------------------------------------------------------------------
section "inventory: the #[ignore]d tests in those binaries"
listed=0
for which in cli reexec; do
  if [ "$which" = cli ]; then set -- "${CARGO_CLI[@]}"; else set -- "${CARGO_REEXEC[@]}"; fi
  # stdout is parsed below; stderr (cargo's compile errors and warnings) goes to
  # a file and is printed if the listing fails, so a red here is readable.
  errfile="$(mktemp)"
  if ! out="$("$@" -- --ignored --list 2>"$errfile")"; then
    echo "--- stderr of the failed listing ---" >&2
    cat "$errfile" >&2
    echo "--- end stderr ---" >&2
    rm -f "$errfile"
    die "listing failed for: $*"
  fi
  rm -f "$errfile"
  echo "$out" | sed -n 's/^\(.*\): test$/  \1/p'
  listed=$((listed + $(echo "$out" | grep -c ': test$' || true)))
done
echo "  listed: $listed"
[ "$listed" -eq "$EXPECTED_IGNORED" ] ||
  die "$listed #[ignore]d test(s) are present; EXPECTED_IGNORED says $EXPECTED_IGNORED.
     A test was added, removed, renamed or moved out of scope. Change the number
     DELIBERATELY, with the reason — this pin is the only thing that turns a
     quietly-dropped acceptance test into a red instead of a silence."

# ---------------------------------------------------------------------------
# 3. Docker, loudly.
# ---------------------------------------------------------------------------
section "preflight: docker"
command -v docker >/dev/null 2>&1 || die "no \`docker\` on PATH."
if run_with_timeout "${DOCKER_PROBE_SECONDS:-30}" docker version --format '{{.Server.Version}}'; then
  server="$(cat "$TIMEOUT_OUT")"
else
  probe_status=$?
  cat "$TIMEOUT_OUT" >&2
  if [ "$probe_status" -eq 124 ]; then
    die "the Docker daemon did not answer within ${DOCKER_PROBE_SECONDS:-30}s — installed
     but wedged. Not a reason to skip, and not a reason to hang for the whole
     job timeout either: restart it, or run this job on a runner that has one."
  fi
  die "the Docker daemon did not answer (\`docker version\` exited $probe_status). Not a
     reason to skip: start it, or run this job on a runner that has one."
fi
# `docker version --format …` can exit 0 having printed an error and no server
# version — measured on this machine, where the daemon was down and the client
# answered "Cannot connect to the Docker daemon". Trusting the exit code alone
# would let the job walk past a dead daemon into a run that fails for reasons
# nobody could read.
case "$server" in
  [0-9]*) ;;
  *) die "\`docker version\` exited 0 but named no server version — a daemon that is not
     there answers exactly like this. What it said: ${server:-<nothing>}" ;;
esac
echo "  docker server $server"

section "preflight: images"
# Pulled HERE rather than by the tests, because `docker_sandbox.rs` asserts the
# images are already present ("pulling it here would make this test a network
# test"). Doing it in the harness keeps the tests themselves offline.
for image in "$BASE_IMAGE" "$OTHER_IMAGE"; do
  if docker image inspect "$image" >/dev/null 2>&1; then
    echo "  present: $image"
  else
    echo "  pulling: $image"
    docker pull --quiet "$image" >/dev/null || die "could not pull $image."
  fi
done
if docker image inspect "$PYTEST_IMAGE" >/dev/null 2>&1; then
  echo "  present: $PYTEST_IMAGE"
else
  echo "  building: $PYTEST_IMAGE (base image plus pytest; no stock image carries one)"
  printf 'FROM %s\nRUN pip install --no-cache-dir pytest\n' "$BASE_IMAGE" |
    docker build --quiet --tag "$PYTEST_IMAGE" - >/dev/null ||
    die "could not build $PYTEST_IMAGE."
fi
export THRULINE_TEST_PYTEST_IMAGE="$PYTEST_IMAGE"

# ---------------------------------------------------------------------------
# 4. Run them, and count what ran.
# ---------------------------------------------------------------------------
section "running the acceptance suite"
status=0
"${CARGO_CLI[@]}" --no-fail-fast -- --ignored --test-threads=1 2>&1 | tee -a "$LOG" || status=1
"${CARGO_REEXEC[@]}" --no-fail-fast -- --ignored --test-threads=1 2>&1 | tee -a "$LOG" || status=1

read -r passed failed skipped < <(
  awk '/^test result:/ {
         for (i = 1; i <= NF; i++) {
           if ($(i+1) ~ /^passed/)  p += $i;
           if ($(i+1) ~ /^failed/)  f += $i;
           if ($(i+1) ~ /^ignored/) g += $i;
         }
       }
       END { print p+0, f+0, g+0 }' "$LOG"
)
executed=$((passed + failed))

section "result"
echo "  executed: $executed   (passed $passed, failed $failed, still-ignored $skipped)"
echo "  expected: $EXPECTED_IGNORED executed, 0 failed, 0 still-ignored"

# The count assertion comes FIRST. A suite that ran six of nine and passed all
# six exits 0 from cargo, and that silence is what this job exists to convert
# into a failure.
[ "$executed" -eq "$EXPECTED_IGNORED" ] ||
  die "$executed acceptance test(s) executed; $EXPECTED_IGNORED were expected.
     cargo's own exit code cannot see this: tests that do not run do not fail."
[ "$skipped" -eq 0 ] ||
  die "$skipped test(s) were STILL ignored under \`--ignored\` — they were built and
     then not run."
[ "$failed" -eq 0 ] || die "$failed acceptance test(s) FAILED. See the log above."
[ "$status" -eq 0 ] ||
  die "a cargo invocation exited non-zero while the summary lines looked clean."

echo
echo "OK: $executed/$EXPECTED_IGNORED M3 acceptance tests executed against Docker server $server."
