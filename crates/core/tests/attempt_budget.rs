//! T-4-4-ATTEMPT-BUDGET (R-2026-09-28-43; Q14 C8) — how many repair attempts a
//! person allowed one Change, counted from the recorded runs and nothing else.
//!
//! The store here is a real ledger with rows written the way the wrapper
//! writes them (one `running` row, then how the run ended), so the count is
//! asked of exactly the record `thruline check --for-change` leaves.

use rusqlite::params;
use thruline_core::attempt_budget::{self, Refusal};
use thruline_core::ledger::Ledger;

const CHANGE: &str = "chg_t44";

fn store(dir: &std::path::Path) -> Ledger {
    let ledger = Ledger::open(dir.join("ledger.db")).expect("open");
    ledger
        .conn()
        .execute_batch(
            "INSERT INTO projects(id,root_path,source,display_name,created_at)
               VALUES('cccccccc-0000-4000-8000-0000000000t4','/nowhere','claude_code','P',
                      '2026-01-01T00:00:00Z');
             INSERT INTO changes(id,project_id,opened_at,title,purpose,requester,request_source)
               VALUES('chg_t44','cccccccc-0000-4000-8000-0000000000t4','2026-01-01T00:00:00Z',
                      'Retry','Kept','sam','person');",
        )
        .expect("seed");
    ledger
}

/// One run as the wrapper records it: `running`, then `state`.
fn run(ledger: &Ledger, key: &str, version: &str, state: &str) {
    let n: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM change_run_executions", [], |r| {
            r.get(0)
        })
        .expect("count");
    let at = format!("2026-01-02T00:{:02}:00Z", n % 60);
    if state == "blocked" {
        ledger
            .conn()
            .execute(
                "INSERT INTO change_run_executions(id,run_key,change_id,name,state,reason,
                     examined_inputs_id,recorded_at)
                 VALUES(?1,?2,?3,'check','blocked','prerequisite_absent',?4,?5)",
                params![format!("rx_{n}"), key, CHANGE, version, at],
            )
            .expect("blocked row");
        return;
    }
    ledger
        .conn()
        .execute(
            "INSERT INTO change_run_executions(id,run_key,change_id,name,state,pid,process_group,
                 started_at,examined_inputs_id,recorded_at)
             VALUES(?1,?2,?3,'check','running',1,1,?4,?5,?4)",
            params![format!("rx_{n}"), key, CHANGE, at, version],
        )
        .expect("running row");
    if state != "running" {
        ledger
            .conn()
            .execute(
                "INSERT INTO change_run_executions(id,run_key,change_id,name,state,pid,
                     process_group,started_at,examined_inputs_id,cleanup_verified,survivors,
                     recorded_at)
                 VALUES(?1,?2,?3,'check',?4,1,1,?5,?6,1,0,?5)",
                params![format!("rx_{n}b"), key, CHANGE, state, at, version],
            )
            .expect("ending row");
    }
}

fn set(ledger: &Ledger, n: u32) {
    attempt_budget::set(ledger, CHANGE, n, "sam", "2026-01-02T01:00:00Z")
        .expect("store")
        .expect("a budget is recorded");
}

fn used(ledger: &Ledger) -> u32 {
    attempt_budget::status(ledger, CHANGE)
        .expect("read")
        .expect("a budget stands")
        .used
}

fn budget_rows(ledger: &Ledger) -> i64 {
    ledger
        .conn()
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", attempt_budget::TABLE),
            [],
            |r| r.get(0),
        )
        .expect("count")
}

#[test]
fn three_new_versions_are_three_attempts_and_the_fourth_is_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = store(dir.path());
    run(&ledger, "run-0", "exi_v0", "completed");
    set(&ledger, 3);
    for (i, v) in ["exi_v1", "exi_v2", "exi_v3"].iter().enumerate() {
        assert!(
            attempt_budget::admits(&ledger, CHANGE, v)
                .expect("read")
                .is_none(),
            "attempt {} was within the budget",
            i + 1
        );
        run(&ledger, &format!("run-{}", i + 1), v, "completed");
    }
    let status = attempt_budget::status(&ledger, CHANGE)
        .expect("read")
        .expect("stands");
    assert_eq!(status.used, 3);
    assert_eq!(status.remaining, 0);
    assert!(status.spent);
    assert_eq!(
        status
            .attempts
            .iter()
            .map(|a| a.examined_inputs_id.clone().unwrap_or_default())
            .collect::<Vec<_>>(),
        vec!["exi_v1", "exi_v2", "exi_v3"]
    );
    let stop = attempt_budget::admits(&ledger, CHANGE, "exi_v4")
        .expect("read")
        .expect("a fourth new version is refused");
    assert!(stop.contains("3"), "{stop}");
    assert!(
        stop.contains("thruline change attempts chg_t44 --budget"),
        "the refusal says what a person can do: {stop}"
    );
    // A version already examined is not a new attempt, before or after the
    // budget was set.
    for v in ["exi_v0", "exi_v2"] {
        assert!(
            attempt_budget::admits(&ledger, CHANGE, v)
                .expect("read")
                .is_none(),
            "{v} is not a new attempt"
        );
    }
}

#[test]
fn a_rerun_of_a_version_already_counted_is_not_a_new_attempt() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = store(dir.path());
    set(&ledger, 2);
    run(&ledger, "run-1", "exi_v1", "completed");
    run(&ledger, "run-2", "exi_v1", "completed");
    run(&ledger, "run-3", "exi_v1", "cancelled");
    let status = attempt_budget::status(&ledger, CHANGE)
        .expect("read")
        .expect("stands");
    assert_eq!(status.used, 1);
    assert_eq!(status.attempts[0].runs, vec!["run-1", "run-2", "run-3"]);
}

#[test]
fn a_version_examined_before_the_budget_was_set_is_not_counted() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = store(dir.path());
    run(&ledger, "run-a", "exi_old", "completed");
    run(&ledger, "run-b", "exi_older", "completed");
    set(&ledger, 1);
    run(&ledger, "run-c", "exi_old", "completed");
    assert_eq!(used(&ledger), 0);
    let status = attempt_budget::status(&ledger, CHANGE)
        .expect("read")
        .expect("stands");
    assert_eq!(
        status.budget.set_on_version.as_deref(),
        Some("exi_older"),
        "the row names the version it was set on"
    );
}

#[test]
fn a_blocked_check_ran_nothing_and_is_not_an_attempt() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = store(dir.path());
    set(&ledger, 1);
    run(&ledger, "run-1", "exi_v1", "blocked");
    assert_eq!(used(&ledger), 0);
    run(&ledger, "run-2", "exi_v2", "completed");
    assert_eq!(used(&ledger), 1);
}

#[test]
fn the_standing_budget_is_the_last_row_appended_whatever_the_clock_says() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = store(dir.path());
    attempt_budget::set(&ledger, CHANGE, 5, "sam", "2026-12-31T00:00:00Z")
        .expect("store")
        .expect("recorded");
    run(&ledger, "run-1", "exi_v1", "completed");
    // Stamped EARLIER than the first, appended later: it is still the one
    // that stands, and it counts from the run recorded before it.
    attempt_budget::set(&ledger, CHANGE, 1, "kim", "2026-01-01T00:00:00Z")
        .expect("store")
        .expect("recorded");
    let status = attempt_budget::status(&ledger, CHANGE)
        .expect("read")
        .expect("stands");
    assert_eq!(status.budget.budget, 1);
    assert_eq!(status.budget.actor, "kim");
    assert_eq!(status.earlier.len(), 1);
    assert_eq!(status.used, 0);
}

#[test]
fn no_budget_limits_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = store(dir.path());
    for i in 0..5 {
        run(
            &ledger,
            &format!("run-{i}"),
            &format!("exi_{i}"),
            "completed",
        );
    }
    assert!(attempt_budget::status(&ledger, CHANGE)
        .expect("read")
        .is_none());
    assert!(attempt_budget::admits(&ledger, CHANGE, "exi_new")
        .expect("read")
        .is_none());
}

#[test]
fn a_refused_budget_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = store(dir.path());
    let at = "2026-01-02T00:00:00Z";
    for (change, n, actor, want) in [
        ("chg_nothing", 2, "sam", Refusal::NoSuchChange),
        (CHANGE, 2, "  ", Refusal::NoActor),
        (CHANGE, 0, "sam", Refusal::OutOfRange),
        (
            CHANGE,
            attempt_budget::MAX_BUDGET + 1,
            "sam",
            Refusal::OutOfRange,
        ),
    ] {
        let got = attempt_budget::set(&ledger, change, n, actor, at).expect("store");
        assert_eq!(got.err(), Some(want));
    }
    assert_eq!(budget_rows(&ledger), 0);
}

#[test]
fn the_budget_table_is_append_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = store(dir.path());
    set(&ledger, 2);
    for sql in [
        format!("UPDATE {} SET budget=9", attempt_budget::TABLE),
        format!("DELETE FROM {}", attempt_budget::TABLE),
    ] {
        assert!(ledger.conn().execute(&sql, []).is_err(), "{sql}");
    }
    assert_eq!(budget_rows(&ledger), 1);
}

/// The status is one list of sentences from one function, ending with the
/// stop sentence, so a later sentence can be inserted before it. "Unresolved"
/// is said only when the readiness answer the caller was served is not Ready.
#[test]
fn the_status_is_one_list_of_sentences_and_says_unresolved_only_when_not_ready() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = store(dir.path());
    set(&ledger, 1);
    run(&ledger, "run-1", "exi_v1", "completed");
    let status = attempt_budget::status(&ledger, CHANGE).expect("read");
    let not_ready = attempt_budget::sentences(CHANGE, status.as_ref(), Some(false));
    let ready = attempt_budget::sentences(CHANGE, status.as_ref(), Some(true));
    let unknown = attempt_budget::sentences(CHANGE, status.as_ref(), None);
    assert!(not_ready
        .last()
        .expect("a stop sentence")
        .contains("unresolved"));
    for said in [&ready, &unknown] {
        assert!(
            !said.iter().any(|s| s.contains("unresolved")),
            "unresolved said without a not-Ready answer: {said:?}"
        );
    }
    for said in [&not_ready, &ready, &unknown] {
        assert!(said.iter().any(|s| s.starts_with("Attempt 1")), "{said:?}");
        for word in ["fixed", "resolved.", "lie", "lied"] {
            assert!(
                !said.iter().any(|s| s.contains(word)),
                "{word:?} in {said:?}"
            );
        }
    }
}

/// The count and the stop take no git input: the module that decides them
/// reads the ledger and nothing else. Pinned on the source, so a later git
/// read (FOUNDER-ASKS #21) has to arrive as a separate sentence, never as an
/// input to the count.
#[test]
fn the_count_and_the_stop_take_no_git_input() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/attempt_budget.rs"),
    )
    .expect("the module is readable");
    let code: String = source
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "git",
        "workspace",
        "observe",
        "snapshot",
        "std::process",
        "Command",
        "std::fs",
        "Path",
    ] {
        assert!(
            !code.contains(forbidden),
            "attempt_budget.rs reaches {forbidden:?}; the count and the stop read the ledger only"
        );
    }
}

// ── T-4-4-ATTEMPT-BUDGET-R2: the decision and the row are one act ──────────

fn start_row(ledger: &Ledger, key: &str, version: &str) {
    thruline_core::run_execution::start(
        ledger,
        &thruline_core::run_execution::Start {
            run_key: key.to_string(),
            change_id: Some(CHANGE.to_string()),
            name: "check".to_string(),
            pid: Some(1),
            process_group: Some(1),
            started_at: "2026-01-02T02:00:00Z".to_string(),
            examined_inputs_id: Some(version.to_string()),
        },
    )
    .expect("running row");
}

fn refused(admission: attempt_budget::Admission<'_>) -> String {
    match admission {
        attempt_budget::Admission::Refused(why) => why,
        attempt_budget::Admission::Admitted(_) => panic!("admitted"),
    }
}

/// While one caller holds its admission, a second caller — another
/// connection, as another process or the other door would be — is refused
/// with nothing written; once the first caller's row is committed, the second
/// sees that attempt as used.
#[test]
fn a_second_caller_waits_out_the_held_admission_and_then_sees_the_attempt_used() {
    let dir = tempfile::tempdir().expect("tempdir");
    let a = store(dir.path());
    run(&a, "before", "exi_before", "completed");
    set(&a, 1);
    let b = Ledger::open(dir.path().join("ledger.db")).expect("second connection");

    let hold = match attempt_budget::admit(&a, CHANGE, "exi_one").expect("admit") {
        attempt_budget::Admission::Admitted(hold) => hold,
        attempt_budget::Admission::Refused(why) => panic!("{why}"),
    };
    let why = refused(attempt_budget::admit(&b, CHANGE, "exi_two").expect("admit"));
    assert!(
        why.starts_with("no check was started") && why.contains("busy"),
        "{why}"
    );
    start_row(&a, "run-one", "exi_one");
    hold.commit().expect("commit");

    assert_eq!(used(&b), 1);
    let why = refused(attempt_budget::admit(&b, CHANGE, "exi_two").expect("admit"));
    assert!(why.contains("sam allowed 1 repair attempt"), "{why}");
    // A re-run of the version already examined is still admitted.
    match attempt_budget::admit(&b, CHANGE, "exi_one").expect("admit") {
        attempt_budget::Admission::Admitted(hold) => drop(hold),
        attempt_budget::Admission::Refused(why) => panic!("{why}"),
    };
}

/// A holder that goes away without writing its row (a crash between "yes"
/// and the row; a spawn that failed) releases the lock and leaves nothing: the
/// attempt it was allowed is still there for the next caller.
#[test]
fn a_holder_that_never_writes_its_row_leaves_the_attempt_unspent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let a = store(dir.path());
    run(&a, "before", "exi_before", "completed");
    set(&a, 1);
    let b = Ledger::open(dir.path().join("ledger.db")).expect("second connection");
    match attempt_budget::admit(&a, CHANGE, "exi_one").expect("admit") {
        attempt_budget::Admission::Admitted(hold) => {
            start_row(&a, "run-lost", "exi_one");
            drop(hold);
        }
        attempt_budget::Admission::Refused(why) => panic!("{why}"),
    }
    assert!(thruline_core::run_execution::latest(&b, "run-lost")
        .expect("read")
        .is_none());
    assert_eq!(used(&b), 0);
    match attempt_budget::admit(&b, CHANGE, "exi_two").expect("admit") {
        attempt_budget::Admission::Admitted(hold) => drop(hold),
        attempt_budget::Admission::Refused(why) => panic!("{why}"),
    };
}

/// A refusal on a spent budget releases the lock at once.
#[test]
fn a_refusal_releases_the_lock() {
    let dir = tempfile::tempdir().expect("tempdir");
    let a = store(dir.path());
    run(&a, "before", "exi_before", "completed");
    set(&a, 1);
    run(&a, "one", "exi_one", "completed");
    let b = Ledger::open(dir.path().join("ledger.db")).expect("second connection");
    let why = refused(attempt_budget::admit(&a, CHANGE, "exi_two").expect("admit"));
    assert!(why.contains("sam allowed 1 repair attempt"), "{why}");
    assert!(
        a.conn().is_autocommit(),
        "the refusal left a transaction open"
    );
    set(&b, 2);
    assert_eq!(budget_rows(&b), 2);
}

/// T-4-4-ATTEMPT-BUDGET-R3 (P-2026-09-29-3) — the own-files limit, stated
/// where a person reads the count. Pinned as literal text, so a change to the
/// words is a change to this test.
const OWN_FILES_LIMIT: &str = "A change made only to ThruLine's own files in the project \
(under .thruline/) currently counts as a new attempt.";

/// Every list of sentences about a set budget — with attempts left, spent,
/// and spent with each readiness answer — says the limit exactly once, before
/// the last sentence (which stays the one that says whether another attempt
/// may be made); the refusal before the spawn says it too, from both
/// functions a door calls.
#[test]
fn the_own_files_limit_is_stated_on_the_count_and_on_the_refusal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = store(dir.path());
    set(&ledger, 1);
    let left = attempt_budget::status(&ledger, CHANGE).expect("read");
    let with_left = attempt_budget::sentences(CHANGE, left.as_ref(), None);
    run(&ledger, "run-1", "exi_v1", "completed");
    let spent = attempt_budget::status(&ledger, CHANGE).expect("read");
    for said in [
        with_left,
        attempt_budget::sentences(CHANGE, spent.as_ref(), Some(false)),
        attempt_budget::sentences(CHANGE, spent.as_ref(), Some(true)),
        attempt_budget::sentences(CHANGE, spent.as_ref(), None),
    ] {
        assert_eq!(
            said.iter()
                .filter(|s| s.as_str() == OWN_FILES_LIMIT)
                .count(),
            1,
            "the limit is one sentence of its own: {said:?}"
        );
        assert_ne!(said.last().map(String::as_str), Some(OWN_FILES_LIMIT));
    }
    let stop = attempt_budget::admits(&ledger, CHANGE, "exi_v2")
        .expect("read")
        .expect("a second new version is refused");
    assert!(stop.ends_with(OWN_FILES_LIMIT), "{stop}");
    let held = refused(attempt_budget::admit(&ledger, CHANGE, "exi_v2").expect("read"));
    assert_eq!(held, stop, "both functions a door calls say the same");
}

// ── T-4-4-ATTEMPT-BUDGET-R4: a run in flight when a budget is set counts ────
//
// The round-3 adversary's three cases, driven through the record rather than
// a clock: the `running` row is written, the budget is set, and only then is
// the run's ending row written — the order a real check that is still running
// when a person sets the budget leaves in the store.

/// The ending row of a run whose `running` row [`run`] already wrote.
fn finish(ledger: &Ledger, key: &str, version: &str, state: &str) {
    ledger
        .conn()
        .execute(
            "INSERT INTO change_run_executions(id,run_key,change_id,name,state,pid,
                 process_group,started_at,examined_inputs_id,cleanup_verified,survivors,
                 recorded_at)
             VALUES(?1,?2,?3,'check',?4,1,1,'2026-01-02T03:00:00Z',?5,1,0,
                    '2026-01-02T03:00:00Z')",
            params![format!("rx_{key}_end"), key, CHANGE, state, version],
        )
        .expect("ending row");
}

fn standing_anchor(ledger: &Ledger) -> Option<String> {
    attempt_budget::status(ledger, CHANGE)
        .expect("read")
        .expect("stands")
        .budget
        .after_execution
}

/// One case: a budget of `first`, one earlier attempt `done`, a check started
/// on a new version, the budget changed to `second` while it runs, then the
/// check ends. The in-flight check is attempt 1 of the new budget.
fn changed_while_running(first: u32, second: u32) {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = store(dir.path());
    run(&ledger, "run-0", "exi_v0", "completed");
    set(&ledger, first);
    run(&ledger, "run-1", "exi_v1", "completed");
    run(&ledger, "run-2", "exi_v2", "running");
    set(&ledger, second);
    // The anchor is the last run that had ended, never the one in flight.
    assert_eq!(
        standing_anchor(&ledger).as_deref(),
        Some("rx_2b"),
        "{first}→{second}: the new budget is anchored on the last ended run"
    );
    assert_eq!(
        used(&ledger),
        1,
        "{first}→{second}: the check in flight is an attempt of the new budget"
    );
    finish(&ledger, "run-2", "exi_v2", "completed");
    let status = attempt_budget::status(&ledger, CHANGE)
        .expect("read")
        .expect("stands");
    assert_eq!(status.used, 1, "{first}→{second}");
    assert_eq!(status.remaining, second - 1, "{first}→{second}");
    assert_eq!(
        status.attempts[0].examined_inputs_id.as_deref(),
        Some("exi_v2")
    );
    assert_eq!(status.attempts[0].states, vec!["completed".to_string()]);
}

#[test]
fn a_budget_raised_from_2_to_10_while_a_check_runs_counts_that_check() {
    changed_while_running(2, 10);
}

#[test]
fn a_budget_raised_from_3_to_20_while_a_check_runs_counts_that_check() {
    changed_while_running(3, 20);
}

/// The adversary's severe form: "stop after this one". A budget of 20, a
/// check starts on a new version, the budget is lowered to 1 while it runs:
/// that check is the one attempt, and the next new version is refused.
#[test]
fn a_budget_lowered_from_20_to_1_while_a_check_runs_stops_the_next_new_version() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = store(dir.path());
    set(&ledger, 20);
    run(&ledger, "run-1", "exi_v1", "running");
    set(&ledger, 1);
    assert_eq!(standing_anchor(&ledger), None, "no run had ended");
    finish(&ledger, "run-1", "exi_v1", "completed");
    let status = attempt_budget::status(&ledger, CHANGE)
        .expect("read")
        .expect("stands");
    assert_eq!(status.used, 1);
    assert!(status.spent, "the one attempt allowed is the one that ran");
    let stop = attempt_budget::admits(&ledger, CHANGE, "exi_v2")
        .expect("read")
        .expect("the next new version is refused");
    assert!(stop.contains("sam allowed 1 repair attempt"), "{stop}");
    let held = refused(attempt_budget::admit(&ledger, CHANGE, "exi_v2").expect("admit"));
    assert_eq!(held, stop);
    // A re-run of the version that attempt examined is still admitted.
    assert!(attempt_budget::admits(&ledger, CHANGE, "exi_v1")
        .expect("read")
        .is_none());
}

/// A run started before another one ended is still in flight when the budget
/// is set, even though the last row appended is the other run's ending: it
/// counts. A run that ended before the budget does not.
#[test]
fn a_run_in_flight_behind_a_later_ending_still_counts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = store(dir.path());
    run(&ledger, "run-a", "exi_a", "running");
    run(&ledger, "run-b", "exi_b", "completed");
    set(&ledger, 1);
    finish(&ledger, "run-a", "exi_a", "cancelled");
    let status = attempt_budget::status(&ledger, CHANGE)
        .expect("read")
        .expect("stands");
    assert_eq!(
        status
            .attempts
            .iter()
            .map(|a| a.examined_inputs_id.clone().unwrap_or_default())
            .collect::<Vec<_>>(),
        vec!["exi_a"],
        "the run in flight counts; the one that had ended does not"
    );
    assert!(status.spent);
    assert!(attempt_budget::admits(&ledger, CHANGE, "exi_b")
        .expect("read")
        .is_none());
}
