//! Actual subprocess + SQLite + replay regressions. Projects and evidence are
//! isolated synthetic fixtures; these are not claims about production users.
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};
#[path = "support/qualified_binary.rs"]
mod qualified_binary;
struct Project {
    _temp: tempfile::TempDir,
    root: PathBuf,
    db: PathBuf,
    digest: String,
}
impl Project {
    fn new(
        program: &str,
        argv: &[&str],
        category: &str,
        runner: &str,
        timeout: u32,
        cases: Value,
    ) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("sample project");
        std::fs::create_dir_all(root.join(".thruline")).unwrap();
        assert!(Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .unwrap()
            .success());
        std::fs::write(root.join("main.py"), program).unwrap();
        std::fs::write(root.join(".gitignore"), "__pycache__/\n").unwrap();
        let document = json!({"version":1,"id":"synthetic-checkout","revision":1,"objective":"Synthetic boundary requirement, not real customer evidence","criteria":[{"id":"observable","description":"The program returns the accepted JSON value for each explicitly enumerated boundary input.","check_ids":["boundary"],"human_acceptance_required":false}],"checks":[{"id":"boundary","category":category,"argv":argv,"cwd":".","runner":runner,"mode":"captured","timeout_seconds":timeout,"artifacts":[],"verification":{"scope":["main.py"],"environment_files":[],"environment_closed":true,"environment_description":"Synthetic fixture: local interpreter and standard library; no third-party dependencies or services.","cases":cases}}],"protected_paths":[".thruline/acceptance.json"]});
        let bytes = serde_json::to_vec_pretty(&document).unwrap();
        std::fs::write(root.join(".thruline/acceptance.json"), &bytes).unwrap();
        let digest = thruline_policy::acceptance::document_digest(&bytes);
        let p = Self {
            db: temp.path().join("evidence/exec.db"),
            _temp: temp,
            root,
            digest,
        };
        p.call("accept", &["--digest", &p.digest]);
        p
    }
    fn call(&self, action: &str, args: &[&str]) -> Value {
        let out = Command::new(qualified_binary::thruline())
            .args(["acceptance", action, "--repo"])
            .arg(&self.root)
            .arg("--exec-db")
            .arg(&self.db)
            .arg("--json")
            .args(args)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .output()
            .unwrap();
        serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("{action}: {e}: {}", String::from_utf8_lossy(&out.stderr)))
    }
    fn run(&self) -> Value {
        self.call("run", &["--check", "boundary", "--digest", &self.digest])
    }
    fn status(&self) -> Value {
        self.call("status", &[])
    }
    fn receipt(&self, id: &str) -> Value {
        self.call("receipt", &["--run", id])
    }
    fn edit(&self, text: &str) {
        std::fs::write(self.root.join("main.py"), text).unwrap();
    }
}
fn cases() -> Value {
    json!([{"id":"discount","description":"A discount changes the observable total","input":{"subtotal":100,"discount":25},"expected":75,"expected_exit":0},{"id":"zero","description":"The zero boundary returns zero","input":{"subtotal":0,"discount":25},"expected":0,"expected_exit":0}])
}
const VALID:&str="import json,sys\nv=json.load(sys.stdin)\nprint(json.dumps(v['subtotal']*(100-v['discount'])//100))\n";
fn behavior(source: &str) -> Project {
    Project::new(
        source,
        &["python3", "-I", "-B", "main.py"],
        "test",
        "thruline-json-cases-v1",
        8,
        cases(),
    )
}
#[test]
fn failure_pass_dirty_stale_repair_and_replay_are_end_to_end() {
    let p = behavior("import json,sys\nv=json.load(sys.stdin)\nprint(json.dumps(v['subtotal']))\n");
    let initial = p.status();
    assert_eq!(initial["pack_results"][0]["status"], "not_run");
    assert_eq!(initial["coverage"]["total"], 134);
    assert_eq!(initial["coverage"]["implemented"], 14);
    let fail = p.run();
    assert_eq!(fail["state"], "failed", "{fail}");
    let failure = p.receipt(fail["run_id"].as_str().unwrap());
    assert_eq!(failure["record"]["historical_state"], "fail");
    assert_eq!(
        failure["record"]["finish"]["pack"]["observations"][0]["observed"],
        100
    );
    assert_eq!(failure["verification"]["replay_agrees"], true);
    assert!(failure["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .all(|o| o["available"] == true));
    assert!(failure["record"]["start"]["inputs"]["process_environment_private"].is_null());
    p.edit(VALID);
    assert_eq!(p.status()["pack_results"][0]["status"], "stale");
    let pass = p.run();
    assert_eq!(pass["state"], "verified", "{pass}");
    let status = p.status();
    assert_eq!(status["pack_results"][0]["status"], "pass", "{status}");
    assert_eq!(status["history"].as_array().unwrap().len(), 2);
    let fresh = p.receipt(pass["run_id"].as_str().unwrap());
    let again = p.receipt(pass["run_id"].as_str().unwrap());
    assert_eq!(fresh["receipt_digest"], again["receipt_digest"]);
    assert_ne!(
        fresh["record"]["start"]["inputs"]["tree"],
        failure["record"]["start"]["inputs"]["tree"]
    );
    assert_eq!(
        p.receipt(fail["run_id"].as_str().unwrap())["receipt_digest"],
        failure["receipt_digest"],
        "historical receipt must not change after repair"
    );
    std::fs::write(p.root.join("untracked-helper.py"), "CHANGED=True\n").unwrap();
    assert_eq!(p.status()["pack_results"][0]["status"], "stale");
    std::fs::remove_file(p.root.join("untracked-helper.py")).unwrap();
    assert_eq!(
        p.status()["pack_results"][0]["status"],
        "stale",
        "restoring bytes must not erase an observed invalidation"
    );
    p.run();
    assert_eq!(p.status()["pack_results"][0]["status"], "pass");
}
#[test]
fn explicitly_scoped_ignored_source_is_bound_and_invalidated() {
    let p = behavior(VALID);
    std::fs::write(p.root.join(".gitignore"), "__pycache__/\nmain.py\n").unwrap();
    let pass = p.run();
    assert_eq!(p.status()["pack_results"][0]["status"], "pass");
    let before = p.receipt(pass["run_id"].as_str().unwrap());
    p.edit("import json,sys\nv=json.load(sys.stdin)\nprint(json.dumps(v['subtotal']))\n");
    assert_eq!(p.status()["pack_results"][0]["status"], "stale");
    let fail = p.run();
    let after = p.receipt(fail["run_id"].as_str().unwrap());
    assert_eq!(after["record"]["historical_state"], "fail");
    assert_eq!(
        before["record"]["start"]["inputs"]["tree"], after["record"]["start"]["inputs"]["tree"],
        "Git excludes this source, so supplemental scope binding must detect the change"
    );
    assert_ne!(
        before["record"]["start"]["inputs"]["environment_files"],
        after["record"]["start"]["inputs"]["environment_files"]
    );
}
#[test]
fn missing_tool_records_a_blocked_receipt() {
    let p = Project::new(
        VALID,
        &["thruline-tool-does-not-exist-93821"],
        "test",
        "thruline-json-cases-v1",
        1,
        cases(),
    );
    let run = p.run();
    assert_eq!(run["state"], "unverified");
    let status = p.status();
    assert_eq!(status["pack_results"][0]["status"], "blocked", "{status}");
    assert!(
        p.receipt(run["run_id"].as_str().unwrap())["verification"]["explanation"]
            .as_str()
            .unwrap()
            .contains("executable")
    );
}
#[test]
fn success_text_and_timeouts_remain_incomplete() {
    for source in ["print('success')\n", "import time\ntime.sleep(3)\n"] {
        let p = Project::new(
            source,
            &["python3", "-I", "-B", "main.py"],
            "test",
            "thruline-json-cases-v1",
            1,
            cases(),
        );
        let run = p.run();
        assert_eq!(run["state"], "unverified", "{run}");
        assert_eq!(p.status()["pack_results"][0]["status"], "unknown");
    }
}
#[test]
fn captured_failure_recovers_and_missing_blob_invalidates_without_a_false_failure() {
    let p = behavior(VALID);
    let run = p.run();
    let id = run["run_id"].as_str().unwrap();
    assert_eq!(p.status()["pack_results"][0]["status"], "pass");
    let receipt = p.receipt(id);
    let hash = receipt["outputs"][0]["digest"]
        .as_str()
        .unwrap()
        .strip_prefix("sha256:")
        .unwrap();
    std::fs::remove_file(
        p.db.parent()
            .unwrap()
            .join(format!("exec.db.acceptance/outputs/{hash}.out")),
    )
    .unwrap();
    assert_eq!(p.status()["pack_results"][0]["status"], "unknown");
    let missing = p.receipt(id);
    assert_eq!(missing["record"]["historical_state"], "pass");
    assert_eq!(missing["verification"]["state"], "unknown");
    assert_eq!(missing["receipt_digest"], receipt["receipt_digest"]);
    assert_eq!(missing["outputs"][0]["available"], false);
    p.run();
    assert_eq!(p.status()["pack_results"][0]["status"], "pass");
}
#[test]
fn direct_build_failure_and_valid_build_are_not_agent_reports() {
    let p = Project::new(
        "def invalid(\n",
        &["python3", "-m", "py_compile", "main.py"],
        "build",
        "python syntax",
        8,
        json!([]),
    );
    assert_eq!(p.run()["state"], "failed");
    p.edit(VALID);
    assert_eq!(p.run()["state"], "verified");
    let status = p.status();
    assert!(status["pack_results"][0]["catalog_results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["check_id"] == "FUN-01" && r["status"] == "pass"));
    let liar = Project::new(
        "",
        &["sh", "-c", "false; echo success"],
        "build",
        "arbitrary wrapper",
        8,
        json!([]),
    );
    assert_eq!(liar.run()["state"], "unverified");
    assert_eq!(liar.status()["pack_results"][0]["status"], "unknown");
}
#[test]
fn protected_oracle_changes_require_new_acceptance_and_history_survives() {
    let p = behavior(VALID);
    p.run();
    let path = p.root.join(".thruline/acceptance.json");
    let mut doc: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    doc["checks"][0]["verification"]["cases"][0]["expected"] = json!(999);
    std::fs::write(path, serde_json::to_vec_pretty(&doc).unwrap()).unwrap();
    let status = p.status();
    assert_eq!(status["pack_results"][0]["status"], "stale");
    assert_eq!(status["pending_draft_change"], true);
}

#[test]
fn external_dependency_change_invalidates_and_unclosed_environment_blocks() {
    let p = behavior(VALID);
    let path = p.root.join(".thruline/acceptance.json");
    let mut doc: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let external = p._temp.path().join("runtime-config");
    std::fs::write(&external, "version-one").unwrap();
    doc["revision"] = json!(2);
    doc["checks"][0]["verification"]["environment_files"] = json!([external]);
    let bytes = serde_json::to_vec_pretty(&doc).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let old = p.status();
    let digest = thruline_policy::acceptance::document_digest(&bytes);
    p.call(
        "accept",
        &[
            "--digest",
            &digest,
            "--supersedes",
            old["acceptance_hash"].as_str().unwrap(),
        ],
    );
    p.call("run", &["--check", "boundary", "--digest", &digest]);
    assert_eq!(p.status()["pack_results"][0]["status"], "pass");
    std::fs::write(&external, "version-two").unwrap();
    assert_eq!(p.status()["pack_results"][0]["status"], "stale");
}
#[test]
fn collector_key_loss_never_turns_imported_or_untrusted_reports_into_evidence() {
    let p = behavior(VALID);
    let run = p.run();
    let id = run["run_id"].as_str().unwrap();
    let before = p.receipt(id);
    assert_eq!(before["verification"]["state"], "pass");
    std::fs::write(
        p.db.parent()
            .unwrap()
            .join("exec.db.acceptance/environment.key"),
        [99u8; 32],
    )
    .unwrap();
    let receipt = p.receipt(id);
    assert_eq!(receipt["verification"]["state"], "unknown");
    assert_eq!(receipt["receipt_digest"], before["receipt_digest"]);
    assert_ne!(p.status()["pack_results"][0]["status"], "pass");
    assert_eq!(p.status()["state"], "unverified");
}
#[test]
fn output_cap_is_an_incomplete_capture_not_a_product_failure() {
    let p = behavior("print('x'*(9*1024*1024))\n");
    let run = p.run();
    assert_eq!(run["state"], "unverified");
    let receipt = p.receipt(run["run_id"].as_str().unwrap());
    assert_eq!(receipt["verification"]["state"], "unknown");
    assert!(receipt["record"]["finish"]["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r.as_str().unwrap().contains("incomplete")));
}
