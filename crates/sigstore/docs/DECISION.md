# Build vs. adopt: the decision, and the evidence for it

BUILD-SPEC §8.1 leaves this open:

> *Sigstore keyless (Fulcio + Rekor) via the `sigstore` crate; if its attestation
> verification is incomplete, implement verification … or adopt
> `wolfv/sigstore-rust`.*

BUILD-PLAN M2.4 calls it a research spike and says **"verify that claim before
committing to the crate."** This document is that verification.

**Decision: build.** Adopt neither as a dependency; use both as differential
oracles in the test suite. The reasoning below is measured, not recalled — every
number was produced on this machine against vendored source on 2026-09-05.

---

## The spec's premise was right, and its reason was wrong

§8.1 hedges that the official crate's *attestation verification* may be
incomplete. It is. But not where anyone expected.

The gap everyone predicts is SCT verification — it is the one most Sigstore
clients stub. **In `sigstore` 0.14.0 SCT verification is real and it works.** It
reconstructs the precertificate, TLS-serializes the `DigitallySigned` blob, and
checks it against the CTFE keyring. I watched it emit `signing certificate's SCT
is valid` against a real production Fulcio leaf, offline.

The actual gap is the transparency log — the thing Rekor exists for:

```rust
// sigstore-rs-0.14.0/src/bundle/verify/verifier.rs:195-202
// 5) Verify the inclusion proof supplied by Rekor for this artifact,
//    if we're doing online verification.
// TODO(tnytown): Merkle inclusion; sigstore-rs#285

// 6) Verify the Signed Entry Timestamp (SET) supplied by Rekor for this
//    artifact.
// TODO(tnytown) SET verification; sigstore-rs#285
```

Those two comments sit exactly where steps 5 and 6 belong, between step 4
("log entry is consistent with other materials") and step 7 (certificate
validity against `integratedTime`).

The crate *ships working Merkle code* — `src/crypto/merkle/proof_verification.rs:44`
— and the bundle verifier never calls it. I checked the call graph myself:
`verify_inclusion` is reached only from `src/rekor/models/log_entry.rs:144` and
`src/rekor/models/inclusion_proof.rs:87`, both in the online Rekor client. Zero
call sites in `src/bundle/verify/`.

**What that means concretely.** `sigstore` 0.14.0 verifying a bundle checks that
a log entry is *present and self-consistent with the material next to it*. It
does not check that the entry is *in the log*. An inclusion proof is the only
thing that distinguishes a transparency log from a claim about a transparency
log, and in the bundle path it is decoration.

Two consequences follow, and the second is worse than the first:

1. The `integratedTime` used at step 7 to decide whether the certificate was
   valid when it signed is read from an unverified structure. In an offline
   bundle it is attacker-supplied. The one time anchor that makes a ten-minute
   certificate meaningful rests on nothing.
2. `CertificatePool::from_certificates(trust_repo.fulcio_certs()?, [])`
   (`verifier.rs:105`) passes an **empty intermediates list**, and `fulcio_certs()`
   flattens every CA cert — roots *and* intermediates — into the trust-anchor
   set. The Fulcio intermediate is therefore trusted as an anchor and its
   signature by the root is never checked.

Neither is a bug I would have found from a README. Both came out of the source.

> **A correction to my own process.** The first pass "proved" the inclusion-proof
> gap with a tamper matrix — garbage the Merkle proof, observe identical output.
> The adversarial check killed it: that run used a deliberately-wrong artifact
> digest, so `verify_digest` aborted at `verifier.rs:78` long before reaching
> step 5. The experiment never touched the code it claimed to test. The finding
> survives on source evidence and call-graph analysis, which is what is cited
> above. A passing differential test that never executes the path is exactly the
> failure `crates/attest` is built against, and it deserves recording rather
> than quiet replacement.

## §8.1's named fallback does not point where it thinks

`wolfv/sigstore-rust` is a personal fork whose `main` is **0 commits ahead and
124 behind** its parent, last moved 2025-12-31 by `renovate[bot]`. Its issue
tracker is disabled and it still ships a BSD-3-Clause licence file while
upstream is Apache-2.0 — a licence audit that reads the fork gets the wrong
answer. Depending on that URL means pre-audit code with nowhere to report a bug.

But it is not vapour, and the truth is more interesting. Its parent is
**`sigstore/sigstore-rust`** — Wolf Vollprecht's prefix-dev implementation,
transferred *into* the sigstore org (`prefix-dev/sigstore-rust` now 301-redirects
there). It ships as **13 crates on crates.io** at 0.11.0, and it is the better
engineered of the two:

- real Merkle verification (`sigstore-merkle`, 206 RFC 6962 tests) **and it is
  wired into the verify path**, which is precisely `sigstore-rs`'s gap;
- SCT on by default (`verify_sct: true`), with an explicit opt-out;
- bundle v0.3 structurally enforced — single certificate, mandatory inclusion proof;
- a **synchronous** `verify()`, no async runtime;
- trust roots compiled in via `include_str!`, so offline works out of the box;
- 373 tests pass on ThruLine's pinned 1.97.1; CI runs the official
  `sigstore/sigstore-conformance` suite.

So §8.1's sentence needs rewriting on two counts: the fallback's URL is wrong,
and the fallback has become an official-org implementation rather than an
alternative to one.

## Why the better crate still cannot be adopted here

`crates/attest/tests/no_network.rs:77` forbids `reqwest ureq hyper curl isahc
surf attohttpc rustls native-tls openssl tokio sigstore tough` in
`crates/attest/Cargo.toml`. That is the guard carrying §0.5's offline guarantee.

`sigstore-verify` cannot satisfy it:

```
crates/sigstore-rekor/Cargo.toml:24   reqwest = { workspace = true }   # not optional
crates/sigstore-rekor/Cargo.toml:31   tokio   = { workspace = true }   # not optional
crates/sigstore-verify/Cargo.toml:16  sigstore-rekor = { default-features = false }
```

The `rustls` / `native-tls` features only select reqwest's TLS backend. Measured:
`default-features = false` still resolves 138 transitive crates **with tokio and
reqwest present**. CLAUDE.md's "no tokio until the watcher daemon milestone" and
the guard both lose.

### The narrower question: adopt only the pure leaf crates?

Worth asking, because Rule 2 says least machinery. `sigstore-merkle` is exactly
the component `sigstore-rs` lacks, with 206 tests behind it. I measured its real
tree rather than reading manifests:

```
sigstore-merkle 0.11.0 + sigstore-types 0.11.0
  → 68 transitive crates
  → zero crates on attest's forbidden list          ← passes the guard
  → aws-lc-rs 1.18.1 + aws-lc-sys 0.45.0            ← a C/BoringSSL build step
  → sha2 0.10.9, digest 0.10.7                      ← the OLD digest generation
```

The C dependency alone would not disqualify it — ThruLine already compiles C
SQLite through `rusqlite`'s `bundled` feature, so "no C" is not a live principle
here and it would be dishonest to invoke one.

The `sha2` version is the remaining objection, and it is **weaker than this
document first claimed** — corrected here rather than left standing.

What I wrote was that `sigstore-merkle` would put "two generations of the same
hash trait in one graph". Measured on 2026-09-06 against the core's own
lockfile, **the core workspace already carries both**: `sha2` 0.10.9 *and*
0.11.0, `digest` 0.10.7 *and* 0.11.3, pulled by dependencies that have nothing
to do with this crate. So the workspace-level version of that argument was
already false when it was made, and I did not check it.

What survives is narrower and still true: **this crate's own subtree resolves to
exactly one `sha2` and one `digest`**, asserted by
`tests/no_network.rs::there_is_exactly_one_sha2_and_one_digest`, and adopting
`sigstore-merkle` would end that. Together with the `aws-lc-sys` C build step it
brings, that is still a poor trade for ~200 lines of Merkle arithmetic which is
verified here against RFC 6962's vectors *and* an independently-written
recursive reference implementation over every index of every tree up to 33.

But it is a preference about this crate's own hygiene, not the structural
impossibility the earlier wording implied. **The decision does not rest on
it.** It rests on the two findings above: `sigstore` 0.14.0's bundle verifier
does not check the inclusion proof or the SET, and `sigstore-verify` cannot be a
dependency of `crates/attest` at all because `reqwest` and `tokio` are
non-optional beneath it.

`sigstore-bundle` and `sigstore-trust-root` are ruled out earlier: both depend on
`sigstore-rekor` or `sigstore-crypto`, so they inherit reqwest/tokio or aws-lc.
`sigstore-types` alone is clean — serde, base64, hex, jiff, pem — but it is
types, and types are the cheapest part of this to write.

## The other disqualifier: no declared MSRV

Every published version of `sigstore` declares `rust_version = null`. ThruLine
pins its toolchain to 1.97.1 and says why:

> *"byte-identical rebuilds are the project's thesis … so the toolchain that
> produces them is part of the contract"* — `rust-toolchain.toml`

A dependency that declares no MSRV can raise it in a patch release and break a
pinned build with no signal. `sigstore-rust` declares `rust-version = 1.70` on
every version, which is the right behaviour — and another mark in its favour that
the reqwest constraint overrides.

## Decision

| | `sigstore` 0.14.0 | `sigstore-rust` 0.11.0 | build |
|---|---|---|---|
| Merkle inclusion in verify path | **no — TODO** | yes (206 tests) | yes |
| SET verification | **no — TODO** | yes | yes |
| SCT | yes | yes | yes |
| Intermediate verified (not anchored) | **no** | yes | yes |
| Modern identity OIDs .1.8–.1.24 | **no** (.1.1–.1.6 only) | yes | yes |
| Emits bundle v0.3 | **no** (v0.2 only) | yes | yes |
| Satisfies `no_network.rs` | no (`tough`, `tokio`) | **no** (`reqwest`, `tokio`) | yes |
| Declared MSRV | **none** | 1.70 | n/a |
| Transitive crates | 254 | 157 | ~60 core |
| Ships the ToB audit fixes | n/a | **no** (0.11.0 predates all of them) | n/a |

Neither can be a dependency of `crates/attest`, so the choice is not really
between three options — it is between building, and abandoning the guard that
makes ThruLine's offline claim true. §8.1 already anticipated this: *"implement
verification (DSSE signature over PAE, certificate chain to the Sigstore root
via TUF, Rekor inclusion proof)."* That is the branch taken.

**What adoption still buys us, without a dependency edge:** both
implementations, plus `sigstore-conformance`'s 70 fixtures (49 of them negative),
become **differential oracles**. Agreement with two independent implementations
on real bundles is stronger evidence than any self-consistent test suite, and it
costs nothing at runtime.

## Assumptions this decision creates (for §12's register)

1. **Two Rust implementations now live in the sigstore org with no declared
   succession**, and `docs.sigstore.dev` still points only at `sigstore`. One
   will likely be deprecated. Re-check trigger: either repo gaining a
   deprecation notice, or `sigstore-rust` shipping its post-audit release.
2. **`sigstore-rust` 0.11.0 predates every Trail of Bits fix** — roughly 25
   unreleased commits, ~14 breaking, including #160 *"authenticate signature
   timestamps and bind log entries to the bundle."* If that crate is ever
   reconsidered, it must be at the post-audit release, not at 0.11.0.
3. **BUILD-SPEC §8.1 line 528 is factually wrong** and should be corrected to
   name `sigstore/sigstore-rust` (crates `sigstore-verify` / `sigstore-bundle`),
   not `wolfv/sigstore-rust`.
4. The ToB audit report itself is unpublished. Fixes are visible in the commit
   trail; the findings are not. We are reading the patches, not the report.
