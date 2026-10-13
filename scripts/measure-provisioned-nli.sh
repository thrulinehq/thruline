#!/usr/bin/env bash
# Produce the current aggregate NLI RSS/throughput receipt from committed,
# synthetic inputs and the exact operator-provisioned default A18 model.
#
# This wrapper is intentionally stricter than an ordinary test invocation. It
# requires checkpointed measuring code, a pre-existing external Cargo target,
# Cargo/ORT offline mode, and a verified default model receipt. It never pulls,
# starts, imports, updates, overwrites, or deletes a model or receipt.

set -euo pipefail

fail() {
  echo "NLI performance receipt refused: $1" >&2
  exit 2
}

command -v cargo >/dev/null 2>&1 || fail "cargo is required"
command -v git >/dev/null 2>&1 || fail "git is required"

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd -P)
REPO_ROOT=$(CDPATH='' cd -- "$SCRIPT_DIR/.." && pwd -P)

[ -n "${CARGO_TARGET_DIR:-}" ] || fail "set CARGO_TARGET_DIR to an external build directory"
case "$CARGO_TARGET_DIR" in
  /*) ;;
  *) fail "CARGO_TARGET_DIR must be absolute" ;;
esac
[ -d "$CARGO_TARGET_DIR" ] || fail "CARGO_TARGET_DIR must already exist"
TARGET_ROOT=$(CDPATH='' cd -- "$CARGO_TARGET_DIR" && pwd -P)
case "$TARGET_ROOT/" in
  "$REPO_ROOT/"*) fail "CARGO_TARGET_DIR must be outside the repository" ;;
esac

MEASUREMENT_PATHS=(
  Cargo.toml
  Cargo.lock
  rust-toolchain.toml
  crates/core/Cargo.toml
  crates/cli/Cargo.toml
  crates/cli/build.rs
  crates/core/src
  crates/cli/src
  crates/core/tests/nli_performance_receipt.rs
  crates/core/tests/fixtures/nli-performance-corpus-v1.json
  scripts/measure-provisioned-nli.sh
)
for path in "${MEASUREMENT_PATHS[@]}"; do
  git -C "$REPO_ROOT" ls-files --error-unmatch -- "$path" >/dev/null 2>&1 \
    || fail "measurement code and corpus must be tracked by the checkpoint"
done

# `perf_basis::engine_sources()` recursively digests every `.rs` file under
# these two trees, including files excluded by normal Git ignore rules. A
# directory pathspec proves only that some tracked child exists; it does not
# prove that every byte entering the digest is tracked. Mirror the recursive
# inventory, NUL-delimited, and require each discovered path individually.
if ! find -L "$REPO_ROOT/crates/core/src" "$REPO_ROOT/crates/cli/src" \
  -type f -name '*.rs' -print0 \
  | while IFS= read -r -d '' source_path; do
      relative_path=${source_path#"$REPO_ROOT/"}
      git -C "$REPO_ROOT" ls-files --error-unmatch -- "$relative_path" >/dev/null 2>&1 \
        || exit 1
    done
then
  fail "every Rust source entering the code digest must be individually tracked"
fi

git -C "$REPO_ROOT" diff --quiet -- "${MEASUREMENT_PATHS[@]}" \
  || fail "measurement code or corpus differs from the checkpoint"
git -C "$REPO_ROOT" diff --cached --quiet -- "${MEASUREMENT_PATHS[@]}" \
  || fail "measurement code or corpus has an uncommitted staged change"

MODEL_DIR="${HOME:?HOME is required}/.thruline/models/nli-deberta-v3-xsmall"
OUTPUT="$REPO_ROOT/.codex/evidence/nli-performance-receipt-v1.json"
git -C "$REPO_ROOT" check-ignore -q -- "$OUTPUT" \
  || fail "the stable evidence path must be ignored by this checkout"
[ ! -e "$OUTPUT" ] || fail "the ignored evidence receipt already exists; preserve it and choose a new checkpoint"

# Cargo's offline flag is also recognized by ort-sys as a no-download order.
# The explicit ORT flag makes that boundary visible even if Cargo changes its
# environment propagation. Clearing proxy variables is defence in depth; the
# measured program contains no network client.
export CARGO_NET_OFFLINE=true
export CARGO_INCREMENTAL=0
export ORT_OFFLINE=1
export NO_PROXY='*'
export no_proxy='*'
unset HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy

cd -- "$REPO_ROOT"
cargo run -p thruline-cli --locked --offline --no-default-features -- \
  model status --dir "$MODEL_DIR" >/dev/null \
  || fail "the exact default A18 model receipt is not ready"

mkdir -p -- "$REPO_ROOT/.codex/evidence"
THRULINE_NLI_PERF_LIVE=1 \
THRULINE_NLI_PERF_OUTPUT="$OUTPUT" \
  cargo test -p thruline-core --release --locked --offline \
  --test nli_performance_receipt provisioned_model_performance_receipt -- \
  --ignored --exact --nocapture --test-threads=1

echo "NLI performance receipt written to .codex/evidence/nli-performance-receipt-v1.json"
