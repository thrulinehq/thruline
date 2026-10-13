//! Guard 3, inverted: the keyless path RUNS, and each axis is its own check.
//!
//! `no_source_file_reaches_a_sigstore_client` asserted that nothing here
//! reached a Sigstore client, and its own failure message set the bar for
//! replacing it (Track B's guard table, DECISIONS #130):
//!
//! > *"Either keyless is real — in which case the certificate chain, the TUF
//! > root and the Rekor inclusion proof are all verified and the documents must
//! > stop saying it is unbuilt — or it is a stub, which is worse than nothing
//! > here."*
//!
//! Keyless is real, so an absence assertion cannot be the guard any more.
//!
//! **B's `tests/end_to_end.rs` travelled with the snapshot and proves the
//! crate**; what it cannot prove is that `attest::verify` reaches it. That is
//! this file: the merged path, driven through `attest`'s own signature arm.

use thruline_attest::statement::{
    AssertedBy, ClaimEntry, Predicate, Session, Statement, Subject, VerifierInfo, PAYLOAD_TYPE,
};
use thruline_attest::verify::{verify, LocalEvidence, Outcome, Report};

const BUNDLE: &str =
    include_str!("../../sigstore/fixtures/bundles/cpython-3.13.7.tgz.sigstore.json");
const PROD_ROOT: &str = include_str!("../../sigstore/fixtures/trustroot/prod-trusted_root.json");

const TREE: &str = "0123456789abcdef0123456789abcdef01234567";

fn key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[11u8; 32])
}

fn statement() -> Statement {
    Statement {
        type_: "https://in-toto.io/Statement/v1".into(),
        subject: vec![Subject {
            name: "workspace".into(),
            digest: [("gitTree".to_string(), TREE.to_string())]
                .into_iter()
                .collect(),
        }],
        predicate_type: thruline_attest::statement::PREDICATE_TYPE.into(),
        predicate: Predicate {
            session: Session {
                id: "s".into(),
                harness: "claude-code".into(),
                event_chain_head: "sha256:aaaa".into(),
                event_count: 1,
                ..Default::default()
            },
            claims: vec![ClaimEntry {
                id: "clm_1".into(),
                kind: "tests_pass".into(),
                text: "tests pass".into(),
                asserted_by: AssertedBy {
                    harness: "claude-code".into(),
                    model: None,
                },
            }],
            verifier: VerifierInfo {
                thruline_version: "0.1.0".into(),
                mode: "local".into(),
                policy_hash: Some("sha256:0f0f".into()),
            },
            ..Default::default()
        },
    }
}

fn envelope() -> thruline_attest::dsse::Envelope {
    let k = key();
    thruline_attest::dsse::sign(
        &serde_json::to_vec(&statement()).expect("serialise"),
        PAYLOAD_TYPE,
        &k,
        Some(thruline_attest::dsse::key_id(&k.verifying_key())),
    )
}

fn policy() -> thruline_sigstore::Policy {
    thruline_sigstore::Policy::new(
        thruline_sigstore::TrustRoot::from_json(PROD_ROOT).expect("production trust root"),
    )
}

fn names(r: &Report) -> Vec<String> {
    r.checks.iter().map(|c| c.name.clone()).collect()
}

#[test]
fn the_keyless_path_runs_and_each_axis_is_reported_on_its_own() {
    let env = envelope();
    let r = verify(
        &env,
        &[],
        &LocalEvidence {
            sigstore: Some((BUNDLE.to_string(), policy())),
            ..LocalEvidence::default()
        },
    );
    let got = names(&r);
    for axis in [
        "signature",
        "certificate chain",
        "SCT",
        "transparency log",
        "identity",
    ] {
        assert!(
            got.iter().any(|n| n == axis),
            "the keyless path did not report {axis:?}: {got:?}. Five checks under \
             five names is the point — collapsing them into one `signature: pass` \
             is how a reader stops being able to tell which guarantee held."
        );
    }
}

#[test]
fn a_corrupted_bundle_does_not_report_a_passing_identity() {
    // One flipped bit in the middle of a base64 field: it survives parsing, so
    // only the cryptography can catch it — B's own corruption method.
    let mut v: serde_json::Value = serde_json::from_str(BUNDLE).expect("bundle json");
    let mut flipped = false;
    for ptr in [
        "/messageSignature/signature",
        "/dsseEnvelope/signatures/0/sig",
    ] {
        if let Some(sig) = v.pointer_mut(ptr) {
            if let Some(txt) = sig.as_str() {
                let mut b = txt.as_bytes().to_vec();
                let i = b.len() / 2;
                b[i] = if b[i] == b'A' { b'B' } else { b'A' };
                *sig = serde_json::Value::String(String::from_utf8(b).expect("ascii"));
                flipped = true;
            }
        }
    }
    assert!(
        flipped,
        "the fixture carries neither signature shape — the \
                      corruption was applied to nothing, which must never read \
                      as a passing test"
    );

    let r = verify(
        &envelope(),
        &[],
        &LocalEvidence {
            sigstore: Some((serde_json::to_string(&v).expect("reserialise"), policy())),
            ..LocalEvidence::default()
        },
    );
    assert!(
        !r.checks
            .iter()
            .any(|c| c.name == "identity" && matches!(c.outcome, Outcome::Pass { .. })),
        "a corrupted bundle reported a passing identity: {:?}",
        names(&r)
    );
}
