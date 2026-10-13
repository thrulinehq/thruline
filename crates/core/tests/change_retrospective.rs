//! QUAL-F2-R2 (`Q7`, brief §16 row 7) — what counts as done, accepted after the
//! work it covers was already in the record, is served as a retrospective
//! acceptance with both times, and is never presented as earlier agreement.
//!
//! "The work" is the one thing the product itself records about a Change's
//! tree: the `intended` examined-inputs records. The first is written when the
//! Change is opened; a later one at a different **revision** is a commit that
//! landed since, as the record holds it. An acceptance made strictly after such
//! a record is retrospective.
//!
//! What is deliberately NOT work, each pinned below because each would put a
//! false "after the work was done" in front of a person:
//!
//! * a re-reading of the same revision (a baseline run, a second prepare);
//! * a different set of uncommitted files at the same revision — build output
//!   and editor files are uncommitted files too;
//! * a record whose time cannot be read, or that shares the acceptance's second;
//! * a revision that moved AGAIN at or after the acceptance — then the work was
//!   not done when the list was accepted, and the product says nothing.
//!
//! Every test reads the served JSON rather than a Rust field, so the file
//! compiles against a tree that has no such field and fails there on an
//! assertion — which is what a test written first has to do. Every one goes
//! through `field()`, which asserts the key is PRESENT, so no test can pass at
//! a tree that has no field by reading a missing key as `null`.
//!
//! Every id is synthetic. No fixture here contains private content.

use rusqlite::params;
use serde_json::Value;
use thruline_core::change::{
    self, AcceptedScope, ChangeRequest, NewChange, RequestSource, ScopeAcceptance,
};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p0f2";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c0f2";
const OTHER_CHANGE: &str = "cccccccc-0000-4000-8000-00000000c0f3";
const BEFORE: &str = "a6b4518000000000000000000000000000000001";
const AFTER: &str = "3062e37400000000000000000000000000000002";
const LATER: &str = "9f00d00000000000000000000000000000000003";

const C1: &str = "Two retry requests for the same delivery do not enqueue duplicate work.";
const C2: &str = "A delivery that was still waiting when the service was stopped is still exactly one delivery after it starts again.";

fn store() -> Ledger {
    let ledger = Ledger::open_in_memory().expect("open an in-memory ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, '/fixture/retry-delivery', 'claude_code', 'Retry delivery',
                    '2026-09-26T08:00:00Z')",
            params![PROJECT],
        )
        .expect("insert project");
    open_change(&ledger, CHANGE);
    ledger
}

fn open_change(ledger: &Ledger, id: &str) {
    change::insert_change(
        ledger,
        &NewChange {
            id: id.to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-09-26T09:00:00Z".to_string(),
            request: ChangeRequest {
                title: "Retry a failed delivery".to_string(),
                purpose: "Two retry requests for the same delivery do not enqueue duplicate work."
                    .to_string(),
                prompt_event_id: None,
                requester: "sam".to_string(),
                source: RequestSource::Person,
            },
        },
    )
    .expect("insert change");
}

/// One `intended` examined-inputs record for `change`, as `change new`,
/// `change prepare` and `check --for-change` write it: the tree's revision,
/// and whether (and how) it held uncommitted files.
fn record(ledger: &Ledger, change: &str, id: &str, at: &str, revision: &str, dirty: Option<&str>) {
    let (state, files, digest) = match dirty {
        None => ("clean", None, None),
        Some(digest) => ("present", Some(2_i64), Some(digest)),
    };
    ledger
        .conn()
        .execute(
            "INSERT INTO examination_inputs(id, subject_kind, subject_id, recorded_at,
                 schema_version, base_revision, uncommitted_state, uncommitted_files,
                 uncommitted_sha256, worktree_path, config_files_json, tools_json, target_kind)
             VALUES(?1, 'intended', ?2, ?3, 1, ?4, ?5, ?6, ?7, '/fixture/retry-delivery',
                    '[]', '[]', 'none')",
            params![id, change, at, revision, state, files, digest],
        )
        .expect("insert intended record");
}

fn intended(ledger: &Ledger, id: &str, at: &str, revision: &str, dirty: Option<&str>) {
    record(ledger, CHANGE, id, at, revision, dirty);
}

fn accept(ledger: &Ledger, at: &str, criteria: &[&str]) -> u32 {
    let scope = AcceptedScope {
        criteria: criteria.iter().map(|c| c.to_string()).collect(),
        allowed_scope: vec!["src/**".to_string()],
        ..AcceptedScope::default()
    };
    let version = change::append_scope(ledger, CHANGE, at, &scope).expect("append scope");
    change::accept_scope(
        ledger,
        CHANGE,
        version,
        &ScopeAcceptance {
            actor: "sam".to_string(),
            accepted_at: at.to_string(),
            rationale: "accepting what was built".to_string(),
        },
    )
    .expect("accept scope");
    version
}

fn served(ledger: &Ledger) -> Value {
    let view = change::read_change(ledger, CHANGE)
        .expect("read")
        .expect("the change exists");
    serde_json::to_value(&view).expect("the view serializes")
}

/// The served `retrospective`, after asserting the key is there at all: the
/// field is served on every Change, so a reader can tell "not retrospective"
/// (`null`) from a build that has no such field (no key).
fn field(json: &Value) -> &Value {
    let served = json
        .as_object()
        .is_some_and(|o| o.contains_key("retrospective"));
    assert!(
        served,
        "`retrospective` is not among the served fields: {:?}",
        json.as_object().map(|o| o.keys().collect::<Vec<_>>())
    );
    &json["retrospective"]
}

fn assert_silent(ledger: &Ledger, why: &str) {
    let json = served(ledger);
    assert!(field(&json).is_null(), "{why}; got {}", field(&json));
}

// ---------------------------------------------------------------------------
// The case (`Q07` V1): opened, implemented, prepared, THEN accepted.
// ---------------------------------------------------------------------------

#[test]
fn an_acceptance_made_after_the_work_was_recorded_is_served_as_retrospective() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1, C2]);

    let json = served(&ledger);
    let r = field(&json);
    assert!(
        r.is_object(),
        "an acceptance made after the work was recorded must be served as retrospective; got {r}"
    );
    assert_eq!(r["scope_version"], 1);
    assert_eq!(
        r["accepted_at"], "2026-09-26T09:20:00Z",
        "the acceptance's own time, unchanged"
    );
    assert_eq!(
        r["accepted_on"], "2026-09-26",
        "the UTC date of that instant, for the sentence that carries the date"
    );
    assert_eq!(
        r["work_recorded_at"], "2026-09-26T09:10:00Z",
        "when the work first entered the record"
    );
    assert_eq!(
        r["work_record_id"], "ei-work",
        "the record the fact rests on, by id"
    );
    assert_eq!(r["opened_revision"], BEFORE);
    assert_eq!(r["work_revision"], AFTER);
    /* Never earlier agreement: the served acceptance time is the row's own,
     * and it is after the work. */
    assert_eq!(
        json["accepted_scope"]["acceptance"]["accepted_at"],
        "2026-09-26T09:20:00Z"
    );
}

/// The served shape's version moves when a field arrives (the convention in
/// `CHANGE_VIEW_SCHEMA_VERSION`'s own comment), so a reader can tell a new
/// field from a missing one. 5 was STEP-7-GIT-LINE's `code_moved`; 6 was this
/// field; 7 is C30 R-LEDGER's `notes`.
#[test]
fn a_new_served_field_moves_the_shape_version() {
    let ledger = store();
    let json = served(&ledger);
    assert!(field(&json).is_null());
    assert_eq!(json["schema_version"], 8);
}

// ---------------------------------------------------------------------------
// The control (`Q07` CTL): accepted first, THEN the work.
// ---------------------------------------------------------------------------

#[test]
fn an_acceptance_made_before_the_work_is_not_retrospective() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    accept(&ledger, "2026-09-26T09:05:00Z", &[C1, C2]);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);

    assert_silent(&ledger, "accepted first: nothing says after");
}

#[test]
fn a_change_with_nothing_accepted_still_serves_the_field() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);

    assert_silent(
        &ledger,
        "nothing accepted, so nothing was accepted after anything",
    );
}

#[test]
fn a_change_with_no_examined_inputs_at_all_says_nothing() {
    let ledger = store();
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);

    assert_silent(&ledger, "no record of the tree, so no record of any work");
}

// ---------------------------------------------------------------------------
// What is NOT work.
// ---------------------------------------------------------------------------

/// A baseline run before accepting — the tests run red on the untouched tree,
/// then the criteria are accepted, then the work starts — reads the same
/// revision again. Nothing moved, so nothing was done yet.
#[test]
fn a_rereading_of_the_same_revision_before_acceptance_is_not_work() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-baseline", "2026-09-26T09:03:00Z", BEFORE, None);
    accept(&ledger, "2026-09-26T09:05:00Z", &[C1]);

    assert_silent(&ledger, "the same revision read twice is not work");
}

/// The tree fingerprint counts untracked files, so a baseline `cargo test`
/// that leaves a `Cargo.lock` behind changes the uncommitted digest of a
/// project that did not change at all. That is a file the run made, not work
/// somebody did, and "accepted after the work was done" would be false.
#[test]
fn uncommitted_files_appearing_at_the_same_revision_are_not_work() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(
        &ledger,
        "ei-baseline",
        "2026-09-26T09:03:00Z",
        BEFORE,
        Some(&"d".repeat(64)),
    );
    accept(&ledger, "2026-09-26T09:05:00Z", &[C1]);

    assert_silent(
        &ledger,
        "a different set of uncommitted files at the same revision is not a commit",
    );
}

/// A Change opened on a tree that already held the work shows no move, so the
/// product says nothing: silence, never "accepted before".
#[test]
fn a_change_opened_on_the_finished_tree_says_nothing() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", AFTER, None);
    intended(&ledger, "ei-again", "2026-09-26T09:10:00Z", AFTER, None);
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);

    assert_silent(
        &ledger,
        "the record never saw the tree before the work, so it cannot say the work came first",
    );
}

/// Same second, stamps at one-second resolution: which came first is not in
/// the record, so the product says nothing rather than guess "after".
#[test]
fn a_record_in_the_same_second_as_the_acceptance_is_not_before_it() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:05:00Z", AFTER, None);
    accept(&ledger, "2026-09-26T09:05:00Z", &[C1]);

    assert_silent(&ledger, "a tie is not 'before'");
}

/// A record whose time cannot be read cannot be ordered against anything.
#[test]
fn a_record_whose_time_cannot_be_read_is_never_counted_as_before() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "sometime on friday", AFTER, None);
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);

    assert_silent(
        &ledger,
        "an unreadable record time is ordered against nothing",
    );
}

/// The same holds for the acceptance's own time.
#[test]
fn an_acceptance_whose_time_cannot_be_read_is_never_retrospective() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);
    accept(&ledger, "the twentieth", &[C1]);

    assert_silent(
        &ledger,
        "an unreadable acceptance time is ordered against nothing",
    );
}

/// Another Change's records, on the same project, are not this Change's work.
#[test]
fn another_changes_records_are_not_this_changes_work() {
    let ledger = store();
    open_change(&ledger, OTHER_CHANGE);
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    record(
        &ledger,
        OTHER_CHANGE,
        "ei-other-work",
        "2026-09-26T09:10:00Z",
        AFTER,
        None,
    );
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);

    assert_silent(
        &ledger,
        "a commit the OTHER Change recorded is not the work of this one",
    );
}

// ---------------------------------------------------------------------------
// Instants, not spellings.
// ---------------------------------------------------------------------------

/// `.500Z` sorts below `Z` as text and is later as an instant.
#[test]
fn a_fractional_stamp_is_ordered_by_its_instant_not_its_spelling() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:05:00.500Z", AFTER, None);
    accept(&ledger, "2026-09-26T09:05:00Z", &[C1]);

    assert_silent(
        &ledger,
        "09:05:00.500 is after 09:05:00, although '.500Z' sorts before 'Z'",
    );
}

#[test]
fn a_fractional_stamp_just_before_the_acceptance_is_before_it() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:04:59.999Z", AFTER, None);
    accept(&ledger, "2026-09-26T09:05:00Z", &[C1]);

    let json = served(&ledger);
    assert!(field(&json).is_object(), "{}", field(&json));
    assert_eq!(field(&json)["work_record_id"], "ei-work");
}

/// A stamp with a numeric offset is a different spelling of an instant. Text
/// says 09:05 is before 09:10; the instants say 10:05Z is after 09:10Z.
#[test]
fn an_offset_stamp_is_ordered_by_its_instant_not_its_spelling() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);
    accept(&ledger, "2026-09-26T09:05:00-01:00", &[C1]);

    let json = served(&ledger);
    assert!(
        field(&json).is_object(),
        "09:05-01:00 is 10:05Z, after the work at 09:10Z; got {}",
        field(&json)
    );
}

#[test]
fn an_offset_stamp_that_reads_later_but_is_earlier_is_not_after() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);
    // 14:00+05:30 is 08:30Z: before the work, although the text says later.
    accept(&ledger, "2026-09-26T14:00:00+05:30", &[C1]);

    assert_silent(&ledger, "14:00+05:30 is 08:30Z, before the work at 09:10Z");
}

/// `+00:00` is UTC spelled another way: the same instant as `Z`.
#[test]
fn a_utc_offset_spelling_is_the_same_instant_as_z() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:05:00+00:00", AFTER, None);
    accept(&ledger, "2026-09-26T09:05:00Z", &[C1]);

    assert_silent(
        &ledger,
        "the same instant is a tie, and a tie is not 'before'",
    );
}

/// The date said is the UTC date of the instant, not the date its spelling
/// happens to start with.
#[test]
fn the_date_is_the_utc_date_of_the_acceptance() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-27T01:00:00Z", AFTER, None);
    // 23:30 at UTC-05:00 on the 26th is 04:30Z on the 27th.
    accept(&ledger, "2026-09-26T23:30:00-05:00", &[C1]);

    let json = served(&ledger);
    assert!(field(&json).is_object(), "{}", field(&json));
    assert_eq!(field(&json)["accepted_on"], "2026-09-27");
    assert_eq!(
        field(&json)["accepted_at"],
        "2026-09-26T23:30:00-05:00",
        "the row's own spelling is served unchanged"
    );
}

// ---------------------------------------------------------------------------
// Work that continued, and work that took more than one step.
// ---------------------------------------------------------------------------

/// The work was recorded in two steps before the list was accepted: it is
/// still one acceptance after the work, and the first move is the receipt.
#[test]
fn work_recorded_in_two_steps_before_acceptance_names_the_first_move() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work-1", "2026-09-26T09:10:00Z", AFTER, None);
    intended(&ledger, "ei-work-2", "2026-09-26T09:15:00Z", LATER, None);
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);

    let json = served(&ledger);
    assert_eq!(field(&json)["work_record_id"], "ei-work-1");
    assert_eq!(field(&json)["work_revision"], AFTER);
}

/// A check run after the acceptance, on the tree that was accepted, is the
/// ordinary next step and does not un-say the fact.
#[test]
fn a_later_record_at_the_accepted_revision_leaves_the_fact_standing() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);
    intended(&ledger, "ei-check", "2026-09-26T09:30:00Z", AFTER, None);

    let json = served(&ledger);
    assert!(field(&json).is_object(), "{}", field(&json));
}

/// If the revision moved AGAIN after the list was accepted, the work was not
/// done when it was accepted. "Accepted after the work was done" would be
/// false, so the product says nothing rather than say it.
#[test]
fn work_that_moved_again_after_the_acceptance_says_nothing() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);
    intended(&ledger, "ei-more", "2026-09-26T09:30:00Z", LATER, None);

    assert_silent(
        &ledger,
        "the record shows a later commit after the acceptance: the work was not done yet",
    );
}

/// A record after the acceptance whose time cannot be read, at a different
/// revision, cannot be shown to be before it: not shown, so not claimed.
#[test]
fn a_later_move_with_an_unreadable_time_says_nothing() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);
    intended(&ledger, "ei-more", "some time later", LATER, None);

    assert_silent(&ledger, "a move whose time is unknown may have come after");
}

// ---------------------------------------------------------------------------
// Do not invent earlier agreement — and do not erase it either.
// ---------------------------------------------------------------------------

/// A list accepted before the work and then re-accepted after it: the standing
/// acceptance is the later one and is retrospective, and the earlier one stays
/// in the history under its own time, untouched.
#[test]
fn a_later_list_accepted_after_the_work_is_retrospective_and_the_earlier_one_keeps_its_time() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    accept(&ledger, "2026-09-26T09:05:00Z", &[C1]);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1, C2]);

    let json = served(&ledger);
    let r = field(&json);
    assert!(
        r.is_object(),
        "the standing list was accepted after the work; got {r}"
    );
    assert_eq!(r["scope_version"], 2);
    assert_eq!(r["accepted_at"], "2026-09-26T09:20:00Z");
    let history = json["scope_history"].as_array().expect("history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0]["version"], 1);
    assert_eq!(
        history[0]["acceptance"]["accepted_at"],
        "2026-09-26T09:05:00Z"
    );
}

// ---------------------------------------------------------------------------
// Derived at read, stored nowhere.
// ---------------------------------------------------------------------------

fn census(ledger: &Ledger) -> Vec<(String, i64)> {
    let conn = ledger.conn();
    let mut names = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .expect("names");
    let names: Vec<String> = names
        .query_map([], |r| r.get::<_, String>(0))
        .expect("query")
        .map(|r| r.expect("name"))
        .collect();
    names
        .into_iter()
        .map(|n| {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM \"{n}\""), [], |r| r.get(0))
                .expect("count");
            (n, count)
        })
        .collect()
}

#[test]
fn reading_the_fact_writes_nothing_and_answers_the_same_twice() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);

    let before = census(&ledger);
    let first = served(&ledger);
    let second = served(&ledger);
    assert!(field(&first).is_object());
    assert_eq!(field(&first), field(&second));
    assert_eq!(
        before,
        census(&ledger),
        "a read changed a table's row count"
    );
}

// ---------------------------------------------------------------------------
// Self-attack (written after the first fix, against it): the hostile cases the
// rule has to survive. Each one is a way the sentence could be made false.
// ---------------------------------------------------------------------------

/// A blank revision is a revision that could not be read, not a commit: a move
/// FROM it is not a move, and the Change was never seen on a real tree.
#[test]
fn a_blank_opened_revision_is_no_baseline() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", "", None);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);

    assert_silent(&ledger, "a blank revision is no tree to have moved from");
}

#[test]
fn a_blank_work_revision_is_no_move() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", "  ", None);
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);

    assert_silent(&ledger, "a blank revision is a revision nobody read");
}

/// The Change was bound only after it was accepted (opened with no folder,
/// prepared later): the first tree the record ever saw is after the
/// acceptance, so there is no earlier tree the work moved from.
#[test]
fn a_tree_first_seen_after_the_acceptance_is_no_baseline() {
    let ledger = store();
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);
    intended(&ledger, "ei-first", "2026-09-26T09:30:00Z", BEFORE, None);
    intended(&ledger, "ei-later", "2026-09-26T09:40:00Z", AFTER, None);

    assert_silent(&ledger, "the record never saw a tree before the acceptance");
}

/// Moved to another commit and then taken back to the opened one before the
/// list was accepted: at the acceptance the record shows the opened tree.
#[test]
fn moved_away_and_back_before_the_acceptance_says_nothing() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);
    intended(&ledger, "ei-back", "2026-09-26T09:15:00Z", BEFORE, None);
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);

    assert_silent(
        &ledger,
        "the tree standing at the acceptance is the opened one",
    );
}

/// One second is enough: the boundary is strict, not fuzzy.
#[test]
fn one_second_before_the_acceptance_is_before_it() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:19:59Z", AFTER, None);
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);

    let json = served(&ledger);
    assert!(field(&json).is_object(), "{}", field(&json));
}

/// A stamp that names a day or a time that does not exist is not a stamp.
#[test]
fn stamps_that_name_impossible_moments_are_ordered_against_nothing() {
    for bad in [
        "2026-02-30T09:10:00Z",
        "2026-13-01T09:10:00Z",
        "2026-09-26T25:00:00Z",
        "2026-09-26T09:60:00Z",
        "2026-09-26T09:10:60Z",
        "2026-09-26T09:10:00.1234567890Z",
        "2026-09-26T09:10:00+24:00",
        "2026-09-26T09:10:00+05:60",
        "2026-09-26t09:10:00z",
        "2026-09-26 09:10:00Z",
        "2026-09-26T09:10:00",
        "2026-09-26T09:10:00.Z",
        "2026-09-26T09:10:00+0530",
    ] {
        let ledger = store();
        intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
        intended(&ledger, "ei-work", bad, AFTER, None);
        accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);
        assert_silent(&ledger, &format!("{bad:?} is not a moment"));

        // and as the acceptance's own time
        let ledger = store();
        intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
        intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);
        accept(&ledger, bad, &[C1]);
        assert_silent(
            &ledger,
            &format!("{bad:?} as an acceptance time is not a moment"),
        );
    }
}

/// A leap day that exists is a stamp.
#[test]
fn a_real_leap_day_is_a_moment() {
    let ledger = store();
    intended(&ledger, "ei-open", "2028-02-29T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2028-02-29T09:10:00Z", AFTER, None);
    accept(&ledger, "2028-03-01T00:00:00Z", &[C1]);

    let json = served(&ledger);
    assert!(field(&json).is_object(), "{}", field(&json));
    assert_eq!(field(&json)["accepted_on"], "2028-03-01");
}

/// A positive offset early in its day is still the day before, in UTC.
#[test]
fn the_utc_date_is_the_earlier_one_for_an_early_morning_offset() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T10:00:00Z", AFTER, None);
    // 02:00 at +05:30 on the 27th is 20:30Z on the 26th.
    accept(&ledger, "2026-09-27T02:00:00+05:30", &[C1]);

    let json = served(&ledger);
    assert!(field(&json).is_object(), "{}", field(&json));
    assert_eq!(field(&json)["accepted_on"], "2026-09-26");
}

/// The route-only read (which also reads git) serves the same fact: it is read
/// from the ledger, so it does not depend on the folder still being there.
#[test]
fn the_live_read_serves_the_same_fact_as_the_plain_one() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);

    let plain = served(&ledger);
    let live = serde_json::to_value(
        change::read_change_live(&ledger, CHANGE)
            .expect("read")
            .expect("the change exists"),
    )
    .expect("serializes");
    assert!(field(&plain).is_object());
    assert_eq!(field(&plain), field(&live));
}

/// The fact says which list it is about. A list proposed after the work and
/// not yet accepted is not accepted, so nothing is said about it.
#[test]
fn a_newer_list_nobody_has_accepted_says_nothing() {
    let ledger = store();
    intended(&ledger, "ei-open", "2026-09-26T09:00:00Z", BEFORE, None);
    intended(&ledger, "ei-work", "2026-09-26T09:10:00Z", AFTER, None);
    accept(&ledger, "2026-09-26T09:20:00Z", &[C1]);
    // v2 is written down and never accepted: it is the standing version.
    let scope = AcceptedScope {
        criteria: vec![C1.to_string(), C2.to_string()],
        allowed_scope: vec!["src/**".to_string()],
        ..AcceptedScope::default()
    };
    change::append_scope(&ledger, CHANGE, "2026-09-26T09:30:00Z", &scope).expect("append scope");

    let json = served(&ledger);
    assert_eq!(json["accepted_scope"]["version"], 2);
    assert!(json["accepted_scope"]["acceptance"].is_null());
    assert!(
        field(&json).is_null(),
        "the standing list has no acceptance; got {}",
        field(&json)
    );
}
