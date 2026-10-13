//! DSSE envelopes (BUILD-SPEC §8.1), and the PAE encoding they sign over.
//!
//! # PAE, spelled out, because getting it wrong is silent
//!
//! ```text
//! PAE(type, body) = "DSSEv1" ‖ SP ‖ LEN(type) ‖ SP ‖ type ‖ SP ‖ LEN(body) ‖ SP ‖ body
//! ```
//!
//! `LEN` is the **byte** length in ASCII decimal. The length prefixes are the
//! whole point: without them a signature over `type ‖ body` could be replayed
//! against a different split of the same bytes. A verifier that reconstructs
//! PAE differently from the signer does not fail loudly — it fails as
//! "signature invalid", which reads as tampering. So the encoding has its own
//! test vector rather than only being exercised through a round trip.
//!
//! # Why the payload is base64 in the envelope but not in PAE
//!
//! DSSE transports the payload base64-encoded and signs the **raw** bytes. A
//! signer that signs the encoded form produces envelopes nothing else can
//! verify, and the mistake is invisible until another implementation reads one.

use base64::Engine as _;
use serde::{Deserialize, Serialize};

/// The DSSE envelope as it travels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    /// Base64 (standard alphabet, padded) of the raw payload.
    pub payload: String,
    #[serde(rename = "payloadType")]
    pub payload_type: String,
    pub signatures: Vec<Signature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// Base64 of the raw signature bytes.
    pub sig: String,
    /// Which key signed. Hint only — a verifier must still check the signature.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyid: Option<String>,
}

/// Something wrong with an envelope.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DsseError {
    #[error("the envelope's payload is not valid base64")]
    Base64,
    #[error("the envelope carries no signatures")]
    NoSignatures,
    #[error("no signature in the envelope verifies against the given key(s)")]
    NoSignatureVerified,
    #[error("the payload is not valid UTF-8 JSON")]
    Payload,
    #[error("payloadType is {found:?}, expected {expected:?}")]
    PayloadType { found: String, expected: String },
}

fn b64() -> base64::engine::general_purpose::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

/// Pre-Authentication Encoding. See the module docs.
pub fn pae(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + payload_type.len() + 32);
    out.extend_from_slice(b"DSSEv1 ");
    out.extend_from_slice(payload_type.len().to_string().as_bytes());
    out.push(b' ');
    out.extend_from_slice(payload_type.as_bytes());
    out.push(b' ');
    out.extend_from_slice(payload.len().to_string().as_bytes());
    out.push(b' ');
    out.extend_from_slice(payload);
    out
}

impl Envelope {
    /// The raw payload bytes this envelope carries.
    pub fn payload_bytes(&self) -> Result<Vec<u8>, DsseError> {
        b64().decode(&self.payload).map_err(|_| DsseError::Base64)
    }

    /// The exact bytes a signature over this envelope covers.
    pub fn signed_bytes(&self) -> Result<Vec<u8>, DsseError> {
        Ok(pae(&self.payload_type, &self.payload_bytes()?))
    }

    /// Verify against any of `keys`. **At least one signature must verify**,
    /// and an envelope with no signatures is an error rather than a pass — an
    /// empty list satisfies "all signatures are valid" vacuously, which is how
    /// an unsigned artifact gets treated as signed.
    pub fn verify(&self, keys: &[ed25519_dalek::VerifyingKey]) -> Result<usize, DsseError> {
        if self.signatures.is_empty() {
            return Err(DsseError::NoSignatures);
        }
        let signed = self.signed_bytes()?;
        for (i, s) in self.signatures.iter().enumerate() {
            let Ok(raw) = b64().decode(&s.sig) else {
                continue;
            };
            let Ok(bytes) = <[u8; 64]>::try_from(raw.as_slice()) else {
                continue;
            };
            let sig = ed25519_dalek::Signature::from_bytes(&bytes);
            for key in keys {
                if ed25519_dalek::Verifier::verify(key, &signed, &sig).is_ok() {
                    return Ok(i);
                }
            }
        }
        Err(DsseError::NoSignatureVerified)
    }
}

/// Build and sign an envelope.
pub fn sign(
    payload: &[u8],
    payload_type: &str,
    key: &ed25519_dalek::SigningKey,
    keyid: Option<String>,
) -> Envelope {
    use ed25519_dalek::Signer as _;
    let sig = key.sign(&pae(payload_type, payload));
    Envelope {
        payload: b64().encode(payload),
        payload_type: payload_type.to_string(),
        signatures: vec![Signature {
            sig: b64().encode(sig.to_bytes()),
            keyid,
        }],
    }
}

/// A stable id for a public key: `sha256:` of its raw bytes.
pub fn key_id(key: &ed25519_dalek::VerifyingKey) -> String {
    format!(
        "sha256:{}",
        thruline_core::chain::sha256_hex(key.as_bytes())
    )
}
