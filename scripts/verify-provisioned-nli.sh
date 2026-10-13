#!/usr/bin/env bash
# Fail-closed, zero-network release gate for the operator-provided A18 scorer.
#
# Ordinary `cargo test` remains friendly to source-only clones and skips the
# model-backed calibration when the three files are absent. A release claim
# must not inherit that convenience: this wrapper verifies the exact closed
# receipt first and then makes absence a hard failure.

set -euo pipefail

MODEL_DIR="${1:-${HOME:?HOME is required}/.thruline/models/nli-deberta-v3-xsmall}"

export CARGO_NET_OFFLINE=true

cargo run -p thruline-cli --locked --offline --no-default-features -- \
  model status --dir "$MODEL_DIR" >/dev/null

DEFAULT_MODEL_DIR="${HOME}/.thruline/models/nli-deberta-v3-xsmall"
if [[ "$MODEL_DIR" != "$DEFAULT_MODEL_DIR" ]]; then
  echo "calibration loads only the production default directory: $DEFAULT_MODEL_DIR" >&2
  echo "verify or import the candidate there before making a release claim" >&2
  exit 2
fi

THRULINE_REQUIRE_NLI_MODEL=1 \
  cargo test -p thruline-core --release --locked --offline \
  --test nli_calibration -- --nocapture --test-threads=1
