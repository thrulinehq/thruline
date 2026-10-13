use serde_json::{json, Value};
use thruline_verify::integrations::decode;

const CAPTURE: &[u8] = include_bytes!("fixtures/dependency-folder-unresolved-native.json");

#[test]
fn actual_native_sink_retains_graph_and_unresolved_scope() {
    let native: Value = serde_json::from_slice(CAPTURE).unwrap();
    let report = decode("dependency-cruiser-json-v1", CAPTURE).unwrap();
    assert!(!report.complete);
    assert_eq!(
        report.facts["dependency_graph"]["folders"],
        native["folders"]
    );
    assert_eq!(
        report.facts["dependency_graph"]["modules"],
        native["modules"]
    );
    assert_eq!(
        report.facts["dependency_graph"]["folders"][1]["moduleCount"],
        -1
    );
    assert!(report
        .limitations
        .iter()
        .any(|s| s == "Some dependencies could not be resolved."));
}

#[test]
fn only_the_native_integer_sentinel_is_accepted() {
    for invalid in [json!(-2), json!(-1.5), json!("-1"), Value::Null] {
        let mut native: Value = serde_json::from_slice(CAPTURE).unwrap();
        native["folders"][1]["moduleCount"] = invalid;
        let error = decode(
            "dependency-cruiser-json-v1",
            &serde_json::to_vec(&native).unwrap(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("invalid native folder"));
    }
}
