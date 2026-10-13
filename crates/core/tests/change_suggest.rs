//! T-2-2 — the suggester proposes, and only ever proposes (`B3.10`–`B3.12`,
//! `B8.4`).
//!
//! Sam's failure mode here is **doesn't trust it**: a Change that looks
//! "backed" by a session about something else is worse than a Change with no
//! sessions at all, because it spends the trust that every other screen rests
//! on. So the tests below are about the difference between a guess and a fact:
//! the guess must be labelled as one, carry its reason, be reproducible, and
//! never turn into a fact by itself.
//!
//! Every id is synthetic (`cccccccc-…`) and no fixture here holds private
//! content.

use rusqlite::params;
use thruline_core::change::{
    self, AcceptedScope, AssociationBasis, AssociationProvenance, ChangeRequest, NewChange,
    RequestSource,
};
use thruline_core::change_link::{self, Window};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
/// The project's working folder. Made on disk by [`store`], because a changed
/// path that is absolute only becomes scope under a root the capture
/// established (T-3-2b): a root that is not a folder on this machine abstains,
/// and every path under it with it.
const ROOT_NAME: &str = "synthetic-project";
const IN_WINDOW: &str = "cccccccc-0000-4000-8000-00000000s001";
const OUT_OF_WINDOW: &str = "cccccccc-0000-4000-8000-00000000s002";
const UNDATED: &str = "cccccccc-0000-4000-8000-00000000s003";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c001";

/// A store with one project, three sessions and the events that give two of
/// them a capture window and some changed files.
fn store() -> (tempfile::TempDir, Ledger, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open a fresh ledger");
    let root_path = dir.path().join(ROOT_NAME);
    std::fs::create_dir_all(&root_path).expect("the project folder");
    let root = root_path
        .canonicalize()
        .unwrap_or(root_path)
        .display()
        .to_string();
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, ?2, 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
            params![PROJECT, root],
        )
        .expect("insert project");
    ledger
        .conn()
        .execute(
            "INSERT INTO workspace_identities(project_id, repo_id, basis, resolved_root,
                 resolved_via, sessions_sampled, observed_at)
             VALUES(?1,'repo','git',?2,'recorded_cwd',1,'2026-01-01T00:00:00Z')",
            params![PROJECT, root],
        )
        .expect("insert workspace identity");
    for (session, started) in [
        (IN_WINDOW, Some("2026-01-02T09:30:00Z")),
        (OUT_OF_WINDOW, Some("2025-06-01T09:00:00Z")),
        (UNDATED, None),
    ] {
        ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id, project_id, file_path, started_at)
                 VALUES(?1, ?2, ?4, ?3)",
                params![session, PROJECT, started, format!("{root}/s.jsonl")],
            )
            .expect("insert session");
    }
    // The in-window session edits two files inside the allowed scope and one
    // outside it; the out-of-window session edits only outside.
    add_edit(
        &ledger,
        IN_WINDOW,
        1,
        "2026-01-02T09:31:00Z",
        "Edit",
        &format!("{root}/src/upload/retry.rs"),
    );
    add_edit(
        &ledger,
        IN_WINDOW,
        2,
        "2026-01-02T09:32:00Z",
        "Write",
        &format!("{root}/src/upload/queue.rs"),
    );
    add_edit(
        &ledger,
        IN_WINDOW,
        3,
        "2026-01-02T09:33:00Z",
        "Edit",
        &format!("{root}/docs/notes.md"),
    );
    add_edit(
        &ledger,
        OUT_OF_WINDOW,
        1,
        "2025-06-01T09:05:00Z",
        "Edit",
        &format!("{root}/docs/history.md"),
    );
    // A read is not a change. This tool is deliberately outside EDIT_TOOLS.
    add_edit(
        &ledger,
        IN_WINDOW,
        4,
        "2026-01-02T09:34:00Z",
        "Read",
        &format!("{root}/src/upload/secret.rs"),
    );
    (dir, ledger, root)
}

fn add_edit(ledger: &Ledger, session: &str, seq: i64, ts: &str, tool: &str, path: &str) {
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES(?1, ?2, ?3, ?4, ?5, 'tool_use', ?6, ?7, '/tmp/s.jsonl', ?4, 'x', '', '')",
            params![
                format!("cccccccc-ev-{session}-{seq}"),
                PROJECT,
                session,
                seq,
                ts,
                tool,
                format!("{{\"file_path\":\"{path}\"}}"),
            ],
        )
        .expect("insert event");
}

fn a_change(ledger: &Ledger) {
    change::insert_change(
        ledger,
        &NewChange {
            id: CHANGE.to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-01-02T09:00:00Z".to_string(),
            request: ChangeRequest {
                title: "Retry failed uploads".to_string(),
                purpose: "Uploads that fail once are lost.".to_string(),
                prompt_event_id: None,
                requester: "sam".to_string(),
                source: RequestSource::Person,
            },
        },
    )
    .expect("insert the change");
    change::append_scope(
        ledger,
        CHANGE,
        "2026-01-02T10:00:00Z",
        &AcceptedScope {
            criteria: vec!["A failed upload is tried again three times.".to_string()],
            exclusions: vec![],
            allowed_scope: vec!["src/upload/**".to_string()],
            constraints: vec![],
            check_profile: vec![],
        },
    )
    .expect("append the scope");
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

/// Same inputs, same proposals — compared as bytes, twice, on one store.
///
/// A suggester whose output wobbles is one nobody can review: the second
/// reading disagrees with the screenshot in the bug report and the
/// conversation is over.
#[test]
fn the_same_store_proposes_the_same_links_twice() {
    let (_dir, ledger, _root) = store();
    a_change(&ledger);
    let first = serde_json::to_string(&change_link::suggest(&ledger, CHANGE).expect("first"))
        .expect("encode");
    let second = serde_json::to_string(&change_link::suggest(&ledger, CHANGE).expect("second"))
        .expect("encode");
    assert_eq!(first, second, "two readings of one store must agree");

    // And the order is a stated property, not a coincidence of two readings
    // taken a microsecond apart. The first version of this test compared two
    // in-process readings only, and a deliberately injected clock-keyed
    // reordering slipped past it — the ordering assertion below is what caught
    // that break, so it is here rather than left to the snapshot.
    let report = change_link::suggest(&ledger, CHANGE).expect("third");
    let keys: Vec<(String, &str)> = report
        .suggestions
        .iter()
        .map(|s| (s.session_id.clone(), s.basis.as_str()))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted, "proposals are ordered by session then basis");
}

/// The proposal's own shape, pinned. Every field a person could be shown is
/// here, and a field that appears or vanishes shows up as a snapshot diff.
#[test]
fn the_proposals_and_what_they_rest_on_are_served_whole() {
    let (_dir, ledger, _root) = store();
    a_change(&ledger);
    let report = change_link::suggest(&ledger, CHANGE).expect("suggest");
    insta::assert_json_snapshot!("suggestion_report", report);
}

// ---------------------------------------------------------------------------
// The basis, recorded rather than implied
// ---------------------------------------------------------------------------

/// Every proposal says why it exists, and carries the thing that made it: an
/// overlap for timing, the intersecting paths for filenames. "We think so" with
/// no reason is what Sam has already stopped believing.
#[test]
fn every_proposal_carries_its_basis_and_its_evidence() {
    let (_dir, ledger, _root) = store();
    a_change(&ledger);
    let report = change_link::suggest(&ledger, CHANGE).expect("suggest");
    assert!(!report.suggestions.is_empty(), "the fixture must propose");
    for suggestion in &report.suggestions {
        match suggestion.basis {
            AssociationBasis::Timing => {
                assert!(
                    suggestion.overlap.is_some(),
                    "a timing proposal must name the instants it rests on"
                );
                assert!(suggestion.intersecting_paths.is_empty());
                assert_eq!(suggestion.sentence_slug, "suggestion_timing");
            }
            AssociationBasis::Filename => {
                assert!(
                    !suggestion.intersecting_paths.is_empty(),
                    "a filename proposal must name the files it rests on"
                );
                assert!(suggestion.overlap.is_none());
                assert_eq!(suggestion.sentence_slug, "suggestion_filename");
            }
            other => panic!("a machine proposal cannot have basis {other:?}"),
        }
    }
}

/// Only the files inside the scope are the reason, and a file the session
/// merely *read* is not a changed file.
#[test]
fn a_filename_proposal_names_only_the_files_inside_the_scope() {
    let (_dir, ledger, _root) = store();
    a_change(&ledger);
    let report = change_link::suggest(&ledger, CHANGE).expect("suggest");
    let filename = report
        .suggestions
        .iter()
        .find(|s| s.basis == AssociationBasis::Filename)
        .expect("a filename proposal");
    assert_eq!(
        filename.intersecting_paths,
        vec![
            "src/upload/queue.rs".to_string(),
            "src/upload/retry.rs".to_string()
        ],
        "the out-of-scope edit and the read-only file are not reasons"
    );
}

/// A session outside the window and outside the scope is not proposed at all.
#[test]
fn a_session_that_matches_neither_basis_is_not_proposed() {
    let (_dir, ledger, _root) = store();
    a_change(&ledger);
    let report = change_link::suggest(&ledger, CHANGE).expect("suggest");
    assert!(
        !report
            .suggestions
            .iter()
            .any(|s| s.session_id == OUT_OF_WINDOW),
        "a session from six months earlier that touched nothing in scope is not a suggestion"
    );
}

// ---------------------------------------------------------------------------
// Ties and empties, reported rather than resolved
// ---------------------------------------------------------------------------

/// A session that matches both ways produces both proposals and is named as a
/// tie. Nothing picks a winner (`B8.4`).
#[test]
fn a_session_matching_both_ways_is_named_a_tie_and_not_resolved() {
    let (_dir, ledger, _root) = store();
    a_change(&ledger);
    let report = change_link::suggest(&ledger, CHANGE).expect("suggest");
    assert_eq!(report.ties, vec![IN_WINDOW.to_string()]);
    let bases: Vec<&str> = report
        .suggestions
        .iter()
        .filter(|s| s.session_id == IN_WINDOW)
        .map(|s| s.basis.as_str())
        .collect();
    assert_eq!(
        bases,
        vec!["filename", "timing"],
        "both reasons survive; neither is dropped in favour of the other"
    );
}

/// Nothing to propose is an answer, with a sentence, not an empty screen.
#[test]
fn nothing_to_propose_is_said_rather_than_shown_as_blank() {
    let (_dir, ledger, _root) = store();
    change::insert_change(
        &ledger,
        &NewChange {
            id: "cccccccc-0000-4000-8000-00000000c002".to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2030-01-01T00:00:00Z".to_string(),
            request: ChangeRequest {
                title: "Something nobody has worked on".to_string(),
                purpose: "Nothing in the record touches this.".to_string(),
                prompt_event_id: None,
                requester: "sam".to_string(),
                source: RequestSource::Person,
            },
        },
    )
    .expect("insert");
    let report =
        change_link::suggest(&ledger, "cccccccc-0000-4000-8000-00000000c002").expect("suggest");
    assert!(report.suggestions.is_empty());
    assert_eq!(report.empty_slug, Some("suggestion_none"));
    assert!(
        thruline_core::voice::association_line("suggestion_none").is_some(),
        "the empty sentence must exist in the copy file"
    );
}

/// A session with no usable time is named, not silently skipped (Rule 12).
#[test]
fn a_session_with_no_usable_time_is_named_rather_than_dropped() {
    let (_dir, ledger, _root) = store();
    a_change(&ledger);
    let report = change_link::suggest(&ledger, CHANGE).expect("suggest");
    assert_eq!(report.undated_sessions, vec![UNDATED.to_string()]);
    assert_eq!(report.sessions_considered, 3);
}

/// A scope line the matcher does not understand matches nothing and is listed.
/// The alternative — treating it as "matches everything" — is how a scope check
/// silently stops being one.
#[test]
fn a_scope_line_we_do_not_understand_is_listed_and_matches_nothing() {
    let (_dir, ledger, _root) = store();
    change::insert_change(
        &ledger,
        &NewChange {
            id: "cccccccc-0000-4000-8000-00000000c003".to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-01-02T09:00:00Z".to_string(),
            request: ChangeRequest {
                title: "Odd scope".to_string(),
                purpose: "The scope line is not a path.".to_string(),
                prompt_event_id: None,
                requester: "sam".to_string(),
                source: RequestSource::Person,
            },
        },
    )
    .expect("insert");
    change::append_scope(
        &ledger,
        "cccccccc-0000-4000-8000-00000000c003",
        "2026-01-02T10:00:00Z",
        &AcceptedScope {
            allowed_scope: vec!["src/**/*.rs".to_string()],
            ..AcceptedScope::default()
        },
    )
    .expect("scope");
    let report =
        change_link::suggest(&ledger, "cccccccc-0000-4000-8000-00000000c003").expect("suggest");
    assert_eq!(
        report.unreadable_scope_patterns,
        vec!["src/**/*.rs".to_string()]
    );
    assert!(
        !report
            .suggestions
            .iter()
            .any(|s| s.basis == AssociationBasis::Filename),
        "an unreadable scope line proposes nothing on a filename basis"
    );
}

// ---------------------------------------------------------------------------
// A suggestion is only ever a suggestion
// ---------------------------------------------------------------------------

/// Recording proposals writes `suggested` rows, with no actor, and re-running
/// on an unchanged store writes nothing more (`B3.12`).
#[test]
fn recording_proposals_writes_only_suggestions_and_is_idempotent() {
    let (_dir, ledger, _root) = store();
    a_change(&ledger);
    let report = change_link::suggest(&ledger, CHANGE).expect("suggest");
    let first =
        change_link::record_suggestions(&ledger, &report, "2026-01-03T00:00:00Z").expect("record");
    assert_eq!(first.written.len(), report.suggestions.len());

    let rows: Vec<(String, Option<String>)> = ledger
        .conn()
        .prepare("SELECT provenance, actor FROM change_session_associations")
        .and_then(|mut s| {
            s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
                .and_then(|rows| rows.collect())
        })
        .expect("read back");
    assert!(
        rows.iter().all(|(p, a)| p == "suggested" && a.is_none()),
        "the suggester can write nothing but an unattributed suggestion: {rows:?}"
    );

    let again =
        change_link::record_suggestions(&ledger, &report, "2026-01-04T00:00:00Z").expect("again");
    assert!(
        again.written.is_empty(),
        "a second run proposes nothing new"
    );
    assert_eq!(again.skipped_existing.len(), report.suggestions.len());
}

/// A pair a person has already decided is left alone. A suggestion cannot
/// argue with a decision, and it certainly cannot overwrite one.
#[test]
fn a_decided_pair_is_not_re_suggested() {
    let (_dir, ledger, _root) = store();
    a_change(&ledger);
    change_link::decide(
        &ledger,
        CHANGE,
        IN_WINDOW,
        AssociationProvenance::Rejected,
        "sam",
        "different feature",
        "2026-01-03T00:00:00Z",
    )
    .expect("reject");
    let report = change_link::suggest(&ledger, CHANGE).expect("suggest");
    let outcome =
        change_link::record_suggestions(&ledger, &report, "2026-01-04T00:00:00Z").expect("record");
    assert!(outcome.written.is_empty());
    assert_eq!(outcome.skipped_decided.len(), 2, "both bases skipped");
    let standing = change_link::standing_link(&ledger, CHANGE, IN_WINDOW)
        .expect("read")
        .expect("a standing link");
    assert_eq!(standing.provenance, AssociationProvenance::Rejected);
}

// ---------------------------------------------------------------------------
// No fabricated criteria (B3.16)
// ---------------------------------------------------------------------------

/// Associating an old session creates no acceptance record and no criteria.
///
/// The temptation this guards is real: migrating historical work is much
/// tidier if you invent the criteria it "must have" been judged against. That
/// is a fabrication with a person's name on it, and the brief forbids it.
#[test]
fn no_act_here_writes_a_criterion_or_an_acceptance() {
    let (_dir, ledger, _root) = store();
    a_change(&ledger);
    let before = scope_fingerprint(&ledger);

    let report = change_link::suggest(&ledger, CHANGE).expect("suggest");
    change_link::record_suggestions(&ledger, &report, "2026-01-03T00:00:00Z").expect("record");
    change_link::decide(
        &ledger,
        CHANGE,
        IN_WINDOW,
        AssociationProvenance::Confirmed,
        "sam",
        "yes, this is the one",
        "2026-01-03T01:00:00Z",
    )
    .expect("confirm");
    change_link::decide(
        &ledger,
        CHANGE,
        OUT_OF_WINDOW,
        AssociationProvenance::Rejected,
        "sam",
        "no",
        "2026-01-03T02:00:00Z",
    )
    .expect("reject");
    change_link::change_results(&ledger, CHANGE).expect("results");
    change_link::session_links(&ledger, IN_WINDOW).expect("links");

    assert_eq!(
        before,
        scope_fingerprint(&ledger),
        "no association act may add, change or remove a criterion or an acceptance"
    );
}

/// Every row of both scope tables, as one comparable string.
fn scope_fingerprint(ledger: &Ledger) -> String {
    let read = |sql: &str| -> String {
        ledger
            .conn()
            .prepare(sql)
            .and_then(|mut s| {
                s.query_map([], |r| r.get::<_, String>(0))
                    .and_then(|rows| rows.collect::<Result<Vec<_>, _>>())
            })
            .map(|v| v.join("|"))
            .unwrap_or_else(|e| format!("unreadable: {e}"))
    };
    format!(
        "{}\n{}",
        read(
            "SELECT change_id || ':' || scope_version || ':' || content_sha256
               FROM change_scope_revisions ORDER BY change_id, scope_version"
        ),
        read(
            "SELECT change_id || ':' || scope_version || ':' || accepted_by
               FROM change_scope_acceptances ORDER BY change_id, scope_version"
        )
    )
}

// ---------------------------------------------------------------------------
// The pieces the suggester is built from
// ---------------------------------------------------------------------------

/// The edit vocabulary is a pinned list, not an inline guess. Widening what
/// counts as "a file was changed" widens what counts as in scope.
#[test]
fn the_tools_that_mean_a_file_changed_are_a_closed_list() {
    assert_eq!(
        change_link::EDIT_TOOLS,
        &[
            "edit",
            "write",
            "multiedit",
            "notebookedit",
            "str_replace_editor",
            "apply_patch"
        ]
    );
    assert!(
        !change_link::EDIT_TOOLS.contains(&"read"),
        "reading a file is not changing it"
    );
    assert!(
        !change_link::EDIT_TOOLS.contains(&"bash"),
        "a shell command could do anything, which is not evidence that it did this"
    );
}

/// Three shapes are understood and everything else says so.
#[test]
fn the_scope_matcher_understands_three_shapes_and_admits_the_rest() {
    use thruline_core::change_link::{pattern_matches, PatternVerdict};
    assert_eq!(
        pattern_matches("src/upload/**", "src/upload/retry.rs"),
        PatternVerdict::Matched
    );
    assert_eq!(
        pattern_matches("src/upload/", "src/upload/retry.rs"),
        PatternVerdict::Matched
    );
    assert_eq!(
        pattern_matches("src/upload/retry.rs", "src/upload/retry.rs"),
        PatternVerdict::Matched
    );
    assert_eq!(
        pattern_matches("**", "anything/at/all"),
        PatternVerdict::Matched
    );
    assert_eq!(
        pattern_matches("src/upload/**", "src/uploads/retry.rs"),
        PatternVerdict::DidNotMatch,
        "a prefix must end at a path separator, or `src/upload` swallows `src/uploads`"
    );
    assert_eq!(
        pattern_matches("src/*/retry.rs", "src/upload/retry.rs"),
        PatternVerdict::Unreadable
    );
    assert_eq!(pattern_matches("", "x"), PatternVerdict::Unreadable);
}

/// Only the exact stored form is treated as a time. Anything else is "we could
/// not tell", because a mis-sorted timestamp is a wrong answer that looks right.
#[test]
fn only_the_stores_own_time_form_is_treated_as_a_time() {
    assert!(change_link::is_canonical_utc("2026-01-02T09:00:00Z"));
    for bad in [
        "2026-01-02 09:00:00Z",
        "2026-01-02T09:00:00+05:30",
        "2026-01-02T09:00:00.123Z",
        "2026-01-02T09:00:00",
        "",
        "yesterday",
    ] {
        assert!(!change_link::is_canonical_utc(bad), "{bad:?} is not a time");
    }
}

/// Windows touch at a shared instant and not one second either side.
#[test]
fn two_windows_overlap_exactly_when_they_share_an_instant() {
    let w = |from: &str, to: &str| Window {
        from: from.to_string(),
        to: to.to_string(),
    };
    let change = w("2026-01-02T09:00:00Z", "2026-01-02T10:00:00Z");
    assert!(change.overlaps(&w("2026-01-02T10:00:00Z", "2026-01-02T11:00:00Z")));
    assert!(!change.overlaps(&w("2026-01-02T10:00:01Z", "2026-01-02T11:00:00Z")));
    assert!(!change.overlaps(&w("2026-01-02T08:00:00Z", "2026-01-02T08:59:59Z")));
}

// ---------------------------------------------------------------------------
// Masking, at every new serving boundary
// ---------------------------------------------------------------------------

/// Every view this task adds masks its text before it returns, and says the
/// masker ran. The reader is the boundary, so no caller can forget.
#[test]
fn every_new_view_masks_before_it_serves() {
    let (_dir, ledger, _root) = store();
    // A Change whose title carries something credential-shaped, and a person's
    // reason that does too.
    change::insert_change(
        &ledger,
        &NewChange {
            id: "cccccccc-0000-4000-8000-00000000c004".to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-01-02T09:00:00Z".to_string(),
            request: ChangeRequest {
                title: "Rotate AKIAIOSFODNN7EXAMPLE".to_string(),
                purpose: "Synthetic.".to_string(),
                prompt_event_id: None,
                requester: "sam".to_string(),
                source: RequestSource::Person,
            },
        },
    )
    .expect("insert");
    change_link::decide(
        &ledger,
        "cccccccc-0000-4000-8000-00000000c004",
        IN_WINDOW,
        AssociationProvenance::Confirmed,
        "sam",
        "the key AKIAIOSFODNN7EXAMPLE is in here",
        "2026-01-03T00:00:00Z",
    )
    .expect("confirm");

    let links = change_link::session_links(&ledger, IN_WINDOW).expect("links");
    let encoded = serde_json::to_string(&links).expect("encode");
    assert!(
        !encoded.contains("AKIAIOSFODNN7EXAMPLE"),
        "raw stored text left the store: {encoded}"
    );
    assert!(
        matches!(links.masking, thruline_core::redact::Treatment::Ran { .. }),
        "the view must state that a masker ran: {:?}",
        links.masking
    );
    assert!(links.masking.masked() >= 1, "it must say what it found");

    let standing =
        change_link::standing_link(&ledger, "cccccccc-0000-4000-8000-00000000c004", IN_WINDOW)
            .expect("read")
            .expect("a standing link");
    assert!(
        !standing
            .reason
            .unwrap_or_default()
            .contains("AKIAIOSFODNN7EXAMPLE"),
        "the reason a person typed is masked like every other stored text"
    );

    let results =
        change_link::change_results(&ledger, "cccccccc-0000-4000-8000-00000000c004").expect("read");
    assert!(matches!(
        results.masking,
        thruline_core::redact::Treatment::Ran { .. }
    ));
}

// ---------------------------------------------------------------------------
// T-2-9 / T-2-8-F2 — the timestamps vendors actually write
// ---------------------------------------------------------------------------

/// A session stamped the way Claude Code stamps one is dated, and suggested.
///
/// The fixture is a **real-shaped** stamp — `2026-09-18T08:42:07.914Z`, copied
/// in shape from a real capture — not a hand-rounded one. That is the whole
/// point of the test: every stamp in this file before today was already in the
/// store's own 20-byte form, so every one of them passed while the product
/// could not date a single real recording. `T-2-8-F2` measured that: two real
/// sessions, both filed undated, timing suggestion unable to fire at all.
#[test]
fn a_session_stamped_with_milliseconds_is_dated_and_suggested() {
    let (_dir, ledger, root) = store();
    a_change(&ledger);
    let millis = "cccccccc-0000-4000-8000-00000000s0m1";
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES(?1, ?2, '/tmp/synthetic-project/m.jsonl', NULL)",
            params![millis, PROJECT],
        )
        .expect("insert session");
    // Inside the Change's window (opened 09:00, last written 10:00), spelled
    // the way the vendor spells it.
    add_edit(
        &ledger,
        millis,
        1,
        "2026-01-02T09:42:07.914Z",
        "Edit",
        &format!("{root}/README.md"),
    );
    add_edit(
        &ledger,
        millis,
        2,
        "2026-01-02T09:42:44.999Z",
        "Edit",
        &format!("{root}/README.md"),
    );

    let window = change_link::session_window(&ledger, millis)
        .expect("read")
        .expect("a session stamped with milliseconds has a window");
    assert_eq!(
        window,
        Window {
            from: "2026-01-02T09:42:07Z".to_string(),
            to: "2026-01-02T09:42:44Z".to_string(),
        },
        "the fraction is truncated to the second the work was already under way in"
    );

    let report = change_link::suggest(&ledger, CHANGE).expect("suggest");
    assert!(
        !report.undated_sessions.contains(&millis.to_string()),
        "a session with a readable stamp is not undated: {:?}",
        report.undated_sessions
    );
    assert!(
        report
            .suggestions
            .iter()
            .any(|s| s.session_id == millis && s.basis == AssociationBasis::Timing),
        "the timing basis must fire on a real-shaped stamp: {:?}",
        report.suggestions
    );
}

/// The two spellings that are accepted, the ones that are not, and the reason
/// the line is drawn there.
#[test]
fn a_stamp_is_normalised_when_it_can_be_and_refused_when_it_would_need_a_guess() {
    for (written, canonical) in [
        ("2026-09-18T08:42:07Z", "2026-09-18T08:42:07Z"),
        ("2026-09-18T08:42:07.914Z", "2026-09-18T08:42:07Z"),
        ("2026-09-18T08:42:07.914229Z", "2026-09-18T08:42:07Z"),
        ("2026-09-18T08:42:07+00:00", "2026-09-18T08:42:07Z"),
        ("2026-09-18T08:42:07.914+00:00", "2026-09-18T08:42:07Z"),
        ("2026-09-18T08:42:07-00:00", "2026-09-18T08:42:07Z"),
    ] {
        assert_eq!(
            change_link::normalise_utc(written).as_deref(),
            Some(canonical),
            "{written:?} is a time this matcher can read"
        );
    }
    // A non-zero offset is a different instant and converting it needs
    // arithmetic on a calendar. The answer stays "we could not tell".
    for refused in [
        "2026-09-18T08:42:07+05:30",
        "2026-09-18T08:42:07.914+05:30",
        "2026-09-18 08:42:07Z",
        "2026-09-18T08:42:07.Z",
        "2026-09-18T08:42:07.abcZ",
        "2026-09-18T08:42:07",
        "yesterday",
        "",
    ] {
        assert_eq!(
            change_link::normalise_utc(refused),
            None,
            "{refused:?} is not a time this matcher may guess at"
        );
    }
}

/// The normalisation is at READ. Nothing is written back, so no store grows by
/// a byte for it (`03-USER-LENS.md` rule 13).
#[test]
fn reading_a_millisecond_stamp_writes_nothing_back() {
    let (_dir, ledger, root) = store();
    a_change(&ledger);
    let millis = "cccccccc-0000-4000-8000-00000000s0m2";
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES(?1, ?2, '/tmp/synthetic-project/m.jsonl', NULL)",
            params![millis, PROJECT],
        )
        .expect("insert session");
    add_edit(
        &ledger,
        millis,
        1,
        "2026-01-02T09:42:07.914Z",
        "Edit",
        &format!("{root}/README.md"),
    );
    let before: String = ledger
        .conn()
        .query_row(
            "SELECT ts FROM events WHERE session_id = ?1",
            params![millis],
            |row| row.get(0),
        )
        .expect("read the stamp back");
    change_link::suggest(&ledger, CHANGE).expect("suggest");
    let after: String = ledger
        .conn()
        .query_row(
            "SELECT ts FROM events WHERE session_id = ?1",
            params![millis],
            |row| row.get(0),
        )
        .expect("read the stamp back");
    assert_eq!(before, after, "the stored stamp is the vendor's own bytes");
    assert_eq!(before, "2026-01-02T09:42:07.914Z");
}
