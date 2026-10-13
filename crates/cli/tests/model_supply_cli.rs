use std::process::Command;

use serde_json::Value;
use tempfile::tempdir;

fn thruline() -> Command {
    Command::new(env!("CARGO_BIN_EXE_thruline"))
}

#[test]
fn explicit_status_is_machine_readable_without_home_or_ledger_access() {
    let temp = tempdir().expect("tempdir");
    let missing = temp.path().join("missing-model");
    let output = thruline()
        .env_remove("HOME")
        .args(["model", "status", "--dir"])
        .arg(&missing)
        .output()
        .expect("run model status");

    assert!(!output.status.success(), "missing model must fail loudly");
    assert!(output.stderr.is_empty(), "status is a bounded JSON result");
    let status: Value = serde_json::from_slice(&output.stdout).expect("status JSON");
    assert_eq!(status["schema"], "thruline-nli-model-status-v1");
    assert_eq!(status["state"], "missing");
    assert_eq!(status["ready"], false);
    assert_eq!(status["files"].as_array().map(Vec::len), Some(3));
    assert!(!missing.exists());
}

#[test]
fn invalid_import_preview_writes_nothing_and_echoes_no_file_content() {
    let temp = tempdir().expect("tempdir");
    let source = temp.path().join("source");
    let destination = temp.path().join("never-created/destination");
    std::fs::create_dir(&source).expect("source");
    std::fs::write(
        source.join("config.json"),
        b"PRIVATE_MODEL_FILE_CONTENT_MUST_NOT_PRINT",
    )
    .expect("invalid model file");

    let output = thruline()
        .env_remove("HOME")
        .args(["model", "import", "--from"])
        .arg(&source)
        .arg("--dir")
        .arg(&destination)
        .output()
        .expect("run model import preview");

    assert!(!output.status.success(), "invalid source must be refused");
    assert!(output.stderr.is_empty(), "refusal is a bounded JSON result");
    let report: Value = serde_json::from_slice(&output.stdout).expect("import JSON");
    assert_eq!(report["schema"], "thruline-nli-model-import-v1");
    assert_eq!(report["apply_requested"], false);
    assert_eq!(report["outcome"], "refused_source");
    assert!(!destination.exists());
    assert!(!String::from_utf8_lossy(&output.stdout)
        .contains("PRIVATE_MODEL_FILE_CONTENT_MUST_NOT_PRINT"));
}
