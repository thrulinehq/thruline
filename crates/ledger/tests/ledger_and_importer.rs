//! Integration tests for the §11 ledger and its legacy importer.

use rusqlite::{params, Connection};
use thruline_core::spec::chain::{append, genesis_prev_hash, verify_chain};
use thruline_core::spec::event::{Event, OpenPayload, Payload, Source};
use thruline_ledger::importer::{DEGRADED_REASON, IMPORT_SOURCE};
use thruline_ledger::{EventRow, ImportOptions, Importer, Ledger, LedgerError, SCHEMA_VERSION};

fn chain_of(session: &str, n: u64) -> Vec<Event> {
    let mut chain = Vec::new();
    for seq in 1..=n {
        let mut payload = OpenPayload::new();
        payload.insert("i".into(), serde_json::json!(seq));
        let event = Event::new(
            session,
            seq,
            "2026-09-02T10:11:12.345Z",
            Source::ExecRecorder,
            Payload::Note(payload),
            "placeholder",
        )
        .expect("constructible");
        append(&mut chain, event).expect("appends");
    }
    chain
}

fn seeded(ledger: &Ledger, session: &str, n: u64) -> Vec<Event> {
    let repo = ledger
        .repo("git@example.invalid:acme/app.git", None)
        .expect("repo");
    ledger
        .upsert_session(session, &repo, "claude-code", None, false, None)
        .expect("session");
    let chain = chain_of(session, n);
    let rows: Vec<EventRow> = chain.iter().cloned().map(EventRow::bare).collect();
    let written = ledger.append_events(&repo, &rows).expect("append");
    assert_eq!(written, n);
    chain
}

#[test]
fn a_fresh_ledger_is_at_the_current_schema_version_and_is_in_wal_mode() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("exec.db");
    let ledger = Ledger::open(&path).expect("open");
    assert_eq!(ledger.schema_version().expect("version"), SCHEMA_VERSION);

    let mode: String = ledger
        .conn()
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .expect("journal_mode");
    assert_eq!(mode.to_lowercase(), "wal", "§11 requires WAL");

    // Every §11 table exists, including the ones that are still empty. A schema
    // that creates tables lazily makes "is the ledger initialised?" unanswerable.
    let mut stmt = ledger
        .conn()
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .expect("prepare");
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    for want in [
        "checks",
        "claims",
        "events",
        "findings",
        "imports",
        "promises",
        "receipts",
        "repos",
        "sessions",
        "snapshots",
        "verdicts",
    ] {
        assert!(
            tables.contains(&want.to_string()),
            "missing table {want}: {tables:?}"
        );
    }
}

#[test]
fn reopening_migrates_nothing_and_a_future_schema_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("exec.db");
    {
        let ledger = Ledger::open(&path).expect("open");
        seeded(&ledger, "s1", 3);
    }
    {
        let ledger = Ledger::open(&path).expect("reopen");
        assert_eq!(ledger.schema_version().expect("version"), SCHEMA_VERSION);
        assert_eq!(ledger.session_events("s1").expect("read").len(), 3);
    }
    // A ledger written by a newer build must be refused, not half-read. An older
    // binary interpreting a newer schema is how a store gets partially read and
    // confidently reported as complete.
    {
        let conn = Connection::open(&path).expect("raw open");
        conn.execute_batch(&format!("PRAGMA user_version = {}", SCHEMA_VERSION + 5))
            .expect("bump");
    }
    match Ledger::open(&path) {
        Err(LedgerError::FromTheFuture { found, supported }) => {
            assert_eq!(found, SCHEMA_VERSION + 5);
            assert_eq!(supported, SCHEMA_VERSION);
        }
        other => panic!("expected a refusal, got {other:?}", other = other.err()),
    }
}

#[test]
fn events_are_append_only_at_the_database_level() {
    let ledger = Ledger::open_in_memory().expect("open");
    seeded(&ledger, "s1", 2);

    let update = ledger.conn().execute(
        "UPDATE events SET kind = 'exec' WHERE session_id = 's1'",
        [],
    );
    assert!(update.is_err(), "an UPDATE on events must raise");
    assert!(format!("{update:?}").contains("append-only"));

    let delete = ledger
        .conn()
        .execute("DELETE FROM events WHERE session_id = 's1'", []);
    assert!(delete.is_err(), "a DELETE on events must raise");
    assert!(format!("{delete:?}").contains("append-only"));

    // And nothing actually changed.
    assert_eq!(ledger.session_events("s1").expect("read").len(), 2);
}

#[test]
fn ten_thousand_events_append_in_one_batch_and_the_read_back_chain_verifies() {
    // BUILD-PLAN M0.4 done-when (b). The point is not the number: it is that the
    // head recomputed from the ROWS matches the head computed in memory, so the
    // round trip through SQLite — JSON payload, text columns, ordering — has not
    // altered a single byte of any preimage.
    let ledger = Ledger::open_in_memory().expect("open");
    let in_memory = seeded(&ledger, "big", 10_000);
    let expected = verify_chain(&in_memory).expect("in-memory chain verifies");

    let read_back = ledger.session_events("big").expect("read");
    assert_eq!(read_back.len(), 10_000);
    assert_eq!(read_back, in_memory, "a row did not round-trip");

    let recomputed = ledger.verify_session("big").expect("stored chain verifies");
    assert_eq!(recomputed, expected);

    // The cached head on `sessions` must agree with the recomputation. It is
    // maintained at append, so it is exactly the kind of derived value that
    // drifts silently.
    assert_eq!(
        ledger.stored_head("big").expect("head"),
        Some(expected.head_hash)
    );
}

#[test]
fn appending_the_same_events_twice_is_idempotent() {
    let ledger = Ledger::open_in_memory().expect("open");
    let chain = seeded(&ledger, "s1", 5);
    let repo = ledger
        .repo("git@example.invalid:acme/app.git", None)
        .expect("repo");
    let rows: Vec<EventRow> = chain.iter().cloned().map(EventRow::bare).collect();
    let again = ledger.append_events(&repo, &rows).expect("re-append");
    assert_eq!(again, 0, "a replayed batch must write nothing");
    assert_eq!(ledger.session_events("s1").expect("read").len(), 5);
    ledger.verify_session("s1").expect("still verifies");
}

#[test]
fn an_unsealed_event_is_refused_before_anything_is_written() {
    let ledger = Ledger::open_in_memory().expect("open");
    let repo = ledger.repo("k", None).expect("repo");
    ledger
        .upsert_session("s1", &repo, "claude-code", None, false, None)
        .expect("session");

    let mut chain = chain_of("s1", 3);
    // `into_wire` drops the hash, so this is simply the unsealed form.
    chain[1] = Event::try_from(chain[1].clone().into_wire()).expect("valid wire");
    let rows: Vec<EventRow> = chain.into_iter().map(EventRow::bare).collect();
    assert!(matches!(
        ledger.append_events(&repo, &rows),
        Err(LedgerError::UnsealedEvent { .. })
    ));
    // Nothing partial: the refusal happens before the transaction opens, so the
    // first event is not left behind for the next batch to chain onto.
    assert_eq!(ledger.session_events("s1").expect("read").len(), 0);
}

#[test]
fn a_second_connection_under_wal_does_not_interleave_seq_within_a_session() {
    // Two hooks landing back to back is the real case (Stop, then SessionEnd).
    // Two connections to the same file, not two handles in one — the UNIQUE
    // constraint has to be what serialises them, not a mutex this process holds.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("exec.db");
    let first = Ledger::open(&path).expect("open");
    let second = Ledger::open(&path).expect("open again");

    let repo = first.repo("k", None).expect("repo");
    first
        .upsert_session("race", &repo, "claude-code", None, false, None)
        .expect("session");

    let chain = chain_of("race", 4);
    let rows: Vec<EventRow> = chain.iter().cloned().map(EventRow::bare).collect();
    assert_eq!(first.append_events(&repo, &rows).expect("first"), 4);

    // The second connection replays the same seq numbers with different content.
    let mut colliding = Vec::new();
    for seq in 1..=4u64 {
        let mut payload = OpenPayload::new();
        payload.insert("other".into(), serde_json::json!(seq));
        let event = Event::new(
            "race",
            seq,
            "2026-09-02T11:00:00Z",
            Source::FsWatcher,
            Payload::Note(payload),
            genesis_prev_hash("race"),
        )
        .expect("constructible")
        .seal()
        .expect("seals");
        colliding.push(EventRow::bare(event));
    }
    let written = second.append_events(&repo, &colliding).expect("second");
    assert_eq!(written, 0, "a colliding seq must not create a second row");

    // One row per seq, and the chain still verifies from either connection.
    let events = second.session_events("race").expect("read");
    assert_eq!(events.len(), 4);
    let seqs: Vec<u64> = events.iter().map(|e| e.seq()).collect();
    assert_eq!(seqs, vec![1, 2, 3, 4]);
    first.verify_session("race").expect("verifies");
    second.verify_session("race").expect("verifies");
}

// ---------------------------------------------------------------------------
// The legacy importer
// ---------------------------------------------------------------------------

/// A synthetic legacy ledger with the real schema shape, so the importer is
/// tested against the columns it will actually meet.
fn legacy_fixture(path: &std::path::Path, sessions: u64, per_session: u64) -> Connection {
    let conn = Connection::open(path).expect("open legacy");
    // WAL, because the real ledger is in WAL and read-only access to a WAL
    // database behaves differently from read-only access to a rollback-journal
    // one. The first version of this fixture used the default journal, so
    // `the_import_reads_the_source_without_modifying_it` was testing an easier
    // case than the one it was written about — and the claim it certified ("no
    // WAL file is created beside the source") was false against the real ledger.
    conn.pragma_update(None, "journal_mode", "WAL")
        .expect("wal");
    conn.execute_batch(
        r#"
CREATE TABLE projects(
  id TEXT PRIMARY KEY, root_path TEXT NOT NULL,
  source TEXT NOT NULL CHECK(source IN ('claude_code','codex')),
  display_name TEXT, created_at TEXT NOT NULL);
CREATE TABLE sessions(
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id),
  file_path TEXT NOT NULL, started_at TEXT, last_seq INTEGER NOT NULL DEFAULT 0,
  raw_line_count INTEGER NOT NULL DEFAULT 0, raw_digest TEXT,
  head_chain_hash TEXT, event_count INTEGER NOT NULL DEFAULT 0);
CREATE TABLE events(
  id TEXT PRIMARY KEY, project_id TEXT NOT NULL, session_id TEXT NOT NULL,
  seq INTEGER NOT NULL, ts TEXT,
  kind TEXT NOT NULL CHECK(kind IN
    ('user_msg','assistant_text','assistant_thinking','tool_use','tool_result','summary','meta')),
  role TEXT, text TEXT,
  tool_name TEXT, source_tool_name TEXT, tool_use_id TEXT, tool_input TEXT, tool_result TEXT,
  is_error INTEGER NOT NULL DEFAULT 0,
  src_uuid TEXT, parent_uuid TEXT, is_sidechain INTEGER NOT NULL DEFAULT 0,
  source_turn_id TEXT, source_turn_boundary_line INTEGER,
  source_turn_boundary_sha256 TEXT, source_message_key TEXT, line_session_id TEXT,
  raw_file TEXT NOT NULL, raw_line INTEGER NOT NULL, raw_sha256 TEXT NOT NULL,
  prev_hash TEXT NOT NULL, chain_hash TEXT NOT NULL,
  UNIQUE(session_id, seq));
"#,
    )
    .expect("legacy ddl");

    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES('proj-a', '/w/app', 'claude_code', 'app', '2026-01-01')",
        [],
    )
    .expect("project");

    // The real corpus is ~70% `meta`; mirror that so counts are meaningful.
    let kinds = [
        "meta",
        "meta",
        "meta",
        "meta",
        "meta",
        "meta",
        "meta",
        "assistant_text",
        "tool_use",
        "tool_result",
    ];
    for s in 0..sessions {
        let session = format!("sess-{s}");
        conn.execute(
            "INSERT INTO sessions(id, project_id, file_path) VALUES(?1,'proj-a',?2)",
            params![session, format!("/gone/{session}.jsonl")],
        )
        .expect("session");
        for seq in 1..=per_session {
            let kind = kinds[(seq as usize) % kinds.len()];
            conn.execute(
                "INSERT INTO events(id, project_id, session_id, seq, ts, kind, text,
                    tool_name, tool_use_id, tool_input, tool_result, is_error,
                    raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
                 VALUES(?1,'proj-a',?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                params![
                    format!("{s:04}{seq:022}"),
                    session,
                    seq as i64,
                    // Deliberately the offset spelling: the importer has to
                    // normalise it, because §3.1 accepts only `Z`.
                    "2026-09-02T10:11:12+00:00",
                    kind,
                    format!("line {seq}"),
                    "Bash",
                    format!("toolu_{seq}"),
                    "{\"command\":\"cargo test\"}",
                    "ok",
                    0i64,
                    format!("/gone/{session}.jsonl"),
                    seq as i64,
                    format!("{:064x}", seq),
                    "0".repeat(64),
                    format!("{:064x}", seq * 7),
                ],
            )
            .expect("event");
        }
    }
    conn
}

#[test]
fn a_legacy_slice_imports_and_the_resulting_chain_verifies() {
    let dir = tempfile::tempdir().expect("tempdir");
    let legacy = dir.path().join("ledger.db");
    let source = legacy_fixture(&legacy, 5, 200);
    drop(source);

    let ledger = Ledger::open_in_memory().expect("open");
    let report = Importer::new(&ledger)
        .run(&ImportOptions::new(&legacy))
        .expect("import");

    assert_eq!(report.rows_read, 1_000);
    // 700 of the 1,000 fixture rows are `meta`. They are not session content —
    // each one names a JSONL field the parser did not recognise — so they fold
    // into one census note per session instead of 700 events. 300 real events
    // plus 5 censuses.
    assert_eq!(report.kinds.get("meta").copied().unwrap_or(0), 700);
    assert_eq!(report.meta_rows_aggregated, 700);
    assert_eq!(report.events_written, 305);
    assert_eq!(report.sessions_seen, 5);
    assert_eq!(
        report.sessions_with_dead_source, 5,
        "the fixture's raw_file paths do not exist, as 99.4% of the real ones do not"
    );
    assert!(
        report.rejected.is_empty(),
        "rejected: {:?}",
        report.rejected
    );

    for s in 0..5 {
        let session = format!("sess-{s}");
        let head = ledger.verify_session(&session).expect("verifies");
        assert_eq!(head.event_count, 61, "60 events + 1 census note");
        assert!(ledger.source_digest_unverifiable(&session).expect("flag"));
    }

    // The census is a real census, not an empty placeholder.
    let census: String = ledger
        .conn()
        .query_row(
            "SELECT payload FROM events WHERE session_id='sess-0'
             AND payload LIKE '%legacy_unknown_field_census%'",
            [],
            |row| row.get(0),
        )
        .expect("census note");
    let census: serde_json::Value = serde_json::from_str(&census).expect("json");
    assert_eq!(census["rows"], serde_json::json!(140));
    let fields = census["legacy_unknown_field_census"]
        .as_object()
        .expect("field map");
    assert!(!fields.is_empty(), "the census recorded no field names");
    assert_eq!(
        fields
            .values()
            .filter_map(serde_json::Value::as_u64)
            .sum::<u64>(),
        140
    );
}

#[test]
fn meta_rows_can_still_be_imported_as_events_when_asked() {
    // The exclusion is a default, not a capability that was removed. Somebody
    // auditing parser coverage row by row must still be able to get the rows.
    let dir = tempfile::tempdir().expect("tempdir");
    let legacy = dir.path().join("ledger.db");
    drop(legacy_fixture(&legacy, 1, 100));

    let ledger = Ledger::open_in_memory().expect("open");
    let mut options = ImportOptions::new(&legacy);
    options.include_meta_events = true;
    let report = Importer::new(&ledger).run(&options).expect("import");

    assert_eq!(report.meta_rows_aggregated, 0);
    assert_eq!(report.events_written, 100, "every row became an event");
    let head = ledger.verify_session("sess-0").expect("verifies");
    assert_eq!(head.event_count, 100);
    // No census note, because nothing was aggregated.
    assert!(!ledger.has_census_note("sess-0").expect("query"));
}

#[test]
fn an_imported_session_is_stamped_degraded_and_that_is_what_blocks_supported() {
    // BUILD-PLAN M0.11 done-when (d): assert the invariant, not the plumbing.
    // §13 makes degraded capture produce `unverifiable`, never `supported`;
    // this asserts the flag that carries it, and that the flag is reachable
    // from a claim by the join a verifier would use.
    let dir = tempfile::tempdir().expect("tempdir");
    let legacy = dir.path().join("ledger.db");
    drop(legacy_fixture(&legacy, 1, 10));

    let ledger = Ledger::open_in_memory().expect("open");
    Importer::new(&ledger)
        .run(&ImportOptions::new(&legacy))
        .expect("import");

    assert!(
        ledger.capture_degraded("sess-0").expect("flag"),
        "an imported session must be marked capture_degraded"
    );
    let reason: String = ledger
        .conn()
        .query_row(
            "SELECT degraded_reason FROM sessions WHERE id = 'sess-0'",
            [],
            |row| row.get(0),
        )
        .expect("reason");
    assert_eq!(reason, DEGRADED_REASON);

    // And the source is visibly not hook or exec-recorder capture, so nothing
    // downstream can mistake a transcript row for execution evidence.
    let sources: Vec<String> = {
        let mut stmt = ledger
            .conn()
            .prepare("SELECT DISTINCT source FROM events WHERE session_id='sess-0'")
            .expect("prepare");
        stmt.query_map([], |row| row.get::<_, String>(0))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("rows")
    };
    assert_eq!(sources, vec![IMPORT_SOURCE.to_string()]);

    // No event carries an execution kind. This is the substantive claim: two
    // million transcript rows contain no exec, fs or snapshot evidence, and the
    // importer must not manufacture any.
    let exec_like: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE kind IN
             ('exec','fs_change','snapshot','compaction_pre','compaction_post','stop')",
            [],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(exec_like, 0, "the importer invented execution evidence");
}

#[test]
fn a_dry_run_reports_counts_and_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let legacy = dir.path().join("ledger.db");
    drop(legacy_fixture(&legacy, 2, 50));

    let ledger = Ledger::open_in_memory().expect("open");
    let mut options = ImportOptions::new(&legacy);
    options.dry_run = true;
    let report = Importer::new(&ledger).run(&options).expect("dry run");

    assert!(report.dry_run);
    assert_eq!(report.rows_read, 100);
    assert_eq!(report.events_written, 0);

    let events: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
        .expect("count");
    assert_eq!(events, 0, "a dry run wrote to the ledger");
    let imports: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM imports", [], |row| row.get(0))
        .expect("count");
    assert_eq!(imports, 0, "a dry run recorded an import");
}

#[test]
fn an_interrupted_import_resumes_without_duplicating_and_continues_the_chain() {
    // BUILD-PLAN M0.11 done-when (c). The chain part is the hard half: a resumed
    // import must link onto the head already stored, not start a second chain
    // from genesis.
    let dir = tempfile::tempdir().expect("tempdir");
    let legacy = dir.path().join("ledger.db");
    drop(legacy_fixture(&legacy, 1, 300));

    let ledger = Ledger::open_in_memory().expect("open");

    let mut partial = ImportOptions::new(&legacy);
    partial.limit = Some(100);
    partial.page_size = 50;
    let first = Importer::new(&ledger)
        .run(&partial)
        .expect("partial import");
    // 100 legacy rows read; 30 of them are non-meta. No census note: the run was
    // truncated mid-session, so its census is incomplete and writing it would
    // both publish a partial count and wedge a synthetic row between two legacy
    // seqs — which is exactly how the resume cursor gets corrupted.
    assert_eq!(first.events_written, 30);
    assert!(!ledger.has_census_note("sess-0").expect("query"));
    let after_first = ledger.verify_session("sess-0").expect("verifies");
    assert_eq!(after_first.event_count, 30);
    let already_stored: Vec<Option<String>> = ledger
        .session_events("sess-0")
        .expect("read")
        .iter()
        .map(|e| e.hash().map(str::to_string))
        .collect();

    // Re-run in full. The first hundred are already there.
    let second = Importer::new(&ledger)
        .run(&ImportOptions::new(&legacy))
        .expect("full import");
    assert_eq!(second.rows_read, 300);
    assert_eq!(
        second.events_written, 61,
        "a resumed import must write only the rows it does not already have, \
         plus the now-complete census"
    );
    assert_eq!(second.events_skipped, 30);

    let events = ledger.session_events("sess-0").expect("read");
    assert_eq!(events.len(), 91, "90 events + 1 census note");
    assert!(ledger.has_census_note("sess-0").expect("query"));
    let head = ledger
        .verify_session("sess-0")
        .expect("the resumed chain verifies");
    assert_eq!(head.event_count, 91);
    // The first hundred are byte-identical. Events are append-only, so a resume
    // that tried to re-chain them would have raised — but the substantive claim
    // is that the resumed rows LINK ONTO them rather than starting a second
    // chain from genesis, and that is only visible by comparing hashes.
    let resumed_hashes: Vec<Option<String>> = events
        .iter()
        .take(already_stored.len())
        .map(|e| e.hash().map(str::to_string))
        .collect();
    assert_eq!(
        resumed_hashes, already_stored,
        "the resume rewrote or re-chained rows that were already imported"
    );
    assert_eq!(
        events[already_stored.len()].prev_hash().to_string(),
        already_stored[already_stored.len() - 1]
            .clone()
            .unwrap_or_default(),
        "the resumed rows did not link onto the stored head"
    );
    assert_ne!(after_first.head_hash, head.head_hash);
}

#[test]
fn the_recorded_transcript_digest_is_carried_forward_not_recomputed() {
    // 75,741 of 76,203 source transcripts no longer exist. An importer that
    // re-hashed the file would fail on 99.4% of the corpus; this fixture's
    // `raw_file` paths deliberately do not exist.
    let dir = tempfile::tempdir().expect("tempdir");
    let legacy = dir.path().join("ledger.db");
    drop(legacy_fixture(&legacy, 1, 12));

    let ledger = Ledger::open_in_memory().expect("open");
    Importer::new(&ledger)
        .run(&ImportOptions::new(&legacy))
        .expect("import");

    let (file, line, sha): (String, i64, String) = ledger
        .conn()
        .query_row(
            "SELECT legacy_raw_file, legacy_raw_line, legacy_raw_sha256
             FROM events WHERE session_id='sess-0' AND seq=9",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("provenance");
    assert!(
        !std::path::Path::new(&file).exists(),
        "fixture path must be dead"
    );
    assert_eq!(line, 9);
    assert_eq!(sha, format!("{:064x}", 9));
    assert!(
        ledger.source_digest_unverifiable("sess-0").expect("flag"),
        "a session whose transcript is gone must be marked — it is a second, \
         independent reason it can never reach `supported`"
    );
}

#[test]
fn the_import_leaves_the_source_database_byte_identical() {
    // The real source is 6.9 GB of the operator's history, in WAL mode. THE
    // property is that the database file is untouched — measured here, and
    // measured again against a copy of the live ledger, where a full
    // 2,046,353-row pass left the SHA-256 identical.
    let dir = tempfile::tempdir().expect("tempdir");
    let legacy = dir.path().join("ledger.db");
    drop(legacy_fixture(&legacy, 2, 20));

    // Confirm the fixture really is in WAL, or this test silently reverts to
    // exercising the easier rollback-journal path it used to.
    {
        let probe = Connection::open(&legacy).expect("probe");
        let mode: String = probe
            .pragma_query_value(None, "journal_mode", |row| row.get(0))
            .expect("journal_mode");
        assert_eq!(mode.to_lowercase(), "wal", "the fixture is not in WAL mode");
    }

    let before = std::fs::read(&legacy).expect("read source");
    let ledger = Ledger::open_in_memory().expect("open");
    Importer::new(&ledger)
        .run(&ImportOptions::new(&legacy))
        .expect("import");
    let after = std::fs::read(&legacy).expect("read source");
    assert_eq!(before, after, "the importer modified the legacy ledger");

    // A rollback journal would mean the importer had opened for WRITING. That
    // stays forbidden.
    assert!(
        !dir.path().join("ledger.db-journal").exists(),
        "the importer created a rollback journal — it opened the source for writing"
    );

    // `-shm` / `-wal` may exist: reading a WAL database requires the
    // shared-memory index even read-only. Asserting their ABSENCE is what the
    // earlier version of this test did, and it only passed because its fixture
    // was not in WAL. Recorded rather than forbidden.
    let shm = dir.path().join("ledger.db-shm").exists();
    let wal = dir.path().join("ledger.db-wal").exists();
    println!("read-only WAL access left -shm={shm} -wal={wal} (database bytes unchanged)");
}

#[test]
fn the_census_lookup_is_served_by_the_session_index() {
    // I reported this query as an unindexed scan and was WRONG. The independent
    // verifier measured it against the real 680,182-row store:
    //   SEARCH events USING INDEX idx_events_session (session_id=?)
    //   0.029 ms per probe, 2.2 s across all 76,203 sessions.
    // The equality is indexed; the LIKE only filters that one session's rows.
    //
    // The "fix" I shipped for the imaginary defect was worse than the defect —
    // a column plus a partial index on a two-million-row table, and a DDL change
    // costs a full pass over every existing ledger. It is reverted, and this
    // test pins the fact that refuted it, so the same wrong instinct cannot be
    // acted on twice.
    let ledger = Ledger::open_in_memory().expect("open");
    seeded(&ledger, "s1", 5);

    let plan = |sql: &str| -> String {
        let mut stmt = ledger
            .conn()
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .expect("prepare");
        let rows: Vec<String> = stmt
            .query_map(["s1"], |row| row.get::<_, String>(3))
            .expect("query")
            .collect::<Result<_, _>>()
            .expect("rows");
        rows.join(" | ")
    };

    let census = plan(
        "SELECT EXISTS(SELECT 1 FROM events
                       WHERE session_id = ?1
                         AND payload LIKE '%legacy_unknown_field_census%')",
    );
    assert!(
        census.contains("idx_events_session"),
        "the census lookup is no longer served by the session index: {census}"
    );
    assert!(
        !census.contains("SCAN events"),
        "the census lookup now scans the events table: {census}"
    );

    let last_seq =
        plan("SELECT MAX(seq) FROM events WHERE session_id = ?1 AND legacy_raw_file IS NOT NULL");
    assert!(
        !last_seq.contains("SCAN events"),
        "the resume cursor query scans the events table: {last_seq}"
    );

    // The negative control: a query that genuinely cannot use an index MUST be
    // reported as a scan, or the assertions above prove nothing about the
    // planner. This is the query I mistakenly believed the census one to be.
    let unindexable = plan("SELECT COUNT(*) FROM events WHERE payload LIKE '%nope%' AND ?1 = ?1");
    assert!(
        unindexable.contains("SCAN"),
        "EXPLAIN QUERY PLAN is not reporting scans at all: {unindexable}"
    );
}

#[test]
fn a_session_carries_exactly_one_census_note() {
    let dir = tempfile::tempdir().expect("tempdir");
    let legacy = dir.path().join("ledger.db");
    drop(legacy_fixture(&legacy, 2, 40));

    let ledger = Ledger::open_in_memory().expect("open");
    Importer::new(&ledger)
        .run(&ImportOptions::new(&legacy))
        .expect("import");

    let notes: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE payload LIKE '%legacy_unknown_field_census%'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(
        notes, 2,
        "one census note per session, no more and no fewer"
    );
    assert!(ledger.has_census_note("sess-0").expect("query"));
    assert!(!ledger.has_census_note("sess-nonexistent").expect("query"));
}

#[test]
fn a_stale_hash_is_refused_at_append_because_the_damage_would_be_permanent() {
    // Measured before the fix: `append_events` checked only that a hash was
    // PRESENT, so an event carrying `b8012697…` while its contents hashed to
    // `7ffe1f5b…` was written. `events` is append-only by trigger, so that row
    // could never be updated or deleted — one forgotten re-seal permanently
    // makes a session unverifiable.
    //
    // This is the last place the row can be rejected while rejection is still
    // cheap.
    let ledger = Ledger::open_in_memory().expect("open");
    let repo = ledger.repo("k", None).expect("repo");
    ledger
        .upsert_session("s1", &repo, "claude-code", None, false, None)
        .expect("session");

    let chain = chain_of("s1", 3);
    let carried = chain[1].hash().map(str::to_string).expect("sealed");

    // Mutate the contents while putting the ORIGINAL hash back: an event that
    // asserts a hash it no longer has.
    let mut wire = chain[1].clone().into_wire();
    wire.seq = 99;
    wire.hash = Some(carried.clone());
    let stale = Event::try_from(wire).expect("still a valid wire");
    assert!(
        stale.verify_hash().is_err(),
        "the fixture is not actually stale"
    );

    let rows: Vec<EventRow> = vec![
        EventRow::bare(chain[0].clone()),
        EventRow::bare(stale),
        EventRow::bare(chain[2].clone()),
    ];
    match ledger.append_events(&repo, &rows) {
        Err(LedgerError::CorruptEvent { .. }) => {}
        Err(other) => panic!("wrong error for a mis-sealed event: {other}"),
        Ok(written) => panic!("a mis-sealed event was accepted: {written} rows written"),
    }

    // And nothing partial: the refusal happens before the transaction opens, so
    // the good first event is not left behind for a later batch to chain onto.
    assert_eq!(
        ledger.session_events("s1").expect("read").len(),
        0,
        "a rejected batch wrote rows"
    );

    // The control: the same batch without the tampered event is accepted, so
    // the refusal is about the stale hash and not about the batch shape.
    let good: Vec<EventRow> = chain.iter().cloned().map(EventRow::bare).collect();
    assert_eq!(ledger.append_events(&repo, &good).expect("append"), 3);
    ledger.verify_session("s1").expect("verifies");
}
