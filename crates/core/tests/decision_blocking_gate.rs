//! A standing decision marked `blocks` is checked at the same version as the
//! person's Approve act. This fixture exercises the real approval door.

#[path = "support/person_action.rs"]
mod person_action;

use person_action::PersonRecorder;

use rusqlite::params;
use serde_json::json;
use thruline_core::change_decision::{self, Decide, Outcome};
use thruline_core::decisions::engine::CHECK_IMPL_VERSION;
use thruline_core::decisions::params::{CheckParams, Surface, TextAbsentParams};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, SourceKind, VerdictInput, WaiverInput,
};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "decisions-gate-project";
const CHANGE: &str = "decisions-gate-change";
const VERSION: &str = "snap_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const FUTURE_VERSION: &str = "snap_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const ACT_AT: &str = "2026-10-07T12:00:00Z";

fn fixture() -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join(".thruline")).unwrap();
    std::fs::write(
        dir.path().join(".thruline/policy.yml"),
        "version: 1\ndecisions:\n  require_hold: blocking\n  on_unverifiable: block\n",
    )
    .unwrap();
    let ledger = Ledger::open(dir.path().join("ledger.db")).unwrap();
    let root = dir.path().to_string_lossy().into_owned();
    ledger.conn().execute(
        "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T10:00:00Z')",
        params![PROJECT, root],
    ).unwrap();
    ledger
        .conn()
        .execute(
            "INSERT INTO changes(id,project_id,opened_at,title,purpose,requester,request_source)
         VALUES(?1,?2,'2026-10-07T10:00:00Z','Gate fixture','Version check','sam','person')",
            params![CHANGE, PROJECT],
        )
        .unwrap();
    ledger
        .conn()
        .execute(
            "INSERT INTO change_scope_revisions(change_id,scope_version,proposed_at,criteria_json,
           exclusions_json,allowed_scope_json,constraints_json,check_profile_json,content_sha256)
         VALUES(?1,1,'2026-10-07T10:01:00Z','[\"Keep the rule\"]','[]','[]','[]','[]','deadbeef')",
            [CHANGE],
        )
        .unwrap();
    ledger.conn().execute(
        "INSERT INTO change_scope_acceptances(change_id,scope_version,accepted_by,accepted_at,rationale)
         VALUES(?1,1,'sam','2026-10-07T10:02:00Z','Agreed')",
        [CHANGE],
    ).unwrap();
    let doc = json!({
        "id": VERSION, "taken_at": "2026-10-07T10:30:00Z", "root": root,
        "git": null,
        "tree": {"staged": [], "unstaged": [], "untracked": [], "deleted": [], "dirty_diff_sha256": null},
        "environment": {"os": "test", "arch": "test", "git_version": null, "env_names": [], "lockfiles": {}},
        "processes": {"ps_sha256": null, "ps_lines": 0, "ports": null, "task_ids": null,
                      "log_cursors": null, "gaps": "none"},
        "complete": true, "note": null
    }).to_string();
    ledger
        .conn()
        .execute(
            "INSERT INTO workspace_snapshots(id,taken_at,root,head,branch,complete,doc,doc_sha256)
         VALUES(?1,'2026-10-07T10:30:00Z',?2,NULL,NULL,1,?3,?4)",
            params![
                VERSION,
                root,
                doc,
                thruline_core::chain::sha256_hex(doc.as_bytes())
            ],
        )
        .unwrap();
    ledger
        .conn()
        .execute(
            "INSERT INTO change_implementations(change_id,binding_version,bound_at,
          repository_identity,base_revision,snapshot_id,workspace_identity)
         VALUES(?1,1,'2026-10-07T10:30:00Z','fixture','base',?2,NULL)",
            params![CHANGE, VERSION],
        )
        .unwrap();
    (dir, ledger)
}

fn decision(ledger: &Ledger, severity: &str, verdict: Option<&str>) -> String {
    decision_from(ledger, severity, verdict, VERSION)
}

fn decision_from(ledger: &Ledger, severity: &str, verdict: Option<&str>, from: &str) -> String {
    decision_from_with_impl(ledger, severity, verdict, from, CHECK_IMPL_VERSION)
}

fn decision_from_with_impl(
    ledger: &Ledger,
    severity: &str,
    verdict: Option<&str>,
    from: &str,
    check_impl_version: &str,
) -> String {
    let source = DecisionSource {
        source_kind: SourceKind::WorkPrompt,
        event_id: Some("founder-prompt-1".into()),
        file_anchor: None,
        raw_file: "record.jsonl".into(),
        raw_line: 1,
        span_start: 0,
        span_end: 23,
        span: "No obsolete copy anywhere".into(),
        actor: Some("sam".into()),
        recorded_at: Some("2026-10-07T10:00:00Z".into()),
    };
    let id = storage::create(
        ledger,
        NewDecision {
            project_id: PROJECT.into(),
            source_anchor: "founder-prompt-1".into(),
            span: source.span.clone(),
            sources: vec![source],
            author_actor: "sam".into(),
            recorded_at: "2026-10-07T10:00:00Z".into(),
            params: CheckParams::TextAbsent(TextAbsentParams {
                test_allowlist: Vec::new(),
                json_field_allowlist: Vec::new(),
                conditional_path_allowlist: Vec::new(),
                span_allowlist: Vec::new(),
                terms: vec!["obsolete".into()],
                synonyms: vec![],
                surfaces: vec![Surface::UiCopy],
                allowlist: vec![],
            }),
        },
    )
    .unwrap();
    let acceptance_id = storage::accept(
        ledger,
        AcceptanceInput {
            decision_id: id.clone(),
            revision: 1,
            actor: "sam".into(),
            at: "2026-10-07T10:10:00Z".into(),
            because: "The UI must honor it".into(),
            severity: severity.into(),
            applies_from_version: from.into(),
        },
    )
    .unwrap();
    PersonRecorder::new(ledger, PROJECT).bind_bridge_acceptance(ledger, &acceptance_id);
    if let Some(verdict) = verdict {
        storage::record_verdict(
            ledger,
            VerdictInput {
                decision_id: id.clone(),
                revision: 1,
                version_id: VERSION.into(),
                tree_hash: "fixture-tree".into(),
                verdict: verdict.into(),
                reason: (verdict == "unverifiable").then(|| "required evidence absent".into()),
                checkable_if: (verdict == "unverifiable").then(|| "Run the named check".into()),
                findings: if verdict == "broken" {
                    json!([{"path":"ui/src/app.jsx","line":12,"surface":"ui_copy"}])
                } else {
                    json!([])
                },
                surfaces: json!([]),
                check_impl_version: check_impl_version.into(),
                tool_versions: json!({}),
                computed_at: "2026-10-07T11:00:00Z".into(),
                basis: json!({"tree_hash":"fixture-tree"}),
            },
        )
        .unwrap();
    }
    id
}

#[test]
fn decision_starting_at_a_later_snapshot_does_not_block_an_earlier_version() {
    let (_dir, ledger) = fixture();
    let doc = json!({"id": FUTURE_VERSION, "taken_at": "2026-10-07T13:00:00Z"}).to_string();
    ledger
        .conn()
        .execute(
            "INSERT INTO workspace_snapshots(id,taken_at,root,head,branch,complete,doc,doc_sha256)
         VALUES(?1,'2026-10-07T13:00:00Z','/tmp/future',NULL,NULL,1,?2,?3)",
            params![
                FUTURE_VERSION,
                doc,
                thruline_core::chain::sha256_hex(doc.as_bytes())
            ],
        )
        .unwrap();
    decision_from(&ledger, "blocks", None, FUTURE_VERSION);
    assert!(matches!(
        approve(&ledger).unwrap(),
        Outcome::Recorded { .. }
    ));
}

fn approve(ledger: &Ledger) -> thruline_core::error::Result<Outcome> {
    change_decision::approve(
        ledger,
        CHANGE,
        &Decide {
            actor: "sam".into(),
            at: ACT_AT.into(),
            seen_revision: VERSION.into(),
            note: None,
        },
    )
}

fn approval_count(ledger: &Ledger) -> i64 {
    ledger
        .conn()
        .query_row(
            "SELECT count(*) FROM change_decisions WHERE change_id=?1 AND act='approved'",
            [CHANGE],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn broken_blocking_decision_refuses_approve_without_writing_an_act() {
    let (_dir, ledger) = fixture();
    let id = decision(&ledger, "blocks", Some("broken"));
    let error = approve(&ledger).unwrap_err().to_string();
    assert!(error.contains(&id), "{error}");
    assert!(error.contains("broken"), "{error}");
    assert!(error.contains("ui/src/app.jsx:12"), "{error}");
    assert_eq!(approval_count(&ledger), 0);
}

#[test]
fn warning_does_not_block_and_exact_version_waiver_allows_approve() {
    let (_dir, ledger) = fixture();
    decision(&ledger, "warns", Some("broken"));
    assert!(matches!(
        approve(&ledger).unwrap(),
        Outcome::Recorded { .. }
    ));

    let (_dir, ledger) = fixture();
    let id = decision(&ledger, "blocks", Some("broken"));
    storage::waive(
        &ledger,
        WaiverInput {
            decision_id: id.clone(),
            version_id: VERSION.into(),
            actor: "sam".into(),
            at: "2026-10-07T11:00:00Z".into(),
            because: "This exception has expired".into(),
            expires_version: None,
            expires_at: Some("2026-10-07T11:59:00Z".into()),
        },
    )
    .unwrap();
    assert!(approve(&ledger).is_err());
    let waiver_id = storage::waive(
        &ledger,
        WaiverInput {
            decision_id: id.clone(),
            version_id: VERSION.into(),
            actor: "sam".into(),
            at: "2026-10-07T11:30:00Z".into(),
            because: "Named exception for this version".into(),
            expires_version: None,
            expires_at: None,
        },
    )
    .unwrap();
    PersonRecorder::new(&ledger, PROJECT).bind_bridge_waiver(&ledger, &waiver_id);
    assert!(matches!(
        approve(&ledger).unwrap(),
        Outcome::Recorded { .. }
    ));
}

#[test]
fn absent_or_unverifiable_blocking_receipt_refuses_approve() {
    for verdict in [None, Some("unverifiable")] {
        let (_dir, ledger) = fixture();
        decision(&ledger, "blocks", verdict);
        assert!(approve(&ledger).is_err());
        assert_eq!(approval_count(&ledger), 0);
    }
}

#[test]
fn stale_hold_from_an_older_check_implementation_does_not_clear_approve() {
    let (_dir, ledger) = fixture();
    let id = decision_from_with_impl(&ledger, "blocks", Some("holds"), VERSION, "fixture-check");
    let error = approve(&ledger).unwrap_err().to_string();
    assert!(error.contains(&id), "{error}");
    assert!(
        error.contains("stale") || error.contains("checkable"),
        "{error}"
    );
    assert_eq!(approval_count(&ledger), 0);
}
