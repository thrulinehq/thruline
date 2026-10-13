//! C44 E8: intended local dry run and explicit external C31 proof location.
//! The tar inputs are explicitly synthetic, never native platform artifacts.

use std::path::{Path, PathBuf};
use std::process::Command;
use thruline_core::decisions::c31_archive::{resolve_archive_absences, ArchiveAbsentScope};

fn scripts() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/distrib")
        .canonicalize()
        .unwrap()
}

fn assert_success(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn consume(repo: &Path, build_target: &Path, target: Option<&Path>, commit: &str, expected: bool) {
    // A separate process keeps an explicit location override from contaminating
    // concurrent/default-path tests in this test binary or other binaries.
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "external_consumer_child", "--nocapture"])
        .env("E8_REPO", repo)
        .env("E8_COMMIT", commit)
        .env("E8_EXPECT_APPLIED", expected.to_string())
        .env("CARGO_TARGET_DIR", build_target)
        .env_remove("THRULINE_SHIP_PROOF_TARGET_DIR");
    if let Some(target) = target {
        command.env("THRULINE_SHIP_PROOF_TARGET_DIR", target);
    }
    assert_success(&mut command);
}

#[test]
fn external_consumer_child() {
    let Ok(repo) = std::env::var("E8_REPO") else {
        return;
    };
    let commit = std::env::var("E8_COMMIT").unwrap();
    let expected = std::env::var("E8_EXPECT_APPLIED").unwrap() == "true";
    let scope = ArchiveAbsentScope {
        pattern: "docs/**".into(),
        release_version: "v0.2.0".into(),
        proof_path: "target/distrib/decisions-ship-proof.json".into(),
        required_source_line: None,
    };
    let result = resolve_archive_absences(Path::new(&repo), &commit, &[scope]);
    assert_eq!(
        result.patterns == ["docs/**"],
        expected,
        "external proof result: {result:?}"
    );
    assert_eq!(result.missing.is_empty(), expected);
}

#[test]
fn local_dry_run_external_target_requires_explicit_override_and_preserves_guards() {
    let parent = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let scratch = tempfile::Builder::new()
        .prefix("e8-synthetic-")
        .tempdir_in(parent)
        .unwrap();
    let root = scratch.path().canonicalize().unwrap();
    let home = root.join("private-home");
    std::fs::create_dir(&home).unwrap();
    let repo = root.join("synthetic-repo");
    let target = root.join("external-target");
    assert_success(
        Command::new("python3")
            .arg(scripts().join("test_ship_proof.py"))
            .arg("--fixture")
            .arg(&root)
            .env("HOME", &home)
            .env("THRULINE_HOME", home.join(".thruline"))
            .env("PYTHONDONTWRITEBYTECODE", "1"),
    );
    // The consumer's positive fixture pins a matching package version. These
    // manifests are authored test data, not a change to the repository version.
    std::fs::write(
        repo.join("Cargo.toml"),
        "[workspace.package]\nversion='0.2.0'\n",
    )
    .unwrap();
    std::fs::create_dir_all(repo.join("crates/cli")).unwrap();
    std::fs::write(
        repo.join("crates/cli/Cargo.toml"),
        "[package]\nname='thruline-cli'\nversion.workspace=true\n",
    )
    .unwrap();
    assert_success(Command::new("git").arg("-C").arg(&repo).args([
        "add",
        "Cargo.toml",
        "crates/cli/Cargo.toml",
    ]));
    assert_success(Command::new("git").arg("-C").arg(&repo).args([
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-qm",
        "matching source package fixture",
    ]));
    assert_success(
        Command::new("python3")
            .arg(scripts().join("local-dry-run.py"))
            .arg("--repo")
            .arg(&repo)
            .arg("--archives-dir")
            .arg(root.join("fixture-archives"))
            .args(["--artifact-kind", "synthetic-fixture"])
            .env("HOME", &home)
            .env("THRULINE_HOME", home.join(".thruline"))
            .env("CARGO_TARGET_DIR", &target)
            .env("THRULINE_SHIP_PROOF_TARGET_DIR", &target)
            .env("PYTHONDONTWRITEBYTECODE", "1"),
    );
    let proof_path = target.join("distrib/decisions-ship-proof.json");
    let proof: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&proof_path).unwrap()).unwrap();
    let commit = proof["checked_commit"].as_str().unwrap();
    let receipt: serde_json::Value = serde_json::from_slice(
        &std::fs::read(target.join("distrib/decisions-local-dry-run.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["artifact_kind"], "synthetic-fixture");
    assert_eq!(receipt["native_platform_build_proved"], false);
    assert!(!repo.join("target").exists());

    consume(&repo, &target, Some(&target), commit, true);
    // CARGO_TARGET_DIR by itself is deliberately not a proof-location switch.
    consume(&repo, &target, None, commit, false);
    consume(&repo, &target, Some(&target), &"f".repeat(40), false);
    consume(
        &repo,
        &target,
        Some(Path::new("relative-target")),
        commit,
        false,
    );
    #[cfg(unix)]
    {
        let alias = root.join("target-alias");
        std::os::unix::fs::symlink(&target, &alias).unwrap();
        consume(&repo, &target, Some(&alias), commit, false);
        let proof_alias = root.join("proof-alias.json");
        std::fs::hard_link(&proof_path, &proof_alias).unwrap();
        consume(&repo, &target, Some(&target), commit, false);
        std::fs::remove_file(proof_alias).unwrap();
    }
    let archive = target.join("distrib/thruline-cli-aarch64-apple-darwin.tar.gz");
    std::fs::write(archive, b"tampered synthetic archive").unwrap();
    consume(&repo, &target, Some(&target), commit, false);
}
