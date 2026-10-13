# shellcheck shell=bash
# release/macos/lib.sh — the steps of the signed, notarized macOS release (T-DIST-1).
#
# Sourced, never run. `release.sh` runs these steps in order against the
# founder's Developer ID identity and notary profile; `release-selftest.sh`
# runs every one that does not need them — ad-hoc signing (`--sign -`),
# stubbed `security` / `xcrun` for the refusals — through THESE functions, so
# a step the self-test passes is the step the release runs, not a copy of it.
#
# Written for the /bin/bash that ships with macOS (3.2): no associative
# arrays, no mapfile, no ${var,,}.
#
# Secrets (docs/loop/dist/SECRETS.md): nothing here reads, writes or prints a
# credential. `security find-identity` is read-only; its identity NAMES are
# never echoed — only the SHA-1 of the one chosen. `codesign` output, which
# carries the name, goes to evidence files under the (gitignored) output
# directory, never to the terminal.

RELEASE_NOTARY_PROFILE=thruline-notary
RELEASE_MIN_FREE_GIB=8
RELEASE_EXIT_FAILED=1
RELEASE_EXIT_BLOCKED=3
# The prefix of the only kind of identity Apple notarizes for distribution
# outside the App Store.
RELEASE_DEVID_PREFIX='Developer ID Application:'
RELEASE_APPLEDEV_PREFIX='Apple Development:'
# The shipped feature set: `--no-default-features`, as the Homebrew formula's
# test block states ("The shipped binary is `--no-default-features`") and
# dist-workspace.toml sets (`default-features = false`). The README's default
# build carries the ORT backend, which "ships nowhere" (RELEASING.md).
RELEASE_FEATURES='--no-default-features'
# Every debug-only seam the licence code reads (crates/cli/src/licence_cmd.rs,
# all `cfg(debug_assertions)`; T-DIST-4's test-key seam was the first). Written
# here so the release binary can be checked for their absence; release.sh never
# sets one. check_release_binary greps the binary for every name and runs the
# binary with each one set (seam_value). check-release-binary-cases.sh (case z)
# holds this list to the source: a seam added there and not here fails it.
RELEASE_SEAM_VARS='THRULINE_LICENCE_DEV_PUBLIC_KEY THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS THRULINE_PAID_GATE_DEV THRULINE_LICENCE_DEV_AFTER_RENAME'
# The one production licence key a release must trust (FI-35, D-239), by key id.
RELEASE_PRODUCTION_KEY_ID=3ef3540d43dbf105

say() { printf '%s\n' "$*"; }
refuse() { printf 'refused: %s\n' "$*"; }

# A path as a person reads it: `~/…` under HOME, so no output carries the home path.
shown() {
  # shellcheck disable=SC2088 # the literal ~ is the point: it is printed, not expanded
  case $1 in
    "$HOME"/*) printf '~/%s' "${1#"$HOME"/}" ;;
    "$HOME") printf '~' ;;
    *) printf '%s' "$1" ;;
  esac
}

# Replace the home path in a stream, for anything captured from a tool.
redact_home() { sed "s#$HOME#~#g"; }

# ── the Apple tools, by absolute path (T-DIST-1-F1 c) ─────────────────────
#
# On a real run every Apple tool is called by its absolute path, so a
# `security` or `xcrun` earlier on PATH (by accident or otherwise) cannot
# answer the preflight and cannot be handed a dmg. Only self-test mode calls
# them by name, which is how release-selftest.sh puts its stand-ins in front.
if [ "${THRULINE_RELEASE_SELFTEST:-}" = 1 ]; then
  SECURITY=security XCRUN=xcrun CODESIGN=codesign HDIUTIL=hdiutil SPCTL=spctl
else
  SECURITY=/usr/bin/security XCRUN=/usr/bin/xcrun CODESIGN=/usr/bin/codesign
  HDIUTIL=/usr/bin/hdiutil SPCTL=/usr/sbin/spctl
fi

# ── the founder's steps (docs/loop/dist/BLOCKED.md B-1) ─────────────────────

# The one source of the text is founder-steps.txt beside this file.
RELEASE_LIB_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

founder_steps() {
  local f=$RELEASE_LIB_DIR/founder-steps.txt
  if [ ! -r "$f" ]; then
    refuse "the steps to unblock this are missing from this checkout ($(shown "$f")). They are also in docs/loop/dist/BLOCKED.md, section B-1."
    return 1
  fi
  grep -v '^#' "$f"
}

# ── preflight ───────────────────────────────────────────────────────────────

# The workspace version in <root>/Cargo.toml's [workspace.package].
workspace_version() {
  awk '
    /^\[workspace\.package\]/ { s = 1; next }
    /^\[/ { s = 0 }
    s && /^version[ \t]*=/ { sub(/^[^"]*"/, ""); sub(/".*$/, ""); print; exit }
  ' "$1/Cargo.toml"
}

# The repository URL, read from Cargo.toml at run time and never written here.
workspace_repository() {
  awk '
    /^\[workspace\.package\]/ { s = 1; next }
    /^\[/ { s = 0 }
    s && /^repository[ \t]*=/ { sub(/^[^"]*"/, ""); sub(/".*$/, ""); print; exit }
  ' "$1/Cargo.toml"
}

# preflight_tree <root> <version> — a clean tree whose HEAD is the version's commit.
preflight_tree() {
  local root=$1 version=$2 dirty head tagged cargo_version
  if ! git -C "$root" rev-parse --git-dir >/dev/null 2>&1; then
    refuse "$(shown "$root") is not a git checkout. A release is built from a clean checkout only."
    return $RELEASE_EXIT_FAILED
  fi
  dirty=$(git -C "$root" status --porcelain --untracked-files=normal | wc -l | tr -d ' ')
  if [ "$dirty" != 0 ]; then
    refuse "the tree has $dirty changed or untracked path(s). A release is built from a clean checkout only; commit or move them aside first."
    return $RELEASE_EXIT_FAILED
  fi
  cargo_version=$(workspace_version "$root")
  if [ "$cargo_version" != "$version" ]; then
    refuse "you asked for $version, but Cargo.toml says ${cargo_version:-nothing}. Bump the workspace version and tag it first (RELEASING.md step 1)."
    return $RELEASE_EXIT_FAILED
  fi
  if ! tagged=$(git -C "$root" rev-parse -q --verify "refs/tags/v$version^{commit}" 2>/dev/null); then
    refuse "there is no tag v$version. Tag the release commit first, then run this again: git tag v$version"
    return $RELEASE_EXIT_FAILED
  fi
  head=$(git -C "$root" rev-parse HEAD)
  if [ "$head" != "$tagged" ]; then
    refuse "HEAD is not the commit tagged v$version. Check out v$version and run again."
    return $RELEASE_EXIT_FAILED
  fi
  say "preflight: the tree is clean and HEAD is v$version (commit ${head:0:12})."
}

# Free space, in whole GiB, on the volume holding <dir>.
free_gib() {
  if [ "${THRULINE_RELEASE_SELFTEST:-}" = 1 ] && [ -n "${THRULINE_RELEASE_FREE_GIB:-}" ]; then
    printf '%s' "$THRULINE_RELEASE_FREE_GIB" # self-test seam only
    return
  fi
  df -k "$1" | awk 'NR == 2 { print int($4 / 1048576) }'
}

preflight_disk() {
  local gib
  gib=$(free_gib "$1")
  if [ "${gib:-0}" -lt "$RELEASE_MIN_FREE_GIB" ]; then
    refuse "only ${gib:-0} GiB is free on the build volume; a release build needs at least $RELEASE_MIN_FREE_GIB GiB. Free some space and run again."
    return $RELEASE_EXIT_FAILED
  fi
  say "preflight: $gib GiB free (at least $RELEASE_MIN_FREE_GIB needed)."
}

# The valid codesigning identities, one per line, as "<SHA-1> <name>".
codesigning_identities() {
  "$SECURITY" find-identity -v -p codesigning 2>/dev/null \
    | sed -n 's/^ *[0-9][0-9]*) \([0-9A-Fa-f]\{40\}\) "\(.*\)"$/\1 \2/p'
}

# The name of the identity whose SHA-1 is <sha>, from the list on stdin.
identity_name_for() {
  local want line
  want=$(printf '%s' "$1" | tr '[:lower:]' '[:upper:]')
  while IFS= read -r line; do
    if [ "$(printf '%s' "${line%% *}" | tr '[:lower:]' '[:upper:]')" = "$want" ]; then
      printf '%s' "${line#* }"
      return 0
    fi
  done
  return 1
}

APPLEDEV_WHY="An Apple Development certificate signs apps for your own devices. Apple will not notarize anything signed with it for download, so a person who downloads it would still see the warning that the developer cannot be verified. A release needs a Developer ID Application identity."

# preflight_identity — sets RELEASE_IDENTITY to the SHA-1 of the chosen
# Developer ID Application identity. Prints the SHA-1, never the name.
preflight_identity() {
  local list name line
  RELEASE_IDENTITY=
  list=$(codesigning_identities)
  if [ -n "${THRULINE_CODESIGN_IDENTITY:-}" ]; then
    if ! name=$(printf '%s\n' "$list" | identity_name_for "$THRULINE_CODESIGN_IDENTITY"); then
      refuse "THRULINE_CODESIGN_IDENTITY names an identity that is not a valid codesigning identity on this machine."
      return $RELEASE_EXIT_BLOCKED
    fi
    case $name in
      "$RELEASE_DEVID_PREFIX"*) RELEASE_IDENTITY=$THRULINE_CODESIGN_IDENTITY ;;
      "$RELEASE_APPLEDEV_PREFIX"*)
        refuse "THRULINE_CODESIGN_IDENTITY names an Apple Development identity. $APPLEDEV_WHY"
        return $RELEASE_EXIT_BLOCKED ;;
      *)
        refuse "THRULINE_CODESIGN_IDENTITY names an identity that is not a Developer ID Application identity. A release needs one."
        return $RELEASE_EXIT_BLOCKED ;;
    esac
  else
    while IFS= read -r line; do
      [ -n "$line" ] || continue
      case ${line#* } in
        "$RELEASE_DEVID_PREFIX"*) RELEASE_IDENTITY=${line%% *}; break ;;
      esac
    done <<EOF
$list
EOF
    if [ -z "$RELEASE_IDENTITY" ]; then
      if printf '%s\n' "$list" | grep -q " $RELEASE_APPLEDEV_PREFIX"; then
        refuse "the only codesigning identity on this machine is an Apple Development one. $APPLEDEV_WHY"
      else
        refuse "no Developer ID Application identity was found on this machine."
      fi
      return $RELEASE_EXIT_BLOCKED
    fi
  fi
  say "preflight: a Developer ID Application identity was found (SHA-1 $RELEASE_IDENTITY)."
}

# preflight_profile <evidence dir> — the notary profile exists and Apple accepts it.
preflight_profile() {
  if ! "$XCRUN" notarytool history --keychain-profile "$RELEASE_NOTARY_PROFILE" \
      >"$1/notary-history.txt" 2>&1; then
    refuse "the notary profile $RELEASE_NOTARY_PROFILE is not stored on this machine, or Apple refused its credentials (xcrun notarytool history failed; its output is in $(shown "$1")/notary-history.txt)."
    return $RELEASE_EXIT_BLOCKED
  fi
  say "preflight: the notary profile $RELEASE_NOTARY_PROFILE answered."
}

# ── build ───────────────────────────────────────────────────────────────────

host_triple() {
  case $(uname -m) in
    arm64) printf 'aarch64-apple-darwin' ;;
    x86_64) printf 'x86_64-apple-darwin' ;;
    *) printf 'unknown' ;;
  esac
}

target_dir_for() { printf '%s' "${CARGO_TARGET_DIR:-$1/target}"; }

# build_release <root> <log> [universal] — prints the built binary's path on the last line.
build_release() {
  local root=$1 log=$2 universal=${3:-} tdir t
  tdir=$(target_dir_for "$root")
  if [ "$universal" = universal ]; then
    for t in aarch64-apple-darwin x86_64-apple-darwin; do
      if ! rustup target list --installed 2>/dev/null | grep -qx "$t"; then
        refuse "--universal needs the $t Rust target, which is not installed. This script never installs one: rustup target add $t, or run without --universal."
        return $RELEASE_EXIT_FAILED
      fi
    done
    for t in aarch64-apple-darwin x86_64-apple-darwin; do
      if ! (cd "$root" && CARGO_INCREMENTAL=0 cargo build --release --locked -p thruline-cli \
          $RELEASE_FEATURES --target "$t") >>"$log" 2>&1; then
        refuse "cargo build for $t failed; the last lines of $(shown "$log"):"
        tail -n 15 "$log" | redact_home
        return $RELEASE_EXIT_FAILED
      fi
    done
    mkdir -p "$tdir/universal2-apple-darwin/release"
    lipo -create -output "$tdir/universal2-apple-darwin/release/thruline" \
      "$tdir/aarch64-apple-darwin/release/thruline" "$tdir/x86_64-apple-darwin/release/thruline" || return $RELEASE_EXIT_FAILED
    printf '%s\n' "$tdir/universal2-apple-darwin/release/thruline"
    return 0
  fi
  if ! (cd "$root" && CARGO_INCREMENTAL=0 cargo build --release --locked -p thruline-cli \
      $RELEASE_FEATURES) >>"$log" 2>&1; then
    refuse "cargo build failed; the last lines of $(shown "$log"):"
    tail -n 15 "$log" | redact_home
    return $RELEASE_EXIT_FAILED
  fi
  printf '%s\n' "$tdir/release/thruline"
}

# ── release-binary checks ───────────────────────────────────────────────────

# make_test_licence <dir> — a licence signed by a key generated here, now, in <dir>.
# Prints the public key's hex. The key never leaves <dir>, which is scratch.
make_test_licence() {
  local dir=$1 pub keyid payload sig
  mkdir -p "$dir"
  openssl genpkey -algorithm ed25519 -out "$dir/test-key.pem" 2>/dev/null || return 1
  chmod 600 "$dir/test-key.pem"
  pub=$(openssl pkey -in "$dir/test-key.pem" -pubout -outform DER 2>/dev/null | tail -c 32 | xxd -p -c 64)
  [ ${#pub} -eq 64 ] || return 1
  # The key id: the first 8 bytes of SHA-256("thruline-licence-key-v1\0" || key),
  # as crates/licence/src/lib.rs defines it.
  keyid=$({ printf 'thruline-licence-key-v1\000'; printf '%s' "$pub" | xxd -r -p; } \
    | shasum -a 256 | cut -c1-16)
  payload='{"format":1,"licence_id":"01J8ZQ4Y6T3M2N1P0R9S8T7V6W","product":"thruline","plan":"pro","issued_at":"2026-09-01T00:00:00Z","expires_at":"2099-01-01T00:00:00Z"}'
  printf '%s' "$payload" >"$dir/payload.bin"
  openssl pkeyutl -sign -inkey "$dir/test-key.pem" -rawin -in "$dir/payload.bin" -out "$dir/sig.bin" 2>/dev/null || return 1
  # The signature is checked here, independently of any thruline binary.
  openssl pkey -in "$dir/test-key.pem" -pubout -out "$dir/test-pub.pem" 2>/dev/null || return 1
  openssl pkeyutl -verify -pubin -inkey "$dir/test-pub.pem" -rawin -in "$dir/payload.bin" \
    -sigfile "$dir/sig.bin" >/dev/null 2>&1 || return 1
  sig=$(xxd -p -c 256 "$dir/sig.bin")
  printf 'ThruLine licence\nkey: %s\npayload: %s\nsignature: %s\n' "$keyid" "$payload" "$sig" >"$dir/licence.txt"
  printf '%s\n' "$pub"
}

TEST_LICENCE_ID=01J8ZQ4Y6T3M2N1P0R9S8T7V6W

# licence_status_json <bin> <home> [VAR=value ...] — `licence status --json` in a
# throwaway home, with nothing from this shell's environment but PATH, the home
# and the assignments given (a debug-only seam, when one is being tried).
licence_status_json() {
  local bin=$1 home=$2
  shift 2
  env -i PATH=/usr/bin:/bin HOME="$home" THRULINE_HOME="$home/.thruline" "$@" "$bin" licence status --json
}

# The as-shipped answers of v0.2 (C20, no trial): what `licence status --json`
# says in a home with no licence. Read from crates/cli/src/licence_cmd.rs
# (`FREE_STATUS`, `status_body`), crates/cli/src/json_out.rs (`LICENCE_STATUS`,
# version 4) and crates/licence/src/lib.rs (`Surface::needs`, `PAID`, `SOLD`).
RELEASE_STATUS_SHAPE=thruline.licence.status
RELEASE_STATUS_VERSION=4
RELEASE_FREE_SENTENCE='Free. Approving a version needs a Pro licence.'
RELEASE_APPROVE_NEEDS='Approving a version needs a Pro licence.'

# leftover_trial_file <home> — the file a v0.1.0 binary wrote when its 14-day
# approval trial started, in the shape it wrote, dated today, so it is one that
# v0.1.0 would still have read as an open trial. v0.2 has no trial: this file
# must never open approving, and must be left exactly as it is (C20).
leftover_trial_file() {
  mkdir -p "$1/.thruline"
  printf '{\n  "format": 1,\n  "started_on": "%s",\n  "created_at": "%sT00:00:00Z"\n}\n' \
    "$(date -u +%Y-%m-%d)" "$(date -u +%Y-%m-%d)" >"$1/.thruline/trial.json"
}

# A throwaway home holding the test licence and a v0.1.0 leftover trial file
# dated today: neither may allow approving in a release binary.
licence_home() {
  local home=$1 licence=$2
  mkdir -p "$home/.thruline"
  cp "$licence" "$home/.thruline/licence.txt"
  leftover_trial_file "$home"
}

# key_id_probe_home <home> <test licence> <key id> — a home holding the test
# licence with its key line naming <key id>. The signature is the test key's, so
# no binary can accept it; what the binary answers says whether it trusts a key
# with that id: `tampered` (the key was found and the signature does not match
# it) or `unknown_key` (no trusted key has that id). crates/licence `verify`
# looks the key up by id before it checks the signature.
key_id_probe_home() {
  mkdir -p "$1/.thruline"
  sed "s/^key: .*/key: $3/" "$2" >"$1/.thruline/licence.txt"
}

# The value that makes a debug-only seam act (crates/cli/src/licence_cmd.rs).
# A name with no entry here is set to 1.
seam_value() { # <seam> <test public key hex> <work dir>
  case $1 in
    THRULINE_LICENCE_DEV_PUBLIC_KEY) printf '%s' "$2" ;;
    THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS) printf 1 ;;
    THRULINE_PAID_GATE_DEV) printf off ;;
    # Read only by `licence install`, after its rename; a path that does not
    # exist, so it can only be observed through the gate and the key count.
    THRULINE_LICENCE_DEV_AFTER_RENAME) printf '%s' "$3/after-rename-seam" ;;
    *) printf 1 ;;
  esac
}

# The seam as a person reads it in a refusal: the test key is not printed.
seam_label() { # <seam> <value>
  case $1 in
    THRULINE_LICENCE_DEV_PUBLIC_KEY) printf '%s=<the test key>' "$1" ;;
    *) printf '%s=%s' "$1" "$2" ;;
  esac
}

json_raw() { plutil -extract "$2" raw -o - "$1" 2>/dev/null; }
# json_has <file> <key path> — the key is there (with any value, null included).
json_has() { plutil -type "$2" "$1" >/dev/null 2>&1; }

# free_and_refused <status file> <what this home is> — refuse (return 1) unless
# the answer is v0.2's "Free, no licence": the v4 shape, licence `absent`,
# headline `free` with its one sentence, no `trial` field, approving refused for
# want of Pro (status `absent`), and approving the one paid surface, sold as Pro
# only — so everything else works without a licence. The approve route answers
# `403 licence_required` with this same entitlement's status, reason and enable
# (crates/cli/src/uiserve.rs, through licence_cmd::approve_version).
free_and_refused() {
  local st=$1 what=$2 got needs
  got="shape '$(json_raw "$st" shape)' v$(json_raw "$st" schema_version), licence '$(json_raw "$st" status)', headline '$(json_raw "$st" headline)'"
  if [ "$(json_raw "$st" shape)" != "$RELEASE_STATUS_SHAPE" ] \
    || [ "$(json_raw "$st" schema_version)" != "$RELEASE_STATUS_VERSION" ] \
    || [ "$(json_raw "$st" status)" != absent ] \
    || [ "$(json_raw "$st" headline)" != free ] \
    || [ "$(json_raw "$st" sentence)" != "$RELEASE_FREE_SENTENCE" ]; then
    refuse "in $what the release binary answers $got, not the as-shipped start state (\"Free, no licence\": $RELEASE_STATUS_SHAPE v$RELEASE_STATUS_VERSION, licence 'absent', headline 'free', \"$RELEASE_FREE_SENTENCE\")."
    return 1
  fi
  if json_has "$st" trial; then
    refuse "in $what the release binary's licence status has a trial field. v0.2 has no trial (C20), and the as-shipped start state is \"Free, no licence\"."
    return 1
  fi
  case $(json_raw "$st" entitlements.approve_version.reason) in
    "$RELEASE_APPROVE_NEEDS"*) needs=yes ;;
    *) needs=no ;;
  esac
  if [ "$(json_raw "$st" entitlements.approve_version.allowed)" != false ] \
    || [ "$(json_raw "$st" entitlements.approve_version.status)" != absent ] || [ "$needs" != yes ]; then
    refuse "in $what the release binary does not refuse approving a version for want of a Pro licence (approve_version allowed '$(json_raw "$st" entitlements.approve_version.allowed)', status '$(json_raw "$st" entitlements.approve_version.status)')."
    return 1
  fi
  if [ "$(json_raw "$st" paid.0.surface)" != approve_version ] || json_has "$st" paid.1 \
    || [ "$(json_raw "$st" paid.0.plans.0)" != pro ] || json_has "$st" paid.0.plans.1; then
    refuse "in $what the release binary's paid surfaces are not exactly one, approving a version, sold as Pro only; something besides approving would need a licence."
    return 1
  fi
}

# read_licence_status <bin> <home> <out> <what was set> [VAR=value ...] — run
# `licence status --json` and refuse (return 1) on anything this check cannot
# read. Round 2 (card INFRA-RELEASE-CHECK): stdout alone is parsed, and a probe
# that fails or prints nothing, output that is not JSON, or a status without a
# boolean `paid_gate`, a string-or-null `technical.licence_id` and an integer
# `technical.keys_built_in` all mean "could not determine".
read_licence_status() {
  local bin=$1 home=$2 st=$3 label=$4
  shift 4
  if ! licence_status_json "$bin" "$home" "$@" >"$st" 2>"$st.stderr"; then
    refuse "could not read the release binary's licence status ($label): \`licence status --json\` exited non-zero. A check that cannot read the answer refuses."
    return 1
  fi
  if [ ! -s "$st" ]; then
    refuse "could not read the release binary's licence status ($label): \`licence status --json\` printed nothing on stdout."
    return 1
  fi
  if ! plutil -convert json -o /dev/null "$st" >/dev/null 2>&1; then
    refuse "could not read the release binary's licence status ($label): its stdout is not JSON."
    return 1
  fi
  if [ "$(plutil -type paid_gate "$st" 2>/dev/null)" != bool ]; then
    refuse "could not read the release binary's licence status ($label): it has no boolean paid_gate."
    return 1
  fi
  case $(plutil -type technical.licence_id "$st" 2>/dev/null) in
    string | '(any)' | null) ;; # '(any)' is JSON null: the binary names no licence
    *)
      refuse "could not read the release binary's licence status ($label): it has no technical.licence_id that is a string or null."
      return 1 ;;
  esac
  if [ "$(plutil -type technical.keys_built_in "$st" 2>/dev/null)" != integer ]; then
    refuse "could not read the release binary's licence status ($label): it has no integer technical.keys_built_in."
    return 1
  fi
}

# check_release_binary <bin> <work dir> [debug bin] — the binary runs, it ships
# with the paid gate ON and trusts the production key, a fresh home is the
# as-shipped start state ("Free, no licence": approving refused without Pro,
# nothing else paid, no trial — C20), a v0.1.0 trial file opens nothing, and no
# debug-only seam is in it: no seam's name is in the file, and setting any seam
# changes nothing. Returns 1 on any failure.
# Card RELEASE-CHECK-GATE-ON (round 3, D-244): a gate-off or keyless build used
# to pass with a warning; it is refused (D-050: that "pass" encoded the defect).
check_release_binary() {
  local bin=$1 work=$2 debug=${3:-} version pub s v label st keys n gate id allowed status before
  local key=$RELEASE_PRODUCTION_KEY_ID
  if ! version=$("$bin" --version 2>&1) || ! printf '%s' "$version" | grep -q '^thruline'; then
    refuse "the built binary did not run: $bin --version"
    return 1
  fi
  say "check: it runs: $version"
  for s in $RELEASE_SEAM_VARS; do
    if grep -a -q -F "$s" "$bin"; then
      refuse "the release binary contains the name of a debug-only seam ($s). It must not."
      return 1
    fi
  done
  say "check: no debug-only seam's name is in the binary ($RELEASE_SEAM_VARS)."
  if ! pub=$(make_test_licence "$work/licence"); then
    refuse "could not make a test-signed licence with openssl (it needs Ed25519: OpenSSL 3, or LibreSSL 3.7+)."
    return 1
  fi
  key_id_probe_home "$work/home-key-probe" "$work/licence/licence.txt" "$key"
  if [ -n "$debug" ] && [ -x "$debug" ]; then
    # Positive controls: where the seams exist (a debug binary), the test licence
    # is accepted with the key seam set, each seam value used below makes its
    # seam act, and the key-id probe tells a trusted id from an unknown one. So
    # the release checks below are not vacuous.
    licence_home "$work/home-debug" "$work/licence/licence.txt"
    licence_status_json "$debug" "$work/home-debug" \
      "THRULINE_LICENCE_DEV_PUBLIC_KEY=$pub" >"$work/status-debug.json" 2>"$work/status-debug.stderr"
    if [ "$(json_raw "$work/status-debug.json" technical.licence_id)" != "$TEST_LICENCE_ID" ]; then
      refuse "the positive control failed: a debug binary with the seam set did not accept the test licence, so the release check below would prove nothing."
      return 1
    fi
    licence_status_json "$debug" "$work/home-debug" "THRULINE_PAID_GATE_DEV=$(seam_value THRULINE_PAID_GATE_DEV "$pub" "$work")" \
      >"$work/status-debug-gate.json" 2>/dev/null
    licence_status_json "$debug" "$work/home-debug" "THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS=$(seam_value THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS "$pub" "$work")" \
      >"$work/status-debug-nokeys.json" 2>/dev/null
    licence_status_json "$debug" "$work/home-key-probe" >"$work/status-debug-key-probe.json" 2>/dev/null
    if [ "$(json_raw "$work/status-debug-gate.json" paid_gate)" != false ] \
      || [ "$(json_raw "$work/status-debug-nokeys.json" technical.keys_built_in)" != 0 ] \
      || [ "$(json_raw "$work/status-debug-key-probe.json" status)" != tampered ]; then
      refuse "the positive control failed: in a debug binary, the paid-gate seam did not turn the gate off, the no-built-in-keys seam did not remove the keys, or a licence naming key $key was not answered 'tampered'; the release checks below would prove nothing."
      return 1
    fi
    say "check: positive control: a debug binary accepts the test licence with the key seam set, its gate goes off and its keys go to 0 with those seams set, and it answers 'tampered' to a licence naming key $key."
  else
    say "check: positive control not run (no debug binary given); the test licence's signature was checked with openssl instead."
  fi

  # 1. A fresh home, no seam: the gate is on, a production key is built in, and
  #    the answer is the as-shipped start state, "Free, no licence" (C20: no
  #    trial), which reading the status does not change.
  mkdir -p "$work/home-fresh"
  st=$work/status-fresh.json
  read_licence_status "$bin" "$work/home-fresh" "$st" "a fresh home" || return 1
  if [ "$(json_raw "$st" paid_gate)" != true ]; then
    refuse "the release binary's paid gate is off (paid_gate: false in a fresh home). A release ships with the gate on (D-238); a build with PAID_GATE false or with no licence key is not a release."
    return 1
  fi
  keys=$(json_raw "$st" technical.keys_built_in)
  if [ "$keys" -lt 1 ]; then
    refuse "the release binary carries no production licence key (keys_built_in: $keys). A release trusts the production key $key."
    return 1
  fi
  free_and_refused "$st" "a fresh home" || return 1
  if [ -e "$work/home-fresh/.thruline/trial.json" ]; then
    refuse "in a fresh home, reading the licence status wrote a trial file. v0.2 has no trial (C20) and writes none."
    return 1
  fi
  say "check: a fresh home: paid gate on, $keys production licence key(s) built in, \"$RELEASE_FREE_SENTENCE\" ($RELEASE_STATUS_SHAPE v$RELEASE_STATUS_VERSION, licence 'absent', no trial field, nothing written); approving a version is refused for want of Pro, and it is the one paid surface."

  # 1b. A home holding only a v0.1.0 leftover trial file dated today: it never
  #     opens approving, the answer is still "Free, no licence", and the file
  #     is left exactly as it was.
  leftover_trial_file "$work/home-leftover"
  before=$(shasum -a 256 <"$work/home-leftover/.thruline/trial.json")
  st=$work/status-leftover.json
  read_licence_status "$bin" "$work/home-leftover" "$st" "a home with a v0.1.0 trial file" || return 1
  free_and_refused "$st" "a home holding a v0.1.0 trial file dated today" || return 1
  if [ "$before" != "$(shasum -a 256 <"$work/home-leftover/.thruline/trial.json" 2>/dev/null)" ]; then
    refuse "in a home holding a v0.1.0 trial file, reading the licence status changed or removed that file. C20 leaves it untouched."
    return 1
  fi
  say "check: a v0.1.0 trial file dated today opens nothing: still \"$RELEASE_FREE_SENTENCE\", approving refused, and the file is left byte for byte as it was."

  # 2. The production key, by id.
  st=$work/status-key-probe.json
  read_licence_status "$bin" "$work/home-key-probe" "$st" "a licence naming key $key" || return 1
  status=$(json_raw "$st" status)
  case $status in
    tampered) say "check: the release binary trusts the production licence key $key (a licence naming it with another key's signature is answered 'tampered', not 'unknown_key')." ;;
    unknown_key)
      refuse "the release binary does not trust the production licence key $key: a licence naming that key id is answered 'unknown_key'."
      return 1 ;;
    *)
      refuse "could not read whether the release binary trusts the production licence key $key: a licence naming it is answered '${status:-?}' (expected 'tampered')."
      return 1 ;;
  esac

  # 3. The test-signed licence, with no seam set, each seam set, and all of them:
  #    the licence is never accepted, the gate stays on, the key count stays
  #    $keys, and approving stays refused (this home also holds a v0.1.0 trial
  #    file dated today, which must open nothing either).
  licence_home "$work/home-release" "$work/licence/licence.txt"
  n=0
  for s in - $RELEASE_SEAM_VARS '*'; do
    n=$((n + 1))
    case $s in
      -) label="no seam"; set -- ;;
      '*')
        label="every seam"
        set --
        for v in $RELEASE_SEAM_VARS; do set -- "$@" "$v=$(seam_value "$v" "$pub" "$work")"; done ;;
      *) v=$(seam_value "$s" "$pub" "$work"); label=$(seam_label "$s" "$v"); set -- "$s=$v" ;;
    esac
    # The first run keeps round 2's file name; release-selftest.sh reads that home.
    if [ "$n" = 1 ]; then st=$work/status-release.json; else st=$work/status-release-$n.json; fi
    read_licence_status "$bin" "$work/home-release" "$st" "with $label set" "$@" || return 1
    id=$(json_raw "$st" technical.licence_id)
    gate=$(json_raw "$st" paid_gate)
    allowed=$(json_raw "$st" entitlements.approve_version.allowed)
    if [ "$id" = "$TEST_LICENCE_ID" ]; then
      refuse "the release binary accepted a test-signed licence with $label set. A debug-only seam is in a release build."
      return 1
    fi
    if [ "$gate" != true ]; then
      refuse "with $label set, the release binary's paid gate is off. A debug-only seam is in a release build."
      return 1
    fi
    if [ "$(json_raw "$st" technical.keys_built_in)" != "$keys" ]; then
      refuse "with $label set, the release binary trusts $(json_raw "$st" technical.keys_built_in) licence key(s) instead of $keys. A debug-only seam is in a release build."
      return 1
    fi
    if [ "$allowed" != false ]; then
      refuse "with $label set, the release binary allows approve_version under a test-signed licence (beside a v0.1.0 trial file). A debug-only seam, or a trial, is in a release build."
      return 1
    fi
  done
  say "check: a test-signed licence is rejected with no seam set, with each of the $((n - 2)) seams set, and with all of them: the gate stays on, the key count stays $keys, approve_version stays denied."
}

# ── codesign ────────────────────────────────────────────────────────────────

# The codesign flags for a binary. Ad-hoc (`-`) takes no secure timestamp:
# asking for one contacts Apple's timestamp server, which the self-test never does.
codesign_binary_flags() {
  if [ "$1" = - ]; then
    printf '%s' "--force --timestamp=none --options runtime"
  else
    printf '%s' "--force --timestamp --options runtime"
  fi
}

codesign_dmg_flags() {
  if [ "$1" = - ]; then printf '%s' "--force --timestamp=none"; else printf '%s' "--force --timestamp"; fi
}

# assert_signature <file> <adhoc|developer-id> <evidence file> <runtime|no-runtime>
assert_signature() {
  local file=$1 mode=$2 ev=$3 runtime=$4 flags
  "$CODESIGN" -dvvv "$file" >"$ev" 2>&1 || { refuse "codesign -dvvv could not read $(shown "$file")."; return 1; }
  flags=$(sed -n 's/^CodeDirectory .* flags=\(0x[0-9a-f]*([^)]*)\).*/\1/p' "$ev")
  if [ "$runtime" = runtime ] && ! printf '%s' "$flags" | grep -q 'runtime'; then
    refuse "$(basename "$file") is signed without the hardened runtime (flags ${flags:-none}); notarization would reject it."
    return 1
  fi
  if [ "$mode" = adhoc ]; then
    grep -q '^Signature=adhoc' "$ev" || { refuse "$(basename "$file") is not ad-hoc signed."; return 1; }
  else
    if ! grep -q "^Authority=$RELEASE_DEVID_PREFIX" "$ev" \
      || ! grep -q '^Authority=Developer ID Certification Authority' "$ev" \
      || ! grep -q '^Authority=Apple Root CA' "$ev"; then
      refuse "$(basename "$file")'s signature does not chain Developer ID Application -> Developer ID Certification Authority -> Apple Root CA."
      return 1
    fi
    grep -q '^Timestamp=' "$ev" || { refuse "$(basename "$file") has no secure timestamp."; return 1; }
  fi
}

# sign_binary <identity SHA-1 or -> <file> <evidence dir>
# No entitlements file: the shipped (--no-default-features) binary links only
# system libraries, uses no JIT and loads no plugin, so the hardened runtime's
# defaults need no exception. release-selftest.sh runs the hardened binary to
# check that.
sign_binary() {
  local id=$1 file=$2 ev=$3 mode=developer-id
  [ "$id" = - ] && mode=adhoc
  # shellcheck disable=SC2046 # the flags are words by construction
  if ! "$CODESIGN" $(codesign_binary_flags "$id") --sign "$id" "$file" >"$ev/codesign-binary.txt" 2>&1; then
    refuse "codesign failed on $(basename "$file"); see $(shown "$ev")/codesign-binary.txt."
    return 1
  fi
  if ! "$CODESIGN" --verify --strict --verbose=2 "$file" >"$ev/codesign-verify-binary.txt" 2>&1; then
    refuse "codesign --verify --strict failed on $(basename "$file"); see $(shown "$ev")/codesign-verify-binary.txt."
    return 1
  fi
  assert_signature "$file" "$mode" "$ev/codesign-display-binary.txt" runtime || return 1
  say "codesign: $(basename "$file") signed ($mode) with the hardened runtime; codesign --verify --strict passed."
}

# ── package ─────────────────────────────────────────────────────────────────

# INSTALL.txt, inside the dmg: plain words, the exact line to paste, and why it
# asks for a password (T-DIST-1-F1 b). /usr/local/bin is on a stock Mac's PATH
# for every new Terminal window (/etc/paths lists it first), but on a new Mac
# the folder itself may not exist yet, hence `mkdir -p`.
install_text() {
  cat <<'EOF'
Installing ThruLine on this Mac

1. Open Terminal. It is in Applications, in the Utilities folder.

2. Copy this line, paste it into Terminal, and press Return:

   sudo mkdir -p /usr/local/bin && sudo cp /Volumes/ThruLine/thruline /usr/local/bin/thruline

   It asks for your Mac's password, the one you use to log in to this Mac.
   It asks because /usr/local/bin, the folder Terminal looks in for commands,
   belongs to the Mac itself, and only an administrator can add a file to it.
   Nothing shows on screen while you type the password. That is normal: type
   it and press Return. If Terminal says you are not allowed, this Mac's
   administrator has to run the line for you.

3. Open a new Terminal window (Shell, then New Window) and run:

   thruline ui

   ThruLine opens in your web browser. Nothing leaves this Mac.
EOF
}

# package_release <identity> <root> <signed binary> <version> <triple> <out dir> <evidence dir>
# Writes the dmg, the Homebrew-shaped .tar.gz, their .sha256 files and sha256.sum.
package_release() {
  local id=$1 root=$2 bin=$3 version=$4 triple=$5 out=$6 ev=$7 arch dmg stage tarroot tarball f
  arch=${triple%%-*}
  stage=$out/stage/dmg
  dmg=$out/thruline-$version-macos-$arch.dmg
  mkdir -p "$stage" "$out/stage/tar"
  cp "$bin" "$stage/thruline"
  cp "$root/LICENSE.md" "$stage/LICENSE.md"
  cp "$root/THIRD_PARTY_NOTICES.md" "$stage/THIRD_PARTY_NOTICES.md"
  install_text >"$stage/INSTALL.txt"
  if ! "$HDIUTIL" create -volname ThruLine -srcfolder "$stage" -ov -format UDZO "$dmg" \
      >"$ev/hdiutil-create.txt" 2>&1; then
    refuse "hdiutil create failed; see $(shown "$ev")/hdiutil-create.txt."
    return 1
  fi
  # shellcheck disable=SC2046
  if ! "$CODESIGN" $(codesign_dmg_flags "$id") --sign "$id" "$dmg" >"$ev/codesign-dmg.txt" 2>&1 \
      || ! "$CODESIGN" --verify --strict --verbose=2 "$dmg" >"$ev/codesign-verify-dmg.txt" 2>&1; then
    refuse "signing the dmg failed; see $(shown "$ev")/codesign-dmg.txt."
    return 1
  fi
  # The archive Homebrew's formula names: one root directory, the binary inside it.
  tarroot=thruline-cli-$triple
  tarball=$out/$tarroot.tar.gz
  mkdir -p "$out/stage/tar/$tarroot"
  cp "$bin" "$out/stage/tar/$tarroot/thruline"
  cp "$root/LICENSE.md" "$out/stage/tar/$tarroot/LICENSE.md"
  cp "$root/THIRD_PARTY_NOTICES.md" "$out/stage/tar/$tarroot/THIRD_PARTY_NOTICES.md"
  if ! (cd "$out/stage/tar" && COPYFILE_DISABLE=1 tar -czf "$tarball" "$tarroot"); then
    refuse "making $(basename "$tarball") failed."
    return 1
  fi
  (
    cd "$out" || exit 1
    : >sha256.sum
    for f in "$(basename "$dmg")" "$(basename "$tarball")"; do
      shasum -a 256 "$f" >"$f.sha256"
      cat "$f.sha256" >>sha256.sum
    done
  ) || return 1
  # shellcheck disable=SC2034 # read by release.sh and release-selftest.sh
  RELEASE_DMG=$dmg
  # shellcheck disable=SC2034
  RELEASE_TARBALL=$tarball
  say "package: $(basename "$dmg") (thruline, LICENSE.md, THIRD_PARTY_NOTICES.md, INSTALL.txt; signed) and $(basename "$tarball"), with sha256.sum."
}

# ── notarize, staple, check ─────────────────────────────────────────────────

# notarize <dmg> <evidence dir>
notarize() {
  local dmg=$1 ev=$2 status id
  "$XCRUN" notarytool submit "$dmg" --keychain-profile "$RELEASE_NOTARY_PROFILE" --wait \
    --output-format json >"$ev/notary-submit.json" 2>"$ev/notary-submit.err"
  status=$(plutil -extract status raw -o - "$ev/notary-submit.json" 2>/dev/null)
  id=$(plutil -extract id raw -o - "$ev/notary-submit.json" 2>/dev/null)
  if [ "$status" != Accepted ]; then
    if [ -n "$id" ]; then
      "$XCRUN" notarytool log "$id" --keychain-profile "$RELEASE_NOTARY_PROFILE" \
        "$ev/notary-log.json" >"$ev/notary-log.err" 2>&1 || true
      refuse "Apple did not accept the submission (status: ${status:-none}). Its log is in $(shown "$ev")/notary-log.json."
    else
      refuse "the notary submission did not complete; see $(shown "$ev")/notary-submit.err."
    fi
    return 1
  fi
  say "notarize: Apple accepted the submission (id $id)."
}

# validate_staple <dmg> <evidence file>
validate_staple() {
  "$XCRUN" stapler validate "$1" >"$2" 2>&1
}

# staple_and_check <dmg> <evidence dir>
staple_and_check() {
  local dmg=$1 ev=$2
  if ! "$XCRUN" stapler staple "$dmg" >"$ev/stapler-staple.txt" 2>&1; then
    refuse "stapler staple failed; see $(shown "$ev")/stapler-staple.txt."
    return 1
  fi
  if ! validate_staple "$dmg" "$ev/stapler-validate.txt"; then
    refuse "stapler validate failed; see $(shown "$ev")/stapler-validate.txt."
    return 1
  fi
  if ! "$SPCTL" --assess --type open --context context:primary-signature --verbose=4 "$dmg" \
      >"$ev/spctl-assess.txt" 2>&1; then
    refuse "Gatekeeper (spctl --assess) rejected the dmg; see $(shown "$ev")/spctl-assess.txt."
    return 1
  fi
  say "staple: the ticket is stapled; stapler validate and spctl --assess both accept the dmg."
}

second_account_steps() {
  cat <<EOF
The last check is yours (T-DIST-1 acceptance B). Publishing is your act:
1. Upload $(basename "$1") to an HTTPS URL (for example a release on ${2:-your repository}).
2. Log in to a second macOS user account (or use a second Mac) that has never run ThruLine.
3. In Safari or Chrome on that account, download the dmg from the HTTPS URL.
4. Double-click the dmg. It should open with no warning that the developer cannot be verified.
5. Open the dmg's INSTALL.txt and do what it says. Then, in a new Terminal window, run: thruline --version
6. Confirm no Gatekeeper dialog appeared at any step, then run: thruline ui
7. Keep a screenshot or screen recording of that account as the evidence.
EOF
}
