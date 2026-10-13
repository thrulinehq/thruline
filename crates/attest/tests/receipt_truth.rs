//! **The keystone: a receipt is checked for TRUTH, not for well-formedness.**
//!
//! Three findings from the red team, reproduced here as tests before the code
//! that refuses them existed. Each one was first observed against the real
//! binary — a real repository, a real `pytest`, a real DSSE envelope signed by
//! the real key — and is restated here at the one place every rendering of the
//! conclusion goes through.
//!
//! - **F1** — `verify.rs` never read `input_tree_hash`. A repository sabotaged
//!   *after* a green run produced a receipt whose subject was the sabotaged
//!   tree, whose check recorded the tree that passed, and whose verdict said
//!   `supported` for the reason `"ran on the final tree"`. Nothing compared the
//!   two hashes, so the sentence in the reason was the only thing asserting it.
//! - **F2** — verdicts were trusted, not re-derived. A genuine receipt over a
//!   **failing** check, re-signed with the verdict flipped to `supported`,
//!   verified at exit 0 with a green badge. The signature was valid because
//!   **the signer is the gated party**: an agent that can run commands can also
//!   write a receipt, and one that can write a receipt holds the key.
//! - **F3** — binding was existence, not identity. `local.tree_hashes.contains`
//!   asks whether an object is *in the store*, which is true of every commit a
//!   hostile branch can reach. A receipt about an entirely different line of
//!   history verified `success` against the head under review.
//!
//! # The direction rule these tests encode
//!
//! Every guard below may only move a verdict **down**. A fix that turns an
//! honest `supported` into `unverifiable` is acceptable; one that turns
//! anything into `supported` is not. So each finding is paired with a control
//! that must stay green — §13.1's most expensive error is accusing honest work,
//! and a guard with no control is a guard nobody can tell from a stuck `Fail`.

use thruline_attest::dsse::{key_id, sign, Envelope};
use thruline_attest::statement::{
    AssertedBy, CheckEntry, ClaimEntry, Predicate, Session, Statement, Subject, VerdictEntry,
    VerifierInfo, PAYLOAD_TYPE,
};
use thruline_attest::verify::{verify, LocalEvidence, Outcome, Report, Verified};

/// The tree the checks ran on, and the tree an honest receipt is about.
const TREE: &str = "0123456789abcdef0123456789abcdef01234567";
/// A different tree — what the repository became after the sabotage.
const OTHER_TREE: &str = "89abcdef0123456789abcdef0123456789abcdef";
const COMMIT: &str = "fedcba9876543210fedcba9876543210fedcba98";
/// The head a reviewer is actually looking at, when it is not the receipt's.
const HEAD_COMMIT: &str = "1111222233334444555566667777888899990000";
const CHAIN: &str = "sha256:aaaabbbbccccdddd";
const POLICY: &str = "sha256:0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f";

fn key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[7u8; 32])
}

/// One check, one claim, one verdict — the smallest receipt that can lie.
fn statement(check: CheckEntry, verdict: VerdictEntry) -> Statement {
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
                event_chain_head: CHAIN.into(),
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
            checks: vec![check],
            verdicts: vec![verdict],
            verifier: VerifierInfo {
                thruline_version: "0.1.0".into(),
                mode: "local".into(),
                policy_hash: Some(POLICY.into()),
            },
            ..Default::default()
        },
    )
}

/// A check that passed, on the tree the receipt is about.
fn honest_check() -> CheckEntry {
    CheckEntry {
        id: "chk_1".into(),
        category: "test".into(),
        runner: "pytest".into(),
        argv_hash: "sha256:a".into(),
        env_hash: "sha256:b".into(),
        cwd_hash: "sha256:c".into(),
        input_tree_hash: TREE.into(),
        exit_code: 0,
        parsed: serde_json::json!({"tests_total": 2, "passed": 2, "failed": 0}),
        reexecuted: false,
        container_image_digest: None,
    }
}

fn supported() -> VerdictEntry {
    VerdictEntry {
        claim_id: "clm_1".into(),
        verdict: "supported".into(),
        verifier: "tests_pass@1".into(),
        evidence: vec!["chk_1".into()],
        reasons: vec!["ran on the final tree".into(), "2/2 passed".into()],
        warnings: vec![],
        findings: vec![],
    }
}

fn envelope_of(s: &Statement) -> Envelope {
    let k = key();
    let payload = serde_json::to_vec(s).expect("serialise");
    sign(&payload, PAYLOAD_TYPE, &k, Some(key_id(&k.verifying_key())))
}

/// A machine that can answer every question, honestly, about an honest receipt:
/// the objects are here, the ledger recomputes, the policy matches, and **the
/// head under review is the commit the receipt is about**.
fn honest_local() -> LocalEvidence {
    LocalEvidence {
        have_objects: true,
        tree_hashes: [TREE.to_string()].into_iter().collect(),
        commits: [COMMIT.to_string(), HEAD_COMMIT.to_string()]
            .into_iter()
            .collect(),
        recomputed_chain: Some((CHAIN.to_string(), 12)),
        ledger_present: true,
        shallow: false,
        policy_hash: Some(POLICY.to_string()),
        content_filters: None,
        head_commit: Some(COMMIT.to_string()),
        receipt_is_ancestor_of_head: Some(true),
        // Deliberately false, so the ancestry path is what these fixtures
        // exercise rather than the working-tree shortcut.
        subject_is_working_tree: false,
        head_tree_differs_at: Some(Vec::new()),
        receipt_path_in_repo: None,
        // The ledger holds exactly the check the receipt copies.
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

fn run(s: &Statement, local: &LocalEvidence) -> Report {
    verify(&envelope_of(s), &[key().verifying_key()], local)
}

#[track_caller]
fn outcome<'a>(r: &'a Report, name: &str) -> &'a Outcome {
    &r.checks
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no check named {name:?} in {:?}", names(r)))
        .outcome
}

fn names(r: &Report) -> Vec<&str> {
    r.checks.iter().map(|c| c.name.as_str()).collect()
}

#[track_caller]
fn assert_failed(r: &Report, name: &str, must_mention: &[&str]) {
    let o = outcome(r, name);
    let Outcome::Fail { reason } = o else {
        panic!("{name} was {o:?}, not a failure — the whole report: {r:#?}");
    };
    for needle in must_mention {
        assert!(
            reason.contains(needle),
            "{name}'s reason does not name {needle:?}: {reason}"
        );
    }
    assert_eq!(
        r.verified(),
        Verified::No,
        "one failed check must make the whole receipt `failed`"
    );
}

#[track_caller]
fn assert_skipped(r: &Report, name: &str) {
    let o = outcome(r, name);
    assert!(
        matches!(o, Outcome::Skipped { .. }),
        "{name} was {o:?}, not a skip"
    );
    assert!(!r.ok(), "a skipped check must block `ok()`");
    assert_ne!(
        r.verified(),
        Verified::Yes,
        "a skipped check may never reach `verified`"
    );
}

// ---------------------------------------------------------------------------
// The control. If this ever fails, every test below is measuring the wrong
// thing.
// ---------------------------------------------------------------------------

#[test]
fn the_control_an_honest_receipt_still_verifies_completely() {
    let r = run(&statement(honest_check(), supported()), &honest_local());
    assert!(r.ok(), "an honest receipt must still verify: {r:#?}");
    assert_eq!(r.verified(), Verified::Yes);
    for required in ["verdict_derivation", "head_binding"] {
        assert!(
            names(&r).contains(&required),
            "{required} was never checked at all: {:?}",
            names(&r)
        );
    }
}

// ---------------------------------------------------------------------------
// F1 — `input_tree_hash` was never read
// ---------------------------------------------------------------------------

/// The measured attack: a green `pytest`, then `app.py` rewritten, then the
/// receipt. `subject.gitTree` is the sabotaged tree; `checks[0].input_tree_hash`
/// is the tree that passed; the verdict reads `supported — "ran on the final
/// tree"`. Nothing in the receipt is forged and the repository fails its tests.
#[test]
fn f1_a_supported_verdict_whose_check_ran_on_another_tree_is_refused() {
    let mut check = honest_check();
    check.input_tree_hash = OTHER_TREE.into();
    let r = run(&statement(check, supported()), &honest_local());
    assert_failed(
        &r,
        "verdict_derivation",
        &["chk_1", OTHER_TREE, TREE, "clm_1"],
    );
}

/// **The reason string is not evidence.** The receipt R sabotaged said
/// *"ran on the final tree"* in prose while its own hashes said otherwise; a
/// guard that trusted the sentence would still pass it.
#[test]
fn f1_the_reason_saying_final_tree_does_not_make_it_so() {
    let mut check = honest_check();
    check.input_tree_hash = OTHER_TREE.into();
    let mut v = supported();
    v.reasons = vec!["ran on the final tree".into()];
    let r = run(&statement(check, v), &honest_local());
    assert_eq!(r.verified(), Verified::No, "{r:#?}");
}

/// The control for F1: a check bound to a *second* subject's tree is bound, and
/// must not be accused. A statement may legitimately carry more than one.
#[test]
fn f1_control_a_check_bound_to_any_subject_tree_is_not_accused() {
    let mut check = honest_check();
    check.input_tree_hash = OTHER_TREE.into();
    let mut s = statement(check, supported());
    let mut second = std::collections::BTreeMap::new();
    second.insert("gitTree".to_string(), OTHER_TREE.to_string());
    s.subject.push(Subject {
        name: "git+/repo@base".into(),
        digest: second,
    });
    let mut local = honest_local();
    local.tree_hashes.insert(OTHER_TREE.to_string());
    let r = run(&s, &local);
    assert!(
        matches!(outcome(&r, "verdict_derivation"), Outcome::Pass { .. }),
        "{r:#?}"
    );
}

// ---------------------------------------------------------------------------
// F2 — the verdict was read, never re-derived
// ---------------------------------------------------------------------------

/// The measured attack, exactly: a check records a **non-zero exit** and the
/// verdict beside it says `supported`. The envelope is genuinely signed — by
/// the party the gate is about.
///
/// # Why this fixture is a `build_ok` claim over a `build` check
///
/// It was first written as `tests_pass` over a failing `test` check — the
/// literal shape R attacked — and mutating the exit-code clause out reddened
/// **nothing**, because §6.3's citation-free rule below already refuses that
/// pair. A rule riding a stronger veto is a rule nobody is holding. Moving the
/// fixture off `tests_pass` leaves the exit code as the only thing that can
/// refuse it, so the clause has a test that can fail.
#[test]
fn f2_a_supported_verdict_over_a_failing_check_is_refused() {
    let mut check = honest_check();
    check.category = "build".into();
    check.runner = "cargo".into();
    check.exit_code = 1;
    check.parsed = serde_json::json!({"warnings": 0});
    let mut s = statement(check, supported());
    s.predicate.claims[0].kind = "build_ok".into();
    let r = run(&s, &honest_local());
    assert_failed(&r, "verdict_derivation", &["chk_1", "exit"]);
}

/// Exit code zero is not the whole answer. A runner that exits 0 while its own
/// tally reports failures — a `|| true`, a wrapper that swallows the status —
/// is caught by the tally, not by the status. Off `tests_pass` for the same
/// reason as the test above: so the tally is the only thing refusing it.
#[test]
fn f2_a_supported_verdict_over_a_zero_exit_with_failures_in_the_tally_is_refused() {
    let mut check = honest_check();
    check.category = "build".into();
    check.exit_code = 0;
    check.parsed = serde_json::json!({"tests_total": 2, "passed": 0, "failed": 2});
    let mut s = statement(check, supported());
    s.predicate.claims[0].kind = "build_ok".into();
    let r = run(&s, &honest_local());
    assert_failed(&r, "verdict_derivation", &["chk_1", "failed"]);
}

/// The `errored` tally is a second spelling the same clause must read: a runner
/// that reports collection errors rather than failures still did not pass.
#[test]
fn f2_a_supported_verdict_over_a_zero_exit_with_errors_in_the_tally_is_refused() {
    let mut check = honest_check();
    check.category = "build".into();
    check.exit_code = 0;
    check.parsed = serde_json::json!({"tests_total": 2, "passed": 0, "errored": 2});
    let mut s = statement(check, supported());
    s.predicate.claims[0].kind = "build_ok".into();
    let r = run(&s, &honest_local());
    assert_failed(&r, "verdict_derivation", &["chk_1", "errored"]);
}

/// §8.1 says `{}` is never written for `parsed` — an unparsed run carries
/// `{"unverifiable": …}` — so an empty block is a producer that wrote a
/// placeholder, and it must not read as a clean run. This is the cheapest
/// evasion of the two tally clauses above: delete the tally rather than change
/// it.
#[test]
fn f2_an_empty_parsed_block_does_not_read_as_a_clean_run() {
    let mut check = honest_check();
    check.parsed = serde_json::json!({});
    let r = run(&statement(check, supported()), &honest_local());
    assert_skipped(&r, "verdict_derivation");
}

/// §6: `unverifiable` never collapses into `supported`. A check whose `parsed`
/// block says the run could not be read cannot back a `supported` verdict.
#[test]
fn f2_a_supported_verdict_over_an_unparsed_run_is_refused() {
    let mut check = honest_check();
    check.parsed = serde_json::json!({"unverifiable": "unknown_runner"});
    let r = run(&statement(check, supported()), &honest_local());
    assert_failed(&r, "verdict_derivation", &["chk_1", "unverifiable"]);
}

/// **The measured forgery cites nothing, and must still be caught.**
///
/// The producer fills `evidence` on the path that reaches `supported` and
/// leaves it empty on the `unsupported` early return, so the cheapest forgery —
/// and the one actually performed against the real binary — flips an
/// evidence-less `unsupported` to `supported`. With nothing cited, the
/// citation-based rule has nothing to resolve and reports "could not
/// re-derive", which is exit 2 rather than a refusal.
///
/// §6.3 supplies a rule that needs no citation: a failing test check outranks
/// any passing one, so a `tests_pass` claim cannot be `supported` in a receipt
/// that carries one.
#[test]
fn f2_a_flipped_verdict_that_cites_nothing_is_still_refused_by_6_3() {
    let mut check = honest_check();
    check.exit_code = 1;
    check.parsed = serde_json::json!({"tests_total": 2, "passed": 0, "failed": 2});
    let mut v = supported();
    v.evidence = vec![]; // exactly what `tests_pass`'s `unsupported` path writes
    v.reasons = vec!["ran on the final tree".into()];
    let r = run(&statement(check, v), &honest_local());
    assert_failed(&r, "verdict_derivation", &["chk_1", "6.3", "clm_1"]);
}

/// The control §6.3's rule must not run over: a **build** check that failed
/// says nothing about a `tests_pass` claim, and a session whose build broke
/// while its tests passed is ordinary.
#[test]
fn f2_control_a_failing_non_test_check_does_not_refuse_a_tests_pass_claim() {
    let mut check = honest_check();
    check.category = "build".into();
    check.exit_code = 1;
    check.parsed = serde_json::json!({"unverifiable": "not a test runner"});
    let mut passing = honest_check();
    passing.id = "chk_2".into();
    let mut s = statement(check, supported());
    s.predicate.checks.push(passing);
    s.predicate.verdicts[0].evidence = vec!["chk_2".into()];
    let r = run(&s, &honest_local());
    assert!(
        matches!(outcome(&r, "verdict_derivation"), Outcome::Pass { .. }),
        "{r:#?}"
    );
}

/// And the rule is scoped to the claim kind it re-derives. A failing test check
/// beside a `docs_updated` claim is not a contradiction of that claim.
#[test]
fn f2_control_6_3_only_speaks_about_tests_pass_claims() {
    let mut check = honest_check();
    check.exit_code = 1;
    check.parsed = serde_json::json!({"tests_total": 2, "passed": 0, "failed": 2});
    let mut s = statement(check, supported());
    s.predicate.claims[0].kind = "docs_updated".into();
    s.predicate.verdicts[0].evidence = vec![];
    let r = run(&s, &honest_local());
    assert!(
        matches!(outcome(&r, "verdict_derivation"), Outcome::Skipped { .. }),
        "a claim §6.3 says nothing about must not be accused by it: {r:#?}"
    );
}

/// **Not a failure — a skip.** A verdict whose evidence is all `evt_…` ids
/// names events, which are not in the receipt. That is a receipt this command
/// cannot re-derive, not a receipt caught lying, and §13.1's rule is that the
/// expensive error is accusing honest work. It still may not reach `verified`.
#[test]
fn f2_a_supported_verdict_this_receipt_cannot_re_derive_is_a_skip_never_a_pass() {
    let mut v = supported();
    v.evidence = vec!["evt_01J0".into()];
    let r = run(&statement(honest_check(), v), &honest_local());
    assert_skipped(&r, "verdict_derivation");
}

/// The control: an honestly recorded `unsupported` verdict over a failing check
/// is a *correct* receipt. Whether it may merge is §9's gate, not this
/// command's, and re-derivation must not turn it into an accusation.
#[test]
fn f2_control_an_honest_unsupported_verdict_over_a_failing_check_is_not_accused() {
    let mut check = honest_check();
    check.exit_code = 1;
    check.parsed = serde_json::json!({"tests_total": 2, "passed": 0, "failed": 2});
    let mut v = supported();
    v.verdict = "unsupported".into();
    v.reasons = vec!["a captured test run failed: `pytest -q` exited 1".into()];
    let r = run(&statement(check, v), &honest_local());
    assert!(
        matches!(outcome(&r, "verdict_derivation"), Outcome::Pass { .. }),
        "{r:#?}"
    );
}

// --- the closed verdict enum, at the door ----------------------------------

/// The published schema's `verdict` domain is exactly three lowercase words,
/// and it is load-bearing: `verify` decides how many verdicts are unverifiable
/// by a string comparison, so `"Unverifiable"` counts as *not* unverifiable and
/// the receipt reports `none left unverifiable`. Refused at `statement_schema`,
/// before any field of the predicate is read.
#[test]
fn f2_an_unknown_verdict_word_is_refused_at_the_statement_schema_door() {
    for word in ["Unverifiable", "unverified", "ok", "SUPPORTED", "", "pass"] {
        let mut v = supported();
        v.verdict = word.into();
        let r = run(&statement(honest_check(), v), &honest_local());
        assert_failed(&r, "statement_schema", &["verdict"]);
        assert!(
            !names(&r).contains(&"verdicts"),
            "a statement refused at the door must not go on to be judged: {:?}",
            names(&r)
        );
    }
}

/// And the three real words are not refused.
#[test]
fn f2_control_the_three_ruled_verdict_words_are_accepted() {
    for word in ["supported", "unsupported", "unverifiable"] {
        let mut v = supported();
        v.verdict = word.into();
        let r = run(&statement(honest_check(), v), &honest_local());
        assert!(
            matches!(outcome(&r, "statement_schema"), Outcome::Pass { .. }),
            "{word} was refused: {r:#?}"
        );
    }
}

// ---------------------------------------------------------------------------
// F2b — the checks themselves, which nothing bound to anything
// ---------------------------------------------------------------------------
//
// Found by attacking the F2 fix rather than by reading it. Re-deriving a
// verdict from `predicate.checks` is only worth something if `predicate.checks`
// is itself evidence — and it was not bound to anything. `chain_head`
// recomputes the EVENT chain; the check rows are beside it, unhashed.
//
// Measured against the real binary: a re-signer who flips the verdict AND
// invents a coherent passing check — right tree, exit 0, a clean tally, cited
// as the verdict's evidence — got `Verified.` at exit 0 in a repository whose
// own `pytest -q` reported `1 failed`. Every guard above passed it, because
// each one asked whether the receipt agreed with itself and it did.

/// The ledger is the record; the receipt is a copy of it. When the ledger is
/// here, the copy must match.
#[test]
fn f2b_a_check_this_ledger_never_recorded_is_refused() {
    let mut invented = honest_check();
    invented.id = "chk_invented".into();
    let mut v = supported();
    v.evidence = vec!["chk_invented".into()];
    let r = run(&statement(invented, v), &honest_local());
    assert_failed(&r, "check_provenance", &["chk_invented"]);
}

/// A check the ledger has, whose recorded exit code the receipt rewrote.
#[test]
fn f2b_a_check_whose_exit_code_was_rewritten_is_refused() {
    let mut doctored = honest_check();
    doctored.exit_code = 0;
    let mut local = honest_local();
    local.ledger_checks = Some([("chk_1".to_string(), (1, TREE.to_string()))].into());
    let r = run(&statement(doctored, supported()), &local);
    assert_failed(&r, "check_provenance", &["chk_1", "exit"]);
}

/// And whose recorded input tree the receipt rewrote — the F1 hash, moved.
#[test]
fn f2b_a_check_whose_input_tree_was_rewritten_is_refused() {
    let mut local = honest_local();
    local.ledger_checks = Some([("chk_1".to_string(), (0, OTHER_TREE.to_string()))].into());
    let r = run(&statement(honest_check(), supported()), &local);
    assert_failed(&r, "check_provenance", &["chk_1", OTHER_TREE]);
}

/// **Not an accusation where there is nothing to compare against.** A reviewer
/// holding only the repository and the receipt has no ledger, and §8.1 says
/// "against the local ledger **(if present)**". Skip, never pass, never blame.
#[test]
fn f2b_with_no_ledger_the_checks_are_unchecked_not_endorsed() {
    let mut local = honest_local();
    local.ledger_checks = None;
    let r = run(&statement(honest_check(), supported()), &local);
    assert_skipped(&r, "check_provenance");
}

/// The control: a receipt whose checks are the ledger's own rows is not
/// accused, and a receipt carrying no checks has nothing to disagree about.
#[test]
fn f2b_control_a_faithful_copy_of_the_ledger_passes() {
    let r = run(&statement(honest_check(), supported()), &honest_local());
    assert!(
        matches!(outcome(&r, "check_provenance"), Outcome::Pass { .. }),
        "{r:#?}"
    );
    let mut s = statement(honest_check(), supported());
    s.predicate.checks.clear();
    s.predicate.verdicts[0].evidence = vec![];
    let r = run(&s, &honest_local());
    assert!(
        matches!(outcome(&r, "check_provenance"), Outcome::Pass { .. }),
        "{r:#?}"
    );
}

// ---------------------------------------------------------------------------
// F3 — binding was existence, not identity
// ---------------------------------------------------------------------------

/// The measured attack: an honest receipt, committed onto an orphan branch
/// whose code is hostile. Every object the receipt names is in the store, so
/// `tree_hashes.contains` says yes — and the receipt describes a line of
/// history the reviewed head is not on.
#[test]
fn f3_a_receipt_whose_commit_is_not_an_ancestor_of_the_reviewed_head_is_refused() {
    let mut local = honest_local();
    local.head_commit = Some(HEAD_COMMIT.to_string());
    local.receipt_is_ancestor_of_head = Some(false);
    local.head_tree_differs_at = Some(vec!["app.py".into(), "test_app.py".into()]);
    let r = run(&statement(honest_check(), supported()), &local);
    assert_failed(&r, "head_binding", &[COMMIT, HEAD_COMMIT, "ancestor"]);
}

/// R's own remedy — *"fail when `subject.gitCommit != $GITHUB_SHA`"* — is
/// wrong, and Track C proved it by building it: committing the receipt is
/// itself a commit, so the two can never be equal on §9's own zero-install
/// path. **This test is the one that rule would break.** The receipt's commit
/// is an ancestor of the head and the only path that moved is the receipt file
/// the reviewer is reading.
#[test]
fn f3_the_documented_zero_install_workflow_still_passes() {
    let mut local = honest_local();
    local.head_commit = Some(HEAD_COMMIT.to_string());
    local.receipt_is_ancestor_of_head = Some(true);
    local.head_tree_differs_at = Some(vec![".thruline/receipts/latest.json".into()]);
    local.receipt_path_in_repo = Some(".thruline/receipts/latest.json".into());
    let r = run(&statement(honest_check(), supported()), &local);
    assert!(
        matches!(outcome(&r, "head_binding"), Outcome::Pass { .. }),
        "the documented workflow must not be refused: {r:#?}"
    );
    assert!(r.ok(), "{r:#?}");
}

/// Ancestry alone is not enough. A commit that adds the receipt **and quietly
/// rewrites the code the check ran on** is an ancestor-descendant pair too.
/// Nothing in the receipt says what its checks covered, so the honest answer is
/// that this could not be established — a skip, never a pass.
#[test]
fn f3_an_ancestor_whose_diff_touches_the_code_is_not_a_pass() {
    let mut local = honest_local();
    local.head_commit = Some(HEAD_COMMIT.to_string());
    local.receipt_is_ancestor_of_head = Some(true);
    local.head_tree_differs_at = Some(vec![
        ".thruline/receipts/latest.json".into(),
        "app.py".into(),
    ]);
    local.receipt_path_in_repo = Some(".thruline/receipts/latest.json".into());
    let r = run(&statement(honest_check(), supported()), &local);
    assert_skipped(&r, "head_binding");
    let Outcome::Skipped { reason } = outcome(&r, "head_binding") else {
        unreachable!()
    };
    assert!(reason.contains("app.py"), "the skip must name it: {reason}");
}

/// Where ancestry cannot be established — no repository, a shallow clone, an
/// object this checkout does not have — the answer is a skip. Never a pass,
/// and never an accusation either: `actions/checkout` defaults to
/// `fetch-depth: 1`, so failing here would redden honest work by default
/// configuration.
#[test]
fn f3_when_ancestry_cannot_be_established_the_answer_is_a_skip() {
    for (label, mutate) in [
        (
            "no head could be resolved",
            Box::new(|l: &mut LocalEvidence| l.head_commit = None)
                as Box<dyn Fn(&mut LocalEvidence)>,
        ),
        (
            "ancestry undecidable",
            Box::new(|l: &mut LocalEvidence| {
                l.head_commit = Some(HEAD_COMMIT.to_string());
                l.receipt_is_ancestor_of_head = None;
            }),
        ),
        (
            "shallow clone",
            Box::new(|l: &mut LocalEvidence| {
                l.head_commit = Some(HEAD_COMMIT.to_string());
                l.receipt_is_ancestor_of_head = None;
                l.shallow = true;
            }),
        ),
        (
            "no object store",
            Box::new(|l: &mut LocalEvidence| {
                l.have_objects = false;
                l.head_commit = None;
            }),
        ),
        (
            "ancestor, but the diff could not be computed",
            Box::new(|l: &mut LocalEvidence| {
                l.head_commit = Some(HEAD_COMMIT.to_string());
                l.receipt_is_ancestor_of_head = Some(true);
                l.head_tree_differs_at = None;
            }),
        ),
    ] {
        let mut local = honest_local();
        mutate(&mut local);
        let r = run(&statement(honest_check(), supported()), &local);
        assert!(
            matches!(outcome(&r, "head_binding"), Outcome::Skipped { .. }),
            "{label}: head_binding was {:?}, not a skip",
            outcome(&r, "head_binding")
        );
        assert_ne!(
            r.verified(),
            Verified::Yes,
            "{label} must not reach verified"
        );
    }
}

/// **The ordinary local case, which has no commit to bind to.**
///
/// A receipt about work in progress is about a tree that is no commit at all,
/// and resolving `HEAD` as "the head under review" there answers a question
/// nobody asked. When the tree in the working directory IS the subject tree,
/// the binding is settled without any commit: the code being looked at is the
/// code the checks ran on. Reaching this requires the reviewer's own checkout
/// to be that tree, which is the binding rather than a way around it.
#[test]
fn f3_a_working_tree_that_is_the_subject_tree_is_bound_without_a_commit() {
    let mut s = statement(honest_check(), supported());
    s.subject[0].digest.remove("gitCommit");
    let mut local = honest_local();
    local.subject_is_working_tree = true;
    local.head_commit = None;
    local.receipt_is_ancestor_of_head = None;
    local.head_tree_differs_at = None;
    let r = run(&s, &local);
    assert!(
        matches!(outcome(&r, "head_binding"), Outcome::Pass { .. }),
        "{r:#?}"
    );
}

/// And it is identity, not a shortcut: a working tree that is NOT the subject
/// tree gets no help from it.
#[test]
fn f3_control_a_different_working_tree_does_not_bind() {
    let mut local = honest_local();
    local.subject_is_working_tree = false;
    local.head_commit = Some(HEAD_COMMIT.to_string());
    local.receipt_is_ancestor_of_head = Some(false);
    let r = run(&statement(honest_check(), supported()), &local);
    assert_failed(&r, "head_binding", &["ancestor"]);
}

/// A receipt that names no commit at all cannot be bound to a head. It is a
/// skip on this axis rather than a failure — an uncommitted working tree is a
/// normal state — but it is not a pass.
#[test]
fn f3_a_receipt_with_no_commit_cannot_be_bound_to_a_head() {
    let mut s = statement(honest_check(), supported());
    s.subject[0].digest.remove("gitCommit");
    let mut local = honest_local();
    local.head_commit = Some(HEAD_COMMIT.to_string());
    let r = run(&s, &local);
    assert_skipped(&r, "head_binding");
}
