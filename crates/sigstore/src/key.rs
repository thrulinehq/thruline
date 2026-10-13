//! Local signing keys, in the format the real `cosign` binary reads and writes.
//!
//! # This closes a gap `crates/attest` wrote down rather than papered over
//!
//! `~/thruline/crates/attest/src/key.rs` says plainly that §8.1 asks for
//! `cosign.key` compatibility and that it ships unencrypted PKCS#8 Ed25519
//! instead, *"written down as a deviation rather than described as cosign
//! compatibility it does not have."* This module is the real thing.
//!
//! # The envelope, exactly
//!
//! A `cosign.key` is a PEM block whose body is JSON:
//!
//! ```json
//! {"kdf":{"name":"scrypt","params":{"N":65536,"r":8,"p":1},"salt":"<b64>"},
//!  "cipher":{"name":"nacl/secretbox","nonce":"<b64>"},
//!  "ciphertext":"<b64>"}
//! ```
//!
//! - PEM type: `ENCRYPTED SIGSTORE PRIVATE KEY`, and the older
//!   `ENCRYPTED COSIGN PRIVATE KEY` — both are read, because refusing the old
//!   one would orphan every key generated before the rename.
//! - **N defaults to 65536, not 32768.** cosign changed the default at v2.2.0.
//!   The reader accepts all three levels it has ever written — 32768 (legacy),
//!   65536 (standard) and 131072 (OWASP) — because a key file states its own
//!   parameters and a reader that only knows the current default cannot open
//!   older keys.
//! - `r = 8`, `p = 1`, 32-byte derived key, 24-byte nonce (XSalsa20-Poly1305,
//!   *not* the ChaCha variant — cosign's files would not open).
//! - The plaintext is **PKCS#8 DER** of an ECDSA P-256 private key.
//!
//! # The order of checks is copied from the reference implementation
//!
//! `cosign/pkg/cosign/keys.go:280` (`LoadPrivateKey`) and
//! `go-securesystemslib/encrypted/encrypted.go:251` (`Decrypt`) between them
//! fix the sequence: PEM type → JSON → `kdf.name` → `cipher.name` →
//! `CheckParams` → scrypt → secretbox open → `ParsePKCS8PrivateKey`. It is
//! reproduced here so that a file cosign refuses is refused here *for the same
//! reason*, which is what makes the error messages worth reading.
//!
//! **`CheckParams` is a denial-of-service guard, not a taste preference.** Its
//! own comment says so: the cost parameters are attacker-controlled input, and
//! `N = 2^30` in a file is a way to make the reader spend a machine's memory on
//! opening it. That is why an unrecognised cost is [`Error::ScryptParams`] —
//! refused before any work is done — and not "tried anyway, just in case".
//!
//! # Where this is deliberately *not* stricter than cosign
//!
//! Go's `scryptKDF.Key` passes the salt straight to `scrypt.Key` and **never
//! checks its length**, so a file with a 16-byte salt opens in real cosign.
//! This reader does not check it either. Writing 32 bytes and refusing to read
//! anything else would produce a verifier that rejects files the reference
//! implementation accepts, and "your key is corrupt" is a bad thing to say
//! about a key that is not. The nonce length *is* checked, because Go checks it
//! too (`secretBoxCipher.Decrypt` returns `encrypted: incorrect nonce size`).

use std::io::Write as _;
use std::path::Path;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use crypto_secretbox::aead::{Aead, KeyInit};
use crypto_secretbox::{Nonce, XSalsa20Poly1305};
use p256::elliptic_curve::Generate as _;
use p256::pkcs8::{DecodePrivateKey, EncodePrivateKey, LineEnding};
use serde::{Deserialize, Serialize};
use spki::EncodePublicKey as _;

use crate::error::{Error, Result};

pub const PEM_TYPE: &str = "ENCRYPTED SIGSTORE PRIVATE KEY";
pub const PEM_TYPE_LEGACY: &str = "ENCRYPTED COSIGN PRIVATE KEY";

/// scrypt cost levels cosign has shipped. A file names its own.
pub const SCRYPT_LEGACY_N: u32 = 32768;
pub const SCRYPT_STANDARD_N: u32 = 65536;
pub const SCRYPT_OWASP_N: u32 = 131072;

/// The block-mix and parallelism factors. cosign has only ever written these
/// two values, and `CheckParams` compares all three fields, so an `r` or `p`
/// that differs is as unacceptable as an unknown `N`.
pub const SCRYPT_R: u32 = 8;
pub const SCRYPT_P: u32 = 1;

/// What [`write_cosign_key`] should be given unless a caller has a reason not
/// to: the level `encrypted.Encrypt` has defaulted to since cosign v2.2.0.
pub const SCRYPT_DEFAULT_N: u32 = SCRYPT_STANDARD_N;

const KDF_NAME: &str = "scrypt";
const CIPHER_NAME: &str = "nacl/secretbox";

/// `boxKeySize`, `boxNonceSize`, `saltSize` — `encrypted.go:19,22,23`.
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 24;
const SALT_LEN: usize = 32;
/// Poly1305 tag, which NaCl's `Seal` layout puts *first*.
const TAG_LEN: usize = 16;

/// [`Error::Key`] wants a path, and the two decode entry points are handed PEM
/// text rather than a file. Naming the artifact instead of inventing a path
/// keeps the message honest: nothing here knows where the bytes came from.
const WHERE: &str = "cosign private key";

/// The refusal [`generate`] returns rather than clobbering a key.
///
/// Exposed as a constant so a caller — and the test suite — can recognise this
/// particular failure without matching on the text of a message. It is the one
/// [`Error::Key`] case that is not "this file is malformed", and mistaking it
/// for one would lead a caller to *delete and retry*, which is exactly the
/// outcome the refusal exists to prevent.
pub const REFUSING_TO_OVERWRITE: &str =
    "a key already exists here; refusing to overwrite it. An overwritten signing key silently \
     invalidates every receipt already issued under it, and the damage surfaces later as a \
     verification failure that reads like tampering";

/// The refusal [`write_cosign_key`] and [`generate`] return for an empty
/// passphrase.
///
/// [`generate`]'s contract has always said *"the passphrase is not optional …
/// this build does not offer the choice"*, and until this constant existed it
/// was a sentence rather than a check: `generate(k, p, b"")` succeeded and
/// wrote a file whose envelope advertises `scrypt N=65536` while being
/// openable by anyone who has it, because `scrypt("", salt)` needs nothing the
/// file does not already carry. A key file that *looks* protected and is not
/// is worse than a plainly unencrypted one — nobody guards what the header
/// says is already guarded.
///
/// This is deliberately narrower than cosign, which does offer the choice:
/// `marshalKeyPair` starts from `password := []byte{}` and a nil `PassFunc`
/// leaves it empty (`cosign/pkg/cosign/keys.go:205`). Reading such a key is
/// still supported — refusing it would orphan every key already written that
/// way, and this build refuses to be the one that calls a real key corrupt —
/// so the narrowing is only on the write side, where the choice is still ours
/// to make.
///
/// A constant rather than a variant of [`Error`] for the same reason as
/// [`REFUSING_TO_OVERWRITE`]: `src/error.rs` is shared, its `axis()` match is
/// exhaustive by design, and a test can assert equality against this without
/// substring-matching a message.
pub const REFUSING_EMPTY_PASSPHRASE: &str =
    "refusing to write a signing key with an empty passphrase. The envelope would still declare \
     scrypt, so the file would look protected while anyone holding it could open it; if the key \
     is genuinely meant to sit unprotected on disk, that decision belongs somewhere it can be \
     seen, not in an empty string";

// The field order below **is** the wire order. Go's `encoding/json` emits
// struct fields in declaration order and `serde_json` does the same, which is
// what lets `write_cosign_key` reproduce a Go-written envelope byte for byte
// given the same salt and nonce — proven in
// `re_encrypting_a_go_golden_vector_is_byte_identical`. Reordering these
// fields would silently break that equality and nothing else would complain.
#[derive(Serialize, Deserialize)]
struct Envelope {
    kdf: Kdf,
    cipher: CipherSpec,
    ciphertext: String,
}

#[derive(Serialize, Deserialize)]
struct Kdf {
    name: String,
    params: ScryptParams,
    salt: String,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct ScryptParams {
    #[serde(rename = "N")]
    n: u32,
    r: u32,
    p: u32,
}

#[derive(Serialize, Deserialize)]
struct CipherSpec {
    name: String,
    nonce: String,
}

fn malformed(reason: impl Into<String>) -> Error {
    Error::Key {
        path: WHERE.to_string(),
        reason: reason.into(),
    }
}

/// Map the three accepted cost levels onto scrypt's `log2(N)`.
///
/// `scrypt::Params::new` takes **log2(N), not N** (`scrypt-0.12.0/src/params.rs:60`).
/// Passing 65536 where 16 is expected asks for `N = 2^65536`; the constructor
/// refuses it, so the mistake shows up as "unsupported parameters" on a
/// perfectly ordinary key file. The mapping is written out rather than derived
/// with `trailing_zeros` so that the whitelist and the exponent cannot drift
/// apart.
fn scrypt_log_n(params: ScryptParams) -> Result<u8> {
    match (params.n, params.r, params.p) {
        (SCRYPT_LEGACY_N, SCRYPT_R, SCRYPT_P) => Ok(15),
        (SCRYPT_STANDARD_N, SCRYPT_R, SCRYPT_P) => Ok(16),
        (SCRYPT_OWASP_N, SCRYPT_R, SCRYPT_P) => Ok(17),
        _ => Err(Error::ScryptParams {
            n: params.n,
            r: params.r,
            p: params.p,
        }),
    }
}

fn derive_key(passphrase: &[u8], salt: &[u8], params: ScryptParams) -> Result<[u8; KEY_LEN]> {
    let log_n = scrypt_log_n(params)?;
    let p = scrypt::Params::new(log_n, SCRYPT_R, SCRYPT_P).map_err(|_| Error::ScryptParams {
        n: params.n,
        r: params.r,
        p: params.p,
    })?;
    let mut out = [0u8; KEY_LEN];
    scrypt::scrypt(passphrase, salt, &p, &mut out).map_err(|_| Error::ScryptParams {
        n: params.n,
        r: params.r,
        p: params.p,
    })?;
    Ok(out)
}

fn b64(field: &'static str, value: &str) -> Result<Vec<u8>> {
    B64.decode(value).map_err(|e| Error::Base64 {
        field,
        reason: e.to_string(),
    })
}

/// `encrypted.Decrypt`, one check at a time and in Go's order.
fn open_envelope(json: &[u8], passphrase: &[u8]) -> Result<Vec<u8>> {
    let e: Envelope = serde_json::from_slice(json)?;
    if e.kdf.name != KDF_NAME {
        return Err(malformed(format!(
            "unknown kdf name {:?}; this build derives keys with {KDF_NAME} only",
            e.kdf.name
        )));
    }
    if e.cipher.name != CIPHER_NAME {
        return Err(malformed(format!(
            "unknown cipher name {:?}; this build opens {CIPHER_NAME} only",
            e.cipher.name
        )));
    }
    // Before the KDF runs, not after: see the DoS note in the module docs.
    let key = derive_key(passphrase, &b64("kdf.salt", &e.kdf.salt)?, e.kdf.params)?;

    let nonce = b64("cipher.nonce", &e.cipher.nonce)?;
    let nonce: [u8; NONCE_LEN] = nonce.as_slice().try_into().map_err(|_| {
        malformed(format!(
            "the nonce is {} bytes; XSalsa20-Poly1305 takes {NONCE_LEN}",
            nonce.len()
        ))
    })?;

    let ct = b64("ciphertext", &e.ciphertext)?;
    if ct.len() < TAG_LEN {
        // Named separately from a failed authentication because it is a
        // different fact: this file cannot contain a tag at all, so no
        // passphrase in the world opens it and suggesting otherwise sends the
        // user hunting for a password that was never the problem.
        return Err(malformed(format!(
            "the ciphertext is {} bytes, shorter than the {TAG_LEN}-byte Poly1305 tag that \
             precedes it",
            ct.len()
        )));
    }

    // VERIFIED against `golang.org/x/crypto/nacl/secretbox`, do not "fix":
    // `crypto_secretbox` 0.1.1 already uses NaCl's tag-first layout ("Make room
    // in the buffer for the tag. It needs to be prepended." — lib.rs:250),
    // which is exactly what Go's `secretbox.Seal` produces. Assuming the usual
    // RustCrypto ciphertext-then-tag convention and shuffling the bytes here
    // breaks every real cosign file, and the only symptom is "wrong
    // passphrase".
    XSalsa20Poly1305::new((&key).into())
        .decrypt(&Nonce::from(nonce), ct.as_ref())
        // Poly1305 cannot tell a wrong key from a flipped byte — both are just
        // a tag that does not match. `Error::PassphraseWrong` is the honest
        // reading of *"the passphrase does not decrypt this key"*, and it is
        // also what cosign reports (`encrypted: decryption failed`), but it is
        // worth knowing that a corrupted ciphertext lands here too.
        .map_err(|_| Error::PassphraseWrong)
}

/// `encrypted.EncryptWithCustomKDFParameters`, with the salt and nonce supplied.
///
/// Kept private and separate from [`write_cosign_key`] for one reason: a format
/// claim that cannot be reproduced byte for byte against another implementation
/// is not a proof. With the salt and nonce pinned, this function's output must
/// equal Go's on the same inputs, and the tests assert exactly that against
/// `go-securesystemslib`'s own golden vectors.
///
/// It is **not** public, and that is deliberate. Reusing a `(passphrase, salt,
/// nonce)` triple across two different plaintexts reuses an XSalsa20 keystream
/// and leaks their XOR; the only safe callers are the tests that replay a
/// recorded envelope.
fn seal_envelope(
    plaintext: &[u8],
    passphrase: &[u8],
    n: u32,
    salt: &[u8],
    nonce: &[u8; NONCE_LEN],
) -> Result<Vec<u8>> {
    let params = ScryptParams {
        n,
        r: SCRYPT_R,
        p: SCRYPT_P,
    };
    let key = derive_key(passphrase, salt, params)?;
    let sealed = XSalsa20Poly1305::new((&key).into())
        .encrypt(&Nonce::from(*nonce), plaintext)
        .map_err(|_| malformed("XSalsa20-Poly1305 refused to seal the private key"))?;
    let envelope = Envelope {
        kdf: Kdf {
            name: KDF_NAME.to_string(),
            params,
            salt: B64.encode(salt),
        },
        cipher: CipherSpec {
            name: CIPHER_NAME.to_string(),
            nonce: B64.encode(nonce),
        },
        ciphertext: B64.encode(&sealed),
    };
    serde_json::to_vec(&envelope).map_err(Error::Json)
}

/// Fill `buf` from the operating system's CSPRNG, or fail.
///
/// There is no fallback. A salt or a nonce from a degraded source is worse than
/// no key at all, because the file it produces looks exactly like a good one.
fn fill_random(buf: &mut [u8]) -> Result<()> {
    getrandom::fill(buf).map_err(|e| {
        malformed(format!(
            "the operating system's random number generator failed ({e}); refusing to write a \
             key with a salt or nonce this build cannot vouch for"
        ))
    })
}

/// The first PEM block, cut at the end of its post-encapsulation boundary.
///
/// # A strict PEM grammar called a good key corrupt
///
/// `der::pem` implements RFC 7468's **strict** grammar, in which a single byte
/// after the closing `-----` makes the whole document unparseable. cosign does
/// not use that grammar. `LoadPrivateKey` is `p, _ := pem.Decode(key)`
/// (`cosign/pkg/cosign/keys.go:282`), and Go's `encoding/pem.Decode` returns
/// the first block and hands the caller the remainder to throw away.
///
/// Measured, not assumed. Go's own `pem.Decode` was run against this crate's
/// `pemsigstorekey.pem` fixture with each of the following appended, and every
/// time it returned the *identical* block body (SHA-256 `89c7aab7a234ed11…`):
///
/// | appended to a valid `cosign.key` | Go / cosign | this crate, before |
/// |---|---|---|
/// | nothing | accepts | accepts |
/// | one `\n` | accepts | **refused** |
/// | two `\n` | accepts | **refused** |
/// | `\r\n` | accepts | **refused** |
/// | one space | accepts | **refused** |
/// | a second PEM block | accepts | **refused** |
///
/// The second row is the one that matters, because it is how a signing key
/// most often reaches disk: `echo "$COSIGN_KEY" > cosign.key` in a CI job
/// appends exactly one newline. The refusal it produced was *"not a PEM block
/// this build can read"* — the same "your key is corrupt" lie about an
/// uncorrupted key that this module already refuses to tell about a short
/// salt, and one that would send a user re-exporting a key that was fine.
///
/// # Only the framing is relaxed
///
/// What lies *between* the boundaries is still handed to the strict decoder —
/// label grammar, base64 alphabet, line width, matching END label — so a
/// truncated or re-typed body is refused exactly as before. The relaxation can
/// only ever discard bytes after the first block's closing `-----`, and
/// discarding trailing bytes cannot turn a malformed encapsulation into a
/// well-formed one.
///
/// A *second* block is ignored rather than merged, which is byte-for-byte what
/// cosign does with it. Both implementations therefore load the **same** key
/// from the same file. That agreement is the point: quietly disagreeing about
/// *which* block is the key would be far worse than refusing both, and is the
/// reason this cuts at the first boundary instead of the last.
fn first_pem_block(pem: &str) -> &str {
    const BEGIN: &str = "-----BEGIN ";
    // Anchored to a line start. `-` is not in the base64 alphabet, so this
    // cannot match inside encapsulated text, but anchoring keeps that true
    // without depending on it.
    const END: &str = "\n-----END ";
    const DASHES: &str = "-----";

    // No boundary at all: hand the original text to the strict decoder, so the
    // problem is named by the parser rather than invented here.
    let Some(begin) = pem.find(BEGIN) else {
        return pem;
    };
    let after = &pem[begin..];
    let Some(end) = after.find(END) else {
        return pem;
    };
    // `end` indexes the newline; the boundary itself starts one byte later.
    let boundary = end + 1;
    let Some(close) = after[boundary + END.len() - 1..].find(DASHES) else {
        return pem;
    };
    // Cut immediately after the boundary's closing `-----`. Every trailing
    // newline, space and appended block falls away in this one place, and a
    // CRLF document is unaffected because the cut lands before its `\r\n`.
    &pem[..begin + boundary + END.len() - 1 + close + DASHES.len()]
}

/// Read an encrypted `cosign.key`, returning the P-256 signing key.
///
/// Both PEM types are accepted, and the framing is cosign's rather than RFC
/// 7468's strict one — see [`first_pem_block`] for what that costs and why.
/// Everything after the type check follows `LoadPrivateKey`'s order, so the
/// failure a caller sees names the same problem cosign would name.
///
/// The one place this is narrower than cosign: `LoadPrivateKey` hands whatever
/// `ParsePKCS8PrivateKey` returns to a generic signer factory and will happily
/// load an RSA or Ed25519 key. This returns a `p256::ecdsa::SigningKey`, so a
/// key on another curve is refused **by name** rather than mis-loaded — the
/// Sigstore algorithm registry binds P-256 to SHA-256 here, and silently
/// treating some other key as one is the kind of confusion that produces
/// signatures nothing can verify.
pub fn load_cosign_key(pem: &str, passphrase: &[u8]) -> Result<p256::ecdsa::SigningKey> {
    let (label, body) = der::pem::decode_vec(first_pem_block(pem).as_bytes())
        .map_err(|e| malformed(format!("not a PEM block this build can read: {e}")))?;
    if label != PEM_TYPE && label != PEM_TYPE_LEGACY {
        return Err(malformed(format!(
            "PEM type is {label:?}; a cosign private key is {PEM_TYPE:?} or the legacy \
             {PEM_TYPE_LEGACY:?}"
        )));
    }
    let der = open_envelope(&body, passphrase)?;
    // A successful decrypt with an unparseable payload is *not* a passphrase
    // problem, and saying so would be a lie the user cannot act on. cosign's
    // own `pemcosigneckey.pem` test fixture is precisely this case: it opens,
    // and the SEC1 body inside it is still refused.
    p256::ecdsa::SigningKey::from_pkcs8_der(&der).map_err(|e| {
        malformed(format!(
            "the passphrase was correct but the {} decrypted bytes are not PKCS#8 for an ECDSA \
             P-256 key: {e}",
            der.len()
        ))
    })
}

/// Write a `cosign.key`-format encrypted PEM that real cosign can read.
///
/// `n` must be one of the three levels cosign has shipped; pass
/// [`SCRYPT_DEFAULT_N`] unless you have a reason. A fresh salt and nonce are
/// drawn from the system CSPRNG on every call — never derived from the
/// passphrase, never reused — because a repeated `(salt, nonce)` under the same
/// passphrase leaks the XOR of the two private keys sealed under it.
///
/// The passphrase must not be empty; see [`REFUSING_EMPTY_PASSPHRASE`]. This is
/// the one place this build is deliberately narrower than cosign on the write
/// side, and it is enforced here rather than in [`generate`] so that both
/// entry points inherit it from one check.
pub fn write_cosign_key(
    key: &p256::ecdsa::SigningKey,
    passphrase: &[u8],
    n: u32,
) -> Result<String> {
    // Both validations happen *before* the RNG or the key material is touched,
    // so a refused call costs nothing and leaves nothing behind.
    if passphrase.is_empty() {
        return Err(malformed(REFUSING_EMPTY_PASSPHRASE));
    }
    scrypt_log_n(ScryptParams {
        n,
        r: SCRYPT_R,
        p: SCRYPT_P,
    })?;

    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    fill_random(&mut salt)?;
    fill_random(&mut nonce)?;

    let der = pkcs8_der(key)?;
    let envelope = seal_envelope(&der, passphrase, n, &salt, &nonce)?;
    der::pem::encode_string(PEM_TYPE, LineEnding::LF, &envelope)
        .map_err(|e| malformed(format!("could not PEM-encode the key envelope: {e}")))
}

/// `cosign.pub`: PEM `PUBLIC KEY`, PKIX/SPKI DER.
///
/// Byte-identical to what cosign writes for the same key — asserted against the
/// public halves that Python's `cryptography` derives from cosign's own test
/// key files, rather than against this crate's own opinion of the encoding.
pub fn public_key_pem(key: &p256::ecdsa::SigningKey) -> Result<String> {
    key.verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .map_err(|e| malformed(format!("could not encode the public key as SPKI PEM: {e}")))
}

/// PKCS#8 export that OpenSSL and Python's `cryptography` both read.
///
/// `crates/attest/src/key.rs:70` records the measured trap this avoids:
/// `ed25519-dalek` emits RFC 5958 OneAsymmetricKey **v2**, and both OpenSSL 3
/// and Python `cryptography` reject it for Ed25519. P-256 has no v0/v2 split in
/// the same form — `p256` emits RFC 5208 v0 — but the lesson is that the claim
/// is only worth what a foreign parser says, so `tests/pkcs8_interop.rs` runs
/// both parsers rather than asserting this in a comment.
pub fn pkcs8_pem(key: &p256::ecdsa::SigningKey) -> Result<String> {
    key.to_pkcs8_pem(LineEnding::LF)
        .map(|pem| pem.to_string())
        .map_err(|e| {
            malformed(format!(
                "could not encode the private key as PKCS#8 PEM: {e}"
            ))
        })
}

/// The same bytes as [`pkcs8_pem`], unarmoured.
///
/// This is also the exact plaintext sealed inside a `cosign.key`, which is why
/// the two share a code path: if the export were encoded differently from the
/// thing stored in the envelope, a key written here would round-trip through
/// this crate and fail in cosign.
///
/// The returned `Vec` is **private key material and is not zeroized on drop**.
/// `to_pkcs8_der` hands back a `SecretDocument` that would wipe itself; the
/// conversion to `Vec<u8>` gives that up, and the caller owns what happens
/// next.
pub fn pkcs8_der(key: &p256::ecdsa::SigningKey) -> Result<Vec<u8>> {
    key.to_pkcs8_der()
        .map(|doc| doc.as_bytes().to_vec())
        .map_err(|e| {
            malformed(format!(
                "could not encode the private key as PKCS#8 DER: {e}"
            ))
        })
}

/// Create `path` and fail if it is already there.
///
/// `create_new` rather than `path.exists()` followed by a write: the gap
/// between asking and writing is a race a second process can win, and the loser
/// is a key file with two authors. On unix the mode is set **at creation**, so
/// the private key is never briefly world-readable — a `set_permissions` call
/// after the write leaves exactly that window open.
fn create_exclusive(path: &Path, mode: u32) -> Result<std::fs::File> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        opts.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    opts.open(path).map_err(|source| {
        if source.kind() == std::io::ErrorKind::AlreadyExists {
            Error::Key {
                path: path.display().to_string(),
                reason: REFUSING_TO_OVERWRITE.to_string(),
            }
        } else {
            Error::Io {
                path: path.display().to_string(),
                source,
            }
        }
    })
}

fn write_all(mut file: std::fs::File, path: &Path, bytes: &[u8]) -> Result<()> {
    file.write_all(bytes).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })
}

/// Write both halves, private key `0600`, refusing to overwrite.
///
/// Mirrors `crates/attest`'s `KeyError::Exists` reasoning, and extends it to
/// the public half: a stale `cosign.pub` left beside a freshly generated
/// `cosign.key` is not a smaller problem than an overwritten private key — it
/// is the same problem wearing a disguise, since every verifier reaches for the
/// public file and would refuse signatures the private file can make.
///
/// Both files are claimed *before* either is written, and the private one is
/// removed if claiming the public one fails. A half-generated pair would leave
/// a private key that the next run then refuses to overwrite, turning one
/// transient failure into a permanently wedged directory.
///
/// The passphrase is not optional, and an **empty** one is not a passphrase.
/// cosign will write an effectively unencrypted key when told to; this build
/// does not offer the choice, because the whole reason this format exists
/// rather than plain PKCS#8 is that the key sits on disk. That refusal is
/// [`REFUSING_EMPTY_PASSPHRASE`], and it is enforced by [`write_cosign_key`]
/// rather than here: the empty passphrase is the one reachable input that
/// fails *after* both files are claimed, which is what puts the post-claim
/// cleanup below under test instead of leaving it a claim.
pub fn generate(private: &Path, public: &Path, passphrase: &[u8]) -> Result<()> {
    if let Some(parent) = private.parent() {
        if !parent.as_os_str().is_empty() {
            // Reported, not swallowed. `attest`'s version discards this error
            // and lets the confusing "no such file or directory" surface from
            // the write instead.
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                path: parent.display().to_string(),
                source,
            })?;
        }
    }

    let private_file = create_exclusive(private, 0o600)?;
    let public_file = match create_exclusive(public, 0o644) {
        Ok(f) => f,
        Err(e) => {
            let _ = std::fs::remove_file(private);
            return Err(e);
        }
    };

    let filled = (move || -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            // Creation already restricts the private file. Set both exact
            // modes before writing, independently of the caller's umask.
            for (file, path, mode) in [
                (&private_file, private, 0o600),
                (&public_file, public, 0o644),
            ] {
                file.set_permissions(std::fs::Permissions::from_mode(mode))
                    .map_err(|source| Error::Io {
                        path: path.display().to_string(),
                        source,
                    })?;
            }
        }
        let key = p256::ecdsa::SigningKey::try_generate_from_rng(&mut getrandom::SysRng).map_err(
            |e| {
                malformed(format!(
                    "the operating system's random number generator failed while generating the \
                     signing key ({e})"
                ))
            },
        )?;
        write_all(
            private_file,
            private,
            write_cosign_key(&key, passphrase, SCRYPT_DEFAULT_N)?.as_bytes(),
        )?;
        write_all(public_file, public, public_key_pem(&key)?.as_bytes())
    })();

    if filled.is_err() {
        // Both files were claimed before either was written, so anything that
        // fails here leaves two empty files that the next run would refuse to
        // overwrite — one transient failure turned into a permanently wedged
        // directory. The cleanup's own errors are discarded deliberately:
        // reporting them would replace the failure the caller needs to see.
        let _ = std::fs::remove_file(private);
        let _ = std::fs::remove_file(public);
    }
    filled
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture provenance is in `fixtures/cosign-keys/PROVENANCE.md`. Every
    /// file under it was produced by Go, not by this crate — a fixture this
    /// crate wrote would only prove it agrees with itself.
    fn fixture(name: &str) -> String {
        let path = format!("{}/fixtures/cosign-keys/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {path}: {e}"))
    }

    // --- go-securesystemslib v0.11.0 encrypted/encrypted_test.go:21-25 -----
    //
    // Envelopes Go wrote, verbatim. Passphrase "supersecret". The three
    // `kdfVectors` wrap `Marshal`'s output, so their plaintext is the JSON
    // encoding of the []byte "reallyimportant" — a base64 string in quotes,
    // not the bytes themselves. `TestDecrypt`'s vector (line 63) wraps the raw
    // bytes.
    const GO_LEGACY: &str = r#"{"kdf":{"name":"scrypt","params":{"N":32768,"r":8,"p":1},"salt":"WO3mVvyTwJ9vwT5/Tk5OW5WPIBUofMjcpEfrLnfY4uA="},"cipher":{"name":"nacl/secretbox","nonce":"tCy7HcTFr4uxv4Nrg/DWmncuZ148U1MX"},"ciphertext":"08n43p5G5yviPEZpO7tPPF4aZQkWiWjkv4taFdhDBA0tamKH4nw="}"#;
    const GO_STANDARD: &str = r#"{"kdf":{"name":"scrypt","params":{"N":65536,"r":8,"p":1},"salt":"FhzPOt9/bJG4PTq6lQ6ecG6GzaOuOy/ynG5+yRiFlNs="},"cipher":{"name":"nacl/secretbox","nonce":"aw1ng1jHaDz/tQ7V2gR9O2+IGQ8xJEuE"},"ciphertext":"HycvuLZL4sYH0BrYTh4E/H20VtAW6u5zL5Pr+IBjYLYnCPzDkq8="}"#;
    const GO_OWASP: &str = r#"{"kdf":{"name":"scrypt","params":{"N":131072,"r":8,"p":1},"salt":"m38E3kouJTtiheLQN22NQ8DTito5hrjpUIskqcd375k="},"cipher":{"name":"nacl/secretbox","nonce":"Y6PM13yA+o44pE/W1ZBwczeGnTV/m9Zc"},"ciphertext":"6H8sqj1K6B6yDjtH5AQ6lbFigg/C2yDDJc4rYJ79w9aVPImFIPI="}"#;
    const GO_TEST_DECRYPT: &str = r#"{"kdf":{"name":"scrypt","params":{"N":32768,"r":8,"p":1},"salt":"N9a7x5JFGbrtB2uBR81jPwp0eiLR4A7FV3mjVAQrg1g="},"cipher":{"name":"nacl/secretbox","nonce":"2h8HxMmgRfuYdpswZBQaU3xJ1nkA/5Ik"},"ciphertext":"SEW6sUh0jf2wfdjJGPNS9+bkk2uB+Cxamf32zR8XkQ=="}"#;
    const GO_PASSPHRASE: &[u8] = b"supersecret";

    /// The passphrase cosign's own `keys_test.go` uses for its embedded PEM
    /// fixtures (`cosign-v3-pkg-cosign-keys_test.go:387`).
    const COSIGN_PASSPHRASE: &[u8] = b"hello";

    /// Public halves of the two cosign fixtures, as derived by Python's
    /// `cryptography` 49.0.0 from the decrypted PKCS#8 — a foreign
    /// implementation's answer, recorded so this crate cannot grade its own
    /// homework.
    const PEMCOSIGNKEY_PUB: &str = "-----BEGIN PUBLIC KEY-----\n\
        MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAECYtspttZZ7e9PMd8wyU2CWDlpEXN\n\
        bqTve0ZBuOXu9xTjtm7Z107cVLCCk96Ma4D0tesnKeCPwrcspLz7GNZJSw==\n\
        -----END PUBLIC KEY-----\n";
    const PEMSIGSTOREKEY_PUB: &str = "-----BEGIN PUBLIC KEY-----\n\
        MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE7z9gA/tqa9iHt/f9F3ulyVi4lwqU\n\
        LfHWfhG2mK4eyJ8LDcLstb+VVhxj3pkMjc3fNo+sSfiIY2NLR7c+DG1V4w==\n\
        -----END PUBLIC KEY-----\n";

    fn envelope_of(pem: &str) -> Vec<u8> {
        der::pem::decode_vec(pem.as_bytes()).expect("pem").1
    }

    fn a_key() -> p256::ecdsa::SigningKey {
        p256::ecdsa::SigningKey::try_generate_from_rng(&mut getrandom::SysRng).expect("rng")
    }

    // --- the envelope, against Go's own bytes -----------------------------

    #[test]
    fn all_three_cost_levels_open_a_go_written_envelope() {
        for (level, v) in [
            ("Legacy", GO_LEGACY),
            ("Standard", GO_STANDARD),
            ("OWASP", GO_OWASP),
        ] {
            let pt = open_envelope(v.as_bytes(), GO_PASSPHRASE)
                .unwrap_or_else(|e| panic!("{level}: {e}"));
            assert_eq!(pt, br#""cmVhbGx5aW1wb3J0YW50""#, "{level}");
        }
        assert_eq!(
            open_envelope(GO_TEST_DECRYPT.as_bytes(), GO_PASSPHRASE).unwrap(),
            b"reallyimportant"
        );
    }

    /// The strongest claim this module makes: given Go's salt and nonce, the
    /// bytes written here are Go's bytes. That covers the JSON field order,
    /// the compact separators, the base64 alphabet, the scrypt cost mapping
    /// **and** the tag-first secretbox layout in one equality — any one of
    /// which, wrong, would still produce a file that opens here and nowhere
    /// else.
    #[test]
    fn re_encrypting_a_go_golden_vector_is_byte_identical() {
        for (level, v) in [
            ("Legacy", GO_LEGACY),
            ("Standard", GO_STANDARD),
            ("OWASP", GO_OWASP),
            ("TestDecrypt", GO_TEST_DECRYPT),
        ] {
            let e: Envelope = serde_json::from_str(v).expect("vector json");
            let pt = open_envelope(v.as_bytes(), GO_PASSPHRASE).expect("open");
            let salt = B64.decode(&e.kdf.salt).expect("salt");
            let nonce: [u8; NONCE_LEN] = B64
                .decode(&e.cipher.nonce)
                .expect("nonce")
                .try_into()
                .expect("24 bytes");
            let again =
                seal_envelope(&pt, GO_PASSPHRASE, e.kdf.params.n, &salt, &nonce).expect("seal");
            assert_eq!(String::from_utf8(again).expect("utf8"), v, "{level}");
        }
    }

    // --- real cosign key files --------------------------------------------

    #[test]
    fn both_pem_types_are_read_and_yield_the_public_key_python_derives() {
        for (file, want_type, want_pub) in [
            ("pemcosignkey.pem", PEM_TYPE_LEGACY, PEMCOSIGNKEY_PUB),
            ("pemsigstorekey.pem", PEM_TYPE, PEMSIGSTOREKEY_PUB),
        ] {
            let pem = fixture(file);
            assert_eq!(
                der::pem::decode_vec(pem.as_bytes()).expect("pem").0,
                want_type,
                "{file}"
            );
            let key =
                load_cosign_key(&pem, COSIGN_PASSPHRASE).unwrap_or_else(|e| panic!("{file}: {e}"));
            assert_eq!(public_key_pem(&key).expect("pub"), want_pub, "{file}");
        }
    }

    /// cosign's `cosign-test.key` from its own CI workflow. Its passphrase is
    /// not published, so this asserts only what can be asserted without it —
    /// that the envelope parses and declares the shape the format requires.
    /// Stated here rather than left as a silently unused fixture.
    #[test]
    fn a_real_ci_key_has_the_declared_envelope_shape() {
        let pem = fixture("cosign-test.key");
        let (label, body) = der::pem::decode_vec(pem.as_bytes()).expect("pem");
        assert_eq!(label, PEM_TYPE_LEGACY);
        let e: Envelope = serde_json::from_slice(&body).expect("envelope json");
        assert_eq!(e.kdf.name, KDF_NAME);
        assert_eq!(e.cipher.name, CIPHER_NAME);
        assert!(scrypt_log_n(e.kdf.params).is_ok());
        assert_eq!(B64.decode(&e.kdf.salt).expect("salt").len(), SALT_LEN);
        assert_eq!(B64.decode(&e.cipher.nonce).expect("nonce").len(), NONCE_LEN);
        assert!(B64.decode(&e.ciphertext).expect("ct").len() > TAG_LEN);
    }

    /// `cosign.pub` from the same workflow: PKIX/SPKI, and the encoding this
    /// module emits reproduces it exactly when round-tripped.
    #[test]
    fn a_real_cosign_pub_round_trips_through_this_encoder() {
        use spki::DecodePublicKey as _;
        let pem = fixture("cosign-test.pub");
        let vk = p256::ecdsa::VerifyingKey::from_public_key_pem(&pem).expect("spki");
        assert_eq!(vk.to_public_key_pem(LineEnding::LF).expect("pem"), pem);
    }

    // --- negative cases ---------------------------------------------------

    /// cosign's `pemcosigneckey.pem` exists in its test suite to prove
    /// `LoadPrivateKey` rejects a SEC1 payload. It decrypts cleanly, so a
    /// reader that reports "wrong passphrase" here sends the user after a
    /// password that was always correct.
    #[test]
    fn a_sec1_payload_decrypts_and_is_still_refused_as_not_pkcs8() {
        let pem = fixture("pemcosigneckey.pem");
        let der = open_envelope(&envelope_of(&pem), COSIGN_PASSPHRASE).expect("decrypts");
        assert_eq!(der.len(), 121);
        assert_eq!(&der[..5], &[0x30, 0x77, 0x02, 0x01, 0x01], "SEC1 version 1");

        let e = load_cosign_key(&pem, COSIGN_PASSPHRASE).unwrap_err();
        assert!(
            matches!(&e, Error::Key { reason, .. } if reason.contains("not PKCS#8")),
            "{e}"
        );
        assert!(!matches!(e, Error::PassphraseWrong));
    }

    #[test]
    fn the_wrong_passphrase_is_named_as_such() {
        let pem = fixture("pemsigstorekey.pem");
        let e = load_cosign_key(&pem, b"not hello").unwrap_err();
        assert!(matches!(e, Error::PassphraseWrong), "{e}");
    }

    /// The DoS guard has to fire *fast*, and this is the test that shows it:
    /// `N = 2^30` at `r = 8` is 1 TiB of scratch, so if the whitelist ran after
    /// the KDF rather than before it, this test would never return. The cost
    /// parameters come out of the file, which makes them attacker-controlled,
    /// which is why `CheckParams` exists in Go at all.
    #[test]
    fn an_unrecognised_scrypt_cost_is_refused_before_the_kdf_runs() {
        // N below the legacy level, N above OWASP, a tampered r, a tampered p.
        for (n, r, p) in [
            (16384, 8, 1),
            (1 << 30, 8, 1),
            (65536, 16, 1),
            (65536, 8, 2),
        ] {
            let json = format!(
                r#"{{"kdf":{{"name":"scrypt","params":{{"N":{n},"r":{r},"p":{p}}},"salt":"{salt}"}},"cipher":{{"name":"nacl/secretbox","nonce":"{nonce}"}},"ciphertext":"{ct}"}}"#,
                salt = B64.encode([7u8; SALT_LEN]),
                nonce = B64.encode([9u8; NONCE_LEN]),
                ct = B64.encode([0u8; 64]),
            );
            let e = open_envelope(json.as_bytes(), b"x").unwrap_err();
            assert!(
                matches!(e, Error::ScryptParams { .. }),
                "N={n} r={r} p={p}: {e}"
            );
        }
    }

    /// The write side refuses an unsupported cost too, and produces **no PEM**
    /// when it does.
    ///
    /// This test used to carry the `N = 2^30` / 1 TiB doc comment that now sits
    /// on the test above, where the 2^30 case actually lives — and it said the
    /// whitelist runs "before the RNG or the key material is touched", which a
    /// mutation run showed it does not check: deleting that early call from
    /// `write_cosign_key` left every test green, because `seal_envelope` reaches
    /// `scrypt_log_n` again through `derive_key` and refuses a moment later.
    /// The early check is still worth keeping — it is what makes a refused call
    /// cost nothing — but *this* is the property that is observable from
    /// outside, so this is the property claimed.
    #[test]
    fn writing_an_unrecognised_cost_is_refused_and_yields_nothing() {
        for n in [16384u32, 1 << 30, u32::MAX, 0] {
            let e = write_cosign_key(&a_key(), b"pw", n).unwrap_err();
            assert!(
                matches!(e, Error::ScryptParams { n: got, .. } if got == n),
                "{e}"
            );
        }
    }

    #[test]
    fn corrupt_base64_is_named_as_base64_not_as_a_bad_passphrase() {
        for (field, broken) in [
            ("kdf.salt", GO_STANDARD.replace("FhzPOt9", "Fhz!Ot9")),
            ("cipher.nonce", GO_STANDARD.replace("aw1ng1jH", "aw1n!1jH")),
            ("ciphertext", GO_STANDARD.replace("HycvuLZL", "Hycv!LZL")),
        ] {
            let e = open_envelope(broken.as_bytes(), GO_PASSPHRASE).unwrap_err();
            assert!(
                matches!(&e, Error::Base64 { field: f, .. } if *f == field),
                "{field}: {e}"
            );
        }
    }

    #[test]
    fn a_ciphertext_too_short_to_hold_a_tag_is_refused_as_malformed() {
        let json = GO_STANDARD.replace(
            "HycvuLZL4sYH0BrYTh4E/H20VtAW6u5zL5Pr+IBjYLYnCPzDkq8=",
            &B64.encode([0u8; TAG_LEN - 1]),
        );
        let e = open_envelope(json.as_bytes(), GO_PASSPHRASE).unwrap_err();
        assert!(
            matches!(&e, Error::Key { reason, .. } if reason.contains("Poly1305 tag")),
            "{e}"
        );
    }

    /// Truncating past the tag is the case Poly1305 cannot distinguish from a
    /// wrong key. Asserted rather than glossed: the error a user sees here says
    /// "passphrase", and this test is the record that it means "the tag did not
    /// match", nothing narrower.
    #[test]
    fn a_ciphertext_truncated_past_the_tag_fails_authentication() {
        let e: Envelope = serde_json::from_str(GO_STANDARD).expect("json");
        let full = B64.decode(&e.ciphertext).expect("ct");
        let json = GO_STANDARD.replace(&e.ciphertext, &B64.encode(&full[..full.len() - 4]));
        let err = open_envelope(json.as_bytes(), GO_PASSPHRASE).unwrap_err();
        assert!(matches!(err, Error::PassphraseWrong), "{err}");
    }

    #[test]
    fn a_single_flipped_ciphertext_bit_is_refused() {
        let e: Envelope = serde_json::from_str(GO_STANDARD).expect("json");
        let mut ct = B64.decode(&e.ciphertext).expect("ct");
        let last = ct.len() - 1;
        ct[last] ^= 0x01;
        let json = GO_STANDARD.replace(&e.ciphertext, &B64.encode(&ct));
        assert!(open_envelope(json.as_bytes(), GO_PASSPHRASE).is_err());
    }

    #[test]
    fn a_nonce_of_the_wrong_length_is_refused_by_name() {
        for len in [16usize, 12, 23, 25, 32] {
            let json = GO_STANDARD.replace(
                "aw1ng1jHaDz/tQ7V2gR9O2+IGQ8xJEuE",
                &B64.encode(vec![0u8; len]),
            );
            let e = open_envelope(json.as_bytes(), GO_PASSPHRASE).unwrap_err();
            assert!(
                matches!(&e, Error::Key { reason, .. } if reason.contains("nonce is")),
                "len {len}: {e}"
            );
        }
    }

    #[test]
    fn an_unknown_kdf_or_cipher_name_is_refused() {
        let bad_kdf = GO_STANDARD.replace(r#""name":"scrypt""#, r#""name":"argon2id""#);
        let e = open_envelope(bad_kdf.as_bytes(), GO_PASSPHRASE).unwrap_err();
        assert!(
            matches!(&e, Error::Key { reason, .. } if reason.contains("unknown kdf name")),
            "{e}"
        );

        let bad_cipher = GO_STANDARD.replace("nacl/secretbox", "chacha20poly1305");
        let e = open_envelope(bad_cipher.as_bytes(), GO_PASSPHRASE).unwrap_err();
        assert!(
            matches!(&e, Error::Key { reason, .. } if reason.contains("unknown cipher name")),
            "{e}"
        );
    }

    #[test]
    fn a_pem_block_of_the_wrong_type_is_refused() {
        let key = a_key();
        for wrong in [
            pkcs8_pem(&key).expect("pkcs8"),
            public_key_pem(&key).expect("spki"),
            der::pem::encode_string("CERTIFICATE", LineEnding::LF, b"nope").expect("pem"),
        ] {
            let e = load_cosign_key(&wrong, b"pw").unwrap_err();
            assert!(
                matches!(&e, Error::Key { reason, .. } if reason.contains("PEM type is")),
                "{e}"
            );
        }
    }

    #[test]
    fn text_that_is_not_pem_at_all_is_refused_as_such() {
        let e = load_cosign_key("{\"kdf\":{}}", b"pw").unwrap_err();
        assert!(
            matches!(&e, Error::Key { reason, .. } if reason.contains("not a PEM block")),
            "{e}"
        );
    }

    // --- PEM framing: what cosign accepts, this accepts ------------------
    //
    // Every row below was run through Go's own `encoding/pem.Decode` — the
    // function `LoadPrivateKey` calls — against this same fixture. Go accepted
    // all of them and returned the identical block body every time. Before
    // `first_pem_block` existed, four of the five were refused here as "not a
    // PEM block this build can read".

    /// The row that matters: `echo "$COSIGN_KEY" > cosign.key` appends one
    /// newline, and that newline used to make a perfectly good signing key
    /// unreadable. A verifier is allowed to be stricter than the reference
    /// implementation about *what a signature means*; it is not allowed to be
    /// stricter about *what a file is* and call the difference corruption.
    #[test]
    fn trailing_whitespace_does_not_make_a_good_key_corrupt() {
        let good = fixture("pemsigstorekey.pem");
        let want = load_cosign_key(&good, COSIGN_PASSPHRASE).expect("pristine");

        for (what, text) in [
            ("one trailing LF", format!("{good}\n")),
            ("two trailing LF", format!("{good}\n\n")),
            ("trailing CRLF", format!("{good}\r\n")),
            ("a trailing space", format!("{good} ")),
            ("no final newline", good.trim_end().to_string()),
            (
                "CRLF throughout, plus one LF",
                format!("{}\n", good.replace('\n', "\r\n")),
            ),
            (
                "a preamble, as RFC 7468 permits",
                format!("Key for CI\n{good}"),
            ),
            // `first_pem_block` slices a `&str` by byte index, and slicing one
            // inside a multi-byte character panics. Every index it computes
            // lands on an ASCII `-` by construction; these two rows are the
            // record that the construction holds, because a panic in a key
            // reader is a worse outcome than any refusal.
            ("a multi-byte preamble", format!("鍵 — clé 🔑\n{good}")),
            ("multi-byte trailing junk", format!("{good}鍵 — clé 🔑\n")),
        ] {
            let got =
                load_cosign_key(&text, COSIGN_PASSPHRASE).unwrap_or_else(|e| panic!("{what}: {e}"));
            assert_eq!(got.to_bytes(), want.to_bytes(), "{what}");
        }
    }

    /// Two blocks in one file: cosign takes the **first** and discards the
    /// rest, and so does this. Agreeing on *which* block is the key matters
    /// more than either choice — a reader that took the last would sign with a
    /// key cosign would never use, and nothing in either tool would say so.
    #[test]
    fn a_second_appended_block_is_ignored_the_way_cosign_ignores_it() {
        let first = fixture("pemsigstorekey.pem");
        let second = fixture("pemcosignkey.pem");
        let want = load_cosign_key(&first, COSIGN_PASSPHRASE).expect("first");
        let other = load_cosign_key(&second, COSIGN_PASSPHRASE).expect("second");
        assert_ne!(
            want.to_bytes(),
            other.to_bytes(),
            "the fixtures are the same key"
        );

        let got = load_cosign_key(&format!("{first}{second}"), COSIGN_PASSPHRASE).expect("load");
        assert_eq!(got.to_bytes(), want.to_bytes(), "the second block won");
    }

    /// The relaxation is framing only. It can discard bytes after the first
    /// block's closing `-----` and nothing else, so every malformed body is
    /// still refused by the strict decoder — asserted here rather than
    /// asserted in the doc comment, because "it only relaxes the framing" is
    /// exactly the kind of claim that quietly stops being true.
    #[test]
    fn relaxing_the_framing_did_not_relax_anything_else() {
        let good = fixture("pemsigstorekey.pem");
        let body_start = good.find('\n').expect("header") + 1;

        for (what, text) in [
            (
                "no END boundary at all",
                good[..good.len() - 46].to_string(),
            ),
            (
                "END label does not match BEGIN",
                good.replace("-----END ENCRYPTED SIGSTORE", "-----END ENCRYPTED COSIGN"),
            ),
            (
                "a byte of the body re-typed",
                format!("{}X{}", &good[..body_start], &good[body_start + 1..]),
            ),
            (
                "the body truncated by one line",
                format!("{}{}", &good[..body_start], &good[body_start + 65..]),
            ),
            (
                "nothing but boundaries",
                "-----BEGIN X-----\n-----END X-----\n".to_string(),
            ),
            (
                "an END boundary and no BEGIN",
                "-----END ENCRYPTED SIGSTORE PRIVATE KEY-----\n".to_string(),
            ),
            ("empty", String::new()),
            (
                "a BEGIN inside a multi-byte line",
                "鍵-----BEGIN X-----🔑".to_string(),
            ),
            (
                "a truncated END boundary",
                format!("{}\n-----END", &good[..good.len() - 46]),
            ),
        ] {
            assert!(
                load_cosign_key(&text, COSIGN_PASSPHRASE).is_err(),
                "{what} was accepted"
            );
        }
    }

    /// `LoadPrivateKey` would load an RSA, Ed25519 or P-384 key here and hand
    /// it to a generic signer. This returns a P-256 `SigningKey`, so it cannot
    /// — and the point of this test is that the refusal **names the algorithm**
    /// instead of reporting the passphrase, which was correct.
    ///
    /// The fixtures are Go's, written through cosign's exact marshal path
    /// (`x509.MarshalPKCS8PrivateKey` → `encrypted.Encrypt` → `pem.Encode`);
    /// see `fixtures/cosign-keys/PROVENANCE.md`. The P-256 control comes from
    /// the same program and the same run, so a refusal of the other three
    /// cannot be blamed on the fixture generator.
    #[test]
    fn a_key_for_another_algorithm_is_refused_by_name_not_as_a_bad_passphrase() {
        load_cosign_key(&fixture("other-algo-p256-control.pem"), COSIGN_PASSPHRASE)
            .expect("the P-256 control from the same generator must load");

        for (file, oid, what) in [
            ("other-algo-p384.pem", "1.3.132.0.34", "secp384r1"),
            ("other-algo-ed25519.pem", "1.3.101.112", "Ed25519"),
            ("other-algo-rsa2048.pem", "1.2.840.113549.1.1.1", "RSA"),
        ] {
            let e = load_cosign_key(&fixture(file), COSIGN_PASSPHRASE).unwrap_err();
            assert!(
                !matches!(e, Error::PassphraseWrong),
                "{what}: reported as a passphrase failure, and the passphrase was right"
            );
            assert!(
                matches!(&e, Error::Key { reason, .. }
                    if reason.contains("not PKCS#8") && reason.contains(oid)),
                "{what}: {e}"
            );
        }
    }

    // --- the empty passphrase ---------------------------------------------

    /// `generate`'s contract has always said the passphrase is not optional.
    /// Until [`REFUSING_EMPTY_PASSPHRASE`] it was a sentence and not a check:
    /// `generate(k, p, b"")` wrote a file whose envelope declares
    /// `scrypt N=65536` and which reopened under `b""` — protection that is
    /// only a header.
    ///
    /// This also puts `generate`'s **post-claim cleanup** under test for the
    /// first time. An empty passphrase is the one reachable input that fails
    /// after both files have been claimed, so if the cleanup were removed both
    /// halves would be left behind as empty files and the next run would
    /// refuse to overwrite them — one transient refusal turned into a
    /// permanently wedged directory.
    #[test]
    fn an_empty_passphrase_is_refused_and_leaves_no_half_written_pair() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (k, p) = (dir.path().join("cosign.key"), dir.path().join("cosign.pub"));

        let e = generate(&k, &p, b"").unwrap_err();
        assert!(
            matches!(&e, Error::Key { reason, .. } if reason == REFUSING_EMPTY_PASSPHRASE),
            "{e}"
        );
        assert!(!k.exists(), "an empty cosign.key was left behind");
        assert!(!p.exists(), "an empty cosign.pub was left behind");

        // And the directory is not wedged: a real passphrase still works.
        generate(&k, &p, b"pw").expect("generate after the refusal");

        let e = write_cosign_key(&a_key(), b"", SCRYPT_DEFAULT_N).unwrap_err();
        assert!(
            matches!(&e, Error::Key { reason, .. } if reason == REFUSING_EMPTY_PASSPHRASE),
            "{e}"
        );
    }

    /// Reading is *not* narrowed. cosign writes empty-passphrase keys when told
    /// to, they exist in the world, and refusing to open one would be this
    /// module calling a real key corrupt — the failure it exists to avoid.
    #[test]
    fn an_empty_passphrase_still_opens_a_key_that_was_written_with_one() {
        // Sealed here rather than by `write_cosign_key`, which now refuses to
        // produce one — the fixture has to come from the format, not from the
        // path under test.
        let key = a_key();
        let der = pkcs8_der(&key).expect("der");
        let envelope = seal_envelope(
            &der,
            b"",
            SCRYPT_LEGACY_N,
            &[3u8; SALT_LEN],
            &[4u8; NONCE_LEN],
        )
        .expect("seal");
        let pem = der::pem::encode_string(PEM_TYPE, LineEnding::LF, &envelope).expect("pem");

        let back = load_cosign_key(&pem, b"").expect("an empty-passphrase key must still open");
        assert_eq!(back.to_bytes(), key.to_bytes());
    }

    #[test]
    fn an_envelope_that_is_not_json_is_refused_as_json() {
        let pem =
            der::pem::encode_string(PEM_TYPE, LineEnding::LF, b"not json at all").expect("pem");
        let e = load_cosign_key(&pem, b"pw").unwrap_err();
        assert!(matches!(e, Error::Json(_)), "{e}");
    }

    // --- what this module writes ------------------------------------------

    #[test]
    fn a_key_written_here_reads_back_here_at_every_cost_level() {
        for n in [SCRYPT_LEGACY_N, SCRYPT_STANDARD_N, SCRYPT_OWASP_N] {
            let key = a_key();
            let pem = write_cosign_key(&key, b"correct horse", n).expect("write");
            assert!(pem.starts_with(&format!("-----BEGIN {PEM_TYPE}-----")));
            let back = load_cosign_key(&pem, b"correct horse").expect("read");
            assert_eq!(back.to_bytes(), key.to_bytes(), "N={n}");
        }
    }

    #[test]
    fn every_write_draws_a_fresh_salt_and_nonce() {
        let key = a_key();
        let a: Envelope = serde_json::from_slice(&envelope_of(
            &write_cosign_key(&key, b"pw", SCRYPT_LEGACY_N).unwrap(),
        ))
        .unwrap();
        let b: Envelope = serde_json::from_slice(&envelope_of(
            &write_cosign_key(&key, b"pw", SCRYPT_LEGACY_N).unwrap(),
        ))
        .unwrap();
        assert_ne!(a.kdf.salt, b.kdf.salt);
        assert_ne!(a.cipher.nonce, b.cipher.nonce);
        assert_ne!(a.ciphertext, b.ciphertext);
        assert_eq!(B64.decode(&a.kdf.salt).unwrap().len(), SALT_LEN);
        assert_eq!(B64.decode(&a.cipher.nonce).unwrap().len(), NONCE_LEN);
    }

    /// cosign moved its default from 32768 to 65536 at v2.2.0. Writing the old
    /// one would be a silent downgrade of everyone's key-at-rest cost, and
    /// nothing in a key file's appearance would show it.
    #[test]
    fn generate_writes_the_post_v2_2_0_default_cost() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (k, p) = (dir.path().join("cosign.key"), dir.path().join("cosign.pub"));
        generate(&k, &p, b"pw").expect("generate");
        let pem = std::fs::read_to_string(&k).expect("read");
        let e: Envelope = serde_json::from_slice(&envelope_of(&pem)).expect("json");
        assert_eq!(e.kdf.params.n, 65536);
        assert_eq!(e.kdf.params.r, SCRYPT_R);
        assert_eq!(e.kdf.params.p, SCRYPT_P);
    }

    #[test]
    fn generate_writes_a_pair_that_agrees_with_itself() {
        use p256::ecdsa::signature::{Signer as _, Verifier as _};
        use spki::DecodePublicKey as _;

        let dir = tempfile::tempdir().expect("tempdir");
        let (k, p) = (
            dir.path().join("d").join("cosign.key"),
            dir.path().join("cosign.pub"),
        );
        generate(&k, &p, b"pw").expect("generate");

        let signing =
            load_cosign_key(&std::fs::read_to_string(&k).expect("key"), b"pw").expect("load");
        let verifying = p256::ecdsa::VerifyingKey::from_public_key_pem(
            &std::fs::read_to_string(&p).expect("pub"),
        )
        .expect("spki");
        assert_eq!(&verifying, signing.verifying_key());

        // ECDSA P-256 over SHA-256, ASN.1 DER — what cosign's ECDSASigner emits.
        let sig: p256::ecdsa::DerSignature = signing.sign(b"the quick brown fox");
        assert_eq!(sig.as_bytes()[0], 0x30, "not ASN.1 DER");
        verifying
            .verify(b"the quick brown fox", &sig)
            .expect("verify");
    }

    #[cfg(unix)]
    #[test]
    fn the_private_key_is_created_mode_0600_and_the_public_is_not() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("tempdir");
        let (k, p) = (dir.path().join("cosign.key"), dir.path().join("cosign.pub"));
        generate(&k, &p, b"pw").expect("generate");
        assert_eq!(
            std::fs::metadata(&k).expect("stat").permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&p).expect("stat").permissions().mode() & 0o777,
            0o644
        );
    }

    #[test]
    fn generate_refuses_to_overwrite_either_half() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (k, p) = (dir.path().join("cosign.key"), dir.path().join("cosign.pub"));
        generate(&k, &p, b"pw").expect("first");
        let before = std::fs::read_to_string(&k).expect("read");

        let e = generate(&k, &p, b"pw").unwrap_err();
        assert!(
            matches!(&e, Error::Key { reason, .. } if reason == REFUSING_TO_OVERWRITE),
            "{e}"
        );
        assert_eq!(std::fs::read_to_string(&k).expect("read"), before);

        // The public half alone is enough to refuse: a stale cosign.pub beside
        // a new cosign.key rejects every signature the new key can make.
        std::fs::remove_file(&k).expect("rm");
        let e = generate(&k, &p, b"pw").unwrap_err();
        assert!(
            matches!(&e, Error::Key { reason, .. } if reason == REFUSING_TO_OVERWRITE),
            "{e}"
        );
        assert!(
            !k.exists(),
            "the private key it claimed and could not finish was left behind"
        );
    }

    // --- PKCS#8 export -----------------------------------------------------

    #[test]
    fn the_pkcs8_export_is_the_same_plaintext_the_envelope_carries() {
        let pem = fixture("pemsigstorekey.pem");
        let sealed = open_envelope(&envelope_of(&pem), COSIGN_PASSPHRASE).expect("open");
        let key = load_cosign_key(&pem, COSIGN_PASSPHRASE).expect("load");
        // If these differed, a key written here would round-trip through this
        // crate and be a different key in cosign.
        assert_eq!(pkcs8_der(&key).expect("der"), sealed);
        assert_eq!(sealed.len(), 138);
    }

    #[test]
    fn the_pkcs8_der_declares_rfc_5208_version_zero() {
        let der = pkcs8_der(&a_key()).expect("der");
        // SEQUENCE, long-form length (one length byte), then INTEGER 0.
        assert_eq!(der[0], 0x30);
        assert_eq!(der[1], 0x81, "expected a one-byte long-form length");
        assert_eq!(&der[3..6], &[0x02, 0x01, 0x00], "version is not 0");
    }

    #[test]
    fn pkcs8_pem_and_der_are_the_same_key() {
        let key = a_key();
        let from_pem =
            p256::ecdsa::SigningKey::from_pkcs8_pem(&pkcs8_pem(&key).expect("pem")).expect("parse");
        let from_der =
            p256::ecdsa::SigningKey::from_pkcs8_der(&pkcs8_der(&key).expect("der")).expect("parse");
        assert_eq!(from_pem.to_bytes(), key.to_bytes());
        assert_eq!(from_der.to_bytes(), key.to_bytes());
    }

    #[test]
    fn key_failures_all_sit_on_the_bundle_axis() {
        use crate::error::Axis;
        assert_eq!(Error::PassphraseWrong.axis(), Axis::Bundle);
        assert_eq!(
            Error::ScryptParams { n: 1, r: 8, p: 1 }.axis(),
            Axis::Bundle
        );
        assert_eq!(malformed("x").axis(), Axis::Bundle);
    }

    // --- cross-implementation, against a second implementation -------------
    //
    // cosign itself is not installed on this machine, so the counterparty is
    // the Python oracle vendored at `fixtures/cosign-keys/oracle/` — a
    // from-source reimplementation (its own XSalsa20-Poly1305, `hashlib.scrypt`,
    // and `cryptography` for the key parsing) whose every constant carries a
    // citation to the Go file it came from.
    //
    // **The oracle files being absent is a failure, not a skip.** The recon
    // phase's own version of this check was an `if let Ok(..)` with no `else`,
    // so it passed in silence on the run where its input file was missing. A
    // cross-implementation test that quietly asserts nothing is worse than no
    // test, because it reports a green that means "we did not look".
    //
    // A missing *interpreter* is different in kind and is reported as a skip —
    // but the envelope's byte-for-byte agreement with Go is proven without any
    // interpreter in `re_encrypting_a_go_golden_vector_is_byte_identical`, so
    // the skip does not take the format claim with it.

    fn oracle_script() -> std::path::PathBuf {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("cosign-keys")
            .join("oracle");
        for required in ["cosign_key.py", "xsalsa20poly1305.py"] {
            let f = dir.join(required);
            assert!(
                f.is_file(),
                "the cross-implementation oracle is missing: {} does not exist. This test proves \
                 nothing without it; it fails rather than passing quietly.",
                f.display()
            );
        }
        dir.join("cosign_key.py")
    }

    /// True when a `python3` that can `import cryptography` is on PATH. Every
    /// false answer prints why, so a skipped cross-check is visible in the test
    /// output rather than inferred from a suspiciously fast green.
    fn python_available() -> bool {
        match std::process::Command::new("python3")
            .args(["-c", "import cryptography"])
            .output()
        {
            Ok(o) if o.status.success() => true,
            Ok(o) => {
                eprintln!(
                    "SKIPPING the Python oracle: python3 is present but `import cryptography` \
                     failed: {}",
                    String::from_utf8_lossy(&o.stderr).trim()
                );
                false
            }
            Err(e) => {
                eprintln!("SKIPPING the Python oracle: python3 could not be run: {e}");
                false
            }
        }
    }

    fn run(cmd: &mut std::process::Command) -> String {
        let out = cmd.output().unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
        assert!(
            out.status.success(),
            "{cmd:?} exited {}\nstdout: {}\nstderr: {}",
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    /// Rust opens what the second implementation sealed, and the two agree on
    /// the public key and on each other's signatures.
    #[test]
    fn rust_reads_a_key_the_python_oracle_wrote() {
        let script = oracle_script();
        if !python_available() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let prefix = dir.path().join("py");
        run(std::process::Command::new("python3").args([
            script.to_str().expect("path"),
            "generate",
            "--out",
            prefix.to_str().expect("path"),
            "--password",
            "oracle-pw",
            "--kdf",
            "legacy",
        ]));

        let key_path = dir.path().join("py.key");
        let key = load_cosign_key(
            &std::fs::read_to_string(&key_path).expect("py.key"),
            b"oracle-pw",
        )
        .expect("Rust could not open a key the oracle wrote");
        assert_eq!(
            public_key_pem(&key).expect("pub"),
            std::fs::read_to_string(dir.path().join("py.pub")).expect("py.pub")
        );
        assert!(
            load_cosign_key(
                &std::fs::read_to_string(&key_path).expect("py.key"),
                b"oracle-pW"
            )
            .is_err(),
            "the oracle's key opened under the wrong passphrase"
        );

        // A signature the oracle made verifies here.
        let blob = dir.path().join("blob.txt");
        std::fs::write(&blob, b"cross-implementation").expect("blob");
        let sig_b64 = run(std::process::Command::new("python3").args([
            script.to_str().expect("path"),
            "sign",
            "--key",
            key_path.to_str().expect("path"),
            "--password",
            "oracle-pw",
            "--in",
            blob.to_str().expect("path"),
        ]));
        let sig = p256::ecdsa::DerSignature::from_bytes(
            &B64.decode(sig_b64.trim()).expect("b64 signature"),
        )
        .expect("ASN.1 DER signature");
        p256::ecdsa::signature::Verifier::verify(
            key.verifying_key(),
            b"cross-implementation",
            &sig,
        )
        .expect("the oracle's ECDSA-P256/SHA-256 signature did not verify here");
    }

    /// The other direction: the second implementation opens what this one
    /// sealed, and accepts a signature made with it.
    #[test]
    fn the_python_oracle_reads_a_key_rust_wrote() {
        use p256::ecdsa::signature::Signer as _;

        let script = oracle_script();
        if !python_available() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let key = a_key();
        let key_path = dir.path().join("rust.key");
        std::fs::write(
            &key_path,
            write_cosign_key(&key, b"rust-pw", SCRYPT_LEGACY_N).expect("write"),
        )
        .expect("rust.key");

        let stdout = run(std::process::Command::new("python3").args([
            script.to_str().expect("path"),
            "read",
            "--key",
            key_path.to_str().expect("path"),
            "--password",
            "rust-pw",
        ]));
        let marker = "-----BEGIN PUBLIC KEY-----";
        let at = stdout
            .find(marker)
            .unwrap_or_else(|| panic!("the oracle printed no public key:\n{stdout}"));
        assert_eq!(
            &stdout[at..],
            public_key_pem(&key).expect("pub"),
            "the oracle recovered a different key than the one written"
        );
        // The oracle reports the envelope it saw; check it read our declared
        // parameters rather than guessing them.
        assert!(stdout.contains("\"N\": 32768"), "{stdout}");
        assert!(
            stdout.contains("\"plaintext_pkcs8_der_len\": 138"),
            "{stdout}"
        );
        assert!(
            stdout.contains("\"pem_type\": \"ENCRYPTED SIGSTORE PRIVATE KEY\""),
            "{stdout}"
        );

        let blob = dir.path().join("blob.txt");
        std::fs::write(&blob, b"cross-implementation").expect("blob");
        let sig: p256::ecdsa::DerSignature = key.sign(b"cross-implementation");
        let sig_path = dir.path().join("blob.sig");
        std::fs::write(&sig_path, B64.encode(sig.as_bytes())).expect("sig");
        let pub_path = dir.path().join("rust.pub");
        std::fs::write(&pub_path, public_key_pem(&key).expect("pub")).expect("pub");
        run(std::process::Command::new("python3").args([
            script.to_str().expect("path"),
            "verify",
            "--pub",
            pub_path.to_str().expect("path"),
            "--in",
            blob.to_str().expect("path"),
            "--sig",
            sig_path.to_str().expect("path"),
        ]));
    }

    /// Same passphrase, same salt, same nonce, same plaintext — same bytes.
    ///
    /// A round trip only shows the two implementations are *compatible*. This
    /// shows they are the *same function*, which is the property that survives
    /// a third implementation showing up.
    #[test]
    fn the_two_implementations_seal_identical_bytes_from_identical_inputs() {
        let script = oracle_script();
        if !python_available() {
            return;
        }
        let plaintext: &[u8] = b"reallyimportant";
        let salt = [0x5au8; SALT_LEN];
        let nonce = [0xa5u8; NONCE_LEN];
        let passphrase = b"supersecret";

        let mine =
            seal_envelope(plaintext, passphrase, SCRYPT_LEGACY_N, &salt, &nonce).expect("seal");

        let program = format!(
            "import sys\n\
             sys.path.insert(0, {dir:?})\n\
             from cosign_key import encrypted_encrypt, KDF_LEGACY\n\
             sys.stdout.write(encrypted_encrypt(bytes.fromhex(sys.argv[1]), \
             sys.argv[2].encode(), kdf_params=KDF_LEGACY, salt=bytes.fromhex(sys.argv[3]), \
             nonce=bytes.fromhex(sys.argv[4])).decode())\n",
            dir = script.parent().expect("oracle dir").to_str().expect("path"),
        );
        let theirs = run(std::process::Command::new("python3").args([
            "-c",
            &program,
            &hex::encode(plaintext),
            "supersecret",
            &hex::encode(salt),
            &hex::encode(nonce),
        ]));
        assert_eq!(String::from_utf8(mine).expect("utf8"), theirs);

        // And the reverse: what the oracle produced opens here.
        assert_eq!(
            open_envelope(theirs.as_bytes(), passphrase).expect("open"),
            plaintext
        );
    }
}
