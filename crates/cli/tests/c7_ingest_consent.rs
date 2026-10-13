//! C7 — the A131 consent flow (A183).
//!
//! The footgun this closes is not hypothetical: a bare `thruline ingest`
//! appended 1,230,467 events, wrote a 7.0 GB mirror, and stopped by filling
//! the disk to 120 MiB free, taking the host down. Nothing about the command
//! said it would do any of that.

use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

fn run(args: &[&str]) -> (String, String, bool) {
    let out = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .args(args)
        .output()
        .expect("run");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.success(),
    )
}

fn seed_transcripts(root: &Path, n: usize) {
    let project = root.join("-tmp-consent-project");
    std::fs::create_dir_all(&project).expect("mkdir");
    for i in 0..n {
        std::fs::write(
            project.join(format!("{i:08}-0000-4000-8000-000000000000.jsonl")),
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"hi\"}}\n",
        )
        .expect("write");
    }
}

#[test]
fn ingest_previews_by_default_and_writes_nothing() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let src = temp.path().join("transcripts");
    seed_transcripts(&src, 3);

    let (out, _, ok) = run(&[
        "--db",
        db.to_str().unwrap(),
        "ingest",
        src.to_str().unwrap(),
        "--mirror-root",
        temp.path().join("mirror").to_str().unwrap(),
    ]);
    assert!(ok, "the preview must succeed");
    assert!(
        out.contains("INGEST PREVIEW") && out.contains("nothing written"),
        "the preview must say it wrote nothing: {out}"
    );
    assert!(out.contains("3 transcript file(s)"), "{out}");
    // A131(d): the mirror is the larger half and is disclosed separately.
    assert!(
        out.contains("mirror"),
        "the mirror growth is not disclosed: {out}"
    );
    assert!(
        out.contains("free space"),
        "the headroom is not shown: {out}"
    );
    assert!(
        out.contains("thruline ingest --apply"),
        "the preview must name how to proceed: {out}"
    );

    // And it truly wrote nothing: no ledger rows exist.
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let events: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .expect("count");
    assert_eq!(events, 0, "a preview appended events");
}

#[test]
fn apply_is_what_actually_ingests() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let src = temp.path().join("transcripts");
    seed_transcripts(&src, 2);

    let (out, err, ok) = run(&[
        "--db",
        db.to_str().unwrap(),
        "ingest",
        src.to_str().unwrap(),
        "--mirror-root",
        temp.path().join("mirror").to_str().unwrap(),
        "--apply",
    ]);
    assert!(
        ok,
        "apply should succeed on a tiny scope; stdout: {out}; stderr: {err}"
    );
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let events: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .expect("count");
    assert!(events > 0, "--apply ingested nothing");
}

#[test]
fn a_second_vendor_is_never_in_scope_without_being_named() {
    // A131(b). The incident widened silently to the whole of ~/.codex. Bare
    // ingest is Claude Code only now, and the preview says so.
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("ledger.db");
    let (out, _, ok) = run(&["--db", db.to_str().unwrap(), "ingest"]);
    assert!(ok);
    assert!(
        !out.contains("codex "),
        "bare ingest put a second vendor in scope: {out}"
    );
    assert!(
        out.contains("--source codex"),
        "the preview must state how a second vendor is opted into: {out}"
    );
}

#[test]
fn the_preview_is_the_documented_default_in_help() {
    let (out, _, _) = run(&["ingest", "--help"]);
    assert!(
        out.contains("previews and writes nothing"),
        "help must state the default is a preview: {out}"
    );
    assert!(out.contains("--apply"), "{out}");
}
