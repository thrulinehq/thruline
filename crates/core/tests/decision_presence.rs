use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};
use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::params::{FeatureDefaultPreservedParams, PathPresentParams};

mod decisions {
    pub use thruline_core::decisions::{checks, params};
}

mod redact {
    pub use thruline_core::redact::Masked;
}

#[path = "../src/decisions/checks_presence.rs"]
mod checks_presence;

fn fixture(kind: &str, tree: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/decisions")
        .join(kind)
        .join(tree)
}

fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn git(repo: &Path, args: &[&str]) -> io::Result<String> {
    let output = Command::new("git").args(args).current_dir(repo).output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn repo_for(kind: &str, tree: &str) -> Result<tempfile::TempDir, Box<dyn Error>> {
    let repo = tempfile::tempdir()?;
    copy_tree(&fixture(kind, tree), repo.path())?;
    git(repo.path(), &["init", "-q"])?;
    git(repo.path(), &["add", "-A"])?;
    git(
        repo.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    )?;
    Ok(repo)
}

fn sha256(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn b1_path_present_holds_and_missing_path_is_broken() -> Result<(), Box<dyn Error>> {
    let holds = repo_for("path_present", "holds")?;
    let broken = repo_for("path_present", "broken")?;
    let params = PathPresentParams {
        path: "REQUIRED.txt".to_string(),
        sha256: None,
    };
    let held = checks_presence::check_path_present(holds.path(), "HEAD", &params);
    let missing = checks_presence::check_path_present(broken.path(), "HEAD", &params);
    assert_eq!(held.state, VerdictState::Holds);
    assert!(held.findings.is_empty());
    assert_eq!(missing.state, VerdictState::Broken);
    assert_eq!(missing.findings.len(), 1);
    assert_eq!(missing.findings[0].path, "REQUIRED.txt");
    assert_eq!(missing.findings[0].line, 0);
    assert_eq!(
        held.tree_hash.as_deref(),
        Some(git(holds.path(), &["rev-parse", "HEAD^{tree}"])?.as_str())
    );
    Ok(())
}

#[test]
fn b1_pinned_hash_uses_the_committed_blob_not_the_working_tree() -> Result<(), Box<dyn Error>> {
    let holds = repo_for("path_present", "holds")?;
    let broken = repo_for("path_present", "broken")?;
    let params = PathPresentParams {
        path: "LICENSE.txt".to_string(),
        sha256: Some(sha256("unchanged\n")),
    };
    let before = checks_presence::check_path_present(holds.path(), "HEAD", &params);
    assert_eq!(before.state, VerdictState::Holds);
    std::fs::write(
        holds.path().join("LICENSE.txt"),
        "changed in working tree\n",
    )?;
    let same_version = checks_presence::check_path_present(holds.path(), "HEAD", &params);
    assert_eq!(same_version, before);
    let changed_commit = checks_presence::check_path_present(broken.path(), "HEAD", &params);
    assert_eq!(changed_commit.state, VerdictState::Broken);
    assert_eq!(changed_commit.findings[0].path, "LICENSE.txt");
    Ok(())
}

#[test]
fn b6_preserves_rust_and_manifest_defaults() -> Result<(), Box<dyn Error>> {
    let holds = repo_for("feature_default_preserved", "holds")?;
    let broken = repo_for("feature_default_preserved", "broken")?;
    for (path, symbol, expected, line) in [
        ("src/lib.rs", "PAID_GATE", "true", 2),
        ("Cargo.toml", "default-features", "false", 2),
    ] {
        let params = FeatureDefaultPreservedParams {
            path: path.to_string(),
            symbol: symbol.to_string(),
            expected_value: expected.to_string(),
        };
        let held = checks_presence::check_feature_default_preserved(holds.path(), "HEAD", &params);
        let changed =
            checks_presence::check_feature_default_preserved(broken.path(), "HEAD", &params);
        assert_eq!(held.state, VerdictState::Holds, "{symbol}");
        assert_eq!(changed.state, VerdictState::Broken, "{symbol}");
        assert_eq!(changed.findings[0].path, path);
        assert_eq!(changed.findings[0].line, line);
    }
    Ok(())
}

#[test]
fn b6_missing_symbol_is_broken_and_the_version_is_exact() -> Result<(), Box<dyn Error>> {
    let repo = repo_for("feature_default_preserved", "holds")?;
    let params = FeatureDefaultPreservedParams {
        path: "src/lib.rs".to_string(),
        symbol: "PAID_GATE".to_string(),
        expected_value: "true".to_string(),
    };
    let held = checks_presence::check_feature_default_preserved(repo.path(), "HEAD", &params);
    assert_eq!(held.state, VerdictState::Holds);
    std::fs::write(
        repo.path().join("src/lib.rs"),
        "pub const PAID_GATE: bool = false;\n",
    )?;
    assert_eq!(
        checks_presence::check_feature_default_preserved(repo.path(), "HEAD", &params),
        held
    );
    let absent = FeatureDefaultPreservedParams {
        symbol: "UNKNOWN_FLAG".to_string(),
        ..params
    };
    let missing = checks_presence::check_feature_default_preserved(repo.path(), "HEAD", &absent);
    assert_eq!(missing.state, VerdictState::Broken);
    assert_eq!(missing.findings[0].line, 0);
    Ok(())
}

#[test]
fn b6_can_preserve_an_absent_feature_default() -> Result<(), Box<dyn Error>> {
    let holds = repo_for("feature_default_preserved", "holds")?;
    let broken = repo_for("feature_default_preserved", "broken")?;
    let params = FeatureDefaultPreservedParams {
        path: "Cargo.toml".to_string(),
        symbol: "legacy_feature".to_string(),
        expected_value: "<absent>".to_string(),
    };
    let held = checks_presence::check_feature_default_preserved(holds.path(), "HEAD", &params);
    let added = checks_presence::check_feature_default_preserved(broken.path(), "HEAD", &params);
    assert_eq!(held.state, VerdictState::Holds);
    assert_eq!(added.state, VerdictState::Broken);
    assert_eq!(added.findings[0].line, 3);
    Ok(())
}
