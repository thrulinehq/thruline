//! C33a: inspect the exact ordinary release CLI, not the unit-test executable.
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

#[test]
#[ignore = "requires the C33a ordinary release build and its exact artifact receipt"]
fn ordinary_release_has_local_authentication_and_no_terminal_test_stub() {
    let artifact = PathBuf::from(
        std::env::var("THRULINE_C33_RELEASE_ARTIFACT").expect("exact ordinary release artifact"),
    );
    let expected =
        std::env::var("THRULINE_C33_RELEASE_SHA256").expect("exact release artifact SHA-256");
    let depinfo = PathBuf::from(
        std::env::var("THRULINE_C33_RELEASE_DEPINFO").expect("ordinary release dependency file"),
    );
    let build_receipt = PathBuf::from(
        std::env::var("THRULINE_C33_RELEASE_BUILD_RECEIPT")
            .expect("ordinary release build receipt"),
    );
    assert_eq!(
        artifact.file_name().and_then(|name| name.to_str()),
        Some("thruline")
    );
    assert_eq!(
        artifact
            .parent()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str()),
        Some("release")
    );
    assert!(fs::symlink_metadata(&artifact)
        .expect("artifact metadata")
        .file_type()
        .is_file());
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(build_receipt).expect("build receipt"))
            .expect("receipt JSON");
    let argv = receipt["argv"].as_array().expect("recorded build argv");
    assert!(argv.iter().any(|arg| arg == "build"));
    assert!(argv.iter().any(|arg| arg == "--release"));
    assert!(!argv.iter().any(|arg| arg == "test" || arg == "--cfg"));
    assert_eq!(receipt["compiler_cfg_overrides_removed"], true);
    assert_eq!(receipt["exit_code"], 0);
    assert_eq!(receipt["stopped_below_15_gib"], false);

    let bytes = fs::read(&artifact).expect("ordinary release executable");
    let actual: String = Sha256::digest(&bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(actual, expected, "inspect the exact recorded artifact");
    let contains = |text: &str| {
        bytes
            .windows(text.len())
            .any(|window| window == text.as_bytes())
    };
    for marker in [
        "terminal_pty_tests",
        "c33_terminal_child",
        "presence_stub_enabled",
        "device_uid_override",
        "THRULINE_C33_UNIT_MODE",
        "THRULINE_C33_UNIT_STATUS",
        "THRULINE_C33_UNIT_PID",
        "THRULINE_NATIVE_UNIT_MODE",
        "THRULINE_NATIVE_UNIT_STATUS",
        "native_click_tests",
        "native_click_child",
        "THRULINE_DECISION_SOURCE_READ_CASE",
        "source_read_worker",
        "THRULINE_RAW_UNIT_MODE",
        "THRULINE_RAW_UNIT_ROOT",
        "bridge_raw_tests",
    ] {
        assert!(
            !contains(marker),
            "test-only hook leaked into release: {marker}"
        );
    }
    for presence in [
        "import LocalAuthentication",
        "let context = LAContext()",
        "context.touchIDAuthenticationAllowableReuseDuration = 0",
        "context.evaluatePolicy(.deviceOwnerAuthentication",
        "import AppKit",
        "native_review_click",
        "THRULINE_NATIVE_REVIEW_FILE",
    ] {
        assert!(
            contains(presence),
            "production presence step missing: {presence}"
        );
    }
    let dependencies = fs::read_to_string(depinfo).expect("release dependency file");
    assert!(dependencies.contains("decisions/bridge.rs"));
    assert!(dependencies.contains("decisions/terminal_presence.swift"));
    assert!(!dependencies.contains("bridge_terminal_tests.rs"));
    assert!(dependencies.contains("decisions/native_review.swift"));
    assert!(!dependencies.contains("decisions/native_click_tests.rs"));
    assert!(!dependencies.contains("decisions/record_source_read_tests.rs"));
    assert!(!dependencies.contains("decisions/bridge_raw_tests.rs"));
    println!(
        "C33a ordinary release {actual}: LocalAuthentication retained; terminal test hooks absent"
    );
}
