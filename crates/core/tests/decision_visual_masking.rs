//! C38: presentation masking follows exact capture checks and precedes cuts.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::checks_visual::{
    check_visual_with, visual_tool_identity, VisualRuntime,
};
use thruline_core::decisions::params::{VisualMatchParams, VisualScreen};
use thruline_core::redact::Masked;

const CANARY: &str = "ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e";

fn params(name: &str, route: &str) -> VisualMatchParams {
    VisualMatchParams {
        screens: vec![VisualScreen {
            screen: name.into(),
            route: route.into(),
            scenario: "fixed-record".into(),
            legacy_route: None,
            legacy_scenario: None,
            reference_path: "refs/fixture.png".into(),
            reference_sha256: "a".repeat(64),
            viewport_width: 1,
            viewport_height: 1,
            full_page: false,
            mask_rectangles: Vec::new(),
        }],
        tolerance_pct: 0.5,
    }
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args([
            "-c",
            "user.name=synthetic fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(repo)
        .output()
        .expect("synthetic Git command");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("Git UTF-8")
        .trim()
        .into()
}

#[test]
fn visual_reports_mask_screen_labels_without_changing_inputs_or_claiming_a_capture() {
    let scratch = tempfile::tempdir().expect("synthetic repository");
    git(scratch.path(), &["init", "-q"]);
    std::fs::write(scratch.path().join("fixture.txt"), "synthetic\n").expect("fixture");
    git(scratch.path(), &["add", "fixture.txt"]);
    git(scratch.path(), &["commit", "-qm", "synthetic fixture"]);
    let name = format!("screen {CANARY}");
    let route = format!("/{CANARY}");
    let input = params(&name, &route);
    let runtime = VisualRuntime {
        reference_root: scratch.path().into(),
        renderer_sha256: Some("a".repeat(64)),
        renderer_version: Some("synthetic renderer".into()),
        capture_clock: None,
        fixture_sha256: None,
        captures: Vec::new(),
        diff_root: None,
    };
    for version in ["HEAD", "missing-version"] {
        let report = check_visual_with(scratch.path(), version, &input, &runtime);
        assert_eq!(report.outcome.state, VerdictState::Unverifiable);
        assert_eq!(report.screens[0].screen, Masked::mask(&name).into_text());
        assert_eq!(report.screens[0].route, Masked::mask(&route).into_text());
        assert!(!serde_json::to_string(&report)
            .expect("report JSON")
            .contains("TLCANARY"));
        assert_eq!(
            input.screens[0].screen, name,
            "raw input must still bind the check"
        );
        assert_eq!(input.screens[0].route, route);
        if version == "HEAD" {
            assert_eq!(
                report.outcome.tree_hash.as_deref(),
                Some(git(scratch.path(), &["rev-parse", "HEAD^{tree}"]).as_str())
            );
        }
    }
    let plain = params("Inbox", "/");
    let report = check_visual_with(scratch.path(), "HEAD", &plain, &runtime);
    assert_eq!(report.screens[0].screen, "Inbox");
    assert_eq!(report.screens[0].route, "/");
    assert_eq!(report.outcome.state, VerdictState::Unverifiable);
}

struct RestoreEnv(Vec<(&'static str, Option<OsString>)>);
impl RestoreEnv {
    fn set(values: &[(&'static str, OsString)]) -> Self {
        let original = values
            .iter()
            .map(|(name, value)| {
                let old = std::env::var_os(name);
                std::env::set_var(name, value);
                (*name, old)
            })
            .collect();
        Self(original)
    }
}
impl Drop for RestoreEnv {
    fn drop(&mut self) {
        for (name, original) in &self.0 {
            if let Some(value) = original {
                std::env::set_var(name, value);
            } else {
                std::env::remove_var(name);
            }
        }
    }
}

#[test]
fn visual_tool_identity_masks_a_credential_straddling_its_display_budget() {
    let scratch = tempfile::tempdir().expect("synthetic cache");
    let input = format!("{}{CANARY}", "version ".repeat(13));
    assert!(input.chars().count() > 120);
    assert!(input
        .chars()
        .take(120)
        .collect::<String>()
        .contains("TLCANARY"));
    let _restore = RestoreEnv::set(&[
        (
            "THRULINE_VISUAL_RENDERER",
            scratch.path().join("absent-renderer").into_os_string(),
        ),
        ("THRULINE_VISUAL_RENDERER_SHA256", input.clone().into()),
        (
            "THRULINE_VISUAL_CACHE_DIR",
            scratch.path().join("cache").into_os_string(),
        ),
    ]);
    let identity = visual_tool_identity(&params("Inbox", "/"));
    let expected = Masked::mask(&input)
        .into_text()
        .chars()
        .take(120)
        .collect::<String>();
    assert_eq!(identity["renderer_pin_sha256"], expected);
    assert!(!serde_json::to_string(&identity)
        .expect("identity JSON")
        .contains("TLCANARY"));
    assert!(
        identity["renderer_sha256"].is_null(),
        "an invalid pin must not approve a renderer"
    );
    assert!(identity["renderer_version"].is_null());
}
