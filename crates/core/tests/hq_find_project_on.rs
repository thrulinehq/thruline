//! HQ-NEEDS (C28, "read-only HQ project lookup for inline HTTP"): the Decisions
//! inline route resolves an HQ project and its member ledger projects before it
//! opens the ledger for writing. `find_project_on` is that lookup on a
//! connection the caller opened `SQLITE_OPEN_READ_ONLY`, so the whole HTTP path
//! can stay physically read-only.
//!
//! It is the same derivation as `find_project` (native recorded cwd, multi-
//! member grouping, scratch roots left out), not a copy of it: this pins that
//! the two agree on a real ingested record, project by project, and that a
//! read-only connection is enough for all of it.

mod scratch;

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use scratch::TempTree;
use thruline_core::hq;
use thruline_core::ingest::{ingest, ingest_codex, CodexIngestOptions};
use thruline_core::Ledger;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hq")
}

#[test]
fn a_read_only_connection_finds_the_same_projects_as_the_ledger_does() {
    let tmp = TempTree::new();
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(
        &mut ledger,
        &[fixtures().join("claude/-work-app")],
        &tmp.path().join("mirrors"),
    )
    .expect("claude ingest");
    let codex: Vec<PathBuf> = std::fs::read_dir(fixtures().join("codex"))
        .expect("codex fixtures")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    ingest_codex(
        &mut ledger,
        &codex,
        &CodexIngestOptions {
            mirror_root: tmp.path().join("mirrors"),
            owner_key: Some("hq-test-owner".to_string()),
        },
    )
    .expect("codex ingest");

    let (expected, expected_excluded) =
        hq::hq_projects_and_excluded(&ledger).expect("projects on the ledger");
    assert!(!expected.is_empty(), "the fixture must hold a project");

    let read_only = Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .expect("a read-only connection");
    let (found, found_excluded) =
        hq::hq_projects_and_excluded_on(&read_only).expect("projects on a read-only connection");
    assert_eq!(format!("{found:?}"), format!("{expected:?}"));
    assert_eq!(found_excluded, expected_excluded);

    for project in &expected {
        let one = hq::find_project_on(&read_only, &project.id)
            .expect("lookup")
            .expect("the project is found by its id");
        assert_eq!(format!("{one:?}"), format!("{project:?}"));
        let on_ledger = hq::find_project(&ledger, &project.id)
            .expect("lookup")
            .expect("the ledger finds it too");
        assert_eq!(format!("{one:?}"), format!("{on_ledger:?}"));
    }
    assert!(hq::find_project_on(&read_only, "no-such-project")
        .expect("lookup")
        .is_none());
    assert!(hq::find_project_on(&read_only, "")
        .expect("lookup")
        .is_none());

    // And the index of the project reads on the same connection.
    let index = hq::index_on(&read_only, &expected[0]).expect("index on a read-only connection");
    let on_ledger = hq::index(&ledger, &expected[0]).expect("index");
    let now = hq::parse_ms("2026-10-07T00:00:00Z").expect("instant");
    let threads = |i: &hq::Index| -> Vec<String> {
        i.view(now).threads.iter().map(|t| t.id.clone()).collect()
    };
    assert!(!threads(&index).is_empty(), "the fixture must hold threads");
    assert_eq!(threads(&index), threads(&on_ledger));
}
