#!/usr/bin/env bash
# packaging/demo.sh — the Live Truth Loop, end to end, on synthetic data.
#
# Stages the A200/A201 proof pattern against a throwaway --db in a temp
# directory: a transcript whose final turn makes one evidenced claim, one
# claim with no proof in the record, and one open promise; then walks
# digest precompute -> brief -> audit -> claim submission over MCP ->
# verify-claims -> `thruline why`, and ends with the evidence landing and
# the brief going silent.
#
# Runs offline. Touches nothing outside its own mktemp dir: the ledger,
# digest cache, and spill path all derive from --db. It never runs
# `thruline init`, so no hooks or settings on this machine are changed.
# Every byte of session data below is synthetic.
#
# Requirements: bash, python3 (JSON plumbing only), and a thruline binary —
#   THRULINE_BIN=/path/to/thruline packaging/demo.sh
# or, from a source checkout, target/release or target/debug is found
# automatically (build one with `cargo build -p thruline-cli [--release]`).

set -euo pipefail

# ── locate the binary ────────────────────────────────────────────────────
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${THRULINE_BIN:-}"
if [ -z "$BIN" ]; then
  for candidate in \
    "$REPO_ROOT/target/release/thruline" \
    "$REPO_ROOT/target/debug/thruline"; do
    [ -x "$candidate" ] && BIN="$candidate" && break
  done
fi
if [ -z "$BIN" ] && command -v thruline >/dev/null 2>&1; then
  BIN="$(command -v thruline)"
fi
if [ -z "$BIN" ]; then
  echo "no thruline binary found — set THRULINE_BIN or run: cargo build -p thruline-cli" >&2
  exit 1
fi
command -v python3 >/dev/null 2>&1 || { echo "python3 is required (JSON plumbing)" >&2; exit 1; }

# ── throwaway world ──────────────────────────────────────────────────────
TMP="$(mktemp -d "${TMPDIR:-/tmp}/thruline-demo.XXXXXX")"
# Sealed mirror generations are written read-only; restore write permission
# on our own throwaway tree before removing it.
trap 'chmod -R u+w "$TMP" 2>/dev/null; rm -rf "$TMP"' EXIT
DB="$TMP/ledger.db"
SESSION="dddddddd-1111-4222-8333-444444444444"
NEXT_SESSION="eeeeeeee-2222-4333-8444-555555555555"
PROJ="-demo-app"
PROJ_DIR="$TMP/projects/$PROJ"
TRANSCRIPT="$PROJ_DIR/$SESSION.jsonl"
mkdir -p "$PROJ_DIR"

tl() { "$BIN" --db "$DB" "$@"; }

say()  { printf '\n\033[1m── %s\033[0m\n' "$*"; }
show() { sed 's/^/  /'; }

# ── the synthetic session: three sentences, three fates ──────────────────
# 1. "All 12 tests pass."            — evidenced (a real green test row below)
# 2. "I've added retry logic ..."    — nothing in the record backs it
# 3. "I'll wire the new panel ..."   — a promise, left open
cat > "$TRANSCRIPT" <<EOF
{"type":"user","uuid":"u1","parentUuid":null,"isSidechain":false,"sessionId":"$SESSION","timestamp":"2026-08-31T03:00:01.000Z","cwd":"/demo/app","message":{"role":"user","content":"finish the retry work"}}
{"type":"assistant","uuid":"a1","parentUuid":"u1","isSidechain":false,"sessionId":"$SESSION","timestamp":"2026-08-31T03:00:02.000Z","cwd":"/demo/app","message":{"role":"assistant","content":[{"type":"text","text":"All 12 tests pass. I've added retry logic to \`client.rs\`. I'll wire the new panel into \`layout.rs\` next."}]}}
EOF

# The session's tool record holds ONE event: a green cargo test run,
# appended through the production sensor appender. That is what makes
# sentence 1 evidenced and sentence 2 a no-proof line.
sensor_append() { # event, payload-json on stdin
  tl sensor append "$1"
}
python3 - "$SESSION" "$TRANSCRIPT" <<'PY' | sensor_append PostToolUse
import json, sys
print(json.dumps({
    "session_id": sys.argv[1],
    "transcript_path": sys.argv[2],
    "cwd": "/demo/app",
    "hook_event_name": "PostToolUse",
    "tool_name": "Bash",
    "tool_input": {"command": "cargo test"},
    "tool_response": {"stdout": "test result: ok. 12 passed; 0 failed",
                      "stderr": "", "interrupted": False},
}))
PY

say "the session ends — the SessionEnd speaker precomputes the digest (silent)"
python3 - "$SESSION" "$TRANSCRIPT" <<'PY' | tl digest precompute --hook session-end
import json, sys
print(json.dumps({"session_id": sys.argv[1], "transcript_path": sys.argv[2],
                  "cwd": "/demo/app", "hook_event_name": "SessionEnd"}))
PY
test -f "$TMP/digest/$PROJ.md" || { echo "expected a digest cache" >&2; exit 1; }
echo "digest cache written:"
cat "$TMP/digest/$PROJ.md" | show

say "the next session starts — the brief injects at most 3 receipt-backed lines"
BRIEF=$(python3 - "$NEXT_SESSION" "$PROJ_DIR/$NEXT_SESSION.jsonl" <<'PY' | tl brief --hook session-start
import json, sys
print(json.dumps({"session_id": sys.argv[1], "transcript_path": sys.argv[2],
                  "cwd": "/demo/app", "hook_event_name": "SessionStart",
                  "source": "startup"}))
PY
)
printf '%s' "$BRIEF" | python3 -c 'import json,sys; print(json.load(sys.stdin)["hookSpecificOutput"]["additionalContext"])' | show
printf '%s' "$BRIEF" | grep -q "claimed, and no proof found in the record" \
  || { echo "expected the honest form in the brief" >&2; exit 1; }

say "the stop audit announces the no-proof claim to the append-only ledger (silent)"
python3 - "$SESSION" "$TRANSCRIPT" <<'PY' | tl audit --hook stop
import json, sys
print(json.dumps({"session_id": sys.argv[1], "transcript_path": sys.argv[2],
                  "cwd": "/demo/app", "hook_event_name": "Stop",
                  "stop_hook_active": False}))
PY

say "thruline why — the receipt behind the finding"
WHY_ID=$(grep -o 'spkf_[0-9a-f]*' "$TMP/digest/$PROJ.md" | head -1)
echo "\$ thruline why $WHY_ID"
tl why "$WHY_ID" | show

# ── the claim loop: an agent claims completion over MCP ──────────────────
say "ingest the transcript, then the agent submits a completion claim over MCP"
tl ingest "$TMP/projects" --apply --mirror-root "$TMP/mirrors" >/dev/null

mcp_call() { # tool, args-json -> tool payload json
  python3 - "$1" "$2" <<'PY' | tl mcp --agent claude | python3 -c '
import json, sys
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    r = json.loads(line)
    if "error" in r:
        raise SystemExit("mcp error: %s" % r["error"])
    print(r["result"]["content"][0]["text"])
'
import json, sys
print(json.dumps({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                  "params": {"name": sys.argv[1],
                             "arguments": json.loads(sys.argv[2])}}))
PY
}

POSITION=$(mcp_call get_context_packet "{\"project\":\"$PROJ\"}" \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["chain_position"])')
CLAIM_ARGS=$(python3 - "$PROJ" "$POSITION" <<'PY'
import json, sys
print(json.dumps({"project": sys.argv[1],
                  "obligation": "finish the retry work",
                  "chain_position": sys.argv[2],
                  "manifest": {"tests": ["cargo test"], "files": ["client.rs"]}}))
PY
)
ACCEPTED=$(mcp_call claim_completion "$CLAIM_ARGS")
echo "claim_completion ->"
printf '%s\n' "$ACCEPTED" | python3 -m json.tool | show
printf '%s' "$ACCEPTED" | grep -q '"ready_for_verification"' \
  || { echo "expected ready_for_verification" >&2; exit 1; }

# The claiming session's own capture delivery of that call — how the
# verifier identifies which session's record to search.
python3 - "$SESSION" "$TRANSCRIPT" "$CLAIM_ARGS" <<'PY' | sensor_append PostToolUse
import json, sys
print(json.dumps({
    "session_id": sys.argv[1], "transcript_path": sys.argv[2],
    "cwd": "/demo/app", "hook_event_name": "PostToolUse",
    "tool_name": "mcp__thruline__claim_completion",
    "tool_input": json.loads(sys.argv[3]),
    "tool_response": {"content": [{"type": "text", "text": "accepted"}]},
}))
PY

say "thruline verify-claims — the record is searched; one entry has no proof"
tl verify-claims | show

say "the next brief now carries the verdict"
BRIEF2=$(python3 - "$NEXT_SESSION" "$PROJ_DIR/$NEXT_SESSION.jsonl" <<'PY' | tl brief --hook session-start
import json, sys
print(json.dumps({"session_id": sys.argv[1], "transcript_path": sys.argv[2],
                  "cwd": "/demo/app", "hook_event_name": "SessionStart",
                  "source": "startup"}))
PY
)
printf '%s' "$BRIEF2" | python3 -c 'import json,sys; print(json.load(sys.stdin)["hookSpecificOutput"]["additionalContext"])' | show

say "thruline why — the verdict, replayed entry by entry"
CVRD_ID=$(grep -o 'cvrd_[0-9a-f]*' "$TMP/digest/$PROJ.md" | head -1 || true)
if [ -n "$CVRD_ID" ]; then
  echo "\$ thruline why $CVRD_ID"
  tl why "$CVRD_ID" | show
fi

# ── the honest ending: the evidence lands, and the loop goes quiet ───────
say "the edit lands in the record — verify-claims again"
python3 - "$SESSION" "$TRANSCRIPT" <<'PY' | sensor_append PostToolUse
import json, sys
print(json.dumps({
    "session_id": sys.argv[1], "transcript_path": sys.argv[2],
    "cwd": "/demo/app", "hook_event_name": "PostToolUse",
    "tool_name": "Edit",
    "tool_input": {"file_path": "/demo/app/src/client.rs"},
    "tool_response": {"filePath": "/demo/app/src/client.rs"},
}))
PY
tl verify-claims | show

# Re-audit the turn: with the edit on record the turn is clean, and the
# stale accusation cannot survive into the next brief.
python3 - "$SESSION" "$TRANSCRIPT" <<'PY' | tl audit --hook stop
import json, sys
print(json.dumps({"session_id": sys.argv[1], "transcript_path": sys.argv[2],
                  "cwd": "/demo/app", "hook_event_name": "Stop",
                  "stop_hook_active": False}))
PY

say "the next brief"
BRIEF3=$(python3 - "$NEXT_SESSION" "$PROJ_DIR/$NEXT_SESSION.jsonl" <<'PY' | tl brief --hook session-start
import json, sys
print(json.dumps({"session_id": sys.argv[1], "transcript_path": sys.argv[2],
                  "cwd": "/demo/app", "hook_event_name": "SessionStart",
                  "source": "startup"}))
PY
)
if [ -z "$BRIEF3" ]; then
  echo "  (silence — supported claims and clean turns say nothing)"
else
  printf '%s' "$BRIEF3" | python3 -c 'import json,sys; print(json.load(sys.stdin)["hookSpecificOutput"]["additionalContext"])' | show
  if printf '%s' "$BRIEF3" | grep -q "no proof found"; then
    echo "a stale accusation survived a supported verdict — that is a bug" >&2
    exit 1
  fi
fi

say "done — everything above was synthetic data in $TMP (removed on exit)"
