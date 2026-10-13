#!/usr/bin/env bash
# Prove a bundle verifies on a FRESH MACHINE holding only the trust root.
#
# "Fresh machine" is meant literally and is enforced rather than asserted:
#
#   --network=none          the container has no route anywhere, and the script
#                           tries to reach Rekor and reports failure if it CAN
#   a clean image           no ~/.sigstore, no ~/.cosign, no cached TUF metadata
#   read-only mounts        the host tree cannot be written back to
#   vendored dependencies   `cargo --offline`, so not even crates.io is needed
#   a different OS          Debian bookworm against a macOS host, and a
#                           different OpenSSL build (3.0.20 vs the host's 3.0.18)
#
# The only Sigstore material that crosses the boundary is
# `fixtures/trustroot/prod-trusted_root.json` and the bundle itself. That is the
# claim: given a trust root, a bundle verifies with nothing else.
#
# Usage: scripts/fresh-machine-verify.sh [image]
set -euo pipefail

CRATE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
IMAGE="${1:-thruline-m6-fresh:local}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "==> staging a hermetic tree (crate sources + fixtures only, no host state)"
mkdir -p "$WORK/crate"
tar cf - -C "$CRATE" Cargo.toml Cargo.lock src tests fixtures examples | tar xf - -C "$WORK/crate"

echo "==> vendoring dependencies so the build itself needs no network"
( cd "$CRATE" && cargo vendor "$WORK/vendor" >/dev/null )
mkdir -p "$WORK/crate/.cargo"
cat > "$WORK/crate/.cargo/config.toml" <<'EOF'
[source.crates-io]
replace-with = "vendored-sources"
[source.vendored-sources]
directory = "/vendor"
EOF

# The image ships the pinned 1.97.1 toolchain already; carrying
# rust-toolchain.toml in would only make rustup try to reach static.rust-lang.org.
TOOLCHAIN="$(docker run --rm "$IMAGE" rustup show active-toolchain 2>/dev/null | awk '{print $1}')"

echo "==> signing a bundle on THIS machine (the private key never crosses over)"
mkdir -p "$WORK/artifact"
( cd "$CRATE" && cargo run --quiet --example sign-local -- "$WORK/keys" "$WORK/artifact/local-bundle.json" )
cp "$WORK/keys/cosign.pub" "$WORK/artifact/"

echo "==> running on a fresh machine, with no network"
docker run --rm --network=none \
  -v "$WORK/crate":/crate:ro \
  -v "$WORK/vendor":/vendor:ro \
  -v "$WORK/artifact":/artifact:ro \
  -w /work -e RUSTUP_TOOLCHAIN="$TOOLCHAIN" \
  "$IMAGE" sh -euc '
    grep PRETTY /etc/os-release
    echo "openssl:  $(openssl version)"
    echo "toolchain: $(rustc --version)"
    echo "sigstore/cosign state in HOME: $(ls -a "$HOME" | grep -cE "sigstore|cosign" || true)  (0 = fresh)"

    cp -r /crate/. /work/

    # A proof that depends on there being no network must demonstrate it.
    if curl -sS --max-time 5 https://rekor.sigstore.dev/api/v1/log >/dev/null 2>&1; then
      echo "FAIL: this machine can reach Rekor, so an offline proof here is worthless"
      exit 1
    fi
    echo "confirmed: no network reachable"

    echo "trust material present:"; ls /work/fixtures/trustroot/

    # The WHOLE default-feature suite, not just the end-to-end file: if any of
    # it needs a network or host state, this is where that shows up.
    #
    # THRULINE_CORE_OPTIONAL: this container has no ThruLine core checkout by
    # design, so tests/core_contract.rs cannot run here. It FAILS rather than
    # skips by default, and opting out is deliberate and recorded — the contract
    # is gated on a machine that has the core.
    THRULINE_CORE_OPTIONAL=1 cargo test --offline

    echo
    echo "== a bundle signed on the OTHER machine, verified here =="
    echo "this machine holds only:"; ls /artifact/
    cargo run --offline --quiet --example verify-bundle -- \
      /artifact/local-bundle.json --public-key /artifact/cosign.pub

    echo
    echo "== a real production keyless bundle, against the trust root alone =="
    cargo run --offline --quiet --example verify-bundle -- \
      /work/fixtures/bundles/cpython-3.13.7.tgz.sigstore.json \
      --trust-root /work/fixtures/trustroot/prod-trusted_root.json
  '

echo
echo "==> a real production Sigstore bundle verified on a machine that held only the trust root."
