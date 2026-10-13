//! T-4-2 — where each thing a review found stands (`B9.10`, `B9.11`, `B9.13`,
//! `B9.14`, `Q9`, `Q10`).
//!
//! Every refusal [`finding_standing::record`] can return has a test here, and
//! the three the card names for break-and-restore are marked GUARD A/B/C:
//!
//! * GUARD A — `fixed_with_evidence` without evidence is refused;
//! * GUARD B — setting a finding aside without a reason or an authority is
//!   refused, and the refusal names what is missing;
//! * GUARD C — a "resolved" mark never reaches `fixed_with_evidence`.
//!
//! Every string below is synthetic. Nothing came off a real machine.

#[path = "support/live_store.rs"]
mod live_store;

use rusqlite::params;
use thruline_core::change::{
    self, AcceptedScope, ChangeRequest, NewChange, RequestSource, ScopeAcceptance,
};
use thruline_core::change_review::{self, NewReview, ReviewFinding, ReviewState, ReviewerType};
use thruline_core::finding_standing::{
    self, finding_key, EvidenceKind, EvidenceRef, Group, Outcome, Standing, StandingAct, Via,
};
use thruline_core::ledger::Ledger;

const CHANGE: &str = "chg_standing_fixture";

/// One `change prepare` that found the code moved: a new snapshot, and the
/// code on disk is that version's own (its own commit, its own identity).
fn record_version(ledger: &Ledger, snapshot: &str, inputs: &str, at: &str) {
    record_code(ledger, snapshot, inputs, at, snapshot);
}

/// One `change prepare`: a snapshot, and the intended record naming it. `code`
/// names what is on disk. Two versions with the same `code` are a re-prepare
/// with nothing changed — one commit, one clean tree, one source identity, a
/// new snapshot id; two with different `code` are the code moving to another
/// commit.
fn record_code(ledger: &Ledger, snapshot: &str, inputs: &str, at: &str, code: &str) {
    let digest = thruline_core::chain::sha256_hex(code.as_bytes());
    ledger
        .conn()
        .execute(
            "INSERT OR IGNORE INTO workspace_snapshots(id,taken_at,root,complete,doc,doc_sha256)
             VALUES(?1,'2026-01-02T00:00:00Z','/fixture/standing',1,'{}',?2)",
            params![snapshot, "a".repeat(64)],
        )
        .unwrap();
    ledger
        .conn()
        .execute(
            "INSERT INTO examination_inputs(id,subject_kind,subject_id,recorded_at,schema_version,
                 snapshot_id,source_identity,base_revision,uncommitted_state,worktree_path,
                 config_files_json,tools_json,target_kind)
             VALUES(?1,'intended',?2,?3,1,?4,?5,?6,'clean',
                 '/fixture/standing','[]','[]','none')",
            params![
                inputs,
                CHANGE,
                at,
                snapshot,
                format!("wsid1:{digest}"),
                &digest[..40]
            ],
        )
        .unwrap();
}

/// The digest of the diff a review of one version reads.
fn diff_of(version: &str) -> String {
    thruline_core::chain::sha256_hex(format!("SYNTHETIC diff of {version}").as_bytes())
}

fn prepared() -> Ledger {
    let ledger = Ledger::open_in_memory().unwrap();
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id,root_path,source,display_name,created_at)
             VALUES('project-standing','/fixture/standing','claude_code','Standing',
                    '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    change::insert_change(
        &ledger,
        &NewChange {
            id: CHANGE.into(),
            project_id: "project-standing".into(),
            opened_at: "2026-01-02T00:00:00Z".into(),
            request: ChangeRequest {
                title: "Calculator".into(),
                purpose: "Correct addition".into(),
                prompt_event_id: None,
                requester: "sam".into(),
                source: RequestSource::Person,
            },
        },
    )
    .unwrap();
    change::append_scope(
        &ledger,
        CHANGE,
        "2026-01-02T00:01:00Z",
        &AcceptedScope {
            criteria: vec!["addition adds".into()],
            exclusions: vec![],
            allowed_scope: vec!["calculator.py".into()],
            constraints: vec![],
            check_profile: vec![],
        },
    )
    .unwrap();
    change::accept_scope(
        &ledger,
        CHANGE,
        1,
        &ScopeAcceptance {
            actor: "sam".into(),
            accepted_at: "2026-01-02T00:02:00Z".into(),
            rationale: "fixture".into(),
        },
    )
    .unwrap();
    record_version(&ledger, "snap-one", "exi-one", "2026-01-02T00:03:00Z");
    ledger
}

fn route() -> String {
    serde_json::json!({
        "endpoint": change_review::ROUTE_ENDPOINT,
        "model": "qwen3:8b",
        "model_manifest_sha256": "5".repeat(64),
        "runtime_sha256": "4".repeat(64),
        "provider_source_commit": change_review::PR_AGENT_SOURCE_COMMIT,
        "image_id": format!("sha256:{}", "9".repeat(64)),
        "plan_digest": "e".repeat(64),
    })
    .to_string()
}

fn finding(kind: &str, path: Option<&str>, line: Option<u64>) -> ReviewFinding {
    ReviewFinding {
        kind: kind.into(),
        message: format!("SYNTHETIC: {kind}"),
        path: path.map(str::to_string),
        line,
    }
}

/// Three things the first review raised on version one: a real bug in
/// `calculator.py`, a note in `README.md`, and a nit in `utils.py`.
fn first_findings() -> Vec<ReviewFinding> {
    vec![
        finding("Addition subtracts", Some("calculator.py"), Some(2)),
        finding("Wording in the readme", Some("README.md"), Some(1)),
        finding("Unused import", Some("utils.py"), Some(1)),
    ]
}

fn review(
    id: &str,
    round: u32,
    target: &str,
    inputs: &str,
    state: ReviewState,
    findings: Vec<ReviewFinding>,
) -> NewReview {
    let complete = state == ReviewState::Complete;
    NewReview {
        id: id.into(),
        change_id: CHANGE.into(),
        review_round: round,
        reviewer_identity: "PR-Agent 0.45.0 via qwen3:8b".into(),
        reviewer_type: ReviewerType::Model,
        review_source: "local/plain-diff".into(),
        reviewer_relationship: "not established".into(),
        relationship_provenance: "fixture".into(),
        target_revision: target.into(),
        scope_version: Some(1),
        examined_inputs_id: inputs.into(),
        diff_sha256: diff_of(target),
        route_identity_json: route(),
        started_at: "2026-01-02T00:04:00Z".into(),
        ended_at: Some("2026-01-02T00:05:00Z".into()),
        state,
        recommendation: None,
        limitations: vec!["advisory only".into()],
        raw_bytes: br#"{"note":"synthetic raw"}"#.to_vec(),
        parser_id: "thruline.pr-agent-structured.v1".into(),
        parsed_json: r#"{"state":"x"}"#.into(),
        parse_complete: complete,
        findings: if complete { findings } else { vec![] },
    }
}

/// Round 1 raised three things on version one; the code moved to version
/// two; round 2 on version two raises only the `utils.py` nit again.
fn two_rounds() -> Ledger {
    let ledger = prepared();
    change_review::append_review(
        &ledger,
        &review(
            "r1",
            1,
            "snap-one",
            "exi-one",
            ReviewState::Complete,
            first_findings(),
        ),
    )
    .unwrap();
    record_version(&ledger, "snap-two", "exi-two", "2026-01-03T00:00:00Z");
    change_review::append_review(
        &ledger,
        &review(
            "r2",
            2,
            "snap-two",
            "exi-two",
            ReviewState::Complete,
            vec![finding("Unused import", Some("utils.py"), Some(3))],
        ),
    )
    .unwrap();
    ledger
}

fn act(finding: &str, to: Standing) -> StandingAct {
    StandingAct {
        finding: finding.into(),
        to,
        via: Via::Person,
        actor: "sam".into(),
        authority: String::new(),
        reason: String::new(),
        evidence: None,
        seen: None,
        at: "2026-01-04T00:00:00Z".into(),
    }
}

fn refused(outcome: Outcome) -> finding_standing::Refusal {
    match outcome {
        Outcome::Refused(r) => r,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn recorded(outcome: Outcome) {
    assert!(
        matches!(outcome, Outcome::Recorded { .. }),
        "expected a recorded act, got {outcome:?}"
    );
}

fn bug() -> String {
    finding_key("r1", 0)
}
fn note() -> String {
    finding_key("r1", 1)
}
fn nit() -> String {
    finding_key("r1", 2)
}

fn view_of(ledger: &Ledger, key: &str) -> finding_standing::FindingStandingView {
    finding_standing::read(ledger, CHANGE)
        .unwrap()
        .findings
        .into_iter()
        .find(|f| f.key == key)
        .unwrap()
}

fn rows(ledger: &Ledger) -> i64 {
    ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM change_review_finding_standings",
            [],
            |r| r.get(0),
        )
        .unwrap()
}

// ── the seven, and where a finding starts ────────────────────────────────

#[test]
fn seven_standings_and_nothing_else() {
    let names: Vec<&str> = Standing::ALL.iter().map(|s| s.as_str()).collect();
    assert_eq!(
        names,
        [
            "open",
            "under_repair",
            "awaiting_verification",
            "fixed_with_evidence",
            "disputed",
            "informational",
            "waived"
        ]
    );
    for s in Standing::ALL {
        assert_eq!(Standing::parse(s.as_str()).unwrap(), s);
    }
    assert!(Standing::parse("resolved").is_err());
    assert!(Standing::parse("proven").is_err());
}

#[test]
fn a_finding_nobody_has_decided_about_is_open_with_no_decision_recorded() {
    let ledger = two_rounds();
    let view = finding_standing::read(&ledger, CHANGE).unwrap();
    // Round 1's three and round 2's one.
    assert_eq!(view.findings.len(), 4);
    for f in &view.findings {
        assert_eq!(f.standing, Standing::Open);
        assert!(!f.decided);
        assert!(f.latest.is_none());
        assert!(f.history.is_empty());
        assert_eq!(f.group, Group::StillWrong);
        assert!(!f.counts_as_fixed);
    }
    assert_eq!(view.summary.still_wrong, 4);
    assert_eq!(view.summary.fixed, 0);
    assert_eq!(rows(&ledger), 0, "reading writes nothing");
}

// ── rule 2: a fix needs evidence (GUARD A) ───────────────────────────────

/// GUARD A.
#[test]
fn fixed_without_evidence_is_refused() {
    let ledger = two_rounds();
    let r = refused(
        finding_standing::record(&ledger, CHANGE, &act(&bug(), Standing::FixedWithEvidence))
            .unwrap(),
    );
    assert_eq!(r.code, "evidence_required");
    assert_eq!(rows(&ledger), 0);
    assert_eq!(view_of(&ledger, &bug()).standing, Standing::Open);
}

#[test]
fn a_later_review_on_the_current_version_that_no_longer_raises_it_shows_the_fix() {
    let ledger = two_rounds();
    // The screen offers exactly the evidence the rule would accept.
    let before = view_of(&ledger, &bug());
    assert_eq!(before.fix_evidence.len(), 1);
    assert_eq!(before.fix_evidence[0].id, "r2");
    assert_eq!(before.fix_evidence[0].kind, EvidenceKind::Review);

    let mut a = act(&bug(), Standing::FixedWithEvidence);
    a.evidence = Some(EvidenceRef {
        kind: EvidenceKind::Review,
        id: "r2".into(),
    });
    recorded(finding_standing::record(&ledger, CHANGE, &a).unwrap());
    let f = view_of(&ledger, &bug());
    assert_eq!(f.standing, Standing::FixedWithEvidence);
    assert!(f.decided);
    assert!(f.counts_as_fixed);
    assert_eq!(f.group, Group::Fixed);
    assert_eq!(f.evidence_applies, Some(true));
    let e = f.evidence.unwrap();
    assert_eq!(e.id, "r2");
    assert_eq!(e.revision, "snap-two");
    assert_eq!(e.review_round, Some(2));
    let row = f.latest.unwrap();
    assert_eq!(row.from, Standing::Open);
    assert_eq!(row.evidence_kind, Some(EvidenceKind::Review));
    assert_eq!(row.evidence_revision.as_deref(), Some("snap-two"));
}

#[test]
fn a_later_review_that_still_raises_something_in_that_file_is_not_a_fix() {
    let ledger = two_rounds();
    assert!(view_of(&ledger, &nit()).fix_evidence.is_empty());
    let mut a = act(&nit(), Standing::FixedWithEvidence);
    a.evidence = Some(EvidenceRef {
        kind: EvidenceKind::Review,
        id: "r2".into(),
    });
    assert_eq!(
        refused(finding_standing::record(&ledger, CHANGE, &a).unwrap()).code,
        "evidence_still_raised"
    );
}

/// FINDING-RENAME-LAUNDERING: the bug is never fixed, only its file renamed
/// from `calculator.py` to `calc.py` between the two versions, and the
/// re-check raises the same thing at the new name. The original path being
/// absent from that re-check does not show the finding is gone.
#[test]
fn a_later_review_that_raises_the_same_thing_in_a_renamed_file_is_not_a_fix() {
    let ledger = prepared();
    change_review::append_review(
        &ledger,
        &review(
            "r1",
            1,
            "snap-one",
            "exi-one",
            ReviewState::Complete,
            first_findings(),
        ),
    )
    .unwrap();
    record_version(&ledger, "snap-two", "exi-two", "2026-01-03T00:00:00Z");
    change_review::append_review(
        &ledger,
        &review(
            "r2",
            2,
            "snap-two",
            "exi-two",
            ReviewState::Complete,
            vec![finding("Addition subtracts", Some("calc.py"), Some(2))],
        ),
    )
    .unwrap();

    // Not offered as evidence, and refused when named.
    assert!(view_of(&ledger, &bug()).fix_evidence.is_empty());
    let mut a = act(&bug(), Standing::FixedWithEvidence);
    a.evidence = Some(EvidenceRef {
        kind: EvidenceKind::Review,
        id: "r2".into(),
    });
    assert_eq!(
        refused(finding_standing::record(&ledger, CHANGE, &a).unwrap()).code,
        "evidence_raised_elsewhere"
    );
    assert_eq!(rows(&ledger), 0);
    let f = view_of(&ledger, &bug());
    assert_eq!(f.standing, Standing::Open);
    assert!(!f.counts_as_fixed);
    assert_eq!(f.group, Group::StillWrong);

    // The README note was not raised again anywhere: that review still
    // shows it gone.
    let mut n = act(&note(), Standing::FixedWithEvidence);
    n.evidence = Some(EvidenceRef {
        kind: EvidenceKind::Review,
        id: "r2".into(),
    });
    recorded(finding_standing::record(&ledger, CHANGE, &n).unwrap());
}

#[test]
fn a_review_of_the_same_version_is_not_a_later_one() {
    let ledger = prepared();
    change_review::append_review(
        &ledger,
        &review(
            "r1",
            1,
            "snap-one",
            "exi-one",
            ReviewState::Complete,
            first_findings(),
        ),
    )
    .unwrap();
    change_review::append_review(
        &ledger,
        &review(
            "r1b",
            2,
            "snap-one",
            "exi-one",
            ReviewState::Complete,
            vec![],
        ),
    )
    .unwrap();
    let mut a = act(&bug(), Standing::FixedWithEvidence);
    a.evidence = Some(EvidenceRef {
        kind: EvidenceKind::Review,
        id: "r1b".into(),
    });
    assert_eq!(
        refused(finding_standing::record(&ledger, CHANGE, &a).unwrap()).code,
        "evidence_not_later"
    );
    // Nor is the review that raised it.
    a.evidence = Some(EvidenceRef {
        kind: EvidenceKind::Review,
        id: "r1".into(),
    });
    assert_eq!(
        refused(finding_standing::record(&ledger, CHANGE, &a).unwrap()).code,
        "evidence_not_later"
    );
}

#[test]
fn a_review_that_did_not_finish_shows_nothing() {
    let ledger = two_rounds();
    change_review::append_review(
        &ledger,
        &review(
            "r3",
            3,
            "snap-two",
            "exi-two",
            ReviewState::TimedOut,
            vec![],
        ),
    )
    .unwrap();
    let mut a = act(&bug(), Standing::FixedWithEvidence);
    a.evidence = Some(EvidenceRef {
        kind: EvidenceKind::Review,
        id: "r3".into(),
    });
    assert_eq!(
        refused(finding_standing::record(&ledger, CHANGE, &a).unwrap()).code,
        "evidence_not_finished"
    );
}

#[test]
fn evidence_that_does_not_exist_or_is_about_another_version_is_refused() {
    let ledger = two_rounds();
    let mut a = act(&bug(), Standing::FixedWithEvidence);
    a.evidence = Some(EvidenceRef {
        kind: EvidenceKind::Review,
        id: "no-such-review".into(),
    });
    assert_eq!(
        refused(finding_standing::record(&ledger, CHANGE, &a).unwrap()).code,
        "evidence_not_found"
    );
    a.evidence = Some(EvidenceRef {
        kind: EvidenceKind::CheckRun,
        id: "no-such-run".into(),
    });
    assert_eq!(
        refused(finding_standing::record(&ledger, CHANGE, &a).unwrap()).code,
        "evidence_not_found"
    );
    // The code moves again: round 2 is now about an earlier version.
    record_version(&ledger, "snap-three", "exi-three", "2026-01-05T00:00:00Z");
    a.evidence = Some(EvidenceRef {
        kind: EvidenceKind::Review,
        id: "r2".into(),
    });
    assert_eq!(
        refused(finding_standing::record(&ledger, CHANGE, &a).unwrap()).code,
        "evidence_not_current"
    );
}

#[test]
fn a_recorded_fix_whose_evidence_no_longer_applies_stops_counting_as_fixed() {
    let ledger = two_rounds();
    let mut a = act(&bug(), Standing::FixedWithEvidence);
    a.evidence = Some(EvidenceRef {
        kind: EvidenceKind::Review,
        id: "r2".into(),
    });
    recorded(finding_standing::record(&ledger, CHANGE, &a).unwrap());
    record_version(&ledger, "snap-three", "exi-three", "2026-01-05T00:00:00Z");
    let f = view_of(&ledger, &bug());
    assert_eq!(
        f.standing,
        Standing::FixedWithEvidence,
        "the row is never edited"
    );
    assert_eq!(f.evidence_applies, Some(false));
    assert_eq!(f.evidence_lapsed, Some("evidence_not_current"));
    assert!(!f.counts_as_fixed);
    assert_eq!(f.group, Group::AwaitingRecheck);
}

#[test]
fn evidence_goes_only_with_a_fix() {
    let ledger = two_rounds();
    let mut a = act(&bug(), Standing::UnderRepair);
    a.evidence = Some(EvidenceRef {
        kind: EvidenceKind::Review,
        id: "r2".into(),
    });
    assert_eq!(
        refused(finding_standing::record(&ledger, CHANGE, &a).unwrap()).code,
        "evidence_not_for_this"
    );
}

// ── rule 2: the code has to have moved (ADV-review-truth-1) ──────────────

/// Round 1 raised three things on version one; then `change prepare` ran
/// again with NOTHING changed — a new snapshot id, the same commit, the same
/// clean tree — and round 2, on that new version, reads a diff with the given
/// digest and raises only the `utils.py` nit.
fn round_one_then_the_same_code_again(round_two_diff: &str) -> Ledger {
    let ledger = prepared();
    change_review::append_review(
        &ledger,
        &review(
            "r1",
            1,
            "snap-one",
            "exi-one",
            ReviewState::Complete,
            first_findings(),
        ),
    )
    .unwrap();
    record_code(
        &ledger,
        "snap-again",
        "exi-again",
        "2026-01-03T00:00:00Z",
        "snap-one",
    );
    let mut r2 = review(
        "r2",
        2,
        "snap-again",
        "exi-again",
        ReviewState::Complete,
        vec![finding("Unused import", Some("utils.py"), Some(3))],
    );
    r2.diff_sha256 = round_two_diff.to_string();
    change_review::append_review(&ledger, &r2).unwrap();
    ledger
}

fn fix_with_review(ledger: &Ledger, review: &str) -> Outcome {
    let mut a = act(&bug(), Standing::FixedWithEvidence);
    a.evidence = Some(EvidenceRef {
        kind: EvidenceKind::Review,
        id: review.into(),
    });
    finding_standing::record(ledger, CHANGE, &a).unwrap()
}

/// The empty-diff digest: what a review that read zero bytes of diff names.
fn empty_diff() -> String {
    thruline_core::chain::sha256_hex(b"")
}

/// Round 1 raised three things on version one; the code moved to version two
/// (another commit); round 2 on version two read a diff with the given digest
/// and raises only the `utils.py` nit.
fn round_one_then_moved_code(round_two_diff: &str) -> Ledger {
    let ledger = prepared();
    change_review::append_review(
        &ledger,
        &review(
            "r1",
            1,
            "snap-one",
            "exi-one",
            ReviewState::Complete,
            first_findings(),
        ),
    )
    .unwrap();
    record_version(&ledger, "snap-two", "exi-two", "2026-01-03T00:00:00Z");
    let mut r2 = review(
        "r2",
        2,
        "snap-two",
        "exi-two",
        ReviewState::Complete,
        vec![finding("Unused import", Some("utils.py"), Some(3))],
    );
    r2.diff_sha256 = round_two_diff.to_string();
    change_review::append_review(&ledger, &r2).unwrap();
    ledger
}

/// Nothing was written and the finding still reads as open, not fixed.
fn nothing_moved(ledger: &Ledger) {
    assert_eq!(rows(ledger), 0, "nothing is written");
    let f = view_of(ledger, &bug());
    assert_eq!(f.standing, Standing::Open);
    assert!(!f.counts_as_fixed);
    assert_eq!(f.group, Group::StillWrong);
    assert_eq!(
        finding_standing::read(ledger, CHANGE)
            .unwrap()
            .summary
            .fixed,
        0
    );
}

/// ADV-review-truth-1 — the founder's named regression: the transition to
/// Fixed is refused when the diff is empty.
///
/// First, the adversary's F1 as they ran it: `change prepare` again with
/// NOTHING changed — the diff between the code the finding was raised on and
/// the code the re-check read is empty (one commit, a clean tree, one source
/// identity; only the snapshot id is new) — and a second review that reads the
/// same diff and no longer raises the bug. The record names a new version, so
/// "later" by version id and by round held, and nothing was repaired.
///
/// Second, the other empty diff: the code did move, but the second review read
/// a diff of zero bytes. A review that read nothing cannot show a fix.
#[test]
fn the_transition_to_fixed_is_refused_when_the_diff_is_empty() {
    // (1) The code did not change between the two versions.
    let ledger = round_one_then_the_same_code_again(&diff_of("snap-one"));
    assert_eq!(
        finding_standing::read(&ledger, CHANGE)
            .unwrap()
            .current_revision
            .as_deref(),
        Some("snap-again"),
        "the record does stand on a new version id"
    );
    let r = refused(fix_with_review(&ledger, "r2"));
    assert_eq!(r.code, "evidence_same_code");
    nothing_moved(&ledger);
    assert!(
        view_of(&ledger, &bug()).fix_evidence.is_empty(),
        "the screen offers the unchanged-code review as evidence of a fix"
    );

    // (2) The code moved, and the re-check read an empty diff.
    let ledger = round_one_then_moved_code(&empty_diff());
    assert_eq!(
        refused(fix_with_review(&ledger, "r2")).code,
        "evidence_same_code"
    );
    nothing_moved(&ledger);
    assert!(
        view_of(&ledger, &bug()).fix_evidence.is_empty(),
        "the screen offers a review of an empty diff as evidence of a fix"
    );
}

/// The second sense of "the diff is empty", on its own so it fails on its own:
/// the code moved to another commit, and the re-check read zero bytes of diff.
#[test]
fn a_review_that_read_an_empty_diff_is_not_a_fix() {
    let ledger = round_one_then_moved_code(&empty_diff());
    assert_eq!(
        refused(fix_with_review(&ledger, "r2")).code,
        "evidence_same_code"
    );
    nothing_moved(&ledger);
    assert!(view_of(&ledger, &bug()).fix_evidence.is_empty());
}

/// The same code with a regenerated diff: the reviewer read different bytes,
/// but the code on disk did not move, so nothing about it was repaired.
#[test]
fn a_new_diff_of_the_same_code_is_not_a_fix() {
    let ledger = round_one_then_the_same_code_again(&diff_of("regenerated"));
    assert_eq!(
        refused(fix_with_review(&ledger, "r2")).code,
        "evidence_same_code"
    );
    nothing_moved(&ledger);
    assert!(view_of(&ledger, &bug()).fix_evidence.is_empty());
}

/// The code moved, but the second review read the very diff the finding was
/// raised on: it did not re-run on the changed code, so it shows no fix.
#[test]
fn a_re_review_of_the_same_diff_is_not_a_fix_even_when_the_code_moved() {
    let ledger = round_one_then_moved_code(&diff_of("snap-one"));
    assert_eq!(
        refused(fix_with_review(&ledger, "r2")).code,
        "evidence_same_code"
    );
    nothing_moved(&ledger);
    assert!(view_of(&ledger, &bug()).fix_evidence.is_empty());
}

/// The live control for the three refusals above: the same fixture, the code
/// moved AND the re-check read a new, non-empty diff of it — accepted. Without
/// this, a rule that refused every review would pass them.
#[test]
fn a_re_review_that_read_a_new_diff_of_moved_code_shows_the_fix() {
    let ledger = round_one_then_moved_code(&diff_of("snap-two"));
    assert_eq!(view_of(&ledger, &bug()).fix_evidence.len(), 1);
    recorded(fix_with_review(&ledger, "r2"));
    let f = view_of(&ledger, &bug());
    assert!(f.counts_as_fixed);
    assert_eq!(f.group, Group::Fixed);
}

// ── rule 4: setting aside needs a reason and an authority (GUARD B) ──────

/// GUARD B.
#[test]
fn setting_a_finding_aside_without_a_reason_or_an_authority_is_refused_and_says_which() {
    let ledger = two_rounds();
    for to in [
        Standing::Disputed,
        Standing::Waived,
        Standing::Informational,
    ] {
        let bare = act(&note(), to);
        let r = refused(finding_standing::record(&ledger, CHANGE, &bare).unwrap());
        assert_eq!(r.code, "missing");
        assert_eq!(r.missing, vec!["reason", "authority"], "{to:?}");

        let mut no_authority = act(&note(), to);
        no_authority.reason = "It is a wording preference.".into();
        let r = refused(finding_standing::record(&ledger, CHANGE, &no_authority).unwrap());
        assert_eq!(r.missing, vec!["authority"], "{to:?}");

        let mut no_reason = act(&note(), to);
        no_reason.authority = "I own this project".into();
        no_reason.reason = "   ".into();
        let r = refused(finding_standing::record(&ledger, CHANGE, &no_reason).unwrap());
        assert_eq!(r.missing, vec!["reason"], "{to:?}");

        let mut nobody = act(&note(), to);
        nobody.actor = " ".into();
        nobody.reason = "It is a wording preference.".into();
        nobody.authority = "I own this project".into();
        let r = refused(finding_standing::record(&ledger, CHANGE, &nobody).unwrap());
        assert_eq!(r.missing, vec!["actor"], "{to:?}");
    }
    assert_eq!(rows(&ledger), 0);
}

/// ADV-review-input-1 (adversary F3): text nobody can see is empty. Each
/// sample is invisible on its own — zero-width space, non-joiner and joiner,
/// word joiner, byte-order mark, soft hyphen, Mongolian vowel separator, a
/// bidi override and isolate, the Hangul fillers, a variation selector, a tag
/// character, the blank braille pattern — alone and mixed with white space.
/// `review-model.test.js` holds the same list for the screen's form.
const INVISIBLE: &[&str] = &[
    "\u{200B}",
    "\u{200C}",
    "\u{200D}",
    "\u{2060}",
    "\u{FEFF}",
    "\u{00AD}",
    "\u{180E}",
    "\u{202E}",
    "\u{2066}",
    "\u{3164}",
    "\u{115F}",
    "\u{FE0F}",
    "\u{E0020}",
    "\u{2800}",
    " \u{200B}\u{00A0}\u{3000}\t\n\u{FEFF} ",
];

#[test]
fn a_reason_a_name_or_an_authority_nobody_can_see_is_empty_and_refused() {
    let ledger = two_rounds();
    for to in [
        Standing::Disputed,
        Standing::Waived,
        Standing::Informational,
    ] {
        for blank in INVISIBLE {
            let mut a = act(&note(), to);
            a.actor = (*blank).into();
            a.authority = (*blank).into();
            a.reason = (*blank).into();
            let r = refused(finding_standing::record(&ledger, CHANGE, &a).unwrap());
            assert_eq!(r.code, "missing", "{to:?} {blank:?}");
            assert_eq!(
                r.missing,
                vec!["actor", "reason", "authority"],
                "{to:?} {blank:?}"
            );
        }
    }
    // A move that needs only a name is refused on an invisible one too.
    let mut nameless = act(&bug(), Standing::UnderRepair);
    nameless.actor = "\u{200B}".into();
    let r = refused(finding_standing::record(&ledger, CHANGE, &nameless).unwrap());
    assert_eq!(r.missing, vec!["actor"]);
    assert_eq!(rows(&ledger), 0, "nothing invisible was written");
}

#[test]
fn what_is_saved_is_the_visible_text_without_its_invisible_edges() {
    let ledger = two_rounds();
    let mut a = act(&note(), Standing::Waived);
    a.actor = "\u{200B}Sam\u{FEFF}".into();
    a.authority = "\u{2060} I own this project\u{200D}".into();
    a.reason = "\u{00A0}The wording is intentional; a\u{200D}b stays.\u{200B}\n".into();
    recorded(finding_standing::record(&ledger, CHANGE, &a).unwrap());
    let row = view_of(&ledger, &note()).latest.unwrap();
    assert_eq!(row.actor, "Sam");
    assert_eq!(row.authority.as_deref(), Some("I own this project"));
    assert_eq!(
        row.reason, "The wording is intentional; a\u{200D}b stays.",
        "only the edges go; a joiner inside the text is kept"
    );
}

/// ADV-review-input-2 (adversary F4): one limit for what a person types, held
/// here so both doors refuse the same thing. Each of the name, the authority
/// and the reason may be 2,000 characters — characters, not bytes, counted
/// after the invisible edges go — and a part over it is refused `too_long`,
/// naming every part that is over, whatever the move. Nothing is written. The
/// Refusal's parts are read through its served shape, which is what a door
/// sees.
#[test]
fn a_part_over_two_thousand_characters_is_refused_by_name_and_nothing_is_written() {
    let ledger = two_rounds();
    let too_long =
        |r: &finding_standing::Refusal| serde_json::to_value(r).unwrap()["too_long"].clone();

    let mut cjk = act(&note(), Standing::Waived);
    cjk.authority = "I own this project".into();
    cjk.reason = "\u{8A9E}".repeat(2001);
    let r = refused(finding_standing::record(&ledger, CHANGE, &cjk).unwrap());
    assert_eq!(r.code, "too_long");
    assert_eq!(too_long(&r), serde_json::json!(["reason"]));

    let mut all = act(&note(), Standing::Disputed);
    all.actor = "\u{1F600}".repeat(2001);
    all.authority = "a".repeat(2001);
    all.reason = "b".repeat(2001);
    let r = refused(finding_standing::record(&ledger, CHANGE, &all).unwrap());
    assert_eq!(r.code, "too_long");
    assert_eq!(
        too_long(&r),
        serde_json::json!(["actor", "reason", "authority"])
    );

    // A move that needs no reason still may not store one over the limit.
    let mut repair = act(&bug(), Standing::UnderRepair);
    repair.reason = "c".repeat(2001);
    let r = refused(finding_standing::record(&ledger, CHANGE, &repair).unwrap());
    assert_eq!(r.code, "too_long");
    assert_eq!(too_long(&r), serde_json::json!(["reason"]));
    assert_eq!(rows(&ledger), 0, "nothing over the limit was written");
}

#[test]
fn a_part_of_exactly_two_thousand_characters_is_saved_whole() {
    let ledger = two_rounds();
    let mut a = act(&note(), Standing::Waived);
    // 2,000 characters, 4,000 UTF-16 units and 8,000 bytes: the count is
    // characters. The invisible edges are not counted and are not kept.
    a.actor = format!("\u{200B}{}\u{FEFF}", "\u{1F600}".repeat(2000));
    a.authority = "\u{8A9E}".repeat(2000);
    a.reason = format!(" {} \n", "\u{8BED}".repeat(2000));
    recorded(finding_standing::record(&ledger, CHANGE, &a).unwrap());
    let row = view_of(&ledger, &note()).latest.unwrap();
    assert_eq!(row.reason, "\u{8BED}".repeat(2000));
    assert_eq!(
        row.authority.as_deref().map(|s| s.chars().count()),
        Some(2000)
    );
    assert_eq!(row.actor.chars().count(), 2000);
    assert_eq!(rows(&ledger), 1);
}

/// ADV-review-input-4 (the pre-gate's variant of F3): a part counts as
/// present only if, once its invisible edges are gone, it holds at least one
/// character a person can read — a letter, a number, punctuation or a symbol
/// (general category L, N, P or S) that is not one of the invisible
/// characters. Each sample holds none: the interlinear annotation characters
/// U+FFF9–U+FFFB (format characters that are not default-ignorable, and that
/// Chrome draws with no ink), an Egyptian hieroglyph format control, a
/// combining mark with nothing to sit on (`Mn`, `Mc` and `Me`), a private-use
/// and an unassigned code point, and those mixed with invisible ones. And the
/// object replacement character U+FFFC: a symbol by category, but the one
/// character of every L, N, P and S code point that a headless Chrome sweep
/// found drawn with no ink (and no width), so it is invisible like U+2800.
/// `review-model.test.js` holds the same list for the screen's form.
const UNREADABLE: &[&str] = &[
    "\u{FFFC}",
    "\u{FFF9}",
    "\u{FFFA}",
    "\u{FFFB}",
    "\u{FFF9}\u{FFFB}",
    "\u{FFF9}\u{FFFA}\u{FFFB}",
    "\u{13430}",
    "\u{0301}",
    "\u{0903}",
    "\u{20DD}",
    "\u{E000}",
    "\u{0378}",
    " \u{200B}\u{FFFA}\u{0301}\u{00A0}\u{FFFB}\n",
];

#[test]
fn a_part_that_holds_nothing_a_person_can_read_is_empty_and_refused() {
    let ledger = two_rounds();
    for to in [
        Standing::Disputed,
        Standing::Waived,
        Standing::Informational,
    ] {
        for blank in UNREADABLE {
            let mut a = act(&note(), to);
            a.actor = (*blank).into();
            a.authority = (*blank).into();
            a.reason = (*blank).into();
            let r = refused(finding_standing::record(&ledger, CHANGE, &a).unwrap());
            assert_eq!(r.code, "missing", "{to:?} {blank:?}");
            assert_eq!(
                r.missing,
                vec!["actor", "reason", "authority"],
                "{to:?} {blank:?}"
            );
        }
    }
    // The pre-gate's own acts: a named person, and an authority and a reason
    // that draw nothing (the terminal's) or only an accent (the route's).
    for (authority, reason) in [("\u{FFFA}", "\u{FFF9}\u{FFFB}"), ("\u{FFF9}", "\u{0301}")] {
        let mut a = act(&note(), Standing::Waived);
        a.actor = "Sam".into();
        a.authority = authority.into();
        a.reason = reason.into();
        let r = refused(finding_standing::record(&ledger, CHANGE, &a).unwrap());
        assert_eq!(
            r.missing,
            vec!["reason", "authority"],
            "{authority:?} {reason:?}"
        );
    }
    // A move that needs only a name is refused on an unreadable one too.
    let mut nameless = act(&bug(), Standing::UnderRepair);
    nameless.actor = "\u{FFFA}".into();
    let r = refused(finding_standing::record(&ledger, CHANGE, &nameless).unwrap());
    assert_eq!(r.missing, vec!["actor"]);
    assert_eq!(rows(&ledger), 0, "nothing unreadable was written");
}

#[test]
fn one_readable_character_makes_a_part_present_and_it_is_saved_as_typed() {
    let ledger = two_rounds();
    // A letter under its accent, a precomposed letter, a digit, a full stop,
    // a symbol, a CJK letter: each is enough, and each is kept whole.
    let samples = ["e\u{0301}", "\u{00E9}", "7", ".", "\u{1F600}", "\u{8A9E}"];
    for (i, sample) in samples.iter().enumerate() {
        let to = if i % 2 == 0 {
            Standing::Disputed
        } else {
            Standing::Informational
        };
        let mut a = act(&note(), to);
        a.actor = (*sample).into();
        a.authority = (*sample).into();
        a.reason = (*sample).into();
        recorded(finding_standing::record(&ledger, CHANGE, &a).unwrap());
        let row = view_of(&ledger, &note()).latest.unwrap();
        assert_eq!(row.actor, *sample);
        assert_eq!(row.authority.as_deref(), Some(*sample));
        assert_eq!(row.reason, *sample);
    }
    assert_eq!(rows(&ledger), samples.len() as i64);
}

#[test]
fn a_reason_nobody_can_read_is_saved_as_nothing_where_none_is_needed() {
    let ledger = two_rounds();
    // `under_repair` needs only a name; what else is typed is kept only if a
    // person can read it, so no row carries a reason that draws nothing.
    let mut a = act(&bug(), Standing::UnderRepair);
    a.reason = "\u{FFF9}\u{0301}\u{FFFB}".into();
    a.authority = "\u{FFFA}".into();
    recorded(finding_standing::record(&ledger, CHANGE, &a).unwrap());
    let row = view_of(&ledger, &bug()).latest.unwrap();
    assert_eq!(
        row.reason, "",
        "a reason with nothing readable is saved as none"
    );
    assert_eq!(row.authority, None, "so is an authority");
    assert_eq!(row.actor, "sam");
}

/// The engine's own walk over every character (the screen's is in
/// `review-model.test.js`). Node's ICU counts 152,308 code points whose
/// category is L, N, P or S in Unicode 16; six of them are in the invisible
/// set — the Hangul fillers U+115F, U+1160, U+3164 and U+FFA0, the blank
/// braille pattern U+2800 and the object replacement character U+FFFC — so
/// 152,302 characters are legible, and no invisible one is.
#[test]
fn the_legible_characters_are_unicodes_letters_numbers_punctuation_and_symbols_less_the_invisible_ones(
) {
    let mut legible = 0usize;
    for c in (0..=0x10FFFFu32).filter_map(char::from_u32) {
        if finding_standing::is_legible(c) {
            legible += 1;
            assert!(!finding_standing::is_invisible(c), "{:#X}", c as u32);
        }
    }
    assert_eq!(legible, 152_302);
    for c in [
        '\u{115F}', '\u{1160}', '\u{3164}', '\u{FFA0}', '\u{2800}', '\u{FFFC}',
    ] {
        assert!(!finding_standing::is_legible(c), "{:#X}", c as u32);
    }
    assert_eq!(finding_standing::visible_text("\u{FFF9}\u{0301}"), "");
    assert_eq!(
        finding_standing::visible_text("\u{200B}e\u{0301}\u{FFFB}"),
        "e\u{0301}\u{FFFB}"
    );
}

#[test]
fn a_waived_finding_keeps_its_reason_and_who_and_is_never_counted_as_fixed() {
    let ledger = two_rounds();
    let mut a = act(&note(), Standing::Waived);
    a.reason = "The readme wording is intentional.".into();
    a.authority = "I own this project".into();
    recorded(finding_standing::record(&ledger, CHANGE, &a).unwrap());
    let view = finding_standing::read(&ledger, CHANGE).unwrap();
    let f = view.findings.iter().find(|f| f.key == note()).unwrap();
    assert_eq!(f.standing, Standing::Waived);
    assert_eq!(f.group, Group::SetAside);
    assert!(!f.counts_as_fixed);
    let row = f.latest.as_ref().unwrap();
    assert_eq!(row.actor, "sam");
    assert_eq!(row.authority.as_deref(), Some("I own this project"));
    assert_eq!(row.reason, "The readme wording is intentional.");
    assert_eq!(view.summary.set_aside, 1);
    assert_eq!(view.summary.fixed, 0);
    // Informational has its own list.
    let mut i = act(&nit(), Standing::Informational);
    i.reason = "Style only; no code change needed.".into();
    i.authority = "the reviewer who raised it".into();
    recorded(finding_standing::record(&ledger, CHANGE, &i).unwrap());
    let view = finding_standing::read(&ledger, CHANGE).unwrap();
    assert_eq!(view.summary.informational, 1);
    assert_eq!(view.summary.fixed, 0);
}

#[test]
fn disputed_is_its_own_standing_with_its_own_reason() {
    let ledger = two_rounds();
    let mut a = act(&note(), Standing::Disputed);
    a.reason = "False positive: the line it names is a comment.".into();
    a.authority = "I own this project".into();
    recorded(finding_standing::record(&ledger, CHANGE, &a).unwrap());
    let f = view_of(&ledger, &note());
    assert_eq!(f.standing, Standing::Disputed);
    assert_eq!(f.group, Group::SetAside);
    assert!(!f.counts_as_fixed);
}

// ── rule 3: a resolved mark is not a fix (GUARD C) ───────────────────────

#[test]
fn a_resolved_mark_reaches_waiting_for_a_re_check_and_is_not_a_fix() {
    let ledger = two_rounds();
    for via in [Via::ResolvedMark, Via::SyntheticResolvedImport] {
        let ledger = two_rounds();
        let mut a = act(&bug(), Standing::AwaitingVerification);
        a.via = via;
        recorded(finding_standing::record(&ledger, CHANGE, &a).unwrap());
        let f = view_of(&ledger, &bug());
        assert_eq!(f.standing, Standing::AwaitingVerification);
        assert_eq!(f.group, Group::AwaitingRecheck);
        assert!(!f.counts_as_fixed);
        assert_eq!(f.latest.unwrap().via, via);
    }
    drop(ledger);
}

/// GUARD C.
#[test]
fn a_resolved_mark_never_reaches_fixed_even_with_evidence() {
    let ledger = two_rounds();
    for via in [Via::ResolvedMark, Via::SyntheticResolvedImport] {
        for to in [
            Standing::FixedWithEvidence,
            Standing::Waived,
            Standing::Open,
        ] {
            let mut a = act(&bug(), to);
            a.via = via;
            a.reason = "closed in the thread".into();
            a.authority = "the thread".into();
            if to == Standing::FixedWithEvidence {
                a.evidence = Some(EvidenceRef {
                    kind: EvidenceKind::Review,
                    id: "r2".into(),
                });
            }
            let r = refused(finding_standing::record(&ledger, CHANGE, &a).unwrap());
            assert_eq!(r.code, "mark_goes_no_further", "{via:?} → {to:?}");
        }
    }
    assert_eq!(rows(&ledger), 0);
}

#[test]
fn only_a_resolved_mark_means_waiting_for_a_re_check() {
    let ledger = two_rounds();
    let r = refused(
        finding_standing::record(
            &ledger,
            CHANGE,
            &act(&bug(), Standing::AwaitingVerification),
        )
        .unwrap(),
    );
    assert_eq!(r.code, "mark_goes_no_further");
}

#[test]
fn a_resolved_mark_on_a_finding_that_is_not_open_is_refused() {
    let ledger = two_rounds();
    let mut w = act(&note(), Standing::Waived);
    w.reason = "intentional".into();
    w.authority = "I own this project".into();
    recorded(finding_standing::record(&ledger, CHANGE, &w).unwrap());
    let mut m = act(&note(), Standing::AwaitingVerification);
    m.via = Via::ResolvedMark;
    let r = refused(finding_standing::record(&ledger, CHANGE, &m).unwrap());
    assert_eq!(r.code, "mark_needs_an_open_finding");
    assert_eq!(r.current, Some(Standing::Waived));
}

/// The phase-exit case: mark a finding resolved with no re-check. Neither the
/// finding nor the Change's readiness treats it as fixed.
#[test]
fn the_phase_exit_case_a_resolved_mark_moves_neither_the_finding_nor_readiness() {
    let ledger = two_rounds();
    let env = thruline_core::change_propose::ProfileEnvironment::empty();
    let readiness_json = |ledger: &Ledger| {
        let coverage = thruline_core::change_evidence::coverage(ledger, CHANGE, &env).unwrap();
        serde_json::to_value(
            thruline_core::change_readiness::readiness(ledger, CHANGE, &coverage).unwrap(),
        )
        .unwrap()
    };
    let before = readiness_json(&ledger);
    let mut m = act(&bug(), Standing::AwaitingVerification);
    m.via = Via::ResolvedMark;
    recorded(finding_standing::record(&ledger, CHANGE, &m).unwrap());
    let f = view_of(&ledger, &bug());
    assert!(!f.counts_as_fixed);
    assert_ne!(f.group, Group::Fixed);
    let after = readiness_json(&ledger);
    assert_eq!(
        before, after,
        "a resolved mark changes nothing readiness reads"
    );
    assert_eq!(after["ready"], serde_json::Value::Bool(false));
    let summary = finding_standing::read(&ledger, CHANGE).unwrap().summary;
    assert_eq!(summary.fixed, 0);
    assert_eq!(summary.awaiting_recheck, 1);
}

// ── the other refusals ───────────────────────────────────────────────────

#[test]
fn no_such_finding_unchanged_and_moved_are_refused() {
    let ledger = two_rounds();
    for key in ["r1:9", "nope:0", "r1", "", "r1:x"] {
        let r = refused(
            finding_standing::record(&ledger, CHANGE, &act(key, Standing::UnderRepair)).unwrap(),
        );
        assert_eq!(r.code, "no_such_finding", "{key}");
    }
    // A finding of another Change is not reachable through this one.
    let r = refused(
        finding_standing::record(&ledger, "chg-other", &act(&bug(), Standing::UnderRepair))
            .unwrap(),
    );
    assert_eq!(r.code, "no_such_finding");

    let r =
        refused(finding_standing::record(&ledger, CHANGE, &act(&bug(), Standing::Open)).unwrap());
    assert_eq!(r.code, "unchanged");

    recorded(
        finding_standing::record(&ledger, CHANGE, &act(&bug(), Standing::UnderRepair)).unwrap(),
    );
    let mut stale = act(&bug(), Standing::Open);
    stale.seen = Some(Standing::Open);
    let r = refused(finding_standing::record(&ledger, CHANGE, &stale).unwrap());
    assert_eq!(r.code, "moved");
    assert_eq!(r.current, Some(Standing::UnderRepair));
}

#[test]
fn a_finding_of_an_altered_review_cannot_be_acted_on() {
    let ledger = two_rounds();
    // Tamper a child row of round 1 underneath its seal: the only way to do it
    // is to drop the append-only trigger, which is exactly the attack.
    ledger
        .conn()
        .execute_batch(
            "DROP TRIGGER change_review_findings_no_update;
             UPDATE change_review_findings SET message='rewritten' WHERE review_id='r1' AND ordinal=0;",
        )
        .unwrap();
    let r = refused(
        finding_standing::record(&ledger, CHANGE, &act(&bug(), Standing::UnderRepair)).unwrap(),
    );
    assert_eq!(r.code, "finding_not_readable");
    assert!(finding_standing::read(&ledger, CHANGE)
        .unwrap()
        .findings
        .iter()
        .all(|f| f.review_id != "r1"));
}

#[test]
fn every_refusal_code_is_in_the_published_list_and_every_published_one_is_used() {
    // A screen renders a sentence for each published code (`review-model.test.js`
    // holds that side); a code returned and not published would reach it bare.
    let published: std::collections::BTreeSet<&str> =
        finding_standing::REFUSAL_CODES.iter().copied().collect();
    assert_eq!(published.len(), finding_standing::REFUSAL_CODES.len());
    let source = include_str!("../src/finding_standing.rs");
    let mut returned = std::collections::BTreeSet::new();
    for marker in ["Refusal::because(\"", "code: \"", "Err(\""] {
        for piece in source.split(marker).skip(1) {
            let code = piece.split('"').next().unwrap();
            if code.chars().all(|c| c.is_ascii_lowercase() || c == '_') && !code.is_empty() {
                returned.insert(code);
            }
        }
    }
    assert!(
        returned.len() >= 15,
        "the scan found the codes: {returned:?}"
    );
    for code in &returned {
        assert!(
            published.contains(code),
            "{code} is returned and not published"
        );
    }
    for code in &published {
        assert!(
            returned.contains(code),
            "{code} is published and never returned"
        );
    }
}

// ── rule 1: append-only, and the history is the story ─────────────────────

#[test]
fn every_change_of_standing_is_a_new_row_and_the_history_keeps_all_of_them() {
    let ledger = two_rounds();
    recorded(
        finding_standing::record(&ledger, CHANGE, &act(&bug(), Standing::UnderRepair)).unwrap(),
    );
    let mut m = act(&bug(), Standing::AwaitingVerification);
    m.via = Via::ResolvedMark;
    recorded(finding_standing::record(&ledger, CHANGE, &m).unwrap());
    let mut fix = act(&bug(), Standing::FixedWithEvidence);
    fix.evidence = Some(EvidenceRef {
        kind: EvidenceKind::Review,
        id: "r2".into(),
    });
    recorded(finding_standing::record(&ledger, CHANGE, &fix).unwrap());
    let mut reopen = act(&bug(), Standing::Open);
    reopen.reason = "It came back.".into();
    recorded(finding_standing::record(&ledger, CHANGE, &reopen).unwrap());
    let f = view_of(&ledger, &bug());
    let path: Vec<(Standing, Standing)> = f.history.iter().map(|r| (r.from, r.to)).collect();
    assert_eq!(
        path,
        vec![
            (Standing::Open, Standing::UnderRepair),
            (Standing::UnderRepair, Standing::AwaitingVerification),
            (Standing::AwaitingVerification, Standing::FixedWithEvidence),
            (Standing::FixedWithEvidence, Standing::Open),
        ]
    );
    assert_eq!(f.standing, Standing::Open);
    assert!(f.decided, "reopened is a decision, and it is recorded");
    assert_eq!(rows(&ledger), 4);
}

#[test]
fn update_and_delete_are_refused_by_the_store() {
    let ledger = two_rounds();
    recorded(
        finding_standing::record(&ledger, CHANGE, &act(&bug(), Standing::UnderRepair)).unwrap(),
    );
    let update = ledger.conn().execute(
        "UPDATE change_review_finding_standings SET to_standing='waived'",
        [],
    );
    assert!(update.unwrap_err().to_string().contains("append-only"));
    let delete = ledger
        .conn()
        .execute("DELETE FROM change_review_finding_standings", []);
    assert!(delete.unwrap_err().to_string().contains("append-only"));
    assert_eq!(rows(&ledger), 1);
    assert_eq!(view_of(&ledger, &bug()).standing, Standing::UnderRepair);
}

/// Every column a raw writer would supply, spelled out on purpose.
#[allow(clippy::too_many_arguments)]
fn raw_insert(
    ledger: &Ledger,
    id: &str,
    from: &str,
    to: &str,
    via: &str,
    reason: &str,
    authority: Option<&str>,
    evidence: Option<(&str, &str, &str)>,
) -> rusqlite::Result<usize> {
    ledger.conn().execute(
        "INSERT INTO change_review_finding_standings(id,change_id,review_id,ordinal,from_standing,
             to_standing,via,actor,authority,reason,evidence_kind,evidence_id,evidence_revision,
             recorded_at)
         VALUES(?1,?2,'r1',0,?3,?4,?5,'sam',?6,?7,?8,?9,?10,'2026-01-04T00:00:00Z')",
        params![
            id,
            CHANGE,
            from,
            to,
            via,
            authority,
            reason,
            evidence.map(|e| e.0),
            evidence.map(|e| e.1),
            evidence.map(|e| e.2)
        ],
    )
}

/// The table's own second guard: a writer that skipped [`record`] still
/// cannot store what the rules forbid.
#[test]
fn the_store_refuses_rows_the_rules_forbid_even_without_the_core_function() {
    let ledger = two_rounds();
    // Fixed without evidence.
    assert!(raw_insert(
        &ledger,
        "x1",
        "open",
        "fixed_with_evidence",
        "person",
        "",
        None,
        None
    )
    .is_err());
    // A resolved mark reaching fixed.
    assert!(raw_insert(
        &ledger,
        "x2",
        "open",
        "fixed_with_evidence",
        "resolved_mark",
        "",
        None,
        Some(("review", "r2", "snap-two"))
    )
    .is_err());
    // A person writing "awaiting" without a mark.
    assert!(raw_insert(
        &ledger,
        "x3",
        "open",
        "awaiting_verification",
        "person",
        "",
        None,
        None
    )
    .is_err());
    // Waived with no reason, and with no authority.
    assert!(raw_insert(
        &ledger,
        "x4",
        "open",
        "waived",
        "person",
        "",
        Some("owner"),
        None
    )
    .is_err());
    assert!(raw_insert(&ledger, "x5", "open", "waived", "person", "why", None, None).is_err());
    // A row that does not start where the finding stands.
    assert!(raw_insert(
        &ledger,
        "x6",
        "under_repair",
        "open",
        "person",
        "",
        None,
        None
    )
    .is_err());
    // An unknown standing, and a real-thread source that does not exist.
    assert!(raw_insert(&ledger, "x7", "open", "resolved", "person", "", None, None).is_err());
    assert!(raw_insert(
        &ledger,
        "x8",
        "open",
        "awaiting_verification",
        "provider_thread",
        "",
        None,
        None
    )
    .is_err());
    assert_eq!(rows(&ledger), 0);
    // And a well-formed one is accepted, so the refusals above are the rules.
    raw_insert(
        &ledger,
        "x9",
        "open",
        "under_repair",
        "person",
        "",
        None,
        None,
    )
    .unwrap();
    assert!(
        raw_insert(
            &ledger,
            "x9",
            "under_repair",
            "open",
            "person",
            "",
            None,
            None
        )
        .is_err(),
        "no replace"
    );
}

// ── FIXED-WORDING (R-2026-09-28-9): "Fixed" says what the record shows ───
//
// The founder's sentence is "Fixed: the re-check didn't raise this finding
// at <location>", and beside it every other thing that same re-check raised,
// read from its recorded findings, so a finding that came back renamed and
// reworded is in plain sight next to the word "Fixed".

/// Round 1 raised the three first findings on version one; the code moved to
/// version two; round 2 on version two raised `round_two`.
fn round_two_raising(round_two: Vec<ReviewFinding>) -> Ledger {
    let ledger = prepared();
    change_review::append_review(
        &ledger,
        &review(
            "r1",
            1,
            "snap-one",
            "exi-one",
            ReviewState::Complete,
            first_findings(),
        ),
    )
    .unwrap();
    record_version(&ledger, "snap-two", "exi-two", "2026-01-03T00:00:00Z");
    change_review::append_review(
        &ledger,
        &review(
            "r2",
            2,
            "snap-two",
            "exi-two",
            ReviewState::Complete,
            round_two,
        ),
    )
    .unwrap();
    ledger
}

fn fixed_by_r2(ledger: &Ledger, key: &str) -> finding_standing::FindingStandingView {
    let mut a = act(key, Standing::FixedWithEvidence);
    a.evidence = Some(EvidenceRef {
        kind: EvidenceKind::Review,
        id: "r2".into(),
    });
    recorded(finding_standing::record(ledger, CHANGE, &a).unwrap());
    view_of(ledger, key)
}

/// The rename-and-reword attack: the bug in `calculator.py` comes back in
/// round 2 as `calc.py`, in other words. Nothing in the record ties the new
/// words to the old finding, so the fix is accepted — and the reworded one is
/// listed right beside "Fixed", in its own words and at its own place.
#[test]
fn a_fix_says_only_that_the_re_check_did_not_raise_it_there_and_lists_what_else_it_raised() {
    let reworded = finding("Sum is off", Some("calc.py"), Some(2));
    let ledger = round_two_raising(vec![reworded.clone()]);
    let f = fixed_by_r2(&ledger, &bug());
    assert!(f.counts_as_fixed);
    assert_eq!(f.recheck_raised, Some(vec![reworded]));
    assert_eq!(
        finding_standing::fixed_lines(&f),
        vec![
            "Fixed: the re-check didn't raise this finding at calculator.py, line 2",
            "The same re-check raised 1 other thing. Look at it in case it is this one, reworded or moved:",
            "  - calc.py, line 2: Sum is off — SYNTHETIC: Sum is off",
        ]
    );
    // Served, so the screen reads the same list.
    let json = serde_json::to_value(finding_standing::read(&ledger, CHANGE).unwrap()).unwrap();
    let served = &json["findings"][0]["recheck_raised"];
    assert_eq!(served[0]["path"], "calc.py");
    assert_eq!(served[0]["kind"], "Sum is off");
    assert_eq!(served[0]["message"], "SYNTHETIC: Sum is off");
    assert_eq!(served[0]["line"], 2);
}

#[test]
fn a_re_check_that_raised_nothing_else_says_so_plainly() {
    let ledger = round_two_raising(vec![]);
    let f = fixed_by_r2(&ledger, &bug());
    assert_eq!(f.recheck_raised, Some(vec![]));
    assert_eq!(
        finding_standing::fixed_lines(&f),
        vec![
            "Fixed: the re-check didn't raise this finding at calculator.py, line 2",
            "The same re-check raised nothing else.",
        ]
    );
}

#[test]
fn every_other_thing_the_re_check_raised_is_listed_in_its_order_with_its_place() {
    let ledger = round_two_raising(vec![
        finding("Sum is off", Some("calc.py"), Some(2)),
        finding("Style", None, None),
        finding("Unused import", Some("utils.py"), None),
    ]);
    let f = fixed_by_r2(&ledger, &bug());
    assert_eq!(f.recheck_raised.as_ref().map(Vec::len), Some(3));
    assert_eq!(
        finding_standing::fixed_lines(&f),
        vec![
            "Fixed: the re-check didn't raise this finding at calculator.py, line 2",
            "The same re-check raised 3 other things. Look at them in case one is this one, reworded or moved:",
            "  - calc.py, line 2: Sum is off — SYNTHETIC: Sum is off",
            "  - No file named: Style — SYNTHETIC: Style",
            "  - utils.py: Unused import — SYNTHETIC: Unused import",
        ]
    );
}

#[test]
fn a_finding_that_is_not_fixed_carries_no_fixed_line_and_no_re_check_list() {
    let ledger = two_rounds();
    let f = view_of(&ledger, &bug());
    assert_eq!(f.standing, Standing::Open);
    assert_eq!(f.recheck_raised, None);
    assert!(finding_standing::fixed_lines(&f).is_empty());
}

#[test]
fn the_fixed_line_names_the_place_the_record_holds_and_says_so_when_it_holds_none() {
    let ledger = round_two_raising(vec![]);
    let f = fixed_by_r2(&ledger, &bug());

    let mut file_only = f.clone();
    file_only.line = None;
    assert_eq!(
        finding_standing::fixed_lines(&file_only)[0],
        "Fixed: the re-check didn't raise this finding at calculator.py"
    );
    let mut nowhere = f.clone();
    nowhere.path = None;
    nowhere.line = None;
    assert_eq!(
        finding_standing::fixed_lines(&nowhere)[0],
        "Fixed: the re-check didn't raise this finding (this finding names no file)"
    );

    // A check run raises no review findings: said, never an empty list.
    let mut by_check = f.clone();
    if let Some(e) = by_check.evidence.as_mut() {
        e.kind = EvidenceKind::CheckRun;
    }
    by_check.recheck_raised = None;
    assert_eq!(
        finding_standing::fixed_lines(&by_check)[1..],
        ["That re-check was a check run, and a check run raises no review findings, so there is nothing else to list."]
    );
    // A review whose findings could not be read: said, never "nothing else".
    let mut unread = f;
    unread.recheck_raised = None;
    assert_eq!(
        finding_standing::fixed_lines(&unread)[1..],
        ["What else that re-check raised cannot be listed: its review could not be read."]
    );
}

#[test]
fn a_fix_whose_version_moved_on_still_lists_what_its_re_check_raised() {
    let reworded = finding("Sum is off", Some("calc.py"), Some(2));
    let ledger = round_two_raising(vec![reworded.clone()]);
    fixed_by_r2(&ledger, &bug());
    record_version(&ledger, "snap-three", "exi-three", "2026-01-05T00:00:00Z");
    let f = view_of(&ledger, &bug());
    assert!(!f.counts_as_fixed);
    assert_eq!(f.recheck_raised, Some(vec![reworded]));
}

// ── served shape ─────────────────────────────────────────────────────────

#[test]
fn the_served_shape_names_the_standing_the_group_and_the_version() {
    let ledger = two_rounds();
    let view = finding_standing::read(&ledger, CHANGE).unwrap();
    let json = serde_json::to_value(&view).unwrap();
    assert_eq!(json["schema_version"], 2);
    assert_eq!(json["current_revision"], "snap-two");
    let first = &json["findings"][0];
    assert_eq!(first["key"], "r1:0");
    assert_eq!(first["standing"], "open");
    assert_eq!(first["group"], "still_wrong");
    assert_eq!(first["raised_on"], "snap-one");
    assert_eq!(first["decided"], false);
    assert!(first["recheck_raised"].is_null());
    assert!(json["summary"].is_object());
}

#[test]
fn free_text_a_person_typed_is_masked_before_it_is_served() {
    let ledger = two_rounds();
    let secret = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let mut a = act(&note(), Standing::Waived);
    a.reason = format!("token {secret} is fine");
    a.authority = "I own this project".into();
    recorded(finding_standing::record(&ledger, CHANGE, &a).unwrap());
    let json = serde_json::to_string(&finding_standing::read(&ledger, CHANGE).unwrap()).unwrap();
    assert!(!json.contains(secret), "a secret reached the served view");
}

// ── a passing check run as evidence ──────────────────────────────────────

mod check_run {
    use super::*;
    use thruline_core::evidence_binding::{
        self, CommandScope, EvidenceBinding, ExecMode, IdentityEdge, ScopeClass, StreamCapture,
        WsidScopeDecl,
    };
    use thruline_core::examined_inputs::{self, ExaminedInputs, Input, SubjectKind, Uncommitted};

    const C: &str = "c1";
    const RUN_KEY: &str = "s_run";
    const EMPTY_SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn inputs(kind: SubjectKind, subject: &str, base: &str) -> ExaminedInputs {
        let mut i = ExaminedInputs::unknown_for(kind, subject);
        i.base_revision = Input::known(base);
        i.worktree_path = Input::known("/tmp/synthetic");
        i.source_identity = Input::known(format!("wsid1:{base}"));
        i.uncommitted = Uncommitted::Clean;
        i
    }

    fn binding(exit: i64, exclusions: Vec<String>) -> EvidenceBinding {
        EvidenceBinding {
            argv: vec!["cargo".into(), "build".into()],
            mode: ExecMode::Direct,
            cwd: "/tmp/synthetic".into(),
            root: "/tmp/synthetic".into(),
            repository_identity: "repo".into(),
            scope: CommandScope {
                class: ScopeClass::Build,
                key: "bash:/tmp/synthetic:cargo build".into(),
            },
            wsid_scope: WsidScopeDecl {
                include_untracked: true,
                exclusions,
            },
            pre: IdentityEdge::complete(format!("wsid1:{}", "ab".repeat(32))),
            post: IdentityEdge::complete(format!("wsid1:{}", "ab".repeat(32))),
            exit_code: Some(exit),
            term_signal: None,
            stdout: StreamCapture::Captured {
                sha256: EMPTY_SHA.into(),
                bytes: 0,
            },
            stderr: StreamCapture::Captured {
                sha256: EMPTY_SHA.into(),
                bytes: 0,
            },
            artifacts: vec![],
            wrapper_defects: vec![],
            started_at: "2026-01-06T09:00:00Z".into(),
            bound_at: "2026-01-06T09:00:05Z".into(),
            source_adapter: "thruline".into(),
            source_session: RUN_KEY.into(),
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        ledger: Ledger,
    }

    /// A Change on a real on-disk ledger with a confirmed session inside its
    /// scope; round 1 raised a finding in `crates/core/a.rs` on version one;
    /// the code moved to version two.
    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let ledger = Ledger::open(dir.path().join("ledger.db")).unwrap();
        let root = dir.path().canonicalize().unwrap().display().to_string();
        let conn = ledger.conn();
        conn.execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('p1',?1,'claude_code','Synthetic','2026-01-01T00:00:00Z')",
            params![root],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO workspace_identities(project_id, repo_id, basis, resolved_root,
                 resolved_via, sessions_sampled, observed_at)
             VALUES('p1','repo','git',?1,'recorded_cwd',1,'2026-01-01T00:00:00Z')",
            params![root],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES(?1,'p1',?2,'2026-01-01T00:00:00Z')",
            params![RUN_KEY, format!("/tmp/synthetic/{RUN_KEY}.jsonl")],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('e1','p1',?1,1,'2026-01-01T00:10:00Z','tool_use','Edit',?2,
                    '/tmp/synthetic/x.jsonl',1,'aa','','bb')",
            params![
                RUN_KEY,
                format!("{{\"file_path\":\"{root}/crates/core/a.rs\"}}")
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester,
                 request_source)
             VALUES('c1','p1','2026-01-02T09:00:00Z','Keep it building','Build.','sam','person')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at,
                 criteria_json, exclusions_json, allowed_scope_json, constraints_json,
                 check_profile_json, content_sha256)
             VALUES('c1',1,'2026-01-02T09:30:00Z','[\"The build passes.\"]','[]',
                    '[\"crates/**\"]','[]','[\"cargo-build\"]','digest')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by,
                 accepted_at, rationale)
             VALUES('c1',1,'sam','2026-01-02T09:40:00Z','this is what counts')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO change_session_associations(id, change_id, session_id, provenance,
                 basis, actor, asserted_at, reason)
             VALUES('a1','c1',?1,'confirmed','user','sam','2026-01-02T10:00:00Z','mine')",
            params![RUN_KEY],
        )
        .unwrap();
        let one = inputs(SubjectKind::Intended, C, "rev-one");
        let one_id = examined_inputs::record(&ledger, &one, "2026-01-02T09:45:00Z")
            .unwrap()
            .id;
        let mut r1 = review(
            "k1",
            1,
            "rev-one",
            &one_id,
            ReviewState::Complete,
            vec![finding(
                "Missing bounds check",
                Some("crates/core/a.rs"),
                Some(10),
            )],
        );
        r1.change_id = C.into();
        r1.scope_version = Some(1);
        change_review::append_review(&ledger, &r1).unwrap();
        let two = inputs(SubjectKind::Intended, C, "rev-two");
        examined_inputs::record(&ledger, &two, "2026-01-06T08:00:00Z").unwrap();
        Fixture { _dir: dir, ledger }
    }

    fn run(f: &Fixture, exit: i64, exclusions: Vec<String>, base: &str) -> String {
        let stored = evidence_binding::store(&f.ledger, &binding(exit, exclusions)).unwrap();
        let id = stored.stored().id.clone();
        examined_inputs::record(
            &f.ledger,
            &inputs(SubjectKind::CheckRun, &id, base),
            "2026-01-06T09:00:05Z",
        )
        .unwrap();
        id
    }

    fn fix_with(f: &Fixture, run: &str) -> Outcome {
        let mut a = act(&finding_key("k1", 0), Standing::FixedWithEvidence);
        a.evidence = Some(EvidenceRef {
            kind: EvidenceKind::CheckRun,
            id: run.into(),
        });
        finding_standing::record(&f.ledger, C, &a).unwrap()
    }

    #[test]
    fn a_passing_check_on_the_new_version_that_covers_the_file_shows_the_fix() {
        let f = fixture();
        let id = run(&f, 0, vec![], "rev-two");
        let offered = finding_standing::read(&f.ledger, C).unwrap().findings[0]
            .fix_evidence
            .clone();
        assert_eq!(offered.len(), 1);
        assert_eq!(offered[0].kind, EvidenceKind::CheckRun);
        recorded(fix_with(&f, &id));
        let view = finding_standing::read(&f.ledger, C).unwrap();
        assert!(view.findings[0].counts_as_fixed);
        assert_eq!(
            view.findings[0].evidence.as_ref().unwrap().revision,
            "rev-two"
        );
    }

    #[test]
    fn a_failing_check_is_not_a_fix() {
        let f = fixture();
        let id = run(&f, 1, vec![], "rev-two");
        assert_eq!(refused(fix_with(&f, &id)).code, "evidence_not_passing");
    }

    #[test]
    fn a_check_on_another_version_is_not_a_fix() {
        let f = fixture();
        let id = run(&f, 0, vec![], "rev-one");
        assert_eq!(refused(fix_with(&f, &id)).code, "evidence_not_current");
    }

    #[test]
    fn a_check_whose_scope_leaves_the_file_out_is_not_a_fix() {
        let f = fixture();
        let id = run(&f, 0, vec!["crates/".into()], "rev-two");
        assert_eq!(refused(fix_with(&f, &id)).code, "evidence_outside_scope");
    }

    #[test]
    fn a_check_cannot_cover_a_finding_that_names_no_file() {
        let f = fixture();
        let two_id = thruline_core::change_review::standing_intended_id(&f.ledger, C)
            .unwrap()
            .unwrap();
        let mut r2 = review(
            "k2",
            2,
            "rev-two",
            &two_id,
            ReviewState::Complete,
            vec![finding("General unease", None, None)],
        );
        r2.change_id = C.into();
        change_review::append_review(&f.ledger, &r2).unwrap();
        record_three(&f);
        let id = run(&f, 0, vec![], "rev-three");
        let mut a = act(&finding_key("k2", 0), Standing::FixedWithEvidence);
        a.evidence = Some(EvidenceRef {
            kind: EvidenceKind::CheckRun,
            id,
        });
        assert_eq!(
            refused(finding_standing::record(&f.ledger, C, &a).unwrap()).code,
            "evidence_no_place"
        );
    }

    fn record_three(f: &Fixture) {
        examined_inputs::record(
            &f.ledger,
            &inputs(SubjectKind::Intended, C, "rev-three"),
            "2026-01-06T08:30:00Z",
        )
        .unwrap();
    }

    #[test]
    fn a_check_on_the_version_the_finding_was_raised_on_is_not_later() {
        let f = fixture();
        // Back to version one: the intended record names rev-one again.
        let back = inputs(SubjectKind::Intended, C, "rev-one");
        f.ledger
            .conn()
            .execute(
                "INSERT INTO examination_inputs(id,subject_kind,subject_id,recorded_at,
                     schema_version,base_revision,source_identity,uncommitted_state,
                     worktree_path,config_files_json,tools_json,target_kind)
                 VALUES('exi-back','intended',?1,'2026-01-07T00:00:00Z',1,'rev-one',
                     'wsid1:rev-one','clean','/tmp/synthetic','[]','[]','none')",
                params![C],
            )
            .unwrap();
        drop(back);
        let id = run(&f, 0, vec![], "rev-one");
        assert_eq!(refused(fix_with(&f, &id)).code, "evidence_not_later");
    }

    /// ADV-review-truth-1, the CheckRun half: `change prepare` again with
    /// nothing changed since the finding was raised — a new snapshot id on
    /// the code of version one — and a passing check run on exactly that
    /// code. The version id moved; the code did not, so the check shows no fix.
    #[test]
    fn a_check_on_a_new_version_id_of_the_same_code_is_not_a_fix() {
        let f = fixture();
        f.ledger
            .conn()
            .execute(
                "INSERT INTO workspace_snapshots(id,taken_at,root,complete,doc,doc_sha256)
                 VALUES('snap-again','2026-01-07T00:00:00Z','/tmp/synthetic',1,'{}',?1)",
                params!["a".repeat(64)],
            )
            .unwrap();
        let mut again = inputs(SubjectKind::Intended, C, "rev-one");
        again.snapshot_id = Input::known("snap-again");
        examined_inputs::record(&f.ledger, &again, "2026-01-07T00:00:00Z").unwrap();
        let id = run(&f, 0, vec![], "rev-one");
        let view = finding_standing::read(&f.ledger, C).unwrap();
        assert_eq!(view.current_revision.as_deref(), Some("snap-again"));
        assert_eq!(refused(fix_with(&f, &id)).code, "evidence_same_code");
        assert_eq!(rows(&f.ledger), 0, "nothing is written");
        let view = finding_standing::read(&f.ledger, C).unwrap();
        assert!(!view.findings[0].counts_as_fixed);
        assert!(
            view.findings[0].fix_evidence.is_empty(),
            "the screen offers a check on unchanged code as evidence of a fix"
        );
    }
}

// ── the screen fixture (evidence only; ignored in the suite) ─────────────

/// Seeds SYNTHETIC review rounds into an existing store, bound to whatever
/// version the Change's record names now, through the real writer. Used only
/// to build the T-4-2 screen-assertion fixture under
/// `~/.thruline-bench/review-lane/`; it never runs in the suite.
///
/// `T42_SEED_DB=<ledger> T42_SEED_CHANGE=<id> T42_SEED_ROUND=<1|2> cargo test
///  -p thruline-core --test finding_standing -- --ignored seed_synthetic`
#[test]
#[ignore = "evidence fixture builder; needs T42_SEED_DB, T42_SEED_CHANGE, T42_SEED_ROUND"]
fn seed_synthetic_review_round_for_the_screen() {
    let db = std::env::var("T42_SEED_DB").expect("T42_SEED_DB");
    live_store::refuse_live_store(std::path::Path::new(&db));
    let change = std::env::var("T42_SEED_CHANGE").expect("T42_SEED_CHANGE");
    let round: u32 = std::env::var("T42_SEED_ROUND")
        .expect("T42_SEED_ROUND")
        .parse()
        .expect("a round number");
    let ledger = Ledger::open(&db).expect("the fixture store");
    let binding = change_review::current_binding(&ledger, &change)
        .expect("binding")
        .expect("the Change names a version");
    let findings = if round == 1 {
        vec![
            finding(
                "SYNTHETIC: addition subtracts",
                Some("calculator.py"),
                Some(2),
            ),
            finding("SYNTHETIC: readme wording", Some("README.md"), Some(1)),
            finding("SYNTHETIC: unused import", Some("utils.py"), Some(1)),
            finding(
                "SYNTHETIC: missing bounds check",
                Some("config.py"),
                Some(4),
            ),
        ]
    } else {
        vec![finding(
            "SYNTHETIC: missing bounds check",
            Some("config.py"),
            Some(4),
        )]
    };
    let mut r = review(
        &format!("synthetic-review-round-{round}"),
        round,
        &binding.target_revision,
        &binding.examined_inputs_id,
        ReviewState::Complete,
        findings,
    );
    r.change_id = change.clone();
    r.scope_version = binding.scope_version;
    r.relationship_provenance = "SYNTHETIC fixture for the T-4-2 screen assertion".into();
    r.limitations = vec!["SYNTHETIC: seeded for a screen assertion; no reviewer ran".into()];
    r.started_at = format!("2026-09-24T0{round}:00:00Z");
    r.ended_at = Some(format!("2026-09-24T0{round}:01:00Z"));
    change_review::append_review(&ledger, &r).expect("append the synthetic review");
    println!("seeded {} on {}", r.id, binding.target_revision);
}
