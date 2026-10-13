//! T-4-1 — review records bound to one version, append-only, with integrity
//! checked per review (`B9.5`, `B9.6`, `Q8`, `Q11`, R02, R08).

use rusqlite::params;
use thruline_core::change::{
    self, AcceptedScope, ChangeRequest, NewChange, RequestSource, ScopeAcceptance,
};
use thruline_core::change_review::{
    self, InspectRefusal, Integrity, NewReview, Recommendation, ReviewFinding, ReviewState,
    ReviewerType,
};
use thruline_core::ledger::Ledger;

const CHANGE: &str = "chg_review_fixture";

fn record_version(ledger: &Ledger, snapshot: &str, inputs: &str) {
    ledger
        .conn()
        .execute(
            "INSERT OR IGNORE INTO workspace_snapshots(id,taken_at,root,complete,doc,doc_sha256)
             VALUES(?1,'2026-01-02T00:00:00Z','/fixture/review',1,'{}',?2)",
            params![snapshot, "a".repeat(64)],
        )
        .unwrap();
    ledger
        .conn()
        .execute(
            "INSERT INTO examination_inputs(id,subject_kind,subject_id,recorded_at,schema_version,
                 snapshot_id,source_identity,base_revision,uncommitted_state,worktree_path,
                 config_files_json,tools_json,target_kind)
             VALUES(?1,'intended',?2,'2026-01-02T00:03:00Z',1,?3,'wsid1:fixture',?4,'clean',
                 '/fixture/review','[]','[]','none')",
            params![inputs, CHANGE, snapshot, "b".repeat(40)],
        )
        .unwrap();
}

fn prepared() -> Ledger {
    let ledger = Ledger::open_in_memory().unwrap();
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id,root_path,source,display_name,created_at)
             VALUES('project-review','/fixture/review','claude_code','Review','2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    change::insert_change(
        &ledger,
        &NewChange {
            id: CHANGE.into(),
            project_id: "project-review".into(),
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
    record_version(&ledger, "snap-one", "exi-one");
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

fn review(id: &str, round: u32, state: ReviewState) -> NewReview {
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
        target_revision: "snap-one".into(),
        scope_version: Some(1),
        examined_inputs_id: "exi-one".into(),
        diff_sha256: "d".repeat(64),
        route_identity_json: route(),
        started_at: "2026-01-02T00:04:00Z".into(),
        ended_at: Some("2026-01-02T00:05:00Z".into()),
        state,
        recommendation: None,
        limitations: vec!["advisory only".into()],
        raw_bytes: br#"{"schema":"thruline.local-agent-capture.v1","note":"raw"}"#.to_vec(),
        parser_id: "thruline.pr-agent-structured.v1".into(),
        parsed_json: r#"{"state":"x"}"#.into(),
        parse_complete: complete,
        findings: if complete {
            vec![ReviewFinding {
                kind: "Addition returns subtraction".into(),
                message: "The operator subtracts.".into(),
                path: Some("calculator.py".into()),
                line: Some(2),
            }]
        } else {
            vec![]
        },
    }
}

#[test]
fn a_complete_review_of_the_standing_version_applies_and_counts_its_findings() {
    let ledger = prepared();
    change_review::append_review(&ledger, &review("r1", 1, ReviewState::Complete)).unwrap();
    let view = change::read_change(&ledger, CHANGE).unwrap().unwrap();
    assert_eq!(view.schema_version, 8);
    assert_eq!(view.reviews.len(), 1);
    let r = &view.reviews[0];
    assert!(r.applies_to_current_version);
    assert_eq!(r.integrity, Integrity::Intact);
    assert_eq!(r.finding_count, 1);
    assert_eq!(r.findings.len(), 1);
    assert_eq!(r.target_revision, "snap-one");
    assert_eq!(r.current_revision.as_deref(), Some("snap-one"));
    assert_eq!(r.recommendation, None);
    assert_eq!(r.reviewer_type, ReviewerType::Model);
}

/// R02 / `Q8` — the version moves; the review stays readable, names the
/// version it looked at, and gets no current credit.
#[test]
fn after_the_version_moves_the_review_is_history_and_gets_no_credit() {
    let ledger = prepared();
    change_review::append_review(&ledger, &review("r1", 1, ReviewState::Complete)).unwrap();
    record_version(&ledger, "snap-two", "exi-two");
    let view = change::read_change(&ledger, CHANGE).unwrap().unwrap();
    let r = &view.reviews[0];
    assert!(!r.applies_to_current_version);
    assert_eq!(r.target_revision, "snap-one");
    assert_eq!(r.current_revision.as_deref(), Some("snap-two"));
    assert_eq!(r.finding_count, 1, "history stays readable");
    assert_eq!(r.integrity, Integrity::Intact);
}

/// The accepted criteria change: a review of the old criteria is history too.
#[test]
fn after_the_criteria_are_re_accepted_the_review_gets_no_credit() {
    let ledger = prepared();
    change_review::append_review(&ledger, &review("r1", 1, ReviewState::Complete)).unwrap();
    change::append_scope(
        &ledger,
        CHANGE,
        "2026-01-03T00:00:00Z",
        &AcceptedScope {
            criteria: vec!["addition adds".into(), "and subtracts".into()],
            exclusions: vec![],
            allowed_scope: vec![],
            constraints: vec![],
            check_profile: vec![],
        },
    )
    .unwrap();
    change::accept_scope(
        &ledger,
        CHANGE,
        2,
        &ScopeAcceptance {
            actor: "sam".into(),
            accepted_at: "2026-01-03T00:01:00Z".into(),
            rationale: "more".into(),
        },
    )
    .unwrap();
    let view = change::read_change(&ledger, CHANGE).unwrap().unwrap();
    assert!(!view.reviews[0].applies_to_current_version);
}

/// `Q11` — a review that did not finish gets no credit, carries no findings,
/// and keeps its own reason word.
#[test]
fn a_review_that_did_not_finish_never_applies_and_keeps_its_own_state() {
    let ledger = prepared();
    for (i, state) in [
        ReviewState::TimedOut,
        ReviewState::Cancelled,
        ReviewState::Unavailable,
        ReviewState::ParseFailed,
        ReviewState::Incomplete,
    ]
    .into_iter()
    .enumerate()
    {
        let round = i as u32 + 1;
        change_review::append_review(&ledger, &review(&format!("r{round}"), round, state)).unwrap();
    }
    let view = change::read_change(&ledger, CHANGE).unwrap().unwrap();
    let states: Vec<ReviewState> = view.reviews.iter().map(|r| r.state).collect();
    assert_eq!(
        states,
        vec![
            ReviewState::TimedOut,
            ReviewState::Cancelled,
            ReviewState::Unavailable,
            ReviewState::ParseFailed,
            ReviewState::Incomplete
        ]
    );
    for r in &view.reviews {
        assert!(!r.applies_to_current_version, "{:?} got credit", r.state);
        assert_eq!(r.finding_count, 0);
    }
}

#[test]
fn a_review_that_did_not_finish_cannot_carry_findings_or_a_recommendation() {
    let ledger = prepared();
    let mut with_finding = review("r1", 1, ReviewState::TimedOut);
    with_finding.findings = review("x", 1, ReviewState::Complete).findings;
    assert!(change_review::append_review(&ledger, &with_finding).is_err());
    let mut with_opinion = review("r2", 1, ReviewState::Cancelled);
    with_opinion.recommendation = Some(Recommendation::Approved);
    assert!(change_review::append_review(&ledger, &with_opinion).is_err());
    let mut complete_without_end = review("r3", 1, ReviewState::Complete);
    complete_without_end.ended_at = None;
    assert!(change_review::append_review(&ledger, &complete_without_end).is_err());
}

#[test]
fn a_route_naming_another_destination_or_source_is_refused() {
    let ledger = prepared();
    for (field, value) in [
        ("endpoint", serde_json::json!("http://127.0.0.1:11435")),
        ("endpoint", serde_json::json!("http://example.com:11436")),
        ("provider_source_commit", serde_json::json!("0".repeat(40))),
        ("image_id", serde_json::json!("latest")),
        ("model_manifest_sha256", serde_json::json!("not-a-digest")),
    ] {
        let mut r = review("r1", 1, ReviewState::Complete);
        let mut route: serde_json::Value = serde_json::from_str(&r.route_identity_json).unwrap();
        route[field] = value;
        r.route_identity_json = route.to_string();
        assert!(
            change_review::append_review(&ledger, &r).is_err(),
            "{field} mutation must be refused"
        );
    }
}

#[test]
fn reviews_are_append_only() {
    let ledger = prepared();
    change_review::append_review(&ledger, &review("r1", 1, ReviewState::Complete)).unwrap();
    for sql in [
        "UPDATE change_reviews SET state='timed_out'",
        "DELETE FROM change_reviews",
        "UPDATE change_review_raw_artifacts SET raw_sha256=raw_sha256",
        "DELETE FROM change_review_raw_artifacts",
        "UPDATE change_review_parsed_artifacts SET parse_complete=0",
        "DELETE FROM change_review_parsed_artifacts",
        "UPDATE change_review_findings SET message='x'",
        "DELETE FROM change_review_findings",
    ] {
        assert!(ledger.conn().execute(sql, []).is_err(), "{sql} must abort");
    }
    // A second round of the same number is a replacement, and refused.
    assert!(
        change_review::append_review(&ledger, &review("r9", 1, ReviewState::Complete)).is_err()
    );
}

/// Bypass the append-only triggers the way only an attacker with the file
/// could, to prove the read side notices. Test-only.
fn tamper(ledger: &Ledger, sql: &str) {
    let conn = ledger.conn();
    conn.execute_batch(
        "DROP TRIGGER change_review_raw_artifacts_no_update;
         DROP TRIGGER change_review_raw_artifacts_no_delete;
         DROP TRIGGER change_review_findings_no_update;",
    )
    .unwrap();
    conn.execute_batch(sql).unwrap();
}

/// R08 — altered bytes: the review is `altered`, earns nothing, and both
/// drill-downs refuse; the Change itself stays readable.
#[test]
fn altered_raw_bytes_are_visible_refused_and_do_not_blank_the_change() {
    let ledger = prepared();
    change_review::append_review(&ledger, &review("r1", 1, ReviewState::Complete)).unwrap();
    assert!(change_review::inspect_raw(&ledger, CHANGE, "r1")
        .unwrap()
        .is_ok());
    tamper(
        &ledger,
        "UPDATE change_review_raw_artifacts SET raw_bytes=CAST('{\"forged\":true}' AS BLOB), byte_length=15",
    );
    let view = change::read_change(&ledger, CHANGE)
        .unwrap()
        .expect("still readable");
    let r = &view.reviews[0];
    assert_eq!(r.integrity, Integrity::Altered);
    assert!(!r.applies_to_current_version);
    assert_eq!(r.finding_count, 0);
    assert!(!r.raw.intact);
    assert_eq!(
        change_review::inspect_raw(&ledger, CHANGE, "r1").unwrap(),
        Err(InspectRefusal::Integrity(Integrity::Altered))
    );
    assert_eq!(
        change_review::inspect_parsed(&ledger, CHANGE, "r1").unwrap(),
        Err(InspectRefusal::Integrity(Integrity::Altered))
    );
}

/// R08 — a raw row whose digest column was forged WITH its bytes is still
/// caught, because the parent row sealed the original digest.
#[test]
fn a_consistent_forgery_of_bytes_and_digest_is_still_caught_by_the_seal() {
    let ledger = prepared();
    change_review::append_review(&ledger, &review("r1", 1, ReviewState::Complete)).unwrap();
    let forged = b"{\"forged\":true}";
    let digest = thruline_core::chain::sha256_hex(forged);
    tamper(
        &ledger,
        &format!(
            "UPDATE change_review_raw_artifacts SET raw_bytes=CAST('{{\"forged\":true}}' AS BLOB), byte_length={}, raw_sha256='{digest}'",
            forged.len()
        ),
    );
    let view = change::read_change(&ledger, CHANGE).unwrap().unwrap();
    assert_eq!(view.reviews[0].integrity, Integrity::Altered);
}

/// R08 — a removed raw artifact is `missing`, and the drill-down says so.
#[test]
fn a_removed_raw_artifact_is_missing_and_refused() {
    let ledger = prepared();
    change_review::append_review(&ledger, &review("r1", 1, ReviewState::Complete)).unwrap();
    tamper(&ledger, "DELETE FROM change_review_raw_artifacts");
    let view = change::read_change(&ledger, CHANGE).unwrap().unwrap();
    assert_eq!(view.reviews[0].integrity, Integrity::Missing);
    assert!(!view.reviews[0].raw.available);
    assert_eq!(
        change_review::inspect_raw(&ledger, CHANGE, "r1").unwrap(),
        Err(InspectRefusal::Integrity(Integrity::Missing))
    );
}

/// A changed typed finding is caught by the seal too.
#[test]
fn a_changed_finding_is_caught() {
    let ledger = prepared();
    change_review::append_review(&ledger, &review("r1", 1, ReviewState::Complete)).unwrap();
    tamper(
        &ledger,
        "UPDATE change_review_findings SET message='nothing wrong here'",
    );
    let view = change::read_change(&ledger, CHANGE).unwrap().unwrap();
    assert_eq!(view.reviews[0].integrity, Integrity::Altered);
    assert!(view.reviews[0].findings.is_empty());
}

#[test]
fn a_review_of_another_change_is_indistinguishable_from_none() {
    let ledger = prepared();
    change_review::append_review(&ledger, &review("r1", 1, ReviewState::Complete)).unwrap();
    assert_eq!(
        change_review::inspect_raw(&ledger, "chg_other", "r1").unwrap(),
        Err(InspectRefusal::NotFound)
    );
}

/// The drill-down is a masked derivative: encoded text is decoded and
/// masked, the original's identity is given beside the derivative's.
#[test]
fn the_raw_drill_down_is_a_masked_labelled_derivative() {
    let ledger = prepared();
    let secret = "ghp_abcdefghijklmnopqrstuvwxyz0123456789";
    let inner = format!("token {secret} in the review");
    let encoded = {
        // standard base64, padded
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let b = inner.as_bytes();
        let mut out = String::new();
        for chunk in b.chunks(3) {
            let n = (u32::from(chunk[0]) << 16)
                | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
                | u32::from(*chunk.get(2).unwrap_or(&0));
            for i in 0..4 {
                if i <= chunk.len() {
                    out.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    };
    let mut r = review("r1", 1, ReviewState::Complete);
    r.raw_bytes = serde_json::to_vec(&serde_json::json!({
        "native_artifacts":[{"relative_path":"native/review.md","bytes_base64":encoded}],
        "note": format!("also {secret}"),
    }))
    .unwrap();
    change_review::append_review(&ledger, &r).unwrap();
    let inspection = change_review::inspect_raw(&ledger, CHANGE, "r1")
        .unwrap()
        .unwrap();
    let served = serde_json::to_string(&inspection).unwrap();
    assert!(!served.contains(secret), "a secret reached the served copy");
    assert!(
        !served.contains(&encoded),
        "encoded text passed through unmasked"
    );
    assert_eq!(
        inspection.original_sha256,
        thruline_core::chain::sha256_hex(&r.raw_bytes)
    );
    assert_ne!(inspection.original_sha256, inspection.derivative_sha256);
    assert!(inspection.content["native_artifacts"][0]["text"]
        .as_str()
        .unwrap()
        .contains("in the review"));
}
