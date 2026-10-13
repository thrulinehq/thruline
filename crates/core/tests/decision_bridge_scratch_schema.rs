//! C28 additive bridge schema check on a separate APFS clone of the founder
//! evidence basis. This test never creates a person act.

use thruline_core::decisions::authority::bridge;
use thruline_core::ledger::Ledger;

#[test]
#[ignore = "set C28_BRIDGE_SCRATCH_DB to a separate APFS clone of the evidence basis"]
fn additive_table_on_founder_scratch_clone_changes_no_recorded_rows() {
    let path = std::env::var("C28_BRIDGE_SCRATCH_DB").expect("scratch clone path");
    assert!(
        path.contains("/c28-scratch/"),
        "must use a separate c28-scratch clone"
    );
    let ledger = Ledger::open_upgrading(&path, &mut |step, index, total| {
        eprintln!("scratch upgrade {index}/{total}: {step}");
    })
    .expect("upgrade separate scratch clone");
    let counts = |ledger: &Ledger| -> (i64, i64, i64, i64) {
        (
            ledger
                .conn()
                .query_row("SELECT count(*) FROM projects", [], |r| r.get(0))
                .unwrap(),
            ledger
                .conn()
                .query_row("SELECT count(*) FROM sessions", [], |r| r.get(0))
                .unwrap(),
            ledger
                .conn()
                .query_row("SELECT count(*) FROM events", [], |r| r.get(0))
                .unwrap(),
            ledger
                .conn()
                .query_row("SELECT count(*) FROM decision_person_acts", [], |r| {
                    r.get(0)
                })
                .unwrap(),
        )
    };
    let before = counts(&ledger);
    bridge::install_schema_for_write(&ledger).expect("additive bridge table");
    let after = counts(&ledger);
    assert_eq!(before, after, "existing record rows changed");
    let table: i64 = ledger.conn().query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='decision_bridge_proofs'",
        [], |r| r.get(0),
    ).expect("bridge table");
    assert_eq!(table, 1);
    let columns = ledger
        .conn()
        .prepare("PRAGMA table_info(decision_bridge_proofs)")
        .expect("bridge columns")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("bridge column names")
        .collect::<Result<Vec<_>, _>>()
        .expect("bridge column values");
    assert!(
        columns.iter().any(|name| name == "supersession_json"),
        "scratch upgrade must support immutable supersession binding"
    );
    let proofs: i64 = ledger
        .conn()
        .query_row("SELECT count(*) FROM decision_bridge_proofs", [], |r| {
            r.get(0)
        })
        .expect("bridge proof rows");
    assert_eq!(proofs, 0, "scratch clone must receive no founder act");
}
