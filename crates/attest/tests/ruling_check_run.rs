//! The operator's ruling on the check run's conclusion (2026-09-04).
//!
//! # The defect this file was written against, measured before it was fixed
//!
//! A receipt whose only claim was decided **`unsupported`** produced a check
//! run concluding `success` and a green `verified` badge. The receipt-level
//! checks — signature, subject, chain, policy digest — all passed, because the
//! receipt *is* an honest record of a session in which a claim went unproved.
//! But `conclusion: "success"` on a pull request does not read as "the
//! signature verified"; it reads as **"this change is fine"**, and the product's
//! own selftest asserted that green as expected behaviour
//! (`packaging/action/selftest.py`, scenario 7, before this change).
//!
//! That is §6's `unverifiable` → `supported` collapse wearing a second face:
//! *absence of a failing check read as success.*
//!
//! # The ruling, and where each point is enforced
//!
//! 1. `thruline verify` stays **integrity-only and non-blocking**; `unsupported`
//!    does not change its exit code. Held at
//!    `crates/attest/tests/adversary.rs::an_unverifiable_verdict_is_never_reported_as_verified`
//!    and by [`ruling_1_verify_itself_stays_integrity_only`] here.
//! 2. The check run may **never** conclude `success` while any claim is
//!    `unsupported` — [`ruling_2_an_unsupported_claim_can_never_conclude_success`].
//! 3. With a policy file present, the conclusion comes from the policy —
//!    [`ruling_3_the_policy_conclusion_decides_when_one_is_supplied`].
//! 4. Without a policy file, an unsupported claim yields **`neutral` with the
//!    verdict table rendered** —
//!    [`ruling_4_without_a_policy_an_unsupported_claim_is_neutral_with_its_table`].
//! 5. `failure` comes only from policy —
//!    [`ruling_5_the_claim_axis_alone_never_produces_failure`].
//! 6. `verify --json` carries a top-level verdict summary — the CLI half lives
//!    in `crates/cli/tests/m2_action.rs`; the shape is fixed here by
//!    [`ruling_6_the_summary_counts_every_verdict_and_states_the_answer`].

use thruline_attest::dsse::{key_id, sign, Envelope};
use thruline_attest::report::{
    badge_of, check_run, conclusion_of, markdown, PolicyConclusion, VerdictSummary,
};
use thruline_attest::statement::{
    AssertedBy, CheckEntry, ClaimEntry, Predicate, Session, Statement, Subject, VerdictEntry,
    VerifierInfo, PAYLOAD_TYPE,
};
use thruline_attest::verify::{verify, LocalEvidence, Report, Verified};

const TREE: &str = "0123456789abcdef0123456789abcdef01234567";
const COMMIT: &str = "fedcba9876543210fedcba9876543210fedcba98";
const HEAD: &str = "sha256:aaaabbbbccccdddd";
const POLICY: &str = "sha256:0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f";

fn key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[7u8; 32])
}

/// A receipt with one claim, decided `verdict`.
fn statement_with(verdict: &str) -> Statement {
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
            // **The check the verdict cites.** These fixtures carried no checks
            // at all, so a `supported` verdict in one asserted support from a
            // record it did not include. Nothing tried to re-derive it, so it
            // read as clean; the moment something did, the premise of ruling 3
            // ("the receipt IS a good receipt") stopped holding. The fixture was
            // what was wrong.
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
                verdict: verdict.into(),
                verifier: "tests_pass@1".into(),
                evidence: vec!["chk_1".into()],
                reasons: vec!["no test check was bound to the final tree".into()],
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

fn envelope_of(s: &Statement, k: &ed25519_dalek::SigningKey) -> Envelope {
    sign(
        &serde_json::to_vec(s).expect("serialise"),
        PAYLOAD_TYPE,
        k,
        Some(key_id(&k.verifying_key())),
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

/// A fully clean **integrity** report over a receipt whose claim was decided
/// `verdict`. Every check passes; only the claim inside is at issue.
fn clean_report(verdict: &str) -> (Report, Statement) {
    let k = key();
    let s = statement_with(verdict);
    let env = envelope_of(&s, &k);
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    (r, s)
}

// ---------------------------------------------------------------------------
// 1. `verify` itself is unchanged: integrity-only, non-blocking.
// ---------------------------------------------------------------------------

#[test]
fn ruling_1_verify_itself_stays_integrity_only() {
    // The premise the whole ruling rests on: the receipt IS a good receipt.
    // `verify`'s business is "do the receipt's checks reproduce here", and an
    // honestly recorded `unsupported` verdict reproduces perfectly.
    let (r, _) = clean_report("unsupported");
    assert!(
        r.ok(),
        "`unsupported` began blocking verification, which the ruling forbids: {r:#?}"
    );
    assert_eq!(r.verified(), Verified::Yes);
    assert_eq!(r.verified().exit_code(), 0);

    // And an `unverifiable` verdict still blocks it, because there the receipt
    // says in writing that it could not decide.
    let (r, _) = clean_report("unverifiable");
    assert_eq!(r.verified(), Verified::Partial);
}

// ---------------------------------------------------------------------------
// 2. The check run may NEVER conclude `success` over an unsupported claim.
// ---------------------------------------------------------------------------

#[test]
fn ruling_2_an_unsupported_claim_can_never_conclude_success() {
    let (r, s) = clean_report("unsupported");
    // The premise, asserted so this test cannot pass for the wrong reason: the
    // integrity axis is entirely green, so `success` is what the old code
    // produced and what a reader would otherwise see.
    assert_eq!(r.verified(), Verified::Yes, "the premise no longer holds");

    let payload = check_run(&r, Some(&s), Some("cafebabe"), None);
    assert_ne!(
        payload["conclusion"], "success",
        "a receipt whose only claim is `unsupported` concluded success: {payload:#}"
    );
    assert_eq!(payload["conclusion"], "neutral", "{payload:#}");

    // The badge is a rendering of the same conclusion and must not say
    // `verified` either — a badge that only ever shows green is a decoration.
    let c = conclusion_of(&r, Some(&s), None);
    assert_eq!(c.as_str(), "neutral");
    let svg = badge_of(c);
    assert!(
        !svg.contains(">verified<"),
        "the badge over an unsupported claim reads verified: {svg}"
    );
    assert!(svg.contains(">partial<"), "{svg}");
}

// ---------------------------------------------------------------------------
// 3. With a policy present, the conclusion comes from the policy.
// ---------------------------------------------------------------------------

#[test]
fn ruling_3_the_policy_conclusion_decides_when_one_is_supplied() {
    // A policy that fails turns a receipt whose integrity is perfect and whose
    // claims are all supported into a red check run. Nothing else in this
    // system can produce `failure` over a claim.
    let (r, s) = clean_report("supported");
    assert_eq!(r.verified(), Verified::Yes, "the premise no longer holds");
    let payload = check_run(&r, Some(&s), None, Some(PolicyConclusion::Failure));
    assert_eq!(payload["conclusion"], "failure", "{payload:#}");

    // A policy that concludes `neutral` caps a green receipt at neutral.
    let payload = check_run(&r, Some(&s), None, Some(PolicyConclusion::Neutral));
    assert_eq!(payload["conclusion"], "neutral", "{payload:#}");

    // And a policy that passes cannot LIFT anything: ruling 2 is absolute, so
    // `success` from the policy over an unsupported claim is still neutral.
    // The combination is worst-of, never last-writer-wins.
    let (r, s) = clean_report("unsupported");
    let payload = check_run(&r, Some(&s), None, Some(PolicyConclusion::Success));
    assert_eq!(
        payload["conclusion"], "neutral",
        "a passing policy lifted an unsupported claim to success: {payload:#}"
    );
}

// ---------------------------------------------------------------------------
// 4. Without a policy: neutral, WITH the verdict table rendered.
// ---------------------------------------------------------------------------

#[test]
fn ruling_4_without_a_policy_an_unsupported_claim_is_neutral_with_its_table() {
    let (r, s) = clean_report("unsupported");
    let payload = check_run(&r, Some(&s), None, None);
    assert_eq!(payload["conclusion"], "neutral", "{payload:#}");

    // "with the verdict table rendered" is half the ruling: a neutral with no
    // explanation is a shrug, and the reviewer cannot act on it.
    let text = payload["output"]["text"].as_str().unwrap_or_default();
    assert!(
        text.contains("| claim | verdict | why |"),
        "the verdict table is missing from the check run: {text}"
    );
    assert!(
        text.contains("**unsupported**"),
        "the unsupported claim is not marked in the table: {text}"
    );
    assert!(
        text.contains("All tests pass."),
        "the claim's own sentence is missing: {text}"
    );
    let summary = payload["output"]["summary"].as_str().unwrap_or_default();
    assert!(
        summary.contains("unsupported"),
        "the summary does not name the unsupported claim: {summary}"
    );

    // The same table is what the PR comment renders.
    let md = markdown(&r, Some(&s));
    assert!(md.contains("**unsupported**"), "{md}");
}

// ---------------------------------------------------------------------------
// 5. `failure` comes only from policy.
// ---------------------------------------------------------------------------

#[test]
fn ruling_5_the_claim_axis_alone_never_produces_failure() {
    // Every verdict word the published schema allows, over a perfect integrity
    // report and with no policy. None may reach `failure`: deciding that an
    // unproved claim may not merge is the repository's call, written in
    // `.thruline/policy.yml`, not this crate's.
    for verdict in ["supported", "unsupported", "unverifiable"] {
        let (r, s) = clean_report(verdict);
        let payload = check_run(&r, Some(&s), None, None);
        assert_ne!(
            payload["conclusion"], "failure",
            "verdict {verdict:?} produced `failure` with no policy behind it: {payload:#}"
        );
    }

    // **A word outside the published set is a different axis, and it now
    // concludes `failure`.**
    //
    // This loop used to include `"invented_by_a_future_build"` and `""` and
    // require them to be neutral. They are no longer claim-axis outcomes at
    // all: `spec/execution-check-v0.1.schema.json` publishes `verdict` as
    // required, non-null and closed over three lowercase words, and says a
    // consumer meeting anything else must refuse the receipt. It is refused at
    // `statement_schema`, before a single field of the predicate is read — so
    // the `failure` here comes from the receipt being unreadable, not from a
    // claim going unproved, and ruling 5 is untouched by it.
    //
    // The alternative was to keep folding unknown words into `other` and
    // report neutral. That reads a document this build does not understand as
    // "nothing contradicts it", which is §6's collapse with an extra step, and
    // it moves a verdict UP. Recorded for the operator as a decision taken
    // alone.
    for verdict in ["invented_by_a_future_build", "", "Unverifiable"] {
        let (r, s) = clean_report(verdict);
        let payload = check_run(&r, Some(&s), None, None);
        assert_eq!(
            payload["conclusion"], "failure",
            "verdict {verdict:?} was not refused at the schema door: {payload:#}"
        );
        assert!(
            payload["output"]["summary"]
                .as_str()
                .unwrap_or_default()
                .contains("statement_schema"),
            "the refusal must be attributed to the schema, not to the claim axis: {payload:#}"
        );
    }

    // The claim axis on its own, at the type level, so the property is not an
    // accident of which integrity report was paired with it.
    for verdict in ["unsupported", "unverifiable", "nonsense", ""] {
        let s = statement_with(verdict);
        let ceiling = VerdictSummary::of(Some(&s)).ceiling();
        assert_ne!(
            ceiling,
            Verified::No,
            "the claim axis produced a failing ceiling for {verdict:?}"
        );
    }

    // An integrity FAILURE is a different axis and still concludes `failure` —
    // otherwise this ruling would have quietly downgraded a contradicted
    // receipt, which moves a verdict UP.
    let k = key();
    let s = statement_with("supported");
    let env = envelope_of(&s, &k);
    let r = verify(
        &env,
        &[k.verifying_key()],
        &LocalEvidence {
            tree_hashes: ["9999999999999999999999999999999999999999".to_string()]
                .into_iter()
                .collect(),
            ..honest_local()
        },
    );
    assert_eq!(r.verified(), Verified::No);
    let payload = check_run(&r, Some(&s), None, None);
    assert_eq!(
        payload["conclusion"], "failure",
        "a contradicted receipt stopped being a failure: {payload:#}"
    );
}

// ---------------------------------------------------------------------------
// 6. The verdict summary: stated, complete, and not derivable by counting.
// ---------------------------------------------------------------------------

#[test]
fn ruling_6_the_summary_counts_every_verdict_and_states_the_answer() {
    let s = statement_with("unsupported");
    let sum = VerdictSummary::of(Some(&s));
    assert_eq!(sum.total, 1);
    assert_eq!(sum.unsupported, 1);
    assert!(!sum.all_supported);
    assert_eq!(sum.unsupported_claim_ids, vec!["clm_1".to_string()]);

    // **The counts must partition the total.** A consumer that adds up the
    // three §3.4 words and compares with `total` must never find a remainder
    // it has to guess at — an unrecognised verdict word lands in `other` and
    // is named, rather than vanishing and making the sum look complete.
    for verdict in [
        "supported",
        "unsupported",
        "unverifiable",
        "invented_by_a_future_build",
        "",
    ] {
        let s = statement_with(verdict);
        let sum = VerdictSummary::of(Some(&s));
        assert_eq!(
            sum.supported + sum.unsupported + sum.unverifiable + sum.other,
            sum.total,
            "the counts do not partition the total for {verdict:?}: {sum:?}"
        );
        assert_eq!(
            sum.all_supported,
            sum.total > 0 && sum.supported == sum.total,
            "`all_supported` disagrees with the counts for {verdict:?}: {sum:?}"
        );
    }

    // An unrecognised word is named, not silently absorbed.
    let sum = VerdictSummary::of(Some(&statement_with("invented_by_a_future_build")));
    assert_eq!(sum.other, 1);
    assert!(
        sum.other_verdicts
            .contains(&"invented_by_a_future_build".to_string()),
        "{sum:?}"
    );
    assert_ne!(sum.ceiling(), Verified::Yes, "{sum:?}");

    // A receipt with no claims at all: nothing to cap, and nothing to pretend.
    let mut s = statement_with("supported");
    s.predicate.claims.clear();
    s.predicate.verdicts.clear();
    let sum = VerdictSummary::of(Some(&s));
    assert_eq!(sum.total, 0);
    assert!(
        !sum.all_supported,
        "no claims is not `all supported`: {sum:?}"
    );
    assert_eq!(sum.ceiling(), Verified::Yes);
}
