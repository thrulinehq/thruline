//! R17a-3 — the SEEDED walk, and the wired-level mutation tests.
//!
//! **THIS FIXTURE IS SEEDED, DELIBERATELY.** Its transcript half is written
//! straight into `events` rather than ingested. It is retained as the fast
//! regression for the producer and its traps; the transcript-to-verdict walk
//! (R17b-1) is the one that proves provenance end to end. Do not cite this file
//! as evidence that an ingested session reaches a verdict — it cannot show
//! that, by construction.
//!
//! The chain an operator will adjudicate: a real command under the real
//! wrapper → a real capsule → a real accepted pair → `check_attested_run` →
//! claim dimensions → the rendered packet.
//!
//! HONEST SCOPE, stated rather than implied: the *transcript* half of the
//! fixture is seeded directly into `events` (the M5 harness's method), because
//! an ingested Claude transcript carries no codex turn identity for the wrapper
//! run to bind to. Everything from the wrapper onward — capsule, acceptance,
//! contract, derivation, packet — is the real path with no stubs.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir;
use thruline_core::capture::{
    accept_capture_pair, attested_runs_for_project, record_source_assurance, CaptureAcceptance,
    CaptureRequest, CaptureStrength, Collector, SourceAssuranceMeasurement, SourceDelivery,
};
use thruline_core::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use thruline_core::codex::source_tool_identity;
use thruline_core::derive::{derive_project, EvidenceState};
use thruline_core::Ledger;

const PROJ: &str = "project-db";
const CLAIM: &str = "The build succeeded.";
const COMMAND: &str = "git status --porcelain";

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
    git(path, &["config", "user.name", "Walk Test"]);
    git(path, &["config", "user.email", "walk@example.invalid"]);
    fs::write(path.join("tracked.txt"), "one\n").expect("write");
    git(path, &["add", "tracked.txt"]);
    git(path, &["commit", "-qm", "fixture"]);
}

/// Seed the transcript half: one assistant claim, one tool_use carrying the
/// command, one tool_result. `extra_tool_use` adds a SECOND identical command
/// in the same turn, which must break the uniqueness requirement.
fn seed(ledger: &Ledger, command: &str, extra_tool_use: bool) {
    let c = ledger.conn();
    c.execute(
        "INSERT INTO projects(id,root_path,source,display_name,created_at)
         VALUES('project-db','/private/repo','codex','fixture','now')",
        [],
    )
    .expect("project");
    c.execute(
        "INSERT INTO source_project_identities(project_id,owner_key,source,source_project_key,normalized_root_hint)
         VALUES('project-db','owner-a','codex','project-native-a',NULL)",
        [],
    )
    .expect("identity");
    c.execute(
        "INSERT INTO sessions(id,project_id,file_path,started_at,last_seq,raw_line_count,raw_digest)
         VALUES('session-db','project-db','private-mirror','now',4,4,?1)",
        [sha256_hex(b"raw")],
    )
    .expect("session");
    c.execute(
        "INSERT INTO source_sessions(session_id,project_id,owner_key,source,source_session_key,source_locator)
         VALUES('session-db','project-db','owner-a','codex','session-native-a',NULL)",
        [],
    )
    .expect("source session");

    let tool_id = source_tool_identity("session-native-a", "transcript-call-a");
    let mut prev = genesis_prev_hash(PROJ);
    let insert =
        |seq: i64, kind: &str, sql: &str, params: &[&dyn rusqlite::ToSql], prev: &mut String| {
            let raw = sha256_hex(format!("raw-{kind}-{seq}").as_bytes());
            let ch = chain_hash(prev, &raw, "session-db", seq);
            let mut all: Vec<&dyn rusqlite::ToSql> = params.to_vec();
            all.push(&raw);
            let p2 = prev.clone();
            all.push(&p2);
            all.push(&ch);
            c.execute(sql, rusqlite::params_from_iter(all))
                .expect("event");
            *prev = ch;
        };

    insert(
        1,
        "assistant_text",
        "INSERT INTO events(id,project_id,session_id,seq,kind,role,text,source_turn_id,
                            raw_file,raw_line,raw_sha256,prev_hash,chain_hash)
         VALUES('event-claim','project-db','session-db',1,'assistant_text','assistant',?1,
                'turn-a','private-mirror',1,?2,?3,?4)",
        &[&CLAIM],
        &mut prev,
    );
    insert(
        2,
        "tool_use",
        "INSERT INTO events(id,project_id,session_id,seq,kind,tool_name,source_tool_name,tool_use_id,tool_input,
                            source_turn_id,raw_file,raw_line,raw_sha256,prev_hash,chain_hash)
         VALUES('event-use','project-db','session-db',2,'tool_use','Bash','exec_command',?1,?2,
                'turn-a','private-mirror',2,?3,?4,?5)",
        &[&tool_id, &format!("{{\"cmd\":\"{command}\"}}")],
        &mut prev,
    );
    insert(
        3,
        "tool_result",
        "INSERT INTO events(id,project_id,session_id,seq,kind,tool_use_id,tool_result,
                            source_turn_id,raw_file,raw_line,raw_sha256,prev_hash,chain_hash)
         VALUES('event-result','project-db','session-db',3,'tool_result',?1,'build succeeded',
                'turn-a','private-mirror',3,?2,?3,?4)",
        &[&tool_id],
        &mut prev,
    );
    if extra_tool_use {
        let second = source_tool_identity("session-native-a", "transcript-call-b");
        insert(
            4,
            "tool_use",
            "INSERT INTO events(id,project_id,session_id,seq,kind,tool_name,source_tool_name,tool_use_id,tool_input,
                                source_turn_id,raw_file,raw_line,raw_sha256,prev_hash,chain_hash)
             VALUES('event-use-2','project-db','session-db',4,'tool_use','Bash','exec_command',?1,?2,
                    'turn-a','private-mirror',4,?3,?4,?5)",
            &[&second, &format!("{{\"cmd\":\"{command}\"}}")],
            &mut prev,
        );
    }
}

struct Walk {
    _temp: tempfile::TempDir,
    ledger: Ledger,
}

/// Run the real wrapper and accept the pair. `wrapper_command` is what actually
/// executes; `transcript_command` is what the seeded tool_use claims ran.
fn walk(transcript_command: &str, wrapper_command: &str, extra_tool_use: bool) -> Walk {
    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).expect("mkdir");
    repository(&repo);
    let ledger = Ledger::open_in_memory().expect("ledger");
    seed(&ledger, transcript_command, extra_tool_use);

    let collector =
        Collector::open(temp.path().join("capture"), "owner-a", "codex").expect("collector");
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
            owner_key: "owner-a".to_string(),
            source: "codex".to_string(),
            source_project_key: "project-native-a".to_string(),
            source_session_key: "session-native-a".to_string(),
            source_turn_id: Some("turn-a".to_string()),
            source_tool_id: "transcript-call-a".to_string(),
            wrapper_nonce: "nonce-walk".to_string(),
            command: wrapper_command.to_string(),
            cwd: repo.clone(),
            source_config_sha256: config.clone(),
            source_version: "walk-v1".to_string(),
            assurance_enabled: true,
        })
        .expect("execute");
    assert_eq!(outcome.receipt.strength, CaptureStrength::Assured);

    let pre = outcome.pre.as_ref().expect("pre");
    let post = outcome.post.as_ref().expect("post");
    let accepted = accept_capture_pair(
        &ledger,
        &collector,
        &CaptureAcceptance {
            outcome: &outcome,
            delivery: &SourceDelivery {
                schema_version: 1,
                attempt_id: outcome.attempt_id.clone(),
                owner_key: "owner-a".to_string(),
                source: "codex".to_string(),
                source_project_key: "project-native-a".to_string(),
                source_session_key: "session-native-a".to_string(),
                source_turn_id: Some("turn-a".to_string()),
                source_tool_id: "transcript-call-a".to_string(),
                wrapper_nonce: "nonce-walk".to_string(),
                source_event_id: "event-result".to_string(),
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
    // Acceptance itself may legitimately refuse a mutated fixture; the walk
    // still returns so the caller can assert the downstream abstention.
    if let Ok(a) = accepted {
        assert_eq!(a.strength, CaptureStrength::Assured);
    }
    Walk {
        _temp: temp,
        ledger,
    }
}

#[test]
fn the_binding_reaches_exactly_one_tool_use() {
    let w = walk(COMMAND, COMMAND, false);
    let runs = attested_runs_for_project(&w.ledger, PROJ).expect("runs");
    assert_eq!(
        runs.len(),
        1,
        "one accepted assured pair must bind one event"
    );
    assert!(
        runs.contains_key("event-use"),
        "the pair must bind the TOOL_USE, never the tool_result: {:?}",
        runs.keys().collect::<Vec<_>>()
    );
    let run = &runs["event-use"];
    assert!(run.assured && run.accepted && run.fingerprint_stable);
    assert_eq!(run.exit_code, Some(0));
    assert!(!run.head_commit.is_empty(), "the capsule must pin a commit");
}

// --- wired-level mutations. Each must abstain end to end. -------------------

fn bound_runs(transcript: &str, wrapper: &str, extra: bool) -> usize {
    let w = walk(transcript, wrapper, extra);
    attested_runs_for_project(&w.ledger, PROJ)
        .expect("runs")
        .len()
}

#[test]
fn mutation_wrong_command_sha_abstains_by_failing_to_bind() {
    // The transcript says one command ran; the wrapper attested another.
    assert_eq!(
        bound_runs("echo something-else", COMMAND, false),
        0,
        "a command the transcript never names must not bind"
    );
}

#[test]
fn mutation_broken_uniqueness_abstains() {
    // Two tool_uses, same command, same turn: which run was attested is
    // unanswerable, so neither may bind.
    assert_eq!(
        bound_runs(COMMAND, COMMAND, true),
        0,
        "an ambiguous turn must bind nothing rather than guess"
    );
}

#[test]
fn mutation_truncated_command_never_wrong_binds() {
    // A command that shares a 2,000-char prefix with the attested one but
    // differs after it. Hashing a truncated copy would bind these together;
    // reading the full input from `events` must not.
    let long = format!("{COMMAND} # {}", "x".repeat(2100));
    let other = format!("{COMMAND} # {}y", "x".repeat(2100));
    assert_eq!(
        bound_runs(&long, &other, false),
        0,
        "commands differing only past the truncation point must never bind"
    );
}

// ---------------------------------------------------------------------------
// The walk itself: capsule -> pair -> contract -> dimensions -> packet.
// ---------------------------------------------------------------------------

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

fn derived_with_annotation(w: &Walk) -> thruline_core::derive::DerivedGraph {
    let cfg = thruline_core::config::Config::default();
    thruline_core::annotate::annotate(&w.ledger, PROJ, &cfg, &mut NeverContradicts, "walk-stub")
        .expect("annotate");
    derive_project(&w.ledger, PROJ).expect("derive")
}

#[test]
fn the_first_real_verified_exists_and_is_walkable() {
    let w = walk(COMMAND, COMMAND, false);
    let graph = derived_with_annotation(&w);

    let verified: Vec<_> = graph
        .nodes
        .iter()
        .flat_map(|n| n.verdicts.iter())
        .filter(|v| v.dimensions.evidence == EvidenceState::Verified)
        .collect();
    assert_eq!(
        verified.len(),
        1,
        "exactly one claim should be verified by the attested run; got {:?}",
        graph
            .nodes
            .iter()
            .flat_map(|n| n.verdicts.iter())
            .map(|v| (v.text.clone(), v.dimensions.evidence.as_str()))
            .collect::<Vec<_>>()
    );

    // The chain is walkable: the verified claim's evidence event is the very
    // tool_use the accepted pair bound.
    let runs = attested_runs_for_project(&w.ledger, PROJ).expect("runs");
    assert!(runs.contains_key("event-use"));
    assert!(
        verified[0]
            .evidence_scopes
            .iter()
            .any(|s| s.event_id == "event-use")
            || verified[0].best_event_id.as_deref() == Some("event-use"),
        "the verified claim must trace to the attested tool_use"
    );
}

#[test]
fn the_node_composition_caps_the_real_verified_at_supported() {
    let w = walk(COMMAND, COMMAND, false);
    let graph = derived_with_annotation(&w);
    assert!(
        graph
            .nodes
            .iter()
            .all(|n| n.dimensions.evidence != EvidenceState::Verified),
        "RULING 4: a composition may never publish verified, even with a real one beneath it"
    );
    assert!(
        graph
            .nodes
            .iter()
            .any(|n| n.dimensions.evidence == EvidenceState::Supported),
        "the capped node should read supported"
    );
}

#[test]
fn the_real_verified_is_shadow_contained_in_the_default_export() {
    let w = walk(COMMAND, COMMAND, false);
    let graph = derived_with_annotation(&w);

    let public = thruline_core::export::export_project_with_options(
        &w.ledger,
        &graph,
        "2026-08-05T00:00:00Z",
        thruline_core::export::ExportOptions::default(),
    )
    .expect("export");
    let public_states: Vec<String> = public["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .flat_map(|n| {
            n["verdicts"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|v| {
                    v["dimensions"]["evidence"]
                        .as_str()
                        .unwrap_or("")
                        .to_string()
                })
        })
        .collect();
    assert!(
        !public_states.iter().any(|e| e == "verified"),
        "an ungraduated class reached the public export: {public_states:?}"
    );

    let diag = thruline_core::export::export_project_with_options(
        &w.ledger,
        &graph,
        "2026-08-05T00:00:00Z",
        thruline_core::export::ExportOptions { diagnostics: true },
    )
    .expect("export");
    let diag_text = serde_json::to_string(&diag).expect("json");
    assert!(
        diag_text.contains("\"evidence\":\"verified\""),
        "diagnostics must disclose the real verified"
    );
}
