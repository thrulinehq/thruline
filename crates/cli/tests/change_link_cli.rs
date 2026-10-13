//! T-2-2 — the acts, at the surface a person actually touches, and the
//! promise that the session list they already have did not move.
//!
//! `B3.16` is the quiet half of this card: old, unassociated work must stay
//! discoverable. The loudest way to break it is to "improve" the session list
//! while adding Changes to it. So the first test here is that the session
//! listing is byte-identical, and the second is that its source did not move
//! at all.
//!
//! Every store is a fixture under a temporary HOME. No real store is opened.

use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::params;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
const SESSION: &str = "cccccccc-0000-4000-8000-00000000s001";
const LONELY: &str = "cccccccc-0000-4000-8000-00000000s009";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c001";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// A fixture ledger with one project, two sessions, one Change and one scope.
fn fixture(dir: &Path) -> PathBuf {
    let db = dir.join("ledger.db");
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open a fresh ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, '/tmp/synthetic-project', 'claude_code', 'Synthetic',
                    '2026-01-01T00:00:00Z')",
            params![PROJECT],
        )
        .expect("project");
    for session in [SESSION, LONELY] {
        ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id, project_id, file_path, started_at)
                 VALUES(?1, ?2, '/tmp/synthetic-project/s.jsonl', '2026-01-02T09:30:00Z')",
                params![session, PROJECT],
            )
            .expect("session");
    }
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('cccccccc-ev-1', ?1, ?2, 1, '2026-01-02T09:31:00Z', 'tool_use', 'Edit',
                    '{\"file_path\":\"/tmp/synthetic-project/src/upload/retry.rs\"}',
                    '/tmp/s.jsonl', 1, 'x', '', '')",
            params![PROJECT, SESSION],
        )
        .expect("event");
    thruline_core::change::insert_change(
        &ledger,
        &thruline_core::change::NewChange {
            id: CHANGE.to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-01-02T09:00:00Z".to_string(),
            request: thruline_core::change::ChangeRequest {
                title: "Retry failed uploads".to_string(),
                purpose: "Synthetic.".to_string(),
                prompt_event_id: None,
                requester: "sam".to_string(),
                source: thruline_core::change::RequestSource::Person,
            },
        },
    )
    .expect("change");
    thruline_core::change::append_scope(
        &ledger,
        CHANGE,
        "2026-01-02T10:00:00Z",
        &thruline_core::change::AcceptedScope {
            allowed_scope: vec!["src/upload/**".to_string()],
            ..Default::default()
        },
    )
    .expect("scope");
    db
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(home: &Path, db: &Path, args: &[&str]) -> Run {
    let out = Command::new(bin())
        .env("HOME", home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .expect("run thruline");
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

// ---------------------------------------------------------------------------
// B3.16 — the session list did not move
// ---------------------------------------------------------------------------

/// `thruline sessions` answers byte-identically before and after a Change and
/// its links exist. It reads the execution store and knows nothing about
/// Changes; this proves that stayed true.
#[test]
fn the_session_listing_is_byte_identical_before_and_after_links_exist() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = fixture(dir.path());
    let exec = dir.path().join("exec.db");

    let before = run(
        &home,
        &db,
        &[
            "sessions",
            "--json",
            "--exec-db",
            exec.to_str().unwrap_or(""),
        ],
    );
    assert_eq!(before.code, 0, "{}", before.stderr);

    let decided = run(
        &home,
        &db,
        &[
            "change",
            "associate",
            CHANGE,
            SESSION,
            "--confirm",
            "--actor",
            "sam",
            "--reason",
            "this is the one",
        ],
    );
    assert_eq!(decided.code, 0, "{}", decided.stderr);

    let after = run(
        &home,
        &db,
        &[
            "sessions",
            "--json",
            "--exec-db",
            exec.to_str().unwrap_or(""),
        ],
    );
    assert_eq!(
        before.stdout, after.stdout,
        "the session list must not change because a Change exists"
    );
    assert_eq!(before.stderr, after.stderr);
}

/// And the listing's source did not move either. The byte-identity above is
/// about one fixture; this is about every store there is.
#[test]
fn the_session_listing_source_is_the_one_head_shipped() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/sessions_cmd.rs");
    let bytes = std::fs::read(&path).expect("read sessions_cmd.rs");
    let digest = thruline_core::chain::sha256_hex(&bytes);
    assert_eq!(
        digest, "896f0860df1ea6b3fe067ded213f7fefbade6c8bcafd2af02e993179ef87acdc",
        "`thruline sessions` is the list old work is found through. T-2-2 must \
         not touch it; if it must, say so and re-pin this deliberately."
    );
}

/// A session linked to nothing answers with a sentence, not an error and not
/// an empty screen.
#[test]
fn a_session_that_belongs_to_no_change_says_so() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = fixture(dir.path());
    let out = run(&home, &db, &["change", "links", "--session", LONELY]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        out.stdout.trim(),
        "No Change is linked to this session yet.",
        "stderr was {}",
        out.stderr
    );
}

// ---------------------------------------------------------------------------
// The act
// ---------------------------------------------------------------------------

/// No name, no link — and the refusal is the copy file's sentence, not a
/// constraint error.
#[test]
fn confirming_without_a_name_is_refused_and_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = fixture(dir.path());
    let out = run(
        &home,
        &db,
        &["change", "associate", CHANGE, SESSION, "--confirm"],
    );
    assert_ne!(out.code, 0, "an unattributed confirmation must be refused");
    assert!(
        out.stderr
            .contains("We cannot confirm a link without a name. Say who is confirming it."),
        "the refusal must say what would fix it: {}",
        out.stderr
    );
    assert_eq!(rows(&db), 0, "a refused act writes nothing");
}

/// An empty `--actor` is the same refusal as none. A whitespace name is not a
/// person.
#[test]
fn a_blank_name_is_the_same_refusal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = fixture(dir.path());
    let out = run(
        &home,
        &db,
        &[
            "change",
            "associate",
            CHANGE,
            SESSION,
            "--confirm",
            "--actor",
            "   ",
        ],
    );
    assert_ne!(out.code, 0);
    assert_eq!(rows(&db), 0);
}

/// Rejecting appends beside the suggestion and leaves it exactly as it was.
/// "What did the tool think" and "what did the person say" stay two facts.
#[test]
fn rejecting_leaves_the_suggestion_it_answers_untouched() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = fixture(dir.path());

    let proposed = run(&home, &db, &["change", "suggest", CHANGE, "--record"]);
    assert_eq!(proposed.code, 0, "{}", proposed.stderr);
    let before = association_rows(&db);
    assert!(
        !before.is_empty() && before.iter().all(|(p, _, _)| p == "suggested"),
        "the suggester writes suggestions: {before:?}"
    );

    let rejected = run(
        &home,
        &db,
        &[
            "change",
            "associate",
            CHANGE,
            SESSION,
            "--reject",
            "--actor",
            "sam",
            "--reason",
            "different feature",
        ],
    );
    assert_eq!(rejected.code, 0, "{}", rejected.stderr);

    let after = association_rows(&db);
    for row in &before {
        assert!(
            after.contains(row),
            "the suggestion {row:?} was edited or removed by a rejection"
        );
    }
    assert_eq!(
        after.len(),
        before.len() + 1,
        "a decision appends exactly one row"
    );
    assert!(after.iter().any(|(p, actor, reason)| p == "rejected"
        && actor.as_deref() == Some("sam")
        && reason.as_deref() == Some("different feature")));
}

/// Confirm and reject are the only decisions, and asking for both or neither
/// is refused rather than guessed at.
#[test]
fn a_decision_must_be_exactly_one_of_confirm_or_reject() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = fixture(dir.path());
    for args in [
        vec!["change", "associate", CHANGE, SESSION, "--actor", "sam"],
        vec![
            "change",
            "associate",
            CHANGE,
            SESSION,
            "--confirm",
            "--reject",
            "--actor",
            "sam",
        ],
    ] {
        let out = run(&home, &db, &args);
        assert_ne!(out.code, 0, "{args:?} must be refused");
    }
    assert_eq!(rows(&db), 0);
}

// ---------------------------------------------------------------------------
// The gated read, at the surface
// ---------------------------------------------------------------------------

/// A suggested link shows the refusal sentence and no number.
#[test]
fn a_suggested_link_reads_as_not_counted_with_the_reason() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = fixture(dir.path());
    run(&home, &db, &["change", "suggest", CHANGE, "--record"]);
    let out = run(&home, &db, &["change", "results", CHANGE]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stdout.contains(
            "Not counted for this Change: this link is still a suggestion. Nothing is \
             carried over until you confirm it."
        ),
        "{}",
        out.stdout
    );
}

/// The `--json` surface carries both registers for every sentence, so the
/// technical layer is complete without being first.
#[test]
fn the_json_surface_carries_both_registers() {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = fixture(dir.path());
    let out = run(&home, &db, &["change", "suggest", CHANGE, "--json"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let value: serde_json::Value = serde_json::from_str(&out.stdout).expect("json");
    let sentences = value["sentences"].as_array().expect("sentences");
    assert!(!sentences.is_empty());
    for sentence in sentences {
        assert!(sentence["plain"].is_string(), "{sentence}");
        assert!(sentence["technical"].is_string(), "{sentence}");
        assert_ne!(sentence["plain"], sentence["technical"]);
    }
}

// ---------------------------------------------------------------------------

fn rows(db: &Path) -> i64 {
    let ledger = thruline_core::ledger::Ledger::open(db).expect("reopen");
    ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM change_session_associations",
            [],
            |r| r.get(0),
        )
        .unwrap_or(-1)
}

type Row = (String, Option<String>, Option<String>);

fn association_rows(db: &Path) -> Vec<Row> {
    let ledger = thruline_core::ledger::Ledger::open(db).expect("reopen");
    let mut statement = ledger
        .conn()
        .prepare(
            "SELECT provenance, actor, reason FROM change_session_associations
              ORDER BY id",
        )
        .expect("prepare");
    let rows = statement
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .expect("query");
    rows.filter_map(|r| r.ok()).collect()
}

// ---------------------------------------------------------------------------
// T-2-9 / T-2-8-F3 — what was not considered is said, not only filed
// ---------------------------------------------------------------------------

/// A store whose every recording carries a time this matcher cannot read, and
/// nothing inside the Change's scope.
///
/// The offsets are `+05:30`, which is a real instant this product refuses to
/// convert rather than guess at, so both sessions are unplaceable in time and
/// nothing can be suggested.
fn all_undated(dir: &Path) -> PathBuf {
    let db = dir.join("ledger.db");
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open a fresh ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, '/tmp/synthetic-project', 'claude_code', 'Synthetic',
                    '2026-01-01T00:00:00Z')",
            params![PROJECT],
        )
        .expect("project");
    for (n, session) in [SESSION, LONELY].iter().enumerate() {
        ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id, project_id, file_path, started_at)
                 VALUES(?1, ?2, '/tmp/synthetic-project/s.jsonl', NULL)",
                params![session, PROJECT],
            )
            .expect("session");
        ledger
            .conn()
            .execute(
                "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                     tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
                 VALUES(?1, ?2, ?3, 1, '2026-01-02T09:31:00+05:30', 'tool_use', 'Edit',
                        '{\"file_path\":\"/tmp/synthetic-project/docs/notes.md\"}',
                        '/tmp/s.jsonl', 1, 'x', '', '')",
                params![format!("cccccccc-ev-u{n}"), PROJECT, session],
            )
            .expect("event");
    }
    thruline_core::change::insert_change(
        &ledger,
        &thruline_core::change::NewChange {
            id: CHANGE.to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-01-02T09:00:00Z".to_string(),
            request: thruline_core::change::ChangeRequest {
                title: "Retry failed uploads".to_string(),
                purpose: "Synthetic.".to_string(),
                prompt_event_id: None,
                requester: "sam".to_string(),
                source: thruline_core::change::RequestSource::Person,
            },
        },
    )
    .expect("change");
    thruline_core::change::append_scope(
        &ledger,
        CHANGE,
        "2026-01-02T10:00:00Z",
        &thruline_core::change::AcceptedScope {
            allowed_scope: vec!["src/upload/**".to_string()],
            ..Default::default()
        },
    )
    .expect("scope");
    db
}

/// With nothing suggested and everything unread, the person is told what was
/// not considered — not that nothing looked related.
///
/// `T-2-8-F3`: the surface returned after printing "Nothing in the record looks
/// like it belongs to this Change yet." and never reached the lines that named
/// the two recordings it could not place in time. An empty answer and an unread
/// one are different answers, and it was giving the reassuring one.
#[test]
fn what_could_not_be_placed_in_time_is_said_even_when_nothing_is_suggested() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = all_undated(dir.path());
    let run = run(dir.path(), &db, &["change", "suggest", CHANGE]);
    assert_eq!(run.code, 0, "{}", run.stderr);
    assert!(
        run.stdout
            .contains("Nothing in the record looks like it belongs to this Change yet."),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains(
            "2 pieces of recorded work could not be placed in time, so they were not considered."
        ),
        "the count and the sentence that carries it are missing:\n{}",
        run.stdout
    );
    for session in [SESSION, LONELY] {
        assert!(
            run.stdout.contains(session),
            "{session} was not named:\n{}",
            run.stdout
        );
    }
    assert!(
        run.stdout
            .contains("We cannot tell when this session ran, so we did not compare it by time."),
        "{}",
        run.stdout
    );
}

/// The same sentence agrees with its own number when there is one of them.
#[test]
fn one_unplaceable_recording_is_said_in_the_singular() {
    let (plain, _) = thruline_core::voice::association_sentence("session_undated_summary", 1, 0)
        .expect("the summary sentence");
    assert_eq!(
        plain,
        "1 piece of recorded work could not be placed in time, so it was not considered."
    );
}
