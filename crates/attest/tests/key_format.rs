//! §8.1's local keys, and the interoperability the cross-machine story rests on.
//!
//! **A key format the common parsers refuse is not a detail.** "Verifies on
//! another machine" depends on the other machine being able to read the key.

use std::process::Command;

#[test]
fn the_private_key_is_pkcs8_v0_at_forty_eight_bytes() {
    // `ed25519-dalek`'s own encoder emits RFC 5958 OneAsymmetricKey **v2**: 83
    // bytes, `version = 1`, public key in a `[1]` attribute. Legal ASN.1, and
    // rejected by OpenSSL 3 and Python `cryptography` alike.
    let key = ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]);
    let der = thruline_attest::key::private_der(&key);
    assert_eq!(der.len(), 48, "not the 48-byte v0 form");
    assert_eq!(der[2..5], [0x02, 0x01, 0x00], "version is not 0");
    assert_eq!(
        der[5..12],
        [0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70],
        "not the Ed25519 algorithm identifier"
    );
    assert!(
        !der[12..].contains(&0x81),
        "a [1] publicKey attribute is present; that is the v2 form the parsers refuse"
    );
    // The seed is carried verbatim, so nothing is lost by dropping the
    // attribute — the public half is derivable.
    assert_eq!(&der[16..], key.as_bytes());
}

#[test]
fn the_key_we_write_round_trips_through_our_own_loader() {
    let dir = tempfile::tempdir().expect("tempdir");
    let priv_path = dir.path().join("signing.key");
    let pub_path = dir.path().join("signing.pub");
    let generated = thruline_attest::key::generate(&priv_path, &pub_path).expect("generate");
    let loaded = thruline_attest::key::load_signing(&priv_path).expect("load");
    assert_eq!(loaded.as_bytes(), generated.as_bytes());
    let verifying = thruline_attest::key::load_verifying(&pub_path).expect("load pub");
    assert_eq!(verifying.as_bytes(), generated.verifying_key().as_bytes());
}

#[test]
fn a_v2_key_from_an_earlier_build_still_loads() {
    // Refusing it would invalidate every receipt already signed with it — a
    // worse failure than the one being fixed.
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    let key = ed25519_dalek::SigningKey::from_bytes(&[9u8; 32]);
    let v2 = key
        .to_pkcs8_pem(ed25519_dalek::pkcs8::spki::der::pem::LineEnding::LF)
        .expect("v2 pem");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("old.key");
    std::fs::write(&path, v2.as_bytes()).expect("write");
    let loaded = thruline_attest::key::load_signing(&path).expect("a v2 key must still load");
    assert_eq!(loaded.as_bytes(), key.as_bytes());
}

#[test]
fn the_private_key_file_is_not_world_readable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let priv_path = dir.path().join("signing.key");
    thruline_attest::key::generate(&priv_path, &dir.path().join("signing.pub")).expect("generate");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&priv_path)
            .expect("stat")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "mode is {:o}", mode & 0o777);
    }
}

#[test]
fn generate_refuses_to_overwrite_an_existing_key() {
    // An overwritten signing key silently invalidates every receipt issued
    // under it, and the failure surfaces later as a verification failure that
    // reads like tampering.
    let dir = tempfile::tempdir().expect("tempdir");
    let priv_path = dir.path().join("signing.key");
    let pub_path = dir.path().join("signing.pub");
    thruline_attest::key::generate(&priv_path, &pub_path).expect("first");
    let before = std::fs::read(&priv_path).expect("read");
    assert!(thruline_attest::key::generate(&priv_path, &pub_path).is_err());
    assert_eq!(
        before,
        std::fs::read(&priv_path).expect("read"),
        "the key was overwritten"
    );
}

/// The interoperability claim, checked against a real parser rather than
/// asserted.
///
/// Skips loudly when neither tool is installed — a green suite that quietly did
/// not run its only external check is the failure this project keeps catching.
#[test]
fn the_key_is_readable_by_a_parser_that_is_not_ours() {
    let dir = tempfile::tempdir().expect("tempdir");
    let priv_path = dir.path().join("signing.key");
    thruline_attest::key::generate(&priv_path, &dir.path().join("signing.pub")).expect("generate");

    let openssl = Command::new("openssl")
        .args(["pkey", "-in"])
        .arg(&priv_path)
        .arg("-noout")
        .output();
    match openssl {
        Ok(out) if out.status.success() => return,
        Ok(out) => panic!(
            "openssl rejected the key we write: {}",
            String::from_utf8_lossy(&out.stderr)
        ),
        Err(_) => {}
    }

    let python = Command::new("python3")
        .args([
            "-c",
            &format!(
                "from cryptography.hazmat.primitives.serialization import load_pem_private_key;\
                 load_pem_private_key(open({:?},'rb').read(), password=None); print('ok')",
                priv_path.display().to_string()
            ),
        ])
        .output();
    match python {
        Ok(out) if out.status.success() => return,
        Ok(out) => panic!(
            "python cryptography rejected the key we write: {}",
            String::from_utf8_lossy(&out.stderr)
        ),
        Err(_) => {}
    }
    eprintln!(
        "SKIPPED: neither openssl nor python-cryptography is available, so the \
         interoperability claim was NOT checked in this run."
    );
}
