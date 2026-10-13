//! Re-ingest is idempotent, appends extend the chain, and a rewritten line is
//! detected rather than silently absorbed.

use std::fs;
use std::path::{Path, PathBuf};

use thruline_core::ingest::{ingest, verify};
use thruline_core::ledger::Ledger;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A throwaway transcript root: <tmp>/-Users-dev-demo/<uuid>.jsonl
struct Scratch {
    _dir: tempfile::TempDir,
    root: PathBuf,
    session: PathBuf,
    db: PathBuf,
}

fn scratch_from(fixture_name: &str) -> Scratch {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    let project = root.join("-Users-dev-demo");
    fs::create_dir_all(&project).expect("mkdir");
    let session = project.join("aaaaaaaa-1111-4222-8333-444444444444.jsonl");
    fs::copy(fixture(fixture_name), &session).expect("copy fixture");
    let db = dir.path().join("ledger.db");
    Scratch {
        _dir: dir,
        root,
        session,
        db,
    }
}

fn ingest_once(s: &Scratch) -> thruline_core::ingest::IngestReport {
    let mut ledger = Ledger::open(&s.db).expect("open ledger");
    ingest(
        &mut ledger,
        std::slice::from_ref(&s.root),
        &s.db.parent().unwrap().join("mirrors"),
    )
    .expect("ingest")
}

#[test]
fn first_ingest_stores_every_event_and_verify_passes() {
    let s = scratch_from("claude_code_sessions.jsonl");
    let first = ingest_once(&s);

    assert_eq!(first.projects, vec!["-Users-dev-demo".to_string()]);
    assert_eq!(first.sessions, 1);
    assert_eq!(first.lines_read, 12);
    assert_eq!(first.new_events, 14, "12 lines fan out to 14 events");
    assert!(first.integrity_alarms.is_empty());

    let ledger = Ledger::open(&s.db).expect("reopen");
    let report = verify(&ledger, None).expect("verify");
    assert!(report.ok(), "{report:#?}");
    let v = &report.projects["-Users-dev-demo"];
    assert_eq!(v.events_checked, 14);
    assert_eq!(v.raw_matches, 14);
    assert_eq!(v.chain_matches, 14);
}

#[test]
fn re_ingest_adds_nothing_and_leaves_the_head_untouched() {
    let s = scratch_from("claude_code_sessions.jsonl");
    let first = ingest_once(&s);
    let second = ingest_once(&s);

    assert_eq!(second.new_events, 0, "no duplicate events");
    assert_eq!(
        second.lines_skipped_already_ingested, 12,
        "every line recognised as already ingested"
    );
    assert!(second.integrity_alarms.is_empty());
    assert_eq!(
        first.heads, second.heads,
        "an idempotent ingest must not move the head"
    );

    let ledger = Ledger::open(&s.db).expect("reopen");
    assert_eq!(ledger.event_count("-Users-dev-demo").expect("count"), 14);
}

#[test]
fn appending_to_a_transcript_ingests_only_the_new_lines() {
    let s = scratch_from("claude_code_sessions.jsonl");
    let first = ingest_once(&s);

    // Simulate the session continuing: two more real lines appended.
    let extra = concat!(
        r#"{"type":"user","uuid":"00000000-0000-4000-8000-00000000c001","parentUuid":null,"isSidechain":false,"sessionId":"00000000-0000-4000-8000-000000000000","timestamp":"2026-07-29T01:00:00.000Z","message":{"role":"user","content":"a later turn"}}"#,
        "\n",
        r#"{"type":"assistant","uuid":"00000000-0000-4000-8000-00000000c002","parentUuid":"00000000-0000-4000-8000-00000000c001","isSidechain":false,"sessionId":"00000000-0000-4000-8000-000000000000","timestamp":"2026-07-29T01:00:01.000Z","message":{"role":"assistant","content":[{"type":"text","text":"reply"},{"type":"tool_use","id":"toolu_00000000000000000099","name":"Bash","input":{"command":"ls"}}]}}"#,
        "\n"
    );
    let mut content = fs::read_to_string(&s.session).expect("read");
    content.push_str(extra);
    fs::write(&s.session, &content).expect("append");

    let second = ingest_once(&s);
    assert_eq!(second.new_events, 3, "1 user_msg + text + tool_use");
    assert_eq!(second.lines_skipped_already_ingested, 12);
    assert!(second.integrity_alarms.is_empty());
    assert_ne!(
        first.heads, second.heads,
        "the head must move when the ledger grows"
    );

    let ledger = Ledger::open(&s.db).expect("reopen");
    assert_eq!(ledger.event_count("-Users-dev-demo").expect("count"), 17);

    // The chain still verifies end to end after the extension.
    let report = verify(&ledger, None).expect("verify");
    assert!(report.ok(), "{report:#?}");
    assert_eq!(report.projects["-Users-dev-demo"].chain_matches, 17);
}

#[test]
fn rewriting_an_ingested_line_is_reported_and_never_absorbed() {
    let s = scratch_from("claude_code_sessions.jsonl");
    let first = ingest_once(&s);

    // Tamper: change one already-ingested line in place.
    let content = fs::read_to_string(&s.session).expect("read");
    let mut lines: Vec<String> = content.lines().map(str::to_owned).collect();
    lines[1] = lines[1].replace("\"role\":\"user\"", "\"role\":\"user\",\"tampered\":true");
    assert_ne!(lines[1], content.lines().nth(1).unwrap_or_default());
    fs::write(&s.session, lines.join("\n") + "\n").expect("write");

    // Ingest notices, refuses to re-hash, and reports.
    let second = ingest_once(&s);
    assert_eq!(
        second.integrity_alarms.len(),
        1,
        "{:?}",
        second.integrity_alarms
    );
    let alarm = &second.integrity_alarms[0];
    assert_eq!(alarm.line, 2);
    assert_ne!(alarm.stored_sha256, alarm.found_sha256);
    assert_eq!(second.new_events, 0, "nothing was appended for a rewrite");
    assert_eq!(
        first.heads, second.heads,
        "the ledger did not absorb the change"
    );

    // Verify independently catches it against the raw file.
    let ledger = Ledger::open(&s.db).expect("reopen");
    let report = verify(&ledger, None).expect("verify");
    assert!(!report.ok(), "verify must fail on a rewritten line");
    let v = &report.projects["-Users-dev-demo"];
    assert_eq!(v.raw_mismatches.len(), 1);
    assert_eq!(v.raw_mismatches[0].line, 2);
    assert!(
        v.chain_mismatches.is_empty(),
        "the stored chain itself is intact — only the source drifted"
    );
}

#[test]
fn verify_reports_a_missing_source_file() {
    // RB1.5 changed this contract DELIBERATELY: ingest seals before parse, so
    // a deleted source whose every line the sealed generation answers for is
    // the T7 sweep shape — reported as swept-but-preserved, and verify holds.
    // A missing file is a FAILURE only when the mirror cannot answer.
    let s = scratch_from("claude_code_sessions.jsonl");
    ingest_once(&s);
    fs::remove_file(&s.session).expect("delete the transcript");

    let ledger = Ledger::open(&s.db).expect("reopen");
    let report = verify(&ledger, None).expect("verify");
    let v = &report.projects["-Users-dev-demo"];
    assert_eq!(v.swept_files_preserved.len(), 1, "{v:?}");
    assert!(v.missing_files.is_empty());
    assert!(v.raw_matches_via_mirror > 0);
    assert!(report.ok(), "a fully preserved sweep is not a wound");
}

#[test]
fn malformed_lines_do_not_stop_ingest() {
    let s = scratch_from("malformed.jsonl");
    let r = ingest_once(&s);

    assert_eq!(r.malformed.len(), 3, "{:?}", r.malformed);
    assert_eq!(r.new_events, 3, "the intact lines still landed");
    assert!(r.unknown_types.contains_key("brand-new-unknown-kind"));

    let ledger = Ledger::open(&s.db).expect("reopen");
    let report = verify(&ledger, None).expect("verify");
    assert!(
        report.ok(),
        "a file with damage still verifies its stored events"
    );
}

#[test]
fn a_single_jsonl_path_is_accepted() {
    let s = scratch_from("claude_code_sessions.jsonl");
    let mut ledger = Ledger::open(&s.db).expect("open");
    let r = ingest(
        &mut ledger,
        std::slice::from_ref(&s.session),
        &s.db.parent().unwrap().join("mirrors"),
    )
    .expect("ingest one file");
    assert_eq!(r.sessions, 1);
    assert_eq!(r.new_events, 14);
}

#[test]
fn ingest_never_writes_inside_the_transcript_tree() {
    let s = scratch_from("claude_code_sessions.jsonl");
    let before = walk(&s.root);
    ingest_once(&s);
    let after = walk(&s.root);
    assert_eq!(before, after, "ingest must be read-only over transcripts");
}

fn walk(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk(&p));
            } else {
                let len = fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                out.push(format!("{}:{len}", p.display()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn a_resumed_session_does_not_collide_with_its_parent() {
    // Regression, found against real data: resuming a session writes a NEW
    // file whose opening lines are a verbatim copy of the parent transcript,
    // still stamped with the PARENT's `sessionId`. Binding a file to the first
    // `sessionId` it contains made the resumed file overwrite its parent's
    // session and produced bogus integrity alarms on a first ingest.
    let dir = tempfile::tempdir().expect("tempdir");
    let project = dir.path().join("projects").join("-Users-dev-resumed");
    fs::create_dir_all(&project).expect("mkdir");

    let parent_id = "aaaaaaaa-1111-4222-8333-444444444444";
    let child_id = "bbbbbbbb-1111-4222-8333-444444444444";
    let line = |sid: &str, uuid: &str, body: &str| {
        format!(
            r#"{{"type":"user","uuid":"{uuid}","parentUuid":null,"isSidechain":false,"sessionId":"{sid}","timestamp":"2026-07-29T00:00:00.000Z","message":{{"role":"user","content":"{body}"}}}}"#
        )
    };

    // Parent transcript: two turns.
    let shared = format!(
        "{}\n{}\n",
        line(
            parent_id,
            "00000000-0000-4000-8000-00000000d001",
            "turn one"
        ),
        line(
            parent_id,
            "00000000-0000-4000-8000-00000000d002",
            "turn two"
        ),
    );
    fs::write(project.join(format!("{parent_id}.jsonl")), &shared).expect("write parent");

    // Resumed transcript: the same two lines copied verbatim (still stamped
    // with the parent's sessionId), then its own turn.
    let mut resumed = shared.clone();
    resumed.push_str(&line(
        child_id,
        "00000000-0000-4000-8000-00000000d003",
        "turn three",
    ));
    resumed.push('\n');
    fs::write(project.join(format!("{child_id}.jsonl")), &resumed).expect("write child");

    let db = dir.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    let r = ingest(
        &mut ledger,
        &[dir.path().join("projects")],
        &dir.path().join("mirrors"),
    )
    .expect("ingest");

    assert!(
        r.integrity_alarms.is_empty(),
        "a first ingest can never raise an integrity alarm: {:?}",
        r.integrity_alarms
    );
    assert_eq!(r.sessions, 2);
    assert_eq!(r.new_events, 5, "2 parent lines + 3 resumed lines");

    let sessions = ledger.sessions_of("-Users-dev-resumed").expect("sessions");
    assert_eq!(sessions.len(), 2, "one session row per file");
    let ids: Vec<&str> = sessions.iter().map(|(id, _)| id.as_str()).collect();
    assert!(
        ids.contains(&parent_id) && ids.contains(&child_id),
        "{ids:?}"
    );

    let report = verify(&ledger, None).expect("verify");
    assert!(report.ok(), "{report:#?}");
}

#[test]
fn rewriting_a_malformed_line_is_now_caught_by_verify() {
    // M1 gap: malformed lines produce no events, so no per-event hash covered
    // them and an edit was invisible. The per-session raw_digest closes it.
    let s = scratch_from("malformed.jsonl");
    let first = ingest_once(&s);
    assert_eq!(first.malformed.len(), 3, "fixture still has damage");

    let ledger = Ledger::open(&s.db).expect("open");
    assert!(
        verify(&ledger, None).expect("verify").ok(),
        "baseline must pass before tampering"
    );
    drop(ledger);

    // Line 6 is the non-JSON line — it stores no event at all.
    let content = fs::read_to_string(&s.session).expect("read");
    let mut lines: Vec<String> = content.split('\n').map(str::to_owned).collect();
    assert_eq!(lines[5], "not json at all", "targeting the malformed line");
    lines[5] = "still not json, but different".to_string();
    fs::write(&s.session, lines.join("\n")).expect("write");

    let ledger = Ledger::open(&s.db).expect("reopen");
    let report = verify(&ledger, None).expect("verify");
    let v = &report.projects["-Users-dev-demo"];
    assert!(
        v.raw_mismatches.is_empty(),
        "no event covers this line — the per-event check cannot see it"
    );
    // RB1.5: the sealed generation catches the rewrite too — two named
    // signals on the digest channel, both pointing at the same tamper.
    assert!(
        v.digest_mismatches.iter().any(|m| m.contains("raw_digest")),
        "the whole-file digest must catch it: {:?}",
        v.digest_mismatches
    );
    assert!(
        v.digest_mismatches
            .iter()
            .any(|m| m.contains("accepted mirror")),
        "the seal must catch it as well: {:?}",
        v.digest_mismatches
    );
    assert!(!report.ok(), "verify must fail");
}

#[test]
fn deleting_a_trailing_line_changes_the_raw_line_count() {
    let s = scratch_from("claude_code_sessions.jsonl");
    ingest_once(&s);
    let content = fs::read_to_string(&s.session).expect("read");
    let kept: Vec<&str> = content.lines().take(11).collect();
    fs::write(&s.session, kept.join("\n") + "\n").expect("truncate");

    let ledger = Ledger::open(&s.db).expect("reopen");
    let report = verify(&ledger, None).expect("verify");
    let v = &report.projects["-Users-dev-demo"];
    assert!(!report.ok());
    // RB1.5: the sealed generation catches the truncation too, so the digest
    // channel now carries two named signals instead of one.
    assert!(
        v.digest_mismatches
            .iter()
            .any(|m| m.contains("raw_line_count")),
        "{:?}",
        v.digest_mismatches
    );
    assert!(
        v.digest_mismatches
            .iter()
            .any(|m| m.contains("accepted mirror")),
        "the seal must catch the truncation as well: {:?}",
        v.digest_mismatches
    );
}

#[test]
fn line_session_id_is_stored_and_differs_from_the_file_stem_on_a_resume() {
    let dir = tempfile::tempdir().expect("tempdir");
    let project = dir.path().join("projects").join("-Users-dev-lineage");
    fs::create_dir_all(&project).expect("mkdir");
    let parent_id = "aaaaaaaa-1111-4222-8333-444444444444";
    let child_id = "bbbbbbbb-1111-4222-8333-444444444444";
    let l = |sid: &str, uuid: &str| {
        format!(
            r#"{{"type":"user","uuid":"{uuid}","parentUuid":null,"isSidechain":false,"sessionId":"{sid}","timestamp":"2026-07-29T00:00:00.000Z","message":{{"role":"user","content":"x"}}}}"#
        )
    };
    fs::write(
        project.join(format!("{parent_id}.jsonl")),
        format!("{}\n", l(parent_id, "00000000-0000-4000-8000-00000000e001")),
    )
    .expect("parent");
    fs::write(
        project.join(format!("{child_id}.jsonl")),
        format!(
            "{}\n{}\n",
            l(parent_id, "00000000-0000-4000-8000-00000000e001"),
            l(child_id, "00000000-0000-4000-8000-00000000e002")
        ),
    )
    .expect("child");

    let db = dir.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(
        &mut ledger,
        &[dir.path().join("projects")],
        &dir.path().join("mirrors"),
    )
    .expect("ingest");

    let mut stmt = ledger
        .conn()
        .prepare("SELECT session_id, line_session_id FROM events ORDER BY session_id, seq")
        .expect("prepare");
    let rows: Vec<(String, Option<String>)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("query")
        .collect::<Result<Vec<_>, _>>()
        .expect("rows");

    // The child's copied first line keeps the PARENT's sessionId while its
    // session_id is the child's own file stem — that is the resume signature.
    let copied = rows
        .iter()
        .find(|(sid, lsid)| sid == child_id && lsid.as_deref() == Some(parent_id));
    assert!(
        copied.is_some(),
        "resume lineage must be queryable: {rows:?}"
    );
}
