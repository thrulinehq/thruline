//! C30 R-LEDGER — what a folder's capture holds say, with the built-in defaults
//! laid in (C34 ruling Q7), and the one function the product's doors call to
//! change them.
//!
//! The chain itself, and the refusals that change nothing, are asked in
//! `r_ledger_migration.rs`. This asks what that file does not: that `env` and
//! `secrets` are exclusions without being rows and are served as "on by default
//! (not recorded)"; that the first change to one appends, and appends the two
//! rows the table needs to say it; that once a key has a row the record decides
//! it; that one folder's pause and exclusions are not another's; and that a
//! pause or an exclusion is recorded and nothing more — the view says, in a
//! field and in words, that nothing honours either yet.
//!
//! A scratch ledger in memory. No real store is opened.

use thruline_core::capture_hold::{
    self, Act, Hold, Recorded, Refusal, DEFAULT_ACTOR, HONOURED_NOTE,
};
use thruline_core::ledger::Ledger;

const APP: &str = "/work/app";
const OTHER: &str = "/work/other";
const AT: &str = "2026-10-08T09:00:00Z";

fn hold<'a>(folder: &'a str, actor: &'a str, at: &'a str) -> Hold<'a> {
    Hold {
        project_id: "hq-project",
        project_folder: folder,
        actor,
        recorded_at: at,
    }
}

fn appended(recorded: Recorded) -> capture_hold::HoldRow {
    match recorded {
        Recorded::Appended(row) => *row,
        Recorded::Refused(why) => panic!("expected an appended row, got {why:?}"),
    }
}

fn person(ledger: &Ledger, act: Act, folder: &str, who: &str, pattern: Option<&str>) -> Recorded {
    capture_hold::record_for_person(ledger, act, &hold(folder, who, AT), pattern).expect("record")
}

fn rows(ledger: &Ledger) -> i64 {
    ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM capture_holds", [], |r| r.get(0))
        .expect("count")
}

fn entry<'a>(view: &'a capture_hold::HoldsView, key: &str) -> &'a capture_hold::ExclusionView {
    view.exclusions
        .iter()
        .find(|e| e.key == key)
        .unwrap_or_else(|| panic!("no {key:?} in {view:?}"))
}

#[test]
fn the_defaults_are_code_not_rows_and_are_served_as_on_by_default_not_recorded() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let view = capture_hold::holds_view(&ledger, APP).expect("view");
    assert_eq!(rows(&ledger), 0, "reading the defaults wrote nothing");
    assert!(!view.paused);
    assert_eq!(view.paused_at, None);
    assert_eq!(view.paused_by, None);

    let keys: Vec<&str> = view.exclusions.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(
        keys,
        ["env", "secrets", "generated"],
        "the order Settings lists them"
    );

    for key in ["env", "secrets"] {
        let e = entry(&view, key);
        assert!(e.on && e.built_in, "{key}: {e:?}");
        assert_eq!(e.source, "default");
        assert_eq!(e.basis, Some("on by default (not recorded)"), "{key}");
        assert_eq!(
            (&e.since, &e.by),
            (&None, &None),
            "a default has no row behind it"
        );
    }
    let generated = entry(&view, "generated");
    assert!(
        !generated.on,
        "the canvas draws `generated` unticked: {generated:?}"
    );
    assert_eq!(generated.source, "default");
    assert_eq!(generated.basis, Some("off by default (not recorded)"));
}

#[test]
fn a_pause_and_an_exclusion_are_honoured_in_their_separate_boundaries() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let view = capture_hold::holds_view(&ledger, APP).expect("view");
    assert!(
        view.honoured,
        "the six capture callers act on a pause (C34 Q5, Q6)"
    );
    assert!(
        view.exclusions_honoured,
        "an exclusion is applied at serve and export time (C34 Q4)"
    );
    assert_eq!(view.note, HONOURED_NOTE);
    assert!(
        HONOURED_NOTE.contains("While paused, ThruLine reads nothing from this folder"),
        "{HONOURED_NOTE}"
    );
    assert!(
        HONOURED_NOTE.contains(
            "A path exclusion leaves matching entries out when ThruLine shows or exports a view"
        ) && HONOURED_NOTE
            .contains("sealed copy of each transcript still holds what Claude Code wrote"),
        "{HONOURED_NOTE}"
    );
    assert!(
        !HONOURED_NOTE.contains("Never record"),
        "the words `Never record` never ship over serve-time behaviour (C34 Q4)"
    );

    // And it says so the same with a pause in force.
    appended(
        capture_hold::record_for_person(&ledger, Act::Paused, &hold(APP, "sam", AT), None)
            .expect("pause"),
    );
    let paused = capture_hold::holds_view(&ledger, APP).expect("view");
    assert!(paused.paused);
    assert!(paused.honoured && paused.exclusions_honoured, "{paused:?}");
}

#[test]
fn excluding_a_default_that_is_already_on_changes_nothing() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    for key in ["env", "secrets"] {
        assert_eq!(
            person(&ledger, Act::Excluded, APP, "sam", Some(key)),
            Recorded::Refused(Refusal::AlreadyExcluded),
            "{key} is on already"
        );
    }
    assert_eq!(rows(&ledger), 0, "a refusal appends nothing");
}

#[test]
fn turning_a_default_off_is_the_first_change_and_writes_two_rows_naming_the_person_on_the_second_only(
) {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let off = appended(person(&ledger, Act::Included, APP, "sam", Some("env")));
    assert_eq!((off.act, off.actor.as_str()), (Act::Included, "sam"));
    assert_eq!(
        rows(&ledger),
        2,
        "the default written down, then the person's act"
    );
    assert_eq!(capture_hold::verify_chain(&ledger).expect("chain"), 2);

    let history = capture_hold::history(&ledger, APP).expect("history");
    let first = &history[0];
    assert_eq!(first.act, Act::Excluded);
    assert_eq!(first.pattern.as_deref(), Some("env"));
    assert_eq!(
        first.actor, DEFAULT_ACTOR,
        "the default is ThruLine's: no person is said to have excluded it"
    );
    assert_eq!(first.recorded_at, off.recorded_at);
    assert_eq!(
        off.ends.as_deref(),
        Some(first.id.as_str()),
        "the inclusion ends the written-down default"
    );

    let view = capture_hold::holds_view(&ledger, APP).expect("view");
    let env = entry(&view, "env");
    assert!(!env.on, "{env:?}");
    assert_eq!(env.source, "recorded");
    assert_eq!(env.basis, None, "a recorded state is not 'by default'");
    assert_eq!(env.by.as_deref(), Some("sam"));
    assert_eq!(env.since.as_deref(), Some(AT));
    // The other default is untouched.
    let secrets = entry(&view, "secrets");
    assert!(secrets.on);
    assert_eq!(secrets.source, "default");
}

#[test]
fn once_a_default_has_a_row_the_record_decides_it() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    appended(person(&ledger, Act::Included, APP, "sam", Some("env")));
    assert_eq!(rows(&ledger), 2);

    // Off, in the record: turning it off again changes nothing.
    assert_eq!(
        person(&ledger, Act::Included, APP, "kim", Some("env")),
        Recorded::Refused(Refusal::NotExcluded)
    );
    assert_eq!(rows(&ledger), 2);

    // Turning it back on is an ordinary row; the default is not consulted.
    let on = appended(person(&ledger, Act::Excluded, APP, "kim", Some("env")));
    assert_eq!((on.actor.as_str(), on.ends), ("kim", None));
    assert_eq!(rows(&ledger), 3);
    let view = capture_hold::holds_view(&ledger, APP).expect("view");
    let env = entry(&view, "env");
    assert!(env.on);
    assert_eq!((env.source, env.by.as_deref()), ("recorded", Some("kim")));

    // And turning it off now is ordinary too: one row, ending kim's.
    let off = appended(person(&ledger, Act::Included, APP, "kim", Some("env")));
    assert_eq!(off.ends.as_deref(), Some(on.id.as_str()));
    assert_eq!(rows(&ledger), 4, "no second written-down default");
    assert_eq!(capture_hold::verify_chain(&ledger).expect("chain"), 4);
}

#[test]
fn a_default_turned_off_in_one_folder_is_still_a_default_in_another() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    appended(person(&ledger, Act::Included, APP, "sam", Some("secrets")));
    let elsewhere = capture_hold::holds_view(&ledger, OTHER).expect("view");
    let secrets = entry(&elsewhere, "secrets");
    assert!(secrets.on);
    assert_eq!(secrets.source, "default");
    assert_eq!(
        person(&ledger, Act::Excluded, OTHER, "sam", Some("secrets")),
        Recorded::Refused(Refusal::AlreadyExcluded),
        "the other folder's default still stands"
    );
}

#[test]
fn generated_is_off_by_default_and_excluding_it_is_one_ordinary_row() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    assert_eq!(
        person(&ledger, Act::Included, APP, "sam", Some("generated")),
        Recorded::Refused(Refusal::NotExcluded),
        "it is not on, so there is nothing to turn off"
    );
    assert_eq!(rows(&ledger), 0);

    let on = appended(person(
        &ledger,
        Act::Excluded,
        APP,
        "sam",
        Some("generated"),
    ));
    assert_eq!(
        rows(&ledger),
        1,
        "no written-down default for a key that is off by default"
    );
    let view = capture_hold::holds_view(&ledger, APP).expect("view");
    let generated = entry(&view, "generated");
    assert!(generated.on);
    assert_eq!(generated.source, "recorded");
    assert_eq!(generated.by.as_deref(), Some("sam"));

    assert_eq!(
        person(&ledger, Act::Excluded, APP, "sam", Some("generated")),
        Recorded::Refused(Refusal::AlreadyExcluded)
    );
    let off = appended(person(
        &ledger,
        Act::Included,
        APP,
        "sam",
        Some("generated"),
    ));
    assert_eq!(off.ends.as_deref(), Some(on.id.as_str()));
    assert!(
        !entry(
            &capture_hold::holds_view(&ledger, APP).expect("view"),
            "generated"
        )
        .on
    );
}

#[test]
fn a_typed_path_is_listed_while_it_is_in_force_and_gone_once_it_is_included_again() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let excluded = appended(person(
        &ledger,
        Act::Excluded,
        APP,
        "sam",
        Some("vendor/legacy"),
    ));
    appended(person(
        &ledger,
        Act::Excluded,
        APP,
        "kim",
        Some("docs/*.draft.md"),
    ));
    let view = capture_hold::holds_view(&ledger, APP).expect("view");
    let keys: Vec<&str> = view.exclusions.iter().map(|e| e.key.as_str()).collect();
    assert_eq!(
        keys,
        [
            "env",
            "secrets",
            "generated",
            "vendor/legacy",
            "docs/*.draft.md"
        ],
        "the three built-ins first, then typed paths oldest first"
    );
    let path = entry(&view, "vendor/legacy");
    assert!(path.on && !path.built_in);
    assert_eq!(
        (path.note.as_deref(), path.covers.as_str()),
        (None, "vendor/legacy")
    );
    assert_eq!(path.by.as_deref(), Some("sam"));

    assert_eq!(
        person(&ledger, Act::Excluded, APP, "sam", Some("vendor/legacy")),
        Recorded::Refused(Refusal::AlreadyExcluded)
    );
    let ended = appended(person(
        &ledger,
        Act::Included,
        APP,
        "sam",
        Some("vendor/legacy"),
    ));
    assert_eq!(ended.ends.as_deref(), Some(excluded.id.as_str()));
    let after = capture_hold::holds_view(&ledger, APP).expect("view");
    assert!(
        after.exclusions.iter().all(|e| e.key != "vendor/legacy"),
        "an included path is not listed: {after:?}"
    );
    assert_eq!(
        person(&ledger, Act::Included, APP, "sam", Some("vendor/legacy")),
        Recorded::Refused(Refusal::NotExcluded)
    );
}

#[test]
fn a_pause_belongs_to_one_folder_and_the_view_says_who_paused_and_when() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    assert_eq!(
        person(&ledger, Act::Resumed, APP, "sam", None),
        Recorded::Refused(Refusal::NotPaused)
    );
    appended(person(&ledger, Act::Paused, APP, "sam", None));
    assert_eq!(
        person(&ledger, Act::Paused, APP, "kim", None),
        Recorded::Refused(Refusal::AlreadyPaused)
    );
    let here = capture_hold::holds_view(&ledger, APP).expect("view");
    assert!(here.paused);
    assert_eq!(here.paused_at.as_deref(), Some(AT));
    assert_eq!(here.paused_by.as_deref(), Some("sam"));
    let there = capture_hold::holds_view(&ledger, OTHER).expect("view");
    assert!(!there.paused, "another folder is not paused");

    appended(person(&ledger, Act::Resumed, APP, "kim", None));
    let after = capture_hold::holds_view(&ledger, APP).expect("view");
    assert!(!after.paused);
    assert_eq!((after.paused_at, after.paused_by), (None, None));
    assert_eq!(capture_hold::verify_chain(&ledger).expect("chain"), 2);
}

#[test]
fn a_leading_space_cannot_hide_an_absolute_path_or_a_climb_from_the_check() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    for bad in [" /etc/passwd", "  ../outside", "\t..\\outside", " ?x"] {
        assert!(
            capture_hold::record_for_person(
                &ledger,
                Act::Excluded,
                &hold(APP, "sam", AT),
                Some(bad)
            )
            .is_err(),
            "{bad:?} must be refused, not trimmed into a path that would have been"
        );
        assert!(
            capture_hold::record(&ledger, Act::Excluded, &hold(APP, "sam", AT), Some(bad)).is_err(),
            "the chain's own writer checks what it stores: {bad:?}"
        );
    }
    assert_eq!(rows(&ledger), 0);
}

#[test]
fn pattern_problem_says_why_and_accepts_what_a_person_would_type() {
    for (bad, why) in [
        ("", "it is empty"),
        ("   ", "it is empty"),
        ("/etc", "it is an absolute path"),
        ("\\share", "it is an absolute path"),
        ("a/../b", "it climbs out of the project with `..`"),
        ("a\nb", "it holds a control character"),
        ("src/?.rs", "`*` is the only wildcard"),
        ("a[b]", "`*` is the only wildcard"),
    ] {
        assert_eq!(capture_hold::pattern_problem(bad), Some(why), "{bad:?}");
        assert!(capture_hold::check_pattern(bad).is_err(), "{bad:?}");
    }
    let long = "a".repeat(capture_hold::MAX_PATTERN_CHARS + 1);
    assert_eq!(
        capture_hold::pattern_problem(&long),
        Some("it is longer than 256 characters")
    );
    assert_eq!(
        capture_hold::pattern_problem(&"a".repeat(capture_hold::MAX_PATTERN_CHARS)),
        None
    );
    for good in [
        "env",
        ".env.local",
        "secrets/",
        "vendor/legacy",
        "src/gen/*.rs",
        "env/",
    ] {
        assert_eq!(capture_hold::pattern_problem(good), None, "{good:?}");
    }
}

#[test]
fn a_folder_called_env_is_written_env_slash_and_is_a_path_not_the_built_in() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    assert!(capture_hold::built_in("env").is_some());
    assert!(capture_hold::built_in("env/").is_none());
    // `env/` is an ordinary path: excluding it appends one row; the built-in
    // `env` default is untouched.
    appended(person(&ledger, Act::Excluded, APP, "sam", Some("env/")));
    assert_eq!(rows(&ledger), 1);
    let view = capture_hold::holds_view(&ledger, APP).expect("view");
    assert_eq!(entry(&view, "env").source, "default");
    assert!(entry(&view, "env/").on);
}

#[test]
fn the_chains_own_writer_still_knows_only_rows() {
    // `record` is the primitive the existing tests and `verify` stand on: it
    // never consults a default, so excluding `env` with it is an ordinary row.
    let ledger = Ledger::open_in_memory().expect("ledger");
    appended(
        capture_hold::record(&ledger, Act::Excluded, &hold(APP, "sam", AT), Some("env"))
            .expect("record"),
    );
    assert_eq!(rows(&ledger), 1);
    assert_eq!(
        capture_hold::record(&ledger, Act::Included, &hold(OTHER, "sam", AT), Some("env"))
            .expect("record"),
        Recorded::Refused(Refusal::NotExcluded),
        "no written-down default through the primitive"
    );
    assert_eq!(rows(&ledger), 1);
}
