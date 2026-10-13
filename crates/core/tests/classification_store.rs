//! §5t classification annotations are immutable, hashed, and idempotent on
//! `(event, model, prompt hash)`.

use thruline_core::chain::sha256_hex;
use thruline_core::classification_store::{
    find, store, NewClassification, StoreDisposition, CLASSIFICATION_LAYER,
};
use thruline_core::{
    EventKind, Ledger, NormalizedEvent, ProjectRef, RawLineRef, SessionRef, Source,
};

const PROJ: &str = "-Users-dev-classification";
const SESS: &str = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";

fn fixture() -> (Ledger, String) {
    let mut ledger = Ledger::open_in_memory().expect("ledger");
    ledger
        .upsert_project(
            &ProjectRef {
                id: PROJ.to_string(),
                root_path: "/Users/dev/classification".to_string(),
                source: Source::ClaudeCode,
                display_name: Some("classification".to_string()),
            },
            "2026-07-31T00:00:00Z",
        )
        .expect("project");
    ledger
        .upsert_session(&SessionRef {
            id: SESS.to_string(),
            project_id: PROJ.to_string(),
            file_path: "/Users/dev/classification/session.jsonl".to_string(),
            started_at: Some("2026-07-31T00:00:00Z".to_string()),
            raw_line_count: 1,
            raw_digest: None,
        })
        .expect("session");
    let event = ledger
        .append_session_events(
            PROJ,
            SESS,
            &[(
                RawLineRef {
                    file: "/Users/dev/classification/session.jsonl".to_string(),
                    line: 1,
                    sha256: "a".repeat(64),
                },
                NormalizedEvent::new(EventKind::AssistantText),
            )],
        )
        .expect("event")
        .remove(0);
    (ledger, event.id)
}

fn input<'a>(event_id: &'a str, model: &'a str, prompt: &'a str) -> NewClassification<'a> {
    NewClassification {
        event_id,
        model,
        prompt,
        output_json: r#"{"category":"finding","confidence":0.94}"#,
        confidence: 0.94,
        created_at: "2026-07-31T00:01:00Z",
    }
}

#[test]
fn storage_fixes_the_layer_and_hashes_prompt_and_output() {
    let (ledger, event_id) = fixture();
    let prompt = "Classify this sentence under charter v1.";
    let result = store(&ledger, input(&event_id, "qwen3:4b", prompt)).expect("store");
    assert_eq!(result.disposition, StoreDisposition::Inserted);
    assert_eq!(
        result.annotation.prompt_sha256,
        sha256_hex(prompt.as_bytes())
    );
    assert_eq!(
        result.annotation.output_sha256,
        sha256_hex(result.annotation.output_json.as_bytes())
    );

    let layer: String = ledger
        .conn()
        .query_row(
            "SELECT layer FROM annotations WHERE id = ?1",
            [&result.annotation.id],
            |r| r.get(0),
        )
        .expect("layer");
    assert_eq!(layer, CLASSIFICATION_LAYER);
    assert_eq!(
        find(&ledger, &event_id, "qwen3:4b", prompt)
            .expect("find")
            .expect("stored"),
        result.annotation
    );
}

#[test]
fn identical_input_reuses_the_first_row_and_never_rewrites_it() {
    let (ledger, event_id) = fixture();
    let prompt = "Classify once.";
    let first = store(&ledger, input(&event_id, "qwen3:4b", prompt)).expect("first");

    let mut replay = input(&event_id, "qwen3:4b", prompt);
    replay.output_json = r#"{"category":"proposal","confidence":0.51}"#;
    replay.confidence = 0.51;
    replay.created_at = "2026-07-31T23:59:59Z";
    let second = store(&ledger, replay).expect("replay");

    assert_eq!(second.disposition, StoreDisposition::Existing);
    assert_eq!(second.annotation, first.annotation);
    let rows: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM annotations WHERE layer = 'classification'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(rows, 1);
}

#[test]
fn model_or_prompt_swap_appends_instead_of_rewriting() {
    let (ledger, event_id) = fixture();
    let first = store(&ledger, input(&event_id, "qwen3:4b", "prompt-v1")).expect("first");
    let model_swap = store(&ledger, input(&event_id, "qwen3:8b", "prompt-v1")).expect("model swap");
    let prompt_swap =
        store(&ledger, input(&event_id, "qwen3:4b", "prompt-v2")).expect("prompt swap");

    assert_eq!(model_swap.disposition, StoreDisposition::Inserted);
    assert_eq!(prompt_swap.disposition, StoreDisposition::Inserted);
    let mut ids = vec![
        first.annotation.id,
        model_swap.annotation.id,
        prompt_swap.annotation.id,
    ];
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 3);
}

#[test]
fn database_guards_classification_rows_against_update_and_delete() {
    let (ledger, event_id) = fixture();
    let stored = store(&ledger, input(&event_id, "qwen3:4b", "immutable")).expect("store");

    for result in [
        ledger.conn().execute(
            "UPDATE annotations SET output_json = '{}' WHERE id = ?1",
            [&stored.annotation.id],
        ),
        ledger.conn().execute(
            "DELETE FROM annotations WHERE id = ?1",
            [&stored.annotation.id],
        ),
    ] {
        let err = result.expect_err("classification rows are append-only");
        assert!(err.to_string().contains("append-only"), "{err}");
    }

    let replacement = ledger.conn().execute(
        "INSERT OR REPLACE INTO annotations(id, event_id, layer, model, prompt_sha256,
             output_json, output_sha256, confidence, created_at)
         VALUES(?1, ?2, 'classification', ?3, ?4, '{}', ?5, 0.1,
             '2026-07-31T23:59:59Z')",
        rusqlite::params![
            stored.annotation.id,
            event_id,
            stored.annotation.model,
            stored.annotation.prompt_sha256,
            sha256_hex(b"{}")
        ],
    );
    let err = replacement.expect_err("OR REPLACE must not bypass append-only storage");
    assert!(err.to_string().contains("append-only"), "{err}");

    assert_eq!(
        find(&ledger, &event_id, "qwen3:4b", "immutable")
            .expect("find")
            .expect("still present"),
        stored.annotation
    );
}

#[test]
fn malformed_outputs_and_confidence_are_rejected_before_storage() {
    let (ledger, event_id) = fixture();
    let mut malformed = input(&event_id, "qwen3:4b", "malformed");
    malformed.output_json = "not JSON";
    assert!(store(&ledger, malformed).is_err());

    let mut scalar = input(&event_id, "qwen3:4b", "scalar");
    scalar.output_json = "42";
    assert!(store(&ledger, scalar).is_err());

    let mut confidence = input(&event_id, "qwen3:4b", "confidence");
    confidence.confidence = 1.01;
    assert!(store(&ledger, confidence).is_err());

    let rows: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM annotations WHERE layer = 'classification'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(rows, 0);
}
