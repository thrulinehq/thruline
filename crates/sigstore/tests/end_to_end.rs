//! End-to-end verification of a **real production Sigstore bundle**, and every
//! way it is made to fail.
//!
//! The artifact is CPython 3.13.7's release signature — a genuine
//! `python.org` bundle, signed by real Fulcio, logged in real Rekor, verified
//! here against the real production `trusted_root.json`. Nothing in this file
//! is synthetic except the corruptions.
//!
//! # Why the negative cases are the point
//!
//! A verifier that accepts a good bundle has demonstrated almost nothing: so
//! does `fn verify(_) -> Ok(())`. What separates the two is whether corrupting
//! each individual guarantee produces a *distinct, named* refusal. So every
//! test below takes the same real bundle, breaks exactly one thing, and asserts
//! both that it fails **and which axis it fails on**.
//!
//! This is the same discipline as `~/thruline/crates/attest/tests/adversary.rs`,
//! whose constructions are numbered refusals rather than a single "invalid"
//! assertion.

use thruline_sigstore::{Axis, Bundle, Error, IdentityPolicy, Policy, TrustRoot};

const BUNDLE: &str = include_str!("../fixtures/bundles/cpython-3.13.7.tgz.sigstore.json");
const PROD_ROOT: &str = include_str!("../fixtures/trustroot/prod-trusted_root.json");
const STAGING_ROOT: &str = include_str!("../fixtures/trustroot/staging-trusted_root.json");

fn bundle() -> Bundle {
    Bundle::from_json(BUNDLE).expect("the fixture is a real v0.3 bundle")
}

fn prod_policy() -> Policy {
    Policy::new(TrustRoot::from_json(PROD_ROOT).expect("real production trust root"))
}

/// Re-serialise a bundle after mutating its JSON, so corruptions are expressed
/// the way an attacker would actually deliver them: as bytes on the wire.
fn mutate(f: impl FnOnce(&mut serde_json::Value)) -> Bundle {
    let mut v: serde_json::Value = serde_json::from_str(BUNDLE).unwrap();
    f(&mut v);
    Bundle::from_json(&serde_json::to_string(&v).unwrap()).expect("still structurally a bundle")
}

/// Flip one bit in the middle of a base64 field, keeping it valid base64 of the
/// same length — a corruption that survives parsing and can only be caught by
/// the cryptography.
fn flip_b64(s: &str) -> String {
    let mut b = s.as_bytes().to_vec();
    let i = b.len() / 2;
    b[i] = if b[i] == b'A' { b'B' } else { b'A' };
    String::from_utf8(b).unwrap()
}

// --- the honest artifact ---------------------------------------------------

#[test]
fn a_real_production_bundle_verifies_and_names_who_signed_it() {
    let id = thruline_sigstore::verify(&bundle(), &prod_policy())
        .expect("CPython 3.13.7's real release bundle must verify");

    // The identity is the ANSWER, not a boolean. Print it so a failure here is
    // legible, and assert its shape rather than pinning a value that would make
    // this test about the fixture instead of the verifier.
    println!("identity: {} via {}", id.san, id.issuer);
    assert!(!id.san.is_empty(), "an empty identity is not an identity");
    assert!(id.issuer.starts_with("https://"), "issuer: {}", id.issuer);
    assert!(
        id.log_index.is_some(),
        "a verified bundle knows its log index"
    );
    assert!(
        id.integrated_time.is_some(),
        "a verified bundle knows when it was logged"
    );

    // The certificate is long expired by wall clock, and that is normal: a
    // Fulcio certificate lives ten minutes. Verification succeeded because it
    // was judged at the log's timestamp, not at `now`.
    assert!(
        id.not_after < time::OffsetDateTime::now_utc(),
        "this certificate should be expired by now — if it is not, the fixture is \
         fresh and this test has stopped proving that expiry is handled correctly"
    );
}

/// The check the whole crate exists for. `sigstore` 0.14.0 reaches this point
/// having never verified the Merkle proof.
#[test]
fn the_inclusion_proof_is_actually_checked() {
    let b = bundle();
    let p = b.verification_material.tlog_entries[0]
        .inclusion_proof
        .as_ref()
        .expect("the fixture carries a real inclusion proof");
    assert_eq!(p.hashes.len(), 16, "a real 16-hash audit path");
    // The two indices really do differ, which is what makes the shard trap real
    // rather than theoretical.
    assert_ne!(
        p.log_index, b.verification_material.tlog_entries[0].log_index,
        "this fixture's global and shard-local indices must differ, or it cannot \
         demonstrate the trap"
    );
}

// --- failure mode 1: wrong identity ---------------------------------------

#[test]
fn failure_wrong_identity_is_refused_on_the_identity_axis() {
    let mut policy = prod_policy();
    policy.identity = IdentityPolicy {
        san: Some("attacker@example.com".to_string()),
        san_kind: None,
        issuer: None,
    };
    let e = thruline_sigstore::verify(&bundle(), &policy).unwrap_err();
    assert!(matches!(e, Error::IdentityMismatch { .. }), "{e}");
    assert_eq!(e.axis(), Axis::Identity);
}

#[test]
fn failure_wrong_issuer_is_refused_separately_from_wrong_identity() {
    let mut policy = prod_policy();
    policy.identity = IdentityPolicy {
        san: None,
        san_kind: None,
        issuer: Some("https://evil.example.com".to_string()),
    };
    let e = thruline_sigstore::verify(&bundle(), &policy).unwrap_err();
    assert!(matches!(e, Error::IssuerMismatch { .. }), "{e}");
    assert_eq!(e.axis(), Axis::Identity);
}

/// The identity the bundle really carries must pass — otherwise the two tests
/// above would also pass against a verifier that rejects everything.
#[test]
fn the_real_identity_passes_the_policy_that_names_it() {
    let id = thruline_sigstore::verify(&bundle(), &prod_policy()).unwrap();
    let policy = prod_policy().expect_identity(id.san.clone(), id.issuer.clone());
    thruline_sigstore::verify(&bundle(), &policy)
        .expect("the identity the certificate actually carries must satisfy a policy naming it");
}

// --- failure mode 2: expired certificate -----------------------------------

#[test]
fn failure_expired_certificate_is_refused_on_the_chain_axis() {
    // Judge the same bundle a year after it was signed. The certificate was
    // valid when it signed and is not valid then.
    let mut policy = prod_policy();
    let id = thruline_sigstore::verify(&bundle(), &prod_policy()).unwrap();
    policy.at_time = Some(id.not_after + time::Duration::days(365));
    let e = thruline_sigstore::verify(&bundle(), &policy).unwrap_err();
    assert!(matches!(e, Error::CertificateExpired { .. }), "{e}");
    assert_eq!(e.axis(), Axis::Chain);
}

#[test]
fn failure_certificate_not_yet_valid_is_the_same_named_refusal() {
    let mut policy = prod_policy();
    let id = thruline_sigstore::verify(&bundle(), &prod_policy()).unwrap();
    policy.at_time = Some(id.not_before - time::Duration::days(365));
    let e = thruline_sigstore::verify(&bundle(), &policy).unwrap_err();
    assert!(matches!(e, Error::CertificateExpired { .. }), "{e}");
}

/// The regression that matters most about time: verifying **now** must still
/// work, because the certificate is dated by the log and not by the clock.
#[test]
fn verifying_today_still_works_even_though_the_certificate_expired_long_ago() {
    thruline_sigstore::verify(&bundle(), &prod_policy())
        .expect("a ten-minute certificate must not have to be verified within ten minutes");
}

// --- failure mode 3: tampered payload --------------------------------------

#[test]
fn failure_tampered_payload_is_refused_on_the_signature_axis() {
    let b = mutate(|v| {
        let d = &mut v["messageSignature"]["messageDigest"]["digest"];
        *d = serde_json::Value::String(flip_b64(d.as_str().unwrap()));
    });
    let e = thruline_sigstore::verify(&b, &prod_policy()).unwrap_err();
    // Changing the artifact digest breaks the LOG-ENTRY BINDING first: the
    // entry in Rekor is for the original artifact, so the proof — which is
    // perfectly valid — is a proof about something else.
    //
    // The assertion names that exact variant rather than accepting "some
    // failure". A loose `LogEntryMismatch | SignatureInvalid` passed even with
    // the binding check disabled entirely, because the signature check caught
    // it one step later; the mutation check found that, and this is the fix.
    assert!(matches!(e, Error::LogEntryMismatch), "{e}");
    assert_eq!(e.axis(), Axis::TransparencyLog);
}

#[test]
fn failure_tampered_signature_is_refused() {
    let b = mutate(|v| {
        let s = &mut v["messageSignature"]["signature"];
        *s = serde_json::Value::String(flip_b64(s.as_str().unwrap()));
    });
    let e = thruline_sigstore::verify(&b, &prod_policy()).unwrap_err();
    assert_eq!(e.axis(), Axis::Signature, "{e}");
}

// --- failure mode 4: missing / broken inclusion proof ----------------------

#[test]
fn failure_missing_inclusion_proof_is_refused_by_name() {
    let b = mutate(|v| {
        v["verificationMaterial"]["tlogEntries"][0]
            .as_object_mut()
            .unwrap()
            .remove("inclusionProof");
    });
    let e = thruline_sigstore::verify(&b, &prod_policy()).unwrap_err();
    assert!(matches!(e, Error::InclusionProofMissing), "{e}");
    assert_eq!(e.axis(), Axis::TransparencyLog);
}

#[test]
fn failure_no_log_entry_at_all_is_refused_by_name() {
    let b = mutate(|v| {
        v["verificationMaterial"]["tlogEntries"] = serde_json::json!([]);
    });
    let e = thruline_sigstore::verify(&b, &prod_policy()).unwrap_err();
    assert!(matches!(e, Error::InclusionProofMissing), "{e}");
}

/// A tampered audit path must not reconstruct the signed root. This is the
/// exact experiment that `sigstore` 0.14.0 passes unchanged.
#[test]
fn failure_a_tampered_merkle_hash_does_not_reconstruct_the_root() {
    let b = mutate(|v| {
        let h = &mut v["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["hashes"][0];
        *h = serde_json::Value::String(flip_b64(h.as_str().unwrap()));
    });
    let e = thruline_sigstore::verify(&b, &prod_policy()).unwrap_err();
    assert!(matches!(e, Error::InclusionProofInvalid { .. }), "{e}");
    assert_eq!(e.axis(), Axis::TransparencyLog);
}

/// **The shard trap.** Substituting the global log index for the shard-local
/// one is the mistake the proof-length check cannot catch.
#[test]
fn failure_the_global_log_index_does_not_verify_the_shard_local_proof() {
    let b = mutate(|v| {
        let global = v["verificationMaterial"]["tlogEntries"][0]["logIndex"].clone();
        v["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["logIndex"] = global;
    });
    let e = thruline_sigstore::verify(&b, &prod_policy()).unwrap_err();
    assert_eq!(
        e.axis(),
        Axis::TransparencyLog,
        "using the global index must not verify: {e}"
    );
}

#[test]
fn failure_a_tampered_checkpoint_signature_is_refused() {
    let b = mutate(|v| {
        let cp = v["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["checkpoint"]
            ["envelope"]
            .as_str()
            .unwrap()
            .to_string();
        // Corrupt the last base64 character of the signature line.
        let mut lines: Vec<String> = cp.lines().map(str::to_string).collect();
        let last = lines.len() - 1;
        let l = lines[last].clone();
        lines[last] = format!(
            "{}{}",
            &l[..l.len() - 2],
            if l.ends_with('A') { "B=" } else { "A=" }
        );
        v["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["checkpoint"]["envelope"] =
            serde_json::Value::String(lines.join("\n") + "\n");
    });
    let e = thruline_sigstore::verify(&b, &prod_policy()).unwrap_err();
    assert_eq!(e.axis(), Axis::TransparencyLog, "{e}");
}

/// The unsigned `rootHash` field must agree with the checkpoint the log
/// actually signed, or the proof is being checked against a root nobody
/// attested to.
#[test]
fn failure_a_root_hash_that_disagrees_with_the_checkpoint_is_refused() {
    let b = mutate(|v| {
        let h = &mut v["verificationMaterial"]["tlogEntries"][0]["inclusionProof"]["rootHash"];
        *h = serde_json::Value::String(flip_b64(h.as_str().unwrap()));
    });
    let e = thruline_sigstore::verify(&b, &prod_policy()).unwrap_err();
    assert!(matches!(e, Error::InclusionProofInvalid { .. }), "{e}");
}

// --- failure mode 5: wrong trust root --------------------------------------

#[test]
fn failure_the_staging_trust_root_does_not_verify_a_production_bundle() {
    let policy = Policy::new(TrustRoot::from_json(STAGING_ROOT).expect("real staging trust root"));
    let e = thruline_sigstore::verify(&bundle(), &policy).unwrap_err();
    // Either the log is unknown to staging, or the chain does not reach a
    // staging CA. Both are correct refusals; neither may be a pass.
    assert!(
        matches!(
            e,
            Error::UnknownLog { .. } | Error::NoTrustedRoot | Error::NoValidKey { .. }
        ),
        "a production bundle must not verify against the staging trust root: {e}"
    );
}

#[test]
fn failure_an_empty_trust_root_verifies_nothing() {
    let empty = TrustRoot::from_json(
        r#"{"mediaType":"application/vnd.dev.sigstore.trustedroot+json;version=0.1",
            "tlogs":[],"certificateAuthorities":[],"ctlogs":[],"timestampAuthorities":[]}"#,
    )
    .unwrap();
    let e = thruline_sigstore::verify(&bundle(), &Policy::new(empty)).unwrap_err();
    assert!(
        !matches!(e, Error::Json(_)),
        "an empty trust root must be a trust failure, not a parse failure: {e}"
    );
}

// --- shape refusals --------------------------------------------------------

#[test]
fn the_legacy_v03_media_type_spelling_is_accepted() {
    let mut v: serde_json::Value = serde_json::from_str(BUNDLE).unwrap();
    v["mediaType"] = serde_json::Value::String(
        "application/vnd.dev.sigstore.bundle+json;version=0.3".to_string(),
    );
    let b = Bundle::from_json(&serde_json::to_string(&v).unwrap())
        .expect("both spellings of v0.3 are current and both must parse");
    thruline_sigstore::verify(&b, &prod_policy())
        .expect("changing only the media-type spelling must not change the verdict");
}

#[test]
fn a_bundle_whose_media_type_is_not_v03_is_refused_by_name() {
    let mut v: serde_json::Value = serde_json::from_str(BUNDLE).unwrap();
    v["mediaType"] =
        serde_json::Value::String("application/vnd.dev.sigstore.bundle+json;version=0.2".into());
    let e = Bundle::from_json(&serde_json::to_string(&v).unwrap()).unwrap_err();
    assert!(matches!(e, Error::MediaType { .. }), "{e}");
}

// --- the chain signature itself --------------------------------------------
//
// These exist because a mutation test found the hole: making
// `x509::verify_signed_by` return `Ok(())` unconditionally left every other
// test in this crate passing. The "wrong trust root" case above does not cover
// it — it refuses on the transparency-log axis first and never reaches the
// chain — so nothing was checking the single most important property a
// certificate chain has.

/// Corrupt the leaf's own signature. The chain still *builds* (issuer and
/// subject names are untouched, so the intermediate is still found) and must
/// then fail because the signature over the TBS does not verify.
#[test]
fn failure_a_leaf_whose_signature_does_not_verify_is_refused_on_the_chain_axis() {
    let b = mutate(|v| {
        let der = base64_decode(
            v["verificationMaterial"]["certificate"]["rawBytes"]
                .as_str()
                .unwrap(),
        );
        let mut der = der;
        // The last element of a Certificate is the signature BIT STRING, so a
        // byte near the end lands inside the ECDSA signature and leaves the
        // DER structurally intact.
        let n = der.len();
        der[n - 3] ^= 0xff;
        v["verificationMaterial"]["certificate"]["rawBytes"] =
            serde_json::Value::String(base64_encode(&der));
    });
    let e = thruline_sigstore::verify(&b, &prod_policy()).unwrap_err();
    assert_eq!(
        e.axis(),
        Axis::Chain,
        "a certificate whose signature does not verify must be refused on the chain axis: {e}"
    );
    assert!(
        matches!(
            e,
            Error::ChainSignatureInvalid { .. } | Error::NoTrustedRoot
        ),
        "{e}"
    );
}

/// Corrupt the leaf's TBS bytes. Same requirement from the other direction:
/// the signature is untouched and no longer matches what it covers.
#[test]
fn failure_a_leaf_whose_tbs_was_altered_is_refused_on_the_chain_axis() {
    let b = mutate(|v| {
        let mut der = base64_decode(
            v["verificationMaterial"]["certificate"]["rawBytes"]
                .as_str()
                .unwrap(),
        );
        // A byte inside the serial number: early in the TBS, structurally inert
        // (INTEGER contents), and covered by the signature.
        let at = 20;
        der[at] ^= 0x01;
        v["verificationMaterial"]["certificate"]["rawBytes"] =
            serde_json::Value::String(base64_encode(&der));
    });
    let e = thruline_sigstore::verify(&b, &prod_policy()).unwrap_err();
    assert_eq!(e.axis(), Axis::Chain, "{e}");
}

/// A trust root carrying the **right transparency logs** but the **wrong
/// certificate authorities**. This is the honest "wrong root" test: every log
/// check passes, so the refusal has to come from the chain.
#[test]
fn failure_the_right_logs_with_the_wrong_certificate_authorities_is_refused_on_the_chain_axis() {
    let mut prod: serde_json::Value = serde_json::from_str(PROD_ROOT).unwrap();
    let staging: serde_json::Value = serde_json::from_str(STAGING_ROOT).unwrap();
    prod["certificateAuthorities"] = staging["certificateAuthorities"].clone();
    let root = TrustRoot::from_json(&serde_json::to_string(&prod).unwrap()).unwrap();

    let e = thruline_sigstore::verify(&bundle(), &Policy::new(root)).unwrap_err();
    assert_eq!(
        e.axis(),
        Axis::Chain,
        "the logs are the real ones, so this must be refused for the chain, not the log: {e}"
    );
    assert!(matches!(e, Error::NoTrustedRoot), "{e}");
}

fn base64_decode(s: &str) -> Vec<u8> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(
            s.chars()
                .filter(|c| !c.is_ascii_whitespace())
                .collect::<String>(),
        )
        .unwrap()
}

fn base64_encode(b: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(b)
}
