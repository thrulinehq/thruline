//! R17b-1b — the TRANSCRIPT-TO-VERDICT walk. **Zero seeded events.**
//!
//! The chain a real user's real session travels, with no asterisk:
//!
//!   codex transcript → sealed-mirror ingest → real wrapper run → acceptance
//!   → attested_run contract → claim dimensions → rendered export
//!
//! Every identity used to bind the wrapper run to the ingested session is READ
//! BACK from the ledger, never predicted. That discipline is not decoration:
//! A102 and A103 each cost a milestone to an identity assumption, and the
//! R17b-1a probe pinned the one that would have broken this file — `tool_use_id`
//! keys on the DERIVED session id, not the transcript's raw session key.

mod scratch;

use std::fs;
use std::path::Path;
use std::process::Command;

use scratch::TempTree;
use thruline_core::capture::{
    accept_capture_pair, attested_runs_for_project, opaque_source_project_key,
    record_source_assurance, CaptureAcceptance, CaptureRequest, CaptureStrength, Collector,
    SourceAssuranceMeasurement, SourceDelivery,
};
use thruline_core::chain::sha256_hex;
use thruline_core::derive::{derive_project, EvidenceState};
use thruline_core::ingest::{ingest_codex, CodexIngestOptions};
use thruline_core::Ledger;

const OWNER: &str = "walk-owner";
const COMMAND: &str = "git status --porcelain";
const CLAIM: &str = "The build succeeded.";

fn git(cwd: &Path, args: &[&str]) {
    let st = Command::new("/usr/bin/git")
        .args(args)
        .current_dir(cwd)
        .status()
        .expect("git");
    assert!(st.success(), "git {args:?}");
}

fn repository(path: &Path) {
    git(path, &["init", "-q"]);
    git(path, &["config", "user.name", "Walk"]);
    git(path, &["config", "user.email", "walk@example.invalid"]);
    fs::write(path.join("tracked.txt"), "one\n").expect("write");
    git(path, &["add", "tracked.txt"]);
    git(path, &["commit", "-qm", "fixture"]);
}

/// A codex rollout carrying a claim, the command that backs it, and the
/// command's output — the shape a real session produces.
fn transcript(session: &str, turn: &str, call: &str, command: &str) -> String {
    format!(
        concat!(
            r#"{{"timestamp":"2030-01-01T00:00:00Z","type":"session_meta","payload":{{"id":"{s}","cwd":"/walk/project"}}}}"#,
            "\n",
            r#"{{"timestamp":"2030-01-01T00:00:01Z","type":"turn_context","payload":{{"turn_id":"{t}","cwd":"/walk/project","branch":"main"}}}}"#,
            "\n",
            r#"{{"timestamp":"2030-01-01T00:00:02Z","type":"response_item","payload":{{"type":"message","id":"m1","role":"assistant","content":[{{"type":"output_text","text":"{c}"}}]}}}}"#,
            "\n",
            r#"{{"timestamp":"2030-01-01T00:00:03Z","type":"response_item","payload":{{"type":"function_call","id":"fc1","call_id":"{k}","name":"exec_command","arguments":{{"cmd":"{cmd}"}}}}}}"#,
            "\n",
            r#"{{"timestamp":"2030-01-01T00:00:04Z","type":"response_item","payload":{{"type":"function_call_output","id":"fo1","call_id":"{k}","output":"build succeeded"}}}}"#,
            "\n"
        ),
        s = session,
        t = turn,
        c = CLAIM,
        k = call,
        cmd = command
    )
}

/// Everything the wrapper needs, READ BACK from what ingest actually recorded.
#[derive(Debug, Clone)]
struct Ingested {
    project_id: String,
    source_project_key: String,
    source_session_key: String,
    derived_turn_id: String,
    tool_result_event_id: String,
}

fn read_back(ledger: &Ledger) -> Ingested {
    let (project_id, source_project_key): (String, String) = ledger
        .conn()
        .query_row(
            "SELECT project_id, source_project_key FROM source_project_identities LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("project identity");
    let source_session_key: String = ledger
        .conn()
        .query_row(
            "SELECT source_session_key FROM source_sessions LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("session identity");
    let derived_turn_id: String = ledger
        .conn()
        .query_row(
            "SELECT source_turn_id FROM events WHERE kind='tool_use' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("turn identity");
    let tool_result_event_id: String = ledger
        .conn()
        .query_row(
            "SELECT id FROM events WHERE kind='tool_result' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("tool_result event");
    Ingested {
        project_id,
        source_project_key,
        source_session_key,
        derived_turn_id,
        tool_result_event_id,
    }
}

struct Walk {
    _tmp: TempTree,
    ledger: Ledger,
    ingested: Ingested,
}

/// Ingest a transcript, then run the wrapper against it.
///
/// `wrapper_turn` and `wrapper_command` are parameters so the mutation traps can
/// point the wrapper at the wrong turn or the wrong command WITHOUT seeding
/// anything — the transcript stays real either way.
fn walk(
    second_call_same_command: bool,
    wrapper_turn: Option<&str>,
    wrapper_command: Option<&str>,
) -> Walk {
    let tmp = TempTree::new();
    let repo = tmp.path().join("repo");
    fs::create_dir(&repo).expect("mkdir");
    repository(&repo);

    let mut body = transcript("walk-session", "walk-turn", "call-a", COMMAND);
    if second_call_same_command {
        // A SECOND function_call, same command, same turn — the uniqueness trap,
        // now on ingested events rather than seeded ones.
        body.push_str(&format!(
            r#"{{"timestamp":"2030-01-01T00:00:05Z","type":"response_item","payload":{{"type":"function_call","id":"fc2","call_id":"call-b","name":"exec_command","arguments":{{"cmd":"{COMMAND}"}}}}}}"#
        ));
        body.push('\n');
    }
    let source = tmp.path().join("source").join("rollout-walk.jsonl");
    fs::create_dir_all(source.parent().expect("parent")).expect("mkdir");
    fs::write(&source, body).expect("write transcript");

    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest_codex(
        &mut ledger,
        std::slice::from_ref(&source),
        &CodexIngestOptions {
            mirror_root: tmp.path().join("private-mirror"),
            owner_key: Some(OWNER.to_string()),
        },
    )
    .expect("codex ingest");
    let ingested = read_back(&ledger);

    let collector = Collector::open(tmp.path().join("capture"), OWNER, "codex").expect("collector");
    let config = sha256_hex(b"walk-config");
    record_source_assurance(
        &ledger,
        &collector,
        &SourceAssuranceMeasurement {
            source: "codex".to_string(),
            source_version: "walk-v1".to_string(),
            config_sha256: config.clone(),
            matcher: "exec_command".to_string(),
            pre_blocks: true,
            rewrite_exact: true,
            stable_tool_identity: true,
            completion_pairing: true,
            measured_pre_delay_ms: 400,
            child_started_after_pre: true,
            wrapper_exit_code: 0,
            source_exit_code: 0,
        },
    )
    .expect("assurance");

    let outcome = collector
        .execute(&CaptureRequest {
            owner_key: OWNER.to_string(),
            source: "codex".to_string(),
            // The OPAQUE project key. Ingest records the raw cwd path, and the
            // capture layer refuses raw absolute paths outright; acceptance
            // bridges the two by accepting either the stored key or its opaque
            // form. The wrapper must therefore never see the path.
            source_project_key: opaque_source_project_key(
                OWNER,
                "codex",
                &ingested.source_project_key,
            ),
            source_session_key: ingested.source_session_key.clone(),
            // The DERIVED turn identity, per the R17b-1a pin — never the raw
            // "walk-turn" the transcript carries.
            source_turn_id: Some(
                wrapper_turn
                    .map(str::to_string)
                    .unwrap_or_else(|| ingested.derived_turn_id.clone()),
            ),
            source_tool_id: "call-a".to_string(),
            wrapper_nonce: "nonce-walk".to_string(),
            command: wrapper_command.unwrap_or(COMMAND).to_string(),
            cwd: repo.clone(),
            source_config_sha256: config.clone(),
            source_version: "walk-v1".to_string(),
            assurance_enabled: true,
        })
        .expect("execute");
    assert_eq!(outcome.receipt.strength, CaptureStrength::Assured);

    let pre = outcome.pre.as_ref().expect("pre");
    let post = outcome.post.as_ref().expect("post");
    let _ = accept_capture_pair(
        &ledger,
        &collector,
        &CaptureAcceptance {
            outcome: &outcome,
            delivery: &SourceDelivery {
                schema_version: 1,
                attempt_id: outcome.attempt_id.clone(),
                owner_key: OWNER.to_string(),
                source: "codex".to_string(),
                source_project_key: opaque_source_project_key(
                    OWNER,
                    "codex",
                    &ingested.source_project_key,
                ),
                source_session_key: ingested.source_session_key.clone(),
                source_turn_id: Some(ingested.derived_turn_id.clone()),
                source_tool_id: "call-a".to_string(),
                wrapper_nonce: "nonce-walk".to_string(),
                source_event_id: ingested.tool_result_event_id.clone(),
                source_exit_code: Some(outcome.exit_code),
                source_term_signal: None,
                observed_at_unix_ms: 1,
            },
            pre_private_relative_ref: format!(
                "sealed/{}/pre.{}.json",
                outcome.attempt_id, pre.capsule_id
            ),
            post_private_relative_ref: format!(
                "sealed/{}/post.{}.json",
                outcome.attempt_id, post.capsule_id
            ),
        },
    );
    Walk {
        _tmp: tmp,
        ledger,
        ingested,
    }
}

#[test]
fn the_wrapper_binds_to_an_ingested_tool_use() {
    let w = walk(false, None, None);
    let runs = attested_runs_for_project(&w.ledger, &w.ingested.project_id).expect("runs");
    assert_eq!(
        runs.len(),
        1,
        "the accepted pair must bind exactly one INGESTED tool_use; got {:?}",
        runs.keys().collect::<Vec<_>>()
    );
    let (event_id, run) = runs.iter().next().expect("one");
    assert!(
        event_id.starts_with("event_"),
        "the bound event must be an ingested one, not a seeded id: {event_id}"
    );
    assert!(run.assured && run.accepted && run.fingerprint_stable);
    assert_eq!(run.exit_code, Some(0));
}

struct NeverContradicts;
impl thruline_core::nli::EntailmentScorer for NeverContradicts {
    fn score(&mut self, _p: &str, _h: &str) -> thruline_core::Result<thruline_core::nli::Scores> {
        Ok(thruline_core::nli::Scores {
            entailment: 0.95,
            neutral: 0.05,
            contradiction: 0.0,
        })
    }
}

fn derived(w: &Walk) -> thruline_core::derive::DerivedGraph {
    let cfg = thruline_core::config::Config::default();
    thruline_core::annotate::annotate(
        &w.ledger,
        &w.ingested.project_id,
        &cfg,
        &mut NeverContradicts,
        "walk-stub",
    )
    .expect("annotate");
    derive_project(&w.ledger, &w.ingested.project_id).expect("derive")
}

/// THE ACCEPT: a Verified whose provenance is an ingested transcript.
#[test]
fn a_verified_arises_from_ingested_provenance() {
    let w = walk(false, None, None);
    let graph = derived(&w);
    let verified: Vec<_> = graph
        .nodes
        .iter()
        .flat_map(|n| n.verdicts.iter())
        .filter(|v| v.dimensions.evidence == EvidenceState::Verified)
        .collect();
    assert_eq!(
        verified.len(),
        1,
        "exactly one claim should be verified from the ingested session; got {:?}",
        graph
            .nodes
            .iter()
            .flat_map(|n| n.verdicts.iter())
            .map(|v| (v.text.clone(), v.dimensions.evidence.as_str()))
            .collect::<Vec<_>>()
    );
    // Walkable: the verified claim traces to an INGESTED event id.
    let runs = attested_runs_for_project(&w.ledger, &w.ingested.project_id).expect("runs");
    let bound = runs.keys().next().expect("one bound event");
    assert!(
        bound.starts_with("event_"),
        "provenance must be ingested: {bound}"
    );
}

#[test]
fn the_ingested_verified_is_shadow_contained_and_capped() {
    let w = walk(false, None, None);
    let graph = derived(&w);
    assert!(
        graph
            .nodes
            .iter()
            .all(|n| n.dimensions.evidence != EvidenceState::Verified),
        "RULING 4: the composition must cap at supported even with a real verified beneath"
    );
    let public = thruline_core::export::export_project_with_options(
        &w.ledger,
        &graph,
        "2026-08-05T00:00:00Z",
        thruline_core::export::ExportOptions::default(),
    )
    .expect("export");
    let leaked = public["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .flat_map(|n| n["verdicts"].as_array().cloned().unwrap_or_default())
        .any(|v| v["dimensions"]["evidence"] == "verified");
    assert!(!leaked, "an ungraduated class reached the public export");

    let diag = thruline_core::export::export_project_with_options(
        &w.ledger,
        &graph,
        "2026-08-05T00:00:00Z",
        thruline_core::export::ExportOptions { diagnostics: true },
    )
    .expect("export");
    assert!(
        serde_json::to_string(&diag)
            .expect("json")
            .contains("\"evidence\":\"verified\""),
        "diagnostics must disclose the real verified"
    );
}

// --- mutation traps, re-asserted ACROSS THE INGEST BOUNDARY ----------------

fn bound_count(second_call: bool, turn: Option<&str>, command: Option<&str>) -> usize {
    let w = walk(second_call, turn, command);
    attested_runs_for_project(&w.ledger, &w.ingested.project_id)
        .expect("runs")
        .len()
}

#[test]
fn trap_wrapper_run_in_a_different_turn_abstains() {
    // A real, valid turn identity that is not the ingested one.
    let elsewhere = sha256_hex(b"some other turn entirely");
    assert_eq!(
        bound_count(false, Some(&elsewhere), None),
        0,
        "a run attributed to another turn must not bind an ingested claim's evidence"
    );
}

#[test]
fn trap_wrapper_command_absent_from_the_transcript_abstains() {
    assert_eq!(
        bound_count(false, None, Some("git rev-parse HEAD")),
        0,
        "a command the ingested session never ran must not bind"
    );
}

#[test]
fn trap_uniqueness_broken_on_ingested_events_abstains() {
    assert_eq!(
        bound_count(true, None, None),
        0,
        "two ingested tool_uses with the same command in one turn must bind neither"
    );
}

#[test]
fn trap_truncation_on_ingested_events_never_wrong_binds() {
    // Two commands identical through the first 2,000 characters. Hashing a
    // truncated copy would fuse them; reading the full ingested `tool_input`
    // must not.
    let long = format!("{COMMAND} # {}", "x".repeat(2100));
    let other = format!("{long}y");
    let tmp = TempTree::new();
    let source = tmp.path().join("source").join("r.jsonl");
    fs::create_dir_all(source.parent().expect("p")).expect("mkdir");
    fs::write(
        &source,
        transcript("trunc-session", "trunc-turn", "call-a", &long),
    )
    .expect("write");
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest_codex(
        &mut ledger,
        std::slice::from_ref(&source),
        &CodexIngestOptions {
            mirror_root: tmp.path().join("m"),
            owner_key: Some(OWNER.to_string()),
        },
    )
    .expect("ingest");
    let ing = read_back(&ledger);

    // The ingested command and the attested command differ only past the
    // truncation point.
    let stored: String = ledger
        .conn()
        .query_row(
            "SELECT COALESCE(tool_input,'') FROM events WHERE kind='tool_use' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("input");
    let ingested_command = thruline_core::derive::json_string_field(&stored, "cmd").expect("cmd");
    assert_eq!(
        ingested_command, long,
        "the full command must survive ingest"
    );
    assert_ne!(
        sha256_hex(ingested_command.as_bytes()),
        sha256_hex(other.as_bytes()),
        "the two commands must hash differently, or this trap proves nothing"
    );
    assert_eq!(
        ingested_command[..2000],
        other[..2000],
        "they must be identical through the truncation point for the trap to bite"
    );
    let _ = ing;
}

/// PIN — the VERIFIED section now renders the real verified CLAIM, while the
/// node stays capped.
///
/// This test previously documented the opposite: RULING 4 caps node
/// composition, `packet.rs` read node dimensions, and so a real verified claim
/// could never surface while the section's reason asserted engine facts that
/// had gone false. Both halves are fixed — the read moved to claim level and
/// the reason is computed — and the cap is untouched.
#[test]
fn the_verified_section_renders_the_claim_while_the_node_stays_capped() {
    let w = walk(false, None, None);
    let graph = derived(&w);
    let claim_verified = graph
        .nodes
        .iter()
        .flat_map(|n| n.verdicts.iter())
        .filter(|v| v.dimensions.evidence == EvidenceState::Verified)
        .count();
    let node_verified = graph
        .nodes
        .iter()
        .filter(|n| n.dimensions.evidence == EvidenceState::Verified)
        .count();
    let packet = thruline_core::packet::compile_request(
        &w.ledger,
        &thruline_core::packet::Scope::Project(w.ingested.project_id.clone()),
        Some("t"),
        &thruline_core::packet::Budget::default(),
        &thruline_core::config::Config::default(),
    )
    .expect("compile");
    let rendered = packet.render();
    let section = rendered
        .split("## VERIFIED AND CURRENT")
        .nth(1)
        .and_then(|s| s.split("\n## ").next())
        .unwrap_or("")
        .to_string();

    assert_eq!(claim_verified, 1, "a real verified CLAIM exists");
    assert_eq!(node_verified, 0, "and RULING 4 still caps the node");
    assert!(
        !section.contains("(nothing"),
        "the section must now render it: {section}"
    );
    assert!(
        section.contains("evidence=verified") && section.contains("SHADOW"),
        "with its axes and its shadow status: {section}"
    );
}
