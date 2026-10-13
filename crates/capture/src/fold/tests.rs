use super::ToolOutcome;
use super::*;
use crate::claude_code;
use crate::redaction::Redactor;
use thruline_ledger::Ledger;

fn spool_with(dir: &Path, deliveries: &[&str]) -> PathBuf {
    let spool = dir.join("claude-code.jsonl");
    for raw in deliveries {
        let line =
            claude_code::prepare(raw.as_bytes(), "2026-09-02T10:11:12.345Z", &Redactor::new())
                .render();
        claude_code::append_line(&spool, &line).expect("append");
    }
    spool
}

fn delivery(event: &str, session: &str, extra: &str) -> String {
    format!(
        r#"{{"hook_event_name":"{event}","session_id":"{session}",
           "transcript_path":"/t","cwd":"/repo"{extra}}}"#
    )
}

fn session_of(n: usize) -> String {
    format!("sess-{n}")
}

fn ledger_and_repo() -> (Ledger, RepoId) {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let repo = ledger
        .repo("git@example.invalid:acme/app.git", None)
        .expect("repo");
    (ledger, repo)
}

#[test]
fn a_spool_folds_into_a_chain_that_verifies() {
    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(1);
    let spool = spool_with(
        dir.path(),
        &[
            &delivery("SessionStart", &s, r#","source":"startup""#),
            &delivery("UserPromptSubmit", &s, r#","prompt":"fix the parser""#),
            &delivery(
                "PreToolUse",
                &s,
                r#","tool_name":"Bash","tool_input":{"command":"cargo test"}"#,
            ),
            &delivery(
                "PostToolUse",
                &s,
                r#","tool_name":"Bash","tool_response":"ok","tool_use_id":"t1""#,
            ),
            &delivery("Stop", &s, r#","last_assistant_message":"All tests pass""#),
            &delivery("SessionEnd", &s, r#","reason":"clear""#),
        ],
    );

    let (ledger, repo) = ledger_and_repo();
    let report = fold(&spool, &ledger, &repo, None).expect("fold");
    assert_eq!(report.lines_read, 6);
    assert_eq!(report.events_written, 6);
    assert!(report.quarantined.is_empty(), "{:?}", report.quarantined);

    let head = ledger
        .verify_session(&s)
        .expect("the folded chain verifies");
    assert_eq!(head.event_count, 6);

    // The kinds are the §3.1 ones, not the hook names.
    let kinds: Vec<String> = ledger
        .session_events(&s)
        .expect("read")
        .iter()
        .map(|e| e.kind().as_str())
        .collect();
    assert_eq!(
        kinds,
        vec![
            "session_start",
            "user_prompt",
            "tool_call",
            "tool_result",
            "stop",
            "session_end"
        ]
    );
}

#[test]
fn a_failed_tool_folds_to_a_result_that_is_not_ok() {
    // The distinction the two hook events exist to carry. If this collapsed,
    // every result would look successful and the product's central claim would
    // be unfalsifiable.
    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(2);
    let spool = spool_with(
        dir.path(),
        &[
            &delivery(
                "PostToolUse",
                &s,
                r#","tool_name":"Bash","tool_response":"1 passed","tool_use_id":"a""#,
            ),
            &delivery(
                "PostToolUseFailure",
                &s,
                r#","tool_name":"Bash","error":"Exit code 1","tool_use_id":"b""#,
            ),
        ],
    );
    let (ledger, repo) = ledger_and_repo();
    fold(&spool, &ledger, &repo, None).expect("fold");

    // Read through `tool_outcome`, like every other reader in the workspace —
    // the census in `tests/ok_basis_census.rs` permits exactly one. It is also
    // strictly stronger than the `["ok"].as_bool().unwrap_or(true)` this
    // replaced: that read an ABSENT field as a success, which is the same
    // fallback-for-a-fact the three-state type exists to remove.
    let outcomes: Vec<ToolOutcome> = ledger
        .session_events(&s)
        .expect("read")
        .iter()
        .map(|e| super::tool_outcome(e.payload()))
        .collect();
    assert_eq!(
        outcomes,
        vec![ToolOutcome::Succeeded, ToolOutcome::Failed],
        "the failure was recorded as a success"
    );

    // And neither invents an exit code the hook never carried.
    for e in ledger.session_events(&s).expect("read") {
        assert!(
            e.payload().get("exit_code").is_none(),
            "an exit code was fabricated from a hook that carries none"
        );
    }
}

#[test]
fn folding_twice_writes_nothing_the_second_time() {
    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(3);
    let spool = spool_with(
        dir.path(),
        &[
            &delivery("SessionStart", &s, ""),
            &delivery("Stop", &s, r#","last_assistant_message":"done""#),
        ],
    );
    let (ledger, repo) = ledger_and_repo();
    let first = fold(&spool, &ledger, &repo, None).expect("fold");
    assert_eq!(first.events_written, 2);

    let head_before = ledger.verify_session(&s).expect("verifies");
    let second = fold(&spool, &ledger, &repo, None).expect("re-fold");
    assert_eq!(second.events_written, 0, "a replay wrote rows");
    assert_eq!(second.events_skipped, 2);
    assert_eq!(
        ledger.verify_session(&s).expect("verifies"),
        head_before,
        "a replay changed the chain"
    );
}

#[test]
fn an_interrupted_fold_replays_to_the_same_chain() {
    // Crash consistency, as a test rather than an argument. A fold interrupted
    // after some batches committed must resume and produce the SAME chain a
    // single uninterrupted fold would.
    //
    // Simulated by folding a prefix of the spool, then folding the whole thing —
    // which is exactly what a process killed mid-run leaves behind, because the
    // spool is append-only and the fold never writes to it.
    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(4);
    let all: Vec<String> = (0..8)
        .map(|i| delivery("UserPromptSubmit", &s, &format!(r#","prompt":"p{i}""#)))
        .collect();
    let refs: Vec<&str> = all.iter().map(String::as_str).collect();

    // The uninterrupted reference chain.
    let full_spool = spool_with(&dir.path().join("full"), &refs);
    let (reference, repo_a) = ledger_and_repo();
    fold(&full_spool, &reference, &repo_a, None).expect("fold");
    let expected = reference.verify_session(&s).expect("verifies");

    // The interrupted one: fold a prefix, then the whole spool.
    let partial_dir = dir.path().join("partial");
    let partial = spool_with(&partial_dir, &refs[..3]);
    let (interrupted, repo_b) = ledger_and_repo();
    fold(&partial, &interrupted, &repo_b, None).expect("partial fold");
    assert_eq!(
        interrupted
            .verify_session(&s)
            .expect("verifies")
            .event_count,
        3
    );

    for raw in &refs[3..] {
        let line =
            claude_code::prepare(raw.as_bytes(), "2026-09-02T10:11:12.345Z", &Redactor::new())
                .render();
        claude_code::append_line(&partial, &line).expect("append");
    }
    let resumed = fold(&partial, &interrupted, &repo_b, None).expect("resumed fold");
    assert_eq!(resumed.events_skipped, 3, "the resume re-wrote stored rows");
    assert_eq!(resumed.events_written, 5);

    let actual = interrupted.verify_session(&s).expect("verifies");
    assert_eq!(
        actual, expected,
        "an interrupted fold produced a different chain from an uninterrupted one"
    );
}

#[test]
fn a_line_that_cannot_be_mapped_is_quarantined_counted_and_never_dropped() {
    // "Never silently drop, always report" — the invariant that produced
    // 1,442,374 preserved `meta` rows in the legacy corpus.
    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(5);
    let spool = spool_with(
        dir.path(),
        &[
            &delivery("SessionStart", &s, ""),
            // A subscribed-looking event with no §3.1 mapping.
            &delivery("TeammateIdle", &s, ""),
            &delivery("Stop", &s, r#","last_assistant_message":"done""#),
        ],
    );
    let (ledger, repo) = ledger_and_repo();
    let report = fold(&spool, &ledger, &repo, None).expect("fold");

    assert_eq!(report.quarantined.len(), 1, "{:?}", report.quarantined);
    assert!(report.quarantined[0].1.contains("TeammateIdle"));
    assert_eq!(report.events_written, 2, "the mappable lines still landed");

    // The quarantine file exists, holds the line, and is machine-readable.
    let q = std::fs::read_to_string(quarantine_path(&spool)).expect("quarantine file");
    let parsed: serde_json::Value =
        serde_json::from_str(q.lines().next().unwrap_or_default()).expect("quarantine is JSON");
    assert_eq!(parsed["spool_seq"], serde_json::json!(2));
    assert!(parsed["reason"]
        .as_str()
        .unwrap_or_default()
        .contains("no §3.1 mapping"));

    // And the surviving chain still verifies — a hole in the spool must not
    // break the events that did land.
    let head = ledger
        .verify_session(&s)
        .expect("the remaining chain verifies");
    assert_eq!(head.event_count, 2);
}

#[test]
fn a_line_with_no_session_is_quarantined_rather_than_guessed_at() {
    let dir = tempfile::tempdir().expect("tempdir");
    let spool = dir.path().join("s.jsonl");
    claude_code::append_line(&spool, "{\"hook_event\":\"Stop\",\"payload\":{}}\n").expect("write");
    claude_code::append_line(&spool, "not json at all\n").expect("write");

    let (ledger, repo) = ledger_and_repo();
    let report = fold(&spool, &ledger, &repo, None).expect("fold");
    assert_eq!(report.lines_read, 2);
    assert_eq!(report.events_written, 0);
    assert_eq!(report.quarantined.len(), 2);
    let reasons: Vec<&str> = report.quarantined.iter().map(|(_, r)| r.as_str()).collect();
    assert!(reasons.iter().any(|r| r.contains("no session_id")));
    assert!(reasons.iter().any(|r| r.contains("not JSON")));
}

#[test]
fn a_missing_spool_is_not_an_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (ledger, repo) = ledger_and_repo();
    let report = fold(
        &dir.path().join("never-written.jsonl"),
        &ledger,
        &repo,
        None,
    )
    .expect("a missing spool means nothing has happened yet");
    assert_eq!(report, FoldReport::default());
}

#[test]
fn two_sessions_in_one_spool_get_independent_chains() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (a, b) = (session_of(6), session_of(7));
    let spool = spool_with(
        dir.path(),
        &[
            &delivery("SessionStart", &a, ""),
            &delivery("SessionStart", &b, ""),
            &delivery("Stop", &a, r#","last_assistant_message":"a done""#),
            &delivery("Stop", &b, r#","last_assistant_message":"b done""#),
        ],
    );
    let (ledger, repo) = ledger_and_repo();
    let report = fold(&spool, &ledger, &repo, None).expect("fold");
    assert_eq!(report.sessions, 2);
    assert_eq!(report.events_written, 4);

    for s in [&a, &b] {
        let head = ledger
            .verify_session(s)
            .expect("each session verifies alone");
        assert_eq!(head.event_count, 2);
        assert_eq!(head.session_id, *s);
    }
    // Interleaved global seq is fine: the chain requires increasing, not
    // contiguous.
    let seqs: Vec<u64> = ledger
        .session_events(&a)
        .expect("read")
        .iter()
        .map(|e| e.seq())
        .collect();
    // The stored `seq` is the spool ordinal times the stride, so the property
    // the old literal `[1, 3]` stood for — increasing, and a pure function of
    // the line's position in the spool — is asserted directly rather than
    // spelled out as two numbers.
    assert_eq!(
        seqs,
        vec![SPOOL_SEQ_STRIDE, 3 * SPOOL_SEQ_STRIDE],
        "seq is no longer a function of the spool ordinal"
    );
}

#[test]
fn a_mutating_tool_produces_a_snapshot_event_and_a_read_only_one_does_not() {
    // §4.4 boundaries. A snapshot after a read-only tool is pure cost: the tree
    // cannot have moved, and the engine writes objects into the user's repo.
    let repo_dir = tempfile::tempdir().expect("tempdir");
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.email", "t@e.invalid"],
        vec!["config", "user.name", "T"],
    ] {
        let out = std::process::Command::new("git")
            .current_dir(repo_dir.path())
            .args(&args)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}");
    }
    std::fs::write(repo_dir.path().join("a.txt"), "a\n").expect("write");

    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(8);
    let spool = spool_with(
        dir.path(),
        &[
            &delivery(
                "PostToolUse",
                &s,
                r#","tool_name":"Read","tool_response":"contents""#,
            ),
            &delivery(
                "PostToolUse",
                &s,
                r#","tool_name":"Write","tool_response":"written""#,
            ),
        ],
    );
    let (ledger, repo) = ledger_and_repo();
    let report = fold(&spool, &ledger, &repo, Some(repo_dir.path())).expect("fold");

    assert_eq!(
        report.snapshots_taken, 1,
        "expected exactly one snapshot: after Write, not after Read"
    );
    let kinds: Vec<String> = ledger
        .session_events(&s)
        .expect("read")
        .iter()
        .map(|e| e.kind().as_str())
        .collect();
    assert_eq!(kinds, vec!["tool_result", "tool_result", "snapshot"]);

    let snap = ledger.session_events(&s).expect("read")[2]
        .payload()
        .clone();
    assert_eq!(snap["tree_hash"].as_str().map(str::len), Some(40));
    assert_eq!(snap["dirty"], serde_json::json!(true));
    ledger
        .verify_session(&s)
        .expect("the chain with a snapshot verifies");
}

#[test]
fn the_fold_never_writes_to_the_spool() {
    // What makes the crash story simple: the spool is append-only and this is a
    // reader. If the fold ever mutated it, an interrupted run could lose
    // deliveries that were never folded.
    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(9);
    let spool = spool_with(dir.path(), &[&delivery("SessionStart", &s, "")]);
    let before = std::fs::read(&spool).expect("read");
    let (ledger, repo) = ledger_and_repo();
    fold(&spool, &ledger, &repo, None).expect("fold");
    fold(&spool, &ledger, &repo, None).expect("fold again");
    assert_eq!(
        before,
        std::fs::read(&spool).expect("read"),
        "the fold wrote to the spool"
    );
}

#[test]
fn the_chain_head_is_a_function_of_the_spool_and_nothing_else() {
    // The property the crash test uncovered, stated directly. Two independent
    // ledgers, same spool, same head — so the head a receipt publishes can be
    // recomputed by anyone holding the evidence. With random event ids this
    // failed: same event count, same last_seq, different hash.
    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(10);
    let spool = spool_with(
        dir.path(),
        &[
            &delivery("SessionStart", &s, r#","source":"startup""#),
            &delivery(
                "PreToolUse",
                &s,
                r#","tool_name":"Bash","tool_input":{"command":"cargo test"}"#,
            ),
            &delivery(
                "PostToolUseFailure",
                &s,
                r#","tool_name":"Bash","error":"Exit code 1""#,
            ),
            &delivery("Stop", &s, r#","last_assistant_message":"tests fail""#),
        ],
    );

    let (a, repo_a) = ledger_and_repo();
    let (b, repo_b) = ledger_and_repo();
    fold(&spool, &a, &repo_a, None).expect("fold a");
    fold(&spool, &b, &repo_b, None).expect("fold b");

    let head_a = a.verify_session(&s).expect("a verifies");
    let head_b = b.verify_session(&s).expect("b verifies");
    assert_eq!(
        head_a, head_b,
        "two folds of one spool produced different chains — the head is not a \
         function of the evidence"
    );

    // And the ids themselves are stable, not merely the head.
    let ids_a: Vec<String> = a
        .session_events(&s)
        .expect("read")
        .iter()
        .map(|e| e.id().to_string())
        .collect();
    let ids_b: Vec<String> = b
        .session_events(&s)
        .expect("read")
        .iter()
        .map(|e| e.id().to_string())
        .collect();
    assert_eq!(ids_a, ids_b);

    // The control: a DIFFERENT spool must produce a different head, or the
    // assertions above would hold for a fold that ignored its input.
    let other = spool_with(
        &dir.path().join("other"),
        &[&delivery("SessionStart", &s, r#","source":"resume""#)],
    );
    let (c, repo_c) = ledger_and_repo();
    fold(&other, &c, &repo_c, None).expect("fold c");
    assert_ne!(
        c.verify_session(&s).expect("c verifies").head_hash,
        head_a.head_hash
    );
}

#[test]
fn a_snapshot_that_cannot_be_taken_is_counted_not_skipped() {
    // Found by the end-to-end run: `snapshots_taken` was 0 while every other
    // number looked right, because the error was discarded with `.ok()?`. A
    // missing snapshot means claims cannot be bound to a tree (§6.1), which is
    // the difference between `unverifiable` and `supported` — so it has to be
    // visible.
    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(11);
    let spool = spool_with(
        dir.path(),
        &[&delivery(
            "PostToolUse",
            &s,
            r#","tool_name":"Write","tool_response":"ok""#,
        )],
    );
    // A path that is not a git repository: the snapshot cannot be taken.
    let not_a_repo = tempfile::tempdir().expect("tempdir");

    let (ledger, repo) = ledger_and_repo();
    let report = fold(&spool, &ledger, &repo, Some(not_a_repo.path())).expect("fold");

    assert_eq!(report.snapshots_taken, 0);
    assert_eq!(
        report.snapshots_failed.len(),
        1,
        "a snapshot failure vanished instead of being counted"
    );
    assert!(report.snapshots_failed[0].1.contains("snapshot of"));
    // The tool_result still landed: one failed snapshot must not cost the event.
    assert_eq!(report.events_written, 1);
    ledger.verify_session(&s).expect("the chain still verifies");
}

#[test]
fn a_delivery_after_a_mutating_tool_is_not_swallowed_by_its_snapshot() {
    // The second silent drop, found by looking for the first one's generator.
    //
    // The fold-derived snapshot was numbered `line_seq + 1`, which is the NEXT
    // line's own ordinal. That line then hit the `seq <= last_seq` resume guard
    // and was counted as *already present*: a real hook delivery vanished and
    // the report called it idempotence. It was invisible only because the one
    // test that exercised a snapshot put the mutating tool LAST.
    //
    // The order here is the fix's whole point: Write (mutating) FIRST, Read
    // after it.
    let repo_dir = tempfile::tempdir().expect("tempdir");
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.email", "t@e.invalid"],
        vec!["config", "user.name", "T"],
    ] {
        let out = std::process::Command::new("git")
            .current_dir(repo_dir.path())
            .args(&args)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}");
    }
    std::fs::write(repo_dir.path().join("a.txt"), "a\n").expect("write");

    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(12);
    let spool = spool_with(
        dir.path(),
        &[
            &delivery(
                "PostToolUse",
                &s,
                r#","tool_name":"Write","tool_response":"written""#,
            ),
            &delivery(
                "PostToolUse",
                &s,
                r#","tool_name":"Read","tool_response":"contents""#,
            ),
            &delivery("Stop", &s, r#","last_assistant_message":"done""#),
        ],
    );
    let (ledger, repo) = ledger_and_repo();
    let report = fold(&spool, &ledger, &repo, Some(repo_dir.path())).expect("fold");

    assert_eq!(report.lines_read, 3);
    assert_eq!(report.snapshots_taken, 1);
    assert_eq!(
        report.events_skipped, 0,
        "a delivery was counted as already present on a first fold"
    );
    assert_eq!(report.events_written, 4, "3 deliveries + 1 snapshot");

    let kinds: Vec<String> = ledger
        .session_events(&s)
        .expect("read")
        .iter()
        .map(|e| e.kind().as_str())
        .collect();
    assert_eq!(
        kinds,
        vec!["tool_result", "snapshot", "tool_result", "stop"],
        "the Read and the Stop must both survive the snapshot inserted before them"
    );
    ledger.verify_session(&s).expect("the chain verifies");

    // And it is still idempotent: a re-fold writes nothing.
    let again = fold(&spool, &ledger, &repo, Some(repo_dir.path())).expect("re-fold");
    assert_eq!(again.events_written, 0);
    assert_eq!(again.events_skipped, 3);
}

#[test]
fn a_quarantine_that_cannot_be_written_is_reported_not_assumed() {
    // The FoldReport's `quarantined` list is a claim about a FILE. When the
    // write failed it was discarded with `let _ =`, so `doctor` would report
    // "N deliveries held at <path>" about a path holding nothing. Same class as
    // the swallowed snapshot: the count was right, the evidence was not there.
    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(13);
    let spool = spool_with(dir.path(), &[&delivery("TeammateIdle", &s, "")]);

    // Make the quarantine path unwritable by making it a directory.
    std::fs::create_dir(quarantine_path(&spool)).expect("occupy the quarantine path");

    let (ledger, repo) = ledger_and_repo();
    let report = fold(&spool, &ledger, &repo, None).expect("fold");
    assert_eq!(report.quarantined.len(), 1);
    let why = report
        .quarantine_write_error
        .as_deref()
        .expect("the quarantine write failure vanished");
    assert!(why.contains("quarantine"), "{why}");
}

#[test]
fn an_exec_spool_line_folds_to_an_exec_event_and_a_parsed_check() {
    // M0's done-when, minus the shim: a `spec_payload` line from the exec
    // recorder becomes an `exec` event and a §3.3 Check with real numbers.
    let dir = tempfile::tempdir().expect("tempdir");
    let store = dir.path().join("store");
    std::fs::create_dir_all(&store).expect("store");
    let out = store.join("out.txt");
    std::fs::write(
        &out,
        "..                                              [100%]\n\
         ================== 2 passed in 0.03s ==================\n",
    )
    .expect("write output");

    let s = session_of(14);
    let spool = dir.path().join("exec.jsonl");
    let line = serde_json::json!({
        "received_at": "2026-09-02T10:11:12.345Z",
        "source": "exec-recorder",
        "session_id": s,
        "raw_sha256": "abc",
        "spec_payload": {
            "kind": "exec",
            "payload": {
                "argv": ["pytest", "-q"],
                "argv_hash": "sha256:aa",
                "cwd": "/repo",
                "env_hash": "sha256:bb",
                "started": "2026-09-02T10:11:10.000Z",
                "ended": "2026-09-02T10:11:12.000Z",
                "exit_code": 0,
                "stdout_hash": "sha256:cc",
                "stderr_hash": "sha256:dd",
                "stdout_excerpt": "2 passed",
                "stderr_excerpt": "",
                "pid": 4242,
                "parent": 4241,
                "input_tree_hash": "0123456789abcdef0123456789abcdef01234567",
                "stdout_store": out.to_string_lossy(),
            }
        }
    });
    claude_code::append_line(&spool, &format!("{line}\n")).expect("append");

    let (ledger, repo) = ledger_and_repo();
    let report = fold(&spool, &ledger, &repo, None).expect("fold");
    assert_eq!(report.exec_events, 1);
    assert_eq!(report.checks_written, 1);
    assert!(report.quarantined.is_empty(), "{:?}", report.quarantined);

    let events = ledger.session_events(&s).expect("read");
    assert_eq!(events[0].kind().as_str(), "exec");
    assert_eq!(
        serde_json::to_value(events[0].source()).ok(),
        Some(serde_json::json!("exec-recorder")),
        "the exec kept the hook's source instead of its own"
    );
    ledger.verify_session(&s).expect("the chain verifies");

    let checks = ledger.checks_for_session(&s).expect("checks");
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].runner, "pytest");
    assert_eq!(checks[0].exit_code, 0);
    assert_eq!(
        checks[0].input_tree_hash,
        "0123456789abcdef0123456789abcdef01234567"
    );
    let parsed: serde_json::Value = serde_json::from_str(&checks[0].parsed).expect("parsed JSON");
    assert_eq!(parsed["passed"], serde_json::json!(2));
    assert_eq!(parsed["failed"], serde_json::json!(0));
}

#[test]
fn an_exec_whose_output_is_missing_is_a_check_that_is_unverifiable_not_clean() {
    // The failure this must never make: a stored `parsed` of `{}` reads as
    // `{total: 0, failed: 0}`, which is a clean run.
    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(15);
    let spool = dir.path().join("exec.jsonl");
    let line = serde_json::json!({
        "received_at": "2026-09-02T10:11:12.345Z",
        "source": "exec-recorder",
        "session_id": s,
        "spec_payload": {
            "kind": "exec",
            "payload": {
                "argv": ["pytest", "-q"], "argv_hash": "sha256:aa", "cwd": "/repo",
                "env_hash": "sha256:bb", "started": "2026-09-02T10:11:10.000Z",
                "ended": "2026-09-02T10:11:12.000Z", "exit_code": 0,
                "stdout_hash": "sha256:cc", "stderr_hash": "sha256:dd",
                "stdout_excerpt": "", "stderr_excerpt": "", "pid": 1, "parent": 0,
                "stdout_store": dir.path().join("gone.txt").to_string_lossy(),
            }
        }
    });
    claude_code::append_line(&spool, &format!("{line}\n")).expect("append");

    let (ledger, repo) = ledger_and_repo();
    let report = fold(&spool, &ledger, &repo, None).expect("fold");
    assert_eq!(
        report.checks_failed.len(),
        1,
        "a missing output was not counted"
    );
    assert!(report.checks_failed[0].1.contains("could not be read"));

    let checks = ledger.checks_for_session(&s).expect("checks");
    assert_eq!(
        checks.len(),
        1,
        "the command still ran and is still a check"
    );
    let parsed: serde_json::Value = serde_json::from_str(&checks[0].parsed).expect("JSON");
    assert!(
        parsed.get("unverifiable").is_some(),
        "an unreadable run was stored as {parsed}"
    );
    assert!(
        parsed.get("passed").is_none(),
        "counts were invented for a run with no output"
    );
    // And no snapshot means no binding: an empty tree hash can never match a
    // real one, so §6.1 cannot mistake this for a bound check.
    assert_eq!(checks[0].input_tree_hash, "");
}

#[test]
fn a_spec_payload_this_build_cannot_read_is_quarantined_not_reshaped() {
    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(16);
    let spool = dir.path().join("exec.jsonl");
    let line = serde_json::json!({
        "received_at": "2026-09-02T10:11:12.345Z",
        "source": "exec-recorder",
        "session_id": s,
        "spec_payload": { "kind": "exec", "payload": { "argv": ["pytest"] } },
    });
    claude_code::append_line(&spool, &format!("{line}\n")).expect("append");
    let (ledger, repo) = ledger_and_repo();
    let report = fold(&spool, &ledger, &repo, None).expect("fold");
    assert_eq!(report.events_written, 0);
    assert_eq!(report.quarantined.len(), 1);
    assert!(
        report.quarantined[0].1.contains("spec_payload"),
        "{:?}",
        report.quarantined
    );
}

#[test]
fn a_mutating_tool_that_names_a_path_yields_an_fs_change_before_its_snapshot() {
    // §6.1 needs the change SET, not just the tree hash, and C.7/S1 requires it
    // to come from worktree observation: a clean filter makes the worktree
    // change while the tree hash does not, so a diff-derived set is blind to
    // exactly the write it exists to detect.
    let repo_dir = tempfile::tempdir().expect("tempdir");
    for args in [
        vec!["init", "-q", "-b", "main"],
        vec!["config", "user.email", "t@e.invalid"],
        vec!["config", "user.name", "T"],
    ] {
        let out = std::process::Command::new("git")
            .current_dir(repo_dir.path())
            .args(&args)
            .output()
            .expect("git");
        assert!(out.status.success());
    }
    std::fs::write(repo_dir.path().join("a.txt"), "a\n").expect("write");

    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(17);
    let spool = spool_with(
        dir.path(),
        &[
            &delivery(
                "PostToolUse",
                &s,
                r#","tool_name":"Edit","tool_input":{"file_path":"src/app.py"},"tool_response":{"success":true}"#,
            ),
            &delivery(
                "PostToolUse",
                &s,
                r#","tool_name":"Read","tool_input":{"file_path":"src/app.py"},"tool_response":"x""#,
            ),
        ],
    );
    let (ledger, repo) = ledger_and_repo();
    let report = fold(&spool, &ledger, &repo, Some(repo_dir.path())).expect("fold");
    assert!(
        report.fs_changes_failed.is_empty(),
        "{:?}",
        report.fs_changes_failed
    );

    let events = ledger.session_events(&s).expect("read");
    let kinds: Vec<String> = events.iter().map(|e| e.kind().as_str()).collect();
    assert_eq!(
        kinds,
        vec!["tool_result", "fs_change", "snapshot", "tool_result"],
        "the fs_change must sit between the tool result and its snapshot"
    );
    let fs = events[1].payload();
    assert_eq!(fs["path"], serde_json::json!("src/app.py"));
    assert_eq!(fs["op"], serde_json::json!("modify"));
    // The inference is stamped, not presented as an observation.
    assert!(fs["op_inferred_from"]
        .as_str()
        .unwrap_or("")
        .contains("tool_name"));
    // Nothing is fabricated for the bytes nobody saw.
    assert!(fs.get("before_hash").is_none());
    ledger.verify_session(&s).expect("the chain verifies");
}

#[test]
fn a_bash_tool_invents_no_fs_change_because_it_names_no_path() {
    // A `Bash` command can change anything and says which nowhere. The exec
    // recorder's before/after snapshots cover it; guessing here would put a
    // path in the record that nothing observed.
    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(18);
    let spool = spool_with(
        dir.path(),
        &[&delivery(
            "PostToolUse",
            &s,
            r#","tool_name":"Bash","tool_input":{"command":"rm -rf build"},"tool_response":"ok""#,
        )],
    );
    let (ledger, repo) = ledger_and_repo();
    fold(&spool, &ledger, &repo, None).expect("fold");
    let kinds: Vec<String> = ledger
        .session_events(&s)
        .expect("read")
        .iter()
        .map(|e| e.kind().as_str())
        .collect();
    assert_eq!(kinds, vec!["tool_result"], "a path was invented for Bash");
}

#[test]
fn a_write_is_a_create_and_an_edit_is_a_modify() {
    let dir = tempfile::tempdir().expect("tempdir");
    let s = session_of(19);
    let spool = spool_with(
        dir.path(),
        &[
            &delivery(
                "PostToolUse",
                &s,
                r#","tool_name":"Write","tool_input":{"file_path":"new.py"},"tool_response":{"success":true}"#,
            ),
            &delivery(
                "PostToolUse",
                &s,
                r#","tool_name":"Edit","tool_input":{"file_path":"old.py"},"tool_response":{"success":true}"#,
            ),
        ],
    );
    let (ledger, repo) = ledger_and_repo();
    fold(&spool, &ledger, &repo, None).expect("fold");
    let ops: Vec<String> = ledger
        .session_events(&s)
        .expect("read")
        .iter()
        .filter(|e| e.kind().as_str() == "fs_change")
        .map(|e| e.payload()["op"].as_str().unwrap_or("?").to_string())
        .collect();
    assert_eq!(ops, vec!["create", "modify"]);
}
