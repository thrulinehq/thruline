//! T-2-3 — the two verbs at the surface a person touches: the tool proposes,
//! and one person accepts.
//!
//! What this file is for that the core tests are not: the *words*. `propose` and
//! `accept` print nothing they wrote themselves, both refuse without a name, and
//! the first screen a person sees is a list of lines they could check — not an
//! empty box asking them to write acceptance criteria, which is the wall
//! `03-USER-LENS.md` rule 8 forbids.
//!
//! Every store is a fixture under a temporary HOME. No real store is opened.

use std::path::{Path, PathBuf};
use std::process::Command;

use rusqlite::params;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
const SESSION: &str = "cccccccc-0000-4000-8000-00000000s001";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c001";

/// The request a person wrote at the start of the session, with a credential
/// shape in it so the masking boundary is observable from outside the process.
const OPENING_REQUEST: &str = "The key is AKIAIOSFODNN7EXAMPLE and it leaked. \
     Two retry requests for the same delivery do not enqueue duplicate work. \
     Make retries reliable.";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// A fixture ledger with one project, one session carrying an opening request,
/// one Change with a scope, and a confirmed link between them.
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
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES(?1, ?2, '/tmp/synthetic-project/s.jsonl', '2026-01-02T09:30:00Z')",
            params![SESSION, PROJECT],
        )
        .expect("session");
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, text,
                 raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('cccccccc-ev-1', ?1, ?2, 1, '2026-01-02T09:30:00Z', 'user_msg', ?3,
                    '/tmp/s.jsonl', 1, 'x', '', '')",
            params![PROJECT, SESSION, OPENING_REQUEST],
        )
        .expect("opening request");
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('cccccccc-ev-2', ?1, ?2, 2, '2026-01-02T09:31:00Z', 'tool_use', 'Edit',
                    '{\"file_path\":\"/tmp/synthetic-project/src/upload/retry.rs\"}',
                    '/tmp/s.jsonl', 2, 'x', '', '')",
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
                purpose: "Uploads that fail once are lost.".to_string(),
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
    thruline_core::change_link::decide(
        &ledger,
        CHANGE,
        SESSION,
        thruline_core::change::AssociationProvenance::Confirmed,
        "sam",
        "this is the retry work",
        "2026-01-02T10:05:00Z",
    )
    .expect("confirm");
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

fn setup() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = fixture(dir.path());
    (dir, db)
}

// ---------------------------------------------------------------------------
// propose
// ---------------------------------------------------------------------------

/// The first screen is a list of lines a person could check, with the reason for
/// each — not an empty box. Rule 8, at the surface.
#[test]
fn propose_shows_lines_a_person_could_check_and_why_each_is_there() {
    let (dir, db) = setup();
    let out = run(dir.path(), &db, &["change", "propose", CHANGE]);
    assert_eq!(out.code, 0, "{out:?}", out = (out.stdout, out.stderr));
    let (stdout, _) = (out.stdout, out.stderr);
    assert!(
        stdout
            .contains("1. Two retry requests for the same delivery do not enqueue duplicate work."),
        "{stdout}"
    );
    assert!(
        stdout.contains("This is your own wording from the start of this work"),
        "every proposal says why it is there: {stdout}"
    );
    // The wish is kept as a note with the missing part named, in Sam's words.
    assert!(stdout.contains("Make retries reliable"), "{stdout}");
    assert!(
        stdout.contains("it does not say what you would see if it were done"),
        "{stdout}"
    );
    // A list of checks is not evidence, and the bound on versions is shown
    // before it refuses anything.
    assert!(stdout.contains("still needs evidence"), "{stdout}");
    assert!(stdout.contains("We keep at most 20."), "{stdout}");
    assert!(
        stdout.contains("Nobody has written down what counts as done for this Change yet."),
        "{stdout}"
    );
}

/// Masking happens before the text is served, and it is observable from
/// outside the process: the credential shape in the person's own request never
/// reaches a terminal.
#[test]
fn propose_never_prints_the_raw_request() {
    let (dir, db) = setup();
    let out = run(dir.path(), &db, &["change", "propose", CHANGE]);
    assert!(
        !out.stdout.contains("AKIAIOSFODNN7EXAMPLE"),
        "raw stored text reached stdout: {}",
        out.stdout
    );
    assert!(out.stdout.contains("[masked: aws]"), "{}", out.stdout);
    let json = run(dir.path(), &db, &["change", "propose", CHANGE, "--json"]);
    assert!(
        !json.stdout.contains("AKIAIOSFODNN7EXAMPLE"),
        "raw stored text reached the JSON surface: {}",
        json.stdout
    );
}

/// `--json` carries the record and both registers of every sentence, under the
/// registered shape and version.
#[test]
fn propose_json_carries_the_record_and_both_registers() {
    let (dir, db) = setup();
    let out = run(dir.path(), &db, &["change", "propose", CHANGE, "--json"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let value: serde_json::Value = serde_json::from_str(out.stdout.trim()).expect("JSON");
    assert_eq!(value["shape"], "thruline.change.propose");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(
        value["proposals"]["criteria"][0]["text"],
        "Two retry requests for the same delivery do not enqueue duplicate work."
    );
    assert!(value["criteria_said"][0]["reasoning"]["plain"]
        .as_str()
        .is_some_and(|s| !s.is_empty()));
    assert!(value["criteria_said"][0]["reasoning"]["technical"]
        .as_str()
        .is_some_and(|s| !s.is_empty()));
    assert!(value["not_evidence"]["plain"]
        .as_str()
        .is_some_and(|s| s.contains("still needs evidence")));
    // A mandatory check the machine cannot run is listed rather than dropped.
    let checks = value["checks_said"].as_array().expect("checks");
    assert!(!checks.is_empty(), "the profile proposed nothing");
    for check in checks {
        assert!(check["need"]["plain"].is_string());
        assert!(check["availability"]["plain"].is_string());
    }
}

/// Proposing writes nothing. The store is byte-identical afterwards.
#[test]
fn propose_writes_nothing_at_all() {
    let (dir, db) = setup();
    let before = std::fs::read(&db).expect("read the store");
    for _ in 0..3 {
        let out = run(dir.path(), &db, &["change", "propose", CHANGE]);
        assert_eq!(out.code, 0, "{}", out.stderr);
    }
    let after = std::fs::read(&db).expect("read the store");
    assert_eq!(
        before.len(),
        after.len(),
        "proposing changed the size of the store"
    );
    assert!(before == after, "proposing changed the store");
}

// ---------------------------------------------------------------------------
// accept
// ---------------------------------------------------------------------------

/// One act, and it says what it did, whose words each line is in, and that it is
/// not an approval.
#[test]
fn accept_writes_what_counts_as_done_and_says_it_is_not_an_approval() {
    let (dir, db) = setup();
    let out = run(
        dir.path(),
        &db,
        &[
            "change",
            "accept",
            CHANGE,
            "--all",
            "--actor",
            "sam",
            "--rationale",
            "this is what I want checked",
        ],
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stdout
            .contains("Saved as what counts as done, version 2 — 1 thing to check."),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("In the words we proposed."),
        "{}",
        out.stdout
    );
    assert!(
        out.stdout.contains(
            "Agreeing what counts as done is not approving the work. Nobody has approved anything."
        ),
        "{}",
        out.stdout
    );

    // Read back from the record, not from the sentence: one new version, one
    // acceptance record, and no decision.
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("ledger");
    let view = thruline_core::change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    let scope = view.accepted_scope.as_ref().expect("standing");
    assert_eq!(scope.version, 2);
    assert_eq!(
        scope.criteria,
        vec!["Two retry requests for the same delivery do not enqueue duplicate work.".to_string()]
    );
    let acceptance = scope.acceptance.as_ref().expect("an acceptance record");
    assert_eq!(acceptance.actor, "sam");
    assert_eq!(acceptance.rationale, "this is what I want checked");
    assert_eq!(view.decision, thruline_core::change::NONE_YET);
}

/// No name, no acceptance, and the refusal is the copy file's sentence.
#[test]
fn accept_without_a_name_is_refused_in_sams_words() {
    let (dir, db) = setup();
    let out = run(dir.path(), &db, &["change", "accept", CHANGE, "--all"]);
    assert_ne!(out.code, 0, "a nameless acceptance must not succeed");
    assert!(
        out.stderr.contains(
            "We cannot write down what counts as done without a name. Say who is accepting it."
        ),
        "{}",
        out.stderr
    );
    // And nothing was written: the fixture's one scope version is still alone.
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("ledger");
    let versions: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM change_scope_revisions WHERE change_id=?1",
            params![CHANGE],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(versions, 1);
}

/// Saying neither which lines nor all of them is refused rather than guessed at.
#[test]
fn accept_that_does_not_say_which_lines_is_refused() {
    let (dir, db) = setup();
    let out = run(
        dir.path(),
        &db,
        &["change", "accept", CHANGE, "--actor", "sam"],
    );
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("--all"), "{}", out.stderr);
    let both = run(
        dir.path(),
        &db,
        &[
            "change",
            "accept",
            CHANGE,
            "--all",
            "--criterion",
            "1",
            "--actor",
            "sam",
        ],
    );
    assert_ne!(both.code, 0, "both at once is ambiguous, not generous");
}

/// A line the person rewrote before accepting is attributed to the person.
#[test]
fn accept_attributes_a_rewritten_line_to_the_person_who_wrote_it() {
    let (dir, db) = setup();
    let out = run(
        dir.path(),
        &db,
        &[
            "change",
            "accept",
            CHANGE,
            "--criterion",
            "1",
            "--edit",
            "1=Two retry requests for the same delivery do not enqueue any work at all.",
            "--actor",
            "sam",
        ],
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(
        out.stdout
            .contains("Not in the words we proposed — this is how you wrote it."),
        "{}",
        out.stdout
    );
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("ledger");
    let view = thruline_core::change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    assert_eq!(
        view.accepted_scope.as_ref().expect("standing").criteria,
        vec![
            "Two retry requests for the same delivery do not enqueue any work at all.".to_string()
        ]
    );
}

/// The hole the hand walkthrough found, closed at the surface: accepting the
/// whole proposed set again does not quietly put the tool's wording back over
/// the line the person rewrote.
#[test]
fn accepting_again_does_not_quietly_replace_the_line_you_rewrote() {
    let (dir, db) = setup();
    let first = run(
        dir.path(),
        &db,
        &[
            "change",
            "accept",
            CHANGE,
            "--all",
            "--edit",
            "1=Two retry requests for the same delivery do not enqueue duplicate work, not even once.",
            "--actor",
            "sam",
        ],
    );
    assert_eq!(first.code, 0, "{}", first.stderr);

    let again = run(
        dir.path(),
        &db,
        &["change", "accept", CHANGE, "--all", "--actor", "sam"],
    );
    assert_ne!(again.code, 0, "a silent replacement must not succeed");
    assert!(
        again.stderr.contains("Nothing was saved"),
        "{}",
        again.stderr
    );
    assert!(
        again
            .stderr
            .contains("Say plainly that you are replacing them"),
        "the refusal says what would make it go through: {}",
        again.stderr
    );

    // The person's line is still what counts as done.
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("ledger");
    let view = thruline_core::change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    assert_eq!(
        view.accepted_scope.as_ref().expect("standing").criteria,
        vec![
            "Two retry requests for the same delivery do not enqueue duplicate work, not even once."
                .to_string()
        ]
    );

    // And saying so plainly is allowed, as an explicit act.
    let replacing = run(
        dir.path(),
        &db,
        &[
            "change",
            "accept",
            CHANGE,
            "--all",
            "--replacing",
            "--actor",
            "sam",
        ],
    );
    assert_eq!(replacing.code, 0, "{}", replacing.stderr);
}

/// The *other* refusal at the surface: softening a line that is still in the
/// words we proposed.
///
/// The test above reaches the authorship refusal, because the line it accepts
/// first was rewritten. Here nothing is ever rewritten, so authorship has
/// nothing to count and only the comparison against the standing version can
/// refuse — and the sentence a person reads proves which one did. Written
/// because disabling the comparison left every other test in this file green.
#[test]
fn softening_a_line_you_accepted_as_proposed_is_refused_at_the_surface() {
    let (dir, db) = setup();
    let first = run(
        dir.path(),
        &db,
        &["change", "accept", CHANGE, "--all", "--actor", "sam"],
    );
    assert_eq!(first.code, 0, "{}", first.stderr);
    assert!(
        first.stdout.contains("In the words we proposed."),
        "the premise is that nothing here is in the person's own words: {}",
        first.stdout
    );

    let softer = run(
        dir.path(),
        &db,
        &[
            "change",
            "accept",
            CHANGE,
            "--all",
            "--edit",
            "1=Two retry requests for the same delivery do not enqueue work.",
            "--actor",
            "sam",
        ],
    );
    assert_ne!(softer.code, 0, "a softer line must not be written quietly");
    assert!(
        softer.stderr.contains("says less"),
        "the weakening refusal must be the one a person reads: {}",
        softer.stderr
    );
    assert!(
        !softer.stderr.contains("put our wording back over"),
        "the authorship refusal cannot be what answers here: {}",
        softer.stderr
    );

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("ledger");
    let view = thruline_core::change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    assert_eq!(
        view.accepted_scope.as_ref().expect("standing").criteria,
        vec!["Two retry requests for the same delivery do not enqueue duplicate work.".to_string()],
        "the softer line was written anyway"
    );
}

/// `--json` carries the acceptance and both registers of every sentence.
#[test]
fn accept_json_carries_the_acceptance_and_both_registers() {
    let (dir, db) = setup();
    let out = run(
        dir.path(),
        &db,
        &[
            "change", "accept", CHANGE, "--all", "--actor", "sam", "--json",
        ],
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    let value: serde_json::Value = serde_json::from_str(out.stdout.trim()).expect("JSON");
    assert_eq!(value["shape"], "thruline.change.accept");
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["accepted"]["version"], 2);
    assert_eq!(value["accepted"]["actor"], "sam");
    assert!(value["not_an_approval"]["plain"]
        .as_str()
        .is_some_and(|s| s.contains("not approving")));
    assert!(value["said"]["technical"]
        .as_str()
        .is_some_and(|s| s.contains("B5.6")));
}

/// A second reading after an acceptance changes nothing, and says so.
#[test]
fn a_second_reading_after_an_acceptance_changes_nothing() {
    let (dir, db) = setup();
    let accepted = run(
        dir.path(),
        &db,
        &["change", "accept", CHANGE, "--all", "--actor", "sam"],
    );
    assert_eq!(accepted.code, 0, "{}", accepted.stderr);
    let first = run(dir.path(), &db, &["change", "propose", CHANGE, "--json"]);
    let second = run(dir.path(), &db, &["change", "propose", CHANGE, "--json"]);
    assert_eq!(
        first.stdout, second.stdout,
        "two readings of one store must agree"
    );
    let value: serde_json::Value = serde_json::from_str(first.stdout.trim()).expect("JSON");
    assert_eq!(value["proposals"]["diff"]["accepted_version"], 2);
    assert_eq!(
        value["proposals"]["accepted"]["criteria"][0]["attribution"],
        "as_proposed"
    );
    assert!(value["proposals"]["not_accepted_slug"].is_null());
}
