//! §14's `thruline sync`, as a **reference implementation that v1 does not
//! ship**.
//!
//! §18 asks whether the hosted sync server ships in v1 or stays a
//! feature-flagged reference implementation. The answer given is the second, so
//! this crate builds the client half and **no server exists** — the transport
//! is a trait, and the only implementation in this tree is the in-memory one
//! the tests drive.
//!
//! # The claim, and what makes it checkable
//!
//! The claim a sync feature has to earn is *"the server cannot read your
//! sessions"*. That claim is worth exactly as much as the smallest gap in it, so
//! this crate is arranged so the gap has somewhere to be caught:
//!
//! - Sealing happens **before** anything reaches a [`Transport`], and the trait
//!   takes an [`Envelope`] rather than bytes, so there is no signature through
//!   which plaintext could pass.
//! - `tests/nothing_leaks.rs` seals a distinctive marker, drives a real push
//!   through a transport that records every byte it is handed, and asserts the
//!   marker and the key appear in none of them.
//! - The AEAD's associated data is the object's own id, so an envelope moved
//!   to a different id fails to open even with the key. A server that shuffles
//!   objects cannot make a client read the wrong one.
//! - Decryption failure is an error. There is no path that returns plaintext
//!   without a successful tag check, and no fallback of any kind.
//!
//! # Key custody
//!
//! The point of sync is a second machine, so the key has to be re-derivable
//! rather than local-only: Argon2id over a passphrase, with a per-vault salt
//! and the parameters recorded **in the envelope**. Recording them is what lets
//! a future client raise the cost without being unable to open old objects, and
//! it is why they are read from the envelope rather than assumed.
//!
//! The key itself never reaches an envelope, a transport, or a log. It is
//! zeroized on drop, which is a mitigation and not a guarantee — a value that
//! has been copied by an allocator or paged out is beyond this crate's reach,
//! and saying so is more useful than implying otherwise.

use base64::Engine as _;
use chacha20poly1305::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[cfg(test)]
mod tests;

/// The envelope format. A change here is a versioned migration, never a
/// refactor — the same rule `protected_store` states for its layout.
pub const FORMAT: &str = "TLSYNC1";

/// A sync key, derived from a passphrase.
///
/// Zeroized on drop. `Debug` is implemented by hand to print nothing: a key in
/// a panic message is a key in a CI log.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SyncKey([u8; 32]);

impl std::fmt::Debug for SyncKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SyncKey(<redacted>)")
    }
}

/// The KDF parameters, recorded so a second machine can re-derive.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Kdf {
    /// Always `argon2id`. Present so a future algorithm is a value rather than
    /// an assumption.
    pub algorithm: String,
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    /// Base64, standard alphabet with padding.
    pub salt: String,
}

impl Kdf {
    /// OWASP's 2024 second-choice profile for Argon2id: 19 MiB, 2 iterations,
    /// 1 lane. Chosen, not measured — and labelled as chosen, because a cost
    /// parameter nobody measured is a number and not a defence.
    pub fn new(salt: &[u8]) -> Kdf {
        Kdf {
            algorithm: "argon2id".to_string(),
            m_cost_kib: 19 * 1024,
            t_cost: 2,
            p_cost: 1,
            salt: base64::engine::general_purpose::STANDARD.encode(salt),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("deriving the sync key: {0}")]
    Kdf(String),
    #[error(
        "this object did not decrypt. Either the passphrase is wrong, or the bytes were \
         changed in transit, or the object was served under a different id than it was \
         sealed under. There is no way to tell which from here, and no partial content \
         is returned."
    )]
    Open,
    #[error("sealing: {0}")]
    Seal(String),
    #[error("the envelope is `{found}`, and this build reads `{FORMAT}`")]
    Format { found: String },
    #[error("the object id is empty")]
    EmptyId,
    #[error("transport: {0}")]
    Transport(String),
}

/// What a server stores. **Ciphertext and metadata; never content.**
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Envelope {
    pub format: String,
    pub kdf: Kdf,
    /// Base64, 24 bytes.
    pub nonce: String,
    /// Base64.
    pub ciphertext: String,
}

/// Derives a sync key from a passphrase.
pub fn derive(passphrase: &str, kdf: &Kdf) -> Result<SyncKey, SyncError> {
    if kdf.algorithm != "argon2id" {
        return Err(SyncError::Kdf(format!(
            "unknown key-derivation algorithm `{}`; this build knows argon2id",
            kdf.algorithm
        )));
    }
    let salt = base64::engine::general_purpose::STANDARD
        .decode(&kdf.salt)
        .map_err(|e| SyncError::Kdf(e.to_string()))?;
    let params = argon2::Params::new(kdf.m_cost_kib, kdf.t_cost, kdf.p_cost, Some(32))
        .map_err(|e| SyncError::Kdf(e.to_string()))?;
    let a = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = [0u8; 32];
    a.hash_password_into(passphrase.as_bytes(), &salt, &mut out)
        .map_err(|e| SyncError::Kdf(e.to_string()))?;
    Ok(SyncKey(out))
}

/// A fresh 16-byte salt.
pub fn fresh_salt() -> [u8; 16] {
    use chacha20poly1305::aead::rand_core::RngCore as _;
    let mut salt = [0u8; 16];
    OsRng.fill_bytes(&mut salt);
    salt
}

/// Seals `plaintext` for `id`.
///
/// The nonce is drawn from the OS RNG for **every** call and is never derived
/// from the content, the id, or a counter. XChaCha's 192-bit nonce is what makes
/// random generation safe here; a 96-bit nonce would need a counter and a
/// counter needs state that survives a crash.
///
/// `id` is the associated data, so an envelope served under a different id
/// fails to open. That is the defence against a server that shuffles objects.
pub fn seal(key: &SyncKey, id: &str, kdf: &Kdf, plaintext: &[u8]) -> Result<Envelope, SyncError> {
    if id.is_empty() {
        return Err(SyncError::EmptyId);
    }
    let cipher = XChaCha20Poly1305::new((&key.0).into());
    let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: id.as_bytes(),
            },
        )
        .map_err(|e| SyncError::Seal(e.to_string()))?;
    let b64 = base64::engine::general_purpose::STANDARD;
    Ok(Envelope {
        format: FORMAT.to_string(),
        kdf: kdf.clone(),
        nonce: b64.encode(nonce),
        ciphertext: b64.encode(ciphertext),
    })
}

/// Opens an envelope, or fails. **There is no third outcome.**
pub fn open(key: &SyncKey, id: &str, envelope: &Envelope) -> Result<Vec<u8>, SyncError> {
    if envelope.format != FORMAT {
        return Err(SyncError::Format {
            found: envelope.format.clone(),
        });
    }
    let b64 = base64::engine::general_purpose::STANDARD;
    let nonce = b64.decode(&envelope.nonce).map_err(|_| SyncError::Open)?;
    if nonce.len() != 24 {
        return Err(SyncError::Open);
    }
    let ciphertext = b64
        .decode(&envelope.ciphertext)
        .map_err(|_| SyncError::Open)?;
    let cipher = XChaCha20Poly1305::new((&key.0).into());
    cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: id.as_bytes(),
            },
        )
        .map_err(|_| SyncError::Open)
}

/// Where sealed objects go.
///
/// **Takes an [`Envelope`], not bytes.** There is no signature here through
/// which plaintext could reach a server, which is what makes the crate's central
/// claim a property of the types rather than of a code review.
pub trait Transport {
    fn push(&mut self, id: &str, envelope: &Envelope) -> Result<(), SyncError>;
    fn pull(&mut self, id: &str) -> Result<Option<Envelope>, SyncError>;
    fn list(&mut self) -> Result<Vec<String>, SyncError>;
}

/// An in-memory transport. **The only one in this tree**, and it is for tests:
/// §18's answer is that no server ships in v1.
#[derive(Debug, Default)]
pub struct Memory {
    objects: std::collections::BTreeMap<String, Envelope>,
    /// Every byte this transport was ever handed, for the leak walk.
    pub seen: Vec<u8>,
}

impl Transport for Memory {
    fn push(&mut self, id: &str, envelope: &Envelope) -> Result<(), SyncError> {
        self.seen.extend_from_slice(id.as_bytes());
        self.seen.extend_from_slice(
            serde_json::to_vec(envelope)
                .map_err(|e| SyncError::Transport(e.to_string()))?
                .as_slice(),
        );
        self.objects.insert(id.to_string(), envelope.clone());
        Ok(())
    }
    fn pull(&mut self, id: &str) -> Result<Option<Envelope>, SyncError> {
        Ok(self.objects.get(id).cloned())
    }
    fn list(&mut self) -> Result<Vec<String>, SyncError> {
        Ok(self.objects.keys().cloned().collect())
    }
}
