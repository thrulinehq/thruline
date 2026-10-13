//! Controlled defect fixtures, executed through the actual CLI and evidence store.
use serde_json::{json, Value};
use std::{path::PathBuf, process::Command};
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
        let doc = json!({"version":1,"id":"controlled-pack3","revision":1,"objective":"Synthetic check-pack regression; not customer evidence","checks":checks,"criteria":criteria,"protected_paths":[".thruline/acceptance.json"]});
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
fn rules() -> Value {
    json!({"id":"layers-v1","layers":{"core":"domain","port":"interface","view":"presentation"},"external_modules":[],"forbidden":[{"id":"domain-no-ui","from":"domain","to":"presentation"}],"forbid_cycles":true,"cycle_exemptions":[]})
}
fn architecture(kind: &str, policy: Value) -> Value {
    check(
        "architecture",
        "lint",
        "thruline-python-architecture-v3",
        json!(["python3"]),
        json!(["src"]),
        json!({"kind":"architecture","root":"src","check":kind,"policy":policy}),
    )
}
fn rust_architecture(kind: &str) -> Value {
    check(
        "architecture",
        "lint",
        "thruline-architecture-v4",
        json!(["python3"]),
        json!(["cargo"]),
        json!({"kind":"architecture","root":"cargo","graph":"rust_packages","check":kind,"policy":{
            "id":"rust-layers-v1","layers":{"package.domain":"domain","package.ui":"presentation","external":"dependency"},
            "external_modules":["external"],"forbidden":[{"id":"domain-no-ui","from":"domain","to":"presentation"}],
            "forbid_cycles":true,"cycle_exemptions":[]}}),
    )
}

#[test]
fn rust_workspace_alias_and_conditional_boundary_have_source_and_fresh_repair() {
    let p = Project::new(&[
        ("cargo/Cargo.toml", "[workspace]\nmembers=['domain','ui']\n[workspace.dependencies]\nfrontend={package='ui',path='ui'}\n"),
        ("cargo/domain/Cargo.toml", "[package]\nname='domain'\nversion='0.1.0'\n[target.'cfg(unix)'.dependencies]\nfrontend={workspace=true,optional=true}\n"),
        ("cargo/ui/Cargo.toml", "[package]\nname='ui'\nversion='0.1.0'\n"),
    ], vec![rust_architecture("edges")], vec![criterion("architecture","architecture")]);
    let run = p.run("architecture");
    let receipt = p.receipt(&run);
    assert_eq!(p.state("architecture"), "fail", "{receipt}");
    let edge = &receipt["record"]["finish"]["pack"]["observations"][0]["observed"]["edges"][0];
    assert_eq!(edge["source"], "package.domain");
    assert_eq!(edge["target"], "package.ui");
    assert_eq!(edge["line"], 5);
    assert_eq!(edge["condition"], "cfg(unix)");
    assert_eq!(edge["workspace_declaration"]["line"], 4);
    assert!(receipt["verification"]["explanation"]
        .as_str()
        .unwrap()
        .contains("domain-no-ui"));
    p.edit(
        "cargo/domain/Cargo.toml",
        "[package]\nname='domain'\nversion='0.1.0'\n",
    );
    assert_eq!(p.state("architecture"), "stale");
    p.run("architecture");
    assert_eq!(p.state("architecture"), "pass");
    assert_eq!(p.receipt(&run)["receipt_digest"], receipt["receipt_digest"]);
}

#[test]
fn rust_dotted_dependencies_have_locations_and_ignore_multiline_value_headers() {
    let p = Project::new(&[
        ("cargo/Cargo.toml", "[workspace]\nmembers=['domain','ui']\n[workspace.dependencies]\nfrontend.package='ui'\nfrontend.path='ui'\n"),
        ("cargo/domain/Cargo.toml", "[package]\nname='domain'\nversion='0.1.0'\ndescription='''\n[target.'cfg(windows)'.dependencies]\nfrontend.workspace=true\n'''\n[target.'cfg(unix)'.dependencies]\n\"frontend\".workspace=true\nfrontend.optional=true\n"),
        ("cargo/ui/Cargo.toml", "[package]\nname='ui'\nversion='0.1.0'\n"),
    ], vec![rust_architecture("edges")], vec![criterion("architecture","architecture")]);
    let run = p.run("architecture");
    let receipt = p.receipt(&run);
    assert_eq!(p.state("architecture"), "fail", "{receipt}");
    let graph = &receipt["record"]["finish"]["pack"]["observations"][0]["observed"];
    assert!(graph["issues"].as_array().unwrap().is_empty(), "{graph}");
    let edges = graph["edges"].as_array().unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0]["line"], 9);
    assert_eq!(edges[0]["condition"], "cfg(unix)");
    assert_eq!(edges[0]["workspace_declaration"]["line"], 4);
    p.edit("cargo/domain/Cargo.toml", "[package]\nname='domain'\nversion='0.1.0'\ndescription=\"\"\"\n[dependencies]\nfrontend.workspace=true\n\"\"\"\n");
    assert_eq!(p.state("architecture"), "stale");
    p.run("architecture");
    assert_eq!(p.state("architecture"), "pass");
    assert_eq!(p.receipt(&run)["receipt_digest"], receipt["receipt_digest"]);
}

#[test]
fn rust_dev_cycle_and_unresolved_path_remain_distinct() {
    let p = Project::new(&[
        ("cargo/Cargo.toml", "[workspace]\nmembers=['domain','ui']\n"),
        ("cargo/domain/Cargo.toml", "[package]\nname='domain'\nversion='0.1.0'\n[dev-dependencies]\nui={path='../ui'}\n"),
        ("cargo/ui/Cargo.toml", "[package]\nname='ui'\nversion='0.1.0'\n[dependencies]\ndomain={path='../domain'}\n"),
    ], vec![rust_architecture("cycles")], vec![criterion("architecture","architecture")]);
    let run = p.run("architecture");
    let receipt = p.receipt(&run);
    assert_eq!(p.state("architecture"), "fail", "{receipt}");
    assert_eq!(
        receipt["record"]["finish"]["pack"]["observations"][0]["observed"]["cycles"][0]["path"],
        json!(["package.domain", "package.ui", "package.domain"])
    );
    p.edit(
        "cargo/domain/Cargo.toml",
        "[package]\nname='domain'\nversion='0.1.0'\n[dependencies]\nui={path='../absent'}\n",
    );
    p.run("architecture");
    assert_eq!(p.state("architecture"), "unknown");
    p.edit(
        "cargo/domain/Cargo.toml",
        "[package]\nname='domain'\nversion='0.1.0'\n",
    );
    p.run("architecture");
    assert_eq!(p.state("architecture"), "pass");
}

#[test]
fn rust_overrides_cannot_support_a_clean_declaration_graph() {
    let p = Project::new(&[
        ("cargo/Cargo.toml", "[workspace]\nmembers=['domain']\n[patch.crates-io]\nforeign={path='replacement'}\n"),
        ("cargo/domain/Cargo.toml", "[package]\nname='domain'\nversion='0.1.0'\n[dependencies]\nforeign='1'\n"),
    ], vec![rust_architecture("edges")], vec![criterion("architecture","architecture")]);
    p.run("architecture");
    assert_eq!(p.state("architecture"), "unknown");
}

#[test]
fn rust_root_manifest_closes_ignored_members_without_traversing_node_modules() {
    let mut c = rust_architecture("edges");
    c["verification"]["scope"] = json!(["Cargo.toml"]);
    c["verification"]["pack"]["root"] = json!(".");
    let p = Project::new(
        &[
            ("Cargo.toml", "[workspace]\nmembers=['crates/*']\n"),
            (
                "crates/domain/Cargo.toml",
                "[package]\nname='domain'\nversion='0.1.0'\n",
            ),
            (
                "crates/ui/Cargo.toml",
                "[package]\nname='ui'\nversion='0.1.0'\n",
            ),
        ],
        vec![c],
        vec![criterion("architecture", "architecture")],
    );
    // The graph declares an ignored member, while an unrelated ignored npm
    // directory has a symlink that cannot be copied as a closed source tree.
    p.edit(".gitignore", "crates/ui/\nnode_modules/\n");
    std::fs::create_dir_all(p.root.join("node_modules/.bin")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("/bin/echo", p.root.join("node_modules/.bin/tool")).unwrap();
    let first = p.run("architecture");
    assert_eq!(p.state("architecture"), "pass", "{}", p.receipt(&first));
    let receipt = p.receipt(&first);
    assert!(
        receipt["record"]["start"]["inputs"]["scope_manifest"]["files"]["crates/ui/Cargo.toml"]
            .is_object()
    );
    p.edit(
        "crates/domain/Cargo.toml",
        "[package]\nname='domain'\nversion='0.1.0'\n[dependencies]\nui={path='../ui'}\n",
    );
    assert_eq!(p.state("architecture"), "stale");
    p.run("architecture");
    assert_eq!(p.state("architecture"), "fail");
    // A newly appearing ignored wildcard member must join the input binding.
    p.edit(".gitignore", "crates/ui/\ncrates/hidden/\nnode_modules/\n");
    std::fs::create_dir_all(p.root.join("crates/hidden")).unwrap();
    p.edit(
        "crates/hidden/Cargo.toml",
        "[package]\nname='unmapped'\nversion='0.1.0'\n",
    );
    p.run("architecture");
    assert_eq!(
        p.state("architecture"),
        "unknown",
        "an ignored unmapped package must prevent a clean graph"
    );
    assert_eq!(
        p.receipt(&first)["receipt_digest"],
        receipt["receipt_digest"]
    );
}
fn resource() -> Value {
    let mut c = check(
        "memory",
        "lint",
        "thruline-python-memory-v3",
        json!(["python3"]),
        json!(["workload.py"]),
        json!({"kind":"resource","module":"workload.py","workload":"Controlled buffer touches, retained until each cycle completes","size":8388608,"size_unit":"bytes per cycle","warmup_cycles":1,"cycles":3,"repetitions":2,"sampled_peak_budget_bytes":134217728,"retained_growth_budget_bytes":33554432}),
    );
    // The sampled-memory workload is a finite two-repetition run; 20 s here was
    // a bound on how fast this machine finishes it, which is what made this
    // check time out (state "unknown") under a concurrent release build.
    c["timeout_seconds"] = json!(FIXTURE_LIVENESS_TIMEOUT_SECONDS);
    c
}
fn arch(kind: &str, policy: Value, source: &str, view: &str) -> Project {
    Project::new(
        &[
            ("src/core.py", source),
            ("src/port.py", "value=1\n"),
            ("src/view.py", view),
        ],
        vec![architecture(kind, policy)],
        vec![criterion("architecture", "architecture")],
    )
}
#[test]
fn boundary_finding_has_rule_line_receipt_and_fresh_repair() {
    let p = arch("edges", rules(), "import view\nvalue=1\n", "value=1\n");
    let run = p.run("architecture");
    let receipt = p.receipt(&run);
    assert_eq!(p.state("architecture"), "fail");
    assert!(receipt["verification"]["explanation"]
        .as_str()
        .unwrap()
        .contains("src/core.py:1"));
    assert!(receipt["verification"]["explanation"]
        .as_str()
        .unwrap()
        .contains("domain-no-ui"));
    assert_eq!(
        receipt["record"]["finish"]["pack"]["observations"][0]["observed"]["edges"][0]["target"],
        "view"
    );
    p.edit("src/core.py", "import port\nvalue=1\n");
    assert_eq!(p.state("architecture"), "stale");
    p.run("architecture");
    assert_eq!(p.state("architecture"), "pass");
    assert_eq!(p.receipt(&run)["receipt_digest"], receipt["receipt_digest"]);
}
#[test]
fn cycle_witness_and_exact_component_exemption() {
    let p = arch("cycles", rules(), "import view\n", "import core\n");
    let run = p.run("architecture");
    assert_eq!(p.state("architecture"), "fail");
    let r = p.receipt(&run);
    assert_eq!(
        r["record"]["finish"]["pack"]["observations"][0]["observed"]["cycles"][0]["path"],
        json!(["core", "view", "core"])
    );
    p.edit("src/view.py", "value=1\n");
    p.run("architecture");
    assert_eq!(p.state("architecture"), "pass");
    let mut policy = rules();
    policy["cycle_exemptions"] =
        json!([{"modules":["core","view"],"reason":"Explicit registration exception"}]);
    let p = arch("cycles", policy, "import view\n", "import core\n");
    p.run("architecture");
    assert_eq!(p.state("architecture"), "pass");
}
#[test]
fn wildcard_module_boundary_has_witness_and_fresh_repair() {
    let p = arch("edges", rules(), "from view import *\n", "value=1\n");
    let run = p.run("architecture");
    let receipt = p.receipt(&run);
    assert_eq!(p.state("architecture"), "fail", "{receipt}");
    let graph = &receipt["record"]["finish"]["pack"]["observations"][0]["observed"];
    assert!(graph["issues"].as_array().unwrap().is_empty(), "{graph}");
    assert_eq!(graph["edges"][0]["target"], "view");
    assert_eq!(graph["edges"][0]["line"], 1);
    assert_eq!(graph["edges"][0]["text"], "from view import *");
    p.edit("src/core.py", "from port import *\n");
    assert_eq!(p.state("architecture"), "stale");
    p.run("architecture");
    assert_eq!(p.state("architecture"), "pass");
    assert_eq!(p.receipt(&run)["receipt_digest"], receipt["receipt_digest"]);
}
#[test]
fn wildcard_module_cycle_uses_module_edges_without_symbol_resolution() {
    let p = arch("cycles", rules(), "from view import *\n", "import core\n");
    let run = p.run("architecture");
    let receipt = p.receipt(&run);
    assert_eq!(p.state("architecture"), "fail", "{receipt}");
    let graph = &receipt["record"]["finish"]["pack"]["observations"][0]["observed"];
    assert!(graph["issues"].as_array().unwrap().is_empty(), "{graph}");
    assert_eq!(graph["cycles"][0]["path"], json!(["core", "view", "core"]));
    p.edit("src/view.py", "__all__ = ['value']\nvalue=1\n");
    p.run("architecture");
    assert_eq!(p.state("architecture"), "pass");
}
#[test]
fn wildcard_declared_external_leaf_obeys_the_same_boundary_policy() {
    let mut policy = rules();
    policy["external_modules"] = json!(["external_view", "external_port"]);
    policy["layers"]["external_view"] = json!("presentation");
    policy["layers"]["external_port"] = json!("interface");
    for (kind, source, expected) in [
        ("edges", "from external_view import *\n", "fail"),
        ("edges", "from external_port import *\n", "pass"),
        ("cycles", "from external_port import *\n", "pass"),
    ] {
        let p = arch(kind, policy.clone(), source, "value=1\n");
        let run = p.run("architecture");
        let receipt = p.receipt(&run);
        assert_eq!(p.state("architecture"), expected, "{receipt}");
        let graph = &receipt["record"]["finish"]["pack"]["observations"][0]["observed"];
        assert!(graph["issues"].as_array().unwrap().is_empty(), "{graph}");
        assert_eq!(graph["edges"].as_array().unwrap().len(), 1);
    }
}
#[test]
fn wildcard_unresolved_dynamic_and_package_reexports_still_abstain() {
    for kind in ["edges", "cycles"] {
        for (source, view) in [
            ("from absent import *\n", "value=1\n"),
            (
                "from view import *\n",
                "def __getattr__(name):\n    raise AttributeError(name)\n",
            ),
            ("from view import *\n", "from absent import value\n"),
        ] {
            let p = arch(kind, rules(), source, view);
            let run = p.run("architecture");
            assert_eq!(p.state("architecture"), "unknown", "{}", p.receipt(&run));
        }
        let p = Project::new(
            &[
                ("src/core.py", "from port import *\n"),
                ("src/port/__init__.py", "__all__ = ['lazy']\n"),
                ("src/port/lazy.py", "import core\n"),
                ("src/view.py", "value=1\n"),
            ],
            vec![architecture(kind, rules())],
            vec![criterion("architecture", "architecture")],
        );
        let run = p.run("architecture");
        let receipt = p.receipt(&run);
        assert_eq!(p.state("architecture"), "unknown", "{receipt}");
        assert!(receipt["verification"]["explanation"]
            .as_str()
            .unwrap()
            .contains("package export closure"));
    }
}
#[test]
fn absent_policy_and_dynamic_ambiguous_unsupported_inputs_abstain() {
    for (source, policy) in [
        ("import view\n", Value::Null),
        ("__import__('view')\n", rules()),
        ("from absent import *\n", rules()),
        ("import absent\n", rules()),
        ("def broken(\n", rules()),
    ] {
        let p = arch("edges", policy, source, "value=1\n");
        p.run("architecture");
        assert_eq!(p.state("architecture"), "unknown");
    }
    let p = arch("edges", rules(), "import port\n", "value=1\n");
    p.edit("src/unsupported.ts", "import x from 'y'\n");
    p.run("architecture");
    assert_eq!(p.state("architecture"), "unknown");
}
#[test]
fn ignored_directory_inputs_are_examined_and_invalidate() {
    let p = arch("edges", rules(), "import port\n", "value=1\n");
    p.edit(".gitignore", "src/core.py\n__pycache__/\n");
    let run = p.run("architecture");
    assert_eq!(p.state("architecture"), "pass");
    p.edit("src/core.py", "import view\n");
    assert_eq!(p.state("architecture"), "stale");
    p.run("architecture");
    assert_eq!(p.state("architecture"), "fail");
    assert_eq!(p.receipt(&run)["verification"]["state"], "pass");
}
#[test]
fn nested_relative_imports_and_package_initializers_are_edges() {
    let policy = json!({"id":"nested","layers":{"pkg":"base","pkg.domain":"core","pkg.ui":"ui"},"external_modules":[],"forbidden":[{"id":"no-ui","from":"core","to":"ui"}],"forbid_cycles":true,"cycle_exemptions":[]});
    let p = Project::new(
        &[
            ("src/pkg/__init__.py", ""),
            ("src/pkg/domain.py", "from . import ui\n"),
            ("src/pkg/ui.py", "value=1\n"),
        ],
        vec![architecture("edges", policy)],
        vec![criterion("architecture", "architecture")],
    );
    p.run("architecture");
    assert_eq!(p.state("architecture"), "fail");
}
#[test]
fn missing_raw_architecture_evidence_cannot_keep_a_pass() {
    let p = arch("edges", rules(), "import port\n", "value=1\n");
    let run = p.run("architecture");
    let r = p.receipt(&run);
    let digest = r["record"]["finish"]["outputs"][0]["digest"]
        .as_str()
        .unwrap()
        .trim_start_matches("sha256:");
    fn remove(root: &std::path::Path, name: &str) {
        for e in std::fs::read_dir(root).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                remove(&p, name)
            } else if p.file_name().unwrap() == name {
                std::fs::remove_file(p).unwrap();
            }
        }
    }
    remove(p.temp.path(), &format!("{digest}.out"));
    assert_eq!(p.state("architecture"), "unknown");
}
const CONTROL:&str="import time\ndef run_cycle(size):\n    data=bytearray(size)\n    for i in range(0,size,4096): data[i]=1\n    time.sleep(.1)\n";
#[test]
fn memory_budgets_are_measured_and_repaired_without_claiming_isolation() {
    let p = Project::new(
        &[("workload.py", CONTROL)],
        vec![resource()],
        vec![criterion("memory", "resource")],
    );
    let run = p.run("memory");
    assert_eq!(p.state("memory"), "pass");
    let r = p.receipt(&run);
    let o = &r["record"]["finish"]["pack"]["observations"][0]["observed"];
    assert!(o["enforced_ram_limit_bytes"].is_null());
    assert_eq!(o["repetitions"].as_array().unwrap().len(), 2);
    p.edit("workload.py","import time\nretained=[]\ndef run_cycle(size):\n    data=bytearray(size*4)\n    for i in range(0,len(data),4096): data[i]=1\n    retained.append(data)\n    time.sleep(.1)\n");
    assert_eq!(p.state("memory"), "stale");
    p.run("memory");
    assert_eq!(p.state("memory"), "fail");
    p.edit("workload.py", CONTROL);
    p.run("memory");
    assert_eq!(p.state("memory"), "pass");
    assert_eq!(p.receipt(&run)["receipt_digest"], r["receipt_digest"]);
}
#[test]
fn resource_exception_and_timeout_preserve_incomplete_evidence() {
    for code in [
        "def run_cycle(size): raise RuntimeError('cannot complete')\n",
        "import time\ndef run_cycle(size): time.sleep(5)\n",
    ] {
        let mut c = resource();
        c["timeout_seconds"] = json!(1);
        let p = Project::new(
            &[("workload.py", code)],
            vec![c],
            vec![criterion("memory", "resource")],
        );
        p.run("memory");
        assert_eq!(p.state("memory"), "unknown");
    }
}

/// Every RSS sample carries how long its own `/bin/ps` call took, all the way
/// into the served observation, so a slow machine is a fact in the record
/// instead of a missing measurement. VERIFICATION-MEMORY-TIMEOUT: the sampler's
/// bound used to be 0.15 s of wall clock, and a loaded machine turned this
/// check's verdict into "unknown". `sample_ms` is a measurement field only —
/// it is read by no verdict, which is why the verdict is asserted here beside
/// it and is the same one this workload has always produced.
#[test]
fn every_memory_sample_records_its_own_duration_without_touching_the_verdict() {
    let p = Project::new(
        &[("workload.py", CONTROL)],
        vec![resource()],
        vec![criterion("memory", "resource")],
    );
    let run = p.run("memory");
    assert_eq!(p.state("memory"), "pass");
    let r = p.receipt(&run);
    let o = &r["record"]["finish"]["pack"]["observations"][0]["observed"];
    assert_eq!(o["sample_liveness_bound_ms"].as_u64(), Some(5000), "{o}");
    let reps = o["repetitions"].as_array().unwrap();
    assert_eq!(reps.len(), 2);
    for rep in reps {
        let rows: Vec<_> = rep["samples"]
            .as_array()
            .unwrap()
            .iter()
            .chain(rep["phases"].as_array().unwrap())
            .collect();
        assert!(!rows.is_empty());
        for row in rows {
            assert!(
                row["sample_ms"].as_u64().is_some(),
                "a sample without its own duration: {row}"
            );
        }
    }
}

#[test]
fn process_tree_includes_child_memory_and_ignores_success_narration() {
    let mut c = resource();
    c["verification"]["pack"]["sampled_peak_budget_bytes"] = json!(50331648);
    let code="import subprocess,sys\ndef run_cycle(size):\n    print('success: all resource budgets passed')\n    subprocess.run([sys.executable,'-I','-B','-c','import time; data=bytearray(67108864); time.sleep(.3)'],check=True)\n";
    let p = Project::new(
        &[("workload.py", code)],
        vec![c],
        vec![criterion("memory", "resource")],
    );
    let run = p.run("memory");
    let r = p.receipt(&run);
    assert_eq!(p.state("memory"), "fail", "{r}");
    let reps = r["record"]["finish"]["pack"]["observations"][0]["observed"]["repetitions"]
        .as_array()
        .unwrap();
    assert!(reps.iter().all(|r| r["samples"]
        .as_array()
        .unwrap()
        .iter()
        .any(|s| s["processes"].as_array().unwrap().len() > 1)));
}
