//! M6.R3 — the Context Packet compiled through the real pipeline.
//!
//! The fixture ledger is GENERATED here (A90: fixtures are generated, never
//! hand-authored) by writing a transcript, ingesting it, annotating it with a
//! model-free scorer, and deriving. The packet is then compiled from the
//! in-memory graph, so the golden file describes the real pipeline's output and
//! not a hand-drawn picture of it.
//!
//! REGENERATING THE GOLDEN FILE:
//!     INSTA_UPDATE=always cargo test -p thruline-core --test context_packet
//! Read the diff before accepting it. A changed packet is either an intended
//! product change or a regression, and the snapshot cannot tell you which.

use std::fs;

use serde_json::{json, Value};
use thruline_core::annotate::annotate;
use thruline_core::config::Config;
use thruline_core::derive::derive_project_with;
use thruline_core::ingest::ingest;
use thruline_core::nli::{EntailmentScorer, Scores};
use thruline_core::packet::{compile, Budget, Empty, Origin, Scope, Section, SECTION_ORDER};
use thruline_core::{Ledger, Result};

const PROJ: &str = "-Users-dev-packet";
const SESSION: &str = "aaaaaaaa-1111-4222-8333-444444444444";

/// Model-free: the NLI tier never decides truth here, and the contradiction the
/// packet renders comes from the deterministic contract (a claim of all-pass
/// against a run reporting a failure), not from a score.
struct NeverContradicts;

impl EntailmentScorer for NeverContradicts {
    fn score(&mut self, _premise: &str, _hypothesis: &str) -> Result<Scores> {
        Ok(Scores {
            entailment: 0.30,
            neutral: 0.70,
            contradiction: 0.0,
        })
    }
}

fn uuid(index: usize) -> String {
    format!("00000000-0000-4000-8000-{index:012}")
}

fn envelope(index: usize, parent: Option<usize>, message: Value) -> Value {
    json!({
        "type": message["role"].as_str().unwrap_or("user"),
        "uuid": uuid(index),
        "parentUuid": parent.map(uuid),
        "isSidechain": false,
        "sessionId": SESSION,
        "timestamp": format!("2026-08-05T02:00:{index:02}.000Z"),
        "cwd": "/repo",
        "gitBranch": "main",
        "message": message,
    })
}

fn transcript() -> String {
    let mut lines = vec![envelope(
        1,
        None,
        json!({"role":"user", "content":"run the suite"}),
    )];
    // One clean run and one that reports a failure, so the packet has both a
    // supportable and a conflicting object to render.
    let runs = [("t1", 12, 0), ("t2", 11, 1)];
    let mut parent = 1usize;
    let mut index = 2usize;
    for (tool_id, passed, failed) in runs {
        lines.push(envelope(
            index,
            Some(parent),
            json!({
                "role":"assistant",
                "content":[{"type":"tool_use","id":tool_id,"name":"Bash","input":{"command":"npm test"}}]
            }),
        ));
        parent = index;
        index += 1;
        lines.push(envelope(
            index,
            Some(parent),
            json!({
                "role":"user",
                "content":[{
                    "type":"tool_result",
                    "tool_use_id":tool_id,
                    "content":format!("# pass {passed}\n# fail {failed}")
                }]
            }),
        ));
        parent = index;
        index += 1;
    }
    lines.push(envelope(
        index,
        Some(parent),
        json!({"role":"assistant","content":[{"type":"text","text":"All 12 tests pass."}]}),
    ));
    lines
        .into_iter()
        .map(|l| serde_json::to_string(&l).expect("serialize"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn built() -> (
    tempfile::TempDir,
    Ledger,
    thruline_core::derive::DerivedGraph,
) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    let project = root.join(PROJ);
    fs::create_dir_all(&project).expect("project dir");
    fs::write(project.join(format!("{SESSION}.jsonl")), transcript()).expect("transcript");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("ledger");
    ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    // The SAME config must reach annotate and derive, or a proposal flips
    // between pending and dropped and the golden encodes a bug.
    let cfg = Config::default();
    annotate(&ledger, PROJ, &cfg, &mut NeverContradicts, "packet-stub-v1").expect("annotate");
    let graph = derive_project_with(&ledger, PROJ, &cfg).expect("derive");
    (dir, ledger, graph)
}

#[test]
fn the_packet_is_a_stable_golden_over_a_generated_ledger() {
    let (_dir, ledger, graph) = built();
    let packet = compile(
        &ledger,
        std::slice::from_ref(&graph),
        &Scope::Project(PROJ.to_string()),
        Some("close out the failing suite"),
        &Budget::default(),
    )
    .expect("compile");
    insta::assert_snapshot!("packet_project", packet.render());
}

#[test]
fn compiling_twice_at_one_chain_position_is_byte_identical() {
    let (_dir, ledger, graph) = built();
    let scope = Scope::Project(PROJ.to_string());
    let a = compile(
        &ledger,
        std::slice::from_ref(&graph),
        &scope,
        Some("t"),
        &Budget::default(),
    )
    .expect("first");
    let b = compile(
        &ledger,
        std::slice::from_ref(&graph),
        &scope,
        Some("t"),
        &Budget::default(),
    )
    .expect("second");
    assert_eq!(a.render(), b.render(), "the packet must be reproducible");
    assert_eq!(
        a.binding.digest, b.binding.digest,
        "chain position is stable"
    );
}

#[test]
fn every_section_appears_in_the_fixed_order_even_when_empty() {
    let (_dir, ledger, graph) = built();
    let rendered = compile(
        &ledger,
        std::slice::from_ref(&graph),
        &Scope::Project(PROJ.to_string()),
        Some("t"),
        &Budget::default(),
    )
    .expect("compile")
    .render();

    let mut cursor = 0usize;
    for section in SECTION_ORDER {
        let heading = format!("## {}", section.title());
        let at = rendered[cursor..]
            .find(&heading)
            .unwrap_or_else(|| panic!("section {} missing from the packet", section.title()));
        cursor += at + heading.len();
    }
}

/// The load-bearing honesty test: no claim reaches the page without its axes.
#[test]
fn no_claim_line_renders_as_bare_fact() {
    let (_dir, ledger, graph) = built();
    let packet = compile(
        &ledger,
        std::slice::from_ref(&graph),
        &Scope::Project(PROJ.to_string()),
        Some("t"),
        &Budget::default(),
    )
    .expect("compile");

    for line in &packet.lines {
        let rendered = line.render();
        assert!(
            rendered.contains("evidence=")
                && rendered.contains("currency=")
                && rendered.contains("applicability="),
            "a line reached the packet without its three axes: {rendered}"
        );
        assert!(
            rendered.contains("because: "),
            "a line reached the packet without its inclusion reason: {rendered}"
        );
        assert!(
            !line.origin.as_str().is_empty(),
            "a line reached the packet without an origin object id: {rendered}"
        );
    }
    assert!(
        !packet.lines.is_empty(),
        "the packet is empty — this test would pass vacuously"
    );
}

/// `verified` has no producer in the engine, so the section must be a MEASURED
/// zero and must never be filled from `supported`, which is a weaker claim.
#[test]
fn verified_is_a_measured_zero_and_is_never_filled_from_supported() {
    let (_dir, ledger, graph) = built();
    let packet = compile(
        &ledger,
        std::slice::from_ref(&graph),
        &Scope::Project(PROJ.to_string()),
        Some("t"),
        &Budget::default(),
    )
    .expect("compile");

    assert!(
        packet
            .lines
            .iter()
            .all(|l| l.section != Section::VerifiedCurrent),
        "nothing may be reported as verified while the engine produces no verified state"
    );
    match packet.empties.get(&Section::VerifiedCurrent) {
        Some(Empty::SourcePresentButEmpty { measured }) => {
            assert!(
                measured.contains("0 of"),
                "the empty reason must carry the measurement: {measured}"
            );
        }
        other => panic!("verified must be a measured zero, got {other:?}"),
    }
}

/// A section with no backing object must say so, and must NOT be reported as a
/// measured zero — the two are different facts about the world.
#[test]
fn structural_gaps_and_measured_zeroes_are_not_interchangeable() {
    let (_dir, ledger, graph) = built();
    let packet = compile(
        &ledger,
        std::slice::from_ref(&graph),
        &Scope::Project(PROJ.to_string()),
        Some("t"),
        &Budget::default(),
    )
    .expect("compile");

    for section in [
        Section::NonGoals,
        Section::Ownership,
        Section::OtherActiveWork,
        Section::FailedApproaches,
    ] {
        assert!(
            matches!(
                packet.empties.get(&section),
                Some(Empty::NoSourceObject { .. })
            ),
            "{} has no backing object and must say so, not report a measured zero",
            section.title()
        );
    }
}

#[test]
fn the_budget_trims_whole_lines_and_reports_every_exclusion() {
    let (_dir, ledger, graph) = built();
    let scope = Scope::Project(PROJ.to_string());
    let full = compile(
        &ledger,
        std::slice::from_ref(&graph),
        &scope,
        Some("t"),
        &Budget::default(),
    )
    .expect("full");
    let tight = compile(
        &ledger,
        std::slice::from_ref(&graph),
        &scope,
        Some("t"),
        &Budget { tokens: 60 },
    )
    .expect("tight");

    assert!(
        tight.lines.len() < full.lines.len(),
        "a tight budget must drop lines"
    );
    assert_eq!(
        tight.exclusions.len(),
        full.lines.len() - tight.lines.len(),
        "every dropped line must appear in the exclusions report"
    );
    // Whole lines only: each surviving line is byte-identical to its full form.
    for line in &tight.lines {
        assert!(
            full.lines.contains(line),
            "a line was altered rather than dropped: {}",
            line.render()
        );
    }
    // Protected sections survive any budget.
    assert!(
        tight.lines.iter().any(|l| l.section == Section::Task),
        "TASK must survive trimming"
    );
    assert!(
        tight
            .lines
            .iter()
            .any(|l| l.section == Section::SnapshotBinding),
        "SNAPSHOT BINDING must survive trimming — it is what makes the rest checkable"
    );
}

#[test]
fn a_caller_supplied_task_is_labelled_as_caller_supplied() {
    let (_dir, ledger, graph) = built();
    let packet = compile(
        &ledger,
        std::slice::from_ref(&graph),
        &Scope::Project(PROJ.to_string()),
        Some("ship the thing"),
        &Budget::default(),
    )
    .expect("compile");
    let task = packet
        .lines
        .iter()
        .find(|l| l.section == Section::Task)
        .expect("task line");
    assert_eq!(task.origin, Origin::Caller);
    assert!(
        task.reason.contains("caller"),
        "the task must be marked as supplied, not derived: {}",
        task.reason
    );
}

#[test]
fn a_conflicting_claim_reaches_the_packet_so_the_golden_is_not_vacuous() {
    // The transcript reports 11 pass / 1 fail against a claim of "All 12 tests
    // pass". If this stops producing a conflict, the golden file above is
    // describing an empty product and every other assertion here is hollow.
    let (_dir, _ledger, graph) = built();
    let conflicting = graph
        .nodes
        .iter()
        .filter(|n| {
            n.dimensions.evidence == thruline_core::derive::EvidenceState::Contradicted
                || n.dimensions.evidence == thruline_core::derive::EvidenceState::Supported
        })
        .count();
    assert!(
        conflicting > 0,
        "the generated ledger produced no supported or contradicted node; the fixture has \
         stopped exercising the verdict path and the golden file is vacuous"
    );
}

// ---------------------------------------------------------------------------
// A94b: the projection fast path, and the fallback that must never hide.
// ---------------------------------------------------------------------------

#[test]
fn a_never_built_projection_falls_back_and_says_so() {
    let (_dir, ledger, _graph) = built();
    let view = thruline_core::packet::view_for(&ledger, PROJ, &Config::default()).expect("view");
    assert_eq!(
        view.served_by,
        thruline_core::packet::ServedBy::InMemoryDerivation,
        "with no projection head, the only correct path is derivation"
    );
}

#[test]
fn a_current_projection_serves_the_fast_path_with_identical_content() {
    let (_dir, ledger, graph) = built();
    thruline_core::derive::rebuild(&ledger, PROJ, &Config::default()).expect("rebuild");

    let view = thruline_core::packet::view_for(&ledger, PROJ, &Config::default()).expect("view");
    assert_eq!(
        view.served_by,
        thruline_core::packet::ServedBy::PersistedProjection
    );

    // The fast path must not be a different answer. Same nodes, same axes.
    let derived = thruline_core::packet::ProjectionView::from(&graph);
    let key = |v: &thruline_core::packet::ProjectionView| {
        let mut rows: Vec<String> = v
            .nodes
            .iter()
            .map(|n| {
                format!(
                    "{}|{}|{}|{}|{}",
                    n.id,
                    n.label,
                    n.dimensions.lifecycle.as_str(),
                    n.dimensions.evidence.as_str(),
                    n.dimensions.currency.as_str()
                )
            })
            .collect();
        rows.sort();
        rows
    };
    assert_eq!(
        key(&view),
        key(&derived),
        "the persisted projection disagreed with a fresh derivation"
    );
}

/// BehindRecord must be reachable, and must route to the fallback — otherwise a
/// projection older than the ledger would answer about a world that has moved.
#[test]
fn a_projection_behind_the_record_falls_back() {
    use thruline_core::attention::{projection_state, ProjectionState};
    let (_dir, ledger, _graph) = built();
    thruline_core::derive::rebuild(&ledger, PROJ, &Config::default()).expect("rebuild");
    assert_eq!(
        projection_state(&ledger, PROJ).expect("state"),
        ProjectionState::Current
    );

    // Move the record: stamp the projection head at a position that is not the
    // current one, which is what an ingest since the last rebuild produces.
    ledger
        .save_projection_head(PROJ, &"0".repeat(64), 0, "2026-08-05T00:00:00Z")
        .expect("stamp");
    assert_eq!(
        projection_state(&ledger, PROJ).expect("state"),
        ProjectionState::BehindRecord,
        "BehindRecord must be reachable, or the fallback is untested"
    );

    let view = thruline_core::packet::view_for(&ledger, PROJ, &Config::default()).expect("view");
    assert_eq!(
        view.served_by,
        thruline_core::packet::ServedBy::InMemoryDerivation,
        "a projection behind the record must never serve the fast path"
    );
    assert!(
        !view.nodes.is_empty(),
        "the fallback must produce real content, not the stale head's zero count"
    );
}

#[test]
fn the_packet_reports_which_path_served_each_project() {
    let (_dir, ledger, graph) = built();
    let packet = compile(
        &ledger,
        std::slice::from_ref(&graph),
        &Scope::Project(PROJ.to_string()),
        Some("t"),
        &Budget::default(),
    )
    .expect("compile");
    assert_eq!(packet.served_by.len(), 1);
    assert!(
        packet
            .render()
            .contains("served from the in-memory derivation"),
        "the packet must name its own path so a fallback cannot pose as the fast path"
    );
}

// ---------------------------------------------------------------------------
// R13: retention and deltas.
// ---------------------------------------------------------------------------

use thruline_core::packet::{context_since, ContextAnswer, DeltaKind};

fn serve(ledger: &mut Ledger, task: &str) -> thruline_core::packet::Packet {
    let packet = thruline_core::packet::compile_request(
        ledger,
        &Scope::Project(PROJ.to_string()),
        Some(task),
        &Budget::default(),
        &Config::default(),
    )
    .expect("compile");
    packet
        .retain(ledger, Some("claude"), 1_700_000_000_000)
        .expect("retain");
    packet
}

#[test]
fn two_fetches_at_one_position_are_two_issues_and_one_content() {
    let (_dir, mut ledger, _g) = built();
    let a = serve(&mut ledger, "t");
    let b = serve(&mut ledger, "t");
    assert_eq!(
        a.binding.digest, b.binding.digest,
        "the position must not move because we asked twice"
    );
    let (issues, contents) = ledger.packet_issue_counts().expect("counts");
    assert_eq!(issues, 2, "every hand-out is its own issue");
    assert_eq!(
        contents, 1,
        "byte-identical packets share one content-addressed row"
    );
}

#[test]
fn an_unserved_position_is_unknown_not_unchanged() {
    let (_dir, ledger, _g) = built();
    let answer = context_since(
        &ledger,
        &Scope::Project(PROJ.to_string()),
        &"0".repeat(64),
        &Config::default(),
    )
    .expect("since");
    match answer {
        ContextAnswer::UnknownPosition { latest } => assert_eq!(latest.len(), 64),
        other => panic!("an unserved position must be UnknownPosition, got {other:?}"),
    }
}

#[test]
fn an_unmoved_position_is_current() {
    let (_dir, mut ledger, _g) = built();
    let p = serve(&mut ledger, "t");
    match context_since(
        &ledger,
        &Scope::Project(PROJ.to_string()),
        &p.binding.digest,
        &Config::default(),
    )
    .expect("since")
    {
        ContextAnswer::Current { position } => assert_eq!(position, p.binding.digest),
        other => panic!("expected Current, got {other:?}"),
    }
}

/// The golden: fetch at A, append events, and the change set must equal exactly
/// the lines that differ between packet-at-A and packet-at-B.
#[test]
fn the_change_set_is_exactly_the_lines_that_differ_at_b() {
    let (dir, mut ledger, _g) = built();
    let at_a = serve(&mut ledger, "close it out");

    // Append real events through the generator, then re-derive so the
    // projection is Current at B.
    let root = dir.path().join("projects");
    let extra = format!(
        "{}\n{}\n",
        serde_json::to_string(&envelope(
            10,
            Some(4),
            json!({"role":"assistant","content":[{"type":"tool_use","id":"t9","name":"Bash","input":{"command":"cargo test"}}]})
        ))
        .expect("ser"),
        serde_json::to_string(&envelope(
            11,
            Some(10),
            json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"t9","content":"# pass 7\n# fail 0"}]})
        ))
        .expect("ser")
    );
    let path = root.join(PROJ).join(format!("{SESSION}.jsonl"));
    let mut body = fs::read_to_string(&path).expect("read");
    body.push_str(&extra);
    fs::write(&path, body).expect("append");
    ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &dir.path().join("mirrors"),
    )
    .expect("re-ingest");
    thruline_core::derive::rebuild(&ledger, PROJ, &Config::default()).expect("rebuild");

    let at_b = thruline_core::packet::compile_request(
        &ledger,
        &Scope::Project(PROJ.to_string()),
        Some("close it out"),
        &Budget::default(),
        &Config::default(),
    )
    .expect("compile at B");
    assert_ne!(
        at_a.binding.digest, at_b.binding.digest,
        "appending events must move the chain position"
    );

    let answer = context_since(
        &ledger,
        &Scope::Project(PROJ.to_string()),
        &at_a.binding.digest,
        &Config::default(),
    )
    .expect("since");
    let ContextAnswer::Changed { lines, latest, .. } = answer else {
        panic!("expected Changed");
    };
    assert_eq!(latest, at_b.binding.digest);

    // The independent expectation: diff the two packets directly.
    let expected =
        thruline_core::packet::diff_lines(&at_a.retained_lines(), &at_b.retained_lines());
    assert_eq!(
        lines, expected,
        "the change set must equal the line difference between packet-at-A and packet-at-B"
    );
    assert!(
        !lines.is_empty(),
        "the fixture stopped producing a difference; this golden is vacuous"
    );
    // And it must be a real change, not only the binding line moving.
    assert!(
        lines.iter().any(|l| l.section != "SNAPSHOT BINDING"),
        "expected a change beyond the position itself: {lines:?}"
    );
}

#[test]
fn every_delta_line_carries_origin_axes_and_reason() {
    let (dir, mut ledger, _g) = built();
    let at_a = serve(&mut ledger, "t");
    let root = dir.path().join("projects");
    let path = root.join(PROJ).join(format!("{SESSION}.jsonl"));
    let mut body = fs::read_to_string(&path).expect("read");
    body.push_str(&format!(
        "{}\n",
        serde_json::to_string(&envelope(
            20,
            Some(4),
            json!({"role":"assistant","content":[{"type":"text","text":"Shipped the retry fix."}]})
        ))
        .expect("ser")
    ));
    fs::write(&path, body).expect("append");
    ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &dir.path().join("mirrors"),
    )
    .expect("re-ingest");
    thruline_core::derive::rebuild(&ledger, PROJ, &Config::default()).expect("rebuild");

    let ContextAnswer::Changed { lines, .. } = context_since(
        &ledger,
        &Scope::Project(PROJ.to_string()),
        &at_a.binding.digest,
        &Config::default(),
    )
    .expect("since") else {
        panic!("expected Changed");
    };
    assert!(!lines.is_empty());
    for l in &lines {
        let r = l.render();
        assert!(!l.origin.is_empty(), "a delta without an origin: {r}");
        assert!(
            r.contains("evidence=") && r.contains("currency=") && r.contains("applicability="),
            "a delta reached the caller without its axes: {r}"
        );
        assert!(r.contains("because: "), "a delta without its reason: {r}");
        assert!(
            matches!(
                l.kind,
                DeltaKind::Added | DeltaKind::Removed | DeltaKind::AxisChanged
            ),
            "every delta must say how it changed"
        );
    }
}

/// §5v: `thruline deltas` is a human surface, so its strings go through the
/// translation authority's banned first-level lexicon.
#[test]
fn the_human_delta_surface_passes_the_banned_lexicon() {
    use thruline_core::voice;

    // The sentences the CLI prints itself.
    for sentence in [
        "Nothing has changed. What you have still matches the record.",
        "No packet was handed out at that position, so what changed cannot be worked out. \
         Ask for a fresh packet.",
        "The record moved, but nothing in your packet changed with it.",
    ] {
        assert!(
            voice::banned_in(sentence).is_none(),
            "banned term {:?} in a first-level sentence: {sentence}",
            voice::banned_in(sentence)
        );
    }

    // And every rendered delta line from a real change set.
    let (dir, mut ledger, _g) = built();
    let at_a = serve(&mut ledger, "t");
    let root = dir.path().join("projects");
    let path = root.join(PROJ).join(format!("{SESSION}.jsonl"));
    let mut body = fs::read_to_string(&path).expect("read");
    body.push_str(&format!(
        "{}\n",
        serde_json::to_string(&envelope(
            30,
            Some(4),
            json!({"role":"assistant","content":[{"type":"text","text":"Added the guard."}]})
        ))
        .expect("ser")
    ));
    fs::write(&path, body).expect("append");
    ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &dir.path().join("mirrors"),
    )
    .expect("re-ingest");
    thruline_core::derive::rebuild(&ledger, PROJ, &Config::default()).expect("rebuild");

    let ContextAnswer::Changed { lines, .. } = context_since(
        &ledger,
        &Scope::Project(PROJ.to_string()),
        &at_a.binding.digest,
        &Config::default(),
    )
    .expect("since") else {
        panic!("expected Changed");
    };
    assert!(!lines.is_empty(), "vacuous lexicon check");
    for l in &lines {
        // The human reads the change description; the axis receipt is engine
        // detail one disclosure deeper, exactly as the doctrine allows.
        assert!(
            voice::banned_in(&l.text).is_none(),
            "banned term {:?} in a delta's first-level text: {}",
            voice::banned_in(&l.text),
            l.text
        );
        assert!(
            voice::banned_in(&l.section).is_none(),
            "banned term in a section title: {}",
            l.section
        );
    }
}

/// R17a: the VERIFIED section must never go silent.
///
/// Before this, the section had a counter and no line-producing branch, so a
/// verified node produced neither a line nor an empty reason and the renderer
/// fell through to a bare "(nothing)" — the section going quiet at exactly the
/// moment it finally had something to say.
///
/// SUPERSEDED IN LEVEL, NOT IN INTENT: the section now reads claim-level
/// verdicts, because RULING 4 caps node composition and a node can therefore
/// never compose to `verified`. The anti-silence property is unchanged; it is
/// asserted here at the level the section actually reads. The companion pin —
/// that a view which *cannot* answer claim-level questions says so rather than
/// reporting zero — lives in `verified_containment.rs`.
#[test]
fn a_verified_claim_renders_a_line_rather_than_going_silent() {
    use thruline_core::derive::{Currency, EvidenceState, Lifecycle, NodeDimensions};
    use thruline_core::packet::{compile_views, ClaimLine, ProjectionView, ServedBy, ViewNode};

    let (_dir, ledger, _g) = built();
    let view = ProjectionView {
        project_id: PROJ.to_string(),
        nodes: vec![ViewNode {
            id: "n1".to_string(),
            label: "the retry guard holds".to_string(),
            dimensions: NodeDimensions {
                lifecycle: Lifecycle::Completed,
                evidence: EvidenceState::Verified,
                currency: Currency::Current,
            },
        }],
        served_by: ServedBy::InMemoryDerivation,
        claim_verdicts: Some(vec![ClaimLine {
            node_id: "n1".to_string(),
            text: "the retry guard holds".to_string(),
            evidence: EvidenceState::Verified,
            currency: Currency::Current,
        }]),
    };
    let packet = compile_views(
        &ledger,
        &[view],
        &Scope::Project(PROJ.to_string()),
        Some("t"),
        &Budget::default(),
    )
    .expect("compile");

    let rendered = packet.render();
    let section = rendered
        .split("## VERIFIED AND CURRENT")
        .nth(1)
        .expect("section present")
        .split("\n## ")
        .next()
        .expect("section body");
    assert!(
        !section.contains("(nothing)"),
        "the VERIFIED section went silent with a verified node present: {section}"
    );
    assert!(
        section.contains("the retry guard holds"),
        "the verified node did not render: {section}"
    );
    assert!(
        section.contains("evidence=verified") && section.contains("because: "),
        "a verified line must carry its axes and its reason: {section}"
    );
}
