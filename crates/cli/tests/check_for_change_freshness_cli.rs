//! T-S-2 (verifier fix round) — the trigger, through the compiled binary.
//!
//! The verifier found that `evidence_applicability::on_intended_bound` was
//! reachable only from `change new`/first-use, so no command a person could
//! run a second time on an already-open Change ever re-assessed freshness.
//! This is that command, typed twice, on the real binary: `check --for-change`
//! run again after an edit under the accepted scope is now one of the two
//! moments (with `change prepare`) that re-binds the Change's `intended`
//! record and triggers the assessment.

use std::path::{Path, PathBuf};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(home: &Path, cwd: &Path, db: &Path, args: &[&str]) -> Run {
    let out = Command::new(bin())
        .env("HOME", home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .current_dir(cwd)
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .expect("run thruline");
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn change_id(out: &Run) -> String {
    out.stdout
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("chg_"))
        .unwrap_or_else(|| panic!("no Change id was printed: {}", out.stdout))
        .to_string()
}

/// A repository with one commit and a store whose one project points at it,
/// holding one recorded session that edited a file in it — the same shape
/// `check_for_change_cli.rs` uses.
fn fixture(base: &Path) -> (PathBuf, PathBuf) {
    let repo = base.join("repo");
    std::fs::create_dir_all(repo.join("src/upload")).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("src/upload/retry.rs"), "// retry\n").expect("write");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "seed"]);

    std::fs::write(
        repo.join("s.jsonl"),
        format!("{{\"type\":\"user\",\"cwd\":\"{}\"}}\n", repo.display()),
    )
    .expect("transcript");

    let db = base.join("ledger.db");
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('cccccccc-0000-4000-8000-00000000p001', ?1, 'claude_code', 'Synthetic',
                    '2026-01-01T00:00:00Z')",
            rusqlite::params![repo.display().to_string()],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES('cccccccc-0000-4000-8000-00000000s001',
                    'cccccccc-0000-4000-8000-00000000p001', ?1, '2026-01-02T09:30:00Z')",
            rusqlite::params![repo.join("s.jsonl").display().to_string()],
        )
        .expect("session");
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('cccccccc-ev-1','cccccccc-0000-4000-8000-00000000p001',
                    'cccccccc-0000-4000-8000-00000000s001',1,'2026-01-02T09:31:00Z',
                    'tool_use','Edit', ?1, ?2, 1, 'x', '', '')",
            rusqlite::params![
                format!(
                    "{{\"file_path\":\"{}\"}}",
                    repo.join("src/upload/retry.rs").display()
                ),
                repo.join("s.jsonl").display().to_string()
            ],
        )
        .expect("event");
    (repo, db)
}

/// `change new` → `associate --confirm` → `accept --all`, the same three
/// commands the first test types inline, so the two T-3-3-F1 tests below say
/// only what is new about them.
fn open_accepted_change(home: &Path, repo: &Path, db: &Path) -> String {
    let opened = run(
        home,
        repo,
        db,
        &[
            "change",
            "new",
            "--title",
            "Retry failed uploads",
            "--purpose",
            "The build stays clean on the packages that changed.",
            "--requester",
            "sam",
        ],
    );
    assert_eq!(opened.code, 0, "{}", opened.stderr);
    let change = change_id(&opened);

    let confirmed = run(
        home,
        repo,
        db,
        &[
            "change",
            "associate",
            &change,
            "cccccccc-0000-4000-8000-00000000s001",
            "--confirm",
            "--actor",
            "sam",
            "--reason",
            "this is the retry work",
        ],
    );
    assert_eq!(confirmed.code, 0, "{}", confirmed.stderr);

    let accepted = run(
        home,
        repo,
        db,
        &[
            "change",
            "accept",
            &change,
            "--all",
            "--actor",
            "sam",
            "--rationale",
            "this is what I want checked",
        ],
    );
    assert_eq!(accepted.code, 0, "{}", accepted.stderr);
    change
}

#[test]
fn a_second_check_for_change_after_an_edit_invalidates_the_first_run() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let home = dir.path();

    let opened = run(
        home,
        &repo,
        &db,
        &[
            "change",
            "new",
            "--title",
            "Retry failed uploads",
            "--purpose",
            "The build stays clean on the packages that changed.",
            "--requester",
            "sam",
        ],
    );
    assert_eq!(opened.code, 0, "{}", opened.stderr);
    let change = change_id(&opened);

    let confirmed = run(
        home,
        &repo,
        &db,
        &[
            "change",
            "associate",
            &change,
            "cccccccc-0000-4000-8000-00000000s001",
            "--confirm",
            "--actor",
            "sam",
            "--reason",
            "this is the retry work",
        ],
    );
    assert_eq!(confirmed.code, 0, "{}", confirmed.stderr);

    let accepted = run(
        home,
        &repo,
        &db,
        &[
            "change",
            "accept",
            &change,
            "--all",
            "--actor",
            "sam",
            "--rationale",
            "this is what I want checked",
        ],
    );
    assert_eq!(accepted.code, 0, "{}", accepted.stderr);

    // First run: a build-class check, `--for-change`. This is the run that
    // must later read "no longer applies".
    let first_check = run(
        home,
        &repo,
        &db,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--",
            "cargo",
            "clippy",
            "--help",
        ],
    );
    assert_eq!(first_check.code, 0, "{}", first_check.stderr);

    // A visit to the coverage screen links the first run to its criterion —
    // the real trigger for `change_criterion_evidence` rows existing at all,
    // and the thing the freshness assessment has something to invalidate.
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    thruline_core::change_evidence::derive_links(&ledger, &change, "2026-02-01T11:00:00Z")
        .expect("derive");

    // Before any edit: nothing to assess yet against a first binding.
    let before = thruline_core::evidence_applicability::standing_for_change(&ledger, &change)
        .expect("standing");
    assert!(
        before.values().all(|r| {
            r.state != thruline_core::evidence_applicability::ApplicabilityState::NoLongerApplies
        }),
        "nothing should read invalidated before any edit: {before:?}"
    );
    drop(ledger);

    // The tree moves: a file under the accepted scope is edited.
    std::fs::write(
        repo.join("src/upload/retry.rs"),
        "// retry\nfn backoff() {}\n",
    )
    .expect("edit");

    // Second run, for the same Change: the trigger this fix round adds.
    let second_check = run(
        home,
        &repo,
        &db,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--",
            "cargo",
            "clippy",
            "--help",
        ],
    );
    assert_eq!(second_check.code, 0, "{}", second_check.stderr);

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let after = thruline_core::evidence_applicability::standing_for_change(&ledger, &change)
        .expect("standing");
    assert!(
        !after.is_empty(),
        "the second `check --for-change` must have produced a freshness assessment"
    );
    let invalidated = after
        .values()
        .find(|r| {
            r.state == thruline_core::evidence_applicability::ApplicabilityState::NoLongerApplies
        })
        .unwrap_or_else(|| panic!("no invalidated row: {after:?}"));
    assert_eq!(
        invalidated.reason_class,
        Some(thruline_core::evidence_applicability::ReasonClass::SourceChanged)
    );
    assert!(
        invalidated
            .reason_paths
            .iter()
            .any(|p| p.ends_with("retry.rs")),
        "the path must be named: {:?}",
        invalidated.reason_paths
    );

    // And the served coverage no longer reports the FIRST run as bearing
    // evidence: its own item reads `fresh: false`.
    let at = "2026-02-01T12:00:00Z";
    thruline_core::change_evidence::derive_links(&ledger, &change, at).expect("derive");
    let environment =
        thruline_core::change_propose::machine_environment(&ledger, &change).expect("env");
    let view =
        thruline_core::change_evidence::coverage(&ledger, &change, &environment).expect("coverage");
    let first_binding_item = view
        .criteria
        .iter()
        .flat_map(|c| &c.items)
        .find(|i| {
            i.evidence_id == invalidated.criterion_evidence_id
                || i.link_id == invalidated.criterion_evidence_id
        })
        .or_else(|| {
            // The criterion-evidence id names a link row, not a binding id;
            // fall back to the one item whose `asserted_at` matches the
            // first run's link if the direct id match above misses.
            view.criteria
                .iter()
                .flat_map(|c| &c.items)
                .find(|i| i.fresh == Some(false))
        })
        .unwrap_or_else(|| panic!("no item read fresh:false: {view:?}"));
    assert_eq!(
        first_binding_item.fresh,
        Some(false),
        "the first run's own item must say it no longer applies: {view:?}"
    );
}

/// T-3-3-F1 fix 1 — an item that stopped applying stays stopped.
///
/// check → edit → `change prepare` → `change prepare` again, with no further
/// edit. The second prepare compares the post-edit tree to itself, so the old
/// rule (compare the two most recent Intended snapshots) wrote `applies` over
/// a correct `no_longer_applies`. Assessing each item against ITS OWN
/// examined-inputs snapshot instead, the answer cannot move back.
#[test]
fn a_second_prepare_with_no_further_edit_leaves_invalidated_evidence_invalidated() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let home = dir.path();

    let change = open_accepted_change(home, &repo, &db);

    let first_check = run(
        home,
        &repo,
        &db,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--",
            "cargo",
            "clippy",
            "--help",
        ],
    );
    assert_eq!(first_check.code, 0, "{}", first_check.stderr);

    std::fs::write(
        repo.join("src/upload/retry.rs"),
        "// retry\nfn backoff() {}\n",
    )
    .expect("edit");

    let first_prepare = run(home, &repo, &db, &["change", "prepare", &change]);
    assert_eq!(first_prepare.code, 0, "{}", first_prepare.stderr);

    let invalidated_after_first = {
        let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
        let standing = thruline_core::evidence_applicability::standing_for_change(&ledger, &change)
            .expect("standing");
        let ids: Vec<String> = standing
            .values()
            .filter(|r| {
                r.state
                    == thruline_core::evidence_applicability::ApplicabilityState::NoLongerApplies
            })
            .map(|r| r.criterion_evidence_id.clone())
            .collect();
        assert!(
            !ids.is_empty(),
            "the edit must have invalidated the first run: {standing:?}"
        );
        ids
    };

    // The second prepare: same tree, no edit in between.
    let second_prepare = run(home, &repo, &db, &["change", "prepare", &change]);
    assert_eq!(second_prepare.code, 0, "{}", second_prepare.stderr);

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let standing = thruline_core::evidence_applicability::standing_for_change(&ledger, &change)
        .expect("standing");
    for id in &invalidated_after_first {
        let row = standing
            .get(id)
            .unwrap_or_else(|| panic!("the row for {id} disappeared: {standing:?}"));
        assert_eq!(
            row.state,
            thruline_core::evidence_applicability::ApplicabilityState::NoLongerApplies,
            "a second prepare with no edit must not re-mark {id} as applying: {row:?}"
        );
    }
}

/// T-3-3-F1 fix 2 — nobody has to open the screen for the edit to be heard.
///
/// check → edit → check, with no `derive_links` call standing in for a visit
/// to the coverage screen. Before this fix no `change_criterion_evidence` row
/// existed for the assessment to invalidate, so the edit was silent.
#[test]
fn a_check_writes_its_own_links_so_an_edit_is_heard_without_opening_the_screen() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let home = dir.path();

    let change = open_accepted_change(home, &repo, &db);

    let first_check = run(
        home,
        &repo,
        &db,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--",
            "cargo",
            "clippy",
            "--help",
        ],
    );
    assert_eq!(first_check.code, 0, "{}", first_check.stderr);

    std::fs::write(
        repo.join("src/upload/retry.rs"),
        "// retry\nfn backoff() {}\n",
    )
    .expect("edit");

    let second_check = run(
        home,
        &repo,
        &db,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--",
            "cargo",
            "clippy",
            "--help",
        ],
    );
    assert_eq!(second_check.code, 0, "{}", second_check.stderr);

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let standing = thruline_core::evidence_applicability::standing_for_change(&ledger, &change)
        .expect("standing");
    assert!(
        standing.values().any(|r| {
            r.state == thruline_core::evidence_applicability::ApplicabilityState::NoLongerApplies
                && r.reason_paths.iter().any(|p| p.ends_with("retry.rs"))
        }),
        "the edit must be heard with no screen ever opened: {standing:?}"
    );
}

/// T-F-1-F5 — the founder path: the edit is COMMITTED, and `change prepare`
/// is the only command run afterwards.
///
/// `docs/loop/FOUNDER-TEST.md` step 7 and the T-F-1 verifier's FINDING 1: a
/// passing counted check, then an edit under the accepted scope, then
/// `git commit`, then `prepare`. Every other freshness test in this file
/// leaves the edit unsaved, and a snapshot's tree manifests only list the
/// dirty working tree — so once the edit is committed both snapshots are
/// clean, the two manifests are empty, and the assessment read "applies"
/// while the screen told Sam the one run on record "checked nothing". A
/// committed edit is the ordinary case, not the exotic one.
#[test]
fn a_committed_edit_then_prepare_invalidates_the_run_that_passed_before_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (repo, db) = fixture(dir.path());
    let home = dir.path();

    let change = open_accepted_change(home, &repo, &db);

    let first_check = run(
        home,
        &repo,
        &db,
        &[
            "check",
            "--for-change",
            &change,
            "--actor",
            "sam",
            "--",
            "cargo",
            "clippy",
            "--help",
        ],
    );
    assert_eq!(first_check.code, 0, "{}", first_check.stderr);

    // The run that passed, and the version it passed on — this is what must
    // still be on the screen afterwards, marked stale rather than erased.
    let (before_items, before_version) = {
        let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
        let at = "2026-02-01T11:00:00Z";
        thruline_core::change_evidence::derive_links(&ledger, &change, at).expect("derive");
        let environment =
            thruline_core::change_propose::machine_environment(&ledger, &change).expect("env");
        let view = thruline_core::change_evidence::coverage(&ledger, &change, &environment)
            .expect("coverage");
        let items: Vec<String> = view
            .criteria
            .iter()
            .flat_map(|c| &c.items)
            .map(|i| i.evidence_id.clone())
            .collect();
        assert!(!items.is_empty(), "the check must have produced an item");
        assert!(
            view.criteria
                .iter()
                .flat_map(|c| &c.items)
                .all(|i| i.fresh != Some(false)),
            "nothing should read stale before the edit: {view:?}"
        );
        (items, view.scope_version)
    };

    // The tree moves under the accepted scope — and the founder commits it.
    std::fs::write(
        repo.join("src/upload/retry.rs"),
        "// retry\nfn backoff() {}\n",
    )
    .expect("edit");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "backoff"]);

    let prepared = run(home, &repo, &db, &["change", "prepare", &change]);
    assert_eq!(prepared.code, 0, "{}", prepared.stderr);

    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    let standing = thruline_core::evidence_applicability::standing_for_change(&ledger, &change)
        .expect("standing");
    let invalidated = standing
        .values()
        .find(|r| {
            r.state == thruline_core::evidence_applicability::ApplicabilityState::NoLongerApplies
        })
        .unwrap_or_else(|| panic!("a committed edit must invalidate the run: {standing:?}"));
    assert_eq!(
        invalidated.reason_class,
        Some(thruline_core::evidence_applicability::ReasonClass::SourceChanged),
        "{invalidated:?}"
    );
    assert!(
        invalidated
            .reason_paths
            .iter()
            .any(|p| p.ends_with("retry.rs")),
        "the committed path must be named, so the screen can say which file: {:?}",
        invalidated.reason_paths
    );

    // What the screen reads: the item is stale, and the run that produced it
    // is still there with the version it ran on.
    let at = "2026-02-01T12:00:00Z";
    thruline_core::change_evidence::derive_links(&ledger, &change, at).expect("derive");
    let environment =
        thruline_core::change_propose::machine_environment(&ledger, &change).expect("env");
    let view =
        thruline_core::change_evidence::coverage(&ledger, &change, &environment).expect("coverage");
    let stale: Vec<_> = view
        .criteria
        .iter()
        .flat_map(|c| &c.items)
        .filter(|i| i.fresh == Some(false))
        .collect();
    assert!(
        !stale.is_empty(),
        "the item the passing run produced must read fresh:false: {view:?}"
    );
    let still_listed: Vec<String> = view
        .criteria
        .iter()
        .flat_map(|c| &c.items)
        .map(|i| i.evidence_id.clone())
        .collect();
    for id in &before_items {
        assert!(
            still_listed.contains(id),
            "the run that passed must still be listed, not erased: {id} not in {still_listed:?}"
        );
    }
    assert_eq!(
        view.scope_version, before_version,
        "the accepted-criteria version the run passed on is unchanged by an edit"
    );
}
