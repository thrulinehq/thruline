//! Guided recipes reuse the accepted-contract chain and content-addressed EVB artifacts.
//! Browser requests select known recipes; they cannot supply argv, roots, or tool paths.
#[path = "megalinter_selection.rs"]
mod megalinter_selection;
#[path = "mutation_selection.rs"]
mod mutation_selection;
#[path = "schemathesis_mode.rs"]
mod schemathesis_mode;
use super::*;
use thruline_ledger::{acceptance::Revision, Ledger as AcceptedLedger, RepoId};
use thruline_policy::acceptance::{self as policy, Contract, Document};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceRef {
    pub store: PathBuf,
    pub revision_hash: String,
    pub check_id: String,
    pub plan_digest: String,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Runtime {
    version: String,
    executable: PathBuf,
    sha256: String,
    runtime: Vec<PathBuf>,
    prefix: Vec<String>,
    #[serde(default)]
    container: Option<container::Spec>,
    #[serde(default)]
    model_manifest: Option<PathBuf>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    schema: String,
    tools: BTreeMap<String, Runtime>,
    #[serde(default)]
    asset_root: Option<PathBuf>,
}
const PACKS: [&str; 22] = [
    "ast-grep",
    "cargo-deny",
    "dependency-cruiser",
    "knip",
    "betterleaks",
    "cargo-mutants",
    "playwright",
    "schemathesis",
    "semgrep",
    "osv-scanner",
    "ruff",
    "opengrep",
    "infer",
    "aider",
    "openhands-sdk",
    "pr-agent",
    "mini-swe-agent",
    "clippy",
    "joern",
    "openrewrite",
    "sonarqube",
    "megalinter",
];

fn knip_selection_args(value: &Value) -> Result<Vec<String>> {
    let object = value
        .as_object()
        .context("Knip selection must be an object")?;
    anyhow::ensure!(
        value["schema"] == "thruline.knip-selection.v1",
        "unsupported Knip selection schema"
    );
    anyhow::ensure!(
        object
            .keys()
            .all(|key| ["schema", "workspaces", "strict", "production"].contains(&key.as_str())),
        "unknown Knip selection field"
    );
    for key in ["strict", "production"] {
        anyhow::ensure!(
            value.get(key).is_none_or(Value::is_boolean),
            "Knip mode flags must be boolean"
        );
    }
    anyhow::ensure!(
        !(value["strict"] == true && value["production"] == false),
        "Native Knip strict mode implies production; conflicting selection is unsupported"
    );
    let mut args = vec![];
    if let Some(workspaces) = value.get("workspaces") {
        let workspaces = workspaces
            .as_array()
            .context("Knip workspaces must be an array")?;
        anyhow::ensure!(
            !workspaces.is_empty() && workspaces.len() <= 32,
            "select 1-32 native workspace selectors"
        );
        let mut seen = std::collections::BTreeSet::new();
        for workspace in workspaces {
            let workspace = workspace
                .as_str()
                .context("invalid Knip workspace selector")?;
            anyhow::ensure!(
                !workspace.is_empty()
                    && workspace.len() <= 256
                    && !workspace.starts_with('-')
                    && !workspace.chars().any(char::is_control)
                    && seen.insert(workspace),
                "invalid or duplicate Knip workspace selector"
            );
            args.extend(["--workspace".into(), workspace.into()]);
        }
    }
    if value["strict"] == true {
        args.push("--strict".into());
    }
    if value["production"] == true {
        args.push("--production".into());
    }
    Ok(args)
}
fn registry_path() -> Result<PathBuf> {
    if let Some(p) = std::env::var_os("THRULINE_INTEGRATION_REGISTRY") {
        return Ok(p.into());
    }
    let exe = std::env::current_exe()?;
    Ok(exe
        .parent()
        .and_then(Path::parent)
        .context("runtime directory unavailable")?
        .join("integration/runtimes/registry.json"))
}
fn runtime(pack: &str) -> Result<(Runtime, PathBuf)> {
    // AI-REVIEW-OUT-OF-LAUNCH: a registered PR-Agent runtime is not this
    // build's to use. Every plan, run, repair and the review availability
    // probe reach a runtime through here, so none of them reaches PR-Agent.
    anyhow::ensure!(!super::withheld(pack), "{}", super::AI_REVIEW_NOT_IN_BUILD);
    let path = registry_path()?;
    let mut registry: Registry = serde_json::from_value(engine::parse_json(&bounded_file(&path, 65536).context("Tools are not provisioned. Run scripts/integrations/provision.py to register the pinned local runtimes.")?)?).map_err(|e| super::shape_error("Invalid tool registry", e))?;
    anyhow::ensure!(
        registry.schema == "thruline.integration-runtimes.v1",
        "unsupported runtime registry"
    );
    let r = registry.tools.remove(pack).context("This tool has not been provisioned. Register its pinned runtime before running this check.")?;
    let version = match pack {
        "ast-grep" => "0.45.3",
        "cargo-deny" => "0.20.2",
        "dependency-cruiser" => "18.2.0",
        "knip" => "6.35.0",
        "betterleaks" => "95237cf8eb4d8e9f67409595b245e674832992cf",
        "cargo-mutants" => "27.1.0",
        "playwright" => "1.63.0",
        "schemathesis" => "4.26.1",
        "semgrep" => "1.176.0",
        "osv-scanner" => "2.5.1",
        "ruff" => "0.16.7",
        "opengrep" => "1.30.0",
        "infer" => "1.3.0",
        "aider" => "0.86.0",
        "openhands-sdk" => "1.47.0",
        "pr-agent" => "0.45.0",
        "mini-swe-agent" => "2.4.6",
        "clippy" => "0.1.97",
        "joern" => "4.0.626",
        "openrewrite" => "8.90.4",
        "sonarqube" => "26.9.0.129388",
        "megalinter" => "10.1.0",
        _ => bail!("No guided recipe for this source"),
    };
    anyhow::ensure!(
        r.version == version,
        "Unsupported runtime version. This recipe has only been exercised with {version}; provision that version or qualify an update first."
    );
    anyhow::ensure!(
        r.executable.is_absolute() && hash_file(&r.executable)? == r.sha256,
        "The installed executable changed. Re-provision and review the new runtime before executing."
    );
    if let Some(spec) = &r.container {
        container::identity(spec)?;
    }
    Ok((r, path))
}
fn runtime_root() -> Result<PathBuf> {
    let path = registry_path()?;
    let registry: Registry =
        serde_json::from_value(engine::parse_json(&bounded_file(&path, 65536)?)?)
            .map_err(|e| super::shape_error("Invalid tool registry", e))?;
    if let Some(root) = registry.asset_root {
        anyhow::ensure!(
            root.is_absolute() && root.is_dir(),
            "runtime asset root is unavailable"
        );
        Ok(root)
    } else {
        Ok(path
            .parent()
            .context("runtime registry parent missing")?
            .to_path_buf())
    }
}
pub(super) fn runtime_configuration_identity(pack: &str) -> Result<String> {
    hash_json(&runtime(pack)?.0)
}
/// The prepared model manifest the pack's registered runtime names: the
/// `--model-provenance` its argv prefix carries (the one the plan inherits),
/// else the registry's `model_manifest`.
pub(super) fn runtime_model_provenance(pack: &str) -> Result<PathBuf> {
    let (r, _) = runtime(pack)?;
    r.prefix
        .iter()
        .position(|a| a == "--model-provenance")
        .and_then(|i| r.prefix.get(i + 1))
        .map(PathBuf::from)
        .or(r.model_manifest)
        .context("The registered runtime names no prepared model manifest")
}
pub(super) fn native_repair_runtime(pack: &str) -> Result<Value> {
    serde_json::to_value(runtime(pack)?.0).map_err(Into::into)
}
fn history(root: &Path, store: &Path) -> Result<Vec<Revision>> {
    if !store.exists() {
        return Ok(vec![]);
    }
    Ok(AcceptedLedger::open_read_only(store)?
        .acceptance_history(&RepoId::from_key(&root.to_string_lossy()))?)
}

fn validate_pr_agent_plain_diff(value: &Value, scope: &Value) -> Result<()> {
    let object = value
        .as_object()
        .context("PR-Agent plain-diff options must be an object")?;
    anyhow::ensure!(
        object.len() == 3
            && object
                .keys()
                .all(|key| ["schema", "mode", "diff_file"].contains(&key.as_str()))
            && value["schema"] == "thruline.pr-agent-plain-diff.v1"
            && value["mode"] == "plain-diff",
        "Select the versioned plain-diff provider with one diff_file"
    );
    let name = value["diff_file"]
        .as_str()
        .context("Missing unified diff path")?;
    anyhow::ensure!(
        !name.is_empty()
            && !name.contains('\0')
            && !Path::new(name).is_absolute()
            && name
                .split('/')
                .all(|part| !["", ".", "..", ".git"].contains(&part))
            && scope["files"] == json!([name])
            && scope.get("base_ref").is_none()
            && scope.get("edit_files").is_none(),
        "Plain-diff captures only its named patch, without committed history or editable context"
    );
    Ok(())
}

fn validate_aider_feedback(feedback: &Value, scope: &Value) -> Result<()> {
    let object = feedback.as_object().context("feedback must be an object")?;
    anyhow::ensure!(
        object.len() == 4
            && ["schema", "mode", "test_file", "command"]
                .iter()
                .all(|key| object.contains_key(*key)),
        "feedback requires only schema, mode, test_file and command"
    );
    anyhow::ensure!(
        feedback["schema"] == "thruline.aider-feedback.v1"
            && matches!(feedback["mode"].as_str(), Some("lint-and-test" | "test")),
        "Unsupported native feedback contract or mode"
    );
    let test = feedback["test_file"]
        .as_str()
        .context("feedback test_file must be a string")?;
    anyhow::ensure!(
        !test.is_empty()
            && test.len() <= 4096
            && !test.contains(['\\', '\0', '\n', '\r'])
            && test
                .split('/')
                .all(|part| !["", ".", "..", ".git"].contains(&part)),
        "Feedback test must remain within selected source"
    );
    let selected = scope["files"]
        .as_array()
        .context("feedback needs selected files")?;
    let editable = scope["edit_files"]
        .as_array()
        .context("feedback requires explicit edit_files")?;
    anyhow::ensure!(
        selected.contains(&Value::String(test.into()))
            && !editable.contains(&Value::String(test.into())),
        "Feedback test must be selected read-only context outside edit_files"
    );
    let command = feedback["command"]
        .as_array()
        .context("feedback command must be an argv list")?;
    anyhow::ensure!(
        (2..=16).contains(&command.len())
            && command
                .iter()
                .all(|arg| arg.as_str().is_some_and(|arg| !arg.is_empty()
                    && arg.len() <= 4096
                    && !arg.contains(['\0', '\n', '\r']))),
        "Feedback command arguments exceed bounds"
    );
    let command: Vec<&str> = command
        .iter()
        .map(|arg| arg.as_str().unwrap_or_default())
        .collect();
    anyhow::ensure!(
        ["python", "python3"].contains(&command[0])
            && command.iter().filter(|arg| **arg == "{feedback}").count() == 1,
        "Feedback requires the pinned Python runtime and one {{feedback}} argument"
    );
    let script = command
        .iter()
        .position(|arg| *arg == "{feedback}")
        .context("missing selected feedback script")?;
    anyhow::ensure!(
        command[1..script]
            .iter()
            .all(|flag| ["-B", "-I", "-u"].contains(flag)),
        "Feedback interpreter options may not replace the selected script"
    );
    anyhow::ensure!(
        command[script + 1..]
            .iter()
            .all(|arg| !arg.contains(['{', '}']) || *arg == "{workspace}"),
        "Unknown feedback command substitution"
    );
    Ok(())
}

fn validate_aider_mode(value: &Value, scope: &Value) -> Result<()> {
    let object = value.as_object().context("Aider mode must be an object")?;
    anyhow::ensure!(
        object.len() == 4
            && ["schema", "mode", "edit_format", "max_reflections"]
                .iter()
                .all(|key| object.contains_key(*key)),
        "Aider mode requires only schema, mode, edit_format and max_reflections"
    );
    anyhow::ensure!(
        value["schema"] == "thruline.aider-mode.v1",
        "Unsupported Aider mode contract"
    );
    let mode = value["mode"]
        .as_str()
        .context("Aider mode must be a string")?;
    anyhow::ensure!(
        ["ask", "architect", "edit"].contains(&mode),
        "Unsupported native Aider mode"
    );
    anyhow::ensure!(
        value["max_reflections"].as_u64().is_some_and(|n| n <= 6),
        "Native Aider reflections must be an integer from 0 to 6"
    );
    if mode == "ask" {
        anyhow::ensure!(
            value["edit_format"].is_null()
                && scope.get("edit_files").is_none()
                && scope.get("feedback").is_none(),
            "Ask mode has read-only context and accepts no edit format, editable scope or repair feedback"
        );
    } else {
        let format = value["edit_format"]
            .as_str()
            .context("Select an explicit native edit format")?;
        anyhow::ensure!(
            [
                "diff",
                "diff-fenced",
                "whole",
                "patch",
                "udiff",
                "udiff-simple",
                "editor-diff",
                "editor-whole",
                "editor-diff-fenced"
            ]
            .contains(&format),
            "Unsupported native Aider edit format"
        );
    }
    Ok(())
}

fn validate_mini_harness(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .context("mini-SWE options must be an object")?;
    let fields = [
        "schema",
        "mode",
        "environment",
        "instance_id",
        "command_timeout",
    ];
    anyhow::ensure!(
        object.len() == fields.len() && fields.iter().all(|key| object.contains_key(*key)),
        "mini-SWE options require exactly the declared fields"
    );
    anyhow::ensure!(
        value["schema"] == "thruline.mini-harness-options.v1"
            && value["mode"] == "owned-swebench-task"
            && value["environment"] == "local-in-linux",
        "Only an owned compatibility task in the Linux LocalEnvironment is configured"
    );
    let id = value["instance_id"]
        .as_str()
        .context("Owned task identifier must be text")?;
    anyhow::ensure!(
        !id.is_empty()
            && id.len() <= 80
            && id.as_bytes()[0].is_ascii_alphanumeric()
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
        "Owned task identifier must be a bounded path-free identifier"
    );
    anyhow::ensure!(
        value["command_timeout"]
            .as_u64()
            .is_some_and(|n| (1..=25).contains(&n)),
        "Native command timeout must be1–25 seconds"
    );
    Ok(())
}

fn validate_openhands_options(value: &Value) -> Result<()> {
    let object = value
        .as_object()
        .context("OpenHands options must be an object")?;
    anyhow::ensure!(
        object
            .keys()
            .all(|key| ["schema", "backend", "condenser"].contains(&key.as_str())),
        "Unknown OpenHands native option"
    );
    anyhow::ensure!(
        value["schema"] == "thruline.openhands-options.v1",
        "Unsupported OpenHands option contract"
    );
    anyhow::ensure!(
        value["backend"]
            .as_str()
            .is_some_and(|backend| ["local", "owned-server"].contains(&backend)),
        "Select local SDK or owned SDK server transport"
    );
    if !value["condenser"].is_null() {
        let condenser = value["condenser"]
            .as_object()
            .context("Condenser must be an object")?;
        anyhow::ensure!(
            condenser.len() == 3
                && ["max_size", "keep_first", "max_tokens"]
                    .iter()
                    .all(|key| condenser.contains_key(*key)),
            "Condenser requires exact max_size, keep_first and max_tokens fields"
        );
        let maximum = value["condenser"]["max_size"]
            .as_u64()
            .context("Invalid condenser max_size")?;
        anyhow::ensure!(
            (8..=240).contains(&maximum),
            "Condenser max_size must be 8-240"
        );
        let first = value["condenser"]["keep_first"]
            .as_u64()
            .context("Invalid condenser keep_first")?;
        anyhow::ensure!(
            first < maximum / 2 - 1,
            "Condenser must preserve a native forgetting range"
        );
        let tokens = &value["condenser"]["max_tokens"];
        anyhow::ensure!(
            tokens.is_null() || tokens.as_u64().is_some_and(|n| (2048..=16384).contains(&n)),
            "Invalid condenser token bound"
        );
    }
    Ok(())
}

pub fn requirement_identity(plan: &Plan) -> Result<String> {
    match &plan.acceptance {
        None => hash_json(&plan.criterion),
        Some(a) => {
            let rows = history(&plan.root, &a.store)?;
            hash_json(&(plan.criterion.clone(), rows.last().map(|r| &r.hash)))
        }
    }
}
fn setup_identity(plan: &Plan) -> Result<String> {
    hash_json(&(
        runtime_configuration_identity(&plan.pack)?,
        match &plan.container {
            Some(spec) => container::identity(spec)?,
            None => hash_file(Path::new(&plan.argv[0]))?,
        },
        input_read::hash_runtime(&plan.root, &plan.runtime)?,
        if semgrep_mode::is_plan(plan) {
            semgrep_mode::rules_identity(plan)?
        } else {
            hash_files(&plan.root, &plan.rules)?
        },
        hash_files(&plan.root, &plan.dataset)?,
        &plan.environment,
    ))
}
fn database_note(plan: &Plan) -> Option<String> {
    if plan.pack != "osv-scanner" {
        return None;
    }
    let path = plan
        .dataset
        .iter()
        .find(|p| p.file_name().is_some_and(|n| n == "provenance.json"))?;
    let value = engine::parse_json(&bounded_file(path, 65536).ok()?).ok()?;
    Some(format!(
        "Advisory snapshot downloaded {}. Scope: {}. A clear result only means no match in this snapshot; refreshing the database requires a new review and run.",
        value.get("downloaded_at")?.as_str()?,
        value.get("scope")?.as_str()?
    ))
}

fn validate_npm_advisory_scope(lock: &Value, provenance: &Value) -> Result<()> {
    anyhow::ensure!(
        provenance["schema"] == "thruline.osv-npm-subset.v1",
        "The npm snapshot needs explicit package/version coverage; an unbounded subset cannot establish a clear result"
    );
    anyhow::ensure!(
        matches!(lock["lockfileVersion"].as_u64(), Some(2 | 3)),
        "The npm advisory recipe needs package-lock version 2 or 3"
    );
    let packages = lock["packages"]
        .as_object()
        .context("Lockfile package inventory missing")?;
    let covered = provenance["package_versions"]
        .as_object()
        .context("Snapshot package/version coverage missing")?;
    anyhow::ensure!(
        (2..=17).contains(&packages.len()) && packages.contains_key(""),
        "Select a nonempty flat npm dependency graph with at most 16 packages"
    );
    for (path, package) in packages.iter().filter(|(name, _)| !name.is_empty()) {
        let name = path
            .strip_prefix("node_modules/")
            .context("Unsupported npm package path")?;
        anyhow::ensure!(
            !name.is_empty()
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
                && ![".", ".."].contains(&name),
            "Scoped, linked and nested npm dependencies need separate advisory coverage"
        );
        let version = package["version"]
            .as_str()
            .context("Locked npm version missing")?;
        anyhow::ensure!(
            package.get("link").is_none()
                && package["resolved"]
                    == format!("https://registry.npmjs.org/{name}/-/{name}-{version}.tgz")
                && package["integrity"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("sha512-")),
            "Only exact registry npm releases with recorded integrity are covered"
        );
        anyhow::ensure!(
            covered
                .get(name)
                .and_then(Value::as_array)
                .is_some_and(|versions| versions.contains(&Value::String(version.into()))),
            "The dated npm subset does not cover {name}@{version}; this is unavailable coverage, not a clear result"
        );
    }
    Ok(())
}
pub fn check_acceptance(plan: &Plan) -> Result<()> {
    if let Some(expected) = &plan.approved_setup {
        anyhow::ensure!(
            *expected == setup_identity(plan)?,
            "The reviewed runtime or rule configuration changed. Review and accept the updated check before running it."
        );
    }
    if let Some(a) = &plan.acceptance {
        let rows = history(&plan.root, &a.store)?;
        let row = rows
            .last()
            .context("The accepted requirement is missing; no check was started.")?;
        anyhow::ensure!(
            row.hash == a.revision_hash,
            "The accepted requirement changed. Review its current check before running again."
        );
        let doc = Document::parse(row.document.as_bytes())?;
        let check = doc
            .contract()
            .checks
            .iter()
            .find(|c| c.id == a.check_id)
            .context("accepted check missing")?;
        anyhow::ensure!(
            check.runner == format!("integration:{}", a.plan_digest),
            "accepted plan identity does not match"
        );
        let mut original = plan.clone();
        original.acceptance = None;
        for (expected, actual) in check.argv.iter().zip(&mut original.argv) {
            if expected == "THRULINE_RUN_OUTPUT" && plan.pack == "cargo-mutants" {
                let output = Path::new(actual);
                anyhow::ensure!(
                    output.is_absolute()
                        && output
                            .file_name()
                            .is_some_and(|n| n.to_string_lossy().starts_with("run-output-")),
                    "invalid generated output directory"
                );
                anyhow::ensure!(
                    matches!(&plan.report, ReportSource::File{path} if *path == output.join("mutants.out/outcomes.json")),
                    "report does not belong to generated output"
                );
                *actual = expected.clone();
                original.report = ReportSource::File {
                    path: PathBuf::from("THRULINE_RUN_OUTPUT/mutants.out/outcomes.json"),
                };
            }
        }
        anyhow::ensure!(
            hash_json(&original)? == a.plan_digest,
            "plan differs from the accepted requirement, condition, or execution policy"
        );
    }
    Ok(())
}
fn needed(root: &Path, candidates: &[&str], message: &str) -> Result<PathBuf> {
    candidates
        .iter()
        .map(|p| root.join(p))
        .find(|p| p.is_file())
        .with_context(|| message.to_string())
}
fn recipe(db: &Path, root: &Path, pack: &str) -> Result<Plan> {
    let root = root.canonicalize()?;
    let (mut r, _) = runtime(pack)?;
    let model_manifest = r.model_manifest.clone();
    let mut argv = if r.container.is_some() {
        vec![]
    } else {
        vec![r.executable.display().to_string()]
    };
    argv.extend(r.prefix);
    // Bind only the selected tool's registry entry. Changing a model or an
    // unrelated runtime must not stale independent deterministic observations.
    let mut rules = vec![];
    let mut dependencies = vec![];
    for path in [
        "Cargo.lock",
        "package-lock.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "package.json",
        "Cargo.toml",
    ] {
        if root.join(path).is_file() {
            dependencies.push(root.join(path));
        }
    }
    let mut environment = BTreeMap::new();
    for key in ["PATH", "HOME", "TMPDIR", "RUSTUP_HOME", "CARGO_HOME"] {
        if let Ok(v) = std::env::var(key) {
            environment.insert(key.into(), v);
        }
    }
    environment.insert("CARGO_NET_OFFLINE".into(), "true".into());
    let mut report = ReportSource::Stdout;
    let mut dataset = vec![];
    let mut condition = engine::Condition::NoObservations;
    let advisory = ["aider", "openhands-sdk", "pr-agent", "mini-swe-agent"].contains(&pack);
    // The review provider has its own registered endpoint.  Keeping this in
    // the plan makes consent to that one destination part of plan identity.
    let mut local_model_port = if pack == "pr-agent" {
        Some(11436)
    } else {
        advisory.then_some(11435)
    };
    let mut dependency_modes_enabled = false;
    let mut osv_modes_enabled = false;
    let (requirement, success, args): (&str, &str, Vec<String>) = match pack {
        "megalinter" => {
            let cfg = needed(
                &root,
                &[".thruline/megalinter.json"],
                "Select explicit linters, flavor, exclusions, limits and local commands in .thruline/megalinter.json v2, or the separate fixed Ruff/yamllint Advisor selection in v1.",
            )?;
            let value = engine::parse_json(&bounded_file(&cfg, 32768)?)?;
            let controlled = value["schema"] == "thruline.megalinter-selection.v2";
            let advisor = if controlled {
                for name in megalinter_selection::files(&value)? {
                    rules.push(needed(&root, &[&name], "The selected local plugin or command script must be captured in this project.")?);
                }
                false
            } else {
                anyhow::ensure!(
                    value["schema"] == "thruline.megalinter-selection.v1"
                        && value.as_object().is_some_and(|o| o.len() == 2),
                    "unknown MegaLinter selection contract"
                );
                value["advisor"]
                    .as_bool()
                    .context("explicit advisor boolean required")?
            };
            rules.push(cfg.clone());
            let mut args = vec![
                "--source".into(),
                root.display().to_string(),
                "--output-base".into(),
                artifact_dir(db).join("megalinter").display().to_string(),
            ];
            if controlled {
                args.extend(["--selection-file".into(), cfg.display().to_string()]);
            }
            if advisor {
                let model = model_manifest.context(
                    "Prepare and register the pinned local Advisor model manifest first",
                )?;
                args.extend([
                    "--advisor".into(),
                    "--model-provenance".into(),
                    model.display().to_string(),
                ]);
                rules.push(model);
                local_model_port = Some(11435);
            }
            (
                if controlled {
                    "Explicitly requested native analyzers and commands must complete their selected source scope without diagnostics."
                } else {
                    "Python and YAML inputs must satisfy the two explicitly selected native analyzer policies."
                },
                if controlled {
                    "Requested native analyzers must execute on nonempty applicable scope. Failed commands, missing flavor applicability, timeouts or missing artifacts remain incomplete. Local plugin reports are not independent acceptance checks."
                } else {
                    "Ruff F401/F841 and yamllint complete their disclosed nonempty scopes with no diagnostics. Optional local Advisor output remains separate from deterministic results and independent verification."
                },
                args,
            )
        }
        "sonarqube" => {
            rules.push(needed(&root, &[".thruline/sonarqube.json"], "Select the explicit source files and isolated Sonar project key in .thruline/sonarqube.json. Prepare the pinned local service and scanner first.")?);
            (
                "Import a completed SonarQube Community analysis for the exact selected source revision.",
                "The native scanner and asynchronous analysis finish, all issue pages are retained, and no unresolved source issue is reported. A server quality gate is a separate observation.",
                vec![artifact_dir(db).join("sonarqube").display().to_string()],
            )
        }
        "clippy" => {
            needed(
                &root,
                &["Cargo.lock"],
                "Prepare the reviewed lockfile and cached Rust dependencies first.",
            )?;
            rules.push(needed(&root, &[".thruline/clippy.json"], "Select Rust features, targets and either the v1 needless_return lint or explicit v2 MSRV-sensitive unnecessary_map_or policy in .thruline/clippy.json.")?);
            (
                "The selected Rust targets and feature configuration must satisfy the named Clippy lint.",
                "Compilation completes for the explicitly selected configuration, without a named lint diagnostic. Uncompiled cfg branches and targets stay outside this result.",
                vec![artifact_dir(db).join("clippy").display().to_string()],
            )
        }
        "joern" => {
            let selection = needed(&root, &[".thruline/joern.json"], "Select explicit sources and the owned C/Python query, binary method structure, or frontend availability in .thruline/joern.json.")?;
            let configuration = engine::parse_json(&bounded_file(&selection, 65536)?)?;
            let predefined = configuration["schema"] == "thruline.joern-predefined-c.v1";
            rules.push(selection);
            if configuration["schema"] == "thruline.joern-frontend.v1"
                || configuration["schema"] == "thruline.joern-frontend-availability.v1"
            {
                let (criterion, success) = if configuration["schema"]
                    == "thruline.joern-frontend-availability.v1"
                {
                    ("The requested frontend needs an implemented native query before source evidence can be assessed.", "Installed frontend/runtime/license inventory remains visible. An availability-only request executes no frontend and stays incomplete.")
                } else if configuration["frontend"] == "ghidra2cpg" {
                    ("The selected native binary frontend must create a concrete graph for the accepted method in one object.", "Native parse and owned method-structure query finish without parser/unknown-syntax gaps. This establishes selected binary graph availability only, not defect absence, source equivalence or linked-library analysis.")
                } else {
                    ("The selected Python method must have no native structural call named str under the accepted query.", "The pinned Python frontend accounts for all selected/discovered Python files and executes the owned bounded query without parse warnings or unknown syntax. Call names do not prove builtin binding, return types or independent consumer behavior.")
                };
                (
                    criterion,
                    success,
                    vec![artifact_dir(db).join("joern").display().to_string()],
                )
            } else if predefined {
                (
                "The selected C source must have no finding from the accepted native predefined nonliteral-format query.",
                "The pinned native query inventory and ScanPass finish for all selected C files without parser gaps or native findings. Constant format pointers can produce heuristic false positives; this does not establish exploitability or all-query coverage.",
                vec![artifact_dir(db).join("joern").display().to_string()],
            )
            } else {
                (
                "The selected C source must have no modeled path between the accepted source and sink.",
                "Graph creation and the owned query finish with selected-file coverage, nonempty source/sink selection and no paths. Unresolved methods and omitted inputs remain incomplete.",
                vec![artifact_dir(db).join("joern").display().to_string()],
            )
            }
        }
        "openrewrite" => {
            let selection = needed(
                &root,
                &[".thruline/openrewrite.json"],
                "Select the named Apache Java or Properties recipe, or request explicit module availability, in .thruline/openrewrite.json.",
            )?;
            let configuration = engine::parse_json(&bounded_file(&selection, 65536)?)?;
            rules.push(selection);
            let (criterion, success) = match configuration["schema"].as_str() {
                Some("thruline.openrewrite-java.v1") => (
                    "The selected Java modules must have reached the accepted method-name migration.",
                    "The native recipe parses and compiles selected Java sources and produces no remaining migration patch. Proposed changes still require independent behavior checks.",
                ),
                Some("thruline.openrewrite-properties.v1") => (
                    "The selected Properties files must have reached the accepted exact-key value migration.",
                    "The native Properties parser accounts for selected and discovered Properties files, the accepted key is present with an old or target value, and the exact-value recipe reaches a fixed point with no remaining patch. This does not compile Java or prove consumer behavior; proposed changes require independent protected checks before apply.",
                ),
                Some("thruline.openrewrite-module.v1") => (
                    "The requested acquired module needs an implemented, available native parser and recipe before its criterion can be assessed.",
                    "Module and license availability must be explicit. An unimplemented or restricted module request remains incomplete and executes no native recipe; root Apache terms do not replace a module's own license.",
                ),
                _ => anyhow::bail!("Unknown OpenRewrite source/module selection contract"),
            };
            (
                criterion,
                success,
                vec![artifact_dir(db).join("openrewrite").display().to_string()],
            )
        }
        "aider" | "openhands-sdk" | "pr-agent" | "mini-swe-agent" => {
            needed(
                &root,
                &[".thruline/review-task.txt"],
                "Write the review task in .thruline/review-task.txt and explicitly list selected files in .thruline/agent-scope.json; the local model is optional advisory work.",
            )?;
            let scope_path = root.join(".thruline/agent-scope.json");
            let scope = engine::parse_json(&bounded_file(&scope_path, 32768)?)?;
            anyhow::ensure!(
                scope["schema"] == "thruline.agent-scope.v1",
                "unsupported agent scope contract"
            );
            let files = scope["files"]
                .as_array()
                .context("agent scope must name selected files")?;
            anyhow::ensure!(
                !files.is_empty() && files.len() <= 128,
                "select 1-128 files for local agent context"
            );
            rules.extend([scope_path, root.join(".thruline/review-task.txt")]);
            condition = engine::Condition::Advisory;
            let mut args = vec![
                "--source".into(),
                root.display().to_string(),
                "--output-base".into(),
                artifact_dir(db).join("local-agents").display().to_string(),
                "--task-file".into(),
                root.join(".thruline/review-task.txt").display().to_string(),
            ];
            if let Some(options) = scope.get("pr_agent") {
                anyhow::ensure!(
                    pack == "pr-agent",
                    "Plain-diff provider belongs to PR-Agent only"
                );
                validate_pr_agent_plain_diff(options, &scope)?;
                let diff_path =
                    root.join(options["diff_file"].as_str().context("Missing diff_file")?);
                let bytes = bounded_file(&diff_path, 1024 * 1024)?;
                anyhow::ensure!(
                    !std::str::from_utf8(&bytes)?.trim().is_empty(),
                    "Unified diff must be nonempty UTF-8 text"
                );
                rules.push(diff_path);
                args.extend(["--pr-agent-json".into(), serde_json::to_string(options)?]);
            } else if pack == "pr-agent" {
                args.extend([
                    "--base-ref".into(),
                    scope["base_ref"]
                        .as_str()
                        .context("PR-Agent requires an explicit committed base_ref")?
                        .into(),
                ]);
            }
            args.extend([
                "--local-model-port".into(),
                local_model_port
                    .context("local advisory endpoint must be selected")?
                    .to_string(),
            ]);
            args.push("--files".into());
            for file in files {
                let file = file.as_str().context("invalid agent source path")?;
                let path = Path::new(file);
                anyhow::ensure!(
                    !path.is_absolute()
                        && path
                            .components()
                            .all(|c| matches!(c, std::path::Component::Normal(_)))
                        && !path.components().any(|c| c.as_os_str() == ".git"),
                    "unsafe agent source selection"
                );
                args.push(file.into());
            }
            if let Some(edit_files) = scope.get("edit_files") {
                let edit_files = edit_files
                    .as_array()
                    .context("edit_files must be an array")?;
                anyhow::ensure!(
                    pack != "pr-agent"
                        && !edit_files.is_empty()
                        && edit_files.iter().all(|file| files.contains(file)),
                    "Editable files must be a nonempty subset of captured context; PR-Agent reviews committed changes"
                );
                args.push("--edit-files".into());
                for file in edit_files {
                    args.push(file.as_str().context("invalid editable path")?.into());
                }
            }
            if let Some(options) = scope.get("mini_swe") {
                anyhow::ensure!(
                    pack == "mini-swe-agent",
                    "mini-SWE harness options belong to mini-swe-agent only"
                );
                validate_mini_harness(options)?;
                args.extend(["--mini-json".into(), serde_json::to_string(options)?]);
            }
            if let Some(options) = scope.get("openhands") {
                anyhow::ensure!(
                    pack == "openhands-sdk",
                    "OpenHands configuration belongs to the SDK only"
                );
                validate_openhands_options(options)?;
                args.extend(["--openhands-json".into(), serde_json::to_string(options)?]);
            }
            if let Some(mode) = scope.get("aider") {
                anyhow::ensure!(
                    pack == "aider",
                    "Aider mode configuration belongs to Aider only"
                );
                validate_aider_mode(mode, &scope)?;
                args.extend(["--aider-json".into(), serde_json::to_string(mode)?]);
            }
            if let Some(feedback) = scope.get("feedback") {
                anyhow::ensure!(
                    pack == "aider",
                    "Native lint/test feedback is supported by Aider only"
                );
                validate_aider_feedback(feedback, &scope)?;
                let test = feedback["test_file"]
                    .as_str()
                    .context("missing feedback test")?;
                let feedback_path = root.join(test);
                let mut current = root.to_path_buf();
                for component in Path::new(test).components() {
                    current.push(component);
                    anyhow::ensure!(
                        !std::fs::symlink_metadata(&current)?
                            .file_type()
                            .is_symlink(),
                        "Feedback path may not contain symlinks"
                    );
                }
                anyhow::ensure!(
                    std::fs::metadata(&feedback_path)?.is_file()
                        && std::fs::metadata(&feedback_path)?.len() <= 1024 * 1024,
                    "Select a regular feedback test no larger than 1MiB"
                );
                rules.push(feedback_path);
                args.extend(["--feedback-json".into(), serde_json::to_string(feedback)?]);
            }
            if let Some(command) = scope.get("command") {
                let command = command.as_str().context("invalid native review command")?;
                anyhow::ensure!(
                    pack == "pr-agent" && ["review", "improve", "describe"].contains(&command),
                    "Only a supported PR-Agent command can be selected"
                );
                args.extend(["--command".into(), command.into()]);
            }
            (
                "Capture an isolated local-agent review or repair proposal for the explicitly selected source and task.",
                "Preserve actual model actions, scope, native output and proposed diff. Agent completion, submission and self-written tests do not verify the product; independent acceptance remains required.",
                args,
            )
        }
        "infer" => {
            if root.join(infer_modes::CONFIG).exists() {
                let selected =
                    infer_modes::recipe(&root, &r.runtime, &artifact_dir(db).join("infer"))?;
                argv = selected.argv;
                rules.extend(selected.rules);
                r.runtime.extend(selected.runtime);
                ("The explicitly selected Infer checker must complete its declared source scope without a supported issue or unresolved capture/model gap.",
                 "Native diagnostics, cost comparisons and any explicit impurity policy are evaluated only within the selected scope. Positive purity facts, caller assumptions, deprecated checkers and experimental/unavailable language paths remain visible; these modes do not automatically repair source.",vec![])
            } else {
                needed(
                &root,
                &[".thruline/infer.json"],
                "Add an explicit Java17 source list in .thruline/infer.json. Infer compilation runs in an isolated retained directory.",
            )?;
                rules.push(root.join(".thruline/infer.json"));
                (
                "The explicitly selected Java build must have no supported Infer issue.",
                "All selected Java sources are freshly captured and analyzed with no reported issue. Missing classpaths, compile errors and excluded sources cannot yield a clean result.",
                vec![artifact_dir(db).join("infer").display().to_string()],
            )
            }
        }
        "ruff" => {
            let selected = root.join(".thruline/ruff.json");
            if selected.is_file() {
                rules.push(selected);
                (
                    "Selected Python modules, stubs and notebook cells must satisfy the explicitly configured native Ruff mode.",
                    "The nonempty selected mode completes with no native diagnostics or undisclosed exclusions. Notebook lines remain cell-local; native formatting and import sorting do not establish behavior.",
                    vec![],
                )
            } else {
                (
                    "Python imports and local variables must satisfy the selected F401 and F841 rules.",
                    "A nonempty Python selection completes both named rules without diagnostics or undisclosed excluded inputs. No fix is applied by this check.",
                    vec![],
                )
            }
        }
        "opengrep" => {
            needed(
                &root,
                &[".thruline/semgrep.yml"],
                "Add reviewed local .thruline/semgrep.yml rules. This recipe explicitly uses Opengrep intrafile taint analysis.",
            )?;
            rules.push(root.join(".thruline/semgrep.yml"));
            let private_home = runtime_root()?.join("opengrep-home");
            anyhow::ensure!(
                private_home.is_dir(),
                "Prepare the private opengrep-home runtime directory before running."
            );
            environment.insert("HOME".into(), private_home.display().to_string());
            environment.insert(
                "XDG_CACHE_HOME".into(),
                private_home.join("cache").display().to_string(),
            );
            environment.insert(
                "SEMGREP_LOG_FILE".into(),
                artifact_dir(db)
                    .join("opengrep")
                    .join("native.log")
                    .display()
                    .to_string(),
            );
            environment.insert(
                "SEMGREP_SETTINGS_FILE".into(),
                artifact_dir(db)
                    .join("opengrep")
                    .join("settings.yml")
                    .display()
                    .to_string(),
            );
            (
                "Source must satisfy the reviewed rules in the selected Opengrep intrafile mode.",
                "A nonempty source selection completes the selected local rules without findings, scanner errors, skipped rules or reported skipped paths. Interfile analysis remains outside this recipe.",
                vec![
                    "scan",
                    "--config",
                    ".thruline/semgrep.yml",
                    "--json",
                    "--dataflow-traces",
                    "--taint-intrafile",
                    "--disable-version-check",
                    "--disable-nosem",
                    "--no-git-ignore",
                    "--strict",
                    "--error",
                    "--timeout",
                    "20",
                    "--max-memory",
                    "2048",
                    ".",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
            )
        }
        "ast-grep" => {
            condition = engine::Condition::NoStructuralMatches;
            let rust = root.join("Cargo.toml").is_file();
            let path = runtime_root()?.join(if rust {
                "policies/no-unsafe.yml"
            } else {
                "policies/no-eval.yml"
            });
            anyhow::ensure!(
                path.is_file(),
                "Starter structural rule is missing. Run scripts/integrations/provision.py."
            );
            rules.push(path.clone());
            (
                if rust {
                    "Rust source must not contain unsafe blocks."
                } else {
                    "JavaScript source must not directly execute strings with eval."
                },
                "No matches for the selected structural rule in the scanned project files.",
                vec![
                    "scan".into(),
                    "--json=compact".into(),
                    "--inspect=entity".into(),
                    "--hint=unused-suppression".into(),
                    "--rule".into(),
                    path.display().to_string(),
                    ".".into(),
                ],
            )
        }
        "cargo-deny" => {
            needed(
                &root,
                &["Cargo.lock"],
                "Generate and review Cargo.lock first; dependency inventory is missing.",
            )?;
            let p = needed(
                &root,
                &["deny.toml", ".thruline/deny.toml"],
                "Add a reviewed deny.toml defining dependency bans and permitted sources. Advisory databases and license policies are separate checks.",
            )?;
            rules.push(p.clone());
            report = ReportSource::Stderr;
            (
                "Dependencies must comply with the project's declared bans and source policy.",
                "No errors or warnings under the configured dependency bans and sources.",
                vec![
                    "--offline",
                    "--locked",
                    "--log-level",
                    "info",
                    "--format",
                    "json",
                    "--config",
                ]
                .into_iter()
                .map(String::from)
                .chain([
                    p.display().to_string(),
                    "check".into(),
                    "bans".into(),
                    "sources".into(),
                ])
                .collect(),
            )
        }
        "dependency-cruiser" => {
            let p = needed(
                &root,
                &[
                    ".dependency-cruiser.cjs",
                    ".dependency-cruiser.json",
                    "architecture.cjs",
                ],
                "Add a reviewed dependency-cruiser configuration declaring allowed module boundaries, exclusions and resolution settings.",
            )?;
            rules.push(p.clone());
            let report_selection = root.join(".thruline/dependency-cruiser-report.json");
            if std::fs::symlink_metadata(&report_selection).is_ok() {
                anyhow::ensure!(
                    std::fs::symlink_metadata(root.join(".thruline"))?
                        .file_type()
                        .is_dir()
                        && std::fs::symlink_metadata(&report_selection)?
                            .file_type()
                            .is_file(),
                    "Native dependency report selection must be a regular JSON file"
                );
                rules.push(report_selection);
            }
            dependency_modes_enabled = dependency_modes::configure(
                db,
                &root,
                &r.runtime,
                &mut rules,
                &mut dependencies,
                &mut environment,
            )?;
            (
                "Module dependencies must respect the project's declared architecture rules.",
                "No reported violations in the configured dependency graph.",
                vec![
                    "--config".into(),
                    p.display().to_string(),
                    "--output-type".into(),
                    "json".into(),
                    ".".into(),
                ],
            )
        }
        "knip" => {
            let p = needed(
                &root,
                &[
                    "knip.json",
                    "knip.jsonc",
                    ".knip.json",
                    ".knip.jsonc",
                    "knip.ts",
                    "knip.js",
                    "knip.config.ts",
                    "knip.config.js",
                ],
                "Add a reviewed Knip configuration with this project's entry points, workspaces and compiler settings. Dynamic compiler configurations execute within the accepted native worker.",
            )?;
            rules.push(p.clone());
            let reporter = runtime_root()?.join("reporters/knip.mjs");
            anyhow::ensure!(
                reporter.is_file(),
                "Provision the Knip coverage reporter before running; a plain finding list does not describe scan coverage."
            );
            rules.push(reporter.clone());
            let selection = root.join(".thruline/knip-selection.json");
            let extra = if selection.is_file() {
                let value = engine::parse_json(&bounded_file(&selection, 16384)?)?;
                let args = knip_selection_args(&value)?;
                rules.push(selection);
                args
            } else {
                vec![]
            };
            let mut args = vec![
                "--config".into(),
                p.display().to_string(),
                "--reporter".into(),
                reporter.display().to_string(),
            ];
            args.extend(extra);
            (
                "The accepted Knip workspace and entry-point selection should have no unresolved, unlisted or unused dependency/export candidates.",
                "All selected native inputs are processed with no configuration gaps or reported issue families. Built-in compiler import extraction does not establish unused exports inside compiled files; source mapping and catalog scope remain explicit.",
                args,
            )
        }
        "betterleaks" => {
            let p = needed(
                &root,
                &[".betterleaks.toml", ".gitleaks.toml"],
                "Add a reviewed .betterleaks.toml describing the intended secret patterns and exclusions. Remote credential validation stays off.",
            )?;
            rules.push(p.clone());
            (
                "Project files must not contain candidates prohibited by the selected secret policy.",
                "No secret candidates under the selected local policy; values are never retained.",
                vec![
                    "dir".into(),
                    ".".into(),
                    "--config".into(),
                    p.display().to_string(),
                    "--report-format".into(),
                    "json".into(),
                    "--report-path".into(),
                    "-".into(),
                    "--redact=100".into(),
                    "--validation=false".into(),
                    "--no-banner".into(),
                    "--no-color".into(),
                ],
            )
        }
        "cargo-mutants" => {
            needed(
                &root,
                &["Cargo.lock"],
                "Generate and review Cargo.lock first. Mutation testing needs a buildable Rust project and cached dependencies.",
            )?;
            condition = engine::Condition::Maximum {
                fact: "missed".into(),
                value: 0.0,
            };
            report = ReportSource::File {
                path: PathBuf::from("THRULINE_RUN_OUTPUT/mutants.out/outcomes.json"),
            };
            let rustc = r
                .runtime
                .iter()
                .find(|p| p.file_name().is_some_and(|name| name == "rustc"))
                .context("Mutation testing requires its explicitly registered Rust compiler")?;
            environment.insert("RUSTC".into(), rustc.display().to_string());
            environment.insert(
                "PATH".into(),
                format!(
                    "{}:/usr/bin:/bin:/usr/sbin:/sbin",
                    rustc
                        .parent()
                        .context("Rust compiler directory missing")?
                        .display()
                ),
            );
            if cfg!(target_os = "macos") {
                let clang = r
                    .runtime
                    .iter()
                    .find(|p| p.file_name().is_some_and(|name| name == "clang"))
                    .context(
                        "Mutation testing on macOS requires a registered native linker and SDK",
                    )?;
                let sdk = r
                    .runtime
                    .iter()
                    .find(|p| p.extension().is_some_and(|ext| ext == "sdk"))
                    .context("Mutation testing requires a registered macOS SDK")?;
                environment.insert("SDKROOT".into(), sdk.display().to_string());
                environment.insert("CC".into(), clang.display().to_string());
                environment.insert(
                    format!(
                        "CARGO_TARGET_{}_APPLE_DARWIN_LINKER",
                        if cfg!(target_arch = "aarch64") {
                            "AARCH64"
                        } else {
                            "X86_64"
                        }
                    ),
                    clang.display().to_string(),
                );
            }
            let selection = mutation_selection::configuration(&root)?;
            let selected_args = if let Some(selection) = &selection {
                rules.extend(selection.rule_files(&root));
                environment.insert(
                    "THRULINE_MUTATION_SELECTION".into(),
                    serde_json::to_string(selection)?,
                );
                if selection.test_runner == "nextest" {
                    let nextest = r.runtime.iter().find(|p| p.file_name().is_some_and(|name| name == "cargo-nextest") && p.is_file())
                        .context("The selected nextest runner is unavailable. Register its pinned executable before accepting this mutation check; no fallback to cargo test is performed.")?;
                    environment.insert(
                        "THRULINE_MUTANTS_NEXTEST".into(),
                        nextest.display().to_string(),
                    );
                }
                selection.args(&root)?
            } else {
                vec!["--jobs".into(), "1".into()]
            };
            let mut native_args: Vec<String> = vec![
                "mutants",
                "--no-config",
                "--no-shuffle",
                "--timeout",
                "10",
                "--build-timeout",
                "60",
                "--output",
                "THRULINE_RUN_OUTPUT",
                "--colors",
                "never",
            ]
            .into_iter()
            .map(String::from)
            .collect();
            native_args.extend(selected_args);
            (
                "Tests should detect the selected viable changes to program behavior.",
                "No viable mutation survives within the accepted package, feature, diff and shard scope. Other shards, excluded or macro-generated code remain outside this result; equivalent mutations require review.",
                native_args,
            )
        }
        "playwright" => {
            needed(
                &root,
                &[".thruline/checks/playwright.config.cjs"],
                "Add .thruline/checks/playwright.config.cjs with authored assertions and a container-local webServer. Install project dependencies in the pinned image; no package is fetched during a check.",
            )?;
            condition = engine::Condition::AllTestsPass { minimum: 1 };
            rules.push(root.join(".thruline/checks/playwright.config.cjs"));
            (
                "The authored user workflows must work, including their failure and persistence assertions.",
                "At least one authored workflow completes; no failed, flaky or skipped test.",
                vec![
                    "test",
                    "--config",
                    "/input/.thruline/checks/playwright.config.cjs",
                    "--reporter=json",
                    "--workers=1",
                    "--retries=0",
                    "--output=/tmp/browser-results",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
            )
        }
        "schemathesis" => {
            if let Some((specification, inputs)) = schemathesis_mode::inputs(&root)? {
                anyhow::ensure!(
                    r.container.is_some(),
                    "The explicit extensions mode requires the pinned isolated verifier container"
                );
                rules.extend(inputs);
                condition = engine::Condition::AllTestsPass { minimum: 1 };
                let criterion = if specification == "graphql" {
                    "The selected owned GraphQL operations must satisfy the registered custom scalar and business-check scope."
                } else {
                    "The selected owned OpenAPI operations must satisfy the registered custom checks, hooks and serializer scope."
                };
                (criterion,
                 "The parsed schema operation inventory matches the accepted list; every operation is tested with assertions. Required extensions are registered and the finite positive fuzzing run has no native failure, error, skipped operation or capture gap. Selected strategy values, seed and examples are bounded coverage, not general API correctness.",
                 vec!["/input/.thruline/checks/api-run.py".into()])
            } else {
                needed(
                &root,
                &[".thruline/checks/api-run.py"],
                "Add .thruline/checks/api-run.py to start a resettable API on container loopback and run Schemathesis 4.26.1 with its JSON report. The supplied fixture shows the runner contract; live APIs are not targets.",
            )?;
                needed(
                    &root,
                    &["openapi.json"],
                    "Add and review openapi.json with observable request/response requirements.",
                )?;
                rules.extend([
                    root.join(".thruline/checks/api-run.py"),
                    root.join("openapi.json"),
                ]);
                condition = engine::Condition::AllTestsPass { minimum: 1 };
                (
                "The isolated API must satisfy the selected schema and generated assertions.",
                "All selected operations are tested with assertions; no failure, error or skipped operation.",
                vec!["/input/.thruline/checks/api-run.py".into()],
            )
            }
        }
        "semgrep" => {
            if let Some(selection) = semgrep_mode::configuration(&root)? {
                anyhow::ensure!(
                    r.container.is_some(),
                    "Explicit Semgrep modes require the separately provisioned immutable mode-protocol image"
                );
                rules.push(root.join(semgrep_mode::CONFIG));
                rules.push(root.join(&selection.rules));
                if let Some(tests) = &selection.tests {
                    rules.push(root.join(tests));
                }
                for file in [".semgrepignore", ".gitignore"] {
                    if root.join(file).is_file() {
                        rules.push(root.join(file));
                    }
                }
                argv = vec![
                    "/opt/python/bin/python".into(),
                    "-I".into(),
                    semgrep_mode::PROTOCOL.into(),
                    "--thruline-mode".into(),
                    semgrep_mode::CONFIG.into(),
                ];
                match selection.mode.as_str() {
                    "baseline" => {
                        let preparation = semgrep_mode::prepare_snapshot(db, &root)?;
                        semgrep_mode::bind_preparation(&mut environment, &preparation)?;
                        argv.extend(["--git-identity".into(), semgrep_mode::git_identity(&root)?]);
                        (
                            "Current source must introduce no new selected Semgrep findings relative to the explicitly accepted ancestor.",
                            "The native current and differential scans finish without gaps. New findings are zero; older current findings remain separately visible and do not establish whole-project cleanliness.",
                            vec![],
                        )
                    }
                    "rule-test" => (
                        "The reviewed local Semgrep rules and fixes must satisfy their selected annotated fixtures.",
                        "Every selected rule exercises positive and negative assertions, every selected fix matches its fixed fixture, and no missing tests, TODO omissions or native parser errors remain. This does not establish application behavior.",
                        vec![],
                    ),
                    _ => (
                        "Source must comply with the selected local Semgrep rules in the explicitly reviewed target scope.",
                        "The nonempty native scan finishes with no rule match or undisclosed exclusion. Exact ignored and excluded paths remain visible coverage limits.",
                        vec![],
                    ),
                }
            } else {
                needed(
                    &root,
                    &[".thruline/semgrep.yml"],
                    "Add reviewed .thruline/semgrep.yml rules for this project's languages. Only the provisioned open-source engine and local rules are used.",
                )?;
                rules.push(root.join(".thruline/semgrep.yml"));
                (
                    "Source must comply with the explicitly reviewed security and correctness patterns.",
                    "No rule matches with inline suppression disabled, scanner errors or reported skipped files in the selected source scope.",
                    vec![
                        "scan",
                        "--config",
                        "/input/.thruline/semgrep.yml",
                        "--json",
                        "--metrics=off",
                        "--disable-version-check",
                        "--strict",
                        "--error",
                        "--disable-nosem",
                        "/input",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                )
            }
        }
        "osv-scanner" => {
            if root.join(osv_modes::CONFIG).exists() {
                anyhow::ensure!(
                    r.container.is_none(),
                    "OSV modes require the normal confined native worker"
                );
                let selected =
                    osv_modes::recipe(&root, &r.runtime, &artifact_dir(db).join("osv-scanner"))?;
                argv = selected.argv;
                rules.extend(selected.rules);
                r.runtime.extend(selected.runtime);
                dataset.extend(selected.dataset);
                osv_modes_enabled = true;
                ("Selected native dependency analysis must complete within its prepared dataset and policy scope without unsuppressed advisory, license-policy or deprecation observations.",
                "Unfiltered findings, exact suppression policy, package/layer inventory, native metadata and available symbol analysis remain inspectable. No unknown-vulnerability or runtime exploitability absence is inferred.", vec![])
            } else {
                let (ecosystem, lockfile, requirement) = if root.join("Cargo.lock").is_file() {
                    (
                        "crates.io",
                        "Cargo.lock",
                        "Selected Rust dependency versions must not match the reviewed advisory database snapshot.",
                    )
                } else {
                    needed(
                        &root,
                        &["package-lock.json"],
                        "Select Cargo.lock with its crates.io snapshot, or package-lock.json with explicit dated npm package/version coverage. Other ecosystems need their own qualified snapshot.",
                    )?;
                    (
                        "npm",
                        "package-lock.json",
                        "Selected npm dependency versions must not match the explicitly covered advisory snapshot.",
                    )
                };
                let database = runtime_root()?.join("osv-db");
                let zip = database.join(format!("osv-scalibr/{ecosystem}/all.zip"));
                let provenance = database.join(format!("osv-scalibr/{ecosystem}/provenance.json"));
                anyhow::ensure!(
                    zip.is_file() && provenance.is_file(),
                    "Provision a dated {ecosystem} database and its acquisition provenance. No database means no advisory check; the scan never downloads one."
                );
                let provenance_value = engine::parse_json(&bounded_file(&provenance, 65536)?)?;
                anyhow::ensure!(
                    provenance_value.get("sha256").and_then(Value::as_str)
                        == Some(hash_file(&zip)?.as_str())
                        && provenance_value.get("bytes").and_then(Value::as_u64)
                            == Some(zip.metadata()?.len()),
                    "The advisory database differs from its recorded acquisition checksum or byte count. Re-prepare the local snapshot and review it; this is unavailable coverage, not a vulnerability-free result."
                );
                if ecosystem == "npm" {
                    let selected_lock = root.join(lockfile);
                    validate_npm_advisory_scope(
                        &engine::parse_json(&bounded_file(&selected_lock, 8 * 1024 * 1024)?)?,
                        &provenance_value,
                    )?;
                    // Coverage is specific to these exact locked versions. Changing
                    // them requires a new accepted plan even if source is otherwise current.
                    rules.push(selected_lock);
                }
                dataset.extend([zip, provenance]);
                environment.insert(
                    "OSV_SCANNER_LOCAL_DB_CACHE_DIRECTORY".into(),
                    database.display().to_string(),
                );
                (
                    requirement,
                    "At least one dependency is recognized and no known advisory matches that dated snapshot. This does not establish exploitability or absence of unknown vulnerabilities.",
                    vec![
                        "scan",
                        "source",
                        "--offline",
                        "--no-resolve",
                        "--no-call-analysis=all",
                        "--all-packages",
                        "--all-vulns",
                        "--format",
                        "json",
                        "--lockfile",
                        lockfile,
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                )
            }
        }
        _ => bail!("No guided recipe for this source"),
    };
    argv.extend(args);
    let infer_mode_capture = pack == "infer"
        && argv.iter().any(|a| {
            Path::new(a)
                .file_name()
                .is_some_and(|n| n == "infer-modes.py")
        });
    let semgrep_mode_capture =
        pack == "semgrep" && argv.iter().any(|a| a == semgrep_mode::PROTOCOL);
    Ok(Plan {
        support: support(pack)?,
        schema: "thruline.integration-plan.v1".into(),
        pack: pack.into(),
        format: if pack == "osv-scanner" {
            "osv-json-v1".into()
        } else {
            format!("{pack}-json-v1")
        },
        root,
        argv,
        container: r.container,
        local_model_port,
        writable_paths: if [
            "infer",
            "joern",
            "openrewrite",
            "clippy",
            "cargo-mutants",
            "opengrep",
        ]
        .contains(&pack)
            || dependency_modes_enabled
            || osv_modes_enabled
        {
            vec![artifact_dir(db).join(pack)]
        } else {
            vec![]
        },
        criterion: Criterion {
            id: format!("integration-{pack}"),
            revision: "1".into(),
            requirement: requirement.into(),
            success: success.into(),
            accepted_by: "local-developer".into(),
            condition,
        },
        acceptance: None,
        approved_setup: None,
        rules,
        runtime: r.runtime,
        dependencies,
        dataset,
        environment,
        input_closure_complete: true,
        timeout_seconds: if infer_mode_capture {
            360
        } else if dependency_modes_enabled || osv_modes_enabled {
            300
        } else if advisory {
            660
        } else if [
            "cargo-mutants",
            "joern",
            "openrewrite",
            "clippy",
            "sonarqube",
            "megalinter",
        ]
        .contains(&pack)
        {
            300
        } else {
            60
        },
        output_limit_bytes: if infer_mode_capture
            || advisory
            || pack == "megalinter"
            || semgrep_mode_capture
            || dependency_modes_enabled
            || osv_modes_enabled
        {
            8 * 1024 * 1024
        } else {
            2 * 1024 * 1024
        },
        cpu_seconds: if infer_mode_capture { 180 } else { 120 },
        memory_mb: if ["joern", "openrewrite"].contains(&pack) {
            4096
        } else {
            2048
        },
        report,
    })
}
fn proposal(
    db: &Path,
    root: &Path,
    pack: &str,
    seal: bool,
    store: &Path,
) -> Result<(Plan, Contract, Option<String>, String)> {
    let rows = history(root, store)?;
    let prior = rows.last();
    let mut plan = recipe(db, root, pack)?;
    let mut contract = match prior {
        Some(row) => Document::parse(row.document.as_bytes())?.contract().clone(),
        None => Contract {
            version: 1,
            id: "project-requirements".into(),
            revision: 0,
            objective: "Review project requirements using explicitly accepted checks.".into(),
            criteria: vec![],
            checks: vec![],
            protected_paths: vec![],
        },
    };
    contract.revision = contract
        .revision
        .checked_add(1)
        .context("requirement revision overflow")?;
    plan.criterion.revision = contract.revision.to_string();
    if seal {
        plan.approved_setup = Some(setup_identity(&plan)?);
    }
    let digest = hash_json(&plan)?;
    contract.checks.retain(|c| c.id != plan.criterion.id);
    contract.criteria.retain(|c| c.id != plan.criterion.id);
    contract.checks.push(policy::Check {
        verification: None,
        id: plan.criterion.id.clone(),
        category: policy::Category::Lint,
        argv: plan.argv.clone(),
        cwd: ".".into(),
        runner: format!("integration:{digest}"),
        mode: policy::ExecutionMode::Captured,
        timeout_seconds: plan.timeout_seconds as u32,
        artifacts: vec![],
    });
    // The tool condition is explicitly narrower than the user's full requirement.
    contract.criteria.push(policy::Criterion {
        evidence_kind: None,
        id: plan.criterion.id.clone(),
        description: format!(
            "{} Check: {}",
            plan.criterion.requirement, plan.criterion.success
        ),
        check_ids: vec![plan.criterion.id.clone()],
        human_acceptance_required: true,
    });
    contract.validate()?;
    let review = hash_json(&(&plan, &contract, prior.map(|r| &r.hash)))?;
    Ok((plan, contract, prior.map(|r| r.hash.clone()), review))
}
pub fn overview(db: &Path, root: &Path) -> Result<Value> {
    overview_at(db, root, &AcceptedLedger::default_path()?)
}
pub fn overview_at(db: &Path, root: &Path, store: &Path) -> Result<Value> {
    let root = root.canonicalize()?;
    let rows = history(&root, store)?;
    let current = rows.last();
    let accepted = current
        .map(|r| Document::parse(r.document.as_bytes()))
        .transpose()?;
    let recipes:Vec<Value>=PACKS.iter().filter(|pack| super::shipped(pack)).map(|pack| {
        let accepted_check=accepted.as_ref().and_then(|doc|doc.contract().checks.iter().find(|c|c.id==format!("integration-{pack}") && c.runner.starts_with("integration:")));
        match proposal(db,&root,pack,false,store) {Ok((plan,_,_,digest))=>json!({"id":pack,"support":plan.support,"ready":true,"database":database_note(&plan),"requirement":plan.criterion.requirement,"success":plan.criterion.success,"why":format!("This check directly measures: {}",plan.criterion.success),"review_digest":digest,"accepted":accepted_check.is_some(),"acceptance_hash":current.map(|r|&r.hash),"next":"Review the requirement and exact check; then accept or run it."}),Err(e)=>json!({"id":pack,"support":support(pack).ok().flatten(),"ready":false,"accepted":accepted_check.is_some(),"setup":e.to_string(),"next":"Complete the named setup, then refresh. Missing setup is not a passing result."})}
    }).collect();
    Ok(
        json!({"schema":"thruline.integration-workflow.v1","ai_review":super::AI_REVIEW,"accepted_revision":current.map(|r|&r.hash),"recipes":recipes,"ruff_repair_configuration":super::repair::configuration_summary(&root),"trust":"Local acceptance; all tool conclusions remain advisory. Human acceptance of the full requirement remains open."}),
    )
}
pub fn action(db: &Path, root: &Path, action: &str, pack: &str, digest: &str) -> Result<Value> {
    action_at(
        db,
        root,
        action,
        pack,
        digest,
        &AcceptedLedger::default_path()?,
    )
}
pub fn action_at(
    db: &Path,
    root: &Path,
    action: &str,
    pack: &str,
    digest: &str,
    accepted_store: &Path,
) -> Result<Value> {
    let root = root.canonicalize()?;
    if action == "status" {
        return overview_at(db, &root, accepted_store);
    }
    anyhow::ensure!(!super::withheld(pack), "{}", super::AI_REVIEW_NOT_IN_BUILD);
    anyhow::ensure!(PACKS.contains(&pack), "unknown guided recipe");
    let store = accepted_store.to_path_buf();
    if action == "run" {
        let rows = history(&root, &store)?;
        let row = rows
            .last()
            .context("Review and accept this requirement before running its check.")?;
        anyhow::ensure!(
            row.hash == digest,
            "The accepted requirements changed. Refresh and review them before running."
        );
        let doc = Document::parse(row.document.as_bytes())?;
        let id = format!("integration-{pack}");
        let check = doc
            .contract()
            .checks
            .iter()
            .find(|c| c.id == id)
            .context("This check has not been accepted.")?;
        let plan_digest = check
            .runner
            .strip_prefix("integration:")
            .context("not an integration check")?;
        anyhow::ensure!(
            plan_digest.len() == 64 && plan_digest.bytes().all(|b| b.is_ascii_hexdigit()),
            "invalid accepted plan digest"
        );
        let bytes = bounded_file(
            &artifact_dir(db).join(format!("{plan_digest}.json")),
            256 * 1024,
        )?;
        anyhow::ensure!(
            sha256_hex(&bytes) == plan_digest,
            "accepted plan artifact changed"
        );
        let mut plan: Plan = serde_json::from_value(engine::parse_json(&bytes)?)?;
        anyhow::ensure!(
            plan.root == root && plan.pack == pack,
            "accepted plan belongs to another project or recipe"
        );
        runtime(pack)?; // unsupported/replaced installation never silently runs
        plan.acceptance = Some(AcceptanceRef {
            store,
            revision_hash: row.hash.clone(),
            check_id: id,
            plan_digest: plan_digest.into(),
        });
        if matches!(plan.report, ReportSource::File { .. }) {
            let output = artifact_dir(db).join("cargo-mutants").join(format!(
                "run-output-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            ));
            for arg in &mut plan.argv {
                if arg == "THRULINE_RUN_OUTPUT" {
                    *arg = output.display().to_string();
                }
            }
            plan.report = ReportSource::File {
                path: output.join("mutants.out/outcomes.json"),
            };
        }
        validate(&plan)?;
        return execute(db, plan);
    }
    let (plan, contract, previous, review) = proposal(db, &root, pack, true, &store)?;
    if action == "preview" {
        return Ok(
            json!({"schema":"thruline.integration-review.v1","digest":review,"database":database_note(&plan),"plan":plan,"contract":contract,"previous":previous,"commands_executed":false,"scope_note":"Git-ignored and configured excluded files may not be scanned. This is a bounded tool condition, not whole-requirement verification."}),
        );
    }
    anyhow::ensure!(action == "accept", "unsupported workflow action");
    anyhow::ensure!(
        review == digest,
        "The reviewed plan or previous requirement changed. Open a new preview before accepting."
    );
    let prepared_dependency = if pack == "dependency-cruiser" {
        dependency_modes::accepted_snapshot(db, &root)?
    } else {
        None
    };
    let (snapshot_head, snapshot_tree) = if let Some(snapshot) = prepared_dependency {
        snapshot
    } else if let Some(snapshot) = semgrep_mode::accepted_snapshot(db, &root)? {
        snapshot
    } else {
        let snapshot = thruline_core::spec::snapshot::snapshot_tree(&root)?;
        (snapshot.head_commit, snapshot.tree_hash)
    };
    private_write(&artifact_dir(db), &serde_json::to_vec(&plan)?)?;
    let ledger = AcceptedLedger::open(&store)?;
    let repo = ledger.repo(&root.to_string_lossy(), Some(&root.to_string_lossy()))?;
    let row = ledger.accept_requirements_at(
        &repo,
        &serde_json::to_string(&contract)?,
        snapshot_head.as_deref(),
        Some(&snapshot_tree),
        "local-developer",
        previous.as_deref(),
    )?;
    Ok(
        json!({"accepted":true,"acceptance_hash":row.hash,"commands_executed":false,"next":"The requirement is accepted. Run its check when ready; acceptance alone proves nothing about the code."}),
    )
}

#[cfg(test)]
mod npm_advisory_tests {
    use super::*;

    fn lock(version: &str) -> Value {
        json!({"lockfileVersion":3,"packages":{"":{},"node_modules/minimist":{"version":version,"resolved":format!("https://registry.npmjs.org/minimist/-/minimist-{version}.tgz"),"integrity":"sha512-publisher-digest"}}})
    }

    fn coverage() -> Value {
        json!({"schema":"thruline.osv-npm-subset.v1","package_versions":{"minimist":["1.2.0","1.2.8"]}})
    }

    #[test]
    fn unqueried_versions_cannot_become_clean_subset_results() {
        for version in ["1.2.0", "1.2.8"] {
            assert!(validate_npm_advisory_scope(&lock(version), &coverage()).is_ok());
        }
        assert!(validate_npm_advisory_scope(&lock("1.2.7"), &coverage()).is_err());
        assert!(validate_npm_advisory_scope(
            &lock("1.2.8"),
            &json!({"package_versions":{"minimist":["1.2.8"]}})
        )
        .is_err());
        let mut unknown_package = lock("1.2.8");
        unknown_package["packages"]["node_modules/other"] = json!({"version":"1.0.0","resolved":"https://registry.npmjs.org/other/-/other-1.0.0.tgz","integrity":"sha512-other"});
        assert!(validate_npm_advisory_scope(&unknown_package, &coverage()).is_err());
    }

    #[test]
    fn incomplete_or_foreign_lock_entries_are_unavailable() {
        for field in ["version", "resolved", "integrity"] {
            let mut value = lock("1.2.8");
            value["packages"]["node_modules/minimist"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(validate_npm_advisory_scope(&value, &coverage()).is_err());
        }
        let mut linked = lock("1.2.8");
        linked["packages"]["node_modules/minimist"]["link"] = json!(true);
        assert!(validate_npm_advisory_scope(&linked, &coverage()).is_err());
        let mut foreign = lock("1.2.8");
        foreign["packages"]["node_modules/minimist"]["resolved"] = json!("file:../local-package");
        assert!(validate_npm_advisory_scope(&foreign, &coverage()).is_err());
        assert!(validate_npm_advisory_scope(
            &json!({"lockfileVersion":3,"packages":{"":{}}}),
            &coverage()
        )
        .is_err());
    }
}

#[cfg(test)]
mod feedback_validation_tests {
    use super::*;
    use serde_json::json;
    fn base() -> Value {
        json!({"schema":"thruline.aider-feedback.v1", "mode":"lint-and-test", "test_file":"tests/feedback.py", "command":["python","{feedback}","{workspace}"]})
    }
    fn scope() -> Value {
        json!({"files":["notes.py","tests/feedback.py"],"edit_files":["notes.py"]})
    }
    #[test]
    fn supported_native_feedback() {
        assert!(validate_aider_feedback(&base(), &scope()).is_ok());
    }
    #[test]
    fn rejects_cross_paths_and_editable_tests() {
        for path in [
            "/etc/passwd",
            "../outside.py",
            "tests/../../outside.py",
            "tests//feedback.py",
            "./tests/feedback.py",
            ".git/config",
            "tests\\feedback.py",
            "notes.py",
            "unselected.py",
        ] {
            let mut value = base();
            value["test_file"] = json!(path);
            assert!(
                validate_aider_feedback(&value, &scope()).is_err(),
                "{}",
                path
            );
        }
    }
    #[test]
    fn rejects_replaced_program_or_script() {
        for command in [
            json!(["sh", "-c", "{feedback}"]),
            json!(["python", "-c", "print(1)", "{feedback}"]),
            json!(["python", "-m", "module", "{feedback}"]),
            json!(["python", "{feedback}", "{feedback}"]),
            json!(["python", "{feedback}", "{host-secret}"]),
            json!("python feedback.py"),
        ] {
            let mut value = base();
            value["command"] = command;
            assert!(validate_aider_feedback(&value, &scope()).is_err());
        }
    }
    #[test]
    fn rejects_missing_readonly_scope() {
        assert!(validate_aider_feedback(
            &base(),
            &json!({"files":["notes.py","tests/feedback.py"]})
        )
        .is_err());
    }
    #[test]
    fn rejects_unknown_or_oversized_configuration() {
        let mut value = base();
        value["extra"] = json!(true);
        assert!(validate_aider_feedback(&value, &scope()).is_err());
        value = base();
        value["command"] = json!(["python", "{feedback}", "x".repeat(4097)]);
        assert!(validate_aider_feedback(&value, &scope()).is_err());
        value = base();
        value["mode"] = json!("future");
        assert!(validate_aider_feedback(&value, &scope()).is_err());
    }
}

#[cfg(test)]
mod aider_mode_validation_tests {
    use super::*;
    use serde_json::json;
    fn mode() -> Value {
        json!({"schema":"thruline.aider-mode.v1","mode":"edit","edit_format":"diff","max_reflections":3})
    }
    #[test]
    fn validates_edit_and_architect() {
        for name in ["edit", "architect"] {
            let mut value = mode();
            value["mode"] = json!(name);
            assert!(validate_aider_mode(&value, &json!({"edit_files":["notes.py"]})).is_ok());
        }
    }
    #[test]
    fn rejects_ask_edit_or_feedback_conflicts() {
        let mut value = mode();
        value["mode"] = json!("ask");
        value["edit_format"] = Value::Null;
        assert!(validate_aider_mode(&value, &json!({})).is_ok());
        for scope in [json!({"edit_files":[]}), json!({"feedback":{}})] {
            assert!(validate_aider_mode(&value, &scope).is_err());
        }
        value["edit_format"] = json!("diff");
        assert!(validate_aider_mode(&value, &json!({})).is_err());
    }
    #[test]
    fn rejects_reflection_and_mode_values() {
        for limit in [json!(-1), json!(7), json!(true), json!(1.5), json!("3")] {
            let mut value = mode();
            value["max_reflections"] = limit;
            assert!(validate_aider_mode(&value, &json!({})).is_err());
        }
        for format in ["ask", "architect", "shell", "future"] {
            let mut value = mode();
            value["edit_format"] = json!(format);
            assert!(validate_aider_mode(&value, &json!({})).is_err());
        }
        let mut value = mode();
        value["extra"] = json!(true);
        assert!(validate_aider_mode(&value, &json!({})).is_err());
    }
}

#[cfg(test)]
mod openhands_option_validation_tests {
    use super::*;
    use serde_json::json;
    fn options() -> Value {
        json!({"schema":"thruline.openhands-options.v1","backend":"local","condenser":{"max_size":10,"keep_first":2,"max_tokens":null}})
    }
    #[test]
    fn accepts_bounded_native_settings() {
        for backend in ["local", "owned-server"] {
            let mut v = options();
            v["backend"] = json!(backend);
            assert!(validate_openhands_options(&v).is_ok());
        }
    }
    #[test]
    fn rejects_unbound_endpoint_model_and_extra_fields() {
        for key in ["host", "url", "api_key", "model", "workspace", "command"] {
            let mut v = options();
            v[key] = json!("http://external.invalid");
            assert!(validate_openhands_options(&v).is_err());
        }
        let mut v = options();
        v["backend"] = json!("http://127.0.0.1:18081");
        assert!(validate_openhands_options(&v).is_err());
    }
    #[test]
    fn rejects_invalid_native_ranges() {
        for maximum in [json!(7), json!(241), json!(true), json!(10.0), json!("10")] {
            let mut v = options();
            v["condenser"]["max_size"] = maximum;
            assert!(validate_openhands_options(&v).is_err());
        }
        for first in [json!(-1), json!(4), json!(true)] {
            let mut v = options();
            v["condenser"]["keep_first"] = first;
            assert!(validate_openhands_options(&v).is_err());
        }
        for tokens in [json!(2047), json!(16385), json!(true)] {
            let mut v = options();
            v["condenser"]["max_tokens"] = tokens;
            assert!(validate_openhands_options(&v).is_err());
        }
    }
}

#[cfg(test)]
mod knip_selection_tests {
    use super::knip_selection_args;
    use serde_json::json;
    #[test]
    fn forwards_supported_native_selection_as_separate_arguments() {
        assert_eq!(knip_selection_args(&json!({"schema":"thruline.knip-selection.v1","workspaces":["packages/consumer","@scope/*"],"strict":true})).unwrap(),vec!["--workspace","packages/consumer","--workspace","@scope/*","--strict"]);
        assert_eq!(
            knip_selection_args(&json!({"schema":"thruline.knip-selection.v1","production":true}))
                .unwrap(),
            vec!["--production"]
        );
        assert!(
            knip_selection_args(&json!({"schema":"thruline.knip-selection.v1"}))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn invalid_or_conflicting_selection_is_refused() {
        for value in [
            json!({"schema":"other"}),
            json!({"schema":"thruline.knip-selection.v1","strict":true,"production":false}),
            json!({"schema":"thruline.knip-selection.v1","strict":"yes"}),
            json!({"schema":"thruline.knip-selection.v1","extra":true}),
            json!({"schema":"thruline.knip-selection.v1","workspaces":[]}),
            json!({"schema":"thruline.knip-selection.v1","workspaces":["a","a"]}),
            json!({"schema":"thruline.knip-selection.v1","workspaces":["--help"]}),
            json!({"schema":"thruline.knip-selection.v1","workspaces":["a\nb"]}),
        ] {
            assert!(knip_selection_args(&value).is_err(), "{value}");
        }
    }
}

#[cfg(test)]
mod pr_agent_plain_diff_tests {
    use super::validate_pr_agent_plain_diff;
    use serde_json::json;

    #[test]
    fn plain_diff_accepts_only_its_explicit_patch_scope() {
        let options = json!({"schema":"thruline.pr-agent-plain-diff.v1","mode":"plain-diff","diff_file":"change.diff"});
        assert!(validate_pr_agent_plain_diff(&options, &json!({"files":["change.diff"]})).is_ok());
        for scope in [
            json!({"files":["change.diff","secret.py"]}),
            json!({"files":["change.diff"],"base_ref":"main"}),
            json!({"files":["change.diff"],"edit_files":["change.diff"]}),
        ] {
            assert!(validate_pr_agent_plain_diff(&options, &scope).is_err());
        }
    }

    #[test]
    fn plain_diff_rejects_unidentified_and_escaping_inputs() {
        for path in [
            "",
            "../change.diff",
            "/tmp/change.diff",
            ".git/config",
            "x//change.diff",
            "x/./change.diff",
        ] {
            let options = json!({"schema":"thruline.pr-agent-plain-diff.v1","mode":"plain-diff","diff_file":path});
            assert!(validate_pr_agent_plain_diff(&options, &json!({"files":[path]})).is_err());
        }
        let options = json!({"schema":"future","mode":"plain-diff","diff_file":"change.diff"});
        assert!(validate_pr_agent_plain_diff(&options, &json!({"files":["change.diff"]})).is_err());
    }
}

#[cfg(test)]
mod mini_harness_validation_tests {
    use super::*;
    #[test]
    fn accepts_only_owned_namespace_and_bounded_literal_task() {
        let good = json!({"schema":"thruline.mini-harness-options.v1","mode":"owned-swebench-task","environment":"local-in-linux","instance_id":"owned_case-01","command_timeout":25});
        assert!(validate_mini_harness(&good).is_ok());
        for (key, bad) in [
            ("environment", json!("docker")),
            ("mode", json!("official-score")),
            ("instance_id", json!("../outside")),
            ("instance_id", json!("")),
            ("command_timeout", json!(26)),
            ("command_timeout", json!(true)),
        ] {
            let mut value = good.clone();
            value[key] = bad;
            assert!(validate_mini_harness(&value).is_err(), "{value}");
        }
        let mut value = good;
        value["run_args"] = json!(["--privileged"]);
        assert!(validate_mini_harness(&value).is_err());
    }
}
