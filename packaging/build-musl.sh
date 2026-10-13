#!/usr/bin/env bash
# Build the Linux release binaries: **static musl**, in Docker.
#
# WHY THIS SCRIPT EXISTS
# ----------------------
# The bench measured a real failure: a `thruline` built against glibc 2.41 does
# not start on a 2.36 image — `libc.so.6: version 'GLIBC_2.39' not found`. The
# fix applied there was to build on the oldest target (`rust:1.97-bookworm`,
# glibc 2.36), which works and which is a standing obligation to keep guessing
# right about every machine anyone will ever run the binary on.
#
# **Static musl closes the libc half and not the arch half**, and the split is
# measured, not assumed. Of the five bench instrument failures first attributed
# to "arch/libc mismatch" — and reported by Terminal-Bench as *"asciinema is not
# installed in the container"*, which would have sent a reader looking for a
# missing recording tool — exactly **one** (`qemu-alpine-ssh`) is the glibc case
# a static binary fixes outright. The other four are genuine architecture
# mismatches: those task Dockerfiles pin `--platform=linux/amd64`, so no aarch64
# binary can execute in them however it is linked. A sixth, `broken-networking`,
# has the same arch mismatch masked behind an earlier apt failure.
#
# That is why this script takes an arch list and why **both** targets are
# published. Building one and calling the class closed would be the same
# overstatement, one layer up.
#
# A statically linked musl binary has no libc to be wrong about. `crt-static`
# is on by default for `*-unknown-linux-musl`, so the output has no interpreter
# and no dynamic dependencies at all, and it runs on Alpine, on Debian
# oldstable, and in a distroless container.
#
# WHY DOCKER RATHER THAN CROSS-COMPILATION
# ----------------------------------------
# macOS has no musl linker, so `cargo build --target x86_64-unknown-linux-musl`
# on this machine fails at link time. Docker's Alpine Rust image already is a
# musl toolchain. Nothing outside Docker has to be provisioned or kept working.
#
# THE VERIFICATION IS PART OF THE BUILD
# -------------------------------------
# It does not just build; it runs `file` and `ldd` on the result and **fails**
# unless the binary is statically linked, then runs it in a *different*
# distro's container. "It compiled" is not the claim being made.
# WHERE THE OUTPUT GOES, AND WHO OWNS IT AFTERWARDS
# -------------------------------------------------
# This writes to `target/musl/`, inside a gitignored build directory. Track E
# consumed it live from there for a day, and a `cargo clean` — the obvious
# reading of "reap your build directory" — would have deleted its inputs
# mid-pilot. It did not, because the reap stopped one path short; that was luck
# dressed as judgement.
#
# The rule that came out of it (DECISIONS #62): **a consumed artefact is copied
# into the consumer's lane with its provenance, and the producer's `target/` is
# then freely reapable.** E carries them at `fixtures/bench/musl/` with a
# committed sha256/BuildID pin naming the producing commit, and refuses a
# mismatch at staging. So this output is scratch again, as a build directory
# should be — and anyone consuming it live is re-creating the problem.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${THRULINE_MUSL_OUT:-$REPO_ROOT/target/musl}"
# Pinned to `rust-toolchain.toml`: the pin is part of the byte-identical
# rebuild contract, and a builder on a different compiler is a different binary.
TOOLCHAIN="$(sed -n 's/^channel *= *"\(.*\)"/\1/p' "$REPO_ROOT/rust-toolchain.toml")"
[ -n "$TOOLCHAIN" ] || { echo "cannot read the channel from rust-toolchain.toml" >&2; exit 1; }

ARCHES="${1:-}"
if [ -z "$ARCHES" ]; then
  case "$(uname -m)" in
    arm64|aarch64) ARCHES="aarch64" ;;
    *)             ARCHES="x86_64" ;;
  esac
  echo "building for the host architecture only ($ARCHES); pass 'x86_64 aarch64' for both" >&2
fi

mkdir -p "$OUT"

for arch in $ARCHES; do
  case "$arch" in
    x86_64)  platform=linux/amd64; target=x86_64-unknown-linux-musl ;;
    aarch64) platform=linux/arm64; target=aarch64-unknown-linux-musl ;;
    *) echo "unknown arch $arch" >&2; exit 1 ;;
  esac
  echo "=== $target ($platform) ==="
  # Named volumes for the registry and the target directory, per architecture.
  # Without them every run is a cold build of ~400 crates, which is slow and —
  # measured — fills Docker's disk: the third rebuild died with
  # `write /dev/stderr: no space left on device` and took Docker Desktop down
  # with it. The volumes are the fix and also the reason a second run is fast.
  # Remove them with `docker volume rm thruline-musl-{registry,target}-$arch`.
  docker volume create "thruline-musl-registry-$arch" >/dev/null
  docker volume create "thruline-musl-target-$arch" >/dev/null
  docker run --rm --platform "$platform" \
    -v "$REPO_ROOT:/src:ro" -v "$OUT:/out" \
    -v "thruline-musl-registry-$arch:/usr/local/cargo/registry" \
    -v "thruline-musl-target-$arch:/w/target" \
    -w /w \
    "rust:${TOOLCHAIN}-alpine" sh -eu -c '
      apk add --no-cache musl-dev >/dev/null
      # `--exclude target`: the cached target volume is mounted there, and
      # copying the host'"'"'s macOS build output over it would poison the cache
      # with objects for the wrong platform.
      tar -C /src -cf - --exclude=./target . | tar -C /w -xf -
      # `--no-default-features`: the CLI defaults to `ort`, whose build script
      # downloads ONNX Runtime from a CDN AT BUILD TIME. A release artifact
      # must not depend on an unpinned mid-build download, and ORT has no musl
      # build at all. The stated consequence is that `annotate` and `eval`
      # refuse in the shipped binary and say why; every truth-loop verb works.
      cargo build --release --locked --no-default-features -p thruline-cli
      cp target/release/thruline /out/thruline-'"$target"'
    '

  bin="$OUT/thruline-$target"
  echo "--- verifying $bin ---"
  # WHAT IS ACTUALLY BEING CLAIMED, AND HOW IT IS CHECKED
  # ----------------------------------------------------
  # The claim is "this binary needs nothing at runtime". Two ELF facts say that
  # and no tool's prose is consulted for either:
  #
  #   * no PT_INTERP segment  -> no program interpreter; the kernel runs it
  #   * no DT_NEEDED entries  -> no shared libraries to resolve
  #
  # This check has now been wrong TWICE, both times by searching for wording
  # instead of the property, and both times it refused a binary that was
  # perfectly good:
  #
  #   1. It grepped `ldd` for glibc's `not a dynamic executable`. musl's loader
  #      says `Not a valid dynamic program`, so it refused the first genuinely
  #      static binary it was ever shown.
  #   2. It grepped `file` for `statically linked`. The two targets do not
  #      produce the same kind of static binary: aarch64-musl yields an
  #      `EXEC`, which `file` calls `statically linked`, while x86_64-musl
  #      yields a **static-pie** — `DYN`, position-independent, no interpreter,
  #      no NEEDED — which `file` calls `static-pie linked`. Refused.
  #
  # REPORT.md tracks this family. `file` and `ldd` are still run and printed,
  # because a human reading this output wants them; neither decides — and for
  # `ldd` that is not a stylistic point. On the x86_64 static-pie, `ldd` prints
  #
  #     /lib/ld-musl-x86_64.so.1 (0x7ffffff58000)
  #
  # which reads exactly like a shared-library dependency and is not one:
  # `readelf` reports zero PT_INTERP and zero DT_NEEDED on the same bytes. A
  # check built on that line would conclude the opposite of the truth, in the
  # unsafe direction, on the artifact five bench tasks depend on.
  docker run --rm --platform "$platform" -v "$OUT:/out" alpine sh -eu -c "
    apk add --no-cache file binutils >/dev/null
    file /out/thruline-$target
    interp=\$(readelf -l /out/thruline-$target | grep -c INTERP || true)
    needed=\$(readelf -d /out/thruline-$target 2>/dev/null | grep -c NEEDED || true)
    if [ \"\$interp\" != 0 ]; then
      echo 'the binary names a program interpreter (PT_INTERP), so it is not static' >&2
      readelf -l /out/thruline-$target | grep -A2 INTERP >&2
      exit 1
    fi
    if [ \"\$needed\" != 0 ]; then
      echo 'the binary has DT_NEEDED entries, so it needs shared libraries at runtime' >&2
      readelf -d /out/thruline-$target | grep NEEDED >&2
      exit 1
    fi
    echo \"no PT_INTERP and no DT_NEEDED: nothing is loaded at runtime.\"
    ldd /out/thruline-$target 2>&1 || true
  "

  # And it runs in a container it was not built in, on a different libc. This is
  # the assertion the glibc failure would have caught: `debian:bookworm-slim` is
  # glibc 2.36, the oldest Terminal-Bench base, and the binary was built on
  # Alpine.
  echo "--- running it on Debian (glibc), built on Alpine (musl) ---"
  docker run --rm --platform "$platform" -v "$OUT:/out:ro" debian:bookworm-slim \
    /out/thruline-"$target" --version
done

echo
echo "artifacts in $OUT:"
ls -l "$OUT"
