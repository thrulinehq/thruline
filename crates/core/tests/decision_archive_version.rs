//! Archive exclusions require the actual package version at the checked commit.

use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;
use thruline_core::decisions::c31_archive::{resolve_archive_absences, ArchiveAbsentScope};

const ARCHIVES: [&str; 4] = [
    "thruline-cli-aarch64-apple-darwin.tar.gz",
    "thruline-cli-x86_64-apple-darwin.tar.gz",
    "thruline-cli-aarch64-unknown-linux-musl.tar.gz",
    "thruline-cli-x86_64-unknown-linux-musl.tar.gz",
];

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8(output.stdout).unwrap().trim().into()
}

fn fixture(workspace: Option<&str>, cli: Option<&str>) -> (tempfile::TempDir, String) {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "-q"]);
    std::fs::write(root.path().join("README.md"), "version fixture\n").unwrap();
    if let Some(manifest) = workspace {
        std::fs::write(root.path().join("Cargo.toml"), manifest).unwrap();
    }
    if let Some(manifest) = cli {
        std::fs::create_dir_all(root.path().join("crates/cli")).unwrap();
        std::fs::write(root.path().join("crates/cli/Cargo.toml"), manifest).unwrap();
    }
    git(root.path(), &["add", "."]);
    git(
        root.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "version",
        ],
    );
    let head = git(root.path(), &["rev-parse", "HEAD"]);
    let stage = root.path().join("stage/thruline-cli-fixture");
    let dist = root.path().join("target/distrib");
    std::fs::create_dir_all(&stage).unwrap();
    std::fs::create_dir_all(&dist).unwrap();
    std::fs::write(stage.join("thruline"), "synthetic archive member\n").unwrap();
    let mut items = Vec::new();
    for name in ARCHIVES {
        let archive = dist.join(name);
        let output = Command::new("tar")
            .arg("-czf")
            .arg(&archive)
            .arg("-C")
            .arg(root.path().join("stage"))
            .arg("thruline-cli-fixture")
            .output()
            .unwrap();
        assert!(output.status.success());
        let sha = Sha256::digest(std::fs::read(archive).unwrap())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        items.push(serde_json::json!({"file_name":name,"sha256":sha}));
    }
    let proof = serde_json::json!({"schema_version":1,"release_version":"v0.2.0",
        "checked_commit":head,"source":"private-release-dry-run","archives":items});
    std::fs::write(dist.join("decisions-ship-proof.json"), proof.to_string()).unwrap();
    (root, head)
}

fn workspace(version: &str) -> String {
    format!("[workspace.package]\nversion = {version}\n")
}

const CLI_INHERITED: &str = "[package]\nname = 'thruline-cli'\nversion.workspace = true\n";

fn applied(root: &Path, head: &str) -> bool {
    let scope = ArchiveAbsentScope {
        pattern: "docs/**".into(),
        release_version: "v0.2.0".into(),
        proof_path: "target/distrib/decisions-ship-proof.json".into(),
        required_source_line: None,
    };
    let result = resolve_archive_absences(root, head, &[scope]);
    if result.patterns.is_empty() {
        eprintln!("version refusal: {result:?}");
        assert_eq!(result.missing.len(), 1);
        assert!(
            result.missing[0].contains("source package version"),
            "{result:?}"
        );
        false
    } else {
        assert_eq!(result.patterns, ["docs/**"]);
        assert!(result.missing.is_empty());
        true
    }
}

#[test]
fn prospective_label_cannot_activate_archives_from_committed_older_package() {
    let (root, head) = fixture(Some(&workspace("'0.1.0'")), Some(CLI_INHERITED));
    assert!(!applied(root.path(), &head));
}

#[test]
fn committed_matching_workspace_and_inherited_cli_activate_the_condition() {
    let (root, head) = fixture(Some(&workspace("'0.2.0'")), Some(CLI_INHERITED));
    assert!(applied(root.path(), &head));
}

#[test]
fn explicit_matching_cli_version_is_supported() {
    let cli = "[package]\nname='thruline-cli'\nversion='0.2.0'\n";
    let (root, head) = fixture(Some(&workspace("'0.2.0'")), Some(cli));
    assert!(applied(root.path(), &head));
}

#[test]
fn cli_override_cannot_disagree_with_matching_workspace() {
    let cli = "[package]\nname='thruline-cli'\nversion='0.1.0'\n";
    let (root, head) = fixture(Some(&workspace("'0.2.0'")), Some(cli));
    assert!(!applied(root.path(), &head));
}

#[test]
fn missing_manifests_never_activate_the_condition() {
    for (workspace, cli) in [
        (None, Some(CLI_INHERITED)),
        (Some(workspace("'0.2.0'")), None),
    ] {
        let (root, head) = fixture(workspace.as_deref(), cli);
        assert!(!applied(root.path(), &head));
    }
}

#[test]
fn malformed_or_ambiguous_manifest_versions_fail_closed() {
    for manifest in [
        "[workspace.package]\nversion='0.2.0'\nversion='0.1.0'\n",
        "[workspace.package]\nversion=true\n",
        "[workspace.package\nversion='0.2.0'\n",
    ] {
        let (root, head) = fixture(Some(manifest), Some(CLI_INHERITED));
        assert!(!applied(root.path(), &head));
    }
    for cli in [
        "[package]\nname='thruline-cli'\n",
        "[package]\nname='other'\nversion='0.2.0'\n",
        "[package]\nname='thruline-cli'\nversion.workspace=false\n",
        "[package]\nname='thruline-cli'\nversion={ workspace=true, other='0.2.0' }\n",
        "[package]\nname='thruline-cli'\nversion='0.2.0'\nversion.workspace=true\n",
        "[workspace.package]\nversion='0.1.0'\n[package]\nname='thruline-cli'\nversion.workspace=true\n",
        "[package]\nname='thruline-cli'\nworkspace='../../other'\nversion.workspace=true\n",
    ] {
        let (root, head) = fixture(Some(&workspace("'0.2.0'")), Some(cli));
        assert!(!applied(root.path(), &head));
    }
}

#[test]
fn mutable_worktree_versions_cannot_repair_or_break_checked_commit() {
    for (recorded, mutable, expected) in
        [("'0.1.0'", "'0.2.0'", false), ("'0.2.0'", "'0.1.0'", true)]
    {
        let (root, head) = fixture(Some(&workspace(recorded)), Some(CLI_INHERITED));
        std::fs::write(root.path().join("Cargo.toml"), workspace(mutable)).unwrap();
        assert_eq!(applied(root.path(), &head), expected);
    }
}

#[cfg(unix)]
#[test]
fn committed_manifest_symlink_is_not_a_version_receipt() {
    use std::os::unix::fs::symlink;
    let (root, _) = fixture(Some(&workspace("'0.2.0'")), Some(CLI_INHERITED));
    let manifest = root.path().join("crates/cli/Cargo.toml");
    std::fs::rename(&manifest, root.path().join("real-cli.toml")).unwrap();
    symlink("../../real-cli.toml", &manifest).unwrap();
    git(
        root.path(),
        &["add", "crates/cli/Cargo.toml", "real-cli.toml"],
    );
    git(
        root.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "alias",
        ],
    );
    let head = git(root.path(), &["rev-parse", "HEAD"]);
    let dist = root.path().join("target/distrib/decisions-ship-proof.json");
    let mut proof: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&dist).unwrap()).unwrap();
    proof["checked_commit"] = head.clone().into();
    std::fs::write(dist, proof.to_string()).unwrap();
    assert!(!applied(root.path(), &head));
}
