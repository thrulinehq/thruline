//! Ledger contract: §7 schema, append-only triggers, chain linkage.

use thruline_core::chain::{chain_hash, genesis_prev_hash};
use thruline_core::ledger::Ledger;
use thruline_core::{EventKind, NormalizedEvent, ProjectRef, RawLineRef, SessionRef, Source};

const PROJ: &str = "-Users-dev-thruline";
const SESS: &str = "11111111-2222-4333-8444-555555555555";

fn ledger_with_project() -> Ledger {
    let led = Ledger::open_in_memory().expect("in-memory ledger opens");
    led.upsert_project(
        &ProjectRef {
            id: PROJ.to_string(),
            root_path: "/Users/dev/thruline".to_string(),
            source: Source::ClaudeCode,
            display_name: Some("thruline".to_string()),
        },
        "2026-07-29T00:00:00Z",
    )
    .expect("project upserts");
    led.upsert_session(&SessionRef {
        id: SESS.to_string(),
        project_id: PROJ.to_string(),
        file_path: "/Users/dev/thruline/s.jsonl".to_string(),
        started_at: Some("2026-07-29T00:00:00Z".to_string()),
        raw_line_count: 0,
        raw_digest: None,
    })
    .expect("session upserts");
    led
}

fn item(line: u64, sha: &str, kind: EventKind) -> (RawLineRef, NormalizedEvent) {
    (
        RawLineRef {
            file: "/Users/dev/thruline/s.jsonl".to_string(),
            line,
            sha256: sha.to_string(),
        },
        NormalizedEvent::new(kind),
    )
}

#[test]
fn schema_applies_and_wal_is_on() {
    let led = Ledger::open_in_memory().expect("opens");
    let mode: String = led
        .conn()
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .expect("journal_mode readable");
    // An in-memory database reports "memory"; a file-backed one reports "wal".
    assert!(mode == "wal" || mode == "memory", "got {mode}");

    let tables: Vec<String> = {
        let mut stmt = led
            .conn()
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .expect("prepare");
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("rows");
        rows
    };
    for t in [
        "annotations",
        "claims",
        "edges",
        "events",
        "evidence_links",
        "ledger_meta",
        "nodes",
        "projects",
        "sessions",
    ] {
        assert!(tables.contains(&t.to_string()), "missing table {t}");
    }

    let node_columns: Vec<String> = {
        let mut stmt = led
            .conn()
            .prepare("PRAGMA table_info(nodes)")
            .expect("prepare");
        stmt.query_map([], |r| r.get::<_, String>(1))
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("columns")
    };
    for dimension in ["sort_key", "lifecycle", "evidence_state", "currency"] {
        assert!(
            node_columns.contains(&dimension.to_string()),
            "nodes is missing {dimension}: {node_columns:?}"
        );
    }
    assert!(
        !node_columns.contains(&"state".to_string()),
        "flat state must be demoted to sort_key: {node_columns:?}"
    );
}

#[test]
fn dimensional_node_domains_are_enforced_by_sqlite() {
    let mut led = ledger_with_project();
    let event = led
        .append_session_events(PROJ, SESS, &[item(1, &"a".repeat(64), EventKind::UserMsg)])
        .expect("append")
        .remove(0);

    let insert = |sort_key: &str, lifecycle: &str, evidence: &str, currency: &str| {
        led.conn().execute(
            "INSERT INTO nodes(id, project_id, label, sort_key, lifecycle, evidence_state,
                 currency, confidence, session_id, anchor_event_id, derived_rev)
             VALUES('n', ?1, 'node', ?2, ?3, ?4, ?5, NULL, ?6, ?7, 1)",
            rusqlite::params![PROJ, sort_key, lifecycle, evidence, currency, SESS, event.id],
        )
    };

    for bad in [
        insert("not-a-sort-key", "completed", "observed", "current"),
        insert("recorded", "not-a-lifecycle", "observed", "current"),
        insert("recorded", "completed", "not-evidence", "current"),
        insert("recorded", "completed", "observed", "not-currency"),
    ] {
        let err = bad.expect_err("each dimensional CHECK must reject unknown vocabulary");
        assert!(err.to_string().to_lowercase().contains("check"), "{err}");
    }
}

#[test]
fn event_insert_invalidation_is_atomic_and_project_scoped() {
    const OTHER_PROJECT: &str = "-Users-dev-other";
    const OTHER_SESSION: &str = "22222222-3333-4444-8555-666666666666";

    let mut led = ledger_with_project();
    led.upsert_project(
        &ProjectRef {
            id: OTHER_PROJECT.to_string(),
            root_path: "/Users/dev/other".to_string(),
            source: Source::ClaudeCode,
            display_name: Some("other".to_string()),
        },
        "2026-07-29T00:00:00Z",
    )
    .expect("other project");
    led.upsert_session(&SessionRef {
        id: OTHER_SESSION.to_string(),
        project_id: OTHER_PROJECT.to_string(),
        file_path: "/Users/dev/other/s.jsonl".to_string(),
        started_at: Some("2026-07-29T00:00:00Z".to_string()),
        raw_line_count: 0,
        raw_digest: None,
    })
    .expect("other session");

    let first = led
        .append_session_events(PROJ, SESS, &[item(1, &"a".repeat(64), EventKind::UserMsg)])
        .expect("first project event")
        .remove(0);
    let other = led
        .append_session_events(
            OTHER_PROJECT,
            OTHER_SESSION,
            &[item(1, &"b".repeat(64), EventKind::UserMsg)],
        )
        .expect("other project event")
        .remove(0);
    for (annotation, event) in [("manifest-main", &first.id), ("manifest-other", &other.id)] {
        led.conn()
            .execute(
                "INSERT INTO annotations(id,event_id,layer,model,prompt_sha256,output_json,
                     output_sha256,confidence,created_at)
                 VALUES(?1,?2,'classification','fixture','prompt','{}','output',NULL,
                        '2026-07-29T00:00:00Z')",
                rusqlite::params![annotation, event],
            )
            .expect("manifest receipt");
    }
    for (project, manifest, scorer) in [
        (PROJ, "manifest-main", "scorer-main"),
        (OTHER_PROJECT, "manifest-other", "scorer-other"),
    ] {
        led.conn()
            .execute(
                "INSERT INTO claim_projection_heads(project_id,extractor,manifest_annotation_id)
                 VALUES(?1,'fixture',?2)",
                rusqlite::params![project, manifest],
            )
            .expect("claim head");
        led.conn()
            .execute(
                "INSERT INTO entailment_projection_heads(project_id,scorer) VALUES(?1,?2)",
                rusqlite::params![project, scorer],
            )
            .expect("entailment head");
    }

    led.append_session_events(
        PROJ,
        SESS,
        &[item(2, &"c".repeat(64), EventKind::AssistantText)],
    )
    .expect("new accepted event");

    let main_heads: (Option<String>, String) = led
        .conn()
        .query_row(
            "SELECT c.manifest_annotation_id, e.scorer
               FROM claim_projection_heads c JOIN entailment_projection_heads e
                 ON e.project_id=c.project_id WHERE c.project_id=?1",
            [PROJ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("main heads");
    let other_heads: (Option<String>, String) = led
        .conn()
        .query_row(
            "SELECT c.manifest_annotation_id, e.scorer
               FROM claim_projection_heads c JOIN entailment_projection_heads e
                 ON e.project_id=c.project_id WHERE c.project_id=?1",
            [OTHER_PROJECT],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("other heads");
    assert_eq!(main_heads, (None, String::new()));
    assert_eq!(
        other_heads,
        (
            Some("manifest-other".to_string()),
            "scorer-other".to_string()
        ),
        "an event may invalidate only its own project's optional authority"
    );
    let annotations: i64 = led
        .conn()
        .query_row("SELECT COUNT(*) FROM annotations", [], |row| row.get(0))
        .expect("annotation count");
    assert_eq!(annotations, 2, "the trigger must not erase audit history");
}

#[test]
fn dimensional_migration_rebuilds_only_derived_tables() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("legacy-flat-state.db");
    let event_id: String;

    {
        let mut led = Ledger::open(&path).expect("create current ledger");
        led.upsert_project(
            &ProjectRef {
                id: PROJ.to_string(),
                root_path: "/Users/dev/thruline".to_string(),
                source: Source::ClaudeCode,
                display_name: Some("thruline".to_string()),
            },
            "2026-07-29T00:00:00Z",
        )
        .expect("project");
        led.upsert_session(&SessionRef {
            id: SESS.to_string(),
            project_id: PROJ.to_string(),
            file_path: "/Users/dev/thruline/s.jsonl".to_string(),
            started_at: Some("2026-07-29T00:00:00Z".to_string()),
            raw_line_count: 1,
            raw_digest: Some("digest".to_string()),
        })
        .expect("session");
        event_id = led
            .append_session_events(
                PROJ,
                SESS,
                &[item(1, &"b".repeat(64), EventKind::AssistantText)],
            )
            .expect("event")
            .remove(0)
            .id;
        led.conn()
            .execute(
                "INSERT INTO annotations(id, event_id, layer, model, prompt_sha256,
                     output_json, output_sha256, confidence, created_at)
                 VALUES('annotation-before-m4', ?1, 'classification', 'fixture',
                     'prompt-hash', '{\"kind\":\"finding\"}', 'output-hash', 0.9,
                     '2026-07-31T00:00:00Z')",
                [&event_id],
            )
            .expect("annotation");

        // Reproduce the M3 shape after placing immutable and annotation rows.
        // Opening this ledger again must discard only these derived rows.
        led.conn()
            .execute_batch(
                "DROP TABLE edges;
                 DROP TABLE nodes;
                 CREATE TABLE nodes(
                   id TEXT PRIMARY KEY, project_id TEXT NOT NULL, label TEXT NOT NULL,
                   state TEXT NOT NULL CHECK(state IN
                     ('recorded','supported','pending','unknown','unsupported','dropped',
                      'contradicted','refused')),
                   confidence REAL, session_id TEXT NOT NULL,
                   anchor_event_id TEXT NOT NULL REFERENCES events(id),
                   derived_rev INTEGER NOT NULL);
                 CREATE TABLE edges(
                   from_node TEXT NOT NULL, to_node TEXT NOT NULL,
                   kind TEXT NOT NULL CHECK(kind IN ('seq','inferred')),
                   derived_rev INTEGER NOT NULL, PRIMARY KEY(from_node,to_node,kind));",
            )
            .expect("legacy derived shape");
        // A ledger left by an M3-era binary carries that binary's schema
        // fingerprint, never this one — dropping the stamp is part of what
        // "written before M4.1" means, not a way around the fast path.
        led.conn()
            .execute(
                "DELETE FROM schema_migrations WHERE version=?1",
                [Ledger::schema_fingerprint()],
            )
            .expect("legacy fingerprint");
        led.conn()
            .execute(
                "INSERT INTO nodes(id, project_id, label, state, confidence, session_id,
                     anchor_event_id, derived_rev)
                 VALUES('legacy-node', ?1, 'legacy', 'recorded', NULL, ?2, ?3, 7)",
                rusqlite::params![PROJ, SESS, event_id],
            )
            .expect("legacy node");
    }

    // A193: migrations happen at the upgrade door, never on an ordinary open.
    let led = Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("M4.1 migration");
    assert_eq!(
        led.conn()
            .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get::<_, i64>(0))
            .expect("nodes"),
        0,
        "derived flat-state rows must be rebuilt, never reinterpreted"
    );
    assert_eq!(
        led.conn()
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))
            .expect("events"),
        1,
        "immutable events survived the derived migration"
    );
    let annotation: (String, String, String) = led
        .conn()
        .query_row(
            "SELECT event_id, output_json, output_sha256 FROM annotations
             WHERE id = 'annotation-before-m4'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("annotation survived");
    assert_eq!(
        annotation,
        (
            event_id,
            "{\"kind\":\"finding\"}".to_string(),
            "output-hash".to_string()
        )
    );
}

#[test]
fn current_dimensional_rows_survive_a_normal_reopen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("current-dimensional.db");

    {
        let mut led = Ledger::open(&path).expect("create ledger");
        led.upsert_project(
            &ProjectRef {
                id: PROJ.to_string(),
                root_path: "/Users/dev/thruline".to_string(),
                source: Source::ClaudeCode,
                display_name: None,
            },
            "2026-07-29T00:00:00Z",
        )
        .expect("project");
        led.upsert_session(&SessionRef {
            id: SESS.to_string(),
            project_id: PROJ.to_string(),
            file_path: "/Users/dev/thruline/s.jsonl".to_string(),
            started_at: None,
            raw_line_count: 1,
            raw_digest: None,
        })
        .expect("session");
        let event = led
            .append_session_events(
                PROJ,
                SESS,
                &[item(1, &"c".repeat(64), EventKind::AssistantText)],
            )
            .expect("event")
            .remove(0);
        led.conn()
            .execute(
                "INSERT INTO nodes(id, project_id, label, sort_key, lifecycle, evidence_state,
                     currency, confidence, session_id, anchor_event_id, derived_rev)
                 VALUES('dimensional-node', ?1, 'kept', 'supported', 'completed', 'supported',
                     'current', 0.99, ?2, ?3, 1)",
                rusqlite::params![PROJ, SESS, event.id],
            )
            .expect("dimensional node");
    }

    let led = Ledger::open(&path).expect("reopen current shape");
    let dimensions: (String, String, String, String) = led
        .conn()
        .query_row(
            "SELECT sort_key, lifecycle, evidence_state, currency FROM nodes
             WHERE id = 'dimensional-node'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .expect("current derived row was not mistaken for a stale schema");
    assert_eq!(
        dimensions,
        (
            "supported".to_string(),
            "completed".to_string(),
            "supported".to_string(),
            "current".to_string()
        )
    );
}

#[test]
fn events_are_append_only() {
    let mut led = ledger_with_project();
    led.append_session_events(PROJ, SESS, &[item(1, &"a".repeat(64), EventKind::UserMsg)])
        .expect("append");

    let upd = led
        .conn()
        .execute("UPDATE events SET text = 'tampered' WHERE seq = 1", []);
    let err = upd.expect_err("UPDATE must abort");
    assert!(
        err.to_string().contains("append-only"),
        "trigger message missing: {err}"
    );

    let del = led.conn().execute("DELETE FROM events WHERE seq = 1", []);
    let err = del.expect_err("DELETE must abort");
    assert!(
        err.to_string().contains("append-only"),
        "trigger message missing: {err}"
    );

    let n: i64 = led
        .conn()
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .expect("count");
    assert_eq!(n, 1, "the row survived both attempts");
}

#[test]
fn chain_links_each_event_to_the_previous() {
    let mut led = ledger_with_project();
    let shas = ["a".repeat(64), "b".repeat(64), "c".repeat(64)];
    let items: Vec<_> = shas
        .iter()
        .enumerate()
        .map(|(i, s)| item(i as u64 + 1, s, EventKind::UserMsg))
        .collect();
    let placed = led
        .append_session_events(PROJ, SESS, &items)
        .expect("append");

    assert_eq!(placed.len(), 3);
    assert_eq!(placed[0].prev_hash, genesis_prev_hash(PROJ), "genesis link");
    for (i, ev) in placed.iter().enumerate() {
        assert_eq!(ev.seq, i as i64 + 1, "seq is per-session monotonic");
        let expect = chain_hash(&ev.prev_hash, &ev.raw.sha256, SESS, ev.seq);
        assert_eq!(ev.chain_hash, expect, "link {i} recomputes");
        if i > 0 {
            assert_eq!(
                ev.prev_hash,
                placed[i - 1].chain_hash,
                "link {i} points at its predecessor"
            );
        }
    }
}

#[test]
fn appending_later_continues_the_same_chain() {
    let mut led = ledger_with_project();
    let first = led
        .append_session_events(PROJ, SESS, &[item(1, &"a".repeat(64), EventKind::UserMsg)])
        .expect("first append");
    let second = led
        .append_session_events(PROJ, SESS, &[item(2, &"b".repeat(64), EventKind::UserMsg)])
        .expect("second append");

    assert_eq!(second[0].seq, 2, "seq continues across calls");
    assert_eq!(
        second[0].prev_hash, first[0].chain_hash,
        "the chain is not restarted"
    );

    let state = led.session_state(SESS).expect("state");
    assert_eq!(state.last_seq, 2);
    assert_eq!(state.max_raw_line, 2);
    assert_eq!(state.event_count, 2);
    assert_eq!(
        state.head_hash.as_deref(),
        Some(second[0].chain_hash.as_str())
    );
}

#[test]
fn fan_out_shares_one_line_hash_but_takes_distinct_seqs() {
    let mut led = ledger_with_project();
    let sha = "d".repeat(64);
    let items = vec![
        item(7, &sha, EventKind::AssistantThinking),
        item(7, &sha, EventKind::AssistantText),
        item(7, &sha, EventKind::ToolUse),
    ];
    let placed = led
        .append_session_events(PROJ, SESS, &items)
        .expect("append");

    assert!(placed.iter().all(|e| e.raw.line == 7));
    assert!(placed.iter().all(|e| e.raw.sha256 == sha));
    assert_eq!(
        placed.iter().map(|e| e.seq).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "each event still gets its own link"
    );
    // Same raw hash three times must still produce three distinct links,
    // because seq participates in the digest.
    let links: std::collections::BTreeSet<_> =
        placed.iter().map(|e| e.chain_hash.clone()).collect();
    assert_eq!(links.len(), 3);
}

#[test]
fn ingested_line_hashes_expose_what_is_already_stored() {
    let mut led = ledger_with_project();
    let sha1 = "a".repeat(64);
    let sha2 = "b".repeat(64);
    led.append_session_events(
        PROJ,
        SESS,
        &[
            item(1, &sha1, EventKind::UserMsg),
            item(2, &sha2, EventKind::AssistantText),
            item(2, &sha2, EventKind::ToolUse),
        ],
    )
    .expect("append");

    let map = led.ingested_line_hashes(SESS).expect("hashes");
    assert_eq!(map.len(), 2, "deduplicated per raw line");
    assert_eq!(map.get(&1), Some(&sha1));
    assert_eq!(map.get(&2), Some(&sha2));
}

#[test]
fn project_head_is_deterministic_and_moves_only_on_append() {
    let mut led = ledger_with_project();
    let (empty_head, empty_n) = led.compute_project_head(PROJ).expect("head");
    assert_eq!(empty_n, 0);
    assert_eq!(
        empty_head,
        genesis_prev_hash(PROJ),
        "empty project = genesis"
    );

    led.append_session_events(PROJ, SESS, &[item(1, &"a".repeat(64), EventKind::UserMsg)])
        .expect("append");
    let (h1, n1) = led.compute_project_head(PROJ).expect("head");
    let (h1_again, _) = led.compute_project_head(PROJ).expect("head again");
    assert_eq!(h1, h1_again, "recomputing must not change the head");
    assert_eq!(n1, 1);

    led.append_session_events(PROJ, SESS, &[item(2, &"b".repeat(64), EventKind::UserMsg)])
        .expect("append");
    let (h2, n2) = led.compute_project_head(PROJ).expect("head");
    assert_ne!(h1, h2, "the head must move when the ledger grows");
    assert_eq!(n2, 2);

    let stored = led.update_ledger_meta(PROJ, None).expect("meta");
    assert_eq!(stored, h2);
    let (row_head, row_count): (String, i64) = led
        .conn()
        .query_row(
            "SELECT head_hash, event_count FROM ledger_meta WHERE project_id = ?1",
            [PROJ],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("meta row");
    assert_eq!(row_head, h2);
    assert_eq!(row_count, 2);
}

#[test]
fn kind_outside_the_ddl_domain_is_rejected_by_sqlite() {
    let led = ledger_with_project();
    let err = led
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, kind, is_error, is_sidechain,
                 raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('x', ?1, ?2, 1, 'not_a_kind', 0, 0, 'f', 1, 'h', 'p', 'c')",
            [PROJ, SESS],
        )
        .expect_err("CHECK constraint must reject an unknown kind");
    assert!(err.to_string().to_lowercase().contains("check"), "{err}");
}

#[test]
fn a_file_backed_ledger_really_is_in_wal_mode() {
    // The in-memory case can only report "memory", so WAL has to be proven
    // against a real file — §7 opens with `PRAGMA journal_mode=WAL`.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("nested").join("ledger.db");
    let led = Ledger::open(&path).expect("opens and creates the directory");
    let mode: String = led
        .conn()
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .expect("journal_mode readable");
    assert_eq!(mode, "wal");
    assert!(path.exists(), "the db file was created");
}

// ---------------------------------------------------------------------------
// A82 — the schema fast path.
// ---------------------------------------------------------------------------

fn file_sha256(path: &std::path::Path) -> String {
    thruline_core::chain::sha256_hex(&std::fs::read(path).expect("read db file"))
}

#[test]
fn a_matching_schema_skips_the_open_work_and_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("stamped.db");

    {
        let led = Ledger::open(&path).expect("create");
        assert!(
            !led.took_schema_fast_path(),
            "a database that did not exist cannot already carry this schema"
        );
        let stamped: i64 = led
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM schema_migrations WHERE version=?1",
                [Ledger::schema_fingerprint()],
                |r| r.get(0),
            )
            .expect("fingerprint");
        assert_eq!(stamped, 1, "the full path stamps its fingerprint");
    }

    // Closed cleanly, so the WAL is checkpointed away and the file on disk is
    // the whole database. Any write during the next open would change it.
    let before = file_sha256(&path);
    {
        let led = Ledger::open(&path).expect("reopen");
        assert!(
            led.took_schema_fast_path(),
            "the same binary reopening its own ledger must skip schema work"
        );
    }
    assert_eq!(
        before,
        file_sha256(&path),
        "the fast path opened read-only: not one byte of the ledger changed"
    );
}

#[test]
fn a_mismatched_schema_still_runs_the_whole_migration_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("mismatched.db");

    {
        let led = Ledger::open(&path).expect("create");
        // What a ledger written by any other schema revision looks like: some
        // other fingerprint, never this one.
        led.conn()
            .execute(
                "DELETE FROM schema_migrations WHERE version=?1",
                [Ledger::schema_fingerprint()],
            )
            .expect("unstamp");
        led.conn()
            .execute(
                "INSERT OR IGNORE INTO schema_migrations(version)
                 VALUES('schema_sha256_from_some_older_binary')",
                [],
            )
            .expect("older stamp");
    }

    // A193: a populated ledger whose fingerprint does not match is REFUSED by
    // `open` and migrated only by the upgrade door. Both halves are asserted
    // here, because "the full path still runs" is only half the contract now —
    // the other half is that it does not run by accident.
    assert!(
        Ledger::open(&path).is_err(),
        "a mismatched schema on a populated ledger must be refused by open"
    );
    let led = Ledger::open_upgrading(&path, &mut |_, _, _| {}).expect("reopen");
    assert!(
        !led.took_schema_fast_path(),
        "an unrecognised schema fingerprint must run the full path"
    );
    let stamped: i64 = led
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM schema_migrations WHERE version=?1",
            [Ledger::schema_fingerprint()],
            |r| r.get(0),
        )
        .expect("fingerprint");
    assert_eq!(stamped, 1, "the completed migration re-stamps");
}

#[test]
fn the_fingerprint_is_the_ddl_and_nothing_else() {
    // The mechanism's one load-bearing property: it cannot silently agree
    // across two different schemas. If a later session edits the DDL without
    // touching anything else, existing ledgers must still migrate.
    let fingerprint = Ledger::schema_fingerprint();
    assert!(
        fingerprint.starts_with("schema_sha256_") && fingerprint.len() == 14 + 64,
        "unexpected fingerprint shape: {fingerprint}"
    );
    assert_eq!(
        fingerprint,
        Ledger::schema_fingerprint(),
        "stable per build"
    );
}
