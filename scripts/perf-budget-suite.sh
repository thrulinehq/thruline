#!/usr/bin/env bash
# L9.7 — the standing budget suite, run end to end on real hardware.
# P2b — and every number it produces now leaves with a stamped basis attached.
#
# These are USER-FACING numbers on purpose: each one times the whole binary,
# including process start and `Ledger::open`, because that is what a person
# waits for. A number that excludes startup would flatter the product against a
# budget the product does not actually meet.
#
# Read-only *of the path you give it* — but `verify` writes verification rows, so
# point it at a clone, not at ~/.thruline/ledger.db.
#
#   sqlite3 ~/.thruline/ledger.db ".backup /tmp/ledger-clone.db"
#   scripts/perf-budget-suite.sh /tmp/ledger-clone.db [runs]
#
# USE `.backup`, NOT `cp`. The ledger runs in WAL mode, so a plain `cp` of
# `ledger.db` alone copies the database without the write-ahead log and produces
# a clone SQLite reports as "database disk image is malformed" — sometimes,
# depending on whether the WAL happened to be checkpointed at that instant. This
# file said "point it at a clone" for two milestones without saying how, and a
# `cp` clone failed exactly that way while P2b was being built. A measurement
# that silently depends on the WAL being empty is a measurement with a hidden
# precondition, which is the class of defect this whole item is about.
#
# WHAT CHANGED AT P2b, and why:
#
#   1. It finishes, and that is now guaranteed rather than hoped for. `verify`
#      on the real corpus has no known upper bound: recorded at 21.9 s when the
#      corpus was 44 projects, at "over 600 s" at A189, and still running at
#      **43 minutes** on a single run while this was being written. Twenty runs
#      of everything was therefore not a long command, it was an unbounded one,
#      and nobody ever ran it as written. `verify` is now a single run with no
#      discarded warm-up under a wall-clock ceiling (VERIFY_BOUND_S, default
#      900). Hitting the ceiling is reported as a bound, not as a number.
#
#   2. It stamps. The run ends by printing a `[[basis]]` block and a `[[figure]]`
#      block in exactly the shape `docs/perf-basis.toml` takes, so a measurement
#      is PASTED into the registry rather than transcribed. A191 stamped its
#      basis by hand and lost the head counts in the process.
#
#   3. The stamp comes from the binary under measurement (`thruline perf
#      --stamp`), not from a second set of SQL in this file. Two implementations
#      of "what the corpus is" is how "projects" came to mean 90 in one document
#      and 91 in another on the same day.

set -euo pipefail

# Kill the child if this script is interrupted.
#
# Without this, killing the suite leaves the invocation it was timing alive —
# and `verify` has no known upper bound on the real corpus. That happened while
# P2b was being built: a superseded run was stopped, its `verify` child kept
# running for another half hour, and the NEXT run was measured on a machine with
# a 6.4 GB scan going in the background. Every number it produced was wrong and
# nothing said so. "Best-effort quiet
# machine" is a precondition, and a precondition you cannot verify is one you
# will violate.
#
# NARROWED (card LOOP-GATE-SIGTERM). This used to be `kill 0`, which signals
# EVERY process in this script's process group — and a script started without
# job control inherits its parent's group, so the group can contain a gate's
# `cargo test`, its `uiserve-*` test binaries, and anything else the same shell
# launched. That is the only group-wide kill this repository contained, and it
# is a standing candidate for the unexplained SIGTERM that ended gate-15's
# cargo-test at 737 s with zero failures. It now signals THIS SCRIPT'S OWN
# DIRECT CHILDREN, by pid, found by parentage and never by name — which is
# exactly the set the paragraph above is about and nothing else.
kill_own_children() {
  local kids
  kids=$(pgrep -P $$ 2>/dev/null | tr '\n' ' ')
  # shellcheck disable=SC2086  # a pid list, deliberately word-split
  [ -n "${kids// /}" ] && kill $kids 2>/dev/null
  return 0
}
trap 'kill_own_children; exit 130' INT TERM

LEDGER="${1:?usage: perf-budget-suite.sh <ledger.db> [runs]}"
RUNS="${2:-20}"
# The wall-clock ceiling on the one unbounded row. Fifteen minutes is long
# enough that a completed `verify` still gets reported, and short enough that
# the documented invocation is guaranteed to end.
VERIFY_BOUND_S="${VERIFY_BOUND_S:-900}"
BIN="target/release/thruline"

[ -f "$LEDGER" ] || { echo "no such ledger: $LEDGER" >&2; exit 1; }
[ -x "$BIN" ] || { echo "build first: cargo build --release" >&2; exit 1; }

# Preflight, so a torn clone is named rather than surfacing as a bare SQLite
# error two lines later. See the header: `cp` of a WAL database is the way this
# goes wrong, and it goes wrong intermittently.
if ! sqlite3 "$LEDGER" "SELECT COUNT(*) FROM events" >/dev/null 2>&1; then
  echo "cannot read $LEDGER." >&2
  echo "If you made it with 'cp', that is the bug: the ledger is in WAL mode." >&2
  echo "Make the clone with:  sqlite3 <source> \".backup $LEDGER\"" >&2
  exit 1
fi

# The rows, in output order, and the figure ids they land under in
# docs/perf-basis.toml. Filled by `measure`.
FIGURES=""

# p95 by nearest-rank, which needs no interpolation and cannot invent a value
# that was never measured.
percentile() {
  local p="$1"; shift
  local sorted; sorted=$(printf '%s\n' "$@" | sort -n)
  local n; n=$(printf '%s\n' "$sorted" | wc -l | tr -d ' ')
  local rank=$(( (p * n + 99) / 100 ))
  [ "$rank" -lt 1 ] && rank=1
  printf '%s\n' "$sorted" | sed -n "${rank}p"
}

# measure <figure-id> <label> <budget|-> <runs> -- <command...>
measure() {
  local id="$1" label="$2" budget="$3" runs="$4"; shift 5   # the 5th is the literal --
  local times=()
  # One warm-up, discarded: the budgets are warm budgets.
  #
  # EXCEPT for a single-run row. `verify` runs for at least tens of minutes on
  # the real corpus, so a discarded warm-up doubles the whole suite's wall clock
  # to pre-warm a measurement that is taken once and has no budget. The first
  # version of this file did exactly that and the documented invocation ran for
  # over half an hour without finishing. A single-run row is measured without a
  # warm-up, in whatever cache state the preceding rows leave, and its emitted
  # figure says so rather than implying a warm number.
  if [ "$runs" -gt 1 ]; then
    "$@" >/dev/null 2>&1 || true
  fi
  for _ in $(seq "$runs"); do
    local start end
    start=$(python3 -c 'import time;print(int(time.perf_counter()*1000000))')
    "$@" >/dev/null 2>&1 || true
    end=$(python3 -c 'import time;print(int(time.perf_counter()*1000000))')
    times+=( $(( (end - start) / 1000 )) )
  done
  local p50 p95 verdict
  p50=$(percentile 50 "${times[@]}")
  p95=$(percentile 95 "${times[@]}")
  if [ "$budget" = "-" ]; then
    verdict="attribution"
  elif [ "$p95" -le "$budget" ]; then
    verdict="PASS"
  else
    verdict="FAIL"
  fi
  printf '| %-34s | %6s | %6s | %8s | %-11s | %4s |\n' \
    "$label" "${p50}" "${p95}" "$budget" "$verdict" "$runs"

  FIGURES="${FIGURES}
[[figure]]
id = \"${id}\"
label = \"${label}\"
basis = \"B-CHANGEME\"
p50_ms = ${p50}
p95_ms = ${p95}"
  if [ "$budget" != "-" ]; then
    FIGURES="${FIGURES}
budget_ms = ${budget}"
  fi
  if [ "$runs" != "$RUNS" ]; then
    FIGURES="${FIGURES}
note = \"${runs} run(s), not the suite's ${RUNS} — bounded deliberately, and with no discarded warm-up, so this is not a warm number.\""
  fi
  FIGURES="${FIGURES}
"
}

# measure_bounded <figure-id> <label> <bound-seconds> -- <command...>
#
# One run, with a wall-clock ceiling. `verify` has no upper bound on this corpus
# — it was recorded at 21.9 s when "the corpus" meant 44 projects, at "over
# 600 s" at A189, and it was still running at **43 minutes** on one run while
# P2b was being written. A suite with an unbounded row is not runnable as
# documented in any useful sense, which is the defect this item exists to close.
#
# Exceeding the bound is a RESULT, not an error: "> N s, aborted" is a true and
# useful statement, and it is a stronger one than a number nobody waited for.
measure_bounded() {
  local id="$1" label="$2" bound="$3"; shift 4   # the 4th is the literal --
  local start end elapsed waited=0 timed_out=0
  start=$(python3 -c 'import time;print(int(time.perf_counter()*1000000))')
  "$@" >/dev/null 2>&1 &
  local pid=$!
  while kill -0 "$pid" 2>/dev/null; do
    if [ "$waited" -ge "$bound" ]; then
      kill "$pid" 2>/dev/null || true
      timed_out=1
      break
    fi
    sleep 5
    waited=$(( waited + 5 ))
  done
  wait "$pid" 2>/dev/null || true
  end=$(python3 -c 'import time;print(int(time.perf_counter()*1000000))')
  elapsed=$(( (end - start) / 1000 ))

  if [ "$timed_out" = 1 ]; then
    printf '| %-34s | %6s | %6s | %8s | %-11s | %4s |\n' \
      "$label" "> $((bound))s" "—" "-" "aborted" "1"
    FIGURES="${FIGURES}
[[figure]]
id = \"${id}\"
label = \"${label}\"
basis = \"B-CHANGEME\"
note = \"Exceeded the suite's ${bound} s ceiling on one run and was aborted. Not a warm number and not a completed one — a bound. Whether this row belongs in the standing suite at all is Q-A189.2.\"
"
  else
    printf '| %-34s | %6s | %6s | %8s | %-11s | %4s |\n' \
      "$label" "$elapsed" "—" "-" "attribution" "1"
    FIGURES="${FIGURES}
[[figure]]
id = \"${id}\"
label = \"${label}\"
basis = \"B-CHANGEME\"
p50_ms = ${elapsed}
note = \"One run, no discarded warm-up, under a ${bound} s ceiling it did not hit. Not a warm number.\"
"
  fi
}

BIGGEST=$(sqlite3 "$LEDGER" \
  "SELECT project_id FROM events GROUP BY project_id ORDER BY COUNT(*) DESC LIMIT 1")
REPO=$(sqlite3 "$LEDGER" \
  "SELECT repo_id FROM workspace_identities WHERE repo_id IS NOT NULL
   GROUP BY repo_id ORDER BY COUNT(*) DESC LIMIT 1" 2>/dev/null || true)

HARNESS_SHA=$(shasum -a 256 "$0" | cut -c1-16)

# "Best-effort quiet machine" is a precondition, and an unrecorded precondition
# is one nobody can check afterwards. A developer's machine is rarely quiet —
# this run happened alongside another project's API server and a VM — so the
# load average is captured before and after and travels with the stamp instead
# of being asserted away.
LOAD_BEFORE=$(uptime | sed 's/.*load averages*: *//')

echo
echo "machine   $(sysctl -n machdep.cpu.brand_string 2>/dev/null || uname -m)"
echo "cores     $(sysctl -n hw.ncpu 2>/dev/null || nproc)"
echo "memory    $(( $(sysctl -n hw.memsize 2>/dev/null || echo 0) / 1073741824 )) GiB"
echo "os        $(uname -sr)"
echo "ledger    $LEDGER"
echo "runs      $RUNS per measurement, warm, one discarded warm-up, best-effort quiet machine"
echo "biggest   $BIGGEST"
echo "harness   $(basename "$0") sha256:$HARNESS_SHA"
echo "load      $LOAD_BEFORE (1/5/15 min, before the first measurement)"
echo
echo "The precondition is part of the measurement. This run is against the path"
echo "above, in whatever state it is in; the suite does not rebuild, ingest, or"
echo "vacuum first, and it does not check that you pointed it at a clone."
echo
printf '| %-34s | %6s | %6s | %8s | %-11s | %4s |\n' \
  "measurement" "p50 ms" "p95 ms" "budget" "verdict" "runs"
printf '|%s|%s|%s|%s|%s|%s|\n' "------------------------------------" "--------" "--------" \
  "----------" "-------------" "------"

measure now-single  "now (single project)"     100 "$RUNS" -- "$BIN" --db "$LEDGER" now --project "$BIGGEST"
measure now-all     "now --all"                100 "$RUNS" -- "$BIN" --db "$LEDGER" now
measure packet-one  "packet (single project)"  150 "$RUNS" -- "$BIN" --db "$LEDGER" packet --project "$BIGGEST"
if [ -n "$REPO" ]; then
  measure packet-repo "packet (repo-scoped)"   150 "$RUNS" -- "$BIN" --db "$LEDGER" packet --repo "$REPO"
fi
measure projects-agg "projects (full aggregation, NOT a bare open)" - "$RUNS" -- "$BIN" --db "$LEDGER" projects
# One run under a wall-clock ceiling. See `measure_bounded`: twenty runs of this
# was over five hours, and one run has no known upper bound at all — it was still
# going at 43 minutes while this file was being written.
measure_bounded verify "verify (whole ledger)" "$VERIFY_BOUND_S" -- "$BIN" --db "$LEDGER" verify

echo
RSS=$( { /usr/bin/time -l "$BIN" --db "$LEDGER" packet --project "$BIGGEST" >/dev/null; } 2>&1 \
  | awk '/maximum resident set size/ {print $1}')
if [ -n "${RSS:-}" ]; then
  RSS_MIB=$(( RSS / 1048576 ))
  echo "peak RSS  ${RSS_MIB} MiB   (budget: full-process <= 1024 MiB)"
  FIGURES="${FIGURES}
[[figure]]
id = \"peak-rss\"
label = \"peak RSS (packet, largest project)\"
basis = \"B-CHANGEME\"
note = \"${RSS_MIB} MiB against a 1024 MiB full-process budget. One run.\"
"
fi

echo
echo "---------------- paste into docs/perf-basis.toml ----------------"
echo "# Replace B-CHANGEME with a real id and set 'recorded'."
echo
"$BIN" --db "$LEDGER" perf --stamp | sed "s|^recorded = .*|recorded = \"$(date -u +%Y-%m-%d)\"|"
echo "harness = \"scripts/$(basename "$0") sha256:$HARNESS_SHA\""
echo "note = \"Load average $LOAD_BEFORE at the start, \
$(uptime | sed 's/.*load averages*: *//') at the end (1/5/15 min). \
The machine was not idle; it never is.\""
echo "machine = \"$(sysctl -n machdep.cpu.brand_string 2>/dev/null || uname -m), \
$(sysctl -n hw.ncpu 2>/dev/null || nproc) cores, \
$(( $(sysctl -n hw.memsize 2>/dev/null || echo 0) / 1073741824 )) GiB, $(uname -sr)\""
echo "$FIGURES"
echo "-----------------------------------------------------------------"
echo
echo "Then: thruline perf --doc-block > the block in docs/perf-baseline.md"
echo
