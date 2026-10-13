//! Folding a rollout, against files written by a real Codex.

use super::*;
use thruline_ledger::Ledger;

fn scratch() -> (tempfile::TempDir, Ledger, RepoId, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).expect("repo");
    let ledger = Ledger::open(&dir.path().join("exec.db")).expect("ledger");
    let key = repo.to_string_lossy().to_string();
    let id = ledger.repo(&key, Some(&key)).expect("repo id");
    (dir, ledger, id, repo)
}

/// One rollout, written where the real layout puts it.
fn write_rollout(root: &Path, lines: &[String]) -> PathBuf {
    let dir = root.join("2026").join("04").join("20");
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("rollout-2026-04-20T19-46-44-abc.jsonl");
    std::fs::write(&path, lines.join("\n")).expect("write");
    path
}

fn meta(cwd: &str) -> String {
    format!(
        r#"{{"timestamp":"2026-04-20T19:46:44.000Z","type":"session_meta","payload":{{"id":"019dab40-abc","cwd":{cwd:?},"originator":"codex_cli_rs"}}}}"#
    )
}

const PATCH_OK: &str = r#"{"timestamp":"2026-04-20T14:22:48.317Z","type":"event_msg","payload":{"type":"patch_apply_end","call_id":"c1","stdout":"ok","stderr":"","success":true,"changes":{"/repo/a.ts":{"type":"update","unified_diff":"@@ -1 +1 @@\n-a\n+b\n","move_path":null},"/repo/new.ts":{"type":"add","content":"x\n"}},"status":"completed"}}"#;

const PATCH_FAILED: &str = r#"{"timestamp":"2026-07-11T07:37:18.851Z","type":"event_msg","payload":{"type":"patch_apply_end","call_id":"c2","stdout":"","stderr":"Failed to write file /repo/w.swift\n","success":false,"changes":{"/repo/w.swift":{"type":"update","unified_diff":"@@ -1 +1 @@\n-a\n+b\n"}}}}"#;

const BOUNDARY: &str = r#"{"timestamp":"2026-05-03T18:28:08.162Z","type":"event_msg","payload":{"type":"context_compacted"}}"#;

const COMPACTED: &str = r#"{"timestamp":"2026-05-03T18:28:08.160Z","type":"compacted","payload":{"message":"summary","replacement_history":[{"type":"message","role":"user"}]}}"#;

const AGENT_MSG: &str = r#"{"timestamp":"2026-04-20T14:30:00.000Z","type":"event_msg","payload":{"type":"agent_message","message":"All tests pass."}}"#;

const UNMAPPED: &str = r#"{"timestamp":"2026-04-20T14:31:00.000Z","type":"event_msg","payload":{"type":"token_count"}}"#;

fn full_rollout(cwd: &str) -> Vec<String> {
    vec![
        meta(cwd),
        PATCH_OK.into(),
        BOUNDARY.into(),
        COMPACTED.into(),
        PATCH_FAILED.into(),
        AGENT_MSG.into(),
        UNMAPPED.into(),
    ]
}

// ---------------------------------------------------------------------------
// The two record types this exists to recover
// ---------------------------------------------------------------------------

#[test]
fn a_rollout_folds_into_the_kinds_the_spec_names() {
    let (dir, ledger, repo_id, repo) = scratch();
    let root = dir.path().join("sessions");
    write_rollout(&root, &full_rollout(&repo.to_string_lossy()));

    let report = import(&root, &ledger, &repo_id, &repo, None).expect("import");
    assert_eq!(report.sessions, 1, "{report:?}");
    assert_eq!(report.rollouts_for_other_repos, 0);
    assert_eq!(report.fs_changes, 2, "both paths of the patch: {report:?}");
    assert_eq!(report.compaction_boundaries, 1);
    assert_eq!(report.patches_failed, 1);
    assert_eq!(
        report.unmapped.get("event_msg/token_count").copied(),
        Some(1),
        "an unmapped family must be named and counted, never dropped: {:?}",
        report.unmapped
    );

    let kinds: Vec<String> = ledger
        .session_events("019dab40-abc")
        .expect("events")
        .iter()
        .map(|e| e.kind().as_str().to_string())
        .collect();
    assert_eq!(
        kinds,
        vec![
            "session_start",
            "fs_change",
            "compaction_pre",
            "compaction_post",
            "note",
            "stop",
        ],
        "the token_count line contributed no event, which is why there are six"
    );
}

/// A `success: false` patch is a **note**, never an `fs_change`.
///
/// It still carries a full `changes` map — measured on real rollouts — so a
/// reader that mapped `changes` unconditionally would record file mutations
/// that did not happen. §0.2 exactly backwards.
#[test]
fn a_failed_patch_never_becomes_a_file_change() {
    let (dir, ledger, repo_id, repo) = scratch();
    let root = dir.path().join("sessions");
    write_rollout(&root, &[meta(&repo.to_string_lossy()), PATCH_FAILED.into()]);
    let report = import(&root, &ledger, &repo_id, &repo, None).expect("import");
    assert_eq!(report.fs_changes, 0);
    assert_eq!(report.patches_failed, 1);
    let events = ledger.session_events("019dab40-abc").expect("events");
    assert!(
        !events.iter().any(|e| e.kind().as_str() == "fs_change"),
        "a patch the record says failed produced an fs_change"
    );
    let note = events
        .iter()
        .find(|e| e.kind().as_str() == "note")
        .expect("the failure is recorded, not dropped");
    assert!(note.payload()["stderr"]
        .as_str()
        .is_some_and(|s| s.contains("Failed to write file")));
}

/// §13: a rollout is the harness's own log, so nothing imported from one can
/// ever support a claim.
#[test]
fn every_imported_session_is_capture_degraded() {
    let (dir, ledger, repo_id, repo) = scratch();
    let root = dir.path().join("sessions");
    write_rollout(&root, &full_rollout(&repo.to_string_lossy()));
    import(&root, &ledger, &repo_id, &repo, None).expect("import");
    assert!(
        ledger.capture_degraded("019dab40-abc").expect("read"),
        "an imported session must be degraded, or its claims could reach supported"
    );
    assert!(DEGRADED_REASON.contains("not execution evidence"));
}

#[test]
fn live_import_waits_for_complete_records_and_masks_before_storage() {
    let (dir, ledger, repo_id, repo) = scratch();
    let path = dir.path().join("live.jsonl");
    let header = format!("{}\n", meta(&repo.to_string_lossy()));
    let secret = "ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44eeVV55ff";
    let message = serde_json::json!({
        "timestamp":"2026-09-12T09:00:00Z",
        "type":"event_msg",
        "payload":{"type":"agent_message","message":format!("Observed {secret} café")}
    })
    .to_string();
    // A growing source can stop inside an encoded field. This fragment must
    // not be decoded, displayed, stored, or mistaken for a completed message.
    let cut = message.find(secret).expect("fixture secret") + 12;
    std::fs::write(&path, format!("{header}{}", &message[..cut])).expect("partial log");
    let (partial, source) =
        import_live_file(&path, &ledger, &repo_id, &repo).expect("partial import");
    assert_eq!(source["complete_bytes"], header.len());
    assert_eq!(
        source["sha256"],
        thruline_core::chain::sha256_hex(header.as_bytes())
    );
    assert_eq!(source["trailing_bytes_waiting"], cut);
    assert_eq!(partial.events_written, 1);
    let before = ledger.session_events("019dab40-abc").expect("events");
    assert_eq!(
        before.len(),
        1,
        "only the complete session header is admitted"
    );
    assert_eq!(before[0].kind().as_str(), "session_start");

    let complete = format!("{header}{message}\n");
    std::fs::write(&path, &complete).expect("complete log");
    let (finished, source) =
        import_live_file(&path, &ledger, &repo_id, &repo).expect("complete import");
    assert_eq!(source["complete_bytes"], complete.len());
    assert_eq!(source["trailing_bytes_waiting"], 0);
    assert_eq!(finished.events_written, 1);
    let after = ledger.session_events("019dab40-abc").expect("events");
    assert_eq!(after.len(), 2);
    let stored = after[1].payload().to_string();
    assert!(
        !stored.contains(secret),
        "completed input leaked its credential: {stored}"
    );
    assert!(
        stored.contains("[masked: github]"),
        "mask treatment missing: {stored}"
    );
    assert!(
        stored.contains("café"),
        "ordinary content changed: {stored}"
    );
    assert!(ledger.capture_degraded("019dab40-abc").expect("degraded"));
    import_live_file(&path, &ledger, &repo_id, &repo).expect("repeat import");
    assert_eq!(
        ledger.session_events("019dab40-abc").expect("events").len(),
        2
    );
}

// ---------------------------------------------------------------------------
// Ordering, idempotence, refusal
// ---------------------------------------------------------------------------

/// `seq` is the file's ordinal, so a re-import writes nothing.
#[test]
fn re_importing_the_same_rollout_writes_nothing() {
    let (dir, ledger, repo_id, repo) = scratch();
    let root = dir.path().join("sessions");
    write_rollout(&root, &full_rollout(&repo.to_string_lossy()));

    let first = import(&root, &ledger, &repo_id, &repo, None).expect("import");
    let head_before = ledger.last_event("019dab40-abc").expect("head");
    assert!(first.events_written > 0);

    let second = import(&root, &ledger, &repo_id, &repo, None).expect("re-import");
    assert_eq!(
        second.events_written, 0,
        "a re-import wrote {} event(s): the chain is not a function of the file",
        second.events_written
    );
    assert_eq!(
        ledger.last_event("019dab40-abc").expect("head"),
        head_before,
        "the chain head moved on a re-import"
    );
}

/// The chain a rollout produces is a function of the rollout, and it verifies.
#[test]
fn the_imported_chain_verifies_and_is_reproducible() {
    let a = scratch();
    let b = scratch();
    let mut heads = Vec::new();
    for (dir, ledger, repo_id, repo) in [a, b] {
        let root = dir.path().join("sessions");
        // Same session id and same lines, different repositories on disk: the
        // chain must depend on the FILE, not on where it was imported.
        write_rollout(&root, &full_rollout(&repo.to_string_lossy()));
        import(&root, &ledger, &repo_id, &repo, None).expect("import");
        ledger
            .verify_session("019dab40-abc")
            .expect("the chain verifies");
        heads.push(
            ledger
                .last_event("019dab40-abc")
                .expect("head")
                .map(|(_, h)| h),
        );
    }
    // The `session_start` payload carries the cwd, which genuinely differs
    // between the two scratch repositories — so the heads differ, and that is
    // the chain depending on recorded content rather than on nothing.
    assert!(heads[0].is_some() && heads[1].is_some());
    assert_ne!(
        heads[0], heads[1],
        "two rollouts recording different cwds produced the same chain head"
    );
}

/// A rollout for another repository is skipped and **counted**.
#[test]
fn another_repositorys_rollout_is_counted_rather_than_silently_skipped() {
    let (dir, ledger, repo_id, repo) = scratch();
    let root = dir.path().join("sessions");
    write_rollout(&root, &full_rollout("/somewhere/else"));
    let report = import(&root, &ledger, &repo_id, &repo, None).expect("import");
    assert_eq!(report.sessions, 0);
    assert_eq!(
        report.rollouts_for_other_repos, 1,
        "\"no sessions found\" and \"none of them were yours\" are different \
         answers: {report:?}"
    );
    assert_eq!(report.events_written, 0);
}

/// Containment is on path components, not on a string prefix.
#[test]
fn a_sibling_directory_with_a_shared_prefix_is_not_inside_the_repository() {
    let (dir, ledger, repo_id, repo) = scratch();
    let root = dir.path().join("sessions");
    // `/…/repository` shares a string prefix with `/…/repo` and is not in it.
    let sibling = format!("{}sitory", repo.to_string_lossy());
    std::fs::create_dir_all(&sibling).expect("sibling");
    write_rollout(&root, &full_rollout(&sibling));
    let report = import(&root, &ledger, &repo_id, &repo, None).expect("import");
    assert_eq!(
        report.sessions, 0,
        "a string-prefix comparison would have imported this"
    );
    assert_eq!(report.rollouts_for_other_repos, 1);
}

/// A line with no timestamp is refused, not stamped with the import's clock.
#[test]
fn a_line_without_a_timestamp_is_refused_and_named() {
    let (dir, ledger, repo_id, repo) = scratch();
    let root = dir.path().join("sessions");
    write_rollout(
        &root,
        &[
            meta(&repo.to_string_lossy()),
            r#"{"type":"event_msg","payload":{"type":"context_compacted"}}"#.into(),
            AGENT_MSG.into(),
        ],
    );
    let report = import(&root, &ledger, &repo_id, &repo, None).expect("import");
    assert_eq!(report.refused.len(), 1, "{report:?}");
    assert!(
        report.refused[0].1.contains("no timestamp"),
        "{:?}",
        report.refused
    );
    // The lines around it still landed: one bad line is not a lost session.
    assert_eq!(report.compaction_boundaries, 0);
    assert!(report.events_written >= 2);
}

/// A rollout with no `session_meta` names no session, and is refused rather
/// than attributed to a guess.
#[test]
fn a_rollout_without_session_meta_is_refused() {
    let (dir, ledger, repo_id, repo) = scratch();
    let root = dir.path().join("sessions");
    write_rollout(&root, &[PATCH_OK.into(), AGENT_MSG.into()]);
    let report = import(&root, &ledger, &repo_id, &repo, None).expect("import");
    assert_eq!(report.sessions, 0);
    assert_eq!(report.events_written, 0);
    assert!(
        report
            .refused
            .iter()
            .any(|(_, why)| why.contains("no session_meta")),
        "{report:?}"
    );
}

/// `--limit` bounds the work, so a first run on years of history is a decision.
#[test]
fn a_limit_bounds_how_many_rollouts_are_imported() {
    let (dir, ledger, repo_id, repo) = scratch();
    let root = dir.path().join("sessions");
    for day in ["20", "21", "22"] {
        let d = root.join("2026").join("04").join(day);
        std::fs::create_dir_all(&d).expect("mkdir");
        let mut lines = full_rollout(&repo.to_string_lossy());
        lines[0] = lines[0].replace("019dab40-abc", &format!("session-{day}"));
        std::fs::write(
            d.join(format!("rollout-2026-04-{day}T00-00-00-x.jsonl")),
            lines.join("\n"),
        )
        .expect("write");
    }
    let report = import(&root, &ledger, &repo_id, &repo, Some(2)).expect("import");
    assert_eq!(report.sessions, 2, "{report:?}");
}

/// Secrets in a rollout are masked before anything reaches the store.
///
/// Unlike the spool, a rollout arrives **unredacted** — it is the vendor's own
/// file, written without ThruLine involved — so this mask is the first these
/// bytes have seen and it is load-bearing rather than idempotent.
#[test]
fn rollout_text_is_masked_on_the_way_in() {
    let (dir, ledger, repo_id, repo) = scratch();
    let root = dir.path().join("sessions");
    let secret = "ghp_0123456789abcdefghijklmnopqrstuvwxyzAB";
    let msg = format!(
        r#"{{"timestamp":"2026-04-20T14:30:00.000Z","type":"event_msg","payload":{{"type":"agent_message","message":"pushed with {secret}"}}}}"#
    );
    write_rollout(&root, &[meta(&repo.to_string_lossy()), msg]);
    import(&root, &ledger, &repo_id, &repo, None).expect("import");
    let events = ledger.session_events("019dab40-abc").expect("events");
    let rendered = serde_json::to_string(
        &events
            .iter()
            .map(|e| e.payload().clone())
            .collect::<Vec<_>>(),
    )
    .expect("json");
    assert!(
        !rendered.contains(secret),
        "an unredacted rollout put a token into the store"
    );
}

/// Every kind this module claims to produce is one the event model accepts.
#[test]
fn the_declared_kinds_are_the_ones_it_produces() {
    let (dir, ledger, repo_id, repo) = scratch();
    let root = dir.path().join("sessions");
    write_rollout(&root, &full_rollout(&repo.to_string_lossy()));
    import(&root, &ledger, &repo_id, &repo, None).expect("import");
    let produced: std::collections::BTreeSet<String> = ledger
        .session_events("019dab40-abc")
        .expect("events")
        .iter()
        .map(|e| e.kind().as_str().to_string())
        .collect();
    let declared: std::collections::BTreeSet<String> = PRODUCED_KINDS
        .iter()
        .map(|k| {
            serde_json::to_value(k)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default()
        })
        .collect();
    assert!(
        produced.is_subset(&declared),
        "produced {produced:?} but declares {declared:?}"
    );
    // `user_prompt` is declared and this fixture has no user message, so the
    // declaration is a superset by design — asserted so the gap is deliberate.
    assert!(declared.len() > produced.len());
}

fn completed(item: Value) -> String {
    serde_json::json!({"timestamp":"2026-09-09T14:46:56.420Z","type":"event_msg","payload":{"type":"item_completed","item":item}}).to_string()
}

#[test]
fn current_completed_items_preserve_reports_without_execution_authority() {
    let (dir, ledger, repo_id, repo) = scratch();
    let root = dir.path().join("sessions");
    let rows = vec![
        meta(&repo.to_string_lossy()),
        completed(serde_json::json!({"type":"UserMessage","content":[{"type":"text","text":"Build the application"}]})),
        completed(serde_json::json!({"type":"AgentMessage","phase":"commentary","content":[{"type":"Text","text":"I will compile next."}]})),
        completed(serde_json::json!({"type":"AgentMessage","content":[{"type":"Text","text":"No phase is not a final answer."}]})),
        completed(serde_json::json!({"type":"CommandExecution","id":"command-1","command":["cargo","test"],"cwd":"file:///repo","status":"completed","exit_code":0,"aggregated_output":"reported tests passed"})),
        completed(serde_json::json!({"type":"FileChange","id":"patch-1","status":"completed","changes":{"src/lib.rs":{"type":"add","content":"source"}}})),
        completed(serde_json::json!({"type":"FileChange","id":"patch-2","status":"failed","stderr":"permission denied","changes":{"src/absent.rs":{"type":"add","content":"not applied"}}})),
        completed(serde_json::json!({"type":"AgentMessage","phase":"final_answer","content":[{"type":"Text","text":"Implementation complete. Tests were not independently observed."}]})),
        serde_json::json!({"timestamp":"2026-09-09T14:46:56.420Z","type":"response_item","payload":{"type":"message","role":"assistant","phase":"final_answer","content":[{"type":"output_text","text":"Implementation complete."}]}}).to_string(),
        completed(serde_json::json!({"type":"NewUnrecognizedItem","status":"success"})),
    ];
    write_rollout(&root, &rows);
    let report = import(&root, &ledger, &repo_id, &repo, None).expect("import");
    assert_eq!(report.events_written, 8);
    assert_eq!(report.fs_changes, 1);
    assert_eq!(report.patches_failed, 1);
    assert_eq!(
        report
            .unmapped
            .get("event_msg/item_completed/NewUnrecognizedItem"),
        Some(&1)
    );
    let events = ledger.session_events("019dab40-abc").expect("events");
    let kinds: Vec<_> = events
        .iter()
        .map(|e| e.kind().as_str().to_string())
        .collect();
    assert_eq!(
        kinds,
        [
            "session_start",
            "user_prompt",
            "note",
            "note",
            "note",
            "fs_change",
            "note",
            "stop"
        ]
    );
    let text = serde_json::to_string(&events).expect("event JSON");
    assert!(text.contains("imported_command_report"));
    assert!(text.contains("not independently captured execution"));
    ledger.verify_session("019dab40-abc").expect("chain");
    assert_eq!(
        import(&root, &ledger, &repo_id, &repo, None)
            .expect("repeat")
            .events_written,
        0
    );
}

#[test]
fn importing_only_session_start_then_appending_preserves_genesis_and_history() {
    let (dir, ledger, repo_id, repo) = scratch();
    let root = dir.path().join("sessions");
    let first = vec![meta(&repo.to_string_lossy())];
    write_rollout(&root, &first);
    assert_eq!(
        import(&root, &ledger, &repo_id, &repo, None)
            .expect("first")
            .events_written,
        1
    );
    let head = ledger.last_event("019dab40-abc").expect("head");
    assert_eq!(
        import(&root, &ledger, &repo_id, &repo, None)
            .expect("repeat")
            .events_written,
        0
    );
    assert_eq!(ledger.last_event("019dab40-abc").expect("head"), head);
    let mut longer = first;
    longer.push(completed(serde_json::json!({"type":"AgentMessage","phase":"commentary","content":[{"type":"Text","text":"New activity"}]})));
    write_rollout(&root, &longer);
    assert_eq!(
        import(&root, &ledger, &repo_id, &repo, None)
            .expect("append")
            .events_written,
        1
    );
    ledger
        .verify_session("019dab40-abc")
        .expect("appended chain");
    assert_eq!(
        ledger.session_events("019dab40-abc").expect("events").len(),
        2
    );
}

#[test]
fn live_prefix_waits_for_complete_lines_and_resumes_without_duplicates() {
    let (dir, ledger, repo_id, repo) = scratch();
    let file = dir.path().join("live.jsonl");
    let header = format!("{}\n", meta(&repo.to_string_lossy()));
    std::fs::write(&file, format!("{}{}", header, &AGENT_MSG[..20])).unwrap();
    let (first, identity) = import_live_file(&file, &ledger, &repo_id, &repo).unwrap();
    assert_eq!(first.events_written, 1);
    assert_eq!(identity["trailing_bytes_waiting"], 20);
    std::fs::write(&file, format!("{}{}\n", header, AGENT_MSG)).unwrap();
    let (second, _) = import_live_file(&file, &ledger, &repo_id, &repo).unwrap();
    assert_eq!(second.events_written, 1);
    drop(ledger);
    let ledger = Ledger::open(&dir.path().join("exec.db")).unwrap();
    assert_eq!(
        import_live_file(&file, &ledger, &repo_id, &repo)
            .unwrap()
            .0
            .events_written,
        0
    );
    assert_eq!(ledger.session_events("019dab40-abc").unwrap().len(), 2);
}
#[test]
fn live_log_limit_refuses_instead_of_silently_truncating() {
    let (dir, ledger, id, repo) = scratch();
    let file = dir.path().join("large.jsonl");
    std::fs::File::create(&file)
        .unwrap()
        .set_len(16 * 1024 * 1024 + 1)
        .unwrap();
    assert!(import_live_file(&file, &ledger, &id, &repo)
        .unwrap_err()
        .contains("16 MiB"));
}
