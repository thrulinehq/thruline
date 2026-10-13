//! HQ: prompts, edits, commands and claims connected into threads — against
//! small Claude Code and Codex fixtures ingested through the real ingest path
//! (`tests/fixtures/hq/`).

mod scratch;

use std::path::{Path, PathBuf};

use scratch::TempTree;
use serde_json::{json, Value};
use thruline_core::hq;
use thruline_core::ingest::{ingest, ingest_codex, CodexIngestOptions};
use thruline_core::Ledger;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hq")
}

struct Store {
    _tmp: TempTree,
    db: PathBuf,
}

fn store() -> Store {
    let tmp = TempTree::new();
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(
        &mut ledger,
        &[fixtures().join("claude/-work-app")],
        &tmp.path().join("mirror"),
    )
    .expect("claude ingest");
    let codex: Vec<PathBuf> = std::fs::read_dir(fixtures().join("codex"))
        .expect("codex fixtures")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    let report = ingest_codex(
        &mut ledger,
        &codex,
        &CodexIngestOptions {
            mirror_root: tmp.path().join("mirror"),
            owner_key: Some("hq-test-owner".to_string()),
        },
    )
    .expect("codex ingest");
    assert_eq!(report.rejected_candidates, 0, "{report:?}");
    Store { _tmp: tmp, db }
}

fn now() -> i64 {
    hq::parse_ms("2026-10-07T00:00:00Z").expect("instant")
}

#[test]
fn waiting_event_id_is_the_unanswered_question_not_a_stale_thread_timestamp() {
    fn view(answered: bool) -> hq::ProjectView {
        let tmp = TempTree::new();
        let db = tmp.path().join("ledger.db");
        let source = tmp.path().join("claude/-work-notification");
        std::fs::create_dir_all(&source).unwrap();
        let session = "cccccccc-1111-4111-8111-111111111111";
        let row = |uuid: &str, timestamp: &str, kind: &str, content: Value| {
            json!({
                "parentUuid": null, "isSidechain": false, "uuid": uuid,
                "timestamp": timestamp, "cwd": "/work/notification", "sessionId": session,
                "version": "2.1.0", "gitBranch": "main", "userType": "external", "type": kind,
                "message": {"role": if kind == "user" { "user" } else { "assistant" }, "model": "m", "content": content}
            })
        };
        let mut lines = vec![
            row(
                "cccccccc-0000-4000-8000-000000000001",
                "2026-10-01T09:00:00Z",
                "user",
                json!("Fix the parser."),
            ),
            row(
                "cccccccc-0000-4000-8000-000000000004",
                "2026-10-01T09:00:30Z",
                "assistant",
                json!([{"type":"tool_use","id":"tu-work","name":"Bash","input":{"command":"printf start","description":"begin work"}}]),
            ),
            row(
                "cccccccc-0000-4000-8000-000000000002",
                "2026-10-01T09:01:00Z",
                "assistant",
                json!([{"type":"text","text":"What format should it use?"}]),
            ),
        ];
        if answered {
            lines.push(row(
                "cccccccc-0000-4000-8000-000000000003",
                "2026-10-01T09:02:00Z",
                "user",
                json!("Use JSON."),
            ));
        }
        std::fs::write(
            source.join(format!("{session}.jsonl")),
            lines
                .iter()
                .map(|line| format!("{line}\n"))
                .collect::<String>(),
        )
        .unwrap();
        let mut ledger = Ledger::open(&db).unwrap();
        ingest(&mut ledger, &[source], &tmp.path().join("mirror")).unwrap();
        hq::project(
            &ledger,
            &hq::project_id_for_root("/work/notification"),
            hq::parse_ms("2026-10-11T00:00:00Z").unwrap(),
        )
        .unwrap()
        .unwrap()
    }
    let open = view(false);
    let thread = open.threads.first().expect("one recorded thread");
    assert_eq!(thread.whose_move_basis, "agent_asked");
    assert!(
        thread.waiting_event_id.is_some(),
        "an unanswered question has its own record ID"
    );
    let answered = view(true);
    let thread = answered.threads.first().expect("one recorded thread");
    assert!(
        thread.waiting_event_id.is_none(),
        "a later user answer closes the question even if the older move basis remains"
    );
}

#[test]
fn an_unanswered_command_exposes_its_masked_text_and_numbered_step() {
    let tmp = TempTree::new();
    let db = tmp.path().join("ledger.db");
    let source = tmp.path().join("claude/-work-inbox-command");
    std::fs::create_dir_all(&source).unwrap();
    let session = "cccccccc-1111-4111-8111-111111111111";
    let row = |n: u32, kind: &str, content: Value| {
        json!({
            "parentUuid": null, "isSidechain": false,
            "uuid": format!("cccccccc-0000-4000-8000-{n:012}"),
            "timestamp": format!("2026-10-01T09:0{n}:00Z"),
            "cwd": "/work/inbox-command", "sessionId": session,
            "type": kind,
            "message": { "role": if kind == "user" { "user" } else { "assistant" }, "content": content }
        })
    };
    let lines = [
        row(1, "user", json!("Clear the test cache.")),
        row(
            2,
            "assistant",
            json!([{"type":"text","text":"May I clear it first?"}]),
        ),
        row(
            3,
            "assistant",
            json!([{"type":"tool_use","id":"tu-1","name":"Bash","input":{"command":"rm -rf .pytest_cache && pytest -q"}}]),
        ),
    ];
    std::fs::write(
        source.join(format!("{session}.jsonl")),
        lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let mut ledger = Ledger::open(&db).unwrap();
    ingest(&mut ledger, &[source], &tmp.path().join("mirror")).unwrap();
    let view = hq::project(
        &ledger,
        &hq::project_id_for_root("/work/inbox-command"),
        hq::parse_ms("2026-10-11T00:00:00Z").unwrap(),
    )
    .unwrap()
    .unwrap();
    let thread = &view.threads[0];
    assert_eq!(thread.whose_move_basis, "agent_asked");
    let waiting = thread.waiting_command.as_ref().expect("unanswered command");
    assert_eq!(waiting.command, "rm -rf .pytest_cache && pytest -q");
    assert!(
        waiting.step > 0,
        "a positive step is assigned from the thread's record"
    );
}

#[test]
fn editing_after_a_backed_completion_claim_reopens_the_thread_for_recheck() {
    let tmp = TempTree::new();
    let db = tmp.path().join("ledger.db");
    let source = tmp.path().join("claude/-work-inbox-recheck");
    std::fs::create_dir_all(&source).unwrap();
    let session = "cccccccc-1111-4111-8111-111111111111";
    let row = |n: u32, kind: &str, content: Value| {
        json!({
            "parentUuid": null, "isSidechain": false,
            "uuid": format!("cccccccc-0000-4000-8000-{n:012}"),
            "timestamp": format!("2026-10-01T09:0{n}:00Z"),
            "cwd": "/work/inbox-recheck", "sessionId": session,
            "type": kind,
            "message": { "role": if kind == "user" { "user" } else { "assistant" }, "content": content }
        })
    };
    let lines = [
        row(1, "user", json!("Fix the parser.")),
        row(
            2,
            "assistant",
            json!([{"type":"tool_use","id":"tu-pass","name":"Bash","input":{"command":"pytest -q"}}]),
        ),
        row(
            3,
            "user",
            json!([{"type":"tool_result","tool_use_id":"tu-pass","content":"1 passed","is_error":false}]),
        ),
        row(
            4,
            "assistant",
            json!([{"type":"text","text":"The parser is fixed and tests pass."}]),
        ),
        row(
            5,
            "assistant",
            json!([{"type":"tool_use","id":"tu-edit","name":"Edit","input":{"file_path":"/work/inbox-recheck/src/parser.py","old_string":"a","new_string":"b"}}]),
        ),
        row(
            6,
            "user",
            json!([{"type":"tool_result","tool_use_id":"tu-edit","content":"ok","is_error":false}]),
        ),
    ];
    std::fs::write(
        source.join(format!("{session}.jsonl")),
        lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let mut ledger = Ledger::open(&db).unwrap();
    ingest(&mut ledger, &[source], &tmp.path().join("mirror")).unwrap();
    let view = hq::project(
        &ledger,
        &hq::project_id_for_root("/work/inbox-recheck"),
        hq::parse_ms("2026-10-11T00:00:00Z").unwrap(),
    )
    .unwrap()
    .unwrap();
    let thread = &view.threads[0];
    assert_eq!(thread.proof.edited_after_last_pass, vec!["src/parser.py"]);
    assert_eq!(thread.whose_move, hq::WhoseMove::You);
    assert_eq!(thread.whose_move_basis, "claim_without_passing_test");
    let claim = thread
        .latest_claim
        .as_ref()
        .expect("latest claim in the summary");
    assert!(claim.text.contains("parser is fixed"));
    assert_eq!(claim.agent, "Claude Code");
    assert!(claim.at.is_some());
    assert!(claim.step > 0);
    assert!(
        !thread.version_thread.is_empty(),
        "the version drawing uses recorded steps"
    );
    assert!(thread
        .version_thread
        .iter()
        .flat_map(|v| &v.events)
        .any(|e| e.kind == "claim" && e.step == claim.step));
}

#[test]
fn a_recorded_file_removal_is_counted_without_judging_the_finished_thread() {
    let tmp = TempTree::new();
    let db = tmp.path().join("ledger.db");
    let source = tmp.path().join("claude/-work-inbox-removal");
    std::fs::create_dir_all(&source).unwrap();
    let session = "cccccccc-1111-4111-8111-111111111111";
    let row = |n: u32, kind: &str, content: Value| {
        json!({
            "parentUuid": null, "isSidechain": false,
            "uuid": format!("cccccccc-0000-4000-8000-{n:012}"),
            "timestamp": format!("2026-10-01T09:0{n}:00Z"),
            "cwd": "/work/inbox-removal", "sessionId": session,
            "type": kind,
            "message": { "role": if kind == "user" { "user" } else { "assistant" }, "content": content }
        })
    };
    let lines = [
        row(1, "user", json!("Add a lexer test.")),
        row(
            2,
            "assistant",
            json!([{"type":"tool_use","id":"tu-remove","name":"Bash","input":{"command":"rm -rf .venv"}}]),
        ),
        row(
            3,
            "user",
            json!([{"type":"tool_result","tool_use_id":"tu-remove","content":"","is_error":false}]),
        ),
        row(
            4,
            "assistant",
            json!([{"type":"tool_use","id":"tu-pass","name":"Bash","input":{"command":"cargo test"}}]),
        ),
        row(
            5,
            "user",
            json!([{"type":"tool_result","tool_use_id":"tu-pass","content":"1 passed","is_error":false}]),
        ),
        row(
            6,
            "assistant",
            json!([{"type":"text","text":"Done. The lexer test passes."}]),
        ),
    ];
    std::fs::write(
        source.join(format!("{session}.jsonl")),
        lines
            .iter()
            .map(|line| format!("{line}\n"))
            .collect::<String>(),
    )
    .unwrap();
    let mut ledger = Ledger::open(&db).unwrap();
    ingest(&mut ledger, &[source], &tmp.path().join("mirror")).unwrap();
    let view = hq::project(
        &ledger,
        &hq::project_id_for_root("/work/inbox-removal"),
        hq::parse_ms("2026-10-11T00:00:00Z").unwrap(),
    )
    .unwrap()
    .unwrap();
    let thread = &view.threads[0];
    assert_eq!(thread.whose_move, hq::WhoseMove::Done);
    assert_eq!(thread.file_removals.len(), 1);
    assert_eq!(thread.file_removals[0].command, "rm -rf .venv");
    assert!(thread.file_removals[0].at.is_some());
}

fn the_project(ledger: &Ledger) -> hq::HqProject {
    let mut all = hq::hq_projects(ledger).expect("projects");
    assert_eq!(
        all.len(),
        1,
        "both vendors' recordings of /work/app are one project"
    );
    all.remove(0)
}

#[test]
fn both_vendors_recordings_of_one_root_are_one_project() {
    let s = store();
    let ledger = Ledger::open(&s.db).expect("open");
    let p = the_project(&ledger);
    assert_eq!(p.root, "/work/app");
    assert_eq!(p.id, hq::project_id_for_root("/work/app"));
    let sources: Vec<&str> = p.members.iter().map(|m| m.source.as_str()).collect();
    assert!(sources.contains(&"claude_code") && sources.contains(&"codex"));
}

/// The thread list, reduced to what the grouping rules decide.
fn shape(view: &hq::ProjectView) -> Value {
    json!({
        "project": {
            "sessions": view.project.sessions,
            "prompts": view.project.prompts,
            "conversation_prompts": view.project.conversation_prompts,
            "edits": view.project.edits,
            "commands": view.project.commands,
            "tests": view.project.tests,
            "claims": view.project.claims,
            "threads": view.project.threads,
            "whose_move": view.project.whose_move,
        },
        "threads": view.threads.iter().map(|t| json!({
            "title": t.title,
            "title_basis": t.title_basis,
            "sessions": t.sessions,
            "prompts": t.prompts,
            "conversation": t.conversation,
            "edits": t.edits,
            "commands": t.commands,
            "tests": t.tests,
            "commits": t.commits,
            "claims": t.claims,
            "whose_move": t.whose_move,
            "whose_move_basis": t.whose_move_basis,
            "last_test": t.proof.last_test.as_ref().map(|l| json!({"outcome": l.outcome, "command": l.command})),
            "edited_after_last_pass": t.proof.edited_after_last_pass,
            "top_files": t.top_files.iter().map(|f| f.path.clone()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

#[test]
fn prompts_connect_into_threads() {
    let s = store();
    let ledger = Ledger::open(&s.db).expect("open");
    let view = hq::project(&ledger, &hq::project_id_for_root("/work/app"), now())
        .expect("read")
        .expect("present");
    insta::assert_json_snapshot!("hq_threads", shape(&view));

    let titles: Vec<&str> = view.threads.iter().map(|t| t.title.as_str()).collect();
    // Rule 5: the next day's unicode work on the same file joined the login
    // thread, so there are three threads, not four.
    assert_eq!(view.threads.len(), 3, "{titles:?}");
    let login = view
        .threads
        .iter()
        .find(|t| t.title.starts_with("Fix the login crash"))
        .expect("login thread, titled by its instruction sentence, not 'Hi.'");
    assert_eq!(login.sessions, 2);
    assert_eq!(login.prompts, 2);
    // "Why was it panicking?" did no work: conversation, not a thread.
    assert_eq!(login.conversation, 1);
    assert_eq!(login.whose_move, hq::WhoseMove::You);
    // Rule 3: the README is disjoint from the committed login work.
    assert!(titles.contains(&"Now add a setup section to the README."));
}

#[test]
fn injected_prompts_never_start_or_title_a_thread() {
    let s = store();
    let ledger = Ledger::open(&s.db).expect("open");
    let view = hq::project(&ledger, &hq::project_id_for_root("/work/app"), now())
        .expect("read")
        .expect("present");
    for t in &view.threads {
        assert!(!t.title.contains("task-notification"), "{}", t.title);
        assert!(!t.title.contains("environment_context"), "{}", t.title);
        assert!(!t.title.contains("/model"), "{}", t.title);
    }
    // 4 Claude prompts + 1 Codex prompt are a person's; the notification,
    // the bare /model wrapper and Codex's environment block are not.
    assert_eq!(view.project.prompts + view.project.conversation_prompts, 5);
}

fn thread_by_title(ledger: &Ledger, prefix: &str) -> hq::ThreadView {
    let id = hq::project_id_for_root("/work/app");
    let view = hq::project(ledger, &id, now())
        .expect("read")
        .expect("present");
    let t = view
        .threads
        .iter()
        .find(|t| t.title.starts_with(prefix))
        .expect("thread");
    hq::thread(ledger, &id, &t.id, now())
        .expect("read")
        .expect("present")
}

#[test]
fn a_thread_holds_every_line_command_claim_and_version() {
    let s = store();
    let ledger = Ledger::open(&s.db).expect("open");
    let v = thread_by_title(&ledger, "Fix the login crash");

    // Prompts: whole text, in order, work and conversation marked.
    let prompts: Vec<(&str, bool)> = v
        .prompts
        .iter()
        .map(|p| (p.text.as_str(), p.work))
        .collect();
    assert_eq!(
        prompts,
        vec![
            (
                "Hi. Fix the login crash in `src/auth.rs`, it panics on empty input. Thanks!",
                true
            ),
            ("Why was it panicking?", false),
            ("Make the login check handle unicode names too.", true),
        ]
    );

    // Edit 1 came from the tool's own structured patch: line numbers and context.
    let first = &v.edits[0];
    assert_eq!(first.file, "src/auth.rs");
    assert_eq!(first.source, "structured_patch");
    assert_eq!((first.lines_added, first.lines_removed), (3, 1));
    let h = &first.hunks[0];
    assert_eq!(
        (h.old_start, h.old_lines, h.new_start, h.new_lines),
        (Some(11), 3, Some(11), 5)
    );
    let kinds: Vec<&str> = h.lines.iter().map(|l| l.kind).collect();
    assert_eq!(
        kinds,
        vec!["context", "removed", "added", "added", "added", "context"]
    );
    assert_eq!(first.ok, Some(true));
    // Edit 2 has no structured patch in its result: the request, said so.
    assert_eq!(v.edits[1].source, "request");
    assert_eq!(v.edits[1].hunks[0].old_start, None);

    // Commands: the pass, the commit, the failure — with tallies.
    let tests: Vec<_> = v.commands.iter().filter(|c| c.is_test).collect();
    assert_eq!(tests.len(), 2);
    let pass = tests[0].tally.as_ref().expect("cargo tally");
    assert_eq!((pass.passed, pass.failed, pass.skipped), (4, 0, 1));
    assert_eq!(tests[1].outcome, hq::Outcome::Failed);
    assert_eq!(tests[1].exit_code, Some(101));
    let fail = tests[1].tally.as_ref().expect("cargo tally");
    assert_eq!((fail.passed, fail.failed), (3, 1));
    assert!(tests[1]
        .tally_line
        .as_deref()
        .is_some_and(|l| l.starts_with("test result: FAILED.")));

    // Versions: the commit closes abc1234; the README thread's and the next
    // day's work sit in their sessions' own versions.
    let ids: Vec<&str> = v.versions.iter().map(|x| x.id.as_str()).collect();
    assert_eq!(ids[0], "abc1234", "{ids:?}");
    assert!(v.versions[0].committed);
    assert_eq!(
        v.versions[0].message.as_deref(),
        Some("Fix login crash on empty input")
    );
    assert_eq!(v.edits[0].version_id.as_deref(), Some("abc1234"));
    assert!(v.edits[1]
        .version_id
        .as_deref()
        .is_some_and(|id| id.starts_with("wt-")));
    assert_eq!(v.current_version_id, v.edits[1].version_id);

    // Claims: "All 4 tests pass" rests on the passing run before it.
    let c = v
        .claims
        .iter()
        .find(|c| c.text.contains("All 4 tests pass"))
        .expect("claim");
    assert_eq!(c.kind, "tests_pass");
    assert!(c.has_verifier);
    assert_eq!(c.rests_on_command_id.as_deref(), Some(tests[0].id.as_str()));
    assert!(!c.followed_by_passing_test);

    // Proof: the last run failed, after the edit to src/auth.rs.
    assert_eq!(v.thread.whose_move, hq::WhoseMove::You);
    assert_eq!(v.thread.proof.edited_after_last_pass, vec!["src/auth.rs"]);
}

#[test]
fn codex_patches_and_code_mode_programs_are_read() {
    let s = store();
    let ledger = Ledger::open(&s.db).expect("open");
    let v = thread_by_title(&ledger, "Rename the helper");
    let files: Vec<&str> = v.edits.iter().map(|e| e.file.as_str()).collect();
    assert_eq!(files, vec!["src/util.rs", "src/names.rs"]);
    let util = &v.edits[0];
    assert_eq!(util.source, "apply_patch");
    assert_eq!(util.via, "codex_patch");
    assert_eq!(util.hunks[0].header.as_deref(), Some("fn helper()"));
    let names = &v.edits[1];
    assert_eq!(names.via, "codex_exec");
    assert_eq!(names.lines_added, 1);
    // The program ran one command and its output states exit 0.
    let cmd = &v.commands[0];
    assert_eq!(cmd.command, "cargo test --lib");
    assert_eq!(cmd.via, "codex_exec");
    assert_eq!(cmd.outcome, hq::Outcome::Succeeded);
    assert_eq!(cmd.tally.as_ref().map(|t| t.passed), Some(2));
    // The claim follows the passing run, so nothing is open.
    assert_eq!(v.thread.whose_move, hq::WhoseMove::Done);
}

#[test]
fn the_session_timeline_is_complete_and_classified() {
    let s = store();
    let ledger = Ledger::open(&s.db).expect("open");
    let t = hq::session(&ledger, "cccccccc-1111-4111-8111-111111111111")
        .expect("read")
        .expect("present");
    let kinds: Vec<&str> = t.items.iter().map(|i| i.kind).collect();
    assert_eq!(kinds.iter().filter(|k| **k == "prompt").count(), 3);
    assert_eq!(kinds.iter().filter(|k| **k == "injected").count(), 2);
    assert_eq!(kinds.iter().filter(|k| **k == "tool").count(), 4);
    // Results ride on their calls, not as separate items.
    assert_eq!(kinds.iter().filter(|k| **k == "tool_result").count(), 0);
    assert_eq!(
        t.hq_project_id.as_deref(),
        Some(hq::project_id_for_root("/work/app").as_str())
    );
}

#[test]
fn every_served_string_is_masked() {
    // A credential typed into a prompt never leaves HQ in the clear.
    let tmp = TempTree::new();
    let dir = tmp.path().join("-work-app");
    std::fs::create_dir_all(&dir).expect("dir");
    let secret = "ghp_0123456789abcdefghijABCDEFGHIJ012345";
    let lines = [
        json!({"type":"user","uuid":"u1","parentUuid":null,"isSidechain":false,"sessionId":"s","timestamp":"2026-10-01T09:00:00Z","cwd":"/work/app",
               "message":{"role":"user","content":format!("Fix the deploy, the token is {secret}")}}),
        json!({"type":"assistant","uuid":"u2","parentUuid":"u1","isSidechain":false,"sessionId":"s","timestamp":"2026-10-01T09:00:01Z","cwd":"/work/app",
               "message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":format!("GH_TOKEN={secret} gh release list")}}]}}),
    ];
    let body: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(dir.join("s.jsonl"), body).expect("write");
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(&mut ledger, &[dir], &tmp.path().join("mirror")).expect("ingest");
    let id = hq::project_id_for_root("/work/app");
    let view = hq::project(&ledger, &id, now())
        .expect("read")
        .expect("present");
    let tv = hq::thread(&ledger, &id, &view.threads[0].id, now())
        .expect("read")
        .expect("present");
    let session = hq::session(&ledger, "s").expect("read").expect("present");
    for served in [
        serde_json::to_string(&view).expect("json"),
        serde_json::to_string(&tv).expect("json"),
        serde_json::to_string(&session).expect("json"),
    ] {
        assert!(!served.contains(secret), "secret served in the clear");
    }
    assert!(tv.masking.masked >= 2, "{:?}", tv.masking);
}

#[test]
fn the_load_plan_finds_only_this_roots_recordings() {
    let tmp = TempTree::new();
    let claude = tmp.path().join("projects");
    let codex = tmp.path().join("codex");
    let write = |path: PathBuf, cwd: &str| {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        std::fs::write(&path, format!("{}\n", json!({"type":"user","cwd":cwd}))).expect("write");
    };
    write(claude.join("-work-app/a.jsonl"), "/work/app");
    write(
        claude.join("-work-app--claude-worktrees-x/b.jsonl"),
        "/work/app/.claude/worktrees/x",
    );
    write(claude.join("-work-app-sub/c.jsonl"), "/work/app/sub");
    // A sibling repository whose mangled name shares the prefix.
    write(claude.join("-work-app-two/d.jsonl"), "/work/app-two");
    let meta = |cwd: &str| {
        format!(
            "{}\n",
            json!({"type":"session_meta","payload":{"id":"x","cwd":cwd}})
        )
    };
    std::fs::create_dir_all(codex.join("2026/10/01")).expect("dir");
    std::fs::write(codex.join("2026/10/01/r1.jsonl"), meta("/work/app")).expect("w");
    std::fs::write(codex.join("2026/10/01/r2.jsonl"), meta("/work/other")).expect("w");
    let plan = hq::load_plan("/work/app", &claude, &[codex]);
    let names: Vec<String> = plan
        .claude_code
        .iter()
        .chain(plan.codex.iter())
        .map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(names, vec!["a.jsonl", "b.jsonl", "c.jsonl", "r1.jsonl"]);
}

#[test]
fn reads_write_nothing() {
    let s = store();
    let ledger = Ledger::open(&s.db).expect("open");
    let count = |l: &Ledger| -> i64 {
        l.conn()
            .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
            .expect("count")
    };
    let before = count(&ledger);
    let id = hq::project_id_for_root("/work/app");
    let view = hq::project(&ledger, &id, now())
        .expect("read")
        .expect("present");
    for t in &view.threads {
        hq::thread(&ledger, &id, &t.id, now()).expect("read");
    }
    assert_eq!(count(&ledger), before);
    // And the same rows give the same answer.
    let again = hq::project(&ledger, &id, now())
        .expect("read")
        .expect("present");
    assert_eq!(
        serde_json::to_string(&view).expect("json"),
        serde_json::to_string(&again).expect("json")
    );
}

#[test]
fn work_recorded_in_a_temporary_folder_is_not_a_project() {
    let s = store();
    let tmp = TempTree::new();
    let dir = tmp.path().join("-private-tmp-claude-501-scratchpad");
    std::fs::create_dir_all(&dir).expect("dir");
    let line = json!({"type":"user","uuid":"t1","parentUuid":null,"isSidechain":false,"sessionId":"t",
        "timestamp":"2026-10-03T09:00:00Z","cwd":"/private/tmp/claude-501/scratchpad",
        "message":{"role":"user","content":"Fix the scratch script."}});
    std::fs::write(dir.join("t.jsonl"), format!("{line}\n")).expect("write");
    let mut ledger = Ledger::open(&s.db).expect("open");
    ingest(&mut ledger, &[dir], &tmp.path().join("mirror")).expect("ingest");
    let (projects, excluded) = hq::hq_projects_and_excluded(&ledger).expect("projects");
    let roots: Vec<&str> = projects.iter().map(|p| p.root.as_str()).collect();
    assert_eq!(roots, vec!["/work/app"]);
    assert_eq!(excluded, 1);
    assert!(hq::is_temp_root("/var/folders/ab/T/x"));
    assert!(hq::is_temp_root("/tmp"));
    assert!(!hq::is_temp_root("/tmpfoo/x"));
    // A repository cloned into a temporary folder is real work.
    let repo = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(repo.path().join(".git")).expect("git");
    std::fs::create_dir_all(repo.path().join("src")).expect("src");
    let inside = repo.path().join("src").display().to_string();
    assert!(hq::is_temp_root(&inside));
    assert!(!hq::is_scratch_root(&inside));
    assert!(hq::is_scratch_root(
        "/private/tmp/claude-501/gone/scratchpad"
    ));
}

#[test]
fn every_statement_has_the_change_models_verdict() {
    let s = store();
    let ledger = Ledger::open(&s.db).expect("open");
    let p = the_project(&ledger);
    let index = hq::index(&ledger, &p).expect("index");
    let st = index.statements();
    let by_text = |needle: &str| {
        let tv = thread_by_title(&ledger, needle);
        tv.claims
    };
    // One row per claim, no text, every one in a thread here.
    assert_eq!(st.claims.len(), 3);
    assert!(st.claims.iter().all(|c| c.thread_id.is_some()));
    let rows: Vec<(String, String, &str)> = st
        .claims
        .iter()
        .map(|c| {
            (
                c.agent.clone(),
                serde_json::to_value(c.verdict)
                    .expect("v")
                    .as_str()
                    .unwrap_or("")
                    .to_string(),
                c.verdict_basis,
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            // "All 4 tests pass" — the next day's run on the same thread failed.
            (
                "claude_code".into(),
                "contradicted".into(),
                "later_run_failed"
            ),
            // "Done. I added the setup section" — no run can settle it.
            ("claude_code".into(), "recorded".into(), "no_verifier"),
            // "Both tests pass" — the run before it passed, nothing edited since.
            ("codex".into(), "proven".into(), "run_holds"),
        ]
    );
    // The thread view says the same for the same claim.
    let login = by_text("Fix the login crash");
    assert_eq!(login[0].verdict_basis, "later_run_failed");
    let rename = by_text("Rename the helper");
    assert_eq!(rename[0].verdict_basis, "run_holds");
    assert_eq!(st.sessions.len(), 3);
    assert!(!serde_json::to_string(&st)
        .expect("json")
        .contains("All 4 tests pass"));
}

/// The three facts the statements route serves beside the verdict, which a
/// screen counting over time needs: when the run that proves a claim was
/// recorded, whether any run came before the claim, and whether the proof it
/// rested on was moved. One session, three threads (a gap of more than six
/// hours and a disjoint file opens each new one).
#[test]
fn a_statement_says_when_it_was_proven_whether_a_run_came_before_it_and_whether_its_proof_moved() {
    let tmp = TempTree::new();
    let dir = tmp.path().join("-work-app");
    std::fs::create_dir_all(&dir).expect("dir");
    let mut n = 0u32;
    let mut rows: Vec<String> = Vec::new();
    let mut push = |ts: &str, kind: &str, msg: Value| {
        n += 1;
        rows.push(
            json!({"type": kind, "uuid": format!("q1-{n}"), "parentUuid": null,
                   "isSidechain": false, "sessionId": "q1", "timestamp": ts,
                   "cwd": "/work/app", "message": msg})
            .to_string(),
        );
    };
    let user = |t: &str| json!({"role": "user", "content": t});
    let say = |t: &str| json!({"role": "assistant", "content": [{"type": "text", "text": t}]});
    let edit = |id: &str, file: &str| {
        json!({"role": "assistant", "content": [{"type": "tool_use", "id": id, "name": "Edit",
        "input": {"file_path": format!("/work/app/{file}"), "old_string": "a", "new_string": "b"}}]})
    };
    let test = |id: &str| {
        json!({"role": "assistant", "content": [{"type": "tool_use", "id": id, "name": "Bash",
        "input": {"command": "cd /work/app && cargo test -p app", "description": "run tests"}}]})
    };
    let result = |id: &str, text: &str, err: bool| {
        json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": id,
        "content": text, "is_error": err}]})
    };
    let pass = "running 4 tests\ntest result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s";
    let fail = "Exit code 101\nrunning 4 tests\ntest result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s";
    let claim = "All 4 tests pass in `cargo test -p app`.";
    let done = "updated";

    // Thread 1: a claim before any run (A); a claim after a passing run (B);
    // an edit; a passing run on the new version; a claim after that (C).
    push(
        "2026-10-05T09:00:00.000Z",
        "user",
        user("Fix the parser in src/p.rs."),
    );
    push(
        "2026-10-05T09:00:05.000Z",
        "assistant",
        edit("t1", "src/p.rs"),
    );
    push(
        "2026-10-05T09:00:06.000Z",
        "user",
        result("t1", done, false),
    );
    push("2026-10-05T09:00:10.000Z", "assistant", say(claim)); // A
    push("2026-10-05T09:01:00.000Z", "assistant", test("t2"));
    push(
        "2026-10-05T09:01:30.000Z",
        "user",
        result("t2", pass, false),
    );
    push("2026-10-05T09:02:00.000Z", "assistant", say(claim)); // B
    push(
        "2026-10-05T09:03:00.000Z",
        "assistant",
        edit("t3", "src/p.rs"),
    );
    push(
        "2026-10-05T09:03:01.000Z",
        "user",
        result("t3", done, false),
    );
    push("2026-10-05T09:04:00.000Z", "assistant", test("t4"));
    push(
        "2026-10-05T09:04:30.000Z",
        "user",
        result("t4", pass, false),
    );
    push("2026-10-05T09:05:00.000Z", "assistant", say(claim)); // C
                                                               // Thread 2: a claim before any run (E); a claim after a passing run, then
                                                               // an edit and no run since (D).
    push(
        "2026-10-05T17:00:00.000Z",
        "user",
        user("Fix the lexer in src/lexer.rs."),
    );
    push(
        "2026-10-05T17:00:05.000Z",
        "assistant",
        edit("t5", "src/lexer.rs"),
    );
    push(
        "2026-10-05T17:00:06.000Z",
        "user",
        result("t5", done, false),
    );
    push("2026-10-05T17:00:10.000Z", "assistant", say(claim)); // E
    push("2026-10-05T17:01:00.000Z", "assistant", test("t6"));
    push(
        "2026-10-05T17:01:30.000Z",
        "user",
        result("t6", pass, false),
    );
    push("2026-10-05T17:02:00.000Z", "assistant", say(claim)); // D
    push(
        "2026-10-05T17:03:00.000Z",
        "assistant",
        edit("t7", "src/lexer.rs"),
    );
    push(
        "2026-10-05T17:03:01.000Z",
        "user",
        result("t7", done, false),
    );
    // Thread 3: a claim after a passing run, an edit, then a failing run (F).
    push(
        "2026-10-06T01:00:00.000Z",
        "user",
        user("Fix the printer in src/printer.rs."),
    );
    push(
        "2026-10-06T01:00:05.000Z",
        "assistant",
        edit("t8", "src/printer.rs"),
    );
    push(
        "2026-10-06T01:00:06.000Z",
        "user",
        result("t8", done, false),
    );
    push("2026-10-06T01:01:00.000Z", "assistant", test("t9"));
    push(
        "2026-10-06T01:01:30.000Z",
        "user",
        result("t9", pass, false),
    );
    push("2026-10-06T01:02:00.000Z", "assistant", say(claim)); // F
    push(
        "2026-10-06T01:03:00.000Z",
        "assistant",
        edit("t10", "src/printer.rs"),
    );
    push(
        "2026-10-06T01:03:01.000Z",
        "user",
        result("t10", done, false),
    );
    push("2026-10-06T01:04:00.000Z", "assistant", test("t11"));
    push(
        "2026-10-06T01:04:30.000Z",
        "user",
        result("t11", fail, true),
    );
    std::fs::write(dir.join("q1.jsonl"), rows.join("\n") + "\n").expect("w");

    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(&mut ledger, &[dir], &tmp.path().join("mirror")).expect("ingest");
    let id = hq::project_id_for_root("/work/app");
    let p = hq::hq_projects(&ledger).expect("projects").remove(0);
    assert_eq!(p.id, id);
    let st = hq::index(&ledger, &p).expect("index").statements();
    let got: Vec<(String, &str, Option<String>, bool, bool)> = st
        .claims
        .iter()
        .map(|c| {
            (
                c.ts.clone().unwrap_or_default(),
                c.verdict_basis,
                c.proven_at.clone(),
                c.run_before,
                c.proof_moved,
            )
        })
        .collect();
    let at = |s: &str| Some(format!("2026-10-05T{s}.000Z"));
    let want: Vec<(String, &str, Option<String>, bool, bool)> = vec![
        // A: said before any run; the run after the edit proves it, 4 minutes later.
        (
            "2026-10-05T09:00:10.000Z".into(),
            "settled_by_later_run",
            at("09:04:00"),
            false,
            false,
        ),
        // B: rested on a passing run, an edit moved it, the later run proves it again.
        (
            "2026-10-05T09:02:00.000Z".into(),
            "settled_by_later_run",
            at("09:04:00"),
            true,
            true,
        ),
        // C: the run it rests on came before it and still holds, so it was proven when said.
        (
            "2026-10-05T09:05:00.000Z".into(),
            "run_holds",
            at("09:04:00"),
            true,
            false,
        ),
        // E: said before any run, and nothing proves it.
        (
            "2026-10-05T17:00:10.000Z".into(),
            "no_run_before",
            None,
            false,
            false,
        ),
        // D: rested on a passing run, then an edit; no run since.
        (
            "2026-10-05T17:02:00.000Z".into(),
            "edited_since_run",
            None,
            true,
            true,
        ),
        // F: rested on a passing run, an edit moved it, and the re-run failed.
        (
            "2026-10-06T01:02:00.000Z".into(),
            "later_run_failed",
            None,
            true,
            true,
        ),
    ];
    assert_eq!(got, want);
    // The facts ride beside the verdict on the served row and carry no text.
    let v = serde_json::to_value(&st).expect("json");
    let first = &v["claims"][0];
    assert_eq!(first["proven_at"], "2026-10-05T09:04:00.000Z");
    assert_eq!(first["run_before"], false);
    assert_eq!(first["proof_moved"], false);
    assert!(v["claims"][3]["proven_at"].is_null());
    // `proven_at` is set exactly when the claim is proven.
    for c in &st.claims {
        let proven = serde_json::to_value(c.verdict).expect("v") == "proven";
        assert_eq!(c.proven_at.is_some(), proven, "{}", c.id);
    }
    // The page's "need a re-check" and its "proof moved, and still open" are one
    // number by construction: a not-proven claim has the fact exactly when the
    // proof chain calls it moved (④ SINCE).
    for c in &st.claims {
        let moved_basis = matches!(c.verdict_basis, "edited_since_run" | "run_on_older_version");
        let not_proven = serde_json::to_value(c.verdict).expect("v") == "not_proven";
        assert_eq!(moved_basis, c.proof_moved && not_proven, "{}", c.id);
    }
}

#[test]
fn a_pasted_brief_is_marked_and_does_not_title_the_thread() {
    let tmp = TempTree::new();
    let dir = tmp.path().join("-work-app");
    std::fs::create_dir_all(&dir).expect("dir");
    let paste = "\n\n<pasted_content id=\"23e0\">\nThruLine — Phase 6 UI shell, parallel session. Paste as the first message.\nRules: never read, write, `cd` into, run a server from, or run git against `~/thruline`.\n</pasted_content id=\"23e0\">\n";
    let mixed = "<pasted_content id=\"a1\">\nFix the whole world.\n</pasted_content id=\"a1\">\nPlease review this brief and fix `src/b.rs`.";
    let line = |n: u32, sid: &str, ts: &str, msg: serde_json::Value| {
        json!({"type": if msg["role"] == "user" {"user"} else {"assistant"}, "uuid": format!("{sid}-{n}"),
               "parentUuid": null, "isSidechain": false, "sessionId": sid, "timestamp": ts,
               "cwd": "/work/app", "message": msg})
        .to_string()
    };
    let edit = |id: &str, file: &str| {
        json!({"role": "assistant", "content": [{"type": "tool_use", "id": id, "name": "Edit",
        "input": {"file_path": format!("/work/app/{file}"), "old_string": "a", "new_string": "b"}}]})
    };
    let one = [
        line(
            1,
            "p1",
            "2026-10-05T09:00:00Z",
            json!({"role": "user", "content": paste}),
        ),
        line(2, "p1", "2026-10-05T09:00:05Z", edit("e1", "src/a.rs")),
    ]
    .join("\n");
    let two = [
        line(
            1,
            "p2",
            "2026-10-06T09:00:00Z",
            json!({"role": "user", "content": mixed}),
        ),
        line(2, "p2", "2026-10-06T09:00:05Z", edit("e2", "src/b.rs")),
    ]
    .join("\n");
    std::fs::write(dir.join("p1.jsonl"), one + "\n").expect("w");
    std::fs::write(dir.join("p2.jsonl"), two + "\n").expect("w");
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(&mut ledger, &[dir], &tmp.path().join("mirror")).expect("ingest");
    let id = hq::project_id_for_root("/work/app");
    let view = hq::project(&ledger, &id, now())
        .expect("read")
        .expect("present");
    let titles: Vec<(String, hq::prompts::TitleBasis)> = view
        .threads
        .iter()
        .map(|t| (t.title.clone(), t.title_basis))
        .collect();
    assert!(
        titles.contains(&(
            "ThruLine — Phase 6 UI shell, parallel session.".to_string(),
            hq::prompts::TitleBasis::PastedFirstSentence
        )),
        "{titles:?}"
    );
    assert!(
        titles.contains(&(
            "Please review this brief and fix `src/b.rs`.".to_string(),
            hq::prompts::TitleBasis::WorkSentence
        )),
        "{titles:?}"
    );
    let t = view
        .threads
        .iter()
        .find(|t| t.title.starts_with("Please review"))
        .expect("thread");
    let tv = hq::thread(&ledger, &id, &t.id, now())
        .expect("read")
        .expect("present");
    let segs = &tv.prompts[0].segments;
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].kind, hq::prompts::SegmentKind::Pasted);
    assert_eq!(segs[0].text, "Fix the whole world.");
    assert_eq!(segs[1].kind, hq::prompts::SegmentKind::Typed);
    // The verbatim text is still served whole.
    assert!(tv.prompts[0].text.contains("<pasted_content id=\"a1\">"));
    let timeline = hq::session(&ledger, "p1").expect("read").expect("present");
    let prompt = timeline
        .items
        .iter()
        .find(|i| i.kind == "prompt")
        .expect("prompt");
    assert_eq!(
        prompt.segments.as_ref().map(|s| s[0].kind),
        Some(hq::prompts::SegmentKind::Pasted)
    );
}

#[test]
fn the_list_row_is_the_views_row_at_any_now_and_the_quick_row_claims_no_counts() {
    let s = store();
    let ledger = Ledger::open(&s.db).expect("open");
    let p = the_project(&ledger);
    let index = hq::index(&ledger, &p).expect("index");
    let listing = index.listing();
    // Long after, and one minute after the last recorded row (the agent is
    // then still working on that thread).
    let last = hq::parse_ms("2026-10-04T08:00:50Z").expect("instant");
    for now in [now(), last + 60_000] {
        assert_eq!(
            serde_json::to_value(listing.at(now)).expect("json"),
            serde_json::to_value(index.view(now).project).expect("json"),
        );
    }
    assert_eq!(listing.at(last + 60_000).whose_move["agent"], 1);

    let quick = hq::quick_summary(&ledger, &p).expect("quick");
    assert!(!quick.complete);
    assert_eq!(quick.sessions, 3);
    assert!(quick.events > 0);
    assert_eq!(
        quick.first_activity.as_deref(),
        Some("2026-10-01T09:00:00.000Z")
    );
    assert_eq!(
        quick.last_activity.as_deref(),
        Some("2026-10-04T08:00:50.000Z")
    );
    let v = serde_json::to_value(&quick).expect("json");
    for field in [
        "prompts",
        "edits",
        "claims",
        "threads",
        "whose_move",
        "commands",
    ] {
        assert!(v[field].is_null(), "{field} must be absent, not zero: {v}");
    }
}

#[test]
fn a_project_recorded_in_the_home_folder_has_the_home_path_as_its_root() {
    let tmp = TempTree::new();
    let dir = tmp.path().join("-Users-fixture-home");
    std::fs::create_dir_all(&dir).expect("dir");
    let line = json!({"type":"user","uuid":"h1","parentUuid":null,"isSidechain":false,"sessionId":"h",
        "timestamp":"2026-10-03T09:00:00Z","cwd":"/Users/fixture-home",
        "message":{"role":"user","content":"Tidy my notes."}});
    std::fs::write(dir.join("h.jsonl"), format!("{line}\n")).expect("write");
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(&mut ledger, &[dir], &tmp.path().join("mirror")).expect("ingest");
    let projects = hq::hq_projects(&ledger).expect("projects");
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0].root, "/Users/fixture-home");
    assert_eq!(projects[0].members[0].root_basis, "recorded_cwd");
}

#[test]
fn a_folder_with_no_recorded_cwd_serves_its_folder_name_not_a_decoded_path() {
    let tmp = TempTree::new();
    let name = "-nowhere-fixture-Telescope-project--claude-worktrees-x";
    let dir = tmp.path().join(name);
    std::fs::create_dir_all(&dir).expect("dir");
    // A recording that states no `cwd`.
    let line = json!({"type":"user","uuid":"n1","parentUuid":null,"isSidechain":false,"sessionId":"n",
        "timestamp":"2026-10-03T09:00:00Z","message":{"role":"user","content":"Fix it."}});
    std::fs::write(dir.join("n.jsonl"), format!("{line}\n")).expect("write");
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(&mut ledger, &[dir], &tmp.path().join("mirror")).expect("ingest");
    let projects = hq::hq_projects(&ledger).expect("projects");
    let m = &projects[0].members[0];
    assert_eq!(m.root_basis, "folder_name");
    // Nothing states where this folder is, so no path is shown for it: the
    // folder name as stored (CTO 2026-10-12: ThruLine must not show a guess).
    assert_eq!(projects[0].shown_root(), name);
    assert_eq!(projects[0].name, name);
    assert_eq!(m.root, name);
    assert!(!projects[0].path_known());
    // What it is identified by is not changed by what is shown: still the
    // decoded name, so its id, and everything keyed on it, does not move.
    assert_eq!(projects[0].root, "/nowhere/fixture/Telescope/project");
    assert_eq!(
        projects[0].id,
        hq::project_id_for_root("/nowhere/fixture/Telescope/project")
    );
}

/// The reported case (onboarding harness, 2026-10-12): a hyphenated folder
/// whose recordings state no `cwd` used to be served as `~/my/app`. It is the
/// folder name now, until a recording states the path.
#[test]
fn a_hyphenated_folder_whose_recordings_state_no_cwd_is_not_served_as_a_path_with_every_hyphen_a_slash(
) {
    let tmp = TempTree::new();
    let dir = tmp.path().join("-nowhere-fixture-my-app");
    std::fs::create_dir_all(&dir).expect("dir");
    let line = json!({"type":"user","uuid":"h1","parentUuid":null,"isSidechain":false,"sessionId":"h",
        "timestamp":"2026-10-12T09:00:00Z","message":{"role":"user","content":"Fix it."}});
    std::fs::write(dir.join("h.jsonl"), format!("{line}\n")).expect("write");
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(&mut ledger, &[dir], &tmp.path().join("mirror")).expect("ingest");
    let projects = hq::hq_projects(&ledger).expect("projects");
    assert_eq!(projects.len(), 1, "{projects:?}");
    assert_eq!(projects[0].shown_root(), "-nowhere-fixture-my-app");
    assert_eq!(projects[0].name, "-nowhere-fixture-my-app");
    assert_eq!(projects[0].members[0].root, "-nowhere-fixture-my-app");
    assert_eq!(projects[0].members[0].root_basis, "folder_name");
    assert_eq!(
        projects[0].id,
        hq::project_id_for_root("/nowhere/fixture/my/app")
    );
    // The flag the screens read, on the row they list.
    assert!(!projects[0].path_known());
    let quick = serde_json::to_value(hq::quick_summary(&ledger, &projects[0]).expect("quick"))
        .expect("json");
    assert_eq!(quick["path_known"], false, "{quick}");
    assert_eq!(quick["root"], "-nowhere-fixture-my-app", "{quick}");
    assert_eq!(quick["name"], "-nowhere-fixture-my-app", "{quick}");
}

/// The same folder once a recording states its `cwd`: that path wins.
#[test]
fn a_hyphenated_folder_that_has_been_recorded_serves_the_recorded_cwd() {
    let tmp = TempTree::new();
    let dir = tmp.path().join("-nowhere-fixture-my-app");
    std::fs::create_dir_all(&dir).expect("dir");
    let line = json!({"type":"user","uuid":"r1","parentUuid":null,"isSidechain":false,"sessionId":"r",
        "timestamp":"2026-10-12T09:00:00Z","cwd":"/nowhere/fixture/my-app",
        "message":{"role":"user","content":"Fix it."}});
    std::fs::write(dir.join("r.jsonl"), format!("{line}\n")).expect("write");
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(&mut ledger, &[dir], &tmp.path().join("mirror")).expect("ingest");
    let projects = hq::hq_projects(&ledger).expect("projects");
    assert_eq!(projects.len(), 1, "{projects:?}");
    assert_eq!(projects[0].root, "/nowhere/fixture/my-app");
    assert_eq!(projects[0].name, "my-app");
    assert_eq!(projects[0].members[0].root_basis, "recorded_cwd");
    assert_eq!(
        projects[0].id,
        hq::project_id_for_root("/nowhere/fixture/my-app")
    );
    assert!(projects[0].path_known());
    let quick = serde_json::to_value(hq::quick_summary(&ledger, &projects[0]).expect("quick"))
        .expect("json");
    assert_eq!(quick["path_known"], true, "{quick}");
}

/// A folder a person picked on the First run screen before anything was
/// recorded in it, with a `-` in its path (`~/my-app`, the reported case): the
/// registration stored the path they picked, so it is not read off the name.
#[test]
fn a_picked_hyphenated_folder_keeps_the_path_that_was_picked() {
    let tmp = TempTree::new();
    let db = tmp.path().join("ledger.db");
    let ledger = Ledger::open(&db).expect("open");
    ledger
        .upsert_project(
            &thruline_core::model::ProjectRef {
                id: "-nowhere-fixture-my-app".to_string(),
                root_path: "/nowhere/fixture/my-app".to_string(),
                source: thruline_core::model::Source::ClaudeCode,
                display_name: Some("app".to_string()),
            },
            "2026-10-12T00:00:00Z",
        )
        .expect("project");
    let projects = hq::hq_projects(&ledger).expect("projects");
    assert_eq!(projects.len(), 1, "{projects:?}");
    assert_eq!(projects[0].shown_root(), "/nowhere/fixture/my-app");
    assert_eq!(projects[0].root, "/nowhere/fixture/my-app");
    assert_eq!(projects[0].name, "my-app");
    assert!(projects[0].path_known());
    assert_eq!(projects[0].members[0].root_basis, "stored_root");
}

/// The common case of the same pick: no `-` in the path, so the stored root
/// equals the name read back, and nothing was recorded that could have
/// written it. It is the pick.
#[test]
fn a_picked_folder_with_nothing_recorded_keeps_the_path_that_was_picked() {
    let tmp = TempTree::new();
    let db = tmp.path().join("ledger.db");
    let ledger = Ledger::open(&db).expect("open");
    ledger
        .upsert_project(
            &thruline_core::model::ProjectRef {
                id: "-nowhere-fixture-shop".to_string(),
                root_path: "/nowhere/fixture/shop".to_string(),
                source: thruline_core::model::Source::ClaudeCode,
                display_name: Some("shop".to_string()),
            },
            "2026-10-12T00:00:00Z",
        )
        .expect("project");
    let projects = hq::hq_projects(&ledger).expect("projects");
    assert_eq!(projects.len(), 1, "{projects:?}");
    assert_eq!(projects[0].shown_root(), "/nowhere/fixture/shop");
    assert!(projects[0].path_known());
}

/// A sibling whose name merely begins with another project's name is not
/// inside it: `proj-two` is not `proj/two`, and nothing says which it is.
#[test]
fn a_sibling_folder_is_not_folded_into_a_project_whose_name_it_begins_with() {
    let tmp = TempTree::new();
    let write = |folder: &str, sid: &str, cwd: Option<&str>| {
        let dir = tmp.path().join(folder);
        std::fs::create_dir_all(&dir).expect("dir");
        let mut line = json!({"type":"user","uuid":format!("{sid}1"),"parentUuid":null,"isSidechain":false,
            "sessionId":sid,"timestamp":"2026-10-03T09:00:00Z","message":{"role":"user","content":"Fix it."}});
        if let Some(c) = cwd {
            line["cwd"] = json!(c);
        }
        std::fs::write(dir.join(format!("{sid}.jsonl")), format!("{line}\n")).expect("write");
        dir
    };
    let a = write("-nowhere-u-proj", "a", Some("/nowhere/u/proj"));
    let b = write("-nowhere-u-proj-two", "b", None);
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(&mut ledger, &[a, b], &tmp.path().join("mirror")).expect("ingest");
    let projects = hq::hq_projects(&ledger).expect("projects");
    assert_eq!(projects.len(), 2, "{projects:?}");
    let roots: Vec<String> = projects.iter().map(|p| p.shown_root()).collect();
    assert!(roots.contains(&"/nowhere/u/proj".to_string()), "{roots:?}");
    assert!(
        roots.contains(&"-nowhere-u-proj-two".to_string()),
        "{roots:?}"
    );
}

#[test]
fn a_worktree_folder_with_no_cwd_folds_into_the_repository_its_name_names() {
    let tmp = TempTree::new();
    let write = |folder: &str, sid: &str, cwd: Option<&str>| {
        let dir = tmp.path().join(folder);
        std::fs::create_dir_all(&dir).expect("dir");
        let mut line = json!({"type":"user","uuid":format!("{sid}1"),"parentUuid":null,"isSidechain":false,
            "sessionId":sid,"timestamp":"2026-10-03T09:00:00Z","message":{"role":"user","content":"Fix it."}});
        if let Some(c) = cwd {
            line["cwd"] = json!(c);
        }
        std::fs::write(dir.join(format!("{sid}.jsonl")), format!("{line}\n")).expect("write");
        dir
    };
    let a = write(
        "-nowhere-u-Telescope-project",
        "a",
        Some("/nowhere/u/Telescope_project"),
    );
    let b = write(
        "-nowhere-u-Telescope-project--claude-worktrees-x-y",
        "b",
        None,
    );
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(&mut ledger, &[a, b], &tmp.path().join("mirror")).expect("ingest");
    let projects = hq::hq_projects(&ledger).expect("projects");
    assert_eq!(projects.len(), 1, "{projects:?}");
    assert_eq!(projects[0].root, "/nowhere/u/Telescope_project");
    let bases: Vec<&str> = projects[0].members.iter().map(|m| m.root_basis).collect();
    assert!(bases.contains(&"known_root"), "{bases:?}");
}

/// HQ-NEEDS (Decisions, C28): each terminal confirmation is recorded as a
/// session of its own, `decision-terminal-<ulid>`, whose log has no recording
/// envelope and states no `cwd`. Five of them used to fill the window of
/// recent files the root is read from, so the project lost its recorded root
/// (and Decisions' route its project). The root comes from native recordings
/// only.
#[test]
fn terminal_confirmation_logs_never_displace_the_recorded_root() {
    let s = store();
    let ledger = Ledger::open(&s.db).expect("open");
    let before = the_project(&ledger);
    let claude = |p: &hq::HqProject| {
        p.members
            .iter()
            .find(|m| m.source == "claude_code")
            .map(|m| (m.root.clone(), m.root_basis))
            .expect("a Claude Code member")
    };
    assert_eq!(claude(&before), ("/work/app".to_string(), "recorded_cwd"));
    let project: String = ledger
        .conn()
        .query_row(
            "SELECT project_id FROM sessions s JOIN projects p ON p.id = s.project_id
              WHERE p.source = 'claude_code' LIMIT 1",
            [],
            |r| r.get(0),
        )
        .expect("the Claude Code project");
    let logs =
        s.db.parent()
            .expect("store dir")
            .join("decision-confirmations");
    std::fs::create_dir_all(&logs).expect("mkdir");
    for i in 0..6 {
        let id = format!("decision-terminal-01K0000000000000000000000{i}");
        let file = logs.join(format!("{id}.jsonl"));
        std::fs::write(
            &file,
            "{\"type\":\"thruline_decision_terminal_confirmation\",\"action\":\"accept\"}\n",
        )
        .expect("write log");
        ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id, project_id, file_path, started_at) VALUES(?1, ?2, ?3, ?4)",
                rusqlite::params![
                    id,
                    project,
                    file.display().to_string(),
                    format!("2099-01-01T00:00:0{i}Z")
                ],
            )
            .expect("record a terminal confirmation session");
    }
    let after = the_project(&ledger);
    assert_eq!(after.id, before.id, "the project keeps its id");
    assert_eq!(
        claude(&after),
        ("/work/app".to_string(), "recorded_cwd"),
        "the root still comes from the recording that states it"
    );
}
