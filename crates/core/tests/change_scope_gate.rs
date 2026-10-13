//! T-2-2 — the refusal that keeps one session's good news off the wrong
//! Change (`B3.13`), and the census that keeps it the only door.
//!
//! The brief's sentence is "never transfer a session's successful run to every
//! linked Change without verifying scope". Two things have to be true for that
//! to hold in a codebase rather than in a document:
//!
//! 1. the check exists and refuses all three ways it can be wrong — no link, a
//!    link that is only a suggestion, and a confirmed link whose work is
//!    outside what the Change covers;
//! 2. **it is the only check**, so a second reader cannot grow its own idea of
//!    what counts as linked. The census at the bottom of this file re-derives
//!    every call site from the sources and fails naming any new one.
//!
//! Every id is synthetic and no fixture here holds private content.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rusqlite::params;
use thruline_core::change::{
    self, AcceptedScope, AssociationProvenance, ChangeRequest, NewChange, RequestSource,
};
use thruline_core::change_link::{self, Applicability};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
/// The project's working folder. It is made on disk, because a changed path
/// that is absolute only becomes scope under a root the capture established
/// (T-3-2b); a root that is not a folder on this machine abstains, and every
/// path under it with it.
const ROOT_NAME: &str = "synthetic-project";
/// Changed `src/upload/retry.rs` — inside the Change's scope.
const IN_SCOPE: &str = "cccccccc-0000-4000-8000-00000000s001";
/// Changed `docs/history.md` — outside it.
const OUT_OF_SCOPE: &str = "cccccccc-0000-4000-8000-00000000s002";
/// Changed nothing the record can read.
const NO_PATHS: &str = "cccccccc-0000-4000-8000-00000000s003";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c001";
const OTHER_CHANGE: &str = "cccccccc-0000-4000-8000-00000000c002";

fn store() -> (tempfile::TempDir, Ledger) {
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
    for session in [IN_SCOPE, OUT_OF_SCOPE, NO_PATHS] {
        ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id, project_id, file_path, started_at)
                 VALUES(?1, ?2, ?3, '2026-01-02T09:30:00Z')",
                params![session, PROJECT, format!("{root}/s.jsonl")],
            )
            .expect("insert session");
    }
    edit(&ledger, IN_SCOPE, 1, &format!("{root}/src/upload/retry.rs"));
    edit(&ledger, OUT_OF_SCOPE, 1, &format!("{root}/docs/history.md"));
    // A verdict on each session, so "was a number attached" is answerable.
    for (seq, session) in [IN_SCOPE, OUT_OF_SCOPE, NO_PATHS].iter().enumerate() {
        verdict(&ledger, session, seq as i64 + 1);
    }
    for (id, title) in [
        (CHANGE, "Retry failed uploads"),
        (OTHER_CHANGE, "Rewrite the docs"),
    ] {
        change::insert_change(
            &ledger,
            &NewChange {
                id: id.to_string(),
                project_id: PROJECT.to_string(),
                opened_at: "2026-01-02T09:00:00Z".to_string(),
                request: ChangeRequest {
                    title: title.to_string(),
                    purpose: "Synthetic.".to_string(),
                    prompt_event_id: None,
                    requester: "sam".to_string(),
                    source: RequestSource::Person,
                },
            },
        )
        .expect("insert change");
    }
    change::append_scope(
        &ledger,
        CHANGE,
        "2026-01-02T10:00:00Z",
        &AcceptedScope {
            allowed_scope: vec!["src/upload/**".to_string()],
            ..AcceptedScope::default()
        },
    )
    .expect("scope");
    change::append_scope(
        &ledger,
        OTHER_CHANGE,
        "2026-01-02T10:00:00Z",
        &AcceptedScope {
            allowed_scope: vec!["docs/**".to_string()],
            ..AcceptedScope::default()
        },
    )
    .expect("scope");
    (dir, ledger)
}

fn edit(ledger: &Ledger, session: &str, seq: i64, path: &str) {
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES(?1, ?2, ?3, ?4, '2026-01-02T09:31:00Z', 'tool_use', 'Edit', ?5,
                    '/tmp/s.jsonl', ?4, 'x', '', '')",
            params![
                format!("cccccccc-ev-{session}-{seq}"),
                PROJECT,
                session,
                seq,
                format!("{{\"file_path\":\"{path}\"}}"),
            ],
        )
        .expect("insert event");
}

/// One recorded `supported` verdict for a session — the "successful run" the
/// brief's sentence is about.
///
/// Both inserts are strict. A fixture that silently fails to write the thing
/// the test is about is a test that passes for the wrong reason, which is the
/// exact defect the gate exists against.
fn verdict(ledger: &Ledger, session: &str, seq: i64) {
    ledger
        .conn()
        .execute(
            "INSERT INTO mcp_calls(call_id, agent, tool, scope, request_sha256, outcome,
                 accepted_at_unix_ms, accepted_seq)
             VALUES(?1, 'synthetic', 'claim_completion', 'synthetic', 'x',
                    'ready_for_verification', 0, ?2)",
            params![format!("call-{session}"), seq],
        )
        .expect("insert the recorded call");
    ledger
        .conn()
        .execute(
            "INSERT INTO claim_verdicts(id, seq, recorded_at, call_id, verdict, reason,
                 detail_json, session_id, evidence_rows_scanned, spilled_deliveries,
                 content_sha256, prev_hash, chain_hash)
             VALUES(?1, ?2, '2026-01-02T09:41:00Z', ?3, 'supported', 'r', '{}', ?4, 1, 0,
                    'x', '', '')",
            params![
                format!("cvrd-{session}"),
                seq,
                format!("call-{session}"),
                session,
            ],
        )
        .expect("insert the recorded verdict");
}

fn confirm(ledger: &Ledger, change_id: &str, session: &str) {
    change_link::decide(
        ledger,
        change_id,
        session,
        AssociationProvenance::Confirmed,
        "sam",
        "yes",
        "2026-01-03T00:00:00Z",
    )
    .expect("confirm");
}

// ---------------------------------------------------------------------------
// The four cases the card names, in order
// ---------------------------------------------------------------------------

/// (a) No link at all. Nothing is attached and the reason is named.
#[test]
fn a_session_with_no_link_is_not_counted() {
    let (_dir, ledger) = store();
    let verdict = change_link::applicability(&ledger, CHANGE, IN_SCOPE).expect("gate");
    assert_eq!(verdict.slug(), "not_applicable_no_link");
    assert!(!verdict.applies());
}

/// (b) A suggestion is not a confirmation. This is the heart of `B3.12`: the
/// tool's own guess must never be enough to move a result.
#[test]
fn a_suggested_link_is_not_counted() {
    let (_dir, ledger) = store();
    let report = change_link::suggest(&ledger, CHANGE).expect("suggest");
    change_link::record_suggestions(&ledger, &report, "2026-01-03T00:00:00Z").expect("record");
    let verdict = change_link::applicability(&ledger, CHANGE, IN_SCOPE).expect("gate");
    assert_eq!(verdict.slug(), "not_applicable_only_suggested");
    let results = change_link::change_results(&ledger, CHANGE).expect("results");
    let row = results
        .sessions
        .iter()
        .find(|s| s.session_id == IN_SCOPE)
        .expect("listed");
    assert!(
        row.verdict_counts.is_empty(),
        "a suggestion carries no number: {:?}",
        row.verdict_counts
    );
}

/// (c) Confirmed, but the work is somewhere else. This is the failure the
/// brief names in so many words, and the one a confirm button makes easy.
#[test]
fn a_confirmed_link_whose_work_is_out_of_scope_is_not_counted() {
    let (_dir, ledger) = store();
    confirm(&ledger, CHANGE, OUT_OF_SCOPE);
    let verdict = change_link::applicability(&ledger, CHANGE, OUT_OF_SCOPE).expect("gate");
    assert_eq!(verdict.slug(), "not_applicable_outside_scope");
    match verdict {
        Applicability::NotApplicable { outside_paths, .. } => {
            assert_eq!(outside_paths, vec!["docs/history.md".to_string()])
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// (d) Confirmed and in scope. Only now is a number read, and it comes from
/// the stored `verdict` column rather than from any prose.
#[test]
fn a_confirmed_link_inside_scope_is_counted() {
    let (_dir, ledger) = store();
    confirm(&ledger, CHANGE, IN_SCOPE);
    let verdict = change_link::applicability(&ledger, CHANGE, IN_SCOPE).expect("gate");
    assert!(verdict.applies());
    assert_eq!(verdict.slug(), "applies_in_scope");
    match verdict {
        Applicability::Applies {
            scope_version,
            in_scope_paths,
        } => {
            assert_eq!(
                scope_version, 1,
                "the version it was checked against is named"
            );
            assert_eq!(in_scope_paths, vec!["src/upload/retry.rs".to_string()]);
        }
        other => panic!("expected an attachment, got {other:?}"),
    }
    let results = change_link::change_results(&ledger, CHANGE).expect("results");
    let row = results
        .sessions
        .iter()
        .find(|s| s.session_id == IN_SCOPE)
        .expect("listed");
    assert_eq!(row.verdict_counts.get("supported"), Some(&1));
}

/// A rejection is not an absence: it has its own reason and its own sentence.
#[test]
fn a_rejected_link_is_not_counted_and_says_which_refusal_it_is() {
    let (_dir, ledger) = store();
    change_link::decide(
        &ledger,
        CHANGE,
        IN_SCOPE,
        AssociationProvenance::Rejected,
        "sam",
        "that was a different feature",
        "2026-01-03T00:00:00Z",
    )
    .expect("reject");
    let verdict = change_link::applicability(&ledger, CHANGE, IN_SCOPE).expect("gate");
    assert_eq!(verdict.slug(), "not_applicable_rejected");
}

/// A Change nobody has scoped cannot pass the check, rather than passing it by
/// default. Missing evidence never renders as a pass.
#[test]
fn a_change_with_no_scope_cannot_count_anything() {
    let (_dir, ledger) = store();
    change::insert_change(
        &ledger,
        &NewChange {
            id: "cccccccc-0000-4000-8000-00000000c003".to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-01-02T09:00:00Z".to_string(),
            request: ChangeRequest {
                title: "Unscoped".to_string(),
                purpose: "Nobody wrote down what counts as done.".to_string(),
                prompt_event_id: None,
                requester: "sam".to_string(),
                source: RequestSource::Person,
            },
        },
    )
    .expect("insert");
    confirm(&ledger, "cccccccc-0000-4000-8000-00000000c003", IN_SCOPE);
    let verdict =
        change_link::applicability(&ledger, "cccccccc-0000-4000-8000-00000000c003", IN_SCOPE)
            .expect("gate");
    assert_eq!(verdict.slug(), "not_applicable_no_scope");
}

/// "We could not read what this session changed" is not "it is fine".
#[test]
fn a_session_whose_changed_files_are_unreadable_cannot_be_counted() {
    let (_dir, ledger) = store();
    confirm(&ledger, CHANGE, NO_PATHS);
    let verdict = change_link::applicability(&ledger, CHANGE, NO_PATHS).expect("gate");
    assert_eq!(verdict.slug(), "not_applicable_no_changed_paths");
}

/// The card's walkthrough B.11, end to end: the same session confirmed on two
/// Changes counts on exactly the one whose scope it is inside.
#[test]
fn one_sessions_result_does_not_arrive_on_every_change_it_is_linked_to() {
    let (_dir, ledger) = store();
    confirm(&ledger, CHANGE, IN_SCOPE);
    confirm(&ledger, OTHER_CHANGE, IN_SCOPE);
    let here = change_link::change_results(&ledger, CHANGE).expect("results");
    let there = change_link::change_results(&ledger, OTHER_CHANGE).expect("results");
    assert_eq!(
        here.sessions[0].verdict_counts.get("supported"),
        Some(&1),
        "the Change whose scope this work is inside counts it"
    );
    assert!(
        there.sessions[0].verdict_counts.is_empty(),
        "the other Change counts nothing, though the link is confirmed on both"
    );
    assert_eq!(
        there.sessions[0].sentence_slug,
        "not_applicable_outside_scope"
    );
}

/// Every refusal is visible. A session that does not pass is listed with its
/// sentence rather than dropped from the answer, because a refusal Sam cannot
/// see is a refusal Sam will not believe.
#[test]
fn a_session_that_fails_the_check_is_listed_rather_than_hidden() {
    let (_dir, ledger) = store();
    confirm(&ledger, CHANGE, OUT_OF_SCOPE);
    let results = change_link::change_results(&ledger, CHANGE).expect("results");
    assert_eq!(results.sessions.len(), 1);
    assert_eq!(results.sessions[0].session_id, OUT_OF_SCOPE);
    assert_eq!(
        results.sessions[0].sentence_slug,
        "not_applicable_outside_scope"
    );
}

/// The served shape, pinned.
#[test]
fn the_gated_results_are_served_whole() {
    let (_dir, ledger) = store();
    confirm(&ledger, CHANGE, IN_SCOPE);
    confirm(&ledger, CHANGE, OUT_OF_SCOPE);
    let results = change_link::change_results(&ledger, CHANGE).expect("results");
    insta::assert_json_snapshot!("change_results", results);
}

/// Confirming a link copies no bytes. `B3.13` and Rule 13: an association is a
/// row, not a copy, and the store grows by the row.
#[test]
fn confirming_a_link_duplicates_no_event_or_verdict_bytes() {
    let (_dir, ledger) = store();
    let before = table_bytes(&ledger);
    confirm(&ledger, CHANGE, IN_SCOPE);
    change_link::change_results(&ledger, CHANGE).expect("results");
    let after = table_bytes(&ledger);
    assert_eq!(
        before, after,
        "confirming a link must not copy a byte of `events` or `claim_verdicts`"
    );
}

/// The exact bytes of every table a result could be copied *into*.
fn table_bytes(ledger: &Ledger) -> String {
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
        read("SELECT id || ':' || COALESCE(tool_input,'') FROM events ORDER BY id"),
        read("SELECT id || ':' || verdict || ':' || COALESCE(session_id,'') FROM claim_verdicts ORDER BY id"),
    )
}

// ---------------------------------------------------------------------------
// The census: one door, and it is enumerated from the sources
// ---------------------------------------------------------------------------

/// Every non-test call of the gate, re-derived from every workspace member's
/// sources, must be exactly this list.
///
/// This is the guard against the failure this repository has produced before:
/// a rule that holds in the one place it was written, and a second reader
/// somewhere else that decides for itself what "linked" means. A new caller is
/// not forbidden — it is forbidden to be *unnoticed*.
///
/// The reason each one may decide what counts as linked:
///
/// * `change_link.rs::change_results` — the original door. It is the only
///   function that serves a session's outcomes under a Change.
/// * `change_evidence.rs::candidates_and_why` — T-3-1. It builds the list of
///   evidence a Change may look at, and it must ask the same question with the
///   same answer: a run in a session nobody confirmed, or one whose paths fall
///   outside the accepted scope, is not evidence about anything here. It
///   re-implements nothing — it calls the gate, drops what fails, and carries
///   the first refusal out so the disabled control on the screen can say why,
///   which is why it is a caller rather than a second rule. It is ONE function
///   on purpose: the reason and the list were briefly two, and this census is
///   what said so.
const GATE_CALL_SITES: &[(&str, &str)] = &[
    ("change_evidence.rs", "candidates_and_why"),
    ("change_link.rs", "change_results"),
];

#[test]
fn the_scope_gate_has_exactly_one_caller_and_it_is_named_here() {
    let found = gate_call_sites(None);
    let expected: BTreeSet<(String, String)> = GATE_CALL_SITES
        .iter()
        .map(|(f, n)| ((*f).to_string(), (*n).to_string()))
        .collect();
    assert_eq!(
        found,
        expected,
        "the callers of `{}` drifted.\n unlisted: {:?}\n listed but absent: {:?}\n\
         A new caller must be added here deliberately, with the reason it may \
         decide what counts as linked.",
        thruline_core::change_link::SCOPE_GATE,
        found.difference(&expected).collect::<Vec<_>>(),
        expected.difference(&found).collect::<Vec<_>>()
    );
}

/// The census's own negative control: a second caller, injected into a real
/// source file, must be found. Without this the walk could be reading nothing
/// and the census above would pass forever.
#[test]
fn the_census_finds_a_second_caller_when_one_is_injected() {
    let found = gate_call_sites(Some((
        "change.rs",
        "\nfn a_second_reader_that_decides_for_itself(l: &Ledger) {\n    \
         let _ = crate::change_link::applicability(l, \"c\", \"s\");\n}\n",
    )));
    assert!(
        found.contains(&(
            "change.rs".to_string(),
            "a_second_reader_that_decides_for_itself".to_string()
        )),
        "the walk did not see an injected caller, so it is not reading the sources: {found:?}"
    );
}

/// The association table is read in exactly two modules. A third would be a
/// second idea of what a link is.
#[test]
fn the_association_table_is_read_in_exactly_two_modules() {
    let mut found = BTreeSet::new();
    for src in member_sources() {
        walk(&src, &mut |path, raw| {
            if strip_cfg_test(raw).contains("change_session_associations") {
                found.insert(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                );
            }
        });
    }
    assert_eq!(
        found,
        ["change.rs", "change_link.rs", "ledger.rs"]
            .iter()
            .map(|s| s.to_string())
            .collect::<BTreeSet<_>>(),
        "the table is defined in ledger.rs, written in change.rs and reasoned \
         about in change_link.rs — a fourth module reading it is a second idea \
         of what a link is"
    );
}

fn gate_call_sites(injection: Option<(&str, &str)>) -> BTreeSet<(String, String)> {
    let mut found = BTreeSet::new();
    let mut injected = injection.is_none();
    for src in member_sources() {
        walk(&src, &mut |path, raw| {
            let file = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let mut text = raw.to_string();
            if let Some((target, snippet)) = injection {
                if file == target && !injected {
                    text.push_str(snippet);
                    injected = true;
                }
            }
            let text = strip_cfg_test(&text);
            let mut current = String::new();
            let mut in_definition = false;
            for line in text.lines() {
                let t = line.trim_start();
                for form in [
                    "pub(crate) fn ",
                    "pub(super) fn ",
                    "pub async fn ",
                    "pub const fn ",
                    "pub fn ",
                    "async fn ",
                    "const fn ",
                    "fn ",
                ] {
                    if let Some(rest) = t.strip_prefix(form) {
                        current = rest
                            .split(['(', '<'])
                            .next()
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        in_definition = current == thruline_core::change_link::SCOPE_GATE;
                        break;
                    }
                }
                if t.starts_with("//") || in_definition {
                    continue;
                }
                if line.contains(&format!("{}(", thruline_core::change_link::SCOPE_GATE)) {
                    found.insert((file.clone(), current.clone()));
                }
            }
        });
    }
    if let Some((target, _)) = injection {
        assert!(injected, "the control never found {target} to inject into");
    }
    found
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root")
        .to_path_buf()
}

fn member_sources() -> Vec<PathBuf> {
    let root = workspace_root();
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("workspace manifest");
    let line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("members"))
        .expect("members key");
    let inner = line
        .split_once('[')
        .and_then(|(_, r)| r.split_once(']'))
        .map(|(m, _)| m)
        .expect("members list");
    let members: Vec<PathBuf> = inner
        .split(',')
        .map(|m| m.trim().trim_matches('"').trim())
        .filter(|m| !m.is_empty())
        .map(|m| root.join(m).join("src"))
        .collect();
    assert!(members.len() >= 2, "the walk must cover every member crate");
    members
}

/// Strip `#[cfg(test)]` items by brace depth, so a test module cannot supply a
/// call site the shipping binary does not have.
fn strip_cfg_test(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if !line.trim_start().starts_with("#[cfg(test)]") {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let mut depth = 0i32;
        let mut started = false;
        for item in lines.by_ref() {
            depth += item.matches('{').count() as i32;
            depth -= item.matches('}').count() as i32;
            if item.contains('{') {
                started = true;
            }
            if started && depth <= 0 {
                break;
            }
            if !started && item.trim_end().ends_with(';') {
                break;
            }
        }
    }
    out
}

fn walk(dir: &Path, f: &mut impl FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, f);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                f(&path, &text);
            }
        }
    }
}

/// A person changing their mind inside one second is answered by what they
/// said last, not by whichever id sorts higher.
///
/// This test exists because the surface produced exactly this and got it
/// wrong: `change associate --confirm` then `--reject` in the same second left
/// the confirmation standing, because the tie between two equal `asserted_at`
/// values fell to a content hash. A decision decided by a hash is not a
/// decision.
#[test]
fn two_decisions_in_one_second_are_resolved_by_what_was_said_last() {
    let (_dir, ledger) = store();
    let same_instant = "2026-01-03T00:00:00Z";
    change_link::decide(
        &ledger,
        CHANGE,
        IN_SCOPE,
        AssociationProvenance::Confirmed,
        "sam",
        "yes",
        same_instant,
    )
    .expect("confirm");
    change_link::decide(
        &ledger,
        CHANGE,
        IN_SCOPE,
        AssociationProvenance::Rejected,
        "sam",
        "no, wrong one",
        same_instant,
    )
    .expect("reject");
    let standing = change_link::standing_link(&ledger, CHANGE, IN_SCOPE)
        .expect("read")
        .expect("a standing link");
    assert_eq!(
        standing.provenance,
        AssociationProvenance::Rejected,
        "the second decision stands"
    );
    assert_eq!(
        change_link::applicability(&ledger, CHANGE, IN_SCOPE)
            .expect("gate")
            .slug(),
        "not_applicable_rejected"
    );

    // And changing their mind back works the same way.
    change_link::decide(
        &ledger,
        CHANGE,
        IN_SCOPE,
        AssociationProvenance::Confirmed,
        "sam",
        "no, it was right",
        same_instant,
    )
    .expect("confirm again");
    assert!(
        change_link::applicability(&ledger, CHANGE, IN_SCOPE)
            .expect("gate")
            .applies(),
        "the third decision stands, and the first two are still on file"
    );
    let rows: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM change_session_associations WHERE change_id=?1
              AND session_id=?2",
            rusqlite::params![CHANGE, IN_SCOPE],
            |r| r.get(0),
        )
        .unwrap_or(-1);
    assert_eq!(rows, 3, "every decision is kept; none was edited");
}

/// And the ordering does not depend on the id being monotonic.
///
/// `decide` mints ULIDs, which happen to sort in insertion order, so the two
/// fixes for the walkthrough defect overlap. This one removes that coincidence:
/// the rows are appended through `change::append_association` with ids chosen
/// to sort *against* the order they were written in, at one instant. Insertion
/// order still decides.
#[test]
fn the_standing_link_does_not_depend_on_ids_sorting_in_order() {
    let (_dir, ledger) = store();
    let at = "2026-01-03T00:00:00Z";
    for (id, provenance) in [("zzz", "confirmed"), ("aaa", "rejected")] {
        change::append_association(
            &ledger,
            &change::NewAssociation {
                id: id.to_string(),
                change_id: CHANGE.to_string(),
                session_id: IN_SCOPE.to_string(),
                provenance: change::AssociationProvenance::parse(provenance).expect("word"),
                basis: change::AssociationBasis::User,
                actor: Some("sam".to_string()),
                asserted_at: at.to_string(),
                reason: Some("synthetic".to_string()),
            },
        )
        .expect("append");
    }
    let standing = change_link::standing_link(&ledger, CHANGE, IN_SCOPE)
        .expect("read")
        .expect("a standing link");
    assert_eq!(
        standing.provenance,
        AssociationProvenance::Rejected,
        "the row written second stands, though its id sorts first"
    );
}
