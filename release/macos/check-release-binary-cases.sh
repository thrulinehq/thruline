#!/bin/bash
# release/macos/check-release-binary-cases.sh — the decision table of
# lib.sh's check_release_binary (card INFRA-RELEASE-CHECK, rounds 1 and 2;
# card RELEASE-CHECK-GATE-ON, round 3; card C31, v0.2's no-trial start state).
#
# check_release_binary asks the release binary `licence status --json` in
# throwaway homes: a fresh one, one holding only a v0.1.0 trial file dated
# today, one holding a licence that names the production key's id, and one
# holding a test-signed licence (beside a v0.1.0 trial file), that last once
# with no debug seam set, once with each seam set, and once with all of them.
# It must refuse a binary whose paid gate is off, that trusts no production key
# or not the one key 3ef3540d43dbf105, that carries any seam's name, that
# honours any seam, whose fresh home is not the as-shipped start state ("Free,
# no licence": thruline.licence.status v4, licence `absent`, headline `free`,
# no trial field, approving refused for want of Pro and the one paid surface),
# or that lets a v0.1.0 trial file open approving or touches it (C20).
# It must also refuse when it cannot read the answer at all. This drives it
# against stand-in binaries and exits 0 only if every case gets its expected
# verdict, and every refusal names the reason the case was written for:
#
#   a  paid gate off (a build with PAID_GATE false)        → refuse  (was "pass": D-050, ruled in D-244)
#   b  honours the test-key seam: accepts the test licence → refuse
#   c  gate on, approve allowed under the test licence     → refuse
#      (and not in a home with no licence, so (c) reaches the seam loop)
#   d  as shipped: gate on, one key 3ef3540d43dbf105       → pass
#   e  a stderr line, then (b)'s answers                   → refuse  (stderr must not blind the parse)
#   f  a stderr line, then (d)'s answers                   → pass    (stdout only is parsed)
#   g  exits non-zero with no output                       → refuse  (could not determine)
#   h  prints nothing, exits 0                             → refuse
#   i  prints a line that is not JSON                      → refuse
#   j  paid_gate renamed to paid_gatex                     → refuse
#   k  licence_id renamed to licenceId                     → refuse
#   l  keyless: no key, so the gate is forced off          → refuse
#   m  keys_built_in 0 while saying the gate is on         → refuse
#   n  one key, but its id is not 3ef3540d43dbf105         → refuse
#   o1-o4  (d), with one seam's name in the binary         → refuse, naming that seam
#   p1 honours the paid-gate seam: gate off when set       → refuse
#   p2 honours the no-built-in-keys seam: no key when set  → refuse
#   p3 (p2), but the gate is not forced off: only the count moves → refuse
#   q  the status document has a trial field (C20)        → refuse
#   r  keys_built_in printed as the string "1"             → refuse  (could not determine)
#   s  reading the status in a fresh home writes a trial file → refuse
#   t  fresh home answers licence `no_key`, not `absent`   → refuse
#   x  a v0.1.0 trial file dated today opens approving     → refuse
#   y  reading the status rewrites a v0.1.0 trial file     → refuse
#   aa fresh home's headline is `licence`, not `free`      → refuse
#   ab a second surface needs a licence                    → refuse  (everything else is free)
#   ac the status document is schema v3, not v4            → refuse
#   ad approving refused, but not for want of Pro          → refuse
#   ae approving allowed in a fresh home, no licence       → refuse
#   u  (d), with a debug stand-in whose seams act          → pass    (positive controls hold)
#   v  (d), with a "debug" stand-in whose seams do nothing → refuse  (the controls would prove nothing)
#   w  (d), with a debug stand-in where only the key seam acts → refuse
#   z  lib.sh's seam list is exactly the THRULINE_* names that
#      crates/cli/src/licence_cmd.rs and crates/licence/src read with env::var
#      (not a check_release_binary verdict; counted so a new seam cannot be
#      added to the source without the release check learning its name)
#
# Needs what check_release_binary needs: openssl with Ed25519, plutil, xxd.
# Writes only to a mktemp directory it removes on exit.
set -u
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
# shellcheck source=release/macos/lib.sh
. "$here/lib.sh"

work=$(mktemp -d "${TMPDIR:-/tmp}/check-release-binary-cases.XXXXXX")
# KEEP_WORK=1 keeps the directory, to read a case's probe outputs afterwards.
trap '[ -n "${KEEP_WORK:-}" ] || rm -rf "$work"' EXIT

PRODUCTION_KEY_ID=3ef3540d43dbf105

# The model of a thruline binary's `licence status --json`, written once and
# sourced by every stand-in. It lives OUTSIDE the stand-ins, because it has to
# name the seams to model them, and check_release_binary greps the binary file
# for those names: a stand-in that contained the model would be refused by that
# grep before any behaviour was looked at. The knobs are set by each stand-in:
#
#   GATE         true | false   the compiled flag
#   KEYS         n              production keys built in
#   KNOWN        hex            the id of the key(s) built in
#   FORCE_OFF    1 | 0          a build with no key forces the gate off (as licence_cmd.rs does)
#   SEAM_PUB     1              honours the test-key seam
#   SEAM_GATE    1              honours the paid-gate seam
#   SEAM_NOKEYS  1              honours the no-built-in-keys seam
#   LEAK_APPROVE 1              allows approve_version under any installed licence
#   FRESH_ALLOW  1              allows approve_version with no licence installed
#   KEYS_QUOTE   1              prints keys_built_in as a string
#   TRIAL_FIELD  1              prints a v0.1.0-style trial field
#   WRITES_TRIAL 1              reading the status writes a trial file
#   LEAK_TRIAL   1              a trial file opens approve_version
#   TOUCHES_TRIAL 1             reading the status rewrites a trial file
#   FRESH_LICENCE slug          the licence status a home with no licence answers
#   HEADLINE     word           the headline, in place of the derived one
#   PAID_EXTRA   1              a second surface needs a licence
#   SCHEMA       n              the schema_version printed (4 as shipped)
#   REASON       text           the approve refusal's first sentence
#   GATE_KEY / ID_KEY           the JSON names it prints for paid_gate / licence_id
cat >"$work/model.sh" <<'MODEL'
gate=$GATE keys=$KEYS seam_trusted=
if [ "${SEAM_GATE:-}" = 1 ] && [ "${THRULINE_PAID_GATE_DEV:-}" = off ]; then gate=false; fi
if [ "${SEAM_NOKEYS:-}" = 1 ] && [ "${THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS:-}" = 1 ]; then keys=0; fi
if [ "${SEAM_PUB:-}" = 1 ] && [ -n "${THRULINE_LICENCE_DEV_PUBLIC_KEY:-}" ]; then
  keys=$((keys + 1)) seam_trusted=1
fi
if [ "${FORCE_OFF:-1}" = 1 ] && [ "$keys" = 0 ]; then gate=false; fi
lic=$HOME/.thruline/licence.txt
tf=$HOME/.thruline/trial.json
status=${FRESH_LICENCE:-absent} id=null
if [ -f "$lic" ]; then
  k=$(sed -n 's/^key: //p' "$lic")
  if [ "$keys" = 0 ]; then status=no_key
  elif [ "$k" = "$KNOWN" ]; then status=tampered
  elif [ -n "$seam_trusted" ]; then status=valid id="\"$TEST_ID\""
  else status=unknown_key
  fi
fi
if [ "${WRITES_TRIAL:-}" = 1 ] && [ ! -f "$tf" ]; then
  mkdir -p "$HOME/.thruline" && echo '{}' >"$tf"
fi
if [ "${TOUCHES_TRIAL:-}" = 1 ] && [ -f "$tf" ]; then echo '{"format": 1}' >"$tf"; fi
if [ "$gate" = true ]; then
  if [ "$status" = valid ]; then allowed=true
  elif [ "${LEAK_TRIAL:-}" = 1 ] && [ -f "$tf" ]; then allowed=true
  else allowed=false
  fi
else
  allowed=true
fi
[ "${LEAK_APPROVE:-}" = 1 ] && [ -f "$lic" ] && allowed=true
[ "${FRESH_ALLOW:-}" = 1 ] && [ ! -f "$lic" ] && allowed=true
headline=licence sentence='No licence is installed on this machine.'
if [ "$gate" = true ] && [ "$status" = absent ]; then
  headline=free sentence='Free. Approving a version needs a Pro licence.'
fi
headline=${HEADLINE:-$headline}
if [ "$allowed" = true ]; then
  approve='{"allowed": true}'
else
  approve="{\"allowed\": false, \"reason\": \"${REASON:-Approving a version needs a Pro licence.} The licence says $status.\", \"enable\": \"Get Pro.\", \"status\": \"$status\"}"
fi
trial=
[ "${TRIAL_FIELD:-}" = 1 ] && trial=', "trial": {"status": "not_started"}'
paid='[{"surface": "approve_version", "plans": ["pro"]}]'
[ "${PAID_EXTRA:-}" = 1 ] && paid='[{"surface": "approve_version", "plans": ["pro"]}, {"surface": "export", "plans": ["pro"]}]'
[ "${KEYS_QUOTE:-}" = 1 ] && keys="\"$keys\""
printf '{"shape": "thruline.licence.status", "schema_version": %s, "status": "%s", "sentence": "%s", "headline": "%s", "paid": %s, "%s": %s, "entitlements": {"approve_version": %s}%s, "technical": {"%s": %s, "keys_built_in": %s}}\n' \
  "${SCHEMA:-4}" "$status" "$sentence" "$headline" "$paid" "${GATE_KEY:-paid_gate}" "$gate" "$approve" "$trial" "${ID_KEY:-licence_id}" "$id" "$keys"
MODEL

stand_in() { # <name> <how the stand-in answers `licence status --json`: a shell fragment> [a line to embed]
  local dir=$work/$1
  mkdir -p "$dir"
  cat >"$dir/thruline" <<EOF
#!/bin/bash
${3:-}
case "\$1" in
  --version) echo "thruline 0.0.0-stand-in" ;;
  licence) $2 ;;
  *) exit 2 ;;
esac
EOF
  chmod +x "$dir/thruline"
  printf '%s\n' "$dir/thruline"
}

# A stand-in answering through the model: the knobs, then the model.
model() { # <knob assignments>
  printf 'TEST_ID=%s GATE=true KEYS=1 KNOWN=%s FORCE_OFF=1 %s; . %s' \
    "$TEST_LICENCE_ID" "$PRODUCTION_KEY_ID" "$1" "$work/model.sh"
}

right=0
wrong=0
verdict() { # <label> <ok: 0|1> <what was expected> <what happened> [output file]
  if [ "$2" = 1 ]; then
    right=$((right + 1))
    printf 'ok      %s: %s\n' "$1" "$4"
  else
    wrong=$((wrong + 1))
    printf 'WRONG   %s: expected %s, got %s\n' "$1" "$3" "$4"
    [ -n "${5:-}" ] && sed 's/^/        | /' "$5"
  fi
}

# <label> <expect: pass|refuse> <shell fragment for `licence status --json`>
#   [the refusal must contain this text] [a line to embed in the stand-in]
case_() {
  local bin got out=$work/$1/out.txt ok=0
  bin=$(stand_in "$1" "$3" "${5:-}")
  if check_release_binary "$bin" "$work/$1/check" >"$out" 2>&1; then got=pass; else got=refuse; fi
  if [ "$got" = "$2" ]; then
    ok=1
    if [ "$got" = refuse ] && [ -n "${4:-}" ] && ! grep '^refused: ' "$out" | grep -q -F -- "$4"; then
      ok=0
      got="refuse, but not for: $4"
    fi
  fi
  verdict "$1" "$ok" "$2${4:+ (for: $4)}" "$got" "$out"
}

# Each fragment runs inside the stand-in under `env -i PATH=/usr/bin:/bin` plus
# whatever seam the check sets, so it uses only /bin and /usr/bin tools.
could_not='could not read the release binary'"'"'s licence status'

case_ "a gate-off" refuse "$(model 'GATE=false')" "paid gate is off"
case_ "b seam-accepted-the-test-licence" refuse "$(model 'SEAM_PUB=1')" "accepted a test-signed licence"
case_ "c gate-on-approve-allowed" refuse "$(model 'LEAK_APPROVE=1')" "allows approve_version under a test-signed licence"
case_ "d as-shipped" pass "$(model '')"
case_ "e stderr-line-then-test-licence" refuse "echo 'note: a diagnostic on stderr' >&2; $(model 'SEAM_PUB=1')" "accepted a test-signed licence"
case_ "f stderr-line-then-as-shipped" pass "echo 'note: a diagnostic on stderr' >&2; $(model '')"
case_ "g probe-exits-nonzero-silent" refuse "exit 3" "$could_not"
case_ "h probe-prints-nothing" refuse "exit 0" "$could_not"
case_ "i probe-prints-non-json" refuse "echo 'licence: status unavailable'" "$could_not"
case_ "j paid-gate-key-renamed" refuse "$(model 'GATE_KEY=paid_gatex')" "no boolean paid_gate"
case_ "k licence-id-key-renamed" refuse "$(model 'ID_KEY=licenceId')" "technical.licence_id"
case_ "l keyless" refuse "$(model 'KEYS=0')" "paid gate is off"
case_ "m keys-zero-gate-on" refuse "$(model 'KEYS=0 FORCE_OFF=0')" "no production licence key"
case_ "n wrong-key-id" refuse "$(model 'KNOWN=0123456789abcdef')" "does not trust the production licence key $PRODUCTION_KEY_ID"
n=0
for s in THRULINE_LICENCE_DEV_PUBLIC_KEY THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS THRULINE_PAID_GATE_DEV THRULINE_LICENCE_DEV_AFTER_RENAME; do
  n=$((n + 1))
  case_ "o$n seam-name-in-binary-$s" refuse "$(model '')" "($s)" "# $s"
done
case_ "p1 honours-the-paid-gate-seam" refuse "$(model 'SEAM_GATE=1')" "with THRULINE_PAID_GATE_DEV=off set, the release binary's paid gate is off"
case_ "p2 honours-the-no-built-in-keys-seam" refuse "$(model 'SEAM_NOKEYS=1')" "with THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS=1 set, the release binary's paid gate is off"
case_ "p3 the-no-built-in-keys-seam-moves-only-the-count" refuse "$(model 'SEAM_NOKEYS=1 FORCE_OFF=0')" "with THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS=1 set, the release binary trusts 0"
case_ "q status-has-a-trial-field" refuse "$(model 'TRIAL_FIELD=1')" "in a fresh home the release binary's licence status has a trial field"
case_ "r keys-built-in-a-string" refuse "$(model 'KEYS_QUOTE=1')" "integer technical.keys_built_in"
case_ "s fresh-home-status-writes-a-trial-file" refuse "$(model 'WRITES_TRIAL=1')" "reading the licence status wrote a trial file"
case_ "t fresh-home-licence-not-absent" refuse "$(model 'FRESH_LICENCE=no_key')" "as-shipped start state"
case_ "x a-v010-trial-file-opens-approving" refuse "$(model 'LEAK_TRIAL=1')" "in a home holding a v0.1.0 trial file dated today the release binary does not refuse approving"
case_ "y reading-the-status-rewrites-a-trial-file" refuse "$(model 'TOUCHES_TRIAL=1')" "changed or removed that file"
case_ "aa fresh-home-headline-not-free" refuse "$(model 'HEADLINE=licence')" "as-shipped start state"
case_ "ab a-second-paid-surface" refuse "$(model 'PAID_EXTRA=1')" "something besides approving would need a licence"
case_ "ac status-schema-v3" refuse "$(model 'SCHEMA=3')" "as-shipped start state"
case_ "ad approve-refused-not-for-want-of-pro" refuse "$(model "REASON='Approving a version needs a Team licence.'")" "does not refuse approving a version for want of a Pro licence"
case_ "ae approve-allowed-with-no-licence" refuse "$(model 'FRESH_ALLOW=1')" "in a fresh home the release binary does not refuse approving"

# u, v: the positive controls, which run only when a debug binary is given.
control_case() { # <label> <expect> <knobs for the debug stand-in> [the refusal must contain]
  local bin dbg got out=$work/$1/out.txt ok=0
  bin=$(stand_in "$1" "$(model '')")
  dbg=$(stand_in "$1-debug" "$(model "$3")")
  if check_release_binary "$bin" "$work/$1/check" "$dbg" >"$out" 2>&1; then got=pass; else got=refuse; fi
  if [ "$got" = "$2" ]; then
    ok=1
    if [ "$got" = refuse ] && [ -n "${4:-}" ] && ! grep '^refused: ' "$out" | grep -q -F -- "$4"; then
      ok=0
      got="refuse, but not for: $4"
    fi
  fi
  verdict "$1" "$ok" "$2${4:+ (for: $4)}" "$got" "$out"
}
control_case "u debug-seams-act" pass 'SEAM_PUB=1 SEAM_GATE=1 SEAM_NOKEYS=1'
control_case "v debug-seams-do-nothing" refuse '' "positive control failed"
control_case "w debug-only-the-key-seam-acts" refuse 'SEAM_PUB=1' "the paid-gate seam did not turn the gate off"

# z: the seam list against the source. Every THRULINE_* name that the licence
# code reads from the environment is a debug-only seam (licence_cmd.rs: "No
# environment variable or flag changes either in a release build").
from_source=$(grep -h -o 'env::var\(_os\)\{0,1\}("THRULINE_[A-Z0-9_]*")' \
  "$root/crates/cli/src/licence_cmd.rs" "$root"/crates/licence/src/*.rs \
  | grep -o 'THRULINE_[A-Z0-9_]*' | sort -u | tr '\n' ' ')
# shellcheck disable=SC2086 # the list is split into its names on purpose
in_lib=$(printf '%s\n' ${RELEASE_SEAM_VARS:-} | sort -u | tr '\n' ' ')
if [ -n "$from_source" ] && [ "$from_source" = "$in_lib" ]; then z=1; else z=0; fi
verdict "z seam-list-matches-source" "$z" "lib.sh's RELEASE_SEAM_VARS = [ $from_source]" "[ $in_lib]"

printf '%s/%s cases right\n' "$right" "$((right + wrong))"
[ "$wrong" -eq 0 ]
