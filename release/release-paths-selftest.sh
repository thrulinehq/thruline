#!/bin/bash
# release/release-paths-selftest.sh — the publish-side scripts, run for real
# against a throwaway checkout (T-DIST-2 A2, A3).
#
#   - render-formula.sh fills Track C's Homebrew template from a synthetic
#     sha256.sum; the result passes `ruby -c` and no placeholder survives; an
#     archive the release lacks is refused by target; a changed archive is
#     refused.
#   - github-release.sh's dry run prints the commands and calls nothing; it
#     cannot reach `gh release create` without --publish AND the typed version.
#
# The throwaway checkout names a synthetic owner, and every run has only a
# stand-in `gh` on its PATH (the real one is not reachable), so nothing here can
# publish. Needs no build. Files go to target/release-dist/paths-selftest/.
#
# Exit: 0 every case passed · 1 a case failed.
set -u
unset CARGO_TARGET_DIR

here=$(cd "$(dirname "$0")" && pwd)
root=$(git -C "$here" rev-parse --show-toplevel)
run=$root/target/release-dist/paths-selftest/run-$(date +%Y%m%dT%H%M%S)
mkdir -p "$run"
printf 'self-test: files in %s\n' "${run/#$HOME/~}"

passed=0
failed=0
ok() { passed=$((passed + 1)); printf 'ok   %s\n' "$*"; }
bad() { failed=$((failed + 1)); printf 'FAIL %s\n' "$*"; }
show() { sed 's/^/       | /' "$1" | sed "s#$HOME#~#g"; }

OWNER=example-owner-selftest
tree_before=$(git -C "$root" status --porcelain --untracked-files=all | shasum -a 256)

# A throwaway checkout carrying copies of the scripts and the template, tagged
# v9.9.9, with a synthetic author and a synthetic repository address.
make_repo() { # <dir>
  local r=$1
  mkdir -p "$r/release/macos" "$r/release/homebrew" "$r/packaging/homebrew/Formula"
  git -C "$r" init -q
  printf '[workspace.package]\nversion = "9.9.9"\nrepository = "https://github.com/%s/thruline"\n' "$OWNER" >"$r/Cargo.toml"
  printf 'target/\n' >"$r/.gitignore"
  cp "$root/release/macos/lib.sh" "$root/release/macos/founder-steps.txt" "$r/release/macos/"
  cp "$root/release/homebrew/render-formula.sh" "$r/release/homebrew/"
  cp "$root/release/github-release.sh" "$r/release/"
  cp "$root/packaging/homebrew/Formula/thruline.rb" "$r/packaging/homebrew/Formula/"
  git -C "$r" add -A
  git -C "$r" -c user.name=selftest -c user.email=selftest@example.invalid -c commit.gpgsign=false commit -q -m fixture
  git -C "$r" -c user.name=selftest -c user.email=selftest@example.invalid -c tag.gpgsign=false tag -a v9.9.9 -m fixture
}

# A synthetic release folder: the named archives (random bytes), a dmg, sha256.sum.
make_dist() { # <dir> <target>...
  local d=$1 t
  shift
  mkdir -p "$d"
  head -c 4096 /dev/urandom >"$d/thruline-9.9.9-macos-arm64.dmg"
  for t in "$@"; do head -c 2048 /dev/urandom >"$d/thruline-cli-$t.tar.gz"; done
  (cd "$d" && : >sha256.sum && for f in *.dmg *.tar.gz; do shasum -a 256 "$f" >>sha256.sum; done)
}

# The stand-in gh: logs, answers, publishes nothing.
stubs=$run/stubs
mkdir -p "$stubs"
cat >"$stubs/gh" <<'EOF'
#!/bin/bash
printf 'gh %s\n' "$*" >>"$GH_LOG"
exit 0
EOF
chmod +x "$stubs/gh"
# The PATH every case runs with: the stand-in, then the system. The real gh
# (Homebrew's) is not on it.
SAFE_PATH=$stubs:/usr/bin:/bin:/usr/sbin:/sbin

MAC=(aarch64-apple-darwin x86_64-apple-darwin)
ALL=(aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl)

# ── A2: render-formula.sh ───────────────────────────────────────────────────
printf '\n── A2. render-formula.sh ──\n'

render() { # <repo> <out file> <args...>
  local r=$1 o=$2
  shift 2
  env PATH="$SAFE_PATH" GH_LOG="$run/gh-render.log" HOME="$HOME" "$r/release/homebrew/render-formula.sh" "$@" >"$o" 2>&1
}

formula_is_good() { # <formula> <dist> <label> <expect linux: yes|no>
  local f=$1 d=$2 label=$3 linux=$4 problems='' t h
  [ -f "$f" ] || { bad "$label: no formula was written"; return; }
  ruby -c "$f" >/dev/null 2>&1 || problems="$problems ruby -c rejects it;"
  grep -q PLACEHOLDER "$f" && problems="$problems a PLACEHOLDER survives;"
  grep -qx '  version "9.9.9"' "$f" || problems="$problems no version 9.9.9;"
  if grep -E '^ *url ' "$f" | grep -vqF "url \"https://github.com/$OWNER/thruline/releases/download/v#{version}/"; then
    problems="$problems a url does not come from Cargo.toml's repository;"
  fi
  for t in "${MAC[@]}"; do
    h=$(shasum -a 256 "$d/thruline-cli-$t.tar.gz" | cut -d' ' -f1)
    grep -qF "sha256 \"$h\"" "$f" || problems="$problems $t's hash is not in it;"
  done
  if [ "$linux" = yes ]; then
    grep -q '^  on_linux do' "$f" || problems="$problems the Linux block is missing;"
  else
    grep -v '^ *#' "$f" | grep -qi linux && problems="$problems it still mentions Linux;"
  fi
  if [ -z "$problems" ]; then
    ok "$label: ruby -c passes, no PLACEHOLDER, version, hashes and addresses from this release and Cargo.toml"
  else
    bad "$label:$problems"
  fi
}

# 1. macOS archives only, no flag: refused, both Linux targets named, nothing written.
r=$run/render-1/repo; make_repo "$r"; d=$r/target/release-dist/9.9.9; make_dist "$d" "${MAC[@]}"
render "$r" "$run/render-1/out.txt" 9.9.9 "$d"; rc=$?
if [ $rc = 1 ] && grep -qF 'aarch64-unknown-linux-musl x86_64-unknown-linux-musl' "$run/render-1/out.txt" \
  && [ ! -e "$d/homebrew/thruline.rb" ]; then
  ok "render: an archive the release lacks is refused by target, nothing written: $(grep -m1 refused "$run/render-1/out.txt" | cut -c1-110)"
else
  bad "render: macOS-only archives without --macos-only returned $rc"; show "$run/render-1/out.txt"
fi

# 2. --macos-only: rendered, and checked here independently of the script's own checks.
render "$r" "$run/render-1/out-macos.txt" --macos-only 9.9.9 "$d"; rc=$?
if [ $rc = 0 ]; then
  formula_is_good "$d/homebrew/thruline.rb" "$d" "render --macos-only" no
  cp "$d/homebrew/thruline.rb" "$run/rendered-macos-only.rb"
else
  bad "render --macos-only returned $rc"; show "$run/render-1/out-macos.txt"
fi
if grep -qF "$OWNER" "$run/render-1/out-macos.txt"; then
  bad "render: the printed output names the owner; it must read it from Cargo.toml instead"
else
  ok "render: the printed tap commands read the owner from Cargo.toml and never print it"
fi

# 3. All four archives: rendered with the Linux block.
r=$run/render-3/repo; make_repo "$r"; d=$r/target/release-dist/9.9.9; make_dist "$d" "${ALL[@]}"
render "$r" "$run/render-3/out.txt" 9.9.9 "$d"; rc=$?
if [ $rc = 0 ]; then
  formula_is_good "$d/homebrew/thruline.rb" "$d" "render, all four archives" yes
else
  bad "render with all four archives returned $rc"; show "$run/render-3/out.txt"
fi

# 4. An archive whose bytes changed after sha256.sum was written: refused.
r=$run/render-4/repo; make_repo "$r"; d=$r/target/release-dist/9.9.9; make_dist "$d" "${MAC[@]}"
printf 'x' >>"$d/thruline-cli-x86_64-apple-darwin.tar.gz"
render "$r" "$run/render-4/out.txt" --macos-only 9.9.9 "$d"; rc=$?
if [ $rc = 1 ] && grep -qF 'thruline-cli-x86_64-apple-darwin.tar.gz does not match' "$run/render-4/out.txt" \
  && [ ! -e "$d/homebrew/thruline.rb" ]; then
  ok "render: an archive that does not match sha256.sum is refused, nothing written"
else
  bad "render: a changed archive returned $rc"; show "$run/render-4/out.txt"
fi

# 5. The template itself is untouched by every render above.
if git -C "$root" diff --quiet -- packaging/homebrew/Formula/thruline.rb; then
  ok "render: packaging/homebrew/Formula/thruline.rb is unchanged in this tree"
else
  bad "render: the template in this tree has changed"
fi

# ── A3: github-release.sh ───────────────────────────────────────────────────
printf '\n── A3. github-release.sh ──\n'
r=$run/gh/repo; make_repo "$r"; d=$r/target/release-dist/9.9.9; make_dist "$d" "${MAC[@]}"
glog=$run/gh/gh-calls.log

ghrel() { # <stdin text or ''> <out file> <args...>
  local in=$1 o=$2
  shift 2
  printf '%s' "$in" | env PATH="$SAFE_PATH" GH_LOG="$glog" HOME="$HOME" "$r/release/github-release.sh" "$@" >"$o" 2>&1
}

: >"$glog"
ghrel '' "$run/gh/dry.txt" 9.9.9; rc=$?
missing=
# shellcheck disable=SC2016 # the literal $REPO is what the dry run prints
for want in 'gh release create v9.9.9 --repo "$REPO" --verify-tag' 'gh release upload v9.9.9 --repo "$REPO"' \
  'git push origin v9.9.9' 'target/release-dist/9.9.9/sha256.sum' 'Dry run: gh was not called'; do
  grep -qF -- "$want" "$run/gh/dry.txt" || missing="$missing [$want]"
done
for f in "$d"/*.dmg "$d"/*.tar.gz; do
  grep -qF "$(shasum -a 256 "$f" | cut -d' ' -f1)  $(basename "$f")" "$run/gh/dry.txt" || missing="$missing [hash of $(basename "$f")]"
done
if [ $rc = 0 ] && [ -z "$missing" ] && [ ! -s "$glog" ]; then
  ok "github-release: the dry run prints both commands, every file and its SHA-256, and calls no gh"
else
  bad "github-release: dry run rc $rc, missing:${missing:- none}, gh calls: $(tr '\n' ';' <"$glog")"; show "$run/gh/dry.txt"
fi
if grep -qF "$OWNER" "$run/gh/dry.txt"; then
  bad "github-release: the dry run prints the owner's name"
else
  ok "github-release: the dry run reads the owner from Cargo.toml and never prints it"
fi

: >"$glog"
ghrel 'yes' "$run/gh/wrong.txt" 9.9.9 --publish; rc=$?
if [ $rc = 1 ] && [ ! -s "$glog" ] && grep -qF 'Stopped. Nothing was published.' "$run/gh/wrong.txt"; then
  ok "github-release: --publish with a wrong answer (yes) stops, and gh is never called"
else
  bad "github-release: --publish with a wrong answer: rc $rc, gh calls: $(tr '\n' ';' <"$glog")"; show "$run/gh/wrong.txt"
fi

: >"$glog"
ghrel '' "$run/gh/empty.txt" 9.9.9 --publish; rc=$?
if [ $rc = 1 ] && [ ! -s "$glog" ]; then
  ok "github-release: --publish with no answer at all stops, and gh is never called"
else
  bad "github-release: --publish with no answer: rc $rc, gh calls: $(tr '\n' ';' <"$glog")"; show "$run/gh/empty.txt"
fi

# Positive control: the stand-in is reachable, so the two cases above are not
# passing only because gh could not be found.
: >"$glog"
ghrel '9.9.9
' "$run/gh/typed.txt" 9.9.9 --publish; rc=$?
if [ $rc = 0 ] && grep -qx "gh release create v9.9.9 --repo $OWNER/thruline --verify-tag --title ThruLine 9.9.9 --notes .*" "$glog" \
  && grep -q "^gh release upload v9.9.9 --repo $OWNER/thruline .*target/release-dist/9.9.9/sha256.sum$" "$glog"; then
  ok "github-release: positive control, with --publish and the typed version the stand-in gh gets create, then upload"
else
  bad "github-release: the positive control did not reach the stand-in: rc $rc, calls: $(tr '\n' ';' <"$glog")"; show "$run/gh/typed.txt"
fi

: >"$glog"
printf 'x' >>"$d/thruline-cli-aarch64-apple-darwin.tar.gz"
ghrel '9.9.9
' "$run/gh/changed.txt" 9.9.9 --publish; rc=$?
if [ $rc = 1 ] && [ ! -s "$glog" ] && grep -qF 'does not match sha256.sum' "$run/gh/changed.txt"; then
  ok "github-release: a file that no longer matches sha256.sum is refused before anything is asked or called"
else
  bad "github-release: a changed file: rc $rc, gh calls: $(tr '\n' ';' <"$glog")"; show "$run/gh/changed.txt"
fi

printf '\n'
tree_after=$(git -C "$root" status --porcelain --untracked-files=all | shasum -a 256)
if [ "$tree_before" = "$tree_after" ]; then ok "the tree's git status is unchanged by the self-test"; else bad "the self-test changed the tree's git status"; fi
printf 'self-test: %d passed, %d failed.\n' "$passed" "$failed"
[ "$failed" = 0 ]
