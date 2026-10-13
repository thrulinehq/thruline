#!/usr/bin/env bash
# Break the verifier on purpose; the tests must notice.
#
# A test suite that passes against deliberately broken code is testing nothing,
# and that is not hypothetical here: this script's second mutation originally
# left ALL 157 tests passing, which is how the missing chain-signature coverage
# was found. Each entry below disables one security check; every one must make
# the suite FAIL.
set -uo pipefail

CRATE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT
cp -r "$CRATE" "$WORK/c"; cd "$WORK/c"

fail=0
mutate () {
  local name="$1" file="$2" find="$3" repl="$4"
  cp "$file" "$file.bak"
  python3 - "$file" "$find" "$repl" <<'PY'
import sys
p, find, repl = sys.argv[1], sys.argv[2], sys.argv[3]
s = open(p).read()
assert find in s, f"mutation anchor not found in {p}: {find!r}"
open(p, 'w').write(s.replace(find, repl, 1))
PY
  if [ $? -ne 0 ]; then echo "  SKIP $name (anchor moved)"; mv "$file.bak" "$file"; return; fi
  out=$(CARGO_TARGET_DIR=/tmp/tsr-mutation cargo test --lib --test end_to_end 2>&1 | grep -cE "^test result: FAILED")
  mv "$file.bak" "$file"
  if [ "$out" -gt 0 ]; then
    echo "  CAUGHT   $name"
  else
    echo "  ESCAPED  $name   <-- nothing tests this"
    fail=1
  fi
}

echo "==> mutation check: each broken check must be caught"
mutate "merkle inclusion proof always succeeds" src/merkle.rs \
  'let computed = root_from_inclusion_proof(index, size, leaf, proof)?;' \
  'if true { return Ok(()); } let computed = root_from_inclusion_proof(index, size, leaf, proof)?;'
mutate "certificate chain signature never checked" src/x509.rs \
  'pub fn verify_signed_by(child: &Certificate, parent: &Certificate) -> Result<()> {' \
  'pub fn verify_signed_by(child: &Certificate, parent: &Certificate) -> Result<()> { if true { return Ok(()); }'
mutate "certificate validity window ignored" src/x509.rs \
  'pub fn check_validity(cert: &Certificate, at: time::OffsetDateTime) -> Result<()> {' \
  'pub fn check_validity(cert: &Certificate, at: time::OffsetDateTime) -> Result<()> { if true { return Ok(()); }'
mutate "code-signing EKU not required" src/x509.rs \
  'pub fn check_code_signing_eku(cert: &Certificate) -> Result<()> {' \
  'pub fn check_code_signing_eku(cert: &Certificate) -> Result<()> { if true { return Ok(()); }'
mutate "log entry not bound to the artifact" src/verify.rs \
  'fn body_matches_bundle(bundle: &Bundle, body: &[u8]) -> Result<bool> {' \
  'fn body_matches_bundle(bundle: &Bundle, body: &[u8]) -> Result<bool> { if true { return Ok(true); }'

echo
[ $fail -eq 0 ] && echo "all mutations caught" || { echo "SOME MUTATIONS ESCAPED"; exit 1; }
