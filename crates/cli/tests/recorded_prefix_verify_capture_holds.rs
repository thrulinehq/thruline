//! The native door distinguishes the absent engine adapter from HQ's verifier.
use std::fs;
use std::process::Command;
use thruline_core::Ledger;

#[test]
fn native_verify_names_the_compiled_capture_hold_adapter_qualification() {
    let temp = tempfile::tempdir().expect("authored fixture");
    let home = temp.path().join("home");
    fs::create_dir(&home).expect("authored HOME");
    let db = temp.path().join("ledger.db");
    drop(Ledger::open(&db).expect("authored empty ledger"));
    let out = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .env("HOME", &home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .arg("--db")
        .arg(&db)
        .args(["verify", "--json"])
        .output()
        .expect("authored native CLI");
    assert_eq!(out.status.code(), Some(0));
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).expect("registered document");
    assert_eq!(doc["shape"], "thruline.verify");
    assert_eq!(doc["schema_version"], 1);
    let core_source = include_str!("../../core/src/lib.rs");
    let has_hq_adapter = core_source
        .lines()
        .any(|line| line.trim() == "pub mod capture_hold;");
    let row = doc
        .get("capture_holds")
        .expect("explicit capture-hold qualification");
    if has_hq_adapter {
        assert_eq!(row["state"], "INTACT", "HQ must supply its real verifier");
        assert_eq!(row["links_checked"], 0, "authored empty capture-hold chain");
        assert_eq!(row["reason"], serde_json::Value::Null);
    } else {
        assert_eq!(
            row,
            &serde_json::Value::Null,
            "an absent adapter cannot claim intact"
        );
    }
}
