//! M4.4 proof-scope v0 through the real ingest → annotate → derive → export path.

use std::fs;
use std::path::PathBuf;

use serde_json::{json, Value};
use thruline_core::annotate::annotate;
use thruline_core::config::Config;
use thruline_core::derive::{derive_project_with, fingerprint};
use thruline_core::export::export_project;
use thruline_core::ingest::ingest;
use thruline_core::nli::{EntailmentScorer, Scores};
use thruline_core::{Ledger, Result};

const PROJ: &str = "-Users-dev-proof-scope";
const SESSION: &str = "44444444-1111-4222-8333-444444444444";

struct NeverContradicts;

struct PersistedScope {
    session: String,
    seq: i64,
    cwd: Option<String>,
    branch: Option<String>,
    identity: String,
}

impl EntailmentScorer for NeverContradicts {
    fn score(&mut self, _premise: &str, _hypothesis: &str) -> Result<Scores> {
        Ok(Scores {
            entailment: 0.30,
            neutral: 0.70,
            contradiction: 0.0,
        })
    }
}

fn uuid(index: usize) -> String {
    format!("00000000-0000-4000-8000-{index:012}")
}

fn envelope(index: usize, parent: Option<usize>, message: Value) -> Value {
    json!({
        "type": message["role"].as_str().unwrap_or("user"),
        "uuid": uuid(index),
        "parentUuid": parent.map(uuid),
        "isSidechain": false,
        "sessionId": SESSION,
        "timestamp": format!("2026-07-31T02:00:{index:02}.000Z"),
        "cwd": "/repo",
        "gitBranch": "main",
        "message": message,
    })
}

fn transcript() -> String {
    let mut lines = vec![envelope(
        1,
        None,
        json!({"role":"user", "content":"run the suite"}),
    )];
    let runs = [("t1", 12, 0), ("t2", 11, 1), ("t3", 12, 0)];
    let mut parent = 1usize;
    let mut index = 2usize;
    for (tool_id, passed, failed) in runs {
        lines.push(envelope(
            index,
            Some(parent),
            json!({
                "role":"assistant",
                "content":[{"type":"tool_use", "id":tool_id, "name":"Bash", "input":{"command":"npm test"}}]
            }),
        ));
        parent = index;
        index += 1;
        lines.push(envelope(
            index,
            Some(parent),
            json!({
                "role":"user",
                "content":[{
                    "type":"tool_result",
                    "tool_use_id":tool_id,
                    "content":format!("# pass {passed}\n# fail {failed}")
                }]
            }),
        ));
        parent = index;
        index += 1;
    }
    lines.push(envelope(
        index,
        Some(parent),
        json!({"role":"assistant", "content":[{"type":"text", "text":"All 12 tests pass."}]}),
    ));
    lines
        .into_iter()
        .map(|line| serde_json::to_string(&line).expect("serialize line"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn setup() -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    let project = root.join(PROJ);
    fs::create_dir_all(&project).expect("project dir");
    fs::write(project.join(format!("{SESSION}.jsonl")), transcript()).expect("transcript");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("ledger");
    ingest(
        &mut ledger,
        std::slice::from_ref(&PathBuf::from(&root)),
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    (dir, ledger)
}

#[test]
fn scope_and_stale_history_survive_the_real_pipeline() {
    let (_dir, ledger) = setup();
    let cfg = Config::default();
    annotate(
        &ledger,
        PROJ,
        &cfg,
        &mut NeverContradicts,
        "proof-scope-stub-v1",
    )
    .expect("annotate");

    let persisted: Vec<PersistedScope> = {
        let mut stmt = ledger
            .conn()
            .prepare(
                "SELECT ev_session_id, ev_seq, ev_cwd, ev_git_branch, ev_identity
                 FROM evidence_links l JOIN claims c ON c.id = l.claim_id
                 JOIN events ce ON ce.id = c.event_id
                 WHERE ce.project_id = ?1 AND l.ev_family = 'test'
                 ORDER BY ev_seq",
            )
            .expect("scope query");
        stmt.query_map([PROJ], |row| {
            Ok(PersistedScope {
                session: row.get(0)?,
                seq: row.get(1)?,
                cwd: row.get(2)?,
                branch: row.get(3)?,
                identity: row.get(4)?,
            })
        })
        .expect("scope rows")
        .collect::<std::result::Result<_, _>>()
        .expect("scope values")
    };
    assert_eq!(persisted.len(), 3);
    assert!(persisted.iter().all(|row| row.session == SESSION));
    assert_eq!(
        persisted.iter().map(|row| row.seq).collect::<Vec<_>>(),
        vec![2, 4, 6]
    );
    assert!(persisted
        .iter()
        .all(|row| row.cwd.as_deref() == Some("/repo")));
    assert!(persisted
        .iter()
        .all(|row| row.branch.as_deref() == Some("main")));
    assert!(persisted
        .iter()
        .all(|row| row.identity.ends_with(":npm test")));

    let first = derive_project_with(&ledger, PROJ, &cfg).expect("derive");
    let second = derive_project_with(&ledger, PROJ, &cfg).expect("derive again");
    assert_eq!(fingerprint(&first), fingerprint(&second));
    let verdict = first
        .nodes
        .iter()
        .flat_map(|node| &node.verdicts)
        .find(|verdict| verdict.text.contains("12 tests"))
        .expect("claim verdict");
    assert_eq!(verdict.evidence_scopes.len(), 3);
    assert_eq!(verdict.evidence_scopes[0].freshness, "stale");
    assert_eq!(verdict.evidence_scopes[1].freshness, "stale");
    assert_eq!(verdict.evidence_scopes[2].freshness, "unknown");
    assert!(verdict.evidence_scopes[0]
        .freshness_note
        .as_deref()
        .is_some_and(|note| note.contains("verified at") && note.contains("superseded by")));
    assert!(
        !verdict.reason.contains("verified at"),
        "candidate freshness must not be attributed to the deciding proof"
    );

    let export = export_project(&ledger, &first, "2026-07-31T02:01:00Z").expect("export");
    let exported_scopes = export["receipts"]
        .as_object()
        .expect("receipts")
        .values()
        .flat_map(|receipt| receipt["claim"]["claims"].as_array().into_iter().flatten())
        .find_map(|claim| claim["evidence_scopes"].as_array())
        .expect("exported evidence scopes");
    assert_eq!(exported_scopes.len(), 3);
    assert_eq!(exported_scopes[0]["freshness"], "stale");
    assert!(exported_scopes[0]["freshness_note"]
        .as_str()
        .is_some_and(|note| note.contains("superseded by")));
}

#[test]
fn populated_pre_m44_links_migrate_without_touching_events() {
    let (dir, ledger) = setup();
    let cfg = Config::default();
    annotate(
        &ledger,
        PROJ,
        &cfg,
        &mut NeverContradicts,
        "legacy-scope-stub-v1",
    )
    .expect("annotate legacy shape");
    let events_before: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .expect("event count");
    let links_before: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM evidence_links", [], |row| row.get(0))
        .expect("link count");
    drop(ledger);

    let db = dir.path().join("ledger.db");
    let conn = rusqlite::Connection::open(&db).expect("raw legacy connection");
    conn.execute_batch(
        "PRAGMA foreign_keys=OFF;
         CREATE TABLE evidence_links_legacy(
           claim_id TEXT NOT NULL REFERENCES claims(id),
           evidence_event_id TEXT NOT NULL REFERENCES events(id),
           entail_score REAL, verdict TEXT NOT NULL, scorer TEXT NOT NULL,
           fidelity TEXT NOT NULL DEFAULT 'fallback',
           pass TEXT NOT NULL DEFAULT 'backward', contradiction REAL,
           related INTEGER NOT NULL DEFAULT 0,
           ev_family TEXT NOT NULL DEFAULT 'none', ev_passed INTEGER,
           ev_failed INTEGER, ev_ok INTEGER,
           match_strength REAL NOT NULL DEFAULT 0,
           ev_identity TEXT NOT NULL DEFAULT '',
           PRIMARY KEY(claim_id,evidence_event_id,scorer));
         INSERT INTO evidence_links_legacy
           SELECT claim_id,evidence_event_id,entail_score,verdict,scorer,
                  fidelity,pass,contradiction,related,ev_family,ev_passed,
                  ev_failed,ev_ok,match_strength,ev_identity
           FROM evidence_links;
         DROP TABLE evidence_links;
         ALTER TABLE evidence_links_legacy RENAME TO evidence_links;",
    )
    .expect("install legacy evidence table");
    // A pre-M4.4 ledger carries a pre-M4.4 schema fingerprint, so dropping the
    // current stamp is part of the fixture, not a way past the A82 fast path.
    conn.execute(
        "DELETE FROM schema_migrations WHERE version=?1",
        [Ledger::schema_fingerprint()],
    )
    .expect("legacy fingerprint");
    drop(conn);

    // A193: the migration door is `open_upgrading`; a plain open would refuse.
    let migrated =
        Ledger::open_upgrading(&db, &mut |_, _, _| {}).expect("migrate populated ledger");
    let events_after: i64 = migrated
        .conn()
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .expect("event count after");
    let links_after: i64 = migrated
        .conn()
        .query_row("SELECT COUNT(*) FROM evidence_links", [], |row| row.get(0))
        .expect("link count after");
    assert_eq!(
        events_after, events_before,
        "events changed during migration"
    );
    assert_eq!(
        links_after, links_before,
        "evidence rows were not preserved"
    );
    let backfilled: i64 = migrated
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM evidence_links
             WHERE ev_session_id IS NOT NULL AND ev_seq IS NOT NULL
               AND ev_cwd IS NULL AND ev_git_branch IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("scope backfill");
    assert_eq!(backfilled, links_before);
}

#[test]
fn rewritten_raw_lines_cannot_supply_scope_or_conviction_identity() {
    let (dir, ledger) = setup();
    let cfg = Config::default();
    let before = derive_project_with(&ledger, PROJ, &cfg).expect("derive with intact raw scope");
    assert!(before
        .nodes
        .iter()
        .flat_map(|node| &node.evidence)
        .any(|row| row.cwd.is_some() && row.git_branch.is_some()));
    let source = dir
        .path()
        .join("projects")
        .join(PROJ)
        .join(format!("{SESSION}.jsonl"));
    let rewritten = fs::read_to_string(&source)
        .expect("read source")
        .replace("/repo", "/rewritten-after-ingest");
    fs::write(&source, rewritten).expect("rewrite source");

    let after = derive_project_with(&ledger, PROJ, &cfg).expect("derive after raw rewrite");
    assert_ne!(
        fingerprint(&before),
        fingerprint(&after),
        "a changed exported scope must change the rebuild fingerprint"
    );
    assert!(after
        .nodes
        .iter()
        .flat_map(|node| &node.evidence)
        .all(|row| {
            row.cwd.is_none() && row.git_branch.is_none() && row.command_identity.is_none()
        }));

    annotate(
        &ledger,
        PROJ,
        &cfg,
        &mut NeverContradicts,
        "tamper-scope-stub-v1",
    )
    .expect("annotate safely");
    let known_scope: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM evidence_links l JOIN claims c ON c.id = l.claim_id
             JOIN events ce ON ce.id = c.event_id
             WHERE ce.project_id = ?1
               AND (l.ev_cwd IS NOT NULL OR l.ev_git_branch IS NOT NULL
                    OR l.ev_identity <> '')",
            [PROJ],
            |row| row.get(0),
        )
        .expect("known scope count");
    assert_eq!(known_scope, 0, "rewritten raw metadata gained authority");
}
