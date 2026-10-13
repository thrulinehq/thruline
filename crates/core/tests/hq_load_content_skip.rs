//! C27 (b) — a reload skips a recording whose `stat` moved but whose bytes did
//! not, and only when a full ingest of it would conclude exactly that.
//!
//! On the founder's store, 39 of the files every no-change reload re-read had
//! the bytes of their accepted generation: 25 Codex rollouts whose device
//! number macOS had renumbered at boot, and 14 Claude Code transcripts with a
//! renumbered device and, mostly, a touched modification time. The seal is
//! content-addressed and never rewritten, so their `stat` could never match
//! it again, and each reload paid a capture, a parse and a lineage check for
//! nothing.
//!
//! Every test here loads the same recordings into two stores and changes
//! them the same way, then reloads one through `hq::run_load` (the skip) and
//! the other through `ingest::ingest` / `ingest::ingest_codex` (the full path,
//! no skip at all), and requires the same events, sessions, heads,
//! generations, generation heads, refusals and locators, and a clean `verify`.
//! A device number cannot be changed from a test; a changed modification time
//! or inode takes the same branch (`stat` differs, size equal).

mod scratch;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use scratch::TempTree;
use thruline_core::hq::{self, LoadFileTiming, LoadOutcome, LoadPlan};
use thruline_core::ingest::{self, CodexIngestOptions, LoadMirror, Unchanged};
use thruline_core::{Ledger, SessionRef};

const ROOT: &str = "/work/app";

fn uuid(n: usize) -> String {
    format!("cccccccc-0000-4000-8000-{n:012}")
}

fn claude_line(session: &str, i: usize) -> String {
    let (kind, content) = if i.is_multiple_of(2) {
        (
            "user",
            serde_json::json!(format!("Step {i}: make the parser handle case {i}.")),
        )
    } else {
        (
            "assistant",
            serde_json::json!([{"type": "text", "text": format!("Done with case {i}.")}]),
        )
    };
    serde_json::json!({
        "parentUuid": null, "isSidechain": false,
        "uuid": uuid(1_000_000 + i), "timestamp": format!("2026-10-01T09:{:02}:{:02}.000Z", (i / 60) % 60, i % 60),
        "cwd": ROOT, "sessionId": session, "version": "2.1.0", "type": kind,
        "message": {"role": kind, "content": content}
    })
    .to_string()
        + "\n"
}

fn codex_meta(id: &str) -> String {
    serde_json::json!({
        "timestamp": "2026-10-04T08:00:00.000Z", "type": "session_meta",
        "payload": {"id": id, "cwd": ROOT, "source": "vscode", "originator": "Codex Desktop"}
    })
    .to_string()
        + "\n"
}

fn codex_line(i: usize) -> String {
    let (role, kind) = if i.is_multiple_of(2) {
        ("user", "input_text")
    } else {
        ("assistant", "output_text")
    };
    serde_json::json!({
        "timestamp": format!("2026-10-04T08:{:02}:{:02}.000Z", (i / 60) % 60, i % 60 ), "type": "response_item",
        "payload": {"type": "message", "id": format!("m{i}"), "role": role,
                    "content": [{"type": kind, "text": format!("Message {i} about the parser.")}]}
    })
    .to_string()
        + "\n"
}

struct World {
    tmp: TempTree,
    plan: LoadPlan,
}

impl World {
    fn new(claude: usize, codex: usize, lines: usize) -> World {
        let tmp = TempTree::new();
        let cdir = tmp.path().join("src/claude/-work-app");
        let xdir = tmp.path().join("src/codex");
        fs::create_dir_all(&cdir).expect("claude dir");
        fs::create_dir_all(&xdir).expect("codex dir");
        let mut plan = LoadPlan::default();
        for n in 0..claude {
            let session = uuid(n);
            let path = cdir.join(format!("{session}.jsonl"));
            let body: String = (0..lines).map(|i| claude_line(&session, i)).collect();
            fs::write(&path, body).expect("write claude");
            plan.claude_code.push(path);
        }
        for n in 0..codex {
            let path = xdir.join(format!("rollout-2026-10-04T08-00-00-x{n:04}.jsonl"));
            let mut body = codex_meta(&format!("codex-synthetic-{n:04}"));
            body.extend((0..lines + n).map(codex_line));
            fs::write(&path, body).expect("write codex");
            plan.codex.push(path);
        }
        World { tmp, plan }
    }

    fn store(&self, name: &str) -> (PathBuf, PathBuf) {
        let dir = self.tmp.path().join(name);
        fs::create_dir_all(&dir).expect("store dir");
        (dir.join("ledger.db"), dir.join("mirrors"))
    }
}

fn load(db: &Path, mirror: &Path, plan: &LoadPlan) -> (LoadOutcome, Vec<LoadFileTiming>) {
    let mut ledger = Ledger::open(db).expect("open store");
    let mut files = Vec::new();
    let outcome = hq::run_load_with(&mut ledger, mirror, plan, 0, &mut |f| {
        files.push(f.clone());
        true
    });
    (outcome, files)
}

/// The full path, with no skip of any kind: every file of `plan` through
/// `ingest` / `ingest_codex`, one call per file, in the plan's order.
fn full_path(db: &Path, mirror: &Path, plan: &LoadPlan) -> (u64, u64) {
    let mut ledger = Ledger::open(db).expect("open store");
    let (mut events, mut refusals) = (0, 0);
    for f in &plan.claude_code {
        let r = ingest::ingest(&mut ledger, std::slice::from_ref(f), mirror).expect("ingest");
        events += r.new_events;
        refusals += r.claude_seal_rejected + r.integrity_alarms.len() as u64;
    }
    let options = CodexIngestOptions {
        mirror_root: mirror.to_path_buf(),
        owner_key: None,
    };
    for f in &plan.codex {
        let r = ingest::ingest_codex(&mut ledger, std::slice::from_ref(f), &options)
            .expect("ingest_codex");
        events += r.new_events;
        refusals += r.rejected_candidates;
    }
    (events, refusals)
}

/// Every deterministic column of every row a load can write: events,
/// sessions, project heads, accepted generations and their heads, refusals,
/// source locators and ledger heads. Event ids are ULIDs and acceptance and
/// observation times are wall-clock, so those are left out; a path into the
/// mirror names a per-store random authority key, so it is cut to the
/// payload's own name.
fn rows(db: &Path) -> Vec<String> {
    let ledger = Ledger::open(db).expect("open store");
    let conn = ledger.conn();
    let tail = |v: rusqlite::types::Value| -> String {
        match v {
            rusqlite::types::Value::Text(t) if t.contains("/generations/") => {
                "mirror:payload.part".to_string()
            }
            other => format!("{other:?}"),
        }
    };
    let mut out = Vec::new();
    for (label, sql, n) in [
        (
            "event",
            "SELECT session_id, seq, kind, role, text, tool_name, tool_use_id, tool_input,
                    tool_result, is_error, src_uuid, parent_uuid, ts, source_turn_id,
                    source_message_key, raw_file, raw_line, raw_sha256, prev_hash, chain_hash,
                    project_id
             FROM events ORDER BY session_id, seq",
            21,
        ),
        (
            "session",
            "SELECT id, project_id, file_path, started_at, last_seq, raw_line_count, raw_digest,
                    head_chain_hash, event_count FROM sessions ORDER BY id",
            9,
        ),
        (
            "head",
            "SELECT project_id, digest, event_total FROM project_heads ORDER BY project_id",
            3,
        ),
        (
            "generation",
            "SELECT id, project_id, session_id, predecessor_id, payload_sha256, payload_bytes,
                    inherited_prefix_bytes, total_envelopes, local_envelopes, local_events
             FROM session_generations ORDER BY id",
            10,
        ),
        (
            "generation_head",
            "SELECT session_id, generation_id FROM session_generation_heads ORDER BY session_id",
            2,
        ),
        (
            "refusal",
            "SELECT id, owner_key, source, source_project_key, source_session_key,
                    predecessor_generation_id, candidate_sha256, candidate_bytes, raw_envelopes,
                    status, reason
             FROM candidate_assessments ORDER BY id",
            11,
        ),
        (
            "source_session",
            "SELECT session_id, project_id, owner_key, source, source_session_key, source_locator
             FROM source_sessions ORDER BY session_id",
            6,
        ),
        (
            "ledger_meta",
            "SELECT project_id, head_hash, event_count FROM ledger_meta ORDER BY project_id",
            3,
        ),
        (
            "provenance",
            "SELECT e.session_id, e.seq, p.generation_id, p.raw_line
             FROM event_generation_provenance p JOIN events e ON e.id = p.event_id
             ORDER BY e.session_id, e.seq",
            4,
        ),
    ] {
        let mut s = conn.prepare(sql).expect(label);
        let mut q = s.query([]).expect("query");
        while let Some(r) = q.next().expect("row") {
            let cols: Vec<String> = (0..n)
                .map(|i| tail(r.get::<_, rusqlite::types::Value>(i).expect("col")))
                .collect();
            out.push(format!("{label} {}", cols.join(" | ")));
        }
    }
    out
}

fn verify_clean(db: &Path) {
    let ledger = Ledger::open(db).expect("open store");
    let report = ingest::verify(&ledger, None).expect("verify");
    assert!(report.ok(), "verify: {report:?}");
}

/// The same bytes, a later modification time, the same inode.
fn touch(path: &Path) {
    let f = fs::File::options()
        .append(true)
        .open(path)
        .expect("open to touch");
    f.set_modified(SystemTime::now() + Duration::from_secs(60))
        .expect("set mtime");
}

/// The same bytes under a new inode (written aside, renamed over), as an
/// editor's save or a restore from a copy gives.
fn replace_with_same_bytes(path: &Path) {
    let bytes = fs::read(path).expect("read");
    let aside = path.with_extension("aside");
    fs::write(&aside, bytes).expect("write aside");
    fs::rename(&aside, path).expect("rename over");
}

/// One character changed, the size kept.
fn rewrite_same_size(path: &Path, from: &str, to: &str) {
    assert_eq!(from.len(), to.len());
    let text = fs::read_to_string(path).expect("read");
    assert!(text.contains(from), "{from:?} not in {}", path.display());
    fs::write(path, text.replacen(from, to, 1)).expect("rewrite");
}

fn how(db: &Path, mirror: &Path, file: &Path, codex: bool) -> Unchanged {
    let mut ledger = Ledger::open(db).expect("open store");
    let loader = LoadMirror::open(&mut ledger, mirror).expect("load mirror");
    loader.unchanged_how(&ledger, file, codex)
}

/// Two stores, A (reloaded through the skip) and B (through the full path),
/// with the same first load.
fn twin_stores(w: &World) -> ((PathBuf, PathBuf), (PathBuf, PathBuf)) {
    let a = w.store("skip");
    let b = w.store("full");
    let (first, _) = load(&a.0, &a.1, &w.plan);
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    let (first, _) = load(&b.0, &b.1, &w.plan);
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert_eq!(rows(&a.0), rows(&b.0), "the twins start the same");
    (a, b)
}

#[test]
fn file_digests_agree_with_the_capture_the_parser_and_whole_file_digest() {
    let tmp = TempTree::new();
    let cases: [&[u8]; 9] = [
        b"",
        b"\n",
        b"{\"a\":1}\n",
        b"{\"a\":1}",
        b"{\"a\":1}\r\n{\"b\":2}\r\n",
        b"{\"a\":1}\n\n   \n{\"b\":2}\n",
        b"not json\n{\"a\":1}\r",
        b"{\"a\":1}\n\r\n\r",
        b"\xff\xfe broken utf8\n{\"a\":1}\n",
    ];
    for (i, bytes) in cases.iter().enumerate() {
        let path = tmp.path().join(format!("case-{i}.jsonl"));
        fs::write(&path, bytes).expect("write");
        let got = ingest::file_digests(&path).expect("file_digests");
        assert_eq!(got.bytes, bytes.len() as u64, "case {i}");
        use sha2::Digest as _;
        let sha = sha2::Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(got.payload_sha256, sha, "case {i}: the capture's digest");
        let (lines, digest) = ingest::whole_file_digest(&path).expect("whole_file_digest");
        assert_eq!(
            (got.raw_lines, got.raw_digest.clone()),
            (lines, digest),
            "case {i}"
        );
        let mut stats = thruline_core::parse::ParseStats::default();
        thruline_core::parse::SessionLines::open(&path)
            .expect("open")
            .collect_with_stats(&mut stats)
            .expect("parse");
        assert_eq!(
            (got.raw_lines, got.raw_digest),
            (stats.raw_lines_total, stats.raw_digest),
            "case {i}: the parser's line count and digest"
        );
    }
}

#[test]
fn same_bytes_under_a_moved_stat_are_skipped_and_match_a_full_ingest() {
    let w = World::new(2, 2, 6);
    let ((a_db, a_mirror), (b_db, b_mirror)) = twin_stores(&w);
    touch(&w.plan.claude_code[0]);
    replace_with_same_bytes(&w.plan.claude_code[1]);
    touch(&w.plan.codex[0]);
    replace_with_same_bytes(&w.plan.codex[1]);

    for f in &w.plan.claude_code {
        assert_eq!(
            how(&a_db, &a_mirror, f, false),
            Unchanged::ByContent,
            "{f:?}"
        );
    }
    for f in &w.plan.codex {
        assert_eq!(
            how(&a_db, &a_mirror, f, true),
            Unchanged::ByContent,
            "{f:?}"
        );
    }

    let (again, files) = load(&a_db, &a_mirror, &w.plan);
    assert!(again.errors.is_empty(), "{:?}", again.errors);
    assert_eq!(again.events_added, 0);
    assert_eq!(again.files_skipped_unopened, 4, "{files:?}");
    assert_eq!(again.sessions_read, 0, "nothing was ingested");

    let (events, refusals) = full_path(&b_db, &b_mirror, &w.plan);
    assert_eq!(
        (events, refusals),
        (0, 0),
        "the full path adds nothing either"
    );
    assert_eq!(rows(&a_db), rows(&b_db));
    verify_clean(&a_db);
    verify_clean(&b_db);
}

#[test]
fn a_same_size_rewrite_takes_the_full_path_and_is_refused_as_before() {
    let w = World::new(1, 1, 6);
    let ((a_db, a_mirror), (b_db, b_mirror)) = twin_stores(&w);
    rewrite_same_size(&w.plan.claude_code[0], "case 2.", "case 9.");
    rewrite_same_size(&w.plan.codex[0], "Message 5 about", "Message 5 above");

    assert_eq!(
        how(&a_db, &a_mirror, &w.plan.claude_code[0], false),
        Unchanged::No
    );
    assert_eq!(how(&a_db, &a_mirror, &w.plan.codex[0], true), Unchanged::No);

    let (again, files) = load(&a_db, &a_mirror, &w.plan);
    assert!(files.iter().all(|f| !f.skipped_unopened), "{files:?}");
    assert!(
        again
            .errors
            .iter()
            .any(|e| e.contains("changed since they were read")),
        "{:?}",
        again.errors
    );
    assert!(
        again.errors.iter().any(|e| e.contains("source_diverged")),
        "{:?}",
        again.errors
    );
    let (events, refusals) = full_path(&b_db, &b_mirror, &w.plan);
    assert_eq!(events, 0);
    assert!(refusals >= 2, "the full path refuses both: {refusals}");
    assert_eq!(rows(&a_db), rows(&b_db));
}

#[test]
fn a_claude_row_whose_digest_is_not_these_bytes_is_parsed_again_not_skipped() {
    let w = World::new(1, 0, 6);
    let ((a_db, a_mirror), (b_db, b_mirror)) = twin_stores(&w);
    // A row whose parse marker belongs to other bytes with the same line
    // count (a parse of a rewrite the seal refused, say): finished, but not
    // for the bytes now on disk.
    for db in [&a_db, &b_db] {
        let ledger = Ledger::open(db).expect("open");
        let changed = ledger
            .conn()
            .execute("UPDATE sessions SET raw_digest = ?1", [&"ab".repeat(32)])
            .expect("another parse's digest");
        assert_eq!(changed, 1);
    }
    let restored = rows(&a_db);
    touch(&w.plan.claude_code[0]);
    assert_eq!(
        how(&a_db, &a_mirror, &w.plan.claude_code[0], false),
        Unchanged::No
    );
    let (again, files) = load(&a_db, &a_mirror, &w.plan);
    assert!(again.errors.is_empty(), "{:?}", again.errors);
    assert!(!files[0].skipped_unopened, "{files:?}");
    full_path(&b_db, &b_mirror, &w.plan);
    let after = rows(&a_db);
    assert_ne!(after, restored, "the parse rewrote the row's digest");
    assert_eq!(after, rows(&b_db));
}

#[test]
fn a_claude_session_whose_provenance_is_not_bound_is_not_skipped_by_content() {
    let w = World::new(1, 0, 6);
    let ((a_db, a_mirror), (b_db, b_mirror)) = twin_stores(&w);
    // The state a parse leaves when it finished before the generation that
    // now holds its bytes was accepted: every event in, the row's digest
    // written, and no event yet bound to that generation's lines. The table
    // is append-only, so the test lifts its delete trigger to build the
    // state, as the unfinished-parse test edits a row.
    for db in [&a_db, &b_db] {
        let ledger = Ledger::open(db).expect("open");
        let conn = ledger.conn();
        let trigger: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name = 'event_generation_provenance_no_delete'",
                [],
                |r| r.get(0),
            )
            .expect("trigger");
        conn.execute_batch("DROP TRIGGER event_generation_provenance_no_delete")
            .expect("drop");
        let removed = conn
            .execute("DELETE FROM event_generation_provenance", [])
            .expect("unbind");
        assert!(removed > 0);
        conn.execute_batch(&trigger).expect("restore trigger");
    }
    let unbound = rows(&a_db);
    touch(&w.plan.claude_code[0]);
    assert_eq!(
        how(&a_db, &a_mirror, &w.plan.claude_code[0], false),
        Unchanged::No
    );
    let (again, files) = load(&a_db, &a_mirror, &w.plan);
    assert!(again.errors.is_empty(), "{:?}", again.errors);
    assert!(!files[0].skipped_unopened, "{files:?}");
    full_path(&b_db, &b_mirror, &w.plan);
    let after = rows(&a_db);
    assert_ne!(after, unbound, "the full path bound the events");
    assert_eq!(after, rows(&b_db));
    verify_clean(&a_db);
}

#[test]
fn a_codex_recording_renamed_into_another_owner_is_not_skipped_by_stat() {
    let w = World::new(0, 2, 6);
    let ((a_db, a_mirror), (b_db, b_mirror)) = twin_stores(&w);
    // Renamed only: the same inode, size and modification time, so its
    // `stat` is exactly what the seal recorded and it is found by that
    // inode. The folder it now sits in is another owner.
    let other = w.tmp.path().join("src/elsewhere");
    fs::create_dir_all(&other).expect("other owner");
    let moved = other.join(w.plan.codex[0].file_name().expect("name"));
    let stat = |p: &Path| {
        use std::os::unix::fs::MetadataExt;
        let m = fs::metadata(p).expect("stat");
        (m.len(), m.modified().expect("mtime"), m.dev(), m.ino())
    };
    let was = stat(&w.plan.codex[0]);
    fs::rename(&w.plan.codex[0], &moved).expect("move");
    assert_eq!(stat(&moved), was, "a rename keeps the stat");
    let mut plan = w.plan.clone();
    plan.codex[0] = moved.clone();

    assert_eq!(how(&a_db, &a_mirror, &moved, true), Unchanged::No);
    let (again, files) = load(&a_db, &a_mirror, &plan);
    assert!(again.errors.is_empty(), "{:?}", again.errors);
    assert!(
        files.iter().any(|f| f.file == moved && !f.skipped_unopened),
        "{files:?}"
    );
    let (events, _) = full_path(&b_db, &b_mirror, &plan);
    assert!(events > 0, "the full path accepts it as a new session");
    assert_eq!(again.events_added, events);
    assert_eq!(rows(&a_db), rows(&b_db));
    verify_clean(&a_db);
}

#[test]
fn a_codex_recording_moved_to_another_owner_is_not_skipped_by_content() {
    let w = World::new(0, 2, 6);
    let ((a_db, a_mirror), (b_db, b_mirror)) = twin_stores(&w);
    // Moved (same inode, so found by its seal's inode) into a folder that is
    // another owner, then touched: the bytes are the accepted generation's,
    // and the full path still routes them to a different session.
    let other = w.tmp.path().join("src/elsewhere");
    fs::create_dir_all(&other).expect("other owner");
    let moved = other.join(w.plan.codex[0].file_name().expect("name"));
    fs::rename(&w.plan.codex[0], &moved).expect("move");
    touch(&moved);
    let mut plan = w.plan.clone();
    plan.codex[0] = moved.clone();

    assert_eq!(how(&a_db, &a_mirror, &moved, true), Unchanged::No);
    let (again, files) = load(&a_db, &a_mirror, &plan);
    assert!(again.errors.is_empty(), "{:?}", again.errors);
    assert!(
        files.iter().any(|f| f.file == moved && !f.skipped_unopened),
        "{files:?}"
    );
    let (events, _) = full_path(&b_db, &b_mirror, &plan);
    assert!(events > 0, "the full path accepts it as a new session");
    assert_eq!(again.events_added, events);
    assert_eq!(rows(&a_db), rows(&b_db));
    verify_clean(&a_db);
}

// ---------------------------------------------------------------------------
// C29 R3 — the unbound-provenance gap in the stat skip.
// ---------------------------------------------------------------------------

/// Events of `db` that no provenance row binds.
fn unbound(db: &Path) -> i64 {
    let ledger = Ledger::open(db).expect("open store");
    ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM events e
             WHERE NOT EXISTS (
               SELECT 1 FROM event_generation_provenance p WHERE p.event_id = e.id)",
            [],
            |r| r.get(0),
        )
        .expect("count unbound")
}

/// The one interleaving that leaves a Claude Code session with events no
/// provenance row binds while every field the stat skip reads says "finished",
/// built through the public API in every store given, the same way in each.
///
/// 1. A load (`ingest`) reads the transcript after it grew and does not accept
///    the grown bytes (in the founder's world, a capture that failed on a live
///    write). Its parse appends the new lines' events, and its backfill binds
///    only what an accepted generation frames: the old lines.
/// 2. A hook seals the file (`seal_transcript`). The acceptance frames every
///    line and binds nothing. It also writes the payload digest into the
///    session row, which the stat skip reads as "not yet parsed" — so this
///    step alone is not a hole, and the helper asserts that.
/// 3. The load's last statement, `upsert_session` with the parse's line count
///    and digest, lands after the hook's commit. They are separate
///    statements, so another process's acceptance can land between them.
///    The helper replays exactly that write, read back from step 1.
///
/// The capture in step 1 is made to fail deterministically: the grown
/// transcript is presented through a symlink, which the mirror refuses and
/// the parser follows. The real file is renamed into place before step 2, so
/// the hook seals exactly the bytes, size, modification time and inode the
/// reload will find — the stat skip's own condition.
fn a_hook_seal_lands_inside_a_parse(w: &World, stores: &[(&Path, &Path)], grow: usize) {
    let file = &w.plan.claude_code[0];
    let session = file
        .file_stem()
        .expect("stem")
        .to_string_lossy()
        .into_owned();
    let body = fs::read_to_string(file).expect("read transcript");
    let had = body.lines().count();
    let real = file.with_extension("real");
    let more: String = (had..had + grow)
        .map(|i| claude_line(&session, i))
        .collect();
    fs::write(&real, body + &more).expect("write grown transcript");
    fs::remove_file(file).expect("remove transcript");
    std::os::unix::fs::symlink(&real, file).expect("symlink");
    let mut parsed_rows = Vec::new();
    for (db, mirror) in stores {
        let mut ledger = Ledger::open(db).expect("open store");
        let r = ingest::ingest(&mut ledger, std::slice::from_ref(file), mirror).expect("ingest");
        assert!(
            !r.claude_seal_failures.is_empty(),
            "the capture failed: {r:?}"
        );
        assert!(r.new_events > 0, "the parse appended the new lines: {r:?}");
        let row: SessionRef = ledger
            .conn()
            .query_row(
                "SELECT id, project_id, file_path, started_at, raw_line_count, raw_digest
                 FROM sessions WHERE id = ?1",
                [&session],
                |r| {
                    Ok(SessionRef {
                        id: r.get(0)?,
                        project_id: r.get(1)?,
                        file_path: r.get(2)?,
                        started_at: r.get(3)?,
                        raw_line_count: r.get::<_, i64>(4)? as u64,
                        raw_digest: r.get(5)?,
                    })
                },
            )
            .expect("the row the parse wrote");
        parsed_rows.push(row);
    }
    fs::rename(&real, file).expect("the real file back in place");
    for ((db, mirror), row) in stores.iter().zip(parsed_rows) {
        {
            let mut ledger = Ledger::open(db).expect("open store");
            let r = ingest::seal_transcript(&mut ledger, mirror, file).expect("hook seal");
            assert!(r.claude_seal_failures.is_empty(), "{r:?}");
        }
        assert_eq!(
            how(db, mirror, file, false),
            Unchanged::No,
            "after the hook's acceptance alone, the row reads as not yet parsed"
        );
        let (lines, digest) = ingest::whole_file_digest(file).expect("digest");
        assert_eq!(
            (row.raw_line_count, row.raw_digest.clone()),
            (lines, Some(digest)),
            "the parse read the bytes the hook sealed"
        );
        let ledger = Ledger::open(db).expect("open store");
        ledger.upsert_session(&row).expect("the parse's last write");
    }
}

#[test]
fn a_claude_session_with_an_unbound_event_is_bound_on_the_stat_skip_path_as_on_a_full_read() {
    let w = World::new(2, 1, 6);
    let ((a_db, a_mirror), (b_db, b_mirror)) = twin_stores(&w);
    a_hook_seal_lands_inside_a_parse(&w, &[(&a_db, &a_mirror), (&b_db, &b_mirror)], 4);
    let before = rows(&a_db);
    assert_eq!(before, rows(&b_db), "the twins are in the same state");
    assert!(
        unbound(&a_db) > 0,
        "the state under test: events left unbound"
    );
    assert_eq!(unbound(&a_db), unbound(&b_db));

    // Every other field the stat skip reads says "nothing to do" for this
    // file — the seal recorded its current stat, the row's digest is the
    // parse's, the line counts agree — and only the provenance says otherwise.
    assert_eq!(
        how(&a_db, &a_mirror, &w.plan.claude_code[0], false),
        Unchanged::No,
        "an unbound session is not skipped"
    );
    // The bound sessions still skip unopened.
    assert_eq!(
        how(&a_db, &a_mirror, &w.plan.claude_code[1], false),
        Unchanged::ByStat
    );
    assert_eq!(
        how(&a_db, &a_mirror, &w.plan.codex[0], true),
        Unchanged::ByStat
    );

    let (again, files) = load(&a_db, &a_mirror, &w.plan);
    assert!(again.errors.is_empty(), "{:?}", again.errors);
    assert_eq!(again.files_skipped_unopened, 2, "{files:?}");
    assert!(
        files
            .iter()
            .any(|f| f.file == w.plan.claude_code[0] && !f.skipped_unopened),
        "{files:?}"
    );
    full_path(&b_db, &b_mirror, &w.plan);

    assert_eq!(unbound(&a_db), 0, "the skip path bound every event");
    assert_eq!(unbound(&b_db), 0, "the full read bound every event");
    let after = rows(&a_db);
    assert_ne!(after, before, "the reload bound the events");
    assert_eq!(after, rows(&b_db));
    verify_clean(&a_db);
    verify_clean(&b_db);
}

#[test]
fn a_claude_session_with_an_unbound_event_is_bound_on_the_content_skip_path_as_on_a_full_read() {
    let w = World::new(1, 0, 6);
    let ((a_db, a_mirror), (b_db, b_mirror)) = twin_stores(&w);
    a_hook_seal_lands_inside_a_parse(&w, &[(&a_db, &a_mirror), (&b_db, &b_mirror)], 4);
    touch(&w.plan.claude_code[0]);
    assert!(unbound(&a_db) > 0);
    assert_eq!(
        how(&a_db, &a_mirror, &w.plan.claude_code[0], false),
        Unchanged::No
    );
    let (again, files) = load(&a_db, &a_mirror, &w.plan);
    assert!(again.errors.is_empty(), "{:?}", again.errors);
    assert!(!files[0].skipped_unopened, "{files:?}");
    full_path(&b_db, &b_mirror, &w.plan);
    assert_eq!(unbound(&a_db), 0);
    assert_eq!(rows(&a_db), rows(&b_db));
    verify_clean(&a_db);
    verify_clean(&b_db);
}

/// The premise the provenance count screen rests on (`ingest.rs`,
/// `claude_provenance_pending`): a provenance row binds an event of the same
/// session as the generation it names. Pinned two ways: on every row that a
/// load, a failed capture, a hook seal and a reload of both sources write;
/// and on the source — exactly two statements in `crates/core/src` write the
/// table, and the backfill joins the event's session to the generation's.
/// A third writer fails this test until it is read against the premise.
#[test]
fn every_provenance_row_binds_an_event_of_its_generations_own_session() {
    let w = World::new(2, 2, 6);
    let ((a_db, a_mirror), _) = twin_stores(&w);
    a_hook_seal_lands_inside_a_parse(&w, &[(&a_db, &a_mirror)], 4);
    let codex = &w.plan.codex[0];
    let mut grown = fs::read_to_string(codex).expect("read rollout");
    grown.extend((100..104).map(codex_line));
    fs::write(codex, grown).expect("grow rollout");
    let (again, _) = load(&a_db, &a_mirror, &w.plan);
    assert!(again.errors.is_empty(), "{:?}", again.errors);
    assert!(again.events_added > 0);

    let ledger = Ledger::open(&a_db).expect("open store");
    let (joined, crossed): (i64, i64) = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(e.session_id <> g.session_id), 0)
             FROM event_generation_provenance p
             JOIN events e ON e.id = p.event_id
             JOIN session_generations g ON g.id = p.generation_id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("premise query");
    let all: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM event_generation_provenance",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert!(joined > 0);
    assert_eq!(
        joined, all,
        "every provenance row names a stored event and generation"
    );
    assert_eq!(crossed, 0, "no provenance row crosses sessions");

    let mut writers = Vec::new();
    let mut stack = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).expect("read src") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let text = fs::read_to_string(&path).expect("read source");
                for (i, _) in text.match_indices("INTO event_generation_provenance") {
                    let statement: String = text[i..].chars().take(600).collect();
                    writers.push((path.clone(), statement));
                }
            }
        }
    }
    let files: Vec<_> = writers.iter().map(|(p, _)| p).collect();
    assert_eq!(writers.len(), 2, "the provenance writers: {files:?}");
    assert!(
        writers.iter().all(|(p, _)| p.ends_with("ledger.rs")),
        "{files:?}"
    );
    assert!(
        writers
            .iter()
            .any(|(_, s)| s.contains("JOIN session_generations g ON g.session_id = e.session_id")),
        "the backfill joins the event's session to the generation's"
    );
}
