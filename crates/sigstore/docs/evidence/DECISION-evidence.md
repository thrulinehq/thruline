# Load-bearing evidence for docs/DECISION.md

Extracted verbatim before the 4.6 GB of vendored source was reaped for host
disk. Both sources are re-obtainable at the exact versions cited:

```
curl -sL https://static.crates.io/crates/sigstore/sigstore-0.14.0.crate | tar xz
git clone https://github.com/sigstore/sigstore-rust
```

## 1. The gap that decided build-over-adopt

sigstore 0.14.0 — `src/bundle/verify/verifier.rs`, steps 5 and 6 of the
bundle verification path. These are the checks that make a transparency log
mean anything, and they are not implemented:

```rust
            .ok_or(SignatureErrorKind::Transparency)?;
        debug!("log entry is consistent with other materials");

        // 5) Verify the inclusion proof supplied by Rekor for this artifact,
        //    if we're doing online verification.
        // TODO(tnytown): Merkle inclusion; sigstore-rs#285

        // 6) Verify the Signed Entry Timestamp (SET) supplied by Rekor for this
        //    artifact.
        // TODO(tnytown) SET verification; sigstore-rs#285

        // 7) Verify that the signing certificate was valid at the time of
        //    signing by comparing the expiry against the integrated timestamp.
        let integrated_time = log_entry.integrated_time as u64;
```

## 2. The intermediate is a trust anchor, not a verified intermediate

sigstore 0.14.0 — `src/bundle/verify/verifier.rs:105`. The intermediates
argument is an empty array, so the intermediate CA is trusted as a root and
its signature by the actual root is never checked:

```rust
        trust_repo: R,
    ) -> SigstoreResult<Self> {
        let cert_pool = CertificatePool::from_certificates(trust_repo.fulcio_certs()?, [])?;
        let ctfe_keyring = Keyring::new(trust_repo.ctfe_keys()?.values().copied())?;

```

## 3. Why sigstore-rust cannot be a dependency of crates/attest

`crates/sigstore-rekor/Cargo.toml` — reqwest and tokio are NOT optional;
the rustls/native-tls features only select reqwest's TLS backend:

```toml
10:[features]
11:default = ["rustls"]
12:rustls = ["reqwest/rustls"]
13:native-tls = ["reqwest/native-tls"]
24:reqwest = { workspace = true }
31:tokio = { workspace = true }
```

`crates/sigstore-crypto/Cargo.toml` — aws-lc-rs (C/BoringSSL) is
non-optional, with no pure-Rust backend feature:

```toml
12:aws-lc-rs = { workspace = true }
```

`crates/sigstore-verify/Cargo.toml` — depends on both, so both are inherited:

```toml
12:sigstore-crypto = { workspace = true }
16:sigstore-rekor = { workspace = true, default-features = false }
```
