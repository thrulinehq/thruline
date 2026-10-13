#!/bin/bash
# release/macos/release.sh [--universal] <version>
#
# One command from a clean checkout at tag v<version> to a signed, notarized,
# stapled macOS release in target/release-dist/<version>/ (T-DIST-1).
# Nothing under the tree is modified: every output is in the target directory,
# which git ignores.
#
# Exit codes: 0 done · 1 failed · 3 blocked on the founder (no Developer ID
# Application identity, or no `thruline-notary` profile). Blocked is not
# failed: exit 3 prints the exact steps that unblock it.
#
# Self-test mode (THRULINE_RELEASE_SELFTEST=1, set only by release-selftest.sh)
# honours two seams — THRULINE_RELEASE_ROOT (a throwaway checkout) and
# THRULINE_RELEASE_FREE_GIB (the free-space reading) — calls the Apple tools by
# name so stand-ins can answer, and STOPS after the preflight, so it can never
# build, sign, or submit anything. A real run calls them by absolute path.
set -u

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=release/macos/lib.sh
. "$here/lib.sh"

universal=
while [ $# -gt 0 ]; do
  case $1 in
    --universal) universal=universal; shift ;;
    -h | --help)
      sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'
      exit 0 ;;
    -*) say "unknown option: $1"; exit $RELEASE_EXIT_FAILED ;;
    *) break ;;
  esac
done
if [ $# -ne 1 ]; then
  say "usage: release/macos/release.sh [--universal] <version>   (for example: 0.1.0)"
  exit $RELEASE_EXIT_FAILED
fi
version=$1

selftest=
if [ "${THRULINE_RELEASE_SELFTEST:-}" = 1 ]; then
  selftest=1
  root=${THRULINE_RELEASE_ROOT:-$(git -C "$here" rev-parse --show-toplevel)}
else
  root=$(git -C "$here" rev-parse --show-toplevel)
fi

if [ "$(uname -s)" != Darwin ]; then
  refuse "this builds the macOS release and runs on macOS only."
  exit $RELEASE_EXIT_FAILED
fi

tdir=$(target_dir_for "$root")
out=$tdir/release-dist/$version
ev=$out/evidence

blocked() {
  printf '\n'
  founder_steps
  # The preflight has already found the tag, so step 5 is only the second line.
  printf '\nYou asked for %s, and the tag v%s is already in place: once steps 1-4 are done,\nrun release/macos/release.sh %s again.\n' "$version" "$version" "$version"
  exit $RELEASE_EXIT_BLOCKED
}

# ── 1. preflight: refuses before doing anything ─────────────────────────────
preflight_tree "$root" "$version" || exit $?
preflight_disk "$root" || exit $?
if [ -z "$selftest" ] && [ -e "$out" ] && [ -n "$(ls -A "$out" 2>/dev/null)" ]; then
  refuse "$(shown "$out") already holds a run. Move it aside first (for example: mv $(shown "$out") $(shown "$out").previous) — this script never deletes one."
  exit $RELEASE_EXIT_FAILED
fi
mkdir -p "$ev"
preflight_identity
rc=$?
[ $rc -eq 0 ] || { [ $rc -eq $RELEASE_EXIT_BLOCKED ] && blocked; exit $rc; }
preflight_profile "$ev"
rc=$?
[ $rc -eq 0 ] || { [ $rc -eq $RELEASE_EXIT_BLOCKED ] && blocked; exit $rc; }

if [ -n "$selftest" ]; then
  say "self-test mode: the preflight passed; stopping before the build."
  exit 0
fi

# ── 2. build ────────────────────────────────────────────────────────────────
triple=$(host_triple)
[ "$triple" != unknown ] || { refuse "unsupported host architecture: $(uname -m)."; exit $RELEASE_EXIT_FAILED; }
[ "$universal" = universal ] && triple=universal2-apple-darwin
built=$(build_release "$root" "$ev/cargo-build.log" "$universal") || { printf '%s\n' "$built"; exit $RELEASE_EXIT_FAILED; }
bin=$(printf '%s\n' "$built" | tail -n 1)
say "build: $(shown "$bin") ($triple, $RELEASE_FEATURES — the shipped feature set, per packaging/homebrew/Formula/thruline.rb and dist-workspace.toml)."

# ── 3. release-binary checks ────────────────────────────────────────────────
mkdir -p "$out/stage"
check_release_binary "$bin" "$out/stage/check" || exit $RELEASE_EXIT_FAILED

# ── 4. codesign (hardened runtime, secure timestamp) ────────────────────────
signed=$out/stage/thruline
cp "$bin" "$signed"
sign_binary "$RELEASE_IDENTITY" "$signed" "$ev" || exit $RELEASE_EXIT_FAILED
"$signed" --version >/dev/null 2>&1 || { refuse "the signed binary did not run."; exit $RELEASE_EXIT_FAILED; }

# ── 5. package: dmg + Homebrew tar.gz + sha256 ──────────────────────────────
# With --universal, both Homebrew archives carry the same universal binary.
if [ "$universal" = universal ]; then
  package_release "$RELEASE_IDENTITY" "$root" "$signed" "$version" aarch64-apple-darwin "$out" "$ev" || exit $RELEASE_EXIT_FAILED
  mv "$RELEASE_DMG" "$out/thruline-$version-macos-universal.dmg"
  RELEASE_DMG=$out/thruline-$version-macos-universal.dmg
  (cd "$out/stage/tar" && mkdir -p thruline-cli-x86_64-apple-darwin \
    && cp "$signed" "$root/LICENSE.md" "$root/THIRD_PARTY_NOTICES.md" thruline-cli-x86_64-apple-darwin/ \
    && COPYFILE_DISABLE=1 tar -czf "$out/thruline-cli-x86_64-apple-darwin.tar.gz" thruline-cli-x86_64-apple-darwin) || exit $RELEASE_EXIT_FAILED
  (cd "$out" && : >sha256.sum && for f in "$(basename "$RELEASE_DMG")" thruline-cli-aarch64-apple-darwin.tar.gz thruline-cli-x86_64-apple-darwin.tar.gz; do
    shasum -a 256 "$f" >"$f.sha256"; cat "$f.sha256" >>sha256.sum; done) || exit $RELEASE_EXIT_FAILED
else
  package_release "$RELEASE_IDENTITY" "$root" "$signed" "$version" "$triple" "$out" "$ev" || exit $RELEASE_EXIT_FAILED
fi

# ── 6. notarize ─────────────────────────────────────────────────────────────
notarize "$RELEASE_DMG" "$ev" || exit $RELEASE_EXIT_FAILED

# ── 7. staple and check ─────────────────────────────────────────────────────
staple_and_check "$RELEASE_DMG" "$ev" || exit $RELEASE_EXIT_FAILED
# The stapled dmg's bytes changed; its checksum is re-taken.
(cd "$out" && : >sha256.sum && for f in *.dmg *.tar.gz; do
  shasum -a 256 "$f" >"$f.sha256"; cat "$f.sha256" >>sha256.sum; done) || exit $RELEASE_EXIT_FAILED
say "done: $(shown "$out") — the stapled dmg, the Homebrew archive(s), sha256.sum, and every tool's output in evidence/."

# ── 8. the founder's second-account check ───────────────────────────────────
printf '\n'
second_account_steps "$RELEASE_DMG" "$(workspace_repository "$root")"
exit 0
