//! M4.R2 source identity and rejected-candidate diagnostics remain outside
//! ordinary project exports and outside accepted event authority.

use serde_json::Value;
use thruline_core::chain::sha256_hex;
use thruline_core::derive::derive_project;
use thruline_core::export::{export_project, export_project_with_options, ExportOptions};
use thruline_core::ledger::{
    AcceptedGeneration, CandidateAssessment, GenerationAcceptance, RawDisposition,
    SourceProjectIdentity, SourceSessionIdentity,
};
use thruline_core::{
    EventKind, Ledger, NormalizedEvent, ProjectRef, RawLineRef, SessionRef, Source,
};

const SHARED_ROOT: &str = "/synthetic/same-root";
const GENERATED_AT: &str = "2026-08-01T12:00:00Z";

#[derive(Clone, Copy)]
struct SourceFixture<'a> {
    project_id: &'a str,
    session_id: &'a str,
    owner_key: &'a str,
    source: Source,
    source_project_key: &'a str,
    source_session_key: &'a str,
    event_text: &'a str,
}

fn initial_acceptance(fixture: SourceFixture<'_>) -> GenerationAcceptance {
    let raw_envelope = format!("{{\"type\":\"message\",\"text\":{:?}}}", fixture.event_text);
    let payload = format!("{raw_envelope}\n");
    let payload_sha256 = sha256_hex(payload.as_bytes());
    let raw_sha256 = sha256_hex(raw_envelope.as_bytes());
    let generation_id = format!("generation-{}", fixture.session_id);
    let mirror_payload = format!("private/generations/{generation_id}/payload");
    let mirror_seal = format!("private/generations/{generation_id}/seal");
    let mut event = NormalizedEvent::new(EventKind::AssistantText);
    event.role = Some("assistant".to_string());
    event.text = Some(fixture.event_text.to_string());
    event.ts = Some(GENERATED_AT.to_string());

    GenerationAcceptance {
        project: ProjectRef {
            id: fixture.project_id.to_string(),
            root_path: SHARED_ROOT.to_string(),
            source: fixture.source,
            display_name: Some("same-root-project".to_string()),
        },
        project_identity: SourceProjectIdentity {
            project_id: fixture.project_id.to_string(),
            owner_key: fixture.owner_key.to_string(),
            source: fixture.source.as_str().to_string(),
            source_project_key: fixture.source_project_key.to_string(),
            normalized_root_hint: Some(SHARED_ROOT.to_string()),
        },
        session: SessionRef {
            id: fixture.session_id.to_string(),
            project_id: fixture.project_id.to_string(),
            file_path: mirror_payload.clone(),
            started_at: Some(GENERATED_AT.to_string()),
            raw_line_count: 1,
            raw_digest: Some(payload_sha256.clone()),
        },
        session_identity: SourceSessionIdentity {
            session_id: fixture.session_id.to_string(),
            project_id: fixture.project_id.to_string(),
            owner_key: fixture.owner_key.to_string(),
            source: fixture.source.as_str().to_string(),
            source_session_key: fixture.source_session_key.to_string(),
            source_locator: Some(format!(
                "/diagnostics-only/{}/{}.jsonl",
                fixture.owner_key, fixture.source_session_key
            )),
        },
        generation: AcceptedGeneration {
            id: generation_id,
            project_id: fixture.project_id.to_string(),
            session_id: fixture.session_id.to_string(),
            predecessor_id: None,
            mirror_payload_path: mirror_payload.clone(),
            mirror_seal_path: mirror_seal.clone(),
            payload_sha256: payload_sha256.clone(),
            payload_bytes: payload.len() as u64,
            seal_sha256: sha256_hex(mirror_seal.as_bytes()),
            inherited_prefix_bytes: 0,
            total_envelopes: 1,
            local_envelopes: 1,
            local_events: 1,
            accepted_at: GENERATED_AT.to_string(),
        },
        expected_predecessor: None,
        dispositions: vec![RawDisposition {
            raw_line: 1,
            raw_sha256: raw_sha256.clone(),
            byte_start: 0,
            byte_length: payload.len() as u64,
            envelope_family: "synthetic_message".to_string(),
            disposition: "emitted".to_string(),
            reason: None,
            source_turn_id: None,
            turn_boundary_raw_line: None,
            turn_boundary_raw_sha256: None,
            emitted_event_count: 1,
        }],
        events: vec![(
            RawLineRef {
                file: mirror_payload,
                line: 1,
                sha256: raw_sha256,
            },
            event,
        )],
    }
}

fn accept(ledger: &mut Ledger, fixture: SourceFixture<'_>) -> AcceptedGeneration {
    let acceptance = initial_acceptance(fixture);
    let placed = ledger
        .accept_source_generation(&acceptance)
        .expect("accept synthetic source generation");
    assert_eq!(placed.len(), 1);
    acceptance.generation
}

fn exports(ledger: &Ledger, project_id: &str) -> (Value, Value) {
    let graph = derive_project(ledger, project_id).expect("derive source project");
    let public = export_project(ledger, &graph, GENERATED_AT).expect("public export");
    let diagnostics = export_project_with_options(
        ledger,
        &graph,
        GENERATED_AT,
        ExportOptions { diagnostics: true },
    )
    .expect("diagnostics export");
    (public, diagnostics)
}

fn assert_only_diagnostics_envelope_differs(public: &Value, diagnostics: &Value) {
    let mut projected = diagnostics.clone();
    let object = projected
        .as_object_mut()
        .expect("diagnostics export is an object");
    object.remove("adapter_health");
    object.insert("diagnostics".to_string(), Value::Bool(false));
    let counts = object
        .get_mut("counts")
        .and_then(Value::as_object_mut)
        .expect("diagnostics counts");
    counts.remove("unsupported");
    counts.remove("dropped");
    assert_eq!(
        &projected, public,
        "non-shadow public facts changed merely by opening diagnostics"
    );
}

fn record_rejection(
    ledger: &Ledger,
    fixture: SourceFixture<'_>,
    generation: &AcceptedGeneration,
    suffix: &str,
    status: &str,
) {
    ledger
        .record_candidate_assessment(&CandidateAssessment {
            id: format!("diagnostic-candidate-{suffix}"),
            owner_key: fixture.owner_key.to_string(),
            source: fixture.source.as_str().to_string(),
            source_project_key: fixture.source_project_key.to_string(),
            source_session_key: fixture.source_session_key.to_string(),
            predecessor_generation_id: Some(generation.id.clone()),
            candidate_sha256: Some(sha256_hex(suffix.as_bytes())),
            candidate_bytes: Some(suffix.len() as u64),
            raw_envelopes: Some(1),
            status: status.to_string(),
            reason: format!("DIAGNOSTICS-ONLY-REASON-{suffix}"),
            observed_at: GENERATED_AT.to_string(),
        })
        .expect("record rejected candidate assessment");
}

#[test]
fn rejected_candidates_are_diagnostics_only_and_have_zero_event_authority() {
    let fixture = SourceFixture {
        project_id: "project-containment",
        session_id: "session-containment",
        owner_key: "PRIVATE-OWNER-CONTAINMENT",
        source: Source::Codex,
        source_project_key: "PRIVATE-NATIVE-PROJECT-CONTAINMENT",
        source_session_key: "PRIVATE-NATIVE-SESSION-CONTAINMENT",
        event_text: "accepted public event",
    };
    let mut ledger = Ledger::open_in_memory().expect("ledger");
    let generation = accept(&mut ledger, fixture);
    let (public_before, diagnostics_before) = exports(&ledger, fixture.project_id);
    let accepted_before = ledger
        .accepted_generation(fixture.session_id)
        .expect("accepted generation");
    let lineage_before = ledger
        .accepted_generation_lineage(fixture.session_id)
        .expect("accepted lineage");
    let chain_before = ledger
        .session_chain(fixture.session_id)
        .expect("accepted event chain");
    let project_head_before = ledger
        .compute_project_head(fixture.project_id)
        .expect("accepted project head");

    record_rejection(
        &ledger,
        fixture,
        &generation,
        "diverged-secret",
        "source_diverged",
    );
    record_rejection(
        &ledger,
        fixture,
        &generation,
        "transaction-secret",
        "transaction_failed",
    );
    record_rejection(
        &ledger,
        fixture,
        &generation,
        "successor-secret",
        "successor_rejected",
    );

    let (public_after, diagnostics_after) = exports(&ledger, fixture.project_id);
    assert_eq!(
        serde_json::to_vec(&public_after).expect("serialize public export"),
        serde_json::to_vec(&public_before).expect("serialize prior public export"),
        "diagnostic candidate writes changed the byte-stable public contract"
    );
    assert!(public_after.get("adapter_health").is_none());
    let public_text = serde_json::to_string(&public_after).expect("public JSON");
    for private in [
        fixture.owner_key,
        fixture.source_project_key,
        fixture.source_session_key,
        "diagnostic-candidate-",
        "DIAGNOSTICS-ONLY-REASON",
        "source_diverged",
        "transaction_failed",
        "successor_rejected",
    ] {
        assert!(
            !public_text.contains(private),
            "diagnostics-only value leaked into public export: {private}"
        );
    }

    assert_eq!(
        diagnostics_before["adapter_health"]["accepted_generations"],
        1
    );
    assert_eq!(
        diagnostics_before["adapter_health"]["rejected_candidates"],
        0
    );
    let health = &diagnostics_after["adapter_health"];
    assert_eq!(health["accepted_generations"], 1);
    assert_eq!(health["accepted_sessions"], 1);
    assert_eq!(health["rejected_candidates"], 3);
    assert_eq!(
        health["rejected_by_status"],
        serde_json::json!({
            "source_diverged": 1,
            "successor_rejected": 1,
            "transaction_failed": 1,
        })
    );
    assert_eq!(health["accepted_raw_envelopes"], 1);
    assert_eq!(health["quarantined_envelopes"], 3);
    assert_eq!(health["raw_envelopes"], 4);
    assert_eq!(health["normalized_events"], 1);
    assert_eq!(health["candidate_authority_events"], 0);
    assert_only_diagnostics_envelope_differs(&public_after, &diagnostics_after);
    let (_, diagnostics_again) = exports(&ledger, fixture.project_id);
    assert_eq!(
        serde_json::to_vec(&diagnostics_again).expect("serialize diagnostics"),
        serde_json::to_vec(&diagnostics_after).expect("serialize prior diagnostics"),
        "diagnostics aggregation is not deterministic"
    );

    assert_eq!(
        ledger
            .accepted_generation(fixture.session_id)
            .expect("generation after rejections"),
        accepted_before
    );
    assert_eq!(
        ledger
            .accepted_generation_lineage(fixture.session_id)
            .expect("lineage after rejections"),
        lineage_before
    );
    assert_eq!(
        ledger
            .session_chain(fixture.session_id)
            .expect("chain after rejections"),
        chain_before
    );
    assert_eq!(
        ledger
            .compute_project_head(fixture.project_id)
            .expect("head after rejections"),
        project_head_before
    );
    assert_eq!(
        ledger.event_count(fixture.project_id).expect("event count"),
        1
    );
}

#[test]
fn same_root_sources_keep_independent_identity_authority_and_exports() {
    let claude = SourceFixture {
        project_id: "project-same-root-claude",
        session_id: "session-same-root-claude",
        owner_key: "same-owner",
        source: Source::ClaudeCode,
        source_project_key: "same-native-project-key",
        source_session_key: "same-native-session-key",
        event_text: "I've updated `claude-only.rs` and cargo test passes.",
    };
    let codex = SourceFixture {
        project_id: "project-same-root-codex",
        session_id: "session-same-root-codex",
        owner_key: "same-owner",
        source: Source::Codex,
        source_project_key: "same-native-project-key",
        source_session_key: "same-native-session-key",
        event_text: "I've updated `codex-only.rs` and cargo test passes.",
    };
    let mut ledger = Ledger::open_in_memory().expect("ledger");

    accept(&mut ledger, claude);
    let (claude_public_before, claude_diagnostics_before) = exports(&ledger, claude.project_id);
    let codex_generation = accept(&mut ledger, codex);
    record_rejection(
        &ledger,
        codex,
        &codex_generation,
        "codex-only-rejection",
        "source_diverged",
    );

    assert_eq!(
        ledger
            .source_project_id(
                claude.owner_key,
                claude.source.as_str(),
                claude.source_project_key,
            )
            .expect("Claude source identity"),
        Some(claude.project_id.to_string())
    );
    assert_eq!(
        ledger
            .source_project_id(
                codex.owner_key,
                codex.source.as_str(),
                codex.source_project_key,
            )
            .expect("Codex source identity"),
        Some(codex.project_id.to_string())
    );
    assert_eq!(
        ledger
            .source_session_id(
                claude.owner_key,
                claude.source.as_str(),
                claude.source_session_key,
            )
            .expect("Claude source session"),
        Some(claude.session_id.to_string())
    );
    assert_eq!(
        ledger
            .source_session_id(
                codex.owner_key,
                codex.source.as_str(),
                codex.source_session_key,
            )
            .expect("Codex source session"),
        Some(codex.session_id.to_string())
    );

    let (claude_public_after, claude_diagnostics_after) = exports(&ledger, claude.project_id);
    let (codex_public, codex_diagnostics) = exports(&ledger, codex.project_id);
    assert_eq!(
        serde_json::to_vec(&claude_public_after).expect("serialize Claude export"),
        serde_json::to_vec(&claude_public_before).expect("serialize prior Claude export"),
        "adding a same-root Codex project changed the Claude public export"
    );
    assert_eq!(
        serde_json::to_vec(&claude_diagnostics_after).expect("serialize Claude diagnostics"),
        serde_json::to_vec(&claude_diagnostics_before).expect("serialize prior diagnostics"),
        "Codex diagnostics leaked into the same-root Claude project"
    );

    assert_eq!(claude_public_after["project"]["id"], claude.project_id);
    assert_eq!(claude_public_after["project"]["source"], "claude_code");
    assert_eq!(codex_public["project"]["id"], codex.project_id);
    assert_eq!(codex_public["project"]["source"], "codex");
    assert_eq!(claude_public_after["ledger"]["events"], 1);
    assert_eq!(codex_public["ledger"]["events"], 1);
    assert_eq!(claude_public_after["sessions"][0]["id"], claude.session_id);
    assert_eq!(codex_public["sessions"][0]["id"], codex.session_id);
    assert_ne!(
        claude_public_after["ledger"]["head"], codex_public["ledger"]["head"],
        "independent source projects unexpectedly share a ledger head"
    );

    let claude_text = serde_json::to_string(&claude_public_after).expect("Claude JSON");
    let codex_text = serde_json::to_string(&codex_public).expect("Codex JSON");
    assert!(claude_text.contains(claude.event_text));
    assert!(!claude_text.contains(codex.event_text));
    assert!(!claude_text.contains(codex.session_id));
    assert!(codex_text.contains(codex.event_text));
    assert!(!codex_text.contains(claude.event_text));
    assert!(!codex_text.contains(claude.session_id));
    assert!(claude_public_after.get("adapter_health").is_none());
    assert!(codex_public.get("adapter_health").is_none());
    let claude_health = &claude_diagnostics_after["adapter_health"];
    assert_eq!(claude_health["accepted_generations"], 1);
    assert_eq!(claude_health["rejected_candidates"], 0);
    assert_eq!(claude_health["raw_envelopes"], 1);
    assert_eq!(claude_health["candidate_authority_events"], 0);
    let codex_health = &codex_diagnostics["adapter_health"];
    assert_eq!(codex_health["accepted_generations"], 1);
    assert_eq!(codex_health["rejected_candidates"], 1);
    assert_eq!(
        codex_health["rejected_by_status"],
        serde_json::json!({"source_diverged": 1})
    );
    assert_eq!(codex_health["accepted_raw_envelopes"], 1);
    assert_eq!(codex_health["quarantined_envelopes"], 1);
    assert_eq!(codex_health["raw_envelopes"], 2);
    assert_eq!(codex_health["candidate_authority_events"], 0);
    assert_only_diagnostics_envelope_differs(&claude_public_after, &claude_diagnostics_after);
    assert_only_diagnostics_envelope_differs(&codex_public, &codex_diagnostics);
}
