//! Binary-process boundary for accepted requirements and explicit local runs.
//! Defaults to Cargo; explicit hash-pinned artifact selection qualifies the installed release.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::{json, Value};

#[path = "support/qualified_binary.rs"]
mod qualified_binary;
use qualified_binary::thruline;

struct Fixture {
    _dir: tempfile::TempDir,
    repo: PathBuf,
    db: PathBuf,
    home: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("project with spaces");
        std::fs::create_dir_all(&repo).unwrap();
        let git = Command::new("git")
            .current_dir(&repo)
            .args(["init", "-q"])
            .output()
            .unwrap();
        assert!(
            git.status.success(),
            "{}",
            String::from_utf8_lossy(&git.stderr)
        );
        std::fs::write(
            repo.join("pyproject.toml"),
            "[tool.pytest.ini_options]\ntestpaths = [\"tests\"]\n",
        )
        .unwrap();
        std::fs::write(repo.join("app.py"), "VALUE = 1\n").unwrap();
        std::fs::write(repo.join(".gitignore"), "__pycache__/\n.pytest_cache/\n").unwrap();
        Self {
            repo,
            db: dir.path().join("state/exec.db"),
            home: dir.path().join("home"),
            _dir: dir,
        }
    }

    fn command(&self, action: &str, extra: &[&str]) -> Output {
        Command::new(thruline())
            .args(["acceptance", action, "--json", "--repo"])
            .arg(&self.repo)
            .arg("--exec-db")
            .arg(&self.db)
            .args(extra)
            .env("THRULINE_HOME", &self.home)
            .env("PYTEST_DISABLE_PLUGIN_AUTOLOAD", "1")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .unwrap()
    }

    fn ok(&self, action: &str, extra: &[&str]) -> Value {
        let output = self.command(action, extra);
        assert!(
            output.status.success(),
            "{action}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["schema_version"], 1);
        value
    }

    fn accept(&self) -> Value {
        let draft = self.ok("init", &[]);
        self.ok("accept", &["--digest", draft["digest"].as_str().unwrap()])
    }

    fn tests(&self, source: &str) {
        std::fs::create_dir_all(self.repo.join("tests")).unwrap();
        std::fs::write(self.repo.join("tests/test_app.py"), source).unwrap();
    }

    fn execute(&self, accepted: &Value) -> Value {
        let output = self.command(
            "run",
            &[
                "--check",
                "pytest",
                "--digest",
                accepted["digest"].as_str().unwrap(),
            ],
        );
        let result: Value = serde_json::from_slice(&output.stdout)
            .unwrap_or_else(|error| panic!("{error}: {}", String::from_utf8_lossy(&output.stderr)));
        let expected = match result["state"].as_str() {
            Some("verified") => 0,
            Some("failed") => 1,
            _ => 2,
        };
        assert_eq!(output.status.code(), Some(expected));
        result
    }

    fn verify_receipt(&self, receipt: &Value, db: Option<&std::path::Path>) -> Output {
        Command::new(thruline())
            .arg("verify-receipt")
            .arg(receipt["path"].as_str().unwrap())
            .arg("--key")
            .arg(receipt["public_key"].as_str().unwrap())
            .arg("--repo")
            .arg(&self.repo)
            .arg("--exec-db")
            .arg(db.unwrap_or(&self.db))
            .arg("--json")
            .env("THRULINE_HOME", &self.home)
            .env("PYTEST_DISABLE_PLUGIN_AUTOLOAD", "1")
            .output()
            .unwrap()
    }

    fn signed(&self) -> Value {
        self.tests("def test_one():\n    assert True\n");
        let accepted = self.accept();
        assert_eq!(self.execute(&accepted)["state"], "verified");
        self.ok("receipt", &[])
    }
}

#[test]
fn q01_accepted_requirements_survive_a_new_process_without_any_agent_claim() {
    let f = Fixture::new();
    let accepted = f.accept();
    assert_eq!(accepted["commands_executed"], false);
    let result = f.ok("status", &[]);
    assert_eq!(result["accepted"], true);
    assert_eq!(result["state"], "unverified");
    assert_eq!(result["evaluation"]["checks"][0]["check_id"], "pytest");
    assert_eq!(result["evaluation"]["checks"][0]["state"], "unverified");
    assert_eq!(result["acceptance_hash"], accepted["acceptance_hash"]);
    assert!(
        !f.home.join("ledger.db").exists(),
        "legacy evidence must not be created or opened"
    );
}

#[test]
fn q02_inspection_and_bad_contracts_do_not_create_an_execution_store() {
    let f = Fixture::new();
    let empty = f.ok("status", &[]);
    assert_eq!(empty["accepted"], false);
    assert_eq!(empty["state"], "unverified");
    assert!(!f.db.exists());
    let draft = f.ok("init", &[]);
    let path = f.repo.join(".thruline/acceptance.json");
    let mut changed = draft["contract"].clone();
    changed["criteria"] = json!([]);
    std::fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
    let refused = f.command("accept", &["--digest", draft["digest"].as_str().unwrap()]);
    assert!(!refused.status.success());
    assert!(!f.db.exists());
}

#[test]
fn q03_changed_or_deleted_draft_cannot_remove_accepted_work() {
    let f = Fixture::new();
    let accepted = f.accept();
    let path = f.repo.join(".thruline/acceptance.json");
    std::fs::remove_file(&path).unwrap();
    let result = f.ok("status", &[]);
    assert_eq!(result["acceptance_hash"], accepted["acceptance_hash"]);
    assert_eq!(result["pending_draft_change"], true);
    assert_eq!(result["evaluation"]["checks"].as_array().unwrap().len(), 1);
}

#[test]
fn q04_replacement_requires_reviewed_digest_revision_and_previous_acceptance() {
    let f = Fixture::new();
    let accepted = f.accept();
    let path = f.repo.join(".thruline/acceptance.json");
    let mut changed: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    changed["objective"] = "Verify the revised requirements".into();
    changed["revision"] = 2.into();
    std::fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
    let status = f.ok("status", &[]);
    let digest = status["draft_digest"].as_str().unwrap();
    assert!(!f.command("accept", &["--digest", digest]).status.success());
    let next = f.ok(
        "accept",
        &[
            "--digest",
            digest,
            "--supersedes",
            accepted["acceptance_hash"].as_str().unwrap(),
        ],
    );
    assert_ne!(next["acceptance_hash"], accepted["acceptance_hash"]);
    let reader = thruline_ledger::Ledger::open_read_only(&f.db).unwrap();
    let repo = f.repo.canonicalize().unwrap();
    let history = reader
        .acceptance_history(&thruline_ledger::RepoId::from_key(&repo.to_string_lossy()))
        .unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].prev_hash, history[0].hash);
}

#[test]
fn q19_manual_packet_carries_requirements_without_claiming_confirmed_recovery() {
    let f = Fixture::new();
    let accepted = f.accept();
    let packet = f.ok("handoff", &[]);
    assert_eq!(packet["handoff_confirmed"], false);
    assert_eq!(packet["acceptance_hash"], accepted["acceptance_hash"]);
    assert_eq!(packet["evaluation"]["criteria"][0]["state"], "unverified");
    assert!(packet["workspace"].as_str().unwrap().len() == 64);
}

#[test]
fn init_refuses_to_overwrite_a_reviewed_draft() {
    let f = Fixture::new();
    f.ok("init", &[]);
    let path = f.repo.join(".thruline/acceptance.json");
    let before = std::fs::read(&path).unwrap();
    assert!(!f.command("init", &[]).status.success());
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn q05_actual_pytest_pass_is_current_and_bound_to_the_accepted_check() {
    let f = Fixture::new();
    f.tests("def test_one():\n    assert 1 + 1 == 2\ndef test_two():\n    assert True\n");
    let accepted = f.accept();
    let result = f.execute(&accepted);
    assert_eq!(result["state"], "verified", "{result:#}");
    assert_eq!(result["attempts_executed"], 1);
    assert_eq!(result["runs"][0]["counts"]["passed"], 2);
    assert_eq!(result["runs"][0]["counts"]["total"], 2);
    assert_eq!(result["runs"][0]["trust"], "local");
    assert_eq!(result["runs"][0]["run_id"], result["run_id"]);
    assert_eq!(f.ok("status", &[])["evaluation"], result["evaluation"]);
}

#[test]
fn q06_source_or_lockfile_change_invalidates_a_historical_pass() {
    let f = Fixture::new();
    f.tests("def test_one():\n    assert True\n");
    let accepted = f.accept();
    assert_eq!(f.execute(&accepted)["state"], "verified");
    std::fs::write(f.repo.join("app.py"), "VALUE = 2\n").unwrap();
    let stale = f.ok("status", &[]);
    assert_eq!(stale["state"], "unverified");
    assert_eq!(stale["runs"][0]["counts"]["passed"], 1);
    assert_eq!(stale["runs"][0]["current"], false);
    std::fs::write(f.repo.join("app.py"), "VALUE = 1\n").unwrap();
    std::fs::write(f.repo.join("requirements.lock"), "changed dependency\n").unwrap();
    assert_eq!(f.ok("status", &[])["state"], "unverified");
}

#[test]
fn q07_real_failed_and_empty_suites_are_not_green() {
    let failed = Fixture::new();
    failed.tests("def test_one():\n    assert False\n");
    let accepted = failed.accept();
    let result = failed.execute(&accepted);
    assert_eq!(result["state"], "failed", "{result:#}");
    assert_eq!(result["runs"][0]["counts"]["failed"], 1);
    let empty = Fixture::new();
    empty.tests("# deliberately no tests\n");
    let accepted = empty.accept();
    assert_ne!(empty.execute(&accepted)["state"], "verified");
}

#[test]
fn q08_timeout_is_bounded_and_remains_unverified_after_restart() {
    let f = Fixture::new();
    f.tests("import time\ndef test_hangs():\n    time.sleep(30)\n");
    let mut draft = f.ok("init", &[])["contract"].clone();
    draft["checks"][0]["timeout_seconds"] = json!(1);
    std::fs::write(
        f.repo.join(".thruline/acceptance.json"),
        serde_json::to_vec(&draft).unwrap(),
    )
    .unwrap();
    let digest = f.ok("status", &[])["draft_digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let accepted = f.ok("accept", &["--digest", &digest]);
    // The declared one-second timeout is enforced by the product, and the
    // product records that it enforced it: the run's outcome is `timed_out`,
    // which it can only be after the child was stopped rather than waited for.
    // This used to be `started.elapsed() < 8 s` -- this test's own clock around
    // an accept, a run and a status, three separate CLI processes -- so a
    // loaded machine could fail it while the timeout worked perfectly.
    // (TIMING-TESTS-OWNER, family of R-2026-09-24-27.)
    let result = f.execute(&accepted);
    let run = result["runs"]
        .as_array()
        .and_then(|runs| runs.iter().find(|r| r["check_id"] == "pytest"))
        .unwrap_or_else(|| panic!("no recorded run for the accepted check: {result}"));
    assert_eq!(
        run["outcome"], "timed_out",
        "declared timeout was not enforced: {result}"
    );
    assert_eq!(result["state"], "unverified");
    assert_eq!(f.ok("status", &[])["state"], "unverified");
}

#[test]
fn q09_an_unwrapped_command_cannot_satisfy_an_obligation() {
    let f = Fixture::new();
    f.tests("def test_one():\n    assert True\n");
    f.accept();
    let out = Command::new("python3")
        .args(["-m", "pytest", "-q"])
        .current_dir(&f.repo)
        .env("PYTEST_DISABLE_PLUGIN_AUTOLOAD", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "pytest prerequisite failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result = f.ok("status", &[]);
    assert_eq!(result["state"], "unverified");
    assert!(result["runs"].as_array().unwrap().is_empty());
}

#[test]
fn q11_changed_captured_output_cannot_remain_green() {
    let f = Fixture::new();
    f.tests("def test_one():\n    assert True\n");
    let accepted = f.accept();
    let result = f.execute(&accepted);
    assert_eq!(result["state"], "verified");
    let hash = result["runs"][0]["outputs"][0]["digest"]
        .as_str()
        .unwrap()
        .strip_prefix("sha256:")
        .unwrap();
    let path =
        f.db.parent()
            .unwrap()
            .join("exec.db.acceptance/outputs")
            .join(format!("{hash}.out"));
    std::fs::write(path, "999 passed in 0.01s\n").unwrap();
    assert_eq!(f.ok("status", &[])["state"], "unverified");
}

#[test]
fn q03_weakened_protected_test_is_visible_even_when_the_new_run_passes() {
    let f = Fixture::new();
    f.tests("def test_one():\n    assert False\n");
    let accepted = f.accept();
    f.tests("def test_one():\n    assert True\n");
    let result = f.execute(&accepted);
    assert_eq!(result["runs"][0]["counts"]["passed"], 1);
    assert_eq!(result["state"], "unverified");
    assert_eq!(result["protected_inputs_verified"], false);
    assert_eq!(result["protected_changes"], json!(["tests/test_app.py"]));
}

#[test]
fn q05_npm_script_runs_actual_node_tests_without_buying_or_installing_dependencies() {
    let f = Fixture::new();
    std::fs::remove_file(f.repo.join("pyproject.toml")).unwrap();
    std::fs::write(
        f.repo.join("package.json"),
        r#"{"scripts":{"test":"node --test tests/*.mjs"}}"#,
    )
    .unwrap();
    std::fs::create_dir_all(f.repo.join("tests")).unwrap();
    std::fs::write(f.repo.join("tests/test.mjs"), "import test from 'node:test'; import assert from 'node:assert/strict'; test('sum',()=>assert.equal(1+1,2));\n").unwrap();
    // Explicit filenames keep discovery literal and auditable.
    std::fs::write(
        f.repo.join("package.json"),
        r#"{"scripts":{"test":"node --test tests/test.mjs"}}"#,
    )
    .unwrap();
    let accepted = f.accept();
    let result = f.ok(
        "run",
        &[
            "--check",
            "test",
            "--digest",
            accepted["digest"].as_str().unwrap(),
        ],
    );
    assert_eq!(result["state"], "verified", "{result:#}");
    assert_eq!(result["runs"][0]["counts"]["passed"], 1);
}

#[test]
fn q06_material_process_environment_change_invalidates_a_pass_without_exporting_values() {
    let f = Fixture::new();
    f.tests("def test_one():\n    assert True\n");
    let accepted = f.accept();
    assert_eq!(f.execute(&accepted)["state"], "verified");
    let output = Command::new(thruline())
        .args(["acceptance", "status", "--json", "--repo"])
        .arg(&f.repo)
        .arg("--exec-db")
        .arg(&f.db)
        .env("THRULINE_HOME", &f.home)
        .env("PYTEST_DISABLE_PLUGIN_AUTOLOAD", "1")
        .env("THRULINE_TEST_MATERIAL_INPUT", "private-low-entropy-value")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["state"], "unverified");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private-low-entropy-value"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("process_environment_private"));
}

#[test]
fn q10_preexisting_report_cannot_become_producer_bound_by_running_a_noop() {
    let f = Fixture::new();
    let mut draft = f.ok("init", &[])["contract"].clone();
    draft["checks"][0]["category"] = json!("artifact");
    draft["checks"][0]["argv"] = json!(["python3", "-c", "pass"]);
    draft["checks"][0]["runner"] = json!("python3");
    draft["checks"][0]["artifacts"] = json!([{"path":"report.xml","sha256":null}]);
    std::fs::write(
        f.repo.join("report.xml"),
        "<testsuite tests=\"10\" failures=\"0\"/>\n",
    )
    .unwrap();
    std::fs::write(
        f.repo.join(".thruline/acceptance.json"),
        serde_json::to_vec(&draft).unwrap(),
    )
    .unwrap();
    let digest = f.ok("status", &[])["draft_digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let accepted = f.ok("accept", &["--digest", &digest]);
    let result = f.execute(&accepted);
    assert_eq!(result["state"], "unverified");
    assert!(result["runs"][0]["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason.as_str().unwrap().contains("already existed")));
}

#[test]
fn q11_protected_input_changed_during_execution_cannot_produce_a_current_pass() {
    let f = Fixture::new();
    f.tests("from pathlib import Path\ndef test_mutates_input():\n    Path('app.py').write_text('VALUE = 99\\n')\n    assert True\n");
    let accepted = f.accept();
    let result = f.execute(&accepted);
    assert_eq!(result["runs"][0]["counts"]["passed"], 1);
    assert_eq!(result["state"], "unverified");
    assert!(result["runs"][0]["reasons"]
        .as_array()
        .unwrap()
        .iter()
        .any(|reason| reason == "inputs changed during execution"));
}

#[test]
fn q10_fresh_artifact_has_an_honest_control_and_later_mutation_is_detected() {
    let f = Fixture::new();
    std::fs::write(f.repo.join(".gitignore"), "report.txt\n").unwrap();
    let mut draft = f.ok("init", &[])["contract"].clone();
    draft["checks"][0]["category"] = json!("artifact");
    draft["checks"][0]["argv"] = json!([
        "python3",
        "-c",
        "from pathlib import Path; Path('report.txt').write_text('delivered')"
    ]);
    draft["checks"][0]["runner"] = json!("python3");
    draft["checks"][0]["artifacts"] = json!([{"path":"report.txt","sha256":null}]);
    std::fs::write(
        f.repo.join(".thruline/acceptance.json"),
        serde_json::to_vec(&draft).unwrap(),
    )
    .unwrap();
    let digest = f.ok("status", &[])["draft_digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let accepted = f.ok("accept", &["--digest", &digest]);
    let result = f.execute(&accepted);
    assert_eq!(result["state"], "verified", "{result:#}");
    assert_eq!(result["runs"][0]["artifacts"].as_object().unwrap().len(), 1);
    std::fs::write(f.repo.join("report.txt"), "changed").unwrap();
    assert_eq!(f.ok("status", &[])["state"], "unverified");
}

#[test]
fn q13_detached_signed_result_reproduces_from_the_current_local_record() {
    let f = Fixture::new();
    let receipt = f.signed();
    assert!(!PathBuf::from(receipt["path"].as_str().unwrap()).starts_with(&f.repo));
    let output = f.verify_receipt(&receipt, None);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["schema_version"], 2);
    assert_eq!(result["signature_valid"], true);
    assert_eq!(result["state"], "verified");
    assert_eq!(result["trust"], "local");
    assert_eq!(
        result["signed_result"]["evaluation"],
        f.ok("status", &[])["evaluation"]
    );
}

#[test]
fn q14_valid_signature_without_the_local_evidence_cannot_grant_trust() {
    let f = Fixture::new();
    let receipt = f.signed();
    let absent = f.home.join("missing-evidence.db");
    let output = f.verify_receipt(&receipt, Some(&absent));
    assert_eq!(
        output.status.code(),
        Some(2),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["signature_valid"], true);
    assert_eq!(result["state"], "unverified");
    assert_eq!(result["trust"], "untrusted");
    assert!(
        !absent.exists(),
        "verification must not create a replacement store"
    );
}

#[test]
fn q16_source_edit_leaves_the_signature_valid_and_invalidates_current_use() {
    let f = Fixture::new();
    let receipt = f.signed();
    std::fs::write(f.repo.join("app.py"), "VALUE = 44\n").unwrap();
    let output = f.verify_receipt(&receipt, None);
    assert_eq!(output.status.code(), Some(2));
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["signature_valid"], true);
    assert_eq!(result["signed_result"]["state"], "verified");
    assert_eq!(result["state"], "unverified");
}

#[test]
fn q13_and_q15_valid_signatures_cannot_cover_incomplete_or_unknown_required_semantics() {
    let f = Fixture::new();
    let receipt = f.signed();
    let path = PathBuf::from(receipt["path"].as_str().unwrap());
    let envelope: thruline_attest::Envelope =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let original: Value = serde_json::from_slice(&envelope.payload_bytes().unwrap()).unwrap();
    let private =
        PathBuf::from(receipt["public_key"].as_str().unwrap()).with_file_name("signing.key");
    let key = thruline_attest::key::load_signing(&private).unwrap();
    for case in [
        "missing-result",
        "duplicate-result",
        "unknown-version",
        "self-trusted",
        "wrong-tally",
        "dangling-run",
        "missing-artifact-binding",
    ] {
        let mut payload = original.clone();
        match case {
            "missing-result" => payload["predicate"]["evaluation"]["checks"] = json!([]),
            "duplicate-result" => {
                let first = payload["predicate"]["evaluation"]["checks"][0].clone();
                payload["predicate"]["evaluation"]["checks"]
                    .as_array_mut()
                    .unwrap()
                    .push(first);
            }
            "unknown-version" => payload["predicate"]["version"] = json!(99),
            "self-trusted" => payload["predicate"]["trusted"] = json!(true),
            "wrong-tally" => payload["predicate"]["runs"][0]["counts"]["total"] = json!(999),
            "dangling-run" => {
                payload["predicate"]["evaluation"]["checks"][0]["run_id"] = json!("different-run")
            }
            "missing-artifact-binding" => {
                payload["predicate"]["contract"]["checks"][0]["artifacts"] =
                    json!([{"path":"required.xml"}]);
                let check: thruline_policy::acceptance::Check =
                    serde_json::from_value(payload["predicate"]["contract"]["checks"][0].clone())
                        .unwrap();
                payload["predicate"]["runs"][0]["check_digest"] = json!(check.digest().unwrap());
            }
            _ => unreachable!(),
        }
        let changed = thruline_attest::sign(
            &serde_json::to_vec(&payload).unwrap(),
            thruline_attest::PAYLOAD_TYPE,
            &key,
            None,
        );
        std::fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
        let output = f.verify_receipt(&receipt, None);
        assert!(!output.status.success(), "{case} unexpectedly verified");
        assert!(
            output.stdout.is_empty(),
            "{case}: malformed semantics were rendered as a result"
        );
    }
}

#[test]
fn q13_changing_a_signed_field_without_resigning_fails_crypto_verification() {
    use base64::Engine;
    let f = Fixture::new();
    let receipt = f.signed();
    let path = PathBuf::from(receipt["path"].as_str().unwrap());
    let mut envelope: thruline_attest::Envelope =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut payload: Value = serde_json::from_slice(&envelope.payload_bytes().unwrap()).unwrap();
    payload["predicate"]["next_action"] = json!("changed after signing");
    envelope.payload =
        base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(&payload).unwrap());
    std::fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    let output = f.verify_receipt(&receipt, None);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no signature"));
}

#[test]
fn q22_portable_receipt_omits_freeform_objectives_and_raw_test_output() {
    let f = Fixture::new();
    let private = "PRIVATE_REQUIREMENT_AND_OUTPUT_SENTINEL";
    f.tests(&format!(
        "def test_one():\n    print('{private}')\n    assert True\n"
    ));
    let mut draft = f.ok("init", &[])["contract"].clone();
    draft["objective"] = json!(private);
    draft["checks"][0]["argv"] = json!(["python3", "-m", "pytest", "-q", "-s"]);
    std::fs::write(
        f.repo.join(".thruline/acceptance.json"),
        serde_json::to_vec(&draft).unwrap(),
    )
    .unwrap();
    let digest = f.ok("status", &[])["draft_digest"]
        .as_str()
        .unwrap()
        .to_owned();
    let accepted = f.ok("accept", &["--digest", &digest]);
    f.execute(&accepted);
    let receipt = f.ok("receipt", &[]);
    let envelope: thruline_attest::Envelope =
        serde_json::from_slice(&std::fs::read(receipt["path"].as_str().unwrap()).unwrap()).unwrap();
    assert!(!String::from_utf8(envelope.payload_bytes().unwrap())
        .unwrap()
        .contains(private));
}

#[test]
fn q18_q24_built_ui_api_is_the_same_cli_record_and_rejects_browser_selected_inputs() {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpStream;
    use std::process::Stdio;
    let f = Fixture::new();
    f.tests("def test_real():\n    assert 2 + 2 == 4\n");
    let draft = f.ok("init", &[]);
    f.ok("accept", &["--digest", draft["digest"].as_str().unwrap()]);
    f.execute(&draft);
    let expected = f.ok("status", &[]);
    struct Server(std::process::Child);
    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut server = Server(
        Command::new(thruline())
            .args(["ui", "--acceptance"])
            .arg(&f.repo)
            .arg("--db")
            .arg(f._dir.path().join("capture/ledger.db"))
            .arg("--exec-db")
            .arg(&f.db)
            .args(["--no-open", "--port", "0", "--dir"])
            .arg(f._dir.path().join("export"))
            .env("THRULINE_HOME", &f.home)
            .env("PYTEST_DISABLE_PLUGIN_AUTOLOAD", "1")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let out = server.0.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if let Some(url) = line.strip_prefix("ThruLine is at http://") {
                let _ = tx.send(url.to_string());
                return;
            }
        }
    });
    let url = rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("server startup is bounded");
    let host = url.split('/').next().unwrap();
    let get = |path: &str, headers: &str| {
        let mut socket = TcpStream::connect(host).unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(35)))
            .unwrap();
        socket
            .write_all(
                format!(
                    "GET {path} HTTP/1.1\r\nHost: {host}\r\n{headers}Connection: close\r\n\r\n"
                )
                .as_bytes(),
            )
            .unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        let (head, body) = response.split_once("\r\n\r\n").expect("HTTP framing");
        let length: usize = head
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .expect("response length")
            .parse()
            .unwrap();
        assert_eq!(
            body.len(),
            length,
            "server closed a partial response for {path}"
        );
        response
    };
    let response = get("/api/acceptance", "");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    let actual: Value = serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(actual, expected);
    assert!(get("/acceptance", "").contains("/assets/index-"));
    assert!(get("/api/acceptance?repo=/tmp", "").starts_with("HTTP/1.1 400"));
    assert!(
        get("/api/acceptance", "Origin: https://example.invalid\r\n").starts_with("HTTP/1.1 403")
    );
    assert!(get("/api/acceptance", "Host: malicious.invalid\r\n").starts_with("HTTP/1.1 403"));
    assert!(get("/api/claims", "").starts_with("HTTP/1.1 404"));
    std::fs::write(f.repo.join("app.py"), "VALUE = 99\n").unwrap();
    let stale = get("/api/acceptance", "");
    let stale: Value = serde_json::from_str(stale.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(stale["state"], "unverified");
    assert_eq!(stale, f.ok("status", &[]));
}

#[test]
fn q19_resume_requires_reviewed_packet_and_matching_workspace_branch_and_evidence() {
    let f = Fixture::new();
    f.tests("def test_one():\n    assert True\n");
    let accepted = f.accept();
    let handoff = f.ok("handoff", &[]);
    let path = f._dir.path().join("handoff.json");
    std::fs::write(&path, serde_json::to_vec(&handoff).unwrap()).unwrap();
    let extra = [
        "--packet",
        path.to_str().unwrap(),
        "--packet-digest",
        handoff["packet_digest"].as_str().unwrap(),
    ];
    let acknowledged = f.ok("resume", &extra);
    assert_eq!(acknowledged["handoff_confirmed"], true);
    assert_eq!(acknowledged["state"], "unverified");
    let ledger = thruline_ledger::Ledger::open_read_only(&f.db).unwrap();
    let chain = ledger
        .verify_session(acknowledged["acknowledgement_session"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        chain.head_hash,
        acknowledged["chain_head"].as_str().unwrap()
    );
    assert_eq!(f.ok("handoff", &[])["handoff_confirmed"], false);
    let other = Fixture::new();
    other.accept();
    assert!(!other.command("resume", &extra).status.success());
    let git = Command::new("git")
        .current_dir(&f.repo)
        .args(["symbolic-ref", "HEAD", "refs/heads/other"])
        .output()
        .unwrap();
    assert!(git.status.success());
    assert!(!f.command("resume", &extra).status.success());
    let original_branch = handoff["packet"]["branch"].as_str().unwrap();
    assert!(Command::new("git")
        .current_dir(&f.repo)
        .args(["symbolic-ref", "HEAD", original_branch])
        .status()
        .unwrap()
        .success());
    f.execute(&accepted);
    // Same tree and contract, but the received open-work evidence is now stale.
    assert!(!f.command("resume", &extra).status.success());
    let mut changed = handoff.clone();
    changed["packet"]["evaluation"]["state"] = json!("verified");
    std::fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
    assert!(!f.command("resume", &extra).status.success());
}

#[test]
fn q02_unknown_empty_and_malformed_contracts_never_create_evidence() {
    for raw in [
        "",
        "{",
        "{}",
        "{\"version\":99}",
        "{\"version\":1,\"trusted\":true}",
    ] {
        let f = Fixture::new();
        std::fs::create_dir_all(f.repo.join(".thruline")).unwrap();
        std::fs::write(f.repo.join(".thruline/acceptance.json"), raw).unwrap();
        assert!(!f.command("status", &[]).status.success(), "{raw}");
        assert!(!f
            .command("accept", &["--digest", "sha256:unknown"])
            .status
            .success());
        assert!(!f.db.exists());
    }
}

#[test]
fn q08_a_new_persisted_begin_without_finish_cannot_fall_back_to_an_old_green_run() {
    use thruline_core::spec::event::{Event, EventId, Source};
    use thruline_ledger::{EventRow, Ledger, RepoId};
    let f = Fixture::new();
    f.tests("def test_one():\n    assert True\n");
    let accepted = f.accept();
    let good = f.execute(&accepted);
    assert_eq!(good["state"], "verified");
    let ledger = Ledger::open(&f.db).unwrap();
    let old = ledger
        .session_events(good["run_id"].as_str().unwrap())
        .unwrap();
    // Persist exactly the begin state left by an interrupted producer, with
    // its original identities. This is a storage fault fixture, not a live crash.
    let id = format!("acceptance-{}", EventId::generate());
    let repo = RepoId::from_key(&f.repo.canonicalize().unwrap().to_string_lossy());
    ledger
        .upsert_session(&id, &repo, "acceptance-local-v1", None, false, None)
        .unwrap();
    let event = Event::new(
        &id,
        1,
        thruline_core::ingest::iso8601_utc_now(),
        Source::Thruline,
        old[0].payload_typed().unwrap(),
        thruline_core::spec::chain::genesis_prev_hash(&id),
    )
    .unwrap()
    .seal()
    .unwrap();
    ledger
        .append_events(&repo, &[EventRow::bare(event)])
        .unwrap();
    drop(ledger);
    let now = f.ok("status", &[]);
    assert_eq!(now["state"], "unverified");
    assert_eq!(now["evaluation"]["checks"][0]["run_id"], id);
    assert_eq!(now["runs"][0]["counts"], Value::Null);
}

#[test]
fn q07_all_skipped_tests_and_q20_repeated_failure_remain_bounded_explicit_attempts() {
    let skipped = Fixture::new();
    skipped.tests("import pytest\n@pytest.mark.skip(reason='control')\ndef test_skipped():\n    assert True\n");
    let accepted = skipped.accept();
    let result = skipped.execute(&accepted);
    assert_eq!(result["state"], "unverified");
    assert_eq!(result["runs"][0]["counts"]["passed"], 0);
    let failed = Fixture::new();
    failed.tests("def test_one():\n    assert False\n");
    let accepted = failed.accept();
    let first = failed.execute(&accepted);
    let second = failed.execute(&accepted);
    assert_eq!(first["state"], "failed");
    assert_eq!(second["state"], "failed");
    assert_eq!(first["attempts_executed"], 1);
    assert_eq!(second["attempts_executed"], 1);
    assert_ne!(first["run_id"], second["run_id"]);
    assert!(second["next_action"].as_str().unwrap().contains("Inspect"));
}

#[test]
fn q23_missing_executable_is_a_refusal_not_a_pass() {
    let f = Fixture::new();
    let mut draft = f.ok("init", &[])["contract"].clone();
    draft["checks"][0]["argv"][0] = json!("definitely-missing-thruline-qualification-runner");
    std::fs::write(
        f.repo.join(".thruline/acceptance.json"),
        serde_json::to_vec(&draft).unwrap(),
    )
    .unwrap();
    let digest = f.ok("status", &[])["draft_digest"]
        .as_str()
        .unwrap()
        .to_string();
    f.ok("accept", &["--digest", &digest]);
    let output = f.command("run", &["--check", "pytest", "--digest", &digest]);
    assert!(!output.status.success());
    assert_eq!(f.ok("status", &[])["state"], "unverified");
}

/// RECEIPT-V01-DEFAULT: `thruline receipt` now defaults to predicate v0.1.
/// `acceptance receipt` is not that verb and does not follow it — it is v0.2
/// only by design, and it has no version flag to move.
#[test]
fn acceptance_receipt_stays_predicate_v02_and_has_no_version_flag() {
    let f = Fixture::new();
    let receipt = f.signed();
    assert_eq!(
        receipt["predicate_type"],
        "https://thrulinehq.com/attestation/execution-check/v0.2"
    );
    let envelope: thruline_attest::Envelope =
        serde_json::from_slice(&std::fs::read(receipt["path"].as_str().unwrap()).unwrap()).unwrap();
    let statement: Value = serde_json::from_slice(&envelope.payload_bytes().unwrap()).unwrap();
    assert_eq!(
        statement["predicateType"],
        "https://thrulinehq.com/attestation/execution-check/v0.2"
    );
    assert_eq!(statement["predicate"]["kind"], "accepted_requirements");

    let refused = f.command("receipt", &["--predicate-version", "0.1"]);
    assert_eq!(
        refused.status.code(),
        Some(2),
        "`acceptance receipt` accepted a version flag: {}",
        String::from_utf8_lossy(&refused.stderr)
    );
}
