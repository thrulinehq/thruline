#!/usr/bin/env bash
# The build slot's lock (R-2026-09-24-32): take with mkdir, release only as holder, never wait on a pattern.
# A holder's pid is advisory only — an agent's shell pid dies after every command — so nothing here reclaims
# a lock on its own. `reclaim` is a deliberate act by whoever has looked (the integrator), recorded in a report.
#   build-lock.sh take <lockdir> <holder-tag>      blocks until taken; writes holder, pid (advisory), taken-at
#   build-lock.sh release <lockdir> <holder-tag>   releases only if holder matches
#   build-lock.sh status <lockdir>                 prints free | held by <tag> since <time> (pid <n>, advisory)
#   build-lock.sh reclaim <lockdir> <reason>       removes a lock deliberately; prints what it removed
#   build-lock.sh selftest                         proves take/contend/refuse/release/reclaim in a mktemp dir
set -u
cmd=${1:-}; lock=${2:-}; tag=${3:-}
case "$cmd" in
  take)
    until mkdir "$lock" 2>/dev/null; do sleep 15; done
    printf '%s\n' "$tag" > "$lock/holder"; printf '%s\n' "${PPID:-$$}" > "$lock/pid"; date -u +%FT%TZ > "$lock/taken-at"; echo "taken by $tag";;
  release)
    [ -d "$lock" ] || { echo "not held"; exit 0; }
    [ "$(cat "$lock/holder" 2>/dev/null)" = "$tag" ] || { echo "refused: held by $(cat "$lock/holder" 2>/dev/null), not $tag" >&2; exit 1; }
    rm -f "$lock/holder" "$lock/pid" "$lock/taken-at"; rmdir "$lock" && echo "released by $tag";;
  status)
    [ -d "$lock" ] || { echo free; exit 0; }
    echo "held by $(cat "$lock/holder" 2>/dev/null) since $(cat "$lock/taken-at" 2>/dev/null) (pid $(cat "$lock/pid" 2>/dev/null), advisory)";;
  reclaim)
    [ -d "$lock" ] || { echo "nothing to reclaim"; exit 0; }
    echo "reclaimed: was held by $(cat "$lock/holder" 2>/dev/null) since $(cat "$lock/taken-at" 2>/dev/null) — reason: ${tag:-unstated}"
    rm -f "$lock/holder" "$lock/pid" "$lock/taken-at"; rmdir "$lock";;
  selftest)
    t=$(mktemp -d); l="$t/lock"; ok=0; fail=0
    chk() { if eval "$2"; then ok=$((ok+1)); else fail=$((fail+1)); echo "FAIL: $1"; fi; }
    "$0" take "$l" A >/dev/null; chk "take writes holder, pid, taken-at" '[ "$(cat "$l/holder")" = A ] && [ -s "$l/pid" ] && [ -s "$l/taken-at" ]'
    chk "status says held by A" '"$0" status "$l" | grep -q "held by A"'
    "$0" release "$l" B >/dev/null 2>&1; chk "release by non-holder refused" '[ -d "$l" ]'
    ( "$0" take "$l" C & p=$!; sleep 2; kill $p 2>/dev/null; wait $p 2>/dev/null ); chk "a second taker blocks, does not steal" '[ "$(cat "$l/holder")" = A ]'
    "$0" release "$l" A >/dev/null; chk "release by holder frees" '[ ! -d "$l" ]'
    "$0" take "$l" D >/dev/null; "$0" reclaim "$l" "selftest" >/dev/null; chk "reclaim is deliberate and frees" '[ ! -d "$l" ]'
    rm -rf "$t"; echo "selftest: $ok ok, $fail failed"; [ "$fail" = 0 ];;
  *) echo "usage: $0 take|release|status|reclaim|selftest …" >&2; exit 2;;
esac
