//! C30 (HQ "What counts as done"): a person's own lines and out-of-scope items,
//! written by the one acceptance act ([`change_propose::accept_with`]) and by
//! nothing else.
//!
//! What these tests hold:
//!
//! * `accept` is `accept_with` with nothing added, byte for byte in the store;
//! * a line the person wrote is accepted in their words and attributed to them;
//! * out of scope is added to what the standing version excludes, and never
//!   removes anything;
//! * an empty part, or a line already in the list, is refused and nothing is
//!   written;
//! * none of it approves anything.
//!
//! Every id is synthetic (`cccccccc-…`) and no fixture here holds private
//! content.

use std::collections::BTreeMap;

use rusqlite::params;
use thruline_core::change::{
    self, AcceptedScope, AssociationProvenance, ChangeRequest, NewChange, RequestSource,
};
use thruline_core::change_link;
use thruline_core::change_propose::{
    self, Acceptance, Additions, Attribution, ProfileEnvironment, ScopeChoice, Selection,
};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p0a1";
const SESSION: &str = "cccccccc-0000-4000-8000-00000000s0a1";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c0a1";
const BARREN: &str = "cccccccc-0000-4000-8000-00000000c0a2";

/// One checkable sentence (proposal 1) and one that stays a note.
const PURPOSE: &str =
    "Uploads that fail once are lost. When an upload fails, the queue keeps one entry for it.";

fn store() -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open a fresh ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, '/tmp/synthetic-additions', 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
            params![PROJECT],
        )
        .expect("insert project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES(?1, ?2, '/tmp/synthetic-additions/s.jsonl', '2026-01-02T09:30:00Z')",
            params![SESSION, PROJECT],
        )
        .expect("insert session");
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('cccccccc-ev-a1', ?1, ?2, 1, '2026-01-02T09:31:00Z', 'tool_use', 'Edit',
                 '{\"file_path\":\"src/upload/queue.rs\"}', '/tmp/s.jsonl', 1, 'x', '', '')",
            params![PROJECT, SESSION],
        )
        .expect("insert edit");
    for (id, purpose) in [
        (CHANGE, PURPOSE),
        (BARREN, "Things are bad and should be better."),
    ] {
        change::insert_change(
            &ledger,
            &NewChange {
                id: id.to_string(),
                project_id: PROJECT.to_string(),
                opened_at: "2026-01-02T09:00:00Z".to_string(),
                request: ChangeRequest {
                    title: "Keep failed uploads".to_string(),
                    purpose: purpose.to_string(),
                    prompt_event_id: None,
                    requester: "sam".to_string(),
                    source: RequestSource::Person,
                },
            },
        )
        .expect("insert the change");
    }
    change_link::decide(
        &ledger,
        CHANGE,
        SESSION,
        AssociationProvenance::Confirmed,
        "sam",
        "this is the upload work",
        "2026-01-02T09:10:00Z",
    )
    .expect("confirm the link");
    (dir, ledger)
}

/// A standing version nobody accepted yet, holding one exclusion: what an
/// acceptance carries forward.
fn with_an_exclusion(ledger: &Ledger) {
    change::append_scope(
        ledger,
        CHANGE,
        "2026-01-02T09:05:00Z",
        &AcceptedScope {
            criteria: Vec::new(),
            exclusions: vec!["anything about billing".to_string()],
            allowed_scope: vec!["src/upload/**".to_string()],
            constraints: Vec::new(),
            check_profile: Vec::new(),
        },
    )
    .expect("append the first scope");
}

fn accepting(selection: Selection) -> Acceptance {
    Acceptance {
        selection,
        scope: ScopeChoice::AsProposed,
        replacing: false,
        edits: BTreeMap::new(),
        actor: "sam".to_string(),
        rationale: String::new(),
        accepted_at: "2026-01-02T11:00:00Z".to_string(),
    }
}

fn adding(lines: &[&str], exclusions: &[&str]) -> Additions {
    Additions {
        lines: lines.iter().map(|s| s.to_string()).collect(),
        exclusions: exclusions.iter().map(|s| s.to_string()).collect(),
    }
}

fn machine() -> ProfileEnvironment {
    ProfileEnvironment::empty()
}

fn versions(ledger: &Ledger, change_id: &str) -> i64 {
    ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM change_scope_revisions WHERE change_id=?1",
            params![change_id],
            |row| row.get(0),
        )
        .expect("count")
}

fn stored(ledger: &Ledger, change_id: &str) -> Vec<(u32, String, String, String)> {
    let mut statement = ledger
        .conn()
        .prepare(
            "SELECT scope_version, criteria_json, exclusions_json, content_sha256
               FROM change_scope_revisions WHERE change_id=?1 ORDER BY scope_version",
        )
        .expect("prepare");
    statement
        .query_map(params![change_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .expect("query")
        .map(|r| r.expect("row"))
        .collect()
}

#[test]
fn accept_is_accept_with_nothing_added() {
    let (_a, one) = store();
    let (_b, two) = store();
    let plain = change_propose::accept(&one, CHANGE, &accepting(Selection::All), &machine())
        .expect("accept");
    let with = change_propose::accept_with(
        &two,
        CHANGE,
        &accepting(Selection::All),
        &Additions::default(),
        &machine(),
    )
    .expect("accept_with");
    assert_eq!(plain, with, "the same act, the same answer");
    assert_eq!(stored(&one, CHANGE), stored(&two, CHANGE), "the same rows");
}

#[test]
fn a_line_the_person_wrote_is_accepted_in_their_words_after_the_chosen_proposals() {
    let (_dir, ledger) = store();
    let view = change_propose::propose(&ledger, CHANGE, &machine()).expect("propose");
    assert!(!view.criteria.is_empty(), "{:?}", view.criteria);
    let mine = "The queue never holds two entries for one upload.";
    let outcome = change_propose::accept_with(
        &ledger,
        CHANGE,
        &accepting(Selection::Numbers(vec![1])),
        &adding(&[mine], &[]),
        &machine(),
    )
    .expect("accept");
    let texts: Vec<&str> = outcome.criteria.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts, vec![view.criteria[0].text.as_str(), mine]);
    assert_eq!(outcome.criteria[0].attribution, Attribution::AsProposed);
    assert_eq!(
        outcome.criteria[1].attribution,
        Attribution::YourWords,
        "a line nobody proposed is the person's, never ours"
    );
    let read = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    let scope = read.accepted_scope.expect("standing");
    assert_eq!(
        scope.criteria,
        vec![view.criteria[0].text.clone(), mine.to_string()]
    );
    assert_eq!(scope.acceptance.expect("accepted").actor, "sam");
    // Accepting is not approving.
    let approvals: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM change_decisions", [], |row| {
            row.get(0)
        })
        .expect("count");
    assert_eq!(approvals, 0);
}

#[test]
fn lines_alone_are_enough_to_accept_even_with_nothing_proposed() {
    let (_dir, ledger) = store();
    let view = change_propose::propose(&ledger, BARREN, &machine()).expect("propose");
    assert!(
        view.criteria.is_empty(),
        "the barren Change proposes nothing"
    );
    let outcome = change_propose::accept_with(
        &ledger,
        BARREN,
        &accepting(Selection::Numbers(Vec::new())),
        &adding(&["The importer rejects a file with no header row."], &[]),
        &machine(),
    )
    .expect("a person's own line is enough");
    assert_eq!(outcome.version, 1);
    assert_eq!(outcome.criteria.len(), 1);
    assert_eq!(outcome.criteria[0].attribution, Attribution::YourWords);
}

#[test]
fn an_empty_line_is_refused_and_nothing_is_written() {
    let (_dir, ledger) = store();
    let refused = change_propose::accept_with(
        &ledger,
        CHANGE,
        &accepting(Selection::Numbers(vec![1])),
        &adding(&["   "], &[]),
        &machine(),
    )
    .expect_err("an empty line is not a line");
    assert!(refused.to_string().contains("empty"), "{refused}");
    assert_eq!(
        versions(&ledger, CHANGE),
        0,
        "a refused acceptance wrote a version"
    );
}

#[test]
fn a_line_already_in_the_list_is_refused_and_nothing_is_written() {
    let (_dir, ledger) = store();
    let view = change_propose::propose(&ledger, CHANGE, &machine()).expect("propose");
    let again = view.criteria[0].text.to_uppercase();
    let refused = change_propose::accept_with(
        &ledger,
        CHANGE,
        &accepting(Selection::Numbers(vec![1])),
        &adding(&[again.as_str()], &[]),
        &machine(),
    )
    .expect_err("the same line twice");
    assert!(refused.to_string().contains("twice"), "{refused}");
    let refused = change_propose::accept_with(
        &ledger,
        CHANGE,
        &accepting(Selection::Numbers(Vec::new())),
        &adding(&["Each retry is logged.", "each retry is logged"], &[]),
        &machine(),
    )
    .expect_err("one of the person's lines twice");
    assert!(refused.to_string().contains("twice"), "{refused}");
    assert_eq!(versions(&ledger, CHANGE), 0);
}

#[test]
fn out_of_scope_is_added_to_what_the_version_already_excludes_and_removes_nothing() {
    let (_dir, ledger) = store();
    with_an_exclusion(&ledger);
    change_propose::accept_with(
        &ledger,
        CHANGE,
        &accepting(Selection::All),
        &adding(&[], &[" refunds ", "anything about billing"]),
        &machine(),
    )
    .expect("accept v2");
    let read = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    assert_eq!(
        read.accepted_scope.expect("standing").exclusions,
        vec!["anything about billing".to_string(), "refunds".to_string()],
        "carried forward, the new one added once, trimmed"
    );
    // A later version that names other things keeps every earlier one.
    let mut again = accepting(Selection::All);
    again.accepted_at = "2026-01-02T12:00:00Z".to_string();
    change_propose::accept_with(
        &ledger,
        CHANGE,
        &again,
        &adding(&[], &["currency rounding"]),
        &machine(),
    )
    .expect("out of scope alone is a new version");
    let read = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    let scope = read.accepted_scope.expect("standing");
    assert_eq!(scope.version, 3);
    assert_eq!(
        scope.exclusions,
        vec![
            "anything about billing".to_string(),
            "refunds".to_string(),
            "currency rounding".to_string()
        ]
    );
}

#[test]
fn an_empty_out_of_scope_item_is_refused_and_nothing_is_written() {
    let (_dir, ledger) = store();
    let refused = change_propose::accept_with(
        &ledger,
        CHANGE,
        &accepting(Selection::All),
        &adding(&[], &[""]),
        &machine(),
    )
    .expect_err("an empty item is not an item");
    assert!(refused.to_string().contains("empty"), "{refused}");
    assert_eq!(versions(&ledger, CHANGE), 0);
}
