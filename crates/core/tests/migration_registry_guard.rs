//! R-2026-09-24-21 — a DDL task that forgets to declare itself goes red here,
//! by name.
//!
//! The five migration tests in this layer used to carry five hand-written
//! lists of the tables later tasks had added. Those lists are now one
//! directory, `crates/core/migrations/`. That trade is only worth making if
//! the directory cannot silently fall behind the schema, so this file asks the
//! question the lists never could: **is every table this tree's DDL creates
//! after the pre-T-3 baseline claimed by exactly one registry file?**
//!
//! It reads the DDL the way a user's store sees it — by creating a fresh
//! ledger and reading `sqlite_master` — because `Ledger::DDL` is private and
//! because what is actually created is the fact that matters, not the string
//! that was meant to create it.

use rusqlite::Connection;
use thruline_core::ledger::Ledger;

mod migration_registry;

/// The schema before the Change layer's Phase 3 tasks began: the Phase 2 exit
/// fixture, which is a real commit's own DDL. Everything this tree creates
/// beyond it is a later task's doing, and every later task owes a registry
/// file.
fn baseline_objects() -> Vec<(String, String)> {
    let sql = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/schema-phase-2.sql"),
    )
    .expect("the Phase 2 exit schema fixture must be readable");
    let dir = tempfile::tempdir().expect("tempdir");
    let conn = Connection::open(dir.path().join("baseline.db")).expect("open baseline");
    conn.execute_batch(&sql)
        .expect("the Phase 2 exit fixture must be executable SQL");
    read_objects(&conn)
}

/// `(kind, name)` for every table, index and trigger in a store.
fn read_objects(conn: &Connection) -> Vec<(String, String)> {
    let mut statement = conn
        .prepare(
            "SELECT type, name FROM sqlite_master
              WHERE type IN ('table','index','trigger')
                AND name NOT LIKE 'sqlite_%'
              ORDER BY type, name",
        )
        .expect("prepare");
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect("query")
        .filter_map(|r| r.ok())
        .collect()
}

/// This tree's DDL as a fresh store actually receives it.
fn head_objects() -> Vec<(String, String)> {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("ledger.db");
    drop(Ledger::open(&path).expect("a fresh ledger must open"));
    let conn = Connection::open(&path).expect("reopen");
    read_objects(&conn)
}

fn names(objects: &[(String, String)], kind: &str) -> Vec<String> {
    objects
        .iter()
        .filter(|(k, _)| k == kind)
        .map(|(_, n)| n.clone())
        .collect()
}

/// Every table this tree adds beyond the pre-T-3 baseline is claimed by
/// exactly one registry file. A new DDL task that forgets its file fails here
/// with the table's name in the message.
#[test]
fn every_table_beyond_the_baseline_is_claimed_by_exactly_one_registry_file() {
    let baseline = names(&baseline_objects(), "table");
    let head = names(&head_objects(), "table");

    let mut claims: Vec<(String, String)> = Vec::new();
    for entry in migration_registry::entries() {
        for table in &entry.tables {
            claims.push((table.clone(), entry.source_file.clone()));
        }
    }

    let unclaimed: Vec<&String> = head
        .iter()
        .filter(|t| !baseline.contains(t))
        .filter(|t| !claims.iter().any(|(c, _)| &c == t))
        .collect();
    assert!(
        unclaimed.is_empty(),
        "these tables exist in this tree's DDL but no file in crates/core/migrations/ \
         claims them — the task that added them owes one: {unclaimed:?}"
    );

    // Claimed twice is the same failure wearing the other face: two files
    // would each look complete while the order between them decided the
    // answer. `migration_registry::entries()` panics on it; assert the panic
    // never had to fire by checking the count agrees.
    let mut claimed_names: Vec<String> = claims.iter().map(|(t, _)| t.clone()).collect();
    claimed_names.sort();
    let mut deduped = claimed_names.clone();
    deduped.dedup();
    assert_eq!(
        claimed_names, deduped,
        "a table is claimed by two registry files"
    );
}

/// The registry claims nothing that is not there. A file naming a table that
/// was renamed or dropped would make every later-task list quietly wrong.
#[test]
fn the_registry_claims_no_object_this_tree_does_not_create() {
    let head = head_objects();
    let tables = names(&head, "table");
    let indexes = names(&head, "index");
    let triggers = names(&head, "trigger");

    let mut missing: Vec<String> = Vec::new();
    for entry in migration_registry::entries() {
        for table in &entry.tables {
            if !tables.contains(table) {
                missing.push(format!("{} claims table {table}", entry.source_file));
            }
        }
        for index in &entry.indexes {
            if !indexes.contains(index) {
                missing.push(format!("{} claims index {index}", entry.source_file));
            }
        }
        for trigger in &entry.triggers {
            if !triggers.contains(trigger) {
                missing.push(format!("{} claims trigger {trigger}", entry.source_file));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "the DDL task registry names objects this tree does not create: {missing:?}"
    );
}

/// Every task the migration tests ask about is in the registry, and the answer
/// it gives shrinks as the tasks go on — the ordering is a real ordering, not
/// nine files in a directory.
#[test]
fn the_registry_orders_the_tasks_the_migration_tests_ask_about() {
    let asked = [
        "T-2-1",
        "T-3-1",
        "T-3-2",
        "T-3-2b",
        "T-S-2",
        "T-S-4",
        "T-3-4",
        "T-3-6-F1",
        "T-5-3",
        "P6-5B",
        "T-4-1",
        "T-4-2",
        "T-4-4",
        "C24-DECISIONS",
        "C24-PERSON-AUTHORITY",
        "C30-R-LEDGER",
        "C47-REMOVE-RECORDS",
    ];
    let mut previous: Option<usize> = None;
    for task in asked {
        let later = migration_registry::tables_added_after(task);
        let mut sorted = later.clone();
        sorted.sort();
        assert_eq!(later, sorted, "{task}'s later-table set must be sorted");
        if let Some(previous) = previous {
            assert!(
                later.len() < previous,
                "{task} must have strictly fewer tables after it than the task before it"
            );
        }
        previous = Some(later.len());
    }
    // The tasks after T-5-3 that ADD tables are the A2 merges, in the order
    // they landed: P6-5B, then the review lane's T-4-1 and T-4-2; then
    // T-4-4's attempt budget on the integration, then C24's Phase 1 decisions
    // and Phase 2 person-action receipts, C30's R-LEDGER (what a person did),
    // and C47's removal receipt. Each brought its own file.
    let mut after_t_5_3: Vec<String> = [
        "P6-5B",
        "T-4-1",
        "T-4-2",
        "T-4-4",
        "C24-DECISIONS",
        "C24-PERSON-AUTHORITY",
        "C30-R-LEDGER",
        "C47-REMOVE-RECORDS",
    ]
    .into_iter()
    .flat_map(migration_registry::tables_added_by)
    .collect();
    after_t_5_3.sort();
    assert_eq!(
        migration_registry::tables_added_after("T-5-3"),
        after_t_5_3,
        "R-2026-09-24-22 landed after T-5-3 and added no table — it renamed one, \
         which is why it claims none here; the tables after T-5-3 are P6-5B's and \
         the review lane's, and C24's two phases, each of which brought its own file, as a task that \
         ADDS one must"
    );
}

/// The tables added by tasks that landed after the hand-written lists were
/// retired — everything ordered after R-2026-09-24-22. Those lists could not
/// have named them, so the comparison with the lists leaves them out, by the
/// registry's own order rather than by a second list kept here.
fn added_since_the_lists() -> Vec<String> {
    let entries = migration_registry::entries();
    let cutoff = entries
        .iter()
        .find(|e| e.task == "R-2026-09-24-22")
        .expect("the rename that followed the lists must be in the registry")
        .order;
    entries
        .into_iter()
        .filter(|e| e.order > cutoff)
        .flat_map(|e| e.tables)
        .collect()
}

/// `tables_added_after(task)` as the retired lists could have known it.
fn as_the_lists_knew_it(task: &str) -> Vec<String> {
    let since = added_since_the_lists();
    migration_registry::tables_added_after(task)
        .into_iter()
        .filter(|t| !since.contains(t))
        .collect()
}

/// The name a table carried at `ba1b8838` mapped to the name it carries now.
/// The mapping comes from the registry, not from a second copy of the same
/// fact written into a test.
fn renamed(table: &str) -> String {
    for rename in migration_registry::renames() {
        if rename.kind == "table" && rename.from == table {
            return rename.to;
        }
    }
    table.to_string()
}

/// The union the registry yields is the union the five hand-written lists
/// yielded, by name. This is the migration's own evidence, kept rather than
/// thrown away: if a future edit to the registry changes the answer, this says
/// so against the values that were there before the change.
#[test]
fn the_registry_yields_the_same_union_the_enumerated_lists_did() {
    // Every table that appeared in any of the five `ADDED_BY_LATER_TASKS` /
    // `LATER_CHANGE_TABLES` arrays at ba1b8838, which is their union.
    //
    // One of them has since been renamed (R-2026-09-24-22). The array below is
    // left as those files carried it, and the rename is applied to it here, by
    // name: editing the history to match the present would throw away the
    // evidence this test exists to keep.
    let enumerated = vec![
        "change_check_sessions",
        "change_criterion_evidence",
        "change_criterion_freshness",
        "change_decision_exceptions",
        "change_decisions",
        "change_dispositions",
        "change_run_executions",
        "check_run_records",
        "check_run_tallies",
        "examination_inputs",
    ];
    let mut enumerated: Vec<String> = enumerated.into_iter().map(renamed).collect();
    enumerated.sort();

    // T-2-1 is the earliest task in the registry, so everything any list could
    // have named is in the set of tables added after it.
    assert_eq!(
        as_the_lists_knew_it("T-2-1"),
        enumerated,
        "the registry's union differs from the union of the lists it replaced"
    );

    // And the per-task answers, each against the array that file carried.
    let cases: [(&str, &[&str]); 4] = [
        (
            "T-3-1",
            &[
                "change_check_sessions",
                "change_criterion_freshness",
                "change_decision_exceptions",
                "change_decisions",
                "change_dispositions",
                "change_run_executions",
                "check_run_records",
                "check_run_tallies",
                "examination_inputs",
            ],
        ),
        (
            "T-3-2",
            &[
                "change_check_sessions",
                "change_criterion_freshness",
                "change_decision_exceptions",
                "change_decisions",
                "change_dispositions",
                "change_run_executions",
                "check_run_records",
                "check_run_tallies",
            ],
        ),
        (
            "T-3-2b",
            &[
                "change_criterion_freshness",
                "change_decision_exceptions",
                "change_decisions",
                "change_dispositions",
                "change_run_executions",
                "check_run_records",
                "check_run_tallies",
            ],
        ),
        (
            "T-S-4",
            &[
                "change_decision_exceptions",
                "change_dispositions",
                "change_run_executions",
                "check_run_records",
                "check_run_tallies",
            ],
        ),
    ];
    for (task, expected) in cases {
        let mut expected: Vec<String> = expected.iter().map(|t| renamed(t)).collect();
        expected.sort();
        assert_eq!(
            as_the_lists_knew_it(task),
            expected,
            "{task}'s registry answer differs from the array it used to carry"
        );
    }
}

/// R-2026-09-24-22 — a recorded rename is a fact about this tree, checkable
/// both ways: the new name is created here, and the old one is **gone**.
///
/// The second half is the half that matters. A rename that left the old object
/// behind would be an addition wearing a rename's clothes, and a store that
/// still had `change_dispositions` would go on being written to by nothing and
/// read by nobody while the waivers in it quietly stopped counting.
#[test]
fn every_recorded_rename_really_happened_in_this_tree() {
    let head = head_objects();
    let mut wrong: Vec<String> = Vec::new();
    for rename in migration_registry::renames() {
        let present = |name: &str| {
            head.iter()
                .any(|(kind, n)| kind == &rename.kind && n == name)
        };
        if !present(&rename.to) {
            wrong.push(format!(
                "the registry says {} {} became {}, but this tree creates no such {}",
                rename.kind, rename.from, rename.to, rename.kind
            ));
        }
        if present(&rename.from) {
            wrong.push(format!(
                "the registry says {} {} was renamed to {}, but this tree still creates {}",
                rename.kind, rename.from, rename.to, rename.from
            ));
        }
    }
    assert!(wrong.is_empty(), "{wrong:?}");
}

/// The retired table name survives in exactly the places that are ABOUT it,
/// and a third would be a rename that did not finish.
///
/// `change_dispositions` is now a historical name: it is allowed in the
/// migration that renames it, in the registry file that records the rename, in
/// the pinned fixture of the schema that had it, and in the prose that explains
/// why it changed. Anywhere else it is a caller or a query that will find
/// nothing.
#[test]
fn the_retired_table_name_survives_only_in_the_migration_and_the_registry() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("the workspace root")
        .to_path_buf();
    let allowed = [
        "crates/core/src/ledger.rs",
        "crates/core/src/change_readiness.rs",
        // The upgrade's row census compares the retired name's count before with
        // the renamed table's count after (`upgrade::RENAMED_TABLES`), so a rename
        // that moves every row does not read as a table that lost them.
        "crates/core/src/upgrade.rs",
        "crates/core/tests/migration_registry_guard.rs",
        "crates/core/tests/obligation_waiver_rename_migration.rs",
        "crates/core/tests/change_exception_migration.rs",
        "crates/core/migrations/090-t-5-3.toml",
        "crates/core/migrations/README.md",
        "crates/core/migrations/100-r-2026-09-24-22.toml",
        "crates/core/tests/fixtures/schema-t-5-3.sql",
        // T-4-4's pinned base schema is `ledger.rs`'s DDL byte for byte (its
        // digest is the pin), so it carries the one DDL comment that explains
        // the rename — the same prose `ledger.rs` is allowed above for. It
        // names no table by the old name.
        "crates/core/tests/fixtures/schema-t-4-4-base.sql",
        // C24 Phase 2 pins the complete Phase 1 DDL byte for byte; it carries
        // the same historical rename comment, not the retired table.
        "crates/core/tests/fixtures/schema-c24-authority-base.sql",
        // C30 R-LEDGER pins the DDL at `74b06079` byte for byte; it carries the
        // same historical rename comment, not the retired table.
        "crates/core/tests/fixtures/schema-c30-r-ledger-base.sql",
    ];
    let searched = [
        "crates/core/src",
        "crates/core/tests",
        "crates/core/migrations",
        "crates/cli/src",
        "crates/cli/tests",
    ];

    let mut offenders: Vec<String> = Vec::new();
    for dir in searched {
        let mut stack = vec![root.join(dir)];
        while let Some(path) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&path) else {
                continue;
            };
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                if !text.contains("change_dispositions") {
                    continue;
                }
                let relative = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                if !allowed.contains(&relative.as_str()) {
                    offenders.push(relative);
                }
            }
        }
    }
    offenders.sort();
    assert!(
        offenders.is_empty(),
        "`change_dispositions` was renamed by R-2026-09-24-22; these files still \
         name it, and none of them is the migration or the registry: {offenders:?}"
    );
}
