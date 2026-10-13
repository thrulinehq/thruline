# thruline-sigstore

Sigstore keyless signing and **offline** bundle verification, for ThruLine's
BUILD-SPEC §8.1.

This is the crate `~/thruline/crates/attest` deliberately did not build. Its
`lib.rs` says so plainly — *"Not implemented: Sigstore keyless"* — and four
tests in `crates/attest/tests/no_network.rs` actively assert that absence. That
restraint was correct: a stub in a verifier is a code path a reader believes in.
This crate is the other half of the decision.

```rust
let bundle = thruline_sigstore::sign(payload, OidcProvider::staging_device_flow())?;

let policy = Policy::new(TrustRoot::from_file(&root)?)
    .expect_identity("thomas@python.org", "https://accounts.google.com");
let identity = thruline_sigstore::verify(&bundle, &policy)?;
```

`verify` returns the **identity it proved**, not a boolean. A boolean is the
shape that lets a caller write `if verify(..).is_ok()` and never look at *who*
signed — which is the only question a signature answers.

## What it verifies

| | |
|---|---|
| **DSSE signature** | over the PAE bytes, by the key the certificate binds |
| **Certificate chain** | to a CA in the trust root, *including the intermediate's signature by the root* |
| **SCT** | embedded SCT against the CT log's key, over a reconstructed precertificate |
| **Inclusion proof** | RFC 6962 Merkle audit path to a root the log **signed** in its checkpoint |
| **SET** | the log's signature over `integratedTime` — which is what dates the certificate |
| **Identity** | SAN *and its form*, plus the OIDC issuer, from the modern DER-encoded Fulcio extensions as well as the legacy ones |

### Order matters, and it is part of the argument

A Fulcio certificate lives ten minutes and is therefore **always expired** by the
time anyone verifies it. It can only be judged against the instant it signed —
and that instant arrives inside the bundle, supplied by whoever produced it.

So the transparency log is verified **first**. `sigstore` 0.14.0 reads
`integratedTime` to check certificate expiry twenty lines below the two `TODO`s
where the inclusion proof and SET verification should be; its one time anchor
rests on nothing. Here the proof and the SET are checked before the timestamp is
used for anything.

## The core cannot open a socket

`crates/attest` will depend on this crate, and its own guard forbids every HTTP
and TLS crate in its manifest. So the default feature set resolves **108
transitive crates and zero transports**, with a single `sha2 0.11.0` /
`digest 0.11.3` matching the workspace pin.

That is proved, not asserted: `tests/no_network.rs` derives it from `cargo tree`
on every run — a manifest substring scan cannot see a transitive `tokio` — and
it checks the *other* direction too, so a feature gate that silently compiles to
nothing fails the suite.

| feature | default | contents |
|---|---|---|
| (core) | ✔ | bundle, DSSE, X.509, SCT, Merkle, checkpoint, identity, trust root, cosign keys, PKCS#8 |
| `net` | | Fulcio, Rekor (`ureq`, blocking — no tokio) |
| `oidc` | | OAuth device flow |

## Local keys

Reads and writes a **real `cosign.key`**: scrypt (N = 32768 / 65536 / 131072,
writing 65536 as cosign has since v2.2.0) → NaCl secretbox (XSalsa20-Poly1305,
24-byte nonce) → PKCS#8 ECDSA P-256. Given Go's own salt and nonce, this crate
reproduces `go-securesystemslib`'s golden envelopes **byte for byte**.

This closes a gap `crates/attest/src/key.rs` documented rather than papered over:
it ships unencrypted PKCS#8 Ed25519 and says so, "written down as a deviation
rather than described as cosign compatibility it does not have."

PKCS#8 export is verified by the foreign parsers themselves — `tests/pkcs8_interop.rs`
shells out to OpenSSL and Python's `cryptography` — because `attest` recorded a
measured failure worth not repeating: `ed25519-dalek` emits RFC 5958 v2, which
**both** reject for Ed25519.

## Proof

```bash
cargo test --all-features        # 313 tests
scripts/fresh-machine-verify.sh  # a real prod bundle, on a machine with nothing but the trust root
scripts/mutation-check.sh        # break each check on purpose; the suite must notice
```

**`fresh-machine-verify.sh`** runs in a Debian container with `--network=none`,
vendored dependencies, no `~/.sigstore`, and a different OpenSSL build. It tries
to reach Rekor and *fails the run if it can*, so the offline claim is enforced
rather than described.

**`mutation-check.sh`** exists because it found real holes. Stubbing
`verify_signed_by` to return `Ok(())` — never checking any certificate signature
— once left all 157 tests passing; the "wrong trust root" test refused on the
transparency-log axis first and never reached the chain. Two more checks (the
code-signing EKU, and the binding of the log entry to *this* artifact) were
equally uncovered. All five mutations are now caught.

`tests/end_to_end.rs` verifies CPython 3.13.7's **real** release bundle against
the real production trust root, then breaks one thing at a time and asserts both
that it fails and *which axis* it fails on.

## What this build does not do

Stated here rather than discovered later:

- **Rekor v2 (tiled) checkpoints are Ed25519.** They are parsed and then
  honestly refused — the refusal names the algorithm and is deliberately *not*
  `CheckpointSignatureInvalid`, which would accuse the log of something it did
  not do. Verifying them needs an Ed25519 verifier this crate does not have.
- **Witness cosignatures** are not verified. C2SP `cosignature/v1` signs a
  different message than the checkpoint body.
- **RFC 3161 timestamp authorities** (`timestampVerificationData`) are parsed
  and ignored; the log's timestamp is the only time anchor used.
- **Precertificate Signing Certificates** (RFC 6962 §3.1) are unhandled. Fulcio
  has never used one.
- **RSA CT logs** are refused by name. Sigstore's only one is staging's retired
  `ctfe.sigstage.dev/test`.
- **TUF is not a client.** The trust root is treated as data — a file you pin,
  ship or refresh — which is the convention cosign (`--trusted-root`) and
  sigstore-go have both moved to. Refresh is a separate, explicit act.
- A **public-key bundle** (local-key mode) proves possession of a key and
  nothing about who holds it, so `verify` refuses it by name and
  `verify_with_public_key` is the function that fits.

## Why this exists rather than a dependency

See [`docs/DECISION.md`](docs/DECISION.md). In short, measured on this machine:

- `sigstore` 0.14.0's **bundle verifier never checks the Merkle inclusion proof
  or the SET** — two literal `TODO`s where steps 5 and 6 belong. The crate ships
  correct Merkle code one module away and never calls it from that path. It also
  treats the Fulcio intermediate as a trust anchor, implements only six legacy
  identity OIDs, and declares no MSRV on any published version.
- `sigstore/sigstore-rust` (the real project behind §8.1's stale
  `wolfv/sigstore-rust` reference) is better engineered and **cannot be used
  here**: `reqwest` and `tokio` are non-optional in `sigstore-rekor`, which
  `sigstore-verify` depends on.
- Adopting just `sigstore-merkle` was measured too: network-clean, but it brings
  `sha2 0.10` beside the workspace's `0.11`.

Both remain valuable as **differential oracles** in the test suite, which costs
nothing at runtime.

## Merging into ThruLine

[`MERGE-PLAN.md`](MERGE-PLAN.md). One source file in `crates/attest` changes
behaviour — `src/verify.rs` — and the commit is five files. Both numbers are
stated there rather than the flattering one.
