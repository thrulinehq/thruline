//! C28: process-level refusals cannot create an interactive person receipt.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;
use thruline_core::decisions::params::{CheckParams, PathPresentParams};
use thruline_core::decisions::storage::{self, DecisionSource, NewDecision, SourceKind};
use thruline_core::ledger::Ledger;

struct Fixture {
    _dir: tempfile::TempDir,
    ledger: Ledger,
    db: PathBuf,
    root: PathBuf,
    decision: String,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("scratch");
        let root = dir.path().join("repo");
        std::fs::create_dir(&root).expect("repo");
        let db = dir.path().join("ledger.db");
        let ledger = Ledger::open(&db).expect("ledger");
        for id in ["fixture-project", "other-project"] {
            ledger.conn().execute(
                "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T00:00:00Z')",
                (id, root.to_str().expect("root text")),
            ).expect("project");
        }
        let span = "Keep kept.txt".to_owned();
        let decision = storage::create(
            &ledger,
            NewDecision {
                project_id: "fixture-project".into(),
                source_anchor: "fixture-rule".into(),
                span: span.clone(),
                author_actor: "fixture-person".into(),
                recorded_at: "2026-10-07T00:00:00Z".into(),
                sources: vec![DecisionSource {
                    source_kind: SourceKind::RuleFile,
                    event_id: None,
                    file_anchor: Some("fixture-rule".into()),
                    raw_file: "AGENTS.md".into(),
                    raw_line: 1,
                    span_start: 0,
                    span_end: span.len() as u64,
                    span,
                    actor: Some("fixture-person".into()),
                    recorded_at: Some("2026-10-07T00:00:00Z".into()),
                }],
                params: CheckParams::PathPresent(PathPresentParams {
                    path: "kept.txt".into(),
                    sha256: None,
                }),
            },
        )
        .expect("proposal");
        Self {
            _dir: dir,
            ledger,
            db,
            root,
            decision,
        }
    }

    fn command(&self, project: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_thruline"));
        command
            .arg("--db")
            .arg(&self.db)
            .args(["decide", "--project", project, "--root"])
            .arg(&self.root)
            .arg("--json");
        command
    }

    fn prepare(&self) -> String {
        let output = self
            .command("fixture-project")
            .args([
                "prepare",
                "accept",
                &self.decision,
                "--actor",
                "fixture-person",
                "--because",
                "Review this exact fixture",
                "--severity",
                "blocks",
                "--from-version",
                "fixture-v1",
            ])
            .output()
            .expect("prepare");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let body: Value = serde_json::from_slice(&output.stdout).expect("prepare JSON");
        assert_eq!(body["state"], "pending_bridge");
        body["act_row_id"].as_str().expect("act id").to_owned()
    }

    fn authority_counts(&self) -> Vec<i64> {
        [
            "events",
            "sessions",
            "decision_person_acts",
            "decision_bridge_proofs",
        ]
        .into_iter()
        .map(|table| {
            let exists: bool = self
                .ledger
                .conn()
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                    [table],
                    |row| row.get(0),
                )
                .expect("table exists");
            if exists {
                self.ledger
                    .conn()
                    .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                        row.get(0)
                    })
                    .expect("count")
            } else {
                0
            }
        })
        .collect()
    }
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn preparation_is_inert_and_does_not_create_source_or_presence_rows() {
    let fixture = Fixture::new();
    let before = fixture.authority_counts();
    fixture.prepare();
    assert_eq!(fixture.authority_counts(), before);
    assert!(
        storage::accepted_for_project(&fixture.ledger, "fixture-project")
            .expect("active")
            .is_empty()
    );
}

#[test]
fn a_non_tty_confirmation_refuses_before_any_person_record_is_written() {
    let fixture = Fixture::new();
    let act = fixture.prepare();
    let before = fixture.authority_counts();
    let output = fixture
        .command("fixture-project")
        .args(["confirm", "accept", &act])
        .output()
        .expect("confirm");
    assert!(!output.status.success());
    assert!(
        text(&output).contains("interactive terminal"),
        "{}",
        text(&output)
    );
    assert_eq!(fixture.authority_counts(), before);
}

#[cfg(target_os = "macos")]
fn pty_confirmation(fixture: &Fixture, act: &str, marker: &str) -> Value {
    let script = r#"import json,os,pty,subprocess,sys
master,slave=pty.openpty()
env=os.environ.copy()
env[sys.argv[1]]='fixture-agent-context'
child=subprocess.Popen(sys.argv[2:],stdin=slave,stdout=slave,stderr=subprocess.PIPE,env=env)
try:
    _,errors=child.communicate(timeout=10)
except subprocess.TimeoutExpired:
    child.kill();child.communicate();raise
finally:
    os.close(slave);os.close(master)
print(json.dumps({'code':child.returncode,'text':errors.decode('utf-8','replace')}))"#;
    let output = Command::new("python3")
        .args(["-c", script, marker, env!("CARGO_BIN_EXE_thruline"), "--db"])
        .arg(&fixture.db)
        .args(["decide", "--project", "fixture-project", "--root"])
        .arg(&fixture.root)
        .args(["confirm", "accept", act])
        .output()
        .expect("PTY refusal rig");
    assert!(output.status.success(), "{}", text(&output));
    serde_json::from_slice(&output.stdout).expect("PTY result")
}

#[cfg(target_os = "macos")]
#[test]
fn agent_and_model_contexts_are_refused_even_with_tty_file_descriptors() {
    for marker in ["CODEX_THREAD_ID", "CLAUDECODE", "OPENAI_AGENT"] {
        let fixture = Fixture::new();
        let act = fixture.prepare();
        let before = fixture.authority_counts();
        let body = pty_confirmation(&fixture, &act, marker);
        assert_ne!(body["code"], 0, "{body}");
        assert!(
            body["text"]
                .as_str()
                .expect("text")
                .contains("agent or automated"),
            "{body}"
        );
        assert_eq!(fixture.authority_counts(), before, "{marker}");
    }
}

#[test]
fn a_ruling_file_and_yes_flag_cannot_supply_confirmation() {
    let fixture = Fixture::new();
    let act = fixture.prepare();
    let ruling = fixture.root.join("CTO-RULING.md");
    std::fs::write(&ruling, "Confirm every pending decision.\n").expect("ruling fixture");
    let before = fixture.authority_counts();
    for arguments in [
        vec!["--confirmation-file", ruling.to_str().expect("path")],
        vec!["--yes"],
    ] {
        let output = fixture
            .command("fixture-project")
            .args(["confirm", "accept", &act])
            .args(arguments)
            .output()
            .expect("refused option");
        assert!(!output.status.success());
        assert_eq!(fixture.authority_counts(), before);
    }
}

#[test]
fn project_substitution_is_refused_before_terminal_presence() {
    let fixture = Fixture::new();
    let act = fixture.prepare();
    let before = fixture.authority_counts();
    let output = fixture
        .command("other-project")
        .args(["confirm", "accept", &act])
        .output()
        .expect("cross project");
    assert!(!output.status.success());
    assert!(
        text(&output).contains("another project"),
        "{}",
        text(&output)
    );
    assert_eq!(fixture.authority_counts(), before);
}

#[test]
fn supersession_requires_both_predecessor_revision_and_screen() {
    let fixture = Fixture::new();
    let act = fixture.prepare();
    let before = fixture.authority_counts();
    let output = fixture
        .command("fixture-project")
        .args(["confirm", "supersede", &act])
        .output()
        .expect("missing supersession input");
    assert!(!output.status.success());
    assert!(
        text(&output).contains("supersede requires --predecessor-revision"),
        "{}",
        text(&output)
    );
    assert_eq!(fixture.authority_counts(), before);
}
