//! Decisions CLI handlers use only a scratch ledger and scratch Git tree.

#[path = "../../core/tests/support/person_action.rs"]
mod person_action;

use person_action::PersonRecorder;

// Include the handler directly so these tests can exercise its typed API too.
#[allow(dead_code)]
#[path = "../src/decide_cmd.rs"]
mod decide_cmd;
#[allow(dead_code)]
#[path = "../src/json_out.rs"]
mod json_out;

use clap::Parser;
use decide_cmd::{Args, Verb};
use serde_json::json;
use thruline_core::chain::sha256_hex;
use thruline_core::decisions::authority::PersonActKind;
use thruline_core::decisions::params::{CheckParams, PathPresentParams, Surface, TextAbsentParams};
use thruline_core::decisions::storage::{
    self, AcceptanceInput, DecisionSource, NewDecision, RetirementInput, RevisionInput, SourceKind,
};
use thruline_core::ledger::Ledger;

fn fixture() -> (tempfile::TempDir, Ledger, std::path::PathBuf) {
    let temp = tempfile::tempdir().expect("scratch");
    let root = temp.path().join("repo");
    std::fs::create_dir(&root).expect("repo");
    let ledger = Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id,root_path,source,created_at) VALUES(?1,?2,'codex','2026-10-07T00:00:00Z')",
            ("project-a", root.to_str().expect("root text")),
        )
        .expect("project");
    (temp, ledger, root)
}

fn proposed(ledger: &Ledger, anchor: &str, path: &str) -> String {
    let span = format!("Keep {path}");
    storage::create(
        ledger,
        NewDecision {
            project_id: "project-a".into(),
            source_anchor: anchor.into(),
            span: span.clone(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::RuleFile,
                event_id: None,
                file_anchor: Some(anchor.into()),
                raw_file: "AGENTS.md".into(),
                raw_line: 1,
                span_start: 0,
                span_end: span.len() as u64,
                span,
                actor: Some("founder".into()),
                recorded_at: Some("2026-10-07T00:00:00Z".into()),
            }],
            author_actor: "founder".into(),
            recorded_at: "2026-10-07T00:00:00Z".into(),
            params: CheckParams::PathPresent(PathPresentParams {
                path: path.into(),
                sha256: None,
            }),
        },
    )
    .expect("proposal")
}

fn args(temp: &tempfile::TempDir, root: &std::path::Path, verb: Verb) -> Args {
    Args {
        db: temp.path().join("ledger.db"),
        project_id: "project-a".into(),
        project_root: root.to_path_buf(),
        verb,
        json: true,
    }
}

fn accept(id: &str, actor: &str, because: &str) -> Verb {
    accept_for_version(id, actor, because, "v1")
}

fn accept_for_version(id: &str, actor: &str, because: &str, version: &str) -> Verb {
    Verb::Accept {
        decision_id: id.into(),
        event_id: "missing-person-event".into(),
        revision: Some(1),
        actor: actor.into(),
        at: "2026-10-07T01:00:00Z".into(),
        because: because.into(),
        severity: "blocks".into(),
        applies_from_version: version.into(),
        terms: Vec::new(),
        allowlist: Vec::new(),
        surfaces: Vec::new(),
    }
}

fn accepted_verb(
    ledger: &Ledger,
    person: &mut PersonRecorder,
    id: &str,
    actor: &str,
    because: &str,
    version: &str,
) -> Verb {
    let (revision, params_json): (u32, String) = ledger.conn().query_row(
        "SELECT rev,params_json FROM decision_revisions WHERE decision_id=?1 ORDER BY rev DESC LIMIT 1",
        [id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).expect("proposed revision");
    let event_id = person.append(
        ledger,
        json!({
            "schema_version":1,"action":"accept","decision_id":id,
            "revision":revision,"params_sha256":sha256_hex(params_json.as_bytes()),
            "severity":"blocks","applies_from_version":version,
            "actor":actor,"because":because
        }),
    );
    let mut verb = accept_for_version(id, actor, because, version);
    if let Verb::Accept {
        event_id: ref mut recorded,
        revision: ref mut selected,
        ..
    } = verb
    {
        *recorded = event_id;
        *selected = Some(revision);
    }
    verb
}

fn accept_bridge(
    ledger: &Ledger,
    person: &mut PersonRecorder,
    root: &std::path::Path,
    id: &str,
    actor: &str,
    because: &str,
    version: &str,
) -> String {
    let revision: u32 = ledger
        .conn()
        .query_row(
            "SELECT max(rev) FROM decision_revisions WHERE decision_id=?1",
            [id],
            |row| row.get(0),
        )
        .expect("current revision");
    let act_id = storage::accept(
        ledger,
        AcceptanceInput {
            decision_id: id.into(),
            revision,
            actor: actor.into(),
            at: "2026-10-07T01:00:00Z".into(),
            because: because.into(),
            severity: "blocks".into(),
            applies_from_version: version.into(),
        },
    )
    .expect("prepared acceptance");
    person.bind_bridge_acceptance(ledger, &act_id);
    storage::write_derived_file(ledger, "project-a", root).expect("derived bridge copy");
    act_id
}

fn git(root: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git text")
        .trim()
        .into()
}

fn commit(root: &std::path::Path) -> String {
    git(root, &["init", "-q"]);
    std::fs::write(root.join("kept.txt"), "still here\n").expect("file");
    git(root, &["add", "kept.txt"]);
    git(
        root,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    );
    git(root, &["rev-parse", "HEAD"])
}

fn rows(ledger: &Ledger, table: &str) -> i64 {
    ledger
        .conn()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("count")
}

#[test]
fn c35_list_keeps_the_confirmed_revision_when_a_new_proposal_arrives() {
    let (temp, ledger, root) = fixture();
    let version = commit(&root);
    let id = proposed(&ledger, "rule:confirmed-selection", "kept.txt");
    let mut person = PersonRecorder::new(&ledger, "project-a");
    accept_bridge(
        &ledger,
        &mut person,
        &root,
        &id,
        "founder",
        "keep the agreed file",
        &version,
    );
    storage::append_revision(
        &ledger,
        RevisionInput {
            decision_id: id.clone(),
            author_actor: "repository".into(),
            recorded_at: "2026-10-07T02:00:00Z".into(),
            params: CheckParams::PathPresent(PathPresentParams {
                path: "different.txt".into(),
                sha256: None,
            }),
            span: "Keep different.txt".into(),
            sources: vec![DecisionSource {
                source_kind: SourceKind::RuleFile,
                event_id: None,
                file_anchor: Some("rule:new-proposal".into()),
                raw_file: "AGENTS.md".into(),
                raw_line: 1,
                span_start: 0,
                span_end: 18,
                span: "Keep different.txt".into(),
                actor: Some("repository".into()),
                recorded_at: Some("2026-10-07T02:00:00Z".into()),
            }],
        },
    )
    .expect("new proposal revision");
    let before = rows(&ledger, "decision_person_acts");
    let output = decide_cmd::execute(&ledger, &args(&temp, &root, Verb::List)).expect("list");
    let selected = &output.body["decisions"][0];
    assert_eq!(selected["lifecycle"], "accepted");
    assert_eq!(selected["revision"], 1);
    assert_eq!(selected["accepted_revision"], 1);
    assert_eq!(selected["params"]["path"], "kept.txt");
    assert_eq!(selected["span"], "Keep kept.txt");
    assert_eq!(selected["latest_proposed_revision"], 2);
    assert_eq!(rows(&ledger, "decision_person_acts"), before);
}

#[derive(Parser)]
struct CliHarness {
    #[command(subcommand)]
    verb: CliHarnessVerb,
}

#[derive(clap::Subcommand)]
enum CliHarnessVerb {
    #[command(visible_alias = "decisions")]
    Decide(Box<decide_cmd::DecisionRootCliArgs>),
}

#[test]
fn clap_exposes_singular_person_acts_and_plural_check() {
    let accepted = CliHarness::try_parse_from([
        "thruline",
        "decide",
        "accept",
        "d1",
        "--event-id",
        "person-event",
        "--actor",
        "founder",
        "--because",
        "Agreed",
        "--project",
        "project-a",
        "--json",
    ])
    .expect("accept syntax");
    let CliHarnessVerb::Decide(accept) = accepted.verb;
    assert_eq!(accept.project.project.as_deref(), Some("project-a"));
    assert!(accept.project.json);
    assert!(matches!(
        accept.verb,
        decide_cmd::DecisionRootCliVerb::Person(decide_cmd::DecideCliVerb::Accept { .. })
    ));

    let check = CliHarness::try_parse_from([
        "thruline",
        "decisions",
        "check",
        "--version",
        "efdc063e",
        "--id",
        "d1",
        "--json",
    ])
    .expect("plural check syntax");
    let CliHarnessVerb::Decide(check) = check.verb;
    assert!(check.project.json);
    assert!(matches!(
        check.verb,
        decide_cmd::DecisionRootCliVerb::Checks(decide_cmd::DecisionsCliVerb::Check { .. })
    ));
    assert!(
        CliHarness::try_parse_from(["thruline", "decisions", "check", "--all", "--id", "d1"])
            .is_err()
    );
}

fn cli(temp: &tempfile::TempDir, root: &std::path::Path, verb: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_thruline"))
        .arg("--db")
        .arg(temp.path().join("ledger.db"))
        .args(verb)
        .arg("--project")
        .arg("project-a")
        .arg("--root")
        .arg(root)
        .arg("--json")
        .output()
        .expect("CLI runs")
}

fn cli_plain(
    temp: &tempfile::TempDir,
    root: &std::path::Path,
    verb: &[&str],
) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_thruline"))
        .arg("--db")
        .arg(temp.path().join("ledger.db"))
        .args(verb)
        .arg("--project")
        .arg("project-a")
        .arg("--root")
        .arg(root)
        .output()
        .expect("CLI runs")
}

fn source_event(ledger: &Ledger, kind: &str, id: &str, text: &str) {
    ledger
        .conn()
        .execute(
            "INSERT OR IGNORE INTO sessions(id,project_id,file_path,started_at,last_seq)
         VALUES('source-session','project-a','session.jsonl','2026-10-06T19:42:04Z',0)",
            [],
        )
        .expect("source session");
    ledger.conn().execute(
        "INSERT INTO events(id,project_id,session_id,seq,ts,kind,role,text,raw_file,raw_line,raw_sha256,prev_hash,chain_hash)
         VALUES(?1,'project-a','source-session',?2,'2026-10-06T19:42:04.820Z',?3,?4,?5,'session.jsonl',3499,'raw','prev','chain')",
        rusqlite::params![id, if kind == "user_msg" { 1 } else { 2 }, kind,
            if kind == "user_msg" { "user" } else { "assistant" }, text],
    ).expect("source event");
}

#[test]
fn binary_propose_reads_only_the_person_event_and_keeps_exact_source_span() {
    let (temp, ledger, root) = fixture();
    let mut person = PersonRecorder::new(&ledger, "project-a");
    let person_event = person.append_text(
        &ledger,
        "Founder ruling C20: no free trial anywhere. Only approving a version is paid.",
    );
    source_event(
        &ledger,
        "assistant_text",
        "agent-event",
        "No free trial anywhere.",
    );
    let refused = cli(
        &temp,
        &root,
        &["decide", "propose", "--event-id", "agent-event"],
    );
    assert_ne!(
        refused.status.code(),
        Some(0),
        "agent source must not propose"
    );
    assert_eq!(rows(&ledger, "decisions"), 0);

    let proposed = cli(
        &temp,
        &root,
        &["decide", "propose", "--event-id", &person_event],
    );
    assert_eq!(
        proposed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&proposed.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&proposed.stdout).expect("propose JSON");
    assert_eq!(body["shape"], "thruline.decide.propose");
    assert_eq!(body["proposals"][0]["span"], "no free trial anywhere");
    assert_eq!(rows(&ledger, "decisions"), 1);
    assert_eq!(rows(&ledger, "decision_acceptances"), 0);
    let source: String = ledger
        .conn()
        .query_row(
            "SELECT sources_json FROM decision_revisions LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("source");
    let source: serde_json::Value = serde_json::from_str(&source).expect("source JSON");
    assert_eq!(source[0]["event_id"], person_event);
    assert_eq!(source[0]["raw_line"], 2);
    assert_eq!(source[0]["span"], "no free trial anywhere");
}

#[test]
fn person_can_fix_a1_terms_surfaces_and_allowlist_in_a_pending_revision() {
    let (temp, ledger, root) = fixture();
    let version = commit(&root);
    let mut person = PersonRecorder::new(&ledger, "project-a");
    let person_event = person.append_text(&ledger, "Founder ruling C20: no free trial anywhere.");
    let proposed = cli(
        &temp,
        &root,
        &["decide", "propose", "--event-id", &person_event],
    );
    assert_eq!(
        proposed.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&proposed.stderr)
    );
    let body: serde_json::Value = serde_json::from_slice(&proposed.stdout).expect("proposal JSON");
    let id = body["proposals"][0]["id"].as_str().expect("id");
    let accepted_params = CheckParams::TextAbsent(TextAbsentParams {
        test_allowlist: Vec::new(),
        json_field_allowlist: Vec::new(),
        conditional_path_allowlist: Vec::new(),
        span_allowlist: Vec::new(),
        terms: vec!["trial".into()],
        synonyms: vec![],
        surfaces: vec![
            Surface::Code,
            Surface::UiCopy,
            Surface::Docs,
            Surface::Tests,
            Surface::Ci,
            Surface::Other,
        ],
        allowlist: vec!["crates/bench/**".into(), "CHANGELOG.md".into()],
    });
    let event_id = person.append(
        &ledger,
        json!({
            "schema_version":1,"action":"accept","decision_id":id,
            "revision":2,"params_sha256":sha256_hex(accepted_params.json().unwrap().as_bytes()),
            "severity":"blocks","applies_from_version":version,
            "actor":"founder","because":"Trial must be gone"
        }),
    );
    let accepted = cli(
        &temp,
        &root,
        &[
            "decide",
            "accept",
            id,
            "--event-id",
            &event_id,
            "--actor",
            "founder",
            "--because",
            "Trial must be gone",
            "--from-version",
            &version,
            "--term",
            "trial",
            "--allow",
            "crates/bench/**",
            "--allow",
            "CHANGELOG.md",
            "--surface",
            "code",
            "--surface",
            "ui_copy",
            "--surface",
            "docs",
            "--surface",
            "tests",
            "--surface",
            "ci",
            "--surface",
            "other",
        ],
    );
    assert_eq!(
        accepted.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    let accepted_body: serde_json::Value =
        serde_json::from_slice(&accepted.stdout).expect("pending acceptance JSON");
    assert_eq!(accepted_body["lifecycle"], "pending_bridge");
    assert!(storage::accepted_for_project(&ledger, "project-a")
        .expect("active selection")
        .is_empty());
    let params: serde_json::Value = ledger
        .conn()
        .query_row(
            "SELECT params_json FROM decision_revisions WHERE decision_id=?1 AND rev=2",
            [id],
            |row| row.get::<_, String>(0),
        )
        .map(|raw| serde_json::from_str(&raw).expect("revision params"))
        .expect("person correction revision");
    assert_eq!(params["terms"], serde_json::json!(["trial"]));
    assert_eq!(
        params["allowlist"],
        serde_json::json!(["crates/bench/**", "CHANGELOG.md"])
    );
    assert_eq!(params["surfaces"].as_array().map(Vec::len), Some(6));
    assert_eq!(rows(&ledger, "decision_revisions"), 2);
    assert_eq!(rows(&ledger, "decision_acceptances"), 1);
    assert!(storage::verify_derived_file(&ledger, "project-a", &root).expect("derived"));
}

#[test]
fn binary_replay_compares_stored_result_and_missing_cache_is_unverifiable() {
    let (temp, ledger, root) = fixture();
    let version = commit(&root);
    let id = proposed(&ledger, "replay", "kept.txt");
    let mut person = PersonRecorder::new(&ledger, "project-a");
    accept_bridge(
        &ledger,
        &mut person,
        &root,
        &id,
        "founder",
        "Agreed",
        &version,
    );
    let first = cli(
        &temp,
        &root,
        &["decisions", "check", "--id", &id, "--version", &version],
    );
    assert_eq!(first.status.code(), Some(0));
    let replay = cli(
        &temp,
        &root,
        &[
            "decisions",
            "check",
            "--id",
            &id,
            "--version",
            &version,
            "--replay",
        ],
    );
    assert_eq!(
        replay.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    assert_eq!(
        rows(&ledger, "decision_verdicts"),
        1,
        "replay appends nothing"
    );
    std::fs::write(root.join("new.txt"), "new\n").expect("new file");
    git(&root, &["add", "new.txt"]);
    git(
        &root,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "new version",
        ],
    );
    let later = git(&root, &["rev-parse", "HEAD"]);
    let missing = cli(
        &temp,
        &root,
        &[
            "decisions",
            "check",
            "--id",
            &id,
            "--version",
            &later,
            "--replay",
        ],
    );
    assert_eq!(
        missing.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&missing.stderr)
    );
    assert_eq!(rows(&ledger, "decision_verdicts"), 1);
}

#[test]
fn human_broken_output_shows_surface_table_and_first_path_line() {
    let (temp, ledger, root) = fixture();
    let version = commit(&root);
    let id = proposed(&ledger, "table", "src/missing.rs");
    let mut person = PersonRecorder::new(&ledger, "project-a");
    accept_bridge(
        &ledger,
        &mut person,
        &root,
        &id,
        "founder",
        "Agreed",
        &version,
    );
    let checked = cli_plain(
        &temp,
        &root,
        &["decisions", "check", "--id", &id, "--version", &version],
    );
    assert_eq!(
        checked.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let text = String::from_utf8(checked.stdout).expect("plain text");
    assert!(text.contains("surface | state | findings"), "{text}");
    assert!(text.contains("code | broken | 1"), "{text}");
    assert!(text.contains("src/missing.rs:0"), "{text}");
}

#[test]
fn why_decision_and_verdict_ids_show_the_six_step_exact_tree_chain() {
    let (temp, ledger, root) = fixture();
    let version = commit(&root);
    let id = proposed(&ledger, "why-bridge", "src/missing.rs");
    let mut person = PersonRecorder::new(&ledger, "project-a");
    accept_bridge(
        &ledger,
        &mut person,
        &root,
        &id,
        "founder",
        "Agreed",
        &version,
    );
    let checked = cli(
        &temp,
        &root,
        &["decisions", "check", "--id", &id, "--version", &version],
    );
    assert_eq!(checked.status.code(), Some(3));
    let checked_body: serde_json::Value =
        serde_json::from_slice(&checked.stdout).expect("check JSON");
    assert_eq!(checked_body["shape"], "thruline.decide.check");
    let verdict_id: String = ledger
        .conn()
        .query_row(
            "SELECT id FROM decision_verdicts WHERE decision_id=?1",
            [&id],
            |row| row.get(0),
        )
        .expect("verdict");
    for selected in [&id, &verdict_id] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_thruline"))
            .arg("--db")
            .arg(temp.path().join("ledger.db"))
            .args(["why", selected, "--json"])
            .output()
            .expect("why runs");
        assert_eq!(
            output.status.code(),
            Some(0),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let body: serde_json::Value = serde_json::from_slice(&output.stdout).expect("proof JSON");
        assert_eq!(body["decision_id"], id);
        assert_eq!(body["verdict_id"], verdict_id);
        assert_eq!(body["selected_version_id"], version);
        assert_eq!(body["display_state"], "broken");
        assert_eq!(body["chain"].as_array().map(Vec::len), Some(6));
    }
}

#[test]
fn legacy_binary_person_acts_remain_pending_and_regenerate_an_inert_derived_file() {
    let (temp, ledger, root) = fixture();
    let version = commit(&root);
    let id = proposed(&ledger, "from-cli", "kept.txt");
    let mut person = PersonRecorder::new(&ledger, "project-a");
    let Verb::Accept { event_id, .. } =
        accepted_verb(&ledger, &mut person, &id, "founder", "Agreed", &version)
    else {
        panic!("acceptance verb")
    };
    let accepted = cli(
        &temp,
        &root,
        &[
            "decide",
            "accept",
            &id,
            "--event-id",
            &event_id,
            "--actor",
            "founder",
            "--because",
            "Agreed",
            "--from-version",
            &version,
        ],
    );
    assert_eq!(
        accepted.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&accepted.stderr)
    );
    let accepted_body: serde_json::Value =
        serde_json::from_slice(&accepted.stdout).expect("accept JSON");
    assert_eq!(accepted_body["shape"], "thruline.decide.accept");
    assert_eq!(accepted_body["lifecycle"], "pending_bridge");
    assert!(storage::verify_derived_file(&ledger, "project-a", &root).expect("derived"));
    let listed = cli(&temp, &root, &["decide", "list"]);
    assert_eq!(listed.status.code(), Some(0));
    let list: serde_json::Value = serde_json::from_slice(&listed.stdout).expect("list JSON");
    assert_eq!(list["shape"], "thruline.decide.list");
    assert_eq!(list["decisions"][0]["lifecycle"], "proposed");
    let retirement_event = person.append(
        &ledger,
        json!({
            "schema_version":1,"action":"retire","decision_id":id.clone(),
            "actor":"founder","because":"Superseded","superseded_by":null
        }),
    );
    let retired = cli(
        &temp,
        &root,
        &[
            "decide",
            "retire",
            &id,
            "--event-id",
            &retirement_event,
            "--actor",
            "founder",
            "--because",
            "Superseded",
        ],
    );
    assert_eq!(
        retired.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&retired.stderr)
    );
    let retired_body: serde_json::Value =
        serde_json::from_slice(&retired.stdout).expect("retire JSON");
    assert_eq!(retired_body["shape"], "thruline.decide.retire");
    assert_eq!(retired_body["lifecycle"], "pending_bridge");
    assert!(
        storage::verify_derived_file(&ledger, "project-a", &root).expect("derived after retire")
    );
    let after = cli(&temp, &root, &["decide", "list"]);
    let after: serde_json::Value =
        serde_json::from_slice(&after.stdout).expect("list after retire");
    assert_eq!(after["decisions"][0]["lifecycle"], "proposed");
}

#[test]
fn confirmed_act_finalizes_the_derived_file_and_repair_needs_no_new_confirmation() {
    let (temp, ledger, root) = fixture();
    let version = commit(&root);
    let id = proposed(&ledger, "bridge-finalize", "kept.txt");
    storage::write_derived_file(&ledger, "project-a", &root).expect("initial inert copy");
    let act_id = storage::accept(
        &ledger,
        AcceptanceInput {
            decision_id: id.clone(),
            revision: 1,
            actor: "founder".into(),
            at: "2026-10-07T01:00:00Z".into(),
            because: "Standing choice".into(),
            severity: "blocks".into(),
            applies_from_version: version,
        },
    )
    .expect("prepared act");
    assert!(
        decide_cmd::finalize_confirmed_act(
            &ledger,
            "project-a",
            &root,
            PersonActKind::Accept,
            &act_id,
        )
        .is_err(),
        "pending act cannot rewrite the active derived copy"
    );
    let mut person = PersonRecorder::bridge_only(&ledger, "project-a");
    person.bind_bridge_acceptance(&ledger, &act_id);
    assert!(
        !storage::verify_derived_file(&ledger, "project-a", &root).expect("stale copy"),
        "the copy must be stale before confirmation finalization"
    );
    decide_cmd::finalize_confirmed_act(&ledger, "project-a", &root, PersonActKind::Accept, &act_id)
        .expect("confirmed act refreshes copy");
    assert!(storage::verify_derived_file(&ledger, "project-a", &root).expect("fresh copy"));
    std::fs::remove_file(root.join(".thruline/decisions.yml")).expect("simulate write loss");
    let repaired = cli(&temp, &root, &["decide", "repair-derived"]);
    assert_eq!(
        repaired.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&repaired.stderr)
    );
    assert!(storage::verify_derived_file(&ledger, "project-a", &root).expect("repaired copy"));
    assert_eq!(rows(&ledger, "decision_person_acts"), 1);
}

#[test]
fn binary_decisions_check_uses_documented_process_exit_codes() {
    let (temp, ledger, root) = fixture();
    let version = commit(&root);
    let held = proposed(&ledger, "binary-held", "kept.txt");
    let broken = proposed(&ledger, "binary-broken", "missing.txt");
    let mut person = PersonRecorder::new(&ledger, "project-a");
    for id in [&held, &broken] {
        accept_bridge(
            &ledger,
            &mut person,
            &root,
            id,
            "founder",
            "Agreed",
            &version,
        );
    }
    let held_run = cli(
        &temp,
        &root,
        &["decisions", "check", "--id", &held, "--version", &version],
    );
    assert_eq!(
        held_run.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&held_run.stderr)
    );
    let broken_run = cli(&temp, &root, &["decisions", "check", "--version", &version]);
    assert_eq!(
        broken_run.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&broken_run.stderr)
    );
    let unknown_run = cli(
        &temp,
        &root,
        &[
            "decisions",
            "check",
            "--id",
            &held,
            "--version",
            "missing-ref",
        ],
    );
    assert_eq!(
        unknown_run.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&unknown_run.stderr)
    );
}

#[test]
fn list_shows_proposed_accepted_and_retired_and_writes_derived_copy_after_each_act() {
    let (temp, ledger, root) = fixture();
    let proposed_id = proposed(&ledger, "proposed", "proposal.txt");
    let accepted_id = proposed(&ledger, "accepted", "accepted.txt");
    let retired_id = proposed(&ledger, "retired", "retired.txt");
    let mut person = PersonRecorder::new(&ledger, "project-a");

    accept_bridge(
        &ledger,
        &mut person,
        &root,
        &accepted_id,
        "founder",
        "Approved",
        "v1",
    );
    accept_bridge(
        &ledger,
        &mut person,
        &root,
        &retired_id,
        "founder",
        "Approved",
        "v1",
    );
    assert!(
        storage::verify_derived_file(&ledger, "project-a", &root).expect("derived after accept")
    );
    let retirement_id = storage::retire(
        &ledger,
        RetirementInput {
            decision_id: retired_id.clone(),
            actor: "founder".into(),
            at: "2026-10-07T02:00:00Z".into(),
            because: "Superseded".into(),
            superseded_by: None,
        },
    )
    .expect("prepared retirement");
    person.bind_bridge_retirement(&ledger, &retirement_id);
    storage::write_derived_file(&ledger, "project-a", &root).expect("derived after retirement");
    assert!(
        storage::verify_derived_file(&ledger, "project-a", &root).expect("derived after retire")
    );

    let list = decide_cmd::execute(&ledger, &args(&temp, &root, Verb::List)).expect("list");
    assert_eq!(list.exit_code, 0);
    let decisions = list.body["decisions"].as_array().expect("decisions");
    assert_eq!(decisions.len(), 3);
    for (id, state) in [
        (proposed_id, "proposed"),
        (accepted_id, "accepted"),
        (retired_id, "retired"),
    ] {
        let found = decisions
            .iter()
            .find(|decision| decision["id"] == id)
            .expect("listed");
        assert_eq!(found["lifecycle"], state);
    }
    assert_eq!(rows(&ledger, "decision_acceptances"), 2);
    assert_eq!(rows(&ledger, "decision_retirements"), 1);
}

#[test]
fn accept_requires_person_fields_and_does_not_write_derived_copy_on_refusal() {
    let (temp, ledger, root) = fixture();
    let id = proposed(&ledger, "rule", "kept.txt");
    assert!(
        decide_cmd::execute(&ledger, &args(&temp, &root, accept(&id, "", "Approved"))).is_err()
    );
    assert!(decide_cmd::execute(&ledger, &args(&temp, &root, accept(&id, "founder", ""))).is_err());
    assert_eq!(rows(&ledger, "decision_acceptances"), 0);
    assert!(!root.join(".thruline/decisions.yml").exists());
    let mut person = PersonRecorder::new(&ledger, "project-a");
    decide_cmd::execute(
        &ledger,
        &args(
            &temp,
            &root,
            accepted_verb(&ledger, &mut person, &id, "founder", "Approved", "v1"),
        ),
    )
    .expect("accept");
    assert_eq!(rows(&ledger, "decision_acceptances"), 1);
    assert!(storage::verify_derived_file(&ledger, "project-a", &root).expect("derived"));
}

#[test]
fn checks_use_exact_commit_and_return_holds_broken_and_unverifiable_exit_codes() {
    let (temp, ledger, root) = fixture();
    let version = commit(&root);
    let held = proposed(&ledger, "held", "kept.txt");
    let broken = proposed(&ledger, "broken", "missing.txt");
    let mut person = PersonRecorder::new(&ledger, "project-a");
    for id in [&held, &broken] {
        accept_bridge(
            &ledger,
            &mut person,
            &root,
            id,
            "founder",
            "Approved",
            &version,
        );
    }
    let held_check = decide_cmd::execute(
        &ledger,
        &args(
            &temp,
            &root,
            Verb::Check {
                version_id: version.clone(),
                decision_id: Some(held.clone()),
                at: "2026-10-07T03:00:00Z".into(),
                replay: false,
            },
        ),
    )
    .expect("check held");
    assert_eq!(held_check.exit_code, 0);
    assert_eq!(held_check.body["verdicts"][0]["state"], "holds");

    let broken_check = decide_cmd::execute(
        &ledger,
        &args(
            &temp,
            &root,
            Verb::Check {
                version_id: version.clone(),
                decision_id: None,
                at: "2026-10-07T03:01:00Z".into(),
                replay: false,
            },
        ),
    )
    .expect("check all");
    assert_eq!(broken_check.exit_code, 3);
    assert_eq!(
        rows(&ledger, "decision_verdicts"),
        2,
        "same tree and check uses the cached held verdict"
    );
    let tree: String = ledger.conn().query_row(
        "SELECT tree_hash FROM decision_verdicts WHERE decision_id=?1 ORDER BY rowid DESC LIMIT 1", [&broken], |row| row.get(0),
    ).expect("recorded tree");
    assert_eq!(tree, git(&root, &["rev-parse", "HEAD^{tree}"]));

    let unavailable = decide_cmd::execute(
        &ledger,
        &args(
            &temp,
            &root,
            Verb::Check {
                version_id: "not-a-commit".into(),
                decision_id: Some(held),
                at: "2026-10-07T03:02:00Z".into(),
                replay: false,
            },
        ),
    )
    .expect("unverifiable check");
    assert_eq!(unavailable.exit_code, 4);
    assert_eq!(unavailable.body["verdicts"][0]["state"], "unverifiable");
    assert_eq!(
        rows(&ledger, "decision_verdicts"),
        2,
        "unknown tree cannot be recorded as an exact-version verdict"
    );
}

#[test]
fn check_id_must_name_an_accepted_decision_in_this_project() {
    let (temp, ledger, root) = fixture();
    let version = commit(&root);
    let proposed_id = proposed(&ledger, "unaccepted", "kept.txt");
    for id in [&proposed_id, &"unknown".to_string()] {
        assert!(decide_cmd::execute(
            &ledger,
            &args(
                &temp,
                &root,
                Verb::Check {
                    version_id: version.clone(),
                    decision_id: Some(id.clone()),
                    at: "2026-10-07T03:00:00Z".into(),
                    replay: false,
                }
            )
        )
        .is_err());
    }
    assert_eq!(rows(&ledger, "decision_verdicts"), 0);
}
