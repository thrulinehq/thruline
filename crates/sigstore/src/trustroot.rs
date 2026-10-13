//! The Sigstore trust root — `trusted_root.json` — and the rotation windows
//! that decide which key was authoritative when.
//!
//! # TUF is how this is refreshed, not how it is used
//!
//! The industry has converged away from "cache a TUF repository and verify
//! against it offline". cosign deprecated `--offline` in favour of
//! `--trusted-root`; sigstore-go's own documentation says repository caching
//! "is not an optimal control for air-gapped environments". So this crate
//! treats the trust root as **data**: a file you ship, pin, or refresh.
//!
//! That makes offline verification honest rather than approximate — there is no
//! TUF metadata whose expiry we would have to either enforce (making air-gapped
//! verification impossible after a few weeks) or quietly ignore (discarding the
//! freshness guarantee TUF exists to provide). `sigstore-python`'s offline mode
//! takes the second option silently. We take neither, because there is no TUF
//! metadata in the verify path at all.
//!
//! Refresh lives behind the `net` feature and is a separate, explicit act.
//!
//! **If a TUF bootstrap is ever added, pin root v12 or later.** Root v11
//! carries a keyid that disagrees with its own key object (an
//! `x-tuf-on-ci-online-uri` field was edited without re-deriving the id), so a
//! walk through it fails to parse in `tough`.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const TRUSTED_ROOT_MEDIA_TYPE: &str =
    "application/vnd.dev.sigstore.trustedroot+json;version=0.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustRoot {
    #[serde(rename = "mediaType")]
    pub media_type: String,
    #[serde(rename = "tlogs", default)]
    pub tlogs: Vec<TransparencyLog>,
    #[serde(rename = "certificateAuthorities", default)]
    pub certificate_authorities: Vec<CertificateAuthority>,
    #[serde(rename = "ctlogs", default)]
    pub ctlogs: Vec<TransparencyLog>,
    #[serde(rename = "timestampAuthorities", default)]
    pub timestamp_authorities: Vec<CertificateAuthority>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransparencyLog {
    #[serde(rename = "baseUrl", default)]
    pub base_url: String,
    #[serde(rename = "hashAlgorithm", default)]
    pub hash_algorithm: String,
    #[serde(rename = "publicKey")]
    pub public_key: PublicKey,
    #[serde(rename = "logId")]
    pub log_id: LogId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicKey {
    /// Base64 DER SubjectPublicKeyInfo.
    #[serde(rename = "rawBytes", default)]
    pub raw_bytes: String,
    #[serde(rename = "keyDetails", default)]
    pub key_details: String,
    #[serde(rename = "validFor", default)]
    pub valid_for: Option<ValidFor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogId {
    #[serde(rename = "keyId")]
    pub key_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificateAuthority {
    #[serde(default)]
    pub subject: serde_json::Value,
    #[serde(default)]
    pub uri: String,
    #[serde(rename = "certChain")]
    pub cert_chain: CertChain,
    #[serde(rename = "validFor", default)]
    pub valid_for: Option<ValidFor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertChain {
    #[serde(default)]
    pub certificates: Vec<RawCert>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawCert {
    #[serde(rename = "rawBytes")]
    pub raw_bytes: String,
}

/// A rotation window. `end` absent means "still current".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidFor {
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub end: Option<String>,
}

impl ValidFor {
    /// Was this key or CA authoritative at `at`?
    ///
    /// **This is how key rotation works**, and skipping it is how a verifier
    /// either rejects every old bundle or accepts one signed by a retired key.
    pub fn covers(&self, at: time::OffsetDateTime) -> Result<bool> {
        if let Some(s) = &self.start {
            if at < parse_rfc3339("validFor.start", s)? {
                return Ok(false);
            }
        }
        if let Some(e) = &self.end {
            if at > parse_rfc3339("validFor.end", e)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

pub fn parse_rfc3339(field: &'static str, s: &str) -> Result<time::OffsetDateTime> {
    time::OffsetDateTime::parse(s, &time::format_description::well_known::Rfc3339).map_err(|_| {
        Error::Timestamp {
            field,
            value: s.to_string(),
        }
    })
}

impl TrustRoot {
    pub fn from_json(s: &str) -> Result<TrustRoot> {
        let r: TrustRoot = serde_json::from_str(s)?;
        // The media type is checked by prefix: the spec's own files carry the
        // versioned form, and a future v0.2 should be a stated refusal rather
        // than a confusing parse failure three fields later.
        if !r
            .media_type
            .starts_with("application/vnd.dev.sigstore.trustedroot+json")
        {
            return Err(Error::TrustRootMediaType {
                found: r.media_type.clone(),
                expected: TRUSTED_ROOT_MEDIA_TYPE.to_string(),
            });
        }
        Ok(r)
    }

    pub fn from_file(path: &std::path::Path) -> Result<TrustRoot> {
        let s = std::fs::read_to_string(path).map_err(|source| Error::Io {
            path: path.display().to_string(),
            source,
        })?;
        TrustRoot::from_json(&s)
    }

    /// Every CA whose window covers `at`, newest window first.
    pub fn cas_valid_at(&self, at: time::OffsetDateTime) -> Result<Vec<&CertificateAuthority>> {
        let mut out = Vec::new();
        for ca in &self.certificate_authorities {
            let ok = match &ca.valid_for {
                Some(v) => v.covers(at)?,
                None => true,
            };
            if ok {
                out.push(ca);
            }
        }
        Ok(out)
    }

    /// The transparency log with this key id (hex or base64 as it appears in
    /// the bundle), if its window covers `at`.
    pub fn tlog_by_id(&self, key_id: &[u8], at: time::OffsetDateTime) -> Result<&TransparencyLog> {
        find_log(&self.tlogs, key_id, at, "transparency log")
    }

    /// The certificate transparency log with this key id.
    pub fn ctlog_by_id(&self, key_id: &[u8], at: time::OffsetDateTime) -> Result<&TransparencyLog> {
        find_log(&self.ctlogs, key_id, at, "certificate transparency log")
    }
}

fn find_log<'a>(
    logs: &'a [TransparencyLog],
    key_id: &[u8],
    at: time::OffsetDateTime,
    kind: &'static str,
) -> Result<&'a TransparencyLog> {
    let want = hex::encode(key_id);
    let mut saw_id = false;
    for l in logs {
        let id = crate::bundle::b64_decode("trustedRoot.logId.keyId", &l.log_id.key_id)
            .map(hex::encode)
            .unwrap_or_else(|_| l.log_id.key_id.to_ascii_lowercase());
        if id != want {
            continue;
        }
        saw_id = true;
        let ok = match &l.public_key.valid_for {
            Some(v) => v.covers(at)?,
            None => true,
        };
        if ok {
            return Ok(l);
        }
    }
    if saw_id {
        return Err(Error::NoValidKey {
            kind,
            at: at.to_string(),
        });
    }
    Err(Error::UnknownLog { log_id: want })
}
