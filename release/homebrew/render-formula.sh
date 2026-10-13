#!/bin/bash
# release/homebrew/render-formula.sh [--macos-only] <version> <release-dist-dir>
#
# Renders Track C's Homebrew formula template for one release (T-DIST-2).
# Reads packaging/homebrew/Formula/thruline.rb, which is never edited, and
# writes target/release-dist/<version>/homebrew/thruline.rb with:
#   - version filled in;
#   - each sha256 filled from <release-dist-dir>/sha256.sum, after checking
#     the archive is there and its bytes match;
#   - every download address built from Cargo.toml's `repository`.
# An archive the formula names but this release did not build is refused,
# by target. --macos-only leaves the Linux block out instead, for a release
# made by release/macos/release.sh alone.
#
# It publishes nothing: publishing the tap is the founder's act, and the
# commands for it are printed, not run.
#
# Exit codes: 0 rendered and checked · 1 refused.
set -u

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=release/macos/lib.sh
. "$here/../macos/lib.sh"
root=$(git -C "$here" rev-parse --show-toplevel)

macos_only=
while [ $# -gt 0 ]; do
  case $1 in
    --macos-only) macos_only=1; shift ;;
    -h | --help) sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) refuse "unknown option: $1"; exit 1 ;;
    *) break ;;
  esac
done
if [ $# -ne 2 ]; then
  say "usage: release/homebrew/render-formula.sh [--macos-only] <version> <release-dist-dir>"
  exit 1
fi
version=$1
dist=$2
case $version in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *) refuse "$version is not a version like 0.1.0."; exit 1 ;;
esac

template=$root/packaging/homebrew/Formula/thruline.rb
sums=$dist/sha256.sum
[ -f "$template" ] || { refuse "the formula template is missing: $(shown "$template")."; exit 1; }
[ -f "$sums" ] || { refuse "there is no sha256.sum in $(shown "$dist"). Run release/macos/release.sh first."; exit 1; }

repository=$(workspace_repository "$root")
case $repository in
  https://github.com/?*/?*) ;;
  *) refuse "Cargo.toml's repository is not a GitHub address, so the download addresses cannot be built from it."; exit 1 ;;
esac
repository=${repository%/}

# The hash sha256.sum lists for <file>, with sha256sum's binary-mode `*` removed.
listed_hash() {
  awk -v n="$1" '{ f = $2; sub(/^\*/, "", f); if (f == n) print $1 }' "$sums" | head -1
}

sed_args=()
missing=
for target in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl; do
  archive=thruline-cli-$target.tar.gz
  placeholder=PLACEHOLDER_$(printf '%s' "$target" | tr '[:lower:]-' '[:upper:]_')
  hash=$(listed_hash "$archive")
  if [ -z "$hash" ]; then
    case $target in
      *-linux-*) [ -n "$macos_only" ] && continue ;;
    esac
    missing="$missing $target"
    continue
  fi
  if [ ! -f "$dist/$archive" ]; then
    refuse "sha256.sum lists $archive, but the file is not in $(shown "$dist")."
    exit 1
  fi
  if [ "$(shasum -a 256 "$dist/$archive" | cut -d' ' -f1)" != "$hash" ]; then
    refuse "$archive does not match the hash sha256.sum lists for it."
    exit 1
  fi
  sed_args+=(-e "s/\"$placeholder\"/\"$hash\"/")
done
if [ -n "$missing" ]; then
  refuse "this release has no archive for:$missing. The formula would be left with a placeholder for each. Build them, or, for a release made by release/macos/release.sh alone, run again with --macos-only."
  exit 1
fi

out=$(target_dir_for "$root")/release-dist/$version/homebrew
mkdir -p "$out"
tmp=$out/thruline.rb.partial
sed "${sed_args[@]}" \
  -e "s/^\(  version \)\"0\.0\.0-PLACEHOLDER\"/\1\"$version\"/" \
  -e "s#\"https://github\.com/[^/\"]*/[^/\"]*/releases/download/#\"$repository/releases/download/#" \
  "$template" >"$tmp"
if [ -n "$macos_only" ]; then
  # From the Linux comment through the end of the on_linux block.
  awk '
    /^  # Linux bottles/ { skip = 1 }
    /^  on_linux do/ { skip = 1; inblock = 1 }
    { if (!skip) print }
    inblock && /^  end$/ { skip = 0; inblock = 0; getline; if ($0 != "") print; next }
  ' "$tmp" >"$tmp.2" && mv "$tmp.2" "$tmp"
  if grep -v '^ *#' "$tmp" | grep -qi 'linux'; then
    refuse "--macos-only could not remove the Linux block: the template's shape has changed. Nothing was written."
    rm -f "$tmp"
    exit 1
  fi
fi

# The checks. A failure leaves no thruline.rb behind.
problems=
if grep -n 'PLACEHOLDER' "$tmp" >"$tmp.hits"; then
  problems="$problems a placeholder is left at line(s) $(cut -d: -f1 "$tmp.hits" | tr '\n' ' ');"
fi
rm -f "$tmp.hits"
if grep -E '^ *url ' "$tmp" | grep -vqF "url \"$repository/releases/download/v#{version}/"; then
  problems="$problems a download address does not start with Cargo.toml's repository;"
fi
if ! grep -qx "  version \"$version\"" "$tmp"; then
  problems="$problems the version line is not $version;"
fi
if ! command -v ruby >/dev/null 2>&1; then
  problems="$problems ruby is not installed here, so the formula's syntax could not be checked;"
elif ! ruby -c "$tmp" >/dev/null 2>"$out/ruby-c.txt"; then
  problems="$problems ruby -c rejects it (see $(shown "$out")/ruby-c.txt);"
fi
if [ -n "$problems" ]; then
  refuse "the rendered formula failed its checks:$problems Nothing was written."
  rm -f "$tmp"
  exit 1
fi
mv "$tmp" "$out/thruline.rb"
say "rendered: $(shown "$out/thruline.rb") (version $version; ruby -c: Syntax OK; no placeholder left)."
[ -n "$macos_only" ] && say "It installs on macOS only: the Linux block was left out (--macos-only)."
shasum -a 256 "$out/thruline.rb" | sed "s#  .*#  thruline.rb#" | sed 's/^/sha256 /'

cat <<EOF

Publishing the tap is yours; nothing below has been run. The tap repository is
named homebrew-thruline under the owner Cargo.toml's repository names (the tap
name is part of the open publish-path decision, and may change). From the top
folder of this checkout, after the GitHub release for v$version is published:

  OWNER=\$(sed -n 's#^repository *= *"https://github.com/\([^/]*\)/.*#\1#p' Cargo.toml)
  gh repo create "\$OWNER/homebrew-thruline" --public          # once, the first time
  gh repo clone "\$OWNER/homebrew-thruline" ../homebrew-thruline
  mkdir -p ../homebrew-thruline/Formula
  cp $(shown "$out/thruline.rb") ../homebrew-thruline/Formula/thruline.rb
  git -C ../homebrew-thruline add Formula/thruline.rb
  git -C ../homebrew-thruline commit -m "thruline $version"
  git -C ../homebrew-thruline push
  brew install "\$OWNER/thruline/thruline"
  brew test thruline
EOF
