#![recursion_limit = "256"]

use serde_json::{json, Value};
use thruline_core::decisions::checks::CheckOutcome;
use thruline_core::decisions::replay_basis::stable_verdict_basis;

fn sample() -> CheckOutcome {
    serde_json::from_value(json!({
        "state":"holds", "tree_hash":"a".repeat(40), "reason":null,
        "checked_places":1, "findings":[], "surfaces":[],
        "visual":[{
            "screen":"Inbox", "route":"#/design/inbox", "state":"holds",
            "actual_route":"#/design/inbox", "actual_scenario":"design",
            "difference_pct":0.0, "different_pixels":0, "checked_pixels":76800,
            "reference_width":320, "reference_height":240,
            "capture_width":320, "capture_height":240,
            "compared_width":320, "compared_height":240,
            "diff_image_path":"immutable-diff.png", "diff_sha256":"b".repeat(64),
            "reference_sha256":"c".repeat(64), "version_id":"d".repeat(40),
            "commit_oid":"d".repeat(40), "tree_hash":"a".repeat(40),
            "renderer_sha256":"e".repeat(64), "renderer_version":"Chrome/154.0.8037.98",
            "capture_clock":"2026-10-06T13:43:10Z", "fixture_sha256":"f".repeat(64),
            "frame_width":320, "frame_height":240, "content_width":320, "content_height":240,
            "overflow_x":false, "overflow_y":false,
            "scroll_regions":[], "clipped_regions":[], "housekeeping_responses":[], "reason":null
        }]
    }))
    .unwrap()
}

fn with_housekeeping(outcome: &CheckOutcome, receipts: Value) -> CheckOutcome {
    let mut value = serde_json::to_value(outcome).unwrap();
    value["visual"][0]["housekeeping_responses"] = receipts;
    serde_json::from_value(value).unwrap()
}

#[test]
fn only_one_valid_transport_diagnostic_can_vary_without_mutating_full_receipts() {
    let empty = sample();
    let one = with_housekeeping(&empty, json!([{"path":"/favicon.ico", "status":204}]));
    let empty_bytes = serde_json::to_vec(&empty).unwrap();
    let one_bytes = serde_json::to_vec(&one).unwrap();
    assert_ne!(empty, one);
    assert_eq!(stable_verdict_basis(&empty), stable_verdict_basis(&one));
    assert_eq!(serde_json::to_vec(&empty).unwrap(), empty_bytes);
    assert_eq!(serde_json::to_vec(&one).unwrap(), one_bytes);
    assert_eq!(one.visual[0].housekeeping_responses.len(), 1);
}

#[test]
fn every_verdict_and_input_field_remains_part_of_replay_comparison() {
    let original = sample();
    for (field, value) in [
        ("screen", json!("Other")),
        ("route", json!("#/other")),
        ("actual_route", json!("#/other")),
        ("actual_scenario", json!("default")),
        ("state", json!("broken")),
        ("reason", json!("unexpected request")),
        ("fixture_sha256", json!("0".repeat(64))),
        ("capture_clock", json!("2026-10-06T13:43:11Z")),
        ("renderer_version", json!("Chrome/0")),
        ("renderer_sha256", json!("0".repeat(64))),
        ("reference_sha256", json!("0".repeat(64))),
        ("version_id", json!("0".repeat(40))),
        ("commit_oid", json!("0".repeat(40))),
        ("tree_hash", json!("0".repeat(40))),
        ("frame_width", json!(321)),
        ("frame_height", json!(241)),
        ("content_width", json!(321)),
        ("content_height", json!(241)),
        ("overflow_x", json!(true)),
        ("overflow_y", json!(true)),
        ("difference_pct", json!(1.0)),
        ("different_pixels", json!(1)),
        ("checked_pixels", json!(1)),
        ("reference_width", json!(321)),
        ("reference_height", json!(241)),
        ("capture_width", json!(321)),
        ("capture_height", json!(241)),
        ("compared_width", json!(321)),
        ("compared_height", json!(241)),
        ("diff_image_path", json!("other-diff.png")),
        ("diff_sha256", json!("0".repeat(64))),
        (
            "scroll_regions",
            json!([{"x":0.0,"y":0.0,"width":100.0,"height":100.0,"content_width":100,"content_height":200,"overflow_x":false,"overflow_y":true}]),
        ),
        (
            "clipped_regions",
            json!([{"x":0.0,"y":0.0,"width":100.0,"height":100.0,"content_width":100,"content_height":200,"overflow_x":false,"overflow_y":true}]),
        ),
    ] {
        let mut value_json = serde_json::to_value(&original).unwrap();
        value_json["visual"][0][field] = value;
        let changed: CheckOutcome = serde_json::from_value(value_json).unwrap();
        assert_ne!(
            stable_verdict_basis(&original),
            stable_verdict_basis(&changed),
            "{field}"
        );
    }
    for (field, value) in [
        ("state", json!("broken")),
        ("reason", json!("unexpected request")),
        ("tree_hash", json!("0".repeat(40))),
        ("checked_places", json!(2)),
        (
            "findings",
            json!([{"surface":"code","path":"src/file.rs","line":1,"span":"changed"}]),
        ),
        (
            "surfaces",
            json!([{"surface":"code","checked_places":1,"finding_count":1,"state":"broken"}]),
        ),
        (
            "not_looked_at",
            json!([{"path":"tests/fixture.rs","source":"accepted_allowlist","pattern":"tests/*","test_name":null,"start_line":null,"end_line":null,"start_column":null,"end_column":null}]),
        ),
        ("visual", json!([])),
    ] {
        let mut value_json = serde_json::to_value(&original).unwrap();
        value_json[field] = value;
        let changed: CheckOutcome = serde_json::from_value(value_json).unwrap();
        assert_ne!(
            stable_verdict_basis(&original),
            stable_verdict_basis(&changed),
            "{field}"
        );
    }
}

#[test]
fn invalid_or_duplicate_housekeeping_diagnostics_still_differ() {
    let original = sample();
    for receipts in [
        json!([{"path":"/favicon.ico", "status":200}]),
        json!([{"path":"/api/record", "status":204}]),
        json!([{"path":"/favicon.ico?x=1", "status":204}]),
        json!([{"path":"/favicon.ico", "status":204}, {"path":"/favicon.ico", "status":204}]),
    ] {
        let changed = with_housekeeping(&original, receipts);
        assert_ne!(
            stable_verdict_basis(&original),
            stable_verdict_basis(&changed)
        );
        assert_eq!(stable_verdict_basis(&changed), changed);
    }
}
