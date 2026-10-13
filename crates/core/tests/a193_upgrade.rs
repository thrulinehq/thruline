//! A193 / Q-A191.1 — a schema upgrade is paid by name, or not at all.
//!
//! The question A191 raised: a DDL change costs a full pass over the ledger,
//! the cost grows with the corpus, and it lands on whoever opens the ledger
//! first after a new binary is installed. That is usually not a person — it is
//! a hook with a five-second timeout, an MCP server, or an editor integration.
//! A168 found exactly that failure live.
//!
//! These tests pin the three properties the ruling asks for: a pending
//! migration is refused and costs nothing, the payment is visible when it is
//! asked for, and no path other than `thruline upgrade` can trigger it.

use rusqlite::Connection;
use thruline_core::error::CoreError;
use thruline_core::ledger::Ledger;
use thruline_core::upgrade::UpgradeState;

/// A ledger on disk carrying a real schema, with its fingerprint removed — the
/// exact state a user's ledger is in the moment they install a binary whose DDL
/// has changed.
fn ledger_with_pending_upgrade(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("ledger.db");
    {
        let ledger = Ledger::open(&path).expect("creating a fresh ledger must still work");
        drop(ledger);
    }
    let conn = Connection::open(&path).expect("reopening");
    conn.execute("DELETE FROM schema_migrations", [])
        .expect("clearing the fingerprint");
    drop(conn);
    path
}

#[test]
fn a_fresh_ledger_is_still_created_by_open() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("new.db");
    assert_eq!(
        Ledger::upgrade_state(&path).expect("state"),
        UpgradeState::Fresh
    );
    // Creating a schema in an empty file is free. Refusing THAT would mean no
    // ledger could ever be made, and the refusal is about migration, not
    // creation.
    let ledger = Ledger::open(&path).expect("a fresh ledger must open");
    assert!(!ledger.took_schema_fast_path());
    drop(ledger);
    assert_eq!(
        Ledger::upgrade_state(&path).expect("state"),
        UpgradeState::Current
    );
}

#[test]
fn an_existing_ledger_with_a_pending_migration_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = ledger_with_pending_upgrade(dir.path());

    let state = Ledger::upgrade_state(&path).expect("state");
    assert!(state.is_pending(), "{state:?}");

    let err = match Ledger::open(&path) {
        Ok(_) => panic!("open must refuse a pending migration"),
        Err(e) => e,
    };
    match err {
        CoreError::UpgradePending { ref estimate, .. } => {
            assert!(!estimate.is_empty(), "the refusal must carry an estimate");
        }
        other => panic!("expected UpgradePending, got {other:?}"),
    }
    assert!(
        err.to_string().contains("thruline upgrade"),
        "the refusal must name the one command that fixes it: {err}"
    );
}

/// The refusal must cost nothing. Not "cost less" — nothing: the fingerprint is
/// still absent afterwards, which is the observable proof that no step of the
/// full path ran.
#[test]
fn the_refusal_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = ledger_with_pending_upgrade(dir.path());

    for _ in 0..3 {
        assert!(Ledger::open(&path).is_err());
    }

    let conn = Connection::open(&path).expect("reopen");
    let stamped: i64 = conn
        .query_row("SELECT COUNT(*) FROM schema_migrations", [], |r| r.get(0))
        .expect("counting fingerprints");
    assert_eq!(
        stamped, 0,
        "a refused open stamped a fingerprint, so it did the work it refused"
    );
}

/// The payment is visible. Every step is announced, in order, exactly once —
/// because a half-minute of silence on a multi-gigabyte file is
/// indistinguishable from a hang, and a migration people kill halfway is the
/// one failure mode a schema upgrade must not have.
#[test]
fn the_upgrade_announces_every_step_in_order_and_then_stamps() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = ledger_with_pending_upgrade(dir.path());

    let mut seen: Vec<(String, usize, usize)> = Vec::new();
    let ledger = Ledger::open_upgrading(&path, &mut |step, index, total| {
        seen.push((step.to_string(), index, total));
    })
    .expect("the upgrade must succeed");
    assert!(!ledger.took_schema_fast_path());
    drop(ledger);

    let expected: Vec<&str> = Ledger::SCHEMA_STEPS.to_vec();
    let names: Vec<&str> = seen.iter().map(|(n, _, _)| n.as_str()).collect();
    assert_eq!(
        names, expected,
        "the announced steps must be the steps, in order"
    );
    for (i, (_, index, total)) in seen.iter().enumerate() {
        assert_eq!(*index, i + 1);
        assert_eq!(*total, expected.len());
    }

    // And afterwards everything works again, by the fast path.
    let reopened = Ledger::open(&path).expect("open after upgrade");
    assert!(
        reopened.took_schema_fast_path(),
        "after an upgrade the next open must cost nothing"
    );
    assert_eq!(
        Ledger::upgrade_state(&path).expect("state"),
        UpgradeState::Current
    );
}

/// An upgrade on an already-current ledger is a no-op that announces nothing,
/// so `thruline upgrade` is safe to run twice and safe to put in a script.
#[test]
fn upgrading_an_already_current_ledger_does_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("ledger.db");
    drop(Ledger::open(&path).expect("create"));

    let mut steps = 0;
    let ledger = Ledger::open_upgrading(&path, &mut |_, _, _| steps += 1).expect("upgrade");
    assert!(ledger.took_schema_fast_path());
    assert_eq!(steps, 0, "a current ledger announced {steps} steps of work");
}

/// The containment gate: `open_upgrading` is the only door to a migration, and
/// exactly one production caller is licensed to use it.
///
/// Enumerating licensed *callers* rather than counting occurrences is the A187
/// lesson — a containment assertion that matches a substring can be satisfied
/// by accident, which is the failure mode a containment gate exists to prevent.
#[test]
fn only_the_upgrade_verb_can_trigger_a_migration() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let licensed = [
        // The definition itself.
        "crates/core/src/ledger.rs",
        // `thruline upgrade`, and nothing else in the CLI.
        "crates/cli/src/main.rs",
    ];

    let mut offenders = Vec::new();
    for dir in ["crates/core/src", "crates/cli/src"] {
        walk(&root.join(dir), &mut |path| {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            if licensed.contains(&rel.as_str()) {
                return;
            }
            let text = std::fs::read_to_string(path).unwrap_or_default();
            if text.contains("open_upgrading") || text.contains("full_schema_path") {
                offenders.push(rel);
            }
        });
    }
    assert!(
        offenders.is_empty(),
        "these files can trigger a schema migration and are not licensed to: {offenders:?}"
    );

    // And in the licensed CLI file, the only caller is the upgrade verb.
    let main = std::fs::read_to_string(root.join("crates/cli/src/main.rs")).expect("main.rs");
    let callers = main.matches("Ledger::open_upgrading").count();
    assert_eq!(
        callers, 1,
        "the CLI calls Ledger::open_upgrading {callers} times; exactly one — \
         `thruline upgrade` — is licensed"
    );
}

fn walk(dir: &std::path::Path, f: &mut dyn FnMut(&std::path::Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, f);
        } else if path.extension().is_some_and(|e| e == "rs") {
            f(&path);
        }
    }
}
