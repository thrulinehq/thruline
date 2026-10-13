//! `thruline import-codex` — BUILD-SPEC §4.2's rollout path, end to end.
//!
//! # The gap this closes
//!
//! `capture::codex::recover` could name a rollout line's §3.1 kind and nothing
//! consumed it. A reader with no consumer is a capability that does not exist:
//! the 56,273 `patch_apply_end` and 2,791 `context_compacted` rows measured in
//! the legacy corpus stayed unmapped while the code to map them sat there.
//!
//! # The live half is `#[ignore]`d, like the Docker one
//!
//! `~/.codex/sessions` exists on this machine and is the only honest test of an
//! adapter for a vendor's file format — the same reason M3's container test
//! runs against a real daemon rather than a mock. It is `#[ignore]`d because a
//! populated Codex history is not a suite dependency, and its figures are
//! recorded in `REPORT.md`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn thruline() -> &'static str {
    qualified_binary::thruline()
}

#[path = "support/qualified_binary.rs"]
mod qualified_binary;

/// A scratch repository, a scratch home, and a scratch sessions root.
fn scratch() -> (tempfile::TempDir, PathBuf, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");
    let repo = dir.path().join("repo");
    let sessions = dir.path().join("sessions");
    std::fs::create_dir_all(&repo).expect("repo");
    (dir, home, repo, sessions)
}

fn write_rollout(sessions: &Path, name: &str, lines: &[String]) {
    let day = sessions.join("2026").join("04").join("20");
    std::fs::create_dir_all(&day).expect("mkdir");
    std::fs::write(day.join(name), lines.join("\n")).expect("write");
}

fn import(home: &Path, repo: &Path, sessions: &Path, extra: &[&str]) -> serde_json::Value {
    let out = Command::new(thruline())
        .arg("import-codex")
        .arg("--sessions-root")
        .arg(sessions)
        .arg("--json")
        .args(extra)
        .current_dir(repo)
        .env("THRULINE_HOME", home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .output()
        .expect("import-codex");
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

fn a_session(cwd: &Path, id: &str) -> Vec<String> {
    vec![
        format!(
            r#"{{"timestamp":"2026-04-20T19:46:44.000Z","type":"session_meta","payload":{{"id":"{id}","cwd":{:?}}}}}"#,
            cwd.to_string_lossy()
        ),
        r#"{"timestamp":"2026-04-20T19:47:00.000Z","type":"event_msg","payload":{"type":"patch_apply_end","call_id":"c1","stdout":"ok","stderr":"","success":true,"changes":{"/repo/a.ts":{"type":"update","unified_diff":"@@\n-a\n+b\n"},"/repo/b.ts":{"type":"add","content":"x"}},"status":"completed"}}"#.into(),
        r#"{"timestamp":"2026-04-20T19:48:00.000Z","type":"event_msg","payload":{"type":"context_compacted"}}"#.into(),
        r#"{"timestamp":"2026-04-20T19:48:00.100Z","type":"compacted","payload":{"message":"summary","replacement_history":[]}}"#.into(),
        r#"{"timestamp":"2026-04-20T19:49:00.000Z","type":"event_msg","payload":{"type":"agent_message","message":"All tests pass."}}"#.into(),
    ]
}

// ---------------------------------------------------------------------------
// The command
// ---------------------------------------------------------------------------

#[test]
fn a_rollout_becomes_events_and_the_session_is_degraded() {
    let (_dir, home, repo, sessions) = scratch();
    write_rollout(&sessions, "rollout-a.jsonl", &a_session(&repo, "sess-a"));

    let report = import(&home, &repo, &sessions, &[]);
    assert_eq!(report["ok"], serde_json::json!(true), "{report:#}");
    assert_eq!(report["sessions"], serde_json::json!(1));
    assert_eq!(report["fs_changes"], serde_json::json!(2));
    assert_eq!(report["compaction_boundaries"], serde_json::json!(1));
    assert_eq!(
        report["capture_degraded"],
        serde_json::json!(true),
        "§13's ceiling must be in the report a machine reads, not only in prose"
    );

    // ...and it is on the row, not merely in the report.
    let ledger = thruline_ledger::Ledger::open(&home.join("exec.db")).expect("ledger");
    assert!(ledger.capture_degraded("sess-a").expect("read"));
    ledger.verify_session("sess-a").expect("the chain verifies");

    // The claim in that rollout says "All tests pass." and there is no
    // execution evidence anywhere. It must not be supportable.
    let out = Command::new(thruline())
        .args(["claims", "--session", "sess-a", "--json"])
        .current_dir(&repo)
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .output()
        .expect("claims");
    let claims: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    // Commit G p4-fog: the degradation on the row is served beside the claims
    // it capped, so a reader can tell Fog from Recorded without a receipt.
    assert_eq!(
        claims["capture_degraded"],
        serde_json::json!(true),
        "claims --json does not serve the session's capture flag: {claims:#}"
    );
    for claim in claims["claims"].as_array().unwrap_or(&Vec::new()) {
        assert_ne!(
            claim["verdict"],
            serde_json::json!("supported"),
            "a claim from an imported rollout reached `supported`: {claim:#}"
        );
    }
}

#[test]
fn explain_does_not_repeat_a_later_claim_at_an_earlier_message() {
    let (_dir, home, repo, sessions) = scratch();
    let mut lines = a_session(&repo, "distinct-messages");
    lines.insert(1, r#"{"timestamp":"2026-04-20T19:46:45Z","type":"event_msg","payload":{"type":"agent_message","message":"I will inspect the source."}}"#.into());
    write_rollout(&sessions, "rollout-messages.jsonl", &lines);
    import(&home, &repo, &sessions, &[]);
    let out = Command::new(thruline())
        .args(["explain", "--session", "distinct-messages", "--json"])
        .current_dir(&repo)
        .env("THRULINE_HOME", &home)
        .output()
        .expect("explain");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        report["claims_vs_proof"].as_array().unwrap().len(),
        1,
        "{report:#}"
    );
}

#[test]
fn a_phased_progress_message_does_not_invent_a_finished_turn() {
    for phase in [
        serde_json::json!("commentary"),
        serde_json::json!("unknown_future_phase"),
        serde_json::Value::Null,
        serde_json::json!(42),
    ] {
        let (_dir, home, repo, sessions) = scratch();
        let meta = a_session(&repo, "progress-only").remove(0);
        let message = serde_json::json!({
            "timestamp": "2026-04-20T19:47:00Z", "type": "event_msg",
            "payload": {"type": "agent_message", "phase": phase, "message": "I will inspect the source."}
        }).to_string();
        write_rollout(&sessions, "rollout-progress.jsonl", &[meta, message]);
        import(&home, &repo, &sessions, &[]);
        let ledger = thruline_ledger::Ledger::open(&home.join("exec.db")).unwrap();
        let events = ledger.session_events("progress-only").unwrap();
        assert!(
            !events.iter().any(|e| e.kind().as_str() == "stop"),
            "{phase} invented a stop"
        );
        assert!(
            events.iter().any(|e| e
                .payload()
                .get("message")
                .and_then(serde_json::Value::as_str)
                == Some("I will inspect the source.")),
            "the progress message must remain in the record"
        );
    }
    let (_dir, home, repo, sessions) = scratch();
    let mut lines = a_session(&repo, "final-message");
    let last = lines.last_mut().unwrap();
    *last = last.replace(
        "\"agent_message\",",
        "\"agent_message\",\"phase\":\"final_answer\",",
    );
    write_rollout(&sessions, "rollout-final.jsonl", &lines);
    import(&home, &repo, &sessions, &[]);
    let ledger = thruline_ledger::Ledger::open(&home.join("exec.db")).unwrap();
    assert!(ledger
        .session_events("final-message")
        .unwrap()
        .iter()
        .any(|e| e.kind().as_str() == "stop"));
}

#[test]
fn a_dry_run_writes_nothing_and_counts_both_sides() {
    let (_dir, home, repo, sessions) = scratch();
    write_rollout(&sessions, "rollout-a.jsonl", &a_session(&repo, "sess-a"));
    write_rollout(
        &sessions,
        "rollout-b.jsonl",
        &a_session(Path::new("/somewhere/else"), "sess-b"),
    );

    let report = import(&home, &repo, &sessions, &["--dry-run"]);
    assert_eq!(report["rollouts_for_this_repo"], serde_json::json!(1));
    assert_eq!(
        report["rollouts_for_other_repos"],
        serde_json::json!(1),
        "\"none found\" and \"none were yours\" are different answers: {report:#}"
    );
    assert!(!home.join("exec.db").exists(), "a dry run created a store");
}

#[test]
fn re_importing_writes_nothing_and_exits_zero() {
    let (_dir, home, repo, sessions) = scratch();
    write_rollout(&sessions, "rollout-a.jsonl", &a_session(&repo, "sess-a"));
    let first = import(&home, &repo, &sessions, &[]);
    assert!(first["events_written"].as_u64().unwrap_or(0) > 0);
    let second = import(&home, &repo, &sessions, &[]);
    assert_eq!(
        second["events_written"],
        serde_json::json!(0),
        "a re-import wrote events: the chain is not a function of the file"
    );
}

/// A missing sessions directory is an error naming the real layout.
///
/// §4.2 says `.codex/sessions/*.jsonl`, which does not exist anywhere; built as
/// written the adapter would find nothing and report a clean absence.
#[test]
fn a_missing_sessions_root_names_the_layout_that_actually_exists() {
    let (_dir, home, repo, sessions) = scratch();
    let out = Command::new(thruline())
        .arg("import-codex")
        .arg("--sessions-root")
        .arg(sessions.join("nope"))
        .current_dir(&repo)
        .env("THRULINE_HOME", &home)
        .output()
        .expect("import-codex");
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("YYYY/MM/DD"),
        "the error must name the real, date-nested layout: {err}"
    );
}

// ---------------------------------------------------------------------------
// Against a real Codex history. Needs one.
// ---------------------------------------------------------------------------

/// The adapter, against files a real Codex wrote.
///
/// Everything above uses rollouts this test wrote, which tells you about the
/// fixtures. This one reads `~/.codex/sessions` — a vendor's own files, in the
/// vendor's own layout, at whatever version wrote them — and is the only thing
/// here that can catch a contract this build has transcribed wrongly.
/// The repository the most rollouts on this machine belong to.
///
/// Read from the rollouts' own `session_meta.payload.cwd`, so the test is about
/// this machine's Codex history rather than about where the test happens to be
/// checked out.
fn busiest_repository() -> Option<PathBuf> {
    use std::collections::BTreeMap;
    use std::io::BufRead;
    let root = thruline_capture::codex_rollout::default_sessions_root()?;
    let mut counts: BTreeMap<PathBuf, usize> = BTreeMap::new();
    for path in thruline_capture::codex::rollouts(&root) {
        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        let mut first = String::new();
        if std::io::BufReader::new(file).read_line(&mut first).is_err() {
            continue;
        }
        if let Ok(thruline_capture::codex::Recovered {
            kind: thruline_capture::codex::RecoveredKind::SessionMeta { cwd, .. },
            ..
        }) = thruline_capture::codex::recover(0, first.trim())
        {
            let cwd = PathBuf::from(&cwd);
            // Only a directory that still exists: an import is about a
            // repository, and one that has been deleted is not one.
            if cwd.is_dir() {
                *counts.entry(cwd).or_insert(0) += 1;
            }
        }
    }
    counts.into_iter().max_by_key(|(_, n)| *n).map(|(p, _)| p)
}

#[test]
#[ignore = "requires a populated ~/.codex/sessions. Run with --ignored."]
fn the_adapter_reads_a_real_codex_history() {
    let root = thruline_capture::codex_rollout::default_sessions_root().expect("HOME");
    assert!(
        root.is_dir(),
        "this test was asked for explicitly and {} does not exist — a skip here \
         would report the adapter as exercised against a real Codex when it was \
         not",
        root.display()
    );
    let dir = tempfile::tempdir().expect("tempdir");
    let home = dir.path().join("home");

    // **The repository comes from the rollouts, not from this checkout.**
    //
    // The first version used `CARGO_MANIFEST_DIR/../..`, which is whatever
    // tree the test is running in — and in a gate worktree that is a temporary
    // path no Codex has ever seen, so the test failed with "no rollout names
    // this repository". That was the test asserting something about its own
    // location. The claim is "the adapter reads a real Codex history", so the
    // subject is whichever repository this machine's history is *of*.
    let repo = busiest_repository().expect(
        "no rollout under ~/.codex/sessions names any repository — the layout \
         assumption or the session_meta reader is wrong",
    );
    eprintln!(
        "real Codex history: importing rollouts for {}",
        repo.display()
    );

    // **Bounded.** Unbounded, this imported the busiest repository's whole
    // history — 272 MB of events and still going after three minutes. A test
    // that heavy is one nobody runs, and the thing being tested is whether the
    // adapter reads a real vendor file at a real version, which twenty-five of
    // them establish as well as four hundred. The unbounded run stays available
    // as `thruline import-codex`; its figures for this repository are in
    // `REPORT.md`.
    let out = Command::new(thruline())
        .arg("import-codex")
        .arg("--sessions-root")
        .arg(&root)
        .arg("--limit")
        .arg("25")
        .arg("--json")
        .arg("--repo")
        .arg(&repo)
        .current_dir(&repo)
        .env("THRULINE_HOME", &home)
        .env_remove("CLAUDE_CODE_SESSION_ID")
        .output()
        .expect("import-codex");
    let report: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}\n{}", String::from_utf8_lossy(&out.stdout)));
    assert_eq!(report["ok"], serde_json::json!(true), "{report:#}");

    let sessions = report["sessions"].as_u64().unwrap_or(0);
    assert!(
        sessions > 0,
        "no rollout in {} names this repository — the containment check or the \
         layout assumption is wrong: {report:#}",
        root.display()
    );
    // The two record types this adapter exists for.
    assert!(
        report["fs_changes"].as_u64().unwrap_or(0) > 0,
        "a real history yielded no recovered file change: {report:#}"
    );

    // Every chain must verify. This is the assertion a hand-written fixture
    // cannot make: real rollouts carry shapes nobody transcribed.
    // The session ids come from the store itself, read directly, because the
    // ledger exposes no session iterator and inventing one for a test would be
    // a bigger change than the test is worth.
    let db = home.join("exec.db");
    let ids: Vec<String> = {
        let conn = rusqlite::Connection::open(&db).expect("open");
        let mut stmt = conn.prepare("SELECT id FROM sessions").expect("prepare");
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .expect("query");
        rows.flatten().collect()
    };
    let ledger = thruline_ledger::Ledger::open(&db).expect("ledger");
    let mut verified = 0u64;
    let mut degraded = 0u64;
    for id in &ids {
        ledger.verify_session(id).unwrap_or_else(|e| {
            panic!("session {id} imported from a real rollout does not verify: {e}")
        });
        verified += 1;
        if ledger.capture_degraded(id).unwrap_or(false) {
            degraded += 1;
        }
    }
    assert_eq!(verified, sessions, "not every imported session was walked");
    assert_eq!(
        degraded,
        verified,
        "{} of {verified} imported sessions are NOT capture_degraded",
        verified - degraded
    );

    eprintln!(
        "real Codex import: {} rollout(s) seen, {} for other repositories, \
         {sessions} session(s), {} line(s), {} event(s), {} file change(s), \
         {} compaction boundary(ies), {} refused",
        report["rollouts_seen"],
        report["rollouts_for_other_repos"],
        report["lines_read"],
        report["events_written"],
        report["fs_changes"],
        report["compaction_boundaries"],
        report["refused"].as_array().map(Vec::len).unwrap_or(0),
    );
    eprintln!("unmapped families: {}", report["unmapped"]);
}
