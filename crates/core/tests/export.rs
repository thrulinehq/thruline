//! §9 conformance: the export is the contract, nothing more.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;
use thruline_core::derive::{
    derive_project, fingerprint, ClaimVerdict, Currency, EvidenceState, Lifecycle, NodeDimensions,
    NodeState,
};
use thruline_core::entity_association::{
    self, AssociationDecision, AssociationSubject, ExactSpan, ProposalInput, ProposalProvenance,
};
use thruline_core::export::{export_project, export_project_with_options, ExportOptions};
use thruline_core::ingest::ingest;
use thruline_core::ledger::Ledger;

const PROJ: &str = "-Users-dev-export";

fn built() -> (Ledger, Value) {
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
    let graph = derive_project(&ledger, PROJ).expect("derive");
    let doc = export_project(&ledger, &graph, "2026-07-29T00:00:00Z").expect("export");
    std::mem::forget(dir); // keep the temp tree alive for the open connection
    (ledger, doc)
}

fn association_subjects(ledger: &Ledger) -> Vec<(String, String, String)> {
    let mut stmt = ledger
        .conn()
        .prepare(
            "SELECT id, session_id, text FROM events
             WHERE project_id = ?1 AND text IS NOT NULL AND text <> ''
             ORDER BY session_id, seq LIMIT 2",
        )
        .expect("association subjects");
    stmt.query_map([PROJ], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .expect("subject query")
        .collect::<Result<Vec<_>, _>>()
        .expect("subject rows")
}

#[test]
fn emits_exactly_the_section_9_keys_and_no_ui_block() {
    let (_l, d) = built();
    let mut keys: Vec<&str> = d
        .as_object()
        .expect("object")
        .keys()
        .map(|s| s.as_str())
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "accusations",
            // A158: the §9 widening adds the Layer-2 candidates section.
            "candidates",
            "contract_version",
            "counts",
            "diagnostics",
            "dimension_counts",
            "duplicate_events",
            "edges",
            "generated_at",
            "ledger",
            "nodes",
            "project",
            "receipts",
            "sessions"
        ]
    );
    assert_eq!(d["contract_version"], 2);
    assert_eq!(d["diagnostics"], false);
    assert!(
        d.get("entity_associations").is_none(),
        "an empty accepted projection must add no root block"
    );
    assert!(d.get("_ui").is_none(), "layout is the UI's job (A7)");
}

#[test]
fn accepted_entity_associations_are_minimal_deterministic_navigation_receipts() {
    let (ledger, before_export) = built();
    let subjects = association_subjects(&ledger);
    assert_eq!(subjects.len(), 2);
    let span_end = subjects[0]
        .2
        .chars()
        .next()
        .expect("subject text")
        .len_utf8() as u32;
    let before_graph = derive_project(&ledger, PROJ).expect("derive before associations");
    let before_fingerprint = fingerprint(&before_graph);

    let definitions = [
        ("Parser family", None, AssociationDecision::Associate, 401),
        (
            "Receipt family",
            Some(ExactSpan::new(0, span_end).expect("exact span")),
            AssociationDecision::Associate,
            402,
        ),
        ("Session family", None, AssociationDecision::Reject, 403),
        (
            "Artifact family",
            None,
            AssociationDecision::InsufficientEvidence,
            404,
        ),
    ];
    let mut proposals = definitions
        .iter()
        .map(|(label, span, _, proposed_at)| {
            entity_association::prepare(
                &ledger,
                ProposalInput {
                    project_id: PROJ.to_string(),
                    first: AssociationSubject::new(&subjects[0].0, *span).expect("first subject"),
                    second: AssociationSubject::new(&subjects[1].0, None).expect("second subject"),
                    label: (*label).to_string(),
                    provenance: ProposalProvenance::Operator {
                        operator: "private-reviewer-name".to_string(),
                    },
                    proposed_at_unix_ms: *proposed_at,
                },
            )
            .expect("association proposal")
        })
        .collect::<Vec<_>>();
    proposals.sort_by(|left, right| right.candidate_id.cmp(&left.candidate_id));
    for proposal in &proposals {
        assert!(entity_association::record_proposal(&ledger, proposal).expect("record proposal"));
        let (_, _, decision, decided_at) = definitions
            .iter()
            .find(|(label, _, _, _)| *label == proposal.label)
            .expect("proposal definition");
        entity_association::adjudicate(
            &ledger,
            &proposal.candidate_id,
            *decision,
            "private-decision-operator",
            "cli:private-route",
            *decided_at,
        )
        .expect("human decision");
    }
    let model_canary = format!("private-model-canary@sha256:{}", "c".repeat(64));
    let prompt_canary = "private-classification-prompt-canary";
    let output_canary = r#"{"classification":"private-output-canary"}"#;
    let model_annotation = thruline_core::classification_store::store(
        &ledger,
        thruline_core::classification_store::NewClassification {
            event_id: &subjects[0].0,
            model: &model_canary,
            prompt: prompt_canary,
            output_json: output_canary,
            confidence: 0.5,
            created_at: "2026-07-29T00:00:00Z",
        },
    )
    .expect("model classification provenance")
    .annotation;
    let model_proposal = entity_association::prepare(
        &ledger,
        ProposalInput {
            project_id: PROJ.to_string(),
            first: AssociationSubject::new(&subjects[0].0, None).expect("model first subject"),
            second: AssociationSubject::new(&subjects[1].0, None).expect("model second subject"),
            label: "Model family".to_string(),
            provenance: ProposalProvenance::Model {
                annotation_id: model_annotation.id.clone(),
            },
            proposed_at_unix_ms: 405,
        },
    )
    .expect("model association proposal");
    assert!(
        entity_association::record_proposal(&ledger, &model_proposal)
            .expect("record model proposal")
    );
    entity_association::adjudicate(
        &ledger,
        &model_proposal.candidate_id,
        AssociationDecision::Associate,
        "private-model-decision-operator",
        "cli:private-model-route",
        405,
    )
    .expect("accept model proposal");

    let graph = derive_project(&ledger, PROJ).expect("derive after associations");
    assert_eq!(
        fingerprint(&graph),
        before_fingerprint,
        "navigation associations must not change deterministic graph truth"
    );
    let public =
        export_project(&ledger, &graph, "2026-07-29T00:00:00Z").expect("public association export");
    let diagnostics = export_project_with_options(
        &ledger,
        &graph,
        "2026-07-29T00:00:00Z",
        ExportOptions { diagnostics: true },
    )
    .expect("diagnostic association export");
    assert_eq!(
        public["entity_associations"], diagnostics["entity_associations"],
        "accepted navigation receipts have one public/diagnostic shape"
    );

    let rows = public["entity_associations"]
        .as_array()
        .expect("association rows");
    assert_eq!(
        rows.len(),
        3,
        "reject and abstention must remain unexported"
    );
    let actual_ids = rows
        .iter()
        .map(|row| row["candidate_id"].as_str().expect("candidate id"))
        .collect::<Vec<_>>();
    let mut expected_ids = proposals
        .iter()
        .filter(|proposal| matches!(proposal.label.as_str(), "Parser family" | "Receipt family"))
        .map(|proposal| proposal.candidate_id.as_str())
        .collect::<Vec<_>>();
    expected_ids.push(model_proposal.candidate_id.as_str());
    expected_ids.sort_unstable();
    assert_eq!(
        actual_ids, expected_ids,
        "rows must sort by stable candidate id"
    );

    let expected_sessions = subjects
        .iter()
        .map(|(event, session, _)| (event.as_str(), session.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    for row in rows {
        let candidate_id = row["candidate_id"].as_str().expect("candidate id");
        let expected_proposal = proposals
            .iter()
            .chain(std::iter::once(&model_proposal))
            .find(|proposal| proposal.candidate_id == candidate_id)
            .expect("exported accepted proposal");
        let mut keys = row
            .as_object()
            .expect("association object")
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "candidate_id",
                "decided_at_unix_ms",
                "decision",
                "operator_attested",
                "project_id",
                "subjects",
            ]
        );
        assert_eq!(row["project_id"], PROJ);
        assert_eq!(row["operator_attested"], true);
        assert_eq!(row["decision"], "associate");
        assert!(row["decided_at_unix_ms"]
            .as_i64()
            .is_some_and(|time| time >= 0));
        let exported_subjects = row["subjects"].as_array().expect("subjects");
        assert_eq!(exported_subjects.len(), 2);
        for (index, subject) in exported_subjects.iter().enumerate() {
            let expected_subject = if index == 0 {
                &expected_proposal.first
            } else {
                &expected_proposal.second
            };
            let event_id = subject["event_id"].as_str().expect("event id");
            assert_eq!(event_id, expected_subject.event_id);
            assert_eq!(
                subject["session_id"], expected_sessions[event_id],
                "session receipt must resolve from the exact event"
            );
            let mut subject_keys = subject
                .as_object()
                .expect("subject object")
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>();
            subject_keys.sort_unstable();
            assert!(
                subject_keys == vec!["event_id", "session_id"]
                    || subject_keys == vec!["event_id", "session_id", "span"]
            );
            match (subject.get("span"), expected_subject.span) {
                (Some(span), Some(expected)) => {
                    let mut span_keys = span
                        .as_object()
                        .expect("span object")
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<_>>();
                    span_keys.sort_unstable();
                    assert_eq!(span_keys, vec!["end", "start"]);
                    assert_eq!(span["start"], expected.start);
                    assert_eq!(span["end"], expected.end);
                }
                (None, None) => {}
                _ => panic!("exported span disagrees with the exact subject receipt"),
            }
        }
    }

    let serialized =
        serde_json::to_string(&public["entity_associations"]).expect("association serialization");
    let raw_file_canary: String = ledger
        .conn()
        .query_row(
            "SELECT raw_file FROM events WHERE id = ?1",
            [&subjects[0].0],
            |row| row.get(0),
        )
        .expect("raw file canary");
    for forbidden in [
        "private-reviewer-name",
        "private-decision-operator",
        "private-model-decision-operator",
        "cli:private-route",
        "cli:private-model-route",
        &model_canary,
        &model_annotation.id,
        prompt_canary,
        output_canary,
        &subjects[0].2,
        &raw_file_canary,
        "Parser family",
        "Receipt family",
        "Model family",
        "provenance",
        "confidence",
        "raw_file",
        "path",
        "merge",
        "caus",
    ] {
        assert!(!serialized.contains(forbidden), "leaked {forbidden:?}");
    }
    let mut without_associations = public.clone();
    without_associations
        .as_object_mut()
        .expect("export object")
        .remove("entity_associations");
    assert_eq!(
        without_associations, before_export,
        "the optional navigation block must be the only export change"
    );
}

#[test]
fn entity_association_export_fails_closed_on_stored_corruption() {
    let (ledger, _) = built();
    let subjects = association_subjects(&ledger);
    let proposal = entity_association::prepare(
        &ledger,
        ProposalInput {
            project_id: PROJ.to_string(),
            first: AssociationSubject::new(&subjects[0].0, None).expect("first subject"),
            second: AssociationSubject::new(&subjects[1].0, None).expect("second subject"),
            label: "Parser family".to_string(),
            provenance: ProposalProvenance::Operator {
                operator: "reviewer".to_string(),
            },
            proposed_at_unix_ms: 500,
        },
    )
    .expect("proposal");
    entity_association::record_proposal(&ledger, &proposal).expect("record proposal");
    entity_association::adjudicate(
        &ledger,
        &proposal.candidate_id,
        AssociationDecision::Associate,
        "reviewer",
        "cli:test",
        501,
    )
    .expect("accept proposal");
    ledger
        .conn()
        .execute("DROP TRIGGER entity_association_proposals_no_update", [])
        .expect("remove application guard in corruption fixture");
    ledger
        .conn()
        .execute(
            "UPDATE entity_association_proposals SET label='unsafe ** label'
             WHERE candidate_id=?1",
            [&proposal.candidate_id],
        )
        .expect("corrupt stored label");

    let graph = derive_project(&ledger, PROJ).expect("derive remains independent");
    let error = export_project(&ledger, &graph, "2026-07-29T00:00:00Z")
        .expect_err("corrupt association must fail closed");
    assert!(
        error.to_string().contains("shadow-label contract"),
        "{error}"
    );
}

#[test]
fn internal_labels_never_export_identity_or_causality_prose() {
    let labels = [
        "Causing parser failure",
        "Parser causation",
        "Parser identity",
        "Same object",
        "Aliases parser",
        "Derived from parser",
        "Related retry path",
    ];
    let (ledger, _) = built();
    let subjects = association_subjects(&ledger);
    for label in labels {
        let proposal = entity_association::prepare(
            &ledger,
            ProposalInput {
                project_id: PROJ.to_string(),
                first: AssociationSubject::new(&subjects[0].0, None).expect("first subject"),
                second: AssociationSubject::new(&subjects[1].0, None).expect("second subject"),
                label: label.to_string(),
                provenance: ProposalProvenance::Operator {
                    operator: "reviewer".to_string(),
                },
                proposed_at_unix_ms: 600,
            },
        )
        .expect("internal label remains valid audit data");
        entity_association::record_proposal(&ledger, &proposal).expect("record proposal");
        entity_association::adjudicate(
            &ledger,
            &proposal.candidate_id,
            AssociationDecision::Associate,
            "reviewer",
            "cli:test",
            601,
        )
        .expect("accept proposal");
    }

    let graph = derive_project(&ledger, PROJ).expect("derive remains independent");
    let exported = export_project(&ledger, &graph, "2026-07-29T00:00:00Z")
        .expect("internal labels cannot brick export");
    let rows = exported["entity_associations"]
        .as_array()
        .expect("association rows");
    assert_eq!(rows.len(), labels.len());
    let serialized = serde_json::to_string(rows).expect("association JSON");
    for label in labels {
        assert!(
            !serialized.contains(label),
            "export leaked internal label {label:?}"
        );
    }
    let mut normalized = None;
    for row in rows {
        assert!(row.get("label").is_none());
        let mut row = row.clone();
        row.as_object_mut()
            .expect("association object")
            .remove("candidate_id");
        match &normalized {
            Some(expected) => assert_eq!(
                &row, expected,
                "internal label must change no public semantic field"
            ),
            None => normalized = Some(row),
        }
    }
}

#[test]
fn counts_cover_the_full_state_domain_and_match_the_nodes() {
    let (_l, d) = built();
    let counts = d["counts"].as_object().expect("counts");
    let mut names: Vec<&str> = counts.keys().map(|s| s.as_str()).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        vec![
            "contradicted",
            "pending",
            "recorded",
            "refused",
            "supported",
            "unknown",
        ],
        "ordinary totals contain graduated public classes only (§5s)"
    );
    for state in ["contradicted", "refused", "supported"] {
        assert_eq!(
            counts[state], 0,
            "{state} must be zero at the deterministic tier"
        );
    }
    for (state, n) in counts {
        let actual = d["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .filter(|x| x["sort_key"] == *state)
            .count();
        assert_eq!(n.as_u64(), Some(actual as u64), "count drift for {state}");
    }
}

#[test]
fn v2_exports_dimensions_and_compositions_without_an_authoritative_flat_state() {
    let (_l, d) = built();
    let nodes = d["nodes"].as_array().expect("nodes");
    assert!(!nodes.is_empty());

    for node in nodes {
        assert!(node.get("state").is_none(), "flat state leaked into v2");
        assert!(node["sort_key"].is_string());
        assert!(node["dimensions"]["lifecycle"].is_string());
        assert!(node["dimensions"]["evidence"].is_string());
        assert!(node["dimensions"]["currency"].is_string());
    }

    for dimension in ["lifecycle", "evidence", "currency"] {
        let total: u64 = d["dimension_counts"][dimension]
            .as_object()
            .expect("dimension count map")
            .values()
            .map(|v| v.as_u64().expect("count"))
            .sum();
        assert_eq!(total, nodes.len() as u64, "{dimension} count drift");
    }
}

#[test]
fn public_composition_keeps_graduated_claims_without_leaking_a_shadow_sibling() {
    let (l, _) = built();
    let mut graph = derive_project(&l, PROJ).expect("derive");
    let node = graph.nodes.first_mut().expect("node");
    node.sort_key = NodeState::Unsupported;
    node.dimensions = NodeDimensions::new(
        Lifecycle::Completed,
        EvidenceState::Supported,
        Currency::Current,
    );
    let verdict = |id: &str, state: NodeState, evidence: EvidenceState| ClaimVerdict {
        claim_id: id.to_string(),
        text: format!("synthetic {id}"),
        kind: "work".to_string(),
        marker: String::new(),
        span_start: 0,
        span_end: 1,
        candidates: 0,
        parsed_candidates: 0,
        best_score: None,
        best_event_id: None,
        best_fidelity: None,
        state,
        dimensions: NodeDimensions::new(Lifecycle::Completed, evidence, Currency::Current),
        reason: "synthetic contract fixture".to_string(),
        evidence_scopes: Vec::new(),
    };
    node.verdicts = vec![
        verdict("graduated", NodeState::Supported, EvidenceState::Supported),
        verdict("shadow", NodeState::Unsupported, EvidenceState::Unknown),
    ];
    let id = node.id.clone();

    let d = export_project(&l, &graph, "2026-07-29T00:00:00Z").expect("export");
    let exported = d["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["id"] == id)
        .expect("graduated sibling keeps node visible");
    assert_eq!(exported["sort_key"], "supported");
    assert_eq!(exported["dimensions"]["evidence"], "supported");
    assert_eq!(exported["composition"]["evidence"]["supported"], 1);
    assert!(exported["composition"]["evidence"].get("unknown").is_none());
    let receipt = &d["receipts"][&id]["claim"]["claims"];
    assert_eq!(receipt.as_array().expect("claims").len(), 1);
    assert!(!serde_json::to_string(&d)
        .expect("json")
        .contains("unsupported"));
}

#[test]
fn shadow_classes_require_the_explicit_diagnostics_export() {
    let (l, _) = built();
    for shadow in [NodeState::Unsupported, NodeState::Dropped] {
        let mut graph = derive_project(&l, PROJ).expect("derive");
        let node = graph.nodes.first_mut().expect("fixture node");
        node.sort_key = shadow;
        node.evidence.clear();
        node.verdicts.clear();
        let id = node.id.clone();

        let public = export_project(&l, &graph, "2026-07-29T00:00:00Z").expect("public export");
        assert_eq!(public["diagnostics"], false);
        assert!(public["counts"].get("unsupported").is_none());
        assert!(public["counts"].get("dropped").is_none());
        assert!(
            public["nodes"]
                .as_array()
                .expect("nodes")
                .iter()
                .all(|node| node["id"] != id),
            "shadow node leaked into public export: {id}"
        );
        assert!(public["receipts"].get(&id).is_none());
        assert!(public["edges"]
            .as_array()
            .expect("edges")
            .iter()
            .all(|edge| { edge["from"] != id && edge["to"] != id }));

        let diagnostics = export_project_with_options(
            &l,
            &graph,
            "2026-07-29T00:00:00Z",
            ExportOptions { diagnostics: true },
        )
        .expect("diagnostics export");
        assert_eq!(diagnostics["diagnostics"], true);
        assert_eq!(diagnostics["counts"][shadow.as_str()], 1);
        assert!(diagnostics["receipts"].get(&id).is_some());
    }
}

#[test]
fn every_node_has_a_receipt_and_every_verdict_is_honest_about_the_tier() {
    let (_l, d) = built();
    let nodes = d["nodes"].as_array().expect("nodes");
    let receipts = d["receipts"].as_object().expect("receipts");
    assert_eq!(nodes.len(), receipts.len());
    for n in nodes {
        let id = n["id"].as_str().expect("id");
        let r = receipts
            .get(id)
            .unwrap_or_else(|| panic!("no receipt for {id}"));
        assert!(r["claim"]["text"].is_string());
        assert!(
            r["claim"]["message_no"].is_number(),
            "receipt must point at a raw line"
        );
        // Nothing is entailment-checked at this tier.
        assert!(
            r["verdict"]["entailed"].is_null(),
            "M2 must not claim entailment"
        );
        assert_eq!(r["verdict"]["model"], "deterministic");
        assert!(
            r["verdict"].get("score").is_none(),
            "no score without a scorer"
        );
        assert_eq!(
            n["evidence_count"].as_u64(),
            Some(r["evidence"].as_array().expect("evidence").len() as u64)
        );
        for e in r["evidence"].as_array().expect("evidence") {
            let kind = e["kind"].as_str().expect("kind");
            assert!(
                matches!(kind, "edit" | "command" | "none"),
                "bad kind {kind}"
            );
            assert!(e["summary"].is_string());
        }
    }
}

#[test]
fn export_is_stable_across_runs() {
    let (l, first) = built();
    let graph = derive_project(&l, PROJ).expect("derive");
    let second = export_project(&l, &graph, "2026-07-29T00:00:00Z").expect("export");
    assert_eq!(
        serde_json::to_string(&first).expect("json"),
        serde_json::to_string(&second).expect("json"),
        "same ledger + same timestamp must give byte-identical JSON"
    );
}

#[test]
fn edges_use_only_the_section_9_kind_domain() {
    let (_l, d) = built();
    for e in d["edges"].as_array().expect("edges") {
        let k = e["kind"].as_str().expect("kind");
        assert!(matches!(k, "seq" | "inferred"), "bad edge kind {k}");
    }
}
