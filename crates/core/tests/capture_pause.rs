//! C34 R-LEDGER (7) — the pause gates' shared reading (`capture_pause`) and the
//! one read they all make (`capture_hold::standing_for`).
//!
//! The process-level proofs live in `crates/cli/tests/capture_gates_noop.rs`
//! (zero holds: byte-for-byte base) and `capture_gates_paused.rs` (a pause is
//! honoured by the hook, the audit and `thruline ingest`). This asks what those
//! cannot cheaply: that "inside" is whole path components and never a string
//! prefix; that the session-start rule is a half-open window on the session's
//! first line and fails toward reading when a start cannot be read; that a read
//! error is never a skip; that every skip is counted per folder; that a load
//! plan is cut and its bytes recomputed; and that a thread view marks a row
//! `recorded_while_paused` by a join, only when it is, and serves the bytes it
//! always served when no one ever paused anything.
//!
//! Scratch ledgers; no real store is opened.

mod scratch;

use std::path::{Path, PathBuf};

use scratch::TempTree;
use serde_json::Value;
use thruline_core::capture_hold::{self, Act, Hold, Recorded};
use thruline_core::capture_pause::{self, PauseCounts, PauseGate, SkipGate, Verdict};
use thruline_core::ledger::Ledger;

const APP: &str = "/work/app";

fn at<'a>(folder: &'a str, when: &'a str) -> Hold<'a> {
    Hold {
        project_id: "hq-project",
        project_folder: folder,
        actor: "robin",
        recorded_at: when,
    }
}

fn pause(ledger: &Ledger, folder: &str, when: &str) {
    match capture_hold::record(ledger, Act::Paused, &at(folder, when), None).expect("record") {
        Recorded::Appended(_) => {}
        Recorded::Refused(why) => panic!("pause refused: {why:?}"),
    }
}

fn resume(ledger: &Ledger, folder: &str, when: &str) {
    match capture_hold::record(ledger, Act::Resumed, &at(folder, when), None).expect("record") {
        Recorded::Appended(_) => {}
        Recorded::Refused(why) => panic!("resume refused: {why:?}"),
    }
}

fn ms(iso: &str) -> i64 {
    thruline_core::hq::parse_ms(iso).expect("instant")
}

#[test]
fn a_path_is_inside_a_folder_by_whole_components_never_by_string_prefix() {
    use capture_hold::path_is_inside as inside;
    assert!(inside("/a/b", "/a/b"));
    assert!(inside("/a/b", "/a/b/c/d"));
    assert!(
        inside("/a/b/", "/a/b/c"),
        "a trailing slash is not a component"
    );
    assert!(
        !inside("/a/b", "/a/bc"),
        "`/a/bc` is another folder that happens to begin with the same letters"
    );
    assert!(!inside("/a/bc", "/a/b"));
    assert!(
        !inside("/a/b/c", "/a/b"),
        "a parent is not inside its child"
    );
    assert!(inside("/a/b", "/a/b/./c"), "`.` is not a component");
    assert!(
        !inside("/a/b", "/a/b/../c"),
        "`..` is taken lexically: that path is /a/c"
    );
    assert!(inside("/a/b", "/a/x/../b/c"));
    assert!(!inside("a/b", "a/b/c"), "a relative path is inside nothing");
    assert!(!inside("/a/b", "b/c"));
}

#[test]
fn standing_for_answers_the_pause_in_force_and_nothing_else() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let none = capture_hold::standing_for(&ledger, "/work/app/src").expect("read");
    assert_eq!(none, None, "no act, no pause");
    assert_eq!(capture_hold::pause_head(&ledger).expect("head"), None);

    pause(&ledger, APP, "2026-10-08T09:00:00Z");
    let head_one = capture_hold::pause_head(&ledger)
        .expect("head")
        .expect("some");
    let found = capture_hold::standing_for(&ledger, "/work/app/src")
        .expect("read")
        .expect("a pause covers a path inside the folder");
    assert_eq!(found.project_folder, APP);
    assert_eq!(found.recorded_at, "2026-10-08T09:00:00Z");
    assert!(capture_hold::standing_for(&ledger, APP)
        .expect("read")
        .is_some());
    assert_eq!(
        capture_hold::standing_for(&ledger, "/work/appx").expect("read"),
        None,
        "string-prefix is not inside"
    );
    assert_eq!(
        capture_hold::standing_for(&ledger, "/work").expect("read"),
        None,
        "the parent of a paused folder is not paused"
    );
    assert_eq!(
        capture_hold::standing_for(&ledger, "/elsewhere").expect("read"),
        None
    );
    assert_eq!(
        capture_hold::standing_for(&ledger, "relative/path").expect("read"),
        None
    );

    resume(&ledger, APP, "2026-10-08T10:00:00Z");
    assert_eq!(
        capture_hold::standing_for(&ledger, "/work/app/src").expect("read"),
        None,
        "a resumed folder is not paused"
    );
    let head_two = capture_hold::pause_head(&ledger)
        .expect("head")
        .expect("some");
    assert_ne!(
        head_one, head_two,
        "an act moves the key a cache is keyed on"
    );
}

#[test]
fn when_folders_nest_the_most_specific_pause_answers_and_a_parents_pause_covers_its_children() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    pause(&ledger, "/a", "2026-10-08T09:00:00Z");
    pause(&ledger, "/a/b", "2026-10-08T09:30:00Z");
    let found = capture_hold::standing_for(&ledger, "/a/b/c")
        .expect("read")
        .expect("covered");
    assert_eq!(found.project_folder, "/a/b");
    resume(&ledger, "/a/b", "2026-10-08T10:00:00Z");
    let found = capture_hold::standing_for(&ledger, "/a/b/c")
        .expect("read")
        .expect("/a is still paused and /a/b is inside it");
    assert_eq!(found.project_folder, "/a");
}

#[test]
fn a_symlink_into_a_paused_folder_does_not_walk_around_the_pause() {
    let tree = TempTree::new();
    let real = tree.join("real/app");
    std::fs::create_dir_all(real.join("src")).expect("mkdir");
    let link = tree.join("link");
    std::os::unix::fs::symlink(&real, &link).expect("symlink");
    let ledger = Ledger::open_in_memory().expect("ledger");
    let folder = real.to_str().expect("utf8").to_string();
    pause(&ledger, &folder, "2026-10-08T09:00:00Z");
    let through_the_link = link.join("src");
    assert!(
        capture_hold::standing_for(&ledger, through_the_link.to_str().expect("utf8"))
            .expect("read")
            .is_some(),
        "the disk says that path is inside the paused folder"
    );
}

#[test]
fn a_session_is_judged_by_where_it_started_against_a_half_open_window() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    pause(&ledger, APP, "2026-10-02T00:00:00Z");
    resume(&ledger, APP, "2026-10-03T00:00:00Z");
    let gate = PauseGate::read(&ledger).expect("gate");
    assert!(!gate.is_empty());
    let started = |iso: &'static str| move || Some(ms(iso));
    let judge = |iso: &'static str| gate.judge(APP, started(iso));

    assert_eq!(
        judge("2026-10-01T23:59:59Z"),
        Verdict::Read,
        "before the pause"
    );
    assert_eq!(
        judge("2026-10-02T00:00:00Z"),
        Verdict::StartedWhilePaused {
            folder: APP.to_string()
        },
        "the pause's own instant is inside it"
    );
    assert_eq!(
        judge("2026-10-02T23:59:59Z"),
        Verdict::StartedWhilePaused {
            folder: APP.to_string()
        }
    );
    assert_eq!(
        judge("2026-10-03T00:00:00Z"),
        Verdict::Read,
        "the resumption's own instant is outside it"
    );
    assert_eq!(
        gate.judge(APP, || None),
        Verdict::Read,
        "a start that cannot be read is read: nothing is dropped on a guess"
    );
    assert_eq!(
        gate.judge("/elsewhere", started("2026-10-02T12:00:00Z")),
        Verdict::Read
    );
    assert_eq!(
        gate.judge("/work/appx", started("2026-10-02T12:00:00Z")),
        Verdict::Read,
        "another folder with the same first letters"
    );

    // And a pause that stands is the stronger word, whenever the session began.
    pause(&ledger, APP, "2026-10-05T00:00:00Z");
    let gate = PauseGate::read(&ledger).expect("gate");
    assert_eq!(
        gate.judge("/work/app/src", started("2026-10-01T00:00:00Z")),
        Verdict::PausedByYou {
            folder: APP.to_string()
        }
    );
}

fn claude_line(cwd: &str, ts: &str) -> String {
    serde_json::json!({
        "type": "user", "uuid": "u", "timestamp": ts, "cwd": cwd,
        "message": {"role": "user", "content": "x"}
    })
    .to_string()
        + "\n"
}

fn codex_line(cwd: &str, ts: &str) -> String {
    serde_json::json!({
        "timestamp": ts, "type": "session_meta",
        "payload": {"id": "c", "cwd": cwd, "timestamp": ts}
    })
    .to_string()
        + "\n"
}

fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
    std::fs::create_dir_all(dir).expect("mkdir");
    let p = dir.join(name);
    std::fs::write(&p, body).expect("write");
    p
}

#[test]
fn recordings_are_left_out_by_their_folder_and_by_their_first_line_and_every_one_is_counted() {
    let tree = TempTree::new();
    let d = tree.join("rec");
    let inside_before = write(&d, "a.jsonl", &claude_line(APP, "2026-10-01T09:00:00.000Z"));
    let inside_during = write(&d, "b.jsonl", &claude_line(APP, "2026-10-02T10:00:00.000Z"));
    let codex_during = write(&d, "c.jsonl", &codex_line(APP, "2026-10-02T11:00:00.000Z"));
    let elsewhere = write(
        &d,
        "d.jsonl",
        &claude_line("/elsewhere", "2026-10-02T10:00:00.000Z"),
    );
    let near_miss = write(
        &d,
        "e.jsonl",
        &claude_line("/work/appx", "2026-10-02T10:00:00.000Z"),
    );
    let no_start = write(&d, "f.jsonl", &format!("{{\"cwd\":\"{APP}\"}}\n"));
    let all = vec![
        inside_before.clone(),
        inside_during,
        codex_during,
        elsewhere.clone(),
        near_miss.clone(),
        no_start.clone(),
    ];

    // With no pause in the record, nothing is opened: paths that do not exist
    // come straight back.
    let ledger = Ledger::open_in_memory().expect("ledger");
    let gate = PauseGate::read(&ledger).expect("gate");
    let ghosts = vec![
        PathBuf::from("/nowhere/1.jsonl"),
        PathBuf::from("/nowhere/2.jsonl"),
    ];
    let (kept, counts) = capture_pause::filter_files(&gate, ghosts.clone());
    assert_eq!(kept, ghosts);
    assert_eq!(counts, PauseCounts::default());

    pause(&ledger, APP, "2026-10-02T00:00:00Z");
    resume(&ledger, APP, "2026-10-03T00:00:00Z");
    let gate = PauseGate::read(&ledger).expect("gate");
    let (kept, counts) = capture_pause::filter_files(&gate, all.clone());
    assert_eq!(
        kept,
        vec![inside_before, elsewhere, near_miss, no_start],
        "kept: started before the pause, elsewhere, a near-miss folder, and one whose start cannot be read"
    );
    assert_eq!(counts.paused_by_you, 0);
    assert_eq!(
        counts.started_while_paused, 2,
        "the Claude Code and the Codex session that started in the pause"
    );
    assert_eq!(counts.total(), 2);
    assert_eq!(
        counts.lines().len(),
        1,
        "one line per paused folder: {:?}",
        counts.lines()
    );
    assert!(counts.lines()[0].contains(APP), "{:?}", counts.lines());

    // A pause that stands: everything under the folder is left out, started when
    // it may.
    pause(&ledger, APP, "2026-10-09T00:00:00Z");
    let gate = PauseGate::read(&ledger).expect("gate");
    let (kept, counts) = capture_pause::filter_files(&gate, all);
    assert_eq!(counts.paused_by_you, 4, "a, b, c and f are under /work/app");
    assert_eq!(counts.started_while_paused, 0);
    assert_eq!(kept.len(), 2, "/elsewhere and /work/appx stay");
}

#[test]
fn a_claude_folder_that_states_no_cwd_is_taken_by_its_name_as_the_hq_load_takes_it() {
    let tree = TempTree::new();
    let mangled = thruline_core::speaker::mangle_root(APP);
    let named = write(&tree.join(&mangled), "x.jsonl", "{\"type\":\"summary\"}\n");
    let other = write(
        &tree.join("-some-other"),
        "y.jsonl",
        "{\"type\":\"summary\"}\n",
    );
    let ledger = Ledger::open_in_memory().expect("ledger");
    pause(&ledger, APP, "2026-10-02T00:00:00Z");
    let gate = PauseGate::read(&ledger).expect("gate");
    assert_eq!(
        gate.judge_file(&named),
        Verdict::PausedByYou {
            folder: APP.to_string()
        }
    );
    assert_eq!(gate.judge_file(&other), Verdict::Read);
}

#[test]
fn a_row_is_marked_inside_a_pause_and_only_there() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    pause(&ledger, APP, "2026-10-01T09:04:30Z");
    resume(&ledger, APP, "2026-10-01T09:08:00Z");
    pause(&ledger, "/other", "2026-10-01T00:00:00Z");
    let gate = PauseGate::read(&ledger).expect("gate");
    let marks = gate.marks_for(APP);
    assert!(!marks.is_empty());
    assert!(marks.marked(Some("2026-10-01T09:05:00.000Z")));
    assert!(
        marks.marked(Some("2026-10-01T09:04:30.000Z")),
        "the start is inside"
    );
    assert!(
        !marks.marked(Some("2026-10-01T09:08:00.000Z")),
        "the end is outside"
    );
    assert!(!marks.marked(Some("2026-10-01T09:04:29.999Z")));
    assert!(!marks.marked(Some("2026-10-01T09:30:00.000Z")));
    assert!(!marks.marked(None), "a row with no time is not marked");
    assert!(!marks.marked(Some("not a time")));
    assert!(
        gate.marks_for("/unpaused").is_empty(),
        "a project nothing was ever paused for has no marks"
    );
    // An open pause marks everything after it began.
    let other = gate.marks_for("/other");
    assert!(other.marked(Some("2030-01-01T00:00:00Z")));
}

#[test]
fn a_read_error_is_never_a_skip_and_a_payload_that_names_no_folder_is_not_paused() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    pause(&ledger, APP, "2026-10-02T00:00:00Z");
    let payload = |cwd: &str| serde_json::json!({"cwd": cwd, "session_id": "s"}).to_string();

    assert_eq!(
        capture_pause::paused_delivery(&ledger, payload("/work/app/src").as_bytes()),
        Some(APP.to_string())
    );
    assert_eq!(
        capture_pause::paused_delivery(&ledger, payload("/work/appx").as_bytes()),
        None
    );
    assert_eq!(
        capture_pause::paused_delivery(&ledger, b"{never json"),
        None
    );
    assert_eq!(capture_pause::paused_delivery(&ledger, b"{}"), None);
    assert_eq!(capture_pause::paused_delivery(&ledger, b""), None);

    // The record cannot be read: the table is gone under an open ledger.
    ledger
        .conn()
        .execute_batch("DROP TABLE capture_holds")
        .expect("drop");
    assert!(PauseGate::read(&ledger).is_err(), "the read fails");
    assert_eq!(
        capture_pause::paused_delivery(&ledger, payload("/work/app/src").as_bytes()),
        None,
        "and a delivery is recorded rather than lost to a failed read"
    );
    assert_eq!(
        capture_pause::paused_session(&ledger, Some(APP), None),
        None
    );
}

#[test]
fn a_session_that_started_in_a_lifted_pause_is_a_skip_for_a_delivery_too() {
    let tree = TempTree::new();
    let during = write(
        &tree.join("p"),
        "s.jsonl",
        &claude_line(APP, "2026-10-02T10:00:00Z"),
    );
    let before = write(
        &tree.join("p"),
        "t.jsonl",
        &claude_line(APP, "2026-10-01T10:00:00Z"),
    );
    let ledger = Ledger::open_in_memory().expect("ledger");
    pause(&ledger, APP, "2026-10-02T00:00:00Z");
    resume(&ledger, APP, "2026-10-03T00:00:00Z");
    assert_eq!(
        capture_pause::paused_session(&ledger, Some(APP), during.to_str()),
        Some(APP.to_string())
    );
    assert_eq!(
        capture_pause::paused_session(&ledger, Some(APP), before.to_str()),
        None
    );
    assert_eq!(
        capture_pause::paused_session(&ledger, Some(APP), None),
        None,
        "no transcript path, no start to judge"
    );
    assert_eq!(
        capture_pause::paused_session(&ledger, None, during.to_str()),
        None,
        "no folder, no pause"
    );
}

#[test]
fn the_hook_and_the_audit_count_their_skips_beside_the_ledger_per_folder() {
    let tree = TempTree::new();
    let db = tree.join("ledger.db");
    assert_eq!(
        capture_pause::skip_count_for(&db, APP),
        Default::default(),
        "nothing counted yet"
    );
    capture_pause::note_skip(&db, SkipGate::HookDelivery, APP, "2026-10-08T09:00:00Z");
    capture_pause::note_skip(&db, SkipGate::HookDelivery, APP, "2026-10-08T09:00:01Z");
    capture_pause::note_skip(&db, SkipGate::Audit, APP, "2026-10-08T09:00:02Z");
    capture_pause::note_skip(
        &db,
        SkipGate::HookDelivery,
        "/other",
        "2026-10-08T09:00:03Z",
    );
    // A line that is not ours is ignored.
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(capture_pause::skip_log_path(&db))
        .expect("open");
    writeln!(f, "not json").expect("write");
    writeln!(f, "{{\"gate\":\"mystery\",\"folder\":\"{APP}\"}}").expect("write");
    let count = capture_pause::skip_count_for(&db, APP);
    assert_eq!(count.hook_deliveries, 2);
    assert_eq!(count.audits, 1);
    assert_eq!(count.last_at.as_deref(), Some("2026-10-08T09:00:02Z"));
    assert_eq!(
        capture_pause::skip_count_for(&db, "/other").hook_deliveries,
        1
    );
    assert_eq!(
        std::fs::read_to_string(capture_pause::skip_log_path(&db))
            .expect("read")
            .matches("session")
            .count(),
        0,
        "a skip line names the folder and the time, never a session or a word of the work"
    );
}

/* ───────────────────── a load plan, and a thread view ───────────────────── */

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hq")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for e in std::fs::read_dir(from).expect("read") {
        let e = e.expect("entry");
        std::fs::copy(e.path(), to.join(e.file_name())).expect("copy");
    }
}

#[test]
fn a_load_plan_is_cut_by_the_pause_and_its_bytes_are_those_of_what_is_left() {
    let tree = TempTree::new();
    let projects = tree.join("claude/projects");
    copy_dir(
        &fixtures().join("claude/-work-app"),
        &projects.join("-work-app"),
    );
    let codex = tree.join("codex");
    copy_dir(&fixtures().join("codex"), &codex);
    let plan = thruline_core::hq::load_plan(APP, &projects, std::slice::from_ref(&codex));
    assert_eq!(plan.claude_code.len(), 2);
    assert_eq!(plan.codex.len(), 1);
    assert_eq!((plan.paused_by_you, plan.started_while_paused), (0, 0));

    // No pause in the record: the plan is returned as it was found.
    let ledger = Ledger::open_in_memory().expect("ledger");
    let same =
        thruline_core::hq::apply_pause_gate(plan.clone(), &PauseGate::read(&ledger).expect("gate"));
    assert_eq!(same.claude_code, plan.claude_code);
    assert_eq!(same.codex, plan.codex);
    assert_eq!(same.bytes, plan.bytes);
    assert_eq!((same.paused_by_you, same.started_while_paused), (0, 0));

    // A pause that was lifted: the second Claude Code session (2026-10-02) and
    // nothing else started inside it.
    pause(&ledger, APP, "2026-10-02T00:00:00Z");
    resume(&ledger, APP, "2026-10-03T00:00:00Z");
    let cut =
        thruline_core::hq::apply_pause_gate(plan.clone(), &PauseGate::read(&ledger).expect("gate"));
    assert_eq!(cut.claude_code.len(), 1);
    assert_eq!(cut.codex.len(), 1);
    assert_eq!((cut.paused_by_you, cut.started_while_paused), (0, 1));
    let left: u64 = cut
        .claude_code
        .iter()
        .chain(cut.codex.iter())
        .map(|p| std::fs::metadata(p).expect("meta").len())
        .sum();
    assert_eq!(cut.bytes, left);
    assert!(cut.bytes < plan.bytes);

    // A pause that stands: all three are under the folder.
    pause(&ledger, APP, "2026-10-09T00:00:00Z");
    let none = thruline_core::hq::apply_pause_gate(plan, &PauseGate::read(&ledger).expect("gate"));
    assert!(none.claude_code.is_empty() && none.codex.is_empty());
    assert_eq!((none.paused_by_you, none.started_while_paused), (3, 0));
    assert_eq!(none.bytes, 0);
}

fn thread_views(ledger: &Ledger) -> Vec<Value> {
    let now = ms("2026-10-07T00:00:00Z");
    let id = thruline_core::hq::project_id_for_root(APP);
    let view = thruline_core::hq::project(ledger, &id, now)
        .expect("project")
        .expect("exists");
    view.threads
        .iter()
        .map(|t| {
            let v = thruline_core::hq::thread(ledger, &id, &t.id, now)
                .expect("thread")
                .expect("view");
            serde_json::to_value(v).expect("json")
        })
        .collect()
}

/// Every served item of every thread view that carries the mark, by its time.
fn marked(views: &[Value]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for v in views {
        for list in ["prompts", "edits", "commands", "claims"] {
            for item in v[list].as_array().into_iter().flatten() {
                if let Some(m) = item.get("recorded_while_paused") {
                    assert_eq!(m, &Value::Bool(true), "absent, never false: {item}");
                    out.push((
                        list.to_string(),
                        item["ts"].as_str().unwrap_or_default().to_string(),
                    ));
                }
            }
        }
    }
    out
}

#[test]
fn a_thread_view_marks_a_row_recorded_inside_a_pause_by_a_join_and_serves_nothing_else_differently()
{
    let tree = TempTree::new();
    let db = tree.join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    thruline_core::ingest::ingest(
        &mut ledger,
        &[fixtures().join("claude/-work-app")],
        &tree.join("mirror"),
    )
    .expect("ingest");
    // No pause ever recorded: no row carries the field, and the view is exactly
    // what it was before the mark existed.
    let plain = thread_views(&ledger);
    assert!(marked(&plain).is_empty());
    assert!(
        !serde_json::to_string(&plain)
            .expect("json")
            .contains("recorded_while_paused"),
        "a project nobody paused serves the bytes it always served"
    );

    // A pause that began after the first answer (09:02:20) and ended before the
    // README prompt (09:10:00): the "Why was it panicking?" prompt (09:05:00)
    // and the answer after it (09:05:10) were recorded inside it.
    pause(&ledger, APP, "2026-10-01T09:04:30Z");
    resume(&ledger, APP, "2026-10-01T09:08:00Z");
    let before_rows: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .expect("count");
    let after = thread_views(&ledger);
    let marks = marked(&after);
    assert!(
        marks
            .iter()
            .any(|(list, ts)| list == "prompts" && ts == "2026-10-01T09:05:00.000Z"),
        "the prompt typed inside the pause is marked: {marks:?}"
    );
    for (_, ts) in &marks {
        let t = ms(ts);
        assert!(
            t >= ms("2026-10-01T09:04:30Z") && t < ms("2026-10-01T09:08:00Z"),
            "only rows timestamped inside the pause are marked: {ts}"
        );
    }
    // Every prompt outside it is served without the field.
    for v in &after {
        for p in v["prompts"].as_array().into_iter().flatten() {
            let inside = ms(p["ts"].as_str().expect("ts")) >= ms("2026-10-01T09:04:30Z")
                && ms(p["ts"].as_str().expect("ts")) < ms("2026-10-01T09:08:00Z");
            assert_eq!(p.get("recorded_while_paused").is_some(), inside, "{p}");
        }
    }
    // A join: nothing was written to the events, and the rest of each view is
    // what it was.
    let after_rows: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .expect("count");
    assert_eq!(before_rows, after_rows);
    let strip = |views: &[Value]| -> String {
        let mut s = serde_json::to_string(views).expect("json");
        s = s.replace(",\"recorded_while_paused\":true", "");
        s
    };
    assert_eq!(
        strip(&after),
        strip(&plain),
        "the mark is the only difference"
    );
}

#[test]
fn the_skip_log_is_bounded_by_one_rotation_and_the_count_reads_both_files() {
    let tree = TempTree::new();
    let db = tree.join("ledger.db");
    let log = capture_pause::skip_log_path(&db);
    let old = capture_pause::skip_log_rotation_path(&db);
    // A log at the bound: the next skip rotates it, so at most two files exist.
    let filler = format!(
        "{{\"at\":\"2026-10-01T00:00:00Z\",\"gate\":\"hook_delivery\",\"folder\":\"{APP}\"}}\n"
    );
    let lines = (capture_pause::SKIP_LOG_MAX_BYTES as usize / filler.len()) + 1;
    std::fs::write(&log, filler.repeat(lines)).unwrap();
    assert!(std::fs::metadata(&log).unwrap().len() >= capture_pause::SKIP_LOG_MAX_BYTES);
    capture_pause::note_skip(&db, SkipGate::Audit, APP, "2026-10-08T09:00:00Z");
    assert!(old.exists(), "the full log became the one rotation");
    assert!(
        std::fs::metadata(&log).unwrap().len() < 1024,
        "counting started a new file"
    );
    // The count covers both, newest last.
    let counted = capture_pause::skip_count_for(&db, APP);
    assert_eq!(counted.hook_deliveries, lines as u64);
    assert_eq!(counted.audits, 1);
    assert_eq!(counted.last_at.as_deref(), Some("2026-10-08T09:00:00Z"));
    // A second rotation replaces the first: never more than two files.
    std::fs::write(&log, filler.repeat(lines)).unwrap();
    capture_pause::note_skip(&db, SkipGate::Audit, APP, "2026-10-08T09:00:01Z");
    let files = std::fs::read_dir(tree.join("."))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("capture-pause-skips")
        })
        .count();
    assert_eq!(files, 2, "at most the log and one rotation are kept");
}
