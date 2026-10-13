//! A205 (MANDATE-v4 §11 / §33) — the unified observation stream.
//!
//! The golden hash vectors below were computed OUTSIDE the code under test,
//! by hand with python3's hashlib over the documented canonical encoding
//! (§32: never generate expected hashes with the code being tested). If they
//! ever disagree with the implementation, the implementation moved — that is
//! a deliberate versioned migration, never a test update.

use thruline_core::ledger::Ledger;
use thruline_core::observation::{
    append, canonical_bytes, for_session, observation_id, of_type, read_all, verify_chain,
    AppendOutcome, Completeness, DerivedRef, NewObservation, ObservationType, ReceiptTerminal,
    SourceReceipt, SCHEMA_VERSION,
};

/// Vector A's receipt JSON, byte-for-byte as hashed.
const RECEIPT_A: &str = r#"{"terminal":{"kind":"filesystem","path":"/tmp/w","sha256":"ab"}}"#;
/// Vector B's receipt JSON, byte-for-byte as hashed.
const RECEIPT_B: &str =
    r#"{"terminal":{"kind":"process_execution","command":"cargo test","exit_code":0}}"#;

fn vector_a_id() -> String {
    observation_id(
        1,
        "WORKSPACE_OBSERVED",
        "thruline",
        "cli:2026-08-31T12:00:00Z",
        "repo:9f8a",
        Some("ws:main"),
        "2026-08-31T12:00:00Z",
        "complete",
        RECEIPT_A,
    )
}

fn vector_b_id() -> String {
    observation_id(
        1,
        "COMMAND_FINISHED",
        "claude_code",
        "4f5d8b47-ec14-45b7-9a6a-206a18f43417",
        "repo:9f8a",
        None,
        "2026-08-31T12:00:05Z",
        "incomplete",
        RECEIPT_B,
    )
}

#[test]
fn golden_vectors_hand_computed_outside_this_code() {
    // sha256("observation") — the chain's genesis prev-hash. Computed with
    // python3 hashlib, 2026-08-31.
    assert_eq!(
        thruline_core::chain::sha256_hex(b"observation"),
        "772c6953848bf5b19aedf9a34ccb066f31eacca29bdbfcb1b9821765f1060149"
    );

    // Vector A: workspace identity PRESENT (tag byte 0x01 in the encoding).
    let a = canonical_bytes(
        1,
        "WORKSPACE_OBSERVED",
        "thruline",
        "cli:2026-08-31T12:00:00Z",
        "repo:9f8a",
        Some("ws:main"),
        "2026-08-31T12:00:00Z",
        "complete",
        RECEIPT_A,
    );
    assert_eq!(a.len(), 231, "hand-computed canonical length, vector A");
    assert_eq!(
        vector_a_id(),
        "7d52b03b07649481f29306c0f1f5e5589176778c62f8d10d1fb48777790dd091"
    );

    // Vector B: workspace identity ABSENT (tag byte 0x00 — one byte, no
    // length, so None and Some("") can never collide).
    let b = canonical_bytes(
        1,
        "COMMAND_FINISHED",
        "claude_code",
        "4f5d8b47-ec14-45b7-9a6a-206a18f43417",
        "repo:9f8a",
        None,
        "2026-08-31T12:00:05Z",
        "incomplete",
        RECEIPT_B,
    );
    assert_eq!(b.len(), 245, "hand-computed canonical length, vector B");
    assert_eq!(
        vector_b_id(),
        "03f24b572d7c0bee8d7419af0fe0c60b02d3285a8943559ef13d72d418e04cd3"
    );

    // Chain links folded by hand over the same vectors: seq 0 from genesis,
    // seq 1 from seq 0.
    let genesis = "772c6953848bf5b19aedf9a34ccb066f31eacca29bdbfcb1b9821765f1060149";
    let link0 = thruline_core::chain::chain_hash(genesis, &vector_a_id(), "observation", 0);
    assert_eq!(
        link0,
        "b054ac5bad454ba145935ce5f4f2b12f74dd20d43a41e6f1f41bf902c582f5d8"
    );
    let link1 = thruline_core::chain::chain_hash(&link0, &vector_b_id(), "observation", 1);
    assert_eq!(
        link1,
        "b398f5e8dce7b982d564cb8d5bab7ac5963ed98a37e688e55222eae73e93c26c"
    );
}

/// The append path must produce exactly the hand-computed chain when fed the
/// vector content — the store and the vectors are one contract.
#[test]
fn append_reproduces_the_hand_computed_chain_and_reads_back_the_envelope() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let receipt_a = SourceReceipt::from_json(RECEIPT_A).expect("receipt A parses");
    let receipt_b = SourceReceipt::from_json(RECEIPT_B).expect("receipt B parses");
    assert_eq!(
        receipt_a.to_canonical_json().expect("canonical A"),
        RECEIPT_A,
        "the typed receipt re-serializes to the hashed bytes"
    );

    let out_a = append(
        &ledger,
        &NewObservation {
            observation_type: ObservationType::WorkspaceObserved,
            source_adapter: "thruline",
            source_session: "cli:2026-08-31T12:00:00Z",
            repository_identity: "repo:9f8a",
            workspace_identity: Some("ws:main"),
            observed_at: "2026-08-31T12:00:00Z",
            completeness: Completeness::Complete,
            receipt: &receipt_a,
        },
    )
    .expect("append A");
    let AppendOutcome::Appended(rec_a) = out_a else {
        panic!("first append must be new");
    };
    assert_eq!(rec_a.seq, 0);
    assert_eq!(rec_a.id, vector_a_id());
    assert_eq!(
        rec_a.chain_hash, "b054ac5bad454ba145935ce5f4f2b12f74dd20d43a41e6f1f41bf902c582f5d8",
        "the stored link matches the hand-computed vector"
    );

    let out_b = append(
        &ledger,
        &NewObservation {
            observation_type: ObservationType::CommandFinished,
            source_adapter: "claude_code",
            source_session: "4f5d8b47-ec14-45b7-9a6a-206a18f43417",
            repository_identity: "repo:9f8a",
            workspace_identity: None,
            observed_at: "2026-08-31T12:00:05Z",
            completeness: Completeness::Incomplete,
            receipt: &receipt_b,
        },
    )
    .expect("append B");
    let rec_b = out_b.record();
    assert_eq!(rec_b.seq, 1);
    assert_eq!(
        rec_b.chain_hash,
        "b398f5e8dce7b982d564cb8d5bab7ac5963ed98a37e688e55222eae73e93c26c"
    );

    // The envelope reads back whole — every §11 field.
    let rows = read_all(&ledger).expect("read");
    assert_eq!(rows.len(), 2);
    let a = &rows[0];
    assert_eq!(a.schema_version, SCHEMA_VERSION);
    assert_eq!(
        a.observation_type(),
        Some(ObservationType::WorkspaceObserved)
    );
    assert_eq!(a.source_adapter, "thruline");
    assert_eq!(a.source_session, "cli:2026-08-31T12:00:00Z");
    assert_eq!(a.repository_identity, "repo:9f8a");
    assert_eq!(a.workspace_identity.as_deref(), Some("ws:main"));
    assert_eq!(a.observed_at, "2026-08-31T12:00:00Z");
    assert_eq!(a.completeness(), Some(Completeness::Complete));
    assert_eq!(a.source_receipt, RECEIPT_A);
    assert!(rows[1].workspace_identity.is_none());

    // Typed readers narrow without reordering.
    let by_session = for_session(&ledger, "4f5d8b47-ec14-45b7-9a6a-206a18f43417").expect("session");
    assert_eq!(by_session.len(), 1);
    assert_eq!(by_session[0].seq, 1);
    let by_type = of_type(&ledger, ObservationType::WorkspaceObserved).expect("type");
    assert_eq!(by_type.len(), 1);
    assert_eq!(by_type[0].id, vector_a_id());

    assert_eq!(verify_chain(&ledger).expect("verify"), 2);
}

#[test]
fn duplicate_append_is_idempotent_by_content_identity() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let receipt = SourceReceipt::new(ReceiptTerminal::Transcript {
        file: "/tmp/t.jsonl".into(),
        line: 7,
    });
    let new = NewObservation {
        observation_type: ObservationType::EvidenceBound,
        source_adapter: "thruline",
        source_session: "sess-1",
        repository_identity: "repo:x",
        workspace_identity: None,
        observed_at: "2026-08-31T13:00:00Z",
        completeness: Completeness::Complete,
        receipt: &receipt,
    };
    let first = append(&ledger, &new).expect("first");
    assert!(matches!(first, AppendOutcome::Appended(_)));
    let second = append(&ledger, &new).expect("second");
    let AppendOutcome::AlreadyRecorded(existing) = second else {
        panic!("identical content must be a no-op, not a second link");
    };
    assert_eq!(existing.seq, 0);
    assert_eq!(read_all(&ledger).expect("read").len(), 1);
    assert_eq!(verify_chain(&ledger).expect("verify"), 1);
}

/// Stable-ID collision surface: every content field, changed alone, must
/// move the id — including the fields inside the length-prefixed boundaries.
#[test]
fn distinct_content_yields_distinct_ids() {
    let base = || {
        (
            1i64,
            "WORKSPACE_OBSERVED",
            "thruline",
            "sess",
            "repo",
            Some("ws"),
            "2026-08-31T12:00:00Z",
            "complete",
            r#"{"terminal":{"kind":"snapshot_manifest","manifest_sha256":"aa"}}"#,
        )
    };
    type Content<'a> = (
        i64,
        &'a str,
        &'a str,
        &'a str,
        &'a str,
        Option<&'a str>,
        &'a str,
        &'a str,
        &'a str,
    );
    fn id_of(t: Content<'_>) -> String {
        observation_id(t.0, t.1, t.2, t.3, t.4, t.5, t.6, t.7, t.8)
    }

    let reference = id_of(base());
    let mut variants = Vec::new();
    let mut v = base();
    v.0 = 2;
    variants.push(id_of(v));
    let mut v = base();
    v.1 = "COMMAND_STARTED";
    variants.push(id_of(v));
    let mut v = base();
    v.2 = "codex";
    variants.push(id_of(v));
    let mut v = base();
    v.3 = "sess2";
    variants.push(id_of(v));
    let mut v = base();
    v.4 = "repo2";
    variants.push(id_of(v));
    let mut v = base();
    v.5 = Some("ws2");
    variants.push(id_of(v));
    let mut v = base();
    v.5 = None;
    variants.push(id_of(v));
    let mut v = base();
    v.6 = "2026-08-31T12:00:01Z";
    variants.push(id_of(v));
    let mut v = base();
    v.7 = "incomplete";
    variants.push(id_of(v));
    let mut v = base();
    v.8 = r#"{"terminal":{"kind":"snapshot_manifest","manifest_sha256":"ab"}}"#;
    variants.push(id_of(v));

    let mut all = variants.clone();
    all.push(reference.clone());
    for id in &variants {
        assert_ne!(id, &reference, "a changed field must change the id");
    }
    let unique: std::collections::BTreeSet<_> = all.iter().collect();
    assert_eq!(unique.len(), all.len(), "no two variants may collide");
}

/// The regression the length-prefixed encoding exists for: content shifted
/// across a field boundary must never produce the same bytes.
#[test]
fn encoding_is_injective_across_field_boundaries() {
    let id = |adapter: &str, session: &str| {
        observation_id(
            1,
            "COMMAND_STARTED",
            adapter,
            session,
            "repo",
            None,
            "t",
            "unknown",
            "{}",
        )
    };
    assert_ne!(id("ab", "c"), id("a", "bc"));
    assert_ne!(id("s1", "12"), id("s", "112"));

    // None vs Some("") — the tag byte alone must separate them.
    let none = observation_id(
        1,
        "COMMAND_STARTED",
        "a",
        "s",
        "r",
        None,
        "t",
        "unknown",
        "{}",
    );
    let empty = observation_id(
        1,
        "COMMAND_STARTED",
        "a",
        "s",
        "r",
        Some(""),
        "t",
        "unknown",
        "{}",
    );
    assert_ne!(none, empty);

    // The completeness/receipt boundary.
    assert_ne!(
        observation_id(
            1,
            "COMMAND_STARTED",
            "a",
            "s",
            "r",
            None,
            "t",
            "unknown",
            "{}"
        ),
        observation_id(
            1,
            "COMMAND_STARTED",
            "a",
            "s",
            "r",
            None,
            "t",
            "unknown{",
            "}"
        ),
    );
}

#[test]
fn append_only_triggers_hold() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let receipt = SourceReceipt::new(ReceiptTerminal::HumanAdjudication {
        adjudicator: "operator".into(),
        adjudicated_at: "2026-08-31T14:00:00Z".into(),
    });
    append(
        &ledger,
        &NewObservation {
            observation_type: ObservationType::RecoveryHumanAdjudicated,
            source_adapter: "thruline",
            source_session: "sess-1",
            repository_identity: "repo:x",
            workspace_identity: None,
            observed_at: "2026-08-31T14:00:00Z",
            completeness: Completeness::Complete,
            receipt: &receipt,
        },
    )
    .expect("append");
    assert!(
        ledger
            .conn()
            .execute("UPDATE observations SET completeness='unknown'", [])
            .is_err(),
        "updates must abort"
    );
    assert!(
        ledger
            .conn()
            .execute("DELETE FROM observations", [])
            .is_err(),
        "deletes must abort"
    );
}

#[test]
fn a_tampered_row_is_named_by_verify() {
    let ledger = Ledger::open_in_memory().expect("ledger");
    let receipt = SourceReceipt::new(ReceiptTerminal::Filesystem {
        path: "/tmp/x".into(),
        sha256: None,
    });
    for (i, session) in ["s1", "s2", "s3"].into_iter().enumerate() {
        append(
            &ledger,
            &NewObservation {
                observation_type: ObservationType::WorkspaceObserved,
                source_adapter: "thruline",
                source_session: session,
                repository_identity: "repo:x",
                workspace_identity: None,
                observed_at: "2026-08-31T14:00:00Z",
                completeness: Completeness::Complete,
                receipt: &receipt,
            },
        )
        .expect("append");
        assert_eq!(verify_chain(&ledger).expect("verify"), i as u64 + 1);
    }
    // Bypass the trigger the way an attacker with the file would: drop it.
    ledger
        .conn()
        .execute_batch(
            "DROP TRIGGER observations_no_update;
             UPDATE observations SET source_receipt='{\"forged\":true}' WHERE seq = 1;",
        )
        .expect("forge");
    let err = verify_chain(&ledger).expect_err("a rewritten receipt must break the fold");
    assert!(
        err.to_string().contains("seq 1"),
        "the error names the broken link: {err}"
    );
}

/// §33 metamorphic: nuisance reordering of a receipt's fields does not change
/// canonical meaning, and unknown receipt fields survive a round trip.
#[test]
fn receipt_reordering_is_nuisance_and_unknown_fields_survive() {
    // The same receipt content, keys in two different textual orders, plus a
    // field this schema does not know.
    let one = r#"{"terminal":{"kind":"transcript","file":"/a.jsonl","line":3},"zz_future":"kept","aa_future":1}"#;
    let two = r#"{"aa_future":1,"zz_future":"kept","terminal":{"line":3,"kind":"transcript","file":"/a.jsonl"}}"#;
    let r1 = SourceReceipt::from_json(one).expect("one parses");
    let r2 = SourceReceipt::from_json(two).expect("two parses");
    assert_eq!(r1, r2, "order is nuisance");
    assert_eq!(
        r1.to_canonical_json().expect("canon 1"),
        r2.to_canonical_json().expect("canon 2"),
        "canonical bytes agree regardless of arrival order"
    );
    assert_eq!(
        r1.extra.len(),
        2,
        "unknown fields are preserved, not dropped"
    );

    // Appended through the store, the unknown fields stay hash-covered.
    let ledger = Ledger::open_in_memory().expect("ledger");
    let out = append(
        &ledger,
        &NewObservation {
            observation_type: ObservationType::EvidenceBound,
            source_adapter: "thruline",
            source_session: "sess-1",
            repository_identity: "repo:x",
            workspace_identity: None,
            observed_at: "2026-08-31T15:00:00Z",
            completeness: Completeness::Complete,
            receipt: &r1,
        },
    )
    .expect("append");
    let stored = out.record();
    let round = stored.receipt().expect("stored receipt parses");
    assert_eq!(round.extra.get("zz_future"), r1.extra.get("zz_future"));
    assert_eq!(verify_chain(&ledger).expect("verify"), 1);

    // And appending the reordered twin is the same observation.
    let again = append(
        &ledger,
        &NewObservation {
            observation_type: ObservationType::EvidenceBound,
            source_adapter: "thruline",
            source_session: "sess-1",
            repository_identity: "repo:x",
            workspace_identity: None,
            observed_at: "2026-08-31T15:00:00Z",
            completeness: Completeness::Complete,
            receipt: &r2,
        },
    )
    .expect("append twin");
    assert!(matches!(again, AppendOutcome::AlreadyRecorded(_)));
}

/// Malformed input can be rejected or recorded — never a panic (§33 fuzz
/// family, targeted cases; proptest is not a dependency of this crate).
#[test]
fn malformed_and_hostile_inputs_never_panic() {
    for garbage in [
        "",
        "not json",
        "{",
        "[]",
        "42",
        "null",
        r#"{"terminal":null}"#,
        r#"{"terminal":{"kind":"unheard_of"}}"#,
        r#"{"terminal":{"kind":"transcript"}}"#,
        r#"{"derived_refs":[{"table":"observations","id":"x"}]}"#,
        "\u{0}\u{1}\u{2}",
        "🦀🦀🦀",
    ] {
        assert!(
            SourceReceipt::from_json(garbage).is_err(),
            "must reject, not panic: {garbage:?}"
        );
    }

    // Hostile-but-valid envelope strings pass through whole and verify.
    let ledger = Ledger::open_in_memory().expect("ledger");
    let receipt = SourceReceipt::new(ReceiptTerminal::ProviderResponse {
        provider: "🦀 provider \"quoted\" \\slash\u{7f}".into(),
        response_sha256: "ab".repeat(320_000),
    });
    let long_session = "s".repeat(100_000);
    append(
        &ledger,
        &NewObservation {
            observation_type: ObservationType::ExternalAttestationReceived,
            source_adapter: "adapter\twith\ncontrol",
            source_session: &long_session,
            repository_identity: "répô — ünïcode",
            workspace_identity: Some(""),
            observed_at: "2026-08-31T16:00:00Z",
            completeness: Completeness::Unknown,
            receipt: &receipt,
        },
    )
    .expect("hostile strings are content, not errors");
    assert_eq!(verify_chain(&ledger).expect("verify"), 1);

    // A row whose stored receipt text is not valid JSON (planted directly,
    // as a hostile writer would): reading must not panic, and the typed
    // receipt accessor fails loudly while the raw text stays readable.
    ledger
        .conn()
        .execute_batch(
            "DROP TRIGGER observations_no_update;
             UPDATE observations SET source_receipt='not json at all' WHERE seq = 0;",
        )
        .expect("plant");
    let rows = read_all(&ledger).expect("read never panics");
    assert_eq!(rows[0].source_receipt, "not json at all");
    assert!(rows[0].receipt().is_err(), "typed access fails loudly");
    assert!(
        verify_chain(&ledger).is_err(),
        "and the chain names the tamper"
    );
}

#[test]
fn the_type_set_is_closed_at_twenty_two_and_the_db_check_mirrors_it() {
    assert_eq!(ObservationType::ALL.len(), 22, "§11's full set, fixed now");
    for t in ObservationType::ALL {
        assert_eq!(ObservationType::parse(t.as_str()), Some(t), "round trip");
    }
    assert_eq!(ObservationType::parse("CONTINUITY_ESTABLISHED"), None);
    assert_eq!(ObservationType::parse("workspace_observed"), None);

    // Every declared type is accepted by the CHECK constraint; one append
    // per type, and the chain folds over all 22 in order.
    let ledger = Ledger::open_in_memory().expect("ledger");
    let receipt = SourceReceipt::new(ReceiptTerminal::SnapshotManifest {
        manifest_sha256: "cd".repeat(32),
    });
    for (i, t) in ObservationType::ALL.iter().enumerate() {
        let observed_at = format!("2026-08-31T17:00:{i:02}Z");
        append(
            &ledger,
            &NewObservation {
                observation_type: *t,
                source_adapter: "thruline",
                source_session: "sess-types",
                repository_identity: "repo:x",
                workspace_identity: None,
                observed_at: &observed_at,
                completeness: Completeness::Complete,
                receipt: &receipt,
            },
        )
        .expect("every declared type appends");
    }
    assert_eq!(verify_chain(&ledger).expect("verify"), 22);

    // A type outside the set is refused by the database itself, even when
    // the writer bypasses the typed API.
    let refused = ledger.conn().execute(
        "INSERT INTO observations(id, seq, recorded_at, schema_version, observation_type,
            source_adapter, source_session, repository_identity, workspace_identity,
            observed_at, completeness, source_receipt, prev_hash, chain_hash)
         VALUES ('x', 99, 't', 1, 'MADE_UP_TYPE', 'a', 's', 'r', NULL, 't', 'complete', '{}', 'p', 'c')",
        [],
    );
    assert!(refused.is_err(), "the CHECK constraint is the closed set");

    // And so is a completeness value outside the closed vocabulary.
    let refused = ledger.conn().execute(
        "INSERT INTO observations(id, seq, recorded_at, schema_version, observation_type,
            source_adapter, source_session, repository_identity, workspace_identity,
            observed_at, completeness, source_receipt, prev_hash, chain_hash)
         VALUES ('y', 99, 't', 1, 'WORKSPACE_OBSERVED', 'a', 's', 'r', NULL, 't', 'partial', '{}', 'p', 'c')",
        [],
    );
    assert!(
        refused.is_err(),
        "completeness is complete/incomplete/unknown only"
    );
}

/// The six §11 termination arms, each constructible and round-tripping; and
/// the structural rule — a derived reference can only ride beside a raw
/// terminal, because a receipt without a terminal does not parse.
#[test]
fn receipts_terminate_at_raw_evidence_by_construction() {
    let arms = [
        ReceiptTerminal::Transcript {
            file: "/a.jsonl".into(),
            line: 1,
        },
        ReceiptTerminal::Filesystem {
            path: "/b".into(),
            sha256: Some("ab".into()),
        },
        ReceiptTerminal::ProcessExecution {
            command: "cargo test".into(),
            exit_code: Some(0),
        },
        ReceiptTerminal::SnapshotManifest {
            manifest_sha256: "cd".into(),
        },
        ReceiptTerminal::ProviderResponse {
            provider: "github".into(),
            response_sha256: "ef".into(),
        },
        ReceiptTerminal::HumanAdjudication {
            adjudicator: "operator".into(),
            adjudicated_at: "2026-08-31T18:00:00Z".into(),
        },
    ];
    for arm in arms {
        let mut receipt = SourceReceipt::new(arm);
        receipt.derived_refs.push(DerivedRef {
            table: "workspace_snapshots".into(),
            id: "legacy-row".into(),
        });
        let json = receipt.to_canonical_json().expect("serialize");
        let back = SourceReceipt::from_json(&json).expect("parse");
        assert_eq!(back, receipt, "each arm round-trips beside a derived ref");
    }

    // A receipt that names ONLY derived rows — other observations included —
    // is unrepresentable: it fails to parse, so it can never be stored.
    let derived_only = r#"{"derived_refs":[{"table":"observations","id":"abc"}]}"#;
    assert!(SourceReceipt::from_json(derived_only).is_err());
}
