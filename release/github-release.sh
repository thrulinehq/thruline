#!/bin/bash
# release/github-release.sh <version> [--publish]
#
# The GitHub release for one version, prepared from what release/macos/release.sh
# left in target/release-dist/<version>/ (T-DIST-2).
#
# By default it is a DRY RUN: it checks the files against sha256.sum and prints
# the exact `gh release create` and `gh release upload` commands, the files and
# their SHA-256 values. It never calls gh.
#
# --publish is for the founder only. It prints the same, asks you to type the
# version, and only then runs the two commands. The owner and repository come
# from Cargo.toml's `repository`; the printed commands read them from there too,
# so this output never carries the owner's name.
#
# Exit codes: 0 done (dry run printed, or published) · 1 refused or stopped.
set -u

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=release/macos/lib.sh
. "$here/macos/lib.sh"
root=$(git -C "$here" rev-parse --show-toplevel)

publish=
version=
for arg in "$@"; do
  case $arg in
    --publish) publish=1 ;;
    -h | --help) sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) refuse "unknown option: $arg"; exit 1 ;;
    *) [ -z "$version" ] || { refuse "one version only."; exit 1; }; version=$arg ;;
  esac
done
if [ -z "$version" ]; then
  say "usage: release/github-release.sh <version> [--publish]   (for example: 0.1.0)"
  exit 1
fi

repository=$(workspace_repository "$root")
repository=${repository%/}
case $repository in
  https://github.com/?*/?*) repo_slug=${repository#https://github.com/} ;;
  *) refuse "Cargo.toml's repository is not a GitHub address, so there is no GitHub release to make."; exit 1 ;;
esac

if ! git -C "$root" rev-parse -q --verify "refs/tags/v$version^{commit}" >/dev/null 2>&1; then
  refuse "there is no tag v$version in this checkout. Tag the release commit first: git tag v$version"
  exit 1
fi

dist=$(target_dir_for "$root")/release-dist/$version
if [ ! -f "$dist/sha256.sum" ]; then
  refuse "there is no sha256.sum in $(shown "$dist"). Run release/macos/release.sh $version first."
  exit 1
fi
if ! (cd "$dist" && shasum -a 256 -c sha256.sum >/dev/null 2>&1); then
  refuse "a file in $(shown "$dist") does not match sha256.sum. Nothing is uploaded from a release whose files changed."
  exit 1
fi

# The files: everything sha256.sum lists, then sha256.sum itself. A dmg or
# archive in the folder that sha256.sum does not list is refused, not skipped.
files=()
while IFS= read -r name; do
  [ -n "$name" ] && files+=("$name")
done <<EOF
$(awk '{ f = $2; sub(/^\*/, "", f); print f }' "$dist/sha256.sum")
EOF
for f in "$dist"/*.dmg "$dist"/*.tar.gz; do
  [ -e "$f" ] || continue
  if ! printf '%s\n' "${files[@]}" | grep -qxF "$(basename "$f")"; then
    refuse "$(basename "$f") is in $(shown "$dist") but not in sha256.sum."
    exit 1
  fi
done
case " ${files[*]} " in
  *.dmg\ *) ;;
  *) refuse "sha256.sum lists no .dmg. Run release/macos/release.sh $version first."; exit 1 ;;
esac
files+=(sha256.sum)

rel=$dist
case $dist in "$root"/*) rel=${dist#"$root"/} ;; esac
paths=()
for f in "${files[@]}"; do paths+=("$rel/$f"); done
title="ThruLine $version"
notes="ThruLine $version: the signed and notarized macOS disk image, the archives the Homebrew formula installs, and sha256.sum."

say "Files for v$version, from $(shown "$dist"):"
for f in "${files[@]}"; do
  say "  $(shasum -a 256 "$dist/$f" | cut -d' ' -f1)  $f"
done
say ""
say "The commands, from the top folder of this checkout. REPO is the owner/name Cargo.toml's"
say "repository names; the first line reads it from there."
say ""
say "  REPO=\$(sed -n 's#^repository *= *\"https://github.com/\\(.*\\)\"#\\1#p' Cargo.toml)"
say "  git push origin v$version"
say "  gh release create v$version --repo \"\$REPO\" --verify-tag --title \"$title\" --notes \"$notes\""
say "  gh release upload v$version --repo \"\$REPO\" ${paths[*]}"
say ""

if [ -z "$publish" ]; then
  say "Dry run: gh was not called and nothing was uploaded. The tag must be pushed first"
  say "(the second line above); gh release create --verify-tag refuses otherwise."
  say "To publish, run: release/github-release.sh $version --publish"
  exit 0
fi

say "This publishes v$version to GitHub, where anyone who can see the repository can download it."
printf 'To go ahead, type the version (%s) and press Return. Anything else stops here: ' "$version"
answer=
IFS= read -r answer || true
if [ "$answer" != "$version" ]; then
  printf '\n'
  say "Stopped. Nothing was published."
  exit 1
fi
command -v gh >/dev/null 2>&1 || { refuse "gh is not installed, so nothing was published."; exit 1; }
(cd "$root" && gh release create "v$version" --repo "$repo_slug" --verify-tag --title "$title" --notes "$notes") \
  || { refuse "gh release create failed; nothing was uploaded."; exit 1; }
(cd "$root" && gh release upload "v$version" --repo "$repo_slug" "${paths[@]}") \
  || { refuse "gh release upload failed. The release exists; run the upload line above again."; exit 1; }
say "Published v$version. Next: release/homebrew/render-formula.sh, then the tap commands it prints."
