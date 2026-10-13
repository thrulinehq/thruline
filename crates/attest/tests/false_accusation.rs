//! Two ways this build calls an honest receipt CONTRADICTED, measured.
//!
//! # Why these are tests and not fixes
//!
//! §13.1: *"accusing honest users of gaming ... is the most expensive error
//! this product can make."* Both constructions below produce
//! `conclusion: "failure"` — a red check run, a red badge, and the message
//! *"A check contradicted the receipt"* — over a receipt nothing contradicted.
//!
//! **Neither is fixed here, and that is deliberate.** Turning either `Fail`
//! into `Skipped` moves a verdict UP (red → amber), and this session's standing
//! rule with the operator absent is to take only the option that can move a
//! verdict DOWN. So the current polarity is **pinned** rather than corrected:
//! these tests fail the moment anyone changes it, and whoever that is reads
//! this file and learns there is a ruling to ask for.
//!
//! **They are characterisation tests. They assert what the build does, not
//! what it should do.** When the operator rules, delete the test, do not edit
//! its expectation.
//!
//! ## F1 — a pull request that edits `.thruline/policy.yml` reddens every
//! receipt signed before the edit
//!
//! Introduced by `b273136`. Before it, the sole producer wrote
//! `policy_hash: ""`, which took a `NotApplicable` arm, so the mismatch branch
//! was **unreachable in practice**; that commit made every receipt state a real
//! digest and made the branch live. Editing a policy file is the ordinary thing
//! a repository does, and the receipt in the same PR was signed under the old
//! one.
//!
//! It has already bitten inside this repository: `packaging/action/selftest.py`
//! built the demo's receipt, then wrote a policy, then expected the job to go
//! red — and the red came from **this check**, not from the policy gate the
//! scenario was written to watch. A broken §9 gate would have passed that
//! scenario. The harness now rebuilds the receipt under the policy in force.
//!
//! ## F2 — owning a ThruLine ledger reddens someone else's receipt
//!
//! Standing since `b026ff9`, not a regression. M2's own done-when is *"a
//! receipt signed on machine A verifies on machine B with only the repo and
//! the receipt."* Machine B, if it has ever run ThruLine, holds a ledger — and
//! a ledger without that session takes the `Fail` arm. A reviewer with **no**
//! ledger gets the honest `neutral`. So the product is hardest on the reviewer
//! most likely to be using it.

use thruline_attest::report::check_run;
use thruline_attest::statement::{
    AssertedBy, CheckEntry, ClaimEntry, Predicate, Session, Statement, Subject, VerdictEntry,
    VerifierInfo, PAYLOAD_TYPE,
};
use thruline_attest::verify::{verify, LocalEvidence, Outcome, Verified};

const TREE: &str = "0123456789abcdef0123456789abcdef01234567";
const COMMIT: &str = "fedcba9876543210fedcba9876543210fedcba98";
const HEAD: &str = "sha256:aaaabbbbccccdddd";
const POLICY: &str = "sha256:0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f";

fn key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[7u8; 32])
}

fn honest_statement() -> Statement {
    let mut digest = std::collections::BTreeMap::new();
    digest.insert("gitTree".to_string(), TREE.to_string());
    digest.insert("gitCommit".to_string(), COMMIT.to_string());
    Statement::new(
        vec![Subject {
            name: "git+/repo@HEAD".into(),
            digest,
        }],
        Predicate {
            session: Session {
                id: "s1".into(),
                harness: "claude-code".into(),
                event_chain_head: HEAD.into(),
                event_count: 12,
                ..Default::default()
            },
            claims: vec![ClaimEntry {
                id: "clm_1".into(),
                kind: "tests_pass".into(),
                text: "All tests pass.".into(),
                asserted_by: AssertedBy {
                    harness: "claude-code".into(),
                    model: None,
                },
            }],
            // **The check the verdict cites, actually present.**
            //
            // This fixture used to name `chk_1` as its evidence and carry no
            // checks at all — an "honest" receipt asserting support from a
            // record it did not include. It passed only while nothing tried to
            // re-derive the verdict; the moment something did, the control went
            // amber and read as a false accusation. It was the fixture that was
            // wrong.
            checks: vec![CheckEntry {
                id: "chk_1".into(),
                category: "test".into(),
                runner: "pytest".into(),
                argv_hash: "sha256:a".into(),
                env_hash: "sha256:b".into(),
                cwd_hash: "sha256:c".into(),
                input_tree_hash: TREE.into(),
                exit_code: 0,
                parsed: serde_json::json!({"tests_total": 4, "passed": 4, "failed": 0}),
                reexecuted: false,
                container_image_digest: None,
            }],
            verdicts: vec![VerdictEntry {
                claim_id: "clm_1".into(),
                verdict: "supported".into(),
                verifier: "tests_pass@1".into(),
                evidence: vec!["chk_1".into()],
                reasons: vec!["ran on the final tree".into()],
                warnings: vec![],
                findings: vec![],
            }],
            verifier: VerifierInfo {
                thruline_version: "0.1.0".into(),
                mode: "local".into(),
                policy_hash: Some(POLICY.into()),
            },
            ..Default::default()
        },
    )
}

fn honest_local() -> LocalEvidence {
    LocalEvidence {
        have_objects: true,
        tree_hashes: [TREE.to_string()].into_iter().collect(),
        commits: [COMMIT.to_string()].into_iter().collect(),
        recomputed_chain: Some((HEAD.to_string(), 12)),
        ledger_present: true,
        shallow: false,
        policy_hash: Some(POLICY.to_string()),
        content_filters: None,
        // The head under review IS the commit this receipt is about, and the
        // tree being reviewed is the tree the checks ran on. That is what makes
        // this machine *honest* about the new binding axis rather than silent
        // on it: `Default` would leave it unresolved, which is a `Skipped`, and
        // a fixture that skips a check is not a fixture that passes it.
        head_commit: Some(COMMIT.to_string()),
        receipt_is_ancestor_of_head: Some(true),
        // Deliberately false, so the ancestry path is what these fixtures
        // exercise rather than the working-tree shortcut.
        subject_is_working_tree: false,
        head_tree_differs_at: Some(Vec::new()),
        receipt_path_in_repo: None,
        // Track B's keyless path is not what these fixtures are about, and
        // `None` says so explicitly rather than through `..Default::default()`
        // — a fixture that quietly opts into a new verification path hides the
        // axis it was written for.
        sigstore: None,
        // F12: no key here is inside a repository under review — these
        // fixtures are about other axes, and a fixture that quietly opted
        // into the new neutral would hide whichever axis it was written for.
        verifying_key_in_repo: None,
        ledger_checks: Some([("chk_1".to_string(), (0, TREE.to_string()))].into()),
    }
}

fn envelope(s: &Statement) -> thruline_attest::dsse::Envelope {
    let k = key();
    thruline_attest::dsse::sign(
        &serde_json::to_vec(s).expect("serialise"),
        PAYLOAD_TYPE,
        &k,
        Some(thruline_attest::dsse::key_id(&k.verifying_key())),
    )
}

fn outcome_of<'a>(r: &'a thruline_attest::verify::Report, name: &str) -> &'a Outcome {
    &r.checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no check named {name}: {r:#?}"))
        .outcome
}

#[test]
fn f1_editing_the_policy_file_reddens_the_receipt_signed_before_the_edit() {
    let k = key();
    let env = envelope(&honest_statement());

    // The control: same policy on both sides, green.
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    assert_eq!(
        r.verified(),
        Verified::Yes,
        "the control is not green: {r:#?}"
    );

    // The repository edited `.thruline/policy.yml`. Nothing about the session
    // changed; the receipt is byte-identical and still validly signed.
    let r = verify(
        &env,
        &[k.verifying_key()],
        &LocalEvidence {
            policy_hash: Some("sha256:1111111111111111111111111111111111111111".into()),
            ..honest_local()
        },
    );
    assert!(
        matches!(outcome_of(&r, "policy_hash"), Outcome::Fail { .. }),
        "F1 has been fixed or changed. Do NOT edit this expectation — delete \
         this test and record the ruling: {r:#?}"
    );
    let payload = check_run(&r, None, None, None);
    assert_eq!(
        payload["conclusion"], "failure",
        "F1: the conclusion moved. Read this file's module docs before touching it."
    );
    assert!(
        payload["output"]["title"]
            .as_str()
            .is_some_and(|t| t.contains("NOT verified")),
        "{payload:#}"
    );
}

#[test]
fn f2_owning_a_ledger_reddens_a_receipt_from_another_machine() {
    let k = key();
    let env = envelope(&honest_statement());

    // A reviewer with NO ledger — the honest answer, and the one M2's
    // done-when describes.
    let r = verify(
        &env,
        &[k.verifying_key()],
        &LocalEvidence {
            recomputed_chain: None,
            ledger_present: false,
            ..honest_local()
        },
    );
    assert!(matches!(
        outcome_of(&r, "chain_head"),
        Outcome::Skipped { .. }
    ));
    assert_eq!(
        check_run(&r, None, None, None)["conclusion"],
        "neutral",
        "{r:#?}"
    );

    // The SAME receipt on a machine that has run ThruLine at all. Its ledger
    // holds its own sessions and not this one, which is true of every machine
    // in the world except the issuer's.
    let r = verify(
        &env,
        &[k.verifying_key()],
        &LocalEvidence {
            recomputed_chain: None,
            ledger_present: true,
            ..honest_local()
        },
    );
    assert!(
        matches!(outcome_of(&r, "chain_head"), Outcome::Fail { .. }),
        "F2 has been fixed or changed. Do NOT edit this expectation — delete \
         this test and record the ruling: {r:#?}"
    );
    assert_eq!(
        check_run(&r, None, None, None)["conclusion"],
        "failure",
        "F2: the conclusion moved. Read this file's module docs before touching it."
    );
}
