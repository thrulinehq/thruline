//! C20: approving needs a paid licence in v0.1.1. A v0.1.0 trial must not be
//! another entitlement source. The signed expiry still determines when a paid
//! Pro licence stops working; the issuer already includes renewal grace in it.

use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use thruline_licence::{
    check_file, decide, seal, verify, Entitlement, Fingerprint, Plan, Problem, Source, Surface,
    TrustedKey, Verdict,
};

const LICENCE_ID: &str = "01J8ZQ4Y6T3M2N1P0R9S8T7V6W";
const PAID_PERIOD_END: i64 = 1_790_726_400; // 30 Sep 2026 00:00 UTC
const IN_RENEWAL_GRACE: i64 = 1_791_331_200; // 7 Oct 2026 00:00 UTC
const SIGNED_EXPIRY: i64 = 1_791_936_000; // 14 Oct 2026 00:00 UTC

fn licence(expires_at: &str) -> (Vec<u8>, TrustedKey) {
    let signing = SigningKey::generate(&mut OsRng);
    let trusted = TrustedKey::from_bytes(&signing.verifying_key().to_bytes()).expect("test key");
    let payload = format!(
        "{{\"format\":1,\"licence_id\":\"{LICENCE_ID}\",\"product\":\"thruline\",\"plan\":\"pro\",\"issued_at\":\"2026-09-01T00:00:00Z\",\"expires_at\":\"{expires_at}\"}}"
    );
    (
        seal(payload.as_bytes(), &signing)
            .expect("seal test licence")
            .into_bytes(),
        trusted,
    )
}

fn source(verdict: Verdict, installed: bool) -> Source {
    Source::Licence { verdict, installed }
}

#[test]
fn no_trial_module_or_entitlement_source_remains() {
    let library = include_str!("../src/lib.rs");
    assert!(
        !library.contains("pub mod trial;"),
        "trial module is still public"
    );
    assert!(
        !library.contains("Trial(Trial)"),
        "trial remains an entitlement source"
    );
    assert!(
        !library.contains("TRIAL_PLAN"),
        "trial still grants a paid plan"
    );
}

#[test]
fn no_installed_licence_cannot_approve_and_names_pro() {
    let (_, trusted) = licence("2026-10-14T00:00:00Z");
    let dir = tempfile::tempdir().expect("temp home");
    let verdict = check_file(
        &dir.path().join("licence.txt"),
        &[trusted],
        IN_RENEWAL_GRACE,
        &Fingerprint::Unavailable,
    );
    assert_eq!(verdict, Verdict::Refused(Problem::Absent));
    match decide(Surface::ApproveVersion, &[source(verdict, false)]) {
        Entitlement::Refused {
            reason,
            enable,
            status,
        } => {
            assert_eq!(status, "absent");
            assert!(reason.contains("Pro licence"), "{reason}");
            assert!(!reason.to_lowercase().contains("trial"), "{reason}");
            assert!(enable.contains("licence install <file>"), "{enable}");
        }
        Entitlement::Allowed => panic!("approving opened with no Pro licence"),
    }
    assert!(!dir.path().join("trial.json").exists());
}

#[test]
fn a_valid_pro_licence_opens_approval() {
    let (bytes, trusted) = licence("2026-10-14T00:00:00Z");
    let verdict = verify(
        &bytes,
        &[trusted],
        IN_RENEWAL_GRACE,
        &Fingerprint::Unavailable,
    );
    assert!(matches!(&verdict, Verdict::Valid(l) if l.plan == Plan::Pro));
    assert_eq!(
        decide(Surface::ApproveVersion, &[source(verdict, true)]),
        Entitlement::Allowed
    );
}

#[test]
fn paid_pro_expiry_preserves_the_issuers_renewal_grace() {
    // The Worker signs the paid period end plus its 14-day renewal grace.
    // This crate trusts that signed expiry; it does not add a local clock.
    assert_eq!(
        thruline_licence::timestamp(PAID_PERIOD_END),
        "2026-09-30T00:00:00Z"
    );
    assert_eq!(
        thruline_licence::timestamp(SIGNED_EXPIRY),
        "2026-10-14T00:00:00Z"
    );
    let (bytes, trusted) = licence("2026-10-14T00:00:00Z");
    for now in [PAID_PERIOD_END, IN_RENEWAL_GRACE, SIGNED_EXPIRY - 1] {
        let verdict = verify(
            &bytes,
            std::slice::from_ref(&trusted),
            now,
            &Fingerprint::Unavailable,
        );
        assert!(matches!(&verdict, Verdict::Valid(l) if l.plan == Plan::Pro));
        assert_eq!(
            decide(Surface::ApproveVersion, &[source(verdict, true)]),
            Entitlement::Allowed,
            "Pro licence stopped before its signed expiry at {now}"
        );
    }
    let expired = verify(&bytes, &[trusted], SIGNED_EXPIRY, &Fingerprint::Unavailable);
    assert!(matches!(expired, Verdict::Refused(Problem::Expired { .. })));
}
