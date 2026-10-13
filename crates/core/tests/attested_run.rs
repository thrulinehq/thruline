//! R17a-2 — the `attested_run` contract, and a GENUINE capsule to feed it.
//!
//! The substrate was empty, so this file produces a real one: a real command
//! run under the real wrapper against a real temporary git repository, through
//! the real acceptance path. Nothing here is hand-authored (the D-rule) — the
//! capsule's fields are whatever the collector actually recorded.
//!
//! The contract is **verified-or-abstain**. There is no contradicted branch,
//! and each trap below pins one way the answer must be abstention rather than a
//! verdict.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir;
use thruline_core::capture::{
    accept_capture_pair, record_source_assurance, CaptureAcceptance, CaptureRequest,
    CaptureStrength, Collector, SourceAssuranceMeasurement, SourceDelivery,
};
use thruline_core::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use thruline_core::codex::source_tool_identity;
use thruline_core::contract::{
    check_attested_run, AttestedCheck, AttestedRun, ClaimAssertion, ClaimPolarity, Family,
};
use thruline_core::Ledger;

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("/usr/bin/git")
        .args(args)
        .current_dir(cwd)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed");
}

fn repository(path: &Path) {
    git(path, &["init", "-q"]);
    git(path, &["config", "user.name", "Attested Test"]);
    git(path, &["config", "user.email", "attested@example.invalid"]);
    fs::write(path.join("tracked.txt"), "one\n").expect("write");
    git(path, &["add", "tracked.txt"]);
    git(path, &["commit", "-qm", "fixture"]);
}

fn assurance(config: &str) -> SourceAssuranceMeasurement {
    SourceAssuranceMeasurement {
        source: "codex".to_string(),
        source_version: "attested-test-v1".to_string(),
        config_sha256: config.to_string(),
        matcher: "exec_command".to_string(),
        pre_blocks: true,
        rewrite_exact: true,
        stable_tool_identity: true,
        completion_pairing: true,
        measured_pre_delay_ms: 400,
        child_started_after_pre: true,
        wrapper_exit_code: 0,
        source_exit_code: 0,
    }
}

fn event_ledger() -> Ledger {
    let ledger = Ledger::open_in_memory().expect("ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id,root_path,source,display_name,created_at)
             VALUES('project-db','/private/repo','codex','fixture','now')",
            [],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO source_project_identities(
               project_id,owner_key,source,source_project_key,normalized_root_hint)
             VALUES('project-db','owner-a','codex','project-native-a',NULL)",
            [],
        )
        .expect("identity");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(
               id,project_id,file_path,started_at,last_seq,raw_line_count,raw_digest)
             VALUES('session-db','project-db','private-mirror','now',2,2,?1)",
            [sha256_hex(b"raw")],
        )
        .expect("session");
    ledger
        .conn()
        .execute(
            "INSERT INTO source_sessions(
               session_id,project_id,owner_key,source,source_session_key,source_locator)
             VALUES('session-db','project-db','owner-a','codex','session-native-a',NULL)",
            [],
        )
        .expect("source session");
    let prev = genesis_prev_hash("project-db");
    let raw_use = sha256_hex(b"raw-use");
    let chain_use = chain_hash(&prev, &raw_use, "session-db", 1);
    ledger
        .conn()
        .execute(
            "INSERT INTO events(
               id,project_id,session_id,seq,kind,tool_name,source_tool_name,tool_use_id,tool_input,
               source_turn_id,raw_file,raw_line,raw_sha256,prev_hash,chain_hash)
             VALUES('event-use','project-db','session-db',1,'tool_use','Bash','exec_command',?1,
                    '{\"cmd\":\"git status --porcelain\"}','turn-a',
                    'private-mirror',1,?2,?3,?4)",
            rusqlite::params![
                source_tool_identity("session-native-a", "transcript-call-a"),
                raw_use,
                prev,
                chain_use,
            ],
        )
        .expect("tool_use event");
    let raw_result = sha256_hex(b"raw-result");
    let chain_result = chain_hash(&chain_use, &raw_result, "session-db", 2);
    ledger
        .conn()
        .execute(
            "INSERT INTO events(
               id,project_id,session_id,seq,kind,tool_use_id,tool_result,
               source_turn_id,raw_file,raw_line,raw_sha256,prev_hash,chain_hash)
             VALUES('event-result','project-db','session-db',2,'tool_result',?1,'ok',
                    'turn-a','private-mirror',2,?2,?3,?4)",
            rusqlite::params![
                source_tool_identity("session-native-a", "transcript-call-a"),
                raw_result,
                chain_use,
                chain_result,
            ],
        )
        .expect("tool_result event");
    ledger
}

/// Produce ONE genuine assured capture pair, end to end. Returns the run as the
/// producer would read it, straight from the recorded rows.
fn genuine_attested_run() -> (tempfile::TempDir, AttestedRun) {
    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).expect("mkdir");
    repository(&repo);

    let capture_root = temp.path().join("capture");
    let collector = Collector::open(&capture_root, "owner-a", "codex").expect("collector");
    let config = sha256_hex(b"attested-config");
    let ledger = event_ledger();
    record_source_assurance(&ledger, &collector, &assurance(&config)).expect("assurance");

    // A real, non-mutating command under the real wrapper.
    let outcome = collector
        .execute(&CaptureRequest {
            owner_key: "owner-a".to_string(),
            source: "codex".to_string(),
            source_project_key: "project-native-a".to_string(),
            source_session_key: "session-native-a".to_string(),
            source_turn_id: Some("turn-a".to_string()),
            source_tool_id: "call-a".to_string(),
            wrapper_nonce: "nonce-a".to_string(),
            command: "git status --porcelain".to_string(),
            cwd: repo.clone(),
            source_config_sha256: config.clone(),
            source_version: "attested-test-v1".to_string(),
            assurance_enabled: true,
        })
        .expect("execute");
    assert_eq!(
        outcome.receipt.strength,
        CaptureStrength::Assured,
        "the fixture must produce a genuinely assured run, not a bounded observation"
    );

    let delivery = SourceDelivery {
        schema_version: 1,
        attempt_id: outcome.attempt_id.clone(),
        owner_key: "owner-a".to_string(),
        source: "codex".to_string(),
        source_project_key: "project-native-a".to_string(),
        source_session_key: "session-native-a".to_string(),
        source_turn_id: Some("turn-a".to_string()),
        source_tool_id: "call-a".to_string(),
        wrapper_nonce: "nonce-a".to_string(),
        source_event_id: "event-result".to_string(),
        source_exit_code: Some(outcome.exit_code),
        source_term_signal: None,
        observed_at_unix_ms: 1,
    };
    let pre = outcome.pre.as_ref().expect("pre capsule");
    let post = outcome.post.as_ref().expect("post capsule");
    let accepted = accept_capture_pair(
        &ledger,
        &collector,
        &CaptureAcceptance {
            outcome: &outcome,
            delivery: &delivery,
            pre_private_relative_ref: format!(
                "sealed/{}/pre.{}.json",
                outcome.attempt_id, pre.capsule_id
            ),
            post_private_relative_ref: format!(
                "sealed/{}/post.{}.json",
                outcome.attempt_id, post.capsule_id
            ),
        },
    )
    .expect("accept");
    assert_eq!(accepted.strength, CaptureStrength::Assured);

    // Read the run back out of the RECORDED rows — the producer's own inputs,
    // never the in-memory outcome.
    let (command_sha256, exit_code, strength, status): (String, Option<i64>, String, String) =
        ledger
            .conn()
            .query_row(
                "SELECT command_sha256, exit_code, strength, status
                 FROM capture_attempts WHERE attempt_id = ?1",
                [&outcome.attempt_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .expect("attempt row");
    let head_commit: String = ledger
        .conn()
        .query_row(
            "SELECT head_commit FROM state_capsules
             WHERE attempt_id = ?1 AND phase = 'post'",
            [&outcome.attempt_id],
            |r| r.get(0),
        )
        .expect("post capsule row");

    let run = AttestedRun {
        assured: strength == "assured",
        accepted: status == "accepted",
        command_sha256,
        exit_code,
        head_commit,
        // The same predicate `accept_capture_pair` itself enforces: the
        // repository fingerprint did not move across the run.
        fingerprint_stable: pre.payload.state == post.payload.state,
    };
    assert!(
        run.assured && run.accepted,
        "recorded rows must agree: {run:?}"
    );
    assert!(!run.head_commit.is_empty(), "a capsule must pin a commit");
    (temp, run)
}

fn success_claim() -> ClaimAssertion {
    ClaimAssertion {
        family: Family::Build,
        passed: None,
        failed: None,
        all_pass: false,
        polarity: ClaimPolarity::AssertsSuccess,
    }
}

#[test]
fn a_genuine_attested_run_verifies_a_matching_success_claim() {
    let (_t, run) = genuine_attested_run();
    let sha = run.command_sha256.clone();
    match check_attested_run(&success_claim(), Some(&sha), &run) {
        AttestedCheck::Verified(reason) => {
            assert!(
                reason.contains("exited 0") && reason.contains("commit"),
                "a verified reason must name what was attested: {reason}"
            );
        }
        AttestedCheck::Abstain(r) => panic!("a genuine assured run must verify: {r}"),
    }
}

// --- the four traps, one test each -----------------------------------------

#[test]
fn trap_mismatched_command_sha_abstains() {
    let (_t, run) = genuine_attested_run();
    let other = sha256_hex(b"a completely different command");
    assert!(matches!(
        check_attested_run(&success_claim(), Some(&other), &run),
        AttestedCheck::Abstain(_)
    ));
}

#[test]
fn trap_moved_fingerprint_abstains() {
    let (_t, mut run) = genuine_attested_run();
    let sha = run.command_sha256.clone();
    run.fingerprint_stable = false;
    assert!(matches!(
        check_attested_run(&success_claim(), Some(&sha), &run),
        AttestedCheck::Abstain(_)
    ));
}

/// A non-zero exit against a pass-claim abstains. It does NOT convict —
/// conviction from an attested run is a future ruling, not a side effect of
/// building a producer.
#[test]
fn trap_nonzero_exit_abstains_and_never_convicts() {
    let (_t, mut run) = genuine_attested_run();
    let sha = run.command_sha256.clone();
    run.exit_code = Some(1);
    match check_attested_run(&success_claim(), Some(&sha), &run) {
        AttestedCheck::Abstain(reason) => assert!(
            reason.contains("separate ruling"),
            "the abstention must say why it is not a conviction: {reason}"
        ),
        AttestedCheck::Verified(_) => panic!("a non-zero exit must never verify"),
    }
}

#[test]
fn trap_count_bearing_claim_abstains() {
    let (_t, run) = genuine_attested_run();
    let sha = run.command_sha256.clone();
    let mut counted = success_claim();
    counted.passed = Some(33);
    match check_attested_run(&counted, Some(&sha), &run) {
        AttestedCheck::Abstain(reason) => assert!(
            reason.contains("counts"),
            "the abstention must name the gap between an exit code and a count: {reason}"
        ),
        AttestedCheck::Verified(_) => {
            panic!(
                "a capsule attests an exit code and carries no output digest; it can never \
                   confirm a count"
            )
        }
    }
}

#[test]
fn a_bounded_observation_never_verifies() {
    let (_t, mut run) = genuine_attested_run();
    let sha = run.command_sha256.clone();
    run.assured = false;
    assert!(matches!(
        check_attested_run(&success_claim(), Some(&sha), &run),
        AttestedCheck::Abstain(_)
    ));
}

/// The binding fact that a design note got WRONG, pinned so it cannot mislead
/// again.
///
/// A102 recorded that the pair is "reachable by identity" because
/// `capture_event_links.source_tool_id` is the vendor tool id "which the
/// evidence tool_use event already carries in `events.tool_use_id`". That is
/// FALSE: the link stores the RAW vendor call id, while the event stores
/// `source_tool_identity(session_key, call_id)` — a namespace hash. Wiring a
/// producer on that premise would have matched nothing, and the obvious repair
/// (walk the tool_result sibling) is precisely the join ruling 2 forbids.
///
/// The only validated bridge is `exact_source_command_match`'s predicate:
/// within one (session, source_turn_id), exactly one `tool_use` whose command
/// hashes to the attempt's `command_sha256`. This test pins both halves — the
/// inequality that killed the wrong design, and the equality that the right one
/// rests on.
#[test]
fn the_capture_link_id_is_not_the_event_tool_use_id() {
    let (_t, ledger, attempt_id) = accepted_pair();

    let link_tool_id: String = ledger
        .conn()
        .query_row(
            "SELECT source_tool_id FROM capture_event_links WHERE attempt_id = ?1",
            [&attempt_id],
            |r| r.get(0),
        )
        .expect("link");
    let event_tool_use_id: String = ledger
        .conn()
        .query_row(
            "SELECT tool_use_id FROM events WHERE kind='tool_use' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("event");

    assert_ne!(
        link_tool_id, event_tool_use_id,
        "if these ever become equal, the A102 design becomes viable and this test \
         should be revisited — but until then a producer must NOT match on them"
    );

    // The bridge that DOES hold: the tool_use and its tool_result share the
    // event-level identity, and the attempt is bound to the turn by command sha.
    let (use_id, result_id): (String, String) = ledger
        .conn()
        .query_row(
            "SELECT (SELECT tool_use_id FROM events WHERE kind='tool_use' LIMIT 1),
                    (SELECT tool_use_id FROM events WHERE kind='tool_result' LIMIT 1)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("ids");
    assert_eq!(
        use_id, result_id,
        "the sibling relation exists — which is exactly why reaching the pair through \
         it had to be ruled out explicitly rather than left to judgement"
    );
}

/// Shared setup: one genuine accepted assured pair, returning the ledger so
/// recorded rows can be inspected.
fn accepted_pair() -> (tempfile::TempDir, Ledger, String) {
    let temp = tempdir().expect("tempdir");
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).expect("mkdir");
    repository(&repo);
    let capture_root = temp.path().join("capture");
    let collector = Collector::open(&capture_root, "owner-a", "codex").expect("collector");
    let config = sha256_hex(b"attested-config");
    let ledger = event_ledger();
    record_source_assurance(&ledger, &collector, &assurance(&config)).expect("assurance");
    let outcome = collector
        .execute(&CaptureRequest {
            owner_key: "owner-a".to_string(),
            source: "codex".to_string(),
            source_project_key: "project-native-a".to_string(),
            source_session_key: "session-native-a".to_string(),
            source_turn_id: Some("turn-a".to_string()),
            source_tool_id: "call-a".to_string(),
            wrapper_nonce: "nonce-pair".to_string(),
            command: "git status --porcelain".to_string(),
            cwd: repo.clone(),
            source_config_sha256: config.clone(),
            source_version: "attested-test-v1".to_string(),
            assurance_enabled: true,
        })
        .expect("execute");
    let delivery = SourceDelivery {
        schema_version: 1,
        attempt_id: outcome.attempt_id.clone(),
        owner_key: "owner-a".to_string(),
        source: "codex".to_string(),
        source_project_key: "project-native-a".to_string(),
        source_session_key: "session-native-a".to_string(),
        source_turn_id: Some("turn-a".to_string()),
        source_tool_id: "call-a".to_string(),
        wrapper_nonce: "nonce-pair".to_string(),
        source_event_id: "event-result".to_string(),
        source_exit_code: Some(outcome.exit_code),
        source_term_signal: None,
        observed_at_unix_ms: 1,
    };
    let pre = outcome.pre.as_ref().expect("pre");
    let post = outcome.post.as_ref().expect("post");
    accept_capture_pair(
        &ledger,
        &collector,
        &CaptureAcceptance {
            outcome: &outcome,
            delivery: &delivery,
            pre_private_relative_ref: format!(
                "sealed/{}/pre.{}.json",
                outcome.attempt_id, pre.capsule_id
            ),
            post_private_relative_ref: format!(
                "sealed/{}/post.{}.json",
                outcome.attempt_id, post.capsule_id
            ),
        },
    )
    .expect("accept");
    let attempt_id = outcome.attempt_id.clone();
    (temp, ledger, attempt_id)
}
