//! §8 and §13 — the receipt adversary.
//!
//! **Every construction here is an evidence attack, not a crypto attack.** None
//! of them forges a signature; each produces a *validly signed* artifact that
//! says something the machine cannot back up. That is the realistic threat: an
//! agent that can run commands can also write a receipt.
//!
//! The failure this suite is built against: **a `verify` that returns success
//! because it never checked** — §6's non-collapse rule wearing a signature.

use std::collections::BTreeSet;

use thruline_attest::dsse::{key_id, pae, sign, Envelope, Signature};
use thruline_attest::statement::{
    AssertedBy, Capture, CheckEntry, ClaimEntry, Predicate, Session, Statement, Subject,
    VerdictEntry, VerifierInfo, PAYLOAD_TYPE,
};
use thruline_attest::verify::{verify, LocalEvidence, EMPTY_TREE_SHA1};

const TREE: &str = "0123456789abcdef0123456789abcdef01234567";
const COMMIT: &str = "fedcba9876543210fedcba9876543210fedcba98";
const HEAD: &str = "sha256:aaaabbbbccccdddd";
/// The policy digest an honest receipt and an honest machine agree on.
const POLICY: &str = "sha256:0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f";

fn key() -> ed25519_dalek::SigningKey {
    // Deterministic, so a failure is reproducible.
    ed25519_dalek::SigningKey::from_bytes(&[7u8; 32])
}

fn statement(tree: &str) -> Statement {
    let mut digest = std::collections::BTreeMap::new();
    digest.insert("gitTree".to_string(), tree.to_string());
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
                // **Stated, because every receipt states one.** A receipt that
                // is silent about its policy is a defective receipt, not a
                // receipt with nothing to check — that reading is the bug this
                // suite exists to hold shut.
                policy_hash: Some(POLICY.into()),
            },
            ..Default::default()
        },
    )
}

fn envelope_of(s: &Statement, k: &ed25519_dalek::SigningKey) -> Envelope {
    let payload = serde_json::to_vec(s).expect("serialise");
    sign(&payload, PAYLOAD_TYPE, k, Some(key_id(&k.verifying_key())))
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

// ---------------------------------------------------------------------------
// The control: an honest receipt verifies
// ---------------------------------------------------------------------------

#[test]
fn an_honest_receipt_verifies_and_every_check_actually_ran() {
    let k = key();
    let env = envelope_of(&statement(TREE), &k);
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    assert!(r.ok(), "{r:#?}");
    // And the checks are real: a report with one check that passed is not the
    // same as a report that checked everything.
    let names: BTreeSet<&str> = r.checks.iter().map(|c| c.name.as_str()).collect();
    for required in [
        "signature",
        "payload_type",
        "statement_schema",
        "subject_tree",
        "subject_commit",
        "chain_head",
        "reexecution",
    ] {
        assert!(
            names.contains(required),
            "{required} was never checked: {names:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// The seven constructions
// ---------------------------------------------------------------------------

#[test]
fn construction_1_the_empty_tree_is_refused_because_it_verifies_everywhere() {
    // `4b825dc642cb…` is byte-identical in every repository on earth, so a
    // receipt about it verifies anywhere and attests to nothing.
    let k = key();
    let env = envelope_of(&statement(EMPTY_TREE_SHA1), &k);
    let mut local = honest_local();
    local.tree_hashes.insert(EMPTY_TREE_SHA1.to_string());
    let r = verify(&env, &[k.verifying_key()], &local);
    assert!(!r.ok());
    assert!(
        r.failed().iter().any(|c| c.name == "subject_tree"),
        "{r:#?}"
    );
}

#[test]
fn construction_2_a_subject_digest_that_is_not_a_local_tree_is_refused() {
    // The statement SAYS what the tree is. That is the claim. Verify recomputes.
    let k = key();
    let env = envelope_of(&statement("1111111111111111111111111111111111111111"), &k);
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    assert!(!r.ok());
    let f = r.failed();
    assert!(f.iter().any(|c| c.name == "subject_tree"), "{r:#?}");
}

#[test]
fn construction_3_a_receipt_replayed_against_another_commit_is_refused() {
    let k = key();
    let env = envelope_of(&statement(TREE), &k);
    // Same signed receipt, a repository that has neither the tree nor the commit.
    let elsewhere = LocalEvidence {
        tree_hashes: ["9999999999999999999999999999999999999999".to_string()]
            .into_iter()
            .collect(),
        commits: ["8888888888888888888888888888888888888888".to_string()]
            .into_iter()
            .collect(),
        ..honest_local()
    };
    let r = verify(&env, &[k.verifying_key()], &elsewhere);
    assert!(!r.ok());
    assert_eq!(r.failed().len(), 2, "{r:#?}");
}

#[test]
fn construction_4_a_tampered_predicate_re_signed_with_a_real_key_is_refused() {
    // The signature is genuinely valid — the attacker owns a key the verifier
    // trusts. What is checked is WHAT was signed.
    let k = key();
    let mut s = statement(TREE);
    s.predicate.verdicts[0].verdict = "supported".into();
    s.predicate.session.event_chain_head = "sha256:something-else-entirely".into();
    let env = envelope_of(&s, &k);
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    assert!(!r.ok());
    assert!(
        r.failed().iter().any(|c| c.name == "chain_head"),
        "a valid signature over a predicate the ledger contradicts was accepted: {r:#?}"
    );
    // The signature check itself still passed — that is the point.
    assert!(r
        .checks
        .iter()
        .any(|c| c.name == "signature" && c.outcome.is_pass()));
}

#[test]
fn construction_5_a_signature_over_a_different_pae_does_not_verify() {
    // Where hand-rolled DSSE fails: a signature over `type ‖ body`, or over the
    // base64 payload, or with the length prefixes omitted.
    use ed25519_dalek::Signer as _;
    let k = key();
    let s = statement(TREE);
    let payload = serde_json::to_vec(&s).expect("serialise");
    let mut env = envelope_of(&s, &k);

    for wrong in [
        // no length prefixes
        [
            b"DSSEv1 ".as_slice(),
            PAYLOAD_TYPE.as_bytes(),
            b" ",
            &payload,
        ]
        .concat(),
        // the raw payload alone
        payload.clone(),
        // the base64 form rather than the raw bytes
        {
            use base64::Engine as _;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&payload);
            pae(PAYLOAD_TYPE, b64.as_bytes())
        },
    ] {
        let sig = k.sign(&wrong);
        env.signatures = vec![Signature {
            sig: {
                use base64::Engine as _;
                base64::engine::general_purpose::STANDARD.encode(sig.to_bytes())
            },
            keyid: None,
        }];
        let r = verify(&env, &[k.verifying_key()], &honest_local());
        assert!(
            !r.ok() && r.failed().iter().any(|c| c.name == "signature"),
            "a signature over a different pre-authentication encoding verified"
        );
    }
}

#[test]
fn construction_6_reexecuted_true_without_a_re_execution_is_refused() {
    // §7 exists now, so the construction is no longer "this build cannot do
    // that" — it is a receipt that SAYS it re-executed and carries nothing
    // that could only have come from a re-execution.
    //
    // The backing is the container image digest, which §7 writes into an `exec`
    // event inside the hash chain this receipt names. A forger can flip the
    // boolean; producing the digest as well means producing the event, which
    // changes `chain_head`.
    let k = key();

    // (a) the boolean alone.
    let mut s = statement(TREE);
    s.predicate.checks[0].reexecuted = true;
    s.predicate.checks[0].container_image_digest = None;
    let env = envelope_of(&s, &k);
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    assert!(!r.ok());
    let reason = r
        .failed()
        .iter()
        .find(|c| c.name == "reexecution")
        .map(|c| format!("{:?}", c.outcome))
        .unwrap_or_else(|| panic!("{r:#?}"));
    assert!(reason.contains("no container image digest"), "{reason}");

    // (b) a digest that is not one. A forger who learns the field exists puts
    //     something in it; a short or unprefixed string is not a digest.
    for bogus in ["", "   ", "yes", "sha256:", "sha256:deadbeef", "0f0d9f07"] {
        let mut s = statement(TREE);
        s.predicate.checks[0].reexecuted = true;
        s.predicate.checks[0].container_image_digest = Some(bogus.to_string());
        let env = envelope_of(&s, &k);
        let r = verify(&env, &[k.verifying_key()], &honest_local());
        assert!(
            !r.ok(),
            "{bogus:?} was accepted as a container image digest"
        );
    }

    // (c) the CONTROL. A well-formed digest passes, so this is a discriminator
    //     and not a blanket refusal — a check that fails everything proves
    //     nothing about the case it was written for.
    let mut s = statement(TREE);
    s.predicate.checks[0].reexecuted = true;
    s.predicate.checks[0].container_image_digest =
        Some("sha256:0f0d9f075ecfcaf8e325a570d2d2fbdd164beba8852c6e74a036c7215259652f".to_string());
    let env = envelope_of(&s, &k);
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    assert!(r.ok(), "an honest re-executed receipt was refused: {r:#?}");

    // (d) and the inverse, so the digest cannot become decorative: a check
    //     carrying one while saying `reexecuted: false` is a record whose two
    //     halves disagree about whether §7 ran.
    let mut s = statement(TREE);
    s.predicate.checks[0].reexecuted = false;
    s.predicate.checks[0].container_image_digest =
        Some("sha256:0f0d9f075ecfcaf8e325a570d2d2fbdd164beba8852c6e74a036c7215259652f".to_string());
    let env = envelope_of(&s, &k);
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    assert!(
        !r.ok(),
        "a record disagreeing with itself was accepted: {r:#?}"
    );
}

#[test]
fn construction_7_a_chain_head_the_ledger_cannot_reproduce_is_refused() {
    let k = key();
    let env = envelope_of(&statement(TREE), &k);

    // (a) the ledger recomputes a different head
    let mut local = honest_local();
    local.recomputed_chain = Some(("sha256:different".into(), 12));
    assert!(verify(&env, &[k.verifying_key()], &local)
        .failed()
        .iter()
        .any(|c| c.name == "chain_head"));

    // (b) the ledger has no such session at all
    let mut local = honest_local();
    local.recomputed_chain = None;
    let r = verify(&env, &[k.verifying_key()], &local);
    assert!(r.failed().iter().any(|c| c.name == "chain_head"), "{r:#?}");

    // (c) TRUNCATION: the head matches because a prefix of a session is a valid
    // chain in its own right — the ledger is a forest of per-session chains.
    // The count is what separates the prefix from the whole.
    let mut local = honest_local();
    local.recomputed_chain = Some((HEAD.to_string(), 7));
    let r = verify(&env, &[k.verifying_key()], &local);
    assert!(
        r.failed()
            .iter()
            .any(|c| c.name == "chain_head" && format!("{:?}", c.outcome).contains("truncated")),
        "a truncated session verified on a matching head: {r:#?}"
    );
}

// ---------------------------------------------------------------------------
// The failure mode the whole file exists for
// ---------------------------------------------------------------------------

#[test]
fn a_check_that_could_not_run_is_never_a_check_that_passed() {
    // "A verify that returns success because it never checked."
    let k = key();
    let env = envelope_of(&statement(TREE), &k);

    // No key: the signature is unchecked. That must not be `ok`.
    let r = verify(&env, &[], &honest_local());
    assert!(!r.ok(), "an unverified signature reported ok: {r:#?}");
    assert!(r.skipped().iter().any(|c| c.name == "signature"));

    // No repository: the subject is unchecked.
    let r = verify(
        &env,
        &[k.verifying_key()],
        &LocalEvidence {
            have_objects: false,
            tree_hashes: BTreeSet::new(),
            commits: BTreeSet::new(),
            ..honest_local()
        },
    );
    assert!(!r.ok(), "an unchecked subject reported ok: {r:#?}");

    // No ledger: the chain head is unchecked, and "no ledger" is distinct from
    // "ledger without the session" — the latter is a FAILURE, this is a skip.
    let r = verify(
        &env,
        &[k.verifying_key()],
        &LocalEvidence {
            recomputed_chain: None,
            ledger_present: false,
            ..honest_local()
        },
    );
    assert!(!r.ok());
    assert!(r.skipped().iter().any(|c| c.name == "chain_head"));
}

#[test]
fn an_empty_report_is_not_ok_because_nothing_was_checked() {
    assert!(!thruline_attest::verify::Report::default().ok());
}

#[test]
fn pae_matches_the_dsse_specification_byte_for_byte() {
    // A verifier that reconstructs PAE differently from the signer fails as
    // "signature invalid", which reads as tampering. So the encoding has its
    // own vector rather than only being exercised through a round trip.
    assert_eq!(
        pae("http://example.com/HelloWorld", b"hello world"),
        b"DSSEv1 29 http://example.com/HelloWorld 11 hello world".to_vec()
    );
    // Empty payload, and a payload containing spaces and newlines: the length
    // prefixes are what make those unambiguous.
    assert_eq!(pae("t", b""), b"DSSEv1 1 t 0 ".to_vec());
    assert_eq!(pae("t", b"a b\nc"), b"DSSEv1 1 t 5 a b\nc".to_vec());
}

#[test]
fn an_absent_and_an_empty_policy_hash_are_different_refusals_and_neither_passes() {
    // # The measured defect this replaces
    //
    // `receipt_cmd` wrote `policy_hash: String::new()`, and the verifier's
    // first match arm was `(_, "") => not_applicable`, which did not block
    // `ok()`. Measured 2026-09-04 on a real repository: **a policy rewritten to
    // its literal opposite — `require_receipt`, `require_reexecuted`,
    // `allow_skips`, `stop_gate.enabled` and `reexec.mode` all flipped —
    // produced byte-identical `Verified.` output at exit 0.**
    //
    // The test that stood here asserted `r.ok()` for a receipt silent about
    // policy. It was the bug, written down as an expectation, and it passed.
    use thruline_attest::verify::Outcome;
    let k = key();
    let mut local = honest_local();
    local.policy_hash = Some("sha256:whatever".into());

    // ABSENT: the receipt does not say. Every session is judged under SOME
    // policy, so silence is a missing assertion — a fail, with its own reason.
    let mut s = statement(TREE);
    s.predicate.verifier.policy_hash = None;
    let env = envelope_of(&s, &k);
    let r = verify(&env, &[k.verifying_key()], &local);
    let absent = r
        .checks
        .iter()
        .find(|c| c.name == "policy_hash")
        .expect("policy_hash check")
        .clone();
    assert!(
        matches!(&absent.outcome, Outcome::Fail { reason } if reason.contains("states NO policy")),
        "{absent:#?}"
    );
    assert!(!r.ok(), "a receipt silent about its policy reported ok");

    // EMPTY: a producer wrote a placeholder. Also refused, and — this is the
    // point — under a DIFFERENT reason, so the two states can never again be
    // read as one thing.
    let mut s = statement(TREE);
    s.predicate.verifier.policy_hash = Some(String::new());
    let env = envelope_of(&s, &k);
    let r = verify(&env, &[k.verifying_key()], &local);
    let empty = r
        .checks
        .iter()
        .find(|c| c.name == "policy_hash")
        .expect("policy_hash check")
        .clone();
    assert!(
        matches!(&empty.outcome, Outcome::Fail { reason } if reason.contains("EMPTY policy hash")),
        "{empty:#?}"
    );
    assert!(!r.ok(), "a receipt with an empty policy hash reported ok");
    assert_ne!(
        absent.outcome, empty.outcome,
        "absent and empty share a representation again"
    );

    // STATED, machine has none → skipped, and NOT ok.
    let env = envelope_of(&statement(TREE), &k);
    let mut local = honest_local();
    local.policy_hash = None;
    let r = verify(&env, &[k.verifying_key()], &local);
    assert!(!r.ok(), "an unchecked policy assertion reported ok: {r:#?}");
    assert!(r.skipped().iter().any(|c| c.name == "policy_hash"));

    // STATED, machine has a DIFFERENT one → fail. This is the mutation
    // experiment, in a unit test.
    let mut local = honest_local();
    local.policy_hash = Some("sha256:local".into());
    let r = verify(&env, &[k.verifying_key()], &local);
    assert!(r.failed().iter().any(|c| c.name == "policy_hash"), "{r:#?}");
}

#[test]
fn there_is_no_outcome_that_fails_to_block_ok() {
    // The generalisation, as a control on the whole enum. `NotApplicable` used
    // to be a fourth outcome that did NOT block `ok()`, and its single producer
    // was the empty-policy arm above. It is gone, and this asserts the
    // property that made removing it worth doing: for every check, in every
    // report this suite can construct, "did not pass" and "blocks" are the same
    // fact.
    use thruline_attest::verify::Outcome;
    let k = key();
    let env = envelope_of(&statement(TREE), &k);
    for local in [
        LocalEvidence {
            tree_hashes: BTreeSet::new(),
            commits: BTreeSet::new(),
            ..honest_local()
        },
        LocalEvidence {
            recomputed_chain: None,
            ledger_present: false,
            ..honest_local()
        },
        LocalEvidence {
            policy_hash: None,
            ..honest_local()
        },
        LocalEvidence {
            content_filters: Some("filter drivers [launder]".into()),
            ..honest_local()
        },
    ] {
        let r = verify(&env, &[], &local);
        assert!(!r.ok(), "{r:#?}");
        for c in &r.checks {
            assert_eq!(
                c.outcome.blocks(),
                !matches!(c.outcome, Outcome::Pass { .. }),
                "an outcome that is not a pass and does not block: {c:?}"
            );
        }
    }
}

#[test]
fn a_content_filter_keeps_the_receipt_out_of_verified_at_both_ends() {
    // §0.3 binds everything to a tree hash, and a `.gitattributes` clean filter
    // breaks that binding at its root: the tree names the FILTERED bytes while
    // the working tree holds the originals.
    //
    // Measured 2026-09-04: `thruline receipt` called the snapshotter, the
    // snapshotter printed `WARN snapshot taken under content filters: the tree
    // hash may not name the bytes on disk`, the finding was discarded, and
    // `verify` printed `Verified. Every check the receipt asserts was confirmed
    // here.` **eight lines below that warning**, at exit 0.
    use thruline_attest::verify::{Outcome, Verified};
    let k = key();

    // (a) the RECEIPT records degraded capture.
    let mut s = statement(TREE);
    s.predicate.capture = Capture {
        capture_degraded: true,
        content_filters: Some("filter drivers [launder] declared in .gitattributes".into()),
        ..Default::default()
    };
    let env = envelope_of(&s, &k);
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    assert!(!r.ok(), "{r:#?}");
    assert_eq!(r.verified(), Verified::Partial);
    assert_ne!(r.verified().exit_code(), 0);
    assert!(r.skipped().iter().any(|c| c.name == "capture_fidelity"));

    // (b) THIS MACHINE has one in scope, so the subject_tree pass above is not
    // evidence about the bytes on disk.
    let env = envelope_of(&statement(TREE), &k);
    let local = LocalEvidence {
        content_filters: Some("filter drivers [launder] declared in .gitattributes".into()),
        ..honest_local()
    };
    let r = verify(&env, &[k.verifying_key()], &local);
    assert!(!r.ok(), "{r:#?}");
    assert_ne!(r.verified().exit_code(), 0);

    // (c) A receipt naming filters while claiming clean capture disagrees with
    // itself — that is a FAIL, not a skip, the same way `reexecuted: false`
    // with a container digest is.
    let mut s = statement(TREE);
    s.predicate.capture = Capture {
        capture_degraded: false,
        content_filters: Some("filter drivers [launder]".into()),
        ..Default::default()
    };
    let env = envelope_of(&s, &k);
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    assert!(matches!(
        r.checks
            .iter()
            .find(|c| c.name == "capture_fidelity")
            .map(|c| &c.outcome),
        Some(Outcome::Fail { .. })
    ));

    // The control: a receipt with neither passes this check, so the check is
    // not simply always-blocking.
    let env = envelope_of(&statement(TREE), &k);
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    assert!(r.ok(), "the honest control stopped verifying: {r:#?}");
}

#[test]
fn an_unverifiable_verdict_is_never_reported_as_verified() {
    // §6: never collapse `unverifiable` into `supported`. A receipt whose only
    // verdict read `unverifiable — no test check was captured in this session`
    // verified clean at exit 0 under the sentence "Every check the receipt
    // asserts was confirmed here." That is the collapse in the most visible
    // place the product has.
    use thruline_attest::verify::Verified;
    let k = key();
    let mut s = statement(TREE);
    s.predicate.verdicts[0].verdict = "unverifiable".into();
    let env = envelope_of(&s, &k);
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    assert!(!r.ok(), "{r:#?}");
    assert_eq!(r.verified(), Verified::Partial);
    assert_ne!(r.verified().exit_code(), 0);
    assert!(r.skipped().iter().any(|c| c.name == "verdicts"));

    // `unsupported` is NOT this command's business — §9's gate decides whether
    // it may merge, and an honestly recorded `unsupported` is a correct
    // receipt. If this ever starts blocking, the gate has moved into `verify`.
    let mut s = statement(TREE);
    s.predicate.verdicts[0].verdict = "unsupported".into();
    let env = envelope_of(&s, &k);
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    assert!(r.ok(), "`unsupported` began blocking verification: {r:#?}");
}

// ---------------------------------------------------------------------------
// The JSON surface, and every subject
// ---------------------------------------------------------------------------

#[test]
fn the_json_states_the_conclusion_rather_than_leaving_it_to_be_derived() {
    // The checks array alone collapses the three-way distinction: a consumer
    // reading `all(c.outcome != "fail")` — the obvious jq idiom — reads a
    // partially-verified receipt as verified while the process exits 2. Every
    // downstream consumer is the surface where §6's non-collapse rule gets
    // undone, so the conclusion is stated.
    use thruline_attest::verify::Verified;
    let k = key();
    let env = envelope_of(&statement(TREE), &k);

    // Full.
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    let j = serde_json::to_value(&r).expect("json");
    assert_eq!(j["verified"], serde_json::json!(true));
    assert_eq!(j["status"], serde_json::json!("verified"));
    assert_eq!(j["exit"], serde_json::json!(0));
    assert_eq!(r.verified(), Verified::Yes);

    // Partial — the case the naive idiom gets wrong.
    let mut local = honest_local();
    local.recomputed_chain = None;
    local.ledger_present = false;
    let r = verify(&env, &[k.verifying_key()], &local);
    let j = serde_json::to_value(&r).expect("json");
    assert_eq!(j["verified"], serde_json::json!(false));
    assert_eq!(j["status"], serde_json::json!("partial"));
    assert_eq!(j["exit"], serde_json::json!(2));
    // The naive idiom would have said yes; the stated conclusion does not.
    let naive_says_ok = j["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .all(|c| c["outcome"] != "fail");
    assert!(naive_says_ok, "the premise of the finding no longer holds");

    // Failed.
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
    let j = serde_json::to_value(&r).expect("json");
    assert_eq!(j["verified"], serde_json::json!(false));
    assert_eq!(j["status"], serde_json::json!("failed"));
    assert_eq!(j["exit"], serde_json::json!(1));
}

#[test]
fn the_truthy_reading_of_every_wire_field_agrees_with_the_conclusion() {
    // The finding this closes: the field used to be a tri-state STRING named
    // `verified`, and `"partial"` and `"no"` are both truthy in Python and in
    // JavaScript. `if r["verified"]:` — which is exactly what that field name
    // invites — read a partially-verified and a FAILED receipt as success.
    // That is the same collapse §6 forbids, re-entering through a field name.
    //
    // So `verified` is a boolean and the tri-state moved to `status`. This test
    // reads every conclusion the way a script would, and asserts the truthy
    // reading is right in all three.
    use thruline_attest::verify::Verified;
    let k = key();
    let env = envelope_of(&statement(TREE), &k);

    let mut partial = honest_local();
    partial.recomputed_chain = None;
    partial.ledger_present = false;
    let failing = LocalEvidence {
        tree_hashes: ["9999999999999999999999999999999999999999".to_string()]
            .into_iter()
            .collect(),
        ..honest_local()
    };

    for (local, expect_true, expect_status, expect_exit) in [
        (honest_local(), true, "verified", 0),
        (partial, false, "partial", 2),
        (failing, false, "failed", 1),
    ] {
        let r = verify(&env, &[k.verifying_key()], &local);
        let j = serde_json::to_value(&r).expect("json");
        let truthy = match &j["verified"] {
            serde_json::Value::Bool(b) => *b,
            // A non-boolean here is the defect itself: a string is truthy in
            // both languages that read this, whatever it says.
            other => panic!(
                "`verified` is {other:?}, not a boolean — a script's \
                             truthiness check would read it as success"
            ),
        };
        assert_eq!(truthy, expect_true, "wrong truthiness for {expect_status}");
        assert_eq!(j["status"], serde_json::json!(expect_status));
        assert_eq!(j["exit"], serde_json::json!(expect_exit));
        assert_eq!(r.verified().is_verified(), expect_true);
    }

    // And the spellings themselves cannot be mistaken for one another by a
    // consumer doing an equality test against the success value.
    let spellings: Vec<String> = [Verified::Yes, Verified::Partial, Verified::No]
        .iter()
        .map(|v| serde_json::to_value(v).expect("json").to_string())
        .collect();
    assert_eq!(spellings.len(), 3);
    assert!(
        spellings
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == 3,
        "two conclusions share a spelling: {spellings:?}"
    );
}

#[test]
fn the_conclusion_and_the_exit_code_come_from_one_place() {
    // If they were computed twice they could drift, and a consumer reading the
    // JSON and one reading `$?` would disagree about the same receipt.
    use thruline_attest::verify::Verified;
    for (v, code) in [
        (Verified::Yes, 0),
        (Verified::No, 1),
        (Verified::Partial, 2),
    ] {
        assert_eq!(v.exit_code(), code);
    }
}

#[test]
fn every_subject_is_checked_and_not_just_the_first() {
    // in-toto statements legitimately carry several subjects, and a reader
    // shown a verified receipt believes all of them were attested. A bogus
    // second subject behind a legitimate first used to verify clean at exit 0.
    let k = key();
    let mut s = statement(TREE);
    let mut bogus = std::collections::BTreeMap::new();
    bogus.insert("gitTree".to_string(), EMPTY_TREE_SHA1.to_string());
    s.subject.push(Subject {
        name: "git+/repo@second".into(),
        digest: bogus,
    });
    let env = envelope_of(&s, &k);
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    assert!(!r.ok(), "a bogus second subject verified clean: {r:#?}");
    assert!(
        r.failed()
            .iter()
            .any(|c| c.name.contains("subject_tree[1]")),
        "the failure was not attributed to the second subject: {r:#?}"
    );
    // And the legitimate first subject still passes, so the report says which.
    assert!(r
        .checks
        .iter()
        .any(|c| c.name == "subject_tree[0]" && c.outcome.is_pass()));
}

#[test]
fn a_single_subject_keeps_its_unindexed_name_so_reports_stay_readable() {
    let k = key();
    let env = envelope_of(&statement(TREE), &k);
    let r = verify(&env, &[k.verifying_key()], &honest_local());
    assert!(r.checks.iter().any(|c| c.name == "subject_tree"));
    assert!(!r.checks.iter().any(|c| c.name.contains('[')));
}

#[test]
fn a_shallow_clone_cannot_see_far_enough_to_say_and_says_so() {
    // The finding: `actions/checkout` defaults to `fetch-depth: 1`, and
    // `cat-file --batch-check` truthfully answers `missing` for an object a
    // shallow clone does not have. Read as a FAILURE, the Action would post a
    // red check run on entirely honest receipts, by default configuration — and
    // a check that fails honest work is as damaging as one that passes
    // cheating.
    let k = key();
    let env = envelope_of(&statement(TREE), &k);
    let shallow = LocalEvidence {
        // The clone has objects, and not these ones.
        tree_hashes: Default::default(),
        commits: Default::default(),
        shallow: true,
        ..honest_local()
    };
    let r = verify(&env, &[k.verifying_key()], &shallow);

    // Skipped, not failed — and skipped still blocks `ok()`, so nothing that
    // would not have reached "verified" now does.
    assert!(
        r.failed().is_empty(),
        "a shallow clone reported a failure: {r:#?}"
    );
    assert!(!r.ok(), "a shallow clone reached `ok()`");
    assert_eq!(r.verified(), thruline_attest::verify::Verified::Partial);
    let skipped: Vec<&str> = r
        .skipped()
        .iter()
        .filter_map(|c| match &c.outcome {
            thruline_attest::verify::Outcome::Skipped { reason } => Some(reason.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        skipped.iter().any(|s| s.contains("fetch-depth: 0")),
        "the reason does not name the setting that fixes it: {skipped:?}"
    );
    assert!(
        skipped.iter().any(|s| s.contains("NOT a pass")),
        "the reason does not say a skip is not a pass: {skipped:?}"
    );

    // **The control.** The same missing objects in a FULL clone are still a
    // failure: this repository can see far enough to say the receipt is wrong,
    // and softening that would be the loosening the shallow case is not.
    let full = LocalEvidence {
        tree_hashes: Default::default(),
        commits: Default::default(),
        shallow: false,
        ..honest_local()
    };
    let r = verify(&env, &[k.verifying_key()], &full);
    assert_eq!(r.verified(), thruline_attest::verify::Verified::No);
    assert!(!r.failed().is_empty());
}

#[test]
fn the_check_run_conclusion_is_success_only_for_a_fully_verified_receipt() {
    // The Action consumes THIS, never the checks array. The construction it is
    // against: `all(c.outcome != "fail")` reads a partially-verified receipt as
    // verified, because a partial receipt contains no failing check.
    use thruline_attest::report::{check_run, conclusion};
    use thruline_attest::verify::Verified;
    let k = key();
    let env = envelope_of(&statement(TREE), &k);

    let mut partial = honest_local();
    partial.recomputed_chain = None;
    partial.ledger_present = false;

    for (local, expect) in [
        (honest_local(), "success"),
        (partial, "neutral"),
        (
            LocalEvidence {
                tree_hashes: ["9999999999999999999999999999999999999999".to_string()]
                    .into_iter()
                    .collect(),
                ..honest_local()
            },
            "failure",
        ),
    ] {
        let r = verify(&env, &[k.verifying_key()], &local);
        // The naive idiom, computed here so the divergence is visible.
        let naive_ok = r
            .checks
            .iter()
            .all(|c| !matches!(c.outcome, thruline_attest::verify::Outcome::Fail { .. }));
        let payload = check_run(&r, None, Some("deadbeef"), None);
        assert_eq!(payload["conclusion"], expect, "{r:#?}");
        assert_eq!(payload["head_sha"], "deadbeef");
        assert_eq!(payload["name"], thruline_attest::report::CHECK_NAME);
        if expect == "neutral" {
            assert!(
                naive_ok,
                "the premise of the finding no longer holds: the naive idiom now says no"
            );
            assert_ne!(payload["conclusion"], "success");
        }
    }
    // And the mapping itself, directly.
    assert_eq!(conclusion(Verified::Yes), "success");
    for other in [Verified::Partial, Verified::No] {
        assert_ne!(conclusion(other), "success");
    }
}
