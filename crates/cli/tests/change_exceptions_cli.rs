//! T-5-3 (`B7.18`-`B7.22`, `B10.9`) — exceptions, through the compiled binary
//! and the served routes.
//!
//! **Why this file exists.** Every claim T-5-3 makes is about a record a person
//! can only produce by running the product: a waiver refused because what
//! counts as done names nobody who may write one, a waiver refused because the
//! prerequisite is not waivable at all, a readiness answer that reads "with
//! exceptions", and an approval that carries what it was given under for as
//! long as it stands. Each is asserted here against the real store, written by
//! real verbs, and read back through the real routes.
//!
//! Every fixture is synthetic. No string here came off a real machine.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Under C20 approving a version needs a Pro licence and there is no trial,
/// so a fresh home can no longer approve. This file is about exceptions, not
/// licensing (that is `licence_cli.rs` / `licence_flow_cli.rs`), so every run
/// turns the paid gate off through the debug-only seam
/// (`licence_cmd::paid_gate_on`, compiled only under `debug_assertions`).
const PAID_GATE_SEAM: (&str, &str) = ("THRULINE_PAID_GATE_DEV", "off");

fn run(home: &Path, cwd: &Path, db: &Path, args: &[&str]) -> Run {
    let out = Command::new(bin())
        .env("HOME", home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .env(PAID_GATE_SEAM.0, PAID_GATE_SEAM.1)
        .current_dir(cwd)
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

fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A repository with one commit, a store whose one project points at it, and
/// one recorded session that edited a file in it.
fn fixture(base: &Path) -> (PathBuf, PathBuf) {
    let repo = base.join("repo");
    std::fs::create_dir_all(repo.join("src/upload")).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("src/upload/retry.rs"), "// retry\n").expect("write");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "seed"]);
    std::fs::write(
        repo.join("s.jsonl"),
        format!("{{\"type\":\"user\",\"cwd\":\"{}\"}}\n", repo.display()),
    )
    .expect("transcript");

    let db = base.join("ledger.db");
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('cccccccc-0000-4000-8000-00000000p001', ?1, 'claude_code', 'Synthetic',
                    '2026-01-01T00:00:00Z')",
            rusqlite::params![repo.display().to_string()],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES('cccccccc-0000-4000-8000-00000000s001',
                    'cccccccc-0000-4000-8000-00000000p001', ?1, '2026-01-02T09:30:00Z')",
            rusqlite::params![repo.join("s.jsonl").display().to_string()],
        )
        .expect("session");
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('cccccccc-ev-1','cccccccc-0000-4000-8000-00000000p001',
                    'cccccccc-0000-4000-8000-00000000s001',1,'2026-01-02T09:31:00Z',
                    'tool_use','Edit', ?1, ?2, 1, 'x', '', '')",
            rusqlite::params![
                format!(
                    "{{\"file_path\":\"{}\"}}",
                    repo.join("src/upload/retry.rs").display()
                ),
                repo.join("s.jsonl").display().to_string()
            ],
        )
        .expect("event");
    (repo, db)
}

/// `change new` → `associate --confirm` → `accept --all` → `prepare`, through
/// the binary, exactly as `change_readiness_cli.rs` does it.
fn an_accepted_change(home: &Path, repo: &Path, db: &Path) -> String {
    let opened = run(
        home,
        repo,
        db,
        &[
            "change",
            "new",
            "--title",
            "Retry failed uploads",
            "--purpose",
            "The build stays clean on the packages that changed.",
            "--requester",
            "sam",
        ],
    );
    assert_eq!(opened.code, 0, "{}", opened.stderr);
    let change = opened
        .stdout
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("chg_"))
        .unwrap_or_else(|| panic!("no Change id was printed: {}", opened.stdout))
        .to_string();

    for args in [
        vec![
            "change",
            "associate",
            &change,
            "cccccccc-0000-4000-8000-00000000s001",
            "--confirm",
            "--actor",
            "sam",
            "--reason",
            "this is the retry work",
        ],
        vec![
            "change",
            "accept",
            &change,
            "--all",
            "--actor",
            "sam",
            "--rationale",
            "this is what I want checked",
        ],
        vec!["change", "prepare", &change],
    ] {
        let out = run(home, repo, db, &args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    change
}

/// The authority a person waives under, as the ACCEPTED profile records it.
const AUTHORITY: &str = "the release policy";

/// Accept a second scope version whose check profile configures an exception
/// path, copying the criteria of the first.
///
/// This is the one hand-written row in the file, and it is the *policy* the
/// product does not yet have an editor for. Everything the task built — the
/// refusal, the record, the readiness answer, the approval — is driven through
/// the binary below.
fn with_an_exception_path(db: &Path, change: &str) {
    let ledger = thruline_core::ledger::Ledger::open(db).expect("open");
    let (criteria, exclusions, allowed, constraints): (String, String, String, String) = ledger
        .conn()
        .query_row(
            "SELECT criteria_json, exclusions_json, allowed_scope_json, constraints_json
               FROM change_scope_revisions WHERE change_id=?1 ORDER BY scope_version DESC LIMIT 1",
            rusqlite::params![change],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("the accepted revision exists");
    let profile = format!(
        "[\"{}{}\"]",
        thruline_core::change_readiness::EXCEPTION_POLICY_PREFIX,
        AUTHORITY
    );
    ledger
        .conn()
        .execute(
            "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at,
                 criteria_json, exclusions_json, allowed_scope_json, constraints_json,
                 check_profile_json, content_sha256)
             VALUES(?1, 2, '2026-01-03T09:00:00Z', ?2, ?3, ?4, ?5, ?6, 'x')",
            rusqlite::params![change, criteria, exclusions, allowed, constraints, profile],
        )
        .expect("a second scope version");
    ledger
        .conn()
        .execute(
            "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by,
                                                  accepted_at, rationale)
             VALUES(?1, 2, 'sam', '2026-01-03T09:05:00Z', 'who may set something aside')",
            rusqlite::params![change],
        )
        .expect("accept it");
}

/* ── 1. the two refusals ───────────────────────────────────────────────── */

#[test]
fn with_no_exception_path_configured_nothing_can_be_set_aside_and_nothing_is_appended() {
    let base = tempfile::tempdir().expect("tempdir");
    let home = base.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    let (repo, db) = fixture(base.path());
    let change = an_accepted_change(&home, &repo, &db);

    let refused = run(
        &home,
        &repo,
        &db,
        &[
            "change",
            "waive",
            "--change",
            &change,
            "--obligation",
            "criterion:1",
            "--actor",
            "sam",
            "--rationale",
            "the payment sandbox is down all week",
        ],
    );
    assert_eq!(refused.code, 1, "nothing set aside is not success");
    assert!(
        refused
            .stdout
            .contains("does not say who is allowed to do that"),
        "the refusal is the copy file's sentence: {}",
        refused.stdout
    );

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let rows: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM change_obligation_waivers", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rows, 0, "a refused act appends nothing");
}

#[test]
fn the_three_prerequisites_are_refused_even_with_an_exception_path_configured() {
    let base = tempfile::tempdir().expect("tempdir");
    let home = base.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    let (repo, db) = fixture(base.path());
    let change = an_accepted_change(&home, &repo, &db);
    with_an_exception_path(&db, &change);

    for prerequisite in [
        "candidate_identity",
        "decision_authority",
        "packet_consistency",
    ] {
        let refused = run(
            &home,
            &repo,
            &db,
            &[
                "change",
                "waive",
                "--change",
                &change,
                "--obligation",
                prerequisite,
                "--actor",
                "sam",
                "--rationale",
                "I would rather not check it",
            ],
        );
        assert_eq!(refused.code, 1, "{prerequisite} must be refused");
        assert!(
            refused
                .stdout
                .contains("it is one of the prerequisites no policy may set aside"),
            "{prerequisite}: {}",
            refused.stdout
        );
    }
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let rows: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM change_obligation_waivers", [], |r| {
            r.get(0)
        })
        .expect("count");
    assert_eq!(rows, 0, "no prerequisite reached the store");
}

/* ── 2-3. the record, the readiness answer, and the approval ───────────── */

/// The whole of Sam's step 7, through the binary and the served routes.
#[test]
fn a_waiver_clears_the_line_without_becoming_evidence_and_the_approval_says_so_for_ever() {
    let base = tempfile::tempdir().expect("tempdir");
    let home = base.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    let (repo, db) = fixture(base.path());
    let change = an_accepted_change(&home, &repo, &db);
    with_an_exception_path(&db, &change);

    let waived = run(
        &home,
        &repo,
        &db,
        &[
            "change",
            "waive",
            "--change",
            &change,
            "--obligation",
            "criterion:1",
            "--actor",
            "sam",
            "--rationale",
            "the payment sandbox is down all week",
            "--json",
        ],
    );
    assert_eq!(waived.code, 0, "{}{}", waived.stdout, waived.stderr);
    let answer: serde_json::Value = serde_json::from_str(&waived.stdout).expect("json");
    let payload = answer.get("data").unwrap_or(&answer);
    assert_eq!(payload["recorded"], serde_json::json!(true));
    assert_eq!(
        payload["authority"],
        serde_json::json!(AUTHORITY),
        "the authority is the accepted profile's, never the caller's"
    );
    assert!(
        payload["plain"]
            .as_str()
            .unwrap_or_default()
            .ends_with("Nobody checked this."),
        "the sentence never reads as a result: {payload}"
    );

    /* The record is one append-only row, and it is not evidence. */
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let (obligation, actor, authority): (String, String, String) = ledger
        .conn()
        .query_row(
            "SELECT obligation_id, actor, authority FROM change_obligation_waivers",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("one row");
    assert_eq!(
        (obligation.as_str(), actor.as_str()),
        ("criterion:1", "sam")
    );
    assert_eq!(authority, AUTHORITY);
    drop(ledger);

    /* Readiness, off the served route the screen reads. */
    let server = Server::start(&home, &repo, &db);
    let readiness = server.get(&format!("/api/changes/{change}/readiness"));
    let doc: serde_json::Value = serde_json::from_str(&readiness).expect("json");
    let r = &doc["readiness"];
    assert_eq!(r["ready"], serde_json::json!(true), "the line is cleared");
    assert_eq!(
        r["with_exceptions"],
        serde_json::json!(true),
        "and it is cleared by a person, not by the record"
    );
    assert_eq!(
        r["headline_slug"],
        serde_json::json!("readiness_ready_with_exceptions")
    );
    assert_eq!(
        r["rests_on"].as_array().map(Vec::len),
        Some(0),
        "a set-aside line is never something the answer rests on"
    );
    assert_eq!(
        r["waived"],
        serde_json::json!(["criterion:1"]),
        "it is listed as set aside, by itself"
    );
    let waived_obligation = r["obligations"]
        .as_array()
        .expect("obligations")
        .iter()
        .find(|o| o["id"] == serde_json::json!("criterion:1"))
        .expect("the line is there");
    assert_eq!(waived_obligation["outcome"], serde_json::json!("waived"));
    assert_ne!(
        waived_obligation["outcome"],
        serde_json::json!("satisfied"),
        "a waiver never renders as a result"
    );

    /* The coverage counts are untouched by it: nothing linked, nothing proven. */
    let coverage: serde_json::Value =
        serde_json::from_str(&server.get(&format!("/api/changes/{change}/coverage")))
            .expect("json");
    let line = &coverage["coverage"]["criteria"][0];
    assert_eq!(line["state"], serde_json::json!("none"), "still unchecked");
    assert_eq!(
        line["items"].as_array().map(Vec::len),
        Some(0),
        "a waiver is not an item"
    );

    /* The approval, through its own route, and what it carries. */
    let version = doc["readiness"]["target_version"]
        .as_str()
        .expect("a version")
        .to_string();
    let approved = server.post(
        &format!("/api/changes/{change}/decisions"),
        &format!("{{\"actor\":\"sam\",\"seen_version\":\"{version}\"}}"),
    );
    let acted: serde_json::Value = serde_json::from_str(&approved).expect("json");
    assert_eq!(
        acted["acted"]["outcome"],
        serde_json::json!("recorded"),
        "{acted}"
    );
    let exceptions = acted["acted"]["decision"]["exceptions"]
        .as_array()
        .expect("the approval carries its exceptions");
    assert_eq!(exceptions.len(), 1, "one exception, bound to the approval");
    assert_eq!(
        exceptions[0]["obligation_id"],
        serde_json::json!("criterion:1")
    );
    assert_eq!(exceptions[0]["actor"], serde_json::json!("sam"));
    assert_eq!(exceptions[0]["authority"], serde_json::json!(AUTHORITY));
    assert_eq!(
        exceptions[0]["rationale"],
        serde_json::json!("the payment sandbox is down all week")
    );

    /* And it is still there on the next read — it is a row, not a screen
     * state. Nothing about the standing approval is recomputed from the
     * waivers: a SECOND waiver written afterwards does not reach it. */
    let second = run(
        &home,
        &repo,
        &db,
        &[
            "change",
            "waive",
            "--change",
            &change,
            "--obligation",
            "review",
            "--actor",
            "sam",
            "--rationale",
            "nobody is available this week",
        ],
    );
    assert_eq!(second.code, 0, "{}{}", second.stdout, second.stderr);
    let after: serde_json::Value =
        serde_json::from_str(&server.get(&format!("/api/changes/{change}"))).expect("json");
    let standing = &after["change"]["decisions"]["standing"];
    assert_eq!(
        standing["exceptions"].as_array().map(Vec::len),
        Some(1),
        "an approval never acquires an exception written after it: {standing}"
    );
}

/* ── DECISION-HISTORY-SHAPE-R2: the export and the served Change agree ───
 * Re-created from the round-1 adversary's reproduction
 * (gates/adversary-DECISION-HISTORY-SHAPE/TRANSCRIPT-436ffe35.md). The
 * decision record file of an approval given with exceptions carries
 * `"act": "approved_with_exceptions"`; the export's copy-standing check used to
 * compare it with the row's narrower `"approved"`, so `GET …/export` said
 * `content_changed` for the very row `GET /api/changes/<id>` served as intact,
 * with no decision act between the two reads. The check now compares the file
 * with what `write_record` writes for that row — and a file edited down to
 * the narrower form is a changed file. */
#[test]
fn adversary_export_disagrees_with_served_change_for_an_approval_with_exceptions() {
    let base = tempfile::tempdir().expect("tempdir");
    let home = base.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    let (repo, db) = fixture(base.path());
    let change = an_accepted_change(&home, &repo, &db);
    with_an_exception_path(&db, &change);

    let waived = run(
        &home,
        &repo,
        &db,
        &[
            "change",
            "waive",
            "--change",
            &change,
            "--obligation",
            "criterion:1",
            "--actor",
            "sam",
            "--rationale",
            "the payment sandbox is down all week",
        ],
    );
    assert_eq!(waived.code, 0, "{}{}", waived.stdout, waived.stderr);

    let server = Server::start(&home, &repo, &db);
    let readiness: serde_json::Value =
        serde_json::from_str(&server.get(&format!("/api/changes/{change}/readiness")))
            .expect("json");
    let version = readiness["readiness"]["target_version"]
        .as_str()
        .expect("a version")
        .to_string();

    let approved = server.post(
        &format!("/api/changes/{change}/decisions"),
        &format!("{{\"actor\":\"sam\",\"seen_version\":\"{version}\"}}"),
    );
    let acted: serde_json::Value = serde_json::from_str(&approved).expect("json");
    assert_eq!(acted["acted"]["outcome"], serde_json::json!("recorded"));
    assert_eq!(
        acted["acted"]["decision"]["exceptions"]
            .as_array()
            .map(Vec::len),
        Some(1),
        "the approval was given with one exception: {acted}"
    );

    // Surface 1: the served Change. No decision act happens between this
    // read and the next — this is exactly the sequence the card says must
    // never disagree.
    let served: serde_json::Value =
        serde_json::from_str(&server.get(&format!("/api/changes/{change}"))).expect("json");
    let served_row = &served["change"]["decisions"]["history"][0];
    assert_eq!(served_row["act"], serde_json::json!("approved"));
    assert_eq!(
        served_row["exceptions"].as_array().map(Vec::len),
        Some(1),
        "served row: {served_row}"
    );

    // Surface 2: the export, reading the very same row and the very same
    // on-disk file `write_record` just wrote for it.
    let export: serde_json::Value =
        serde_json::from_str(&server.get(&format!("/api/changes/{change}/export"))).expect("json");
    let decision_copy = &export["decisions"][0];
    eprintln!("export decision copy: {decision_copy}");

    assert_eq!(
        decision_copy["standing"],
        serde_json::json!("readable"),
        "REFUTED: the export surface says {} for the exact row the served \
         Change ({served_row}) shows as an intact, unchanged approval; no \
         decision act happened between the two reads, and the underlying \
         file `write_record` wrote is untouched. Full export decisions[0]: \
         {decision_copy}",
        decision_copy["standing"]
    );

    // The comparison is with what was written, never with a narrower form: a
    // copy edited to say a plain "approved" no longer says what the approval
    // was given under, and reads as changed.
    let path = repo
        .join(".thruline/decisions")
        .join(format!("{version}.json"));
    let written = std::fs::read_to_string(&path).expect("the decision copy");
    assert!(
        written.contains("\"approved_with_exceptions\""),
        "the copy says what it was given under: {written}"
    );
    std::fs::write(
        &path,
        written.replace("\"approved_with_exceptions\"", "\"approved\""),
    )
    .expect("edit the copy");
    let export: serde_json::Value =
        serde_json::from_str(&server.get(&format!("/api/changes/{change}/export"))).expect("json");
    assert_eq!(
        export["decisions"][0]["standing"],
        serde_json::json!("content_changed"),
        "a copy narrowed to a plain approval is a changed copy: {}",
        export["decisions"][0]
    );
}

/* ── DECISION-HISTORY-SHAPE-R4: one order for an approval's exceptions ───
 * Re-created from the round-3 adversary's reproduction
 * (gates/adversary-DECISION-HISTORY-SHAPE-R3/TRANSCRIPT-7503af6e.md,
 * attack_a and attack_a2; its file is adv_decision_history3.rs there).
 *
 * `write_record` wrote the exceptions in readiness order — criterion:2 then
 * criterion:10 — while the row was read back `ORDER BY obligation_id` as TEXT,
 * criterion:10 then criterion:2. The whole-document comparison of round 3 is
 * order-sensitive, so on a Change with ten or more criteria an untouched copy
 * read `content_changed` and a copy reversed by hand read `readable`, and the
 * act response and the later served history ordered the same approval two ways.
 * Every test here goes through the real binary and the real routes. */

/// Like `with_an_exception_path`, but the accepted scope has `n` criteria.
fn with_path_and_criteria(db: &Path, change: &str, n: usize) {
    let ledger = thruline_core::ledger::Ledger::open(db).expect("open");
    let criteria: Vec<String> = (1..=n)
        .map(|i| format!("Criterion number {i} holds"))
        .collect();
    let criteria = serde_json::to_string(&criteria).expect("json");
    let profile = format!(
        "[\"{}{}\"]",
        thruline_core::change_readiness::EXCEPTION_POLICY_PREFIX,
        AUTHORITY
    );
    ledger
        .conn()
        .execute(
            "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at,
                 criteria_json, exclusions_json, allowed_scope_json, constraints_json,
                 check_profile_json, content_sha256)
             VALUES(?1, 2, '2026-01-03T09:00:00Z', ?2, '[]', '[]', '[]', ?3, 'x')",
            rusqlite::params![change, criteria, profile],
        )
        .expect("a second scope version");
    ledger
        .conn()
        .execute(
            "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by,
                                                  accepted_at, rationale)
             VALUES(?1, 2, 'sam', '2026-01-03T09:05:00Z', 'many criteria')",
            rusqlite::params![change],
        )
        .expect("accept it");
}

/// The ids an `exceptions` array lists, in the order it lists them.
fn exception_ids(exceptions: &serde_json::Value) -> Vec<String> {
    exceptions
        .as_array()
        .unwrap_or_else(|| panic!("not an exceptions array: {exceptions}"))
        .iter()
        .map(|e| {
            e["obligation_id"]
                .as_str()
                .unwrap_or_else(|| panic!("no obligation id in {e}"))
                .to_string()
        })
        .collect()
}

/// One approval given with exceptions, and everything a test reads it back
/// through. `server` is declared before `_base` so the server stops first.
struct Approved {
    server: Server,
    repo: PathBuf,
    change: String,
    version: String,
    acted: serde_json::Value,
    _base: tempfile::TempDir,
}

impl Approved {
    /// `criteria` accepted lines, one waiver per id in `waived`, written in
    /// exactly that order, then one approval through `POST …/decisions`.
    fn new(criteria: usize, waived: &[&str]) -> Self {
        let base = tempfile::tempdir().expect("tempdir");
        let home = base.path().join("home");
        std::fs::create_dir_all(&home).expect("home");
        let (repo, db) = fixture(base.path());
        let change = an_accepted_change(&home, &repo, &db);
        with_path_and_criteria(&db, &change, criteria);
        for obligation in waived {
            let out = run(
                &home,
                &repo,
                &db,
                &[
                    "change",
                    "waive",
                    "--change",
                    &change,
                    "--obligation",
                    obligation,
                    "--actor",
                    "sam",
                    "--rationale",
                    &format!("{obligation} cannot be checked this week"),
                ],
            );
            assert_eq!(
                out.code, 0,
                "waive {obligation}: {}{}",
                out.stdout, out.stderr
            );
        }
        let server = Server::start(&home, &repo, &db);
        let readiness: serde_json::Value =
            serde_json::from_str(&server.get(&format!("/api/changes/{change}/readiness")))
                .expect("json");
        let version = readiness["readiness"]["target_version"]
            .as_str()
            .expect("a version")
            .to_string();
        let acted: serde_json::Value = serde_json::from_str(&server.post(
            &format!("/api/changes/{change}/decisions"),
            &format!("{{\"actor\":\"sam\",\"seen_version\":\"{version}\"}}"),
        ))
        .expect("json");
        assert_eq!(acted["acted"]["outcome"], serde_json::json!("recorded"));
        Self {
            server,
            repo,
            change,
            version,
            acted,
            _base: base,
        }
    }

    fn copy(&self) -> PathBuf {
        self.repo
            .join(".thruline/decisions")
            .join(format!("{}.json", self.version))
    }

    fn file(&self) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(self.copy()).expect("the decision copy"))
            .expect("json")
    }

    fn served(&self) -> serde_json::Value {
        serde_json::from_str(&self.server.get(&format!("/api/changes/{}", self.change)))
            .expect("json")
    }

    /// What `GET …/export` says about this approval's copy, right now.
    fn standing(&self) -> String {
        let export: serde_json::Value = serde_json::from_str(
            &self
                .server
                .get(&format!("/api/changes/{}/export", self.change)),
        )
        .expect("json");
        export["decisions"][0]["standing"]
            .as_str()
            .unwrap_or("?")
            .to_string()
    }

    /// The order every surface that serves this one approval lists its
    /// exceptions in, by name, so a failure says which one disagreed.
    fn orders(&self) -> Vec<(&'static str, Vec<String>)> {
        let served = self.served();
        vec![
            (
                "the act response (the row just recorded)",
                exception_ids(&self.acted["acted"]["decision"]["exceptions"]),
            ),
            (
                "the act response (the history re-read after it)",
                exception_ids(&self.acted["decisions"]["history"][0]["exceptions"]),
            ),
            (
                "the act response (the standing approval)",
                exception_ids(&self.acted["decisions"]["standing"]["exceptions"]),
            ),
            (
                "the decision file under the project",
                exception_ids(&self.file()["exceptions"]),
            ),
            (
                "the served Change (history)",
                exception_ids(&served["change"]["decisions"]["history"][0]["exceptions"]),
            ),
            (
                "the served Change (standing)",
                exception_ids(&served["change"]["decisions"]["standing"]["exceptions"]),
            ),
        ]
    }
}

/// The card's three assertions, on one approval given with `waived` exceptions
/// (written in that order) on a Change of `criteria` lines.
fn assert_one_order_everywhere(criteria: usize, waived: &[&str], expected: &[&str]) {
    let a = Approved::new(criteria, waived);
    let expected: Vec<String> = expected.iter().map(|s| s.to_string()).collect();

    // 1. Every surface lists the exceptions in the one order.
    for (surface, order) in a.orders() {
        assert_eq!(
            order, expected,
            "{surface} lists this approval's exceptions in a different order"
        );
    }

    // 2. The export, which builds the document from the row and compares it
    //    with the file as written, calls the untouched copy unchanged.
    assert_eq!(
        a.standing(),
        "readable",
        "an untouched copy of an approval with exceptions {waived:?} on {criteria} criteria \
         must read readable; file: {}",
        a.file()
    );

    // 3. The same file with its exceptions reversed by hand is an edited file.
    let original = std::fs::read_to_string(a.copy()).expect("copy");
    let mut doc = a.file();
    doc["exceptions"]
        .as_array_mut()
        .expect("an array")
        .reverse();
    std::fs::write(
        a.copy(),
        serde_json::to_vec_pretty(&doc).expect("serialize"),
    )
    .expect("reverse the exceptions by hand");
    assert_eq!(
        a.standing(),
        "content_changed",
        "a copy whose exceptions were reversed by hand is a changed copy"
    );

    // And putting back what the product wrote reads unchanged again.
    std::fs::write(a.copy(), original).expect("restore");
    assert_eq!(a.standing(), "readable");
}

/// Round-3 attack_a: ten criteria, exceptions on 2 and 10.
#[test]
fn ten_criteria_exceptions_on_two_and_ten_are_listed_one_way_and_the_copy_reads_unchanged() {
    assert_one_order_everywhere(
        10,
        &["criterion:2", "criterion:10"],
        &["criterion:2", "criterion:10"],
    );
}

/// Round-3 attack_a2: twelve criteria, exceptions on 9 and 11 (text order puts
/// 11 first, number order puts 9 first).
#[test]
fn twelve_criteria_exceptions_on_nine_and_eleven_are_listed_one_way_and_the_copy_reads_unchanged() {
    assert_one_order_everywhere(
        12,
        &["criterion:9", "criterion:11"],
        &["criterion:9", "criterion:11"],
    );
}

/// The order the waivers were written in is not the order of the approval:
/// the approval lists them as the Change lists its criteria.
#[test]
fn the_order_the_waivers_were_written_in_does_not_decide_the_order_of_the_approval() {
    assert_one_order_everywhere(
        12,
        &["criterion:11", "criterion:2", "criterion:10", "criterion:9"],
        &["criterion:2", "criterion:9", "criterion:10", "criterion:11"],
    );
}

/// Rounds 1–3 still hold on an approval with several exceptions: every edit
/// to one value of the copy reads `content_changed` (60 or more cases, one
/// value at a time, each restored), a copy that only says the same thing in a
/// different layout still reads `readable`, and nothing here depends on the
/// order of the array beyond the one order the product writes.
#[test]
fn every_edit_to_a_copy_still_reads_changed_and_a_reformatted_copy_still_reads_unchanged() {
    use serde_json::{json, Value};
    let a = Approved::new(12, &["criterion:2", "criterion:10"]);
    let path = a.copy();
    let original = std::fs::read_to_string(&path).expect("copy");
    let doc: Value = serde_json::from_str(&original).expect("json");
    let mut cases: Vec<(String, &'static str, String)> = Vec::new();
    let pretty = |d: &Value| serde_json::to_string_pretty(d).expect("serialize");
    cases.push(("baseline".into(), "readable", original.clone()));
    // every top-level field, edited and removed, one at a time
    for key in doc.as_object().expect("object").keys() {
        let mut d = doc.clone();
        d[key] = match &d[key] {
            Value::String(s) => json!(format!("{s}x")),
            Value::Number(_) => json!(99),
            Value::Array(_) => json!([]),
            _ => json!(null),
        };
        cases.push((format!("edit field {key}"), "content_changed", pretty(&d)));
        let mut d = doc.clone();
        d.as_object_mut().expect("object").remove(key);
        cases.push((format!("remove field {key}"), "content_changed", pretty(&d)));
    }
    // every field of every exception, edited and removed
    for i in 0..2 {
        for key in doc["exceptions"][i].as_object().expect("object").keys() {
            let mut d = doc.clone();
            d["exceptions"][i][key] = json!("zzz");
            cases.push((
                format!("edit exceptions[{i}].{key}"),
                "content_changed",
                pretty(&d),
            ));
            let mut d = doc.clone();
            d["exceptions"][i]
                .as_object_mut()
                .expect("object")
                .remove(key);
            cases.push((
                format!("remove exceptions[{i}].{key}"),
                "content_changed",
                pretty(&d),
            ));
        }
        let mut d = doc.clone();
        d["exceptions"][i]["extra"] = json!(1);
        cases.push((
            format!("add exceptions[{i}].extra"),
            "content_changed",
            pretty(&d),
        ));
        // one exception dropped, one duplicated
        let mut d = doc.clone();
        d["exceptions"].as_array_mut().expect("array").remove(i);
        cases.push((
            format!("drop exceptions[{i}]"),
            "content_changed",
            pretty(&d),
        ));
        let mut d = doc.clone();
        let copy = d["exceptions"][i].clone();
        d["exceptions"].as_array_mut().expect("array").push(copy);
        cases.push((
            format!("duplicate exceptions[{i}]"),
            "content_changed",
            pretty(&d),
        ));
    }
    let mut d = doc.clone();
    d["exceptions"].as_array_mut().expect("array").reverse();
    cases.push((
        "reverse the exceptions".into(),
        "content_changed",
        pretty(&d),
    ));
    // types and number spellings
    let mut d = doc.clone();
    d["accepted_criteria_version"] = json!("2");
    cases.push(("a number as a string".into(), "content_changed", pretty(&d)));
    cases.push((
        "2.0".into(),
        "content_changed",
        original.replace(
            "\"accepted_criteria_version\": 2",
            "\"accepted_criteria_version\": 2.0",
        ),
    ));
    cases.push((
        "2e0".into(),
        "content_changed",
        original.replace(
            "\"accepted_criteria_version\": 2",
            "\"accepted_criteria_version\": 2e0",
        ),
    ));
    // shapes that are not the document
    for (label, body) in [
        ("array", "[]".to_string()),
        ("null", "null".to_string()),
        ("empty object", "{}".to_string()),
        ("string", "\"x\"".to_string()),
        ("number", "3".to_string()),
        ("empty file", String::new()),
        ("BOM + original", format!("\u{feff}{original}")),
        ("trailing garbage", format!("{original} x")),
        ("two documents", format!("{original}{original}")),
    ] {
        cases.push((format!("shape: {label}"), "content_changed", body));
    }
    // a layout that says the same thing
    cases.push((
        "a \\u escape of the same text".into(),
        "readable",
        original.replace("sam", "\\u0073am"),
    ));
    let reformatted = {
        let mut keys: Vec<(String, Value)> = doc
            .as_object()
            .expect("object")
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        keys.reverse();
        let body = keys
            .iter()
            .map(|(k, v)| format!("\n\t {} :\n {}", serde_json::to_string(k).expect("key"), v))
            .collect::<Vec<_>>()
            .join(" ,");
        format!("  {{{body}\n}}\n\n")
    };
    cases.push((
        "keys reversed, whitespace changed".into(),
        "readable",
        reformatted,
    ));

    let changed = cases
        .iter()
        .filter(|(_, want, _)| *want == "content_changed")
        .count();
    assert!(
        changed >= 60,
        "the tamper set is meant to be 60 or more cases, it is {changed}"
    );
    let mut wrong = Vec::new();
    for (label, want, body) in &cases {
        std::fs::write(&path, body).expect("write the case");
        let got = a.standing();
        if got != *want {
            wrong.push(format!("{label}: wanted {want}, read {got}"));
        }
    }
    std::fs::write(&path, &original).expect("restore");
    assert!(
        wrong.is_empty(),
        "{} of {} cases: {wrong:#?}",
        wrong.len(),
        cases.len()
    );
}

/* ── the one server these tests read through ───────────────────────────── */

struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn start(home: &Path, repo: &Path, db: &Path) -> Self {
        let mut child = Command::new(bin())
            .env("HOME", home)
            .env("THRULINE_HOME", home.join(".thruline"))
            .env(PAID_GATE_SEAM.0, PAID_GATE_SEAM.1)
            .current_dir(repo)
            .arg("--db")
            .arg(db)
            .args(["ui", "--no-open", "--port", "0"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("thruline ui");
        let stdout = child.stdout.take().expect("stdout");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) => {
                    if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:")
                    {
                        let port: u16 = rest
                            .trim()
                            .split('/')
                            .next()
                            .unwrap_or_default()
                            .parse()
                            .expect("port in the printed URL");
                        return Self { child, port };
                    }
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("the server never printed its address");
                }
            }
        }
    }

    fn request(&self, verb: &str, path: &str, body: Option<&str>) -> String {
        let mut stream =
            TcpStream::connect(("127.0.0.1", self.port)).expect("connect to the server");
        let head = match body {
            Some(b) => format!(
                "{verb} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nOrigin: http://127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
                self.port,
                self.port,
                b.len()
            ),
            None => format!(
                "{verb} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                self.port
            ),
        };
        stream.write_all(head.as_bytes()).expect("write");
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).expect("read");
        let split = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("header terminator");
        String::from_utf8_lossy(&raw[split + 4..]).to_string()
    }

    fn get(&self, path: &str) -> String {
        self.request("GET", path, None)
    }

    fn post(&self, path: &str, body: &str) -> String {
        self.request("POST", path, Some(body))
    }
}
