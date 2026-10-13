//! F12 / operator ruling 8 — a signing key committed in the gated repository
//! verifies to `neutral`, with the reason stated.
//!
//! > *"`neutral` is the hosted App's ceiling. A repo-committed signing key
//! > verifies to `neutral` with the reason stated. `success` requires a
//! > signature whose identity the policy names: a keyless OIDC identity via B,
//! > or a policy-listed public key held outside the repository. Core implements
//! > the repo-committed-key half of F12."*
//!
//! # The attack, and why the signature check cannot see it
//!
//! A repository commits `signing.key` and `signing.pub`, signs its own receipt,
//! and the Action verifies with the committed public half. Every existing check
//! passes honestly: the signature really does verify over the PAE, the payload
//! type is right, the trees and the chain are what they say. And the answer is
//! worth nothing, because everyone who can open a pull request against that
//! repository can produce the same signature.
//!
//! This is `verify.rs`'s own sentence, one layer out: *"a signature says who
//! wrote the document, never whether the checks passed."* If the key is in the
//! repository, it does not even say who.
//!
//! **So the new check SKIPS rather than fails.** The cryptography is fine, and
//! a `Fail` would be a false statement about the artefact — the error §13.1
//! calls the most expensive this product can make. `Skipped` is
//! `Verified::Partial` is `Conclusion::Neutral`, which is exactly the ceiling
//! ruling 8 sets.

use thruline_attest::report::Conclusion;
use thruline_attest::statement::{
    AssertedBy, ClaimEntry, Predicate, Session, Statement, Subject, VerifierInfo, PAYLOAD_TYPE,
};
use thruline_attest::verify::{verify, LocalEvidence, Outcome, Verified};

const TREE: &str = "0123456789abcdef0123456789abcdef01234567";
const COMMIT: &str = "fedcba9876543210fedcba9876543210fedcba98";
const HEAD: &str = "sha256:aaaabbbbccccdddd";
const POLICY: &str = "sha256:0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f";

fn key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[9u8; 32])
}

fn statement() -> Statement {
    Statement {
        type_: "https://in-toto.io/Statement/v1".into(),
        subject: vec![Subject {
            name: "workspace".into(),
            digest: [
                ("gitTree".to_string(), TREE.to_string()),
                ("gitCommit".to_string(), COMMIT.to_string()),
            ]
            .into_iter()
            .collect(),
        }],
        predicate_type: thruline_attest::statement::PREDICATE_TYPE.into(),
        predicate: Predicate {
            session: Session {
                id: "s".into(),
                harness: "claude-code".into(),
                event_chain_head: HEAD.into(),
                event_count: 12,
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
                policy_hash: Some(POLICY.into()),
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

/// A machine that can answer every other question, so the only thing under test
/// below is where the key lives.
fn local() -> LocalEvidence {
    LocalEvidence {
        have_objects: true,
        tree_hashes: [TREE.to_string()].into_iter().collect(),
        commits: [COMMIT.to_string()].into_iter().collect(),
        recomputed_chain: Some((HEAD.to_string(), 12)),
        ledger_present: true,
        // No `checks` in this statement, so an EMPTY map is the honest answer:
        // the ledger holds this session and it has no check rows to compare.
        // `None` would be "no ledger here", which skips, and a control that
        // skips is not a control.
        ledger_checks: Some(std::collections::BTreeMap::new()),
        shallow: false,
        policy_hash: Some(POLICY.to_string()),
        head_commit: Some(COMMIT.to_string()),
        receipt_is_ancestor_of_head: Some(true),
        subject_is_working_tree: false,
        head_tree_differs_at: Some(Vec::new()),
        verifying_key_in_repo: None,
        ..LocalEvidence::default()
    }
}

fn outcome_of<'a>(r: &'a thruline_attest::verify::Report, name: &str) -> Option<&'a Outcome> {
    r.checks.iter().find(|c| c.name == name).map(|c| &c.outcome)
}

#[test]
fn the_control_a_key_that_is_not_in_the_repository_still_verifies() {
    // Without this, everything below is satisfied by a build that verifies
    // nothing at all.
    let r = verify(&envelope(), &[key().verifying_key()], &local());
    assert_eq!(
        r.verified(),
        Verified::Yes,
        "the control is not green: {r:#?}"
    );
    assert_eq!(Conclusion::from(r.verified()), Conclusion::Success);
    assert!(
        outcome_of(&r, "signing_identity").is_none(),
        "a receipt with no repo-committed key gained an identity check anyway: {r:#?}"
    );
}

#[test]
fn a_key_committed_in_the_gated_repository_verifies_to_neutral() {
    let r = verify(
        &envelope(),
        &[key().verifying_key()],
        &LocalEvidence {
            verifying_key_in_repo: Some(".thruline/signing.pub".into()),
            ..local()
        },
    );

    assert_eq!(
        r.verified(),
        Verified::Partial,
        "a receipt verified by a key committed in the repository under review \
         reached {:?}: {r:#?}",
        r.verified()
    );
    assert_eq!(
        Conclusion::from(r.verified()),
        Conclusion::Neutral,
        "ruling 8 sets neutral as the ceiling for this case"
    );
}

#[test]
fn the_signature_check_itself_still_passes_because_the_signature_is_valid() {
    // The distinction the whole design rests on. Calling a valid signature
    // invalid would be a false statement about the artefact, and §13.1 names
    // accusing an honest user as the most expensive error here. The signature
    // passes; a SEPARATE check abstains on identity.
    let r = verify(
        &envelope(),
        &[key().verifying_key()],
        &LocalEvidence {
            verifying_key_in_repo: Some(".thruline/signing.pub".into()),
            ..local()
        },
    );
    assert!(
        matches!(outcome_of(&r, "signature"), Some(Outcome::Pass { .. })),
        "the signature check stopped passing on a valid signature: {r:#?}"
    );
    assert!(
        r.failed().is_empty(),
        "a repo-committed key produced a FAILING check; ruling 8 says neutral, \
         not failure: {:#?}",
        r.failed()
    );
}

#[test]
fn the_reason_is_stated_and_names_the_path_and_what_success_would_need() {
    // "with the reason stated" is half the ruling. A neutral whose reason a
    // reader cannot find is a neutral they will read as a transient failure and
    // re-run.
    let r = verify(
        &envelope(),
        &[key().verifying_key()],
        &LocalEvidence {
            verifying_key_in_repo: Some(".thruline/signing.pub".into()),
            ..local()
        },
    );
    let Some(Outcome::Skipped { reason }) = outcome_of(&r, "signing_identity") else {
        panic!("no skipped `signing_identity` check: {r:#?}");
    };
    assert!(
        reason.contains(".thruline/signing.pub"),
        "the reason does not say WHICH key: {reason}"
    );
    assert!(
        reason.contains("anyone who can write this repository"),
        "the reason does not say why a valid signature is not enough: {reason}"
    );
    assert!(
        reason.contains("policy names"),
        "the reason does not say what would reach success: {reason}"
    );
}

#[test]
fn a_missing_key_and_a_repo_committed_key_do_not_share_one_sentence() {
    // Both land on neutral, and they are different facts. One says nothing was
    // checked; the other says something was checked and proves nothing. A
    // reader handed the wrong one takes the wrong next step.
    let no_key = verify(&envelope(), &[], &local());
    let in_repo = verify(
        &envelope(),
        &[key().verifying_key()],
        &LocalEvidence {
            verifying_key_in_repo: Some(".thruline/signing.pub".into()),
            ..local()
        },
    );
    assert_eq!(no_key.verified(), Verified::Partial);
    assert_eq!(in_repo.verified(), Verified::Partial);

    let Some(Outcome::Skipped { reason: a }) = outcome_of(&no_key, "signature") else {
        panic!("{no_key:#?}");
    };
    let Some(Outcome::Skipped { reason: b }) = outcome_of(&in_repo, "signing_identity") else {
        panic!("{in_repo:#?}");
    };
    assert_ne!(a, b);
    // The sentence gained a clause when Track B's keyless arm landed: there are
    // now two ways to have nothing to check with. The assertion follows it
    // rather than being loosened — the point is that this reason is DIFFERENT
    // from the repo-committed-key one, not that it never changes.
    assert!(
        a.contains("no verifying key and no Sigstore bundle were supplied"),
        "{a}"
    );
}

// ---------------------------------------------------------------------------
// §5.7 — an open token set, and the rule that makes it safe
// ---------------------------------------------------------------------------

/// The spec's clause, which is the one that is easy to break by accident:
/// *"The token set is open by design, and a consumer that meets one it does not
/// recognise MUST report it rather than drop it — a cause you cannot parse is
/// still a reason the capture was incomplete, and silently discarding it turns
/// an incomplete record into a clean-looking one."*
///
/// That is D18's own failure one layer up: a record that is incomplete, read as
/// one that is fine. This pins the reader against it, because the natural
/// refactor — `match token { known => …, _ => continue }` — is exactly the
/// mistake and looks tidy.
#[test]
fn an_unrecognised_degraded_cause_is_reported_not_dropped() {
    let mut statement = statement();
    statement.predicate.capture = thruline_attest::statement::Capture {
        sources: vec!["claude-code-hook".into()],
        capture_degraded: true,
        degraded_causes: vec![
            // One this build emits, and one from a future producer.
            "hook_events_missing: the Stop hook did not fire for this session".into(),
            "quantum_flux_detected: a cause no version of this reader knows".into(),
        ],
        ..Default::default()
    };
    let k = key();
    let env = thruline_attest::dsse::sign(
        &serde_json::to_vec(&statement).expect("serialise"),
        PAYLOAD_TYPE,
        &k,
        Some(thruline_attest::dsse::key_id(&k.verifying_key())),
    );

    let r = verify(&env, &[k.verifying_key()], &local());
    let Some(Outcome::Skipped { reason }) = outcome_of(&r, "capture_fidelity") else {
        panic!("degraded capture did not skip capture_fidelity: {r:#?}");
    };
    assert!(
        reason.contains("quantum_flux_detected"),
        "an unrecognised cause was dropped, which turns an incomplete record into a \
         clean-looking one: {reason}"
    );
    // And the known one is still there — a reader that reported only the
    // unknown one would pass the assertion above and be just as wrong.
    assert!(reason.contains("hook_events_missing"), "{reason}");
}
