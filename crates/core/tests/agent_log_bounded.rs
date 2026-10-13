//! AGENT-LOG-LOCATOR-BOUNDED — looking for where the agent said it never waits
//! on, and never re-reads, a log.
//!
//! The Change route asks `agent_log` for the line of each linked conversation's
//! final claim on every request. It used to read the whole transcript each time,
//! from whatever the recorded path named: a named pipe made the request wait
//! forever, and a very large log cost its whole read on every request. Here:
//!
//! * a path that is not an ordinary file (a named pipe, a socket, a directory,
//!   a device) is not located, says so, and is never read;
//! * a log is read when it is first looked at and again only when the file
//!   changed (device, inode, size, modification and change time) or the ledger
//!   took in more of the conversation; looking again at a log nobody touched
//!   reads nothing;
//! * none of that moves the receipt: the same file and line, from the same
//!   lines, the same `log_changed` and `no_agent_claim` answers.
//!
//! Every transcript is written by the test and ingested by the real `ingest`.
//! Every id is synthetic (`cccccccc-` prefix); no fixture holds private content.

use std::collections::BTreeSet;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::sync::{Arc, Barrier};
use std::time::{Duration, SystemTime};

use rusqlite::params;
use thruline_core::agent_log::{self, NotLocated};
use thruline_core::ledger::Ledger;

const SESSION: &str = "cccccccc-0000-4000-8000-00000000b0d1";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000b0c1";
const WORDS: &str = "Done. The cart total is added and the tests pass.";

struct Store {
    dir: tempfile::TempDir,
    ledger: Ledger,
}

impl Store {
    fn root(&self) -> PathBuf {
        self.dir.path().join("projects")
    }

    fn transcript(&self) -> PathBuf {
        self.root()
            .join("-synthetic-shop")
            .join(format!("{SESSION}.jsonl"))
    }

    fn ingest_again(&mut self) {
        let root = self.root();
        thruline_core::ingest::ingest(
            &mut self.ledger,
            std::slice::from_ref(&root),
            &self.dir.path().join("mirrors"),
        )
        .expect("ingest");
    }
}

fn line(n: u32, kind: &str, role: &str, content: serde_json::Value) -> String {
    serde_json::json!({
        "type": kind, "uuid": format!("cccccccc-0000-4000-8000-0000000000{n:02}"),
        "isSidechain": false, "sessionId": SESSION, "cwd": "/synthetic/shop",
        "timestamp": format!("2026-09-23T09:00:{n:02}.000Z"),
        "message": { "role": role, "content": content },
    })
    .to_string()
}

fn user(n: u32, text: &str) -> String {
    line(n, "user", "user", serde_json::json!(text))
}

fn said(n: u32, text: &str) -> String {
    line(
        n,
        "assistant",
        "assistant",
        serde_json::json!([{ "type": "text", "text": text }]),
    )
}

/// The walk's conversation: the final message is on line 4.
fn walk(words: &str) -> Vec<String> {
    vec![
        user(1, "Add a cart total function."),
        line(
            2,
            "assistant",
            "assistant",
            serde_json::json!([{ "type": "tool_use", "id": "toolu_b1", "name": "Edit",
                "input": { "file_path": "/synthetic/shop/src/lib.rs", "old_string": "x", "new_string": "y" } }]),
        ),
        line(
            3,
            "user",
            "user",
            serde_json::json!([{ "type": "tool_result", "tool_use_id": "toolu_b1",
                "content": "The file has been updated." }]),
        ),
        said(4, words),
    ]
}

fn store_with(lines: &[String]) -> Store {
    let dir = tempfile::tempdir().expect("tempdir");
    let project = dir.path().join("projects").join("-synthetic-shop");
    std::fs::create_dir_all(&project).expect("mkdir");
    std::fs::write(
        project.join(format!("{SESSION}.jsonl")),
        lines.join("\n") + "\n",
    )
    .expect("transcript");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    thruline_core::ingest::ingest(
        &mut ledger,
        &[dir.path().join("projects")],
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    let project_id: String = ledger
        .conn()
        .query_row(
            "SELECT project_id FROM events WHERE session_id=?1 LIMIT 1",
            params![SESSION],
            |r| r.get(0),
        )
        .expect("the conversation was ingested");
    ledger
        .conn()
        .execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, request_event_id,
                                 requester, request_source)
             VALUES(?1, ?2, '2026-09-23T08:00:00Z', 'Cart total', 'Add a cart total.', NULL,
                    'sam', 'person')",
            params![CHANGE, project_id],
        )
        .expect("change");
    ledger
        .conn()
        .execute(
            "INSERT INTO change_session_associations(id, change_id, session_id, provenance,
                                                     basis, actor, asserted_at, reason)
             VALUES('cccccccc-0000-4000-8000-00000000b0a1', ?1, ?2, 'confirmed', 'user', 'sam',
                    '2026-09-23T09:40:00Z', NULL)",
            params![CHANGE, SESSION],
        )
        .expect("association");
    Store { dir, ledger }
}

fn view(store: &Store) -> agent_log::AgentLogView {
    let change = thruline_core::change::read_change(&store.ledger, CHANGE)
        .expect("read")
        .expect("the Change");
    agent_log::for_change(&store.ledger, &change).expect("agent_log")
}

fn entry(view: &agent_log::AgentLogView) -> &agent_log::AgentLogEntry {
    assert_eq!(view.sessions.len(), 1, "{view:?}");
    &view.sessions[0]
}

/// `view`, on a thread, with a bound. A locator that opens a named pipe for
/// reading never returns; this turns that into a failure with a reason instead
/// of a test run that never ends.
fn view_within_bound(store: Store) -> (Store, agent_log::AgentLogView) {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let v = view(&store);
        let _ = tx.send((store, v));
    });
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(answer) => answer,
        Err(_) => panic!(
            "the locator did not answer within 20 s: it is opening, and waiting on, a log \
             that is not an ordinary file"
        ),
    }
}

/// Replace the transcript with something that is not an ordinary file, then
/// look for the place. Nothing may wait, nothing may be read, and the answer is
/// a reason, never a place.
fn assert_not_located_and_not_read(store: Store) {
    let log = store.transcript();
    let (store, v) = view_within_bound(store);
    let e = entry(&v);
    assert_eq!(e.receipt, None, "{e:?}");
    assert_eq!(e.not_located, Some(NotLocated::LogUnreadable), "{e:?}");
    assert_eq!(
        agent_log::reads_of(&log),
        0,
        "the locator read from a path that is not an ordinary file"
    );
    drop(store);
}

/* ── a path that is not an ordinary file is never read ───────────────── */

#[test]
fn a_named_pipe_in_place_of_the_log_is_not_located_and_nothing_waits() {
    let store = store_with(&walk(WORDS));
    let log = store.transcript();
    std::fs::remove_file(&log).expect("remove");
    let made = Command::new("mkfifo").arg(&log).status().expect("mkfifo");
    assert!(made.success(), "mkfifo failed");
    assert_not_located_and_not_read(store);
}

#[test]
fn a_socket_in_place_of_the_log_is_not_located_and_nothing_is_read() {
    let store = store_with(&walk(WORDS));
    let log = store.transcript();
    std::fs::remove_file(&log).expect("remove");
    // A socket's name has to be short; the transcript's is not, so the socket
    // is made under a short name and moved into place.
    let short = tempfile::Builder::new()
        .prefix("s")
        .tempdir()
        .expect("short dir");
    let at = short.path().join("s");
    let listener = std::os::unix::net::UnixListener::bind(&at).unwrap_or_else(|e| {
        panic!(
            "binding a socket at {}: {e} (is TMPDIR too long for a socket's name?)",
            at.display()
        )
    });
    std::fs::rename(&at, &log).expect("move the socket into place");
    assert_not_located_and_not_read(store);
    drop(listener);
}

#[test]
fn a_directory_in_place_of_the_log_is_not_located_and_nothing_is_read() {
    let store = store_with(&walk(WORDS));
    let log = store.transcript();
    std::fs::remove_file(&log).expect("remove");
    std::fs::create_dir(&log).expect("mkdir");
    assert_not_located_and_not_read(store);
}

#[test]
fn a_device_in_place_of_the_log_is_not_located_and_nothing_is_read() {
    let store = store_with(&walk(WORDS));
    let log = store.transcript();
    std::fs::remove_file(&log).expect("remove");
    // `/dev/null` is a character device and always empty, so looking at it
    // cannot wedge a run that still opens it. The answer is a reason, not a
    // log that "changed": it is not a log.
    std::os::unix::fs::symlink("/dev/null", &log).expect("symlink");
    assert_not_located_and_not_read(store);
}

#[test]
fn a_symlink_to_an_ordinary_log_is_still_located() {
    // The refusal is about what the path names, not about links: a log kept
    // behind a link is the log.
    let store = store_with(&walk(WORDS));
    let log = store.transcript();
    let moved = store.dir.path().join("moved.jsonl");
    std::fs::rename(&log, &moved).expect("move");
    std::os::unix::fs::symlink(&moved, &log).expect("symlink");
    let v = view(&store);
    assert_eq!(
        entry(&v).receipt.as_ref().expect("a place").line,
        4,
        "{v:?}"
    );
}

/* ── a log nobody changed is read once ────────────────────────────────── */

#[test]
fn a_log_nobody_changed_is_read_once_however_often_it_is_looked_at() {
    let store = store_with(&walk(WORDS));
    let log = store.transcript();
    let first = view(&store);
    assert_eq!(entry(&first).receipt.as_ref().expect("a place").line, 4);
    assert_eq!(agent_log::reads_of(&log), 1);
    for _ in 0..5 {
        assert_eq!(view(&store), first, "the answer moved with nothing changed");
    }
    assert_eq!(
        agent_log::reads_of(&log),
        1,
        "an unchanged log was read again"
    );
}

#[test]
fn simultaneous_cold_looks_with_independent_ledgers_read_one_unchanged_log_once() {
    let store = store_with(&walk(WORDS));
    let log = store.transcript();

    // Grow the already-ingested log without adding a held line. The read and
    // parse last long enough for every caller released at the barrier to
    // encounter the same cold computation, while the receipt stays on line 4.
    let mut out = File::options().append(true).open(&log).expect("open log");
    let block = vec![b' '; 1024 * 1024];
    for _ in 0..128 {
        out.write_all(&block).expect("grow synthetic log");
    }
    out.write_all(b"\n").expect("end whitespace line");
    drop(out);

    assert_eq!(agent_log::reads_of(&log), 0, "fresh path was already read");
    let ledgers: Vec<_> = (0..4)
        .map(|_| Ledger::open(store.dir.path().join("ledger.db")).expect("independent ledger"))
        .collect();
    let barrier = Arc::new(Barrier::new(ledgers.len() + 1));
    let threads: Vec<_> = ledgers
        .into_iter()
        .map(|ledger| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let change = thruline_core::change::read_change(&ledger, CHANGE)
                    .expect("read Change")
                    .expect("Change");
                barrier.wait();
                agent_log::for_change(&ledger, &change).expect("agent log")
            })
        })
        .collect();
    barrier.wait();
    for thread in threads {
        let answer = thread.join().expect("caller finished");
        let at = entry(&answer);
        assert_eq!(at.receipt.as_ref().expect("held receipt").line, 4);
        assert_eq!(at.not_located, None);
    }
    let reads = agent_log::reads_of(&log);
    eprintln!(
        "cold fan-out: callers=4 bytes={} reads={reads}",
        std::fs::metadata(&log).expect("log metadata").len()
    );
    assert_eq!(reads, 1, "one unchanged log was read more than once");
}

fn append_large_unheld_line(log: &Path, mib: usize) {
    let mut out = File::options().append(true).open(log).expect("open log");
    let block = vec![b' '; 1024 * 1024];
    for _ in 0..mib {
        out.write_all(&block).expect("grow synthetic log");
    }
    out.write_all(b"\n").expect("end whitespace line");
}

fn simultaneous_looks(store: &Store, callers: usize) -> Vec<agent_log::AgentLogView> {
    let ledgers: Vec<_> = (0..callers)
        .map(|_| Ledger::open(store.dir.path().join("ledger.db")).expect("independent ledger"))
        .collect();
    let barrier = Arc::new(Barrier::new(callers + 1));
    let threads: Vec<_> = ledgers
        .into_iter()
        .map(|ledger| {
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                let change = thruline_core::change::read_change(&ledger, CHANGE)
                    .expect("read Change")
                    .expect("Change");
                barrier.wait();
                agent_log::for_change(&ledger, &change).expect("agent log")
            })
        })
        .collect();
    barrier.wait();
    threads
        .into_iter()
        .map(|thread| thread.join().expect("caller finished"))
        .collect()
}

fn assert_held_receipts(answers: &[agent_log::AgentLogView]) {
    let first = &answers[0];
    for answer in answers {
        assert_eq!(answer, first, "simultaneous answers differed");
        let at = entry(answer);
        assert_eq!(at.receipt.as_ref().expect("held receipt").line, 4);
        assert_eq!(at.not_located, None);
    }
}

#[test]
fn cold_fanouts_from_two_through_thirty_two_each_read_once_on_fresh_paths() {
    // The four-way case occurs four times, each with a fresh path and memo
    // miss. Each fixture is dropped before the next so the largest fan-out
    // does not stack large files on disk.
    let mut paths = BTreeSet::new();
    for callers in [2, 4, 8, 16, 32, 4, 4, 4] {
        let store = store_with(&walk(WORDS));
        let log = store.transcript();
        assert!(paths.insert(log.clone()), "fixture path was reused");
        append_large_unheld_line(&log, 64);
        assert_eq!(agent_log::reads_of(&log), 0, "not a cold look");
        let answers = simultaneous_looks(&store, callers);
        assert_held_receipts(&answers);
        let reads = agent_log::reads_of(&log);
        eprintln!(
            "fresh cold fan-out: callers={callers} bytes={} reads={reads}",
            std::fs::metadata(&log).expect("log metadata").len()
        );
        assert_eq!(reads, 1, "{callers} callers read the unchanged log twice");
        assert_eq!(view(&store), answers[0], "a warm answer changed");
        assert_eq!(agent_log::reads_of(&log), 1, "a warm look read again");
    }
    assert_eq!(paths.len(), 8);
}

#[test]
fn changed_metadata_causes_one_refresh_under_concurrent_looks() {
    let store = store_with(&walk(WORDS));
    let log = store.transcript();
    append_large_unheld_line(&log, 64);
    let first = simultaneous_looks(&store, 8);
    assert_held_receipts(&first);
    assert_eq!(agent_log::reads_of(&log), 1);

    File::options()
        .write(true)
        .open(&log)
        .expect("open")
        .set_modified(SystemTime::now() + Duration::from_secs(3600))
        .expect("change metadata");
    let refreshed = simultaneous_looks(&store, 8);
    assert_held_receipts(&refreshed);
    assert_eq!(refreshed, first, "mtime alone changed the held receipt");
    assert_eq!(agent_log::reads_of(&log), 2, "metadata change read twice");
    assert_eq!(view(&store), first[0]);
    assert_eq!(agent_log::reads_of(&log), 2, "warm refresh read again");
}

#[test]
fn unreadable_log_does_not_poison_its_path_or_another_valid_log() {
    use std::os::unix::fs::PermissionsExt;

    let failing = store_with(&walk(WORDS));
    let valid = store_with(&walk(WORDS));
    let failing_log = failing.transcript();
    let valid_log = valid.transcript();
    assert_ne!(failing_log, valid_log);

    std::fs::set_permissions(&failing_log, std::fs::Permissions::from_mode(0o000))
        .expect("make log unreadable");
    let unreadable = simultaneous_looks(&failing, 4);
    for answer in &unreadable {
        let at = entry(answer);
        assert_eq!(at.receipt, None);
        assert_eq!(at.not_located, Some(NotLocated::LogUnreadable));
    }
    assert_eq!(agent_log::reads_of(&failing_log), 0);

    let good = simultaneous_looks(&valid, 4);
    assert_held_receipts(&good);
    assert_eq!(agent_log::reads_of(&valid_log), 1);

    std::fs::set_permissions(&failing_log, std::fs::Permissions::from_mode(0o600))
        .expect("restore readable log");
    let recovered = simultaneous_looks(&failing, 4);
    assert_held_receipts(&recovered);
    assert_eq!(agent_log::reads_of(&failing_log), 1);
    assert_eq!(view(&valid), good[0]);
    assert_eq!(agent_log::reads_of(&valid_log), 1);
}

#[test]
fn an_answer_with_no_place_is_remembered_too() {
    // A log whose final turn holds no claim is also a fact worked out from one
    // read; looking again does not re-derive it.
    let store = store_with(&[user(1, "Add a cart total function."), said(2, "Ok.")]);
    let log = store.transcript();
    let first = view(&store);
    assert_eq!(
        entry(&first).not_located,
        Some(NotLocated::NoAgentClaim),
        "{first:?}"
    );
    for _ in 0..3 {
        assert_eq!(view(&store), first);
    }
    assert_eq!(agent_log::reads_of(&log), 1);
}

/* ── what makes the memory stale, and is read again ───────────────────── */

#[test]
fn an_appended_log_is_read_again_and_its_unheld_lines_still_count_for_nothing() {
    let store = store_with(&walk(WORDS));
    let log = store.transcript();
    let first = view(&store);
    assert_eq!(agent_log::reads_of(&log), 1);

    // The agent goes on. ThruLine has not ingested the new lines, so the
    // ledger holds none of them and the place is where it was.
    let mut more = std::fs::read_to_string(&log).expect("read");
    more.push_str(&(said(5, "Shipped. Everything is deployed and the tests pass.") + "\n"));
    std::fs::write(&log, more).expect("append");
    let after = view(&store);
    assert_eq!(agent_log::reads_of(&log), 2, "a grown log was not re-read");
    assert_eq!(
        after, first,
        "a line the ledger does not hold moved the place"
    );
    assert_eq!(entry(&after).receipt.as_ref().expect("a place").line, 4);
}

#[test]
fn a_line_rewritten_after_the_log_was_remembered_is_not_located() {
    // `log_changed`'s rule still runs on the new bytes: the answer the memory
    // held is not served once the file is not the file it was.
    let store = store_with(&walk(WORDS));
    let log = store.transcript();
    let first = view(&store);
    assert!(entry(&first).receipt.is_some());

    let rewritten = walk("Done. Everything is finished and shipped.").join("\n") + "\n";
    std::fs::write(&log, rewritten).expect("rewrite");
    let after = view(&store);
    assert_eq!(entry(&after).receipt, None, "{after:?}");
    assert!(entry(&after).not_located.is_some(), "{after:?}");
    assert_eq!(agent_log::reads_of(&log), 2);
}

#[test]
fn a_log_whose_only_change_is_its_modification_time_is_read_again() {
    let store = store_with(&walk(WORDS));
    let log = store.transcript();
    let first = view(&store);
    assert_eq!(agent_log::reads_of(&log), 1);
    File::options()
        .write(true)
        .open(&log)
        .expect("open")
        .set_modified(SystemTime::now() + Duration::from_secs(3600))
        .expect("touch");
    assert_eq!(view(&store), first);
    assert_eq!(
        agent_log::reads_of(&log),
        2,
        "a touched log was not re-read"
    );
}

#[test]
fn a_log_whose_only_change_is_its_change_time_is_read_again() {
    // A program can put a modification time back; it cannot put the change
    // time back. A chmod moves the change time and nothing else.
    use std::os::unix::fs::PermissionsExt;
    let store = store_with(&walk(WORDS));
    let log = store.transcript();
    let first = view(&store);
    assert_eq!(agent_log::reads_of(&log), 1);
    let before = std::fs::metadata(&log).expect("stat");
    std::fs::set_permissions(&log, std::fs::Permissions::from_mode(0o600)).expect("chmod");
    let after = std::fs::metadata(&log).expect("stat");
    assert_eq!(after.len(), before.len());
    assert_eq!(
        after.modified().expect("mtime"),
        before.modified().expect("mtime"),
        "the test changed more than the change time"
    );
    assert_eq!(view(&store), first);
    assert_eq!(
        agent_log::reads_of(&log),
        2,
        "a log whose change time moved was not re-read"
    );
}

#[test]
fn a_log_replaced_by_a_copy_of_the_same_size_and_time_is_read_again() {
    // Same bytes, same size, same modification time: only the file is another
    // file. The memory is of a file, not of a name.
    let store = store_with(&walk(WORDS));
    let log = store.transcript();
    let first = view(&store);
    assert_eq!(agent_log::reads_of(&log), 1);
    let mtime = std::fs::metadata(&log)
        .expect("stat")
        .modified()
        .expect("mtime");
    let copy = store.dir.path().join("copy.jsonl");
    std::fs::copy(&log, &copy).expect("copy");
    File::options()
        .write(true)
        .open(&copy)
        .expect("open")
        .set_modified(mtime)
        .expect("same mtime");
    std::fs::rename(&copy, &log).expect("replace");
    assert_eq!(view(&store), first);
    assert_eq!(
        agent_log::reads_of(&log),
        2,
        "a different file at the same name was taken for the one remembered"
    );
}

#[test]
fn a_line_the_ledger_takes_in_later_is_read_again_though_the_file_did_not_change() {
    // The ledger is the other half of what the answer rests on. The log grows
    // and is looked at (its new lines are not held, so the place stays); then
    // ThruLine ingests them. The file has not changed since it was last read,
    // and the place still has to move.
    let mut store = store_with(&walk(WORDS));
    let log = store.transcript();
    let first = view(&store);
    assert_eq!(entry(&first).receipt.as_ref().expect("a place").line, 4);

    let mut more = std::fs::read_to_string(&log).expect("read");
    more.push_str(&(user(5, "Now the coupon field.") + "\n"));
    more.push_str(&(said(6, "The coupon field is added and the tests pass.") + "\n"));
    std::fs::write(&log, more).expect("append");
    let unheld = view(&store);
    assert_eq!(
        entry(&unheld).receipt.as_ref().expect("a place").line,
        4,
        "a line the ledger does not hold moved the place"
    );
    let reads = agent_log::reads_of(&log);
    assert_eq!(reads, 2);

    store.ingest_again();
    let held = view(&store);
    assert_eq!(
        entry(&held).receipt.as_ref().expect("a place").line,
        6,
        "{held:?}"
    );
    assert_eq!(
        agent_log::reads_of(&log),
        reads + 1,
        "the ledger took in more of the conversation and the answer was not re-derived"
    );
}

/* ── what stays the same ──────────────────────────────────────────────── */

#[test]
fn a_remembered_answer_is_the_answer_a_fresh_look_gives_with_the_same_masking() {
    // The masker runs on the file name on every call, so what it did is said
    // every time: a second look does not report "nothing masked".
    let dir = tempfile::tempdir().expect("tempdir");
    let project = dir
        .path()
        .join("projects")
        .join("-synthetic-AKIAIOSFODNN7EXAMPLE-shop");
    std::fs::create_dir_all(&project).expect("mkdir");
    std::fs::write(
        project.join(format!("{SESSION}.jsonl")),
        walk(WORDS).join("\n") + "\n",
    )
    .expect("transcript");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    thruline_core::ingest::ingest(
        &mut ledger,
        &[dir.path().join("projects")],
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    let log: &Path = &project.join(format!("{SESSION}.jsonl"));

    let mut first = thruline_core::redact::MaskRecord::default();
    let a = agent_log::locate(&ledger, SESSION, &mut first).expect("locate");
    let mut second = thruline_core::redact::MaskRecord::default();
    let b = agent_log::locate(&ledger, SESSION, &mut second).expect("locate again");
    assert_eq!(a, b);
    assert!(first.masked > 0, "the masker did not run on the file name");
    assert_eq!(
        first.masked, second.masked,
        "the second look masked differently from the first"
    );
    assert_eq!(agent_log::reads_of(log), 1);
    let agent_log::Located::At(receipt) = b else {
        panic!("not located");
    };
    assert!(
        !receipt.file.contains("AKIAIOSFODNN7EXAMPLE"),
        "{receipt:?}"
    );
}
