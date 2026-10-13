//! Sigstore bundle v0.3, and the ProtoJSON quirks that bite.
//!
//! # Three encoding rules that are not obvious and are not optional
//!
//! 1. **int64 fields are JSON *strings***. `logIndex`, `treeSize` and
//!    `integratedTime` arrive quoted. Confirmed across 94 real bundles.
//! 2. **bytes fields are standard-alphabet base64**, never URL-safe — and real
//!    bundles contain **line-wrapped** base64. `prost-reflect` rejects those;
//!    `sigstore-python` accepts them. We strip ASCII whitespace before
//!    decoding, so we parse what the ecosystem actually emits.
//! 3. **v0.3 carries a single leaf `certificate`**, not `x509CertificateChain`.
//!    The chain form was removed so a verifier cannot be handed its own
//!    intermediates by the artifact it is checking.

use serde::{Deserialize, Serialize};

use crate::dsse::Envelope;
use crate::error::{Error, Result};

/// Decode standard base64, tolerating the line wrapping real bundles contain.
pub fn b64_decode(field: &'static str, s: &str) -> Result<Vec<u8>> {
    use base64::Engine as _;
    let stripped: String = s.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(&stripped)
        .map_err(|e| Error::Base64 {
            field,
            reason: e.to_string(),
        })
}

pub fn b64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// A ProtoJSON int64: quoted in the wire form, integer in ours.
fn de_i64_str<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<i64, D::Error> {
    struct V;
    impl serde::de::Visitor<'_> for V {
        type Value = i64;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("an int64 as a JSON string or number")
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<i64, E> {
            v.parse().map_err(E::custom)
        }
        fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<i64, E> {
            Ok(v)
        }
        fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<i64, E> {
            Ok(v as i64)
        }
    }
    d.deserialize_any(V)
}

fn de_u64_str<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<u64, D::Error> {
    Ok(de_i64_str(d)?.max(0) as u64)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    #[serde(rename = "verificationMaterial")]
    pub verification_material: VerificationMaterial,
    #[serde(
        rename = "messageSignature",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub message_signature: Option<MessageSignature>,
    #[serde(
        rename = "dsseEnvelope",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub dsse_envelope: Option<Envelope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationMaterial {
    /// v0.3's single leaf. Base64 DER.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate: Option<Certificate>,
    /// v0.1/v0.2's chain form. Parsed only so it can be **rejected by name**
    /// in a v0.3 bundle rather than silently ignored.
    #[serde(
        rename = "x509CertificateChain",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub x509_certificate_chain: Option<CertificateChain>,
    #[serde(rename = "publicKey", default, skip_serializing_if = "Option::is_none")]
    pub public_key: Option<serde_json::Value>,
    #[serde(rename = "tlogEntries", default)]
    pub tlog_entries: Vec<TlogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Certificate {
    #[serde(rename = "rawBytes")]
    pub raw_bytes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateChain {
    #[serde(default)]
    pub certificates: Vec<Certificate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSignature {
    #[serde(rename = "messageDigest")]
    pub message_digest: MessageDigest,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageDigest {
    pub algorithm: String,
    pub digest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TlogEntry {
    #[serde(rename = "logIndex", deserialize_with = "de_u64_str")]
    pub log_index: u64,
    #[serde(rename = "logId")]
    pub log_id: LogId,
    #[serde(rename = "kindVersion")]
    pub kind_version: KindVersion,
    #[serde(rename = "integratedTime", deserialize_with = "de_i64_str")]
    pub integrated_time: i64,
    #[serde(
        rename = "inclusionPromise",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub inclusion_promise: Option<InclusionPromise>,
    #[serde(
        rename = "inclusionProof",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub inclusion_proof: Option<InclusionProof>,
    #[serde(rename = "canonicalizedBody")]
    pub canonicalized_body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogId {
    #[serde(rename = "keyId")]
    pub key_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KindVersion {
    pub kind: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InclusionPromise {
    #[serde(rename = "signedEntryTimestamp")]
    pub signed_entry_timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InclusionProof {
    /// **The shard-local index.** Not the same number as
    /// [`TlogEntry::log_index`], which is global across Rekor's shards. Using
    /// the global one here silently computes a different Merkle root, and the
    /// proof-length check does not catch it.
    #[serde(rename = "logIndex", deserialize_with = "de_u64_str")]
    pub log_index: u64,
    #[serde(rename = "rootHash")]
    pub root_hash: String,
    #[serde(rename = "treeSize", deserialize_with = "de_u64_str")]
    pub tree_size: u64,
    #[serde(default)]
    pub hashes: Vec<String>,
    #[serde(default)]
    pub checkpoint: Checkpoint,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Checkpoint {
    #[serde(default)]
    pub envelope: String,
}

impl Bundle {
    pub fn from_json(s: &str) -> Result<Bundle> {
        let b: Bundle = serde_json::from_str(s)?;
        b.validate_shape()?;
        Ok(b)
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Is this one of the two current spellings of v0.3?
    pub fn is_v03(&self) -> bool {
        self.media_type == crate::BUNDLE_V03 || self.media_type == crate::BUNDLE_V03_LEGACY
    }

    /// Structural rules checked before any cryptography.
    ///
    /// A correctly signed document of the wrong *shape* is a valid signature
    /// over something this build does not understand.
    pub fn validate_shape(&self) -> Result<()> {
        if !self.is_v03() {
            return Err(Error::MediaType {
                found: self.media_type.clone(),
            });
        }
        // v0.3's verification material is a oneof. `certificate` is the keyless
        // form; `publicKey` is the legitimate bring-your-own-key form and does
        // occur in the wild (4 of 61 v0.3 bundles in the reference corpus).
        // `x509CertificateChain` is the one v0.3 removed, and it is refused by
        // name: it is how a bundle would hand a verifier its own intermediates.
        //
        // `sigstore` 0.14.0 rejects `publicKey` outright, so it cannot read a
        // locally-signed bundle at all.
        if self.verification_material.certificate.is_none() {
            if self.verification_material.x509_certificate_chain.is_some() {
                return Err(Error::WrongVerificationMaterial {
                    found: "x509CertificateChain",
                });
            }
            if self.verification_material.public_key.is_none() {
                return Err(Error::WrongVerificationMaterial { found: "nothing" });
            }
        }
        if self.message_signature.is_none() && self.dsse_envelope.is_none() {
            return Err(Error::NoContent);
        }
        Ok(())
    }

    /// Was this signed with a bare public key rather than a Fulcio certificate?
    ///
    /// Such a bundle proves possession of a key and **nothing about who holds
    /// it** — there is no certificate, so no identity, no issuer, no SCT and
    /// usually no log entry. [`crate::verify`] refuses it by name rather than
    /// returning an [`crate::Identity`] it cannot substantiate.
    pub fn is_public_key_bundle(&self) -> bool {
        self.verification_material.certificate.is_none()
            && self.verification_material.public_key.is_some()
    }

    /// The leaf certificate's DER bytes.
    pub fn leaf_der(&self) -> Result<Vec<u8>> {
        let c = self
            .verification_material
            .certificate
            .as_ref()
            .ok_or(Error::WrongVerificationMaterial { found: "nothing" })?;
        b64_decode("verificationMaterial.certificate.rawBytes", &c.raw_bytes)
    }
}
