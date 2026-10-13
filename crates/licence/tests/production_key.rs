//! LICENCE-GATE-ON (P11: FI-34, FI-35, D-238, D-239, D-244) — the one
//! production public key this crate trusts, and the flag that makes it matter.
//!
//! What is pinned here is what a person could not see by looking at the binary:
//! that exactly ONE key is compiled in and that it parses (a constant that does
//! not parse is dropped by `production_keys`, so the two counts must agree); that
//! it is the founder's key and not some other 32 bytes (its `key_id`, and the
//! fingerprint D-239 recorded, which is a SHA-256 over the 32 RAW bytes and not
//! over their hex text); that a licence made with any other key — including one
//! generated a moment ago, which is what every test in this repository uses — is
//! refused as `UnknownKey` against exactly this list; and that the test-mode
//! Worker's key never entered it.
//!
//! No private key exists in this repository, in this file or anywhere a test can
//! reach: the only key pair used below is generated at run time, in memory.

use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use sha2::{Digest, Sha256};
use thruline_licence::{
    fingerprint, production_keys, seal, to_hex, verify, Problem, Verdict, PAID_GATE,
    PRODUCTION_PUBLIC_KEYS,
};

/// The key id FI-35 recorded for the production key.
const PRODUCTION_KEY_ID: &str = "3ef3540d43dbf105";

/// The first 16 hex characters of SHA-256 over the key's 32 raw bytes, as D-239
/// recorded it.
const PRODUCTION_KEY_FINGERPRINT: &str = "2253220dc1a324d7";

/// The first 8 hex characters of the test-mode licence Worker's public key,
/// from the release repository's `payments/phase3/PHASE3-RECORD.md` (the
/// `LICENSE_PUBLIC_KEY_HEX` row). Only the prefix is pinned: a key with this
/// prefix in the list is that key, and a key without it is not.
const TEST_MODE_WORKER_KEY_PREFIX: &str = "46cef04e";

/// 2026-09-23T12:00:00Z. Passed in; the crate reads no clock.
const NOW: i64 = 1_790_164_800;

const PAYLOAD: &str = "{\"format\":1,\"licence_id\":\"01J8ZQ4Y6T3M2N1P0R9S8T7V6W\",\"product\":\"thruline\",\"plan\":\"pro\",\"issued_at\":\"2026-09-01T00:00:00Z\",\"expires_at\":\"2099-01-01T00:00:00Z\"}";

#[test]
fn the_paid_gate_ships_on() {
    // Read at run time, so a build that ships the gate off fails this test by
    // name, and clippy has no constant assertion to object to.
    assert!(
        std::hint::black_box(PAID_GATE),
        "PAID_GATE is false: v0.1.0 would ship with the paid gate disabled, which P11 \
         (FI-34, D-238) says it does not"
    );
}

#[test]
fn exactly_one_production_key_is_compiled_in_and_it_parses() {
    assert_eq!(
        PRODUCTION_PUBLIC_KEYS.len(),
        1,
        "the list of production keys is not exactly the founder's one key"
    );
    // `production_keys` drops a constant that does not parse instead of trusting
    // it, and says nothing: this is the test that counts what it dropped.
    assert_eq!(
        production_keys().len(),
        1,
        "a compiled-in production key does not parse as an Ed25519 public key"
    );
}

#[test]
fn the_key_is_the_founders_and_not_some_other_thirty_two_bytes() {
    let keys = production_keys();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].id(), PRODUCTION_KEY_ID, "key_id");
    assert_eq!(
        thruline_licence::key_id(&PRODUCTION_PUBLIC_KEYS[0]),
        PRODUCTION_KEY_ID,
        "the constant and the parsed key name the same id"
    );
    // D-239 hashed the 32 RAW bytes, not the 64 hex characters that spell them.
    let raw = to_hex(&Sha256::digest(PRODUCTION_PUBLIC_KEYS[0]));
    assert_eq!(
        &raw[..16],
        PRODUCTION_KEY_FINGERPRINT,
        "sha256 over the 32 raw bytes"
    );
    let over_the_hex_text = to_hex(&Sha256::digest(
        to_hex(&PRODUCTION_PUBLIC_KEYS[0]).as_bytes(),
    ));
    assert_ne!(
        &over_the_hex_text[..16],
        PRODUCTION_KEY_FINGERPRINT,
        "the recorded fingerprint is over the raw bytes, and the hex text would give another value"
    );
}

#[test]
fn a_licence_signed_by_a_key_made_at_run_time_is_refused_unknown_key() {
    let stranger = SigningKey::generate(&mut OsRng);
    let licence = seal(PAYLOAD.as_bytes(), &stranger).expect("seal");
    let verdict = verify(
        licence.as_bytes(),
        &production_keys(),
        NOW,
        &fingerprint::of_platform_id("cccccccc-1111-2222-3333-444444444444"),
    );
    assert_eq!(
        verdict,
        Verdict::Refused(Problem::UnknownKey),
        "a licence made by a key this binary was not built with must be refused as an unknown key \
         (a test key must never verify against the production list)"
    );
}

#[test]
fn the_test_mode_workers_public_key_is_not_in_the_list() {
    // Not vacuously: an empty list holds no test key either, and says nothing.
    assert!(
        !PRODUCTION_PUBLIC_KEYS.is_empty(),
        "there is no production key to compare the test-mode Worker's against"
    );
    for key in PRODUCTION_PUBLIC_KEYS {
        assert!(
            !to_hex(key).starts_with(TEST_MODE_WORKER_KEY_PREFIX),
            "a production key carries the test-mode Worker's prefix {TEST_MODE_WORKER_KEY_PREFIX}: \
             a licence sold in test mode would then open a paid surface in a shipped build"
        );
    }
}
