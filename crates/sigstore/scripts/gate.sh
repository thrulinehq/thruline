#!/usr/bin/env bash
# Run every gate at a given commit, in a fresh clone, and print the table.
#
# # Why this is a script and not a paragraph
#
# The merge queue asks the plan to "name HEAD and state exit codes at a fresh
# directory". A static document cannot: the commit that *records* a gate is
# necessarily a commit later than the one it gates, so a written table is always
# one commit stale by construction. Restating the figures by hand just moves the
# staleness around.
#
# So the plan names the commit it was gated at *and* names this script. Anyone
# can re-run it at HEAD and get the current answer, which is the property the
# queue actually wants — the numbers are reproducible rather than asserted.
#
# # Exit codes are real
#
# Every gate runs UNPIPED and its own `$?` is captured. An earlier version of
# the table scraped counts through `awk`, which reports awk's status and not
# cargo's — a failing suite piped into a successful filter reads as green. The
# counts here are parsed from a saved log afterwards, never from the pipeline
# that determines the exit code.
#
# Usage: scripts/gate.sh [ref]        (default: HEAD)
#        THRULINE_CORE=/path/to/thruline scripts/gate.sh
#
# Exits non-zero if any gate did.
set -uo pipefail

CRATE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
REF="${1:-HEAD}"
CORE="${THRULINE_CORE:-$HOME/thruline}"

WORK="$(mktemp -d)"
LOG="$WORK/log"
# Ruling 16 / E's amendment: a build directory is reaped once its gate is
# recorded. This one reaps itself even if a gate fails.
trap 'rm -rf "$WORK"' EXIT

git -C "$CRATE" rev-parse --verify --quiet "$REF" >/dev/null || {
  echo "no such ref: $REF" >&2; exit 2; }

git clone -q "$CRATE" "$WORK/c"
git -C "$WORK/c" checkout -q "$REF"

HASH="$(git -C "$WORK/c" rev-parse --short HEAD)"
FILES="$(git -C "$WORK/c" ls-files | wc -l | tr -d ' ')"
CORE_HASH="$(git -C "$CORE" rev-parse --short HEAD 2>/dev/null || echo 'absent')"
# Is the core tree in a state anyone would cite?
#
# Measured: a gate recorded `core=af6a7c6`, which was a commit the core itself
# had labelled "SCRATCH, not for push" and moved off again minutes later. The
# contract held there, but citing a transient state as the verified core is
# misleading — another session is committing to that tree while this runs, and a
# four-minute gate cannot assume the head it started with is the head it ends
# with. So the observed state is recorded, dirtiness included, rather than
# assumed clean.
CORE_STATE="clean"
if [ -n "$(git -C "$CORE" status --porcelain 2>/dev/null | head -1)" ]; then
  CORE_STATE="DIRTY(uncommitted changes)"
fi
if [ -e "$CORE/.git/MERGE_HEAD" ]; then CORE_STATE="$CORE_STATE MID-MERGE"; fi
CORE_SUBJECT="$(git -C "$CORE" log -1 --format=%s 2>/dev/null | cut -c1-60)"

cd "$WORK/c"
export CARGO_TARGET_DIR="$WORK/t"
export THRULINE_CORE="$CORE"

fail=0
declare -a ROWS=()

gate () {
  local label="$1"; shift
  "$@" >"$LOG" 2>&1
  local rc=$?
  [ $rc -ne 0 ] && fail=1
  # Counts are parsed AFTER the fact, from the saved log — never from a pipe
  # whose exit status would mask the command's.
  # `END{if(NR)}` would be wrong: NR counts every line read, not every line
  # MATCHED, so a gate with no test output (clippy, fmt) reported a confident
  # "0 passing, 0 failing" instead of "not applicable". Count matches.
  local counts
  counts="$(awk '/^test result/{n++;p+=$4;f+=$6;i+=$8} END{if(n)printf "%d passing, %d failing, %d ignored",p,f,i}' "$LOG")"
  [ -z "$counts" ] && counts="—"
  ROWS+=("| \`$label\` | **$rc** | $counts |")
  printf '  %-46s exit=%d  %s\n' "$label" "$rc" "$counts" >&2
}

echo "gating $HASH against core $CORE_HASH [$CORE_STATE] ($FILES tracked files)" >&2
echo "  core head: $CORE_SUBJECT" >&2
gate "cargo fmt --check"                                   cargo fmt --check
gate "cargo clippy --all-targets --all-features -D warnings" cargo clippy --all-targets --all-features -- -D warnings
gate "cargo test --all-features"                           cargo test --all-features
gate "cargo test (default features)"                       cargo test
gate "cargo test --test core_contract"                     cargo test --test core_contract
gate "scripts/mutation-check.sh"                           ./scripts/mutation-check.sh
gate "scripts/fresh-machine-verify.sh"                     ./scripts/fresh-machine-verify.sh

# The table, ready to paste into MERGE-PLAN §0.
{
  echo
  echo "| gate | exit | result at \`$HASH\` |"
  echo "|---|---|---|"
  printf '%s\n' "${ROWS[@]}"
  echo "| tracked files | — | $FILES; nothing under \`target/\` |"
  echo "| core contract verified against | — | \`$CORE_HASH\` — $CORE_STATE |"
  echo "| core head subject | — | $CORE_SUBJECT |"
  echo
  echo "_Produced by \`scripts/gate.sh $REF\`; re-runnable at any commit._"
  # A completion marker, because a run that DIED mid-way leaves a plausible
  # partial table and no other sign. Measured: an earlier run was killed by a
  # syntax error and the wrapper around it still reported exit 0, because that
  # was the wrapper's status and not the script's. Cite no table that lacks
  # this line.
  echo
  echo "<!-- GATE COMPLETE $HASH core=$CORE_HASH core_state=$CORE_STATE gates=${#ROWS[@]} failed=$fail -->"
}

[ $fail -eq 0 ] || { echo "AT LEAST ONE GATE FAILED" >&2; exit 1; }
