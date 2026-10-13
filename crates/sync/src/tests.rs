use super::*;

/// Argon2id at 19 MiB is slow on purpose, so the tests derive once.
fn vault() -> (SyncKey, Kdf) {
    let kdf = Kdf::new(&[7u8; 16]);
    let key = derive("a passphrase the server never sees", &kdf).expect("derive");
    (key, kdf)
}

#[test]
fn a_sealed_object_opens_under_its_own_id() {
    let (key, kdf) = vault();
    let e = seal(&key, "session/abc", &kdf, b"the plaintext").expect("seal");
    assert_eq!(
        open(&key, "session/abc", &e).expect("open"),
        b"the plaintext"
    );
}

/// **The defence against a server that shuffles objects.** The id is the AEAD's
/// associated data, so an envelope served under a different id fails even with
/// the right key.
#[test]
fn an_envelope_served_under_another_id_does_not_open() {
    let (key, kdf) = vault();
    let e = seal(&key, "session/abc", &kdf, b"the plaintext").expect("seal");
    assert!(matches!(
        open(&key, "session/def", &e),
        Err(SyncError::Open)
    ));
}

/// Every mutation fails, and none returns partial content. There is no third
/// outcome between "the tag verified" and "an error".
#[test]
fn a_tampered_envelope_never_yields_partial_plaintext() {
    let (key, kdf) = vault();
    let e = seal(&key, "id", &kdf, b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").expect("seal");
    let b64 = base64::engine::general_purpose::STANDARD;

    let mut ct = b64.decode(&e.ciphertext).expect("decode");
    for at in [0usize, ct.len() / 2, ct.len() - 1] {
        let mut broken = ct.clone();
        broken[at] ^= 0x01;
        let mut tampered = e.clone();
        tampered.ciphertext = b64.encode(&broken);
        assert!(matches!(open(&key, "id", &tampered), Err(SyncError::Open)));
    }
    // Truncation, which is the mutation a naive stream cipher would survive.
    ct.truncate(ct.len() / 2);
    let mut truncated = e.clone();
    truncated.ciphertext = b64.encode(&ct);
    assert!(matches!(open(&key, "id", &truncated), Err(SyncError::Open)));

    // A different nonce, same ciphertext.
    let mut renonced = e.clone();
    renonced.nonce = b64.encode([9u8; 24]);
    assert!(matches!(open(&key, "id", &renonced), Err(SyncError::Open)));

    // A nonce of the wrong length is refused rather than padded.
    let mut short = e.clone();
    short.nonce = b64.encode([9u8; 12]);
    assert!(matches!(open(&key, "id", &short), Err(SyncError::Open)));
}

/// A nonce is drawn fresh every time. Reuse under one key is the failure that
/// takes an AEAD from "secure" to "reveals the xor of two plaintexts", and it
/// is invisible until someone looks.
#[test]
fn every_seal_draws_a_fresh_nonce() {
    let (key, kdf) = vault();
    let mut nonces = std::collections::BTreeSet::new();
    let mut ciphertexts = std::collections::BTreeSet::new();
    for _ in 0..64 {
        let e = seal(&key, "same/id", &kdf, b"the identical plaintext").expect("seal");
        nonces.insert(e.nonce);
        ciphertexts.insert(e.ciphertext);
    }
    assert_eq!(nonces.len(), 64, "a nonce repeated across 64 seals");
    assert_eq!(
        ciphertexts.len(),
        64,
        "identical plaintext produced identical ciphertext, so the nonce is not \
         reaching the cipher"
    );
}

#[test]
fn the_wrong_passphrase_does_not_open() {
    let (key, kdf) = vault();
    let e = seal(&key, "id", &kdf, b"secret").expect("seal");
    let other = derive("a different passphrase", &kdf).expect("derive");
    assert!(matches!(open(&other, "id", &e), Err(SyncError::Open)));
}

/// The parameters are recorded so a second machine re-derives the same key —
/// which is the whole point of sync — and so a future client can raise the cost
/// without being unable to open old objects.
#[test]
fn a_second_machine_re_derives_the_key_from_the_envelope() {
    let (key, kdf) = vault();
    let e = seal(&key, "id", &kdf, b"portable").expect("seal");
    // The second machine has the passphrase and the envelope, and nothing else.
    let elsewhere = derive("a passphrase the server never sees", &e.kdf).expect("derive");
    assert_eq!(open(&elsewhere, "id", &e).expect("open"), b"portable");
    // Different parameters derive a different key, so the parameters are load
    // bearing rather than decorative.
    let mut cheaper = e.kdf.clone();
    cheaper.t_cost = 1;
    let wrong = derive("a passphrase the server never sees", &cheaper).expect("derive");
    assert!(matches!(open(&wrong, "id", &e), Err(SyncError::Open)));
}

#[test]
fn an_unknown_envelope_format_is_refused_rather_than_guessed_at() {
    let (key, kdf) = vault();
    let mut e = seal(&key, "id", &kdf, b"x").expect("seal");
    e.format = "TLSYNC2".to_string();
    assert!(matches!(
        open(&key, "id", &e),
        Err(SyncError::Format { .. })
    ));
}

#[test]
fn an_unknown_kdf_is_refused_rather_than_defaulted() {
    let mut kdf = Kdf::new(&[1u8; 16]);
    kdf.algorithm = "pbkdf2".to_string();
    assert!(matches!(derive("x", &kdf), Err(SyncError::Kdf(_))));
}

#[test]
fn an_empty_id_is_refused_because_it_would_be_associated_data_of_nothing() {
    let (key, kdf) = vault();
    assert!(matches!(
        seal(&key, "", &kdf, b"x"),
        Err(SyncError::EmptyId)
    ));
}

/// The key must not be printable by accident — a key in a panic message is a
/// key in a CI log.
#[test]
fn the_key_does_not_print_itself() {
    let (key, _) = vault();
    assert_eq!(format!("{key:?}"), "SyncKey(<redacted>)");
}
