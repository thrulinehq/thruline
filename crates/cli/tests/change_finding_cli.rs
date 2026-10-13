//! T-4-2 — `thruline change finding`, through the compiled binary.
//!
//! The terminal door calls the same function the screen's write route calls,
//! so it refuses for the same reasons, in the same words, and leaves rows of
//! the same shape. A real store under a temporary HOME; every string is
//! synthetic.

use std::path::Path;
use std::process::Command;

use thruline_core::change::{
    self, AcceptedScope, ChangeRequest, NewChange, RequestSource, ScopeAcceptance,
};
use thruline_core::change_review::{self, NewReview, ReviewFinding, ReviewState, ReviewerType};
use thruline_core::finding_standing::{self, refusal_line, Standing, Via};
use thruline_core::ledger::Ledger;

const CHANGE: &str = "chg-cli-standing";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
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
        .env("USER", "sam")
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

fn store(dir: &Path) -> std::path::PathBuf {
    let db = dir.join("ledger.db");
    let ledger = Ledger::open(&db).expect("ledger");
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO projects(id,root_path,source,display_name,created_at)
               VALUES('p1','/synthetic','claude_code','Synthetic','2026-01-01T00:00:00Z');",
        )
        .expect("project");
    change::insert_change(
        &ledger,
        &NewChange {
            id: CHANGE.into(),
            project_id: "p1".into(),
            opened_at: "2026-01-02T00:00:00Z".into(),
            request: ChangeRequest {
                title: "SYNTHETIC calculator".into(),
                purpose: "add() should add".into(),
                prompt_event_id: None,
                requester: "sam".into(),
                source: RequestSource::Person,
            },
        },
    )
    .expect("change");
    change::append_scope(
        &ledger,
        CHANGE,
        "2026-01-02T00:01:00Z",
        &AcceptedScope {
            criteria: vec!["addition adds".into()],
            exclusions: vec![],
            allowed_scope: vec![],
            constraints: vec![],
            check_profile: vec![],
        },
    )
    .expect("scope");
    change::accept_scope(
        &ledger,
        CHANGE,
        1,
        &ScopeAcceptance {
            actor: "sam".into(),
            accepted_at: "2026-01-02T00:02:00Z".into(),
            rationale: "synthetic".into(),
        },
    )
    .expect("accept");
    ledger
        .conn()
        .execute(
            "INSERT INTO examination_inputs(id,subject_kind,subject_id,recorded_at,schema_version,
                 base_revision,uncommitted_state,config_files_json,tools_json,target_kind)
               VALUES('exi-cli','intended',?1,'2026-01-02T00:03:00Z',1,
                 'abcdef1234567890abcdef1234567890abcdef12','clean','[]','[]','none')",
            [CHANGE],
        )
        .expect("version");
    change_review::append_review(
        &ledger,
        &NewReview {
            id: "rev-cli".into(),
            change_id: CHANGE.into(),
            review_round: 1,
            reviewer_identity: "PR-Agent 0.45.0 via qwen3:8b".into(),
            reviewer_type: ReviewerType::Model,
            review_source: "local/plain-diff".into(),
            reviewer_relationship: "not established".into(),
            relationship_provenance: "synthetic".into(),
            target_revision: "abcdef1234567890abcdef1234567890abcdef12".into(),
            scope_version: Some(1),
            examined_inputs_id: "exi-cli".into(),
            diff_sha256: "d".repeat(64),
            route_identity_json: serde_json::json!({
                "endpoint": change_review::ROUTE_ENDPOINT,
                "model": "qwen3:8b",
                "model_manifest_sha256": "5".repeat(64),
                "runtime_sha256": "4".repeat(64),
                "provider_source_commit": change_review::PR_AGENT_SOURCE_COMMIT,
                "image_id": format!("sha256:{}", "9".repeat(64)),
                "plan_digest": "e".repeat(64),
            })
            .to_string(),
            started_at: "2026-01-02T00:04:00Z".into(),
            ended_at: Some("2026-01-02T00:05:00Z".into()),
            state: ReviewState::Complete,
            recommendation: None,
            limitations: vec![],
            raw_bytes: b"{}".to_vec(),
            parser_id: "thruline.pr-agent-structured.v1".into(),
            parsed_json: "{}".into(),
            parse_complete: true,
            findings: vec![
                ReviewFinding {
                    kind: "SYNTHETIC: addition subtracts".into(),
                    message: "The operator subtracts.".into(),
                    path: Some("calculator.py".into()),
                    line: Some(2),
                },
                ReviewFinding {
                    kind: "SYNTHETIC: naming".into(),
                    message: "Rename x.".into(),
                    path: Some("calculator.py".into()),
                    line: Some(1),
                },
            ],
        },
    )
    .expect("review");
    db
}

fn view(db: &Path) -> finding_standing::FindingsView {
    finding_standing::read(&Ledger::open(db).expect("ledger"), CHANGE).expect("read")
}

#[test]
fn a_waiver_from_a_terminal_needs_a_reason_and_an_authority_and_says_which_is_missing() {
    let home = tempfile::tempdir().expect("home");
    let db = store(home.path());
    let bare = run(
        home.path(),
        &db,
        &[
            "change",
            "finding",
            "--change",
            CHANGE,
            "--finding",
            "rev-cli:0",
            "--to",
            "waived",
        ],
    );
    assert_eq!(bare.code, 1, "{}{}", bare.stdout, bare.stderr);
    assert!(
        bare.stdout.contains(refusal_line("missing")),
        "{}",
        bare.stdout
    );
    assert!(
        bare.stdout.contains("the reason, why you can decide this"),
        "{}",
        bare.stdout
    );
    assert!(
        view(&db).findings[0].history.is_empty(),
        "nothing was written"
    );

    let full = run(
        home.path(),
        &db,
        &[
            "change",
            "finding",
            "--change",
            CHANGE,
            "--finding",
            "rev-cli:0",
            "--to",
            "waived",
            "--reason",
            "The client asked for subtraction.",
            "--authority",
            "I own this project",
        ],
    );
    assert_eq!(full.code, 0, "{}{}", full.stdout, full.stderr);
    assert_eq!(full.stdout.trim(), "Still open → Waived");
    let f = &view(&db).findings[0];
    assert_eq!(f.standing, Standing::Waived);
    let row = f.latest.as_ref().expect("row");
    assert_eq!(
        row.actor, "sam",
        "the account this ran as is the name on it"
    );
    assert_eq!(row.via, Via::Person);
}

#[test]
fn a_resolved_mark_from_a_terminal_reaches_waiting_and_never_fixed() {
    let home = tempfile::tempdir().expect("home");
    let db = store(home.path());
    let too_far = run(
        home.path(),
        &db,
        &[
            "change",
            "finding",
            "--change",
            CHANGE,
            "--finding",
            "rev-cli:1",
            "--to",
            "fixed_with_evidence",
            "--via",
            "synthetic_resolved_import",
            "--evidence-review",
            "rev-cli",
        ],
    );
    assert_eq!(too_far.code, 1);
    assert!(
        too_far
            .stdout
            .contains(refusal_line("mark_goes_no_further")),
        "{}",
        too_far.stdout
    );

    let marked = run(
        home.path(),
        &db,
        &[
            "change",
            "finding",
            "--change",
            CHANGE,
            "--finding",
            "rev-cli:1",
            "--to",
            "awaiting_verification",
            "--via",
            "synthetic_resolved_import",
            "--json",
        ],
    );
    assert_eq!(marked.code, 0, "{}{}", marked.stdout, marked.stderr);
    let doc: serde_json::Value = serde_json::from_str(&marked.stdout).expect("json");
    assert_eq!(doc["shape"], "thruline.change.finding");
    assert_eq!(doc["acted"]["to"], "awaiting_verification");
    let f = &view(&db).findings[1];
    assert_eq!(f.standing, Standing::AwaitingVerification);
    assert!(!f.counts_as_fixed);
    assert_eq!(
        f.latest.as_ref().expect("row").via,
        Via::SyntheticResolvedImport,
        "a synthetic import is stored as exactly that"
    );
}

#[test]
fn the_terminal_door_refuses_a_store_that_needs_an_upgrade_and_pays_nothing() {
    let home = tempfile::tempdir().expect("home");
    let db = store(home.path());
    let conn = rusqlite::Connection::open(&db).expect("open");
    conn.execute("DELETE FROM schema_migrations", [])
        .expect("clear");
    drop(conn);
    let out = run(
        home.path(),
        &db,
        &[
            "change",
            "finding",
            "--change",
            CHANGE,
            "--finding",
            "rev-cli:0",
            "--to",
            "under_repair",
        ],
    );
    assert_eq!(out.code, 3, "{}{}", out.stdout, out.stderr);
    assert!(out.stderr.contains("thruline upgrade"), "{}", out.stderr);
    let conn = rusqlite::Connection::open(&db).expect("open");
    let rows: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM change_review_finding_standings",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(rows, 0);
}

/// ADV-review-input-1 (adversary F3): the terminal door refuses a reason and
/// an authority nobody can see, in the copy file's words, and writes nothing.
/// An invisible `--actor` is treated as no `--actor`, exactly like an empty
/// one: the account the command runs as is the name on the row.
#[test]
fn a_waiver_from_a_terminal_with_an_invisible_reason_and_authority_is_refused() {
    let home = tempfile::tempdir().expect("home");
    let db = store(home.path());
    let invisible = run(
        home.path(),
        &db,
        &[
            "change",
            "finding",
            "--change",
            CHANGE,
            "--finding",
            "rev-cli:0",
            "--to",
            "waived",
            "--actor",
            "\u{200B}",
            "--reason",
            "\u{200B}",
            "--authority",
            "\u{FEFF}\u{2060}",
        ],
    );
    assert_eq!(
        invisible.code, 1,
        "{}{}",
        invisible.stdout, invisible.stderr
    );
    assert!(
        invisible.stdout.contains(refusal_line("missing")),
        "{}",
        invisible.stdout
    );
    assert!(
        invisible
            .stdout
            .contains("the reason, why you can decide this"),
        "{}",
        invisible.stdout
    );
    assert!(
        view(&db).findings[0].history.is_empty(),
        "nothing was written"
    );

    let edged = run(
        home.path(),
        &db,
        &[
            "change",
            "finding",
            "--change",
            CHANGE,
            "--finding",
            "rev-cli:0",
            "--to",
            "waived",
            "--actor",
            "\u{200B}",
            "--reason",
            "\u{200B}The client asked for subtraction.\u{200B}",
            "--authority",
            "\u{FEFF}I own this project",
        ],
    );
    assert_eq!(edged.code, 0, "{}{}", edged.stdout, edged.stderr);
    let row = view(&db).findings[0].latest.clone().expect("row");
    assert_eq!(row.actor, "sam", "an invisible --actor is no --actor");
    assert_eq!(row.reason, "The client asked for subtraction.");
    assert_eq!(row.authority.as_deref(), Some("I own this project"));
}

/// ADV-review-input-2 (adversary F4): the terminal door and the screen's route
/// refuse the same thing. A reason of 2,001 characters is refused from a
/// terminal in the copy file's words, naming the part, and nothing is
/// written; one of exactly 2,000 characters is saved whole, as it is through
/// the route (`uiserve.rs`,
/// `a_long_reason_is_refused_and_saved_by_the_same_rule_as_the_terminal`).
#[test]
fn a_reason_over_two_thousand_characters_is_refused_from_a_terminal_as_it_is_from_the_screen() {
    let home = tempfile::tempdir().expect("home");
    let db = store(home.path());
    let over = "\u{8A9E}".repeat(2001);
    let refused = run(
        home.path(),
        &db,
        &[
            "change",
            "finding",
            "--change",
            CHANGE,
            "--finding",
            "rev-cli:0",
            "--to",
            "disputed",
            "--reason",
            &over,
            "--authority",
            "I own this project",
        ],
    );
    assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
    assert_eq!(
        refused.stdout,
        format!("{}\n  the reason\n", refusal_line("too_long")),
        "the copy file's sentence, then the part that is over"
    );
    assert!(
        view(&db).findings[0].history.is_empty(),
        "nothing was written"
    );

    let at = "\u{8A9E}".repeat(2000);
    let saved = run(
        home.path(),
        &db,
        &[
            "change",
            "finding",
            "--change",
            CHANGE,
            "--finding",
            "rev-cli:0",
            "--to",
            "disputed",
            "--reason",
            &at,
            "--authority",
            "I own this project",
        ],
    );
    assert_eq!(saved.code, 0, "{}{}", saved.stdout, saved.stderr);
    let row = view(&db).findings[0].latest.clone().expect("row");
    assert_eq!(row.reason, at);
}

/// ADV-review-input-4 (the pre-gate's variant of F3): the pre-gate's own
/// terminal act — `--actor Sam --authority U+FFFA --reason U+FFF9U+FFFB`,
/// which printed "Still open → Waived" — is refused in the copy file's words,
/// naming the reason and the authority, and nothing is written. So is an
/// authority and a reason of one combining accent with nothing under it. An
/// `--actor` that holds nothing a person can read is no `--actor`: the account
/// the command runs as is the name on the row, exactly as for an empty one.
#[test]
fn a_waiver_from_a_terminal_whose_authority_and_reason_hold_nothing_readable_is_refused() {
    let home = tempfile::tempdir().expect("home");
    let db = store(home.path());
    for (authority, reason) in [("\u{FFFA}", "\u{FFF9}\u{FFFB}"), ("\u{0301}", "\u{0301}")] {
        let refused = run(
            home.path(),
            &db,
            &[
                "change",
                "finding",
                "--change",
                CHANGE,
                "--finding",
                "rev-cli:0",
                "--to",
                "waived",
                "--actor",
                "Sam",
                "--authority",
                authority,
                "--reason",
                reason,
            ],
        );
        assert_eq!(refused.code, 1, "{}{}", refused.stdout, refused.stderr);
        assert_eq!(
            refused.stdout,
            format!(
                "{}\n  the reason, why you can decide this\n",
                refusal_line("missing")
            ),
            "the copy file's sentence, then the parts that are empty"
        );
        assert!(
            view(&db).findings[0].history.is_empty(),
            "nothing was written"
        );
    }

    let unnamed = run(
        home.path(),
        &db,
        &[
            "change",
            "finding",
            "--change",
            CHANGE,
            "--finding",
            "rev-cli:0",
            "--to",
            "waived",
            "--actor",
            "\u{FFFA}\u{0301}",
            "--reason",
            "The client asked for subtraction.",
            "--authority",
            "I own this project",
        ],
    );
    assert_eq!(unnamed.code, 0, "{}{}", unnamed.stdout, unnamed.stderr);
    let row = view(&db).findings[0].latest.clone().expect("row");
    assert_eq!(row.actor, "sam", "an unreadable --actor is no --actor");
}

/// FIXED-WORDING (R-2026-09-28-9): the code moves to a second version, and a
/// second review of it raises `findings`.
fn second_round(db: &Path, findings: Vec<ReviewFinding>) {
    let ledger = Ledger::open(db).expect("ledger");
    let second = "fedcba0987654321fedcba0987654321fedcba09";
    ledger
        .conn()
        .execute(
            "INSERT INTO examination_inputs(id,subject_kind,subject_id,recorded_at,schema_version,
                 base_revision,uncommitted_state,config_files_json,tools_json,target_kind)
               VALUES('exi-cli-2','intended',?1,'2026-01-03T00:00:00Z',1,?2,'clean','[]','[]','none')",
            [CHANGE, second],
        )
        .expect("second version");
    change_review::append_review(
        &ledger,
        &NewReview {
            id: "rev-cli-2".into(),
            change_id: CHANGE.into(),
            review_round: 2,
            reviewer_identity: "PR-Agent 0.45.0 via qwen3:8b".into(),
            reviewer_type: ReviewerType::Model,
            review_source: "local/plain-diff".into(),
            reviewer_relationship: "not established".into(),
            relationship_provenance: "synthetic".into(),
            target_revision: second.into(),
            scope_version: Some(1),
            examined_inputs_id: "exi-cli-2".into(),
            diff_sha256: "e".repeat(64),
            route_identity_json: serde_json::json!({
                "endpoint": change_review::ROUTE_ENDPOINT,
                "model": "qwen3:8b",
                "model_manifest_sha256": "5".repeat(64),
                "runtime_sha256": "4".repeat(64),
                "provider_source_commit": change_review::PR_AGENT_SOURCE_COMMIT,
                "image_id": format!("sha256:{}", "9".repeat(64)),
                "plan_digest": "e".repeat(64),
            })
            .to_string(),
            started_at: "2026-01-03T00:04:00Z".into(),
            ended_at: Some("2026-01-03T00:05:00Z".into()),
            state: ReviewState::Complete,
            recommendation: None,
            limitations: vec![],
            raw_bytes: b"{}".to_vec(),
            parser_id: "thruline.pr-agent-structured.v1".into(),
            parsed_json: "{}".into(),
            parse_complete: true,
            findings,
        },
    )
    .expect("second review");
}

/// The rename-and-reword attack, from a terminal: the bug in `calculator.py`
/// comes back in round 2 as `calc.py`, in other words. The fix is saved, and
/// what the terminal says is only what the record shows, with the reworded
/// finding printed right under "Fixed".
#[test]
fn a_fix_from_a_terminal_says_only_what_the_record_shows_and_lists_what_else_the_re_check_raised() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db = store(temp.path());
    second_round(
        &db,
        vec![ReviewFinding {
            kind: "SYNTHETIC: sum is off".into(),
            message: "The total is wrong.".into(),
            path: Some("calc.py".into()),
            line: Some(2),
        }],
    );
    let out = run(
        temp.path(),
        &db,
        &[
            "change",
            "finding",
            "--change",
            CHANGE,
            "--finding",
            "rev-cli:0",
            "--to",
            "fixed_with_evidence",
            "--evidence-review",
            "rev-cli-2",
        ],
    );
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout.lines().collect::<Vec<_>>(),
        vec![
            "Still open → Fixed",
            "Fixed: the re-check didn't raise this finding at calculator.py, line 2",
            "The same re-check raised 1 other thing. Look at it in case it is this one, reworded or moved:",
            "  - calc.py, line 2: SYNTHETIC: sum is off — The total is wrong.",
        ]
    );
    assert_eq!(view(&db).findings[0].standing, Standing::FixedWithEvidence);
}

#[test]
fn a_fix_from_a_terminal_whose_re_check_raised_nothing_else_says_so() {
    let temp = tempfile::tempdir().expect("tempdir");
    let db = store(temp.path());
    second_round(&db, vec![]);
    let out = run(
        temp.path(),
        &db,
        &[
            "change",
            "finding",
            "--change",
            CHANGE,
            "--finding",
            "rev-cli:1",
            "--to",
            "fixed_with_evidence",
            "--evidence-review",
            "rev-cli-2",
        ],
    );
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    assert_eq!(
        out.stdout.lines().collect::<Vec<_>>(),
        vec![
            "Still open → Fixed",
            "Fixed: the re-check didn't raise this finding at calculator.py, line 1",
            "The same re-check raised nothing else.",
        ]
    );
}
