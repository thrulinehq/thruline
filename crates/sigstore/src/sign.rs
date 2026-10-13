//! Producing a bundle — keyless via Fulcio, or with a local cosign key.

use crate::bundle::Bundle;
use crate::error::{Error, Result};

/// How the signer proves who it is.
#[derive(Debug, Clone)]
pub enum OidcProvider {
    /// Interactive OAuth **device flow**: we print a URL and a user code, and a
    /// human approves in their own browser under their own account. We never
    /// handle their credentials.
    DeviceFlow {
        issuer: String,
        client_id: String,
        fulcio_url: String,
        rekor_url: String,
    },
    /// A token the caller already holds — a CI ambient credential, or one
    /// obtained out of band.
    Token {
        token: String,
        fulcio_url: String,
        rekor_url: String,
    },
    /// No OIDC: sign with a local `cosign.key`. The offline/enterprise path.
    LocalKey {
        path: std::path::PathBuf,
        passphrase: Vec<u8>,
    },
}

impl OidcProvider {
    /// Sigstore **staging**. Never production by default: a mistake against
    /// staging is a test, and against production it is a permanent public
    /// transparency-log entry.
    pub fn staging_device_flow() -> OidcProvider {
        OidcProvider::DeviceFlow {
            issuer: "https://oauth2.sigstage.dev/auth".to_string(),
            client_id: "sigstore".to_string(),
            fulcio_url: "https://fulcio.sigstage.dev".to_string(),
            rekor_url: "https://rekor.sigstage.dev".to_string(),
        }
    }
}

/// Sign `payload` and return a v0.3 bundle.
///
/// The payload is wrapped in a DSSE envelope with the in-toto payload type,
/// matching what `crates/attest` already produces.
pub fn sign(payload: &[u8], identity: OidcProvider) -> Result<Bundle> {
    match identity {
        OidcProvider::LocalKey { path, passphrase } => {
            sign_with_local_key(payload, &path, &passphrase)
        }
        #[cfg(not(feature = "net"))]
        _ => Err(Error::FeatureDisabled {
            operation: "keyless signing",
            feature: "net",
        }),
        #[cfg(feature = "net")]
        other => crate::net::sign_keyless(payload, other),
    }
}

/// Sign with a real `cosign.key`, producing a v0.3 bundle whose verification
/// material is the bare public key.
///
/// # What this bundle does and does not prove
///
/// It proves the payload was signed by the holder of that key. It proves
/// **nothing about who that is** — there is no certificate, so no identity, no
/// issuer, no SCT, and nothing in a transparency log. That is the honest shape
/// of offline signing, and it is why [`crate::verify`] refuses these bundles by
/// name instead of returning an [`crate::Identity`] it cannot substantiate;
/// [`crate::verify_with_public_key`] is the function that fits.
///
/// The DSSE envelope inside is byte-compatible with the one
/// `crates/attest` already produces and consumes — same PAE, same
/// `payloadType` — so the merge changes who verifies a signature, not what is
/// signed.
fn sign_with_local_key(
    payload: &[u8],
    path: &std::path::Path,
    passphrase: &[u8],
) -> Result<Bundle> {
    use p256::ecdsa::{signature::Signer, DerSignature};
    use spki::EncodePublicKey;

    let pem = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    let key = crate::key::load_cosign_key(&pem, passphrase)?;

    let sig: DerSignature = key.sign(&crate::dsse::pae(crate::IN_TOTO_PAYLOAD_TYPE, payload));
    let spki = key
        .verifying_key()
        .to_public_key_der()
        .map_err(|e| Error::Key {
            path: path.display().to_string(),
            reason: format!("encoding the public key: {e}"),
        })?;

    Ok(Bundle {
        media_type: crate::BUNDLE_V03.to_string(),
        verification_material: crate::bundle::VerificationMaterial {
            certificate: None,
            x509_certificate_chain: None,
            public_key: Some(serde_json::json!({
                "hint": crate::bundle::b64_encode(spki.as_bytes()),
            })),
            tlog_entries: Vec::new(),
        },
        message_signature: None,
        dsse_envelope: Some(crate::dsse::Envelope {
            payload: crate::bundle::b64_encode(payload),
            payload_type: crate::IN_TOTO_PAYLOAD_TYPE.to_string(),
            signatures: vec![crate::dsse::Signature {
                sig: crate::bundle::b64_encode(sig.as_bytes()),
                keyid: None,
            }],
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_at(dir: &std::path::Path) -> std::path::PathBuf {
        let priv_path = dir.join("cosign.key");
        crate::key::generate(&priv_path, &dir.join("cosign.pub"), b"hunter2").unwrap();
        priv_path
    }

    #[test]
    fn a_locally_signed_bundle_verifies_against_the_key_that_signed_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = key_at(dir.path());
        let bundle = sign(
            b"{\"_type\":\"test\"}",
            OidcProvider::LocalKey {
                path: path.clone(),
                passphrase: b"hunter2".to_vec(),
            },
        )
        .unwrap();

        let pem = std::fs::read_to_string(&path).unwrap();
        let key = crate::key::load_cosign_key(&pem, b"hunter2").unwrap();
        use spki::EncodePublicKey;
        let spki = key.verifying_key().to_public_key_der().unwrap();

        let key_id = crate::verify_with_public_key(&bundle, spki.as_bytes())
            .expect("the bundle must verify against the key that produced it");
        // The id names the key that actually verified, which is what a policy
        // listing several keys needs in order to say which one signed.
        assert_eq!(key_id, crate::public_key_id(spki.as_bytes()));
        assert!(key_id.starts_with("sha256:"), "{key_id}");
    }

    #[test]
    fn a_locally_signed_bundle_does_not_verify_against_another_key() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = sign(
            b"payload",
            OidcProvider::LocalKey {
                path: key_at(dir.path()),
                passphrase: b"hunter2".to_vec(),
            },
        )
        .unwrap();

        let other = tempfile::tempdir().unwrap();
        let other_path = key_at(other.path());
        let other_key =
            crate::key::load_cosign_key(&std::fs::read_to_string(&other_path).unwrap(), b"hunter2")
                .unwrap();
        use spki::EncodePublicKey;
        let spki = other_key.verifying_key().to_public_key_der().unwrap();

        assert!(crate::verify_with_public_key(&bundle, spki.as_bytes()).is_err());
    }

    /// A tampered payload must not verify — the signature covers the PAE bytes,
    /// not the envelope's framing.
    #[test]
    fn a_tampered_payload_does_not_verify() {
        let dir = tempfile::tempdir().unwrap();
        let path = key_at(dir.path());
        let mut bundle = sign(
            b"the original",
            OidcProvider::LocalKey {
                path: path.clone(),
                passphrase: b"hunter2".to_vec(),
            },
        )
        .unwrap();
        bundle.dsse_envelope.as_mut().unwrap().payload =
            crate::bundle::b64_encode(b"something else");

        let key = crate::key::load_cosign_key(&std::fs::read_to_string(&path).unwrap(), b"hunter2")
            .unwrap();
        use spki::EncodePublicKey;
        let spki = key.verifying_key().to_public_key_der().unwrap();
        assert!(crate::verify_with_public_key(&bundle, spki.as_bytes()).is_err());
    }

    /// Two different keys must get two different ids, or a policy listing both
    /// could not tell them apart.
    #[test]
    fn different_keys_get_different_ids() {
        use spki::EncodePublicKey;
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let ka = crate::key::load_cosign_key(
            &std::fs::read_to_string(key_at(a.path())).unwrap(),
            b"hunter2",
        )
        .unwrap();
        let kb = crate::key::load_cosign_key(
            &std::fs::read_to_string(key_at(b.path())).unwrap(),
            b"hunter2",
        )
        .unwrap();
        let ida = crate::public_key_id(ka.verifying_key().to_public_key_der().unwrap().as_bytes());
        let idb = crate::public_key_id(kb.verifying_key().to_public_key_der().unwrap().as_bytes());
        assert_ne!(ida, idb);
        assert!(ida.starts_with("sha256:") && ida.len() == 71, "{ida}");
    }

    /// `verify` proves an identity. A public-key bundle has none, so it must
    /// refuse by name rather than return one it cannot substantiate.
    #[test]
    fn verify_refuses_a_public_key_bundle_because_it_names_nobody() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = sign(
            b"payload",
            OidcProvider::LocalKey {
                path: key_at(dir.path()),
                passphrase: b"hunter2".to_vec(),
            },
        )
        .unwrap();
        let root = crate::TrustRoot::from_json(include_str!(
            "../fixtures/trustroot/prod-trusted_root.json"
        ))
        .unwrap();
        let e = crate::verify(&bundle, &crate::Policy::new(root)).unwrap_err();
        assert!(matches!(e, Error::NoCertificateOnlyAPublicKey), "{e}");
    }

    #[test]
    fn the_envelope_is_the_one_attest_already_understands() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = sign(
            b"x",
            OidcProvider::LocalKey {
                path: key_at(dir.path()),
                passphrase: b"hunter2".to_vec(),
            },
        )
        .unwrap();
        let env = bundle.dsse_envelope.unwrap();
        assert_eq!(env.payload_type, crate::IN_TOTO_PAYLOAD_TYPE);
        assert_eq!(env.signatures.len(), 1);
    }
}
