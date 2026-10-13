use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde_json::Value;
use tempfile::tempdir;
use thruline_core::capture::{
    canonical_json, capture_stabilized, AttemptIntent, CaptureRequest, Collector,
};
use thruline_core::chain::sha256_hex;

const ITERATIONS: usize = 20;

fn git(cwd: &Path, args: &[&str]) {
    let status = Command::new("/usr/bin/git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed in {}", cwd.display());
}

fn base_repo(path: &Path) {
    fs::create_dir_all(path).unwrap();
    git(path, &["init", "-q"]);
    git(path, &["config", "user.name", "Performance Test"]);
    git(
        path,
        &["config", "user.email", "performance@example.invalid"],
    );
    fs::write(path.join("tracked.txt"), "base\n").unwrap();
    git(path, &["add", "tracked.txt"]);
    git(path, &["commit", "-qm", "base"]);
}

fn case_matrix(root: &Path) -> BTreeMap<String, PathBuf> {
    let mut cases = BTreeMap::new();
    let tiny = root.join("tiny");
    base_repo(&tiny);
    cases.insert("tiny_clean".to_string(), tiny.clone());

    let medium = root.join("medium");
    base_repo(&medium);
    for index in 0..1_000 {
        fs::write(
            medium.join(format!("tracked-{index:04}.txt")),
            vec![b'x'; 2_048],
        )
        .unwrap();
    }
    git(&medium, &["add", "."]);
    git(&medium, &["commit", "-qm", "medium"]);
    cases.insert("medium_clean".to_string(), medium);

    let staged = root.join("staged");
    base_repo(&staged);
    fs::write(staged.join("tracked.txt"), "staged\n").unwrap();
    git(&staged, &["add", "tracked.txt"]);
    cases.insert("staged_changes".to_string(), staged);

    let unstaged = root.join("unstaged");
    base_repo(&unstaged);
    fs::write(unstaged.join("tracked.txt"), "unstaged\n").unwrap();
    cases.insert("unstaged_changes".to_string(), unstaged);

    let untracked = root.join("untracked-1000");
    base_repo(&untracked);
    for index in 0..1_000 {
        fs::write(
            untracked.join(format!("new-{index:04}.txt")),
            b"untracked\n",
        )
        .unwrap();
    }
    cases.insert("untracked_1000".to_string(), untracked);

    let large = root.join("large-untracked");
    base_repo(&large);
    fs::write(large.join("large.bin"), vec![b'z'; 32 * 1024 * 1024]).unwrap();
    cases.insert("large_untracked_32_mib".to_string(), large);

    let sub_origin = root.join("sub-origin");
    base_repo(&sub_origin);
    let submodule = root.join("submodule");
    base_repo(&submodule);
    git(
        &submodule,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            sub_origin.to_str().unwrap(),
            "vendor/sub",
        ],
    );
    git(&submodule, &["commit", "-qm", "submodule"]);
    cases.insert("submodule".to_string(), submodule);

    let linked_source = root.join("linked-source");
    base_repo(&linked_source);
    let linked = root.join("linked-worktree");
    git(
        &linked_source,
        &[
            "worktree",
            "add",
            "-q",
            linked.to_str().unwrap(),
            "-b",
            "linked",
        ],
    );
    cases.insert("linked_worktree".to_string(), linked);

    let conflict = root.join("conflict");
    base_repo(&conflict);
    git(&conflict, &["checkout", "-qb", "other"]);
    fs::write(conflict.join("tracked.txt"), "other\n").unwrap();
    git(&conflict, &["commit", "-qam", "other"]);
    git(&conflict, &["checkout", "-q", "master"]);
    fs::write(conflict.join("tracked.txt"), "master\n").unwrap();
    git(&conflict, &["commit", "-qam", "master"]);
    let merge = Command::new("/usr/bin/git")
        .args(["merge", "other"])
        .current_dir(&conflict)
        .status()
        .unwrap();
    assert!(!merge.success());
    cases.insert("merge_conflict".to_string(), conflict);

    let detached = root.join("detached");
    base_repo(&detached);
    git(&detached, &["checkout", "-q", "--detach"]);
    cases.insert("detached_head".to_string(), detached);
    cases
}

fn percentile_95(values: &mut [u64]) -> u64 {
    values.sort_unstable();
    let index = ((values.len() as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(values.len().saturating_sub(1));
    values[index]
}

fn directory_bytes(path: &Path) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![path.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let metadata = entry.metadata().unwrap();
            if metadata.is_dir() {
                stack.push(entry.path());
            } else {
                total += metadata.len();
            }
        }
    }
    total
}

#[test]
#[ignore = "M5.R1 release performance gate"]
fn required_synthetic_matrix_meets_capture_budgets() {
    let temp = tempdir().unwrap();
    let matrix = case_matrix(temp.path());
    let mut output = serde_json::Map::new();
    let secret = [7u8; 32];
    for (name, repo) in &matrix {
        let mut durations = Vec::new();
        let mut sizes = Vec::new();
        let mut complete = true;
        for _ in 0..ITERATIONS {
            let started = Instant::now();
            let state = capture_stabilized(repo, "git status --porcelain", &secret).unwrap();
            durations.push(started.elapsed().as_micros() as u64);
            sizes.push(canonical_json(&state).unwrap().len() as u64);
            complete &= state.completeness.complete();
        }
        let p95_us = percentile_95(&mut durations);
        let p95_bytes = percentile_95(&mut sizes);
        if name == "submodule" {
            assert!(!complete);
        } else {
            assert!(complete, "{name} unexpectedly incomplete");
        }
        assert!(p95_us <= 350_000, "{name} p95 {p95_us} us");
        assert!(p95_bytes <= 8 * 1024, "{name} capsule {p95_bytes} bytes");
        output.insert(
            name.clone(),
            serde_json::json!({
                "iterations": ITERATIONS,
                "stabilized_p95_us": p95_us,
                "capsule_p95_bytes": p95_bytes,
                "complete": complete,
            }),
        );
    }

    let collector = Collector::open(temp.path().join("capture"), "owner-perf", "codex").unwrap();
    let mut pair_us = Vec::new();
    let mut pair_growth = Vec::new();
    for index in 0..ITERATIONS {
        let before = directory_bytes(collector.root());
        let request = CaptureRequest {
            owner_key: "owner-perf".to_string(),
            source: "codex".to_string(),
            source_project_key: "cap-project-perf".to_string(),
            source_session_key: "session-perf".to_string(),
            source_turn_id: None,
            source_tool_id: format!("tool-{index}"),
            wrapper_nonce: format!("nonce-{index}"),
            command: "git status --porcelain".to_string(),
            cwd: matrix["tiny_clean"].clone(),
            source_config_sha256: sha256_hex(b"config"),
            source_version: "performance".to_string(),
            assurance_enabled: false,
        };
        let started = Instant::now();
        let outcome = collector.execute(&request).unwrap();
        pair_us.push(started.elapsed().as_micros() as u64);
        assert_eq!(outcome.exit_code, 0);
        pair_growth.push(directory_bytes(collector.root()).saturating_sub(before));
    }
    let pair_p95_us = percentile_95(&mut pair_us);
    let pair_p95_bytes = percentile_95(&mut pair_growth);
    assert!(pair_p95_us <= 700_000, "pair p95 {pair_p95_us} us");
    assert!(pair_p95_bytes <= 24 * 1024, "pair growth {pair_p95_bytes}");
    output.insert(
        "command_pair".to_string(),
        serde_json::json!({
            "iterations": ITERATIONS,
            "capture_only_p95_us_including_small_command": pair_p95_us,
            "private_growth_p95_bytes": pair_p95_bytes,
        }),
    );
    println!("M5_CAPTURE_PERFORMANCE={}", Value::Object(output));
}

#[test]
#[ignore = "M5.R1 1000-orphan recovery performance gate"]
fn thousand_orphans_recover_under_five_seconds() {
    let temp = tempdir().unwrap();
    let collector = Collector::open(temp.path(), "owner-recovery", "codex").unwrap();
    for index in 0..1_000 {
        collector
            .write_intent(&AttemptIntent {
                schema_version: 1,
                attempt_id: format!("cap_{index:064x}"),
                owner_key: "owner-recovery".to_string(),
                source: "codex".to_string(),
                source_project_key: "cap-project-recovery".to_string(),
                source_session_key: "session-recovery".to_string(),
                source_turn_id: None,
                source_tool_id: format!("tool-{index}"),
                wrapper_nonce: format!("nonce-{index}"),
                command_sha256: sha256_hex(b"git status"),
                source_config_sha256: sha256_hex(b"config"),
                source_version: "recovery".to_string(),
                command: "git status".to_string(),
                cwd: "private".to_string(),
                created_at_unix_ms: 1,
            })
            .unwrap();
    }
    let started = Instant::now();
    let recovered = collector.recover_orphans().unwrap();
    let elapsed_ms = started.elapsed().as_millis();
    assert_eq!(recovered, 1_000);
    assert!(elapsed_ms <= 5_000, "recovery took {elapsed_ms} ms");
    assert_eq!(collector.recover_orphans().unwrap(), 0);
    println!("M5_ORPHAN_RECOVERY={{\"records\":1000,\"elapsed_ms\":{elapsed_ms}}}");
}
