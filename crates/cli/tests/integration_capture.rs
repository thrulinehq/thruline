//! Production CLI and ledger with deliberately controlled protocol producers.
//! Actual upstream executable cases live in scripts/integrations/validate-*.py.
#![cfg(target_os = "macos")]
use serde_json::{json, Value};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}
fn git(root: &Path, args: &[&str]) {
    let o = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@localhost")
        .env("GIT_COMMITTER_NAME", "fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@localhost")
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
}
struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    db: PathBuf,
    plan: PathBuf,
    output: PathBuf,
}
impl Fixture {
    fn new(script: &str, timeout: u64, limit: u64) -> Self {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("repo");
        std::fs::create_dir(&root).unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        std::fs::write(root.join("source.js"), "let x=1\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-qm", "fixture"]);
        let script = format!("printf 'sg: entity|rule|fixture-condition: finalSeverity=Warning\\nsg: entity|file|source.js: language=JavaScript,appliedRuleCount=1\\nsg: summary|file: scannedFileCount=1,skippedFileCount=0\\nsg: summary|rule: effectiveRuleCount=1,skippedRuleCount=0\\n' >&2; {script}");
        let plan = d.path().join("plan.json");
        let db = d.path().join("ledger.db");
        let output = d.path().join("managed-output");
        std::fs::create_dir(&output).unwrap();
        std::fs::write(&plan,serde_json::to_vec(&json!({"schema":"thruline.integration-plan.v1","pack":"ast-grep","format":"ast-grep-json-v1","root":root,"argv":["/bin/sh","-c",script],"criterion":{"id":"fixture-condition","revision":"1","requirement":"Controlled capture protocol fixture","success":"No observations from this producer","accepted_by":"fixture operator","condition":{"kind":"no_observations"}},"rules":[],"runtime":[],"dependencies":[],"dataset":[],"writable_paths":[output],"environment":{"PATH":"/usr/bin:/bin","HOME":d.path(),"THRULINE_FIXTURE_OUTPUT":output},"input_closure_complete":true,"timeout_seconds":timeout,"cpu_seconds":10,"memory_mb":256,"output_limit_bytes":limit,"report":{"kind":"stdout"}})).unwrap()).unwrap();
        Self {
            _dir: d,
            root,
            db,
            plan,
            output,
        }
    }
    fn cmd(&self) -> Command {
        let mut c = Command::new(bin());
        c.arg("--db")
            .arg(&self.db)
            .args(["integration", "run", "--plan"])
            .arg(&self.plan)
            .arg("--accept");
        c
    }
    fn run(&self) -> Value {
        let o = self.cmd().output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        serde_json::from_slice(&o.stdout).unwrap()
    }
}
#[test]
fn captured_condition_has_a_real_binding_and_changes_become_stale() {
    let f = Fixture::new("printf '[]'", 5, 4096);
    let r = f.run();
    assert_eq!(r["assessment"]["state"], "passed");
    assert_eq!(r["assessment"]["verdict"], "unverifiable");
    let ledger = thruline_core::ledger::Ledger::open(&f.db).unwrap();
    let b = thruline_core::evidence_binding::load(&ledger, r["binding_id"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert!(b.binding.argv.iter().any(|v| v == "worker-exec"));
    assert_eq!(b.binding.exit_code, Some(0));
    assert_eq!(b.binding.artifacts.len(), 2);
    std::fs::write(f.root.join("source.js"), "let x=2\n").unwrap();
    let o = Command::new(bin())
        .arg("--db")
        .arg(&f.db)
        .args(["integration", "results", "--root"])
        .arg(&f.root)
        .output()
        .unwrap();
    assert!(o.status.success(), "{o:?}");
    let v: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["runs"][0]["assessment"]["state"], "stale");
}
#[test]
fn malformed_or_mixed_output_does_not_pass() {
    for s in [
        "printf '{}'",
        "printf '[] trailing-output'",
        "printf '[]'; exit 2",
    ] {
        let r = Fixture::new(s, 5, 4096).run();
        assert_eq!(r["assessment"]["state"], "unknown");
    }
}
#[test]
fn timeout_and_output_limit_are_recorded_not_successful() {
    let f = Fixture::new("sleep 10", 1, 4096);
    let t = Instant::now();
    let r = f.run();
    assert!(t.elapsed() < Duration::from_secs(6));
    assert_eq!(r["record"]["admission"]["run_state"], "timed_out");
    assert_eq!(r["assessment"]["state"], "blocked");
    let r = Fixture::new(
        "while :; do printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'; done",
        3,
        1024,
    )
    .run();
    assert!(matches!(
        r["record"]["admission"]["run_state"].as_str(),
        Some("output_limit" | "capture_failed")
    ));
    assert_eq!(r["assessment"]["state"], "blocked");
}
#[test]
fn cancellation_retains_receipt_and_cleans_ordinary_descendants() {
    let f = Fixture::new(
        "sleep 20 & echo $! > \"$THRULINE_FIXTURE_OUTPUT/child.pid\"; wait",
        30,
        4096,
    );
    let child = f
        .cmd()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    while !f.output.join("child.pid").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let pid = std::fs::read_to_string(f.output.join("child.pid")).unwrap();
    assert!(Command::new("/bin/kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap()
        .success());
    let o = child.wait_with_output().unwrap();
    assert!(o.status.success(), "{o:?}");
    let r: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(r["record"]["admission"]["run_state"], "cancelled");
    assert_eq!(r["assessment"]["state"], "blocked");
    std::thread::sleep(Duration::from_millis(80));
    let status = Command::new("/bin/ps")
        .args(["-o", "stat=", "-p", pid.trim()])
        .output()
        .unwrap();
    let state = String::from_utf8_lossy(&status.stdout);
    assert!(
        state.trim().is_empty() || state.trim().starts_with('Z'),
        "descendant still running: {state}"
    );
}
#[test]
fn concurrent_runs_do_not_overbook_the_worker() {
    let f = Fixture::new(
        "echo ready > \"$THRULINE_FIXTURE_OUTPUT/started\"; sleep 2; printf '[]'",
        5,
        4096,
    );
    let child = f.cmd().stdout(Stdio::piped()).spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !f.output.join("started").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    let second = f.cmd().output().unwrap();
    assert!(!second.status.success());
    assert!(String::from_utf8_lossy(&second.stderr).contains("already owns"));
    assert!(child.wait_with_output().unwrap().status.success());
}
#[test]
fn plan_needs_explicit_acceptance() {
    let f = Fixture::new("touch should-not-exist; printf '[]'", 5, 4096);
    let o = Command::new(bin())
        .arg("--db")
        .arg(&f.db)
        .args(["integration", "run", "--plan"])
        .arg(&f.plan)
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(!f.root.join("should-not-exist").exists());
}

#[test]
fn memory_and_cpu_budgets_prevent_a_successful_result() {
    for (script, key, value) in [
        (
            "exec /usr/bin/perl -e '$x = \"x\" x (256 * 1024 * 1024); sleep 3'",
            "memory_mb",
            128,
        ),
        ("while :; do :; done", "cpu_seconds", 1),
    ] {
        let f = Fixture::new(script, 6, 4096);
        let mut plan: Value = serde_json::from_slice(&std::fs::read(&f.plan).unwrap()).unwrap();
        plan[key] = json!(value);
        std::fs::write(&f.plan, serde_json::to_vec(&plan).unwrap()).unwrap();
        let r = f.run();
        assert_eq!(r["assessment"]["state"], "blocked", "{r}");
        if key == "memory_mb" {
            assert_eq!(r["record"]["admission"]["run_state"], "resource_limit");
            assert!(r["record"]["sampled_peak_rss_bytes"].as_u64().unwrap() > 128 * 1048576);
        }
    }
}

#[test]
fn a_worker_cannot_connect_to_a_local_network_listener() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let f = Fixture::new(
        &format!("exec /usr/bin/curl --max-time 1 http://{addr}/"),
        5,
        4096,
    );
    let r = f.run();
    assert_ne!(r["assessment"]["state"], "passed");
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn changed_receipt_bytes_are_rejected_on_read() {
    let f = Fixture::new("printf '[]'", 5, 4096);
    let r = f.run();
    std::fs::write(r["artifact"].as_str().unwrap(), b"{}").unwrap();
    let o = Command::new(bin())
        .arg("--db")
        .arg(&f.db)
        .args(["integration", "results", "--root"])
        .arg(&f.root)
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("artifact was changed"));
}

#[test]
fn special_input_files_are_refused_without_hanging() {
    let f = Fixture::new("printf '[]'", 5, 4096);
    let mut p: Value = serde_json::from_slice(&std::fs::read(&f.plan).unwrap()).unwrap();
    p["runtime"] = json!(["/dev/zero"]);
    std::fs::write(&f.plan, serde_json::to_vec(&p).unwrap()).unwrap();
    let before = Instant::now();
    let o = f.cmd().output().unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("regular file"));
    assert!(before.elapsed() < Duration::from_secs(3));
    let f = Fixture::new("mkfifo \"$THRULINE_FIXTURE_OUTPUT/report.json\"", 5, 4096);
    let mut p: Value = serde_json::from_slice(&std::fs::read(&f.plan).unwrap()).unwrap();
    p["report"] = json!({"kind":"file","path":f.output.join("report.json")});
    std::fs::write(&f.plan, serde_json::to_vec(&p).unwrap()).unwrap();
    let r = f.run();
    assert_eq!(r["assessment"]["state"], "unknown");
    assert!(r["record"]["decode_error"]
        .as_str()
        .unwrap()
        .contains("regular file"));
}

#[test]
fn native_worker_denies_outside_reads_source_writes_and_link_escapes() {
    let script = r#"
if cat "$WORKER_PROBE_PATH" >/dev/null 2>&1; then exit 91; fi
if (printf changed > source.js) 2>/dev/null; then exit 92; fi
if (printf changed > "$WORKER_PROBE_PATH") 2>/dev/null; then exit 93; fi
ln -s "$WORKER_PROBE_PATH" "$THRULINE_FIXTURE_OUTPUT/alias"
if cat "$THRULINE_FIXTURE_OUTPUT/alias" >/dev/null 2>&1; then exit 94; fi
if ln "$WORKER_PROBE_PATH" "$THRULINE_FIXTURE_OUTPUT/hardlink" 2>/dev/null; then exit 95; fi
printf managed > "$THRULINE_FIXTURE_OUTPUT/result.txt"
printf '[]'
"#;
    let f = Fixture::new(script, 5, 4096);
    let outside = f._dir.path().join("inert-canary.txt");
    std::fs::write(&outside, "INERT_UNRELATED_HOST_DATA").unwrap();
    let mut plan: Value = serde_json::from_slice(&std::fs::read(&f.plan).unwrap()).unwrap();
    plan["environment"]["WORKER_PROBE_PATH"] = json!(outside);
    std::fs::write(&f.plan, serde_json::to_vec(&plan).unwrap()).unwrap();
    let r = f.run();
    assert_eq!(r["assessment"]["state"], "passed", "{r}");
    assert_eq!(
        std::fs::read_to_string(&outside).unwrap(),
        "INERT_UNRELATED_HOST_DATA"
    );
    assert_eq!(
        std::fs::read_to_string(f.root.join("source.js")).unwrap(),
        "let x=1\n"
    );
    assert_eq!(
        std::fs::read_to_string(f.output.join("result.txt")).unwrap(),
        "managed"
    );
    assert_eq!(
        r["record"]["isolation"]["backend"],
        "macos-seatbelt-native-v1"
    );
}
