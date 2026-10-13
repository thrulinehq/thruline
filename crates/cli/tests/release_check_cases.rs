//! Card INFRA-RELEASE-CHECK: `release/macos/lib.sh`'s `check_release_binary`
//! must refuse a release build only on evidence that the debug seam accepted a
//! test-signed licence, not merely because `approve_version` is allowed, which
//! is true of every licence state while the paid gate is off (R-2026-09-24-25).
//!
//! This runs `release/macos/check-release-binary-cases.sh`, the decision table
//! that drives the real `check_release_binary` against twenty-eight
//! stand-ins plus one check of its seam list against the source, so the gate
//! holds the release check to refusing whatever it cannot read (round 2) and,
//! since card RELEASE-CHECK-GATE-ON (round 3, D-244), to refusing a build whose
//! paid gate is off, that trusts no key or not the production key
//! 3ef3540d43dbf105, that carries or honours any debug-only seam, or whose
//! fresh home is not the as-shipped start state.
#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| panic!("no repository root above {}", env!("CARGO_MANIFEST_DIR")))
}

#[test]
fn the_release_check_refuses_only_when_the_seam_accepted_a_licence() {
    let script = root().join("release/macos/check-release-binary-cases.sh");
    let out = Command::new("bash")
        .arg(&script)
        .output()
        .unwrap_or_else(|e| panic!("could not run {}: {e}", script.display()));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.status.success(),
        "check_release_binary's decision table has a wrong verdict:\n{text}"
    );
    assert!(
        text.contains("36/36 cases right"),
        "expected all thirty-six cases to run and be right:\n{text}"
    );
}
