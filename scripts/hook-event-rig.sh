#!/usr/bin/env bash
#
# Verify Codex hook capture WITHOUT SPENDING ANYTHING.
#
#   scripts/hook-event-rig.sh [tools|compaction|notrust]
#
# Codex fires `SessionStart` and `UserPromptSubmit` before its first model
# request, and every other subscribed event is driven by what the model
# *replies*. A scripted local endpoint replies without a model — so capture
# coverage, which looked like something only a paid run could demonstrate, is
# provable for free.
#
# There is NO credential here and none is read. Codex is pointed at 127.0.0.1
# via `model_providers.*.base_url`, and the key it sends is a placeholder this
# script invents. Nothing reaches a paid API.
set -euo pipefail
set +m   # no job-control chatter when the server is killed

MODE="${1:-tools}"
PORT="${RIG_PORT:-8749}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="${RIG_WORK:-${TMPDIR:-/tmp}/thruline-hook-rig}"
BIN="${THRULINE_BIN:-$ROOT/target/debug/thruline}"

die() { printf 'rig: %s\n' "$*" >&2; exit 1; }

command -v codex >/dev/null || die "codex is not on PATH"
[ -x "$BIN" ] || die "thruline binary not found at $BIN (cargo build first)"

rm -rf "$WORK"; mkdir -p "$WORK/codexhome" "$WORK/work"
PLAN="$WORK/plan.json"

case "$MODE" in
  tools)
    # One success and one FAILURE. The failing command is the CONFLICT-6b case:
    # `PostToolUse` fires for it, and carries nothing that says it failed.
    cat > "$PLAN" <<'J'
[["exec","echo thruline-rig-ok"],["exec","exit 42"],["text","finished"]]
J
    CTX=""
    ;;
  compaction)
    # A tiny context window plus bulky tool output forces Codex to compact,
    # which is the only way PreCompact/PostCompact can fire.
    cat > "$PLAN" <<'J'
[["exec","echo start"],
 ["exec","for i in $(seq 1 900); do echo 'padding to consume the tiny context window'; done"],
 ["exec","for i in $(seq 1 900); do echo 'more padding to force a compaction boundary'; done"],
 ["exec","for i in $(seq 1 900); do echo 'still more padding'; done"],
 ["exec","echo after-compaction"],
 ["text","finished"]]
J
    CTX="-c model_context_window=2000"
    ;;
  notrust)
    # The control for the trust finding: identical to `tools`, run WITHOUT
    # --dangerously-bypass-hook-trust. Expect ZERO captured events.
    cat > "$PLAN" <<'J'
[["exec","echo thruline-rig-ok"],["text","finished"]]
J
    CTX=""
    ;;
  *) die "unknown mode '$MODE' (tools|compaction|notrust)" ;;
esac

export CODEX_HOME="$WORK/codexhome"
export THRULINE_HOME="$WORK/th"
# A placeholder. Codex requires *a* value; this one is meaningless and never
# leaves the loopback interface.
export OPENAI_API_KEY="rig-placeholder-never-leaves-loopback"

"$BIN" hook-install --harness codex --scope user >"$WORK/install.log" 2>&1 \
  || die "hook-install failed; see $WORK/install.log"

python3 "$ROOT/scripts/rig/fake_responses.py" "$PORT" "$WORK/req.log" "$PLAN" &
SRV=$!
# REAP IT, don't mute it. bash 3.2 announces a signalled background job
# ("Terminated: 15 … fake_responses.py") at the NEXT command after the kill, and
# `set +m` does not stop that — which put a line reading like an error in the
# middle of an otherwise clean proof. `wait` collects the status here, so the
# shell has nothing left to report asynchronously. Redirecting the rig's stderr
# would have hidden real failures along with it; this silences exactly the one
# line. The `|| true` is for `wait`'s 143 under `set -e`.
stop_server() {
  kill "$SRV" 2>/dev/null || true
  wait "$SRV" 2>/dev/null || true
}
trap stop_server EXIT
sleep 1

TRUST=(--dangerously-bypass-hook-trust)
[ "$MODE" = "notrust" ] && TRUST=()

cd "$WORK/work"
set +e
# `${a[@]+"${a[@]}"}` and not `"${a[@]}"`: under `set -u`, bash 3.2 (which is
# what macOS ships) treats an EMPTY array expansion as an unbound variable and
# aborts. The `notrust` control is exactly the case with an empty array, so the
# control silently failed to run at all — the one arm whose whole purpose is to
# capture nothing, producing nothing for the reason it was not supposed to.
PATH="$(dirname "$BIN"):$PATH" codex exec ${TRUST[@]+"${TRUST[@]}"} \
  -c model=fake-model -c model_provider=fake $CTX \
  -c model_providers.fake.name=fake \
  -c "model_providers.fake.base_url=http://127.0.0.1:$PORT/v1" \
  -c model_providers.fake.wire_api=responses \
  -c model_providers.fake.env_key=OPENAI_API_KEY \
  --sandbox danger-full-access --skip-git-repo-check \
  -- "run the rig" </dev/null >"$WORK/codex.out" 2>&1
set -e
stop_server

printf '\nmode        : %s\n' "$MODE"
printf 'trust flag  : %s\n' "$([ "$MODE" = notrust ] && echo 'ABSENT (control)' || echo present)"
printf 'requests    : %s\n' "$(wc -l <"$WORK/req.log" 2>/dev/null || echo 0)"
printf 'spool       : %s\n' "$WORK/th/spool/codex.jsonl"
printf 'events captured:\n'
python3 - "$WORK/th/spool/codex.jsonl" <<'PY'
import collections, json, os, sys
p = sys.argv[1]
if not os.path.exists(p):
    print("  NONE — no spool file was written at all")
    raise SystemExit
c = collections.Counter(json.loads(l).get("hook_event") for l in open(p))
for k, v in sorted(c.items()):
    print(f"  {k}: {v}")
print(f"  ({len(c)} distinct)")
PY
