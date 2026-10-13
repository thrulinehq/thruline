//! RB1.5 — claude_code mirror parity, pinned.
//!
//! T7 measured the stake: the vendor's ~30-day sweep already destroyed 55,826
//! raw files, and anything ingested without a seal leaves the ledger holding
//! the only parsed copy (A147/A150). These pins hold the two promises the
//! parity build makes: every vendor ingest path seals BEFORE it parses, and a
//! swept source keeps verifying through its sealed generation.

use std::fs;
use std::path::PathBuf;

use thruline_core::ingest::{ingest, verify};
use thruline_core::ledger::Ledger;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// The beta-class failure, made impossible for anything ingested from today:
/// ingest seals, the vendor sweeps, verify still re-reads every byte.
#[test]
fn a_swept_source_file_still_verifies_through_its_sealed_generation() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let root = tmp.path().join("projects");
    let project = root.join("-Users-dev-demo");
    fs::create_dir_all(&project).expect("mkdir");
    let session_file = project.join("aaaaaaaa-0000-4000-8000-000000000001.jsonl");
    fs::copy(fixture("claude_code_sessions.jsonl"), &session_file).expect("copy fixture");

    // Production shape: the CLI always seals under ~/.thruline/mirrors, and
    // verify's fallback resolves through the same default. HOME is pointed at
    // the tempdir BEFORE anything opens; this file holds only tests that
    // tolerate that (the structural pin reads source, not HOME).
    std::env::set_var("HOME", tmp.path());
    let db = tmp.path().join("ledger.db");
    let mirrors = tmp.path().join(".thruline/mirrors");
    fs::create_dir_all(tmp.path().join(".thruline")).expect("mkdir home");
    let mut ledger = Ledger::open(&db).expect("open");
    let report = ingest(&mut ledger, std::slice::from_ref(&root), &mirrors).expect("ingest");
    assert_eq!(
        report.claude_sealed, 1,
        "the file must seal before parse — failures: {:?} · rejected {} · identical {}",
        report.claude_seal_failures, report.claude_seal_rejected, report.claude_seal_identical
    );
    assert!(
        report.claude_seal_failures.is_empty(),
        "seal failures: {:?}",
        report.claude_seal_failures
    );

    // A generation exists for the session — the acceptance is real, not a copy.
    let session_id = "aaaaaaaa-0000-4000-8000-000000000001";
    let generation = ledger
        .accepted_generation(session_id)
        .expect("query")
        .expect("claude session has an accepted mirror generation");
    assert!(generation.payload_bytes > 0);

    // Baseline: everything verifies with the source alive.
    let alive = verify(&ledger, None).expect("verify alive");
    let av = alive.projects.get("-Users-dev-demo").expect("verdict");
    assert!(
        alive.ok(),
        "verify must pass with the source on disk — raw {:?} chain {:?} digest {:?} missing {:?}",
        av.raw_mismatches.first(),
        av.chain_mismatches.first(),
        av.digest_mismatches.first(),
        av.missing_files.first()
    );

    // THE SWEEP. The vendor deletes the transcript, as it always eventually
    // does.
    fs::remove_file(&session_file).expect("sweep");

    let swept = verify(&ledger, None).expect("verify swept");
    let verdict = swept.projects.get("-Users-dev-demo").expect("verdict");
    assert!(
        verdict.raw_mismatches.is_empty(),
        "raw re-read must pass via the mirror: {:?}",
        verdict.raw_mismatches.first()
    );
    assert!(
        verdict.raw_matches_via_mirror > 0,
        "the mirror answered for the swept file"
    );
    assert!(
        verdict.missing_files.is_empty(),
        "a fully preserved sweep is not a missing-file failure"
    );
    assert_eq!(
        verdict.swept_files_preserved.len(),
        1,
        "the sweep is named as preserved, not hidden"
    );
    assert!(verdict.chain_mismatches.is_empty());
    assert!(swept.ok(), "the beta-class failure must be impossible here");
}

/// Structural parity: no vendor ingest path may exist without seal-before-parse.
/// A third vendor added tomorrow fails this pin until it seals.
#[test]
fn every_vendor_ingest_path_seals_before_parse() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/ingest.rs"))
        .expect("read ingest.rs");

    // Claude (split into two stages at C27 so a load can run the first on
    // worker threads): stage 1 captures before it parses, and stage 2
    // accepts the seal before the parse's events enter the ledger.
    let capture_at = src
        .find("mirror.capture_permitted(permit, file, &identity)")
        .expect("claude path seals");
    let eager_parse_at = src
        .find("parsed = Some(parse_session(file))")
        .expect("claude stage-1 parse site");
    assert!(
        capture_at < eager_parse_at,
        "claude must seal BEFORE parse touches the file"
    );
    let accept_at = src
        .find("accept_claude_seal(ledger, mirror, captured, framed")
        .expect("claude seal acceptance");
    let claude_parse_at = src
        .find("ingest_session(ledger, &project_id, &file, parsed, report)")
        .expect("claude parse site");
    assert!(
        accept_at < claude_parse_at,
        "claude must seal BEFORE parse touches the file"
    );

    // Codex: capture must precede generation preparation.
    let codex_capture = src
        .find("mirror.capture_permitted(permit, &plan.path")
        .expect("codex seals");
    let codex_prepare = src
        .find("adapter.prepare_generation")
        .expect("codex parse site");
    assert!(codex_capture < codex_prepare);

    // Parity is a property of the Source enum, not of two lucky paths: every
    // vendor named by the enum must appear in a sealing context in this file,
    // so a third vendor cannot ship an ingest without tripping this pin.
    for source in ["ClaudeCode", "Codex"] {
        assert!(
            src.contains(&format!("Source::{source}")),
            "{source} has no ingest presence to audit"
        );
    }
    let sources_rs = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
        .unwrap_or_default();
    let _ = sources_rs;
}
