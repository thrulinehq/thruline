//! C27 (a): the project index read by several read-only connections at once
//! is the index one statement reads. For the same record and the same clock,
//! the project view, the list row, the statements and every thread view are
//! byte-identical whichever way the rows were read — against the HQ fixtures
//! (`tests/fixtures/hq/`) ingested through the real ingest path.

mod scratch;

use std::path::{Path, PathBuf};

use scratch::TempTree;
use thruline_core::hq::{self, RowRead};
use thruline_core::ingest::{ingest, ingest_codex, CodexIngestOptions};
use thruline_core::Ledger;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hq")
}

/// The fixtures, Codex first: its session id sorts after Claude Code's, so
/// rowid order and session order disagree, and a read that took rows in
/// rowid order without putting them back would serve them out of order.
fn store(tmp: &TempTree) -> PathBuf {
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    let codex: Vec<PathBuf> = std::fs::read_dir(fixtures().join("codex"))
        .expect("codex fixtures")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    let report = ingest_codex(
        &mut ledger,
        &codex,
        &CodexIngestOptions {
            mirror_root: tmp.path().join("mirror"),
            owner_key: Some("hq-test-owner".to_string()),
        },
    )
    .expect("codex ingest");
    assert_eq!(report.rejected_candidates, 0, "{report:?}");
    ingest(
        &mut ledger,
        &[fixtures().join("claude/-work-app")],
        &tmp.path().join("mirror"),
    )
    .expect("claude ingest");
    db
}

/// Every served document an index answers, as the bytes a route sends.
fn served(ledger: &Ledger, index: &hq::Index, now: i64) -> Vec<(String, Vec<u8>)> {
    let view = index.view(now);
    let mut out = vec![
        (
            "project".to_string(),
            serde_json::to_vec(&view).expect("json"),
        ),
        (
            "listing".to_string(),
            serde_json::to_vec(&index.listing().at(now)).expect("json"),
        ),
        (
            "statements".to_string(),
            serde_json::to_vec(&index.statements()).expect("json"),
        ),
    ];
    for t in &view.threads {
        let tv = index
            .thread_view(ledger, &t.id, now)
            .expect("thread")
            .expect("present");
        out.push((
            format!("thread {}", t.id),
            serde_json::to_vec(&tv).expect("json"),
        ));
    }
    out
}

#[test]
fn every_way_of_reading_rows_serves_the_same_bytes() {
    let tmp = TempTree::new();
    let db = store(&tmp);
    let ledger = Ledger::open(&db).expect("open");
    let projects = hq::hq_projects(&ledger).expect("projects");
    assert_eq!(projects.len(), 1);
    let project = &projects[0];
    // Two instants: one where every thread's last row is long past, and one
    // inside the agent-active window of the newest rows (whose move moves).
    let instants = [
        hq::parse_ms("2026-10-07T00:00:00Z").expect("instant"),
        hq::parse_ms("2026-10-04T08:05:00Z").expect("instant"),
    ];

    let reference =
        hq::index_with(ledger.conn(), project, RowRead::OneStatement).expect("one statement");
    let view = reference.view(instants[0]);
    assert!(
        view.threads.len() >= 2 && view.project.sessions >= 3,
        "the fixtures must give the readers several sessions and threads to split: {} sessions, {} threads",
        view.project.sessions,
        view.threads.len()
    );

    // The precondition the rowid-order ways are tested on.
    let by_rowid: Vec<String> = {
        let mut stmt = ledger
            .conn()
            .prepare("SELECT session_id FROM events GROUP BY session_id ORDER BY MIN(rowid)")
            .expect("prepare");
        stmt.query_map([], |r| r.get(0))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("rows")
    };
    let mut by_id = by_rowid.clone();
    by_id.sort();
    assert_ne!(
        by_rowid, by_id,
        "rowid order must differ from session order"
    );

    let mut ways = Vec::new();
    for workers in [1, 2, 3, 4, 16] {
        for rowid_order in [true, false] {
            // One run per reader puts several sessions in one run; four
            // per reader leaves a session to a run on the fixtures.
            for runs_per_reader in [1, 4] {
                ways.push(RowRead::Parallel {
                    workers,
                    rowid_order,
                    runs_per_reader,
                });
            }
        }
    }
    for now in instants {
        let want = served(&ledger, &reference, now);
        for way in &ways {
            let got = hq::index_with(ledger.conn(), project, *way).expect("parallel");
            assert_eq!(
                got.row_ids(),
                reference.row_ids(),
                "{way:?}: the same rows in the same order"
            );
            let got = served(&ledger, &got, now);
            assert_eq!(
                got.iter().map(|d| &d.0).collect::<Vec<_>>(),
                want.iter().map(|d| &d.0).collect::<Vec<_>>(),
                "{way:?}: the same documents"
            );
            for ((name, a), (_, b)) in got.iter().zip(&want) {
                assert!(
                    a == b,
                    "{way:?}: {name} differs from the one-statement read\n got: {}\nwant: {}",
                    String::from_utf8_lossy(a),
                    String::from_utf8_lossy(b)
                );
            }
        }
        // The default the routes use.
        let routes = hq::index(&ledger, project).expect("index");
        assert_eq!(served(&ledger, &routes, now), want, "hq::index");
    }
}

/// A ledger with no file cannot be opened a second time, so a parallel read
/// of it is the one-statement read, not an error.
#[test]
fn a_ledger_in_memory_is_read_on_its_own_connection() {
    let ledger = Ledger::open_in_memory().expect("open");
    let project = hq::HqProject {
        id: hq::project_id_for_root("/nowhere"),
        name: "nowhere".to_string(),
        root: "/nowhere".to_string(),
        members: vec![hq::Member {
            ledger_project_id: "p".to_string(),
            source: "claude_code".to_string(),
            root: "/nowhere".to_string(),
            root_basis: "recorded_cwd",
        }],
    };
    let index = hq::index_with(
        ledger.conn(),
        &project,
        RowRead::Parallel {
            workers: 4,
            rowid_order: true,
            runs_per_reader: 4,
        },
    )
    .expect("index");
    assert_eq!(index.view(0).project.sessions, 0);
}
