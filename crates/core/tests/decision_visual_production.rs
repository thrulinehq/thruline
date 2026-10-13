use std::path::{Path, PathBuf};

use serde_json::json;
use sha2::{Digest, Sha256};
use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::checks_visual::check_visual_at;
use thruline_core::decisions::params::{parse_params, CheckParams};

fn hash(path: &Path) -> String {
    Sha256::digest(std::fs::read(path).unwrap())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
#[ignore = "requires pinned local Chrome and builds a historical version"]
fn exact_historical_ui_and_server_capture_a_legacy_route() {
    let renderer = PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
    assert!(renderer.is_file(), "pinned renderer is required");
    std::env::set_var("THRULINE_VISUAL_RENDERER", &renderer);
    std::env::set_var("THRULINE_VISUAL_RENDERER_SHA256", hash(&renderer));
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let reference = repo.join("docs/hq-next/reference/Onboarding.png");
    let value = json!({
        "kind": "visual_match", "tolerance_pct": 0.5,
        "screens": [{
            "name": "Onboarding", "route": "#/design/Onboarding", "scenario": "design",
            "legacy_route": "#/start", "legacy_scenario": "fixed-record",
            "reference_path": "docs/hq-next/reference/Onboarding.png",
            "reference_sha256": hash(&reference),
            "width": 1512, "height": 920, "full_page": false,
            "mask_rectangles": []
        }]
    });
    let CheckParams::VisualMatch(params) =
        parse_params("visual_match", 1, &value.to_string()).unwrap()
    else {
        panic!("not C1")
    };
    let scratch = tempfile::tempdir().unwrap();
    let db = scratch.path().join("ledger.db");
    let _db = rusqlite::Connection::open(&db).unwrap();
    explicit_fixture(&db);
    let outcome = check_visual_at(&repo, "efdc063e", &params, &db);
    assert_eq!(outcome.state, VerdictState::Broken, "{outcome:?}");
    assert_eq!(outcome.visual.len(), 1);
    assert_eq!(outcome.visual[0].actual_route.as_deref(), Some("#/start"));
    assert!(outcome.visual[0].difference_pct.is_some(), "{outcome:?}");
    assert!(outcome.visual[0].tree_hash.is_some());
}

#[test]
#[ignore = "requires pinned local Chrome and builds the exact HQ design version"]
fn exact_hq_design_capture_matches_the_pinned_inbox() {
    let renderer = PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
    assert!(renderer.is_file(), "pinned renderer is required");
    std::env::set_var("THRULINE_VISUAL_RENDERER", &renderer);
    std::env::set_var("THRULINE_VISUAL_RENDERER_SHA256", hash(&renderer));
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let reference = repo.join("docs/hq-next/reference/Inbox.png");
    let value = json!({
        "kind": "visual_match", "tolerance_pct": 0.5,
        "screens": [{
            "name": "Inbox", "route": "#/design/Inbox", "scenario": "design",
            "reference_path": "docs/hq-next/reference/Inbox.png",
            "reference_sha256": hash(&reference),
            "width": 1512, "height": 920, "full_page": false,
            "mask_rectangles": []
        }]
    });
    let CheckParams::VisualMatch(params) =
        parse_params("visual_match", 1, &value.to_string()).unwrap()
    else {
        panic!("not C1")
    };
    let scratch = tempfile::tempdir().unwrap();
    let db = scratch.path().join("ledger.db");
    let _db = rusqlite::Connection::open(&db).unwrap();
    explicit_fixture(&db);
    let outcome = check_visual_at(&repo, "HEAD", &params, &db);
    assert_eq!(outcome.visual.len(), 1, "{outcome:?}");
    assert_eq!(
        outcome.visual[0].actual_route.as_deref(),
        Some("#/design/Inbox")
    );
    assert_eq!(outcome.state, VerdictState::Holds, "{outcome:?}");
    assert_eq!(outcome.visual[0].difference_pct, Some(0.0), "{outcome:?}");
}

#[test]
#[ignore = "requires pinned local Chrome and builds a historical version"]
fn exact_historical_build_uses_pinned_toolchain_with_scratch_home() {
    let renderer = PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome");
    assert!(renderer.is_file(), "pinned renderer is required");
    std::env::set_var("THRULINE_VISUAL_RENDERER", &renderer);
    std::env::set_var("THRULINE_VISUAL_RENDERER_SHA256", hash(&renderer));
    let scratch = tempfile::tempdir().unwrap();
    let scratch_home = scratch.path().join("home");
    std::fs::create_dir_all(&scratch_home).unwrap();
    let original_home = std::env::var_os("HOME");
    let original_cargo_home = std::env::var_os("CARGO_HOME");
    let original_rustup_home = std::env::var_os("RUSTUP_HOME");
    let original_auto_install = std::env::var_os("RUSTUP_AUTO_INSTALL");
    let original_cache = std::env::var_os("THRULINE_VISUAL_CACHE_DIR");
    let original_profile = std::env::var_os("THRULINE_VISUAL_PROFILE");
    std::env::set_var("HOME", &scratch_home);
    std::env::remove_var("CARGO_HOME");
    std::env::remove_var("RUSTUP_HOME");
    std::env::set_var("RUSTUP_AUTO_INSTALL", "0");
    std::env::set_var(
        "THRULINE_VISUAL_CACHE_DIR",
        scratch.path().join("visual-cache"),
    );

    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let reference = repo.join("docs/hq-next/reference/Onboarding.png");
    let value = json!({
        "kind": "visual_match", "tolerance_pct": 0.5,
        "screens": [{
            "name": "Onboarding", "route": "#/design/Onboarding", "scenario": "design",
            "legacy_route": "#/start", "legacy_scenario": "fixed-record",
            "reference_path": "docs/hq-next/reference/Onboarding.png",
            "reference_sha256": hash(&reference),
            "width": 1512, "height": 920, "full_page": false,
            "mask_rectangles": []
        }]
    });
    let CheckParams::VisualMatch(params) =
        parse_params("visual_match", 1, &value.to_string()).unwrap()
    else {
        panic!("not C1")
    };
    let db = scratch.path().join("ledger.db");
    let _db = rusqlite::Connection::open(&db).unwrap();
    explicit_fixture(&db);
    let outcome = check_visual_at(&repo, "efdc063e", &params, &db);
    std::thread::sleep(std::time::Duration::from_secs(2));
    let profile = scratch.path().join("warm-profile.jsonl");
    std::env::set_var("THRULINE_VISUAL_PROFILE", &profile);
    let replay = check_visual_at(&repo, "efdc063e", &params, &db);

    restore_env("HOME", original_home);
    restore_env("CARGO_HOME", original_cargo_home);
    restore_env("RUSTUP_HOME", original_rustup_home);
    restore_env("RUSTUP_AUTO_INSTALL", original_auto_install);
    restore_env("THRULINE_VISUAL_CACHE_DIR", original_cache);
    restore_env("THRULINE_VISUAL_PROFILE", original_profile);

    assert_eq!(outcome.state, VerdictState::Broken, "{outcome:?}");
    assert!(outcome.visual[0].difference_pct.is_some(), "{outcome:?}");
    assert!(outcome.visual[0].capture_clock.is_some(), "{outcome:?}");
    assert_eq!(
        outcome, replay,
        "visual replay must keep the same verdict bytes"
    );
    let stages = std::fs::read_to_string(profile).unwrap();
    for line in stages.lines() {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        assert!(
            ![
                "archive",
                "npm_install",
                "npm_build",
                "cargo_build",
                "snapshot",
                "native_upgrade",
                "export",
                "server_start",
                "capture_total",
                "diff"
            ]
            .contains(&row["stage"].as_str().unwrap()),
            "historical replay must reuse its verified build, pixels and comparison: {row}"
        );
    }
    eprintln!("Historical warm replay: verified artifacts reused; no build, snapshot, export, Chrome or diff stage");
}

fn restore_env(key: &str, value: Option<std::ffi::OsString>) {
    if let Some(value) = value {
        std::env::set_var(key, value);
    } else {
        std::env::remove_var(key);
    }
}

fn explicit_fixture(db: &Path) {
    let manifest = thruline_core::decisions::checks_visual::visual_inputs::VisualFixtureManifest {
        schema_version: 1,
        record_path: None,
        record_sha256: hash(db),
        record_bytes: std::fs::metadata(db).unwrap().len(),
        capture_clock: "2026-10-06T23:42:17Z".into(),
        clock_basis: "explicit_fixture".into(),
        clock_witness: None,
        fixture_description: "Explicit synthetic empty ledger for production capture fixture"
            .into(),
    };
    let path = db.with_extension("visual-fixture.json");
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
}
