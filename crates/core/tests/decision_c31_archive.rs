//! C31: an accepted future docs condition stays dormant without four actual archives.

use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;
use thruline_core::decisions::c31_archive::{
    resolve_archive_absences, ArchiveAbsentScope, RequiredSourceLine,
};

const ARCHIVES: [&str; 4] = [
    "thruline-cli-aarch64-apple-darwin.tar.gz",
    "thruline-cli-x86_64-apple-darwin.tar.gz",
    "thruline-cli-aarch64-unknown-linux-musl.tar.gz",
    "thruline-cli-x86_64-unknown-linux-musl.tar.gz",
];

fn scope() -> ArchiveAbsentScope {
    ArchiveAbsentScope {
        pattern: "docs/**".into(),
        release_version: "v0.2.0".into(),
        proof_path: "target/distrib/decisions-ship-proof.json".into(),
        required_source_line: None,
    }
}

fn release_manifests(root: &Path) {
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace.package]\nversion='0.2.0'\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("crates/cli")).unwrap();
    std::fs::write(
        root.join("crates/cli/Cargo.toml"),
        "[package]\nname='thruline-cli'\nversion.workspace=true\n",
    )
    .unwrap();
}

fn checked_commit(root: &Path) -> String {
    git(root, &["rev-parse", "HEAD"])
}

fn archives(root: &Path, with_docs: bool) {
    if !root.join(".git").exists() {
        git(root, &["init", "-q"]);
        release_manifests(root);
        git(root, &["add", "Cargo.toml", "crates/cli/Cargo.toml"]);
        git(
            root,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "release source fixture",
            ],
        );
    }
    let stage = root.join("staging/thruline-cli-v0.2.0");
    std::fs::create_dir_all(stage.join("bin")).unwrap();
    std::fs::write(stage.join("bin/thruline"), "private fixture").unwrap();
    if with_docs {
        std::fs::create_dir_all(stage.join("docs")).unwrap();
        std::fs::write(
            stage.join("docs/plan.md"),
            include_str!("fixtures/decisions/c31/legacy-label.txt"),
        )
        .unwrap();
    }
    let dist = root.join("target/distrib");
    std::fs::create_dir_all(&dist).unwrap();
    for file_name in ARCHIVES {
        let out = Command::new("tar")
            .arg("-czf")
            .arg(dist.join(file_name))
            .arg("-C")
            .arg(root.join("staging"))
            .arg("thruline-cli-v0.2.0")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

fn write_proof(root: &Path, commit: &str) {
    let files = ARCHIVES
        .iter()
        .map(|name| {
            let bytes = std::fs::read(root.join("target/distrib").join(name)).unwrap();
            serde_json::json!({"file_name":name,"sha256":Sha256::digest(&bytes).iter().map(|byte| format!("{byte:02x}")).collect::<String>()})
        })
        .collect::<Vec<_>>();
    let proof = serde_json::json!({
        "schema_version":1,
        "release_version":"v0.2.0",
        "checked_commit":commit,
        "source":"private-release-dry-run",
        "archives":files
    });
    std::fs::write(
        root.join("target/distrib/decisions-ship-proof.json"),
        proof.to_string(),
    )
    .unwrap();
}

#[test]
fn missing_proof_never_excludes_docs() {
    let root = tempfile::tempdir().unwrap();
    archives(root.path(), false);
    let result = resolve_archive_absences(root.path(), &checked_commit(root.path()), &[scope()]);
    assert!(result.patterns.is_empty());
    assert_eq!(result.missing.len(), 1);
    assert!(result.missing[0].contains("v0.2.0"));
}

#[test]
fn four_hashed_archives_without_docs_activate_exact_pattern() {
    let root = tempfile::tempdir().unwrap();
    archives(root.path(), false);
    write_proof(root.path(), &checked_commit(root.path()));
    let result = resolve_archive_absences(root.path(), &checked_commit(root.path()), &[scope()]);
    assert_eq!(result.patterns, vec!["docs/**"]);
    assert!(result.missing.is_empty());
}

#[test]
fn docs_in_any_archive_keeps_condition_dormant() {
    let root = tempfile::tempdir().unwrap();
    archives(root.path(), true);
    write_proof(root.path(), &checked_commit(root.path()));
    let result = resolve_archive_absences(root.path(), &checked_commit(root.path()), &[scope()]);
    assert!(result.patterns.is_empty());
    assert!(result.missing.join(" ").contains("docs/**"));
}

#[test]
fn docs_nested_below_an_archive_folder_still_count_as_shipped() {
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("staging/thruline-cli-v0.2.0/extras/docs");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(
        nested.join("history.md"),
        include_str!("fixtures/decisions/c31/legacy-label.txt"),
    )
    .unwrap();
    archives(root.path(), false);
    write_proof(root.path(), &checked_commit(root.path()));
    let result = resolve_archive_absences(root.path(), &checked_commit(root.path()), &[scope()]);
    assert!(result.patterns.is_empty());
    assert!(result.missing.join(" ").contains("docs/**"));
}

#[test]
fn exact_nested_markdown_member_keeps_condition_dormant() {
    let root = tempfile::tempdir().unwrap();
    let nested = root.path().join("staging/thruline-cli-v0.2.0/notes");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("PLAN.md"), "release notes").unwrap();
    archives(root.path(), false);
    write_proof(root.path(), &checked_commit(root.path()));
    let mut exact = scope();
    exact.pattern = "notes/PLAN.md".into();
    let result = resolve_archive_absences(root.path(), &checked_commit(root.path()), &[exact]);
    assert!(
        result.patterns.is_empty(),
        "a shipped nested path is present"
    );
    assert!(result.missing.join(" ").contains("notes/PLAN.md"));
}

#[test]
fn mismatched_commit_or_tampered_archive_keeps_condition_dormant() {
    let root = tempfile::tempdir().unwrap();
    archives(root.path(), false);
    write_proof(root.path(), "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    assert!(
        resolve_archive_absences(root.path(), &checked_commit(root.path()), &[scope()])
            .patterns
            .is_empty()
    );
    write_proof(root.path(), &checked_commit(root.path()));
    std::fs::write(
        root.path().join("target/distrib").join(ARCHIVES[0]),
        "tampered",
    )
    .unwrap();
    assert!(
        resolve_archive_absences(root.path(), &checked_commit(root.path()), &[scope()])
            .patterns
            .is_empty()
    );
}

#[test]
fn proof_without_all_four_present_archives_cannot_activate_condition() {
    let root = tempfile::tempdir().unwrap();
    archives(root.path(), false);
    write_proof(root.path(), &checked_commit(root.path()));
    std::fs::remove_file(root.path().join("target/distrib").join(ARCHIVES[3])).unwrap();
    let result = resolve_archive_absences(root.path(), &checked_commit(root.path()), &[scope()]);
    assert!(result.patterns.is_empty());
    assert_eq!(result.missing.len(), 1);
}

#[test]
fn an_extra_platform_archive_not_listed_in_proof_keeps_condition_dormant() {
    let root = tempfile::tempdir().unwrap();
    archives(root.path(), false);
    write_proof(root.path(), &checked_commit(root.path()));
    std::fs::write(
        root.path()
            .join("target/distrib/thruline-cli-extra-platform.tar.gz"),
        "extra",
    )
    .unwrap();
    assert!(
        resolve_archive_absences(root.path(), &checked_commit(root.path()), &[scope()])
            .patterns
            .is_empty()
    );
}

#[cfg(unix)]
#[test]
fn symlink_proof_and_archive_are_not_accepted() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    archives(root.path(), false);
    write_proof(root.path(), &checked_commit(root.path()));
    let path = root.path().join("target/distrib/decisions-ship-proof.json");
    let actual = root.path().join("real-proof.json");
    std::fs::rename(&path, &actual).unwrap();
    symlink(&actual, &path).unwrap();
    assert!(
        resolve_archive_absences(root.path(), &checked_commit(root.path()), &[scope()])
            .patterns
            .is_empty()
    );
    std::fs::remove_file(&path).unwrap();
    std::fs::rename(&actual, &path).unwrap();
    let archive = root.path().join("target/distrib").join(ARCHIVES[0]);
    let actual = root.path().join("actual-archive.tar.gz");
    std::fs::rename(&archive, &actual).unwrap();
    symlink(&actual, &archive).unwrap();
    assert!(
        resolve_archive_absences(root.path(), &checked_commit(root.path()), &[scope()])
            .patterns
            .is_empty()
    );
}

#[cfg(unix)]
#[test]
fn symlink_distribution_directory_is_not_an_inventory_source() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    archives(root.path(), false);
    write_proof(root.path(), &checked_commit(root.path()));
    let original = root.path().join("target/distrib");
    let moved = root.path().join("outside-distrib");
    std::fs::rename(&original, &moved).unwrap();
    symlink(&moved, &original).unwrap();
    assert!(
        resolve_archive_absences(root.path(), &checked_commit(root.path()), &[scope()])
            .patterns
            .is_empty()
    );
}

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {:?}: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

const MARKER: &str = include_str!("fixtures/decisions/c31/process-marker.txt");

fn source_fixture(marker: &str) -> (tempfile::TempDir, String) {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "-q"]);
    std::fs::write(
        repo.path().join("CLAUDE.md"),
        format!("# {marker}\nprocess history\n"),
    )
    .unwrap();
    release_manifests(repo.path());
    git(
        repo.path(),
        &["add", "CLAUDE.md", "Cargo.toml", "crates/cli/Cargo.toml"],
    );
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
    );
    let head = git(repo.path(), &["rev-parse", "HEAD"]);
    (repo, head)
}

fn process_scope() -> ArchiveAbsentScope {
    ArchiveAbsentScope {
        pattern: "CLAUDE.md".into(),
        release_version: "v0.2.0".into(),
        proof_path: "target/distrib/decisions-ship-proof.json".into(),
        required_source_line: Some(RequiredSourceLine {
            path: "CLAUDE.md".into(),
            text: MARKER.into(),
        }),
    }
}

#[test]
fn process_history_exclusion_needs_exact_committed_supersession_line() {
    let (repo, head) = source_fixture("C20 — NO TRIAL (supersedes TRIAL-STARTS-AT-FIRST-APPROVAL)");
    archives(repo.path(), false);
    write_proof(repo.path(), &head);
    let mut live = process_scope();
    live.pattern = "LIVE-HANDOFF.md".into();
    let mut report = process_scope();
    report.pattern = "REPORT.md".into();
    let scopes = [process_scope(), live, report];
    let missing = resolve_archive_absences(repo.path(), &head, &scopes);
    assert!(missing.patterns.is_empty());
    assert_eq!(missing.missing.len(), 3);
    assert!(missing.missing.join(" ").contains("CLAUDE.md"));
    // Working-tree prose cannot repair the exact commit being checked.
    std::fs::write(repo.path().join("CLAUDE.md"), format!("# {MARKER}\n")).unwrap();
    assert!(resolve_archive_absences(repo.path(), &head, &scopes)
        .patterns
        .is_empty());
    git(repo.path(), &["add", "CLAUDE.md"]);
    git(
        repo.path(),
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "C20",
        ],
    );
    let new_head = git(repo.path(), &["rev-parse", "HEAD"]);
    write_proof(repo.path(), &new_head);
    let result = resolve_archive_absences(repo.path(), &new_head, &scopes);
    assert_eq!(
        result.patterns,
        vec!["CLAUDE.md", "LIVE-HANDOFF.md", "REPORT.md"]
    );
    assert!(result.missing.is_empty());
}
