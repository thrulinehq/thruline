//! R17a-1 — the cage, built and demonstrated holding BEFORE anything real can
//! enter it.
//!
//! There is still no `EvidenceState::Verified` producer. Both containments are
//! nonetheless reachable today through caller-supplied graphs, which the R17a
//! render fix already proved, so they are exercised here with synthetic
//! verified states. A cage tested only after the animal arrives is not a cage.

use thruline_core::derive::{
    compose_dimensions_for_test, Currency, EvidenceState, Lifecycle, NodeDimensions,
};

fn claim(evidence: EvidenceState) -> NodeDimensions {
    NodeDimensions::new(Lifecycle::Completed, evidence, Currency::Current)
}

// ---------------------------------------------------------------------------
// RULING 4 — the composition cap, pinned in BOTH directions.
// ---------------------------------------------------------------------------

#[test]
fn one_verified_claim_does_not_verify_the_whole_node() {
    let composed = compose_dimensions_for_test(&[
        claim(EvidenceState::Verified),
        claim(EvidenceState::Unknown),
        claim(EvidenceState::Unknown),
    ]);
    assert_ne!(
        composed.evidence,
        EvidenceState::Verified,
        "one verified claim among unknowns must not publish the node as verified — \
         that is worst-claim-wins pointed the other way"
    );
    assert_eq!(
        composed.evidence,
        EvidenceState::Supported,
        "the cap must land on the strongest class a composition may honestly assert"
    );
}

#[test]
fn an_all_verified_node_is_still_capped() {
    // Even unanimity does not license it: the cap is about what a COMPOSITION
    // may assert, not about how many claims agree.
    let composed = compose_dimensions_for_test(&[
        claim(EvidenceState::Verified),
        claim(EvidenceState::Verified),
    ]);
    assert_eq!(composed.evidence, EvidenceState::Supported);
}

/// The other direction of the asymmetry — asserted, not assumed. An accusation
/// rising is protective, and this test exists so that capping promotions cannot
/// quietly cap accusations too.
#[test]
fn one_contradicted_claim_still_rises_to_the_node() {
    let composed = compose_dimensions_for_test(&[
        claim(EvidenceState::Contradicted),
        claim(EvidenceState::Unknown),
        claim(EvidenceState::Supported),
    ]);
    assert_eq!(
        composed.evidence,
        EvidenceState::Contradicted,
        "one conflict among many claims must still surface at node level"
    );
}

#[test]
fn the_ordinary_fold_is_untouched() {
    assert_eq!(
        compose_dimensions_for_test(&[
            claim(EvidenceState::Supported),
            claim(EvidenceState::Unknown)
        ])
        .evidence,
        EvidenceState::Supported
    );
    assert_eq!(
        compose_dimensions_for_test(&[
            claim(EvidenceState::Observed),
            claim(EvidenceState::Unknown)
        ])
        .evidence,
        EvidenceState::Observed
    );
    assert_eq!(
        compose_dimensions_for_test(&[
            claim(EvidenceState::Stale),
            claim(EvidenceState::Supported)
        ])
        .evidence,
        EvidenceState::Stale,
        "stale rising is a warning, and warnings rise"
    );
}

// ---------------------------------------------------------------------------
// RULING 3 — the shadow gate on evidence classes.
// ---------------------------------------------------------------------------

use std::fs;
use std::path::PathBuf;
use thruline_core::derive::derive_project;
use thruline_core::export::{export_project_with_options, ExportOptions};
use thruline_core::ingest::ingest;
use thruline_core::ledger::Ledger;

const PROJ: &str = "-Users-dev-containment";

/// A real graph from the real pipeline, with ONE node's evidence set to
/// `verified` by hand. Synthetic on purpose: no producer exists, and the
/// containment has to be shown holding before one does.
fn graph_with_a_synthetic_verified() -> (
    tempfile::TempDir,
    Ledger,
    thruline_core::derive::DerivedGraph,
) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    fs::create_dir_all(root.join(PROJ)).expect("mkdir");
    let body = fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claude_code_sessions.jsonl"),
    )
    .expect("fixture");
    fs::write(
        root.join(PROJ)
            .join("00000000-0000-4000-8000-000000000000.jsonl"),
        body,
    )
    .expect("write");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    let mut graph = derive_project(&ledger, PROJ).expect("derive");
    assert!(!graph.nodes.is_empty(), "fixture produced no nodes");
    graph.nodes[0].dimensions = NodeDimensions::new(
        Lifecycle::Completed,
        EvidenceState::Verified,
        Currency::Current,
    );
    (dir, ledger, graph)
}

fn node_evidence(doc: &serde_json::Value) -> Vec<String> {
    doc["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .map(|n| {
            n["dimensions"]["evidence"]
                .as_str()
                .unwrap_or("")
                .to_string()
        })
        .collect()
}

fn evidence_counts(doc: &serde_json::Value) -> serde_json::Value {
    // Located by structure rather than assumed: find the first object that has
    // all six evidence-class keys.
    fn walk(v: &serde_json::Value) -> Option<serde_json::Value> {
        if let Some(o) = v.as_object() {
            if [
                "contradicted",
                "observed",
                "stale",
                "supported",
                "unknown",
                "verified",
            ]
            .iter()
            .all(|k| o.contains_key(*k))
            {
                return Some(v.clone());
            }
            for (_, child) in o {
                if let Some(found) = walk(child) {
                    return Some(found);
                }
            }
        }
        if let Some(a) = v.as_array() {
            for child in a {
                if let Some(found) = walk(child) {
                    return Some(found);
                }
            }
        }
        None
    }
    walk(doc).expect("an evidence counts object must exist in the export")
}

#[test]
fn a_verified_state_never_reaches_the_default_public_export() {
    let (_d, ledger, graph) = graph_with_a_synthetic_verified();
    let doc = export_project_with_options(
        &ledger,
        &graph,
        "2026-08-05T00:00:00Z",
        ExportOptions::default(),
    )
    .expect("export");

    // Assert on the parsed document, not on a substring scan: this file also
    // contains `"ledger":{"verified":false}`, which is the CHAIN verification
    // flag and an entirely different sense of the word. A grep-shaped test
    // cannot tell an evidence class from a homonym.
    assert!(
        !node_evidence(&doc).iter().any(|e| e == "verified"),
        "an ungraduated evidence class reached the default public export: {:?}",
        node_evidence(&doc)
    );
    assert_eq!(
        evidence_counts(&doc)["verified"],
        serde_json::json!(0),
        "the public composition counted a verified it must not publish"
    );
    assert_eq!(
        evidence_counts(&doc)["supported"],
        serde_json::json!(1),
        "the capped node must be counted at its graduated floor instead"
    );
}

#[test]
fn diagnostics_still_discloses_the_true_class() {
    // Containment hides it from the product surface; it must never hide it from
    // the operator, or the engine would be lying to its own author.
    let (_d, ledger, graph) = graph_with_a_synthetic_verified();
    let doc = export_project_with_options(
        &ledger,
        &graph,
        "2026-08-05T00:00:00Z",
        ExportOptions { diagnostics: true },
    )
    .expect("export");
    assert!(
        node_evidence(&doc).iter().any(|e| e == "verified"),
        "the diagnostics export must disclose the true evidence class: {:?}",
        node_evidence(&doc)
    );
    assert_eq!(evidence_counts(&doc)["verified"], serde_json::json!(1));
}

#[test]
fn containment_only_ever_downgrades() {
    // The published class must be weaker than or equal to the real one. A
    // containment that sharpened a verdict would be worse than none.
    let (_d, ledger, graph) = graph_with_a_synthetic_verified();
    let doc = export_project_with_options(
        &ledger,
        &graph,
        "2026-08-05T00:00:00Z",
        ExportOptions::default(),
    )
    .expect("export");
    for sharper in ["contradicted", "stale"] {
        assert_eq!(
            evidence_counts(&doc)[sharper],
            serde_json::json!(0),
            "containment published {sharper}, which is sharper than the verified it replaced"
        );
    }
    assert_eq!(node_evidence(&doc), vec!["supported".to_string()]);
}

// ---------------------------------------------------------------------------
// Item A — the VERIFIED section reads claim level, and says only what it measured.
// ---------------------------------------------------------------------------

use thruline_core::packet::{
    compile_views, Budget, ClaimLine, ProjectionView, Scope, ServedBy, ViewNode,
};

fn view_with(claims: Vec<ClaimLine>, from: ServedBy) -> ProjectionView {
    ProjectionView {
        project_id: PROJ.to_string(),
        nodes: vec![ViewNode {
            id: "n1".to_string(),
            label: "a node".to_string(),
            // The node stays CAPPED — RULING 4 is untouched.
            dimensions: NodeDimensions::new(
                Lifecycle::Completed,
                EvidenceState::Supported,
                Currency::Current,
            ),
        }],
        served_by: from,
        claim_verdicts: Some(claims),
    }
}

fn section_of(view: ProjectionView) -> String {
    let (_d, ledger, _g) = graph_with_a_synthetic_verified();
    let packet = compile_views(
        &ledger,
        &[view],
        &Scope::Project(PROJ.to_string()),
        Some("t"),
        &Budget::default(),
    )
    .expect("compile");
    let r = packet.render();
    r.split("## VERIFIED AND CURRENT")
        .nth(1)
        .and_then(|s| s.split("\n## ").next())
        .unwrap_or("")
        .to_string()
}

#[test]
fn a_verified_claim_renders_even_though_its_node_is_capped() {
    let section = section_of(view_with(
        vec![ClaimLine {
            node_id: "n1".to_string(),
            text: "the retry guard holds".to_string(),
            evidence: EvidenceState::Verified,
            currency: Currency::Current,
        }],
        ServedBy::InMemoryDerivation,
    ));
    assert!(
        section.contains("the retry guard holds"),
        "a verified CLAIM must render even though RULING 4 caps its node: {section}"
    );
    assert!(
        section.contains("evidence=verified") && section.contains("because: "),
        "Line discipline intact — origin, axes and reason: {section}"
    );
    assert!(
        section.contains("SHADOW"),
        "and its shadow status must be disclosed on the line: {section}"
    );
    assert!(
        section.contains("claim:"),
        "the origin must be the CLAIM, not the node: {section}"
    );
}

/// The empty reason must state ONLY what was measured. The old text asserted
/// "No engine path produces `verified`" and "the capture substrate is empty" —
/// prose about engine capability that went false the moment R17a-3 shipped a
/// producer, and that no compile can verify.
#[test]
fn the_empty_reason_carries_no_engine_capability_prose() {
    let section = section_of(view_with(
        vec![ClaimLine {
            node_id: "n1".to_string(),
            text: "unproven".to_string(),
            evidence: EvidenceState::Supported,
            currency: Currency::Current,
        }],
        ServedBy::InMemoryDerivation,
    ));
    assert!(section.contains("0 of 1 claim(s)"), "it counts: {section}");
    for prose in [
        "No engine path produces",
        "capture substrate is empty",
        "at M6.R3",
    ] {
        assert!(
            !section.contains(prose),
            "the empty reason must not assert engine capability it cannot verify: {prose:?}"
        );
    }
}

/// "The projection cannot answer" is a different fact from "nothing is
/// verified", and collapsing them is the A94a mistake.
#[test]
fn a_fast_path_view_says_it_cannot_answer_rather_than_zero() {
    let mut view = view_with(vec![], ServedBy::PersistedProjection);
    view.claim_verdicts = None;
    let section = section_of(view);
    assert!(
        section.contains("cannot be answered from the persisted projection"),
        "it must name the limitation: {section}"
    );
    assert!(
        !section.contains("0 of"),
        "and must NOT report a measured zero it did not measure: {section}"
    );
}
