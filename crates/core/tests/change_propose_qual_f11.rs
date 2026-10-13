//! QUAL-F11-LAUNCH — the proposal keeps every sentence of the ask, including
//! one joined by a semicolon (path step 2, PW-F1; rehearsal QD-1..9).
//!
//! Two asks stopped at the propose step:
//!
//! * the qualification Change's purpose (the Q02 case, step 12), whose third
//!   sentence — "A delivery that has exhausted its attempts is shown as
//!   failed; the service saying 'sent' does not make it sent." — was cut at
//!   its semicolon and never proposed;
//! * the path walk's two-sentence ask, whose "The total shows the sum of the
//!   prices." was kept as a note because it states no condition, although it
//!   is exactly what the person asked to be true.
//!
//! What these tests hold: the frozen sentence is proposed whole; both of the
//! walk's behaviour sentences are proposed; nothing proposed is anything but a
//! sentence of the ask; nothing of the ask disappears without a note saying so;
//! and a longer list of criteria can only make "Ready" harder, never easier.
//!
//! Every id is synthetic (`cccccccc-…`) and no fixture here holds private
//! content.

use rusqlite::params;
use thruline_core::change::{self, ChangeRequest, NewChange, RequestSource};
use thruline_core::change_evidence::{
    coverage as derived_coverage, derive_links, CandidateView, CoverageState, CoverageView,
    CriterionCoverage, EvidenceItemView, EvidenceKind, LinkBasis, Observed, RunClass,
};
use thruline_core::change_propose::{self, ProfileEnvironment, ProposalView, ShapeRefusal};
use thruline_core::change_readiness::{compute, ReadinessInput};
use thruline_core::ledger::Ledger;
use thruline_core::run_execution::ExecutionState;

const PROJECT: &str = "cccccccc-0000-4000-8000-00000000f011";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000cf11";

/// The Q02 case's purpose, step 12, verbatim. Pinned against its three
/// sentences by `the_purpose_is_exactly_its_three_sentences`, so the two cannot
/// drift.
const Q02_PURPOSE: &str = "Two retry requests for the same delivery do not enqueue duplicate work. A delivery that was still waiting when the service was stopped is still exactly one delivery after it starts again, and it is attempted again. A delivery that has exhausted its attempts is shown as failed; the service saying 'sent' does not make it sent.";

/// The three sentences the harness binds as C1, C2 and C3, as it spells them.
const Q02_SENTENCES: [&str; 3] = [
    "Two retry requests for the same delivery do not enqueue duplicate work.",
    "A delivery that was still waiting when the service was stopped is still exactly one delivery after it starts again, and it is attempted again.",
    "A delivery that has exhausted its attempts is shown as failed; the service saying 'sent' does not make it sent.",
];

/// The path walk's ask, step 2.
const WALK_ASK: &str = "Add a cart total function. When the cart is empty, the total shows zero. The total shows the sum of the prices.";

fn store_with_purpose(purpose: &str) -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open a fresh ledger");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, '/tmp/synthetic-f11', 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
            params![PROJECT],
        )
        .expect("insert project");
    change::insert_change(
        &ledger,
        &NewChange {
            id: CHANGE.to_string(),
            project_id: PROJECT.to_string(),
            opened_at: "2026-01-02T09:00:00Z".to_string(),
            request: ChangeRequest {
                title: "Synthetic".to_string(),
                purpose: purpose.to_string(),
                prompt_event_id: None,
                requester: "sam".to_string(),
                source: RequestSource::Person,
            },
        },
    )
    .expect("insert the change");
    (dir, ledger)
}

fn proposals_for(purpose: &str) -> ProposalView {
    let (_dir, ledger) = store_with_purpose(purpose);
    change_propose::propose(&ledger, CHANGE, &ProfileEnvironment::empty()).expect("propose")
}

/// Whitespace collapsed, as the harness binds.
fn collapsed(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn criterion_texts(view: &ProposalView) -> Vec<String> {
    view.criteria.iter().map(|c| collapsed(&c.text)).collect()
}

#[test]
fn the_purpose_is_exactly_its_three_sentences() {
    assert_eq!(Q02_SENTENCES.join(" "), Q02_PURPOSE);
}

/// (a) The frozen C3 sentence, with its semicolon, is proposed whole as one
/// criterion — and so are C1 and C2.
#[test]
fn the_semicolon_sentence_is_proposed_whole() {
    let view = proposals_for(Q02_PURPOSE);
    let texts = criterion_texts(&view);
    for sentence in Q02_SENTENCES {
        assert!(
            texts.contains(&collapsed(sentence)),
            "not proposed: {sentence:?}; proposed {texts:?}; notes {:?}",
            view.notes.iter().map(|n| &n.text).collect::<Vec<_>>()
        );
    }
    assert!(
        view.notes.is_empty(),
        "no piece of the three sentences is left over: {:?}",
        view.notes.iter().map(|n| &n.text).collect::<Vec<_>>()
    );
    for piece in [
        "A delivery that has exhausted its attempts is shown as failed.",
        "the service saying 'sent' does not make it sent.",
    ] {
        assert!(
            !texts.iter().any(|t| t == piece),
            "a half of the semicolon sentence was proposed on its own: {piece:?}"
        );
    }
}

/// A hard wrap right after the semicolon does not cut the sentence either.
#[test]
fn a_semicolon_before_a_soft_wrap_does_not_end_the_sentence() {
    let wrapped = "A delivery that has exhausted its attempts is shown as failed;\nthe service saying 'sent' does not make it sent.";
    let view = proposals_for(wrapped);
    assert_eq!(
        criterion_texts(&view),
        vec![collapsed(Q02_SENTENCES[2])],
        "notes: {:?}",
        view.notes.iter().map(|n| &n.text).collect::<Vec<_>>()
    );
}

/// A semicolon before a capital still ends a span, the way a line break
/// would: two whole sentences, each proposed on its own.
#[test]
fn a_semicolon_before_a_capital_still_ends_a_span() {
    let view = proposals_for(
        "When the cart is empty, the total shows zero; When a price is negative, the form shows an error.",
    );
    assert_eq!(
        criterion_texts(&view),
        vec![
            "When the cart is empty, the total shows zero.".to_string(),
            "When a price is negative, the form shows an error.".to_string(),
        ]
    );
}

/// (b) The walk's ask proposes both behaviour sentences. The one sentence that
/// is not a behaviour is kept as a note that says, in plain words, why.
#[test]
fn the_walks_ask_proposes_both_behaviour_sentences() {
    let view = proposals_for(WALK_ASK);
    assert_eq!(
        criterion_texts(&view),
        vec![
            "When the cart is empty, the total shows zero.".to_string(),
            "The total shows the sum of the prices.".to_string(),
        ],
        "notes: {:?}",
        view.notes.iter().map(|n| &n.text).collect::<Vec<_>>()
    );
    let sum = &view.criteria[1];
    assert_eq!(sum.shape.subject, "The total");
    assert_eq!(
        sum.shape.condition, "",
        "no condition was stated, so none is invented"
    );
    assert_eq!(sum.shape.outcome, "shows the sum of the prices");

    assert_eq!(view.notes.len(), 1);
    let note = &view.notes[0];
    assert_eq!(note.text, "Add a cart total function");
    assert_eq!(note.refusal, ShapeRefusal::NoOutcome);
    let line = thruline_core::voice::proposal_line(note.reason_slug)
        .expect("the reason is a copy-file line");
    assert!(line.plain.starts_with("Kept as a note"), "{}", line.plain);
}

/// (c) Nothing is invented, and nothing of the ask is dropped without a note.
///
/// Every proposed criterion, less the one full stop the proposer adds, is a
/// contiguous span of the ask; every sentence of the ask is a criterion or a
/// note, whole.
#[test]
fn nothing_is_invented_and_nothing_is_dropped_silently() {
    for ask in [Q02_PURPOSE, WALK_ASK] {
        let view = proposals_for(ask);
        for criterion in &view.criteria {
            let body = criterion.text.strip_suffix('.').expect("ends in a stop");
            assert!(
                ask.contains(body),
                "{:?} is not a span of the ask",
                criterion.text
            );
            let quoted =
                &ask[criterion.source.offset..criterion.source.offset + criterion.source.length];
            assert_eq!(quoted, body, "the receipt span points at the words");
            for part in [
                &criterion.shape.subject,
                &criterion.shape.condition,
                &criterion.shape.outcome,
            ] {
                assert!(body.contains(part.as_str()), "{part:?} is not in {body:?}");
            }
        }
        let kept: Vec<String> = view
            .criteria
            .iter()
            .map(|c| c.text.trim_end_matches('.').to_string())
            .chain(view.notes.iter().map(|n| n.text.clone()))
            .collect();
        for sentence in ask.split(". ") {
            let sentence = sentence.trim_end_matches('.');
            assert!(
                kept.iter().any(|k| k == sentence),
                "{sentence:?} is neither proposed nor kept as a note: {kept:?}"
            );
        }
    }
}

/// A sentence that describes how things stand now is not a requirement, even
/// with a subject and an outcome; it stays a note, as it always did.
#[test]
fn a_description_of_how_things_are_now_is_not_proposed() {
    let view = proposals_for("The upload handler currently returns an error.");
    assert!(view.criteria.is_empty(), "{:?}", criterion_texts(&view));
    assert_eq!(view.notes.len(), 1);
    assert_eq!(view.notes[0].refusal, ShapeRefusal::NoCondition);
}

/// An outcome word with nothing after it has not said what someone would see.
#[test]
fn an_outcome_word_alone_is_not_a_result() {
    assert_eq!(
        change_propose::observable_shape("The total shows"),
        Err(ShapeRefusal::NoOutcome)
    );
}

/// A piece of a sentence is still kept as a note and never proposed, whatever
/// its shape.
#[test]
fn a_piece_of_a_sentence_is_kept_as_a_note() {
    let view =
        proposals_for("Does the total add up? and then the total shows the sum of the prices.");
    assert!(view.criteria.is_empty(), "{:?}", criterion_texts(&view));
    let fragment = view
        .notes
        .iter()
        .find(|n| n.refusal == ShapeRefusal::Fragment)
        .expect("the piece is kept as a note");
    assert!(fragment.text.starts_with("and then"), "{:?}", fragment.text);
    assert_eq!(fragment.reason_slug, "note_fragment");
}

// ---------------------------------------------------------------------------
// It claims less: one more criterion can only make Ready harder
// ---------------------------------------------------------------------------

/// One evidence item the real derivation produced, from a synthetic store:
/// the same fixture `change_readiness_derivation.rs` walks (one accepted
/// criterion, one confirmed session, one captured check run, derived links).
///
/// Taken from `coverage()` rather than written as a struct literal
/// (`QUAL-F11-LAUNCH-R5`, item 5): the item's shape grows on other lines of
/// work (`EvidenceItemView::assessable` arrived on the integration after this
/// lineage branched), and a literal compiles against only one of them. Every
/// field `admissible_item` needs is then set by name.
fn derived_item() -> EvidenceItemView {
    const RUN_KEY: &str = "s_run";
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    let root = dir
        .path()
        .canonicalize()
        .unwrap_or_else(|_| dir.path().to_path_buf())
        .display()
        .to_string();
    let conn = ledger.conn();
    conn.execute(
        "INSERT INTO projects(id, root_path, source, display_name, created_at)
         VALUES('p1',?1,'claude_code','Synthetic','2026-01-01T00:00:00Z')",
        params![root],
    )
    .expect("project");
    conn.execute(
        "INSERT INTO workspace_identities(project_id, repo_id, basis, resolved_root,
             resolved_via, sessions_sampled, observed_at)
         VALUES('p1','repo','git',?1,'recorded_cwd',1,'2026-01-01T00:00:00Z')",
        params![root],
    )
    .expect("identity");
    conn.execute(
        "INSERT INTO sessions(id, project_id, file_path, started_at)
         VALUES(?1,'p1',?2,'2026-01-01T00:00:00Z')",
        params![RUN_KEY, format!("/tmp/synthetic/{RUN_KEY}.jsonl")],
    )
    .expect("session");
    conn.execute(
        "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
             tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
         VALUES('e1','p1',?1,1,'2026-01-01T00:10:00Z','tool_use','Edit',?2,
                '/tmp/synthetic/x.jsonl',1,'aa','','bb')",
        params![
            RUN_KEY,
            format!("{{\"file_path\":\"{root}/crates/core/a.rs\"}}")
        ],
    )
    .expect("event");
    conn.execute(
        "INSERT INTO changes(id, project_id, opened_at, title, purpose, requester,
             request_source)
         VALUES('c1','p1','2026-01-02T09:00:00Z','Keep the suite green',
                'The suite should pass on what changed.','sam','person')",
        [],
    )
    .expect("change");
    let json = |v: &[&str]| serde_json::to_string(v).expect("json");
    conn.execute(
        "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at,
             criteria_json, exclusions_json, allowed_scope_json, constraints_json,
             check_profile_json, content_sha256)
         VALUES('c1',1,'2026-01-02T09:30:00Z',?1,'[]',?2,'[]',?3,'digest')",
        params![
            json(&["The test suite passes on the packages that changed."]),
            json(&["crates/**"]),
            json(&["cargo-test"])
        ],
    )
    .expect("scope");
    conn.execute(
        "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by,
             accepted_at, rationale)
         VALUES('c1',1,'sam','2026-01-02T09:40:00Z','this is what counts')",
        [],
    )
    .expect("acceptance");
    conn.execute(
        "INSERT INTO change_session_associations(id, change_id, session_id, provenance,
             basis, actor, asserted_at, reason)
         VALUES('a1','c1',?1,'confirmed','user','sam','2026-01-02T10:00:00Z','mine')",
        params![RUN_KEY],
    )
    .expect("confirm");
    conn.execute(
        "INSERT INTO evidence_bindings(id, seq, recorded_at, schema_version, scope_class,
             scope_key, root, repository_identity, pre_identity, post_identity,
             exit_code, term_signal, bound_at, source_session, doc, doc_sha256)
         VALUES('eb_test',0,'2026-01-03T09:00:00Z',1,'test',?1,'/tmp/synthetic','repo',
                'pre','post',0,NULL,'2026-01-03T09:00:00Z',?2,?3,'ddd')",
        params![
            "bash:/tmp/synthetic:cargo test --workspace",
            RUN_KEY,
            "{\"stdout\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":10},\
              \"stderr\":{\"kind\":\"captured\",\"sha256\":\"dd\",\"bytes\":10}}"
        ],
    )
    .expect("binding");
    derive_links(&ledger, "c1", "2026-02-01T12:00:00Z").expect("derive");
    let mut env = ProfileEnvironment::empty();
    env.tools_present.insert("cargo".to_string());
    let view = derived_coverage(&ledger, "c1", &env).expect("coverage");
    view.criteria
        .into_iter()
        .flat_map(|c| c.items)
        .next()
        .expect("the derivation produced an evidence item")
}

fn admissible_item() -> EvidenceItemView {
    let mut item = derived_item();
    item.link_id = "l1".into();
    item.kind = EvidenceKind::CheckRun;
    item.evidence_id = "e1".into();
    item.session_id = "s1".into();
    item.basis = LinkBasis::Declared;
    item.basis_slug = LinkBasis::Declared.label_slug();
    item.detail = None;
    item.actor = None;
    item.asserted_at = "2026-09-22T10:00:00Z".into();
    item.reason = None;
    item.name = "cargo test".into();
    item.observed = Observed::Proven;
    item.bears = true;
    item.note_slug = None;
    item.applies = Some(true);
    item.applies_slug = Some("inputs_applies");
    item.fresh = Some(true);
    item.fresh_slug = Some("applicability_still_applies");
    item.execution = ExecutionState::Completed;
    item.execution_slug = ExecutionState::Completed.slug();
    item.failure_line = None;
    item.evidence_class = None;
    item.evidence_class_slug = None;
    item
}

fn line(index: u32, text: &str, state: CoverageState) -> CriterionCoverage {
    let items = if state == CoverageState::Evidence {
        vec![admissible_item()]
    } else {
        Vec::new()
    };
    CriterionCoverage {
        index,
        text: text.into(),
        class: RunClass::Build,
        state,
        state_slug: state.slug(),
        absent_tool: None,
        items,
        removed: Vec::new(),
    }
}

fn coverage(criteria: Vec<CriterionCoverage>) -> CoverageView {
    CoverageView {
        schema_version: 1,
        change_id: "ch1".into(),
        scope_version: Some(1),
        criteria,
        candidates: Vec::<CandidateView>::new(),
        executions: Vec::new(),
        link_rows: 1,
        nothing_to_link_slug: None,
        nothing_bound_slug: None,
        execution_sessions: Vec::new(),
        masking: Default::default(),
    }
}

fn ready(view: &CoverageView) -> bool {
    compute(&ReadinessInput {
        change_id: "ch1",
        coverage: view,
        target_version: Some("e71c5b0aaaa".into()),
        has_examined_inputs: true,
        review_required: false,
        review_recorded: false,
        waivers: &[],
    })
    .ready
}

/// C1 and C2 proven is Ready. Adding the newly proposed C3 in any state that is
/// not itself proven takes Ready away; adding it proven keeps Ready. No state
/// of the extra line ever turns a not-ready answer into a ready one.
#[test]
fn one_more_criterion_can_only_make_ready_harder() {
    let states = [
        CoverageState::Evidence,
        CoverageState::None,
        CoverageState::AbsentTool,
        CoverageState::RanNothing,
        CoverageState::OtherVersion,
    ];
    for c2 in states {
        let without = coverage(vec![
            line(1, Q02_SENTENCES[0], CoverageState::Evidence),
            line(2, Q02_SENTENCES[1], c2),
        ]);
        for c3 in states {
            let with = coverage(vec![
                line(1, Q02_SENTENCES[0], CoverageState::Evidence),
                line(2, Q02_SENTENCES[1], c2),
                line(3, Q02_SENTENCES[2], c3),
            ]);
            assert!(
                !ready(&with) || ready(&without),
                "adding C3 ({c3:?}) made a not-ready Change ready (C2 {c2:?})"
            );
            if c3 != CoverageState::Evidence {
                assert!(!ready(&with), "an unproven C3 ({c3:?}) left it ready");
            }
        }
    }
    assert!(ready(&coverage(vec![
        line(1, Q02_SENTENCES[0], CoverageState::Evidence),
        line(2, Q02_SENTENCES[1], CoverageState::Evidence),
    ])));
}

/// The present-state table is closed, sorted, lower case and free of
/// duplicates, like the three tables beside it.
#[test]
fn the_present_state_table_is_closed_and_ordered() {
    let table = change_propose::PRESENT_STATE_MARKERS;
    let mut sorted = table.to_vec();
    sorted.sort_unstable();
    assert_eq!(table, sorted.as_slice());
    let unique: std::collections::BTreeSet<&&str> = table.iter().collect();
    assert_eq!(unique.len(), table.len());
    assert!(table
        .iter()
        .all(|e| e.chars().all(|c| c.is_ascii_lowercase() || c == ' ')));
}
