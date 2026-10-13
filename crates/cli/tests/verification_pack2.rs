//! Controlled defect fixtures, executed through the actual CLI and evidence store.
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};
#[path = "support/qualified_binary.rs"]
mod qualified_binary;
struct Project {
    temp: tempfile::TempDir,
    root: PathBuf,
    db: PathBuf,
    doc: Value,
    digest: String,
}
impl Project {
    fn new(files: &[(&str, &str)], checks: Vec<Value>, criteria: Vec<Value>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        std::fs::create_dir_all(root.join(".thruline")).unwrap();
        assert!(Command::new("git")
            .args(["init", "-q"])
            .arg(&root)
            .status()
            .unwrap()
            .success());
        for (path, text) in files {
            let p = root.join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, text).unwrap();
        }
        std::fs::write(root.join(".gitignore"), "__pycache__/\n").unwrap();
        let doc = json!({"version":1,"id":"controlled-pack2","revision":1,"objective":"Synthetic check-pack regression; not customer evidence","checks":checks,"criteria":criteria,"protected_paths":[".thruline/acceptance.json"]});
        let mut p = Self {
            db: temp.path().join("evidence/exec.db"),
            temp,
            root,
            doc,
            digest: String::new(),
        };
        p.accept();
        p
    }
    fn command(&self, action: &str) -> Command {
        let mut c = Command::new(qualified_binary::thruline());
        c.args(["acceptance", action, "--repo"])
            .arg(&self.root)
            .arg("--exec-db")
            .arg(&self.db)
            .arg("--json");
        c
    }
    fn call(&self, action: &str, args: &[&str]) -> Value {
        let o = self.command(action).args(args).output().unwrap();
        serde_json::from_slice(&o.stdout)
            .unwrap_or_else(|_| panic!("{action}: {}", String::from_utf8_lossy(&o.stderr)))
    }
    fn accept(&mut self) {
        let old = self.call("status", &[]);
        let bytes = serde_json::to_vec_pretty(&self.doc).unwrap();
        std::fs::write(self.root.join(".thruline/acceptance.json"), &bytes).unwrap();
        self.digest = thruline_policy::acceptance::document_digest(&bytes);
        let mut a = vec!["--digest", self.digest.as_str()];
        if let Some(hash) = old["acceptance_hash"].as_str() {
            a.extend(["--supersedes", hash]);
        }
        self.call("accept", &a);
    }
    fn run(&self, id: &str) -> Value {
        self.call("run", &["--check", id, "--digest", &self.digest])
    }
    fn status(&self) -> Value {
        self.call("status", &[])
    }
    fn state(&self, id: &str) -> String {
        let s = self.status();
        s["pack_results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap()["status"]
            .as_str()
            .unwrap()
            .into()
    }
    fn receipt(&self, run: &Value) -> Value {
        self.call("receipt", &["--run", run["run_id"].as_str().unwrap()])
    }
    fn edit(&self, p: &str, text: &str) {
        std::fs::write(self.root.join(p), text).unwrap();
    }
}
/// A **liveness** bound on a fixture check, not a speed bound. No fixture in
/// this file measures how fast this machine can spawn `python3` or how fast it
/// can walk a ledger, so a bound sized against machine speed can only turn
/// load into a red. A check that has not finished in this long is wedged, not
/// slow. Tests that measure the product's *deadline* behaviour set their own
/// short `timeout_seconds` explicitly, beside the fixture that needs it.
/// (TIMING-TESTS-OWNER, family of R-2026-09-24-27.)
const FIXTURE_LIVENESS_TIMEOUT_SECONDS: u32 = 120;
fn check(id: &str, category: &str, runner: &str, argv: Value, scope: Value, pack: Value) -> Value {
    json!({"id":id,"category":category,"argv":argv,"cwd":".","runner":runner,"mode":"captured","timeout_seconds":FIXTURE_LIVENESS_TIMEOUT_SECONDS,"artifacts":[],"verification":{"scope":scope,"environment_files":[],"environment_closed":true,"environment_description":"Controlled fixture: declared local source and standard-library runtime only","cases":[],"pack":pack}})
}
fn criterion(id: &str, kind: &str) -> Value {
    json!({"id":id,"description":format!("Accepted observable {id} criterion"),"check_ids":[id],"human_acceptance_required":false,"evidence_kind":kind})
}
fn suite() -> Value {
    check(
        "suite",
        "test",
        "thruline-unittest-v2",
        json!(["python3"]),
        json!(["tests/test_total.py", "total.py"]),
        json!({"kind":"suite","directory":"tests","pattern":"test*.py","expected_tests":["test_total.Total.test_discount","test_total.Total.test_zero"],"exclusions":[]}),
    )
}
const SOURCE: &str = "def total(value):\n    return value * 3 // 4\n";
const TESTS:&str="import unittest\nfrom total import total\nclass Total(unittest.TestCase):\n    def test_discount(self): self.assertEqual(total(100),75)\n    def test_zero(self): self.assertEqual(total(0),0)\n";
fn sample() -> Project {
    Project::new(
        &[("total.py", SOURCE), ("tests/test_total.py", TESTS)],
        vec![suite()],
        vec![criterion("suite", "tests")],
    )
}
#[test]
fn discovery_execution_skip_and_repair_are_distinct() {
    let p = sample();
    let pass = p.run("suite");
    assert_eq!(p.state("suite"), "pass");
    let receipt = p.receipt(&pass);
    assert_eq!(receipt["verification"]["replay_agrees"], true);
    p.edit(
        "tests/test_total.py",
        &TESTS.replace(
            "    def test_discount",
            "    @unittest.skip('temporary')\n    def test_discount",
        ),
    );
    assert_eq!(p.state("suite"), "stale");
    let skipped = p.run("suite");
    assert_eq!(p.state("suite"), "fail");
    assert!(
        p.receipt(&skipped)["record"]["finish"]["pack"]["observations"][0]["reason"]
            .as_str()
            .unwrap()
            .contains("did not execute")
    );
    p.edit("tests/test_total.py", TESTS);
    p.run("suite");
    assert_eq!(p.state("suite"), "pass");
    assert_eq!(
        p.receipt(&pass)["receipt_digest"],
        receipt["receipt_digest"]
    );
}
#[test]
fn genuine_failure_and_printed_success_do_not_change_execution_coverage() {
    let p = sample();
    p.edit(
        "total.py",
        "def total(value):\n    print('success: 999 passed')\n    return -1\n",
    );
    let run = p.run("suite");
    assert_eq!(p.state("suite"), "fail");
    let r = p.receipt(&run);
    assert_eq!(r["record"]["finish"]["counts"]["failed"], 2);
    assert_eq!(r["verification"]["replay_agrees"], true);
    let status = p.status();
    let rows = status["pack_results"][0]["catalog_results"]
        .as_array()
        .unwrap();
    for id in ["TST-01", "EVD-05"] {
        assert_eq!(
            rows.iter().find(|r| r["check_id"] == id).unwrap()["status"],
            "pass",
            "a failing test still executed"
        );
    }
}
#[test]
fn accepted_exclusion_is_visible_and_test_edits_are_advisory() {
    let mut p = sample();
    p.edit(
        "tests/test_total.py",
        &TESTS.replace(
            "    def test_zero",
            "    @unittest.skip('accepted unavailable boundary')\n    def test_zero",
        ),
    );
    p.doc["revision"] = json!(2);
    p.doc["checks"][0]["verification"]["pack"]["exclusions"] =
        json!(["test_total.Total.test_zero"]);
    p.accept();
    p.run("suite");
    assert_eq!(p.state("suite"), "pass");
    p.edit(
        "tests/test_total.py",
        &TESTS.replace("self.assertEqual(total(100),75)", "self.assertTrue(True)"),
    );
    let run = p.run("suite");
    assert_eq!(p.state("suite"), "pass");
    let r = p.receipt(&run);
    assert!(r["record"]["finish"]["pack"]["review_notes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n.as_str().unwrap().contains("does not by itself")));
}
#[test]
fn missing_test_and_discovery_error_are_not_the_same_result() {
    let p = sample();
    p.edit(
        "tests/test_total.py",
        &TESTS.replace("test_zero", "renamed_case"),
    );
    p.run("suite");
    assert_eq!(p.state("suite"), "fail");
    p.edit("tests/test_total.py", "import missing_dependency_pack2\n");
    p.run("suite");
    assert_eq!(p.state("suite"), "unknown");
}
fn json_check() -> Value {
    let mut c = check(
        "behavior",
        "test",
        "thruline-json-cases-v1",
        json!(["python3", "-I", "-B", "probe.py"]),
        json!(["probe.py"]),
        Value::Null,
    );
    c["verification"].as_object_mut().unwrap().remove("pack");
    c["verification"]["cases"] = json!([{"id":"value","description":"Accepted output equals 75","input":{},"expected":75,"expected_exit":0}]);
    c
}
#[test]
fn concurrent_edit_cannot_attach_a_fresh_pass_to_unexamined_source() {
    let signal_dir = tempfile::tempdir().unwrap();
    let signal = signal_dir.path().join("started");
    let program = format!("import pathlib,time\npathlib.Path({:?}).write_text('started')\ntime.sleep(0.8)\nprint(75)\n", signal.to_string_lossy());
    let p = Project::new(
        &[("probe.py", &program)],
        vec![json_check()],
        vec![criterion("behavior", "behavior")],
    );
    let mut child = p
        .command("run")
        .args(["--check", "behavior", "--digest", &p.digest])
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    // Liveness, not speed: this waits for the child to reach a synchronization
    // point it reaches in milliseconds when the machine is idle. Ten seconds
    // was a bound on how fast this machine starts a CLI and a Python probe.
    let deadline =
        Instant::now() + Duration::from_secs(u64::from(FIXTURE_LIVENESS_TIMEOUT_SECONDS));
    while !signal.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        signal.exists(),
        "verification process never reached its synchronization point"
    );
    p.edit("probe.py", "print(99)\n");
    child.wait().unwrap();
    let state = p.state("behavior");
    assert!(matches!(state.as_str(), "stale" | "unknown"), "{state}");
    let s = p.status();
    let historical = s["history"][0]["record"]["historical_state"]
        .as_str()
        .unwrap();
    assert_ne!(historical, "pass");
    p.edit("probe.py", "print(75)\n");
    p.run("behavior");
    assert_eq!(p.state("behavior"), "pass");
}
#[test]
fn self_edit_and_revert_inside_private_copy_remains_incomplete() {
    let code="from pathlib import Path\np=Path(__file__)\ns=p.read_text()\np.write_text(s+'\\n')\np.write_text(s)\nprint(75)\n";
    let p = Project::new(
        &[("probe.py", code)],
        vec![json_check()],
        vec![criterion("behavior", "behavior")],
    );
    let run = p.run("behavior");
    assert_eq!(p.state("behavior"), "unknown");
    let receipt = p.receipt(&run);
    assert!(receipt["verification"]["explanation"]
        .as_str()
        .unwrap()
        .contains("execution copy"));
    assert_eq!(
        std::fs::read_to_string(p.root.join("probe.py")).unwrap(),
        code
    );
}
#[test]
fn ignored_lockfile_configuration_and_incomplete_scope_are_enforced() {
    let mut p = sample();
    p.edit(".gitignore", "package-lock.json\n__pycache__/\n");
    p.edit("package-lock.json", "{\"version\":1}\n");
    p.run("suite");
    assert_eq!(p.state("suite"), "pass");
    p.edit("package-lock.json", "{\"version\":2}\n");
    assert_eq!(p.state("suite"), "stale");
    p.run("suite");
    assert_eq!(p.state("suite"), "pass");
    p.doc["revision"] = json!(2);
    p.doc["checks"][0]["verification"]["environment_closed"] = json!(false);
    p.accept();
    p.run("suite");
    assert_eq!(p.state("suite"), "blocked");
}
fn build() -> Value {
    let mut c = check(
        "build",
        "build",
        "accepted-python-build",
        json!(["python3", "-I", "-B", "build.py"]),
        json!(["build.py"]),
        json!({"kind":"artifact"}),
    );
    c["artifacts"] =
        json!([{"path":"result.txt","sha256":thruline_core::chain::sha256_hex(b"ready\n")}]);
    c
}
#[test]
fn clean_artifact_identity_missing_output_and_repair_have_receipts() {
    let good="from pathlib import Path\nassert not Path('result.txt').exists()\nPath('result.txt').write_text('ready\\n')\n";
    let p = Project::new(
        &[("build.py", good), ("result.txt", "old artifact")],
        vec![build()],
        vec![criterion("build", "artifact")],
    );
    let run = p.run("build");
    assert_eq!(p.state("build"), "pass");
    assert_eq!(
        std::fs::read_to_string(p.root.join("result.txt")).unwrap(),
        "old artifact"
    );
    assert_eq!(p.receipt(&run)["verification"]["replay_agrees"], true);
    p.edit(
        "build.py",
        "from pathlib import Path\nPath('result.txt').write_text('wrong')\n",
    );
    p.run("build");
    assert_eq!(p.state("build"), "fail");
    p.edit("build.py", "print('success')\n");
    p.run("build");
    assert_eq!(p.state("build"), "unknown");
    p.edit("build.py", good);
    p.run("build");
    assert_eq!(p.state("build"), "pass");
}
#[test]
fn artifact_loss_does_not_rewrite_historical_receipt() {
    let p = Project::new(
        &[(
            "build.py",
            "from pathlib import Path\nPath('result.txt').write_text('ready\\n')\n",
        )],
        vec![build()],
        vec![criterion("build", "artifact")],
    );
    let run = p.run("build");
    let r = p.receipt(&run);
    let digest = r["record"]["finish"]["artifacts"]["result.txt"]
        .as_str()
        .unwrap();
    std::fs::remove_file(
        p.temp
            .path()
            .join("evidence/exec.db.acceptance/artifacts")
            .join(digest),
    )
    .unwrap();
    let again = p.receipt(&run);
    assert_eq!(again["verification"]["state"], "unknown");
    assert_eq!(again["receipt_digest"], r["receipt_digest"]);
    p.run("build");
    assert_eq!(p.state("build"), "pass");
}
fn audit() -> Value {
    check(
        "completion",
        "artifact",
        "thruline-completion-v2",
        json!(["thruline-completion-v2"]),
        json!([".thruline/acceptance.json"]),
        json!({"kind":"completion"}),
    )
}
#[test]
fn completion_requires_suitable_behavior_and_current_child_receipts() {
    let p = Project::new(
        &[("probe.py", "print(75)\n")],
        vec![json_check(), audit()],
        vec![
            criterion("behavior", "behavior"),
            criterion("completion", "audit"),
        ],
    );
    p.run("completion");
    assert_eq!(p.state("completion"), "unknown");
    p.run("behavior");
    let run = p.run("completion");
    let s = p.status();
    assert_eq!(p.state("completion"), "pass", "{s}");
    assert_eq!(p.receipt(&run)["verification"]["replay_agrees"], true);
    p.run("behavior");
    assert_eq!(p.state("completion"), "stale");
    p.run("completion");
    assert_eq!(p.state("completion"), "pass");
    let q = Project::new(
        &[(
            "build.py",
            "from pathlib import Path\nPath('result.txt').write_text('ready\\n')\n",
        )],
        vec![build(), audit()],
        vec![
            criterion("build", "behavior"),
            criterion("completion", "audit"),
        ],
    );
    q.run("build");
    q.run("completion");
    assert_eq!(
        q.state("completion"),
        "unknown",
        "a build cannot establish behavior"
    );
}
#[test]
fn store_budget_abstains_without_deleting_evidence() {
    let p = sample();
    let directory = p.temp.path().join("evidence/exec.db.acceptance/outputs");
    std::fs::create_dir_all(&directory).unwrap();
    let file = directory.join("retained-data");
    std::fs::File::create(&file)
        .unwrap()
        .set_len(417 * 1024 * 1024)
        .unwrap();
    let run = p.run("suite");
    assert_eq!(p.state("suite"), "blocked");
    assert!(file.exists());
    assert!(p.receipt(&run)["verification"]["explanation"]
        .as_str()
        .unwrap()
        .contains("admission budget"));
}
#[test]
fn deadline_is_not_a_product_defect() {
    let mut c = json_check();
    c["timeout_seconds"] = json!(1);
    let p = Project::new(
        &[("probe.py", "import time\ntime.sleep(20)\nprint(75)\n")],
        vec![c],
        vec![criterion("behavior", "behavior")],
    );
    // What is being proved is that the *product* refused to wait for a probe
    // that sleeps twenty times its declared deadline. That fact is in the
    // product's own typed record -- `timed_out`, the observation outcome, and
    // the absence of an exit the probe never reached -- so it is asserted
    // there. The previous bound was this test's own wall clock
    // (`elapsed < 8 s`), which measures how fast this machine runs four CLI
    // subprocesses and a repository setup, and went red under load while the
    // deadline itself worked.
    let run = p.run("behavior");
    let r = p.receipt(&run);
    let finish = &r["record"]["finish"];
    assert_eq!(finish["timed_out"], json!(true), "{finish}");
    assert_eq!(finish["outcome"], json!("timed_out"), "{finish}");
    let observed = &finish["pack"]["observations"][0];
    assert_eq!(observed["outcome"], json!("timed_out"), "{finish}");
    assert_eq!(
        observed["exit"],
        Value::Null,
        "the probe must never have reached its own exit: {finish}"
    );
    assert_eq!(p.state("behavior"), "unknown");
}

#[test]
fn reported_all_green_counts_cannot_override_a_captured_failure() {
    let mut definition = suite();
    definition["verification"]["pack"]["claimed_counts"] =
        json!({"passed":2,"failed":0,"skipped":0,"expected_failure":0});
    let p = Project::new(
        &[
            ("total.py", "def total(value): return value\n"),
            ("tests/test_total.py", TESTS),
        ],
        vec![definition],
        vec![criterion("suite", "tests")],
    );
    p.run("suite");
    let status = p.status();
    let result = &status["pack_results"][0];
    let checks = result["catalog_results"].as_array().unwrap();
    assert_eq!(
        checks.iter().find(|c| c["check_id"] == "EVD-05").unwrap()["status"],
        "fail"
    );
    assert_eq!(
        checks.iter().find(|c| c["check_id"] == "TST-01").unwrap()["status"],
        "pass"
    );
    p.edit(
        "tests/test_total.py",
        &TESTS.replace(
            "    def test_discount",
            "    @unittest.skip(\"pending\")\n    def test_discount",
        ),
    );
    p.run("suite");
    let s = p.status();
    let checks = s["pack_results"][0]["catalog_results"].as_array().unwrap();
    for id in ["EVD-05", "TST-01"] {
        assert_eq!(
            checks.iter().find(|c| c["check_id"] == id).unwrap()["status"],
            "fail",
            "{id}: a count contradiction and skipped required test must both remain visible"
        );
    }
}

#[test]
fn missing_declared_fixture_blocks_instead_of_warning_about_product_behavior() {
    let mut c = json_check();
    c["verification"]["scope"] = json!(["probe.py", "fixture.json"]);
    let p = Project::new(
        &[(
            "probe.py",
            "from pathlib import Path\nprint(Path('fixture.json').read_text())\n",
        )],
        vec![c],
        vec![criterion("behavior", "behavior")],
    );
    let missing = p.run("behavior");
    assert_eq!(p.state("behavior"), "blocked");
    let receipt = p.receipt(&missing);
    assert!(receipt["record"]["finish"]["pack"]["observations"]
        .as_array()
        .unwrap()
        .is_empty());
    p.edit("fixture.json", "75");
    p.run("behavior");
    assert_eq!(p.state("behavior"), "pass");
}
