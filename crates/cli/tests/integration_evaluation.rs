use serde_json::{json, Value};
use std::process::Command;
#[test]
fn evaluation_binds_the_original_oracles_and_cannot_hide_missing_predictions() {
    let dir = tempfile::tempdir().unwrap();
    let labels = json!({"schema":"thruline.integration-qualification-cases.v2","cases":[{"id":"known-failure","pack":"ast-grep","label":"failure"}]});
    let bytes = serde_json::to_vec(&labels).unwrap();
    let oracle = dir.path().join("labels.json");
    std::fs::write(&oracle, &bytes).unwrap();
    let predictions = dir.path().join("results.json");
    std::fs::write(&predictions, serde_json::to_vec(&json!({"schema":"thruline.upstream-qualification.v1","manifest_sha256":thruline_core::chain::sha256_hex(&bytes),"cases":[]})).unwrap()).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_thruline"))
            .args(["integration", "evaluate", "--cases"])
            .arg(&oracle)
            .arg("--results")
            .arg(&predictions)
            .output()
            .unwrap()
    };
    let result = run();
    assert!(result.status.success(), "{:?}", result);
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["total"]["missing_predictions"], 1);
    assert_eq!(value["total"]["missed_failures"], 1);
    assert_eq!(value["qualified"], false);
    let mut changed = labels;
    changed["cases"][0]["label"] = json!("clean");
    std::fs::write(&oracle, serde_json::to_vec(&changed).unwrap()).unwrap();
    let refused = run();
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("exact oracle manifest"));
}
