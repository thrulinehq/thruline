//! C30 R-LEDGER — a second person approving the version an approval stands for,
//! and taking that back.
//!
//! The sentences these tests hold:
//!
//!   1. a second is kept with who and when, copied from the approval's version,
//!      and served masked on the Change as `seconds[]`, with the standing
//!      approval carrying a derived `seconds_standing` (absent at zero, so an
//!      approval nobody seconded serves the keys it always did);
//!   2. names are compared trimmed and case-folded (ruling Q3) and the refusal
//!      is a served sentence that says the name is not checked against anyone;
//!   3. a second needs a standing approval and the version the approval names;
//!   4. a person takes back only their own standing second, and may second again;
//!   5. a second stops counting the moment its approval is withdrawn or replaced,
//!      and is never carried onto the approval that replaces it;
//!   6. a second never makes an approval stand and never rewrites the decision
//!      file or the approval's row;
//!   7. every refusal appends nothing.
//!
//! Every id is synthetic and no fixture here contains private content.

use rusqlite::params;
use thruline_core::change_decision::{self, Decide, Outcome};
use thruline_core::change_decision_second::{
    self as seconds, SecondAct, Seconding, Withdrawing, NAME_NOT_CHECKED,
};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c001";
const VERSION: &str = "snap_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

/// A ledger with one project, one Change, one accepted scope version and one
/// implementation binding at `VERSION`, whose snapshot roots at `root`.
fn store(root: &std::path::Path) -> (tempfile::TempDir, Ledger) {
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
    let doc = serde_json::json!({
        "id": VERSION,
        "taken_at": "2026-09-22T10:00:00Z",
        "root": root.to_string_lossy(),
        "git": null,
        "tree": {
            "staged": [{ "path": "checkout/totals.py", "sha256": "aaa", "bytes": 1 }],
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
    conn.execute(
        "INSERT INTO workspace_snapshots(id, taken_at, root, head, branch, complete, doc,
                                         doc_sha256)
         VALUES(?1, '2026-09-22T10:00:00Z', ?2, NULL, NULL, 1, ?3, ?4)",
        params![
            VERSION,
            root.to_string_lossy(),
            text,
            thruline_core::chain::sha256_hex(text.as_bytes())
        ],
    )
    .expect("insert snapshot");
    conn.execute(
        "INSERT INTO change_implementations(change_id, binding_version, bound_at,
                                            repository_identity, base_revision, snapshot_id,
                                            workspace_identity)
         VALUES(?1, 1, '2026-09-22T10:00:00Z', 'shop', 'base0000', ?2, NULL)",
        params![CHANGE, VERSION],
    )
    .expect("insert implementation");
    (dir, ledger)
}

fn approve_as(ledger: &Ledger, who: &str) -> change_decision::DecisionRow {
    let decide = Decide {
        actor: who.to_string(),
        at: "2026-09-22T11:00:00Z".to_string(),
        seen_revision: VERSION.to_string(),
        note: None,
    };
    match change_decision::approve(ledger, CHANGE, &decide).expect("the approval") {
        Outcome::Recorded { decision } => decision,
        other => panic!("the approval must be recorded: {other:?}"),
    }
}

fn withdraw_approval(ledger: &Ledger, who: &str) {
    let decide = Decide {
        actor: who.to_string(),
        at: "2026-09-22T11:30:00Z".to_string(),
        seen_revision: VERSION.to_string(),
        note: None,
    };
    assert!(matches!(
        change_decision::withdraw(ledger, CHANGE, &decide).expect("the withdrawal"),
        Outcome::Recorded { .. }
    ));
}

fn second_as(ledger: &Ledger, who: &str, seen: &str, at: &str) -> Seconding {
    let decide = Decide {
        actor: who.to_string(),
        at: at.to_string(),
        seen_revision: seen.to_string(),
        note: None,
    };
    seconds::second(ledger, CHANGE, &decide).expect("the ledger answers")
}

fn recorded(outcome: Seconding) -> seconds::SecondRow {
    match outcome {
        Seconding::Recorded { second } => second,
        other => panic!("the second must be recorded: {other:?}"),
    }
}

fn count(ledger: &Ledger, table: &str) -> i64 {
    ledger
        .conn()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .expect("count")
}

fn view(ledger: &Ledger) -> thruline_core::change::ChangeView {
    thruline_core::change::read_change(ledger, CHANGE)
        .expect("the ledger answers")
        .expect("the Change")
}

fn standing_count(ledger: &Ledger) -> Option<u32> {
    view(ledger)
        .decisions
        .standing
        .and_then(|row| row.seconds_standing)
}

const AT: &str = "2026-09-22T12:00:00Z";

/* ── 1. kept, copied, served, derived ──────────────────────────────────── */

#[test]
fn a_second_is_kept_with_who_and_when_copied_from_the_approval_and_served_masked() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    let approval = approve_as(&ledger, "sam");
    let canary = format!("ghp_{}", "TLCANARYaaZZ11bbYY22ccXX33ddWW44e");

    let kept = recorded(second_as(&ledger, &format!("  {canary}  "), VERSION, AT));
    assert_eq!(kept.act, SecondAct::Approved);
    assert_eq!(kept.decision_id, approval.id, "it joins the approval");
    assert_eq!(kept.target_revision, VERSION, "copied, never inherited");
    assert_eq!(kept.scope_version, 1);
    assert_eq!(kept.recorded_at, AT);
    assert_eq!(kept.withdraws, None);
    assert_eq!(kept.actor, canary, "kept trimmed, as typed");

    let served = view(&ledger);
    assert_eq!(served.seconds.len(), 1);
    let one = &served.seconds[0];
    assert_eq!(one.id, kept.id);
    assert!(one.counting, "it stands on the standing approval");
    assert!(
        !one.actor.contains(&canary),
        "the Change serves it masked: {}",
        one.actor
    );
    assert_eq!(standing_count(&ledger), Some(1));
    // Stored as typed (trimmed), served masked.
    let stored: String = ledger
        .conn()
        .query_row("SELECT actor FROM change_decision_seconds", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(stored, canary);
    match &served.masking {
        thruline_core::redact::Treatment::Ran { masked, .. } => assert!(
            *masked > 0,
            "the masker fired on a second, so the Change's treatment must say so: {:?}",
            served.masking
        ),
        other => panic!("a masking pass ran over the second: {other:?}"),
    }
}

#[test]
fn an_approval_nobody_seconded_serves_the_keys_it_always_did() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    approve_as(&ledger, "sam");
    let doc = serde_json::to_value(view(&ledger)).unwrap();
    assert_eq!(
        doc["seconds"],
        serde_json::json!([]),
        "seconds[] is always served"
    );
    let standing = doc["decisions"]["standing"].as_object().unwrap();
    assert!(
        !standing.contains_key("seconds_standing"),
        "a derived field is absent when it does not apply: {standing:?}"
    );
    let history = doc["decisions"]["history"][0].as_object().unwrap();
    assert!(!history.contains_key("seconds_standing"));
}

#[test]
fn seconds_standing_counts_only_standing_seconds_and_only_on_the_standing_approval() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    approve_as(&ledger, "sam");
    recorded(second_as(&ledger, "alex", VERSION, "2026-09-22T12:00:00Z"));
    recorded(second_as(&ledger, "kim", VERSION, "2026-09-22T12:05:00Z"));
    let served = view(&ledger);
    assert_eq!(
        served
            .decisions
            .standing
            .as_ref()
            .and_then(|r| r.seconds_standing),
        Some(2)
    );
    // The history entry for the same approval is the record as it was written;
    // only the standing approval carries the derived count.
    assert_eq!(served.decisions.history[0].seconds_standing, None);
    let who: Vec<&str> = served.seconds.iter().map(|s| s.actor.as_str()).collect();
    assert_eq!(who, ["alex", "kim"], "oldest first, in the order appended");
}

/* ── 2. who counts as another person, and what the refusal says ────────── */

#[test]
fn names_are_compared_trimmed_and_case_folded_and_the_refusal_says_the_name_is_not_checked() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    approve_as(&ledger, "Sam");
    for same in ["sam", "  SAM ", "sAm"] {
        assert_eq!(
            second_as(&ledger, same, VERSION, AT),
            Seconding::SamePerson {
                with: "Sam".to_string()
            },
            "{same:?} is the approval's own actor (Q3)"
        );
    }
    recorded(second_as(&ledger, "Alex", VERSION, AT));
    assert_eq!(
        second_as(&ledger, " aLEX ", VERSION, AT),
        Seconding::SamePerson {
            with: "Alex".to_string()
        },
        "a standing second's actor cannot second twice"
    );
    assert_eq!(
        count(&ledger, "change_decision_seconds"),
        1,
        "refusals append nothing"
    );

    // The served sentence: the ruling's words, whole.
    assert_eq!(
        NAME_NOT_CHECKED,
        "ThruLine records the name you type; it does not check who is at the keyboard."
    );
    let said = seconds::same_person_sentence("Alex");
    assert!(said.contains("Alex"), "{said}");
    assert!(said.ends_with(NAME_NOT_CHECKED), "{said}");
}

#[test]
fn a_second_must_name_who_made_it() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    approve_as(&ledger, "sam");
    let decide = Decide {
        actor: "   ".to_string(),
        at: AT.to_string(),
        seen_revision: VERSION.to_string(),
        note: None,
    };
    assert!(seconds::second(&ledger, CHANGE, &decide).is_err());
    assert!(seconds::withdraw(&ledger, CHANGE, "  ", AT).is_err());
    assert_eq!(count(&ledger, "change_decision_seconds"), 0);
}

/* ── 3. what must stand before a second ────────────────────────────────── */

#[test]
fn nothing_to_second_until_an_approval_stands_and_after_it_is_withdrawn() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    assert_eq!(
        second_as(&ledger, "alex", VERSION, AT),
        Seconding::NothingToSecond
    );
    approve_as(&ledger, "sam");
    recorded(second_as(&ledger, "alex", VERSION, AT));
    withdraw_approval(&ledger, "sam");
    assert!(view(&ledger).decisions.standing.is_none());
    assert_eq!(
        second_as(&ledger, "kim", VERSION, AT),
        Seconding::NothingToSecond
    );
    assert!(seconds::NOTHING_TO_SECOND.contains("Nothing was saved"));
    assert_eq!(count(&ledger, "change_decision_seconds"), 1);
}

#[test]
fn another_version_than_the_approvals_is_answered_as_moved_and_writes_nothing() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    approve_as(&ledger, "sam");
    match second_as(&ledger, "alex", "snap_other", AT) {
        Seconding::Moved { seen, now, .. } => {
            assert_eq!(seen, "snap_other");
            assert_eq!(now, VERSION);
        }
        other => panic!("expected Moved, got {other:?}"),
    }
    assert_eq!(count(&ledger, "change_decision_seconds"), 0);
}

#[test]
fn only_a_change_this_store_holds_exists() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    assert!(seconds::exists(&ledger, CHANGE).unwrap());
    assert!(!seconds::exists(&ledger, "no-such-change").unwrap());
}

/* ── 4. taking a second back ───────────────────────────────────────────── */

#[test]
fn a_person_takes_back_only_their_own_standing_second_and_may_second_again() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    let approval = approve_as(&ledger, "sam");
    let alex = recorded(second_as(&ledger, "alex", VERSION, "2026-09-22T12:00:00Z"));
    recorded(second_as(&ledger, "kim", VERSION, "2026-09-22T12:05:00Z"));

    // Someone who never seconded has nothing to take back.
    assert_eq!(
        seconds::withdraw(&ledger, CHANGE, "pat", "2026-09-22T12:10:00Z").unwrap(),
        Withdrawing::NoSecondToWithdraw
    );
    // The approval's own actor seconded nothing either.
    assert_eq!(
        seconds::withdraw(&ledger, CHANGE, "sam", "2026-09-22T12:10:00Z").unwrap(),
        Withdrawing::NoSecondToWithdraw
    );
    assert_eq!(count(&ledger, "change_decision_seconds"), 2);

    // Alex takes theirs back, typed any way the name was typed.
    let Withdrawing::Recorded { withdrawal } =
        seconds::withdraw(&ledger, CHANGE, "  ALEX ", "2026-09-22T12:15:00Z").unwrap()
    else {
        panic!("alex has a standing second to take back");
    };
    assert_eq!(withdrawal.act, SecondAct::Withdrawn);
    assert_eq!(withdrawal.withdraws.as_deref(), Some(alex.id.as_str()));
    assert_eq!(withdrawal.decision_id, approval.id);
    assert_eq!(withdrawal.target_revision, VERSION);
    assert_eq!(withdrawal.actor, "ALEX");

    let served = view(&ledger);
    assert_eq!(
        served
            .decisions
            .standing
            .as_ref()
            .and_then(|r| r.seconds_standing),
        Some(1),
        "only kim's second still counts"
    );
    let counting: Vec<(&str, bool)> = served
        .seconds
        .iter()
        .map(|s| (s.actor.as_str(), s.counting))
        .collect();
    assert_eq!(
        counting,
        [("alex", false), ("kim", true), ("ALEX", false)],
        "rows are history; only an approval no one took back counts"
    );

    // Nothing left to take back; a second time appends nothing.
    assert_eq!(
        seconds::withdraw(&ledger, CHANGE, "alex", "2026-09-22T12:20:00Z").unwrap(),
        Withdrawing::NoSecondToWithdraw
    );
    // And alex may second again: a new row, the earlier one stays.
    recorded(second_as(&ledger, "alex", VERSION, "2026-09-22T12:25:00Z"));
    assert_eq!(standing_count(&ledger), Some(2));
    assert_eq!(count(&ledger, "change_decision_seconds"), 4);
}

/* ── 5. a second belongs to one approval ───────────────────────────────── */

#[test]
fn a_second_stops_counting_with_its_approval_and_is_not_carried_to_the_next() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    approve_as(&ledger, "sam");
    recorded(second_as(&ledger, "alex", VERSION, AT));

    withdraw_approval(&ledger, "sam");
    let after_withdrawal = view(&ledger);
    assert!(after_withdrawal.decisions.standing.is_none());
    assert!(
        !after_withdrawal.seconds[0].counting,
        "its approval was taken back"
    );
    // With nothing standing, a withdrawal of the second has nothing to find.
    assert_eq!(
        seconds::withdraw(&ledger, CHANGE, "alex", AT).unwrap(),
        Withdrawing::NoSecondToWithdraw
    );

    // A new approval of the same version: the earlier second is not on it.
    let again = approve_as(&ledger, "sam");
    let renewed = view(&ledger);
    assert_eq!(
        renewed.decisions.standing.as_ref().map(|r| r.id.clone()),
        Some(again.id.clone())
    );
    assert_eq!(standing_count(&ledger), None);
    assert!(
        !renewed.seconds[0].counting,
        "it is the old approval's second"
    );
    // alex may now second the NEW approval.
    let fresh = recorded(second_as(&ledger, "alex", VERSION, "2026-09-22T13:00:00Z"));
    assert_eq!(fresh.decision_id, again.id);
    assert_eq!(standing_count(&ledger), Some(1));
}

/* ── 6. a second never makes anything stand or rewrites a file ─────────── */

#[test]
fn a_second_never_rewrites_the_decision_file_or_the_approvals_row() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    let approval = approve_as(&ledger, "sam");
    let path = change_decision::record_path_for(project.path(), VERSION);
    let file_before = std::fs::read(&path).expect("the decision file");
    let rows_before = count(&ledger, "change_decisions");
    let row_before: (String, String, String) = ledger
        .conn()
        .query_row(
            "SELECT id, actor, decided_at FROM change_decisions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();

    recorded(second_as(&ledger, "alex", VERSION, AT));
    recorded(second_as(&ledger, "kim", VERSION, AT));
    seconds::withdraw(&ledger, CHANGE, "alex", AT).unwrap();

    assert_eq!(
        std::fs::read(&path).unwrap(),
        file_before,
        "the file is untouched"
    );
    assert_eq!(count(&ledger, "change_decisions"), rows_before);
    let row_after: (String, String, String) = ledger
        .conn()
        .query_row(
            "SELECT id, actor, decided_at FROM change_decisions",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(row_after, row_before);
    let doc: serde_json::Value = serde_json::from_slice(&file_before).unwrap();
    assert_eq!(doc["schema_version"], 3, "the file stays schema 3 (Q11)");
    let text = String::from_utf8(file_before).unwrap();
    assert!(
        !text.contains("alex") && !text.contains("kim"),
        "the file does not name a second: {text}"
    );
    assert_eq!(
        view(&ledger).decisions.standing.map(|r| r.id),
        Some(approval.id),
        "the same approval stands"
    );
}

#[test]
fn a_second_row_cannot_be_edited_or_removed() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    approve_as(&ledger, "sam");
    recorded(second_as(&ledger, "alex", VERSION, AT));
    for sql in [
        "UPDATE change_decision_seconds SET actor='someone else'",
        "DELETE FROM change_decision_seconds",
    ] {
        let err = ledger.conn().execute(sql, params![]).unwrap_err();
        assert!(
            err.to_string().contains("append-only"),
            "{sql} must be refused by the trigger: {err}"
        );
    }
}
