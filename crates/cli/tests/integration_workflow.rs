//! Security and acceptance regressions using a controlled producer, not scanner qualification.
#![cfg(target_os = "macos")]
use serde_json::{json, Value};
use std::{fs, path::PathBuf, process::Command};
struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    db: PathBuf,
    registry: PathBuf,
    home: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("repo");
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let home = temp.path().join("home");
        fs::create_dir(&home).unwrap();
        let db = temp.path().join("core.db");
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.name", "Fixture"],
            vec!["config", "user.email", "fixture@localhost"],
        ] {
            assert!(Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .status()
                .unwrap()
                .success());
        }
        fs::write(root.join("source.js"), "const value = 1;\n").unwrap();
        assert!(Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["add", "."])
            .status()
            .unwrap()
            .success());
        assert!(Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["commit", "-qm", "fixture"])
            .status()
            .unwrap()
            .success());
        let registry = temp.path().join("registry.json");
        fs::create_dir(temp.path().join("policies")).unwrap();
        fs::write(temp.path().join("policies/no-eval.yml"), "fixture rule").unwrap();
        let sha = thruline_core::chain::sha256_hex(&fs::read("/bin/echo").unwrap());
        fs::write(&registry,serde_json::to_vec(&json!({"schema":"thruline.integration-runtimes.v1","tools":{"ast-grep":{"version":"0.45.3","executable":"/bin/echo","sha256":sha,"runtime":[],"prefix":[]}}})).unwrap()).unwrap();
        Self {
            _temp: temp,
            root,
            db,
            registry,
            home,
        }
    }
    fn cmd(&self, action: &str, digest: &str) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_thruline"));
        c.env("THRULINE_HOME", &self.home)
            .env("THRULINE_INTEGRATION_REGISTRY", &self.registry)
            .arg("--db")
            .arg(&self.db)
            .args(["integration", "workflow", "--root"])
            .arg(&self.root)
            .args([
                "--action", action, "--recipe", "ast-grep", "--digest", digest,
            ]);
        c
    }
    fn ok(&self, action: &str, digest: &str) -> Value {
        let o = self.cmd(action, digest).output().unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        serde_json::from_slice(&o.stdout).unwrap()
    }
}
#[test]
fn preview_is_read_only_acceptance_is_separate_and_conflicting_reviews_refuse() {
    let f = Fixture::new();
    let preview = f.ok("preview", "");
    assert_eq!(
        preview["plan"]["criterion"]["condition"]["kind"],
        "no_structural_matches"
    );
    assert_eq!(
        preview["plan"]["criterion"]["success"],
        "No matches for the selected structural rule in the scanned project files."
    );
    assert!(preview["plan"]["argv"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "--hint=unused-suppression"));
    assert!(!f.db.exists());
    assert!(!f.home.join("exec.db").exists());
    assert!(!f.cmd("run", "not-accepted").status().unwrap().success());
    let accepted = f.ok("accept", preview["digest"].as_str().unwrap());
    assert_eq!(accepted["commands_executed"], false);
    assert!(!f
        .cmd("accept", preview["digest"].as_str().unwrap())
        .status()
        .unwrap()
        .success());
    let store = thruline_ledger::Ledger::open_read_only(&f.home.join("exec.db")).unwrap();
    let history = store
        .acceptance_history(&thruline_ledger::RepoId::from_key(
            &f.root.to_string_lossy(),
        ))
        .unwrap();
    assert_eq!(history.len(), 1);
    let doc = thruline_policy::acceptance::Document::parse(history[0].document.as_bytes()).unwrap();
    assert!(doc.contract().criteria[0].human_acceptance_required);
}

#[test]
fn structural_condition_cannot_be_applied_to_another_native_route() {
    let f = Fixture::new();
    let mut plan = f.ok("preview", "")["plan"].clone();
    plan["pack"] = json!("semgrep");
    plan["format"] = json!("semgrep-json-v1");
    plan["support"] = Value::Null;
    let path = f._temp.path().join("wrong-route.json");
    fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .arg("--db")
        .arg(&f.db)
        .args(["integration", "run", "--plan"])
        .arg(path)
        .arg("--accept")
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr)
        .contains("no_structural_matches requires the native ast-grep capture route"));
    assert!(!f.db.exists());
}
#[test]
fn changed_rules_and_unsupported_versions_require_another_review() {
    let f = Fixture::new();
    let preview = f.ok("preview", "");
    let accepted = f.ok("accept", preview["digest"].as_str().unwrap());
    fs::write(
        f.registry.parent().unwrap().join("policies/no-eval.yml"),
        "weakened rule",
    )
    .unwrap();
    let o = f
        .cmd("run", accepted["acceptance_hash"].as_str().unwrap())
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("configuration changed"));
    let mut registry: Value = serde_json::from_slice(&fs::read(&f.registry).unwrap()).unwrap();
    registry["tools"]["ast-grep"]["version"] = json!("999.0");
    fs::write(&f.registry, serde_json::to_vec(&registry).unwrap()).unwrap();
    let status = f.ok("status", "");
    assert_eq!(status["recipes"][0]["ready"], false);
    assert!(status["recipes"][0]["setup"]
        .as_str()
        .unwrap()
        .contains("Unsupported runtime version"));
}
#[test]
fn accepted_plan_cannot_be_changed_to_a_weaker_condition() {
    let f = Fixture::new();
    let preview = f.ok("preview", "");
    let accepted = f.ok("accept", preview["digest"].as_str().unwrap());
    let mut plan = preview["plan"].clone();
    // Use the actual accepted runner digest, independent of JSON map order.
    let ledger = thruline_ledger::Ledger::open_read_only(&f.home.join("exec.db")).unwrap();
    let rows = ledger
        .acceptance_history(&thruline_ledger::RepoId::from_key(
            &f.root.to_string_lossy(),
        ))
        .unwrap();
    let doc = thruline_policy::acceptance::Document::parse(rows[0].document.as_bytes()).unwrap();
    let actual = doc.contract().checks[0]
        .runner
        .strip_prefix("integration:")
        .unwrap();
    assert_ne!(actual, "");
    plan["acceptance"] = json!({"store":f.home.join("exec.db"),"revision_hash":accepted["acceptance_hash"],"check_id":"integration-ast-grep","plan_digest":actual});
    plan["criterion"]["success"] = json!("Pretend any output is enough");
    let path = f.home.join("attack.json");
    fs::write(&path, serde_json::to_vec(&plan).unwrap()).unwrap();
    let o = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .env("THRULINE_HOME", &f.home)
        .env("THRULINE_INTEGRATION_REGISTRY", &f.registry)
        .arg("--db")
        .arg(&f.db)
        .args(["integration", "run", "--plan"])
        .arg(path)
        .arg("--accept")
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("plan differs from the accepted"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
}

#[test]
fn fresh_project_uses_selected_acceptance_store_and_versioned_support() {
    let f = Fixture::new();
    let store = f._temp.path().join("selected-acceptance.db");
    let call = |action: &str, digest: &str| -> Value {
        let o = f
            .cmd(action, digest)
            .arg("--exec-db")
            .arg(&store)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        serde_json::from_slice(&o.stdout).unwrap()
    };
    let preview = call("preview", "");
    assert_eq!(
        preview["plan"]["support"]["schema"],
        "thruline.tool-support.v1"
    );
    assert!(!store.exists());
    let accepted = call("accept", preview["digest"].as_str().unwrap());
    assert!(store.exists());
    assert!(!f.home.join("exec.db").exists());
    let ledger = thruline_ledger::Ledger::open_read_only(&store).unwrap();
    let history = ledger
        .acceptance_history(&thruline_ledger::RepoId::from_key(
            &f.root.to_string_lossy(),
        ))
        .unwrap();
    assert_eq!(history.len(), 1);
    let o = Command::new(env!("CARGO_BIN_EXE_thruline"))
        .env("THRULINE_HOME", &f.home)
        .env("THRULINE_INTEGRATION_REGISTRY", &f.registry)
        .arg("--db")
        .arg(&f.db)
        .args(["integration", "project", "--root"])
        .arg(&f.root)
        .arg("--exec-db")
        .arg(&store)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let projection: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(projection["session"], "local-verification");
    assert_eq!(
        projection["workflow"]["accepted_revision"],
        accepted["acceptance_hash"]
    );
    assert_eq!(projection["workflow"]["recipes"][0]["accepted"], true);
    assert_eq!(
        projection["workflow"]["recipes"][0]["support"],
        preview["plan"]["support"]
    );
    assert!(projection["workflow"]["recipes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["support"].is_object()));
}
