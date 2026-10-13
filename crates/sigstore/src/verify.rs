//! The orchestrator: five checks, in an order that is itself part of the
//! argument.
//!
//! # Why the transparency log is verified *before* the certificate
//!
//! A Fulcio certificate lives ten minutes, so its validity window can only be
//! judged against the instant it signed. That instant comes from the log entry.
//! But the log entry arrives inside the bundle, supplied by whoever produced
//! it — so using its `integratedTime` before establishing that the entry is
//! real is dating a certificate by a number the attacker chose.
//!
//! `sigstore` 0.14.0 does exactly that: `verifier.rs:217` reads
//! `log_entry.integrated_time` to check the certificate's expiry, while the
//! inclusion proof and the SET that would authenticate that timestamp are the
//! two `TODO`s twenty lines above. The time anchor rests on nothing.
//!
//! So here the log comes first:
//!
//! 1. **Inclusion proof** — the entry is in a tree whose root the log signed.
//! 2. **SET** — the log's own signature over `integratedTime`, which is what
//!    makes the timestamp evidence rather than an assertion. `integratedTime`
//!    is *not* inside `canonicalizedBody`, so the inclusion proof alone does
//!    not cover it; this is a separate signature and it is checked separately.
//! 3. **Entry body** matches the bundle's own content, so the proof is about
//!    *this* artifact and not some other entry in the same log.
//! 4. **Chain**, at the now-trusted instant.
//! 5. **SCT**, **identity**, and the **DSSE signature** by the certified key.
//!
//! # There is no path here that returns success without checking
//!
//! Every step is a `?`. The function returns [`Identity`] — what was proved —
//! rather than a boolean, because a boolean is the shape that lets a caller
//! write `if verify(..).is_ok()` and never look at *who* signed, which is the
//! whole question a signature answers.

use sha2::{Digest, Sha256};

use crate::bundle::{b64_decode, Bundle, TlogEntry};
use crate::error::{Error, Result};
use crate::identity::Identity;
use crate::policy::Policy;

/// Verify a bundle, returning the identity it proves.
pub fn verify(bundle: &Bundle, policy: &Policy) -> Result<Identity> {
    bundle.validate_shape()?;
    if bundle.is_public_key_bundle() {
        // There is no certificate, so there is no identity to return. Returning
        // Ok here with a placeholder identity would be the exact collapse this
        // crate is built against.
        return Err(Error::NoCertificateOnlyAPublicKey);
    }

    let leaf_der = bundle.leaf_der()?;
    let leaf = crate::x509::parse_der(&leaf_der)?;

    // --- 1-3. the transparency log, before anything trusts its clock --------
    let entry = bundle.verification_material.tlog_entries.first();
    let signing_time = match entry {
        Some(e) => {
            let verified = verify_tlog_entry(bundle, e, policy)?;
            Some(verified)
        }
        None => {
            if policy.require_inclusion_proof {
                return Err(Error::InclusionProofMissing);
            }
            None
        }
    };

    // The instant the certificate is judged at. An explicit `at_time` overrides
    // — that is how a caller re-checks a historical bundle — but the default is
    // the log's own timestamp, never `now`.
    let at = match (policy.at_time, signing_time) {
        (Some(t), _) => t,
        (None, Some(t)) => t,
        (None, None) => {
            // No log entry and no supplied instant: there is no defensible time
            // to judge a ten-minute certificate at. Refuse rather than reach
            // for the wall clock.
            return Err(Error::NoValidKey {
                kind: "verified timestamp (no log entry and no at_time supplied)",
                at: "unknown".to_string(),
            });
        }
    };

    // --- 4. the chain, at that instant --------------------------------------
    let (roots, intermediates) = split_cas(policy, at)?;
    if roots.is_empty() {
        return Err(Error::NoTrustedRoot);
    }
    let path = crate::x509::verify_chain(&leaf, &intermediates, &roots, at)?;

    // --- 5. SCT -------------------------------------------------------------
    if policy.require_sct {
        let issuer = path.get(1).ok_or(Error::NoTrustedRoot)?;
        let scts = crate::sct::extract(&leaf)?;
        if scts.is_empty() {
            return Err(Error::SctMissing);
        }
        let mut any = false;
        let mut last = None;
        for sct in &scts {
            let log = match policy.trust_root.ctlog_by_id(&sct.log_id, at) {
                Ok(l) => l,
                Err(e) => {
                    last = Some(e);
                    continue;
                }
            };
            let spki = b64_decode(
                "trustedRoot.ctlogs[].publicKey.rawBytes",
                &log.public_key.raw_bytes,
            )?;
            match crate::sct::verify(sct, &leaf, issuer, &spki) {
                Ok(()) => {
                    any = true;
                    break;
                }
                Err(e) => last = Some(e),
            }
        }
        if !any {
            return Err(last.unwrap_or(Error::SctInvalid));
        }
    }

    // --- 6. identity --------------------------------------------------------
    let (san, san_kind, issuer_url) = crate::identity::extract(&leaf)?;
    // One implementation, in `policy`, so the rule a test exercises is the rule
    // verification applies.
    policy.identity.check(&san, san_kind)?;
    if let Some(want) = &policy.identity.issuer {
        if &issuer_url != want {
            return Err(Error::IssuerMismatch {
                found: issuer_url.clone(),
                expected: want.clone(),
            });
        }
    }

    // --- 7. the DSSE signature by the certified key -------------------------
    let spki = spki_of(&leaf)?;
    if let Some(env) = &bundle.dsse_envelope {
        env.verify_with_spki(&spki)?;
    } else if let Some(ms) = &bundle.message_signature {
        let digest = b64_decode(
            "messageSignature.messageDigest.digest",
            &ms.message_digest.digest,
        )?;
        let sig = b64_decode("messageSignature.signature", &ms.signature)?;
        // A message signature is over the digest itself as a prehash.
        crate::x509::verify_ecdsa_prehash(&spki, &digest, &sig)?;
    } else {
        return Err(Error::NoContent);
    }

    let v = leaf.tbs_certificate().validity();
    Ok(Identity {
        san,
        san_kind,
        issuer: issuer_url,
        not_before: unix(v.not_before.to_unix_duration().as_secs() as i64)?,
        not_after: unix(v.not_after.to_unix_duration().as_secs() as i64)?,
        log_index: entry.map(|e| e.log_index),
        integrated_time: signing_time,
    })
}

fn unix(s: i64) -> Result<time::OffsetDateTime> {
    time::OffsetDateTime::from_unix_timestamp(s).map_err(|_| Error::Timestamp {
        field: "certificate validity",
        value: s.to_string(),
    })
}

fn spki_of(cert: &x509_cert::Certificate) -> Result<Vec<u8>> {
    use der::Encode;
    cert.tbs_certificate()
        .subject_public_key_info()
        .to_der()
        .map_err(|e| Error::CertificateParse(format!("re-encoding SubjectPublicKeyInfo: {e}")))
}

/// Roots and intermediates from the trust root's CAs, at `at`.
///
/// **The last certificate of each `certChain` is the root; everything before it
/// is an intermediate.** Flattening them into one anchor set — which is what
/// `sigstore` 0.14.0's `fulcio_certs()` does — makes the intermediate a trust
/// anchor and skips verifying its signature by the root.
fn split_cas(
    policy: &Policy,
    at: time::OffsetDateTime,
) -> Result<(Vec<x509_cert::Certificate>, Vec<x509_cert::Certificate>)> {
    let mut roots = Vec::new();
    let mut intermediates = Vec::new();
    for ca in policy.trust_root.cas_valid_at(at)? {
        let n = ca.cert_chain.certificates.len();
        for (i, raw) in ca.cert_chain.certificates.iter().enumerate() {
            let der = b64_decode(
                "trustedRoot.certificateAuthorities[].certChain.certificates[].rawBytes",
                &raw.raw_bytes,
            )?;
            let cert = crate::x509::parse_der(&der)?;
            if i + 1 == n {
                roots.push(cert);
            } else {
                intermediates.push(cert);
            }
        }
    }
    Ok((roots, intermediates))
}

/// Steps 1-3. Returns the **verified** signing instant.
fn verify_tlog_entry(
    bundle: &Bundle,
    entry: &TlogEntry,
    policy: &Policy,
) -> Result<time::OffsetDateTime> {
    let log_id = b64_decode("tlogEntries[].logId.keyId", &entry.log_id.key_id)?;
    let integrated = unix(entry.integrated_time)?;
    let log = policy.trust_root.tlog_by_id(&log_id, integrated)?;
    let log_spki = b64_decode(
        "trustedRoot.tlogs[].publicKey.rawBytes",
        &log.public_key.raw_bytes,
    )?;

    let body = b64_decode("tlogEntries[].canonicalizedBody", &entry.canonicalized_body)?;

    // --- 1. inclusion proof ------------------------------------------------
    match &entry.inclusion_proof {
        None => {
            if policy.require_inclusion_proof {
                return Err(Error::InclusionProofMissing);
            }
        }
        Some(p) => {
            let checkpoint = crate::checkpoint::Checkpoint::parse(&p.checkpoint.envelope)?;
            checkpoint.verify(&log_spki)?;

            let root = b64_or_hex("inclusionProof.rootHash", &p.root_hash)?;
            // The checkpoint is the log's signature over the root; the
            // unsigned `rootHash` field must agree with it or the proof is
            // being checked against a root nobody attested.
            if checkpoint.root_hash != root {
                return Err(Error::InclusionProofInvalid {
                    computed: hex::encode(root),
                    expected: hex::encode(checkpoint.root_hash),
                });
            }

            let mut hashes = Vec::with_capacity(p.hashes.len());
            for h in &p.hashes {
                hashes.push(b64_or_hex("inclusionProof.hashes[]", h)?);
            }
            let leaf = crate::merkle::leaf_hash(&body);
            // NOTE: the SHARD-LOCAL index, `p.log_index` — not the global
            // `entry.log_index`. The two differ by the sum of the preceding
            // shards' sizes, and using the global one computes a different root
            // while still passing the proof-length check.
            crate::merkle::verify_inclusion(
                p.log_index,
                p.tree_size,
                &leaf,
                &hashes,
                &checkpoint.root_hash,
            )?;
        }
    }

    // --- 2. the SET, which is what authenticates `integratedTime` -----------
    if let Some(promise) = &entry.inclusion_promise {
        let sig = b64_decode(
            "inclusionPromise.signedEntryTimestamp",
            &promise.signed_entry_timestamp,
        )?;
        let payload = set_payload(entry);
        crate::x509::verify_ecdsa_spki(&log_spki, payload.as_bytes(), &sig)
            .map_err(|_| Error::CheckpointSignatureInvalid)?;
    } else if entry.inclusion_proof.is_none() {
        // Neither a proof nor a promise: nothing ties this entry to the log.
        return Err(Error::InclusionProofMissing);
    }

    // --- 3. the entry is about THIS artifact --------------------------------
    if !body_matches_bundle(bundle, &body)? {
        return Err(Error::LogEntryMismatch);
    }

    Ok(integrated)
}

/// The exact bytes a Signed Entry Timestamp covers.
///
/// RFC 8785-style canonical JSON of the entry without its `verification` block:
/// keys sorted lexicographically, no whitespace, numbers unquoted. `body` is
/// the base64 string as it arrived; `logID` is hex.
///
/// `"logID"` sorts before `"logIndex"` because `'D'` (0x44) precedes `'n'`
/// (0x6e) — an ordering that is easy to get backwards by eye and produces a
/// signature that never verifies.
fn set_payload(entry: &TlogEntry) -> String {
    format!(
        r#"{{"body":"{}","integratedTime":{},"logID":"{}","logIndex":{}}}"#,
        entry.canonicalized_body.replace(['\n', '\r'], ""),
        entry.integrated_time,
        log_id_hex(&entry.log_id.key_id),
        entry.log_index
    )
}

fn log_id_hex(key_id: &str) -> String {
    match b64_decode("tlogEntries[].logId.keyId", key_id) {
        Ok(b) => hex::encode(b),
        Err(_) => key_id.to_ascii_lowercase(),
    }
}

/// Rekor encodes hashes as hex in some places and base64 in others; a verifier
/// that assumes one silently mis-parses the other into a wrong root.
fn b64_or_hex(field: &'static str, s: &str) -> Result<[u8; 32]> {
    if s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()) {
        if let Ok(v) = hex::decode(s) {
            if let Ok(a) = <[u8; 32]>::try_from(v.as_slice()) {
                return Ok(a);
            }
        }
    }
    let v = b64_decode(field, s)?;
    <[u8; 32]>::try_from(v.as_slice()).map_err(|_| Error::Base64 {
        field,
        reason: format!("expected 32 bytes, got {}", v.len()),
    })
}

/// Does the log entry describe the artifact this bundle carries?
///
/// Without this, a valid inclusion proof for *somebody else's* entry would
/// satisfy every transparency check while the bundle attested to something
/// entirely different.
fn body_matches_bundle(bundle: &Bundle, body: &[u8]) -> Result<bool> {
    let v: serde_json::Value = serde_json::from_slice(body)?;
    let kind = v.get("kind").and_then(|k| k.as_str()).unwrap_or_default();
    match kind {
        "dsse" | "intoto" => {
            // The entry records the payload's digest; compare it with ours.
            let Some(env) = &bundle.dsse_envelope else {
                return Ok(false);
            };
            let payload = env.payload_bytes()?;
            let want = hex::encode(Sha256::digest(&payload));
            Ok(json_contains_str(&v, &want))
        }
        "hashedrekord" => {
            let Some(ms) = &bundle.message_signature else {
                return Ok(false);
            };
            let digest = b64_decode(
                "messageSignature.messageDigest.digest",
                &ms.message_digest.digest,
            )?;
            Ok(json_contains_str(&v, &hex::encode(digest)))
        }
        // An entry type this build does not model is **not** a pass. Saying so
        // is the difference between "checked and agreed" and "did not look".
        _ => Ok(false),
    }
}

fn json_contains_str(v: &serde_json::Value, needle: &str) -> bool {
    match v {
        serde_json::Value::String(s) => s.eq_ignore_ascii_case(needle),
        serde_json::Value::Array(a) => a.iter().any(|x| json_contains_str(x, needle)),
        serde_json::Value::Object(o) => o.values().any(|x| json_contains_str(x, needle)),
        _ => false,
    }
}

/// Verify a bundle signed with a bare public key, against a key you already
/// trust. Returns the **id of the key that verified it**.
///
/// This is the offline/enterprise path: no Fulcio, no transparency log, no
/// certificate. It cannot return an [`Identity`], because a bare key names
/// nobody — inventing a plausible-looking one here would be the whole failure
/// this crate is against.
///
/// # Why it returns the key id rather than `()`
///
/// Operator ruling 8 (2026-09-06): *"`success` requires a signature whose
/// identity the policy names: a keyless OIDC identity … or a policy-listed
/// public key held outside the repository."* A `()` return leaves the caller
/// unable to record **which** listed key matched, so a policy holding three
/// keys could not say which one signed — and "some key on the list" is not an
/// identity a gate can name.
///
/// The id is `sha256:<hex>` over the DER `SubjectPublicKeyInfo`, mirroring
/// `thruline_attest::dsse::key_id`'s spelling so the two are comparable at a
/// glance. **The bytes hashed differ**: `attest` hashes an Ed25519 key's 32 raw
/// bytes, this hashes the whole SPKI, because an EC key is not meaningful
/// without its curve. Same format, different preimage — said here so nobody
/// compares the two ids and concludes the keys differ.
///
/// Whether the key is *policy-listed* and *held outside the repository* is the
/// core's judgement, not this crate's; ruling 8 assigns that half to the core.
pub fn verify_with_public_key(bundle: &Bundle, spki_der: &[u8]) -> Result<String> {
    bundle.validate_shape()?;
    if let Some(env) = &bundle.dsse_envelope {
        env.verify_with_spki(spki_der)?;
        Ok(public_key_id(spki_der))
    } else if let Some(ms) = &bundle.message_signature {
        let digest = b64_decode(
            "messageSignature.messageDigest.digest",
            &ms.message_digest.digest,
        )?;
        let sig = b64_decode("messageSignature.signature", &ms.signature)?;
        crate::x509::verify_ecdsa_prehash(spki_der, &digest, &sig)?;
        Ok(public_key_id(spki_der))
    } else {
        Err(Error::NoContent)
    }
}

/// `sha256:<hex>` over a DER SubjectPublicKeyInfo. See
/// [`verify_with_public_key`] for why the preimage is the whole SPKI.
pub fn public_key_id(spki_der: &[u8]) -> String {
    format!("sha256:{}", hex::encode(Sha256::digest(spki_der)))
}
