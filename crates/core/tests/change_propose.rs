//! T-2-3 — the tool proposes what counts as done, and only a person accepts it
//! (`B5.1`–`B5.10`, `B11.5`, `03-USER-LENS.md` rule 8).
//!
//! Sam's failure mode here is **gives up**: a screen that asks Sam to write
//! acceptance criteria before showing anything is the wall the user lens
//! forbids. The second is **doesn't trust it**: a proposal that reads "make
//! retries reliable" is not something Sam could check, and a tool that offers it
//! has spent the trust the rest of the product rests on. So the tests below are
//! about two things — that every proposal is a sentence someone could check, and
//! that nothing the tool proposes becomes agreed without one explicit act.
//!
//! Every id is synthetic (`cccccccc-…`) and no fixture here holds private
//! content.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::params;
use thruline_core::change::{
    self, AcceptedScope, AssociationProvenance, ChangeRequest, NewChange, RequestSource,
};
use thruline_core::change_link;
use thruline_core::change_propose::{
    self, Acceptance, Attribution, ChangeType, ProfileEnvironment, Selection, ShapeRefusal,
    MAX_SCOPE_VERSIONS,
};
use thruline_core::ledger::Ledger;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000p001";
const ROOT: &str = "/tmp/synthetic-project";
const CONFIRMED: &str = "cccccccc-0000-4000-8000-00000000s001";
const ONLY_SUGGESTED: &str = "cccccccc-0000-4000-8000-00000000s002";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c001";
const BARREN: &str = "cccccccc-0000-4000-8000-00000000c002";
const BARREN_SESSION: &str = "cccccccc-0000-4000-8000-00000000s003";

/// The request a person wrote, as this fixture's Change carries it.
const PURPOSE: &str =
    "Uploads that fail once are lost. When an upload fails, the queue keeps one entry for it.";

/// The opening request of the confirmed session. Four of its five sentences
/// cannot be cut into three parts, which is the realistic ratio and the reason
/// the notes list exists.
const CONFIRMED_ASK: &str = "Retries are a total mess right now. \
     Two retry requests for the same delivery do not enqueue duplicate work. \
     When a delivery fails three times, the record shows a single failure row. \
     Make retries reliable. \
     Please also update the documentation at some point.";

/// A session nobody has confirmed. Its request is perfectly criterion-shaped,
/// which is exactly why it must not contribute one.
const SUGGESTED_ASK: &str =
    "The dashboard shows a banner when the upload queue is empty. Ship it today.";

fn store() -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open a fresh ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, ?2, 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
            params![PROJECT, ROOT],
        )
        .expect("insert project");
    for session in [CONFIRMED, ONLY_SUGGESTED, BARREN_SESSION] {
        ledger
            .conn()
            .execute(
                "INSERT INTO sessions(id, project_id, file_path, started_at)
                 VALUES(?1, ?2, '/tmp/synthetic-project/s.jsonl', '2026-01-02T09:30:00Z')",
                params![session, PROJECT],
            )
            .expect("insert session");
    }
    add_message(&ledger, CONFIRMED, 1, CONFIRMED_ASK);
    add_edit(&ledger, CONFIRMED, 2, "Edit", "src/upload/retry.rs");
    add_edit(&ledger, CONFIRMED, 3, "Write", "src/upload/queue.rs");
    add_edit(&ledger, CONFIRMED, 4, "Edit", "docs/notes.md");
    add_message(&ledger, ONLY_SUGGESTED, 1, SUGGESTED_ASK);
    add_edit(&ledger, ONLY_SUGGESTED, 2, "Edit", "src/upload/other.rs");
    add_message(
        &ledger,
        BARREN_SESSION,
        1,
        "Make retries reliable. Clean this up please.",
    );
    add_edit(&ledger, BARREN_SESSION, 2, "Edit", "src/upload/barren.rs");
    (dir, ledger)
}

fn add_message(ledger: &Ledger, session: &str, seq: i64, text: &str) {
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, text,
                 raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES(?1, ?2, ?3, ?4, '2026-01-02T09:30:00Z', 'user_msg', ?5,
                 '/tmp/s.jsonl', ?4, 'x', '', '')",
            params![
                format!("cccccccc-ev-{session}-{seq}"),
                PROJECT,
                session,
                seq,
                text
            ],
        )
        .expect("insert message");
}

fn add_edit(ledger: &Ledger, session: &str, seq: i64, tool: &str, relative: &str) {
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES(?1, ?2, ?3, ?4, '2026-01-02T09:31:00Z', 'tool_use', ?5, ?6,
                 '/tmp/s.jsonl', ?4, 'x', '', '')",
            params![
                format!("cccccccc-ev-{session}-{seq}"),
                PROJECT,
                session,
                seq,
                tool,
                format!("{{\"file_path\":\"{relative}\"}}"),
            ],
        )
        .expect("insert edit");
}

/// The Change, its scope, and one confirmed link plus one that stays a
/// suggestion.
fn a_change(ledger: &Ledger) {
    change::insert_change(
        ledger,
        &NewChange {
            id: CHANGE.to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-01-02T09:00:00Z".to_string(),
            request: ChangeRequest {
                title: "Retry failed uploads".to_string(),
                purpose: PURPOSE.to_string(),
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
        "2026-01-02T09:05:00Z",
        &AcceptedScope {
            criteria: Vec::new(),
            exclusions: vec!["anything about billing".to_string()],
            allowed_scope: vec!["src/upload/**".to_string()],
            constraints: vec!["no new dependencies".to_string()],
            check_profile: Vec::new(),
        },
    )
    .expect("append the first scope");
    change_link::decide(
        ledger,
        CHANGE,
        CONFIRMED,
        AssociationProvenance::Confirmed,
        "sam",
        "this is the retry work",
        "2026-01-02T09:10:00Z",
    )
    .expect("confirm the link");
    change::append_association(
        ledger,
        &change::NewAssociation {
            id: change_link::suggestion_id(
                CHANGE,
                ONLY_SUGGESTED,
                change::AssociationBasis::Timing,
            ),
            change_id: CHANGE.to_string(),
            session_id: ONLY_SUGGESTED.to_string(),
            provenance: AssociationProvenance::Suggested,
            basis: change::AssociationBasis::Timing,
            actor: None,
            asserted_at: "2026-01-02T09:11:00Z".to_string(),
            reason: None,
        },
    )
    .expect("record the suggestion");
}

/// A Change whose every sentence is a wish. The `B.9` case: nothing to propose,
/// and nothing that may be accepted.
fn a_barren_change(ledger: &Ledger) {
    change::insert_change(
        ledger,
        &NewChange {
            id: BARREN.to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-01-03T09:00:00Z".to_string(),
            request: ChangeRequest {
                title: "Tidy up".to_string(),
                purpose: "Things are bad and should be better.".to_string(),
                prompt_event_id: None,
                requester: "sam".to_string(),
                source: RequestSource::Person,
            },
        },
    )
    .expect("insert the barren change");
    change_link::decide(
        ledger,
        BARREN,
        BARREN_SESSION,
        AssociationProvenance::Confirmed,
        "sam",
        "this is the tidy-up",
        "2026-01-03T09:10:00Z",
    )
    .expect("confirm the barren link");
}

fn machine() -> ProfileEnvironment {
    ProfileEnvironment::empty()
}

fn accepting(selection: Selection) -> Acceptance {
    Acceptance {
        selection,
        scope: thruline_core::change_propose::ScopeChoice::AsProposed,
        replacing: false,
        edits: BTreeMap::new(),
        actor: "sam".to_string(),
        rationale: "this is what I want checked".to_string(),
        accepted_at: "2026-01-02T11:00:00Z".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Determinism
// ---------------------------------------------------------------------------

/// The same store proposes the same criteria, in the same order, twice.
///
/// A proposer that wobbles cannot be reviewed: the numbers Sam typed into
/// `--criterion 2` would mean a different line on the next reading.
#[test]
fn the_same_store_proposes_the_same_criteria_twice() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let first = serde_json::to_string(
        &change_propose::propose(&ledger, CHANGE, &machine()).expect("first"),
    )
    .expect("encode");
    let second = serde_json::to_string(
        &change_propose::propose(&ledger, CHANGE, &machine()).expect("second"),
    )
    .expect("encode");
    assert_eq!(first, second, "two readings of one store must agree");

    let view = change_propose::propose(&ledger, CHANGE, &machine()).expect("third");
    let numbers: Vec<u32> = view.criteria.iter().map(|c| c.number).collect();
    assert_eq!(
        numbers,
        (1..=view.criteria.len() as u32).collect::<Vec<_>>(),
        "numbering is 1..n in reading order, so a number means one line"
    );
}

/// The whole served shape, pinned. A field that appears or vanishes is a
/// snapshot diff rather than a surprise on somebody's screen.
#[test]
fn the_proposals_and_everything_they_rest_on_are_served_whole() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let view = change_propose::propose(&ledger, CHANGE, &machine()).expect("propose");
    insta::assert_json_snapshot!("proposal_view", view);
}

// ---------------------------------------------------------------------------
// Observable, or kept as a note
// ---------------------------------------------------------------------------

/// Every proposal has all three parts, and the parts are the person's own
/// words — each one appears in the criterion text.
#[test]
fn every_proposed_criterion_has_all_three_parts() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let view = change_propose::propose(&ledger, CHANGE, &machine()).expect("propose");
    assert!(
        !view.criteria.is_empty(),
        "the fixture must propose something"
    );
    for criterion in &view.criteria {
        for part in [
            &criterion.shape.subject,
            &criterion.shape.condition,
            &criterion.shape.outcome,
        ] {
            assert!(
                !part.trim().is_empty(),
                "{:?} is missing a part: {:?}",
                criterion.text,
                criterion.shape
            );
            assert!(
                criterion.text.contains(part.as_str()),
                "part {part:?} is not a span of {:?} — the proposer rewrote it",
                criterion.text
            );
        }
        assert!(
            change_propose::observable_shape(&criterion.text).is_ok(),
            "{:?} does not re-read as observable",
            criterion.text
        );
    }
}

/// **The negative control.** The brief's own example of an unacceptable
/// requirement is refused, and the reason names what was missing.
///
/// A shape check that only ever says yes is not a check. This is the sentence
/// `B5.2` writes out, and the one beside it that `B5.2` says to replace it with.
#[test]
fn a_wish_is_not_a_criterion() {
    assert_eq!(
        change_propose::observable_shape("make retries reliable"),
        Err(ShapeRefusal::NoOutcome),
        "a wish with no observable outcome must be refused, and say so"
    );
    assert!(
        change_propose::observable_shape(
            "two retry requests for the same delivery do not enqueue duplicate work"
        )
        .is_ok(),
        "the brief's replacement sentence must pass the same check"
    );
    // And each other refusal is reachable, so "no" always names a missing part.
    //
    // QUAL-F11-LAUNCH (D-050): this line used to assert that "the queue keeps
    // one entry" is refused for having no condition. That refusal is the
    // defect the path walk found — "The total shows the sum of the prices."
    // was never proposed although it is exactly what was asked. A sentence
    // with a subject and a stated result is proposed with no condition
    // invented; NoCondition stays reachable where the condition's boundary
    // cannot be found.
    assert!(change_propose::observable_shape("the queue keeps one entry").is_ok());
    assert_eq!(
        change_propose::observable_shape("When the queue fills the worker stops"),
        Err(ShapeRefusal::NoCondition)
    );
    assert_eq!(
        change_propose::observable_shape("When the delivery fails, it shows a row"),
        Err(ShapeRefusal::NoSubject),
        "a subject of nothing but stopwords is not a subject"
    );
    assert_eq!(
        change_propose::observable_shape(&"the queue keeps one entry when it fails ".repeat(20)),
        Err(ShapeRefusal::TooLong)
    );
}

/// The wish is in the fixture, and it is kept as a note with its reason — not
/// dropped, and not proposed.
#[test]
fn an_unrewritable_sentence_is_kept_as_a_note_with_its_reason() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let view = change_propose::propose(&ledger, CHANGE, &machine()).expect("propose");
    assert!(
        view.criteria
            .iter()
            .all(|c| !c.text.to_lowercase().contains("reliable")),
        "a wish must never become a criterion"
    );
    let note = view
        .notes
        .iter()
        .find(|n| n.text == "Make retries reliable")
        .expect("the wish is kept as a note");
    assert_eq!(note.refusal, ShapeRefusal::NoOutcome);
    assert_eq!(note.reason_slug, "note_no_outcome");
    assert!(
        thruline_core::voice::proposal_line(note.reason_slug).is_some(),
        "every note's reason has a sentence in the copy file"
    );
    assert_eq!(
        view.notes.len(),
        4,
        "four of the fixture's sentences cannot be cut into three parts: {:?}",
        view.notes.iter().map(|n| &n.text).collect::<Vec<_>>()
    );
}

/// Every proposal says where it came from and why, and the span it names points
/// at the words it was cut from.
#[test]
fn every_proposal_names_where_it_came_from_and_why() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let view = change_propose::propose(&ledger, CHANGE, &machine()).expect("propose");
    for criterion in &view.criteria {
        assert!(
            criterion.source.ask_id.starts_with("req_"),
            "a proposal must name the request it came from"
        );
        assert_eq!(criterion.reasoning_slug, "proposal_reasoning");
        assert!(
            thruline_core::voice::proposal_line(criterion.reasoning_slug).is_some(),
            "the reasoning is a copy-file line, not prose built at a surface"
        );
        assert!(criterion.source.length > 0, "a span of nothing is no span");
        match criterion.source.origin {
            change_propose::AskOrigin::ChangePurpose => {
                assert!(criterion.source.session_id.is_none());
                let quoted = &PURPOSE
                    [criterion.source.offset..criterion.source.offset + criterion.source.length];
                assert!(
                    criterion.text.starts_with(quoted),
                    "{quoted:?} is not the start of {:?}",
                    criterion.text
                );
            }
            change_propose::AskOrigin::SessionOpening => {
                assert_eq!(criterion.source.session_id.as_deref(), Some(CONFIRMED));
                assert!(criterion.source.event_id.is_some());
            }
            change_propose::AskOrigin::OriginalPrompt => {
                assert!(criterion.source.event_id.is_some());
            }
        }
    }
    // Both the request and the confirmed work contributed, so the proposer is
    // reading everything the card says it reads.
    let origins: BTreeSet<&str> = view
        .criteria
        .iter()
        .map(|c| c.source.origin.as_str())
        .collect();
    assert!(origins.contains("change_purpose"), "{origins:?}");
    assert!(origins.contains("session_opening"), "{origins:?}");
}

/// The original request travels with the proposals it produced (`B5.3`).
#[test]
fn the_request_is_retained_beside_what_it_produced() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let view = change_propose::propose(&ledger, CHANGE, &machine()).expect("propose");
    assert_eq!(view.request.title, "Retry failed uploads");
    assert_eq!(view.request.purpose, PURPOSE);
    assert_eq!(view.request.requester, "sam");
}

/// A scope version nobody agreed to is shown as not agreed.
///
/// The fixture's version 1 was written with no acceptance record — which is
/// exactly what T-2-1's `append_scope` allows and what a surface would most
/// easily draw as "what counts as done". It reads as missing here, and the
/// sentence that says so is served with it.
#[test]
fn a_list_nobody_agreed_to_is_shown_as_not_agreed() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let view = change_propose::propose(&ledger, CHANGE, &machine()).expect("propose");
    let accepted = view.accepted.as_ref().expect("a version exists");
    assert_eq!(accepted.version, 1);
    assert!(
        accepted.acceptance.is_none(),
        "nobody accepted version 1 in this fixture"
    );
    assert_eq!(view.not_accepted_slug, Some("accepted_scope_none"));

    change_propose::accept(&ledger, CHANGE, &accepting(Selection::All), &machine())
        .expect("accept");
    let after = change_propose::propose(&ledger, CHANGE, &machine()).expect("propose again");
    assert_eq!(after.not_accepted_slug, None, "now something is accepted");
}

/// A session nobody confirmed contributes nothing, however well its request is
/// written. `B3.12`: a suggestion is not a confirmation, and this is what that
/// costs — a perfectly checkable sentence left on the floor until a person says
/// it belongs.
#[test]
fn a_session_that_is_only_suggested_contributes_nothing() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let view = change_propose::propose(&ledger, CHANGE, &machine()).expect("propose");
    assert_eq!(view.confirmed_sessions, vec![CONFIRMED.to_string()]);
    for item in view
        .criteria
        .iter()
        .map(|c| c.text.clone())
        .chain(view.notes.iter().map(|n| n.text.clone()))
    {
        assert!(
            !item.to_lowercase().contains("dashboard"),
            "an unconfirmed session's words reached the proposals: {item:?}"
        );
    }
}

/// One sentence is said once, however many ways the record holds it.
///
/// Found by hand, not by reasoning: the commonest real configuration reads the
/// same text twice, because a Change opened from a session's first prompt points
/// at the very event that session opens with. The walkthrough printed every note
/// twice.
#[test]
fn the_same_sentence_read_twice_is_proposed_once() {
    let (_dir, ledger) = store();
    // A Change whose request event IS the confirmed session's opening request.
    change::insert_change(
        &ledger,
        &NewChange {
            id: "cccccccc-0000-4000-8000-00000000c003".to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-01-02T09:00:00Z".to_string(),
            request: ChangeRequest {
                title: "Retry failed uploads".to_string(),
                purpose: String::new(),
                prompt_event_id: Some(format!("cccccccc-ev-{CONFIRMED}-1")),
                requester: "sam".to_string(),
                source: RequestSource::Person,
            },
        },
    )
    .expect("insert the change");
    change_link::decide(
        &ledger,
        "cccccccc-0000-4000-8000-00000000c003",
        CONFIRMED,
        AssociationProvenance::Confirmed,
        "sam",
        "same work",
        "2026-01-02T09:10:00Z",
    )
    .expect("confirm");
    let view = change_propose::propose(&ledger, "cccccccc-0000-4000-8000-00000000c003", &machine())
        .expect("propose");
    let mut texts: Vec<String> = view
        .criteria
        .iter()
        .map(|c| c.text.clone())
        .chain(view.notes.iter().map(|n| n.text.clone()))
        .collect();
    let before = texts.len();
    texts.sort();
    texts.dedup();
    assert_eq!(
        before,
        texts.len(),
        "the same sentence was read twice and said twice: {texts:?}"
    );
    assert!(!view.criteria.is_empty(), "and it still proposes");
}

/// A sentence the masker cut into is never proposed, and the reason is said.
///
/// This is the one refusal here that is about provenance rather than shape, and
/// it was found by running the thing rather than by reasoning about it. Masking
/// runs before the cut, and a secret written just before a full stop takes the
/// full stop with it: the fixture below produced one welded "criterion" whose
/// subject was half of somebody's complaint and a mask marker. The marker is
/// built here by running the real masker, so if its shape ever changes this test
/// fails instead of the guard quietly stopping.
#[test]
fn a_sentence_the_masker_cut_into_is_never_proposed() {
    let (masked, _) = thruline_core::redact::mask_recorded(
        "The queue keeps one entry when AKIAIOSFODNN7EXAMPLE arrives",
    );
    assert_ne!(
        masked, "The queue keeps one entry when AKIAIOSFODNN7EXAMPLE arrives",
        "the fixture must actually be masked, or this test proves nothing"
    );
    assert_eq!(
        change_propose::observable_shape(&masked),
        Err(ShapeRefusal::Masked),
        "a sentence the masker cut into is not the person's sentence any more"
    );
    // The same sentence without the secret is a criterion, so the refusal is
    // about the mask and not about the shape.
    assert!(
        change_propose::observable_shape("The queue keeps one entry when a delivery arrives")
            .is_ok()
    );
}

// ---------------------------------------------------------------------------
// The check profile
// ---------------------------------------------------------------------------

/// One profile per kind of work, read off the paths (`B5.8`).
#[test]
fn the_check_profile_follows_the_kind_of_work() {
    let cases: &[(&str, ChangeType, &[&str])] = &[
        ("docs/guide.md", ChangeType::Documentation, &["megalinter"]),
        (
            "ui/src/panel.jsx",
            ChangeType::InterfaceBehaviour,
            &["knip", "playwright"],
        ),
        (
            "src/queue.rs",
            ChangeType::BackendBehaviour,
            &["cargo-test", "clippy"],
        ),
        (
            "src/auth/token.rs",
            ChangeType::AuthorizationSensitive,
            &["betterleaks", "semgrep"],
        ),
        (
            "migrations/003_add_column.sql",
            ChangeType::DataSchema,
            &["cargo-mutants", "cargo-test"],
        ),
    ];
    for (path, kind, expected) in cases {
        assert!(
            change_propose::types_of_path(path).contains(kind),
            "{path} should read as {kind:?}, got {:?}",
            change_propose::types_of_path(path)
        );
        let profile =
            change_propose::profile_for_paths(&[path.to_string()], &ProfileEnvironment::empty());
        let ids: Vec<&str> = profile.checks.iter().map(|c| c.id).collect();
        for id in *expected {
            assert!(
                ids.contains(id),
                "{path}: {expected:?} expected, got {ids:?}"
            );
        }
        for slug in &profile.change_type_slugs {
            assert!(
                thruline_core::voice::proposal_line(slug).is_some(),
                "{slug} has no sentence in the copy file"
            );
        }
    }
    // A path that is two kinds at once is both, and the profile carries both.
    let both = change_propose::types_of_path("src/auth/migrations/001_tokens.sql");
    assert!(both.contains(&ChangeType::DataSchema), "{both:?}");
    assert!(
        both.contains(&ChangeType::AuthorizationSensitive),
        "{both:?}"
    );
}

/// **`B5.10`.** A mandatory check whose tool is not on this machine stays on the
/// list, marked, and never disappears — a dropped check reads exactly like a
/// passed one.
#[test]
fn a_mandatory_check_that_cannot_run_here_is_still_listed() {
    let profile = change_propose::profile_for_paths(
        &["src/auth/token.rs".to_string()],
        &ProfileEnvironment::empty(),
    );
    let semgrep = profile
        .checks
        .iter()
        .find(|c| c.id == "semgrep")
        .expect("the mandatory check is listed even though the tool is absent");
    assert!(semgrep.mandatory);
    assert_eq!(semgrep.availability_slug, "check_not_available_here");
    assert_eq!(semgrep.availability_name, "semgrep");
    let (plain, _) = thruline_core::voice::proposal_sentence(
        semgrep.availability_slug,
        0,
        0,
        &semgrep.availability_name,
    )
    .expect("the sentence");
    assert_eq!(plain, "Not available here: semgrep.");

    // Installed but with no reviewed settings file is a third answer, not a
    // "ready": the baseline measured 3 tools present and 0 with a recipe.
    let installed = ProfileEnvironment {
        tools_present: ["semgrep".to_string()].into_iter().collect(),
        recipes_present: BTreeSet::new(),
    };
    let profile = change_propose::profile_for_paths(&["src/auth/token.rs".to_string()], &installed);
    let semgrep = profile
        .checks
        .iter()
        .find(|c| c.id == "semgrep")
        .expect("still listed");
    assert_eq!(semgrep.availability_slug, "check_needs_a_recipe");
    assert_eq!(semgrep.availability_name, ".thruline/semgrep.json");

    // And with both, it is ready to run — which is still not a result.
    let ready = ProfileEnvironment {
        tools_present: ["semgrep".to_string()].into_iter().collect(),
        recipes_present: ["semgrep".to_string()].into_iter().collect(),
    };
    let profile = change_propose::profile_for_paths(&["src/auth/token.rs".to_string()], &ready);
    let semgrep = profile
        .checks
        .iter()
        .find(|c| c.id == "semgrep")
        .expect("still listed");
    assert_eq!(semgrep.availability_slug, "check_ready");
}

/// A profile is not evidence, and every reading says so.
#[test]
fn a_profile_is_never_evidence() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let view = change_propose::propose(&ledger, CHANGE, &machine()).expect("propose");
    assert_eq!(view.profile.not_evidence_slug, "profile_is_not_evidence");
    let (plain, _) =
        thruline_core::voice::proposal_sentence("profile_is_not_evidence", 0, 0, "").expect("line");
    assert!(
        plain.contains("still needs evidence"),
        "the sentence must say what state a suggested check leaves things in: {plain}"
    );
    // The profile carries no outcome field at all, which is the guarantee the
    // sentence rests on: there is nowhere for a result to be put.
    let encoded = serde_json::to_string(&view.profile).expect("encode");
    for forbidden in ["\"result\"", "\"passed\"", "\"outcome\"", "\"verdict\""] {
        assert!(
            !encoded.contains(forbidden),
            "{forbidden} in a suggested profile"
        );
    }
}

// ---------------------------------------------------------------------------
// The acceptance act
// ---------------------------------------------------------------------------

/// Accepting writes one version and one record with actor, time, version and
/// rationale (`B5.6`) — and touches no decision (`B5.7`).
#[test]
fn accepting_writes_a_version_and_a_record_and_approves_nothing() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let before = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    let outcome = change_propose::accept(&ledger, CHANGE, &accepting(Selection::All), &machine())
        .expect("accept");
    assert_eq!(outcome.version, 2, "the first scope revision was version 1");
    assert_eq!(outcome.actor, "sam");
    assert_eq!(outcome.accepted_at, "2026-01-02T11:00:00Z");
    assert_eq!(outcome.rationale, "this is what I want checked");
    assert_eq!(outcome.sentence_slug, "accept_written");
    assert_eq!(outcome.not_an_approval_slug, "accept_is_not_approval");

    let after = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    let scope = after.accepted_scope.as_ref().expect("a standing scope");
    assert_eq!(scope.version, 2);
    assert_eq!(
        scope.criteria.len(),
        outcome.criteria.len(),
        "the stored list is the accepted list"
    );
    let acceptance = scope.acceptance.as_ref().expect("an acceptance record");
    assert_eq!(acceptance.actor, "sam");
    assert_eq!(acceptance.accepted_at, "2026-01-02T11:00:00Z");
    assert_eq!(acceptance.rationale, "this is what I want checked");

    // Accepting is not approving. The decision stage is untouched, and there is
    // no row anywhere for it to have been written to.
    assert_eq!(before.decision, after.decision);
    assert_eq!(after.decision, change::NONE_YET);
    assert_eq!(after.examination, change::NONE_YET);
    let mut statement = ledger
        .conn()
        .prepare(
            "SELECT name FROM sqlite_master WHERE type='table'
               AND (name LIKE '%decision%' OR name LIKE '%approval%') ORDER BY name",
        )
        .expect("decision-shaped tables");
    let decision_tables = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("list decision-shaped tables")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("read decision-shaped tables");
    // Scope acceptance must not make an entity decision, approve a Change, or
    // create a standing Decision. Name every table so a later decision surface
    // requires this separation to be reviewed again.
    let mut expected_tables = vec![
        "entity_association_decisions",
        "change_decisions",
        "change_decision_exceptions",
        // C30 R-LEDGER: a second person's name beside an approval. Accepting a
        // scope is not an approval, so it can have no second.
        "change_decision_seconds",
        "decisions",
        "decision_revisions",
        "decision_acceptances",
        "decision_retirements",
        "decision_verdicts",
        "decision_waivers",
        "decision_person_acts",
    ];
    expected_tables.sort();
    assert_eq!(decision_tables, expected_tables);
    let approvals: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM change_decisions", [], |row| {
            row.get(0)
        })
        .expect("count approvals");
    assert_eq!(approvals, 0, "accepting a scope approved something");
    let exceptions: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM change_decision_exceptions",
            [],
            |row| row.get(0),
        )
        .expect("count exceptions");
    assert_eq!(exceptions, 0, "accepting a scope set something aside");
    let seconds: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM change_decision_seconds", [], |row| {
            row.get(0)
        })
        .expect("count seconds");
    assert_eq!(
        seconds, 0,
        "accepting a scope put a second name on something"
    );
    let entity_decisions: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM entity_association_decisions",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(entity_decisions, 0, "accepting wrote a decision row");
    for table in [
        "decisions",
        "decision_revisions",
        "decision_acceptances",
        "decision_retirements",
        "decision_verdicts",
        "decision_waivers",
        "decision_person_acts",
    ] {
        let rows: i64 = ledger
            .conn()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("count standing Decision rows");
        assert_eq!(
            rows, 0,
            "accepting a scope wrote a standing Decision row to {table}"
        );
    }

    // The earlier version is still there. `B6.13` needs it to be.
    assert_eq!(after.scope_history.len(), 1);
    assert_eq!(after.scope_history[0].version, 1);
}

/// What the acceptance serves back, pinned.
#[test]
fn what_an_acceptance_answers_with_is_served_whole() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let outcome = change_propose::accept(&ledger, CHANGE, &accepting(Selection::All), &machine())
        .expect("accept");
    insta::assert_json_snapshot!("acceptance_outcome", outcome);
}

/// No name, no acceptance. `B5.6` wants an actor and a store cannot invent one.
#[test]
fn accepting_without_a_name_is_refused() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let mut nameless = accepting(Selection::All);
    nameless.actor = "   ".to_string();
    let refused = change_propose::accept(&ledger, CHANGE, &nameless, &machine())
        .expect_err("a nameless acceptance must be refused");
    let (expected, _) =
        thruline_core::voice::proposal_sentence("accept_needs_actor", 0, 0, "").expect("line");
    assert!(
        refused.to_string().contains(&expected),
        "the refusal must be the copy file's sentence, got {refused}"
    );
    // And nothing was written.
    assert_eq!(
        change_propose::propose(&ledger, CHANGE, &machine())
            .expect("propose")
            .versions_used,
        1
    );
}

/// **The `B.9` walkthrough.** Every sentence a wish: nothing proposed, the notes
/// shown, and accepting an empty set refused — an empty accepted scope is not a
/// scope.
#[test]
fn nothing_to_propose_is_an_answer_and_an_empty_scope_is_refused() {
    let (_dir, ledger) = store();
    a_barren_change(&ledger);
    let view = change_propose::propose(&ledger, BARREN, &machine()).expect("propose");
    assert!(view.criteria.is_empty(), "{:?}", view.criteria);
    assert_eq!(view.empty_slug, Some("proposal_none"));
    assert!(
        !view.notes.is_empty(),
        "the sentences are kept as notes, not dropped"
    );
    assert_eq!(view.not_accepted_slug, Some("accepted_scope_none"));
    assert!(view.accepted.is_none(), "nothing is accepted yet");

    let refused = change_propose::accept(&ledger, BARREN, &accepting(Selection::All), &machine())
        .expect_err("an empty list must not be accepted");
    let (expected, _) =
        thruline_core::voice::proposal_sentence("accept_needs_a_criterion", 0, 0, "")
            .expect("line");
    assert!(refused.to_string().contains(&expected), "{refused}");
    let versions: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM change_scope_revisions WHERE change_id=?1",
            params![BARREN],
            |row| row.get(0),
        )
        .expect("count");
    assert_eq!(versions, 0, "a refused acceptance wrote a version");
}

/// A number nobody proposed is refused rather than skipped, and so is a rewrite
/// of a line that was not chosen — both would silently save a different list
/// from the one that was asked for.
#[test]
fn a_selection_the_proposals_do_not_contain_is_refused() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let refused = change_propose::accept(
        &ledger,
        CHANGE,
        &accepting(Selection::Numbers(vec![99])),
        &machine(),
    )
    .expect_err("a line nobody proposed cannot be accepted");
    assert!(refused.to_string().contains("99"), "{refused}");

    let mut stray = accepting(Selection::Numbers(vec![1]));
    stray.edits.insert(2, "something else entirely".to_string());
    let refused = change_propose::accept(&ledger, CHANGE, &stray, &machine())
        .expect_err("an edit of an unchosen line cannot be silently dropped");
    assert!(refused.to_string().contains('2'), "{refused}");
}

/// Accepting only part of the set accepts only that part, and carries forward
/// everything the previous version said about scope, exclusions and constraints.
#[test]
fn accepting_part_of_the_set_keeps_what_the_last_version_already_said() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let outcome = change_propose::accept(
        &ledger,
        CHANGE,
        &accepting(Selection::Numbers(vec![1])),
        &machine(),
    )
    .expect("accept one");
    assert_eq!(outcome.criteria.len(), 1);
    let view = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    let scope = view.accepted_scope.as_ref().expect("standing");
    assert_eq!(scope.exclusions, vec!["anything about billing".to_string()]);
    assert_eq!(scope.allowed_scope, vec!["src/upload/**".to_string()]);
    assert_eq!(scope.constraints, vec!["no new dependencies".to_string()]);
    assert!(
        scope.check_profile.contains(&"cargo-test".to_string()),
        "the suggested profile is part of what was accepted: {:?}",
        scope.check_profile
    );
}

/// Accepting the same list twice is refused: a version that changes nothing
/// spends one of the bounded slots and makes the history harder to read.
#[test]
fn accepting_the_same_list_again_is_refused() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    change_propose::accept(&ledger, CHANGE, &accepting(Selection::All), &machine())
        .expect("accept once");
    let refused = change_propose::accept(&ledger, CHANGE, &accepting(Selection::All), &machine())
        .expect_err("the same list twice");
    assert!(refused.to_string().contains("version 2"), "{refused}");
}

/// The version series is bounded, the bound is shown on every reading, and the
/// refusal at the bound says what to do instead.
#[test]
fn the_versions_are_bounded_and_the_bound_is_shown_before_it_bites() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let view = change_propose::propose(&ledger, CHANGE, &machine()).expect("propose");
    assert_eq!(view.versions_used, 1);
    assert_eq!(view.versions_allowed, MAX_SCOPE_VERSIONS);
    assert_eq!(view.versions_slug, "scope_versions_bounded");

    // Fill the series with distinct versions, then ask for one more.
    for index in 2..=MAX_SCOPE_VERSIONS {
        change::append_scope(
            &ledger,
            CHANGE,
            "2026-01-02T12:00:00Z",
            &AcceptedScope {
                criteria: vec![format!("filler version {index}")],
                ..AcceptedScope::default()
            },
        )
        .expect("append filler");
    }
    let refused = change_propose::accept(&ledger, CHANGE, &accepting(Selection::All), &machine())
        .expect_err("the bound must refuse");
    assert!(
        refused
            .to_string()
            .contains(&MAX_SCOPE_VERSIONS.to_string()),
        "the refusal names the bound: {refused}"
    );
    assert!(
        refused.to_string().to_lowercase().contains("new change"),
        "the refusal says what to do instead: {refused}"
    );
}

// ---------------------------------------------------------------------------
// Never silently weaken
// ---------------------------------------------------------------------------

/// Reading the record again never changes what was accepted.
#[test]
fn reading_again_changes_nothing_that_was_accepted() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    change_propose::accept(&ledger, CHANGE, &accepting(Selection::All), &machine())
        .expect("accept");
    let digest_before: String = ledger
        .conn()
        .query_row(
            "SELECT group_concat(content_sha256) FROM change_scope_revisions
              WHERE change_id=?1 ORDER BY scope_version",
            params![CHANGE],
            |row| row.get(0),
        )
        .expect("digest");
    for _ in 0..3 {
        change_propose::propose(&ledger, CHANGE, &machine()).expect("propose again");
    }
    let digest_after: String = ledger
        .conn()
        .query_row(
            "SELECT group_concat(content_sha256) FROM change_scope_revisions
              WHERE change_id=?1 ORDER BY scope_version",
            params![CHANGE],
            |row| row.get(0),
        )
        .expect("digest");
    assert_eq!(
        digest_before, digest_after,
        "a reading changed the accepted versions"
    );
}

/// **`B5.5`.** A second round that says less is named as saying less, and
/// changes nothing.
#[test]
fn a_weaker_restatement_is_named_and_not_applied() {
    let accepted = vec![
        "Two retry requests for the same delivery do not enqueue duplicate work.".to_string(),
        "When an upload fails, the queue keeps one entry for it.".to_string(),
    ];
    let proposed = vec![
        proposal(
            1,
            "Two retry requests for the same delivery enqueue duplicate work.",
        ),
        proposal(2, "The dashboard shows a banner when the queue is empty."),
    ];
    let diff = change_propose::diff_against_accepted(2, &accepted, &proposed);

    assert_eq!(diff.weakened.len(), 1, "{:?}", diff.weakened);
    let weakened = &diff.weakened[0];
    assert_eq!(weakened.accepted, accepted[0]);
    assert_eq!(weakened.proposed_number, Some(1));
    assert_eq!(weakened.sentence_slug, "reproposal_weakened");

    assert_eq!(diff.removed.len(), 1, "{:?}", diff.removed);
    assert_eq!(diff.removed[0].accepted, accepted[1]);
    assert_eq!(diff.removed[0].sentence_slug, "reproposal_removed");

    assert_eq!(diff.added, vec![2], "the new line is named as new");
    assert_eq!(diff.nothing_changed_slug, "reproposal_changes_nothing");

    // The second weakening test: saying strictly less about the same subject.
    let quieter = vec![proposal(
        1,
        "Two retry requests for the same delivery do not enqueue.",
    )];
    let diff = change_propose::diff_against_accepted(2, &accepted[..1], &quieter);
    assert_eq!(diff.weakened.len(), 1, "{diff:?}");
    assert!(diff.restated.is_empty(), "{diff:?}");

    // A different wording that is neither stronger nor weaker is a restatement,
    // named as one rather than hidden or called a weakening.
    let reworded = vec![proposal(
        1,
        "Two retry requests for one delivery do not enqueue a second unit of duplicate work.",
    )];
    let diff = change_propose::diff_against_accepted(2, &accepted[..1], &reworded);
    assert!(diff.weakened.is_empty(), "{diff:?}");
    assert_eq!(diff.restated.len(), 1, "{diff:?}");
    assert_eq!(diff.restated[0].sentence_slug, "reproposal_restated");
}

/// A `ProposedCriterion` built from one sentence, for the diff tests.
fn proposal(number: u32, text: &str) -> change_propose::ProposedCriterion {
    let shape = change_propose::observable_shape(text).expect("the fixture sentence is observable");
    change_propose::ProposedCriterion {
        number,
        text: change_propose::criterion_text(text),
        subject_key: shape.subject_key(),
        shape,
        source: change_propose::AskSource {
            ask_id: "req_test".to_string(),
            origin: change_propose::AskOrigin::ChangePurpose,
            session_id: None,
            event_id: None,
            offset: 0,
            length: text.len(),
        },
        reasoning_slug: "proposal_reasoning",
    }
}

/// The diff is shown beside a real accepted version, from the store.
#[test]
fn a_second_round_is_shown_beside_the_accepted_version() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    change_propose::accept(&ledger, CHANGE, &accepting(Selection::All), &machine())
        .expect("accept");
    let view = change_propose::propose(&ledger, CHANGE, &machine()).expect("propose again");
    let diff = view.diff.as_ref().expect("a diff once there is a version");
    assert_eq!(diff.accepted_version, 2);
    assert!(
        diff.weakened.is_empty() && diff.removed.is_empty(),
        "an unchanged store cannot weaken anything: {diff:?}"
    );
    let accepted = view.accepted.as_ref().expect("an accepted echo");
    assert_eq!(accepted.version, 2);
    assert!(accepted
        .criteria
        .iter()
        .all(|c| c.attribution == Attribution::AsProposed));
}

/// **`B5.5` inside the act.** Accepting a list that drops or softens a line
/// already accepted is refused, and named — and goes through when the person
/// says that is what they mean.
///
/// Found by hand: accept one rewritten line, then `--all` again, and the second
/// act put the tool's wording back over the person's without a word. The
/// `accept_nothing_new` guard did not catch it, because the list was not the
/// same — it was worse.
#[test]
fn accepting_a_list_that_drops_or_softens_an_accepted_line_is_refused() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let mut edited = accepting(Selection::All);
    edited.edits.insert(
        2,
        "Two retry requests for the same delivery do not enqueue duplicate work, not even once."
            .to_string(),
    );
    change_propose::accept(&ledger, CHANGE, &edited, &machine()).expect("accept the rewrite");

    // The whole proposed set again: the rewritten line would be replaced by the
    // shorter proposal, which says strictly less.
    let refused = change_propose::accept(&ledger, CHANGE, &accepting(Selection::All), &machine())
        .expect_err("a silent replacement must be refused");
    assert!(
        refused.to_string().contains("Nothing was saved"),
        "{refused}"
    );

    // Dropping a line is the same refusal.
    let refused = change_propose::accept(
        &ledger,
        CHANGE,
        &accepting(Selection::Numbers(vec![1])),
        &machine(),
    )
    .expect_err("dropping accepted lines must be refused");
    assert!(
        refused.to_string().contains("Nothing was saved"),
        "{refused}"
    );

    // And when the person says so, it goes through — as an explicit act.
    let mut replacing = accepting(Selection::Numbers(vec![1]));
    replacing.replacing = true;
    let outcome = change_propose::accept(&ledger, CHANGE, &replacing, &machine())
        .expect("say so and it does");
    assert_eq!(outcome.criteria.len(), 1);
    assert_eq!(outcome.version, 3);
}

/// **Which of the two refusals the test above actually reaches.**
///
/// Written because a verifier disabled the weakening branch of `accept` and this
/// file stayed green: every scenario above also rewrites a line into the
/// person's own words, so the *authorship* refusal fires first and the weakening
/// branch is never reached. Nothing was wrong with the guard; the claim that
/// these scenarios exercised it was wrong. This test pins what they do exercise,
/// so the two can never be confused again, and the three tests after it exercise
/// the weakening branch with no rewritten line anywhere in the store.
#[test]
fn the_refusal_above_is_the_authorship_one_not_the_weakening_one() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let mut edited = accepting(Selection::All);
    edited.edits.insert(
        2,
        "Two retry requests for the same delivery do not enqueue duplicate work, not even once."
            .to_string(),
    );
    change_propose::accept(&ledger, CHANGE, &edited, &machine()).expect("accept the rewrite");
    let refused = change_propose::accept(&ledger, CHANGE, &accepting(Selection::All), &machine())
        .expect_err("refused");
    assert!(
        refused.to_string().contains("put our wording back over"),
        "this scenario reaches the authorship refusal, not the weakening one: {refused}"
    );
}

/// **The weakening branch of the acceptance act, on its own.** Dropping a line
/// you accepted *word for word as we proposed it* is refused too.
///
/// No line in this store is ever rewritten, so every accepted line is "in the
/// words we proposed" and the authorship refusal has nothing to count. The only
/// thing that can refuse here is the comparison against the standing version,
/// and the sentence proves which one answered.
#[test]
fn dropping_a_line_you_accepted_word_for_word_is_refused() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let first = change_propose::accept(&ledger, CHANGE, &accepting(Selection::All), &machine())
        .expect("accept the whole set unchanged");
    assert_eq!(first.criteria.len(), 3);
    assert!(
        first
            .criteria
            .iter()
            .all(|c| c.attribution == Attribution::AsProposed),
        "the premise of this test is that nothing here is in the person's words: {:?}",
        first.criteria
    );

    let refused = change_propose::accept(
        &ledger,
        CHANGE,
        &accepting(Selection::Numbers(vec![1, 2])),
        &machine(),
    )
    .expect_err("dropping an accepted line must be refused");
    assert!(
        refused.to_string().contains("you already accepted"),
        "the weakening refusal must be the one that answers: {refused}"
    );
    assert!(
        !refused.to_string().contains("put our wording back over"),
        "the authorship refusal cannot be what answers here: {refused}"
    );
    assert!(
        refused.to_string().contains("Nothing was saved"),
        "{refused}"
    );

    // Nothing was written: the standing version still holds all three lines.
    let view = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    let standing = view.accepted_scope.as_ref().expect("standing");
    assert_eq!(standing.version, 2);
    assert_eq!(standing.criteria.len(), 3);

    // And when the person says plainly that they mean it, it goes through.
    let mut replacing = accepting(Selection::Numbers(vec![1, 2]));
    replacing.replacing = true;
    let outcome = change_propose::accept(&ledger, CHANGE, &replacing, &machine())
        .expect("say so and it does");
    assert_eq!(outcome.criteria.len(), 2);
    assert_eq!(outcome.version, 3);
}

/// **The other half of the same branch: says less, rather than gone.** An edit
/// whose every word is already in the accepted line, and which has fewer of
/// them, is a softening — refused even though the accepted line is in our own
/// proposed words and nobody's authorship is at stake.
#[test]
fn softening_a_line_you_accepted_word_for_word_is_refused() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    change_propose::accept(&ledger, CHANGE, &accepting(Selection::All), &machine())
        .expect("accept the whole set unchanged");

    let mut softened = accepting(Selection::All);
    softened.edits.insert(
        2,
        "Two retry requests for the same delivery do not enqueue work.".to_string(),
    );
    let refused = change_propose::accept(&ledger, CHANGE, &softened, &machine())
        .expect_err("a softer restatement must be refused");
    assert!(
        refused.to_string().contains("says less"),
        "the weakening refusal must be the one that answers: {refused}"
    );
    assert!(
        !refused.to_string().contains("put our wording back over"),
        "the authorship refusal cannot be what answers here: {refused}"
    );

    let view = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    assert!(
        view.accepted_scope
            .as_ref()
            .expect("standing")
            .criteria
            .iter()
            .any(|c| c.contains("duplicate work")),
        "the softer line was written anyway"
    );
}

/// **The dropped negation, at acceptance time.** "do not enqueue duplicate work"
/// rewritten without the "not" is longer, not shorter, so the subset test cannot
/// see it. The negation test can, and this is the acceptance act's use of it.
#[test]
fn dropping_the_not_from_a_line_you_accepted_word_for_word_is_refused() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    change_propose::accept(&ledger, CHANGE, &accepting(Selection::All), &machine())
        .expect("accept the whole set unchanged");

    let mut reversed = accepting(Selection::All);
    reversed.edits.insert(
        2,
        "Two retry requests for the same delivery enqueue one unit of duplicate work.".to_string(),
    );
    let refused = change_propose::accept(&ledger, CHANGE, &reversed, &machine())
        .expect_err("a reversed requirement must be refused");
    assert!(
        refused.to_string().contains("says less"),
        "the weakening refusal must be the one that answers: {refused}"
    );
    assert!(
        !refused.to_string().contains("put our wording back over"),
        "the authorship refusal cannot be what answers here: {refused}"
    );
}

/// **The second hole the walkthrough found.** Our wording may not go back over a
/// line the person wrote, even when it is neither weaker nor shorter.
///
/// "the record shows exactly one failure row" replaced by "the record shows a
/// single failure row" passes both weakening tests — it is simply our words over
/// theirs, which is what `B5.5` is actually about. Caught by authorship rather
/// than by strength.
#[test]
fn our_wording_does_not_go_back_over_a_line_you_wrote() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let mut edited = accepting(Selection::All);
    edited.edits.insert(
        3,
        "When a delivery fails three times, the record shows exactly one failure row.".to_string(),
    );
    change_propose::accept(&ledger, CHANGE, &edited, &machine()).expect("accept the rewrite");

    let refused = change_propose::accept(&ledger, CHANGE, &accepting(Selection::All), &machine())
        .expect_err("our wording must not replace theirs quietly");
    assert!(
        refused.to_string().contains("put our wording back over"),
        "{refused}"
    );
    assert!(
        refused.to_string().contains("Nothing was saved"),
        "{refused}"
    );

    // The person's line is still what counts as done.
    let view = change::read_change(&ledger, CHANGE)
        .expect("read")
        .expect("exists");
    assert!(
        view.accepted_scope
            .as_ref()
            .expect("standing")
            .criteria
            .iter()
            .any(|c| c.contains("exactly one failure row")),
        "the rewrite was replaced anyway"
    );

    // Saying so plainly is allowed, as an explicit act.
    let mut replacing = accepting(Selection::All);
    replacing.replacing = true;
    change_propose::accept(&ledger, CHANGE, &replacing, &machine()).expect("say so and it does");
}

// ---------------------------------------------------------------------------
// Whose words
// ---------------------------------------------------------------------------

/// **`B5.5`, the other half.** A line the person rewrote before accepting is
/// attributed to the person, never to the tool, and the proposal it replaced is
/// still derivable from the record.
#[test]
fn a_line_you_rewrote_before_accepting_is_attributed_to_you() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let original = change_propose::propose(&ledger, CHANGE, &machine())
        .expect("propose")
        .criteria[0]
        .text
        .clone();
    let mut edited = accepting(Selection::Numbers(vec![1]));
    edited.edits.insert(
        1,
        "When an upload fails, the queue keeps exactly one entry for it.".to_string(),
    );
    let outcome = change_propose::accept(&ledger, CHANGE, &edited, &machine()).expect("accept");
    assert_eq!(outcome.criteria.len(), 1);
    assert_eq!(outcome.criteria[0].attribution, Attribution::YourWords);
    assert_eq!(outcome.criteria[0].attribution_slug, "criterion_your_words");
    assert_eq!(
        outcome.criteria[0].text,
        "When an upload fails, the queue keeps exactly one entry for it."
    );
    assert!(
        outcome.criteria[0].not_observable_slug.is_none(),
        "the rewrite is still checkable"
    );

    // The original proposal is not gone: the store still derives it, so "what
    // did the tool propose and what did the person accept" are two readable
    // facts.
    let after = change_propose::propose(&ledger, CHANGE, &machine()).expect("propose again");
    assert!(
        after.criteria.iter().any(|c| c.text == original),
        "the proposal the person replaced is no longer derivable"
    );
    let accepted = after.accepted.as_ref().expect("an accepted echo");
    assert_eq!(accepted.criteria[0].attribution, Attribution::YourWords);
}

/// A rewrite that cannot be cut into three parts is kept exactly as written and
/// marked — never refused, and never quietly dropped.
#[test]
fn a_rewrite_nobody_could_check_is_kept_and_marked() {
    let (_dir, ledger) = store();
    a_change(&ledger);
    let mut edited = accepting(Selection::Numbers(vec![1]));
    edited.edits.insert(1, "make retries reliable".to_string());
    let outcome = change_propose::accept(&ledger, CHANGE, &edited, &machine()).expect("accept");
    assert_eq!(outcome.criteria[0].text, "make retries reliable");
    assert_eq!(
        outcome.criteria[0].not_observable_slug,
        Some("criterion_not_observable_as_written")
    );
    assert_eq!(outcome.criteria[0].attribution, Attribution::YourWords);
}

// ---------------------------------------------------------------------------
// The vocabulary tables
// ---------------------------------------------------------------------------

/// The three tables the shape check rests on are closed, sorted and free of
/// duplicates.
///
/// Pinned for the same reason `EDIT_TOOLS` is: the sets are claims about which
/// sentences this product is willing to call checkable, and a silent addition
/// widens the claim without anybody deciding to.
#[test]
fn the_shape_vocabulary_is_closed_and_ordered() {
    for (name, table) in [
        ("outcomes", change_propose::OUTCOME_MARKERS),
        ("negations", change_propose::NEGATION_MARKERS),
        ("conditions", change_propose::CONDITION_MARKERS),
    ] {
        let mut sorted = table.to_vec();
        sorted.sort_unstable();
        assert_eq!(table, sorted.as_slice(), "{name} is not in order");
        let unique: BTreeSet<&&str> = table.iter().collect();
        assert_eq!(unique.len(), table.len(), "{name} has a duplicate");
        assert!(
            table.iter().all(|entry| entry
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == ' ' || c == '\'')),
            "{name} has an entry that will never match a lowercased word"
        );
    }
    assert!(change_propose::OUTCOME_MARKERS.contains(&"enqueues"));
    assert!(!change_propose::OUTCOME_MARKERS.contains(&"reliable"));
}

// ---------------------------------------------------------------------------
// T-2-9 / T-2-8-F1 — whole sentences, or nothing
// ---------------------------------------------------------------------------

const F1_CHANGE: &str = "cccccccc-0000-4000-8000-00000000c009";

/// A real agent prompt: hard-wrapped, with a path, a version number, an
/// abbreviation and backticked code in it.
///
/// Shaped from the prompt `T-2-8` actually sent to Claude Code, because a
/// hand-tidied one is the fixture that let this ship: every request in this
/// file before today was one line per sentence with no dots inside its words.
const F1_ASK: &str = "Implement the retry flow in this outbox service.

`Outbox::request_retry` in src/lib.rs currently returns OutboxError::RetryNotImplemented. Write it:
asking for a delivery to be tried again enqueues one job in the `retry_jobs` table, and asking
twice for the same delivery while a job is still pending must NOT enqueue a second job.

When the service is upgraded from v1.2.3, a delivery that is already sent must not be retried; e.g.
the caller gets an error rather than silence.";

fn an_f1_change(ledger: &Ledger) {
    change::insert_change(
        ledger,
        &NewChange {
            id: F1_CHANGE.to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-01-04T09:00:00Z".to_string(),
            request: ChangeRequest {
                title: "Implement the retry flow in this outbox service".to_string(),
                purpose: F1_ASK.to_string(),
                prompt_event_id: None,
                requester: "sam".to_string(),
                source: RequestSource::Person,
            },
        },
    )
    .expect("insert the change");
}

/// Nothing offered as something that counts as done is a piece of a sentence.
///
/// `T-2-8-F1`: on the first-use path, against this shape of prompt, the
/// product proposed "asking for a delivery to be tried again enqueues one job
/// in the `retry_jobs` table, and asking." for one-click acceptance, and kept
/// "rs currently returns OutboxError::RetryNotImplemented" as a note. Both are
/// halves of somebody's sentence, cut at a soft wrap and at the dot inside
/// `src/lib.rs`.
#[test]
fn a_real_agent_prompt_yields_whole_sentences_only() {
    let (_dir, ledger) = store();
    an_f1_change(&ledger);
    let view = change_propose::propose(&ledger, F1_CHANGE, &machine()).expect("propose");
    for criterion in &view.criteria {
        let first = criterion.text.chars().next().expect("a criterion has text");
        assert!(
            first.is_uppercase() || first.is_numeric(),
            "a criterion that starts mid-sentence: {:?}",
            criterion.text
        );
        assert!(
            criterion.text.ends_with('.'),
            "a criterion that trails off: {:?}",
            criterion.text
        );
    }

    // The three dots that are not full stops, each checked where it lives.
    let everything: Vec<&str> = view
        .criteria
        .iter()
        .map(|c| c.text.as_str())
        .chain(view.notes.iter().map(|n| n.text.as_str()))
        .collect();
    for text in &everything {
        assert!(
            !text.ends_with("in src/lib"),
            "the dot in a path was read as a full stop: {text:?}"
        );
        assert!(
            !text.starts_with("rs currently"),
            "the other half of the same cut: {text:?}"
        );
        assert!(
            !text.ends_with("from v1"),
            "the dot in a version number was read as a full stop: {text:?}"
        );
    }
    assert!(
        everything.iter().any(|t| t.contains("src/lib.rs")),
        "the path survived whole: {everything:?}"
    );
    assert!(
        everything.iter().any(|t| t.contains("v1.2.3")),
        "the version survived whole: {everything:?}"
    );
    assert!(
        everything
            .iter()
            .any(|t| t.contains("a second job") && t.contains("retry_jobs")),
        "the soft-wrapped sentence arrived in one piece: {everything:?}"
    );

    // QUAL-F11-LAUNCH (D-050): this block used to require the piece after
    // "; e.g." to be a fragment note. That piece existed only because the
    // splitter cut at the semicolon, which is the defect. The sentence around
    // `e.g.` now arrives whole, is kept as a note with its reason, and is never
    // proposed; nothing starts with "e.g.". The fragment path itself is pinned
    // by `a_piece_of_a_sentence_is_kept_as_a_note` in
    // `crates/core/tests/change_propose_qual_f11.rs`.
    assert!(
        !everything.iter().any(|t| t.starts_with("e.g.")),
        "the semicolon before `e.g.` was read as the end of a sentence: {everything:?}"
    );
    let whole = view
        .notes
        .iter()
        .find(|n| n.text.contains("must not be retried; e.g."))
        .expect("the sentence around `e.g.` is kept as a note, in one piece");
    assert!(
        thruline_core::voice::proposal_line(whole.reason_slug).is_some(),
        "the reason is a copy-file line, not prose built at a surface"
    );
}

/// The same store, twice: the splitter is a function of the characters and
/// nothing else.
#[test]
fn the_real_prompt_cuts_the_same_way_twice() {
    let (_dir, ledger) = store();
    an_f1_change(&ledger);
    let first = change_propose::propose(&ledger, F1_CHANGE, &machine()).expect("propose");
    let second = change_propose::propose(&ledger, F1_CHANGE, &machine()).expect("propose");
    assert_eq!(
        serde_json::to_string(&first).expect("serialize"),
        serde_json::to_string(&second).expect("serialize")
    );
}
