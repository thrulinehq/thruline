use serde_json::{json, Value};
use thruline_verify::integrations::decode;

fn captured() -> Value {
    serde_json::from_slice(include_bytes!("fixtures/pr-agent-plain-diff-capture.json")).unwrap()
}

#[test]
fn actual_native_plain_diff_preserves_patch_context_and_advisory_output() {
    let value = captured();
    let report = decode("pr-agent-json-v1", &serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(report.complete);
    assert_eq!(report.facts["advisory_only"], true);
    assert_eq!(report.facts["acceptance_state"], "not_evaluated");
    assert_eq!(
        report.facts["pr_agent_plain_diff"],
        value["pr_agent_plain_diff"]
    );
    assert_eq!(report.facts["native_output"], value["native_output"]);
    assert!(report.facts["native_output"]["text"]
        .as_str()
        .unwrap()
        .contains("a - b"));
}

#[test]
fn absent_mismatched_or_empty_native_patch_scope_cannot_be_complete() {
    for (pointer, replacement) in [
        ("/pr_agent_plain_diff", Value::Null),
        ("/pr_agent_plain_diff/input/sha256", json!("f".repeat(64))),
        ("/pr_agent_plain_diff/native_files/0/patch", json!("")),
        ("/pr_agent_plain_diff/working_tree_enrichment", json!(true)),
        (
            "/pr_agent_plain_diff/native_files/0/head_file",
            json!("unexpected source"),
        ),
    ] {
        let mut value = captured();
        *value.pointer_mut(pointer).unwrap() = replacement;
        let report = decode("pr-agent-json-v1", &serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(!report.complete, "{pointer}");
        assert_eq!(report.facts["native_output"], value["native_output"]);
    }
}

#[test]
fn supplied_patch_cannot_claim_editable_source_or_another_provider() {
    for (pointer, replacement) in [
        ("/source/editable_files", json!(["change.diff"])),
        ("/source/capture_basis", json!("committed_HEAD")),
        ("/pr_agent_options/mode", json!("hosted")),
    ] {
        let mut value = captured();
        *value.pointer_mut(pointer).unwrap() = replacement;
        assert!(decode("pr-agent-json-v1", &serde_json::to_vec(&value).unwrap()).is_err());
    }
}
