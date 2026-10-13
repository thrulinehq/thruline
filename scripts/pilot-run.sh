#!/usr/bin/env bash
#
# BUILD-SPEC §12 pilot runner — one task, end to end, with the capture chain
# that the oracle validation proved at $0.
#
#   scripts/pilot-run.sh [agent] [model]
#
#   scripts/pilot-run.sh                              # oracle, free, no key needed
#   scripts/pilot-run.sh codex gpt-5.6-luna           # PAYS. Requires the key.
#   scripts/pilot-run.sh claude-code-scripted         # FREE. A real Claude Code
#                                                     # session against a local
#                                                     # fake endpoint. No key.
#
# ---------------------------------------------------------------------------
# NO SHELL TRACING. This script reads a credential, and `set -x` prints every
# command it runs — including the `export OPENAI_API_KEY=…` line inside the
# sourced env file. That would turn a mode-600 file into a line in a log.
# Enforced by, not merely described in:
#   crates/bench/tests/pilot.rs
#     ::no_script_that_touches_the_credential_enables_shell_tracing
# `set -e -u -o pipefail` is the whole of what is allowed here.
# ---------------------------------------------------------------------------
set -euo pipefail

# ---------------------------------------------------------------------------
# `--holdout-run` — the only way to spend a task on the holdout side.
#
# Parsed out of argv BEFORE $1/$2 are read, and accepted anywhere in the
# argument list, so `pilot-run.sh --holdout-run codex gpt-5.6-luna` and
# `pilot-run.sh codex gpt-5.6-luna --holdout-run` mean the same thing. It is a
# flag and not merely an environment variable because a holdout spend should be
# visible in the command someone typed and in the shell history that survives
# it; PILOT_HOLDOUT_RUN=1 is honoured too, for the callers that set the rest of
# this script's inputs the same way.
# ---------------------------------------------------------------------------
#
# THE `done` BELOW IS INDENTED ON PURPOSE, and removing the indent breaks a test
# rather than a style rule. scripts/pilot-run-guard-test.sh builds its
# "prologue" as lines 1..<first line matching ^done$> of this file — the
# occupancy loop's terminator — and runs that to exercise the run-id grammar
# gate, the lock and the trap. A top-level `done` here would cut the prologue at
# this line instead, leaving a 25-line stub, and all 19 of that script's cases
# would exercise nothing while still reporting.
HOLDOUT_RUN="${PILOT_HOLDOUT_RUN:-0}"
_pilot_args=()
for _a in "$@"
  do
    case "$_a" in
      --holdout-run) HOLDOUT_RUN=1 ;;
      *) _pilot_args+=("$_a") ;;
    esac
  done
if [ "${#_pilot_args[@]}" -gt 0 ]; then set -- "${_pilot_args[@]}"; else set --; fi
unset _a

AGENT="${1:-oracle}"
MODEL="${2:-}"
TASK_ID="${PILOT_TASK_ID:-broken-python}"

# No per-session path baked in. The committed default used to be a scratchpad
# under a session UUID that exists on exactly one machine, which made this
# script unrunnable by anyone else without their knowing why.
SP="${THRULINE_PILOT_SP:-$HOME/.thruline-bench}"
PILOT="$SP/pilot"
TB="$PILOT/venv/bin/tb"
# The SAME interpreter tb runs under, never a bare `python3`. Apple's
# /usr/bin/python3 has neither ruamel.yaml nor PyYAML, and
# scripts/compose-instrument.py REQUIRES ruamel — it exits 4 rather than falling
# back to a loader that strips the corpus's canary comments and silently accepts
# a duplicate-key compose file. The pinned archive's venv has both, because
# tb-src/pyproject.toml hard-requires them.
PILOT_PY="$PILOT/venv/bin/python"
TB_SRC="$PILOT/tb-src"
TASKS_SRC="$TB_SRC/original-tasks"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Docker Compose's project-name grammar, which is the ONLY grammar that
# matters here because Compose is what actually consumes this id:
#
#   must consist only of lowercase alphanumeric characters, hyphens and
#   underscores, and must start with a letter or number
#
# The default used to be `...%Y%m%dT%H%M%SZ`. That id is valid everywhere in
# this script, valid as a directory name, valid as a tb --run-id — and invalid
# in the one place it is consumed. The harness builds the Compose project name
# as `<task>-1-of-1-<run-id>`, so two characters of an ISO-8601 timestamp, `T`
# and `Z`, made `docker compose build` fail, and the container never built.
# Hence: no `T`, no `Z`, and an explicit transform plus an assertion below.
RUN_ID_RAW="${PILOT_RUN_ID:-pilot-$AGENT-$(date -u +%Y%m%d-%H%M%S)}"
# BUMPED FROM v2 ON 2026-09-04, and the reason is a measurement, not hygiene.
#
# The `v2` image was built 2026-09-03T11:26Z — BEFORE `9ecc792`, the commit that
# made `hook-install` write `--spool <path>` into the hook command. Its binary
# is bind-mounted over the container's own, so every run since has installed a
# pack that resolves its spool at fire time instead of naming it, which is
# exactly the split ruling 4 closed. Caught by the in-container pack assertion,
# which killed the container at setup — the container died, `tb` printed a
# traceback and STILL EXITED 0, and the scripted run's own endpoint check was
# what refused. Three guards, one of which had to hold.
#
# The tag is bumped rather than `docker image rm`d: another session may have a
# run in flight against v2, and deleting an image out from under it turns this
# fix into someone else's failure.
IMAGE="thruline-oracle:v3"

die() { printf 'pilot-run: %s\n' "$*" >&2; exit 1; }
say() { printf '\n=== %s ===\n' "$*"; }

# An EXPLICIT transform, not a hope that the caller behaved: lowercase, map
# every character outside the grammar to `-`, then drop any leading character
# that is not alphanumeric.
compose_safe() {
  printf '%s' "$1" \
    | tr '[:upper:]' '[:lower:]' \
    | sed -e 's/[^a-z0-9_-]/-/g' -e 's/^[^a-z0-9]*//'
}

# Whole-string, not per-line. `grep -q` matches if ANY line matches, so
# $'ok-run\n!!!INVALID!!!' passed a grep-based check on the strength of its
# first line and handed the rest to Compose. Bash's `=~` anchors to the whole
# string, and no character class here contains a newline.
is_compose_atom() {
  [[ "$1" =~ ^[a-z0-9][a-z0-9_-]*$ ]]
}

RUN_ID="$(compose_safe "$RUN_ID_RAW")"

# **An all-punctuation run-id sanitises to the EMPTY STRING**, and empty was not
# refused. `***` and `###` both produced the Compose project name
# `broken-python-1-of-1-` — identical to each other — and identical `staged-`,
# `mount-` and `runs-` directories. Staging begins `rm -rf "$STAGE" "$MOUNT"`,
# so a second such run DELETES THE FIRST RUN'S CAPTURED SPOOL.
#
# Instance 8's shape again, one consumer further out: a value legal everywhere
# in this script and wrong where it is used — except here the consumer is the
# directory holding the evidence, so the failure mode is destruction rather than
# a refusal. The composed name hid it, because `broken-python-1-of-1-` is itself
# a legal Compose atom.
if [ -z "$RUN_ID" ]; then
  die "run-id '$RUN_ID_RAW' contains no character legal in a Docker Compose
  project name, so it sanitises to the empty string.

  Refusing rather than continuing: an empty run-id makes the staging, mount and
  results directories collide with every other such run, and staging starts with
  'rm -rf' — a second run would destroy the first run's captured spool."
fi
if ! is_compose_atom "$RUN_ID"; then
  die "run-id '$RUN_ID' is not a legal Docker Compose project atom after
  sanitising. Expected ^[a-z0-9][a-z0-9_-]*\$."
fi
if [ "${#RUN_ID}" -gt 64 ]; then
  # Lower severity than the empty case, but a 300-character id also passed.
  die "run-id is ${#RUN_ID} characters; cap is 64. Long ids make container and
  directory names unreadable and push at limits nothing here has measured."
fi

if [ "$RUN_ID" != "$RUN_ID_RAW" ]; then
  # Loudly, never silently: the output directory is named after this, so the
  # caller must be told the id they passed is not the id that will be used.
  printf 'pilot-run: run-id sanitised for Docker Compose: %s -> %s\n' \
    "$RUN_ID_RAW" "$RUN_ID" >&2
fi

# ---------------------------------------------------------------------------
# THE TASK ID — the input that actually reached the consumer unchecked.
#
# The previous gate validated `compose_safe(...)`, which is the SANITISER'S OWN
# OUTPUT, against the sanitiser's own grammar: total onto that grammar, so it
# could never refuse anything (measured: 2000/2000 random inputs passed). And
# `--task-id "$TASK_ID"` went to the harness RAW, so `Broken.Python_X` produced
# exactly the uppercase, dot-bearing project name that killed the paid run.
#
# TASK_ID cannot be sanitised the way RUN_ID can: it names a directory that must
# exist, so rewriting it would just point `cp` at nothing. It is therefore
# CONSTRAINED — and these refusals fire on real input.
# ---------------------------------------------------------------------------
case "$TASK_ID" in
  ""|*/*|*.*|.*)
    die "task id must be a plain directory name inside the dataset — no '/', no
  '.', not empty. Got: '$TASK_ID'.

  This also closes a path-traversal shape: TASK_ID flows into 'cp -R' and into
  'rm -rf \$STAGE', so '../../etc' must not survive this line." ;;
esac
[ -d "$TASKS_SRC/$TASK_ID" ] || die "no task '$TASK_ID' in $TASKS_SRC.
  Constrained to what the pinned dataset actually contains, so a name that
  merely looks plausible cannot reach Docker."
is_compose_atom "$TASK_ID" || die "task '$TASK_ID' exists but its name is not a
  legal Docker Compose project atom (lowercase alphanumerics, hyphens and
  underscores, starting with a letter or number). Refusing rather than mangling
  it: the name IS the source directory, so a rewritten one would not resolve."


# The composed name Compose will actually receive. Both halves are now either
# constrained (task) or transformed (run), so this is a SELF-CHECK ON THE
# TRANSFORM rather than a check on input — it fires when `compose_safe` is
# broken, which is proven by mutating it and watching this refuse, and it is
# honest about being that and not more.
PROJECT_NAME="${TASK_ID}-1-of-1-${RUN_ID}"
if ! is_compose_atom "$PROJECT_NAME"; then
  die "the Docker Compose project name this run would use is invalid:

    $PROJECT_NAME

  Compose requires lowercase alphanumerics, hyphens and underscores only,
  starting with a letter or number. Derived from task '$TASK_ID' and run-id
  '$RUN_ID'. Failing here, at this script's own gate, rather than forty lines
  into a Docker stack trace after the build has already failed."
fi

# Derived from the SANITISED id, never the raw one, so the directories on disk
# and the project name Compose sees cannot disagree about which run this is.
STAGE="$PILOT/staged-$RUN_ID"
MOUNT="$PILOT/mount-$RUN_ID"
OUT="$PILOT/runs-$RUN_ID"
# The compose instrumenter's receipts. A FOURTH directory, and deliberately not
# a subdirectory of any of the other three:
#
#   not under $STAGE — that is tb's --dataset-path, and tb enumerates it with
#     `Path.iterdir()` (terminal_bench/dataset/dataset.py:178,224), which does
#     NOT skip dot-directories. `--task-id` happens to route around that today;
#     a `.thruline-receipts` sitting inside the dataset is a task-shaped
#     directory waiting for the day it does not.
#   not under $MOUNT — that is bind-mounted into the container.
#   not under $OUT — Terminal-Bench treats an existing --output-path as a resume
#     with no flag asked for, so creating it early would make tb skip the task.
#
# And above all not beside the compose file: `postgres-csv-clean` bind-mounts
# `./:/app/data` into the tree the setup chain `git add -A`s, and three tasks
# `COPY . /app`. The instrument must not appear in the tree it measures.
RECEIPTS="$PILOT/receipts-$RUN_ID"

# ---------------------------------------------------------------------------
# OCCUPANCY. The grammar guard above is sound and is not enough.
#
# `compose_safe` is MANY-TO-ONE — `nightly-a!` and `nightly-a?` both sanitise to
# `nightly-a-` — and plain reuse of an explicit PILOT_RUN_ID collides with no
# sanitising at all. Staging then begins `rm -rf "$STAGE" "$MOUNT"`,
# unconditionally, so the second run DELETES THE FIRST RUN'S CAPTURED SPOOL. The
# empty-string case was already refused for exactly this reason; the refusal was
# just narrower than the hazard.
#
# $OUT is in this check although it is NOT in that `rm -rf`, and that asymmetry
# is the worse half. Terminal-Bench sets `self._is_resuming = self._run_path
# .exists()` with no `--resume` flag to ask for it, so a reused id gives:
# capture mount wiped, results directory intact, tb skipping the task as already
# complete. The run reports a pass, and the pass is last time's.
#
# Refusing costs one flag. Continuing costs evidence that cannot be recovered.
#
# ---------------------------------------------------------------------------
# THE LOCK, taken FIRST and ATOMICALLY, because the check below cannot be.
#
# `[ -e ] … die` is a TEST FOLLOWED BY AN ACT, and two processes can both pass
# the test before either acts. Measured against this exact prologue, 3 processes
# sharing one PILOT_RUN_ID: **exactly one winner in 0 of 20 trials — all three
# proceeded, every time.** With the lock: 20 of 20. The directories the check
# looks for are not created until `stage task`, forty lines and one Docker build
# later, so the window is not narrow — it is the whole image build.
#
# `mkdir` on an existing directory fails in ONE syscall; there is no window
# between test and set. `flock` is not on macOS, `ln -s` reintroduces the very
# dangling-symlink problem the occupancy loop below is being fixed for, and
# `set -o noclobber` is defeated by `>|`. mkdir is the portable atomic
# test-and-set.
#
# THE TRAP IS INSTALLED BEFORE THE MKDIR, and its INT/TERM handlers RE-RAISE.
# Both halves are load-bearing:
#
#   * before, because a lock taken and then interrupted one line later would be
#     leaked with nobody holding it;
#   * re-raise, because MEASURED ON THIS MACHINE a bash script whose handler
#     merely returns **absorbs SIGTERM, resumes the next line and exits 0**,
#     where the identical script with no trap exits 143. On a paying run that
#     converts an operator's `kill` into a completed run that spends, charges
#     and reports success. `trap cleanup EXIT INT TERM` — the obvious spelling,
#     and the one the draft of this change used — is exactly that bug.
#     Reproduce it: scripts/pilot-run-guard-test.sh.
LOCK="$PILOT/lock-$RUN_ID"
LOCK_HELD=0
cleanup_lock() {
  if [ "$LOCK_HELD" -eq 1 ]; then rm -rf "$LOCK"; LOCK_HELD=0; fi
  return 0
}
# EXIT gets the plain handler. INT and TERM get handlers that clean up, restore
# the default disposition and re-send the signal to this shell, so the process
# dies OF the signal and the caller sees 130/143 rather than 0.
on_signal() {
  cleanup_lock
  trap - "$1"
  kill -s "$1" $$
}
trap cleanup_lock EXIT
trap 'on_signal INT' INT
trap 'on_signal TERM' TERM

mkdir -p "$PILOT"
if mkdir "$LOCK" 2>/dev/null; then
  LOCK_HELD=1
  printf 'pid=%s\nhost=%s\nstarted=%s\nrun_id=%s\ntask=%s\nagent=%s\n' \
    "$$" "$(hostname)" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$RUN_ID" "$TASK_ID" "$AGENT" \
    > "$LOCK/owner" 2>/dev/null || true
else
  die "run-id '$RUN_ID' is LOCKED — another run holds $LOCK.

  Owner:
$(sed 's/^/    /' "$LOCK/owner" 2>/dev/null || echo '    (no owner file — the lock was interrupted before it could write one)')

  Two invocations with the same run-id share \$STAGE, \$MOUNT and \$OUT, and
  staging starts with 'rm -rf \$STAGE \$MOUNT'. The occupancy check below cannot
  stop that on its own: it is a test followed by an act, and measured here all
  three of three concurrent runs passed it, 20 trials out of 20.

  If that run is dead the lock is STALE, and it is NOT broken automatically —
  breaking it for you would restore the race it exists to remove. Break it
  deliberately, having checked the pid above is gone:
    rm -rf $LOCK"
fi
# ---------------------------------------------------------------------------

for existing in "$STAGE" "$MOUNT" "$OUT" "$RECEIPTS"; do
  # `-e` FOLLOWS SYMLINKS. A dangling symlink at any of these four was
  # therefore INVISIBLE here and the run proceeded over it. `-e || -L` is the
  # complete predicate: true for every directory entry that exists, broken
  # symlinks included. A name that is taken is taken whatever it points at.
  { [ -e "$existing" ] || [ -L "$existing" ]; } || continue
  die "run-id '$RUN_ID' is already taken — $existing exists.

  Refusing rather than reusing it. Staging starts with 'rm -rf \$STAGE \$MOUNT',
  so continuing would destroy that run's captured spool, and \$OUT is NOT in
  that rm: Terminal-Bench treats an existing --output-path as a resume with no
  flag asked for, so it would skip the task as already complete and report the
  PREVIOUS run's result over a mount this run had just emptied.

  Pass a different PILOT_RUN_ID, or remove the directories deliberately:
    rm -rf $STAGE $MOUNT $OUT $RECEIPTS

  Note that '$RUN_ID' may not be the id you passed: compose_safe is
  many-to-one, so two different raw ids can land on the same run."
done

# The split guard sits HERE, after the occupancy loop, and not up beside the
# other TASK_ID gates where it reads more naturally. The reason is mechanical:
# scripts/pilot-run-guard-test.sh cuts its prologue at the first `^done$` — the
# line directly above — and runs that copy from a temp directory, where
# $REPO_ROOT resolves outside this repository and the split fixture does not
# exist. This guard REFUSES when it cannot read the split, by design, so placing
# it in the prologue would make every one of that script's cases refuse for the
# wrong reason. Everything expensive is still downstream: no credential is read,
# nothing is staged, and Docker is not touched until well below.
# >>> THRULINE-SPLIT-GUARD-BEGIN >>>
# These two sentinels are a MUTATION BOUNDARY, not decoration.
# `crates/bench/tests/split_enforcement_census.rs` copies this script, deletes
# everything between them, and asserts the copy STOPS refusing a live-holdout
# id. That is the census's negative control: it is how the census knows the
# refusal it just watched came from THIS block and not from somewhere else.
# Moving enforcement outside these markers turns that control red. Deleting the
# markers turns it red too, and says so by name.
# ---------------------------------------------------------------------------
# THE SPLIT GUARD — the refusal that did not exist on 2026-09-04.
#
# fixtures/terminal-bench-holdout-v1.tsv was committed at 03:10:42. At 03:37:20
# this script ran `ancient-puzzle` — holdout side, no observed row, live
# holdout — and produced a graded results.json. Nothing refused it, and the
# reason was not a weak check: THIS FILE CONTAINED ZERO OCCURRENCES OF THE WORD
# "holdout". The entire anti-contamination construction (pilot::plan,
# check_from_tune, TerminalBenchSplit, HoldoutLedger) is library code that
# nothing shipping called. A guard nothing calls is not a weak guard; it is a
# comment with a test suite.
#
# FAIL CLOSED, IN EVERY DIRECTION. The three refusals below — unreadable
# fixture, unknown id, live holdout without the flag — all exit non-zero. A
# split-enforcement guard that falls through to "allow" when it cannot read its
# own input is worse than no guard at all, because it reports as enforcement;
# this program has a documented instance of exactly that inversion (the coverage
# gate wired to nothing), and it is not repeating it here.
#
# WHAT COUNTS AS SPENDABLE. `holdout` in the side column is a HASH FACT
# (bucket(salt,id) < cut) and is not editable — crates/bench/src/pilot.rs
# refuses an edited assignment as SplitError::AssignmentDrift. Eligibility is
# `side == holdout AND no observed row`, which is exactly the set
# `TerminalBenchSplit::holdout` returns. An observed holdout-side task has
# already been seen, is already in `TerminalBenchSplit::tune()`, and running it
# again costs nothing that is not already gone.
# ---------------------------------------------------------------------------
SPLIT_FILE="${PILOT_SPLIT_FILE:-$REPO_ROOT/fixtures/terminal-bench-holdout-v1.tsv}"
# NOT under $PILOT/runs-* and not keyed by run-id: the manifest has to outlive
# every individual run for a second spend of the same id to be detectable at
# all. One file, append-only, beside the bench scratch root.
HOLDOUT_MANIFEST="${PILOT_HOLDOUT_MANIFEST:-$SP/holdout-spend.tsv}"

if [ ! -f "$SPLIT_FILE" ] || [ ! -r "$SPLIT_FILE" ]; then
  die "the committed split is missing or unreadable:

    $SPLIT_FILE

  REFUSING, and deliberately not falling through to 'allow'. Without this file
  there is no way to know whether '$TASK_ID' is holdout, and a run that cannot
  prove a task is spendable must not spend it. Restore the fixture, or point
  PILOT_SPLIT_FILE at it."
fi

SPLIT_ROWS="$(awk -F'\t' '$1=="task"{n++} END{print n+0}' "$SPLIT_FILE")"
if [ "$SPLIT_ROWS" -lt 1 ]; then
  die "the committed split at $SPLIT_FILE parses to $SPLIT_ROWS task rows.

  REFUSING: a split with no assignments in it cannot say that '$TASK_ID' is
  spendable, and treating an empty answer as 'not holdout' is the fail-open
  inversion this guard exists to prevent."
fi

SPLIT_SIDE="$(awk -F'\t' -v id="$TASK_ID" '$1=="task" && $2==id {print $3; exit}' "$SPLIT_FILE")"
if [ -z "$SPLIT_SIDE" ]; then
  die "'$TASK_ID' is a task in the dataset but is NOT in the committed split
  ($SPLIT_ROWS ids, $SPLIT_FILE).

  REFUSING: the corpus on this machine is not the corpus the split was cut
  from, so no side can be derived for this id, and 'no side' must not mean
  'tune'. A corpus bump is a re-measurement — re-cut the split."
fi

if [ "$SPLIT_SIDE" = "holdout" ]; then
  if awk -F'\t' -v id="$TASK_ID" '$1=="observed" && $2==id {f=1} END{exit !f}' "$SPLIT_FILE"; then
    # Holdout side, but already observed: it is in tune() and costs nothing.
    printf "pilot-run: '%s' is on the holdout side but carries an observed row — already spent, running it costs nothing further.\n" \
      "$TASK_ID" >&2
  elif [ "$HOLDOUT_RUN" != "1" ]; then
    die "REFUSED: '$TASK_ID' is on the HOLDOUT side of the committed split and
  has no observed row, so it is live holdout.

    split: $SPLIT_FILE
    task:  $TASK_ID
    side:  holdout (live — no observed row)

  A holdout is worth exactly one spend, and this script spent 'ancient-puzzle'
  by accident 27 minutes after the split was committed because no version of
  this file had ever read it.

  If you MEAN to spend it, say so and it will be recorded:
    scripts/pilot-run.sh --holdout-run $AGENT ${MODEL:-}
  (with PILOT_TASK_ID=$TASK_ID). The spend is appended to
    $HOLDOUT_MANIFEST
  before the run starts, and a second run of this id is refused from that file."
  else
    # ---- a DELIBERATE holdout spend: record it, then refuse a repeat --------
    mkdir -p "$(dirname "$HOLDOUT_MANIFEST")"
    if [ ! -f "$HOLDOUT_MANIFEST" ]; then
      {
        printf '# ThruLine holdout spend manifest — append-only, one row per deliberate spend.\n'
        printf '# Written by scripts/pilot-run.sh --holdout-run BEFORE the run starts. Written\n'
        printf '# early on purpose: a run that dies after this line leaves a spend recorded that\n'
        printf '# did not happen, which is the SAFE direction for a holdout. The other order\n'
        printf '# loses the record whenever the run crashes, and an unrecorded spend is\n'
        printf '# indistinguishable from no spend at all.\n'
        printf '# columns: run <task-id> <side> <run-id> <agent> <model> <utc> <split-sha256>\n'
      } > "$HOLDOUT_MANIFEST"
    fi
    if awk -F'\t' -v id="$TASK_ID" '$1=="run" && $2==id {f=1} END{exit !f}' "$HOLDOUT_MANIFEST"; then
      die "REFUSED: '$TASK_ID' has ALREADY been spent as a holdout.

  $(awk -F'\t' -v id="$TASK_ID" '$1=="run" && $2==id {printf "    %s\n", $0}' "$HOLDOUT_MANIFEST")

  Recorded in $HOLDOUT_MANIFEST. A holdout read a second time is a validation
  set wearing a holdout's name — the second measurement is not independent of
  the first, and --holdout-run does not make it so.

  To retire this task properly, add an 'observed' row for it to
    $SPLIT_FILE
  with the results.json that proves it was run; the re-derivation in
  crates/bench/src/pilot.rs will make the counts agree or refuse the edit."
    fi
    if command -v shasum >/dev/null 2>&1; then
      SPLIT_SHA="sha256:$(shasum -a 256 "$SPLIT_FILE" | awk '{print $1}')"
    elif command -v sha256sum >/dev/null 2>&1; then
      SPLIT_SHA="sha256:$(sha256sum "$SPLIT_FILE" | awk '{print $1}')"
    else
      SPLIT_SHA="unknown-no-sha256-tool"
    fi
    printf 'run\t%s\tholdout\t%s\t%s\t%s\t%s\t%s\n' \
      "$TASK_ID" "$RUN_ID" "$AGENT" "${MODEL:--}" \
      "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$SPLIT_SHA" >> "$HOLDOUT_MANIFEST"
    printf "pilot-run: DELIBERATE HOLDOUT SPEND of '%s' recorded in %s. This task can never serve as holdout again.\n" \
      "$TASK_ID" "$HOLDOUT_MANIFEST" >&2
  fi
fi
# <<< THRULINE-SPLIT-GUARD-END <<<

# ---------------------------------------------------------------------------
# 0. The credential gate. Fail CLOSED: `oracle` is the only agent that makes no
#    model calls, so every other agent name is treated as paying — including one
#    added later that nobody remembered to list here.
#
#    IT GATED THE WRONG DOOR. This demanded OPENAI_API_KEY for *every* non-oracle
#    agent and never looked at ANTHROPIC_API_KEY, so `pilot-run.sh claude-code
#    claude-haiku-4-5` passed the gate on an OpenAI key it would never use and
#    then ran unauthenticated — which is the exact failure the gate's own error
#    message describes: empty output that reads downstream as a coverage finding
#    rather than as a missing key.
#
#    The variable is now DERIVED FROM THE AGENT, and an agent this script has no
#    row for is refused rather than defaulted. An unknown agent silently
#    inheriting OpenAI's door is how the wrong-door bug survived.
# ---------------------------------------------------------------------------
PAYS=1
CRED_VAR=""
ENV_FILE=""
# Per-agent MODEL ALLOWLIST. The previous gate tested `[ -z "$MODEL" ]` — any
# non-empty string passed. `gpt-5.6-sol` passed it, and this repository's own
# price-table comment says Sol "is frontier, which the authorisation forbids
# outright"; so did every unpriced model, every typo, and every Claude id handed
# to Codex. An allowlist refuses all of those, and it is deliberately EXACTLY the
# set that `crates/bench/src/pilot.rs::PRICES` can price: a model this project
# cannot price is a model it must not spend on.
MODELS_OK=""
# ---------------------------------------------------------------------------
# AND THE HARNESS THE CONTAINER IS INSTRUMENTED FOR — RULING 3's PREREQUISITE.
#
# This was not a variable. `scripts/compose-instrument.py` hardcoded
# `--harness codex`, `/root/.codex/hooks.json`, `chmod +x /thruline/bin/codex`
# and a Codex-shaped `grep`, so `pilot-run.sh claude-code` installed a hook pack
# CLAUDE CODE NEVER READS (it reads `~/.claude/settings.json`). The spool would
# have carried execs and no SessionStart/Stop, `SessionBinding` would have
# fallen to `Unbound`, and "hooks folded" — the numerator of the chain-coverage
# gate — would have been ZERO BY CONSTRUCTION, with nothing in the run saying so.
#
# `oracle` keeps `codex` because no agent runs at all under it: the harness
# replays a recorded solution and opens no session, so which pack is installed
# changes nothing that is measured, and holding the value it has always had
# keeps every oracle receipt comparable.
HARNESS="codex"
# The agent name handed to `tb`. `claude-code-scripted` is this script's name
# for a configuration, not one of tb's agents.
TB_AGENT="$AGENT"
# 1 = a scripted run against a LOCAL FAKE endpoint. Spends nothing and must be
# incapable of spending; see the block that starts the endpoint.
SCRIPTED=0
case "$AGENT" in
  oracle)
    PAYS=0 ;;
  codex)
    CRED_VAR="OPENAI_API_KEY"
    ENV_FILE="$HOME/.thruline-bench/openai.env"
    MODELS_OK="gpt-5.6-luna" ;;
  claude-code)
    HARNESS="claude-code"
    CRED_VAR="ANTHROPIC_API_KEY"
    ENV_FILE="$HOME/.thruline-bench/anthropic.env"
    MODELS_OK="claude-haiku-4-5 claude-haiku-4-5-20251001" ;;
  claude-code-scripted)
    # A REAL Claude Code session — SessionStart, a real tool call that runs the
    # task's own test command, a real Stop — driven by a canned reply script from
    # a local HTTP server. It is the only configuration in which the
    # chain-coverage gate is MEASURABLE AT $0: the gate's numerator is hooks
    # folded, and Terminal-Bench runs `run-tests.sh` strictly AFTER the agent
    # returns, so `check`-binding can never produce a SessionStart or a Stop.
    #
    # PAYS=0 is not an exemption from the credential gate; it is a statement
    # that there is no credential. Two independent guards, either sufficient:
    #   * ANTHROPIC_BASE_URL points at 127.0.0.1 via the Docker gateway;
    #   * ANTHROPIC_API_KEY is the literal string `sk-ant-FAKE-…`, which
    #     authenticates against nothing.
    # A dead base URL does NOT fall back to the real API — measured: the run
    # hung and exited 137. A misconfigured port cannot spend by falling through.
    PAYS=0
    HARNESS="claude-code"
    TB_AGENT="claude-code"
    SCRIPTED=1
    [ -z "$MODEL" ] || die "agent 'claude-code-scripted' takes no model: the endpoint
  is a canned script that answers whatever model it is asked for, and pinning one
  here would put a real model id into a run that must not be able to reach one." ;;
  terminus)
    die "'terminus' is refused as a pilot harness. It is driven through litellm and
  fires no hooks at all, so measure (a) would be zero BY CONSTRUCTION and would
  read as a property of the corpus rather than of the harness chosen.
  See crates/bench/src/pilot.rs::PilotError::HarnessWithoutHooks." ;;
  *)
    die "unknown agent '$AGENT'. Refusing rather than guessing which credential it
  needs: this gate used to hand every unrecognised agent OpenAI's door, which is
  how a claude-code run passed a credential check on a key it never uses and then
  ran unauthenticated. Known agents: oracle, codex, claude-code,
  claude-code-scripted." ;;
esac

# The loopback port the scripted endpoint listens on. Only read when SCRIPTED=1.
RIG_PORT="${THRULINE_RIG_PORT:-8752}"

if [ "$PAYS" -eq 1 ]; then
  # Sourced, never read, never printed, never copied. Nothing below ever
  # expands the key into a word this script can display — the check is on
  # PRESENCE, matching crates/bench/src/pilot.rs::require_credential.
  if [ -f "$ENV_FILE" ]; then
    set +u
    # shellcheck disable=SC1090
    . "$ENV_FILE"
    set -u
  fi
  # Indirect expansion, so the value never appears in a word this script writes.
  if [ -z "${!CRED_VAR:-}" ]; then
    die "$CRED_VAR is not set.

  Agent '$AGENT' makes model calls through that credential and this script will
  not fall back to an unauthenticated path. An unauthenticated run does not fail
  loudly: it produces empty transcripts, which read downstream as a CAPTURE
  COVERAGE FINDING rather than as a missing key — a wrong number that looks like
  a real one.

  Export it, or put it in $ENV_FILE (mode 600).
  Only 'oracle' runs without a credential."
  fi
  if [ -z "$MODEL" ]; then
    die "agent '$AGENT' pays, so the model must be pinned: an unpinned model is an
  unpriced run, and the cost line would be an estimate wearing a measurement's
  clothes. Pass it as the second argument, e.g. '$AGENT ${MODELS_OK%% *}'."
  fi
  MODEL_OK=0
  for m in $MODELS_OK; do
    [ "$MODEL" = "$m" ] && MODEL_OK=1
  done
  if [ "$MODEL_OK" -ne 1 ]; then
    die "model '$MODEL' is not on the allowlist for agent '$AGENT'.

  Allowed: $MODELS_OK

  This is an allowlist and not a non-empty check on purpose. The previous gate
  accepted ANY non-empty string, including frontier models this project's own
  price table records as forbidden by the authorisation, and including every
  model it cannot price — an unpriced run cannot be charged against the ceiling,
  so it cannot be capped."
  fi
fi

# ---------------------------------------------------------------------------
# 0b. THE MONEY CEILING, and the host binary that enforces it.
#
# `Budget`, `BudgetStop`, `Ledger::charge` and `check_first_task` were reachable
# from exactly one place outside their own definitions — the test file.
# `grep -c pilot crates/cli/src/main.rs` returned 0. This is where that stops
# being true: `thruline pilot-budget` is checked BEFORE the harness runs and
# charged AFTER the report, against an append-only file that survives the
# process, because the runner is one `tb run` per invocation and ten tasks were
# ten amnesiac runs each starting from $0.
#
# The binary under $PILOT/bin is a LINUX build extracted from the task image and
# cannot run on a macOS host, so the ceiling needs a host build named separately.
# ---------------------------------------------------------------------------
SPEND_LEDGER="${THRULINE_PILOT_SPEND:-$PILOT/spend.tsv}"
TL_HOST="${THRULINE_HOST_BIN:-}"
if [ -z "$TL_HOST" ]; then
  for c in "$REPO_ROOT/target/release/thruline" "$REPO_ROOT/target/debug/thruline" \
           "$(command -v thruline 2>/dev/null || true)"; do
    if [ -n "$c" ] && [ -x "$c" ]; then TL_HOST="$c"; break; fi
  done
fi
if [ "$PAYS" -eq 1 ] && [ -z "$TL_HOST" ]; then
  die "no host 'thruline' binary, so the \$3 run ceiling cannot be enforced.

  Refusing to spend money with the guard switched off — a guard that is skipped
  because its binary is missing is the ninth instance of that shape in this
  program. Build one:

    cargo build -p thruline-cli --no-default-features

  or set THRULINE_HOST_BIN. The binary under \$PILOT/bin is a Linux build
  extracted from the task image and will not run on this host."
fi

# ---------------------------------------------------------------------------
# 0c. THE GUARD'S OWN IDENTITY. Resolving a binary is not the same as resolving
#     THE binary.
#
#     The fallback chain above takes the first EXECUTABLE FILE it finds at
#     target/release, then target/debug, then $PATH, and never asks whether that
#     file is the build under test — or whether it can enforce a ceiling at all.
#     Right now `target/debug/thruline` exists on this machine and does NOT have
#     the `pilot-budget` subcommand: clap exits 2. The script then reached the
#     `check` call, saw a non-zero status, and died saying
#
#         "the budget guard refused this run before it started (exit 2)"
#
#     — a refusal that never happened. That failure is closed, so this is debt
#     rather than loss; but a STALE binary that DOES have `pilot-budget`, with
#     older caps compiled into it, would enforce the wrong ceiling and say
#     nothing at all. The subcommand's presence is asserted here, before any
#     money is at stake, and the version is printed into the run's own output so
#     the report says which binary held the ceiling.
if [ -n "${THRULINE_HOST_BIN:-}" ] && [ ! -x "${THRULINE_HOST_BIN:-}" ]; then
  die "THRULINE_HOST_BIN is set to '$THRULINE_HOST_BIN', which is not an
  executable file. Refusing rather than falling back to the search path: an
  explicitly named guard binary that silently becomes a different one is the
  whole hazard this check exists for."
fi
if [ "$PAYS" -eq 1 ]; then
  if ! "$TL_HOST" pilot-budget --help >/dev/null 2>&1; then
    die "host binary has no pilot-budget subcommand:

    $TL_HOST
    $("$TL_HOST" --version 2>&1 | head -1)

  This binary CANNOT enforce the run ceiling. Without this check the script ran
  on and died at the budget call itself, reporting 'the budget guard refused
  this run before it started' — a refusal that never happened, from a guard that
  was never there. Build the binary under test and point at it:

    cargo build -p thruline-cli --no-default-features
    THRULINE_HOST_BIN=\$PWD/target/debug/thruline scripts/pilot-run.sh …"
  fi
  TL_HOST_VERSION="$("$TL_HOST" --version 2>&1 | head -1)"
else
  TL_HOST_VERSION="$([ -n "$TL_HOST" ] && "$TL_HOST" --version 2>&1 | head -1 || echo 'n/a — no ceiling to enforce')"
fi

# ---------------------------------------------------------------------------
# 1. Preflight. Every one of these produced a silent wrong answer at least once.
# ---------------------------------------------------------------------------
say "preflight"
[ -x "$TB" ]        || die "tb not found at $TB (expected the pinned-archive venv)"
[ -x "$PILOT_PY" ]  || die "no python at $PILOT_PY.
  The compose instrumenter runs under the pinned archive's venv, not under a
  bare 'python3': it requires ruamel.yaml and refuses to fall back to PyYAML,
  which strips comments and accepts duplicate keys."
"$PILOT_PY" -c 'import ruamel.yaml, yaml' 2>/dev/null || die "$PILOT_PY cannot
  import both ruamel.yaml and PyYAML. scripts/compose-instrument.py needs the
  first to round-trip and the second as an INDEPENDENT verifier of its own
  output; with one of them missing it exits 4 rather than instrumenting."
[ -f "$REPO_ROOT/scripts/compose-instrument.py" ] || die "scripts/compose-instrument.py
  is missing — the compose file cannot be instrumented, and a run with an
  uninstrumented compose file captures nothing."
[ -d "$TASKS_SRC" ] || die "task source not found at $TASKS_SRC"
[ -d "$TASKS_SRC/$TASK_ID" ] || die "task '$TASK_ID' not in $TASKS_SRC"
command -v docker >/dev/null || die "docker not on PATH"
docker info >/dev/null 2>&1  || die "docker is not running"
printf 'tb        : %s\n' "$TB"
printf 'tasks     : %s\n' "$TASKS_SRC"
printf 'task      : %s\n' "$TASK_ID"
printf 'compose   : %s\n' "$PROJECT_NAME"
printf 'agent     : %s%s\n' "$AGENT" "$([ -n "$MODEL" ] && printf ' (model %s)' "$MODEL")"
printf 'pays      : %s\n' "$([ "$PAYS" -eq 1 ] && echo 'YES — real money' || echo 'no — zero model calls')"
printf 'guard     : %s\n' "${TL_HOST:-none resolved}"
printf 'guard ver : %s\n' "$TL_HOST_VERSION"

# ---------------------------------------------------------------------------
# 2. The runner image: multi-stage, glibc-compatible builder.
#
#    `rust:1.97` is glibc 2.41; the Terminal-Bench bases are 2.36. glibc is
#    backward but NOT forward compatible, so a binary built on the default tag
#    dies on the task image with `GLIBC_2.39 not found`. Measured, not guessed.
#    `--no-default-features` drops ONNX, which has no verdict-path use (§0.1).
# ---------------------------------------------------------------------------
say "runner image"
if docker image inspect "$IMAGE" >/dev/null 2>&1; then
  echo "$IMAGE present — reusing"
  # S6, NOT FIXED HERE, said out loud rather than left to be discovered.
  #
  # $IMAGE is a hand-typed tag and this branch never rebuilds, so once the tag
  # exists NO change under crates/ reaches any pilot run until somebody edits
  # the literal at the top of this file. The binary is extracted from this image
  # a few lines below and bind-mounted over the container's own, so a stale
  # image silently means a stale engine — and the run reports whatever that
  # older engine measures, with nothing anywhere saying which build it was.
  # A digest check was considered and dropped: it costs a full rebuild to
  # answer, and disk on the pilot machine is the binding constraint.
  printf '  NOTE: this image is NEVER rebuilt while the tag exists. If crates/ has\n'
  printf '        changed since it was built, this run measures the OLD engine.\n'
  printf '        Bump IMAGE at the top of this script, or: docker image rm %s\n' "$IMAGE"
  printf '        built: %s\n' "$(docker image inspect -f '{{.Created}}' "$IMAGE" 2>/dev/null || echo unknown)"
else
  # TRAP, not a trailing rm. The `rm -rf "$BUILD_CTX"` below sits after
  # `docker build`, and under `set -e` a failed build skips it — so the runs
  # that leak are the runs that fail, which is the shape this repository spent a
  # night removing from its tests. An EXIT trap covers the failure paths.
  # STATED LIMIT: it does not cover SIGKILL. No in-process owner does, and an
  # Aug-31 root on this machine is the proof; that residue is the core census's.
  BUILD_CTX="$(mktemp -d)"
  trap 'rm -rf "$BUILD_CTX"' EXIT
  cat > "$BUILD_CTX/Dockerfile" <<'DOCKER'
FROM rust:1.97-bookworm AS thruline-builder
WORKDIR /src
COPY . /src
RUN cargo build --release --locked --no-default-features -p thruline-cli

FROM ghcr.io/laude-institute/t-bench/python-3-13:20250620
RUN apt-get update \
 && apt-get install -y --no-install-recommends git ca-certificates \
 && rm -rf /var/lib/apt/lists/*
COPY --from=thruline-builder /src/target/release/thruline /usr/local/bin/thruline
RUN thruline --version && git --version
DOCKER
  cat > "$REPO_ROOT/.dockerignore" <<'IGN'
target*/
fixtures/bench/*/
.git/
ui/node_modules/
IGN
  docker build -f "$BUILD_CTX/Dockerfile" -t "$IMAGE" "$REPO_ROOT"
  rm -f "$REPO_ROOT/.dockerignore"
  rm -rf "$BUILD_CTX"; trap - EXIT
fi

BIN="$PILOT/bin/thruline"
mkdir -p "$PILOT/bin"
# EXTRACTED EVERY RUN, never cached. Compose mounts this file OVER the image's
# own `/usr/local/bin/thruline`, so a stale copy silently replaces the binary
# the image was just built with — and `if [ ! -x "$BIN" ]` pinned it to
# whenever it was first extracted, for the life of the directory.
#
# Caught in the act: a run whose image had `uv` in `shim::WRAPPED` mounted a
# binary from an earlier build that did not, so the container installed 11
# wrappers instead of 15. The `test -x /thruline/shim/uv` assertion killed the
# container, which is why this was a refusal rather than a quiet 3.7%
# measurement — but the mismatch should never have been possible. A `docker cp`
# costs about a second; a binary that disagrees with the image it came from
# costs a measurement.
CID="$(docker create "$IMAGE")"
docker cp "$CID:/usr/local/bin/thruline" "$BIN" >/dev/null
docker rm -f "$CID" >/dev/null


# ---------------------------------------------------------------------------
# 3. Stage the task. COPIED from the pinned archive, never edited in place, so
#    harness and tasks stay on commit d28711d0 and the corpus is not mutated.
# ---------------------------------------------------------------------------
say "stage task"
# NEVER give either of these a trailing slash. Measured on this machine
# (macOS 26.1, /bin/rm): with a symlink `link -> real/`,
#
#   rm -rf link    removes the link.       real/ and its contents survive.
#   rm -rf link/   removes real/ AND ITS CONTENTS, and leaves `link` dangling.
#
# $MOUNT holds the captured spool, and the occupancy guard above now sees a
# dangling symlink (`-e || -L`) instead of stepping over it — but if one is ever
# created between that check and this line, the slash is the difference between
# deleting a symlink here and deleting whatever it points at, anywhere on the
# disk. There is no reason to add one and one way for it to be catastrophic.
rm -rf "$STAGE" "$MOUNT" "$RECEIPTS"
# The transcript directory is the harness's, not Codex's. Compose mounts
# `$MOUNT/<dir>` over the path that harness writes its session JSONL to, and a
# bind mount whose host side does not exist is created by Docker as ROOT-OWNED —
# which is fine here and is why it is made explicitly rather than left to chance.
case "$HARNESS" in
  codex)       TRANSCRIPT_DIR="codex-sessions" ; INGEST_SOURCE="codex" ;;
  claude-code) TRANSCRIPT_DIR="claude-projects"; INGEST_SOURCE="claude-code" ;;
  *) die "no transcript directory known for harness '$HARNESS'" ;;
esac
SPOOL_FILE="$MOUNT/spool/$HARNESS.jsonl"
mkdir -p "$STAGE" "$MOUNT/spool" "$MOUNT/home" "$MOUNT/$TRANSCRIPT_DIR" "$MOUNT/bin"

# ---------------------------------------------------------------------------
# F2 — STATIC MUSL BINARIES, ONE PER ARCHITECTURE, CONSUMED NOT BUILT.
#
# `$BIN` above is aarch64 and DYNAMICALLY linked against glibc 2.36 (verified:
# `ldd` inside $IMAGE resolves libc.so.6 from /lib/aarch64-linux-gnu). It is
# mounted read-only over /usr/local/bin/thruline in EVERY task container,
# whatever that container is, and five m1 instrument failures are that mismatch:
#
#   * one is the LIBC half  — `qemu-alpine-ssh` runs Alpine, which has no glibc
#     at all, so no glibc build can work there however new it is;
#   * four are the ARCH half — their Dockerfiles pin `--platform=linux/amd64`,
#     so Docker runs them as amd64 under emulation on this arm64 host and an
#     aarch64 binary cannot execute inside them however it is linked. A fifth,
#     `broken-networking` (`--platform=linux/amd64 ubuntu:22.04`), is masked
#     behind an earlier apt failure and surfaces the moment that one is fixed.
#
# TERMINAL-BENCH REPORTED ALL OF IT AS "asciinema is not installed in the
# container", which is the lesson worth carrying: the harness's own failure_mode
# string named the wrong subsystem, and a reader trusting it would have gone
# looking for a missing recording tool. So the check below is `--version` — does
# this binary EXECUTE in THIS image — and the refusal names the architecture.
#
# THE BINARIES ARE NOT BUILT HERE. Track C produces them under a verification
# gate (`file` says statically linked, `readelf` shows no PT_INTERP, and the
# artefact is run in debian:bookworm-slim and in the t-bench base image before
# it is emitted). Point THRULINE_MUSL_DIR at that output directory. Independently
# re-verified here on the aarch64 artefact rather than taken on report:
#
#   sha256 f175469ffb76ab07eab8be6fdb95b690338e35a894b13a3ae361c1d056101091
#   21200336 bytes, BuildID sha1 650b8a801419ec300f0f17b85cf051c6dbe52c43
#   `readelf -l` INTERP segments: 0
#   runs in ghcr.io/laude-institute/t-bench/python-3-13:20250620 -> thruline 0.1.0
#
# UNSET IS THE OLD BEHAVIOUR, SAID OUT LOUD. With no THRULINE_MUSL_DIR the run
# mounts the glibc binary exactly as before and prints that it did, so a sweep
# that silently reverted to the failing configuration is not possible.
MUSL_STAGED=""
mkdir -p "$MOUNT/bin/arch"
# THE DEFAULT IS THIS LANE'S COPY, NOT THE PRODUCER'S BUILD DIRECTORY.
#
# This read Track C's `~/thruline-gate/target/musl` live. That is inside another
# track's gitignored build directory, and `cargo clean` — the obvious reading of
# "reap your build directory" — deletes it. C reaped that tree and stopped one
# path short of these two binaries. DECISIONS #62: an artefact one track
# consumes must not live where another track's housekeeping reaches; the
# CONSUMER copies it into its own lane with the producer's provenance beside it.
#
# fixtures/bench/musl/ holds the copies (43 MB, gitignored) and
# fixtures/bench/musl-artefacts.tsv is the committed pin — producer commit,
# sha256, size and BuildID per arch. Override THRULINE_MUSL_DIR to point
# elsewhere; the digest check below still applies, which is the point of pinning
# rather than merely copying.
MUSL_MANIFEST="$REPO_ROOT/fixtures/bench/musl-artefacts.tsv"
: "${THRULINE_MUSL_DIR:=$REPO_ROOT/fixtures/bench/musl}"
if [ -n "${THRULINE_MUSL_DIR:-}" ] && [ -d "$THRULINE_MUSL_DIR" ]; then
  for f in "$THRULINE_MUSL_DIR"/thruline-*-unknown-linux-musl; do
    [ -f "$f" ] || continue
    # The triple's FIRST field is the architecture, and it is taken from the
    # filename rather than from `file` output: `file`'s wording is not a
    # contract (musl's loader says "Not a valid dynamic program" where glibc
    # says "not a dynamic executable", which is how a static-binary guard
    # elsewhere rejected the first genuinely static binary it was ever shown).
    base="$(basename "$f")"; arch="${base#thruline-}"; arch="${arch%%-*}"
    # THE PROPERTY, NOT THE PHRASING: no PT_INTERP segment and no DT_NEEDED
    # entry. That is what "static" means to the kernel and the loader, and it is
    # the only formulation that is true of BOTH targets — which is not a
    # hypothetical:
    #
    #   aarch64-unknown-linux-musl -> ELF type EXEC, `file` says
    #                                 "statically linked"
    #   x86_64-unknown-linux-musl  -> ELF type DYN (static-PIE), `file` says
    #                                 "static-pie linked"
    #
    # This check was `file | grep -q 'statically linked'` when it was committed,
    # and it would have REFUSED the x86_64 binary outright — the one five tasks
    # need — while readelf sat there telling it the truth. Worse, `ldd` on that
    # same static-PIE prints `/lib/ld-musl-x86_64.so.1 (0x...)`, which reads
    # exactly like a shared-library dependency and is not one: a check built on
    # THAT line fails in the UNSAFE direction, reporting a dependency that does
    # not exist on a binary about to be mounted into 123 containers.
    #
    # Run in a container because macOS has no readelf. Two invocations per pilot
    # run, about a second each, and the alternative is trusting a string.
    if ! docker run --rm -v "$THRULINE_MUSL_DIR:/b:ro" "$IMAGE" sh -c "
        i=\$(readelf -l /b/$base 2>/dev/null | grep -c INTERP)
        n=\$(readelf -d /b/$base 2>/dev/null | grep -c NEEDED)
        [ \"\$i\" = 0 ] && [ \"\$n\" = 0 ]" >/dev/null 2>&1; then
      die "$f is not statically linked: it carries a PT_INTERP segment, a
  DT_NEEDED entry, or both. A binary with either is resolved by the container's
  own loader against the container's own libraries, which is the failure this
  whole block exists to stop. Checked as a property; do not soften this to a
  \`file\` string match — both spellings of static are real and one of them was
  already rejected here by mistake."
    fi
    # THE DIGEST, against the committed manifest. A copy in this lane is only
    # worth more than a live read if it is PINNED — otherwise it is the same
    # unverified artefact one directory along. A missing manifest row is a
    # refusal too: an arch nobody recorded is an arch nobody checked.
    if [ -f "$MUSL_MANIFEST" ]; then
      want="$(awk -v a="$arch" -F'\t' '$1==a{print $2}' "$MUSL_MANIFEST" | head -1)"
      got="$(shasum -a 256 "$f" | cut -d' ' -f1)"
      if [ -z "$want" ]; then
        die "$f is a $arch binary and $MUSL_MANIFEST records no $arch row.
  Refusing to stage an artefact nothing pins. Add the row — arch, sha256, bytes,
  build id — or point THRULINE_MUSL_DIR somewhere the manifest describes."
      fi
      if [ "$want" != "$got" ]; then
        die "$f does not match the pin in $MUSL_MANIFEST.
    want $want
    got  $got
  The binary changed, or it is not the one whose provenance this repository
  records. Re-verify it against its producer and update the manifest
  deliberately; do not stage past this."
      fi
    fi
    cp "$f" "$MOUNT/bin/arch/$arch"
    chmod +x "$MOUNT/bin/arch/$arch"
    MUSL_STAGED="$MUSL_STAGED $arch"
    printf 'musl      : staged %-8s %s (%s bytes)\n' "$arch" \
      "$(shasum -a 256 "$f" | cut -c1-16)" "$(stat -f %z "$f" 2>/dev/null || stat -c %s "$f")"
  done
fi
if [ -n "$MUSL_STAGED" ]; then
  printf '%s\n' "${MUSL_STAGED# }" > "$MOUNT/bin/arch/STAGED"
  printf 'musl      : the container will select by `uname -m` and REFUSE if its\n'
  printf '            architecture is not among:%s\n' "$MUSL_STAGED"
else
  printf 'musl      : none staged (%s is empty or absent) — this run\n' "$THRULINE_MUSL_DIR"
  printf '            mounts the aarch64 glibc binary, which is the configuration\n'
  printf '            that produced five instrument failures. Expect them.\n'
fi
cp -R "$TASKS_SRC/$TASK_ID" "$STAGE/$TASK_ID"
printf 'copied from %s\n' "$TASKS_SRC/$TASK_ID" > "$MOUNT/task-provenance.txt"

# ---------------------------------------------------------------------------
# The codex trust wrapper.
#
# Codex records hook trust against the hook definition's HASH, so the pack the
# container writes is new by construction and every hook in it is marked for
# review and SKIPPED. That is a SECOND, INDEPENDENT cause of the symptom the
# /etc/profile.d fix addresses — hooks installed, nothing fires, spool empty —
# and fixing either alone still yields zero hook events.
#
# tb's codex adapter hardcodes `codex exec --sandbox … --model … -- <task>`
# with no injection point, and the pinned archive must not be edited. So the
# flag is added by a wrapper placed FIRST on PATH, which the profile.d drop-in
# already guarantees for the login shell tb drives.
#
# Written here on the host rather than generated inside a YAML-embedded shell
# string, because that nesting is its own class of bug.
# Its OWN directory, never /thruline/shim. `thruline shim install` REFUSES to
# install into a directory holding files it did not write — "already holds 1
# file(s) ThruLine did not write (codex); refusing to install into it" — which
# is the right behaviour and which a shared directory would have fought. The
# wrapper goes earlier on PATH than the shim, and the two never mix.
#
# CODEX ONLY. Claude Code has no equivalent gate under Terminal-Bench's adapter
# — measured twice, independently, at $0: its hooks fire with no trust prompt,
# because the check is skipped in print mode, which is the mode the adapter
# drives (`claude -p …`). Writing a wrapper for it would be a `chmod +x` on a
# file no harness reads and an assertion that could never fail.
if [ "$HARNESS" = "codex" ]; then
mkdir -p "$MOUNT/bin"
cat > "$MOUNT/bin/codex" <<'WRAP'
#!/bin/sh
# Adds --dangerously-bypass-hook-trust and hands the rest of argv through
# untouched.
#
# BOTH OF THIS WRAPPER'S RESIDUAL ASSUMPTIONS WERE WRONG, and
# scripts/vendor-freshness.py found them (facts F7 and F8):
#
#  1. It searched THREE HARDCODED GLOBS — ~/.nvm/…, /usr/local/bin, /usr/bin —
#     which tracked one nvm-based install. Codex's own standalone installer puts
#     the launcher in ~/.local/bin, which none of them cover: on the machine
#     this pilot was written on, the wrapper would have exited 127. It now asks
#     $PATH (skipping its own directory so it cannot find itself), and keeps the
#     globs only as a fallback for a PATH that has been stripped.
#  2. `sub="$1"` assumed SUBCOMMAND-FIRST argv and re-inserted the flag after
#     it. Codex's own grammar is `codex [OPTIONS] <COMMAND> [ARGS]` and
#     `codex --dangerously-bypass-hook-trust exec …` parses — the flag is
#     GLOBAL. The reordering bought nothing and corrupted any argv whose first
#     word was an option (it would have landed between an option and its value).
#     argv is now passed through in order.
set -eu
REAL=""
me_dir=$(CDPATH= cd -- "$(dirname -- "$0")" 2>/dev/null && pwd) || me_dir=""
# `if` rather than `[ … ] && continue`: under `set -e` the status of an AND-OR
# list whose left side fails is 1, and the shells disagree about how much of
# that is exempt. This is the same class of bug as the `${a[@]+…}` note in
# hook-event-rig.sh, and it would fail on the ARM that must skip.
IFS=:
for d in $PATH; do
  [ -n "$d" ] || d=.
  if [ "$d" != "$me_dir" ] && [ -x "$d/codex" ]; then REAL="$d/codex"; break; fi
done
unset IFS
if [ -z "$REAL" ]; then
  for c in "$HOME"/.local/bin/codex "$HOME"/.nvm/versions/node/*/bin/codex \
           /usr/local/bin/codex /usr/bin/codex; do
    if [ "$c" != "$0" ] && [ -x "$c" ]; then REAL="$c"; break; fi
  done
fi
[ -n "$REAL" ] || { echo "thruline: real codex not found on this container" >&2; exit 127; }
exec "$REAL" --dangerously-bypass-hook-trust "$@"
WRAP
chmod +x "$MOUNT/bin/codex"
fi

# ---------------------------------------------------------------------------
# THE IMAGE LAYER, INSERTED BEFORE THE FINAL STAGE'S `USER` — not appended.
#
# This block used to be `cat >>` onto the end of the Dockerfile, and that is a
# measured breakage, not a style point. `shell-deobfuscation` ends its target
# stage with `USER agent`, so an appended `RUN … apt-get install git` layer runs
# AS AGENT and **the image never builds** — a task the oracle solves, scored as
# a non-pass by the instrument measuring it, contributing an empty spool to a
# coverage average whose denominator still counted it.
#
# The insertion point is the FIRST `USER` after the LAST `FROM`, so a multi-stage
# Dockerfile that sets a user in an earlier builder stage (as
# `shell-deobfuscation` nearly does) cannot pull the layer into the wrong stage.
# With no such line the block is appended, with an explicit newline first —
# `privilege-escalation`'s Dockerfile has no trailing newline and `cat >>` would
# have concatenated onto `WORKDIR /home/alice`.
#
# WHAT THE BLOCK INSTALLS
#
#   git — absent from both dominant Terminal-Bench base images (measured: 0 of
#   241 tasks carry a .git; git missing from 185/241, apt-get present). §7 binds
#   a check to a TREE OBJECT, so without this every exec record comes back with
#   input_tree_hash: null and there is nothing for a check to bind to.
#
#   uv, AT BUILD TIME, under the image's own PATH. 221 of the 241 run-tests.sh
#   install uv themselves and 219 then `source $HOME/.local/bin/env`. Under the
#   instrumented PATH that source FAILS, and the reason is not the one that
#   reads naturally. Measured, paired, in one container on the t-bench
#   python-3-13 base with HOME=/root:
#
#     control PATH, no shim:  installs -> `env env.fish uv uvx`;  source ok
#     instrumented PATH:      installs -> `uv uvx` ONLY;          source FAILS
#                             ("cannot open /root/.local/bin/env")
#     instrumented, but uv
#     provisioned at build:   env survives the re-install;        source ok
#
#   The compose PATH ends in `/root/.local/bin` so that uv's env script, whose
#   whole body is *prepend `$HOME/.local/bin` unless present*, prepends nothing
#   and the shim stays first. That tail is load-bearing — scripts/classify-
#   corpus.py derives 25/241 without it and 241/241 with it, BOTH UNDER A
#   `WrapPolicy::All` MODEL THAT DOES NOT SHIP (the installer uses
#   `Resolvable`), so both are upper bounds and the *delta* is what this comment
#   is actually claiming. The tail's load-bearingness rests on the delta, which
#   survives the model change; the absolute figures do not, and are withdrawn as
#   reach — see BUILD-SPEC §1. The fix is NOT
#   to reorder PATH. It is to install uv while nothing is instrumented yet, so
#   the env script is on disk before any run-tests.sh needs it. The build-time
#   install also makes `uv` RESOLVE at shim-install time, which is what keeps the
#   shim's amended shadowing predicate writing a `uv` wrapper at all.
#
#   This adds a NETWORK FETCH to every task image build. If astral.sh is
#   unreachable the build fails loudly, which is correct: a silently
#   unprovisioned image is the 219-task failure above, wearing a green build.
# ---------------------------------------------------------------------------
UV_VERSION="0.8.14"
DF="$STAGE/$TASK_ID/Dockerfile"
BLOCK="$STAGE/.thruline-layer"
cat > "$BLOCK" <<DOCKER

# ---- ThruLine pilot instrumentation (inserted by scripts/pilot-run.sh) ----
RUN if ! command -v git >/dev/null 2>&1; then \\
      apt-get update && apt-get install -y --no-install-recommends git \\
      && rm -rf /var/lib/apt/lists/*; \\
    fi
# HOME is EXPORTED, not just read into a local: uv's installer reads \$HOME
# itself to choose its install directory, and BuildKit does not guarantee HOME
# is set inside a RUN. A first draft computed H="\${HOME:-/root}" and then piped
# the installer, which would have read an unset HOME — the assertions below
# would have caught it, at the cost of every image build.
RUN set -eu; \\
    export HOME="\${HOME:-/root}"; \\
    if [ ! -x "\$HOME/.local/bin/uv" ] || [ ! -f "\$HOME/.local/bin/env" ]; then \\
      if ! command -v curl >/dev/null 2>&1; then \\
        apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \\
        && rm -rf /var/lib/apt/lists/*; \\
      fi; \\
      curl -LsSf https://astral.sh/uv/$UV_VERSION/install.sh | sh; \\
    fi; \\
    test -x "\$HOME/.local/bin/uv"; \\
    test -f "\$HOME/.local/bin/env"
# ---- end ThruLine pilot instrumentation ----
DOCKER

# The first USER after the last FROM, or 0 for "there is none".
INSERT_AT="$(awk '
  /^[[:space:]]*[Ff][Rr][Oo][Mm][[:space:]]/ { n = 0 }
  /^[[:space:]]*[Uu][Ss][Ee][Rr][[:space:]]/ { if (n == 0) n = NR }
  END { print n + 0 }' "$DF")"
# The user that stage ends as — the fact the compose patch needs.
#
# THE LAST `USER` OF THE FINAL STAGE, NOT THE FIRST. This read `if (u == "")
# u = line`, which is `INSERT_AT`'s rule (first USER after the last FROM) copied
# into a computation that needs the opposite. It is the same word for two
# different questions: WHERE the instrumentation layer goes — before the first
# USER, while the stage is still root — and WHAT THE STAGE ENDS AS, which only
# the last one answers.
#
# Re-derived over all 241 pinned tasks: exactly one Dockerfile changes answer,
# `postgres-csv-clean` (`USER postgres` L50, `USER root` L57). The first-USER
# rule called it `postgres`, so it was handed a `--service-user root` override
# it does not need — making the AGENT root, which is the price the comment
# below says is paid only where the image would otherwise not start. Four tasks
# fired the override where three should, and a perfectly scorable task was
# marked instrument-affected and pulled from the coverage denominator. The
# comment forty lines down says "the other 238 tasks keep a byte-identical
# semantic delta"; under the old rule it was 237, and the comment was the half
# that was right.
#
# It was also wrong in the dangerous direction in principle: `USER root` then
# `USER app` reads as root, applies no override, and the container dies in
# setup — an INSTRUMENT failure presenting as an empty spool, which is the one
# shape this pilot must never produce. No task in the pinned corpus has that
# shape; that is luck, not a property, and the rule below does not depend on it.
FINAL_USER="$(awk '
  /^[[:space:]]*[Ff][Rr][Oo][Mm][[:space:]]/ { u = "" }
  /^[[:space:]]*[Uu][Ss][Ee][Rr][[:space:]]/ {
    line = $0
    sub(/^[[:space:]]*[Uu][Ss][Ee][Rr][[:space:]]+/, "", line)
    sub(/[[:space:]]+$/, "", line)
    u = line
  }
  END { print u }' "$DF")"

if [ "$INSERT_AT" -gt 0 ]; then
  { head -n "$((INSERT_AT - 1))" "$DF"; cat "$BLOCK"; tail -n +"$INSERT_AT" "$DF"; } \
    > "$DF.thruline" && mv "$DF.thruline" "$DF"
  printf 'dockerfile: instrumentation inserted before line %s (`USER %s`)\n' \
    "$INSERT_AT" "$FINAL_USER"
else
  { cat "$DF"; printf '\n'; cat "$BLOCK"; } > "$DF.thruline" && mv "$DF.thruline" "$DF"
  printf 'dockerfile: instrumentation appended (no USER in the final stage)\n'
fi
rm -f "$BLOCK"

# A self-check on the transform, not on the corpus: after this, no `USER` may
# precede our layer inside the final stage. A silent regression here is a build
# failure forty lines into a Docker log, on a task the oracle solves.
if ! awk '
  /^[[:space:]]*[Ff][Rr][Oo][Mm][[:space:]]/ { seen_user = 0 }
  /^[[:space:]]*[Uu][Ss][Ee][Rr][[:space:]]/ { seen_user = 1 }
  /ThruLine pilot instrumentation \(inserted/ { if (seen_user) bad = 1 }
  # A FLAG, AND THE EXIT IN `END`. Written first as `exit 1` inside the rule —
  # which runs END, whose `exit 0` overrode it, so the guard passed on the exact
  # input it exists to refuse. Found by watching it fail to fail.
  END { exit (bad ? 1 : 0) }' "$DF"; then
  die "the instrumentation layer landed AFTER a USER directive in $DF.
  Its apt-get and curl steps need root; as a non-root user the image does not
  build at all, and the task reports an empty spool rather than a refusal."
fi

# ---------------------------------------------------------------------------
# THE COMPOSE `user:` OVERRIDE — only where the image ends non-root.
#
# `privilege-escalation` builds (git is already in its base) and then dies: the
# setup chain's root-only `printf > /etc/profile.d/…` fails as alice, the
# container never reaches `sleep infinity`, and docker reports 409. Ruling 3's
# fix is `user: root` on the patched service.
#
# IT IS NOT FREE AND IT IS NOT APPLIED BLINDLY. terminal_bench/terminal/
# terminal.py:73 takes the AGENT's exec user from the container's configured
# user, so `user: root` makes the agent root as well — and on
# `privilege-escalation` and `unprivileged-headless-pyrender` that is the
# variable under test. Those runs measure INSTRUMENT REACH (ruling 2) and are
# not scored passes. Applied only where the image would otherwise not start, so
# the other 238 tasks keep a byte-identical semantic delta.
# ---------------------------------------------------------------------------
SERVICE_USER_ARGS=()
case "$FINAL_USER" in
  ""|root|0|"root:root") ;;
  *)
    SERVICE_USER_ARGS=(--service-user root)
    printf 'compose   : final stage runs as `%s`; patching the service to `user: root`\n' "$FINAL_USER"
    printf '            NOTE: this makes the AGENT root too (tb reads the container user).\n'
    printf '            Instrument reach only — not a scored pass for this task.\n' ;;
esac

# ---------------------------------------------------------------------------
# THE SCRIPTED AGENT'S SCRIPT.
#
# The plan is TWO turns: one real Bash tool call running THE TASK'S OWN TEST
# COMMAND, then a text reply that ends the turn. That shape is chosen, not
# convenient — it is the minimum that makes every part of the chain real:
#
#   SessionStart      fires before the first model request
#   PreToolUse        the agent decided to run a command
#   the exec itself   a real process, through the shim, with a tree to bind to
#   PostToolUse       the outcome
#   Stop / SessionEnd the phase boundary the gate needs
#
# `run-tests.sh` is copied into the CAPTURE MOUNT, never into the task tree: the
# task tree is `git add -A`ed into the baseline the agent can see, and three
# tasks `COPY . /app` into their image. The instrument must not appear in the
# tree it measures.
#
# NOTE, and it is a real limitation rather than a rough edge: during the agent
# phase `$TEST_DIR` (/tests) is EMPTY — Terminal-Bench copies the tests in
# strictly after the agent returns. So the task's test command usually runs its
# installs and then finds no test file. That is a real exec with a real exit
# status and it is what the chain records; it is NOT a scored pass and must
# never be read as one.
if [ "$SCRIPTED" -eq 1 ]; then
  [ -f "$STAGE/$TASK_ID/run-tests.sh" ] || die "task '$TASK_ID' has no run-tests.sh,
  so there is no task test command for the scripted agent to run. Refusing rather
  than substituting one: a scripted run whose tool call is invented measures the
  rig, not the task."
  cp "$STAGE/$TASK_ID/run-tests.sh" "$MOUNT/task-run-tests.sh"
  cat > "$MOUNT/rig-plan.json" <<'J'
[["tool", {"name": "Bash", "input": {"command": "bash /thruline/task-run-tests.sh"}}],
 ["text", "ran the task's own test command; stopping"]]
J
fi

# ---------------------------------------------------------------------------
# INSTRUMENT THE COMPOSE FILE — structurally, in scripts/compose-instrument.py.
#
# This used to be a 116-line `python3 - <<'PY'` heredoc containing three
# WHOLE-FILE SUBSTRING assertions, leading whitespace included:
#
#   "      - ${T_BENCH_TASK_AGENT_LOGS_PATH}:${T_BENCH_CONTAINER_AGENT_LOGS_PATH}"
#   '    environment:\n      - TEST_DIR=${T_BENCH_TEST_DIR}'
#   '    command: [ "sh", "-c", "sleep infinity" ]'
#
# A task that wrote `["sh","-c","sleep infinity"]` without the inner spaces was
# refused. A task that mounts no agent-logs directory was refused. Re-measured
# here, not remembered: over the pinned 241-task corpus the string form
# instruments 218 and refuses 23; the structural patcher instruments 239 and
# refuses 2. `scripts/compose-instrument-test.py --corpus "$TASKS_SRC"`
# re-derives both numbers, and asserts the shape of every refusal.
#
# THE TWO REFUSALS ARE CORRECT AND ARE NOT DEBT. `install-windows-xp` and
# `slurm-simple-node-monitoring` declare no `command:` at all, so the container's
# behaviour is the image's own ENTRYPOINT/CMD, which the compose file does not
# contain. Writing a `command:` there would DISCARD it, and an instrumented run
# of a task whose entrypoint was silently thrown away is not a measurement of
# that task.
#
# "INSTRUMENTED" MEANS THE PATCHER WROTE A FILE COMPOSE CAN PARSE. It does not
# mean the task runs. Three tasks in the instrumented set
# (`privilege-escalation` — one of the 21 the structural patcher rescues — plus
# `shell-deobfuscation` and `unprivileged-headless-pyrender`, both already in
# the old 218) end their Dockerfile with a non-root `USER`. Both halves of that
# are now addressed ABOVE, in the Dockerfile transform and the `--service-user`
# gate, and neither is free: `shell-deobfuscation` was a BUILD failure and is
# fixed outright, while the other two need `user: root`, which makes the agent
# root and therefore buys a running container at the price of the variable those
# two tasks are about. Ruling 6 is where those belong — an instrument-reach
# number with its mechanism named. Do not launder either into a coverage number.
#
# The service is selected by the ${T_BENCH_TASK_DOCKER_CLIENT_CONTAINER_NAME}
# contract rather than by the literal name `client`, which is what makes
# `home-server-https` — whose agent service is called `webserver` — work instead
# of silently no-op.
#
# THE RECEIPT GOES OUTSIDE THE TASK DIRECTORY, and that is not tidiness.
# `postgres-csv-clean` bind-mounts `./:/app/data`, and the setup chain below
# `git init`s and `git add -A`s /app — so a receipt beside the compose file
# would be inside the baseline tree the agent can read. Three tasks
# (`cross-entropy-method`, `multistep-definite-integral`, `play-lord`) `COPY .
# /app`, which bakes it into their image. The instrument must not appear in the
# tree it measures.
#
# THE PATH LINE THE PATCHER ADDS, and the number that used to be here.
# `/root/.local/bin` is on the END of the injected PATH, BEHIND the shim. 219 of
# the 241 `run-tests.sh` run their suite through `uv`, and 217 of those first
# `source $HOME/.local/bin/env` — a script whose whole body is *prepend
# `$HOME/.local/bin` unless it is already there*, which is where uv's installer
# puts `uv`. Naming that directory here, behind the shim, makes the env script
# find it present and prepend nothing, so the wrapper still wins.
#
# This comment used to end: "Measured, not reasoned: classify_corpus over all
# 241 tasks gives 18/241 without this line and 235/241 with it."
# `grep -rl classify_corpus .` returned exactly one file — this comment. The
# tool did not exist and the number could not be re-derived by anyone. It is
# withdrawn. `scripts/classify-corpus.py` now IS that tool; run it and read its
# output, which on the pinned corpus is 25/241 without the line and 241/241 with
# it, under a PATH model the script states and this comment does not restate:
#
#     scripts/classify-corpus.py "$TASKS_SRC"
#
# Those are different numbers from the withdrawn ones and they are not a
# correction of them — nothing is known about how 18 and 235 were produced.
#
# AND THE 241 IS NOT REACH EITHER. 2026-09-04: the script models a wrapper for
# every WRAPPED name — `WrapPolicy::All` — while the installer ships
# `WrapPolicy::Resolvable`, under which a wrapper exists only where the program
# resolves inside that task's image. `Resolvable` writes a subset, so 25 and 241
# are upper bounds; 241 equals the corpus size, so as a bound it says nothing.
# Reach under the shipped policy is UNMEASURED and no substitute figure is given
# — one cannot be derived without building the 241 images. The script re-derives
# the shipped policy from crates/capture/src/shim.rs and prints this above its
# own numbers, so the next reader is told by the tool and not by this comment.
# The DELTA (+216) is what makes the PATH tail load-bearing and it is unaffected.
# ---------------------------------------------------------------------------
mkdir -p "$RECEIPTS"
# THE EXTRA ENVIRONMENT ENTRY, and why it cannot go anywhere else.
#
# Terminal-Bench's installed-agent adapters propagate a FIXED set of variables
# into the container. Claude Code's is exactly ANTHROPIC_API_KEY plus two
# background-task flags (`claude_code_agent.py::_env`), and the adapter writes
# them to /installed-agent/setup-env.sh, which the session sources — overwriting
# nothing else. ANTHROPIC_BASE_URL is not in that set, so the ONLY way to point
# a scripted run at the local endpoint is the compose `environment:` list the
# session inherits.
INSTRUMENT_ENV_ARGS=()
if [ "$SCRIPTED" -eq 1 ]; then
  INSTRUMENT_ENV_ARGS=(--extra-env "ANTHROPIC_BASE_URL=http://host.docker.internal:$RIG_PORT")
fi
"$PILOT_PY" "$REPO_ROOT/scripts/compose-instrument.py" \
  "$STAGE/$TASK_ID/docker-compose.yaml" \
  --harness "$HARNESS" \
  ${SERVICE_USER_ARGS[@]+"${SERVICE_USER_ARGS[@]}"} \
  ${INSTRUMENT_ENV_ARGS[@]+"${INSTRUMENT_ENV_ARGS[@]}"} \
  --receipt "$RECEIPTS/$TASK_ID.json" > "$RECEIPTS/$TASK_ID.stdout"

# ---------------------------------------------------------------------------
# 4. Run.
# ---------------------------------------------------------------------------
say "run"

# THE CEILING, CHECKED BEFORE THE MODEL IS CALLED. `charge` alone cannot hold a
# run total — it learns a task's cost only after the model has been paid for it.
# This asks whether a WORST-CASE task still fits, which makes the arithmetic
# closed: spent_after <= spent_before + per_task <= run_total. Exit 4 is a
# budget halt and nothing else in that binary uses it.
if [ "$PAYS" -eq 1 ]; then
  set +e
  "$TL_HOST" pilot-budget --ledger "$SPEND_LEDGER" --model "$MODEL" check
  BUDGET_RC=$?
  set -e
  if [ "$BUDGET_RC" -ne 0 ]; then
    die "the budget guard refused this run before it started (exit $BUDGET_RC).
  Nothing has been spent. The spend ledger is $SPEND_LEDGER — it is append-only
  and it survives this process, which is the whole point: ten invocations share
  one total instead of ten fresh zeroes."
  fi
fi

export THRULINE_BIN="$BIN"
export THRULINE_MOUNT="$MOUNT"
export THRULINE_SESSION="$RUN_ID"
[ "$HARNESS" = "codex" ] && export CODEX_HOME=/root/.codex

# ---------------------------------------------------------------------------
# THE SCRIPTED ENDPOINT. $0, and it must be IMPOSSIBLE to mistake for a paid run.
#
# It binds 0.0.0.0 rather than loopback because the container reaches it through
# the Docker gateway (`host.docker.internal`), which is not the host's loopback
# interface. It holds no credential, serves canned SSE, and is killed below.
#
# THE PREFLIGHT AND THE BASELINE. The endpoint is proved reachable from the host
# BEFORE the harness starts — a run that discovers the port was wrong after
# forty minutes of image build has wasted the measurement. But the preflight
# writes a request line of its own, so the "was it reached from the CONTAINER"
# assertion cannot simply ask whether the log is non-empty: it must ask whether
# it GREW. RIG_BASELINE is that line count, taken after the preflight.
RIG_PID=""
RIG_BASELINE=0
stop_rig() {
  if [ -n "$RIG_PID" ]; then
    kill "$RIG_PID" 2>/dev/null || true
    wait "$RIG_PID" 2>/dev/null || true
    RIG_PID=""
  fi
}
if [ "$SCRIPTED" -eq 1 ]; then
  # NOT A KEY. Claude Code requires *a* value in ANTHROPIC_API_KEY and tb's
  # adapter reads it with `os.environ[...]`, so an unset one is a KeyError forty
  # minutes in. This is the literal placeholder; it authenticates against
  # nothing, and the base URL means it is never offered to anything real.
  export ANTHROPIC_API_KEY="sk-ant-FAKE-scripted-rig-never-leaves-this-machine"
  # AND A MODEL NAME THAT IS NOT A MODEL. tb's adapter forwards ANTHROPIC_MODEL
  # from the invoking environment when it is set, so a run started from a shell
  # that happens to export a real model id put that id on the wire — measured:
  # the endpoint logged `claude-opus-5`. It went to 127.0.0.1 and cost nothing,
  # but a real model id in a $0 run's record is a thing a later reader has to
  # take on trust. Pinning it makes the record say what it is. Verified against
  # the real CLI: an unknown model name is passed through unvalidated.
  export ANTHROPIC_MODEL="thruline-scripted-rig-fake-model"
  "$PILOT_PY" "$REPO_ROOT/scripts/rig/fake_anthropic.py" \
    "$RIG_PORT" "$MOUNT/rig-requests.log" "$MOUNT/rig-plan.json" 0.0.0.0 &
  RIG_PID=$!
  trap stop_rig EXIT
  sleep 1
  curl -sS -m 5 -o /dev/null "http://127.0.0.1:$RIG_PORT/v1/models" \
    || die "the scripted endpoint did not answer on 127.0.0.1:$RIG_PORT.
  Refusing to start a run whose agent would have nothing to talk to: the failure
  would arrive as an empty spool after the image build, and read as a capture
  finding rather than as a rig that never came up.
  Set THRULINE_RIG_PORT to a free port."
  RIG_BASELINE="$(wc -l <"$MOUNT/rig-requests.log" 2>/dev/null | tr -d ' ')"
  printf 'scripted  : endpoint up on 0.0.0.0:%s (container reaches it at\n' "$RIG_PORT"
  printf '            host.docker.internal:%s); %s preflight request(s) logged\n' \
    "$RIG_PORT" "$RIG_BASELINE"
fi

TB_ARGS=(run --agent "$TB_AGENT" --dataset-path "$STAGE" --task-id "$TASK_ID"
         --output-path "$OUT" --run-id "$RUN_ID")
[ -n "$MODEL" ] && TB_ARGS+=(--model "$MODEL")

set +e
"$TB" "${TB_ARGS[@]}"
TB_RC=$?
set -e
printf 'tb exit: %s\n' "$TB_RC"
stop_rig

# ---------------------------------------------------------------------------
# DID THE AGENT ACTUALLY TALK TO THE FAKE ENDPOINT?
#
# A run that reached NO endpoint and a run that reached a REAL one both leave
# this log at its preflight baseline. Neither may be reportable as a success —
# the first is a rig failure wearing a capture finding's clothes, and the second
# is the thing this whole configuration exists to make impossible.
if [ "$SCRIPTED" -eq 1 ]; then
  RIG_FINAL="$(wc -l <"$MOUNT/rig-requests.log" 2>/dev/null | tr -d ' ')"
  RIG_FROM_CONTAINER=$(( ${RIG_FINAL:-0} - ${RIG_BASELINE:-0} ))
  printf 'scripted  : %s request(s) from the container (%s total, %s preflight)\n' \
    "$RIG_FROM_CONTAINER" "$RIG_FINAL" "$RIG_BASELINE"
  if [ "$RIG_FROM_CONTAINER" -le 0 ]; then
    die "THE SCRIPTED AGENT NEVER REACHED THE FAKE ENDPOINT.

  $MOUNT/rig-requests.log did not grow past its preflight baseline, so the agent
  inside the container talked to nothing — or to something else. Either way the
  spool below is not a measurement of capture coverage.

  Check that ANTHROPIC_BASE_URL survived into the container:
    grep ANTHROPIC_BASE_URL $STAGE/$TASK_ID/docker-compose.yaml
  and that host.docker.internal resolves from it. And check that nothing was
  spent: this configuration carries a fake key, so a real API would have
  refused it, but say so out loud rather than assuming."
  fi
fi

# ---------------------------------------------------------------------------
# 5. Fold, then report from the EXECUTION RECORD (§0.2) rather than from tb's
#    summary. tb reported 100% on a run that captured nothing; its results are
#    corroboration and may never be the sole basis for a bound check.
# ---------------------------------------------------------------------------
say "fold"
# THE FOLD CONTAINER MUST NOT MANUFACTURE A SNAPSHOT, and it used to.
#
# `thruline fold --repo <r>` re-snapshots `<r>` at every mutating-tool boundary
# (crates/capture/src/fold.rs:376) and writes each result as a `snapshot` event.
# This container mounts only "$MOUNT"; the task's workspace is NOT here and its
# container is already gone. The three lines that used to sit here —
# `mkdir -p /app && cd /app`, `git init`, `git commit --allow-empty
# -m fold-baseline` — built an empty repository for that re-snapshot to find,
# so every fold appended a `snapshot` event asserting the workspace hashed to
# git's empty tree at a moment when the workspace did not exist.
#
# MEASURED, A/B, on a copy of `mount-m1-jq-data-processing` with `exec.db`
# removed and the same 39-line spool folded both ways:
#
#   with the git init:  40 events · 14 snapshots · 1 EMPTY-tree · 2 checks
#   without it:         39 events · 13 snapshots · 0 EMPTY-tree · 2 checks
#
# Exactly one event differs and it is the fabricated one — it carried
# `"taken_at":"fold"` and `"contemporaneous":false`, so the record said what it
# was and nothing read it. The 13 real snapshots and BOTH checks, with their
# real trees, are identical across the two folds: `checks.input_tree_hash` is
# copied from the spooled payload (fold.rs:578), never re-derived here, so the
# PINNED component-3 figure cannot move by this. What moves is the ALTERNATE
# reading and any future count over `snapshot` events.
#
# Across the m1 records this was not rare: 69 tasks whose in-container baseline
# tree is real carry exactly one empty-tree snapshot each. That one is this one.
#
# Without a repository to find, the fold now PRINTS ITS FAILURE — "snapshot of
# <ws> failed: … not a git repository" — once per fold, and exits 0. That line
# is correct and expected, and it is the honest form of the same fact: this
# container cannot snapshot the workspace. Do not "fix" it by restoring the
# `git init`. `thruline fold` cannot be told to skip snapshotting at all
# (`repo: Option<PathBuf>` defaults to the cwd and `shim_cmd::fold` always
# passes `Some`), which is filed as R2 in docs/tracks/E/INTERFACE-REQUESTS.md.
if [ -s "$SPOOL_FILE" ]; then
  docker run --rm -v "$MOUNT:/thruline" -e "TL_SPOOL=/thruline/spool/$HARNESS.jsonl" \
    "$IMAGE" sh -c '
    set -eu
    export THRULINE_HOME=/thruline/home
    # The workspace the SETUP CHAIN recorded, not a literal. Older mounts
    # predate workdir.txt and fall back to the value it would have held.
    WS="$(cat /thruline/workdir.txt 2>/dev/null || echo /app)"
    echo "fold: workspace was $WS (absent in this container by design)"
    thruline fold --spool "$TL_SPOOL" --exec-db /thruline/exec.db --repo "$WS"
  '
else
  echo "spool is EMPTY — nothing to fold. This is a capture failure, not a clean run."
fi

# The transcript side. `ingest` needs --apply or it only previews (A131: a bare
# ingest once filled a disk), and --source codex rather than auto, because auto
# fails closed on an ambiguous path and a silent no-op here would look like a
# clean ingest of nothing.
# The transcript FILENAME grammar is the harness's too: Codex writes
# `rollout-*.jsonl` under a date tree; Claude Code writes `<uuid>.jsonl` under a
# per-project directory. A `rollout-*` glob over a Claude Code mount finds
# nothing and prints "no transcripts", which reads as an agent that produced
# none rather than as a search for the wrong name.
say "ingest"
case "$HARNESS" in
  codex)       TRANSCRIPT_GLOB='rollout-*.jsonl' ;;
  claude-code) TRANSCRIPT_GLOB='*.jsonl' ;;
esac
if find "$MOUNT/$TRANSCRIPT_DIR" -name "$TRANSCRIPT_GLOB" -print -quit 2>/dev/null | grep -q .; then
  # HOME, AND THE DIRECTORY UNDER IT. Measured: this step died with
  #
  #   i/o error at /root/.thruline/mirrors: No such file or directory
  #
  # and, under `set -e`, took the whole run with it — the report never printed.
  # Two facts behind that, both worth stating because the obvious fix is the
  # wrong one:
  #
  #  * The mirror root is `$HOME/.thruline/mirrors` and NOT `$THRULINE_HOME/…`
  #    (crates/core/src/ingest.rs::default_mirror_root reads HOME only), so
  #    passing THRULINE_HOME here changes nothing.
  #  * It is not created on demand.
  #
  # HOME is pointed at the capture mount rather than /root because the mirror is
  # evidence: inside a `--rm` container, /root does not survive the command that
  # wrote it. This was invisible on the codex path only because no run there has
  # ever had a rollout to ingest.
  docker run --rm -v "$MOUNT:/thruline" -e HOME=/thruline/home "$IMAGE" sh -c '
    set -eu
    mkdir -p "$HOME/.thruline/mirrors"
    # 0700, ASKED FOR EXPLICITLY. The ingest refuses a mirror root that is not
    # already private — "configured mirror root is not already private (0700)" —
    # and the bind-mounted /thruline is created 0755. That refusal is right: a
    # transcript mirror is the most sensitive thing this pilot writes.
    chmod 700 "$HOME/.thruline" "$HOME/.thruline/mirrors"
    thruline ingest --source "$1" --apply --db /thruline/ledger.db "/thruline/$2"
  ' sh "$INGEST_SOURCE" "$TRANSCRIPT_DIR"
else
  echo "no rollout transcripts to ingest."
  if [ "$PAYS" -eq 1 ]; then
    echo "  For a PAYING agent this is a finding, not an absence: the agent ran and"
    echo "  produced no session rollout, so usage cannot be measured. See the COST"
    echo "  section — pilot::check_first_task halts on exactly this."
  else
    echo "  Expected: 'oracle' replays a recorded solution and opens no model session."
  fi
fi

say "report"
set +e
MOUNT="$MOUNT" OUT="$OUT" RUN_ID="$RUN_ID" TASK_ID="$TASK_ID" \
AGENT="$AGENT" MODEL="$MODEL" PAYS="$PAYS" \
TL_HOST="$TL_HOST" TL_HOST_VERSION="$TL_HOST_VERSION" \
python3 "$REPO_ROOT/scripts/pilot-report.py"
REPORT_RC=$?
set -e

# ---------------------------------------------------------------------------
# 6. CHARGE THE LEDGER. The report writes `usage.tsv` beside the capture mount
#    so that the numbers charged are the numbers printed, rather than a second
#    derivation that could disagree with the first.
#
#    A paying run that produced no usage.tsv is NOT charged $0 — it is a halt.
#    Recording zero for a task the model was paid for makes the ledger
#    understate every later check, which is how a ceiling stops being one.
# ---------------------------------------------------------------------------
if [ "$PAYS" -eq 1 ]; then
  say "charge"
  if [ -s "$MOUNT/usage.tsv" ]; then
    IFS=$'\t' read -r U_IN U_OUT U_CR U_CW5 U_CW1 U_SRC _ < "$MOUNT/usage.tsv"
    set +e
    "$TL_HOST" pilot-budget --ledger "$SPEND_LEDGER" --model "$MODEL" charge \
      --task "$TASK_ID" --input "$U_IN" --output "$U_OUT" \
      --cache-read "$U_CR" --cache-write-5m "$U_CW5" --cache-write-1h "$U_CW1" \
      --source "$U_SRC"
    CHARGE_RC=$?
    set -e
  else
    echo "no usage.tsv — the report could not measure this task's usage." >&2
    CHARGE_RC=5
  fi
else
  CHARGE_RC=0
fi

# The report is printed first — it is worth having either way — and THEN the
# exit code tells the truth. This script used to exit 0 after a run in which
# the container never built and nothing was captured, so a caller reading only
# the status saw success. Rule 12: fail loud.
if [ "$TB_RC" -ne 0 ]; then
  die "the harness exited $TB_RC — the run did not complete. The coverage
  measures above describe an INFRASTRUCTURE FAULT, not a plumbing finding."
fi
if [ ! -s "$SPOOL_FILE" ]; then
  die "the harness exited 0 but captured NOTHING. A green run with an empty
  spool is the failure this pilot exists to catch — do not read the zeros above
  as a measurement."
fi
if [ "$REPORT_RC" -ne 0 ]; then
  die "the report exited $REPORT_RC — it found a halting condition, named above.
  It used to PRINT 'this HALTS the run' and exit 0, so a caller reading only the
  status saw a clean pilot over an unmeasured spend. A halt that returns success
  is not a halt."
fi
if [ "$CHARGE_RC" -ne 0 ]; then
  die "the spend could not be charged to $SPEND_LEDGER (exit $CHARGE_RC).
  Either the run crossed the ceiling — in which case STOP, the result is the
  halt — or the usage could not be measured, in which case do NOT record \$0 for
  a task the model was paid for."
fi
