//! A205 (MANDATE-v4 §11) — `thruline verify` covers the observation chain.
//!
//! The compiled production binary, a real SQLite file, fresh processes: the
//! §33 integration shape. No new verbs this stage — the observation stream is
//! part of what "verify the ledger" means from here on.

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;
use thruline_core::observation::{
    append, Completeness, NewObservation, ObservationType, ReceiptTerminal, SourceReceipt,
};
use thruline_core::Ledger;

fn run_verify(db: &Path) -> (String, String, Option<i32>) {
    let out = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args(["--db", db.to_str().expect("db path"), "verify"])
        .output()
        .expect("run thruline verify");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code(),
    )
}

#[test]
fn verify_covers_the_observation_chain() {
    let dir = tempdir().expect("tempdir");
    let db = dir.path().join("ledger.db");

    // A fresh ledger: zero links, chain vacuously whole, exit 0.
    {
        Ledger::open(&db).expect("create");
    }
    let (stdout, stderr, code) = run_verify(&db);
    assert_eq!(code, Some(0), "fresh ledger verifies; stderr: {stderr}");
    assert!(
        stdout.contains("observations"),
        "verify must report the observation chain: {stdout}"
    );

    // Two observations on the chain: the count is visible.
    {
        let ledger = Ledger::open(&db).expect("open");
        let receipt = SourceReceipt::new(ReceiptTerminal::Filesystem {
            path: "/tmp/observed".into(),
            sha256: None,
        });
        for (i, t) in [
            ObservationType::WorkspaceObserved,
            ObservationType::CommandFinished,
        ]
        .into_iter()
        .enumerate()
        {
            let observed_at = format!("2026-08-31T18:00:0{i}Z");
            append(
                &ledger,
                &NewObservation {
                    observation_type: t,
                    source_adapter: "thruline",
                    source_session: "sess-cli",
                    repository_identity: "repo:cli",
                    workspace_identity: None,
                    observed_at: &observed_at,
                    completeness: Completeness::Complete,
                    receipt: &receipt,
                },
            )
            .expect("append");
        }
    }
    let (stdout, stderr, code) = run_verify(&db);
    assert_eq!(code, Some(0), "intact chain verifies; stderr: {stderr}");
    assert!(
        stdout.contains("observations") && stdout.contains("2 links add up"),
        "the two links are reported: {stdout}"
    );

    // A tampered link fails the whole verify, loudly, with a nonzero exit.
    {
        let ledger = Ledger::open(&db).expect("open");
        ledger
            .conn()
            .execute_batch(
                "DROP TRIGGER observations_no_update;
                 UPDATE observations SET completeness='unknown' WHERE seq = 0;",
            )
            .expect("forge");
    }
    let (stdout, _stderr, code) = run_verify(&db);
    assert_ne!(code, Some(0), "a broken observation chain must fail verify");
    assert!(
        stdout.contains("DOES NOT ADD UP"),
        "the failure is named: {stdout}"
    );
}
