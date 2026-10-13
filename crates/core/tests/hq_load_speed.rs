//! C27 — the HQ load reads only what changed, and reads it fast.
//!
//! Synthetic recordings only (generated here; nothing private). Every test
//! drives `hq::run_load`, the code `POST /api/hq/projects/<id>/load` runs.
//!
//! * An unchanged file is skipped without being opened: the tests make every
//!   source file unreadable (`chmod 000`) before the reload, so any open
//!   would fail loudly.
//! * A file that grew gains exactly the rows a full ingest of the same bytes
//!   gives, hash chain included.
//! * A file that was rewritten or shrank takes today's path.
//! * A file that is gone changes nothing.
//! * A parallel load and a sequential load write the same rows.
//! * Speed: a no-change reload of a few hundred recordings is far under the
//!   first load's time and under a generous absolute bound.

mod scratch;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use scratch::TempTree;
use thruline_core::hq::{self, LoadFileTiming, LoadOutcome, LoadPlan};
use thruline_core::Ledger;

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

/// A scratch world: recordings under `<tmp>/src`, the store under `<tmp>/store`.
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
            // `lines + n` lines: every Codex recording a different size, so
            // the parallel load maps them on its workers (same-size ones it
            // runs one at a time).
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

    fn files(&self) -> Vec<PathBuf> {
        self.plan
            .claude_code
            .iter()
            .chain(self.plan.codex.iter())
            .cloned()
            .collect()
    }
}

fn load(
    db: &Path,
    mirror: &Path,
    plan: &LoadPlan,
    workers: usize,
) -> (LoadOutcome, Vec<LoadFileTiming>) {
    let mut ledger = Ledger::open(db).expect("open store");
    let mut files = Vec::new();
    let outcome = hq::run_load_with(&mut ledger, mirror, plan, workers, &mut |f| {
        files.push(f.clone());
        true
    });
    (outcome, files)
}

/// Every deterministic column of every event, session and head, in a fixed
/// order. Event ids are ULIDs and acceptance times are wall-clock, so no two
/// loads — not even two sequential ones — write byte-identical files; this is
/// everything else. `raw_file` and a Codex session's `file_path` name a
/// mirror payload under a per-store random authority key, so the mirror
/// prefix is cut to the payload's own name.
fn rows(db: &Path) -> Vec<String> {
    let ledger = Ledger::open(db).expect("open store");
    let conn = ledger.conn();
    let tail = |p: String| -> String {
        match p.find("/generations/") {
            Some(_) => "mirror:payload.part".to_string(),
            None => p,
        }
    };
    let mut out = Vec::new();
    let mut s = conn
        .prepare(
            "SELECT session_id, seq, kind, role, text, tool_name, tool_use_id, tool_input,
                    tool_result, is_error, src_uuid, parent_uuid, ts, source_turn_id,
                    source_message_key, raw_file, raw_line, raw_sha256, prev_hash, chain_hash,
                    project_id
             FROM events ORDER BY session_id, seq",
        )
        .expect("events");
    let mut q = s.query([]).expect("query");
    while let Some(r) = q.next().expect("row") {
        let mut cols = Vec::new();
        for i in 0..21 {
            let v: rusqlite::types::Value = r.get(i).expect("col");
            let text = match v {
                rusqlite::types::Value::Text(t) if i == 15 => tail(t),
                other => format!("{other:?}"),
            };
            cols.push(text);
        }
        out.push(format!("event {}", cols.join(" | ")));
    }
    let mut s = conn
        .prepare(
            "SELECT id, project_id, file_path, last_seq, raw_line_count, raw_digest,
                    head_chain_hash, event_count FROM sessions ORDER BY id",
        )
        .expect("sessions");
    let mut q = s.query([]).expect("query");
    while let Some(r) = q.next().expect("row") {
        let file: String = r.get(2).expect("file");
        out.push(format!(
            "session {} {} {} {:?} {:?} {:?} {:?} {:?}",
            r.get::<_, String>(0).expect("id"),
            r.get::<_, String>(1).expect("project"),
            tail(file),
            r.get::<_, i64>(3).expect("seq"),
            r.get::<_, i64>(4).expect("lines"),
            r.get::<_, Option<String>>(5).expect("digest"),
            r.get::<_, Option<String>>(6).expect("head"),
            r.get::<_, i64>(7).expect("count"),
        ));
    }
    let mut s = conn
        .prepare("SELECT project_id, digest, event_total FROM project_heads ORDER BY project_id")
        .expect("heads");
    let mut q = s.query([]).expect("query");
    while let Some(r) = q.next().expect("row") {
        out.push(format!(
            "head {} {} {}",
            r.get::<_, String>(0).expect("p"),
            r.get::<_, String>(1).expect("d"),
            r.get::<_, i64>(2).expect("n")
        ));
    }
    out
}

fn verify_clean(db: &Path) {
    let ledger = Ledger::open(db).expect("open store");
    let report = thruline_core::ingest::verify(&ledger, None).expect("verify");
    assert!(report.ok(), "verify: {report:?}");
}

/// Make every file unreadable for the life of the guard: a load that opens
/// one fails on it, loudly.
struct Unreadable(Vec<PathBuf>);

impl Unreadable {
    fn new(files: Vec<PathBuf>) -> Unreadable {
        for f in &files {
            fs::set_permissions(f, fs::Permissions::from_mode(0o000)).expect("chmod 000");
        }
        Unreadable(files)
    }
}

impl Drop for Unreadable {
    fn drop(&mut self) {
        for f in &self.0 {
            let _ = fs::set_permissions(f, fs::Permissions::from_mode(0o644));
        }
    }
}

fn append(path: &Path, text: &str) {
    use std::io::Write as _;
    let mut f = fs::OpenOptions::new()
        .append(true)
        .open(path)
        .expect("open for append");
    f.write_all(text.as_bytes()).expect("append");
}

#[test]
fn an_unchanged_file_is_skipped_without_being_opened() {
    let w = World::new(3, 2, 6);
    let (db, mirror) = w.store("store");
    let (first, _) = load(&db, &mirror, &w.plan, 0);
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert!(first.events_added > 0);
    let before = rows(&db);

    let _guard = Unreadable::new(w.files());
    let (again, files) = load(&db, &mirror, &w.plan, 0);
    assert!(
        again.errors.is_empty(),
        "an unchanged file was opened: {:?}",
        again.errors
    );
    assert_eq!(again.files_done, 5);
    assert_eq!(again.files_skipped_unopened, 5, "{files:?}");
    assert_eq!(again.events_added, 0);
    assert!(files.iter().all(|f| f.skipped_unopened));
    drop(_guard);
    assert_eq!(rows(&db), before, "a skipped file changed the ledger");
    verify_clean(&db);
}

#[test]
fn a_file_that_grew_gains_exactly_the_rows_a_full_ingest_of_its_bytes_gives() {
    let w = World::new(2, 2, 6);
    let (db, mirror) = w.store("incremental");
    let (first, _) = load(&db, &mirror, &w.plan, 0);
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    let prefix = rows(&db);

    // One Claude and one Codex recording grow; the others do not.
    let claude = w.plan.claude_code[0].clone();
    let session = claude
        .file_stem()
        .and_then(|s| s.to_str())
        .expect("stem")
        .to_string();
    let grown: String = (6..10).map(|i| claude_line(&session, i)).collect();
    append(&claude, &grown);
    let codex = w.plan.codex[0].clone();
    append(&codex, &(100..104).map(codex_line).collect::<String>());

    let (again, files) = load(&db, &mirror, &w.plan, 0);
    assert!(again.errors.is_empty(), "{:?}", again.errors);
    let read: Vec<&Path> = files
        .iter()
        .filter(|f| !f.skipped_unopened)
        .map(|f| f.file.as_path())
        .collect();
    assert_eq!(read.len(), 2, "only the two grown files are read: {read:?}");
    assert!(read.contains(&claude.as_path()) && read.contains(&codex.as_path()));
    assert_eq!(again.events_added, 8, "four new lines in each grown file");

    // Every row written before is still there, unchanged.
    let after = rows(&db);
    let events_before: Vec<&String> = prefix.iter().filter(|r| r.starts_with("event")).collect();
    for row in &events_before {
        assert!(after.contains(row), "a prefix row changed: {row}");
    }

    // And the whole ledger is what one full ingest of the final bytes gives.
    let (full_db, full_mirror) = w.store("full");
    let (full, _) = load(&full_db, &full_mirror, &w.plan, 0);
    assert!(full.errors.is_empty(), "{:?}", full.errors);
    assert_eq!(rows(&db), rows(&full_db));
    verify_clean(&db);
}

#[test]
fn a_rewritten_or_shrunk_file_takes_todays_path() {
    let w = World::new(2, 2, 6);
    let (db, mirror) = w.store("store");
    load(&db, &mirror, &w.plan, 0);
    let before = rows(&db);

    // Claude: the same number of lines, one of them different.
    let claude = w.plan.claude_code[0].clone();
    let text = fs::read_to_string(&claude).expect("read");
    fs::write(&claude, text.replacen("case 2.", "case 9.", 1)).expect("rewrite");
    // Claude: shrunk to its first two lines.
    let shrunk = w.plan.claude_code[1].clone();
    let text = fs::read_to_string(&shrunk).expect("read");
    let head: String = text.lines().take(2).map(|l| format!("{l}\n")).collect();
    fs::write(&shrunk, head).expect("shrink");
    // Codex: a different last line.
    let codex = w.plan.codex[0].clone();
    let text = fs::read_to_string(&codex).expect("read");
    fs::write(
        &codex,
        text.replacen("Message 5 about", "Message 5 above", 1),
    )
    .expect("rewrite");

    let (again, files) = load(&db, &mirror, &w.plan, 0);
    let read: Vec<&Path> = files
        .iter()
        .filter(|f| !f.skipped_unopened)
        .map(|f| f.file.as_path())
        .collect();
    assert_eq!(read.len(), 3, "{read:?}");
    assert_eq!(
        again.events_added, 0,
        "a rewrite never re-hashes into the ledger"
    );
    assert!(
        again
            .errors
            .iter()
            .any(|e| e.contains("changed since they were read")),
        "the rewritten Claude line raises today's integrity alarm: {:?}",
        again.errors
    );
    assert!(
        again.errors.iter().any(|e| e.contains("source_diverged")),
        "the rewritten Codex file is refused as today: {:?}",
        again.errors
    );
    // Today's path leaves every event where it was.
    let events = |r: &[String]| -> Vec<String> {
        r.iter()
            .filter(|x| x.starts_with("event"))
            .cloned()
            .collect()
    };
    assert_eq!(events(&rows(&db)), events(&before));
}

#[test]
fn a_file_that_is_gone_changes_nothing() {
    let w = World::new(2, 1, 4);
    let (db, mirror) = w.store("store");
    load(&db, &mirror, &w.plan, 0);
    let before = rows(&db);
    fs::remove_file(&w.plan.claude_code[0]).expect("delete");
    fs::remove_file(&w.plan.codex[0]).expect("delete");
    let (again, _) = load(&db, &mirror, &w.plan, 0);
    assert!(again.errors.is_empty(), "{:?}", again.errors);
    assert_eq!(again.files_gone, 2);
    assert_eq!(again.files_skipped_unopened, 1);
    assert_eq!(rows(&db), before);
}

/// C27 (b): a new modification time no longer sends a file down the full
/// path. It is read once, its bytes are found to be the accepted
/// generation's, and it is not ingested (`hq_load_content_skip.rs` compares
/// that with a full ingest row for row).
#[test]
fn a_touched_but_identical_file_is_read_once_and_adds_nothing() {
    let w = World::new(1, 1, 4);
    let (db, mirror) = w.store("store");
    load(&db, &mirror, &w.plan, 0);
    let before = rows(&db);
    for f in w.files() {
        let bytes = fs::read(&f).expect("read");
        std::thread::sleep(Duration::from_millis(5));
        fs::write(&f, bytes).expect("same bytes, new mtime");
    }
    let (again, files) = load(&db, &mirror, &w.plan, 0);
    assert!(again.errors.is_empty(), "{:?}", again.errors);
    assert!(
        files.iter().all(|f| f.skipped_unopened),
        "identical bytes are not ingested: {files:?}"
    );
    assert_eq!(again.sessions_read, 0);
    assert_eq!(again.events_added, 0);
    assert_eq!(rows(&db), before);
}

#[test]
fn a_claude_parse_that_did_not_finish_is_not_skipped() {
    let w = World::new(1, 0, 4);
    let (db, mirror) = w.store("store");
    load(&db, &mirror, &w.plan, 0);
    let before = rows(&db);
    // The state a load leaves when the seal was accepted and the parse that
    // follows it did not finish: acceptance writes the payload digest into
    // the session row, and only a finished parse replaces it.
    {
        let ledger = Ledger::open(&db).expect("open");
        let changed = ledger
            .conn()
            .execute(
                "UPDATE sessions SET raw_digest = (
                   SELECT g.payload_sha256 FROM session_generation_heads h
                   JOIN session_generations g ON g.id = h.generation_id
                   WHERE h.session_id = sessions.id)",
                [],
            )
            .expect("simulate an unfinished parse");
        assert_eq!(changed, 1);
    }
    let (again, files) = load(&db, &mirror, &w.plan, 0);
    assert!(again.errors.is_empty(), "{:?}", again.errors);
    assert!(
        !files[0].skipped_unopened,
        "an unfinished parse is finished, not skipped"
    );
    assert_eq!(
        rows(&db),
        before,
        "and finishing it restores exactly the finished state"
    );
}

#[test]
fn a_parallel_load_writes_the_same_rows_as_a_sequential_one() {
    let mut w = World::new(24, 12, 12);
    // Plus the HQ fixtures, whose prompts, edits, runs and claims give the
    // derived graph something to derive.
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hq");
    for (sub, codex) in [("claude/-work-app", false), ("codex", true)] {
        for entry in fs::read_dir(fixtures.join(sub)).expect("fixtures") {
            let from = entry.expect("entry").path();
            let to = if codex {
                w.tmp.path().join("src/codex")
            } else {
                w.tmp.path().join("src/claude/-work-app")
            }
            .join(from.file_name().expect("name"));
            fs::copy(&from, &to).expect("copy fixture");
            if codex {
                w.plan.codex.push(to);
            } else {
                w.plan.claude_code.push(to);
            }
        }
    }
    let (seq_db, seq_mirror) = w.store("sequential");
    let (par_db, par_mirror) = w.store("parallel");
    let (s, s_files) = load(&seq_db, &seq_mirror, &w.plan, 0);
    let (p, p_files) = load(&par_db, &par_mirror, &w.plan, 4);
    assert!(
        s.errors.is_empty() && p.errors.is_empty(),
        "{:?} {:?}",
        s.errors,
        p.errors
    );
    assert_eq!(s.events_added, p.events_added);
    let order = |f: &[LoadFileTiming]| f.iter().map(|x| x.file.clone()).collect::<Vec<_>>();
    assert_eq!(
        order(&s_files),
        order(&p_files),
        "the same files, in the same order"
    );
    assert_eq!(
        p_files.iter().filter(|f| f.worker_mapped).count(),
        13,
        "every Codex recording was mapped on a worker and used"
    );
    assert!(s_files.iter().all(|f| !f.worker_mapped));
    assert_eq!(rows(&seq_db), rows(&par_db));
    verify_clean(&seq_db);
    verify_clean(&par_db);
    // And rebuilding the derived graph from each gives the same answer.
    assert_eq!(rebuilt(&seq_db), rebuilt(&par_db));
}

/// `thruline rebuild` for every project, then every node and edge, with each
/// node's anchor event named by `session:seq` (event ids are ULIDs).
fn rebuilt(db: &Path) -> Vec<String> {
    let ledger = Ledger::open(db).expect("open");
    let cfg = thruline_core::config::Config::default();
    for id in ledger.project_ids().expect("projects") {
        thruline_core::derive::rebuild(&ledger, &id, &cfg).expect("rebuild");
    }
    let conn = ledger.conn();
    let mut out = Vec::new();
    let mut s = conn
        .prepare(
            "SELECT n.id, n.project_id, n.label, n.sort_key, n.lifecycle, n.evidence_state,
                    n.currency, n.session_id, e.session_id || ':' || e.seq
             FROM nodes n LEFT JOIN events e ON e.id = n.anchor_event_id
             ORDER BY n.project_id, n.id",
        )
        .expect("nodes");
    let mut q = s.query([]).expect("query");
    while let Some(r) = q.next().expect("row") {
        let cols: Vec<String> = (0..9)
            .map(|i| format!("{:?}", r.get::<_, rusqlite::types::Value>(i).expect("col")))
            .collect();
        out.push(cols.join(" | "));
    }
    let mut s = conn
        .prepare("SELECT from_node, to_node, kind FROM edges ORDER BY from_node, to_node, kind")
        .expect("edges");
    let mut q = s.query([]).expect("query");
    while let Some(r) = q.next().expect("row") {
        out.push(format!(
            "edge {} {} {}",
            r.get::<_, String>(0).expect("from"),
            r.get::<_, String>(1).expect("to"),
            r.get::<_, String>(2).expect("kind")
        ));
    }
    assert!(!out.is_empty(), "the rebuild derived nothing to compare");
    out
}

#[test]
fn the_newest_file_is_loaded_first() {
    let w = World::new(3, 2, 4);
    let newest = w.plan.codex[1].clone();
    append(&newest, &codex_line(99));
    let order: Vec<PathBuf> = hq::load_order(&w.plan)
        .into_iter()
        .map(|(p, _)| p)
        .collect();
    assert_eq!(order[0], newest);
}

/// The gate's speed check. Generous and load-tolerant on purpose: the reload
/// must be far under the first load (a fifth), under an absolute bound, and
/// must open nothing.
#[test]
fn a_no_change_reload_of_hundreds_of_recordings_is_fast_and_opens_nothing() {
    let w = World::new(240, 80, 20);
    let (db, mirror) = w.store("store");
    let t = Instant::now();
    let (first, _) = load(&db, &mirror, &w.plan, 4);
    let first_time = t.elapsed();
    assert!(first.errors.is_empty(), "{:?}", first.errors);
    assert_eq!(first.files_done, 320);

    let _guard = Unreadable::new(w.files());
    let t = Instant::now();
    let (again, files) = load(&db, &mirror, &w.plan, 4);
    let reload = t.elapsed();
    let checks: u64 = files
        .iter()
        .map(|f| f.phase_ns.get("check").copied().unwrap_or(0))
        .sum();
    eprintln!(
        "reload: mirror open + orphan scan {:?}, unchanged checks {:?}",
        Duration::from_nanos(again.setup_ns),
        Duration::from_nanos(checks)
    );
    drop(_guard);
    assert!(
        again.errors.is_empty(),
        "an unchanged file was opened: {:?}",
        again.errors
    );
    assert_eq!(again.files_skipped_unopened, 320);
    eprintln!("first load {first_time:?}, no-change reload {reload:?}");
    assert!(
        reload < first_time / 5,
        "a no-change reload ({reload:?}) must be far under the first load ({first_time:?})"
    );
    assert!(
        reload < Duration::from_secs(20),
        "no-change reload took {reload:?}"
    );
}

#[test]
fn a_parallel_reload_of_grown_files_matches_a_sequential_one() {
    let w = World::new(6, 6, 6);
    let (seq_db, seq_mirror) = w.store("sequential");
    let (par_db, par_mirror) = w.store("parallel");
    load(&seq_db, &seq_mirror, &w.plan, 0);
    load(&par_db, &par_mirror, &w.plan, 4);
    for f in w.plan.claude_code.iter().take(3) {
        let session = f
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("stem")
            .to_string();
        append(
            f,
            &(6..9).map(|i| claude_line(&session, i)).collect::<String>(),
        );
    }
    for f in w.plan.codex.iter().take(3) {
        append(f, &(100..103).map(codex_line).collect::<String>());
    }
    let (s, _) = load(&seq_db, &seq_mirror, &w.plan, 0);
    let (p, _) = load(&par_db, &par_mirror, &w.plan, 4);
    assert!(
        s.errors.is_empty() && p.errors.is_empty(),
        "{:?} {:?}",
        s.errors,
        p.errors
    );
    assert_eq!((s.events_added, p.events_added), (18, 18));
    assert_eq!(rows(&seq_db), rows(&par_db));
    verify_clean(&par_db);
}

#[test]
fn two_copies_of_one_codex_session_load_the_same_in_parallel() {
    // Codex moves finished rollouts to `archived_sessions/`; a copy can sit in
    // both places, and the two must not race each other into a refusal.
    // Six pairs, three rounds: the race is a matter of timing, so the test
    // gives it many chances to happen.
    for round in 0..3 {
        let mut w = World::new(1, 6, 6);
        for (k, original) in w.plan.codex.clone().into_iter().enumerate() {
            let copy = w
                .tmp
                .path()
                .join(format!("src/codex/archived-copy-{k}.jsonl"));
            fs::copy(&original, &copy).expect("copy");
            w.plan.codex.push(copy);
        }
        let (seq_db, seq_mirror) = w.store("sequential");
        let (par_db, par_mirror) = w.store("parallel");
        let (s, _) = load(&seq_db, &seq_mirror, &w.plan, 0);
        let (p, _) = load(&par_db, &par_mirror, &w.plan, 4);
        assert_eq!(s.errors, p.errors, "round {round}");
        assert_eq!(s.events_added, p.events_added, "round {round}");
        assert_eq!(rows(&seq_db), rows(&par_db), "round {round}");
    }
}

#[test]
fn a_codex_recording_moved_to_the_archive_is_still_skipped_unopened() {
    let w = World::new(1, 2, 6);
    // Codex's own layout: `sessions/` and `archived_sessions/` under one
    // `.codex` folder, which is the owner of both. (Two folders with no
    // `.codex` above them are two owners, and the full path would accept a
    // renamed file as a new session: `hq_load_content_skip.rs`.)
    let sessions = w.tmp.path().join("src/home/.codex/sessions");
    fs::create_dir_all(&sessions).expect("sessions dir");
    let mut plan = w.plan.clone();
    let live = sessions.join(w.plan.codex[0].file_name().expect("name"));
    fs::rename(&w.plan.codex[0], &live).expect("into .codex");
    plan.codex[0] = live.clone();
    let (db, mirror) = w.store("store");
    load(&db, &mirror, &plan, 0);
    let before = rows(&db);
    // Codex moves a finished rollout into `archived_sessions/` by renaming it:
    // same inode, same bytes, new path.
    let archive = w.tmp.path().join("src/home/.codex/archived_sessions");
    fs::create_dir_all(&archive).expect("archive dir");
    let moved = archive.join(live.file_name().expect("name"));
    fs::rename(&live, &moved).expect("move");
    plan.codex[0] = moved.clone();
    let _guard = Unreadable::new(vec![moved]);
    let (again, files) = load(&db, &mirror, &plan, 0);
    assert!(
        again.errors.is_empty(),
        "the moved file was opened: {:?}",
        again.errors
    );
    assert!(files.iter().all(|f| f.skipped_unopened), "{files:?}");
    drop(_guard);
    assert_eq!(rows(&db), before);
}

#[test]
fn a_worker_mapping_for_a_session_the_ledger_already_holds_is_never_used() {
    // A grown copy of an already-loaded Codex recording, at a path the ledger
    // has never seen: the worker maps it as a session never accepted, and
    // the writer must find that the ledger does hold it and map it again.
    let w = World::new(1, 2, 6);
    let (seq_db, seq_mirror) = w.store("sequential");
    let (par_db, par_mirror) = w.store("parallel");
    load(&seq_db, &seq_mirror, &w.plan, 0);
    load(&par_db, &par_mirror, &w.plan, 4);
    let grown = w.tmp.path().join("src/codex/grown-copy.jsonl");
    let mut bytes = fs::read(&w.plan.codex[0]).expect("read");
    bytes.extend((100..104).map(codex_line).collect::<String>().into_bytes());
    fs::write(&grown, bytes).expect("write");
    let mut plan = w.plan.clone();
    plan.codex.push(grown);
    let (s, _) = load(&seq_db, &seq_mirror, &plan, 0);
    let (p, p_files) = load(&par_db, &par_mirror, &plan, 4);
    assert_eq!(s.errors, p.errors);
    assert_eq!((s.events_added, p.events_added), (4, 4));
    assert!(p_files.iter().all(|f| !f.worker_mapped), "{p_files:?}");
    assert_eq!(rows(&seq_db), rows(&par_db));
    verify_clean(&par_db);
}
