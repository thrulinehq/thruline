#!/bin/sh
# Deterministic operator acceptance for the three exact M4 language tiers.
#
# This script never starts, pulls, downloads, or updates a model. It probes
# only the fixed loopback Ollama endpoint, then runs the ignored synthetic
# full-project Rust harness once per already-installed exact tag.

set -eu

fail() {
    echo "language-tier acceptance refused: $1" >&2
    exit 2
}

command -v curl >/dev/null 2>&1 || fail "curl is required"
command -v python3 >/dev/null 2>&1 || fail "python3 is required"
command -v cargo >/dev/null 2>&1 || fail "cargo is required"

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

umask 077
PROBE_FILE=$(mktemp "${TMPDIR:-/tmp}/thruline-tier-probe.XXXXXX")
MODEL_FILE=$(mktemp "${TMPDIR:-/tmp}/thruline-tier-models.XXXXXX")
cleanup() {
    rm -f -- "$PROBE_FILE" "$MODEL_FILE"
}
trap cleanup EXIT HUP INT TERM

# No -L: a loopback service must not redirect this read-only probe.
curl --disable --noproxy '*' --fail --silent --show-error \
    --connect-timeout 2 --max-time 5 --max-filesize 65536 \
    --header 'Accept: application/json' \
    --output "$PROBE_FILE" \
    'http://127.0.0.1:11434/api/tags' \
    || fail "fixed-loopback Ollama probe failed"

python3 - "$PROBE_FILE" "$MODEL_FILE" <<'PY'
import json
import re
import sys

source, destination = sys.argv[1:]
required = ("qwen3:0.6b", "qwen3:4b", "qwen3:8b")
try:
    with open(source, "rb") as handle:
        payload = json.load(handle)
except (OSError, ValueError) as error:
    raise SystemExit(f"language-tier acceptance refused: invalid tags response ({type(error).__name__})")

models = payload.get("models") if isinstance(payload, dict) else None
if not isinstance(models, list):
    raise SystemExit("language-tier acceptance refused: tags response has no model list")

resolved = []
for tag in required:
    matches = [row for row in models if isinstance(row, dict) and row.get("name") == tag]
    if len(matches) != 1:
        raise SystemExit(f"language-tier acceptance refused: exact tag {tag} is missing or ambiguous")
    digest = matches[0].get("digest")
    if not isinstance(digest, str) or re.fullmatch(r"[0-9a-f]{64}", digest) is None:
        raise SystemExit(f"language-tier acceptance refused: exact tag {tag} has no canonical digest")
    resolved.append((tag, digest))

with open(destination, "w", encoding="ascii", newline="\n") as handle:
    for tag, digest in resolved:
        handle.write(f"{tag}\t{digest}\n")
PY

cd -- "$REPO_ROOT"
tab=$(printf '\t')
while IFS="$tab" read -r model digest; do
    [ -n "$model" ] && [ -n "$digest" ] || fail "resolved model record is malformed"
    THRULINE_TIER_MODEL="$model" \
    THRULINE_TIER_DIGEST="$digest" \
    CARGO_INCREMENTAL=0 \
        cargo test -p thruline-core --locked --offline --no-default-features \
        --test language_tier_acceptance exact_tier_full_project_acceptance -- \
        --ignored --exact --nocapture --test-threads=1
done < "$MODEL_FILE"
