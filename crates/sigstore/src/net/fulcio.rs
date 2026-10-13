//! Fulcio: exchanging an identity token for a ten-minute signing certificate.
//!
//! # The proof of possession is signed over the wrong thing in the docs
//!
//! `fulcio.proto` describes `proof_of_possession` as
//!
//! > *a signature over the `sub` claim from the OIDC identity token*
//!
//! and Fulcio's published Swagger says the same. **Both are wrong for the
//! issuer this crate defaults to.** `pkg/server/grpc_server.go:155` verifies
//! the proof against `principal.Name(ctx)`; for an issuer configured
//! `issuerType: email` — which `https://oauth2.sigstage.dev/auth` is, per
//! `fixtures/net/fulcio-staging-configuration.json` — `Name()` is the **email
//! address**. See [`crate::net::oidc::IdToken::proof_subject`] for the full
//! rule and where it comes from.
//!
//! The cost of getting it wrong is that every request fails with
//!
//! ```text
//! {"code":3,"message":"There was an error processing the identity token","details":[]}
//! ```
//!
//! recorded in `fixtures/net/fulcio-staging-signingcert-400.json` — which is
//! also what Fulcio returns for a malformed token, an unknown issuer, an
//! expired token, and a wrong audience. There is no error message that will
//! ever tell you the proof was over the wrong string.
//!
//! # A 400 tells you nothing about the rest of the body
//!
//! `grpc_server.go` authenticates before it looks at the public key, so the
//! identity-token 400 arrives whether or not the rest of the request is
//! well-formed — a probe with `{"totallyBogusField":123}` returns the *same*
//! body. That is why [`signing_cert_request`] has its own shape test rather
//! than being validated by "staging accepted it".
//!
//! # Two response arms, and both are real
//!
//! Fulcio answers with `signedCertificateEmbeddedSct` when the CT log stapled
//! an SCT into the certificate, and `signedCertificateDetachedSct` when the
//! SCT travels beside it. Public Fulcio emits the embedded form; the detached
//! form is what a self-hosted instance without SCT embedding returns. A client
//! that knows only one silently reports "Fulcio returned nothing" against half
//! the deployments in existence, so both are parsed and the detached SCT is
//! **not** discarded silently — it is returned, so a caller can see it exists.

use der::{Encode, EncodePem};
use serde::Deserialize;
use x509_cert::Certificate;

use crate::error::{Error, Result};
use crate::net::oidc::IdToken;
use crate::net::Transport;

/// The `algorithm` Fulcio's `PublicKey` message expects for a P-256 key.
///
/// Fulcio's enum spells this `ECDSA`; the *bundle* spells the same key
/// `PKIX_ECDSA_P256_SHA_256`. They are different vocabularies for the same
/// thing and neither is accepted where the other belongs.
pub const ALGORITHM_ECDSA: &str = "ECDSA";

/// `POST <fulcio>/api/v2/signingCert`.
pub const SIGNING_CERT_PATH: &str = "/api/v2/signingCert";

/// `GET <fulcio>/api/v2/configuration` — the issuer table.
pub const CONFIGURATION_PATH: &str = "/api/v2/configuration";

/// How many times to redraw a scalar before giving up.
///
/// A uniformly random 32-byte string is outside P-256's scalar range with
/// probability under 2⁻¹²⁸, so eight failures in a row is not bad luck — it is
/// a broken entropy source, and the honest answer is to say so rather than to
/// loop forever.
const KEY_DRAW_ATTEMPTS: usize = 8;

/// A fresh P-256 signing key that never touches disk.
///
/// # Why rejection sampling and not `SigningKey::random`
///
/// `ecdsa::SigningKey::random` wants an infallible `CryptoRng`, and the
/// adapter that turns the system RNG into one (`rand_core::UnwrapErr`)
/// **panics** when the OS refuses entropy. This crate's rule is no `unwrap` and
/// no `expect` in library code, and a panic inside a signing routine is the
/// worst possible shape for that failure: it aborts a process mid-flow with no
/// error a caller can report. Drawing bytes from `getrandom::fill` directly
/// keeps the failure typed.
///
/// The scratch buffer is zeroized, and `SigningKey` itself is `ZeroizeOnDrop`
/// (asserted in `crate::net`'s tests), so the scalar does not outlive the
/// signing call.
pub fn ephemeral_key() -> Result<p256::ecdsa::SigningKey> {
    use p256::elliptic_curve::zeroize::Zeroize as _;
    let mut raw = [0u8; 32];
    for _ in 0..KEY_DRAW_ATTEMPTS {
        getrandom::fill(&mut raw).map_err(|e| Error::Key {
            path: "<ephemeral P-256 key>".to_string(),
            reason: format!("the operating system's random number generator failed: {e}"),
        })?;
        let attempt = p256::ecdsa::SigningKey::from_bytes(&raw.into());
        raw.zeroize();
        if let Ok(key) = attempt {
            return Ok(key);
        }
    }
    Err(Error::Key {
        path: "<ephemeral P-256 key>".to_string(),
        reason: format!(
            "{KEY_DRAW_ATTEMPTS} consecutive draws fell outside the P-256 scalar range, which has \
             probability below 2^-1000 with a working entropy source"
        ),
    })
}

/// The PEM SubjectPublicKeyInfo Fulcio's `publicKey.content` carries.
///
/// PEM and not DER: `challenges.ParsePublicKey` tries PEM first and DER only as
/// a fallback, and the DER fallback is `x509.ParsePKIXPublicKey` over the
/// *raw bytes of the JSON string* — which, after protojson has treated the
/// field as a string rather than as `bytes`, is not what a base64-encoded DER
/// blob decodes to. PEM is the path that works.
pub fn public_key_pem(key: &p256::ecdsa::SigningKey) -> Result<String> {
    use spki::EncodePublicKey;
    key.verifying_key()
        .to_public_key_pem(der::pem::LineEnding::LF)
        .map_err(|e| Error::Key {
            path: "<ephemeral P-256 key>".to_string(),
            reason: format!("encoding the public key as PEM: {e}"),
        })
}

/// Sign the challenge string, base64 for the wire.
///
/// The signature is over the **message**, hashed with SHA-256 by the signer —
/// which is what `sigstore/sigstore`'s `LoadDefaultVerifier` does on the other
/// side for a P-256 key. ASN.1 DER, because Fulcio verifies with
/// `ecdsa.VerifyASN1`.
pub fn proof_of_possession(key: &p256::ecdsa::SigningKey, subject: &str) -> String {
    use p256::ecdsa::{signature::Signer, DerSignature};
    let sig: DerSignature = key.sign(subject.as_bytes());
    crate::bundle::b64_encode(sig.as_bytes())
}

/// The `CreateSigningCertificateRequest` body.
///
/// Built as a value rather than a format string so the two `oneof` arms cannot
/// both end up set — Fulcio answers that with
/// `oneof … .key is already set`, one of the few errors it reports precisely.
pub fn signing_cert_request(
    id_token: &str,
    public_key_pem: &str,
    proof_of_possession_b64: &str,
) -> serde_json::Value {
    serde_json::json!({
        "credentials": { "oidcIdentityToken": id_token },
        "publicKeyRequest": {
            "publicKey": { "algorithm": ALGORITHM_ECDSA, "content": public_key_pem },
            "proofOfPossession": proof_of_possession_b64,
        }
    })
}

#[derive(Debug, Deserialize)]
struct CertificateChainJson {
    #[serde(default)]
    certificates: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EmbeddedArm {
    #[serde(default)]
    chain: Option<CertificateChainJson>,
}

#[derive(Debug, Deserialize)]
struct DetachedArm {
    #[serde(default)]
    chain: Option<CertificateChainJson>,
    #[serde(rename = "signedCertificateTimestamp", default)]
    signed_certificate_timestamp: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SigningCertResponse {
    #[serde(rename = "signedCertificateEmbeddedSct", default)]
    embedded: Option<EmbeddedArm>,
    #[serde(rename = "signedCertificateDetachedSct", default)]
    detached: Option<DetachedArm>,
}

/// What Fulcio issued: the chain, leaf first, and the detached SCT if there was
/// one.
#[derive(Debug, Clone)]
pub struct IssuedCertificate {
    /// Leaf, then intermediates, then root — the order Fulcio documents.
    pub chain: Vec<Certificate>,
    /// Present only on the detached arm. Base64 of a CT `AddChainResponse`.
    ///
    /// Carried rather than dropped: a caller that needs to staple it can, and
    /// a caller that ignores it is at least ignoring something it can see.
    pub detached_sct: Option<String>,
}

/// Parse a `CreateSigningCertificateResponse`.
pub fn parse_signing_cert_response(body: &str) -> Result<IssuedCertificate> {
    let r: SigningCertResponse = serde_json::from_str(body).map_err(|e| {
        Error::CertificateParse(format!("Fulcio's signingCert response is not JSON: {e}"))
    })?;
    let (chain_json, detached_sct) = match (r.embedded, r.detached) {
        (Some(e), None) => (e.chain, None),
        (None, Some(d)) => (d.chain, d.signed_certificate_timestamp),
        // Both arms set is impossible from a real server (they are a protobuf
        // `oneof`), and neither set means the response carried no certificate
        // at all. Both are named rather than producing an empty chain that
        // later reads as "Fulcio issued nothing".
        (Some(_), Some(_)) => {
            return Err(Error::CertificateParse(
                "Fulcio's response set both signedCertificateEmbeddedSct and \
                 signedCertificateDetachedSct, which the protobuf oneof forbids"
                    .into(),
            ))
        }
        (None, None) => {
            return Err(Error::CertificateParse(
                "Fulcio's response carried neither signedCertificateEmbeddedSct nor \
                 signedCertificateDetachedSct"
                    .into(),
            ))
        }
    };
    let pems = chain_json.map(|c| c.certificates).unwrap_or_default();
    if pems.is_empty() {
        return Err(Error::CertificateParse(
            "Fulcio returned a certificate chain with no certificates in it".into(),
        ));
    }
    let mut chain = Vec::with_capacity(pems.len());
    for pem in &pems {
        // One `certificates[]` element may itself hold several concatenated
        // PEM blocks: Fulcio's own `/api/v2/trustBundle` returns the
        // intermediate and the root in one string, and the signingCert
        // response is produced by the same helper.
        let parsed = crate::x509::parse_pem_chain(pem.as_bytes())?;
        if parsed.is_empty() {
            return Err(Error::CertificateParse(
                "a certificates[] entry contained no PEM certificate block".into(),
            ));
        }
        chain.extend(parsed);
    }
    Ok(IssuedCertificate {
        chain,
        detached_sct,
    })
}

/// Ask Fulcio for a certificate over `key`, proving possession for `token`.
pub fn request_certificate(
    t: &dyn Transport,
    fulcio_url: &str,
    token: &IdToken,
    key: &p256::ecdsa::SigningKey,
) -> Result<Vec<Certificate>> {
    Ok(request_certificate_full(t, fulcio_url, token, key)?.chain)
}

/// As [`request_certificate`], keeping the detached SCT when there is one.
pub fn request_certificate_full(
    t: &dyn Transport,
    fulcio_url: &str,
    token: &IdToken,
    key: &p256::ecdsa::SigningKey,
) -> Result<IssuedCertificate> {
    let subject = token.proof_subject()?;
    let body = signing_cert_request(
        token.raw(),
        &public_key_pem(key)?,
        &proof_of_possession(key, subject),
    );
    let url = format!("{}{SIGNING_CERT_PATH}", fulcio_url.trim_end_matches('/'));
    let r = t.post_json(&url, &body, "requesting a signing certificate from Fulcio")?;
    if !r.is_success() {
        return Err(Error::FulcioRejected {
            status: r.status,
            body: r.body,
        });
    }
    let issued = parse_signing_cert_response(&r.body)?;
    // The one thing a caller of this function cannot check for itself, checked
    // here so that no caller can skip it. See `check_certifies`.
    let leaf = issued
        .chain
        .first()
        .ok_or_else(|| Error::CertificateParse("Fulcio returned an empty chain".into()))?;
    check_certifies(leaf, key)?;
    Ok(issued)
}

/// Does `cert` certify the key we asked Fulcio to certify?
///
/// # Why this runs before anything is written to Rekor
///
/// A Fulcio certificate binds *one* public key, and everything downstream
/// assumes it is the ephemeral key in hand: the DSSE signature is made with
/// that key, and a verifier checks that signature against the key inside the
/// leaf. If the two ever diverge, nothing fails at signing time — a bundle is
/// produced, it is well-formed, and it is **permanently unverifiable**. Worse,
/// by then the Rekor entry has already been written, and a Rekor entry cannot
/// be deleted. The mistake would surface days later, at verification, as
/// `SignatureInvalid` — which reads as tampering.
///
/// So this is not a paranoid double-check of a server that has no reason to
/// lie. It is the difference between a local error and a permanent public
/// artifact that says nothing true.
///
/// # Compared as a *key*, not as bytes
///
/// The comparison decodes both sides to a `VerifyingKey` rather than diffing
/// SubjectPublicKeyInfo DER. Fulcio round-trips the key through Go's
/// `ParsePKIXPublicKey` → `CreateCertificate`, and Go re-marshals the point
/// itself; today both sides emit the uncompressed form (measured: `p256`
/// 0.14.0 writes a 91-byte SPKI whose point starts `0x04`, which is what Go
/// writes too), so a byte comparison would pass — but it would pass by
/// coincidence, and a producer that emitted a compressed point would then be
/// refused for an encoding difference that changes no key.
///
/// A leaf whose key this build cannot decode at all is a **refusal**, not a
/// pass: an unreadable key is an unchecked binding.
pub fn check_certifies(cert: &Certificate, key: &p256::ecdsa::SigningKey) -> Result<()> {
    check_certifies_verifying_key(cert, key.verifying_key())
}

/// [`check_certifies`] against a public key alone.
///
/// Split out so the *accepting* half can be tested. There is no private key
/// anywhere for a real Fulcio certificate — issuance needs a human-approved
/// identity token — so a test that could only pass a `SigningKey` could
/// exercise the refusal and never the acceptance, and a check that refuses
/// everything passes a refusal test perfectly.
pub fn check_certifies_verifying_key(
    cert: &Certificate,
    key: &p256::ecdsa::VerifyingKey,
) -> Result<()> {
    use spki::DecodePublicKey as _;
    let spki = cert
        .tbs_certificate()
        .subject_public_key_info()
        .to_der()
        .map_err(|e| Error::CertificateParse(format!("re-encoding the leaf's SPKI: {e}")))?;
    let certified =
        p256::ecdsa::VerifyingKey::from_public_key_der(&spki).map_err(|e| Error::Key {
            path: "<ephemeral P-256 key>".to_string(),
            reason: format!(
                "the certificate Fulcio returned carries a public key this build cannot read as \
                 P-256 ({e}), so whether it certifies the signing key is unknown — and an \
                 unchecked binding is not a checked one"
            ),
        })?;
    if &certified != key {
        return Err(Error::Key {
            path: "<ephemeral P-256 key>".to_string(),
            reason: "the certificate does not carry the public key that was sent for signing. \
                     Every signature made with the key in hand would fail against this \
                     certificate, and the failure would arrive at verification time — after the \
                     Rekor entry it was written beside had become permanent"
                .to_string(),
        });
    }
    Ok(())
}

/// One issuer from `GET /api/v2/configuration`.
///
/// Read for evidence, never for control flow. `challengeClaim` looks like it
/// answers "what do I sign?" and does not: Fulcio's Chainguard issuer publishes
/// `"sub"` while its `Name()` returns the email. The authoritative rule lives
/// in [`crate::net::oidc::IdToken::proof_subject`]; this type exists so a test
/// can assert what the live server says about the issuer this crate defaults
/// to, and so that assertion breaks if the deployment ever changes.
#[derive(Debug, Clone, Deserialize)]
pub struct IssuerConfig {
    #[serde(rename = "issuerUrl", default)]
    pub issuer_url: String,
    #[serde(default)]
    pub audience: String,
    #[serde(rename = "challengeClaim", default)]
    pub challenge_claim: String,
    #[serde(rename = "issuerType", default)]
    pub issuer_type: String,
}

#[derive(Debug, Deserialize)]
struct ConfigurationResponse {
    #[serde(default)]
    issuers: Vec<IssuerConfig>,
}

pub fn parse_configuration(body: &str) -> Result<Vec<IssuerConfig>> {
    let c: ConfigurationResponse = serde_json::from_str(body).map_err(|e| Error::Transport {
        context: "reading Fulcio's issuer configuration".to_string(),
        reason: e.to_string(),
    })?;
    Ok(c.issuers)
}

pub fn fetch_configuration(t: &dyn Transport, fulcio_url: &str) -> Result<Vec<IssuerConfig>> {
    let url = format!("{}{CONFIGURATION_PATH}", fulcio_url.trim_end_matches('/'));
    let r = t.get(&url, "fetching Fulcio's issuer configuration")?;
    if !r.is_success() {
        return Err(Error::FulcioRejected {
            status: r.status,
            body: r.body,
        });
    }
    parse_configuration(&r.body)
}

/// DER for the bundle's `certificate.rawBytes`.
pub fn certificate_to_der(cert: &Certificate) -> Result<Vec<u8>> {
    cert.to_der()
        .map_err(|e| Error::CertificateParse(format!("re-encoding the certificate as DER: {e}")))
}

/// PEM for Rekor's `verifiers[]`, which wants a PEM certificate.
pub fn certificate_to_pem(cert: &Certificate) -> Result<String> {
    cert.to_pem(der::pem::LineEnding::LF)
        .map_err(|e| Error::CertificateParse(format!("re-encoding the certificate as PEM: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::oidc::testing::{recorded_staging_token, token_with_claims};
    use crate::net::testing::{fixture, Exchange, Script};

    fn staging_response() -> String {
        fixture("fulcio-staging-signingcert-embedded-sct.json")
    }

    // --- the proof of possession -----------------------------------------

    /// The trap, stated as a test: the signed string is the email, and the
    /// `sub` the proto documents would produce a different signature.
    #[test]
    fn the_proof_of_possession_is_over_the_email_not_the_sub() {
        let key = ephemeral_key().unwrap();
        let token = IdToken::parse(&recorded_staging_token()).unwrap();
        let over_email = proof_of_possession(&key, "signer@example.com");
        let over_sub = proof_of_possession(&key, token.subject());
        assert_eq!(
            proof_of_possession(&key, token.proof_subject().unwrap()),
            over_email,
            "the request must sign the email for an email issuer"
        );
        // ECDSA is randomized, so equal strings need not give equal signatures
        // — but the *verification* must differ, which is what matters.
        let _ = over_sub;
        verify_pop(&key, &over_email, "signer@example.com").unwrap();
        assert!(
            verify_pop(&key, &over_email, token.subject()).is_err(),
            "a proof over the email must not verify as a proof over the sub"
        );
    }

    /// Re-verify a proof the way Fulcio does: SHA-256 of the message, ASN.1
    /// DER signature, the certified public key.
    fn verify_pop(
        key: &p256::ecdsa::SigningKey,
        b64: &str,
        subject: &str,
    ) -> crate::error::Result<()> {
        use spki::EncodePublicKey;
        let sig = crate::bundle::b64_decode("proofOfPossession", b64)?;
        let spki = key
            .verifying_key()
            .to_public_key_der()
            .map_err(|e| Error::CertificateParse(e.to_string()))?;
        crate::x509::verify_ecdsa_spki(spki.as_bytes(), subject.as_bytes(), &sig)
    }

    /// The signature is over the message, hashed SHA-256 by the signer — the
    /// same convention `crate::x509::verify_ecdsa_spki` verifies under, which
    /// is what `sigstore/sigstore`'s default P-256 verifier does in Go.
    #[test]
    fn the_proof_verifies_under_the_key_it_names_and_no_other() {
        let a = ephemeral_key().unwrap();
        let b = ephemeral_key().unwrap();
        let proof = proof_of_possession(&a, "signer@example.com");
        verify_pop(&a, &proof, "signer@example.com").unwrap();
        assert!(verify_pop(&b, &proof, "signer@example.com").is_err());
    }

    /// The proof is base64 of ASN.1 DER — a SEQUENCE, so it starts 0x30 — and
    /// not the fixed-width r‖s form, which `ecdsa.VerifyASN1` rejects.
    #[test]
    fn the_proof_is_base64_of_asn1_der() {
        let key = ephemeral_key().unwrap();
        let raw =
            crate::bundle::b64_decode("p", &proof_of_possession(&key, "x@example.com")).unwrap();
        assert_eq!(raw[0], 0x30, "not a DER SEQUENCE");
        assert_ne!(raw.len(), 64, "this is the fixed-width form, not DER");
    }

    // --- the request body ------------------------------------------------

    /// The shape has to be asserted here, because staging cannot confirm it:
    /// Fulcio authenticates before parsing, so a request with a garbage body
    /// and a bad token returns exactly the same 400 as a perfect request with
    /// a bad token.
    #[test]
    fn the_request_body_is_the_shape_fulcio_documents() {
        let key = ephemeral_key().unwrap();
        let pem = public_key_pem(&key).unwrap();
        let pop = proof_of_possession(&key, "signer@example.com");
        let body = signing_cert_request("the.id.token", &pem, &pop);

        assert_eq!(body["credentials"]["oidcIdentityToken"], "the.id.token");
        assert_eq!(body["publicKeyRequest"]["publicKey"]["algorithm"], "ECDSA");
        assert_eq!(body["publicKeyRequest"]["proofOfPossession"], pop);
        let content = body["publicKeyRequest"]["publicKey"]["content"]
            .as_str()
            .unwrap();
        assert!(
            content.starts_with("-----BEGIN PUBLIC KEY-----"),
            "{content}"
        );
        assert!(content.ends_with("-----END PUBLIC KEY-----\n"), "{content}");
        // The `oneof` must have exactly one arm set.
        assert!(body["certificateSigningRequest"].is_null());
        assert!(body["publicKeyRequest"].is_object());
    }

    /// The public key that goes on the wire is the key that will sign the
    /// DSSE envelope — a PEM parsed back must be the same point.
    #[test]
    fn the_public_key_pem_round_trips_to_the_signing_key() {
        use spki::{DecodePublicKey, EncodePublicKey};
        let key = ephemeral_key().unwrap();
        let pem = public_key_pem(&key).unwrap();
        let back = p256::ecdsa::VerifyingKey::from_public_key_pem(&pem).unwrap();
        assert_eq!(
            back.to_public_key_der().unwrap().as_bytes(),
            key.verifying_key().to_public_key_der().unwrap().as_bytes()
        );
    }

    /// Two calls give two keys. An ephemeral key that is not ephemeral is a
    /// long-lived signing key with a ten-minute certificate over it.
    #[test]
    fn every_ephemeral_key_is_a_different_key() {
        use spki::EncodePublicKey;
        let mut seen = std::collections::HashSet::new();
        for _ in 0..16 {
            let k = ephemeral_key().unwrap();
            assert!(seen.insert(
                k.verifying_key()
                    .to_public_key_der()
                    .unwrap()
                    .as_bytes()
                    .to_vec()
            ));
        }
    }

    // --- the response ----------------------------------------------------

    /// The embedded-SCT arm, over the real staging chain.
    #[test]
    fn the_embedded_sct_arm_parses_a_real_staging_chain() {
        let issued = parse_signing_cert_response(&staging_response()).unwrap();
        assert_eq!(issued.chain.len(), 3, "leaf, intermediate, root");
        assert!(issued.detached_sct.is_none());
        let leaf = &issued.chain[0];
        assert_eq!(
            leaf.tbs_certificate().issuer().to_string(),
            "CN=sigstore-intermediate,O=sigstore.dev"
        );
        // Leaf first, then its issuer, then the root — the order Fulcio
        // documents, and the order a verifier needs.
        assert_eq!(
            issued.chain[1].tbs_certificate().subject().to_string(),
            leaf.tbs_certificate().issuer().to_string()
        );
        assert_eq!(
            issued.chain[2].tbs_certificate().subject().to_string(),
            issued.chain[1].tbs_certificate().issuer().to_string()
        );
    }

    /// The detached arm parses too, and the SCT is handed back rather than
    /// dropped.
    #[test]
    fn the_detached_sct_arm_parses_and_keeps_the_sct() {
        let issued =
            parse_signing_cert_response(&fixture("fulcio-staging-signingcert-detached-sct.json"))
                .unwrap();
        assert_eq!(issued.chain.len(), 3);
        assert_eq!(
            issued.detached_sct.as_deref(),
            Some("eyJzY3RfdmVyc2lvbiI6MH0=")
        );
    }

    /// A response with neither arm is refused by name. Returning an empty
    /// chain here is how "Fulcio issued nothing" becomes "the leaf is
    /// missing", three functions away.
    #[test]
    fn a_response_with_no_certificate_is_refused_by_name() {
        for body in [
            "{}",
            r#"{"signedCertificateEmbeddedSct":{}}"#,
            r#"{"signedCertificateEmbeddedSct":{"chain":{"certificates":[]}}}"#,
        ] {
            let e = parse_signing_cert_response(body).unwrap_err();
            assert!(matches!(e, Error::CertificateParse(_)), "{body}: {e}");
        }
    }

    /// Both `oneof` arms set is a malformed response, not a chain to pick
    /// from.
    #[test]
    fn a_response_setting_both_oneof_arms_is_refused() {
        let body = r#"{"signedCertificateEmbeddedSct":{"chain":{"certificates":["x"]}},
                       "signedCertificateDetachedSct":{"chain":{"certificates":["y"]}}}"#;
        let e = parse_signing_cert_response(body).unwrap_err();
        assert!(e.to_string().contains("oneof"), "{e}");
    }

    /// Garbage inside the PEM is a parse failure, not an empty chain.
    #[test]
    fn a_chain_entry_that_is_not_a_certificate_is_refused() {
        let body = r#"{"signedCertificateEmbeddedSct":{"chain":{"certificates":[
            "-----BEGIN CERTIFICATE-----\nbm90IGEgY2VydGlmaWNhdGU=\n-----END CERTIFICATE-----\n"]}}}"#;
        assert!(parse_signing_cert_response(body).is_err());
    }

    /// The certificate this module hands on is byte-identical to the one
    /// Fulcio sent. `crate::x509::verify_signed_by` re-encodes the
    /// TBSCertificate to check the issuer's signature, so a parser that
    /// normalised anything would produce a chain that fails to verify.
    #[test]
    fn the_certificate_survives_der_and_pem_round_trips_byte_for_byte() {
        let issued = parse_signing_cert_response(&staging_response()).unwrap();
        for cert in &issued.chain {
            let der = certificate_to_der(cert).unwrap();
            let back = crate::x509::parse_der(&der).unwrap();
            assert_eq!(certificate_to_der(&back).unwrap(), der);

            let pem = certificate_to_pem(cert).unwrap();
            let from_pem = crate::x509::parse_pem_chain(pem.as_bytes()).unwrap();
            assert_eq!(from_pem.len(), 1);
            assert_eq!(certificate_to_der(&from_pem[0]).unwrap(), der);
        }
    }

    /// The real chain the fixture carries actually chains — including the
    /// intermediate's signature by the root, which is the step `sigstore`
    /// 0.14.0 skips.
    #[test]
    fn the_real_staging_chain_verifies_up_to_its_own_root() {
        let issued = parse_signing_cert_response(&staging_response()).unwrap();
        let (leaf, intermediate, root) = (&issued.chain[0], &issued.chain[1], &issued.chain[2]);
        crate::x509::verify_signed_by(leaf, intermediate).unwrap();
        crate::x509::verify_signed_by(intermediate, root).unwrap();
        // …and the leaf is not signed by the root directly.
        assert!(crate::x509::verify_signed_by(leaf, root).is_err());
    }

    // --- the request round trip -------------------------------------------

    #[test]
    fn a_400_from_fulcio_is_reported_with_its_body_intact() {
        let key = ephemeral_key().unwrap();
        let token = IdToken::parse(&recorded_staging_token()).unwrap();
        let script = Script::new(vec![Exchange::status(
            "/api/v2/signingCert",
            400,
            fixture("fulcio-staging-signingcert-400.json"),
        )]);
        let e =
            request_certificate(&script, "https://fulcio.sigstage.dev", &token, &key).unwrap_err();
        match e {
            Error::FulcioRejected { status, ref body } => {
                assert_eq!(status, 400);
                assert!(body.contains("There was an error processing the identity token"));
            }
            other => panic!("wrong variant: {other}"),
        }
        assert_eq!(e.axis(), crate::Axis::Bundle);
    }

    /// A token that names nobody is refused **before** the request goes out.
    /// The remote error for this case is the same opaque 400 as everything
    /// else, so failing locally is the only way the cause is ever visible.
    #[test]
    fn a_token_with_no_usable_claim_never_reaches_the_network() {
        let key = ephemeral_key().unwrap();
        let token = IdToken::parse(&token_with_claims(serde_json::json!({"iss": "x"}))).unwrap();
        let script = Script::new(vec![]);
        assert!(request_certificate(&script, "https://fulcio.sigstage.dev", &token, &key).is_err());
        assert!(script.sent.borrow().is_empty());
    }

    #[test]
    fn the_request_goes_to_the_v2_signing_cert_path() {
        let key = ephemeral_key().unwrap();
        let token = IdToken::parse(&recorded_staging_token()).unwrap();
        // A trailing slash on the configured URL must not double up.
        let script = Script::new(vec![Exchange::ok(
            "/api/v2/signingCert",
            staging_response(),
        )]);
        // The recorded response certifies somebody else's key — see
        // `fixtures/net/PROVENANCE.md` — so the request completes, the
        // response parses, and the binding check refuses it. The URL is what
        // this test is about, and it is asserted either way.
        let _ = request_certificate(&script, "https://fulcio.sigstage.dev/", &token, &key);
        assert_eq!(
            script.urls()[0],
            "https://fulcio.sigstage.dev/api/v2/signingCert"
        );
    }

    /// **The rule this module is built around, checked on the request that
    /// actually goes out.**
    ///
    /// Every other test of the proof-of-possession rule calls
    /// [`proof_of_possession`] directly, so none of them says anything about
    /// what [`request_certificate_full`] passes it. Replacing
    /// `token.proof_subject()?` with `token.subject()` in that function — the
    /// exact mistake `fulcio.proto` and Fulcio's Swagger both invite — changed
    /// **no test result** before this existed. The failure it would cause is
    /// the opaque 400 the module docs describe, so nothing downstream would
    /// have said which string was wrong either.
    #[test]
    fn the_request_that_goes_out_proves_possession_over_the_proof_subject() {
        let key = ephemeral_key().unwrap();
        let token = IdToken::parse(&recorded_staging_token()).unwrap();
        let script = Script::new(vec![Exchange::ok(
            "/api/v2/signingCert",
            staging_response(),
        )]);
        // The recorded response certifies somebody else's key, so this ends in
        // a refusal — after the request has been built and sent, which is what
        // is under test.
        let _ = request_certificate(&script, "https://fulcio.sigstage.dev", &token, &key);

        let sent = script.sent.borrow();
        let body = sent[0].json.as_ref().expect("the request carried no JSON");
        let pop = body["publicKeyRequest"]["proofOfPossession"]
            .as_str()
            .expect("no proofOfPossession in the request");

        // The email, which is what Fulcio's email principal returns from
        // `Name()` and therefore what it verifies the proof against.
        verify_pop(&key, pop, "signer@example.com").expect("the proof is not over the email");
        // …and demonstrably not the `sub` the protobuf documents.
        assert_ne!(token.subject(), "signer@example.com");
        assert!(
            verify_pop(&key, pop, token.subject()).is_err(),
            "the proof verifies over the sub, so this request signs the documented-but-wrong string"
        );

        // The key on the wire is the key the proof was made with, so a verifier
        // on Fulcio's side has something to check the proof against.
        let content = body["publicKeyRequest"]["publicKey"]["content"]
            .as_str()
            .unwrap();
        assert_eq!(content, public_key_pem(&key).unwrap());
        assert_eq!(
            body["credentials"]["oidcIdentityToken"].as_str().unwrap(),
            token.raw()
        );
    }

    // --- the certificate binds the key we sent ----------------------------

    /// A certificate over a key we do not hold is refused **at issuance**, not
    /// discovered at verification time after a permanent Rekor entry has been
    /// written beside it.
    #[test]
    fn a_certificate_over_a_different_key_is_refused_before_anything_is_logged() {
        let key = ephemeral_key().unwrap();
        let token = IdToken::parse(&recorded_staging_token()).unwrap();
        let script = Script::new(vec![Exchange::ok(
            "/api/v2/signingCert",
            staging_response(),
        )]);
        let e =
            request_certificate(&script, "https://fulcio.sigstage.dev", &token, &key).unwrap_err();
        match e {
            Error::Key { ref reason, .. } => {
                assert!(reason.contains("does not carry the public key"), "{reason}")
            }
            other => panic!("wrong variant: {other}"),
        }
        // Exactly one request went out, and it was the Fulcio one. Nothing
        // downstream — least of all Rekor — was reached.
        assert_eq!(script.urls().len(), 1);
    }

    /// The positive half, on the same real certificate: the check **accepts**
    /// the key that certificate actually carries. Without this the refusal
    /// test above would pass for a `check_certifies` that refuses everything.
    #[test]
    fn the_check_accepts_the_key_the_certificate_really_carries() {
        use spki::DecodePublicKey as _;
        let issued = parse_signing_cert_response(&staging_response()).unwrap();
        let leaf = &issued.chain[0];
        let spki = leaf
            .tbs_certificate()
            .subject_public_key_info()
            .to_der()
            .unwrap();
        // No private key for this certificate exists anywhere — Fulcio issued
        // it to somebody else on 2026-09-04 — so the accepting case is driven
        // with the public half pulled back out of the leaf. It is the same
        // comparison the signing path makes.
        let certified = p256::ecdsa::VerifyingKey::from_public_key_der(&spki).unwrap();
        check_certifies_verifying_key(leaf, &certified).unwrap();

        // And a different key over the same certificate is refused, so the
        // acceptance above is not simply "this function returns Ok".
        let other = ephemeral_key().unwrap();
        assert!(check_certifies(leaf, &other).is_err());
    }

    /// A leaf whose key is not P-256 — the Fulcio **intermediate** is a real
    /// P-384 certificate — is refused for being unreadable rather than passed
    /// for being unchecked.
    #[test]
    fn a_leaf_this_build_cannot_read_is_refused_rather_than_skipped() {
        let issued = parse_signing_cert_response(&staging_response()).unwrap();
        let key = ephemeral_key().unwrap();
        let e = check_certifies(&issued.chain[2], &key).unwrap_err();
        match e {
            Error::Key { ref reason, .. } => {
                assert!(reason.contains("cannot read as P-256"), "{reason}")
            }
            other => panic!("wrong variant: {other}"),
        }
    }

    /// The comparison is on the *key*, not on its encoding. Go re-marshals the
    /// point when it builds the certificate, so a byte-for-byte SPKI diff
    /// would be a check that happens to pass rather than one that means
    /// anything.
    #[test]
    fn the_binding_check_is_by_key_and_not_by_point_encoding() {
        use der::asn1::BitString;
        use p256::elliptic_curve::sec1::ToSec1Point as _;
        use spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned};

        let key = ephemeral_key().unwrap();
        // The same key, written with a *compressed* point: 33 bytes beginning
        // 0x02 or 0x03 rather than 65 beginning 0x04.
        let public = p256::PublicKey::from(*key.verifying_key());
        let compressed = public.to_sec1_point(true);
        assert_eq!(compressed.as_bytes().len(), 33);
        let spki = SubjectPublicKeyInfoOwned {
            algorithm: AlgorithmIdentifierOwned {
                oid: "1.2.840.10045.2.1".parse().unwrap(),
                parameters: Some(der::Any::from(
                    "1.2.840.10045.3.1.7"
                        .parse::<const_oid::ObjectIdentifier>()
                        .unwrap(),
                )),
            },
            subject_public_key: BitString::from_bytes(compressed.as_bytes()).unwrap(),
        };
        let compressed_der = der::Encode::to_der(&spki).unwrap();
        use spki::DecodePublicKey as _;
        let back = p256::ecdsa::VerifyingKey::from_public_key_der(&compressed_der).unwrap();
        assert_eq!(&back, key.verifying_key());

        // And the encodings really do differ, so the point above is not vacuous.
        use spki::EncodePublicKey as _;
        assert_ne!(
            key.verifying_key().to_public_key_der().unwrap().as_bytes(),
            compressed_der.as_slice()
        );
    }

    // --- the issuer table -------------------------------------------------

    /// The live claim behind the whole proof-of-possession rule: staging's Dex
    /// is an **email** issuer. If Sigstore ever reconfigures it, this test is
    /// where that shows up — rather than as an unexplained 400 in the field.
    #[test]
    fn the_staging_issuer_is_configured_as_an_email_issuer() {
        let issuers = parse_configuration(&fixture("fulcio-staging-configuration.json")).unwrap();
        let dex = issuers
            .iter()
            .find(|i| i.issuer_url == "https://oauth2.sigstage.dev/auth")
            .expect("staging Dex is not in the recorded issuer table");
        assert_eq!(dex.issuer_type, "email");
        assert_eq!(dex.challenge_claim, "email");
        assert_eq!(dex.audience, "sigstore");

        // …and the counter-example that makes `challengeClaim` unusable as the
        // rule: Chainguard publishes "sub" while Fulcio's own principal for it
        // returns `SubjectFromToken`, which prefers the email claim.
        if let Some(cg) = issuers
            .iter()
            .find(|i| i.issuer_type == "chainguard-identity")
        {
            assert_eq!(cg.challenge_claim, "sub");
        }
    }
}

// --- tests that genuinely need the network ---------------------------------
//
// `#[ignore]`; run with `cargo test --features net,oidc -- --ignored`.
//
// Issuance itself cannot be tested here: it needs a human-approved identity
// token. What can be tested is everything that decides *how* the request is
// built — the live issuer table that says the staging issuer is an email
// issuer, and the live refusal shape a wrong token produces.
#[cfg(test)]
mod live {
    use super::*;
    use crate::net::Http;

    const STAGING: &str = "https://fulcio.sigstage.dev";

    /// The claim the proof-of-possession rule rests on, asked of the live
    /// server rather than of a file recorded once.
    #[test]
    #[ignore = "requires the network"]
    fn the_live_staging_issuer_table_still_says_email() {
        let http = Http::new();
        let issuers = fetch_configuration(&http, STAGING).unwrap();
        let dex = issuers
            .iter()
            .find(|i| i.issuer_url == "https://oauth2.sigstage.dev/auth")
            .expect("staging Dex is no longer a configured issuer");
        assert_eq!(dex.issuer_type, "email");
        assert_eq!(dex.audience, "sigstore");
    }

    /// A well-formed request with a token Fulcio cannot verify. Proves the
    /// endpoint, the path and the body shape reach Fulcio's authenticator —
    /// and nothing beyond it, because `grpc_server.go` authenticates before it
    /// parses the rest.
    #[test]
    #[ignore = "requires the network"]
    fn a_well_formed_request_with_an_unusable_token_gets_the_documented_400() {
        let http = Http::new();
        let key = ephemeral_key().unwrap();
        let token = IdToken::parse(&crate::net::oidc::testing::recorded_staging_token()).unwrap();
        let e = request_certificate(&http, STAGING, &token, &key).unwrap_err();
        match e {
            Error::FulcioRejected { status, body } => {
                assert_eq!(status, 400);
                assert!(
                    body.contains("There was an error processing the identity token"),
                    "{body}"
                );
            }
            other => panic!("expected a Fulcio refusal, got {other}"),
        }
    }
}
