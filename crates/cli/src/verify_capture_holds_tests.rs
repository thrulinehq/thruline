//! Injected authored receipts qualify the caller seam; native HQ checks remain
//! the merged lane's responsibility. These hooks never enter an ordinary CLI.
use super::*;
use std::fs;
use std::path::{Path, PathBuf};

const RECEIPT: &[u8] = b"authored capture-hold chain receipt";
const CANARY: &str = concat!("sk-", "TLC44CaptureCanaryAlpha12345Beta67890Gamma");

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("authored fixture");
    let db = temp.path().join("ledger.db");
    drop(Ledger::open(&db).expect("authored empty ledger"));
    fs::write(db.with_extension("capture-receipt"), RECEIPT).expect("authored receipt");
    (temp, db)
}

fn authored_capture_check(ledger: &Ledger) -> thruline_core::error::Result<u64> {
    let db = Path::new(ledger.conn().path().expect("authored on-disk ledger"));
    let path = db.with_extension("capture-receipt");
    let bytes = fs::read(&path).map_err(|error| CoreError::io(&path, error))?;
    if sha256_hex(&bytes) != sha256_hex(RECEIPT) {
        return Err(CoreError::Integrity(
            "authored capture receipt changed".into(),
        ));
    }
    Ok(1)
}

fn exit(checks: &Checks) -> u8 {
    exit_code(
        &checks.report,
        checks.error.as_ref(),
        &checks.observations,
        &checks.permissions,
        checks.capture_holds.as_ref(),
    )
}

#[test]
fn supplied_capture_receipt_is_checked_and_changed_or_gone_are_distinct() {
    let (_temp, db) = fixture();
    let checks = collect_checks(&db, None, Some(authored_capture_check)).expect("checks");
    assert_eq!(checks.report.exit_code(), 0);
    assert_eq!(exit(&checks), 0);
    let row = chain_json(checks.capture_holds.as_ref().expect("provided check"));
    assert_eq!(row["state"], "INTACT");
    assert_eq!(row["links_checked"], 1);
    assert_eq!(row["reason"], Value::Null);

    fs::write(
        db.with_extension("capture-receipt"),
        b"changed authored bytes",
    )
    .expect("authored contradiction");
    let checks = collect_checks(&db, None, Some(authored_capture_check)).expect("checks");
    assert_eq!(checks.report.exit_code(), 0);
    assert_eq!(
        exit(&checks),
        1,
        "the supplied chain must contribute to exit"
    );
    let row = chain_json(checks.capture_holds.as_ref().expect("provided check"));
    assert_eq!(row["state"], "MISMATCH");
    assert_eq!(row["links_checked"], Value::Null);

    fs::remove_file(db.with_extension("capture-receipt")).expect("authored source gone");
    let checks = collect_checks(&db, None, Some(authored_capture_check)).expect("checks");
    assert_eq!(exit(&checks), 2, "missing evidence is not a mismatch");
    let row = chain_json(checks.capture_holds.as_ref().expect("provided check"));
    assert_eq!(row["state"], "UNVERIFIABLE");
}

#[test]
fn no_capture_adapter_is_absent_without_a_success_claim() {
    let (_temp, db) = fixture();
    let checks = collect_checks(&db, None, None).expect("checks");
    assert!(checks.capture_holds.is_none());
    assert_eq!(exit(&checks), 0);
}

#[test]
fn supplied_capture_check_still_runs_after_primary_read_abort() {
    let (_temp, db) = fixture();
    let ledger = Ledger::open(&db).expect("authored ledger");
    ledger
        .conn()
        .execute_batch("DROP TABLE projects")
        .expect("authored read failure");
    fs::write(
        db.with_extension("capture-receipt"),
        b"changed authored bytes",
    )
    .expect("authored contradiction");
    let checks = checks_for_ledger(&ledger, None, Some(authored_capture_check));
    assert!(matches!(checks.error, Some(CoreError::Sqlite(_))));
    assert!(!checks.report.complete);
    assert_eq!(
        exit(&checks),
        1,
        "known supplied-chain mismatch dominates U"
    );
    fs::remove_file(db.with_extension("capture-receipt")).expect("authored source gone");
    let checks = checks_for_ledger(&ledger, None, Some(authored_capture_check));
    assert_eq!(exit(&checks), 2);
}

fn must_not_run(_: &Ledger) -> thruline_core::error::Result<u64> {
    panic!("adapter must not run without a qualified open ledger")
}

#[test]
fn present_capture_adapter_is_not_checked_when_ledger_open_is_unavailable() {
    let temp = tempfile::tempdir().expect("authored fixture");
    let blocking_file = temp.path().join(CANARY);
    fs::write(&blocking_file, b"authored path blocker").expect("authored blocker");
    let checks = collect_checks(&blocking_file.join("ledger.db"), None, Some(must_not_run))
        .expect("registered unavailable report");
    assert_eq!(exit(&checks), 2);
    let row = chain_json(checks.capture_holds.as_ref().expect("present adapter"));
    assert_eq!(row["state"], "UNVERIFIABLE");
    assert_eq!(row["links_checked"], Value::Null);
    assert_eq!(row["reason"], "not checked: ledger unavailable");
}

#[test]
fn capture_error_locator_is_masked_without_changing_qualified_state() {
    let temp = tempfile::tempdir().expect("authored fixture");
    let folder = temp.path().join(CANARY);
    fs::create_dir(&folder).expect("authored directory");
    let db = folder.join("ledger.db");
    drop(Ledger::open(&db).expect("authored ledger"));
    let checks = collect_checks(&db, None, Some(authored_capture_check)).expect("checks");
    assert_eq!(exit(&checks), 2);
    let row = chain_json(checks.capture_holds.as_ref().expect("provided check"));
    assert_eq!(row["state"], "UNVERIFIABLE");
    assert!(!row.to_string().contains(CANARY), "authored canary leaked");
}

#[test]
fn schema_pending_remains_three_without_calling_the_capture_adapter() {
    let (_temp, db) = fixture();
    let conn = rusqlite::Connection::open(&db).expect("authored database");
    conn.execute("DELETE FROM schema_migrations", [])
        .expect("authored pending schema");
    drop(conn);
    assert!(matches!(
        collect_checks(&db, None, Some(must_not_run)),
        Err(CoreError::UpgradePending { .. })
    ));
}
