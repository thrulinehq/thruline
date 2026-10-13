mod scratch;

use std::fs;
use std::path::{Path, PathBuf};

use scratch::TempTree;
use thruline_core::adapter::{stable_source_project_id, stable_source_session_id};
use thruline_core::ingest::{
    ingest_codex, ingest_codex_with_prepare_failpoint, verify, CodexIngestOptions,
};
use thruline_core::{Ledger, Source};

const OWNER: &str = "synthetic-owner";
const ROOT: &str = "/synthetic/shared-root";
const SESSION: &str = "synthetic-session";

struct Scratch {
    _tmp: TempTree,
    db: PathBuf,
    source: PathBuf,
    mirror: PathBuf,
}

impl Scratch {
    fn new() -> Self {
        let tmp = TempTree::new();
        Self {
            db: tmp.path().join("ledger.db"),
            source: tmp.path().join("source").join("rollout.jsonl"),
            mirror: tmp.path().join("private-mirror"),
            _tmp: tmp,
        }
    }

    fn write(&self, bytes: &[u8]) {
        fs::create_dir_all(self.source.parent().expect("source parent"))
            .expect("create source parent");
        fs::write(&self.source, bytes).expect("write source");
    }

    fn ingest_path(&self, path: &Path) -> thruline_core::ingest::IngestReport {
        let mut ledger = Ledger::open(&self.db).expect("open ledger");
        ingest_codex(
            &mut ledger,
            &[path.to_path_buf()],
            &CodexIngestOptions {
                mirror_root: self.mirror.clone(),
                owner_key: Some(OWNER.to_string()),
            },
        )
        .expect("Codex ingest")
    }

    fn ingest(&self) -> thruline_core::ingest::IngestReport {
        self.ingest_path(&self.source)
    }

    fn ledger(&self) -> Ledger {
        Ledger::open(&self.db).expect("open ledger")
    }
}

fn g1() -> Vec<u8> {
    format!(
        concat!(
            "{{\"timestamp\":\"2026-08-01T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{SESSION}\",\"cwd\":\"{ROOT}\"}}}}\n",
            "{{\"timestamp\":\"2026-08-01T00:00:01Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"id\":\"m1\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":\"Synthetic answer\"}}]}}}}\n",
            "{{\"timestamp\":\"2026-08-01T00:00:02Z\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"agent_message\",\"message\":\"Synthetic answer\"}}}}\n"
        ),
        SESSION = SESSION,
        ROOT = ROOT,
    )
    .into_bytes()
}

fn valid_suffix() -> &'static [u8] {
    b"{\"timestamp\":\"2026-08-01T00:00:03Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"m2\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"A later answer\"}]}}\n"
}

fn ids() -> (String, String) {
    (
        stable_source_project_id(OWNER, Source::Codex, ROOT),
        stable_source_session_id(OWNER, Source::Codex, SESSION),
    )
}

#[test]
fn first_middle_and_final_envelope_failures_never_publish_partial_authority() {
    for fail_at_raw_line in [1, 2, 3] {
        let scratch = Scratch::new();
        scratch.write(&g1());
        let mut ledger = scratch.ledger();
        let failed = ingest_codex_with_prepare_failpoint(
            &mut ledger,
            std::slice::from_ref(&scratch.source),
            &CodexIngestOptions {
                mirror_root: scratch.mirror.clone(),
                owner_key: Some(OWNER.to_string()),
            },
            fail_at_raw_line,
        )
        .expect("deterministic parser failure is disposition-accounted");
        assert_eq!(failed.accepted_generations, 0, "line {fail_at_raw_line}");
        assert_eq!(failed.new_events, 0, "line {fail_at_raw_line}");
        assert_eq!(failed.rejected_candidates, 1, "line {fail_at_raw_line}");
        for table in [
            "events",
            "session_generations",
            "session_generation_heads",
            "generation_raw_dispositions",
            "event_generation_provenance",
        ] {
            let rows: i64 = ledger
                .conn()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .expect("authority table count");
            assert_eq!(rows, 0, "{table} leaked after line {fail_at_raw_line}");
        }
        drop(ledger);

        let retry = scratch.ingest();
        assert_eq!(retry.accepted_generations, 1, "line {fail_at_raw_line}");
        assert!(retry.new_events > 0, "line {fail_at_raw_line}");
        let unchanged = scratch.ingest();
        assert_eq!(unchanged.new_events, 0, "line {fail_at_raw_line}");
        assert_eq!(
            unchanged.identical_observations, 1,
            "line {fail_at_raw_line}"
        );
        assert!(verify(&scratch.ledger(), None).expect("verify retry").ok());
    }
}

#[test]
fn scratch_ledger_cannot_recover_another_ledgers_accepted_mirror() {
    let tmp = TempTree::new();
    let source = tmp.path().join("source").join("rollout.jsonl");
    let mirror_base = tmp.path().join("shared-private-mirror");
    fs::create_dir_all(source.parent().expect("source parent")).expect("source parent");
    fs::write(&source, g1()).expect("write G1");

    let db_a = tmp.path().join("ledger-a.db");
    let mut ledger_a = Ledger::open(&db_a).expect("ledger A");
    let authority_a = ledger_a.mirror_authority_key().expect("ledger A authority");
    let accepted = ingest_codex(
        &mut ledger_a,
        std::slice::from_ref(&source),
        &CodexIngestOptions {
            mirror_root: mirror_base.clone(),
            owner_key: Some(OWNER.to_string()),
        },
    )
    .expect("ledger A ingest");
    assert_eq!(accepted.accepted_generations, 1);
    let (project_id, session_id) = ids();
    let generation_a = ledger_a
        .accepted_generation(&session_id)
        .expect("ledger A generation")
        .expect("accepted G1");
    let chain_a = ledger_a.session_chain(&session_id).expect("ledger A chain");
    let head_a = ledger_a
        .compute_project_head(&project_id)
        .expect("ledger A head");
    let namespace_a = mirror_base.join("ledgers").join(&authority_a);
    drop(ledger_a);

    let db_b = tmp.path().join("ledger-b.db");
    let mut ledger_b = Ledger::open(&db_b).expect("ledger B");
    let authority_b = ledger_b.mirror_authority_key().expect("ledger B authority");
    assert_ne!(authority_a, authority_b);
    let accepted_b = ingest_codex(
        &mut ledger_b,
        std::slice::from_ref(&source),
        &CodexIngestOptions {
            mirror_root: mirror_base.clone(),
            owner_key: Some(OWNER.to_string()),
        },
    )
    .expect("ledger B ingest");
    assert_eq!(accepted_b.accepted_generations, 1);
    drop(ledger_b);

    let ledger_a = Ledger::open(&db_a).expect("reopen ledger A");
    assert_eq!(
        ledger_a
            .accepted_generation(&session_id)
            .expect("ledger A generation after B"),
        Some(generation_a)
    );
    assert_eq!(
        ledger_a
            .session_chain(&session_id)
            .expect("ledger A chain after B"),
        chain_a
    );
    assert_eq!(
        ledger_a
            .compute_project_head(&project_id)
            .expect("ledger A head after B"),
        head_a
    );
    let verified = verify(&ledger_a, None).expect("verify ledger A after ledger B recovery");
    assert!(verified.projects[&project_id].ok());
    assert!(namespace_a.join("generations").exists());
    assert!(namespace_a
        .join("orphans")
        .read_dir()
        .expect("A orphans")
        .next()
        .is_none());
}

#[test]
fn copied_ledger_cannot_move_a_generation_accepted_by_the_primary_copy() {
    let scratch = Scratch::new();
    scratch.write(&g1());
    assert_eq!(scratch.ingest().accepted_generations, 1);
    {
        let ledger = scratch.ledger();
        ledger
            .conn()
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .expect("checkpoint G1 before copying the ledger");
    }
    let copied_db = scratch.db.with_file_name("ledger-copy.db");
    fs::copy(&scratch.db, &copied_db).expect("copy ledger at G1");

    let mut g2 = g1();
    g2.extend_from_slice(valid_suffix());
    scratch.write(&g2);
    assert_eq!(scratch.ingest().accepted_generations, 1);
    let (project_id, session_id) = ids();
    let primary = scratch.ledger();
    let primary_generation = primary
        .accepted_generation(&session_id)
        .expect("primary generation")
        .expect("primary G2");
    let primary_chain = primary.session_chain(&session_id).expect("primary chain");
    let primary_head = primary
        .compute_project_head(&project_id)
        .expect("primary head");
    drop(primary);

    let mut copied = Ledger::open(&copied_db).expect("open copied G1 ledger");
    let copy_report = ingest_codex(
        &mut copied,
        std::slice::from_ref(&scratch.source),
        &CodexIngestOptions {
            mirror_root: scratch.mirror.clone(),
            owner_key: Some(OWNER.to_string()),
        },
    )
    .expect("copied ledger observes and accepts G2 without moving it");
    assert_eq!(copy_report.accepted_generations, 1);
    assert!(verify(&copied, None).expect("verify copied ledger").ok());
    drop(copied);

    let primary = scratch.ledger();
    assert_eq!(
        primary
            .accepted_generation(&session_id)
            .expect("primary G2 after copied-ledger recovery"),
        Some(primary_generation)
    );
    assert_eq!(
        primary
            .session_chain(&session_id)
            .expect("primary chain after copy"),
        primary_chain
    );
    assert_eq!(
        primary
            .compute_project_head(&project_id)
            .expect("primary head after copy"),
        primary_head
    );
    assert!(verify(&primary, None)
        .expect("verify primary after copy")
        .ok());
}

#[test]
fn g1_identical_and_prefix_g2_are_exactly_once() {
    let scratch = Scratch::new();
    scratch.write(&g1());
    let first = scratch.ingest();
    assert_eq!(first.accepted_generations, 1);
    assert_eq!(first.suppressed_mirrors, 1);
    let (project_id, session_id) = ids();
    let ledger = scratch.ledger();
    let g1_head = ledger
        .accepted_generation(&session_id)
        .expect("generation")
        .expect("G1");
    let g1_chain = ledger.session_chain(&session_id).expect("G1 chain");
    let g1_project_head = ledger.compute_project_head(&project_id).expect("head");
    drop(ledger);

    let identical = scratch.ingest();
    assert_eq!(identical.identical_observations, 1);
    assert_eq!(identical.new_events, 0);
    let ledger = scratch.ledger();
    assert_eq!(
        ledger.accepted_generation(&session_id).expect("generation"),
        Some(g1_head.clone())
    );
    assert_eq!(ledger.session_chain(&session_id).expect("chain"), g1_chain);
    assert_eq!(
        ledger.compute_project_head(&project_id).expect("head"),
        g1_project_head
    );
    drop(ledger);

    let mut g2 = g1();
    g2.extend_from_slice(valid_suffix());
    scratch.write(&g2);
    let successor = scratch.ingest();
    assert_eq!(successor.accepted_generations, 1);
    assert_eq!(successor.new_events, 1);
    let ledger = scratch.ledger();
    let accepted = ledger
        .accepted_generation(&session_id)
        .expect("generation")
        .expect("G2");
    assert_eq!(
        accepted.predecessor_id.as_deref(),
        Some(g1_head.id.as_str())
    );
    assert_eq!(accepted.local_envelopes, 1);
    assert_eq!(accepted.local_events, 1);
    let chain = ledger.session_chain(&session_id).expect("chain");
    assert_eq!(&chain[..g1_chain.len()], &g1_chain);
    assert_eq!(chain.len(), g1_chain.len() + 1);
}

#[test]
fn malformed_rewrite_and_truncation_never_revoke_g1() {
    for mutation in ["malformed", "rewrite", "truncate", "ambiguous_route"] {
        let scratch = Scratch::new();
        let original = g1();
        scratch.write(&original);
        assert_eq!(scratch.ingest().accepted_generations, 1);
        let (project_id, session_id) = ids();
        let ledger = scratch.ledger();
        let accepted_before = ledger.accepted_generation(&session_id).expect("G1");
        let chain_before = ledger.session_chain(&session_id).expect("chain");
        let head_before = ledger.compute_project_head(&project_id).expect("head");
        drop(ledger);

        match mutation {
            "malformed" => {
                let mut bytes = original.clone();
                bytes.extend_from_slice(b"{malformed\n");
                scratch.write(&bytes);
            }
            "rewrite" => {
                let rewritten = String::from_utf8(original.clone())
                    .expect("utf8")
                    .replace("Synthetic answer", "Rewritten answer");
                scratch.write(rewritten.as_bytes());
            }
            "truncate" => scratch.write(&original[..original.len() / 2]),
            "ambiguous_route" => {
                let mut bytes = original.clone();
                bytes.extend_from_slice(
                    b"{\"type\":\"turn_context\",\"payload\":{\"turn_id\":\"other\",\"cwd\":\"/synthetic/other-root\"}}\n",
                );
                scratch.write(&bytes);
            }
            _ => unreachable!(),
        }
        let rejected = scratch.ingest();
        assert_eq!(rejected.rejected_candidates, 1, "{mutation}");
        assert_eq!(rejected.new_events, 0, "{mutation}");
        let ledger = scratch.ledger();
        if mutation == "ambiguous_route" {
            let linked: i64 = ledger
                .conn()
                .query_row(
                    "SELECT COUNT(*) FROM candidate_assessments
                     WHERE status='continuity_unknown'
                       AND predecessor_generation_id IS NOT NULL",
                    [],
                    |row| row.get(0),
                )
                .expect("linked continuity assessment");
            assert_eq!(
                linked, 1,
                "ambiguous suffix must retain predecessor linkage"
            );
        }
        assert_eq!(
            ledger.accepted_generation(&session_id).expect("G1 after"),
            accepted_before,
            "{mutation}"
        );
        assert_eq!(
            ledger.session_chain(&session_id).expect("chain"),
            chain_before
        );
        assert_eq!(
            ledger.compute_project_head(&project_id).expect("head"),
            head_before
        );
    }
}

#[test]
fn bad_successor_does_not_block_a_later_valid_successor() {
    let scratch = Scratch::new();
    let original = g1();
    scratch.write(&original);
    assert_eq!(scratch.ingest().accepted_generations, 1);

    let mut bad = original.clone();
    bad.extend_from_slice(b"{bad\n");
    scratch.write(&bad);
    assert_eq!(scratch.ingest().rejected_candidates, 1);

    let mut valid = original;
    valid.extend_from_slice(valid_suffix());
    scratch.write(&valid);
    let accepted = scratch.ingest();
    assert_eq!(accepted.accepted_generations, 1);
    assert_eq!(accepted.new_events, 1);
    let (_, session_id) = ids();
    let ledger = scratch.ledger();
    assert_eq!(
        ledger
            .accepted_generation(&session_id)
            .expect("head")
            .expect("G2")
            .local_events,
        1
    );
}

#[test]
fn accepted_mirror_and_receipts_survive_source_move_and_disappearance() {
    let scratch = Scratch::new();
    scratch.write(&g1());
    assert_eq!(scratch.ingest().accepted_generations, 1);
    let moved = scratch.source.parent().expect("parent").join("moved.jsonl");
    fs::rename(&scratch.source, &moved).expect("move live source");
    let identical = scratch.ingest_path(&moved);
    assert_eq!(identical.identical_observations, 1);
    fs::remove_file(&moved).expect("remove moved live source");

    let ledger = scratch.ledger();
    let verified = verify(&ledger, None).expect("verify from mirrors");
    assert!(verified.ok(), "{verified:#?}");
    let (_, session_id) = ids();
    let generation = ledger
        .accepted_generation(&session_id)
        .expect("generation")
        .expect("accepted");
    assert!(!generation.mirror_payload_path.is_empty());
}

#[test]
fn exact_native_replay_is_raw_accounted_without_a_duplicate_event() {
    let scratch = Scratch::new();
    scratch.write(&g1());
    assert_eq!(scratch.ingest().accepted_generations, 1);
    let (_, session_id) = ids();
    let chain_before = scratch
        .ledger()
        .session_chain(&session_id)
        .expect("G1 chain");

    let mut successor = g1();
    successor.extend_from_slice(
        b"{\"timestamp\":\"2026-08-01T00:00:09Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"m1\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Synthetic answer\"}]}}\n",
    );
    scratch.write(&successor);
    let replay = scratch.ingest();
    assert_eq!(replay.accepted_generations, 1);
    assert_eq!(replay.suppressed_replays, 1);
    assert_eq!(replay.new_events, 0);
    let ledger = scratch.ledger();
    assert_eq!(
        ledger.session_chain(&session_id).expect("G2 chain"),
        chain_before
    );
    let head = ledger
        .accepted_generation(&session_id)
        .expect("head")
        .expect("G2");
    assert_eq!(head.local_envelopes, 1);
    assert_eq!(head.local_events, 0);
}

#[test]
fn prior_fallback_never_erases_a_later_canonical_and_delayed_mirror_consumes_it_once() {
    let scratch = Scratch::new();
    let first = format!(
        concat!(
            "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{SESSION}\",\"cwd\":\"{ROOT}\"}}}}\n",
            "{{\"type\":\"turn_context\",\"payload\":{{\"turn_id\":\"turn-a\",\"cwd\":\"{ROOT}\"}}}}\n",
            "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"agent_message\",\"message\":\"Repeated synthetic text\"}}}}\n"
        ),
        SESSION = SESSION,
        ROOT = ROOT,
    );
    scratch.write(first.as_bytes());
    let g1_report = scratch.ingest();
    assert_eq!(g1_report.fallback_messages, 1);

    let canonical = "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"id\":\"canonical-a\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Repeated synthetic text\"}]}}\n";
    let mut second = first.as_bytes().to_vec();
    second.extend_from_slice(canonical.as_bytes());
    scratch.write(&second);
    let g2_report = scratch.ingest();
    assert_eq!(
        g2_report.new_events, 1,
        "the canonical item is new authority"
    );
    assert_eq!(g2_report.suppressed_mirrors, 0);

    let delayed = "{\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"Repeated synthetic text\"}}\n";
    let mut third = second;
    third.extend_from_slice(delayed.as_bytes());
    scratch.write(&third);
    let g3_report = scratch.ingest();
    assert_eq!(g3_report.new_events, 0);
    assert_eq!(g3_report.suppressed_mirrors, 1);

    let ledger = scratch.ledger();
    let receipts: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM codex_reconciliation_receipts",
            [],
            |row| row.get(0),
        )
        .expect("reconciliation count");
    assert_eq!(
        receipts, 1,
        "only the delayed mirror consumes the canonical slot"
    );
}

#[test]
fn repeated_text_in_a_different_turn_is_not_a_mirror() {
    let scratch = Scratch::new();
    let bytes = format!(
        concat!(
            "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{SESSION}\",\"cwd\":\"{ROOT}\"}}}}\n",
            "{{\"type\":\"turn_context\",\"payload\":{{\"turn_id\":\"turn-a\",\"cwd\":\"{ROOT}\"}}}}\n",
            "{{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"id\":\"a\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":\"Same words\"}}]}}}}\n",
            "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"agent_message\",\"message\":\"Same words\"}}}}\n",
            "{{\"type\":\"turn_context\",\"payload\":{{\"turn_id\":\"turn-b\",\"cwd\":\"{ROOT}\"}}}}\n",
            "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"agent_message\",\"message\":\"Same words\"}}}}\n"
        ),
        SESSION = SESSION,
        ROOT = ROOT,
    );
    scratch.write(bytes.as_bytes());
    let report = scratch.ingest();
    assert_eq!(report.suppressed_mirrors, 1);
    assert_eq!(report.fallback_messages, 1);
    let (_, session_id) = ids();
    let assistant_events: i64 = scratch
        .ledger()
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE session_id=?1 AND kind='assistant_text'",
            [&session_id],
            |row| row.get(0),
        )
        .expect("assistant count");
    assert_eq!(assistant_events, 2);
}

#[test]
fn leading_malformed_row_is_captured_and_disposition_accounted() {
    let scratch = Scratch::new();
    let bytes = format!(
        concat!(
            "{{malformed\n",
            "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{SESSION}\",\"cwd\":\"{ROOT}\"}}}}\n",
            "{{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"id\":\"valid\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":\"Valid after malformed\"}}]}}}}\n"
        ),
        SESSION = SESSION,
        ROOT = ROOT,
    );
    scratch.write(bytes.as_bytes());
    let report = scratch.ingest();
    assert_eq!(report.accepted_generations, 1);
    assert_eq!(report.adapter_malformed, 1);
    assert_eq!(report.dispositions, 3);
    assert_eq!(report.quarantined_envelopes, 0);
}

#[test]
fn divergent_payload_under_the_same_native_id_rejects_only_the_successor() {
    let scratch = Scratch::new();
    scratch.write(&g1());
    assert_eq!(scratch.ingest().accepted_generations, 1);
    let (_, session_id) = ids();
    let ledger = scratch.ledger();
    let head_before = ledger.accepted_generation(&session_id).expect("G1");
    let chain_before = ledger.session_chain(&session_id).expect("chain");
    drop(ledger);

    let mut successor = g1();
    successor.extend_from_slice(
        b"{\"type\":\"response_item\",\"payload\":{\"type\":\"reasoning\",\"id\":\"m1\",\"summary\":[{\"type\":\"summary_text\",\"text\":\"Different payload and type\"}]}}\n",
    );
    scratch.write(&successor);
    let rejected = scratch.ingest();
    assert_eq!(rejected.rejected_candidates, 1);
    assert_eq!(rejected.new_events, 0);
    let ledger = scratch.ledger();
    assert_eq!(
        ledger.accepted_generation(&session_id).expect("G1 after"),
        head_before
    );
    assert_eq!(
        ledger.session_chain(&session_id).expect("chain after"),
        chain_before
    );
}
