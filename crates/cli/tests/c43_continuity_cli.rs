//! P1 Packet 1 — process-level continuity truth-gate behavior.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::tempdir;
use thruline_core::ledger::Ledger;

const NOW: &str = "2026-08-26T10:00:00Z";

fn seeded(root: &Path) -> (PathBuf, PathBuf) {
    let db = root.join("ledger.db");
    let capsule_path = root.join("capsule.json");
    let ledger = Ledger::open(&db).expect("ledger");
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
               VALUES ('p1','/tmp/p1','claude_code','p1','2026-08-26T00:00:00Z');
             INSERT INTO sessions(id, project_id, file_path, started_at, last_seq)
               VALUES ('s1','p1','/t/s1.jsonl','2026-08-26T00:00:00Z',0);
             INSERT INTO events(id, project_id, session_id, seq, ts, kind, text,
                                raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
               VALUES ('e1','p1','s1',0,'2026-08-26T00:00:01Z','user_msg','finish packet one',
                       '/t/s1.jsonl',1,'x','g','c1');",
        )
        .expect("seed");
    let capsule = thruline_core::capsule::compile(&ledger, "p1", None, NOW).expect("capsule");
    std::fs::write(
        &capsule_path,
        serde_json::to_string_pretty(&capsule).expect("serialize capsule"),
    )
    .expect("write capsule");
    (db, capsule_path)
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args(args)
        .output()
        .expect("run thruline")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn rescue_prints_the_actual_continuity_command() {
    let temp = tempdir().expect("tempdir");
    let (db, _) = seeded(temp.path());
    let staged = temp.path().join("rescue");
    let output = run(&[
        "--db",
        db.to_str().expect("db path"),
        "rescue",
        "--project",
        "p1",
        "--agent",
        "codex",
        "--out",
        staged.to_str().expect("staged path"),
        "--acknowledge",
    ]);
    assert!(output.status.success(), "{output:?}");
    let text = stdout(&output);
    assert!(
        text.contains("verify-continuity --project=p1 --capsule")
            && text.contains("--objective-acknowledged"),
        "rescue did not print the real command and arguments: {text}"
    );
    assert!(
        !text.contains("thruline verify --project"),
        "rescue still routes continuity through ledger verification: {text}"
    );
}

#[test]
fn synthetic_demo_names_nonclosure_and_missing_successor_evidence() {
    let output = run(&["demo", "--recovery"]);
    assert!(
        output.status.success(),
        "the demonstration itself failed: {output:?}"
    );
    let text = stdout(&output);
    assert!(text.contains("SYNTHETIC KILL-9 PREPARATION WALK"), "{text}");
    assert!(text.contains("outcome: insufficient"), "{text}");
    assert!(text.contains("successor_evidence_missing"), "{text}");
    assert!(!text.contains("outcome: verified"), "{text}");
    assert!(!text.contains("RECOVERED —"), "{text}");
}

#[test]
fn insufficient_failed_overridden_and_malformed_results_exit_nonzero() {
    let temp = tempdir().expect("tempdir");
    let (db, capsule) = seeded(temp.path());
    let common = [
        "--db",
        db.to_str().expect("db path"),
        "verify-continuity",
        "--project",
        "p1",
        "--capsule",
        capsule.to_str().expect("capsule path"),
        "--root",
        temp.path().to_str().expect("root path"),
    ];

    let insufficient = run(&[
        common[0],
        common[1],
        common[2],
        common[3],
        common[4],
        common[5],
        common[6],
        common[7],
        common[8],
        "--objective-acknowledged",
    ]);
    assert!(!insufficient.status.success(), "{insufficient:?}");
    let insufficient_text = stdout(&insufficient);
    assert!(
        insufficient_text.contains("CONTINUITY INSUFFICIENT"),
        "{insufficient_text}"
    );
    assert!(
        !insufficient_text.contains("RECOVERED —"),
        "an insufficient result claimed recovery: {insufficient_text}"
    );

    let failed = run(&common);
    assert!(!failed.status.success(), "{failed:?}");
    assert!(stdout(&failed).contains("CONTINUITY FAILED"));

    let overridden = run(&[
        common[0],
        common[1],
        common[2],
        common[3],
        common[4],
        common[5],
        common[6],
        common[7],
        common[8],
        "--objective-acknowledged",
        "--override-reason",
        "operator chose to continue",
    ]);
    assert!(!overridden.status.success(), "{overridden:?}");
    assert!(stdout(&overridden).contains("RESUMED WITH WARNINGS"));

    let malformed = temp.path().join("malformed.json");
    std::fs::write(&malformed, "{not-json\n").expect("write malformed capsule");
    let malformed_output = run(&[
        "--db",
        db.to_str().expect("db path"),
        "verify-continuity",
        "--project",
        "p1",
        "--capsule",
        malformed.to_str().expect("malformed path"),
    ]);
    assert!(!malformed_output.status.success(), "{malformed_output:?}");

    let ledger = Ledger::open(&db).expect("reopen ledger");
    let (outcome, insufficient_count, detail): (String, i64, String) = ledger
        .conn()
        .query_row(
            "SELECT outcome, insufficient, detail FROM continuity_checks
             WHERE insufficient > 0 ORDER BY rowid LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("stored insufficient result");
    assert_eq!(outcome, "failed", "legacy storage must remain non-success");
    assert!(insufficient_count > 0);
    assert!(detail.contains("successor_evidence_missing"));
}
