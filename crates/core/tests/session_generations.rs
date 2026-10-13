//! Accepted source generations are atomic, immutable authority.

mod scratch;

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use scratch::TempTree;
use thruline_core::chain::sha256_hex;
use thruline_core::ledger::{
    AcceptedGeneration, GenerationAcceptance, GenerationCommitFailpoint, RawDisposition,
    SourceProjectIdentity, SourceSessionIdentity,
};
use thruline_core::mirror::{LineageDisposition, MirrorIdentity, MirrorStore, SealedMirror};
use thruline_core::{
    CoreError, EventKind, Ledger, NormalizedEvent, ProjectRef, RawLineRef, SessionRef, Source,
};

const PROJECT_ID: &str = "project-generation-integration";
const SESSION_ID: &str = "session-generation-integration";
const OWNER_KEY: &str = "owner-generation-integration";
const SOURCE_PROJECT_KEY: &str = "native-project-generation-integration";
const SOURCE_SESSION_KEY: &str = "native-session-generation-integration";
const ROOT_HINT: &str = "/workspace/generation-integration";

const FIRST_LINE: &str = r#"{"type":"user","message":"first"}"#;
const SECOND_LINE: &str = r#"{"type":"assistant","message":"second"}"#;
const THIRD_LINE: &str = r#"{"type":"assistant","message":"third"}"#;

struct CapturedLineage {
    _temp: TempTree,
    store: MirrorStore,
    source: PathBuf,
    first: SealedMirror,
    successor: SealedMirror,
}

impl CapturedLineage {
    fn new() -> Self {
        let temp = TempTree::new();
        let source = temp.path().join("live-session.jsonl");
        let store = MirrorStore::open(temp.path().join("private-mirror"))
            .expect("private mirror store opens outside the repository");
        let lease = store.acquire_ingest_lease().expect("mirror lease");
        let identity = mirror_identity();

        let first_payload = format!("{FIRST_LINE}\n{SECOND_LINE}\n");
        fs::write(&source, first_payload).expect("write first live generation");
        let first = store
            .capture(&lease, &source, &identity)
            .expect("capture and seal first generation");
        assert_eq!(
            store.classify(&first, None).expect("classify first"),
            LineageDisposition::Initial
        );

        let successor_payload = format!("{FIRST_LINE}\n{SECOND_LINE}\n{THIRD_LINE}\n");
        fs::write(&source, successor_payload).expect("append valid live suffix");
        let successor = store
            .capture(&lease, &source, &identity)
            .expect("capture and seal successor generation");
        assert_eq!(
            store
                .classify(&successor, Some(&first))
                .expect("classify successor"),
            LineageDisposition::PrefixSuccessor
        );

        Self {
            _temp: temp,
            store,
            source,
            first,
            successor,
        }
    }

    fn first_acceptance(&self) -> GenerationAcceptance {
        acceptance(
            &self.store,
            &self.source,
            &self.first,
            None,
            0,
            2,
            &[
                (1, FIRST_LINE, 0, EventKind::UserMsg, "first"),
                (
                    2,
                    SECOND_LINE,
                    (FIRST_LINE.len() + 1) as u64,
                    EventKind::AssistantText,
                    "second",
                ),
            ],
        )
    }

    fn successor_acceptance(&self) -> GenerationAcceptance {
        acceptance(
            &self.store,
            &self.source,
            &self.successor,
            Some(&self.first.generation_key),
            self.first.payload_bytes,
            3,
            &[(
                3,
                THIRD_LINE,
                self.first.payload_bytes,
                EventKind::AssistantText,
                "third",
            )],
        )
    }

    fn reconciled_first_acceptance(&self) -> GenerationAcceptance {
        let mut acceptance = self.first_acceptance();
        let slot_key = "a".repeat(64);
        acceptance.events[0].1.src_uuid = Some(slot_key.clone());
        acceptance.events[0].1.source_message_key = Some("b".repeat(64));
        acceptance.events.remove(1);
        acceptance.dispositions[1].disposition = "suppressed_mirror".to_string();
        acceptance.dispositions[1].reason = Some(format!("matched_slot:{slot_key}"));
        acceptance.dispositions[1].emitted_event_count = 0;
        acceptance.generation.local_events = 1;
        acceptance
    }
}

fn mirror_identity() -> MirrorIdentity {
    MirrorIdentity {
        owner_key: OWNER_KEY.to_string(),
        source_project_key: SOURCE_PROJECT_KEY.to_string(),
        source_session_key: SOURCE_SESSION_KEY.to_string(),
    }
}

fn acceptance(
    store: &MirrorStore,
    source: &std::path::Path,
    sealed: &SealedMirror,
    predecessor: Option<&str>,
    inherited_prefix_bytes: u64,
    total_envelopes: u64,
    local: &[(u64, &str, u64, EventKind, &str)],
) -> GenerationAcceptance {
    let mirror_payload = store
        .resolve_payload(sealed)
        .expect("sealed payload resolves")
        .to_string_lossy()
        .into_owned();
    let predecessor = predecessor.map(str::to_string);
    let dispositions = local
        .iter()
        .map(|(line, raw, byte_start, _, _)| RawDisposition {
            raw_line: *line,
            raw_sha256: sha256_hex(raw.as_bytes()),
            byte_start: *byte_start,
            byte_length: (raw.len() + 1) as u64,
            envelope_family: "fixture_message".to_string(),
            disposition: "emitted".to_string(),
            reason: None,
            source_turn_id: None,
            turn_boundary_raw_line: None,
            turn_boundary_raw_sha256: None,
            emitted_event_count: 1,
        })
        .collect();
    let events = local
        .iter()
        .map(|(line, raw, _, kind, text)| {
            let mut event = NormalizedEvent::new(*kind);
            event.text = Some((*text).to_string());
            (
                RawLineRef {
                    file: mirror_payload.clone(),
                    line: *line,
                    sha256: sha256_hex(raw.as_bytes()),
                },
                event,
            )
        })
        .collect();

    GenerationAcceptance {
        project: ProjectRef {
            id: PROJECT_ID.to_string(),
            root_path: ROOT_HINT.to_string(),
            source: Source::Codex,
            display_name: Some("generation-integration".to_string()),
        },
        project_identity: SourceProjectIdentity {
            project_id: PROJECT_ID.to_string(),
            owner_key: OWNER_KEY.to_string(),
            source: Source::Codex.as_str().to_string(),
            source_project_key: SOURCE_PROJECT_KEY.to_string(),
            normalized_root_hint: Some(ROOT_HINT.to_string()),
        },
        session: SessionRef {
            id: SESSION_ID.to_string(),
            project_id: PROJECT_ID.to_string(),
            file_path: mirror_payload,
            started_at: Some("2026-08-01T00:00:00Z".to_string()),
            raw_line_count: total_envelopes,
            raw_digest: Some(sealed.payload_sha256.clone()),
        },
        session_identity: SourceSessionIdentity {
            session_id: SESSION_ID.to_string(),
            project_id: PROJECT_ID.to_string(),
            owner_key: OWNER_KEY.to_string(),
            source: Source::Codex.as_str().to_string(),
            source_session_key: SOURCE_SESSION_KEY.to_string(),
            source_locator: Some(source.to_string_lossy().into_owned()),
        },
        generation: AcceptedGeneration {
            id: sealed.generation_key.clone(),
            project_id: PROJECT_ID.to_string(),
            session_id: SESSION_ID.to_string(),
            predecessor_id: predecessor.clone(),
            mirror_payload_path: sealed.relative_payload.clone(),
            mirror_seal_path: sealed.relative_seal.clone(),
            payload_sha256: sealed.payload_sha256.clone(),
            payload_bytes: sealed.payload_bytes,
            seal_sha256: sealed.seal_sha256.clone(),
            inherited_prefix_bytes,
            total_envelopes,
            local_envelopes: local.len() as u64,
            local_events: local.len() as u64,
            accepted_at: if predecessor.is_some() {
                "2026-08-01T00:01:00Z".to_string()
            } else {
                "2026-08-01T00:00:00Z".to_string()
            },
        },
        expected_predecessor: predecessor,
        dispositions,
        events,
    }
}

fn table_count(ledger: &Ledger, table: &str) -> i64 {
    ledger
        .conn()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .expect("count generation transaction table")
}

fn event_ids(ledger: &Ledger) -> Vec<String> {
    let mut statement = ledger
        .conn()
        .prepare("SELECT id FROM events WHERE session_id=?1 ORDER BY seq")
        .expect("prepare event id query");
    statement
        .query_map([SESSION_ID], |row| row.get(0))
        .expect("query event ids")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect event ids")
}

fn event_provenance(ledger: &Ledger) -> Vec<(String, String, u64)> {
    let mut statement = ledger
        .conn()
        .prepare(
            "SELECT e.id, p.generation_id, p.raw_line
             FROM events e JOIN event_generation_provenance p ON p.event_id=e.id
             WHERE e.session_id=?1 ORDER BY e.seq",
        )
        .expect("prepare event provenance query");
    statement
        .query_map([SESSION_ID], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get::<_, i64>(2)?.max(0) as u64,
            ))
        })
        .expect("query event provenance")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect event provenance")
}

const FAILPOINTS: [GenerationCommitFailpoint; 5] = [
    GenerationCommitFailpoint::AfterGeneration,
    GenerationCommitFailpoint::AfterDispositions,
    GenerationCommitFailpoint::AfterEvents,
    GenerationCommitFailpoint::BeforeHead,
    GenerationCommitFailpoint::BeforeCommit,
];

#[test]
fn every_initial_commit_failpoint_rolls_back_identity_events_and_authority() {
    let lineage = CapturedLineage::new();
    let first = lineage.first_acceptance();

    for failpoint in FAILPOINTS {
        let mut ledger = Ledger::open_in_memory().expect("fresh ledger");
        let error = ledger
            .accept_source_generation_with_failpoint(&first, Some(failpoint))
            .expect_err("injected transaction failure must abort acceptance");
        assert!(
            error
                .to_string()
                .contains("injected generation transaction failure"),
            "unexpected error at {failpoint:?}: {error}"
        );

        assert_eq!(
            ledger.project_ids().expect("projects"),
            Vec::<String>::new()
        );
        assert_eq!(
            ledger
                .source_project_id(OWNER_KEY, Source::Codex.as_str(), SOURCE_PROJECT_KEY)
                .expect("project identity lookup"),
            None
        );
        assert_eq!(
            ledger
                .source_session_id(OWNER_KEY, Source::Codex.as_str(), SOURCE_SESSION_KEY)
                .expect("session identity lookup"),
            None
        );
        assert_eq!(
            ledger
                .accepted_generation(SESSION_ID)
                .expect("generation head lookup"),
            None
        );
        assert_eq!(
            ledger.session_state(SESSION_ID).expect("session state"),
            Default::default()
        );
        for table in [
            "projects",
            "source_project_identities",
            "sessions",
            "source_sessions",
            "session_generations",
            "generation_raw_dispositions",
            "events",
            "event_generation_provenance",
            "session_generation_heads",
            "ledger_meta",
        ] {
            assert_eq!(
                table_count(&ledger, table),
                0,
                "{table} leaked a row after {failpoint:?}"
            );
        }
    }
}

#[test]
fn every_successor_commit_failpoint_preserves_the_complete_g1_authority() {
    let lineage = CapturedLineage::new();
    let first = lineage.first_acceptance();
    let successor = lineage.successor_acceptance();
    let mut ledger = Ledger::open_in_memory().expect("ledger");
    ledger
        .accept_source_generation(&first)
        .expect("accept G1 atomically");

    let accepted_before = ledger
        .accepted_generation(SESSION_ID)
        .expect("G1 head lookup")
        .expect("G1 is authoritative");
    let chain_before = ledger.session_chain(SESSION_ID).expect("G1 chain");
    let state_before = ledger.session_state(SESSION_ID).expect("G1 state");
    let digests_before = ledger.session_digests(PROJECT_ID).expect("G1 digest");
    let project_head_before = ledger
        .compute_project_head(PROJECT_ID)
        .expect("G1 project head");
    let event_ids_before = event_ids(&ledger);
    let generation_rows_before = table_count(&ledger, "session_generations");
    let disposition_rows_before = table_count(&ledger, "generation_raw_dispositions");
    let provenance_rows_before = table_count(&ledger, "event_generation_provenance");

    for failpoint in FAILPOINTS {
        let error = ledger
            .accept_source_generation_with_failpoint(&successor, Some(failpoint))
            .expect_err("failed G2 must have zero authority");
        assert!(
            error
                .to_string()
                .contains("injected generation transaction failure"),
            "unexpected error at {failpoint:?}: {error}"
        );

        assert_eq!(
            ledger
                .accepted_generation(SESSION_ID)
                .expect("generation head after failed G2"),
            Some(accepted_before.clone()),
            "{failpoint:?} moved or revoked the accepted G1 head"
        );
        assert_eq!(
            ledger
                .session_chain(SESSION_ID)
                .expect("chain after failed G2"),
            chain_before,
            "{failpoint:?} rewrote or appended the G1 event chain"
        );
        assert_eq!(
            ledger
                .session_state(SESSION_ID)
                .expect("state after failed G2"),
            state_before,
            "{failpoint:?} changed the authoritative session state"
        );
        assert_eq!(
            ledger
                .session_digests(PROJECT_ID)
                .expect("digest after failed G2"),
            digests_before,
            "{failpoint:?} published G2 whole-file metadata"
        );
        assert_eq!(
            ledger
                .compute_project_head(PROJECT_ID)
                .expect("project head after failed G2"),
            project_head_before,
            "{failpoint:?} changed the project event head"
        );
        assert_eq!(event_ids(&ledger), event_ids_before);
        assert_eq!(
            table_count(&ledger, "session_generations"),
            generation_rows_before
        );
        assert_eq!(
            table_count(&ledger, "generation_raw_dispositions"),
            disposition_rows_before
        );
        assert_eq!(
            table_count(&ledger, "event_generation_provenance"),
            provenance_rows_before
        );
    }
}

#[test]
fn sealed_successor_transaction_failure_has_deterministic_orphan_recovery() {
    let lineage = CapturedLineage::new();
    let first = lineage.first_acceptance();
    let successor = lineage.successor_acceptance();
    let mut ledger = Ledger::open_in_memory().expect("ledger");
    ledger
        .accept_source_generation(&first)
        .expect("accept G1 atomically");
    let g1_head = ledger
        .accepted_generation(SESSION_ID)
        .expect("G1 head")
        .expect("G1 accepted");
    let g1_chain = ledger.session_chain(SESSION_ID).expect("G1 chain");

    ledger
        .accept_source_generation_with_failpoint(
            &successor,
            Some(GenerationCommitFailpoint::BeforeCommit),
        )
        .expect_err("sealed G2 transaction must fail");

    let lease = lineage
        .store
        .acquire_ingest_lease()
        .expect("recovery lease");
    let accepted = BTreeSet::from([lineage.first.generation_key.clone()]);
    assert_eq!(
        lineage
            .store
            .recover_unaccepted_generations(&lease, &accepted)
            .expect("recover transaction orphan"),
        1
    );
    assert_eq!(
        lineage
            .store
            .recover_unaccepted_generations(&lease, &accepted)
            .expect("repeat recovery is idempotent"),
        0
    );
    assert!(lineage.store.verify(&lineage.first).is_ok());
    assert!(
        lineage.store.verify(&lineage.successor).is_ok(),
        "one ledger's transaction failure cannot mutate bytes another copy may reference"
    );
    assert_eq!(
        ledger
            .accepted_generation(SESSION_ID)
            .expect("head after recovery"),
        Some(g1_head)
    );
    assert_eq!(
        ledger
            .session_chain(SESSION_ID)
            .expect("chain after recovery"),
        g1_chain
    );
}

#[test]
fn final_continuity_failure_rolls_back_after_all_candidate_rows_are_staged() {
    let lineage = CapturedLineage::new();
    let first = lineage.first_acceptance();
    let successor = lineage.successor_acceptance();
    let mut ledger = Ledger::open_in_memory().expect("ledger");
    ledger
        .accept_source_generation(&first)
        .expect("accept G1 atomically");
    let accepted_before = ledger.accepted_generation(SESSION_ID).expect("G1");
    let chain_before = ledger.session_chain(SESSION_ID).expect("G1 chain");
    let generation_count = table_count(&ledger, "session_generations");

    let error = ledger
        .accept_source_generation_with_final_check(&successor, || {
            Err(CoreError::Integrity(
                "synthetic live source changed at final check".to_string(),
            ))
        })
        .expect_err("final continuity failure must roll back G2");
    assert!(error.to_string().contains("final check"));
    assert_eq!(
        ledger.accepted_generation(SESSION_ID).expect("head"),
        accepted_before
    );
    assert_eq!(
        ledger.session_chain(SESSION_ID).expect("chain"),
        chain_before
    );
    assert_eq!(
        table_count(&ledger, "session_generations"),
        generation_count
    );
    assert_eq!(table_count(&ledger, "codex_message_slots"), 0);
    assert_eq!(table_count(&ledger, "codex_reconciliation_receipts"), 0);
}

#[test]
fn reconciliation_slots_and_receipts_share_the_generation_transaction() {
    let lineage = CapturedLineage::new();
    let acceptance = lineage.reconciled_first_acceptance();

    for failpoint in FAILPOINTS {
        let mut ledger = Ledger::open_in_memory().expect("ledger");
        ledger
            .accept_source_generation_with_failpoint(&acceptance, Some(failpoint))
            .expect_err("fault must roll back reconciliation authority");
        assert_eq!(table_count(&ledger, "codex_message_slots"), 0);
        assert_eq!(table_count(&ledger, "codex_reconciliation_receipts"), 0);
        assert_eq!(table_count(&ledger, "events"), 0);
        assert_eq!(table_count(&ledger, "session_generations"), 0);
    }

    let mut ledger = Ledger::open_in_memory().expect("ledger");
    ledger
        .accept_source_generation(&acceptance)
        .expect("complete reconciliation commits");
    assert_eq!(table_count(&ledger, "codex_message_slots"), 1);
    assert_eq!(table_count(&ledger, "codex_reconciliation_receipts"), 1);
    assert!(ledger
        .available_codex_message_slots(SESSION_ID)
        .expect("available slots")
        .is_empty());
}

#[test]
fn prefix_successor_appends_only_g2_events_and_advances_both_heads() {
    let lineage = CapturedLineage::new();
    let first = lineage.first_acceptance();
    let successor = lineage.successor_acceptance();
    let mut ledger = Ledger::open_in_memory().expect("ledger");

    let g1_events = ledger
        .accept_source_generation(&first)
        .expect("accept G1 atomically");
    assert_eq!(g1_events.len(), 2);
    assert_eq!(
        g1_events.iter().map(|event| event.seq).collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_eq!(
        ledger
            .accepted_generation(SESSION_ID)
            .expect("G1 head")
            .expect("G1 accepted"),
        first.generation
    );
    let g1_chain = ledger.session_chain(SESSION_ID).expect("G1 chain");
    let g1_ids = event_ids(&ledger);
    let (g1_project_head, g1_count) = ledger.compute_project_head(PROJECT_ID).expect("G1 head");
    assert_eq!(g1_count, 2);

    let g2_events = ledger
        .accept_source_generation(&successor)
        .expect("accept prefix-preserving G2 atomically");
    assert_eq!(g2_events.len(), 1, "only the G2 suffix may emit");
    assert_eq!(g2_events[0].seq, 3);
    assert_eq!(g2_events[0].raw.line, 3);
    assert_eq!(g2_events[0].prev_hash, g1_chain[1].chain_hash);
    assert_eq!(
        g2_events[0].raw.file,
        lineage
            .store
            .resolve_payload(&lineage.successor)
            .expect("G2 mirror payload")
            .to_string_lossy()
    );

    let accepted = ledger
        .accepted_generation(SESSION_ID)
        .expect("G2 head lookup")
        .expect("G2 is authoritative");
    assert_eq!(accepted, successor.generation);
    assert_eq!(
        accepted.predecessor_id.as_deref(),
        Some(first.generation.id.as_str())
    );

    let g2_chain = ledger.session_chain(SESSION_ID).expect("G2 chain");
    assert_eq!(g2_chain.len(), 3);
    assert_eq!(
        &g2_chain[..g1_chain.len()],
        g1_chain.as_slice(),
        "G2 must not rewrite G1 chain links or mirror receipts"
    );
    assert_eq!(
        &event_ids(&ledger)[..g1_ids.len()],
        g1_ids.as_slice(),
        "G1 deterministic event ids changed during G2"
    );

    let provenance = event_provenance(&ledger);
    assert_eq!(
        provenance,
        vec![
            (g1_ids[0].clone(), first.generation.id.clone(), 1),
            (g1_ids[1].clone(), first.generation.id.clone(), 2),
            (g2_events[0].id.clone(), successor.generation.id.clone(), 3,),
        ],
        "each event must retain the generation and raw envelope that authorized it"
    );

    let state = ledger.session_state(SESSION_ID).expect("G2 session state");
    assert_eq!(state.last_seq, 3);
    assert_eq!(state.event_count, 3);
    assert_eq!(
        state.head_hash.as_deref(),
        Some(g2_chain[2].chain_hash.as_str())
    );
    let (g2_project_head, g2_count) = ledger.compute_project_head(PROJECT_ID).expect("G2 head");
    assert_eq!(g2_count, 3);
    assert_ne!(g2_project_head, g1_project_head);
    assert_eq!(table_count(&ledger, "session_generations"), 2);
    assert_eq!(table_count(&ledger, "generation_raw_dispositions"), 3);
    assert_eq!(table_count(&ledger, "event_generation_provenance"), 3);
}

#[test]
fn accepted_codex_session_cannot_bypass_generation_provenance() {
    let lineage = CapturedLineage::new();
    let first = lineage.first_acceptance();
    let mut ledger = Ledger::open_in_memory().expect("ledger");
    ledger
        .accept_source_generation(&first)
        .expect("accept first generation");
    let state_before = ledger.session_state(SESSION_ID).expect("state");
    let raw = RawLineRef {
        file: "unsealed-live-source".to_string(),
        line: 3,
        sha256: sha256_hex(THIRD_LINE.as_bytes()),
    };
    let event = NormalizedEvent::new(EventKind::AssistantText);
    assert!(ledger
        .append_session_events(PROJECT_ID, SESSION_ID, &[(raw, event)])
        .is_err());
    assert_eq!(
        ledger.session_state(SESSION_ID).expect("state after"),
        state_before
    );
}
