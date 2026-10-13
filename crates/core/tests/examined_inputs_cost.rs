//! T-3-2 / `L1.1` / Rule 13 — what the store grows by per examined-inputs
//! record, measured rather than estimated.
//!
//! The card's rule: *the examined-inputs record is a small typed row per
//! examination referencing existing snapshot ids and hashes, never a copy of
//! files; its cost per examination is measured and written into the doc
//! comment.* This test is the measurement, and it fails if the cost leaves the
//! band in either direction — a row that quietly grew a copy of a file, and an
//! edit that made the number meaningless, are both things a person should be
//! told about.
//!
//! `PRAGMA page_count × PRAGMA page_size` before and after N inserts, with the
//! store checkpointed and vacuumed so the WAL and free pages cannot flatter it.
//! Run with `--nocapture` to see the numbers.

use rusqlite::params;
use thruline_core::examined_inputs::{self as exi, ExaminedInputs, Input, SubjectKind, Target};
use thruline_core::ledger::Ledger;

/// Enough rows that one page of slack is noise rather than the answer.
const N: usize = 1_000;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
const SESSION: &str = "cccccccc-0000-4000-8000-00000000s001";

fn bytes_on_disk(ledger: &Ledger) -> u64 {
    let _ = ledger
        .conn()
        .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()));
    let _ = ledger.conn().execute_batch("VACUUM");
    let pages: i64 = ledger
        .conn()
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .unwrap_or(0);
    let size: i64 = ledger
        .conn()
        .query_row("PRAGMA page_size", [], |r| r.get(0))
        .unwrap_or(0);
    (pages.max(0) as u64) * (size.max(0) as u64)
}

/// One captured execution per record, because the store refuses a record about
/// an examination that does not exist.
fn binding(ledger: &Ledger, n: usize) -> String {
    let id = format!("cccccccc-0000-4000-8000-{n:012}");
    ledger
        .conn()
        .execute(
            "INSERT INTO evidence_bindings(id, seq, recorded_at, schema_version, scope_class,
                 scope_key, root, repository_identity, pre_identity, post_identity, exit_code,
                 term_signal, bound_at, source_session, doc, doc_sha256)
             VALUES(?1,?2,'2026-01-03T09:00:00Z',1,'test','bash:/tmp/p:cargo test',
                    '/tmp/p','repo','pre','post',0,NULL,'2026-01-03T09:00:00Z',?3,'{}','d')",
            params![id, n as i64, SESSION],
        )
        .expect("binding");
    id
}

/// Realistic field lengths: a 40-character revision, a tagged workspace
/// identity, a real-looking path, two 64-character digests and two tool
/// entries. Nothing here is a one-character placeholder that would flatter the
/// number.
fn record_for(id: &str, n: usize, with_app: bool) -> ExaminedInputs {
    let mut inputs = ExaminedInputs::unknown_for(SubjectKind::CheckRun, id);
    inputs.snapshot_id = Input::known(snapshot_id(n));
    inputs.source_identity = Input::known(format!("wsid1:{}", "a".repeat(64)));
    inputs.base_revision = Input::known("b".repeat(40));
    inputs.worktree_path = Input::known("/Users/example/projects/retry-delivery");
    inputs.lockfiles_sha256 = Input::known("c".repeat(64));
    inputs.config_sha256 = Input::known("d".repeat(64));
    inputs.config_files = vec![".thruline/cargo-test.json".to_string()];
    inputs.uncommitted = exi::Uncommitted::Present {
        files: 3,
        diff_sha256: Input::known("e".repeat(64)),
    };
    inputs.tools = vec![exi::ToolInput {
        name: "cargo".to_string(),
        version: Input::known("cargo 1.97.1 (abcdef123 2026-01-01)"),
        present: true,
    }];
    if with_app {
        inputs.target = Target::RunningApp {
            port: 45781,
            owner_pid: Input::known("12345"),
            owner_exe: Input::known("/Users/example/projects/retry-delivery/target/debug/serve"),
            database_path: Input::known(
                "/Users/example/projects/retry-delivery/data/outbox.sqlite3",
            ),
        };
    }
    inputs
}

/// The id of the workspace snapshot this record points at. The record holds
/// the id and nothing else: that is the whole point of the cost being small.
fn snapshot_id(n: usize) -> String {
    format!("ws_{n:026}")
}

fn measure(with_app: bool) -> u64 {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, '/tmp/p', 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
            params![PROJECT],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES(?1, ?2, '/tmp/p/s.jsonl', '2026-01-01T00:00:00Z')",
            params![SESSION, PROJECT],
        )
        .expect("session");
    let ids: Vec<String> = (0..N).map(|n| binding(&ledger, n)).collect();
    // The snapshots already exist: the record references them, and a reference
    // is what is being measured.
    for n in 0..N {
        ledger
            .conn()
            .execute(
                "INSERT INTO workspace_snapshots(id, taken_at, root, head, branch, complete,
                     doc, doc_sha256)
                 VALUES(?1,'2026-01-03T09:00:00Z','/tmp/p','abc','main',1,'{}','d')",
                params![snapshot_id(n)],
            )
            .expect("snapshot");
    }

    let before = bytes_on_disk(&ledger);
    for (n, id) in ids.iter().enumerate() {
        exi::record(
            &ledger,
            &record_for(id, n, with_app),
            "2026-01-03T09:05:00Z",
        )
        .expect("record");
    }
    let after = bytes_on_disk(&ledger);
    (after - before) / N as u64
}

#[test]
fn the_store_grows_by_a_measured_amount_per_examined_inputs_record() {
    let source_only = measure(false);
    let with_app = measure(true);
    println!("one examined-inputs record, source only       = {source_only} B");
    println!("one examined-inputs record with a running app = {with_app} B");

    // The band is wide enough for page-boundary noise and narrow enough that a
    // row which started copying a file would leave it.
    assert!(
        (400..=1_100).contains(&source_only),
        "source-only record cost left the band: {source_only} B"
    );
    assert!(
        (400..=1_300).contains(&with_app),
        "running-app record cost left the band: {with_app} B"
    );
    assert!(
        with_app >= source_only,
        "a record that says more cannot cost less: {with_app} vs {source_only}"
    );
}
