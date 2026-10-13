//! DSSE envelopes and the PAE encoding they sign over.
//!
//! # Byte-compatible with `thruline_attest::dsse` on purpose
//!
//! `crates/attest/src/dsse.rs` already implements PAE and already has a
//! byte-for-byte test vector for it. The merge replaces *who verifies the
//! signature*, not *what is signed*, so this module reproduces that encoding
//! exactly and its own test asserts the same vector. A signer and a verifier
//! that reconstruct PAE differently do not fail loudly — they fail as
//! "signature invalid", which reads as tampering.
//!
//! ```text
//! PAE(type, body) = "DSSEv1" ‖ SP ‖ LEN(type) ‖ SP ‖ type ‖ SP ‖ LEN(body) ‖ SP ‖ body
//! ```
//!
//! The payload travels base64 and is signed **raw**.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    /// Base64 (standard alphabet) of the raw payload.
    pub payload: String,
    #[serde(rename = "payloadType")]
    pub payload_type: String,
    pub signatures: Vec<Signature>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signature {
    /// Base64 of the raw signature bytes.
    pub sig: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyid: Option<String>,
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
    /// The raw payload bytes.
    pub fn payload_bytes(&self) -> Result<Vec<u8>> {
        crate::bundle::b64_decode("dsseEnvelope.payload", &self.payload)
    }

    /// The exact bytes a signature over this envelope covers.
    pub fn signed_bytes(&self) -> Result<Vec<u8>> {
        Ok(pae(&self.payload_type, &self.payload_bytes()?))
    }

    /// Verify against a DER SubjectPublicKeyInfo, trying P-256 then P-384.
    ///
    /// **An envelope with no signatures is an error, not a pass** — an empty
    /// list satisfies "all signatures are valid" vacuously, which is how an
    /// unsigned artifact gets treated as signed.
    pub fn verify_with_spki(&self, spki_der: &[u8]) -> Result<usize> {
        if self.signatures.is_empty() {
            return Err(Error::NoSignatures);
        }
        let signed = self.signed_bytes()?;
        for (i, s) in self.signatures.iter().enumerate() {
            let Ok(raw) = crate::bundle::b64_decode("dsseEnvelope.signatures[].sig", &s.sig) else {
                continue;
            };
            if crate::x509::verify_ecdsa_spki(spki_der, &signed, &raw).is_ok() {
                return Ok(i);
            }
        }
        Err(Error::SignatureInvalid)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn pae_matches_the_dsse_specification_byte_for_byte() {
        // The same vector `crates/attest/tests/adversary.rs` asserts.
        assert_eq!(
            super::pae("http://example.com/HelloWorld", b"hello world"),
            b"DSSEv1 29 http://example.com/HelloWorld 11 hello world".to_vec()
        );
    }

    #[test]
    fn length_prefixes_prevent_a_replay_across_a_different_split() {
        // Without LEN(), these two would sign identical bytes.
        assert_ne!(super::pae("ab", b"cd"), super::pae("abc", b"d"));
    }
}
