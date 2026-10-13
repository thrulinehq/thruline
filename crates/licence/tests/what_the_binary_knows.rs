//! ADV-dist-copy-4 — a refused licence is told exactly what the binary knows.
//!
//! Before anything inside a licence is believed, the binary knows two things:
//! whether the key the file names is one of the keys built into it, and
//! whether the file checks out against that key. Neither says who made the
//! file. A licence made with a newer key (after a rotation, a second entry in
//! `PRODUCTION_PUBLIC_KEYS`) reaches an older binary as an unknown key, and
//! the fix is to update ThruLine — so the sentence may not say the licence
//! "wasn't issued by ThruLine". A file signed by some other key that names a
//! trusted key fails the check exactly as a changed file does, and the
//! binary cannot tell the two apart — so the sentence names both.
//!
//! Every key pair here is generated at run time, in memory.

use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;
use thruline_licence::{
    fingerprint, key_id, seal, to_hex, verify, Fingerprint, Problem, TrustedKey, Verdict,
};

/// 2026-09-23T12:00:00Z. Passed in; the crate reads no clock.
const NOW: i64 = 1_790_164_800;

const PAYLOAD: &str = "{\"format\":1,\"licence_id\":\"01J8ZQ4Y6T3M2N1P0R9S8T7V6W\",\"product\":\"thruline\",\"plan\":\"pro\",\"issued_at\":\"2026-09-01T00:00:00Z\",\"expires_at\":\"2099-01-01T00:00:00Z\"}";

const DOWNLOAD_AGAIN: &str =
    "Download it again from the licence page on ThruLine's website, with your licence key.";

fn trusted(signing: &SigningKey) -> TrustedKey {
    TrustedKey::from_bytes(&signing.verifying_key().to_bytes()).expect("key")
}

fn here() -> Fingerprint {
    fingerprint::of_platform_id("cccccccc-1111-2222-3333-444444444444")
}

fn refused(v: Verdict) -> Problem {
    match v {
        Verdict::Refused(p) => p,
        Verdict::Valid(l) => panic!("expected a refusal, got a valid licence: {l:?}"),
    }
}

#[test]
fn after_a_key_rotation_an_older_build_says_update_and_never_says_not_thrulines() {
    let first = SigningKey::generate(&mut OsRng);
    let second = SigningKey::generate(&mut OsRng);
    // A licence made with the second key, as it would be after a rotation.
    let licence = seal(PAYLOAD.as_bytes(), &second).expect("seal");

    let older = [trusted(&first)];
    let p = refused(verify(licence.as_bytes(), &older, NOW, &here()));
    assert_eq!(p, Problem::UnknownKey);
    assert_eq!(
        (p.problem(), p.fix()),
        (
            "This licence names a maker this version of ThruLine doesn't know, so it can't check it."
                .to_string(),
            "Update ThruLine, then install the licence again; if it's still refused, download it again from the licence page on ThruLine's website, with your licence key."
                .to_string()
        )
    );
    assert!(
        !p.problem().contains("wasn't issued by ThruLine"),
        "the binary only knows the key is not one of its own: {}",
        p.problem()
    );
    assert!(p.fix().starts_with("Update ThruLine"), "{}", p.fix());

    // The same bytes, read by a build that carries both keys, are a licence.
    let newer = [trusted(&first), trusted(&second)];
    assert!(matches!(
        verify(licence.as_bytes(), &newer, NOW, &here()),
        Verdict::Valid(_)
    ));
}

#[test]
fn a_foreign_key_carrying_a_trusted_key_id_is_told_both_things_it_could_be() {
    let ours = SigningKey::generate(&mut OsRng);
    let foreign = SigningKey::generate(&mut OsRng);
    // Signed by the foreign key, and naming ours: the adversary's
    // `unknown-key-claims-trusted-id`. Nothing in it was changed after it was
    // made; it was never made with the key it names.
    let signature = foreign.sign(PAYLOAD.as_bytes()).to_bytes();
    let claims_ours = format!(
        "ThruLine licence\nkey: {}\npayload: {PAYLOAD}\nsignature: {}\n",
        key_id(&ours.verifying_key().to_bytes()),
        to_hex(&signature)
    );
    let keys = [trusted(&ours)];
    let p = refused(verify(claims_ours.as_bytes(), &keys, NOW, &here()));
    assert_eq!(p, Problem::Tampered);
    assert_eq!(
        (p.problem(), p.fix()),
        (
            "This licence has been changed, or wasn't made by the maker it names, so it can't be trusted."
                .to_string(),
            DOWNLOAD_AGAIN.to_string()
        )
    );

    // A file made with our key and then changed gets the very same pair: the
    // binary cannot tell the two apart, so it does not pretend to.
    let changed = seal(PAYLOAD.as_bytes(), &ours).expect("seal").replacen(
        "\"plan\":\"pro\"",
        "\"plan\":\"prn\"",
        1,
    );
    assert_eq!(refused(verify(changed.as_bytes(), &keys, NOW, &here())), p);
}
