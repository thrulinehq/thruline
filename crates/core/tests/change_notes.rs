//! C30 R-LEDGER — a person's note on a Change, and the Approve note.
//!
//! The sentences these tests hold:
//!
//!   1. a note is kept as typed, with who and when, and served masked;
//!   2. nothing but whitespace, and nothing over 2,000 characters, is kept, and
//!      a refusal writes nothing;
//!   3. an approval or a withdrawal keeps its note in the SAME transaction: the
//!      two commit together or neither does;
//!   4. the decision row and the decision file are byte-identical with and
//!      without a note (the file stays schema 3, and does not carry it);
//!   5. a decision recorded without a note serves exactly the keys it always
//!      did (a derived field is absent when it does not apply);
//!   6. a note is words: readiness, coverage and which approval stands are the
//!      same with and without one.
//!
//! Every id is synthetic and no fixture here contains private content.

use rusqlite::params;
use thruline_core::change_decision::{self, Decide, DecisionAct, Outcome};
use thruline_core::change_note::{self, NoteProblem, Refusal, MAX_NOTE_CHARS};
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

fn decide(note: Option<&str>) -> Decide {
    Decide {
        actor: "sam".to_string(),
        at: "2026-09-22T11:00:00Z".to_string(),
        seen_revision: VERSION.to_string(),
        note: note.map(str::to_string),
    }
}

fn recorded(outcome: Outcome) -> change_decision::DecisionRow {
    match outcome {
        Outcome::Recorded { decision } => decision,
        other => panic!("the act must be recorded: {other:?}"),
    }
}

fn count(ledger: &Ledger, table: &str) -> i64 {
    ledger
        .conn()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .expect("count")
}

const AT: &str = "2026-09-22T12:00:00Z";

/* ── 1. a note is kept as typed, with who and when ─────────────────────── */

#[test]
fn a_note_is_kept_as_typed_with_who_and_when_and_served_masked() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    let canary = format!("ghp_{}", "TLCANARYaaZZ11bbYY22ccXX33ddWW44e");
    let words = format!("  Checked it by hand. {canary}\n");

    let kept = change_note::add(&ledger, CHANGE, "  alex  ", &words, AT)
        .expect("the ledger answers")
        .expect("a note with words is kept");
    assert_eq!(kept.actor, "alex", "the name is kept trimmed");
    assert_eq!(kept.recorded_at, AT);
    assert_eq!(kept.decision_id, None, "a Brief note names no decision");
    assert!(
        !kept.body.contains(&canary),
        "the answer to the writer is masked too: {}",
        kept.body
    );

    // The table keeps the words exactly as typed ...
    let stored: String = ledger
        .conn()
        .query_row("SELECT body FROM change_notes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(stored, words);

    // ... and every reader serves them masked.
    let view = thruline_core::change::read_change(&ledger, CHANGE)
        .unwrap()
        .expect("the change");
    assert_eq!(view.notes.len(), 1);
    assert_eq!(view.notes[0], kept);
    assert!(!view.notes[0].body.contains(&canary));
    match &view.masking {
        thruline_core::redact::Treatment::Ran { masked, .. } => assert!(
            *masked > 0,
            "the masker fired on a note, so the Change's treatment must say so: {:?}",
            view.masking
        ),
        other => panic!("a masking pass ran over the note: {other:?}"),
    }
}

#[test]
fn notes_are_served_oldest_first_in_the_order_they_were_appended() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    for (n, words) in ["first", "second", "third"].into_iter().enumerate() {
        // The same second for all three: the append order decides, not the clock.
        change_note::add(&ledger, CHANGE, "sam", words, AT)
            .unwrap()
            .unwrap_or_else(|r| panic!("note {n} refused: {r:?}"));
    }
    let view = thruline_core::change::read_change(&ledger, CHANGE)
        .unwrap()
        .unwrap();
    let bodies: Vec<&str> = view.notes.iter().map(|n| n.body.as_str()).collect();
    assert_eq!(bodies, ["first", "second", "third"]);
}

/* ── 2. what is not kept ───────────────────────────────────────────────── */

#[test]
fn nothing_but_whitespace_and_nothing_over_the_limit_is_kept_and_a_refusal_writes_nothing() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());

    for blank in ["", "   ", "\n\t \n"] {
        assert_eq!(
            change_note::add(&ledger, CHANGE, "sam", blank, AT).unwrap(),
            Err(Refusal::Note(NoteProblem::Empty)),
            "{blank:?} says nothing"
        );
    }
    let over = "x".repeat(MAX_NOTE_CHARS + 1);
    assert_eq!(
        change_note::add(&ledger, CHANGE, "sam", &over, AT).unwrap(),
        Err(Refusal::Note(NoteProblem::TooLong {
            chars: MAX_NOTE_CHARS + 1
        }))
    );
    assert_eq!(
        count(&ledger, "change_notes"),
        0,
        "a refusal writes nothing"
    );

    // The limit is in characters, not bytes: 2,000 of the widest ones is fine,
    // and the 2,001st is the first one over.
    let widest = "😀".repeat(MAX_NOTE_CHARS);
    assert!(change_note::add(&ledger, CHANGE, "sam", &widest, AT)
        .unwrap()
        .is_ok());
    let exactly = "y".repeat(MAX_NOTE_CHARS);
    assert!(change_note::add(&ledger, CHANGE, "sam", &exactly, AT)
        .unwrap()
        .is_ok());
    assert_eq!(count(&ledger, "change_notes"), 2);
}

#[test]
fn a_note_on_no_change_or_with_no_name_is_refused_and_writes_nothing() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    assert_eq!(
        change_note::add(&ledger, "no-such-change", "sam", "hello", AT).unwrap(),
        Err(Refusal::NoSuchChange)
    );
    assert_eq!(
        change_note::add(&ledger, CHANGE, "   ", "hello", AT).unwrap(),
        Err(Refusal::NoActor)
    );
    assert_eq!(count(&ledger, "change_notes"), 0);
}

/* ── 3. an act keeps its note in its own transaction ───────────────────── */

#[test]
fn an_approval_keeps_its_note_in_the_same_transaction() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    let row = recorded(
        change_decision::approve(&ledger, CHANGE, &decide(Some("Reviewed the diff myself.")))
            .unwrap(),
    );
    assert_eq!(row.note.as_deref(), Some("Reviewed the diff myself."));

    let (decision_id, actor, body, at, change): (String, String, String, String, String) = ledger
        .conn()
        .query_row(
            "SELECT decision_id, actor, body, recorded_at, change_id FROM change_notes",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(
        decision_id, row.id,
        "the note names the row it was kept with"
    );
    assert_eq!(actor, "sam", "the note carries the approval's own name");
    assert_eq!(body, "Reviewed the diff myself.");
    assert_eq!(at, "2026-09-22T11:00:00Z", "and the approval's own time");
    assert_eq!(change, CHANGE);

    // Read back: the standing approval carries it, and so does the Change.
    let view = change_decision::read(&ledger, CHANGE).unwrap();
    assert_eq!(
        view.standing.as_ref().and_then(|s| s.note.as_deref()),
        Some("Reviewed the diff myself.")
    );
    let served = thruline_core::change::read_change(&ledger, CHANGE)
        .unwrap()
        .unwrap();
    assert_eq!(served.notes.len(), 1);
    assert_eq!(
        served.notes[0].decision_id.as_deref(),
        Some(row.id.as_str())
    );
}

#[test]
fn a_withdrawal_keeps_its_note_too() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    let approval = recorded(change_decision::approve(&ledger, CHANGE, &decide(None)).unwrap());
    let withdrawn = recorded(
        change_decision::withdraw(
            &ledger,
            CHANGE,
            &Decide {
                at: "2026-09-22T11:30:00Z".to_string(),
                ..decide(Some("The agent changed the tests after I looked."))
            },
        )
        .unwrap(),
    );
    assert_eq!(withdrawn.act, DecisionAct::Withdrawn);
    let view = change_decision::read(&ledger, CHANGE).unwrap();
    let approval_after = view.history.iter().find(|r| r.id == approval.id).unwrap();
    let withdrawal_after = view.history.iter().find(|r| r.id == withdrawn.id).unwrap();
    assert_eq!(approval_after.note, None, "the approval had no note");
    assert_eq!(
        withdrawal_after.note.as_deref(),
        Some("The agent changed the tests after I looked.")
    );
    let notes: Vec<(String, String)> = ledger
        .conn()
        .prepare("SELECT decision_id, recorded_at FROM change_notes")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(
        notes,
        vec![(withdrawn.id.clone(), "2026-09-22T11:30:00Z".to_string())]
    );
}

#[test]
fn an_act_and_its_note_commit_together_or_not_at_all() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());

    // A note that cannot be written takes the decision down with it: this
    // trigger (on this test's own scratch ledger) makes the note's INSERT fail
    // after the decision row is in.
    ledger
        .conn()
        .execute_batch(
            "CREATE TRIGGER scratch_refuse_notes BEFORE INSERT ON change_notes
             BEGIN SELECT RAISE(ABORT,'scratch: the note cannot be written'); END;",
        )
        .unwrap();
    let refused = change_decision::approve(&ledger, CHANGE, &decide(Some("kept with it")));
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(
        count(&ledger, "change_decisions"),
        0,
        "no approval without its note"
    );
    assert_eq!(count(&ledger, "change_notes"), 0);
    ledger
        .conn()
        .execute_batch("DROP TRIGGER scratch_refuse_notes")
        .unwrap();

    // The same act with the trigger gone is recorded, both rows or neither.
    let row =
        recorded(change_decision::approve(&ledger, CHANGE, &decide(Some("kept with it"))).unwrap());
    assert_eq!(count(&ledger, "change_decisions"), 1);
    assert_eq!(count(&ledger, "change_notes"), 1);
    assert_eq!(row.note.as_deref(), Some("kept with it"));
}

#[test]
fn a_note_over_the_limit_refuses_the_whole_act_before_anything_is_written() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    let over = "z".repeat(MAX_NOTE_CHARS + 1);
    let refused = change_decision::approve(&ledger, CHANGE, &decide(Some(&over)));
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(count(&ledger, "change_decisions"), 0);
    assert_eq!(count(&ledger, "change_notes"), 0);

    // Nothing but whitespace is no note: the approval is recorded without one.
    let row = recorded(change_decision::approve(&ledger, CHANGE, &decide(Some("  \n "))).unwrap());
    assert_eq!(row.note, None);
    assert_eq!(count(&ledger, "change_decisions"), 1);
    assert_eq!(count(&ledger, "change_notes"), 0);
}

#[test]
fn a_refused_approval_keeps_no_note() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    recorded(change_decision::approve(&ledger, CHANGE, &decide(None)).unwrap());
    // One approval stands at a time: a second of the same version is refused,
    // and the words that came with it are not kept either.
    let second = change_decision::approve(&ledger, CHANGE, &decide(Some("and again")));
    assert!(second.is_err(), "{second:?}");
    assert_eq!(count(&ledger, "change_notes"), 0);
}

/* ── 4. the row and the file do not change ─────────────────────────────── */

/// Everything the decision row holds as columns, except the things that are
/// different on every run (its own id and the file's path).
fn row_columns(ledger: &Ledger) -> Vec<String> {
    let (act, target, scope, actor, at, withdraws): (
        String,
        String,
        i64,
        String,
        String,
        Option<String>,
    ) = ledger
        .conn()
        .query_row(
            "SELECT act, target_revision, scope_version, actor, decided_at, withdraws
                   FROM change_decisions",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap();
    vec![
        act,
        target,
        scope.to_string(),
        actor,
        at,
        format!("{withdraws:?}"),
    ]
}

fn without_the_row_id(file: &std::path::Path) -> serde_json::Value {
    let mut doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(file).expect("the decision file")).expect("json");
    doc.as_object_mut().unwrap().remove("decision_id");
    doc
}

#[test]
fn the_decision_row_and_the_decision_file_are_the_same_with_and_without_a_note() {
    let words = "Reviewed the diff myself; the file list is the one I expected.";
    let without_root = tempfile::tempdir().unwrap();
    let with_root = tempfile::tempdir().unwrap();
    let (_d1, plain) = store(without_root.path());
    let (_d2, noted) = store(with_root.path());
    let a = recorded(change_decision::approve(&plain, CHANGE, &decide(None)).unwrap());
    let b = recorded(change_decision::approve(&noted, CHANGE, &decide(Some(words))).unwrap());

    assert_eq!(row_columns(&plain), row_columns(&noted));

    let file_a = change_decision::record_path_for(without_root.path(), VERSION);
    let file_b = change_decision::record_path_for(with_root.path(), VERSION);
    let (doc_a, doc_b) = (without_the_row_id(&file_a), without_the_row_id(&file_b));
    assert_eq!(doc_a, doc_b, "the file is the same document");
    assert_eq!(
        doc_b["schema_version"],
        serde_json::json!(change_decision::DECISION_RECORD_SCHEMA_VERSION),
        "and it stays at schema 3"
    );
    let text = std::fs::read_to_string(&file_b).unwrap();
    assert!(
        !text.contains(words),
        "the decision file does not carry the note (ruling Q11)"
    );
    assert_ne!(a.id, b.id);
}

/* ── 5. the shape of a decision row ────────────────────────────────────── */

#[test]
fn a_decision_without_a_note_serves_exactly_the_keys_it_always_did() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    let approval = recorded(change_decision::approve(&ledger, CHANGE, &decide(None)).unwrap());

    let keys = |row: &change_decision::DecisionRow| -> Vec<String> {
        serde_json::to_value(row)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    };
    let history = change_decision::read(&ledger, CHANGE).unwrap().history;
    let mut expected = vec![
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
    expected.sort_unstable();
    let mut served = keys(&history[0]);
    served.sort();
    assert_eq!(served, expected, "no `note` key when there is no note");
    assert_eq!(keys(&approval).len(), expected.len());

    // With a note, exactly one key more.
    let project2 = tempfile::tempdir().unwrap();
    let (_d2, other) = store(project2.path());
    let noted = recorded(change_decision::approve(&other, CHANGE, &decide(Some("hi"))).unwrap());
    let mut with = keys(&noted);
    with.sort();
    let mut plus = expected.clone();
    plus.push("note");
    plus.sort_unstable();
    assert_eq!(with, plus);
}

/* ── 6. a note is words ────────────────────────────────────────────────── */

/// Everything a count, readiness or an approval's standing is read from, as the
/// bytes each serves.
fn results(ledger: &Ledger) -> Vec<(&'static str, Vec<u8>)> {
    let environment =
        thruline_core::change_propose::machine_environment(ledger, CHANGE).expect("environment");
    let coverage =
        thruline_core::change_evidence::coverage(ledger, CHANGE, &environment).expect("coverage");
    let readiness =
        thruline_core::change_readiness::readiness(ledger, CHANGE, &coverage).expect("readiness");
    let decisions = change_decision::read(ledger, CHANGE).expect("decisions");
    vec![
        ("coverage", serde_json::to_vec(&coverage).unwrap()),
        ("readiness", serde_json::to_vec(&readiness).unwrap()),
        (
            "which approval stands",
            serde_json::to_vec(&decisions.standing.as_ref().map(|s| s.id.clone())).unwrap(),
        ),
        ("decisions", serde_json::to_vec(&decisions).unwrap()),
    ]
}

#[test]
fn a_note_added_to_the_change_moves_no_result() {
    let project = tempfile::tempdir().unwrap();
    let (_dir, ledger) = store(project.path());
    recorded(change_decision::approve(&ledger, CHANGE, &decide(None)).unwrap());
    let before = results(&ledger);
    for words in ["looks right to me", "waiting on the other reviewer"] {
        change_note::add(&ledger, CHANGE, "alex", words, AT)
            .unwrap()
            .unwrap();
    }
    let after = results(&ledger);
    for ((name, was), (_, now)) in before.iter().zip(after.iter()) {
        assert!(
            was == now,
            "{name} changed because a note was kept:\nbefore {}\nafter  {}",
            String::from_utf8_lossy(was),
            String::from_utf8_lossy(now)
        );
    }
}

#[test]
fn an_approval_given_with_a_note_gives_the_same_counts_readiness_and_standing() {
    let (a_root, b_root) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (_d1, plain) = store(a_root.path());
    let (_d2, noted) = store(b_root.path());
    recorded(change_decision::approve(&plain, CHANGE, &decide(None)).unwrap());
    recorded(change_decision::approve(&noted, CHANGE, &decide(Some("a note"))).unwrap());

    let (a, b) = (results(&plain), results(&noted));
    for name in ["coverage", "readiness"] {
        let find = |r: &Vec<(&'static str, Vec<u8>)>| {
            r.iter().find(|(n, _)| *n == name).unwrap().1.clone()
        };
        assert!(
            find(&a) == find(&b),
            "{name} differs because an approval carried a note"
        );
    }
    // The standing approval is the same one in every way a result can see,
    // except the words it carries.
    let standing = |ledger: &Ledger| {
        let mut row = serde_json::to_value(
            change_decision::read(ledger, CHANGE)
                .unwrap()
                .standing
                .unwrap(),
        )
        .unwrap();
        let object = row.as_object_mut().unwrap();
        for key in ["id", "record_path", "note"] {
            object.remove(key);
        }
        row
    };
    assert_eq!(standing(&plain), standing(&noted));
}
