//! L9.6 — the persisted chain position.
//!
//! The packet's binding is its chain position, and reading it used to fold
//! every event in the project. On the live corpus that was 719 ms for
//! the largest project — the entire read budget spent before a line is compiled.
//!
//! A cache on a hash-chained ledger is only allowed if it can be proven equal
//! to the thing it caches. That is what this file is: every test here compares
//! the persisted head against `fold_project_head_from_events`, which is never
//! cached and never skipped.

mod scratch;

use std::fs;

use scratch::TempTree;
use thruline_core::ingest::{ingest, ingest_codex, CodexIngestOptions};
use thruline_core::ledger::HeadPath;
use thruline_core::Ledger;

/// Two Claude sessions in one project, plus a second project, so the fold has
/// more than one session to order and more than one project to keep apart.
fn claude_corpus(dir: &std::path::Path, project: &str, sessions: &[(&str, usize)]) {
    let pdir = dir.join(project);
    fs::create_dir_all(&pdir).expect("mkdir");
    for (session, lines) in sessions {
        let mut body = String::new();
        for i in 0..*lines {
            body.push_str(&format!(
                r#"{{"type":"user","uuid":"{session}-{i:04}","sessionId":"{session}","timestamp":"2030-01-01T00:00:0{}.000Z","parentUuid":null,"isSidechain":false,"cwd":"/x/y","message":{{"role":"user","content":"line {i}"}}}}"#,
                i % 10
            ));
            body.push('\n');
        }
        fs::write(pdir.join(format!("{session}.jsonl")), body).expect("write");
    }
}

fn built() -> (TempTree, Ledger) {
    let tmp = TempTree::new();
    let root = tmp.path().join("projects");
    claude_corpus(&root, "-proj-one", &[("aaaa1111", 5), ("bbbb2222", 3)]);
    claude_corpus(&root, "-proj-two", &[("cccc3333", 4)]);
    let mut ledger = Ledger::open(tmp.path().join("ledger.db")).expect("open");
    ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &tmp.path().join("mirrors"),
    )
    .expect("ingest");
    (tmp, ledger)
}

fn projects(ledger: &Ledger) -> Vec<String> {
    let mut stmt = ledger
        .conn()
        .prepare("SELECT DISTINCT project_id FROM events ORDER BY project_id")
        .expect("prepare");
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))
        .expect("query")
        .collect::<std::result::Result<Vec<_>, _>>()
        .expect("collect");
    rows
}

/// THE INVARIANT. Everything else in this file is a way of stressing it.
#[test]
fn the_persisted_head_equals_the_fold_over_events() {
    let (_tmp, ledger) = built();
    let ids = projects(&ledger);
    assert!(ids.len() >= 2, "need more than one project: {ids:?}");
    for project_id in &ids {
        let truth = ledger
            .fold_project_head_from_events(project_id)
            .expect("fold");
        let (served, path) = ledger.project_head_with_path(project_id).expect("head");
        assert_eq!(
            path,
            HeadPath::Persisted,
            "ingest must leave a persisted head for {project_id}"
        );
        assert_eq!(
            served, truth,
            "the cache disagrees with the fold for {project_id} — the cache is a lie"
        );
    }
}

/// Appending again must move the head, and must move it to the RIGHT value.
/// A cache that is merely self-consistent would pass a weaker test than this.
#[test]
fn a_second_ingest_moves_the_head_and_keeps_it_equal_to_the_fold() {
    let tmp = TempTree::new();
    let root = tmp.path().join("projects");
    claude_corpus(&root, "-proj-one", &[("aaaa1111", 3)]);
    let mut ledger = Ledger::open(tmp.path().join("ledger.db")).expect("open");
    ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &tmp.path().join("mirrors"),
    )
    .expect("first");
    let project_id = projects(&ledger).first().cloned().expect("a project");
    let before = ledger.compute_project_head(&project_id).expect("before");

    // A new session in the same project, and more lines in the existing one.
    claude_corpus(&root, "-proj-one", &[("aaaa1111", 6), ("dddd4444", 2)]);
    ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &tmp.path().join("mirrors"),
    )
    .expect("second");

    let after = ledger.compute_project_head(&project_id).expect("after");
    let truth = ledger
        .fold_project_head_from_events(&project_id)
        .expect("fold");
    assert_ne!(before, after, "appending must move the chain position");
    assert_eq!(after, truth, "and it must move it to the folded value");
    assert!(after.1 > before.1, "the event total must grow");
}

/// Derived state must be rebuildable byte-identically — the same rule the
/// project already applies to `nodes`/`edges`. Corrupt the cache on purpose and
/// prove the rebuild restores exactly the folded value.
#[test]
fn a_corrupted_cache_is_restored_byte_identically_by_rebuild() {
    let (_tmp, ledger) = built();
    let ids = projects(&ledger);
    let truth: Vec<(String, u64)> = ids
        .iter()
        .map(|p| ledger.fold_project_head_from_events(p).expect("fold"))
        .collect();

    ledger
        .conn()
        .execute_batch(
            "UPDATE project_heads SET digest='0000000000000000000000000000000000000000000000000000000000000000', event_total=0;
             UPDATE sessions SET head_chain_hash=NULL, event_count=0;",
        )
        .expect("corrupt");
    for (project_id, was) in ids.iter().zip(truth.iter()) {
        assert_ne!(
            ledger.compute_project_head(project_id).expect("read"),
            *was,
            "the corruption must actually be visible, or this test proves nothing"
        );
    }

    let report = ledger.rebuild_heads().expect("rebuild");
    assert!(
        report.mismatches.is_empty(),
        "rebuild disagreed with the fold for: {:?}",
        report.mismatches
    );
    assert_eq!(report.projects, ids.len());
    for (project_id, was) in ids.iter().zip(truth.iter()) {
        assert_eq!(
            ledger.compute_project_head(project_id).expect("read"),
            *was,
            "rebuild must restore the exact prior value for {project_id}"
        );
    }
}

/// A ledger with no persisted head is CORRECT, just slow. This is the
/// migration path for every ledger that exists today, so it is pinned: the
/// fallback must return the same answer and must admit which path it took.
#[test]
fn an_unbuilt_cache_falls_back_to_the_fold_and_says_so() {
    let (_tmp, ledger) = built();
    let project_id = projects(&ledger).first().cloned().expect("a project");
    let truth = ledger
        .fold_project_head_from_events(&project_id)
        .expect("fold");

    ledger
        .conn()
        .execute("DELETE FROM project_heads", [])
        .expect("clear");

    let (served, path) = ledger.project_head_with_path(&project_id).expect("head");
    assert_eq!(
        path,
        HeadPath::FoldedOverEvents,
        "it must admit the slow path"
    );
    assert_eq!(served, truth, "and still be right");
}

/// The codex append path is a different transaction with different machinery.
/// It maintains the head too, or the cache is right for one vendor only.
#[test]
fn the_codex_append_path_maintains_the_head_as_well() {
    let tmp = TempTree::new();
    let body = concat!(
        r#"{"timestamp":"2030-01-01T00:00:00Z","type":"session_meta","payload":{"id":"cx-1","cwd":"/x/y"}}"#,
        "\n",
        r#"{"timestamp":"2030-01-01T00:00:01Z","type":"turn_context","payload":{"turn_id":"t1","cwd":"/x/y","branch":"main"}}"#,
        "\n",
        r#"{"timestamp":"2030-01-01T00:00:02Z","type":"response_item","payload":{"type":"message","id":"m1","role":"assistant","content":[{"type":"output_text","text":"Done."}]}}"#,
        "\n"
    );
    let source = tmp.path().join("rollout.jsonl");
    fs::write(&source, body).expect("write");
    let mut ledger = Ledger::open(tmp.path().join("ledger.db")).expect("open");
    ingest_codex(
        &mut ledger,
        std::slice::from_ref(&source),
        &CodexIngestOptions {
            mirror_root: tmp.path().join("mirror"),
            owner_key: Some("o".to_string()),
        },
    )
    .expect("codex ingest");

    for project_id in projects(&ledger) {
        let truth = ledger
            .fold_project_head_from_events(&project_id)
            .expect("fold");
        let (served, path) = ledger.project_head_with_path(&project_id).expect("head");
        assert_eq!(
            path,
            HeadPath::Persisted,
            "codex ingest must persist a head"
        );
        assert_eq!(served, truth, "and it must equal the fold");
    }
}

/// A125 — the migration must POPULATE the heads, not merely add the columns.
///
/// Leaving them NULL is "correct but slow", and slow was measured: `now --all`
/// fell back to the O(events) fold once per project and took 1.4–5.6 s instead
/// of ~104 ms on the live corpus. Nobody would know to run `rebuild --all` to
/// fix a slowness they were never told about, so the upgrade pays it once.
///
/// Simulated the only honest way — by actually dropping the persisted state and
/// reopening, rather than by asserting against a hand-made schema.
#[test]
fn reopening_a_ledger_whose_heads_were_never_populated_backfills_them() {
    let (tmp, ledger) = built();
    let ids = projects(&ledger);
    let truth: Vec<(String, u64)> = ids
        .iter()
        .map(|p| ledger.fold_project_head_from_events(p).expect("fold"))
        .collect();

    let path = tmp.path().join("ledger.db");
    // Reproduce the PRE-A117 SHAPE, which is the only thing that triggers the
    // backfill and the only thing a real upgrade presents: the columns and the
    // table genuinely absent, and the schema fingerprint gone with them. Not a
    // stand-in — after this, the file is byte-for-byte the schema an older
    // build wrote.
    ledger
        .conn()
        .execute_batch(
            "DROP TABLE project_heads;
             ALTER TABLE sessions DROP COLUMN head_chain_hash;
             ALTER TABLE sessions DROP COLUMN event_count;
             DELETE FROM schema_migrations;",
        )
        .expect("reproduce the pre-A117 schema");
    drop(ledger);

    // Reopening through the upgrade door is the whole act under test: no
    // rebuild, no explicit backfill. A193 moved the migration from `open` to
    // `open_upgrading`; the property under test — that the heads arrive without
    // the user running anything but the upgrade — is unchanged.
    let reopened = Ledger::open_upgrading(path, &mut |_, _, _| {}).expect("reopen");
    for (project_id, was) in ids.iter().zip(truth.iter()) {
        let (served, path_used) = reopened
            .project_head_with_path(project_id)
            .expect("head after reopen");
        assert_eq!(
            path_used,
            HeadPath::Persisted,
            "reopening must leave {project_id} on the fast path, with no user action"
        );
        assert_eq!(
            served, *was,
            "and the backfilled head must equal the fold for {project_id}"
        );
    }
}

/// `ledger_meta`'s head, as the acceptance path last wrote it, if it did.
fn meta_head(ledger: &Ledger, project_id: &str) -> Option<(String, u64)> {
    ledger
        .conn()
        .query_row(
            "SELECT head_hash, event_count FROM ledger_meta WHERE project_id = ?1",
            [project_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?.max(0) as u64)),
        )
        .ok()
}

/// Every `ledger_meta` head present equals the authority, the fold over
/// `events`; returns how many projects had one.
fn meta_heads_equal_the_fold(ledger: &Ledger, when: &str) -> usize {
    let mut seen = 0;
    for project_id in projects(ledger) {
        if let Some(meta) = meta_head(ledger, &project_id) {
            let truth = ledger
                .fold_project_head_from_events(&project_id)
                .expect("fold");
            assert_eq!(meta, truth, "{when}: ledger_meta of {project_id}");
            seen += 1;
        }
    }
    seen
}

/// C27: a generation's acceptance writes `ledger_meta` from the session rows
/// it has just brought up to date, instead of re-reading every event of the
/// project (1–2 s and more on a 600k-event project, paid even by a live
/// candidate the final check then refuses). It must still be the authority's
/// value: on a first acceptance and after a growth, for Codex and Claude Code.
#[test]
fn the_head_an_acceptance_writes_to_ledger_meta_equals_the_fold_over_events() {
    // Codex: one rollout, accepted, then grown and accepted again.
    let tmp = TempTree::new();
    let first = concat!(
        r#"{"timestamp":"2030-01-01T00:00:00Z","type":"session_meta","payload":{"id":"cx-1","cwd":"/x/y"}}"#,
        "\n",
        r#"{"timestamp":"2030-01-01T00:00:01Z","type":"turn_context","payload":{"turn_id":"t1","cwd":"/x/y","branch":"main"}}"#,
        "\n",
        r#"{"timestamp":"2030-01-01T00:00:02Z","type":"response_item","payload":{"type":"message","id":"m1","role":"assistant","content":[{"type":"output_text","text":"Done."}]}}"#,
        "\n"
    );
    let grown = concat!(
        r#"{"timestamp":"2030-01-01T00:00:03Z","type":"response_item","payload":{"type":"message","id":"m2","role":"assistant","content":[{"type":"output_text","text":"And this."}]}}"#,
        "\n"
    );
    let source = tmp.path().join("rollout.jsonl");
    fs::write(&source, first).expect("write");
    let mut ledger = Ledger::open(tmp.path().join("ledger.db")).expect("open");
    let options = CodexIngestOptions {
        mirror_root: tmp.path().join("mirror"),
        owner_key: Some("o".to_string()),
    };
    ingest_codex(&mut ledger, std::slice::from_ref(&source), &options).expect("codex ingest");
    assert!(
        meta_heads_equal_the_fold(&ledger, "codex, first acceptance") > 0,
        "a Codex acceptance writes ledger_meta"
    );
    fs::write(&source, format!("{first}{grown}")).expect("grow");
    let report =
        ingest_codex(&mut ledger, std::slice::from_ref(&source), &options).expect("codex again");
    assert!(report.new_events > 0, "the growth was accepted: {report:?}");
    assert!(meta_heads_equal_the_fold(&ledger, "codex, after a growth") > 0);

    // Claude Code: two projects, then one session grown.
    let (tmp, mut ledger) = built();
    meta_heads_equal_the_fold(&ledger, "claude, first acceptance");
    claude_corpus(
        &tmp.path().join("projects"),
        "-proj-one",
        &[("aaaa1111", 9)],
    );
    ingest(
        &mut ledger,
        &[tmp.path().join("projects")],
        &tmp.path().join("mirrors"),
    )
    .expect("re-ingest");
    meta_heads_equal_the_fold(&ledger, "claude, after a growth");
}
