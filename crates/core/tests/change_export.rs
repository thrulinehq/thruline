//! T-5-4 (`B10.16`, `Q18`) — whether the records a Change's taken-away copy
//! rests on can be read now, and if not, why.
//!
//! One test per way a record can stop being readable, each made by doing that
//! thing to a real file or a real row: `chmod 000`, delete, rewrite, point a
//! link at nothing. Plus the two properties the export leans on: the module
//! writes nothing, and every path it serves went through the masker.
//!
//! Every id is synthetic and no fixture here contains private content.

use std::path::{Path, PathBuf};

use rusqlite::params;
use thruline_core::chain::sha256_hex;
use thruline_core::change_decision::{self, Decide};
use thruline_core::change_export::{self, Standing};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c001";
const SESSION: &str = "cccccccc-0000-4000-8000-00000000s001";
const VERSION: &str = "snap_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const AT: &str = "2026-09-23T12:00:00Z";

struct Store {
    dir: tempfile::TempDir,
    ledger: Ledger,
}

impl Store {
    fn root(&self) -> PathBuf {
        self.dir.path().join("worktree")
    }
    fn transcript(&self) -> PathBuf {
        self.dir.path().join("transcript.jsonl")
    }
}

/// One project, one Change with an accepted scope, bound to a snapshot whose
/// root is a real folder, one confirmed session whose two events name two
/// lines of a real transcript file.
fn store() -> Store {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("worktree");
    std::fs::create_dir_all(&root).expect("worktree");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open a fresh ledger");
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES(?1, ?2, 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
        params![PROJECT, root.to_string_lossy()],
    )
    .expect("project");
    conn.execute(
        "INSERT INTO changes(id, project_id, opened_at, title, purpose, request_event_id,
                             requester, request_source)
         VALUES(?1, ?2, '2026-09-22T08:00:00Z', 'Totals add up', 'They do not.', NULL,
                'sam', 'person')",
        params![CHANGE, PROJECT],
    )
    .expect("change");
    conn.execute(
        "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at, criteria_json,
                                            exclusions_json, allowed_scope_json, constraints_json,
                                            check_profile_json, content_sha256)
         VALUES(?1, 1, '2026-09-22T08:30:00Z', '[\"The totals add up\",\"Nothing else breaks\"]',
                '[]', '[\"checkout/\"]', '[]', '[]', 'deadbeef')",
        params![CHANGE],
    )
    .expect("scope");
    conn.execute(
        "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by, accepted_at,
                                              rationale)
         VALUES(?1, 1, 'sam', '2026-09-22T09:00:00Z', '')",
        params![CHANGE],
    )
    .expect("acceptance");
    let doc = serde_json::json!({
        "id": VERSION, "taken_at": "2026-09-22T10:00:00Z", "root": root.to_string_lossy(),
        "git": null,
        "tree": { "staged": [{"path": "checkout/totals.py", "sha256": "aaa", "bytes": 1}],
                  "unstaged": [], "untracked": [], "deleted": [], "dirty_diff_sha256": null },
        "environment": { "os": "test", "arch": "test", "git_version": null,
                         "env_names": [], "lockfiles": {} },
        "processes": { "ps_sha256": null, "ps_lines": 0, "ports": null, "task_ids": null,
                       "log_cursors": null, "gaps": "none" },
        "complete": true, "note": null,
    });
    let text = serde_json::to_string(&doc).expect("snapshot doc");
    conn.execute(
        "INSERT INTO workspace_snapshots(id, taken_at, root, head, branch, complete, doc, doc_sha256)
         VALUES(?1, '2026-09-22T10:00:00Z', ?2, NULL, NULL, 1, ?3, ?4)",
        params![VERSION, root.to_string_lossy(), text, sha256_hex(text.as_bytes())],
    )
    .expect("snapshot");
    conn.execute(
        "INSERT INTO change_implementations(change_id, binding_version, bound_at,
                                            repository_identity, base_revision, snapshot_id,
                                            workspace_identity)
         VALUES(?1, 1, '2026-09-22T10:00:00Z', 'shop', 'base0000', ?2, NULL)",
        params![CHANGE, VERSION],
    )
    .expect("implementation");

    let transcript = dir.path().join("transcript.jsonl");
    let lines = [
        "{\"type\":\"user\",\"n\":1}",
        "",
        "{\"type\":\"assistant\",\"n\":3}",
    ];
    std::fs::write(&transcript, lines.join("\n") + "\n").expect("transcript");
    conn.execute(
        "INSERT INTO sessions(id, project_id, file_path, started_at)
         VALUES(?1, ?2, ?3, '2026-09-22T09:30:00Z')",
        params![SESSION, PROJECT, transcript.to_string_lossy()],
    )
    .expect("session");
    for (seq, line) in [(1_i64, 1_i64), (2, 3)] {
        let raw = lines[(line - 1) as usize];
        conn.execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, raw_file, raw_line,
                                raw_sha256, prev_hash, chain_hash)
             VALUES(?1, ?2, ?3, ?4, '2026-09-22T09:31:00Z', 'user_msg', ?5, ?6, ?7, '', '')",
            params![
                format!("ev-{seq}"),
                PROJECT,
                SESSION,
                seq,
                transcript.to_string_lossy(),
                line,
                sha256_hex(raw.as_bytes())
            ],
        )
        .expect("event");
    }
    conn.execute(
        "INSERT INTO change_session_associations(id, change_id, session_id, provenance, basis,
                                                 actor, asserted_at, reason)
         VALUES('as1', ?1, ?2, 'confirmed', 'user', 'sam', '2026-09-22T09:40:00Z', NULL)",
        params![CHANGE, SESSION],
    )
    .expect("association");
    Store { dir, ledger }
}

fn approve(store: &Store) -> PathBuf {
    let outcome = change_decision::approve(
        &store.ledger,
        CHANGE,
        &Decide {
            actor: "sam".into(),
            at: "2026-09-22T11:00:00Z".into(),
            seen_revision: VERSION.into(),
            note: None,
        },
    )
    .expect("approve");
    assert!(matches!(outcome, change_decision::Outcome::Recorded { .. }));
    let file = change_decision::record_path_for(&store.root(), VERSION);
    assert!(file.is_file(), "the approval wrote its copy");
    file
}

fn read(store: &Store) -> change_export::ExportRecords {
    change_export::records(&store.ledger, CHANGE, AT)
        .expect("records")
        .expect("the Change exists")
}

#[cfg(unix)]
fn chmod(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("chmod");
}

/// A test that `chmod 000`s a file is meaningless as root, who reads anything.
#[cfg(unix)]
fn permissions_bite(path: &Path) -> bool {
    std::fs::File::open(path).is_err()
}

/* ── the whole record, readable ────────────────────────────────────────── */

#[test]
fn every_record_readable_says_so_and_counts_itself() {
    let store = store();
    approve(&store);
    let r = read(&store);
    assert_eq!(
        r.schema_version,
        change_export::CHANGE_EXPORT_SCHEMA_VERSION
    );
    assert_eq!(r.change_id, CHANGE);
    assert_eq!(r.checked_at, AT);
    assert_eq!(
        r.worktree.as_deref(),
        Some(store.root().to_string_lossy().as_ref()),
        "the recorded folder is served, so a copy can leave it out"
    );
    assert_eq!(r.decisions.len(), 1);
    assert_eq!(r.decisions[0].standing, Some(Standing::Readable));
    assert_eq!(r.decisions[0].version, VERSION);
    assert_eq!(r.transcripts.len(), 1);
    assert_eq!(r.transcripts[0].lines, 2, "two events name two lines");
    assert_eq!(r.transcripts[0].standing, Standing::Readable);
    assert_eq!(r.totals.records, 2);
    assert_eq!(r.totals.readable, 2);
    assert_eq!(r.totals.not_readable, 0);
}

#[test]
fn an_unknown_change_is_none_not_an_empty_all_clear() {
    let store = store();
    assert!(change_export::records(&store.ledger, "nope", AT)
        .expect("records")
        .is_none());
}

/* ── decisions ─────────────────────────────────────────────────────────── */

#[cfg(unix)]
#[test]
fn a_decision_copy_this_machine_will_not_open_is_permission_denied() {
    let store = store();
    let file = approve(&store);
    chmod(&file, 0o000);
    if !permissions_bite(&file) {
        chmod(&file, 0o644);
        eprintln!("running as a user chmod does not stop; nothing to observe");
        return;
    }
    let r = read(&store);
    chmod(&file, 0o644);
    assert_eq!(r.decisions[0].standing, Some(Standing::PermissionDenied));
    assert_eq!(r.totals.not_readable, 1);
}

#[test]
fn a_deleted_decision_copy_is_missing() {
    let store = store();
    let file = approve(&store);
    std::fs::remove_file(&file).expect("delete");
    assert_eq!(read(&store).decisions[0].standing, Some(Standing::Missing));
}

#[test]
fn a_rewritten_decision_copy_is_content_changed() {
    let store = store();
    let file = approve(&store);
    let text = std::fs::read_to_string(&file).expect("read");
    std::fs::write(&file, text.replace("\"sam\"", "\"mallory\"")).expect("rewrite");
    assert_eq!(
        read(&store).decisions[0].standing,
        Some(Standing::ContentChanged)
    );
    std::fs::write(&file, "not json").expect("rewrite");
    assert_eq!(
        read(&store).decisions[0].standing,
        Some(Standing::ContentChanged)
    );
}

/* ── DECISION-HISTORY-SHAPE-R3: the whole document, exceptions included ──
 *
 * Found by the round-2 adversary (its test, kept under its own name): the
 * copy-standing check compared six scalar fields and never `exceptions`,
 * the array `write_record` also writes and the one that says what an
 * approval was given UNDER (B7.20/B7.22/B10.9). A rewritten rationale, or an
 * emptied array, read as an unchanged copy. */

/// Give the one approval row an exception, and rewrite its copy to what
/// `write_record` would have written for that row, so file and row agree in
/// full before anything is tampered with.
fn approve_with_one_exception(store: &Store) -> PathBuf {
    let file = approve(store);
    let decision_id: String = store
        .ledger
        .conn()
        .query_row(
            "SELECT id FROM change_decisions WHERE change_id=?1",
            params![CHANGE],
            |r| r.get(0),
        )
        .expect("the approval's row id");
    store
        .ledger
        .conn()
        .execute(
            "INSERT INTO change_obligation_waivers(id, change_id, obligation_id, kind, actor,
                                                    authority, rationale, recorded_at)
             VALUES('wv1', ?1, 'criterion:1', 'waived', 'sam', 'accepted-profile',
                    'the payment sandbox is down all week', '2026-09-22T10:55:00Z')",
            params![CHANGE],
        )
        .expect("waiver");
    store
        .ledger
        .conn()
        .execute(
            "INSERT INTO change_decision_exceptions(id, decision_id, waiver_id, obligation_id,
                                                     kind, actor, authority, rationale)
             VALUES('ce1', ?1, 'wv1', 'criterion:1', 'waived', 'sam', 'accepted-profile',
                    'the payment sandbox is down all week')",
            params![decision_id],
        )
        .expect("exception row");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).expect("read")).expect("json");
    doc["act"] = serde_json::json!("approved_with_exceptions");
    doc["exceptions"] = serde_json::json!([{
        "waiver_id": "wv1",
        "obligation_id": "criterion:1",
        "kind": "waived",
        "actor": "sam",
        "authority": "accepted-profile",
        "rationale": "the payment sandbox is down all week",
    }]);
    std::fs::write(&file, serde_json::to_vec_pretty(&doc).expect("json")).expect("rewrite");
    assert_eq!(
        read(store).decisions[0].standing,
        Some(Standing::Readable),
        "baseline: file and row agree on the exception"
    );
    file
}

fn edit(file: &Path, change: impl FnOnce(&mut serde_json::Value)) {
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(file).expect("read")).expect("json");
    change(&mut doc);
    std::fs::write(file, serde_json::to_vec_pretty(&doc).expect("json")).expect("rewrite");
}

#[test]
fn adversary_a_tampered_exceptions_array_reads_as_unchanged() {
    let store = store();
    let file = approve_with_one_exception(&store);

    edit(&file, |doc| {
        doc["exceptions"][0]["rationale"] =
            serde_json::json!("actually the sandbox works fine, ignore this")
    });
    assert_eq!(
        read(&store).decisions[0].standing,
        Some(Standing::ContentChanged),
        "a rewritten exception rationale is a changed copy"
    );

    edit(&file, |doc| doc["exceptions"] = serde_json::json!([]));
    assert_eq!(
        read(&store).decisions[0].standing,
        Some(Standing::ContentChanged),
        "an emptied exceptions array is a changed copy"
    );
}

/// Every value the file holds is compared, not a hand-picked list: the
/// fields the six-field rule never read (`schema_version`, `note`, an added
/// key, a removed key) each make the copy changed.
#[test]
fn any_value_difference_anywhere_in_the_copy_is_content_changed() {
    type Edit = Box<dyn Fn(&mut serde_json::Value)>;
    let edits: Vec<(&str, Edit)> = vec![
        (
            "schema_version",
            Box::new(|d| d["schema_version"] = serde_json::json!(99)),
        ),
        (
            "note",
            Box::new(|d| d["note"] = serde_json::json!("This merges.")),
        ),
        (
            "an added key",
            Box::new(|d| d["merged"] = serde_json::json!(true)),
        ),
        (
            "a removed key",
            Box::new(|d| {
                d.as_object_mut().expect("object").remove("note");
            }),
        ),
        (
            "an exception's authority",
            Box::new(|d| d["exceptions"][0]["authority"] = serde_json::json!("nobody")),
        ),
        (
            "an added exception",
            Box::new(|d| {
                let first = d["exceptions"][0].clone();
                d["exceptions"].as_array_mut().expect("array").push(first);
            }),
        ),
    ];
    for (what, change) in edits {
        let store = store();
        let file = approve_with_one_exception(&store);
        edit(&file, |doc| change(doc));
        assert_eq!(
            read(&store).decisions[0].standing,
            Some(Standing::ContentChanged),
            "{what} changed and the copy still read as unchanged"
        );
    }
}

/// Whitespace and key order are how a file is laid out, not what it says.
#[test]
fn a_copy_reformatted_but_saying_the_same_thing_is_still_readable() {
    let store = store();
    let file = approve_with_one_exception(&store);
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).expect("read")).expect("json");
    let mut keys: Vec<(String, serde_json::Value)> = doc
        .as_object()
        .expect("object")
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    keys.reverse();
    let body = keys
        .iter()
        .map(|(k, v)| format!("\n\t {} :\n {}", serde_json::to_string(k).unwrap(), v))
        .collect::<Vec<_>>()
        .join(" ,");
    std::fs::write(&file, format!("  {{{body}\n}}\n\n")).expect("rewrite");
    assert_eq!(
        read(&store).decisions[0].standing,
        Some(Standing::Readable),
        "a reformatted copy with every value the same is unchanged"
    );
}

#[test]
fn a_withdrawal_has_no_copy_and_is_not_counted_as_readable() {
    let store = store();
    approve(&store);
    change_decision::withdraw(
        &store.ledger,
        CHANGE,
        &Decide {
            actor: "sam".into(),
            at: "2026-09-22T11:30:00Z".into(),
            seen_revision: VERSION.into(),
            note: None,
        },
    )
    .expect("withdraw");
    let r = read(&store);
    assert_eq!(r.decisions.len(), 2);
    assert_eq!(r.decisions[1].act, "withdrawn");
    assert_eq!(
        r.decisions[1].standing, None,
        "no copy by design, and no pass for it"
    );
    assert_eq!(r.decisions[1].path, None);
    assert_eq!(
        r.totals.records, 2,
        "the approval copy and the transcript, nothing for the withdrawal"
    );
}

/* ── evidence a link names ─────────────────────────────────────────────── */

#[test]
fn a_link_to_a_run_the_ledger_does_not_hold_is_a_dangling_reference() {
    let store = store();
    store
        .ledger
        .conn()
        .execute(
            "INSERT INTO change_criterion_evidence(id, change_id, scope_version, criterion_index,
                 evidence_kind, evidence_id, session_id, basis, detail, actor, asserted_at, reason)
             VALUES('l1', ?1, 1, 1, 'check_run', 'evb-gone', ?2, 'manual', NULL, 'sam',
                    '2026-09-22T10:30:00Z', NULL)",
            params![CHANGE, SESSION],
        )
        .expect("a link to nothing");
    let r = read(&store);
    assert_eq!(r.evidence.len(), 1);
    assert_eq!(r.evidence[0].evidence_id, "evb-gone");
    assert_eq!(r.evidence[0].standing, Standing::DanglingReference);
    assert_eq!(r.evidence[0].name, None);
    assert_eq!(
        r.evidence[0].criteria,
        vec![1],
        "the gap belongs on the line that links it"
    );
    assert_eq!(r.evidence[0].execution_recorded, None);
    assert_eq!(r.totals.not_readable, 1);
}

#[test]
fn a_link_taken_away_is_not_a_record_the_copy_rests_on() {
    let store = store();
    for (id, basis, at) in [
        ("l1", "manual", "2026-09-22T10:30:00Z"),
        ("l2", "not-bearing", "2026-09-22T10:40:00Z"),
    ] {
        store
            .ledger
            .conn()
            .execute(
                "INSERT INTO change_criterion_evidence(id, change_id, scope_version, criterion_index,
                     evidence_kind, evidence_id, session_id, basis, detail, actor, asserted_at, reason)
                 VALUES(?1, ?2, 1, 1, 'check_run', 'evb-gone', ?3, ?4, NULL, 'sam', ?5, NULL)",
                params![id, CHANGE, SESSION, basis, at],
            )
            .expect("link");
    }
    assert!(read(&store).evidence.is_empty());
}

/* ── the conversation lines ────────────────────────────────────────────── */

#[test]
fn a_rewritten_transcript_line_is_content_changed_and_counted() {
    let store = store();
    let text = std::fs::read_to_string(store.transcript()).expect("read");
    std::fs::write(store.transcript(), text.replace("\"n\":3", "\"n\":4")).expect("rewrite");
    let t = &read(&store).transcripts[0];
    assert_eq!(t.standing, Standing::ContentChanged);
    assert_eq!((t.lines_changed, t.lines_gone), (1, 0));
}

#[test]
fn a_truncated_transcript_is_content_changed_with_the_lines_gone() {
    let store = store();
    std::fs::write(store.transcript(), "{\"type\":\"user\",\"n\":1}\n").expect("truncate");
    let t = &read(&store).transcripts[0];
    assert_eq!(t.standing, Standing::ContentChanged);
    assert_eq!((t.lines_changed, t.lines_gone), (0, 1));
}

#[test]
fn a_removed_transcript_is_missing() {
    let store = store();
    std::fs::remove_file(store.transcript()).expect("remove");
    assert_eq!(read(&store).transcripts[0].standing, Standing::Missing);
}

#[cfg(unix)]
#[test]
fn a_locked_transcript_is_permission_denied() {
    let store = store();
    chmod(&store.transcript(), 0o000);
    if !permissions_bite(&store.transcript()) {
        chmod(&store.transcript(), 0o644);
        return;
    }
    let r = read(&store);
    chmod(&store.transcript(), 0o644);
    assert_eq!(r.transcripts[0].standing, Standing::PermissionDenied);
}

#[test]
fn only_confirmed_work_is_looked_at_and_the_latest_word_wins() {
    let store = store();
    store
        .ledger
        .conn()
        .execute(
            "INSERT INTO change_session_associations(id, change_id, session_id, provenance, basis,
                                                     actor, asserted_at, reason)
             VALUES('as2', ?1, ?2, 'rejected', 'user', 'sam', '2026-09-22T09:50:00Z', 'not it')",
            params![CHANGE, SESSION],
        )
        .expect("reject");
    assert!(read(&store).transcripts.is_empty());
}

/* ── the digest rule is ingest's ───────────────────────────────────────── */

#[test]
fn the_restated_line_digest_matches_every_digest_ingest_wrote() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path().join("projects");
    let project = root.join("-Users-dev-demo");
    std::fs::create_dir_all(&project).expect("mkdir");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude_code_sessions.jsonl");
    let file = project.join("cccccccc-0000-4000-8000-000000000001.jsonl");
    std::fs::copy(&fixture, &file).expect("copy fixture");
    let mut ledger = Ledger::open(tmp.path().join("ledger.db")).expect("open");
    thruline_core::ingest::ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &tmp.path().join("mirrors"),
    )
    .expect("ingest");
    let mut statement = ledger
        .conn()
        .prepare("SELECT raw_file, raw_line, raw_sha256 FROM events")
        .expect("prepare");
    let rows: Vec<(String, i64, String)> = statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query")
        .collect::<Result<_, _>>()
        .expect("rows");
    assert!(
        rows.len() > 3,
        "the fixture ingested nothing to compare against"
    );
    let mut checked = 0;
    for (raw_file, line, digest) in rows {
        let disk = change_export::line_digests(Path::new(&raw_file)).expect("readable");
        assert_eq!(disk.get(&(line as u64)), Some(&digest), "{raw_file}:{line}");
        checked += 1;
    }
    assert!(checked > 3);
}

/* ── it writes nothing, and masks what it serves ───────────────────────── */

#[test]
fn looking_writes_nothing() {
    let store = store();
    approve(&store);
    let before = store.ledger.conn().total_changes();
    let listing = |p: &Path| {
        let mut v: Vec<_> = walk(p);
        v.sort();
        v
    };
    let files_before = listing(store.dir.path());
    read(&store);
    assert_eq!(
        store.ledger.conn().total_changes(),
        before,
        "a ledger row was written"
    );
    assert_eq!(
        listing(store.dir.path()),
        files_before,
        "a file was written"
    );
}

fn walk(p: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(p) {
        for e in entries.flatten() {
            let path = e.path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else if !path.to_string_lossy().ends_with("-wal")
                && !path.to_string_lossy().ends_with("-shm")
            {
                let bytes = std::fs::read(&path).map(|b| b.len() as u64).unwrap_or(0);
                out.push((path.to_string_lossy().into_owned(), bytes));
            }
        }
    }
    out
}

#[test]
fn a_secret_shaped_path_is_masked_before_it_is_served_and_the_treatment_says_so() {
    let store = store();
    let secret_dir = store
        .dir
        .path()
        .join("ghp_0123456789abcdefghijABCDEFGHIJ012345");
    std::fs::create_dir_all(&secret_dir).expect("dir");
    let moved = secret_dir.join("t.jsonl");
    std::fs::copy(store.transcript(), &moved).expect("copy");
    store
        .ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, raw_file, raw_line,
                                raw_sha256, prev_hash, chain_hash)
             VALUES('ev-9', ?1, ?2, 9, '2026-09-22T09:39:00Z', 'user_msg', ?3, 1, 'x', '', '')",
            params![PROJECT, SESSION, moved.to_string_lossy()],
        )
        .expect("event");
    let r = read(&store);
    let served = serde_json::to_string(&r).expect("json");
    assert!(
        !served.contains("ghp_0123456789abcdefghijABCDEFGHIJ012345"),
        "{served}"
    );
    assert!(
        r.masking.masked() >= 1,
        "the treatment does not say a mask happened"
    );
}

/* ── DECISION-FILE-AFTER-WITHDRAW: the copy follows the latest act ──────── */

fn withdraw(store: &Store) -> thruline_core::error::Result<change_decision::Outcome> {
    change_decision::withdraw(
        &store.ledger,
        CHANGE,
        &Decide {
            actor: "sam".into(),
            at: "2026-09-22T11:30:00Z".into(),
            seen_revision: VERSION.into(),
            note: None,
        },
    )
}

fn decision_rows(store: &Store) -> i64 {
    store
        .ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM change_decisions", [], |r| r.get(0))
        .expect("count")
}

fn decisions_folder(store: &Store) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(store.root().join(".thruline/decisions"))
        .expect("the decisions folder")
        .map(|e| e.expect("entry").file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// Card item 2. After a withdrawal the export reads the approval's file as
/// holding something other than that approval — `content_changed`, counted as
/// not readable — and the file it reads is the withdrawal's own copy, naming
/// the withdrawal row. The withdrawal itself still has no copy of its own to
/// count. Nothing but `<version>.json` is left in the folder (no temporary).
#[test]
fn after_a_withdrawal_the_export_reads_the_replaced_copy_and_the_file_names_the_withdrawal() {
    let store = store();
    let file = approve(&store);
    let Ok(change_decision::Outcome::Recorded {
        decision: withdrawn,
    }) = withdraw(&store)
    else {
        panic!("withdraw");
    };
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&file).expect("read")).expect("json");
    assert_eq!(doc["act"], "withdrawn");
    assert_eq!(doc["decision_id"], withdrawn.id.as_str());
    assert_eq!(doc["derived_copy"], true);
    assert_eq!(
        doc,
        change_decision::record_document_for_row(CHANGE, &withdrawn),
        "the file is exactly the document built from the withdrawal row"
    );
    let r = read(&store);
    assert_eq!(r.decisions[0].act, "approved");
    assert_eq!(r.decisions[0].standing, Some(Standing::ContentChanged));
    assert_eq!(r.decisions[1].standing, None);
    assert_eq!(r.totals.not_readable, 1);
    assert_eq!(decisions_folder(&store), vec![format!("{VERSION}.json")]);
}

/// An approval copy written by an older release (schema 2: no `derived_copy`,
/// no `decision_id`, no `withdraws`) is still read as unchanged after the
/// upgrade, and an edit to it still reads as changed.
#[test]
fn a_schema_2_copy_written_before_the_upgrade_still_reads_unchanged() {
    let store = store();
    let file = approve(&store);
    let row = change_decision::read(&store.ledger, CHANGE)
        .expect("read")
        .history
        .remove(0);
    let old = change_decision::record_document_v2_for_row(CHANGE, &row);
    assert_eq!(old["schema_version"], 2);
    assert!(old.get("derived_copy").is_none() && old.get("decision_id").is_none());
    std::fs::write(&file, serde_json::to_vec_pretty(&old).expect("json")).expect("write");
    assert_eq!(read(&store).decisions[0].standing, Some(Standing::Readable));

    let mut edited = old.clone();
    edited["actor"] = serde_json::json!("mallory");
    std::fs::write(&file, serde_json::to_vec_pretty(&edited).expect("json")).expect("write");
    assert_eq!(
        read(&store).decisions[0].standing,
        Some(Standing::ContentChanged)
    );

    /* A schema-3 copy relabelled as 2 keeps its extra fields, so it is not
     * what the older release wrote and reads as changed. */
    let mut relabelled = change_decision::record_document_for_row(CHANGE, &row);
    relabelled["schema_version"] = serde_json::json!(2);
    std::fs::write(&file, serde_json::to_vec_pretty(&relabelled).expect("json")).expect("write");
    assert_eq!(
        read(&store).decisions[0].standing,
        Some(Standing::ContentChanged)
    );
}

/// A copy that is already gone says nothing stale: the withdrawal is
/// recorded and no file is created in its place.
#[test]
fn a_withdrawal_whose_copy_is_gone_is_recorded_and_creates_nothing() {
    let store = store();
    let file = approve(&store);
    std::fs::remove_file(&file).expect("delete");
    assert!(matches!(
        withdraw(&store),
        Ok(change_decision::Outcome::Recorded { .. })
    ));
    assert!(!file.exists(), "a missing copy is not re-created");
    assert_eq!(decision_rows(&store), 2);
}

/// A copy that is there and cannot be replaced refuses the withdrawal, names
/// the file, records nothing, and leaves the file as it was.
#[cfg(unix)]
#[test]
fn a_withdrawal_whose_copy_cannot_be_replaced_is_refused_and_records_nothing() {
    let store = store();
    let file = approve(&store);
    let before = std::fs::read(&file).expect("read");
    let folder = store.root().join(".thruline/decisions");
    chmod(&folder, 0o555);
    let probe = folder.join("probe");
    if std::fs::write(&probe, b"x").is_ok() {
        let _ = std::fs::remove_file(&probe);
        chmod(&folder, 0o755);
        eprintln!("running as a user chmod does not stop; nothing to observe");
        return;
    }
    let refused = withdraw(&store);
    chmod(&folder, 0o755);
    let err = refused.expect_err("a copy that cannot be replaced refuses the act");
    assert!(err.to_string().contains(&*file.to_string_lossy()), "{err}");
    assert_eq!(decision_rows(&store), 1, "nothing was recorded");
    assert_eq!(std::fs::read(&file).expect("read"), before);
    assert!(change_decision::read(&store.ledger, CHANGE)
        .expect("read")
        .standing
        .is_some());
}

/// A decisions folder that is a symlink is never written through: the
/// approval is recorded without a copy, and the folder it points at stays
/// empty.
#[cfg(unix)]
#[test]
fn a_symlinked_decisions_folder_is_not_written_through() {
    let store = store();
    let elsewhere = store.dir.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).expect("elsewhere");
    std::fs::create_dir_all(store.root().join(".thruline")).expect(".thruline");
    std::os::unix::fs::symlink(&elsewhere, store.root().join(".thruline/decisions"))
        .expect("symlink");
    let outcome = change_decision::approve(
        &store.ledger,
        CHANGE,
        &Decide {
            actor: "sam".into(),
            at: "2026-09-22T11:00:00Z".into(),
            seen_revision: VERSION.into(),
            note: None,
        },
    )
    .expect("approve");
    let change_decision::Outcome::Recorded { decision } = outcome else {
        panic!("approve");
    };
    assert_eq!(decision.record_path, None);
    assert_eq!(std::fs::read_dir(&elsewhere).expect("dir").count(), 0);
}
