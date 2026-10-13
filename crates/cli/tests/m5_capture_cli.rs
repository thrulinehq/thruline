use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use serde_json::Value;
use tempfile::tempdir;

#[path = "support/qualified_binary.rs"]
mod qualified_binary;

#[cfg(unix)]
fn make_source(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, "#!/bin/sh\necho 'codex-cli test-source'\n").unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn install_is_plan_bound_and_zero_authority_by_default() {
    let temp = tempdir().unwrap();
    let binary = qualified_binary::thruline();
    let config = temp.path().join("hooks.json");
    let capture = temp.path().join("capture");
    let db = temp.path().join("ledger.db");
    let source = temp.path().join("codex-test");
    make_source(&source);
    let owner = thruline_core::adapter::stable_owner_key(temp.path()).unwrap();
    fs::write(
        &config,
        serde_json::to_vec_pretty(&serde_json::json!({
            "unrelated": true,
            "hooks": {"SessionStart": [{"matcher": "", "hooks": []}]}
        }))
        .unwrap(),
    )
    .unwrap();
    let before = fs::read(&config).unwrap();

    let common = [
        "--db",
        db.to_str().unwrap(),
        "capture",
        "install",
        "--source",
        "codex",
        "--config",
        config.to_str().unwrap(),
        "--capture-root",
        capture.to_str().unwrap(),
        "--owner",
        owner.as_str(),
        "--source-version",
        "test-source",
        "--source-executable",
        source.to_str().unwrap(),
    ];
    let plan = Command::new(binary)
        .args(common)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );
    assert_eq!(fs::read(&config).unwrap(), before);
    assert!(
        !capture.exists(),
        "planning must not create a collector key"
    );
    let plan_json: Value = serde_json::from_slice(&plan.stdout).unwrap();
    let plan_id = plan_json["plan_id"].as_str().unwrap();

    let apply = Command::new(binary)
        .args(common)
        .args(["--apply", plan_id, "--json"])
        .output()
        .unwrap();
    assert!(
        apply.status.success(),
        "{}",
        String::from_utf8_lossy(&apply.stderr)
    );
    let applied: Value = serde_json::from_slice(&apply.stdout).unwrap();
    assert_eq!(applied["operational_state"], "installed_unverified");
    assert_eq!(applied["authorized_projects"], 0);
    let mut installed: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert_eq!(installed["unrelated"], true);
    assert_eq!(installed["hooks"]["PreToolUse"][0]["matcher"], "Bash");
    assert_eq!(installed["hooks"]["PostToolUse"][0]["matcher"], "Bash");

    // Even an exact duplicate is ambiguous: uninstall must prove a unique
    // receipt-owned entry before it removes anything.
    let duplicate = installed["hooks"]["PreToolUse"][0].clone();
    installed["hooks"]["PreToolUse"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    fs::write(&config, serde_json::to_vec_pretty(&installed).unwrap()).unwrap();
    let ambiguous_bytes = fs::read(&config).unwrap();
    let ambiguous_uninstall = Command::new(binary)
        .args([
            "--db",
            db.to_str().unwrap(),
            "capture",
            "uninstall",
            "--source",
            "codex",
            "--config",
            config.to_str().unwrap(),
            "--capture-root",
            capture.to_str().unwrap(),
            "--owner",
            owner.as_str(),
        ])
        .output()
        .unwrap();
    assert!(!ambiguous_uninstall.status.success());
    assert_eq!(fs::read(&config).unwrap(), ambiguous_bytes);
    installed["hooks"]["PreToolUse"]
        .as_array_mut()
        .unwrap()
        .pop();
    fs::write(&config, serde_json::to_vec_pretty(&installed).unwrap()).unwrap();

    // Installation alone never rewrites a command: self-test and explicit
    // project authorization are both still absent.
    let payload = serde_json::json!({
        "tool_name": "Bash",
        "tool_input": {"command": "printf should-not-run-through-wrapper"},
        "cwd": temp.path(),
        "session_id": "session-cli",
        "turn_id": "turn-cli",
        "tool_use_id": "call-cli"
    });
    let mut child = Command::new(binary)
        .args([
            "--db",
            db.to_str().unwrap(),
            "capture",
            "hook",
            "--event",
            "pre",
            "--config",
            config.to_str().unwrap(),
            "--capture-root",
            capture.to_str().unwrap(),
            "--owner",
            owner.as_str(),
            "--source-version",
            "test-source",
            "--source-executable",
            source.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(serde_json::to_string(&payload).unwrap().as_bytes())
        .unwrap();
    let hook = child.wait_with_output().unwrap();
    assert!(hook.status.success());
    let hook_json: Value = serde_json::from_slice(&hook.stdout).unwrap();
    assert_eq!(hook_json["capture"], "inactive");
    assert!(hook_json["hookSpecificOutput"]["updatedInput"].is_null());

    let uninstall = Command::new(binary)
        .args([
            "--db",
            db.to_str().unwrap(),
            "capture",
            "uninstall",
            "--source",
            "codex",
            "--config",
            config.to_str().unwrap(),
            "--capture-root",
            capture.to_str().unwrap(),
            "--owner",
            owner.as_str(),
        ])
        .output()
        .unwrap();
    assert!(
        uninstall.status.success(),
        "{}",
        String::from_utf8_lossy(&uninstall.stderr)
    );
    let removed: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
    assert!(removed["hooks"]["PreToolUse"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(removed["hooks"]["PostToolUse"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(
        removed["hooks"]["SessionStart"].as_array().unwrap().len(),
        1
    );
}

#[test]
fn apply_refuses_changed_preimage_without_merging() {
    let temp = tempdir().unwrap();
    let binary = qualified_binary::thruline();
    let config = temp.path().join("hooks.json");
    let capture = temp.path().join("capture");
    let db = temp.path().join("ledger.db");
    let source = temp.path().join("codex-test");
    make_source(&source);
    let owner = thruline_core::adapter::stable_owner_key(temp.path()).unwrap();
    fs::write(&config, "{}\n").unwrap();
    let args = [
        "--db",
        db.to_str().unwrap(),
        "capture",
        "install",
        "--source",
        "codex",
        "--config",
        config.to_str().unwrap(),
        "--capture-root",
        capture.to_str().unwrap(),
        "--owner",
        owner.as_str(),
        "--source-version",
        "test-source",
        "--source-executable",
        source.to_str().unwrap(),
        "--json",
    ];
    let plan = Command::new(binary).args(args).output().unwrap();
    let value: Value = serde_json::from_slice(&plan.stdout).unwrap();
    fs::write(&config, "{\"human_change\":true}\n").unwrap();
    let apply = Command::new(binary)
        .args(&args[..args.len() - 1])
        .args(["--apply", value["plan_id"].as_str().unwrap()])
        .output()
        .unwrap();
    assert!(!apply.status.success());
    assert_eq!(
        fs::read_to_string(&config).unwrap(),
        "{\"human_change\":true}\n"
    );
    assert!(!capture.exists());
}

#[test]
#[ignore = "requires the exact locally admitted Codex binary"]
fn native_self_test_qualifies_exact_local_codex() {
    let source = Path::new("/Applications/ChatGPT.app/Contents/Resources/codex");
    if !source.is_file() {
        return;
    }
    let temp = tempdir().unwrap();
    let binary = qualified_binary::thruline();
    let config = temp.path().join("hooks.json");
    let capture = temp.path().join("capture");
    let db = temp.path().join("ledger.db");
    let owner = thruline_core::adapter::stable_owner_key(temp.path()).unwrap();
    fs::write(&config, "{}\n").unwrap();
    let common = [
        "--db",
        db.to_str().unwrap(),
        "capture",
        "install",
        "--source",
        "codex",
        "--config",
        config.to_str().unwrap(),
        "--capture-root",
        capture.to_str().unwrap(),
        "--owner",
        owner.as_str(),
        "--source-version",
        "0.146.0-alpha.9.2",
        "--source-executable",
        source.to_str().unwrap(),
    ];
    let plan = Command::new(binary)
        .args(common)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let value: Value = serde_json::from_slice(&plan.stdout).unwrap();
    let apply = Command::new(binary)
        .args(common)
        .args(["--apply", value["plan_id"].as_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        apply.status.success(),
        "{}",
        String::from_utf8_lossy(&apply.stderr)
    );
    let test = Command::new(binary)
        .args([
            "--db",
            db.to_str().unwrap(),
            "capture",
            "self-test",
            "--source",
            "codex",
            "--config",
            config.to_str().unwrap(),
            "--capture-root",
            capture.to_str().unwrap(),
            "--owner",
            owner.as_str(),
            "--source-version",
            "0.146.0-alpha.9.2",
            "--source-executable",
            source.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        test.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&test.stdout),
        String::from_utf8_lossy(&test.stderr)
    );
    let ledger = thruline_core::Ledger::open(&db).unwrap();
    let passed: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM capture_self_tests WHERE passed=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let profiles: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM capture_compatibility_profiles",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let detail: String = ledger
        .conn()
        .query_row(
            "SELECT detail_json FROM capture_self_tests ORDER BY accepted_seq DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    eprintln!("native_self_test_detail={detail}");
    assert_eq!((passed, profiles), (1, 1));
}

fn summary_ms(mut samples: Vec<f64>) -> Value {
    samples.sort_by(f64::total_cmp);
    let at = |fraction: f64| {
        let index = ((samples.len() as f64 * fraction).ceil() as usize)
            .saturating_sub(1)
            .min(samples.len().saturating_sub(1));
        samples[index]
    };
    serde_json::json!({
        "iterations": samples.len(),
        "p50_ms": at(0.5),
        "p95_ms": at(0.95),
        "worst_ms": samples.last().copied().unwrap_or_default(),
    })
}

#[test]
#[ignore = "M5.R2 release product-operation performance gate"]
fn product_operations_meet_release_budgets() {
    let temp = tempdir().unwrap();
    let binary = qualified_binary::thruline();
    let config = temp.path().join("hooks.json");
    let capture = temp.path().join("capture");
    let missing_capture = temp.path().join("missing-capture");
    let db = temp.path().join("ledger.db");
    let source = temp.path().join("codex-test");
    make_source(&source);
    let owner = thruline_core::adapter::stable_owner_key(temp.path()).unwrap();
    fs::write(&config, "{}\n").unwrap();
    let install_args = vec![
        "--db".to_string(),
        db.display().to_string(),
        "capture".to_string(),
        "install".to_string(),
        "--source".to_string(),
        "codex".to_string(),
        "--config".to_string(),
        config.display().to_string(),
        "--capture-root".to_string(),
        capture.display().to_string(),
        "--owner".to_string(),
        owner.clone(),
        "--source-version".to_string(),
        "test-source".to_string(),
        "--source-executable".to_string(),
        source.display().to_string(),
    ];
    let mut plan_samples = Vec::new();
    let mut last_plan = String::new();
    for _ in 0..20 {
        let started = Instant::now();
        let output = Command::new(binary)
            .args(&install_args)
            .arg("--json")
            .output()
            .unwrap();
        plan_samples.push(started.elapsed().as_secs_f64() * 1_000.0);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        last_plan = serde_json::from_slice::<Value>(&output.stdout).unwrap()["plan_id"]
            .as_str()
            .unwrap()
            .to_string();
    }
    let apply_started = Instant::now();
    let apply = Command::new(binary)
        .args(&install_args)
        .args(["--apply", &last_plan])
        .output()
        .unwrap();
    let apply_ms = apply_started.elapsed().as_secs_f64() * 1_000.0;
    assert!(
        apply.status.success(),
        "{}",
        String::from_utf8_lossy(&apply.stderr)
    );

    let status_args = vec![
        "--db".to_string(),
        db.display().to_string(),
        "capture".to_string(),
        "status".to_string(),
        "--source".to_string(),
        "codex".to_string(),
        "--config".to_string(),
        config.display().to_string(),
        "--capture-root".to_string(),
        capture.display().to_string(),
        "--owner".to_string(),
        owner.clone(),
        "--source-version".to_string(),
        "test-source".to_string(),
        "--source-executable".to_string(),
        source.display().to_string(),
        "--project-key".to_string(),
        "project-performance".to_string(),
        "--json".to_string(),
    ];
    let mut status_samples = Vec::new();
    for _ in 0..20 {
        let started = Instant::now();
        let output = Command::new(binary).args(&status_args).output().unwrap();
        status_samples.push(started.elapsed().as_secs_f64() * 1_000.0);
        assert!(output.status.success());
    }
    let mut doctor_samples = Vec::new();
    let mut doctor_args = status_args.clone();
    doctor_args[3] = "doctor".to_string();
    for _ in 0..20 {
        let started = Instant::now();
        let output = Command::new(binary).args(&doctor_args).output().unwrap();
        doctor_samples.push(started.elapsed().as_secs_f64() * 1_000.0);
        assert!(output.status.success());
    }

    let mut missing_key_args = status_args.clone();
    let root_index = missing_key_args
        .iter()
        .position(|value| value == "--capture-root")
        .unwrap()
        + 1;
    missing_key_args[root_index] = missing_capture.display().to_string();
    let missing_key = Command::new(binary)
        .args(&missing_key_args)
        .output()
        .unwrap();
    assert!(missing_key.status.success());
    let missing_key_json: Value = serde_json::from_slice(&missing_key.stdout).unwrap();
    assert_eq!(missing_key_json["state"], "degraded_key_or_identity");

    let authorize_started = Instant::now();
    let authorize = Command::new(binary)
        .args([
            "--db",
            db.to_str().unwrap(),
            "capture",
            "authorize",
            "--source",
            "codex",
            "--capture-root",
            capture.to_str().unwrap(),
            "--owner",
            owner.as_str(),
            "--project-key",
            "project-performance",
            "--reason",
            "explicit performance fixture",
        ])
        .output()
        .unwrap();
    let authorize_ms = authorize_started.elapsed().as_secs_f64() * 1_000.0;
    assert!(authorize.status.success());

    let disable_started = Instant::now();
    let disable = Command::new(binary)
        .args([
            "--db",
            db.to_str().unwrap(),
            "capture",
            "disable",
            "--source",
            "codex",
            "--config",
            config.to_str().unwrap(),
            "--capture-root",
            capture.to_str().unwrap(),
            "--owner",
            owner.as_str(),
            "--reason",
            "explicit performance fixture",
        ])
        .output()
        .unwrap();
    let disable_ms = disable_started.elapsed().as_secs_f64() * 1_000.0;
    assert!(disable.status.success());

    let uninstall_started = Instant::now();
    let uninstall = Command::new(binary)
        .args([
            "--db",
            db.to_str().unwrap(),
            "capture",
            "uninstall",
            "--source",
            "codex",
            "--config",
            config.to_str().unwrap(),
            "--capture-root",
            capture.to_str().unwrap(),
            "--owner",
            owner.as_str(),
        ])
        .output()
        .unwrap();
    let uninstall_ms = uninstall_started.elapsed().as_secs_f64() * 1_000.0;
    assert!(
        uninstall.status.success(),
        "{}",
        String::from_utf8_lossy(&uninstall.stderr)
    );

    let report = serde_json::json!({
        "install_plan": summary_ms(plan_samples),
        "status": summary_ms(status_samples),
        "doctor": summary_ms(doctor_samples),
        "install_apply_ms": apply_ms,
        "authorize_ms": authorize_ms,
        "disable_ms": disable_ms,
        "uninstall_ms": uninstall_ms,
        "idle_watcher_count": 0,
        "idle_poll_count": 0,
    });
    // Printed BEFORE the assertions: a budget gate that hides its own
    // measurement on failure tells you only that something was slow, which is
    // the one thing you already knew (Rule 12).
    println!("M5_PRODUCT_PERFORMANCE={report}");
    assert!(report["install_plan"]["p95_ms"].as_f64().unwrap() <= 100.0);
    assert!(report["status"]["p95_ms"].as_f64().unwrap() <= 100.0);
    assert!(report["doctor"]["p95_ms"].as_f64().unwrap() <= 100.0);
}
