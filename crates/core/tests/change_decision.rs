//! T-S-4 — the decision: approving exactly one version, and taking it back.
//!
//! Sam's question is "what exactly did I just approve, and can I take it back?"
//! These tests are the four sentences the answer has to be able to make:
//!
//!   1. an approval names ONE version, one actor, one time and one
//!      accepted-criteria version, and all four are in the row;
//!   2. it never inherits — moving the record to a new version does not move
//!      the approval, and approving against a version the screen was showing
//!      but the record has moved off is REFUSED, with the files that changed;
//!   3. a withdrawal is its own act, its own row, and leaves both readable;
//!   4. the JSON copy exists under the project with the named fields.
//!
//! Every id is synthetic and no fixture here contains private content.

use rusqlite::params;
use thruline_core::change_decision::{self, Decide, DecisionAct, Outcome};
use thruline_core::change_export::{self, Standing};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c001";
const VERSION_A: &str = "snap_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const VERSION_B: &str = "snap_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

/// A ledger with one project, one Change, one accepted scope version and one
/// implementation binding at `VERSION_A`, whose snapshot roots at `root`.
fn store(root: &std::path::Path) -> (tempfile::TempDir, Ledger) {
    store_with(root, true)
}

/// The same store, with or without a deliberate implementation binding. Without
/// one is the state of every Change reachable by the documented flow.
fn store_with(root: &std::path::Path, bound: bool) -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open a fresh ledger");
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES(?1, '/tmp/synthetic-project', 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
        params![PROJECT],
    )
    .expect("insert project");
    conn.execute(
        "INSERT INTO changes(id, project_id, opened_at, title, purpose, request_event_id,
                             requester, request_source)
         VALUES(?1, ?2, '2026-09-22T08:00:00Z', 'Totals add up', 'They do not.', NULL,
                'sam', 'person')",
        params![CHANGE, PROJECT],
    )
    .expect("insert change");
    conn.execute(
        "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at, criteria_json,
                                            exclusions_json, allowed_scope_json, constraints_json,
                                            check_profile_json, content_sha256)
         VALUES(?1, 1, '2026-09-22T08:30:00Z', '[\"The totals add up\"]', '[]', '[\"checkout/\"]',
                '[]', '[]', 'deadbeef')",
        params![CHANGE],
    )
    .expect("insert scope revision");
    conn.execute(
        "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by, accepted_at,
                                              rationale)
         VALUES(?1, 1, 'sam', '2026-09-22T09:00:00Z', '')",
        params![CHANGE],
    )
    .expect("insert scope acceptance");
    snapshot(&ledger, VERSION_A, root, &[("checkout/totals.py", "aaa")]);
    if bound {
        conn.execute(
            "INSERT INTO change_implementations(change_id, binding_version, bound_at,
                                                repository_identity, base_revision, snapshot_id,
                                                workspace_identity)
             VALUES(?1, 1, '2026-09-22T10:00:00Z', 'shop', 'base0000', ?2, NULL)",
            params![CHANGE, VERSION_A],
        )
        .expect("insert implementation");
    }
    (dir, ledger)
}

/// One recorded snapshot with a known file list, so "what changed" is a fact
/// rather than a guess.
fn snapshot(ledger: &Ledger, id: &str, root: &std::path::Path, files: &[(&str, &str)]) {
    snapshot_at(ledger, id, root, files, None)
}

/// The same, standing on a named commit. T-5-2 needs this: the founder path is
/// an edit that has been COMMITTED, which leaves the working tree clean and so
/// leaves nothing in any of the manifests above.
fn snapshot_at(
    ledger: &Ledger,
    id: &str,
    root: &std::path::Path,
    files: &[(&str, &str)],
    head: Option<&str>,
) {
    let doc = serde_json::json!({
        "id": id,
        "taken_at": "2026-09-22T10:00:00Z",
        "root": root.to_string_lossy(),
        "git": head.map(|h| serde_json::json!({
            "branch": "main",
            "head": h,
            "worktree_root": root.to_string_lossy(),
            "git_common_dir": root.join(".git").to_string_lossy(),
            "remote_origin": null,
        })),
        "tree": {
            "staged": files.iter().map(|(p, d)| serde_json::json!({
                "path": p, "sha256": d, "bytes": 1
            })).collect::<Vec<_>>(),
            "unstaged": [], "untracked": [], "deleted": [], "dirty_diff_sha256": null,
        },
        "environment": { "os": "test", "arch": "test", "git_version": null,
                         "env_names": [], "lockfiles": {} },
        "processes": { "ps_sha256": null, "ps_lines": 0, "ports": null, "task_ids": null,
                       "log_cursors": null, "gaps": "none" },
        "complete": true,
        "note": null,
    });
    let text = serde_json::to_string(&doc).expect("serialize the snapshot doc");
    ledger
        .conn()
        .execute(
            "INSERT INTO workspace_snapshots(id, taken_at, root, head, branch, complete, doc,
                                             doc_sha256)
             VALUES(?1, '2026-09-22T10:00:00Z', ?2, NULL, NULL, 1, ?3, ?4)",
            params![
                id,
                root.to_string_lossy(),
                text,
                thruline_core::chain::sha256_hex(text.as_bytes())
            ],
        )
        .expect("insert snapshot");
}

fn bind(ledger: &Ledger, version: u32, snapshot_id: &str) {
    ledger
        .conn()
        .execute(
            "INSERT INTO change_implementations(change_id, binding_version, bound_at,
                                                repository_identity, base_revision, snapshot_id,
                                                workspace_identity)
             VALUES(?1, ?2, '2026-09-22T11:30:00Z', 'shop', 'base0000', ?3, NULL)",
            params![CHANGE, version, snapshot_id],
        )
        .expect("bind a later version");
}

fn decide(seen: &str) -> Decide {
    Decide {
        actor: "sam".to_string(),
        at: "2026-09-22T11:00:00Z".to_string(),
        seen_revision: seen.to_string(),
        note: None,
    }
}

/* ── 1. an approval names exactly one of everything ───────────────────── */

#[test]
fn an_approval_names_one_version_one_actor_one_time_and_one_accepted_criteria_version() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());

    let Outcome::Recorded { decision } =
        change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve")
    else {
        panic!("a matching version must be approvable");
    };
    assert_eq!(decision.act, DecisionAct::Approved);
    assert_eq!(decision.target_revision, VERSION_A);
    assert_eq!(decision.actor, "sam");
    assert_eq!(decision.decided_at, "2026-09-22T11:00:00Z");
    assert_eq!(decision.scope_version, 1);
    assert_eq!(decision.withdraws, None);

    let view = change_decision::read(&ledger, CHANGE).expect("read");
    assert_eq!(
        view.standing.as_ref().map(|d| d.id.clone()),
        Some(decision.id)
    );
    assert_eq!(view.history.len(), 1);
    assert!(!view.code_changed_since());
}

#[test]
fn an_approval_with_no_name_on_it_is_refused() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    let mut nameless = decide(VERSION_A);
    nameless.actor = "   ".to_string();
    assert!(change_decision::approve(&ledger, CHANGE, &nameless).is_err());
    assert!(change_decision::read(&ledger, CHANGE)
        .expect("read")
        .history
        .is_empty());
}

/* ── 2. it never inherits, and a moved tree is refused with the files ─── */

#[test]
fn an_approval_given_for_one_version_does_not_move_to_the_next_one() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve");

    snapshot(
        &ledger,
        VERSION_B,
        project.path(),
        &[("checkout/totals.py", "bbb")],
    );
    bind(&ledger, 2, VERSION_B);

    let view = change_decision::read(&ledger, CHANGE).expect("read");
    assert_eq!(
        view.standing.as_ref().map(|d| d.target_revision.clone()),
        Some(VERSION_A.to_string()),
        "the approval still names the version it was given for"
    );
    assert_eq!(view.target_revision, Some(VERSION_B.to_string()));
    assert!(
        view.code_changed_since(),
        "the two are compared and the difference is stated, never resolved"
    );
}

/// T-5-2, the other half of the same rule. An approval standing on the OLD
/// version does not block approving the new one: that is a second, separate
/// act, naming its own version, and both stay in the history. What is refused
/// is a second approval of the SAME version.
#[test]
fn approving_the_version_the_record_has_moved_to_is_a_new_approval_of_its_own() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve A");

    snapshot(
        &ledger,
        VERSION_B,
        project.path(),
        &[("checkout/totals.py", "bbb")],
    );
    bind(&ledger, 2, VERSION_B);

    match change_decision::approve(&ledger, CHANGE, &decide(VERSION_B)).expect("approve B") {
        Outcome::Recorded { decision } => assert_eq!(decision.target_revision, VERSION_B),
        other => panic!("approving the version on screen records it: {other:?}"),
    }
    let view = change_decision::read(&ledger, CHANGE).expect("read");
    assert_eq!(view.history.len(), 2, "both acts are in the record");
    assert_eq!(
        view.history
            .iter()
            .map(|r| r.target_revision.as_str())
            .collect::<Vec<_>>(),
        vec![VERSION_A, VERSION_B],
        "each act names the version it was about"
    );
    assert_eq!(
        view.standing.map(|d| d.target_revision),
        Some(VERSION_B.to_string()),
        "the newest approval is the one that stands"
    );

    /* And a second approval of that same version is refused, by name. */
    let again = change_decision::approve(&ledger, CHANGE, &decide(VERSION_B))
        .expect_err("the same version cannot be approved twice");
    assert!(
        again.to_string().contains("approved this version"),
        "{again}"
    );
}

#[test]
fn approving_a_version_the_record_has_moved_off_is_refused_and_names_what_changed() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    snapshot(
        &ledger,
        VERSION_B,
        project.path(),
        &[("checkout/totals.py", "bbb"), ("checkout/cart.py", "ccc")],
    );
    bind(&ledger, 2, VERSION_B);

    // The screen was showing A; the record stands at B.
    let outcome = change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve");
    let Outcome::Moved { seen, now, changed } = outcome else {
        panic!("a moved tree must refuse the approval");
    };
    assert_eq!(seen, VERSION_A);
    assert_eq!(now, VERSION_B);
    assert_eq!(changed, vec!["checkout/cart.py", "checkout/totals.py"]);
    assert!(
        change_decision::read(&ledger, CHANGE)
            .expect("read")
            .history
            .is_empty(),
        "a refused approval records nothing"
    );
}

/* ── 3. a withdrawal is its own act ───────────────────────────────────── */

#[test]
fn a_withdrawal_is_its_own_row_and_leaves_both_acts_readable() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    let Outcome::Recorded { decision: approval } =
        change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve")
    else {
        panic!("approve");
    };

    let Outcome::Recorded { decision: taken } =
        change_decision::withdraw(&ledger, CHANGE, &decide(VERSION_A)).expect("withdraw")
    else {
        panic!("withdraw");
    };
    assert_eq!(taken.act, DecisionAct::Withdrawn);
    assert_eq!(taken.withdraws.as_deref(), Some(approval.id.as_str()));
    assert_eq!(taken.target_revision, VERSION_A);

    let view = change_decision::read(&ledger, CHANGE).expect("read");
    assert!(view.standing.is_none(), "nothing stands after a withdrawal");
    assert_eq!(view.history.len(), 2, "both acts are still there");
    assert_eq!(view.history[0].act, DecisionAct::Approved);
    assert_eq!(view.history[1].act, DecisionAct::Withdrawn);

    // And the version can be approved again, as a NEW act.
    change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve again");
    assert_eq!(
        change_decision::read(&ledger, CHANGE)
            .expect("read")
            .history
            .len(),
        3
    );
}

#[test]
fn there_is_nothing_to_withdraw_when_nothing_was_approved() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    assert!(change_decision::withdraw(&ledger, CHANGE, &decide(VERSION_A)).is_err());
}

/* ── 4. the file under the project ────────────────────────────────────── */

#[test]
fn the_json_record_exists_under_the_project_with_the_named_fields() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    let Outcome::Recorded { decision } =
        change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve")
    else {
        panic!("approve");
    };
    let path = decision.record_path.expect("the record path is on the row");
    let expected = change_decision::record_path_for(project.path(), VERSION_A);
    assert_eq!(std::path::Path::new(&path), expected.as_path());

    let body = std::fs::read_to_string(&path).expect("the record must be readable");
    let doc: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(doc["act"], "approved");
    assert_eq!(doc["change_id"], CHANGE);
    assert_eq!(doc["version"], VERSION_A);
    assert_eq!(doc["accepted_criteria_version"], 1);
    assert_eq!(doc["actor"], "sam");
    assert_eq!(doc["decided_at"], "2026-09-22T11:00:00Z");
    assert_eq!(
        doc["note"],
        "This records an approval. It does not merge, deploy or tell the agent anything."
    );
}

/* ── the table is append-only, like everything else in this layer ─────── */

#[test]
fn a_recorded_decision_cannot_be_edited_or_deleted() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve");
    assert!(ledger
        .conn()
        .execute("UPDATE change_decisions SET actor='someone else'", [])
        .is_err());
    assert!(ledger
        .conn()
        .execute("DELETE FROM change_decisions", [])
        .is_err());
}

/* ── T-S-4-F1: the version a decision names is the one the product records ── */

/// The ordinary path, with NO row in `change_implementations` at all: the
/// standing `intended` examined-inputs record (T-3-2) is what `change new`,
/// first use, `change prepare` and `check --for-change` write, and it is what
/// the decision names. Before this fix Approve was permanently off for every
/// Change a person could reach, because nothing the product ships writes an
/// implementation binding.
#[test]
fn a_change_with_only_the_ordinary_examined_inputs_record_can_be_approved() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store_with(project.path(), false);
    assert_eq!(
        change_decision::standing_target(&ledger, CHANGE).expect("target"),
        (None, Some(1)),
        "with neither record there is genuinely no version"
    );

    let mut inputs = thruline_core::examined_inputs::ExaminedInputs::unknown_for(
        thruline_core::examined_inputs::SubjectKind::Intended,
        CHANGE,
    );
    inputs.base_revision = thruline_core::examined_inputs::Input::known("base0000");
    inputs.snapshot_id = thruline_core::examined_inputs::Input::known(VERSION_A);
    inputs.worktree_path =
        thruline_core::examined_inputs::Input::known(project.path().to_string_lossy());
    thruline_core::examined_inputs::record(&ledger, &inputs, "2026-09-22T10:30:00Z")
        .expect("record the intended inputs");

    assert_eq!(
        change_decision::intended_revision(&ledger, CHANGE).expect("intended"),
        Some(VERSION_A.to_string())
    );
    let Outcome::Recorded { decision } =
        change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve")
    else {
        panic!("the ordinary path must be approvable");
    };
    assert_eq!(decision.target_revision, VERSION_A);
    // And the screen reads the same version off the served record.
    let view = thruline_core::change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("the change exists");
    let implementation = view.implementation.expect("a version is named");
    assert_eq!(implementation.snapshot_id.as_deref(), Some(VERSION_A));
    assert_eq!(
        implementation.version, 0,
        "a derived reading is not a deliberate binding"
    );
}

/* ── T-S-4-F3: our own folder is not the person's edit ────────────────── */

/// Approving writes `.thruline/decisions/<version>.json` into the project. That
/// file must not then be reported back to the person as "code changed since" —
/// and it is excluded from the COMPARISON, not merely from the printed list, so
/// writing a decision cannot move the version at all.
#[test]
fn the_products_own_folder_is_excluded_from_what_changed() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    snapshot(
        &ledger,
        VERSION_B,
        project.path(),
        &[
            ("checkout/totals.py", "aaa"),
            (".thruline/decisions/snap_a.json", "written-by-us"),
        ],
    );
    assert!(
        change_decision::changed_files(&ledger, CHANGE, VERSION_A, VERSION_B)
            .expect("compare")
            .is_empty(),
        "only our own folder differs, so nothing the person did has changed"
    );
}

/* ── T-5-2: what moved, when the edit has been committed ──────────────── */

/// Run one git command in `root` and give back its trimmed stdout.
fn run_git(root: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_AUTHOR_NAME", "T")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_NAME", "T")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .output()
        .expect("run git");
    assert!(out.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repository with one file committed twice, and the two commits it stands
/// on. This is the founder path — edit, commit, prepare — after which the
/// working tree is CLEAN and the edit lives in no manifest at all.
fn repo_with_one_committed_edit(root: &std::path::Path) -> (String, String) {
    run_git(root, &["init", "--initial-branch=main", "."]);
    std::fs::create_dir_all(root.join("src")).expect("src dir");
    std::fs::write(root.join("src/lib.rs"), "fn one() {}\n").expect("write lib.rs");
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "one"]);
    let before = run_git(root, &["rev-parse", "HEAD"]);
    std::fs::write(root.join("src/lib.rs"), "fn one() {}\nfn two() {}\n").expect("edit lib.rs");
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-m", "two"]);
    let after = run_git(root, &["rev-parse", "HEAD"]);
    assert_ne!(before, after, "the commit has to have moved");
    (before, after)
}

/// T-F-1-F6, the whole of it. The list the refusal shows used to be a
/// one-element array holding an internal marker — a non-printable character
/// and the word `head`, on the screen where a filename belongs — and the file
/// that actually changed never appeared, because only working-tree dirtiness
/// was ever diffed.
#[test]
fn a_committed_edit_is_named_by_what_changed_and_no_internal_key_reaches_the_list() {
    let project = tempfile::tempdir().expect("project dir");
    let root = project.path();
    let (before, after) = repo_with_one_committed_edit(root);

    let (_dir, ledger) = store(root);
    snapshot_at(&ledger, "snap_seen000", root, &[], Some(&before));
    snapshot_at(&ledger, VERSION_B, root, &[], Some(&after));

    let changed = change_decision::changed_files(&ledger, CHANGE, "snap_seen000", VERSION_B)
        .expect("compare two committed versions");
    assert_eq!(
        changed,
        vec!["src/lib.rs".to_string()],
        "the file the person actually changed is the file the refusal names"
    );
    for entry in &changed {
        assert!(
            !entry.chars().any(char::is_control),
            "no control character ever reaches a list of file names: {entry:?}"
        );
        assert!(
            !entry.contains("head"),
            "no internal key ever reaches a list of file names: {entry:?}"
        );
    }
}

/// And the refusal a person actually sees carries that same list, rather than
/// the approval being carried onto a version they never read.
#[test]
fn approving_after_a_commit_is_refused_and_the_refusal_names_the_committed_file() {
    let project = tempfile::tempdir().expect("project dir");
    let root = project.path();
    let (before, after) = repo_with_one_committed_edit(root);

    let (_dir, ledger) = store(root);
    snapshot_at(&ledger, "snap_seen000", root, &[], Some(&before));
    snapshot_at(&ledger, VERSION_B, root, &[], Some(&after));
    bind(&ledger, 2, VERSION_B);

    match change_decision::approve(&ledger, CHANGE, &decide("snap_seen000")).expect("act") {
        Outcome::Moved { seen, now, changed } => {
            assert_eq!(seen, "snap_seen000");
            assert_eq!(now, VERSION_B);
            assert_eq!(changed, vec!["src/lib.rs".to_string()]);
        }
        other => panic!("a moved tree must refuse, not record: {other:?}"),
    }
    let rows: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM change_decisions", [], |r| r.get(0))
        .expect("count the rows");
    assert_eq!(rows, 0, "a refused approval writes nothing");
}

/* ── T-5-2: two clicks at the same instant (`B6.14`) ──────────────────── */

/// Two approvals racing on the same version leave exactly ONE row, and the one
/// that lost is told the version is already approved and by whom — never a
/// second approval, and never a lock message.
#[test]
fn two_approvals_at_the_same_instant_leave_exactly_one_row() {
    let project = tempfile::tempdir().expect("project dir");
    let (dir, ledger) = store(project.path());
    let db = dir.path().join("ledger.db");
    drop(ledger);

    /* Both connections are opened before either thread starts, so what is
     * raced is the ACT and not the schema check. */
    let first = Ledger::open(&db).expect("first connection");
    let second = Ledger::open(&db).expect("second connection");
    let gate = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut handles = Vec::new();
    for ledger in [first, second] {
        let gate = std::sync::Arc::clone(&gate);
        handles.push(std::thread::spawn(move || {
            gate.wait();
            change_decision::approve(&ledger, CHANGE, &decide(VERSION_A))
        }));
    }
    let outcomes: Vec<_> = handles
        .into_iter()
        .map(|h| h.join().expect("the thread finished"))
        .collect();

    let recorded = outcomes.iter().filter(|o| o.is_ok()).count();
    assert_eq!(
        recorded, 1,
        "exactly one of the two clicks records: {outcomes:?}"
    );
    let refused = outcomes
        .iter()
        .find_map(|o| o.as_ref().err())
        .map(|e| e.to_string())
        .expect("the other click is refused");
    assert!(
        refused.contains("sam") && refused.contains("approved this version"),
        "the loser is told the version is already approved, and by whom: {refused}"
    );

    let ledger = Ledger::open(&db).expect("re-open the ledger");
    let rows: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM change_decisions WHERE act='approved'",
            [],
            |r| r.get(0),
        )
        .expect("count the approvals");
    assert_eq!(rows, 1, "one version, one approval, whatever the timing");
}

/* ── T-5-2-NO-REVIVAL (R-2026-09-28-10) ───────────────────────────────── */

/// Approve A, the record moves to B, approve B, withdraw B. The person's last
/// act was a withdrawal, so NOTHING stands: A's approval was replaced by B's
/// the moment B's was recorded, and taking B's back does not bring A's back.
/// The history keeps all three acts, each naming its own version, and says of
/// A's that a later approval replaced it.
#[test]
fn withdrawing_the_latest_approval_never_revives_an_earlier_one() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    let Outcome::Recorded { decision: first } =
        change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve A")
    else {
        panic!("approve A");
    };

    snapshot(
        &ledger,
        VERSION_B,
        project.path(),
        &[("checkout/totals.py", "bbb")],
    );
    bind(&ledger, 2, VERSION_B);
    let Outcome::Recorded { decision: second } =
        change_decision::approve(&ledger, CHANGE, &decide(VERSION_B)).expect("approve B")
    else {
        panic!("approve B");
    };
    let Outcome::Recorded { decision: taken } =
        change_decision::withdraw(&ledger, CHANGE, &decide(VERSION_B)).expect("withdraw B")
    else {
        panic!("withdraw B");
    };
    assert_eq!(taken.withdraws.as_deref(), Some(second.id.as_str()));

    let view = change_decision::read(&ledger, CHANGE).expect("read");
    assert_eq!(
        view.standing.as_ref().map(|d| d.target_revision.as_str()),
        None,
        "the last act was a withdrawal: nothing stands, and A's approval is not revived"
    );
    assert!(
        !view.code_changed_since(),
        "with nothing standing, there is no 'code changed since' to say"
    );

    /* All three acts, each with its own version, oldest first. */
    assert_eq!(
        view.history
            .iter()
            .map(|r| (r.act, r.target_revision.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (DecisionAct::Approved, VERSION_A),
            (DecisionAct::Approved, VERSION_B),
            (DecisionAct::Withdrawn, VERSION_B),
        ],
        "the history lists every act, each naming the version it was about"
    );

    /* A's approval is shown as replaced by the later one — in the served shape,
     * so every surface reads the same answer. */
    let served = serde_json::to_value(&view).expect("serialize the view");
    assert_eq!(
        served["history"][0]["superseded_by"],
        serde_json::json!(second.id),
        "A's approval names the later approval that replaced it: {served}"
    );
    assert_eq!(served["history"][0]["id"], serde_json::json!(first.id));
    assert_eq!(
        served["history"][1]["superseded_by"],
        serde_json::Value::Null,
        "B's approval was withdrawn, not replaced"
    );
    assert_eq!(
        served["history"][2]["superseded_by"],
        serde_json::Value::Null,
        "a withdrawal replaces nothing"
    );

    /* And A's approval cannot be reached by a second withdrawal either: there
     * is nothing standing to take back. */
    let again = change_decision::withdraw(&ledger, CHANGE, &decide(""))
        .expect_err("nothing stands, so there is nothing to withdraw");
    assert!(
        again.to_string().contains("no approval standing"),
        "{again}"
    );
    assert_eq!(
        change_decision::read(&ledger, CHANGE)
            .expect("read")
            .history
            .len(),
        3,
        "a refused withdrawal records nothing"
    );

    /* B can be approved again, as a new act of its own, and then it stands. */
    change_decision::approve(&ledger, CHANGE, &decide(VERSION_B)).expect("approve B again");
    assert_eq!(
        change_decision::read(&ledger, CHANGE)
            .expect("read")
            .standing
            .map(|d| d.target_revision),
        Some(VERSION_B.to_string())
    );
}

/// The same rule with no move: approve A, withdraw A, approve A again. The
/// latest approval has not been withdrawn, so it stands — and the first one is
/// not "replaced", it was withdrawn.
#[test]
fn a_fresh_approval_after_a_withdrawal_stands_and_the_withdrawn_one_is_not_replaced() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve");
    change_decision::withdraw(&ledger, CHANGE, &decide(VERSION_A)).expect("withdraw");
    let Outcome::Recorded { decision: fresh } =
        change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve again")
    else {
        panic!("approve again");
    };
    let view = change_decision::read(&ledger, CHANGE).expect("read");
    assert_eq!(view.standing.map(|d| d.id), Some(fresh.id));
    let served = serde_json::to_value(&view.history).expect("serialize");
    assert!(
        served
            .as_array()
            .expect("a list")
            .iter()
            .all(|r| r["superseded_by"].is_null()),
        "nothing here was replaced by a later approval: {served}"
    );
}

/* ── DECISION-FILE-AFTER-WITHDRAW: one derived standing copy ──────────── */

fn decision_file(root: &std::path::Path) -> serde_json::Value {
    serde_json::from_slice(
        &std::fs::read(change_decision::record_path_for(root, VERSION_A))
            .expect("read the decision file in the recorded worktree"),
    )
    .expect("decision file is JSON")
}

#[test]
fn withdrawal_replaces_the_derived_copy_with_its_exact_row() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    let Outcome::Recorded { decision: approved } =
        change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve")
    else {
        panic!("approve");
    };
    let withdrawal = Decide {
        at: "2026-09-22T11:30:00Z".into(),
        ..decide(VERSION_A)
    };
    let Outcome::Recorded {
        decision: withdrawn,
    } = change_decision::withdraw(&ledger, CHANGE, &withdrawal).expect("withdraw")
    else {
        panic!("withdraw");
    };

    let file = decision_file(project.path());
    assert_eq!(file["act"], "withdrawn", "the file must not say approved");
    assert_eq!(file["decision_id"], withdrawn.id);
    assert_eq!(file["withdraws"], approved.id);
    assert_eq!(file["decided_at"], withdrawal.at);
    assert_eq!(file["version"], VERSION_A);
    assert_eq!(file["derived_copy"], true);
    assert_eq!(file["schema_version"], 3);
    assert_eq!(
        file["note"],
        "This records a withdrawal. It does not merge, deploy or tell the agent anything."
    );

    let view = change_decision::read(&ledger, CHANGE).expect("read history");
    assert!(view.standing.is_none());
    assert_eq!(view.history.len(), 2);
    assert_eq!(view.history[0].id, approved.id);
    assert_eq!(view.history[1].id, withdrawn.id);
    assert_eq!(view.history[1].record_path, None);

    let export = change_export::records(&ledger, CHANGE, "2026-09-22T11:31:00Z")
        .expect("export reading")
        .expect("change exists");
    assert_eq!(export.decisions.len(), 2);
    assert_eq!(export.decisions[0].decision_id, approved.id);
    assert_eq!(export.decisions[0].standing, Some(Standing::ContentChanged));
    assert_eq!(export.decisions[1].decision_id, withdrawn.id);
    assert_eq!(export.decisions[1].standing, None);
    assert_eq!(export.totals.records, 1);
    assert_eq!(export.totals.not_readable, 1);
}

#[test]
fn reapproval_replaces_the_derived_copy_with_the_second_approval() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    let Outcome::Recorded { decision: first } =
        change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("first approval")
    else {
        panic!("first approval");
    };
    change_decision::withdraw(
        &ledger,
        CHANGE,
        &Decide {
            at: "2026-09-22T11:30:00Z".into(),
            ..decide(VERSION_A)
        },
    )
    .expect("withdraw");
    let second_act = Decide {
        at: "2026-09-22T12:00:00Z".into(),
        ..decide(VERSION_A)
    };
    let Outcome::Recorded { decision: second } =
        change_decision::approve(&ledger, CHANGE, &second_act).expect("second approval")
    else {
        panic!("second approval");
    };
    assert_ne!(first.id, second.id);

    let file = decision_file(project.path());
    assert_eq!(file["act"], "approved");
    assert_eq!(file["decision_id"], second.id);
    assert_eq!(file["decided_at"], second_act.at);
    assert_eq!(file["withdraws"], serde_json::Value::Null);
    assert_eq!(file["derived_copy"], true);

    let view = change_decision::read(&ledger, CHANGE).expect("read history");
    assert_eq!(view.standing.as_ref().map(|row| &row.id), Some(&second.id));
    assert_eq!(view.history.len(), 3, "all rows remain append-only");
    assert_eq!(view.history[0].id, first.id);
    let export = change_export::records(&ledger, CHANGE, "2026-09-22T12:01:00Z")
        .expect("export reading")
        .expect("change exists");
    assert_eq!(export.decisions.len(), 3);
    assert_eq!(export.decisions[0].standing, Some(Standing::ContentChanged));
    assert_eq!(export.decisions[1].standing, None);
    assert_eq!(export.decisions[2].decision_id, second.id);
    assert_eq!(export.decisions[2].standing, Some(Standing::Readable));
}

/* ── DECISION-HISTORY-SHAPE (Q19 V1 E7, QD-11) ─────────────────────────── */

/// The field set a decision-history row carried when no later approval had
/// replaced it, exactly as the older release (e48a3310, the Q19 baseline)
/// served it at step 14 of the rehearsal:
/// `codex-dev-20260927/qual/runs/baseline-4e65121b-bb21d988/observed/Q19/steps/V1/step-14-before.json`.
const UNREPLACED_ROW_FIELDS: &[&str] = &[
    "act",
    "actor",
    "decided_at",
    "exceptions",
    "id",
    "record_path",
    "scope_version",
    "target_revision",
    "withdraws",
];

fn served_history(ledger: &Ledger) -> String {
    let view = thruline_core::change::read_change(ledger, CHANGE)
        .expect("read the change")
        .expect("the change exists");
    let served = serde_json::to_value(&view).expect("serialize the served change");
    serde_json::to_string(&served["decisions"]["history"]).expect("serialize the history")
}

/// Q19's shape: approve, capture `change.decisions.history`, do things that
/// append NO decision act (the code moves to a new version, the record is read
/// again), capture again. The two captures are byte-identical, and the row
/// carries exactly the fields it carried before a derived field existed — so a
/// store read before and after an upgrade still says the same thing.
#[test]
fn decision_history_reads_the_same_until_a_decision_act_changes_it() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve A");
    let before = served_history(&ledger);

    /* No decision act: the code moves, and the record is read again. */
    snapshot(
        &ledger,
        VERSION_B,
        project.path(),
        &[("checkout/totals.py", "bbb")],
    );
    bind(&ledger, 2, VERSION_B);
    let after = served_history(&ledger);
    assert_eq!(
        before, after,
        "no decision act, so the history reads the same"
    );

    let rows: serde_json::Value = serde_json::from_str(&after).expect("parse");
    let mut keys: Vec<&str> = rows[0]
        .as_object()
        .expect("a row is an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys, UNREPLACED_ROW_FIELDS,
        "an approval nothing replaced serves the fields the older release served, and no derived null: {after}"
    );
}

/// The same rule's other half: when a later approval DID replace one, the row
/// says so — the field is present exactly when it applies.
#[test]
fn a_replaced_approval_still_names_the_one_that_replaced_it() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve A");
    snapshot(
        &ledger,
        VERSION_B,
        project.path(),
        &[("checkout/totals.py", "bbb")],
    );
    bind(&ledger, 2, VERSION_B);
    let Outcome::Recorded { decision: second } =
        change_decision::approve(&ledger, CHANGE, &decide(VERSION_B)).expect("approve B")
    else {
        panic!("approve B");
    };
    let rows: serde_json::Value = serde_json::from_str(&served_history(&ledger)).expect("parse");
    assert_eq!(
        rows[0].get("superseded_by"),
        Some(&serde_json::json!(second.id)),
        "{rows}"
    );
    assert!(
        rows[1].get("superseded_by").is_none(),
        "the standing approval was replaced by nothing: {rows}"
    );
}

/* ── DECISION-HISTORY-SHAPE-R4: one order for an approval's exceptions ───
 *
 * `write_record` wrote an approval's exceptions in readiness order
 * (criterion:2, criterion:10) while the row was read back `ORDER BY
 * obligation_id` as TEXT (criterion:10, criterion:2), so the act response, the
 * file, the served history and the export's comparison of the two did not
 * agree about one approval once a Change had ten criteria. Found by the
 * round-3 adversary (gates/adversary-DECISION-HISTORY-SHAPE-R3,
 * attack_a / attack_a2).
 *
 * The rule these pin: an approval's exceptions have ONE order, whichever way
 * they were written, read or served — by the number in `criterion:<n>`, then
 * the review obligation, then anything else by name. */

use thruline_core::change_decision::DecisionException;

fn ids(exceptions: &[DecisionException]) -> Vec<&str> {
    exceptions
        .iter()
        .map(|e| e.obligation_id.as_str())
        .collect()
}

fn ids_in(document: &serde_json::Value) -> Vec<String> {
    document["exceptions"]
        .as_array()
        .expect("an exceptions array")
        .iter()
        .map(|e| e["obligation_id"].as_str().expect("an id").to_string())
        .collect()
}

/// Accept a second scope version of `n` criteria whose check profile names who
/// may set something aside.
fn with_exception_path_and_criteria(ledger: &Ledger, n: usize) {
    let criteria: Vec<String> = (1..=n).map(|i| format!("Criterion {i} holds")).collect();
    ledger
        .conn()
        .execute(
            "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at, criteria_json,
                                                exclusions_json, allowed_scope_json,
                                                constraints_json, check_profile_json, content_sha256)
             VALUES(?1, 2, '2026-09-22T08:45:00Z', ?2, '[]', '[\"checkout/\"]', '[]', ?3, 'deadbeef')",
            params![
                CHANGE,
                serde_json::to_string(&criteria).expect("json"),
                format!(
                    "[\"{}the release policy\"]",
                    thruline_core::change_readiness::EXCEPTION_POLICY_PREFIX
                )
            ],
        )
        .expect("a second scope version");
    ledger
        .conn()
        .execute(
            "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by, accepted_at,
                                                  rationale)
             VALUES(?1, 2, 'sam', '2026-09-22T08:50:00Z', '')",
            params![CHANGE],
        )
        .expect("accept it");
}

fn waive(ledger: &Ledger, obligation: &str, at: &str) {
    thruline_core::change_readiness::waive(
        ledger,
        CHANGE,
        obligation,
        thruline_core::change_readiness::WaiverKind::Waived,
        "sam",
        &format!("{obligation} cannot be checked this week"),
        at,
    )
    .expect("waive");
}

/// A real approval through `approve`: what it returns, what `read` serves, and
/// what `write_record` wrote agree, in the same order, and the whole document
/// the export would compare the file with is the file.
#[test]
fn an_approval_lists_its_exceptions_in_one_order_on_every_surface() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    with_exception_path_and_criteria(&ledger, 12);
    // Written in a scrambled order, on ids that sort differently as text
    // (10, 11, 2, 9) than as numbers (2, 9, 10, 11).
    for (i, obligation) in ["criterion:11", "criterion:2", "criterion:10", "criterion:9"]
        .iter()
        .enumerate()
    {
        waive(&ledger, obligation, &format!("2026-09-22T10:5{i}:00Z"));
    }

    let Outcome::Recorded { decision } =
        change_decision::approve(&ledger, CHANGE, &decide(VERSION_A)).expect("approve")
    else {
        panic!("approve");
    };
    let expected = ["criterion:2", "criterion:9", "criterion:10", "criterion:11"];
    assert_eq!(
        ids(&decision.exceptions),
        expected,
        "the row `approve` returns"
    );

    let view = change_decision::read(&ledger, CHANGE).expect("read");
    assert_eq!(
        ids(&view.history[0].exceptions),
        expected,
        "the row `read` serves, off the stored rows"
    );
    assert_eq!(
        view.history[0], decision,
        "the row served later is the row that was returned"
    );
    let standing = view.standing.as_ref().expect("it stands");
    assert_eq!(ids(&standing.exceptions), expected, "the standing approval");

    let file: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(change_decision::record_path_for(project.path(), VERSION_A))
            .expect("the decision copy"),
    )
    .expect("json");
    assert_eq!(
        ids_in(&file),
        expected,
        "the file written under the project"
    );
    assert_eq!(
        change_decision::record_document_for_row(CHANGE, &view.history[0]),
        file,
        "the document built from the served row is the document that was written"
    );
    let served = serde_json::to_value(&view.history[0]).expect("serialize");
    assert_eq!(
        served["exceptions"], file["exceptions"],
        "the served row and the file list the same exceptions in the same order"
    );
}

fn an_exception(obligation: &str) -> DecisionException {
    DecisionException {
        waiver_id: format!("wv_{obligation}"),
        obligation_id: obligation.to_string(),
        kind: "waived".to_string(),
        actor: "sam".to_string(),
        authority: "the release policy".to_string(),
        rationale: format!("{obligation} cannot be checked this week"),
    }
}

fn permutations<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for i in 0..items.len() {
        let mut rest = items.to_vec();
        let head = rest.remove(i);
        for mut tail in permutations(&rest) {
            tail.insert(0, head.clone());
            out.push(tail);
        }
    }
    out
}

/// The document is built ONE way: whatever order it is handed the exceptions
/// in, it lists them in the one order.
#[test]
fn the_decision_document_lists_exceptions_in_one_order_whatever_order_it_is_handed() {
    let all: Vec<DecisionException> = [
        "criterion:10",
        "criterion:2",
        "review",
        "criterion:9",
        "criterion:11",
    ]
    .iter()
    .map(|o| an_exception(o))
    .collect();
    let build = |exceptions: &[DecisionException]| {
        change_decision::record_document(
            DecisionAct::Approved,
            CHANGE,
            VERSION_A,
            1,
            "sam",
            "2026-09-22T11:00:00Z",
            exceptions,
        )
    };
    let reference = build(&all);
    assert_eq!(
        ids_in(&reference),
        [
            "criterion:2",
            "criterion:9",
            "criterion:10",
            "criterion:11",
            "review"
        ],
        "numbered criteria by number, then the review obligation"
    );
    let perms = permutations(&all);
    assert_eq!(perms.len(), 120);
    for permutation in perms {
        assert_eq!(
            build(&permutation),
            reference,
            "handed {:?}",
            ids(&permutation)
        );
    }
}

/// The reader does not depend on the order the rows were written in, nor on a
/// text sort of an id with a number inside.
#[test]
fn stored_exception_rows_are_read_back_in_the_one_order_whatever_order_they_were_written() {
    let project = tempfile::tempdir().expect("project dir");
    let (_dir, ledger) = store(project.path());
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO change_decisions(id, change_id, act, target_revision, scope_version, actor,
                                      decided_at, withdraws, record_path)
         VALUES('cd_one', ?1, 'approved', ?2, 1, 'sam', '2026-09-22T11:00:00Z', NULL, NULL)",
        params![CHANGE, VERSION_A],
    )
    .expect("an approval row");
    // Written 11, 2, review, 10, 9; text order is 10, 11, 2, 9, review.
    for (n, obligation) in [
        "criterion:11",
        "criterion:2",
        "review",
        "criterion:10",
        "criterion:9",
    ]
    .iter()
    .enumerate()
    {
        conn.execute(
            "INSERT INTO change_obligation_waivers(id, change_id, obligation_id, kind, actor,
                                                    authority, rationale, recorded_at)
             VALUES(?1, ?2, ?3, 'waived', 'sam', 'the release policy', 'not this week',
                    '2026-09-22T10:55:00Z')",
            params![format!("wv{n}"), CHANGE, obligation],
        )
        .expect("waiver");
        conn.execute(
            "INSERT INTO change_decision_exceptions(id, decision_id, waiver_id, obligation_id,
                                                     kind, actor, authority, rationale)
             VALUES(?1, 'cd_one', ?2, ?3, 'waived', 'sam', 'the release policy', 'not this week')",
            params![format!("ce{n}"), format!("wv{n}"), obligation],
        )
        .expect("exception row");
    }
    let view = change_decision::read(&ledger, CHANGE).expect("read");
    assert_eq!(
        ids(&view.history[0].exceptions),
        [
            "criterion:2",
            "criterion:9",
            "criterion:10",
            "criterion:11",
            "review"
        ]
    );
}
