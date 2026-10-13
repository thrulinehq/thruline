//! The Stop speaker emits one cached Decisions line and never checks again.

#[path = "../../core/tests/support/person_action.rs"]
mod person_action;

use person_action::PersonRecorder;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::json;
use thruline_core::decisions::engine::CHECK_IMPL_VERSION;
use thruline_core::decisions::params::{CheckParams, PathPresentParams};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, SourceKind, VerdictInput,
};
use thruline_core::Ledger;

const SESSION: &str = "cccccccc-3333-4333-8333-333333333333";

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("git text")
        .trim()
        .into()
}

fn audit(db: &Path, cwd: &Path, transcript: &Path) -> String {
    let payload = json!({
        "session_id":SESSION,
        "transcript_path":transcript.to_str().expect("transcript path"),
        "cwd":cwd.to_str().expect("repo path"),
        "hook_event_name":"Stop",
        "stop_hook_active":false,
    });
    let mut child = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args([
            "--db",
            db.to_str().expect("db path"),
            "audit",
            "--hook",
            "stop",
        ])
        // The hook's own read is budgeted at 50 ms and silent on expiry, so on
        // a loaded machine the line this test asserts would race two Git
        // spawns. The environment may only lengthen that budget; every
        // assertion below is unchanged, and a dirty tree is still silent for
        // its real reason, not for want of time.
        .env("THRULINE_STOP_ADVISORY_BUDGET_MS", "2000")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("thruline");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(payload.to_string().as_bytes())
        .expect("payload");
    let out = child.wait_with_output().expect("stop output");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("stdout")
}

#[test]
fn stop_injects_one_cached_line_for_clean_exact_head_only() {
    let temp = tempfile::tempdir().expect("scratch");
    let root = temp.path().join("repo");
    std::fs::create_dir_all(&root).expect("repo");
    git(&root, &["init", "-q"]);
    std::fs::write(root.join("kept.txt"), "present\n").expect("source");
    git(&root, &["add", "kept.txt"]);
    git(
        &root,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "base",
        ],
    );
    let version = git(&root, &["rev-parse", "HEAD"]);
    let tree = git(&root, &["rev-parse", "HEAD^{tree}"]);
    let db = temp.path().join("ledger.db");
    let ledger = Ledger::open(&db).expect("ledger");
    ledger.conn().execute(
        "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T00:00:00Z')",
        ("speaker-project", root.to_str().expect("root path")),
    ).expect("project");
    let id = storage::create(
        &ledger,
        NewDecision {
            project_id: "speaker-project".into(),
            source_anchor: "speaker-source".into(),
            span: "Keep the agreed rule".into(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::RuleFile,
                event_id: None,
                file_anchor: Some("speaker-source".into()),
                raw_file: "AGENTS.md".into(),
                raw_line: 1,
                span_start: 0,
                span_end: 20,
                span: "Keep the agreed rule".into(),
                actor: Some("person".into()),
                recorded_at: Some("2026-10-07T00:00:00Z".into()),
            }],
            author_actor: "person".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params: CheckParams::PathPresent(PathPresentParams {
                path: "kept.txt".into(),
                sha256: None,
            }),
        },
    )
    .expect("proposal");
    let acceptance_id = storage::accept(
        &ledger,
        AcceptanceInput {
            decision_id: id.clone(),
            revision: 1,
            actor: "person".into(),
            at: "2026-10-07T00:01:00Z".into(),
            because: "kept rule".into(),
            severity: "blocks".into(),
            applies_from_version: version.clone(),
        },
    )
    .expect("accepted");
    let mut person = PersonRecorder::new(&ledger, "speaker-project");
    person.bind_bridge_acceptance(&ledger, &acceptance_id);
    storage::record_verdict(
        &ledger,
        VerdictInput {
            decision_id: id.clone(),
            revision: 1,
            version_id: version.clone(),
            tree_hash: tree,
            verdict: "broken".into(),
            reason: Some("missing rule".into()),
            checkable_if: None,
            findings: json!([]),
            surfaces: json!([]),
            check_impl_version: CHECK_IMPL_VERSION.into(),
            tool_versions: json!({"git":"fixture"}),
            computed_at: "2026-10-07T00:02:00Z".into(),
            basis: json!({"checked_places":1}),
        },
    )
    .expect("verdict");
    drop(ledger);

    let project = temp.path().join("projects").join("-repo-app");
    std::fs::create_dir_all(&project).expect("project transcript dir");
    let transcript = project.join(format!("{SESSION}.jsonl"));
    let lines = [
        json!({"type":"user","uuid":"u1","isSidechain":false,"sessionId":SESSION,
            "timestamp":"2026-10-07T00:03:00Z","cwd":root,"message":{"role":"user","content":"finish"}}),
        json!({"type":"assistant","uuid":"a1","isSidechain":false,"sessionId":SESSION,
            "timestamp":"2026-10-07T00:03:01Z","cwd":root,
            "message":{"role":"assistant","content":[{"type":"text","text":"Done."}]}}),
    ];
    std::fs::write(
        &transcript,
        lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>(),
    )
    .expect("transcript");
    let stdout = audit(&db, &root, &transcript);
    let output: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("one Stop JSON object");
    assert_eq!(output["hookSpecificOutput"]["hookEventName"], "Stop");
    let line = output["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .expect("one line");
    assert!(line.contains(&version[..8]), "{line}");
    assert!(line.contains(&format!("why {id}")), "{line}");
    assert_eq!(line.lines().count(), 1);

    std::fs::write(root.join("kept.txt"), "edited\n").expect("dirty source");
    assert!(audit(&db, &root, &transcript).is_empty());
    let ledger = Ledger::open(&db).expect("ledger");
    let count: i64 = ledger
        .conn()
        .query_row("SELECT count(*) FROM decision_verdicts", [], |row| {
            row.get(0)
        })
        .expect("count");
    assert_eq!(count, 1, "Stop must not recompute verdicts");
}
