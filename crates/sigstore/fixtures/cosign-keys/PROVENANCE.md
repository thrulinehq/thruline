# `fixtures/cosign-keys/` — where each file came from

Every file here was produced by an implementation that is **not this crate**.
A fixture this crate wrote would only prove it agrees with itself, which is the
exact shape of the round-trip test that let `crates/attest` ship an Ed25519
PKCS#8 document neither OpenSSL 3 nor Python `cryptography` could read
(`~/thruline/crates/attest/src/key.rs:70`).

| file | origin | passphrase |
|---|---|---|
| `pemcosignkey.pem` | `cosign/pkg/cosign/keys_test.go:290`, const `pemcosignkey` | `hello` |
| `pemcosigneckey.pem` | `cosign/pkg/cosign/keys_test.go:303`, const `pemcosigneckey` | `hello` |
| `pemsigstorekey.pem` | `cosign/pkg/cosign/keys_test.go:316`, const `pemsigstorekey` | `hello` |
| `cosign-test.key` | `cosign/.github/workflows/` CI key | **not published** |
| `cosign-test.pub` | `cosign/.github/workflows/` CI key | n/a (public) |
| `other-algo-p384.pem` | Go, `gen/other_algorithms.go` | `hello` |
| `other-algo-ed25519.pem` | Go, `gen/other_algorithms.go` | `hello` |
| `other-algo-rsa2048.pem` | Go, `gen/other_algorithms.go` | `hello` |
| `other-algo-p256-control.pem` | Go, `gen/other_algorithms.go` | `hello` |

`cosign-test.key`'s passphrase is not in the repository, so
`a_real_ci_key_has_the_declared_envelope_shape` asserts only what can be
asserted without it — that the envelope parses and declares the shape the
format requires. That is written down rather than left as an unexplained
fixture nothing reads.

`pemcosigneckey.pem` is the interesting one: it exists in cosign's own suite to
prove `LoadPrivateKey` **rejects** a SEC1 payload. It decrypts cleanly with the
passphrase `hello` and is still refused, which is why this crate reports it as
"the passphrase was correct but the bytes are not PKCS#8" rather than as a
passphrase failure.

## `oracle/`

`cosign_key.py` and `xsalsa20poly1305.py` are a from-source reimplementation of
the format: its own XSalsa20-Poly1305 (PyNaCl is not installed anywhere this
runs, and `cryptography` exposes no Salsa20 primitive), `hashlib.scrypt` for the
KDF, and `cryptography` for the key parsing. Every constant in it carries a
citation to the Go file it was derived from —
`go-securesystemslib v0.11.0 encrypted/encrypted.go`,
`cosign pkg/cosign/keys.go`, `sigstore v1.10.8 pkg/signature/ecdsa.go`.

It is the counterparty for the cross-implementation tests in `src/key.rs`,
because the `cosign` binary itself is not installed on the development machine.
**Its absence fails those tests rather than skipping them** — the recon phase's
own version of the same check was an `if let Ok(..)` with no `else` and passed
in silence on a run where its input file was missing.

## `other-algo-*.pem` and `gen/`

`load_cosign_key` returns a `p256::ecdsa::SigningKey`, so it is deliberately
narrower than cosign's `LoadPrivateKey`, which hands whatever
`x509.ParsePKCS8PrivateKey` returns to a generic signer and will load RSA,
Ed25519 or P-384 without comment. That narrowing was **reasoned about rather
than observed** until these fixtures existed: there was no key on another curve
to point the reader at, so "it refuses one" was an argument, not a measurement.

`gen/other_algorithms.go` produces them through cosign's exact marshal path —
`x509.MarshalPKCS8PrivateKey` → `encrypted.Encrypt` → `pem.EncodeToMemory`, the
three calls of `marshalKeyPair` (`cosign/pkg/cosign/keys.go:199`) — against the
vendored `go-securesystemslib v0.11.0`. It reads each file back with
`pem.Decode` + `encrypted.Decrypt` + `x509.ParsePKCS8PrivateKey` before writing
it, so every fixture here is one Go itself loads:

```
other-algo-p384.pem            pkcs8= 185 bytes  -> *ecdsa.PrivateKey
other-algo-ed25519.pem         pkcs8=  48 bytes  -> ed25519.PrivateKey
other-algo-rsa2048.pem         pkcs8=1217 bytes  -> *rsa.PrivateKey
other-algo-p256-control.pem    pkcs8= 138 bytes  -> *ecdsa.PrivateKey
```

The **control** is the point of the set. It is a P-256 key from the same
program and the same run, and it loads here; without it, a refusal of the other
three could just as well mean the generator was broken. It is also a fourth
independent Go-written key that this crate opens, beyond cosign's three
checked-in ones.
