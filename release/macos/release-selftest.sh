#!/bin/bash
# release/macos/release-selftest.sh — every step of release.sh that does not
# need the founder's credentials, run for real (T-DIST-1).
#
#   - the preflight refusals, by running release.sh itself in self-test mode
#     against a throwaway checkout, with stand-in `security` and `xcrun` on
#     PATH that answer only the read-only questions and refuse anything else;
#   - the release build (one `cargo build --release`; cargo reuses it when it
#     is already fresh, and says so in its log);
#   - the release-binary checks, including T-DIST-4's seam being absent;
#   - AD-HOC signing (`--sign -`) with the hardened runtime, and codesign --verify;
#   - the dmg, the Homebrew archive and the checksums;
#   - the notarize step's accept and reject paths, against the stand-in xcrun;
#   - `stapler validate` failing on the un-notarized dmg, as expected.
#
# It never uses a real signing identity (an Apple Development one would raise
# a keychain dialog), never talks to Apple, and never writes to any keychain.
# Its files go to $THRULINE_SELFTEST_SCRATCH (default: target/release-dist/
# selftest), one new run-<time> folder per run; nothing is deleted.
#
# Exit: 0 every case passed · 1 a case failed.
set -u
# The self-test decides what the stand-in keychain holds; a real override
# from the caller's shell must not leak into its cases.
unset THRULINE_CODESIGN_IDENTITY THRULINE_RELEASE_FREE_GIB THRULINE_RELEASE_ROOT THRULINE_RELEASE_SELFTEST
# Set in THIS shell only, never exported, before lib.sh is sourced: lib.sh then
# calls the Apple tools by name, so the notarize step below reaches the
# stand-in xcrun and never the real one. A child started by this script is a
# real run unless its own environment says otherwise.
THRULINE_RELEASE_SELFTEST=1

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=release/macos/lib.sh
. "$here/lib.sh"
root=$(git -C "$here" rev-parse --show-toplevel)
tdir=$(target_dir_for "$root")
base=${THRULINE_SELFTEST_SCRATCH:-$tdir/release-dist/selftest}
run=$base/run-$(date +%Y%m%dT%H%M%S)
mkdir -p "$run"
say "self-test: files in $(shown "$run")"

passed=0
failed=0
ok() { passed=$((passed + 1)); say "ok   $*"; }
bad() { failed=$((failed + 1)); say "FAIL $*"; }

tree_before=$(git -C "$root" status --porcelain --untracked-files=all | shasum -a 256)

# ── stand-ins for security and xcrun ────────────────────────────────────────
# Driven by files, so each case says what the "keychain" holds. Any request
# other than the read-only ones below exits 99 and is logged, so a stray
# write could not reach a real keychain even by mistake.
stubs=$run/stubs
mkdir -p "$stubs"
cat >"$stubs/security" <<'EOF'
#!/bin/bash
printf 'security %s\n' "$*" >>"$STUB_DIR/calls.log"
if [ "$1" = find-identity ] && [ "$2" = -v ] && [ "$3" = -p ] && [ "$4" = codesigning ]; then
  n=0
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    n=$((n + 1))
    printf '  %d) %s "%s"\n' "$n" "${line%% *}" "${line#* }"
  done <"$STUB_DIR/identities"
  printf '     %d valid identities found\n' "$n"
  exit 0
fi
printf 'stand-in security refuses: %s\n' "$*" >&2
exit 99
EOF
cat >"$stubs/xcrun" <<'EOF'
#!/bin/bash
printf 'xcrun %s\n' "$*" >>"$STUB_DIR/calls.log"
if [ "$1 $2" = "notarytool history" ] && [ "$3 $4" = "--keychain-profile thruline-notary" ]; then
  if [ -e "$STUB_DIR/profile-ok" ]; then printf 'No submission history.\n'; exit 0; fi
  printf 'Error: No Keychain password item found for profile: thruline-notary\n' >&2
  exit 69
fi
if [ "$1 $2" = "notarytool submit" ]; then
  printf '{"id":"00000000-stub-0000","status":"%s","message":"stand-in"}\n' "$(cat "$STUB_DIR/submit-status")"
  [ "$(cat "$STUB_DIR/submit-status")" = Accepted ]
  exit $?
fi
if [ "$1 $2" = "notarytool log" ]; then
  printf '{"status":"Invalid","issues":[{"message":"stand-in log"}]}\n' >"$6"
  exit 0
fi
printf 'stand-in xcrun refuses: %s\n' "$*" >&2
exit 99
EOF
chmod +x "$stubs/security" "$stubs/xcrun"

# Synthetic identities, assembled at run time: a SHA-1 of repeated letters,
# and names no real certificate carries.
devid_sha=$(printf 'D%.0s' 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40)
adev_sha=$(printf 'A%.0s' 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40)
stub_devid_name="$RELEASE_DEVID_PREFIX Selftest Stand-in (NOTAREALID)"
stub_adev_name="$RELEASE_APPLEDEV_PREFIX Selftest Stand-in (NOTAREALID)"

# A throwaway checkout at tag v9.9.9. Commits carry a synthetic author, never
# this machine's git identity.
fake_repo() { # <dir>
  mkdir -p "$1"
  git -C "$1" init -q
  printf '[workspace.package]\nversion = "9.9.9"\n' >"$1/Cargo.toml"
  printf 'target/\n' >"$1/.gitignore"
  git -C "$1" add Cargo.toml .gitignore
  git -C "$1" -c user.name=selftest -c user.email=selftest@example.invalid -c commit.gpgsign=false \
    commit -q -m fixture
  git -C "$1" -c user.name=selftest -c user.email=selftest@example.invalid -c tag.gpgsign=false \
    tag -a v9.9.9 -m fixture
}

# The fenced commands in docs/loop/dist/BLOCKED.md's B-1 section, sorted.
b1_commands() {
  local f=$root/docs/loop/dist/BLOCKED.md
  [ -f "$f" ] || return 0
  awk '/^## B-1/{s=1;next} /^## /{s=0} s' "$f" | awk '/^ *```/{f=!f;next} f' \
    | sed 's/^ *//' | grep -v '^$' | LC_ALL=C sort -u
}

# The commands in release.sh's printed output: lines indented by 7 spaces or
# more (founder-steps.txt's layout rule), sorted.
printed_commands() {
  grep -E '^ {7,}[^ ]' "$1" | sed 's/^ *//' | LC_ALL=C sort -u
}

# preflight_case <name> <expected exit> <expected sentence> [env assignments...]
# Runs release.sh in self-test mode in a fresh stub dir and fake checkout.
case_n=0
preflight_case() {
  local name=$1 want=$2 sentence=$3 dir repo outf rc
  shift 3
  case_n=$((case_n + 1))
  dir=$run/preflight-$case_n
  repo=$dir/repo
  mkdir -p "$dir/stub"
  : >"$dir/stub/identities"
  fake_repo "$repo"
  # the case's own setup, from its remaining arguments
  while [ $# -gt 0 ]; do
    case $1 in
      identity=devid) printf '%s %s\n' "$devid_sha" "$stub_devid_name" >>"$dir/stub/identities" ;;
      identity=appledev) printf '%s %s\n' "$adev_sha" "$stub_adev_name" >>"$dir/stub/identities" ;;
      profile=ok) : >"$dir/stub/profile-ok" ;;
      dirty) printf 'x\n' >"$repo/untracked.txt" ;;
      moved) printf 'y\n' >"$repo/later.txt"; git -C "$repo" add later.txt
             git -C "$repo" -c user.name=selftest -c user.email=selftest@example.invalid \
               -c commit.gpgsign=false commit -q -m later ;;
      free=*) export THRULINE_RELEASE_FREE_GIB=${1#free=} ;;
      override=appledev) export THRULINE_CODESIGN_IDENTITY=$adev_sha ;;
      notag) git -C "$repo" tag -d v9.9.9 >/dev/null ;;
    esac
    shift
  done
  outf=$dir/output.txt
  env STUB_DIR="$dir/stub" PATH="$stubs:$PATH" THRULINE_RELEASE_SELFTEST=1 \
    THRULINE_RELEASE_ROOT="$repo" CARGO_TARGET_DIR="$repo/target" \
    "$here/release.sh" 9.9.9 >"$outf" 2>&1
  rc=$?
  unset THRULINE_RELEASE_FREE_GIB THRULINE_CODESIGN_IDENTITY
  if [ "$rc" != "$want" ]; then
    bad "preflight: $name — exit $rc, wanted $want"
    sed 's/^/       | /' "$outf" | redact_home
    return
  fi
  if ! grep -qF -- "$sentence" "$outf"; then
    bad "preflight: $name — exit $rc as wanted, but the output lacks: $sentence"
    sed 's/^/       | /' "$outf" | redact_home
    return
  fi
  if grep -qF "Stand-in (NOTAREALID)" "$outf"; then
    bad "preflight: $name — the output echoed an identity name"
    return
  fi
  if grep -q 'refuses:' "$dir/stub/calls.log" 2>/dev/null || grep -vqE '^(security find-identity -v -p codesigning|xcrun notarytool history --keychain-profile thruline-notary)$' "$dir/stub/calls.log" 2>/dev/null; then
    bad "preflight: $name — release.sh asked the stand-ins something other than the two read-only questions"
    sed 's/^/       | /' "$dir/stub/calls.log"
    return
  fi
  if [ "$want" = 3 ]; then
    # The printed founder steps and BLOCKED.md B-1 carry the same commands,
    # character for character, in both directions (T-DIST-1-F1 a).
    local b1 printed only_b1 only_printed
    b1=$(b1_commands)
    printed=$(printed_commands "$outf")
    if [ -z "$b1" ]; then
      bad "preflight: $name — found no fenced command in BLOCKED.md's B-1 section, so the printed steps cannot be checked against it"
      return
    fi
    only_b1=$(comm -23 <(printf '%s\n' "$b1") <(printf '%s\n' "$printed"))
    only_printed=$(comm -13 <(printf '%s\n' "$b1") <(printf '%s\n' "$printed"))
    if [ -n "$only_b1" ] || [ -n "$only_printed" ]; then
      [ -z "$only_b1" ] || bad "preflight: $name — the printed steps lack B-1's command(s): $only_b1"
      [ -z "$only_printed" ] || bad "preflight: $name — the printed steps carry command(s) B-1 does not: $only_printed"
      return
    fi
    # And the three things B-1 says that are not commands.
    local phrase
    for phrase in 'Keychain Access -> Certificate Assistant' 'Your Team ID is the 10-character code' \
      'on a second macOS user account'; do
      grep -qF -- "$phrase" "$outf" || { bad "preflight: $name — the printed steps lack B-1's: $phrase"; return; }
    done
  fi
  ok "preflight: $name — exit $rc: $(grep -m1 -F -- "$sentence" "$outf" | cut -c1-140)"
}

say ""
say "── 1. preflight (release.sh in self-test mode, stand-in security/xcrun) ──"
preflight_case "no tag for the version (the refusal names the command)" 1 "then run this again: git tag v9.9.9" notag identity=devid profile=ok
preflight_case "a dirty tree" 1 "changed or untracked path" dirty identity=devid profile=ok
preflight_case "HEAD not at the version's commit" 1 "HEAD is not the commit tagged v9.9.9" moved identity=devid profile=ok
preflight_case "less than 8 GiB free" 1 "a release build needs at least 8 GiB" free=7 identity=devid profile=ok
preflight_case "no identity at all" 3 "no Developer ID Application identity was found" profile=ok
preflight_case "Apple Development only" 3 "Apple will not notarize anything signed with it" identity=appledev profile=ok
preflight_case "THRULINE_CODESIGN_IDENTITY naming an Apple Development identity" 3 "names an Apple Development identity" identity=appledev identity=devid profile=ok override=appledev
preflight_case "no notary profile" 3 "the notary profile thruline-notary is not stored" identity=devid
preflight_case "everything present (the refusals are not vacuous)" 0 "a Developer ID Application identity was found (SHA-1 $devid_sha)" identity=devid profile=ok

# A REAL run (no THRULINE_RELEASE_SELFTEST) with every stand-in first on PATH
# must not call one (T-DIST-1-F1 c). The scripts are copied into a throwaway
# checkout tagged v9.9.9, so the real run's own root is that checkout. The
# override names an identity only the stand-in `security` lists: the real
# /usr/bin/security (read-only find-identity) does not, so a real run stops
# there with exit 3 and never reaches xcrun, a build or a signature. If the
# run used the stand-in instead, it would get past that sentence.
real_run_case() {
  local dir=$run/real-run repo outf rc t
  repo=$dir/repo
  mkdir -p "$dir/stub" "$dir/stubs"
  cp "$stubs/security" "$stubs/xcrun" "$dir/stubs/"
  for t in codesign hdiutil spctl; do
    # shellcheck disable=SC2016 # the stand-in's own $* and $STUB_DIR, expanded when it runs
    printf '#!/bin/bash\nprintf "%s %%s\\n" "$*" >>"$STUB_DIR/calls.log"\nexit 99\n' "$t" >"$dir/stubs/$t"
    chmod +x "$dir/stubs/$t"
  done
  printf '%s %s\n' "$devid_sha" "$stub_devid_name" >"$dir/stub/identities"
  : >"$dir/stub/profile-ok"
  mkdir -p "$repo/release/macos"
  git -C "$repo" init -q
  printf '[workspace.package]\nversion = "9.9.9"\n' >"$repo/Cargo.toml"
  printf 'target/\n' >"$repo/.gitignore"
  cp "$here/release.sh" "$here/lib.sh" "$here/founder-steps.txt" "$repo/release/macos/"
  git -C "$repo" add -A
  git -C "$repo" -c user.name=selftest -c user.email=selftest@example.invalid -c commit.gpgsign=false \
    commit -q -m fixture
  git -C "$repo" -c user.name=selftest -c user.email=selftest@example.invalid -c tag.gpgsign=false \
    tag -a v9.9.9 -m fixture
  outf=$dir/output.txt
  env -u THRULINE_RELEASE_SELFTEST -u THRULINE_RELEASE_ROOT -u THRULINE_RELEASE_FREE_GIB \
    STUB_DIR="$dir/stub" PATH="$dir/stubs:$PATH" THRULINE_CODESIGN_IDENTITY="$devid_sha" \
    CARGO_TARGET_DIR="$repo/target" "$repo/release/macos/release.sh" 9.9.9 >"$outf" 2>&1
  rc=$?
  if [ -s "$dir/stub/calls.log" ]; then
    bad "real run: a stand-in on PATH was called: $(tr '\n' ';' <"$dir/stub/calls.log")"
    sed 's/^/       | /' "$outf" | redact_home
  elif [ "$rc" != 3 ] || ! grep -qF "names an identity that is not a valid codesigning identity" "$outf"; then
    bad "real run: exit $rc, wanted 3 at the identity check"
    sed 's/^/       | /' "$outf" | redact_home
  elif grep -qF "Stand-in (NOTAREALID)" "$outf"; then
    bad "real run: the output echoed an identity name"
  else
    ok "real run: stand-ins first on PATH were never called; /usr/bin/security answered, exit 3"
  fi
}
real_run_case

say ""
say "── 2. build (the one release build) ──"
df_line=$(df -h "$tdir" 2>/dev/null | tail -1 | awk '{print $4 " free"}')
say "     disk before the build: $df_line"
gib=$(df -k "$tdir" | awk 'NR == 2 { print int($4 / 1048576) }')
if [ "$gib" -lt "$RELEASE_MIN_FREE_GIB" ]; then
  bad "build: only $gib GiB free; not building below $RELEASE_MIN_FREE_GIB"
  say "self-test: $passed passed, $failed failed."
  exit 1
fi
t0=$(date +%s)
if built=$(build_release "$root" "$run/cargo-build.log"); then
  bin=$(printf '%s\n' "$built" | tail -n 1)
  fresh=$(grep -c 'Compiling' "$run/cargo-build.log" | tr -d ' ')
  ok "build: $(shown "$bin") ($RELEASE_FEATURES, $(host_triple)) in $(( $(date +%s) - t0 )) s; $fresh crate(s) compiled (0 means cargo reused a fresh build)"
else
  printf '%s\n' "$built"
  bad "build: cargo build --release failed"
  say "self-test: $passed passed, $failed failed."
  exit 1
fi

say ""
say "── 3. release-binary checks ──"
debug_bin=$tdir/debug/thruline
if [ -x "$debug_bin" ]; then
  say "     positive control: $(shown "$debug_bin"), built $(date -r "$debug_bin" '+%Y-%m-%d %H:%M') — not rebuilt by this self-test"
else
  debug_bin=
fi
if check_release_binary "$bin" "$run/check" "$debug_bin" >"$run/check.txt" 2>&1; then
  sed 's/^/     /' "$run/check.txt"
  ok "check: --version runs; the paid gate is on with the production key, and no debug-only seam is in the release binary or acts on it"
else
  sed 's/^/     /' "$run/check.txt"
  bad "check: the release-binary checks failed"
fi

say ""
say "── 4. ad-hoc codesign, hardened runtime ──"
ev=$run/evidence
mkdir -p "$ev" "$run/stage"
signed=$run/stage/thruline
cp "$bin" "$signed"
if sign_binary - "$signed" "$ev" >"$run/sign.txt" 2>&1; then
  sed 's/^/     /' "$run/sign.txt"
  ok "codesign: sign_binary - succeeded"
else
  sed 's/^/     /' "$run/sign.txt"
  bad "codesign: sign_binary - failed"
fi
# The hardened-runtime assertion, read here from codesign itself.
flags=$(codesign -dvvv "$signed" 2>&1 | sed -n 's/^CodeDirectory .* flags=\(0x[0-9a-f]*([^)]*)\).*/\1/p')
case $flags in
  *runtime*) ok "codesign: the signed binary carries the hardened runtime (flags $flags)" ;;
  *) bad "codesign: the signed binary lacks the hardened runtime (flags ${flags:-none})" ;;
esac
if codesign --verify --strict --verbose=2 "$signed" >/dev/null 2>&1; then
  ok "codesign: codesign --verify --strict passes"
else
  bad "codesign: codesign --verify --strict fails"
fi
if "$signed" --version >/dev/null 2>&1 \
  && licence_status_json "$signed" "$run/check/home-release" >/dev/null 2>&1; then
  ok "codesign: the hardened binary runs (--version, licence status) with no entitlements file"
else
  bad "codesign: the hardened binary does not run without entitlements"
fi
libs=$(otool -L "$signed" | tail -n +2 | awk '{print $1}' | grep -v -E '^(/usr/lib/|/System/Library/)' || true)
if [ -z "$libs" ]; then
  ok "codesign: it links only system libraries, so library validation needs no exception"
else
  bad "codesign: it links non-system libraries: $libs"
fi
real_flags=$(codesign_binary_flags "$devid_sha")
case " $real_flags " in
  *" --timestamp "*"--options runtime "*) ok "codesign: a Developer ID signing uses: $real_flags" ;;
  *) bad "codesign: a Developer ID signing would use '$real_flags' (needs --timestamp and --options runtime)" ;;
esac

# INSTALL.txt in plain words (T-DIST-1-F1 b): the exact line that puts
# thruline where a new Terminal window finds it on a stock Mac, the first
# command, and why it asks for a password. No "PATH".
install_words_check() {
  local f=$1 miss=
  grep -qxF '   sudo mkdir -p /usr/local/bin && sudo cp /Volumes/ThruLine/thruline /usr/local/bin/thruline' "$f" || miss="$miss the copy line;"
  grep -qxF '   thruline ui' "$f" || miss="$miss the first command (thruline ui);"
  grep -qF "asks for your Mac's password" "$f" || miss="$miss that it asks for the password;"
  grep -qF 'It asks because' "$f" || miss="$miss why it asks;"
  grep -qF 'new Terminal window' "$f" || miss="$miss a new Terminal window;"
  grep -q 'PATH' "$f" && miss="$miss (it says PATH, which is jargon);"
  # /usr/local/bin must be on a stock Mac's PATH for the copy line to be enough.
  grep -qx '/usr/local/bin' /etc/paths || miss="$miss (/etc/paths on this Mac does not list /usr/local/bin);"
  if [ -z "$miss" ]; then
    ok "package: INSTALL.txt gives the copy line, says why it asks for the password, and ends on thruline ui"
  else
    bad "package: INSTALL.txt lacks:$miss"
  fi
}

say ""
say "── 5. package ──"
out=$run/out
mkdir -p "$out"
if package_release - "$root" "$signed" 0.0.0-selftest "$(host_triple)" "$out" "$ev" >"$run/package.txt" 2>&1; then
  sed 's/^/     /' "$run/package.txt"
  dmg=$(ls "$out"/*.dmg)
  tarball=$(ls "$out"/*.tar.gz)
  ok "package: $(basename "$dmg") and $(basename "$tarball") made"
  if codesign --verify --strict "$dmg" >/dev/null 2>&1; then ok "package: the dmg is signed and verifies"; else bad "package: the dmg does not verify"; fi
  mnt=$run/mnt
  mkdir -p "$mnt"
  if hdiutil attach -nobrowse -readonly -noautoopen -mountpoint "$mnt" "$dmg" >"$ev/hdiutil-attach.txt" 2>&1; then
    contents=$(cd "$mnt" && find . -mindepth 1 -maxdepth 1 ! -name '.*' | sed 's#^\./##' | sort | tr '\n' ' ')
    if [ "$contents" = "INSTALL.txt LICENSE.md THIRD_PARTY_NOTICES.md thruline " ] && cmp -s "$mnt/INSTALL.txt" <(install_text) \
      && cmp -s "$mnt/thruline" "$signed"; then
      ok "package: the dmg holds exactly INSTALL.txt (install_text's words), LICENSE.md, THIRD_PARTY_NOTICES.md, and the signed binary"
    else
      bad "package: the dmg holds '$contents', and INSTALL.txt or the binary is not what was staged"
    fi
    install_words_check "$mnt/INSTALL.txt"
    hdiutil detach "$mnt" >/dev/null 2>&1 || hdiutil detach -force "$mnt" >/dev/null 2>&1
  else
    bad "package: the dmg would not mount"
  fi
  want_tar=thruline-cli-$(host_triple).tar.gz
  listing=$(tar -tzf "$tarball" | sort | tr '\n' ' ')
  if [ "$(basename "$tarball")" = "$want_tar" ] && grep -q "$want_tar" "$root/packaging/homebrew/Formula/thruline.rb" \
    && [ "$listing" = "thruline-cli-$(host_triple)/ thruline-cli-$(host_triple)/LICENSE.md thruline-cli-$(host_triple)/THIRD_PARTY_NOTICES.md thruline-cli-$(host_triple)/thruline " ]; then
    ok "package: $want_tar is the name the Homebrew formula fetches, one root directory, binary inside"
  else
    bad "package: archive $(basename "$tarball") holds '$listing'"
  fi
  if (cd "$out" && shasum -a 256 -c sha256.sum >/dev/null 2>&1); then ok "package: sha256.sum verifies both files"; else bad "package: sha256.sum does not verify"; fi
else
  sed 's/^/     /' "$run/package.txt"
  bad "package: package_release failed"
  dmg=
fi

say ""
say "── 6. notarize (stand-in xcrun only; nothing is sent to Apple) ──"
nstub=$run/notary-stub
mkdir -p "$nstub" "$run/notary-reject" "$run/notary-accept"
printf 'Invalid' >"$nstub/submit-status"
if [ "$XCRUN" != xcrun ]; then
  # lib.sh resolved xcrun to its absolute path, so the stand-in would not
  # answer and the real notarytool would. Never submit from a self-test.
  bad "notarize: lib.sh is calling $XCRUN, not the stand-in; not running the notarize cases"
elif [ -n "${dmg:-}" ]; then
  STUB_DIR=$nstub PATH="$stubs:$PATH" notarize "$dmg" "$run/notary-reject" >"$run/notary-reject.txt" 2>&1
  rc=$?
  if [ $rc = 1 ] && [ -s "$run/notary-reject/notary-log.json" ]; then
    ok "notarize: a rejected submission exits 1 and its log is saved"
  else
    bad "notarize: a rejected submission returned $rc; log saved: $([ -s "$run/notary-reject/notary-log.json" ] && echo yes || echo no)"
  fi
  printf 'Accepted' >"$nstub/submit-status"
  if STUB_DIR=$nstub PATH="$stubs:$PATH" notarize "$dmg" "$run/notary-accept" >"$run/notary-accept.txt" 2>&1; then
    ok "notarize: an accepted submission passes"
  else
    bad "notarize: an accepted submission did not pass"
  fi
fi

say ""
say "── 7. staple check on the un-notarized dmg ──"
if [ -n "${dmg:-}" ]; then
  if validate_staple "$dmg" "$ev/stapler-validate.txt"; then
    bad "staple: stapler validate PASSED on a dmg that was never notarized"
  else
    ok "staple: stapler validate fails on the un-notarized dmg, as expected: $(grep -v '^Processing' "$ev/stapler-validate.txt" | head -1 | redact_home)"
  fi
fi

say ""
tree_after=$(git -C "$root" status --porcelain --untracked-files=all | shasum -a 256)
if [ "$tree_before" = "$tree_after" ]; then
  ok "the tree's git status is unchanged by the self-test"
else
  bad "the self-test changed the tree's git status"
fi
say "self-test: $passed passed, $failed failed."
[ "$failed" = 0 ]
