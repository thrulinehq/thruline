#!/usr/bin/env bash
#
# Observed refusals for scripts/pilot-run.sh's start-up guards.
#
#   scripts/pilot-run-guard-test.sh
#
# Every guard here is EXERCISED, not asserted about. A guard that has only ever
# been read is a guard nobody has seen refuse, and this program has now shipped
# nine of those. Each case below either watches the real code refuse, or
# measures the platform behaviour the guard exists because of.
#
# It needs no Docker, no network, no credential and no corpus: the cases that
# need the script build a PROLOGUE — lines 1..<end of the occupancy loop> of
# pilot-run.sh, verbatim — and run that. The prologue contains the run-id
# grammar gate, the lock, the trap and the occupancy check, which is exactly
# what is under test; nothing after it is reached, so nothing is built, run or
# paid for.
#
# THRULINE_PILOT_SP is honoured, and defaults to a throwaway directory made
# here, so this never needs the real corpus.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="$REPO_ROOT/scripts/pilot-run.sh"
TRIALS="${GUARD_TEST_TRIALS:-20}"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/thruline-guard-test.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

PASS=0
FAIL=0
ok()   { printf '  ok   %s\n' "$*"; PASS=$((PASS+1)); }
bad()  { printf '  FAIL %s\n' "$*"; FAIL=$((FAIL+1)); }
head1() { printf '\n== %s ==\n' "$*"; }

# ---------------------------------------------------------------------------
# A throwaway pilot tree. Only the shape the prologue reads is needed: a task
# directory under tb-src/original-tasks.
# ---------------------------------------------------------------------------
SP="$WORK/sp"
mkdir -p "$SP/pilot/tb-src/original-tasks/broken-python"

# ---------------------------------------------------------------------------
# The prologue, cut from the real script at the first top-level `done`, which is
# the end of the occupancy loop.
# ---------------------------------------------------------------------------
END="$(grep -n '^done$' "$SRC" | head -1 | cut -d: -f1)"
if [ -z "$END" ]; then
  echo "guard-test: could not find the occupancy loop in $SRC" >&2
  exit 2
fi
PRE="$WORK/prologue.sh"
head -n "$END" "$SRC" > "$PRE"
# THE WINNER RECORDS THE LOCK'S INODE, and that is not decoration.
#
# `mkdir` allocates a NEW inode every time it creates a directory. So two
# winners sharing an inode means two processes passed one `mkdir` — a real race
# — while two winners with DIFFERENT inodes means the lock was created,
# released, and created again, which is a lock working correctly. Without this
# field the two are indistinguishable, and S4b spent weeks reporting them under
# one message ("0 or >1 winners") that could not tell you which had happened.
# THE HOLD ENDS WHEN THE CONTENTION HAS HAPPENED, NOT AFTER A DURATION I GUESSED.
#
# The first fix for S4b's flake held the lock for a fixed 3 s, chosen to outlast
# a measured 0.5 s launch skew. Track C read that and named it: a noisy quantity
# compared against a fixed bound is the same shape as the memory test that has
# flaked five times elsewhere on this board today, and the honest answer was that
# S4b was robust only by the sleep. Under skew worse than the bound it would go
# red again, and the bound would have to be raised — which is guessing twice.
#
# So the winner now WAITS FOR ITS CONTENDERS instead. Each loser's `die` prints
# `is LOCKED`; with one output file per contender the winner counts those files
# and releases the moment `GUARD_CONTENDERS` of them exist. Skew stops mattering:
# however late a contender starts, the lock is still held when it arrives.
#
# `GUARD_HOLD_SECS` survives as a CEILING, not the mechanism, and hitting it
# writes `HOLD-CEILING` so a hold that ended for the wrong reason is reported
# rather than passing as a fast one. Without `GUARD_CONTENDERS` the old fixed
# sleep is used unchanged — S4c needs it, because it is waiting to be sent a
# SIGTERM and has no contenders to wait for.
cat >> "$PRE" <<'EOF'
printf 'ACQUIRED %s inode=%s\n' "$$" "$(stat -f %i "$LOCK" 2>/dev/null || stat -c %i "$LOCK" 2>/dev/null || echo NOINO)" >> "$GUARD_LOG"
if [ "${GUARD_HOLD:-0}" -eq 1 ]; then
  if [ -n "${GUARD_CONTENDERS:-}" ]; then
    __n=0
    __ceil=$(( ${GUARD_HOLD_SECS:-30} * 20 ))
    until [ "$(grep -l 'is LOCKED' "$GUARD_OUT"/out-* 2>/dev/null | wc -l | tr -d ' ')" \
              -ge "$GUARD_CONTENDERS" ] || [ "$__n" -ge "$__ceil" ]; do
      sleep 0.05; __n=$((__n+1))
    done
    [ "$__n" -ge "$__ceil" ] && printf 'HOLD-CEILING %s\n' "$$" >> "$GUARD_LOG"
  else
    sleep "${GUARD_HOLD_SECS:-30}"
  fi
fi
exit 0
EOF
chmod +x "$PRE"
printf 'prologue: lines 1-%s of scripts/pilot-run.sh\n' "$END"

run_prologue() { # run_prologue <run-id> <log> [hold] [hold-secs] [contenders] [outdir]
  env THRULINE_PILOT_SP="$SP" PILOT_RUN_ID="$1" PILOT_TASK_ID=broken-python \
      GUARD_LOG="$2" GUARD_HOLD="${3:-0}" GUARD_HOLD_SECS="${4:-30}" \
      GUARD_CONTENDERS="${5:-}" GUARD_OUT="${6:-}" bash "$PRE" oracle
}

# ---------------------------------------------------------------------------
head1 "S4c  a returning trap ABSORBS SIGTERM — why INT/TERM must re-raise"
# This is the measurement the trap comment in pilot-run.sh cites. Three
# one-page scripts, identical but for their traps, each sent a real SIGTERM
# while sleeping.
mk() { printf '#!/bin/bash\nset -euo pipefail\n%s\necho READY > "$1"\nsleep 20\necho RESUMED >> "$1"\nexit 0\n' "$2" > "$1"; chmod +x "$1"; }
mk "$WORK/t-none.sh" ''
mk "$WORK/t-return.sh" 'h() { : ; }
trap h EXIT INT TERM'
mk "$WORK/t-reraise.sh" 'c() { : ; }
on_signal() { c; trap - "$1"; kill -s "$1" $$; }
trap c EXIT
trap '"'"'on_signal INT'"'"' INT
trap '"'"'on_signal TERM'"'"' TERM'

sigterm_rc() { # sigterm_rc <script> -> "<rc> <resumed?>"
  local out="$WORK/$(basename "$1").out"; : > "$out"
  "$1" "$out" >/dev/null 2>&1 &
  local pid=$! n=0
  until grep -q READY "$out" 2>/dev/null || [ $n -ge 200 ]; do sleep 0.05; n=$((n+1)); done
  kill -TERM "$pid" 2>/dev/null
  wait "$pid" 2>/dev/null; local rc=$?
  local res=no; grep -q RESUMED "$out" 2>/dev/null && res=yes
  printf '%s %s' "$rc" "$res"
}
r_none="$(sigterm_rc "$WORK/t-none.sh")"
r_ret="$(sigterm_rc "$WORK/t-return.sh")"
r_re="$(sigterm_rc "$WORK/t-reraise.sh")"
printf '  no trap          : exit=%s resumed=%s\n' $r_none
printf '  returning trap   : exit=%s resumed=%s\n' $r_ret
printf '  re-raising trap  : exit=%s resumed=%s\n' $r_re
[ "${r_none%% *}" = "143" ] && ok "no trap dies of SIGTERM (143)" \
  || bad "no trap: expected 143, got '$r_none'"
[ "$r_ret" = "0 yes" ] \
  && ok "a RETURNING trap absorbs SIGTERM: exit 0 and the script resumes — the bug" \
  || bad "returning trap: expected '0 yes', got '$r_ret'"
[ "$r_re" = "143 no" ] \
  && ok "the RE-RAISING trap used by pilot-run.sh dies of the signal (143), no resume" \
  || bad "re-raising trap: expected '143 no', got '$r_re'"

# ---------------------------------------------------------------------------
head1 "S4c  a real SIGTERM to the prologue: non-zero exit AND the lock released"
LOG="$WORK/sig.log"; : > "$LOG"
RID="sigterm-case"
# Backgrounded DIRECTLY, not through run_prologue: `f &` where f is a shell
# function forks a subshell that then runs the command, so `$!` would be the
# subshell and the SIGTERM would never reach the script under test. That mistake
# made this case report a leaked lock the guard had not leaked — worth writing
# down, because a test that kills the wrong process reports the wrong verdict
# with total confidence.
env THRULINE_PILOT_SP="$SP" PILOT_RUN_ID="$RID" PILOT_TASK_ID=broken-python \
    GUARD_LOG="$LOG" GUARD_HOLD=1 bash "$PRE" oracle >/dev/null 2>&1 &
pid=$!
n=0
until [ -d "$SP/pilot/lock-$RID" ] || [ $n -ge 200 ]; do sleep 0.05; n=$((n+1)); done
if [ -d "$SP/pilot/lock-$RID" ]; then ok "the lock exists while the run holds it"; else bad "lock never appeared"; fi
kill -TERM "$pid" 2>/dev/null
wait "$pid" 2>/dev/null; rc=$?
printf '  prologue exit after SIGTERM: %s\n' "$rc"
[ "$rc" -ne 0 ] && ok "SIGTERM produces a NON-ZERO exit ($rc), not a completed run" \
  || bad "SIGTERM was absorbed: exit $rc"
[ ! -e "$SP/pilot/lock-$RID" ] && ok "the lock was released by the signal handler" \
  || bad "lock leaked at $SP/pilot/lock-$RID"
rm -rf "$SP/pilot/lock-$RID" "$SP/pilot/staged-$RID" "$SP/pilot/mount-$RID" "$SP/pilot/runs-$RID" "$SP/pilot/receipts-$RID"

# ---------------------------------------------------------------------------
head1 "S4b  three concurrent runs sharing one run-id: exactly one may proceed"
#
# THE WINNER NOW HOLDS THE LOCK WHILE THE OTHERS ARRIVE, and that is the whole
# fix. This trial used to pass GUARD_HOLD=0, so the winner printed ACQUIRED and
# exited in MILLISECONDS — and exiting fires `trap cleanup_lock EXIT`
# (pilot-run.sh), which removes the lock. Under load the three launches skew far
# enough that the first process has finished and released before the second
# reaches `mkdir`, so the second acquires a FREE lock, legitimately, and prints
# a second ACQUIRED. The trial then counted ACQUIRED lines across the whole
# trial and called the total "winners" — conflating HELD SIMULTANEOUSLY with
# HELD AT SOME POINT, and failing about one run in three on a busy machine.
#
# Measured before the fix, with the inode field above:
#
#   40 trials at load 44.48   exactly-one 39 · zero 0 · multi 1
#   the one multi             inodes 363839511 and 363839602 — DIFFERENT
#   multi sharing an inode    0, at any load
#   0.5s deliberate stagger   reproduces it every time
#
# The lock was never the problem: no trial at any load ever had two processes
# pass one `mkdir`. The real runner holds this lock across image build, `tb
# run`, fold and ingest — minutes — so a second acquisition there requires the
# first run to have FINISHED. S4c already held the lock for exactly this reason
# and never flaked; S4b is now consistent with it.
#
# `== TRIALS` is KEPT, deliberately. Relaxing to `<= 1` would have turned the
# flake green while blinding the safe direction — a trial where NOBODY acquires
# a free lock would then pass forever. Zero-winner trials did not occur in 40
# trials at load 44, which is not the same as evidence that they cannot.
#
# THREE SECONDS, and why not thirty. The hold only has to outlast the launch
# skew; the measured skew that broke this was 0.5 s. Thirty (S4c's value, which
# is unchanged — it is waiting for a SIGTERM, not for contenders) would cost ten
# minutes over 20 trials. Raise it with GUARD_TEST_HOLD_SECS on a machine so
# loaded that three `bash` starts cannot land inside three seconds.
HOLD_SECS="${GUARD_TEST_HOLD_SECS:-30}"
WIN=0
CEILING_HITS=0
for t in $(seq 1 "$TRIALS"); do
  RID="conc-$$-$t"
  LOG="$WORK/conc-$t.log"; : > "$LOG"
  OUTD="$WORK/conc-out-$t"; rm -rf "$OUTD"; mkdir -p "$OUTD"
  # One output file per contender, which is what lets the winner COUNT refusals
  # instead of waiting out a clock. Two of the three will contain `is LOCKED`.
  for i in 1 2 3; do
    run_prologue "$RID" "$LOG" 1 "$HOLD_SECS" 2 "$OUTD" > "$OUTD/out-$i" 2>&1 &
  done
  wait
  grep -q 'HOLD-CEILING' "$LOG" 2>/dev/null && CEILING_HITS=$((CEILING_HITS+1))
  n="$(grep -c ACQUIRED "$LOG" 2>/dev/null || true)"
  [ -z "$n" ] && n=0
  [ "$n" -eq 1 ] && WIN=$((WIN+1))
  # A trial that failed says WHICH way it failed, so the next reader does not
  # have to rebuild the inode probe to find out.
  if [ "$n" -ne 1 ]; then
    inos="$(sed -n 's/.*inode=//p' "$LOG" | sort -u | wc -l | tr -d ' ')"
    printf '  trial %s: %s winner(s), %s distinct lock inode(s)%s\n' \
      "$t" "$n" "$inos" \
      "$([ "$n" -gt 1 ] && [ "$inos" -eq 1 ] && printf ' — SAME INODE: TWO PROCESSES PASSED ONE mkdir' || printf '')"
    sed 's/^/      /' "$LOG"
  fi
  rm -rf "$SP/pilot/lock-$RID" "$SP/pilot/staged-$RID" "$SP/pilot/mount-$RID" "$SP/pilot/runs-$RID" "$SP/pilot/receipts-$RID"
done
printf '  exactly one winner in %s of %s trials (winner holds until both contenders are refused)\n' "$WIN" "$TRIALS"
[ "$WIN" -eq "$TRIALS" ] && ok "the mkdir lock is atomic under 3-way concurrency" \
  || bad "$((TRIALS-WIN)) of $TRIALS trials did not have exactly one SIMULTANEOUS winner"
# THE HOLD MUST HAVE ENDED FOR THE RIGHT REASON. If the winner released on the
# ceiling rather than on its contenders arriving, the trial measured a timeout
# and the pass above means what the old fixed sleep meant — which is the thing
# this rework removed. A green that reached the ceiling is not a green.
[ "$CEILING_HITS" -eq 0 ] \
  && ok "and every hold ended on contention, not on the ${HOLD_SECS}s ceiling" \
  || bad "$CEILING_HITS of $TRIALS trials hit the hold ceiling: the winner timed out instead of observing its contenders, so those trials are back to comparing a noisy quantity against a fixed bound"

# ---------------------------------------------------------------------------
head1 "S4b  a DELIBERATE stagger: the hold must outlast a late contender"
# THE QUESTION THIS ANSWERS, asked by Track C: is S4b robust by construction, or
# only by the sleep? Under the fixed 3 s hold it was only by the sleep — a noisy
# quantity (launch skew) against a fixed bound, which is the shape that has
# flaked repeatedly elsewhere on this board. Raising the bound would have been
# guessing twice.
#
# The winner now waits until both contenders have been REFUSED, so skew cannot
# outrun it. This case proves that by making the skew large on purpose: the
# contenders are launched a full STAGGER seconds after the winner, far past any
# hold a fixed sleep would plausibly have used. Exactly one winner must still be
# observed, and the hold must have ended on contention rather than the ceiling.
STAGGER="${GUARD_TEST_STAGGER_SECS:-6}"
RID="stagger-$$"
LOG="$WORK/stagger.log"; : > "$LOG"
OUTD="$WORK/stagger-out"; rm -rf "$OUTD"; mkdir -p "$OUTD"
run_prologue "$RID" "$LOG" 1 "$HOLD_SECS" 2 "$OUTD" > "$OUTD/out-1" 2>&1 &
sleep "$STAGGER"
for i in 2 3; do
  run_prologue "$RID" "$LOG" 1 "$HOLD_SECS" 2 "$OUTD" > "$OUTD/out-$i" 2>&1 &
done
wait
n="$(grep -c ACQUIRED "$LOG" 2>/dev/null || true)"; [ -z "$n" ] && n=0
printf '  contenders launched %ss after the winner: %s winner(s)\n' "$STAGGER" "$n"
[ "$n" -eq 1 ] \
  && ok "the hold outlasted a ${STAGGER}s stagger — robust by construction, not by a chosen duration" \
  || bad "$n winners under a ${STAGGER}s stagger. The winner released before its contenders arrived, so the hold is again a fixed bound against a noisy quantity."
grep -q 'HOLD-CEILING' "$LOG" 2>/dev/null \
  && bad "the hold ended on the ceiling, not on contention — this case proves nothing about construction" \
  || ok "and it ended on contention, not on the ceiling"
rm -rf "$SP/pilot/lock-$RID" "$SP/pilot/staged-$RID" "$SP/pilot/mount-$RID" "$SP/pilot/runs-$RID" "$SP/pilot/receipts-$RID"

# THE CONTROL FOR IT, and it is deterministic on purpose. The obvious control —
# re-run with the old fixed sleep and watch it fail — depends on the winner's
# hold expiring before the contenders arrive, which is a race between two
# durations and could itself flake. Instead the winner is told NOT TO HOLD AT
# ALL (`GUARD_HOLD=0`): it releases on exit, every time, and a contender
# arriving STAGGER seconds later must then acquire a free lock. If that does not
# produce a second winner, the case above is passing for some reason other than
# the hold and proves nothing about construction.
RID="stagger-ctl-$$"
LOG="$WORK/stagger-ctl.log"; : > "$LOG"
OUTD="$WORK/stagger-ctl-out"; rm -rf "$OUTD"; mkdir -p "$OUTD"
run_prologue "$RID" "$LOG" 0 > "$OUTD/out-1" 2>&1 &
sleep "$STAGGER"
for i in 2 3; do run_prologue "$RID" "$LOG" 0 > "$OUTD/out-$i" 2>&1 & done
wait
nc="$(grep -c ACQUIRED "$LOG" 2>/dev/null || true)"; [ -z "$nc" ] && nc=0
printf '  same stagger with NO hold: %s winner(s)\n' "$nc"
[ "$nc" -ge 2 ] \
  && ok "a winner that does not hold lets a late contender in — so the case above is measuring the hold" \
  || bad "a non-holding winner still produced $nc winner(s) under a ${STAGGER}s stagger. The stagger case cannot distinguish a hold that works from one that never mattered."
rm -rf "$SP/pilot/lock-$RID" "$SP/pilot/staged-$RID" "$SP/pilot/mount-$RID" "$SP/pilot/runs-$RID" "$SP/pilot/receipts-$RID"

# ---------------------------------------------------------------------------
head1 "S4b  the mutation control: a NON-ATOMIC lock must make that trial FAIL"
# A trial that has never been seen to fail is not a trial. This is the same
# discipline `split_enforcement_census.rs` applies to its probes: the assertion
# above is worth exactly what its failure direction is worth, and until now
# nobody had watched it refuse a broken lock.
#
# The mutation is the classic one the real guard exists to avoid: a test
# followed by an act. `[ -d "$LOCK" ] || mkdir -p "$LOCK"` looks equivalent to
# `mkdir "$LOCK"` and is not — two processes can both pass the test before
# either acts, and both proceed.
BROKEN="$WORK/prologue-broken.sh"
head -n "$END" "$SRC" \
  | sed 's@^if mkdir "$LOCK" 2>/dev/null; then@if [ ! -d "$LOCK" ] \&\& mkdir -p "$LOCK"; then@' \
  > "$BROKEN"
cat >> "$BROKEN" <<'EOF'
printf 'ACQUIRED %s inode=%s\n' "$$" "$(stat -f %i "$LOCK" 2>/dev/null || stat -c %i "$LOCK" 2>/dev/null || echo NOINO)" >> "$GUARD_LOG"
if [ "${GUARD_HOLD:-0}" -eq 1 ]; then sleep "${GUARD_HOLD_SECS:-30}"; fi
exit 0
EOF
if ! grep -q 'mkdir -p "$LOCK"' "$BROKEN"; then
  bad "the mutation did not apply — the lock's spelling in pilot-run.sh changed, so this control proves nothing"
else
  MUT_MULTI=0
  for t in $(seq 1 5); do
    RID="mut-$$-$t"; LOG="$WORK/mut-$t.log"; : > "$LOG"
    # A FIXED SHORT HOLD HERE, DELIBERATELY, AND NOT THE CONTENTION WAIT.
    # Under the mutated lock ALL THREE processes acquire, so none of them ever
    # prints `is LOCKED` and there is nothing to wait for: every contender would
    # sit out the full ceiling and this control would take 30 s a trial to prove
    # something that is true within milliseconds. Waiting on contenders is the
    # right mechanism only where contention actually happens.
    for i in 1 2 3; do
      env THRULINE_PILOT_SP="$SP" PILOT_RUN_ID="$RID" PILOT_TASK_ID=broken-python \
          GUARD_LOG="$LOG" GUARD_HOLD=1 GUARD_HOLD_SECS=1 \
          bash "$BROKEN" oracle >/dev/null 2>&1 &
    done
    wait
    n="$(grep -c ACQUIRED "$LOG" 2>/dev/null || true)"; [ -z "$n" ] && n=0
    [ "$n" -gt 1 ] && MUT_MULTI=$((MUT_MULTI+1))
    rm -rf "$SP/pilot/lock-$RID" "$SP/pilot/staged-$RID" "$SP/pilot/mount-$RID" "$SP/pilot/runs-$RID" "$SP/pilot/receipts-$RID"
  done
  printf '  a test-then-act lock admitted >1 simultaneous winner in %s of 5 trials\n' "$MUT_MULTI"
  [ "$MUT_MULTI" -gt 0 ] \
    && ok "the trial DETECTS a non-atomic lock — so the pass above is load-bearing" \
    || bad "a deliberately non-atomic lock still produced exactly one winner every time. The trial above cannot tell a broken lock from a working one and its pass means nothing."
fi

# ---------------------------------------------------------------------------
head1 "S4b  the lock refuses, and does NOT break a live lock for you"
RID="held"
mkdir -p "$SP/pilot/lock-$RID"
printf 'pid=99999\nhost=elsewhere\n' > "$SP/pilot/lock-$RID/owner"
OUT="$(run_prologue "$RID" "$WORK/held.log" 2>&1)"; rc=$?
printf '%s\n' "$OUT" | sed 's/^/  | /' | head -6
[ "$rc" -ne 0 ] && ok "a held lock refuses (exit $rc)" || bad "a held lock did not refuse"
case "$OUT" in *"is LOCKED"*) ok "and names the lock" ;; *) bad "message did not say LOCKED" ;; esac
case "$OUT" in *"pid=99999"*) ok "and prints the owner so a stale lock can be judged" ;;
               *) bad "owner not shown" ;; esac
[ -d "$SP/pilot/lock-$RID" ] && ok "the live lock was NOT broken automatically" \
  || bad "the refusing run deleted a lock it did not own"
rm -rf "$SP/pilot/lock-$RID"

# ---------------------------------------------------------------------------
head1 "S4a  a DANGLING SYMLINK at \$MOUNT is occupancy, not emptiness"
RID="dangling"
ln -s "$WORK/target-that-does-not-exist" "$SP/pilot/mount-$RID"
OUT="$(run_prologue "$RID" "$WORK/dangle.log" 2>&1)"; rc=$?
printf '%s\n' "$OUT" | sed 's/^/  | /' | head -3
[ "$rc" -ne 0 ] && ok "refused (exit $rc) — \`[ -e ]\` alone follows the link and sees nothing" \
  || bad "a dangling symlink at \$MOUNT did not refuse"
case "$OUT" in *"already taken"*) ok "and says the run-id is taken" ;;
               *) bad "message was not the occupancy refusal" ;; esac
rm -f "$SP/pilot/mount-$RID"; rm -rf "$SP/pilot/lock-$RID"

# ---------------------------------------------------------------------------
head1 "S4a  why \`rm -rf \$MOUNT\` must never gain a trailing slash"
mkdir -p "$WORK/slash/real"; echo evidence > "$WORK/slash/real/spool.jsonl"
ln -s "$WORK/slash/real" "$WORK/slash/link"
rm -rf "$WORK/slash/link/"
if [ ! -e "$WORK/slash/real/spool.jsonl" ]; then
  ok "measured here: \`rm -rf link/\` DELETED THE TARGET'S CONTENTS (link survives)"
else
  bad "\`rm -rf link/\` did not delete the target on this platform — re-check the comment at \`stage task\`"
fi
rm -rf "$WORK/slash"
mkdir -p "$WORK/slash/real"; echo evidence > "$WORK/slash/real/spool.jsonl"
ln -s "$WORK/slash/real" "$WORK/slash/link"
rm -rf "$WORK/slash/link"
[ -f "$WORK/slash/real/spool.jsonl" ] \
  && ok "and \`rm -rf link\` (no slash) removes only the link — the shipped form" \
  || bad "\`rm -rf link\` destroyed the target"

# ---------------------------------------------------------------------------
head1 "S5  a host binary without \`pilot-budget\` is named as such"
# A PAYING agent, so the ceiling is required — and the refusal lands at the
# identity check, which is before preflight, before Docker and before tb. No
# image is built, no harness runs, nothing is spent.
#
# THE CREDENTIAL IS A FAKE, AND HOME IS THIS SUITE'S OWN (2026-10-05). The case
# used to inherit the invoker's HOME, and pilot-run.sh sources
# $HOME/.thruline-bench/openai.env before its credential check — so on a
# machine that holds a real key, `cargo test` loaded that key into this process,
# and on one that does not, the case could never pass. Neither is acceptable for
# a test: it must not read a real credential, and its verdict must not depend on
# whether one exists. Everything before the identity check is local (no curl,
# no docker, no tb), so an obviously fake value reaches exactly the refusal this
# case is about and nothing else.
FAKE="$WORK/fake-thruline"
printf '#!/bin/sh\ncase "$1" in\n  --version) echo "thruline 0.0.0-stale"; exit 0 ;;\nesac\necho "error: unrecognized subcommand" >&2\nexit 2\n' > "$FAKE"
chmod +x "$FAKE"
mkdir -p "$WORK/s5-home"
OUT="$(env HOME="$WORK/s5-home" OPENAI_API_KEY="sk-FAKE-guard-test-not-a-key" \
        THRULINE_PILOT_SP="$SP" THRULINE_HOST_BIN="$FAKE" PILOT_RUN_ID="s5-case" \
        PILOT_TASK_ID=broken-python bash "$SRC" codex gpt-5.6-luna 2>&1)"; rc=$?
printf '%s\n' "$OUT" | sed 's/^/  | /' | head -8
[ "$rc" -ne 0 ] && ok "refused (exit $rc)" || bad "a guard binary without pilot-budget did not refuse"
case "$OUT" in
  *"host binary has no pilot-budget"*)
    ok "and says 'host binary has no pilot-budget'" ;;
  *"budget guard refused this run"*)
    bad "reported a refusal that never happened — the S5 bug is still present" ;;
  *"OPENAI_API_KEY is not set"*)
    printf '  --   skipped: no OPENAI_API_KEY on this machine, so the credential gate\n'
    printf '       refuses first. This case needs the paying path to be reachable;\n'
    printf '       it is NOT a pass and is not counted as one.\n'
    FAIL=$((FAIL+1)) ;;
  *) bad "unexpected message" ;;
esac
case "$OUT" in *"0.0.0-stale"*) ok "and prints the version of the binary it rejected" ;;
               *) printf '  --   version line not shown (see above)\n' ;; esac
rm -rf "$SP/pilot/lock-s5-case"

# ---------------------------------------------------------------------------
head1 "S5  an explicitly-set THRULINE_HOST_BIN that is not executable"
OUT="$(env THRULINE_PILOT_SP="$SP" THRULINE_HOST_BIN="$WORK/not-a-file" \
        PILOT_RUN_ID="s5-missing" PILOT_TASK_ID=broken-python bash "$SRC" oracle 2>&1)"; rc=$?
printf '%s\n' "$OUT" | sed 's/^/  | /' | head -4
[ "$rc" -ne 0 ] && ok "refused (exit $rc) even for the free agent" \
  || bad "a non-executable THRULINE_HOST_BIN was accepted"
rm -rf "$SP/pilot/lock-s5-missing"

# ---------------------------------------------------------------------------
# S6  THE BLOCKS PAST THE OCCUPANCY LOOP, which nothing reached until now.
#
# Everything above runs a PROLOGUE cut at line ~340, so `cargo test` reported
# green while executing none of the runner past that point. Operator ruling 6
# closed that: the musl staging block (~L841) and the fold (~L1450) are changes
# whose regression this suite could not have seen.
#
# A DEEP PROLOGUE, cut after the task copy, reaches the staging block. It needs
# `docker` and `tb`, so both are STUBS on PATH — this suite still touches no
# real Docker, no network, no credential and no corpus, exactly as the header
# says. The stub docker records nothing and answers the four calls the runner
# makes; FAKE_DOCKER_RUN_RC drives the one that matters, the readelf probe.
#
# REPO_ROOT is derived from the script's own directory, so the deep prologue is
# written into a MIMIC of the repository layout with the real scripts and the
# real split fixture symlinked in. Running it from $WORK resolves REPO_ROOT to
# $WORK and the script refuses at the split guard — correctly, and it is worth
# knowing that refusal is what a misplaced prologue looks like.
head1 "S6  the musl staging block stages a static binary and REFUSES a dynamic one"
DEEP_END="$(grep -n "^printf 'copied from %s" "$SRC" | head -1 | cut -d: -f1)"
if [ -z "$DEEP_END" ]; then
  bad "could not find the end of the staging section in $SRC — the deep prologue cannot be cut, so L841 is unreached again"
else
  D="$WORK/deep"
  mkdir -p "$D/bin" "$D/repo/scripts" "$D/repo/fixtures" "$D/musl" \
           "$D/sp/pilot/tb-src/original-tasks/broken-python" "$D/sp/pilot/venv/bin"
  head -n "$DEEP_END" "$SRC" > "$D/repo/scripts/deep.sh"
  printf 'echo REACHED-END-OF-STAGING\nexit 0\n' >> "$D/repo/scripts/deep.sh"
  for f in compose-instrument.py pilot-report.py classify-corpus.py rig; do
    ln -sf "$REPO_ROOT/scripts/$f" "$D/repo/scripts/$f"
  done
  ln -sf "$REPO_ROOT/fixtures/terminal-bench-holdout-v1.tsv" "$D/repo/fixtures/"
  # `image inspect` succeeds so no image is built; `create`/`cp`/`rm` stand in
  # for the binary extraction; `run` is the readelf probe and its exit code is
  # the variable under test.
  cat > "$D/bin/docker" <<'DOCKEREOF'
#!/bin/sh
case "$1" in
  image)  [ "$2" = inspect ] && exit 0 ;;
  create) echo fakecid; exit 0 ;;
  cp)     : > "$3"; chmod +x "$3"; exit 0 ;;
  rm)     exit 0 ;;
  run)    exit "${FAKE_DOCKER_RUN_RC:-0}" ;;
esac
exit 0
DOCKEREOF
  printf '#!/bin/sh\ncase "$1" in --version) echo "thruline 0.1.0-fake";; esac\nexit 0\n' > "$D/bin/thruline"
  printf '#!/bin/sh\nexit 0\n' > "$D/sp/pilot/venv/bin/tb"
  printf '#!/bin/sh\nexit 0\n' > "$D/sp/pilot/venv/bin/python"
  chmod +x "$D/bin/docker" "$D/bin/thruline" "$D/sp/pilot/venv/bin/tb" "$D/sp/pilot/venv/bin/python"
  printf 'x' > "$D/musl/thruline-aarch64-unknown-linux-musl"
  printf 'x' > "$D/musl/thruline-x86_64-unknown-linux-musl"
  # THE MANIFEST THE STAGING BLOCK PINS AGAINST. Without this the mimic has no
  # fixtures/bench/musl-artefacts.tsv, the `[ -f ]` guard is false, and the
  # digest check is skipped — which is how a refusal nothing drives gets added.
  mkdir -p "$D/repo/fixtures/bench"
  MUSL_TSV="$D/repo/fixtures/bench/musl-artefacts.tsv"
  fake_sha() { shasum -a 256 "$1" | cut -d' ' -f1; }
  write_manifest() { # write_manifest <aarch64-sha> <x86_64-sha>
    printf '# arch\tsha256\tbytes\tbuild_id\n' > "$MUSL_TSV"
    printf 'aarch64\t%s\t1\tfake\n' "$1" >> "$MUSL_TSV"
    printf 'x86_64\t%s\t1\tfake\n' "$2" >> "$MUSL_TSV"
  }
  write_manifest "$(fake_sha "$D/musl/thruline-aarch64-unknown-linux-musl")" \
                 "$(fake_sha "$D/musl/thruline-x86_64-unknown-linux-musl")"

  run_deep() { # run_deep <run-id> <readelf-rc>
    rm -rf "$D/sp/pilot/lock-$1" "$D/sp/pilot/staged-$1" "$D/sp/pilot/mount-$1"
    PATH="$D/bin:$PATH" env THRULINE_PILOT_SP="$D/sp" THRULINE_HOST_BIN="$D/bin/thruline" \
      THRULINE_MUSL_DIR="$D/musl" FAKE_DOCKER_RUN_RC="$2" \
      PILOT_RUN_ID="$1" PILOT_TASK_ID=broken-python bash "$D/repo/scripts/deep.sh" oracle 2>&1
  }

  OUT="$(run_deep musl-ok 0)"
  case "$OUT" in *REACHED-END-OF-STAGING*)
      ok "the deep prologue reaches the staging block (line $DEEP_END)" ;;
    *) bad "the deep prologue did not reach the end of staging — L841 is still unreached. Output: $(printf '%s' "$OUT" | tail -3 | tr '\n' ' ')" ;;
  esac
  STAGED_DIR="$D/sp/pilot/mount-musl-ok/bin/arch"
  { [ -f "$STAGED_DIR/aarch64" ] && [ -f "$STAGED_DIR/x86_64" ]; } \
    && ok "a binary passing the readelf probe is staged, per architecture" \
    || bad "the staging block did not stage both architectures into $STAGED_DIR"
  [ -f "$STAGED_DIR/STAGED" ] \
    && ok "and the STAGED manifest the container's refusal quotes is written" \
    || bad "no STAGED manifest — the container's arch refusal would say 'none'"

  # THE FAILURE DIRECTION. A binary carrying PT_INTERP or DT_NEEDED must be
  # refused, not staged: mounting one into 123 containers is the arch/libc
  # failure this block exists to stop, and the previous version of this check
  # was a `file` string match that would have refused a GOOD static-pie binary
  # while accepting nothing worse.
  OUT="$(run_deep musl-bad 1)"
  case "$OUT" in *"is not statically linked"*)
      ok "a binary failing the readelf probe is REFUSED, naming PT_INTERP/DT_NEEDED" ;;
    *) bad "a dynamically linked binary was ACCEPTED as a cross-architecture fallback" ;;
  esac
  n="$(ls "$D/sp/pilot/mount-musl-bad/bin/arch" 2>/dev/null | wc -l | tr -d ' ')"
  [ "${n:-0}" -eq 0 ] \
    && ok "and nothing was staged from the refused directory" \
    || bad "$n file(s) were staged despite the refusal"

  # THE PIN, DRIVEN. The copy in this lane is only worth more than a live read
  # of the producer's build directory if something checks it, and a check that
  # has never been seen refuse is not a check.
  write_manifest "0000000000000000000000000000000000000000000000000000000000000000" \
                 "0000000000000000000000000000000000000000000000000000000000000000"
  OUT="$(run_deep musl-digest 0)"
  case "$OUT" in *"does not match the pin"*)
      ok "a binary whose digest disagrees with the manifest is REFUSED" ;;
    *) bad "a binary was staged whose sha256 does not match fixtures/bench/musl-artefacts.tsv. The copy is then no better than the live read it replaced." ;;
  esac
  n="$(ls "$D/sp/pilot/mount-musl-digest/bin/arch" 2>/dev/null | wc -l | tr -d ' ')"
  [ "${n:-0}" -eq 0 ] && ok "and nothing was staged past the digest refusal" \
    || bad "$n file(s) staged despite a digest mismatch"

  # An architecture the manifest does not describe is also a refusal: an arch
  # nobody recorded is an arch nobody checked.
  printf '# arch\tsha256\tbytes\tbuild_id\n' > "$MUSL_TSV"
  OUT="$(run_deep musl-norow 0)"
  case "$OUT" in *"records no "*)
      ok "an architecture with no manifest row is REFUSED, not staged unpinned" ;;
    *) bad "an unpinned architecture was staged" ;;
  esac

  # ...and restored, so a later case is not read against a mutilated manifest.
  write_manifest "$(fake_sha "$D/musl/thruline-aarch64-unknown-linux-musl")" \
                 "$(fake_sha "$D/musl/thruline-x86_64-unknown-linux-musl")"
fi

# ---------------------------------------------------------------------------
head1 "S6  the fold does not build a repository for its own snapshot to find"
# `thruline fold --repo <r>` re-snapshots <r> at every mutating-tool boundary.
# The fold container mounts only the capture volume — the task workspace is not
# in it and its container is gone — so a `git init` here gives that re-snapshot
# an empty repository to find, and every fold appends a `snapshot` event
# asserting the workspace hashed to the empty tree at a moment when the
# workspace did not exist. Measured on the m1 records: 69 tasks with a real
# baseline tree carry exactly one such fabricated event each.
# OVER THE RUNNING LINES ONLY, and this is the fourth instance of that family
# in this repository. `crates/bench/tests/driver.rs` already documents the
# third: its musl check first matched the COMMENT explaining why musl is wrong,
# "the same wrong-search-space failure this file documents, committed inside the
# test written to guard against it". The first draft of the check below did
# exactly that — the comment above the fold step names `git init` in order to
# explain what was removed, and the check read it as the thing still being
# there. A comment is not an instruction and a search space that includes it
# answers a different question.
FOLD_BODY="$(awk '/^say "fold"$/,/^else$/' "$SRC" | sed 's/[[:space:]]*#.*$//')"
if [ -z "$FOLD_BODY" ]; then
  bad "could not extract the fold step from $SRC — L1450 is unreached"
else
  case "$FOLD_BODY" in
    *"git init"*)
      bad "the fold step runs \`git init\`. That manufactures an empty repository for \`--repo\` to re-snapshot, and the fold will append a snapshot event describing a workspace that is not in this container." ;;
    *) ok "the fold step does NOT create a repository in the fold container" ;;
  esac
  case "$FOLD_BODY" in
    *"workdir.txt"*)
      ok "and it reads the workspace the setup chain recorded, rather than assuming /app" ;;
    *) bad "the fold step does not read /thruline/workdir.txt — it is assuming a workspace path the setup chain no longer hardcodes" ;;
  esac
  case "$FOLD_BODY" in
    *'--repo "$WS"'*)
      ok "and binds --repo to that recorded workspace" ;;
    *) bad "the fold step does not pass the recorded workspace to --repo" ;;
  esac
fi

# ---------------------------------------------------------------------------
printf '\n%s passed, %s failed\n' "$PASS" "$FAIL"
[ "$FAIL" -eq 0 ] || exit 1
