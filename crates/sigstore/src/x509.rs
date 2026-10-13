//! Certificate parsing and chain verification.
//!
//! # Scope, stated rather than implied
//!
//! This is **not** general RFC 5280 path validation. Sigstore chains are short
//! and their shape is fixed: leaf → Fulcio intermediate → root, against a root
//! set already pinned by the trust root. What is checked is exactly:
//!
//! - each certificate's signature by the one above it, **including the
//!   intermediate's by the root**;
//! - validity windows, at a *supplied* instant rather than at `now`;
//! - `basicConstraints` cA on every issuer;
//! - extended key usage `id-kp-codeSigning` on the leaf.
//!
//! Name constraints, policy mapping, CRLs and OCSP are **out of scope and are
//! not silently treated as passing** — nothing here claims to have checked
//! them, and [`verify_chain`] returns the path it built so a caller can see
//! what it actually validated.
//!
//! ## Why the intermediate's signature is checked
//!
//! `sigstore` 0.14.0 builds its pool with
//! `CertificatePool::from_certificates(trust_repo.fulcio_certs()?, [])` —
//! an **empty intermediates list** — while `fulcio_certs()` flattens every CA
//! certificate, roots *and* intermediates, into the anchor set. The Fulcio
//! intermediate is therefore trusted as a root and its signature by the actual
//! root is never verified. A chain that stops at an anchor it was handed is a
//! chain to nowhere.
//!
//! # Both curves, because the chain uses both
//!
//! Live Fulcio leaves are ECDSA P-256/SHA-256; the intermediate and root are
//! **P-384/SHA-384**. A P-256-only verifier checks the leaf and then cannot
//! check what signed it — which is why `p384` is a hard dependency here rather
//! than an optional extra.

use const_oid::ObjectIdentifier;
use der::{Decode, Encode};
use spki::DecodePublicKey;
use x509_cert::ext::pkix::{BasicConstraints, ExtendedKeyUsage};
use x509_cert::Certificate;

use crate::error::{Error, Result};

/// `id-kp-codeSigning`.
pub const EKU_CODE_SIGNING: &str = "1.3.6.1.5.5.7.3.3";

const OID_EC_PUBLIC_KEY: &str = "1.2.840.10045.2.1";
const OID_P256: &str = "1.2.840.10045.3.1.7";
const OID_P384: &str = "1.3.132.0.34";
const OID_ECDSA_SHA256: &str = "1.2.840.10045.4.3.2";
const OID_ECDSA_SHA384: &str = "1.2.840.10045.4.3.3";

/// A Sigstore chain is leaf → intermediate → root. Anything longer is refused
/// rather than walked, so a crafted bundle cannot make a verifier chase a long
/// path.
pub const MAX_CHAIN_DEPTH: usize = 8;

pub fn parse_der(der: &[u8]) -> Result<Certificate> {
    Certificate::from_der(der).map_err(|e| Error::CertificateParse(e.to_string()))
}

pub fn parse_pem_chain(pem: &[u8]) -> Result<Vec<Certificate>> {
    Certificate::load_pem_chain(pem).map_err(|e| Error::CertificateParse(e.to_string()))
}

/// Which curve a SubjectPublicKeyInfo names, and the digest that goes with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Curve {
    P256,
    P384,
}

fn curve_of_spki(spki_der: &[u8]) -> Result<(Curve, Vec<u8>)> {
    let spki = spki::SubjectPublicKeyInfoRef::from_der(spki_der)
        .map_err(|e| Error::CertificateParse(format!("SubjectPublicKeyInfo: {e}")))?;
    if spki.algorithm.oid.to_string() != OID_EC_PUBLIC_KEY {
        return Err(Error::CertificateParse(format!(
            "public key algorithm {} is not id-ecPublicKey; this build verifies ECDSA only",
            spki.algorithm.oid
        )));
    }
    let params = spki
        .algorithm
        .parameters
        .ok_or_else(|| Error::CertificateParse("EC key names no curve".into()))?;
    let named: ObjectIdentifier = params
        .decode_as()
        .map_err(|e| Error::CertificateParse(format!("EC curve parameters: {e}")))?;
    let curve = match named.to_string().as_str() {
        OID_P256 => Curve::P256,
        OID_P384 => Curve::P384,
        other => {
            return Err(Error::CertificateParse(format!(
                "unsupported curve {other}; Sigstore uses P-256 leaves and P-384 CAs"
            )))
        }
    };
    Ok((curve, spki_der.to_vec()))
}

/// Verify an ECDSA signature over `msg` by the key in `spki_der`.
///
/// The digest follows the curve — P-256/SHA-256, P-384/SHA-384 — which is what
/// Sigstore's algorithm registry binds. Signatures are ASN.1 DER; the fixed-width
/// `r ‖ s` form is accepted as a fallback because some producers emit it.
pub fn verify_ecdsa_spki(spki_der: &[u8], msg: &[u8], sig: &[u8]) -> Result<()> {
    let (curve, _) = curve_of_spki(spki_der)?;
    match curve {
        Curve::P256 => {
            use p256::ecdsa::{signature::Verifier, DerSignature, Signature, VerifyingKey};
            let vk = VerifyingKey::from_public_key_der(spki_der)
                .map_err(|e| Error::CertificateParse(format!("P-256 key: {e}")))?;
            if let Ok(s) = DerSignature::try_from(sig) {
                if vk.verify(msg, &s).is_ok() {
                    return Ok(());
                }
            }
            if let Ok(s) = Signature::try_from(sig) {
                if vk.verify(msg, &s).is_ok() {
                    return Ok(());
                }
            }
            Err(Error::SignatureInvalid)
        }
        Curve::P384 => {
            use p384::ecdsa::{signature::Verifier, DerSignature, Signature, VerifyingKey};
            let vk = VerifyingKey::from_public_key_der(spki_der)
                .map_err(|e| Error::CertificateParse(format!("P-384 key: {e}")))?;
            if let Ok(s) = DerSignature::try_from(sig) {
                if vk.verify(msg, &s).is_ok() {
                    return Ok(());
                }
            }
            if let Ok(s) = Signature::try_from(sig) {
                if vk.verify(msg, &s).is_ok() {
                    return Ok(());
                }
            }
            Err(Error::SignatureInvalid)
        }
    }
}

/// Verify an ECDSA signature over a **pre-hashed** message.
///
/// A `messageSignature` bundle carries the digest, not the message, so the
/// verifier must not hash again. Hashing twice produces a signature that never
/// verifies — which looks exactly like tampering.
pub fn verify_ecdsa_prehash(spki_der: &[u8], digest: &[u8], sig: &[u8]) -> Result<()> {
    let (curve, _) = curve_of_spki(spki_der)?;
    match curve {
        Curve::P256 => {
            use p256::ecdsa::{DerSignature, Signature, VerifyingKey};
            use signature::hazmat::PrehashVerifier;
            let vk = VerifyingKey::from_public_key_der(spki_der)
                .map_err(|e| Error::CertificateParse(format!("P-256 key: {e}")))?;
            if let Ok(s) = DerSignature::try_from(sig) {
                if vk.verify_prehash(digest, &s).is_ok() {
                    return Ok(());
                }
            }
            if let Ok(s) = Signature::try_from(sig) {
                if vk.verify_prehash(digest, &s).is_ok() {
                    return Ok(());
                }
            }
            Err(Error::SignatureInvalid)
        }
        Curve::P384 => {
            use p384::ecdsa::{DerSignature, Signature, VerifyingKey};
            use signature::hazmat::PrehashVerifier;
            let vk = VerifyingKey::from_public_key_der(spki_der)
                .map_err(|e| Error::CertificateParse(format!("P-384 key: {e}")))?;
            if let Ok(s) = DerSignature::try_from(sig) {
                if vk.verify_prehash(digest, &s).is_ok() {
                    return Ok(());
                }
            }
            if let Ok(s) = Signature::try_from(sig) {
                if vk.verify_prehash(digest, &s).is_ok() {
                    return Ok(());
                }
            }
            Err(Error::SignatureInvalid)
        }
    }
}

fn spki_der_of(cert: &Certificate) -> Result<Vec<u8>> {
    cert.tbs_certificate()
        .subject_public_key_info()
        .to_der()
        .map_err(|e| Error::CertificateParse(format!("re-encoding SubjectPublicKeyInfo: {e}")))
}

fn name_of(cert: &Certificate) -> String {
    cert.tbs_certificate().subject().to_string()
}

/// Is `child` signed by `parent`?
///
/// The signed bytes are the **re-encoded TBSCertificate**, which is why the
/// certificate must round-trip through DER identically — a parser that
/// normalizes anything would produce a verifier that rejects valid signatures.
pub fn verify_signed_by(child: &Certificate, parent: &Certificate) -> Result<()> {
    let tbs = child
        .tbs_certificate()
        .to_der()
        .map_err(|e| Error::CertificateParse(format!("re-encoding TBSCertificate: {e}")))?;
    let sig = child
        .signature()
        .as_bytes()
        .ok_or_else(|| Error::CertificateParse("signature is not whole bytes".into()))?;

    // The certificate's own signatureAlgorithm decides the digest; the curve
    // alone would guess it.
    let alg = child.signature_algorithm().oid.to_string();
    let spki = spki_der_of(parent)?;
    let ok = match alg.as_str() {
        OID_ECDSA_SHA256 | OID_ECDSA_SHA384 => verify_ecdsa_spki(&spki, &tbs, sig).is_ok(),
        other => {
            return Err(Error::CertificateParse(format!(
                "unsupported certificate signature algorithm {other}"
            )))
        }
    };
    if ok {
        Ok(())
    } else {
        Err(Error::ChainSignatureInvalid {
            subject: name_of(child),
            issuer: name_of(parent),
        })
    }
}

/// Was the certificate inside its validity window at `at`?
///
/// **Never checked against `now`.** A Fulcio certificate lives ten minutes and
/// is always expired by the time anyone verifies it.
pub fn check_validity(cert: &Certificate, at: time::OffsetDateTime) -> Result<()> {
    let v = cert.tbs_certificate().validity();
    let nb = v.not_before.to_unix_duration().as_secs() as i64;
    let na = v.not_after.to_unix_duration().as_secs() as i64;
    let t = at.unix_timestamp();
    if t < nb || t > na {
        return Err(Error::CertificateExpired {
            at: at.to_string(),
            not_before: fmt_unix(nb),
            not_after: fmt_unix(na),
        });
    }
    Ok(())
}

fn fmt_unix(s: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(s)
        .map(|t| t.to_string())
        .unwrap_or_else(|_| format!("unix:{s}"))
}

pub fn check_code_signing_eku(cert: &Certificate) -> Result<()> {
    let want: ObjectIdentifier = EKU_CODE_SIGNING.parse().expect("constant OID");
    match cert.tbs_certificate().get_extension::<ExtendedKeyUsage>() {
        Ok(Some((_, eku))) if eku.0.contains(&want) => Ok(()),
        _ => Err(Error::NotCodeSigning),
    }
}

fn check_is_ca(cert: &Certificate) -> Result<()> {
    match cert.tbs_certificate().get_extension::<BasicConstraints>() {
        Ok(Some((_, bc))) if bc.ca => Ok(()),
        _ => Err(Error::NotACa {
            which: name_of(cert),
        }),
    }
}

/// Build and verify a chain from `leaf` to one of `roots`.
///
/// Returns the path actually validated, leaf first. Returning the path rather
/// than `()` is deliberate: a caller can assert *what* was trusted, and a test
/// can tell "chained to the root" from "stopped at an intermediate it was
/// handed" — the distinction `sigstore` 0.14.0 loses.
pub fn verify_chain(
    leaf: &Certificate,
    intermediates: &[Certificate],
    roots: &[Certificate],
    at: time::OffsetDateTime,
) -> Result<Vec<Certificate>> {
    check_validity(leaf, at)?;
    check_code_signing_eku(leaf)?;

    let mut path = vec![leaf.clone()];
    let mut current = leaf.clone();

    loop {
        if path.len() > MAX_CHAIN_DEPTH {
            return Err(Error::ChainTooDeep {
                depth: path.len(),
                max: MAX_CHAIN_DEPTH,
            });
        }

        // A root that signed the current certificate ends the walk.
        if let Some(root) = roots.iter().find(|r| {
            r.tbs_certificate().subject() == current.tbs_certificate().issuer()
                && verify_signed_by(&current, r).is_ok()
        }) {
            check_validity(root, at)?;
            check_is_ca(root)?;
            path.push(root.clone());
            return Ok(path);
        }

        // Otherwise step up through an intermediate — and verify that step,
        // rather than accepting the intermediate as an anchor.
        let Some(next) = intermediates.iter().find(|i| {
            i.tbs_certificate().subject() == current.tbs_certificate().issuer()
                && !path
                    .iter()
                    .any(|p| p.tbs_certificate().subject() == i.tbs_certificate().subject())
                && verify_signed_by(&current, i).is_ok()
        }) else {
            return Err(Error::NoTrustedRoot);
        };
        check_validity(next, at)?;
        check_is_ca(next)?;
        path.push(next.clone());
        current = next.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_self_signed_key_verifies_its_own_signature_and_not_anothers() {
        use p256::ecdsa::{signature::Signer, DerSignature, SigningKey};
        use spki::EncodePublicKey;
        let mut seed = [7u8; 32];
        seed[0] = 1;
        let sk = SigningKey::from_bytes(&seed.into()).unwrap();
        let spki = sk.verifying_key().to_public_key_der().unwrap();
        let sig: DerSignature = sk.sign(b"hello");
        verify_ecdsa_spki(spki.as_bytes(), b"hello", sig.as_bytes()).unwrap();
        assert!(verify_ecdsa_spki(spki.as_bytes(), b"goodbye", sig.as_bytes()).is_err());
    }

    #[test]
    fn p384_is_supported_because_the_fulcio_chain_needs_it() {
        use p384::ecdsa::{signature::Signer, DerSignature, SigningKey};
        use spki::EncodePublicKey;
        let sk = SigningKey::from_bytes(&[3u8; 48].into()).unwrap();
        let spki = sk.verifying_key().to_public_key_der().unwrap();
        let sig: DerSignature = sk.sign(b"chain");
        verify_ecdsa_spki(spki.as_bytes(), b"chain", sig.as_bytes()).unwrap();
        assert!(verify_ecdsa_spki(spki.as_bytes(), b"chair", sig.as_bytes()).is_err());
    }

    /// The EKU check is a real gate, not decoration: a Fulcio leaf carries
    /// `Code Signing` and a CA root carries no EKU at all, so a verifier that
    /// skipped this would accept a CA certificate as a signing certificate.
    ///
    /// Both fixtures are real certificates from the production Sigstore chain.
    #[test]
    fn code_signing_eku_is_required_and_a_ca_root_does_not_have_it() {
        let leaf = parse_pem_chain(include_bytes!("../fixtures/identity-email-leaf.pem")).unwrap();
        check_code_signing_eku(&leaf[0]).expect("a real Fulcio leaf is a code-signing certificate");

        let root = parse_pem_chain(include_bytes!("../fixtures/identity-fulcio-root.pem")).unwrap();
        let e = check_code_signing_eku(&root[0])
            .expect_err("a CA root carries no code-signing EKU and must not pass as a leaf");
        assert!(matches!(e, Error::NotCodeSigning), "{e}");
    }
}
