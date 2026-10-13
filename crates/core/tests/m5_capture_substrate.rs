use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;
use std::time::Instant;

use tempfile::tempdir;
use thruline_core::capture::{
    accept_capture_pair, accept_capture_pair_with_failpoint, accept_observation,
    accept_observation_and_publish, capture_stabilized, compare_states, latest_proof_currentness,
    record_source_assurance, record_state_comparison, CaptureAcceptance, CaptureCommitFailpoint,
    CaptureRequest, CaptureStrength, Collector, ProofApplicability, SourceAssuranceMeasurement,
    SourceDelivery,
};
use thruline_core::chain::{chain_hash, genesis_prev_hash, sha256_hex};
use thruline_core::codex::source_tool_identity;
use thruline_core::Ledger;

fn run(cwd: &Path, args: &[&str]) {
    let status = Command::new("/usr/bin/git")
        .args(args)
        .current_dir(cwd)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

#[test]
fn explicit_refresh_publishes_current_stale_current_once_and_keeps_truth_untouched() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    repository(&repo);
    let collector = Collector::open(temp.path().join("capture"), "owner-a", "codex").unwrap();
    let config = sha256_hex(b"config");
    let ledger = event_ledger();
    record_source_assurance(&ledger, &collector, &assurance(&config)).unwrap();
    let outcome = collector
        .execute(&request(&repo, "refresh-execution", &config, true))
        .unwrap();
    let delivery = SourceDelivery {
        schema_version: 1,
        attempt_id: outcome.attempt_id.clone(),
        owner_key: "owner-a".to_string(),
        source: "codex".to_string(),
        source_project_key: "project-native-a".to_string(),
        source_session_key: "session-native-a".to_string(),
        source_turn_id: Some("turn-native-a".to_string()),
        source_tool_id: "call-native-a".to_string(),
        wrapper_nonce: "refresh-execution".to_string(),
        source_event_id: "event-result".to_string(),
        source_exit_code: Some(0),
        source_term_signal: None,
        observed_at_unix_ms: 1,
    };
    let pre = outcome.pre.as_ref().unwrap();
    let post = outcome.post.as_ref().unwrap();
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
    .unwrap();

    let observe = |nonce: &str| {
        let observation = collector
            .observe(&request(&repo, nonce, &config, false))
            .unwrap();
        let publication =
            accept_observation_and_publish(&ledger, &collector, &observation).unwrap();
        (observation, publication)
    };
    let (_first, current) = observe("refresh-current-1");
    assert_eq!((current.comparison_count, current.current_count), (1, 1));

    fs::write(repo.join("tracked.txt"), "changed\n").unwrap();
    let (_second, stale) = observe("refresh-stale");
    assert_eq!((stale.comparison_count, stale.stale_count), (1, 1));

    fs::write(repo.join("tracked.txt"), "one\n").unwrap();
    let (third, current_again) = observe("refresh-current-2");
    assert_eq!(
        (current_again.comparison_count, current_again.current_count),
        (1, 1)
    );
    let retry = accept_observation_and_publish(&ledger, &collector, &third).unwrap();
    assert_eq!(retry.refresh_id, current_again.refresh_id);
    let counts: (i64, i64, i64) = ledger
        .conn()
        .query_row(
            "SELECT (SELECT COUNT(*) FROM state_comparisons),
                    (SELECT COUNT(*) FROM capture_refresh_receipts),
                    (SELECT COUNT(*) FROM events WHERE project_id='project-db')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (3, 3, 2));
    let history: Vec<String> = ledger
        .conn()
        .prepare("SELECT currentness FROM state_comparisons ORDER BY accepted_seq")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(history, ["current", "stale", "current"]);
}

fn repository(path: &Path) {
    run(path, &["init", "-q"]);
    run(path, &["config", "user.name", "Capture Test"]);
    run(path, &["config", "user.email", "capture@example.invalid"]);
    fs::write(path.join("tracked.txt"), "one\n").unwrap();
    fs::write(
        path.join("Cargo.toml"),
        "[package]\nname='capture-fixture'\nversion='0.1.0'\n",
    )
    .unwrap();
    run(path, &["add", "tracked.txt", "Cargo.toml"]);
    run(path, &["commit", "-qm", "fixture"]);
}

fn assurance(config: &str) -> SourceAssuranceMeasurement {
    SourceAssuranceMeasurement {
        source: "codex".to_string(),
        source_version: "capture-test-v1".to_string(),
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

fn request(root: &Path, nonce: &str, config: &str, assurance_enabled: bool) -> CaptureRequest {
    CaptureRequest {
        owner_key: "owner-a".to_string(),
        source: "codex".to_string(),
        source_project_key: "project-native-a".to_string(),
        source_session_key: "session-native-a".to_string(),
        source_turn_id: Some("turn-native-a".to_string()),
        source_tool_id: "call-native-a".to_string(),
        wrapper_nonce: nonce.to_string(),
        command: "git status --porcelain".to_string(),
        cwd: root.to_path_buf(),
        source_config_sha256: config.to_string(),
        source_version: "capture-test-v1".to_string(),
        assurance_enabled,
    }
}

fn event_ledger() -> Ledger {
    let ledger = Ledger::open_in_memory().unwrap();
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id,root_path,source,display_name,created_at)
             VALUES('project-db','/private/repo','codex','fixture','now')",
            [],
        )
        .unwrap();
    ledger
        .conn()
        .execute(
            "INSERT INTO source_project_identities(
               project_id,owner_key,source,source_project_key,normalized_root_hint)
             VALUES('project-db','owner-a','codex','project-native-a',NULL)",
            [],
        )
        .unwrap();
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(
               id,project_id,file_path,started_at,last_seq,raw_line_count,raw_digest)
             VALUES('session-db','project-db','private-mirror','now',2,2,?1)",
            [sha256_hex(b"raw")],
        )
        .unwrap();
    ledger
        .conn()
        .execute(
            "INSERT INTO source_sessions(
               session_id,project_id,owner_key,source,source_session_key,source_locator)
             VALUES('session-db','project-db','owner-a','codex','session-native-a',NULL)",
            [],
        )
        .unwrap();
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
                    '{\"cmd\":\"git status --porcelain\"}','turn-native-a',
                    'private-mirror',1,?2,?3,?4)",
            rusqlite::params![
                source_tool_identity("session-native-a", "transcript-call-a"),
                raw_use,
                prev,
                chain_use,
            ],
        )
        .unwrap();
    let raw_result = sha256_hex(b"raw-result");
    let chain_result = chain_hash(&chain_use, &raw_result, "session-db", 2);
    ledger
        .conn()
        .execute(
            "INSERT INTO events(
               id,project_id,session_id,seq,kind,tool_use_id,tool_result,
               source_turn_id,raw_file,raw_line,raw_sha256,prev_hash,chain_hash)
             VALUES('event-result','project-db','session-db',2,'tool_result',?1,'ok',
                    'turn-native-a','private-mirror',2,?2,?3,?4)",
            rusqlite::params![
                source_tool_identity("session-native-a", "transcript-call-a"),
                raw_result,
                chain_use,
                chain_result,
            ],
        )
        .unwrap();
    ledger
}

#[test]
fn complete_pair_commits_atomically_and_is_idempotent() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    repository(&repo);
    let capture_root = temp.path().join("capture");
    let collector = Collector::open(&capture_root, "owner-a", "codex").unwrap();
    let config = sha256_hex(b"config");
    let ledger = event_ledger();
    record_source_assurance(&ledger, &collector, &assurance(&config)).unwrap();
    let outcome = collector
        .execute(&request(&repo, "nonce-a", &config, true))
        .unwrap();
    assert_eq!(outcome.exit_code, 0);
    assert_eq!(outcome.receipt.strength, CaptureStrength::Assured);
    let delivery = SourceDelivery {
        schema_version: 1,
        attempt_id: outcome.attempt_id.clone(),
        owner_key: "owner-a".to_string(),
        source: "codex".to_string(),
        source_project_key: "project-native-a".to_string(),
        source_session_key: "session-native-a".to_string(),
        source_turn_id: Some("turn-native-a".to_string()),
        source_tool_id: "call-native-a".to_string(),
        wrapper_nonce: "nonce-a".to_string(),
        source_event_id: "event-result".to_string(),
        source_exit_code: Some(0),
        source_term_signal: None,
        observed_at_unix_ms: 1,
    };
    let pre = outcome.pre.as_ref().unwrap();
    let post = outcome.post.as_ref().unwrap();
    let acceptance = CaptureAcceptance {
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
    };
    let accepted = accept_capture_pair(&ledger, &collector, &acceptance).unwrap();
    assert_eq!(accepted.strength, CaptureStrength::Assured);
    accept_capture_pair(&ledger, &collector, &acceptance).unwrap();
    let counts: (i64, i64, i64) = ledger
        .conn()
        .query_row(
            "SELECT (SELECT COUNT(*) FROM capture_attempts),
                    (SELECT COUNT(*) FROM state_capsules),
                    (SELECT COUNT(*) FROM capture_event_links)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(counts, (1, 2, 1));
    ledger.update_ledger_meta("project-db", None).unwrap();
    let graph = thruline_core::derive::DerivedGraph {
        project_id: "project-db".to_string(),
        sessions: Vec::new(),
        nodes: Vec::new(),
        edges: Vec::new(),
        duplicate_events: Vec::new(),
        stats: thruline_core::derive::DeriveStats::default(),
    };
    let first = thruline_core::export::export_project(&ledger, &graph, "fixed").unwrap();
    let second = thruline_core::export::export_project(&ledger, &graph, "fixed").unwrap();
    assert_eq!(first, second);
    assert!(first["capture_receipts"]["event-result"].is_object());
    assert_eq!(
        first["capture_receipts"]["event-result"]["execution"]["source_pairing"],
        "complete"
    );
    assert_eq!(
        first["capture_receipts"]["event-result"]["execution"]["exit_code"],
        0
    );
    assert_eq!(
        first["capture_receipts"]["event-result"]["currentness_reason"],
        "no_comparable_observation"
    );
    let public = serde_json::to_string(&first).unwrap();
    assert!(!public.contains("/private/repo"));
    assert!(!public.contains("git status --porcelain"));

    let capture_addition = serde_json::to_vec(&serde_json::json!({
        "assured_capture": first.get("assured_capture"),
        "capture_receipts": first.get("capture_receipts"),
    }))
    .unwrap();
    assert!(
        capture_addition.len() <= 256 * 1024,
        "public capture addition exceeded the 256-KiB project budget"
    );
    let mut receipt_export_ms = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        let exported = thruline_core::export::export_project(&ledger, &graph, "fixed").unwrap();
        assert!(exported["capture_receipts"]["event-result"].is_object());
        receipt_export_ms.push(started.elapsed().as_secs_f64() * 1_000.0);
    }
    receipt_export_ms.sort_by(f64::total_cmp);
    let p95_ms = receipt_export_ms[18];
    assert!(p95_ms <= 150.0, "receipt export p95 was {p95_ms:.3} ms");
    eprintln!(
        "M5_RECEIPT_EXPORT={{\"iterations\":20,\"p95_ms\":{p95_ms},\"capture_addition_bytes\":{}}}",
        capture_addition.len()
    );
}

#[test]
fn transaction_failure_has_zero_partial_authority() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    repository(&repo);
    let collector = Collector::open(temp.path().join("capture"), "owner-a", "codex").unwrap();
    let config = sha256_hex(b"config");
    let ledger = event_ledger();
    record_source_assurance(&ledger, &collector, &assurance(&config)).unwrap();
    let outcome = collector
        .execute(&request(&repo, "nonce-b", &config, true))
        .unwrap();
    let delivery = SourceDelivery {
        schema_version: 1,
        attempt_id: outcome.attempt_id.clone(),
        owner_key: "owner-a".to_string(),
        source: "codex".to_string(),
        source_project_key: "project-native-a".to_string(),
        source_session_key: "session-native-a".to_string(),
        source_turn_id: Some("turn-native-a".to_string()),
        source_tool_id: "call-native-a".to_string(),
        wrapper_nonce: "nonce-b".to_string(),
        source_event_id: "event-result".to_string(),
        source_exit_code: Some(0),
        source_term_signal: None,
        observed_at_unix_ms: 1,
    };
    let pre = outcome.pre.as_ref().unwrap();
    let post = outcome.post.as_ref().unwrap();
    let acceptance = CaptureAcceptance {
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
    };
    assert!(accept_capture_pair_with_failpoint(
        &ledger,
        &collector,
        &acceptance,
        Some(CaptureCommitFailpoint::BeforeCommit)
    )
    .is_err());
    let rows: i64 = ledger
        .conn()
        .query_row(
            "SELECT (SELECT COUNT(*) FROM capture_attempts)
                    +(SELECT COUNT(*) FROM state_capsules)
                    +(SELECT COUNT(*) FROM capture_event_links)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn later_observations_preserve_stale_history() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    repository(&repo);
    let collector = Collector::open(temp.path().join("capture"), "owner-a", "codex").unwrap();
    let config = sha256_hex(b"config");
    let ledger = event_ledger();
    record_source_assurance(&ledger, &collector, &assurance(&config)).unwrap();
    let execution = collector
        .execute(&request(&repo, "execution", &config, true))
        .unwrap();
    let delivery = SourceDelivery {
        schema_version: 1,
        attempt_id: execution.attempt_id.clone(),
        owner_key: "owner-a".to_string(),
        source: "codex".to_string(),
        source_project_key: "project-native-a".to_string(),
        source_session_key: "session-native-a".to_string(),
        source_turn_id: Some("turn-native-a".to_string()),
        source_tool_id: "call-native-a".to_string(),
        wrapper_nonce: "execution".to_string(),
        source_event_id: "event-result".to_string(),
        source_exit_code: Some(0),
        source_term_signal: None,
        observed_at_unix_ms: 1,
    };
    let pre = execution.pre.as_ref().unwrap();
    let post = execution.post.as_ref().unwrap();
    accept_capture_pair(
        &ledger,
        &collector,
        &CaptureAcceptance {
            outcome: &execution,
            delivery: &delivery,
            pre_private_relative_ref: format!(
                "sealed/{}/pre.{}.json",
                execution.attempt_id, pre.capsule_id
            ),
            post_private_relative_ref: format!(
                "sealed/{}/post.{}.json",
                execution.attempt_id, post.capsule_id
            ),
        },
    )
    .unwrap();
    ledger
        .conn()
        .execute(
            "INSERT INTO nodes(
               id,project_id,label,sort_key,lifecycle,evidence_state,currency,
               session_id,anchor_event_id,derived_rev)
             VALUES('supported-node','project-db','supported','supported','completed',
                    'supported','current','session-db','event-result',1)",
            [],
        )
        .unwrap();

    fs::write(repo.join("tracked.txt"), "two\n").unwrap();
    let mut stale_request = request(&repo, "observation-stale", &config, true);
    stale_request.source_tool_id = "observation-stale".to_string();
    let stale_observation = collector.observe(&stale_request).unwrap();
    assert_eq!(
        stale_observation.capsule.payload.strength,
        CaptureStrength::BoundedObservation
    );
    accept_observation(&ledger, &collector, &stale_observation).unwrap();
    let accepted_observation_strength: String = ledger
        .conn()
        .query_row(
            "SELECT strength FROM capture_attempts WHERE attempt_id=?1",
            [&stale_observation.attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(accepted_observation_strength, "bounded_observation");
    let stale = compare_states(
        &post.payload.state,
        &stale_observation.capsule.payload.state,
    );
    assert_eq!(stale.currentness, ProofApplicability::Stale);
    record_state_comparison(
        &ledger,
        &post.capsule_id,
        &stale_observation.capsule.capsule_id,
        &stale,
    )
    .unwrap();
    assert_eq!(
        latest_proof_currentness(&ledger, "event-result").unwrap(),
        ProofApplicability::Stale
    );
    let evidence_state: String = ledger
        .conn()
        .query_row(
            "SELECT evidence_state FROM nodes WHERE id='supported-node'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(evidence_state, "supported");

    fs::write(repo.join("tracked.txt"), "one\n").unwrap();
    let mut current_request = request(&repo, "observation-current", &config, true);
    current_request.source_tool_id = "observation-current".to_string();
    let current_observation = collector.observe(&current_request).unwrap();
    accept_observation(&ledger, &collector, &current_observation).unwrap();
    let current = compare_states(
        &post.payload.state,
        &current_observation.capsule.payload.state,
    );
    assert_eq!(current.currentness, ProofApplicability::Current);
    record_state_comparison(
        &ledger,
        &post.capsule_id,
        &current_observation.capsule.capsule_id,
        &current,
    )
    .unwrap();
    assert_eq!(
        latest_proof_currentness(&ledger, "event-result").unwrap(),
        ProofApplicability::Current
    );
    let history: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM state_comparisons", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(history, 2);
}

#[test]
fn capture_tables_are_append_only() {
    let ledger = Ledger::open_in_memory().unwrap();
    for table in [
        "collector_keys",
        "source_assurance_attestations",
        "capture_attempts",
        "state_capsules",
        "capture_event_links",
        "state_comparisons",
        "capture_continuity_receipts",
    ] {
        let triggers: i64 = ledger
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='trigger' AND tbl_name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert!(triggers >= 2, "{table} lacks append-only triggers");
    }
}

#[test]
fn owner_and_source_scopes_produce_distinct_keys() {
    let temp = tempdir().unwrap();
    let codex_a = Collector::open(temp.path().join("a"), "owner-a", "codex").unwrap();
    let codex_b = Collector::open(temp.path().join("b"), "owner-b", "codex").unwrap();
    let claude_a = Collector::open(temp.path().join("c"), "owner-a", "claude_code").unwrap();
    assert_ne!(codex_a.key_id(), codex_b.key_id());
    assert_ne!(codex_a.key_id(), claude_a.key_id());
}

#[test]
fn staged_unstaged_and_untracked_dimensions_are_independent() {
    let temp = tempdir().unwrap();
    repository(temp.path());
    let secret = [3u8; 32];
    let clean = capture_stabilized(temp.path(), "git status --porcelain", &secret).unwrap();

    fs::write(temp.path().join("tracked.txt"), "staged\n").unwrap();
    run(temp.path(), &["add", "tracked.txt"]);
    let staged = capture_stabilized(temp.path(), "git status --porcelain", &secret).unwrap();
    assert_eq!(clean.head_commit, staged.head_commit);
    assert_ne!(clean.index_digest, staged.index_digest);
    assert_ne!(clean.staged_digest, staged.staged_digest);
    assert_ne!(
        clean.tracked_worktree_digest,
        staged.tracked_worktree_digest
    );

    run(temp.path(), &["reset", "--hard", "-q", "HEAD"]);
    fs::write(temp.path().join("tracked.txt"), "unstaged\n").unwrap();
    let unstaged = capture_stabilized(temp.path(), "git status --porcelain", &secret).unwrap();
    assert_eq!(clean.index_digest, unstaged.index_digest);
    assert_eq!(clean.staged_digest, unstaged.staged_digest);
    assert_ne!(
        clean.tracked_worktree_digest,
        unstaged.tracked_worktree_digest
    );

    run(temp.path(), &["reset", "--hard", "-q", "HEAD"]);
    fs::write(temp.path().join("untracked.txt"), "new\n").unwrap();
    let untracked = capture_stabilized(temp.path(), "git status --porcelain", &secret).unwrap();
    assert_eq!(clean.index_digest, untracked.index_digest);
    assert_eq!(
        clean.tracked_worktree_digest,
        untracked.tracked_worktree_digest
    );
    assert_ne!(clean.untracked_digest, untracked.untracked_digest);
}

#[test]
fn command_mutation_and_exact_nonzero_exit_are_distinguished() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    repository(&repo);
    let collector = Collector::open(temp.path().join("capture"), "owner-a", "codex").unwrap();
    let config = sha256_hex(b"config");

    let mut failed = request(&repo, "failed", &config, true);
    failed.command = "git rev-parse --verify refs/heads/definitely-absent".to_string();
    let failed_outcome = collector.execute(&failed).unwrap();
    assert_ne!(failed_outcome.exit_code, 0);
    assert_eq!(failed_outcome.receipt.strength, CaptureStrength::Assured);
    assert_eq!(
        failed_outcome.pre.unwrap().payload.state,
        failed_outcome.post.unwrap().payload.state
    );

    fs::write(repo.join("tracked.txt"), "dirty\n").unwrap();
    let mut mutating = request(&repo, "mutating", &config, true);
    mutating.command = "git checkout -- tracked.txt".to_string();
    let mutating_outcome = collector.execute(&mutating).unwrap();
    assert_eq!(mutating_outcome.exit_code, 0);
    assert_eq!(mutating_outcome.receipt.strength, CaptureStrength::Unknown);
    assert_ne!(
        mutating_outcome.pre.unwrap().payload.state,
        mutating_outcome.post.unwrap().payload.state
    );
}

#[test]
fn non_git_and_ignored_inputs_never_assure() {
    let temp = tempdir().unwrap();
    let collector = Collector::open(temp.path().join("capture"), "owner-a", "codex").unwrap();
    let config = sha256_hex(b"config");
    let outcome = collector
        .execute(&request(temp.path(), "non-git", &config, true))
        .unwrap();
    assert_eq!(outcome.receipt.strength, CaptureStrength::Unknown);

    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    repository(&repo);
    fs::write(repo.join(".gitignore"), "ignored.dat\n").unwrap();
    run(&repo, &["add", ".gitignore"]);
    run(&repo, &["commit", "-qm", "ignore"]);
    fs::write(repo.join("ignored.dat"), "input\n").unwrap();
    let state = capture_stabilized(&repo, "git status --porcelain", &[4u8; 32]).unwrap();
    assert!(!state.completeness.complete());
    assert!(state
        .completeness
        .failures
        .iter()
        .any(|failure| failure == "ignored_inputs_present"));
}

#[test]
fn missing_phase_exit_mismatch_and_out_of_order_post_fail_closed() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    repository(&repo);
    let collector = Collector::open(temp.path().join("capture"), "owner-a", "codex").unwrap();
    assert!(collector
        .record_hook_delivery("session-native-a", Some("turn-native-a"), "call-native-a")
        .is_err());
    let config = sha256_hex(b"config");
    let ledger = event_ledger();
    record_source_assurance(&ledger, &collector, &assurance(&config)).unwrap();
    let outcome = collector
        .execute(&request(&repo, "phase-test", &config, true))
        .unwrap();
    let mut missing = outcome.clone();
    missing.pre = None;
    let delivery = SourceDelivery {
        schema_version: 1,
        attempt_id: outcome.attempt_id.clone(),
        owner_key: "owner-a".to_string(),
        source: "codex".to_string(),
        source_project_key: "project-native-a".to_string(),
        source_session_key: "session-native-a".to_string(),
        source_turn_id: Some("turn-native-a".to_string()),
        source_tool_id: "call-native-a".to_string(),
        wrapper_nonce: "phase-test".to_string(),
        source_event_id: "event-result".to_string(),
        source_exit_code: Some(0),
        source_term_signal: None,
        observed_at_unix_ms: 1,
    };
    assert!(accept_capture_pair(
        &ledger,
        &collector,
        &CaptureAcceptance {
            outcome: &missing,
            delivery: &delivery,
            pre_private_relative_ref: "sealed/missing/pre.json".to_string(),
            post_private_relative_ref: "sealed/missing/post.json".to_string(),
        }
    )
    .is_err());
    let mut missing_post = outcome.clone();
    missing_post.post = None;
    assert!(accept_capture_pair(
        &ledger,
        &collector,
        &CaptureAcceptance {
            outcome: &missing_post,
            delivery: &delivery,
            pre_private_relative_ref: "sealed/missing/pre.json".to_string(),
            post_private_relative_ref: "sealed/missing/post.json".to_string(),
        }
    )
    .is_err());
    let mut wrong_exit = delivery.clone();
    wrong_exit.source_exit_code = Some(7);
    let pre = outcome.pre.as_ref().unwrap();
    let post = outcome.post.as_ref().unwrap();
    assert!(accept_capture_pair(
        &ledger,
        &collector,
        &CaptureAcceptance {
            outcome: &outcome,
            delivery: &wrong_exit,
            pre_private_relative_ref: format!(
                "sealed/{}/pre.{}.json",
                outcome.attempt_id, pre.capsule_id
            ),
            post_private_relative_ref: format!(
                "sealed/{}/post.{}.json",
                outcome.attempt_id, post.capsule_id
            ),
        }
    )
    .is_err());
    let mut wrong_turn = delivery.clone();
    wrong_turn.source_turn_id = Some("different-turn".to_string());
    assert!(accept_capture_pair(
        &ledger,
        &collector,
        &CaptureAcceptance {
            outcome: &outcome,
            delivery: &wrong_turn,
            pre_private_relative_ref: format!(
                "sealed/{}/pre.{}.json",
                outcome.attempt_id, pre.capsule_id
            ),
            post_private_relative_ref: format!(
                "sealed/{}/post.{}.json",
                outcome.attempt_id, post.capsule_id
            ),
        }
    )
    .is_err());
    let previous_chain: String = ledger
        .conn()
        .query_row(
            "SELECT chain_hash FROM events WHERE id='event-result'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let duplicate_raw = sha256_hex(b"duplicate-command");
    let duplicate_chain = chain_hash(&previous_chain, &duplicate_raw, "session-db", 3);
    ledger
        .conn()
        .execute(
            "INSERT INTO events(
               id,project_id,session_id,seq,kind,tool_name,source_tool_name,tool_use_id,tool_input,
               source_turn_id,raw_file,raw_line,raw_sha256,prev_hash,chain_hash)
             VALUES('event-ambiguous','project-db','session-db',3,'tool_use','Bash','exec_command',
                    'different-transcript-call',
                    '{\"cmd\":\"git status --porcelain\"}','turn-native-a',
                    'private-mirror',3,?1,?2,?3)",
            rusqlite::params![duplicate_raw, previous_chain, duplicate_chain],
        )
        .unwrap();
    assert!(accept_capture_pair(
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
        }
    )
    .is_err());
}

#[test]
fn private_payload_deletion_and_tampering_lose_verification() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    repository(&repo);
    let collector = Collector::open(temp.path().join("capture"), "owner-a", "codex").unwrap();
    let config = sha256_hex(b"config");
    let ledger = event_ledger();
    record_source_assurance(&ledger, &collector, &assurance(&config)).unwrap();
    let outcome = collector
        .execute(&request(&repo, "tamper", &config, true))
        .unwrap();
    let delivery = SourceDelivery {
        schema_version: 1,
        attempt_id: outcome.attempt_id.clone(),
        owner_key: "owner-a".to_string(),
        source: "codex".to_string(),
        source_project_key: "project-native-a".to_string(),
        source_session_key: "session-native-a".to_string(),
        source_turn_id: Some("turn-native-a".to_string()),
        source_tool_id: "call-native-a".to_string(),
        wrapper_nonce: "tamper".to_string(),
        source_event_id: "event-result".to_string(),
        source_exit_code: Some(0),
        source_term_signal: None,
        observed_at_unix_ms: 1,
    };
    let pre = outcome.pre.as_ref().unwrap();
    let post = outcome.post.as_ref().unwrap();
    let pre_ref = format!("sealed/{}/pre.{}.json", outcome.attempt_id, pre.capsule_id);
    let post_ref = format!(
        "sealed/{}/post.{}.json",
        outcome.attempt_id, post.capsule_id
    );
    accept_capture_pair(
        &ledger,
        &collector,
        &CaptureAcceptance {
            outcome: &outcome,
            delivery: &delivery,
            pre_private_relative_ref: pre_ref,
            post_private_relative_ref: post_ref.clone(),
        },
    )
    .unwrap();
    assert_eq!(collector.verify_accepted_capsules(&ledger).unwrap(), 2);
    fs::write(collector.root().join(post_ref), b"tampered").unwrap();
    assert!(collector.verify_accepted_capsules(&ledger).is_err());
}

#[test]
fn branch_head_worktree_lock_and_toolchain_changes_are_stale() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    repository(&repo);
    let secret = [9u8; 32];
    let base = capture_stabilized(&repo, "git status --porcelain", &secret).unwrap();

    run(&repo, &["checkout", "-qb", "other"]);
    let branch = capture_stabilized(&repo, "git status --porcelain", &secret).unwrap();
    assert_eq!(
        compare_states(&base, &branch).currentness,
        ProofApplicability::Stale
    );

    run(&repo, &["checkout", "-q", "-"]);
    fs::write(repo.join("tracked.txt"), "worktree\n").unwrap();
    let worktree = capture_stabilized(&repo, "git status --porcelain", &secret).unwrap();
    assert_eq!(
        compare_states(&base, &worktree).currentness,
        ProofApplicability::Stale
    );

    run(&repo, &["reset", "--hard", "-q", "HEAD"]);
    fs::write(repo.join("Cargo.lock"), "version = 3\n").unwrap();
    let lockfile = capture_stabilized(&repo, "git status --porcelain", &secret).unwrap();
    let lock_comparison = compare_states(&base, &lockfile);
    assert_eq!(lock_comparison.currentness, ProofApplicability::Stale);
    assert!(lock_comparison
        .changed_dimensions
        .iter()
        .any(|name| name == "lock_config"));

    use std::os::unix::fs::PermissionsExt;
    let tools = temp.path().join("tools");
    fs::create_dir(&tools).unwrap();
    let fake_git = tools.join("git");
    fs::write(
        &fake_git,
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo fake-git-one; fi\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&fake_git, fs::Permissions::from_mode(0o755)).unwrap();
    let fake_command = format!("{} status", fake_git.display());
    let toolchain_one = capture_stabilized(&repo, &fake_command, &secret).unwrap();
    fs::write(
        &fake_git,
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo fake-git-two; fi\nexit 0\n",
    )
    .unwrap();
    fs::set_permissions(&fake_git, fs::Permissions::from_mode(0o755)).unwrap();
    let toolchain_two = capture_stabilized(&repo, &fake_command, &secret).unwrap();
    let toolchain_comparison = compare_states(&toolchain_one, &toolchain_two);
    assert_eq!(toolchain_comparison.currentness, ProofApplicability::Stale);
    assert!(toolchain_comparison
        .changed_dimensions
        .iter()
        .any(|name| name == "toolchain"));
}

#[test]
fn concurrent_external_edit_never_false_assures() {
    let temp = tempdir().unwrap();
    let repo = temp.path().join("repo");
    fs::create_dir(&repo).unwrap();
    repository(&repo);
    let collector = Collector::open(temp.path().join("capture"), "owner-a", "codex").unwrap();
    let config = sha256_hex(b"config");
    let mut concurrent = request(&repo, "concurrent", &config, true);
    concurrent.command = "git -c alias.wait='!sleep 0.5' wait".to_string();
    let edited = repo.join("tracked.txt");
    let editor = thread::spawn(move || {
        thread::sleep(Duration::from_millis(350));
        fs::write(edited, "external\n").unwrap();
    });
    let outcome = collector.execute(&concurrent).unwrap();
    editor.join().unwrap();
    assert_ne!(outcome.receipt.strength, CaptureStrength::Assured);
}

#[test]
fn hardened_state_reader_executes_no_repository_helpers() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    repository(temp.path());
    let marker = temp.path().join("helper-ran");
    let hook = temp.path().join(".git/hooks/post-checkout");
    fs::write(
        &hook,
        format!("#!/bin/sh\nprintf ran > '{}'\n", marker.display()),
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    run(
        temp.path(),
        &["config", "diff.external", hook.to_str().unwrap()],
    );
    run(
        temp.path(),
        &["config", "credential.helper", hook.to_str().unwrap()],
    );
    fs::write(
        temp.path().join(".gitattributes"),
        "*.txt diff=hostile filter=hostile\n",
    )
    .unwrap();
    run(
        temp.path(),
        &["config", "diff.hostile.textconv", hook.to_str().unwrap()],
    );
    run(
        temp.path(),
        &["config", "filter.hostile.smudge", hook.to_str().unwrap()],
    );
    let state = capture_stabilized(temp.path(), "git status --porcelain", &[11u8; 32]).unwrap();
    assert!(state.completeness.complete());
    assert!(!marker.exists());
}
