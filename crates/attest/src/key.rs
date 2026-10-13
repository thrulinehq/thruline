//! Local signing keys (BUILD-SPEC §8.1's *"local key signing … for
//! offline/enterprise use"*).
//!
//! # PKCS#8 PEM, and why not cosign's own format
//!
//! §8.1 says `cosign.key`-compatible. A real `cosign.key` is an **encrypted**
//! PEM: scrypt over a passphrase, then NaCl secretbox, wrapping an ECDSA P-256
//! key. Reimplementing that means a passphrase prompt, a KDF, an AEAD and a
//! second curve — and getting any of them subtly wrong produces keys that look
//! fine and interoperate with nothing.
//!
//! This ships **unencrypted PKCS#8 PEM ed25519** instead, which is a format
//! `openssl`, `age`, `ssh-keygen -m PKCS8` and the Rust ecosystem all read, and
//! it is written down as a deviation rather than described as cosign
//! compatibility it does not have. What M2's done-when needs is a key that
//! signs here and verifies there; that is met. Cosign's own container format is
//! an interoperability question for M6's packaging, not a correctness one.
//!
//! **The private key file is written `0600`**, and the check is a test rather
//! than a comment.

use std::path::Path;

use ed25519_dalek::pkcs8::spki::der::pem::LineEnding;
use ed25519_dalek::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePublicKey};

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{path} is not a PKCS#8 PEM key this build can read: {reason}")]
    Format { path: String, reason: String },
    #[error("a key already exists at {path}; refusing to overwrite it")]
    Exists { path: String },
}

/// Generate a fresh key pair and write both halves.
///
/// **Refuses to overwrite.** An overwritten signing key silently invalidates
/// every receipt already issued under it, and the failure appears later, as a
/// verification failure that reads like tampering.
pub fn generate(
    private_path: &Path,
    public_path: &Path,
) -> Result<ed25519_dalek::SigningKey, KeyError> {
    if private_path.exists() {
        return Err(KeyError::Exists {
            path: private_path.display().to_string(),
        });
    }
    let key = ed25519_dalek::SigningKey::generate(&mut rand_core::OsRng);
    write_private(&key, private_path)?;
    let pem = key
        .verifying_key()
        .to_public_key_pem(LineEnding::LF)
        .map_err(|e| KeyError::Format {
            path: public_path.display().to_string(),
            reason: e.to_string(),
        })?;
    std::fs::write(public_path, pem).map_err(|source| KeyError::Io {
        path: public_path.display().to_string(),
        source,
    })?;
    Ok(key)
}

/// PKCS#8 **v0** DER for an Ed25519 seed: 48 bytes, exactly.
///
/// ```text
/// 30 2e             SEQUENCE (46)
///    02 01 00       INTEGER 0            -- version 0, not 1
///    30 05
///       06 03 2b 65 70                   -- OID 1.3.101.112, Ed25519
///    04 22
///       04 20 <32-byte seed>
/// ```
///
/// # Why this is written by hand
///
/// `ed25519-dalek`'s own `to_pkcs8_der` emits **RFC 5958 OneAsymmetricKey v2**:
/// 83 bytes, `version = 1`, with the public key in a `[1]` attribute. That is
/// legal ASN.1 — and **both OpenSSL 3 and Python `cryptography` reject it** for
/// Ed25519. Measured on the key this crate used to write: `openssl pkey`
/// returns `ossl_store_handle_load_result:unsupported`, and
/// `load_pem_private_key` raises `ValueError`.
///
/// Key portability is precisely what "verifies on another machine" rests on, so
/// a format the two most common parsers refuse is not a detail. v0 is 48 bytes,
/// carries no public-key attribute, and both accept it.
fn pkcs8_v0_der(key: &ed25519_dalek::SigningKey) -> [u8; 48] {
    let mut out = [0u8; 48];
    out[..16].copy_from_slice(&[
        0x30, 0x2e, // SEQUENCE, 46 bytes
        0x02, 0x01, 0x00, // INTEGER 0 — version 0
        0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, // AlgorithmIdentifier: Ed25519
        0x04, 0x22, 0x04, 0x20, // OCTET STRING(34) { OCTET STRING(32) }
    ]);
    out[16..].copy_from_slice(key.as_bytes());
    out
}

fn write_private(key: &ed25519_dalek::SigningKey, path: &Path) -> Result<(), KeyError> {
    use base64::Engine as _;
    let der = pkcs8_v0_der(key);
    let b64 = base64::engine::general_purpose::STANDARD.encode(der);
    let mut pem = String::from("-----BEGIN PRIVATE KEY-----\n");
    for chunk in b64.as_bytes().chunks(64) {
        pem.push_str(&String::from_utf8_lossy(chunk));
        pem.push('\n');
    }
    pem.push_str("-----END PRIVATE KEY-----\n");
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(path, pem.as_bytes()).map_err(|source| KeyError::Io {
        path: path.display().to_string(),
        source,
    })?;
    restrict(path)
}

/// `0600` on the private key.
#[cfg(unix)]
fn restrict(path: &Path) -> Result<(), KeyError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|source| {
        KeyError::Io {
            path: path.display().to_string(),
            source,
        }
    })
}

#[cfg(not(unix))]
fn restrict(_path: &Path) -> Result<(), KeyError> {
    Ok(())
}

/// The exact DER bytes this crate writes, for tests and for anyone checking
/// interoperability without a filesystem.
pub fn private_der(key: &ed25519_dalek::SigningKey) -> [u8; 48] {
    pkcs8_v0_der(key)
}

/// Reads **both** v0 and v2.
///
/// A key generated by an earlier build is v2, and refusing it would invalidate
/// every receipt already signed with it — a worse failure than the one being
/// fixed.
pub fn load_signing(path: &Path) -> Result<ed25519_dalek::SigningKey, KeyError> {
    let pem = std::fs::read_to_string(path).map_err(|source| KeyError::Io {
        path: path.display().to_string(),
        source,
    })?;
    ed25519_dalek::SigningKey::from_pkcs8_pem(&pem).map_err(|e| KeyError::Format {
        path: path.display().to_string(),
        reason: e.to_string(),
    })
}

pub fn load_verifying(path: &Path) -> Result<ed25519_dalek::VerifyingKey, KeyError> {
    let pem = std::fs::read_to_string(path).map_err(|source| KeyError::Io {
        path: path.display().to_string(),
        source,
    })?;
    ed25519_dalek::VerifyingKey::from_public_key_pem(&pem).map_err(|e| KeyError::Format {
        path: path.display().to_string(),
        reason: e.to_string(),
    })
}
