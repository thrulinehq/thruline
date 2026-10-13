//! Explicit, bounded integration runs. Reuses EVB_V1 and the observation chain;
//! report files are content-addressed artifacts, never a second verdict ledger.
pub(crate) mod agent_repair;
mod container;
mod dependency_modes;
#[cfg(test)]
mod door_census;
mod infer_modes;
mod input_read;
mod mutation_artifacts;
pub(crate) mod native_repair;
mod osv_modes;
pub(crate) mod repair;
pub(crate) mod review;
mod semgrep_mode;
mod suppressions;
mod worker;
pub(crate) mod workflow;
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::ExitCode,
};
use thruline_core::{
    chain::sha256_hex,
    evidence_binding::{
        self as evb, ArtifactDigest, EvidenceBinding, ExecMode, IdentityEdge, WsidScopeDecl,
    },
    ledger::Ledger,
    workspace_identity::{self as wsid, Observation as WObservation, Scope},
};
use thruline_verify::integrations::{
    self as engine, Admission, Criterion, Inputs, Report, RunState,
};

/* ── AI-REVIEW-OUT-OF-LAUNCH (founder ruling R-2026-09-28-20) ─────────── */

/// Whether this build offers AI review (the PR-Agent route). It is the
/// `ai-review` cargo feature and nothing else: the launch build is the default
/// build, where it is off, and no project file or environment variable can
/// turn it back on. Served to the screens as `ai_review`.
pub(crate) const AI_REVIEW: bool = cfg!(feature = "ai-review");

/// The one recipe and catalog source the flag withholds.
pub(crate) const AI_REVIEW_PACK: &str = "pr-agent";

/// Why a door refuses the AI review recipe in a build without it. It keeps
/// the words "not provisioned" the review tool's own refusal uses, so the
/// T-4-1a sentence that leads a refusal ("the review tool isn't set up on
/// this machine") has an honest reason underneath it.
pub(crate) const AI_REVIEW_NOT_IN_BUILD: &str =
    "AI review is not provisioned in this build of ThruLine: it was built without the ai-review feature, so no PR-Agent review runs";

/// The ids this build ships: the recipes and sources it may NAME in anything
/// it serves or prints (AI-REVIEW-OUT-OF-LAUNCH-R5). It is an allow-list, and
/// it is the launch UI twin's list (`ai-review-ui.launch.js` ships the
/// catalog's ids, minus the AI-review row; `ai-review-allow-list.test.js`
/// holds the two equal, and this to the catalog).
///
/// Four rounds of R1–R4 recognised the withheld id by what it looks like, and
/// every recogniser lost to the next spelling. So nothing here asks what an
/// id looks like: an id is named when it is in this list, exactly, and is
/// otherwise counted and never named. A new source row in the catalog is not
/// shipped until it is added here.
pub(crate) const SHIPPED_IDS: [&str; 32] = [
    "in-toto",
    "salsa",
    "ast-grep",
    "dependency-cruiser",
    "knip",
    "cargo-mutants",
    "proptest",
    "cargo-fuzz",
    "kani",
    "loom",
    "criterion",
    "dhat",
    "cargo-deny",
    "osv-scanner",
    "betterleaks",
    "semgrep",
    "playwright",
    "schemathesis",
    "swe-bench",
    "inspect-ai",
    "promptfoo",
    "ruff",
    "opengrep",
    "infer",
    "aider",
    "openhands-sdk",
    "mini-swe-agent",
    "clippy",
    "joern",
    "openrewrite",
    "sonarqube",
    "megalinter",
];

/// May this build name this id? In a build without AI review, only an id in
/// [`SHIPPED_IDS`], spelled exactly. The `ai-review` build ships everything it
/// ever did, so for it this is always true and every gate it feeds is open.
pub(crate) fn shipped(id: &str) -> bool {
    AI_REVIEW || SHIPPED_IDS.contains(&id)
}

/// What the HQ route may be asked to act on: an id the build ships, or one of
/// the two review modes (the review screen sends its mode as the recipe).
pub(crate) fn route_recipe_admitted(recipe: &str) -> bool {
    shipped(recipe) || matches!(recipe, "full" | "change")
}

/// What a registered selection this build does not run becomes: a count,
/// never the words that were registered. `hq-copy.js`
/// `change.review.not_run_here`, held word for word by
/// `ui/src/workspace/ai-review-allow-list.test.js`.
pub(crate) const SELECTION_NOT_RUN_ONE: &str =
    "1 registered selection is not one this build runs, so it was left out of this review.";
pub(crate) const SELECTION_NOT_RUN_MANY: &str =
    "{n} registered selections are not ones this build runs, so they were left out of this review.";

/// What a request for a check this build does not run is answered with.
pub(crate) const CHECK_NOT_RUN: &str = "This build does not run that check.";

pub(crate) fn selections_not_run(count: usize) -> String {
    if count == 1 {
        SELECTION_NOT_RUN_ONE.to_string()
    } else {
        SELECTION_NOT_RUN_MANY.replace("{n}", &count.to_string())
    }
}

/// Is this the withheld review, in any spelling a person would read as its
/// name? This decides which SENTENCE a direct request for it is refused with
/// (round 1's, which names the review): it is not what keeps an id out of what
/// is served. That is [`shipped`], which never asks what an id looks like, so
/// a spelling this misses is still not named. In the `ai-review` build it is
/// never true.
pub(crate) fn withheld(pack: &str) -> bool {
    !AI_REVIEW && normalised_id(pack) == normalised_id(AI_REVIEW_PACK)
}

/// A recipe id reduced to what a person would read as the same name.
fn normalised_id(id: &str) -> String {
    id.trim()
        .chars()
        .map(|c| match u32::from(c) {
            0xFF01..=0xFF5E => char::from_u32(u32::from(c) - 0xFEE0).unwrap_or(c),
            _ => c,
        })
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// What is said when an operator-owned file has the wrong shape. serde names
/// an unknown field or a value of the wrong kind by quoting it, and those
/// words are the operator's own, so an id this build does not ship would reach
/// the screen through the error (AI-REVIEW-OUT-OF-LAUNCH-R5). Keep what serde
/// says up to its first quotation, and name nothing. A missing field is a
/// name the code chose, so it stays whole.
pub(crate) fn shape_error(what: &str, error: serde_json::Error) -> anyhow::Error {
    // The `ai-review` build says what serde says, as it always did.
    if AI_REVIEW {
        return error.into();
    }
    let text = error.to_string();
    if text.starts_with("missing field") {
        return anyhow::anyhow!("{what}: {text}");
    }
    let cut = text.find(['`', '"']).unwrap_or(text.len());
    let head = text[..cut].trim_end_matches([' ', ':', ',']);
    anyhow::anyhow!("{what}: {head}")
}

/// The packs an operator's evaluation files name (`cases[].pack`, in the labels
/// and in the predictions), refused unless the build ships every one
/// (AI-REVIEW-OUT-OF-LAUNCH-R6). `integration evaluate` printed each case's
/// pack verbatim, as a `by_adapter` key and as `cases[].pack`, so an operator's
/// own "qodo-merge" came back out of a launch build, and round 5's hand-made
/// list of the doors that echo an id had missed it. It is refused, as the
/// sibling doors (`plan`, `run`, `decode`) refuse a pack the build does not
/// ship, and not counted into an unnamed bucket: a partial evaluation that
/// quietly leaves cases out of its named rows is one a person could mistake
/// for the whole of what they supplied. The withheld review keeps round 1's
/// sentence, as at every other door; any other pack gets the words a request
/// for a check this build does not run is always answered in, echoing none of
/// what was sent. In the `ai-review` build every pack is shipped, so this
/// accepts everything and the evaluation is exactly what it was.
fn refuse_packs_not_shipped(documents: &[&Value]) -> Result<()> {
    let packs: Vec<&str> = documents
        .iter()
        .flat_map(|document| document.get("cases").and_then(Value::as_array))
        .flatten()
        .filter_map(|case| case.get("pack").and_then(Value::as_str))
        .collect();
    anyhow::ensure!(
        !packs.iter().any(|pack| withheld(pack)),
        "{AI_REVIEW_NOT_IN_BUILD}"
    );
    anyhow::ensure!(packs.iter().all(|pack| shipped(pack)), "{CHECK_NOT_RUN}");
    Ok(())
}

/// The source catalog as this build serves and prints it: only the sources it
/// ships, so without the withheld source when AI review is not part of the
/// build.
pub(crate) fn served_catalog() -> Result<Value> {
    let mut catalog: Value = serde_json::from_str(engine::CATALOG)?;
    if let Some(projects) = catalog.get_mut("projects").and_then(Value::as_array_mut) {
        projects.retain(|p| p["id"].as_str().is_some_and(shipped));
    }
    Ok(catalog)
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Inspect, verify or apply a bounded native deterministic remedy.
    NativeRepair {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        action: String,
        #[arg(long)]
        recipe: String,
        #[arg(long, default_value = "")]
        digest: String,
    },
    /// Resume native agent work or independently verify a captured proposal.
    AgentRepair {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        action: String,
        #[arg(long)]
        recipe: String,
        #[arg(long, default_value = "")]
        digest: String,
    },
    /// Discover, accept, run, resume and export a bounded repository review.
    Review {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        exec_db: Option<PathBuf>,
        #[arg(long)]
        action: String,
        #[arg(long, default_value = "full")]
        mode: String,
        #[arg(long, default_value = "")]
        digest: String,
        /// Bind this new queue explicitly to one Change. Omit with
        /// --examined-inputs-id for legacy project-only reviews. Hidden in a
        /// build without AI review, where no Change-bound review can run.
        #[arg(long, hide = !AI_REVIEW)]
        change_id: Option<String>,
        /// Existing receipt for the exact inputs examined by the review.
        #[arg(long, hide = !AI_REVIEW)]
        examined_inputs_id: Option<String>,
    },
    /// Preview, independently verify, apply or inspect a supported isolated Ruff repair.
    Repair {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        action: String,
        #[arg(long, default_value = "")]
        digest: String,
    },
    /// List pinned sources and native report routes. Executes nothing.
    Catalog,
    /// Measure supplied independent case labels against recorded outcomes; never graduates a tool.
    Evaluate {
        #[arg(long)]
        cases: PathBuf,
        #[arg(long)]
        results: PathBuf,
    },
    /// Guided, requirement-linked checks; no shell command construction.
    Workflow {
        #[arg(long)]
        root: PathBuf,
        /// Existing acceptance store, when this project uses a non-default store.
        #[arg(long)]
        exec_db: Option<PathBuf>,
        #[arg(long)]
        action: String,
        #[arg(long, default_value = "")]
        recipe: String,
        #[arg(long, default_value = "")]
        digest: String,
    },
    #[command(hide = true)]
    SemgrepSnapshot {
        #[arg(long)]
        root: PathBuf,
    },
    #[command(hide = true)]
    WorkerExec {
        #[arg(long)]
        cpu_seconds: u64,
        #[arg(long)]
        memory_mb: u64,
        #[arg(long)]
        local_model_port: Option<u16>,
        #[arg(long)]
        read_path: Vec<PathBuf>,
        #[arg(long)]
        write_path: Vec<PathBuf>,
        #[arg(last=true,num_args=1..)]
        argv: Vec<String>,
    },
    /// Read the tool workspace for a recorded session; never starts a check.
    Workspace {
        #[arg(long)]
        session: String,
        #[arg(long)]
        presentation: bool,
    },
    /// Read tool setup and receipts for an explicitly selected local project.
    Project {
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        exec_db: Option<PathBuf>,
        #[arg(long)]
        presentation: bool,
    },
    /// Produce a reviewable plan for one tool. Nothing is accepted or executed.
    Plan {
        pack: String,
        #[arg(long)]
        root: PathBuf,
        #[arg(long)]
        executable: PathBuf,
    },
    /// Execute the exact plan, accepting its explicit criterion for this run.
    Run {
        #[arg(long)]
        plan: PathBuf,
        #[arg(long)]
        accept: bool,
    },
    /// Decode an existing report. Imported output is never a captured execution.
    Decode { pack: String, report: PathBuf },
    /// Re-evaluate stored runs against current inputs. Keeps historical receipts.
    Results {
        #[arg(long)]
        root: PathBuf,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub schema: String,
    pub pack: String,
    pub format: String,
    /// Frozen support declaration shown during review and retained in receipts.
    /// Older plans omit it; their original scope must not be retroactively inferred.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support: Option<Value>,
    pub root: PathBuf,
    pub argv: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<container::Spec>,
    /// Explicit optional advisory model endpoint. Deterministic checks retain
    /// the existing deny-all-network worker policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_model_port: Option<u16>,
    /// Managed scratch/artifact locations; the reviewed source stays read-only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub writable_paths: Vec<PathBuf>,
    pub criterion: Criterion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acceptance: Option<workflow::AcceptanceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_setup: Option<String>,
    /// Explicit files outside or inside the workspace; no ignored input is assumed covered.
    pub rules: Vec<PathBuf>,
    #[serde(default)]
    pub runtime: Vec<PathBuf>,
    pub dependencies: Vec<PathBuf>,
    pub dataset: Vec<PathBuf>,
    pub environment: BTreeMap<String, String>,
    pub input_closure_complete: bool,
    pub timeout_seconds: u64,
    pub output_limit_bytes: u64,
    pub cpu_seconds: u64,
    pub memory_mb: u64,
    pub report: ReportSource,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ReportSource {
    Stdout,
    Stderr,
    File { path: PathBuf },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Record {
    pub contract: String,
    pub plan: Plan,
    pub admission: Admission,
    pub report: Option<Report>,
    pub decode_error: Option<String>,
    pub raw_report_sha256: String,
    #[serde(default)]
    pub stderr_excerpt: String,
    pub sampled_peak_rss_bytes: Option<u64>,
    pub resource_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub isolation: Option<Value>,
    pub duration_ms: u64,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub started_at: String,
    pub completed_at: String,
    pub limitations: Vec<String>,
}
fn hash_json(v: &impl Serialize) -> Result<String> {
    Ok(sha256_hex(&serde_json::to_vec(v)?))
}
pub fn support(pack: &str) -> Result<Option<Value>> {
    let registry: Value =
        serde_json::from_str(include_str!("../../../spec/integrations/support.json"))?;
    Ok(registry.get(pack).cloned())
}
fn bounded_file(path: &Path, limit: u64) -> Result<Vec<u8>> {
    use std::io::Read;
    anyhow::ensure!(
        fs::metadata(path)?.is_file(),
        "report input must be a regular file: {}",
        path.display()
    );
    let f = fs::File::open(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut b = Vec::new();
    f.take(limit + 1).read_to_end(&mut b)?;
    if b.len() as u64 > limit {
        bail!("file exceeds {limit} bytes: {}", path.display());
    }
    Ok(b)
}
fn hash_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let metadata = fs::metadata(path)?;
    anyhow::ensure!(
        metadata.is_file(),
        "bound input must be a regular file: {}",
        path.display()
    );
    anyhow::ensure!(
        metadata.len() <= 1024 * 1024 * 1024,
        "bound file exceeds 1 GiB"
    );
    let mut f = fs::File::open(path).with_context(|| format!("cannot bind {}", path.display()))?;
    let mut streamed = 0u64;
    let mut h = Sha256::new();
    let mut b = [0u8; 65536];
    loop {
        let n = f.read(&mut b)?;
        if n == 0 {
            break;
        }
        streamed += n as u64;
        anyhow::ensure!(
            streamed <= 1024 * 1024 * 1024,
            "bound input grew past 1 GiB"
        );
        h.update(&b[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
fn path_at(root: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() {
        p.into()
    } else {
        root.join(p)
    }
}
fn hash_files(root: &Path, paths: &[PathBuf]) -> Result<String> {
    let mut identities = BTreeMap::new();
    let mut count = 0usize;
    let mut total = 0u64;
    for p in paths {
        let full = path_at(root, p).canonicalize()?;
        for entry in walkdir::WalkDir::new(&full).follow_links(false) {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type().is_dir() {
                continue;
            }
            count += 1;
            anyhow::ensure!(count <= 100_000, "input closure exceeds 100,000 files");
            let identity = if entry.file_type().is_symlink() {
                let target = fs::read_link(path)?;
                let resolved = path.canonicalize()?;
                anyhow::ensure!(
                    resolved.is_file(),
                    "declare symlinked input directories explicitly"
                );
                total = total.saturating_add(fs::metadata(&resolved)?.len());
                anyhow::ensure!(
                    total <= 2 * 1024 * 1024 * 1024,
                    "declared external input closure exceeds 2 GiB"
                );
                hash_json(&(target.to_string_lossy(), hash_file(&resolved)?))?
            } else {
                total = total.saturating_add(entry.metadata()?.len());
                anyhow::ensure!(
                    total <= 2 * 1024 * 1024 * 1024,
                    "declared external input closure exceeds 2 GiB"
                );
                hash_file(path)?
            };
            identities.insert(path.to_string_lossy().into_owned(), identity);
        }
    }
    // An explicit empty set has a digest. This is not an unidentified input.
    hash_json(&identities)
}
fn inputs(plan: &Plan, obs: &WObservation) -> Result<Inputs> {
    input_read::InputRead::default().read(plan, obs)
}
fn validate(plan: &Plan) -> Result<()> {
    anyhow::ensure!(!withheld(&plan.pack), "{AI_REVIEW_NOT_IN_BUILD}");
    if let Some(port) = plan.local_model_port {
        let registered = if plan.pack == "pr-agent" {
            11436
        } else {
            11435
        };
        anyhow::ensure!(port == registered && plan.container.is_none()
            && ((["aider", "openhands-sdk", "pr-agent", "mini-swe-agent"].contains(&plan.pack.as_str())
                && matches!(plan.criterion.condition, engine::Condition::Advisory))
                || (plan.pack == "megalinter" && matches!(plan.criterion.condition, engine::Condition::NoObservations))),
            "Only an explicitly accepted local advisory recipe may contact the registered loopback model");
    }
    if plan.schema != "thruline.integration-plan.v1" {
        bail!("unsupported plan schema");
    }
    if plan.support.is_some() && plan.support != support(&plan.pack)? {
        bail!("support declaration differs from this adapter version; review a fresh plan");
    }
    if let Some(spec) = &plan.container {
        container::validate(spec)?;
    }
    let c = engine::catalog()?;
    let pack = c
        .projects
        .iter()
        .find(|p| p.id == plan.pack && shipped(&p.id))
        .context("unknown integration pack")?;
    if pack.format.as_deref() != Some(plan.format.as_str())
        && !pack.import_formats.contains(&plan.format)
    {
        bail!("report format does not belong to this pack");
    }
    if matches!(
        &plan.criterion.condition,
        engine::Condition::NoStructuralMatches
    ) && (plan.pack != "ast-grep" || plan.format != "ast-grep-json-v1")
    {
        bail!("no_structural_matches requires the native ast-grep capture route");
    }
    if plan.argv.is_empty() || !Path::new(&plan.argv[0]).is_absolute() {
        bail!("the plan must name an absolute executable path");
    }
    if !(1..=3600).contains(&plan.timeout_seconds)
        || !(1024..=8 * 1024 * 1024).contains(&plan.output_limit_bytes)
    {
        bail!("timeout must be 1–3600 seconds; output limit must be 1 KiB–8 MiB");
    }
    if plan.argv.iter().any(|a| {
        [
            "--fix",
            "--fix-deps",
            "--autofix",
            "--update-all",
            "--delete",
            "--allow-dirty",
            "--in-place",
            "--leak-dirs",
        ]
        .iter()
        .any(|flag| a == flag || a.starts_with(&format!("{flag}=")))
    }) {
        bail!(
            "automatic rewriting is not an integration check; use a separate reviewed correction"
        );
    }
    if !(1..=3600).contains(&plan.cpu_seconds) || !(128..=16384).contains(&plan.memory_mb) {
        bail!("CPU budget must be 1–3600 seconds and memory budget 128–16384 MiB");
    }
    if plan.environment.keys().any(|k| {
        ["TOKEN", "SECRET", "PASSWORD", "API_KEY"]
            .iter()
            .any(|part| k.to_ascii_uppercase().contains(part))
    }) {
        bail!("do not put credentials in a persisted check plan; provision a separate restricted environment");
    }
    if plan
        .environment
        .keys()
        .any(|k| k.is_empty() || k.contains('=') || k.contains('\0'))
    {
        bail!("invalid environment name");
    }
    Ok(())
}
pub fn run(db: &Path, command: Command) -> Result<ExitCode> {
    let value = match command {
        Command::NativeRepair {
            root,
            action,
            recipe,
            digest,
        } => native_repair::action(db, &root, &action, &recipe, &digest)?,
        Command::AgentRepair {
            root,
            action,
            recipe,
            digest,
        } => agent_repair::action(db, &root, &recipe, &action, &digest)?,
        Command::Review {
            root,
            exec_db,
            action,
            mode,
            digest,
            change_id,
            examined_inputs_id,
        } => {
            let value = review::action(
                db,
                &root,
                &exec_db.unwrap_or(thruline_ledger::Ledger::default_path()?),
                &action,
                &mode,
                &digest,
                review::BindingRequest::from_parts(change_id, examined_inputs_id)?,
            )?;
            // T-4-1a-FIX: a person at the terminal reads the plain sentence
            // too, not only a field inside the JSON.
            if let Some(sentence) = value["review_findings"]["sentence"].as_str() {
                eprintln!("{sentence}");
            }
            value
        }
        Command::Repair {
            root,
            action,
            digest,
        } => repair::action(db, &root, &action, &digest)?,
        Command::SemgrepSnapshot { root } => semgrep_mode::snapshot_child(&root)?,
        Command::WorkerExec {
            cpu_seconds,
            memory_mb,
            local_model_port,
            read_path,
            write_path,
            argv,
        } => {
            return worker::exec_limited(
                cpu_seconds,
                memory_mb,
                local_model_port,
                &argv,
                &read_path,
                &write_path,
            )
        }
        Command::Catalog => served_catalog()?,
        Command::Evaluate { cases, results } => {
            let labels = bounded_file(&cases, 8 * 1024 * 1024)?;
            let predictions = bounded_file(&results, 8 * 1024 * 1024)?;
            let prediction_value = engine::parse_json(&predictions)?;
            anyhow::ensure!(prediction_value.get("manifest_sha256").and_then(Value::as_str) == Some(sha256_hex(&labels).as_str()),
                "The predictions were not recorded against this exact oracle manifest; do not relabel a completed evaluation.");
            let label_value = engine::parse_json(&labels)?;
            // AI-REVIEW-OUT-OF-LAUNCH-R6: the evaluation names each case's
            // pack (as a by_adapter key and as cases[].pack), so it is not made
            // for a pack this build does not ship.
            refuse_packs_not_shipped(&[&label_value, &prediction_value])?;
            let mut value = engine::evaluate(&label_value, &prediction_value)?;
            value["oracle_manifest_sha256"] = json!(sha256_hex(&labels));
            value["prediction_report_sha256"] = json!(sha256_hex(&predictions));
            value
        }
        Command::Workflow {
            root,
            exec_db,
            action,
            recipe,
            digest,
        } => match exec_db {
            Some(store) => workflow::action_at(db, &root, &action, &recipe, &digest, &store)?,
            None => workflow::action(db, &root, &action, &recipe, &digest)?,
        },
        Command::Project {
            root,
            exec_db,
            presentation,
        } => {
            let root = root.canonicalize()?;
            let store = exec_db.unwrap_or(thruline_ledger::Ledger::default_path()?);
            let result = results(db, &root);
            let mut view = json!({"schema":"thruline.integration-workspace.v1","session":"local-verification","root":root,
                "workflow":workflow::overview_at(db,&root,&store).unwrap_or_else(|e|json!({"error":e.to_string()})),
                "project_profile":project_profile(&root),"catalog":served_catalog()?,
                "results":result.as_ref().ok(),"results_error":result.err().map(|e|e.to_string())});
            if presentation {
                presentation_view(&mut view);
            }
            view
        }
        Command::Workspace {
            session,
            presentation,
        } => {
            let store =
                thruline_ledger::Ledger::open_read_only(&thruline_ledger::Ledger::default_path()?)?;
            let root:String=store.conn().query_row("SELECT r.root_path FROM sessions s JOIN repos r ON r.id=s.repo_id WHERE s.id=?1",[&session],|row|row.get(0)).context("session has no recorded repository path")?;
            let result = results(db, Path::new(&root));
            let mut view = json!({"schema":"thruline.integration-workspace.v1","session":session,"root":root,"workflow":workflow::overview(db, Path::new(&root)).unwrap_or_else(|e| json!({"error":e.to_string()})),"project_profile":project_profile(Path::new(&root)),"catalog":served_catalog()?,"results":result.as_ref().ok(),"results_error":result.err().map(|e|e.to_string())});
            if presentation {
                presentation_view(&mut view);
            }
            view
        }
        Command::Plan {
            pack,
            root,
            executable,
        } => {
            anyhow::ensure!(!withheld(&pack), "{AI_REVIEW_NOT_IN_BUILD}");
            let c = engine::catalog()?;
            let p = c
                .projects
                .iter()
                .find(|p| p.id == pack && shipped(&p.id))
                .context("unknown integration pack")?;
            let format = p
                .format
                .clone()
                .context("this source is a design reference, not an executable check")?;
            let criterion = Criterion {
                id: format!("{pack}:criterion"),
                revision: "1".into(),
                requirement: "REPLACE with your accepted requirement".into(),
                success: "REPLACE with the observable success criterion".into(),
                accepted_by: "".into(),
                condition: engine::Condition::NoObservations,
            };
            let argv = match pack.as_str() {
                "ast-grep" => vec![
                    executable.display().to_string(),
                    "scan".into(),
                    "--json=compact".into(),
                    "--rule".into(),
                    "REPLACE-rule.yml".into(),
                    ".".into(),
                ],
                "cargo-deny" => vec![
                    executable.display().to_string(),
                    "--offline".into(),
                    "--locked".into(),
                    "--format".into(),
                    "json".into(),
                    "check".into(),
                    "bans".into(),
                    "sources".into(),
                ],
                _ => vec![
                    executable.display().to_string(),
                    "REPLACE-with-explicit-arguments".into(),
                ],
            };
            serde_json::to_value(Plan {
                support: support(&pack)?,
                schema: "thruline.integration-plan.v1".into(),
                pack: pack.clone(),
                format,
                root: root.canonicalize()?,
                argv,
                container: None,
                local_model_port: None,
                writable_paths: vec![],
                criterion,
                acceptance: None,
                approved_setup: None,
                rules: vec![],
                runtime: vec![],
                dependencies: vec![],
                dataset: vec![],
                environment: BTreeMap::new(),
                input_closure_complete: false,
                timeout_seconds: 60,
                cpu_seconds: 30,
                memory_mb: 2048,
                output_limit_bytes: 2 * 1024 * 1024,
                report: if pack == "cargo-deny" {
                    ReportSource::Stderr
                } else {
                    ReportSource::Stdout
                },
            })?
        }
        Command::Run { plan, accept } => {
            if !accept {
                bail!("review the plan, then use --accept to accept its specific criterion and execute its exact argv");
            }
            let value = engine::parse_json(&bounded_file(&plan, 256 * 1024)?)?;
            let p: Plan = serde_json::from_value(value)?;
            validate(&p)?;
            if [
                &p.criterion.id,
                &p.criterion.revision,
                &p.criterion.requirement,
                &p.criterion.success,
                &p.criterion.accepted_by,
            ]
            .iter()
            .any(|s| s.trim().is_empty() || s.contains("REPLACE"))
                || p.argv.iter().any(|s| s.contains("REPLACE"))
            {
                bail!("fill in the requirement, success criterion, accepting person, and exact command before running");
            }
            execute(db, p)?
        }
        Command::Decode { pack, report } => {
            // AI-REVIEW-OUT-OF-LAUNCH-R5: a report of the withheld review is
            // not decoded by a build that does not run it, and this refuses
            // before the report is read.
            anyhow::ensure!(!withheld(&pack), "{AI_REVIEW_NOT_IN_BUILD}");
            let c = engine::catalog()?;
            let p = c
                .projects
                .iter()
                .find(|p| p.id == pack && shipped(&p.id))
                .context("unknown integration pack")?;
            let bytes = bounded_file(&report, 8 * 1024 * 1024)?;
            json!({"contract":engine::CONTRACT,"provenance":"imported","state":"reported","word":"Recorded","raw_report_sha256":sha256_hex(&bytes),"report":engine::decode(p.format.as_deref().context("design reference has no report format")?,&bytes)?})
        }
        Command::Results { root } => results(db, &root)?,
    };
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(ExitCode::SUCCESS)
}
fn project_profile(root: &Path) -> Value {
    let manifests: Vec<&str> = [
        "Cargo.toml",
        "package.json",
        "ui/package.json",
        "pyproject.toml",
        "go.mod",
    ]
    .into_iter()
    .filter(|p| root.join(p).is_file())
    .collect();
    let mut suggested = vec!["ast-grep", "betterleaks"];
    if manifests.contains(&"Cargo.toml") {
        suggested.extend(["cargo-deny", "cargo-mutants"]);
    }
    if manifests.contains(&"package.json") || manifests.contains(&"ui/package.json") {
        suggested.extend(["dependency-cruiser", "knip"]);
    }
    json!({"manifests":manifests,"suggested_packs":suggested,"basis":"Suggestions use manifest presence only. Actual scope and rules still require an accepted plan; nothing is run automatically."})
}

fn artifact_dir(db: &Path) -> PathBuf {
    let absolute = if db.is_absolute() {
        db.to_path_buf()
    } else {
        std::env::current_dir()
            .expect("current directory required for artifact storage")
            .join(db)
    };
    absolute.with_extension("integration-artifacts")
}
fn private_write(dir: &Path, bytes: &[u8]) -> Result<PathBuf> {
    if fs::symlink_metadata(dir).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("artifact directory cannot be a symlink");
    }
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    let path = dir.join(format!("{}.json", sha256_hex(bytes)));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(&path) {
        Ok(mut f) => {
            f.write_all(bytes)?;
            f.sync_all()?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if !fs::symlink_metadata(&path)?.is_file() || fs::read(&path)? != bytes {
                bail!("artifact identity conflict");
            }
        }
        Err(e) => return Err(e.into()),
    }
    Ok(path)
}
fn execute(db: &Path, mut plan: Plan) -> Result<Value> {
    plan.root = plan.root.canonicalize()?;
    workflow::check_acceptance(&plan)?;
    // Ledger admission happens before spawning. No implicit migrations.
    let ledger = Ledger::open(db)?;
    let pre = wsid::observe(&plan.root, Scope::everything())?;
    let (root, repo) = match &pre {
        WObservation::Complete(fp) => (
            fp.doc().root.clone(),
            fp.doc()
                .repo_id
                .clone()
                .context("workspace has no repository identity")?,
        ),
        _ => bail!("workspace observation is incomplete; no tool was started"),
    };
    if let ReportSource::File { path } = &plan.report {
        if path_at(&plan.root, path).exists() {
            bail!("report output already exists; choose a fresh output path so an older report cannot stand in for this run");
        }
    }
    let dir = artifact_dir(db);
    anyhow::ensure!(
        !fs::symlink_metadata(&dir).is_ok_and(|m| m.file_type().is_symlink()),
        "artifact directory cannot be a symlink"
    );
    fs::create_dir_all(&dir)?;
    let mut lock_options = fs::OpenOptions::new();
    lock_options
        .create(true)
        .read(true)
        .write(true)
        .truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        lock_options.mode(0o600);
    }
    let lock = lock_options.open(dir.join("worker.lock"))?;
    lock.try_lock().context(
        "an integration run already owns this store's worker; try again after it completes",
    )?;
    let mut input_reader = input_read::InputRead::default();
    let before = input_reader.read(&plan, &pre)?;
    let source_snapshot = input_reader.suppression_snapshot(&plan)?.clone();
    drop(input_reader);
    let started = thruline_core::ingest::iso8601_utc_now();
    let scope = evb::classify_command(&plan.argv.join(" "), &root);
    let mut prepared =
        if ["aider", "openhands-sdk", "pr-agent", "mini-swe-agent"].contains(&plan.pack.as_str()) {
            anyhow::ensure!(
                plan.local_model_port
                    == Some(if plan.pack == "pr-agent" {
                        11436
                    } else {
                        11435
                    })
                    && matches!(plan.criterion.condition, engine::Condition::Advisory),
                "local agent broker requires the accepted advisory policy"
            );
            worker::prepare_broker(&plan)?
        } else if ["sonarqube", "megalinter"].contains(&plan.pack.as_str()) {
            worker::prepare_broker(&plan)?
        } else {
            worker::prepare(&plan)?
        };
    let mut binding = EvidenceBinding {
        argv: prepared.argv.clone(),
        mode: ExecMode::Direct,
        cwd: root.clone(),
        root,
        repository_identity: repo,
        scope,
        wsid_scope: WsidScopeDecl::everything(),
        pre: IdentityEdge::from_observation(&pre),
        post: IdentityEdge::incomplete("running".into()),
        exit_code: None,
        term_signal: None,
        stdout: evb::StreamCapture::Truncated {
            reason: "running".into(),
        },
        stderr: evb::StreamCapture::Truncated {
            reason: "running".into(),
        },
        artifacts: vec![],
        wrapper_defects: vec![],
        started_at: started.clone(),
        bound_at: started.clone(),
        source_adapter: format!("thruline-integration:{}", plan.pack),
        source_session: format!("integration:{started}"),
    };
    let outcome = worker::execute(&plan, &mut prepared, |pid| {
        evb::record_command_started(&ledger, &binding, pid)
            .map(|_| ())
            .map_err(Into::into)
    })?;
    let completed = thruline_core::ingest::iso8601_utc_now();
    let post = wsid::observe(&plan.root, Scope::everything()).unwrap_or_else(|e| {
        WObservation::Incomplete {
            reason: format!("post-run observation failed: {e}"),
            repo_id: Some(binding.repository_identity.clone()),
            root: binding.root.clone(),
        }
    });
    let after = inputs(&plan, &post).unwrap_or_default();
    let report_bytes = match &plan.report {
        ReportSource::Stdout => Ok(outcome.stdout.bytes.clone()),
        ReportSource::Stderr => Ok(outcome.stderr.bytes.clone()),
        ReportSource::File { path } => {
            bounded_file(&path_at(&plan.root, path), plan.output_limit_bytes)
        }
    };
    let mut report_artifact = None;
    let mut mutation_artifacts = vec![];
    let (report, digest, decode_error) = match report_bytes {
        Ok(bytes) => {
            // Secret scanner payloads are deliberately not retained. The stream
            // digest still commits to them; normalized candidates omit values.
            if plan.pack != "betterleaks" {
                let path = private_write(&artifact_dir(db), &bytes)?;
                report_artifact = Some(ArtifactDigest {
                    path: path.display().to_string(),
                    sha256: Some(sha256_hex(&bytes)),
                });
            }
            match engine::decode(&plan.format, &bytes) {
                Ok(mut r) => {
                    engine::apply_capture_diagnostics(&mut r, &outcome.stderr.bytes);
                    if plan.pack == "cargo-mutants" {
                        if let Some(selection) = plan.environment.get("THRULINE_MUTATION_SELECTION")
                        {
                            r.facts.insert("mutation_scope".into(), json!({"selection":engine::parse_json(selection.as_bytes())?,"accepted_native_argv":plan.argv,"baseline_policy":"run on every shard","coverage_boundary":"Only the accepted package, feature, diff, file and shard selection. Macro expansions and equivalent mutants are not settled; shard completion does not establish all-shard coverage."}));
                        }
                        if let ReportSource::File { path } = &plan.report {
                            let sealed = mutation_artifacts::seal(
                                db,
                                &path_at(&plan.root, path),
                                &bytes,
                                plan.environment.contains_key("THRULINE_MUTATION_SELECTION"),
                            );
                            if !sealed.errors.is_empty() {
                                r.complete = false;
                                r.limitations.extend(sealed.errors);
                            }
                            r.facts
                                .insert("native_artifact_closure".into(), sealed.manifest);
                            mutation_artifacts = sealed.artifacts;
                        }
                    }
                    if plan.pack == "dependency-cruiser" {
                        dependency_modes::annotate_preparation(&plan, &mut r)?;
                    }
                    suppressions::apply(&plan, &mut r, &source_snapshot);
                    (Some(r), sha256_hex(&bytes), None)
                }
                Err(e) => (None, sha256_hex(&bytes), Some(e.to_string())),
            }
        }
        Err(e) => (None, String::new(), Some(e.to_string())),
    };
    drop(source_snapshot);
    let admission = Admission {
        accepted: true,
        captured: true,
        input_closure_complete: plan.input_closure_complete,
        exit_code: outcome.exit_code,
        run_state: outcome.state.clone(),
        before,
        after,
    };
    let isolation_limits = if plan.container.is_some() {
        "The local Docker broker runs outside the workload sandbox. The workload uses the pinned image, no external network, a read-only project, one CPU quota, per-process CPU limits, a cgroup memory limit and bounded temporary storage. RSS samples measure the host broker group, not container memory. SIGKILL/daemon failure can prevent cleanup and must be investigated."
    } else if plan.pack == "sonarqube" {
        "The trusted SonarQube broker communicates with the registered local Docker service. The scanner and server run on a private container network. Captured compute-task status establishes analysis completion; cancelling the broker does not establish termination of a server task. Broker-group RSS does not measure the server container."
    } else if plan.pack == "megalinter" {
        "The trusted MegaLinter broker runs native analyzers in a macOS child sandbox. Its optional Advisor may contact the registered localhost model. Native child policies and captured calls describe this boundary; broker-group RSS is sampled and model output does not establish acceptance."
    } else if plan.local_model_port.is_some() {
        "The trusted local-agent broker runs outside the workload sandbox. Its native agent and independent oracle children apply their own macOS sandbox; the agent may contact only the registered localhost model. CPU limits apply per process and broker-group RSS is sampled every 200 ms. Read the captured child policy and terminal observations; model output cannot establish acceptance."
    } else {
        "CPU limits apply per process. Worker-group RSS is sampled every 200 ms and over-budget groups are terminated; transient peaks between samples may be missed. Network access is denied by the macOS worker sandbox; other platforms require a supported network sandbox."
    };
    let mut record = Record {
        contract: engine::CONTRACT.into(), plan, admission, report, decode_error,
        raw_report_sha256: digest,
        stderr_excerpt: if plan_pack_is_secret(&binding.source_adapter) {
            "Secret scanner diagnostics are withheld; use the normalized locations.".into()
        } else {
            let masked = thruline_core::redact::mask_recorded(&String::from_utf8_lossy(&outcome.stderr.bytes)).0;
            thruline_core::redact::truncate_respecting_markers(&masked, 4000)
        },
        duration_ms: outcome.duration_ms,
        sampled_peak_rss_bytes: outcome.sampled_peak_rss_bytes,
        resource_error: outcome.resource_error,
        isolation: outcome.isolation,
        stdout_bytes: outcome.stdout.total, stderr_bytes: outcome.stderr.total,
        started_at: started, completed_at: completed.clone(),
        limitations: vec![
            "Capture runs as the same OS user as the workload; it does not establish independent or tamper-resistant producer truth.".into(),
            "Foreground worker limits wall time and retained output. Process-group termination is not an OS sandbox; a hostile child can escape its group.".into(),
            isolation_limits.into(),
            "Configured input closure is operator-declared; ignored files and undeclared external tools are not inferred.".into(),
        ],
    };
    let mut bytes = serde_json::to_vec(&record)?;
    if bytes.len() > 8 * 1024 * 1024 {
        // Keep a bounded failure receipt and the original report digest rather
        // than leaving an executed check without a terminal explanation.
        record.report = None;
        record.decode_error = Some("The normalized report exceeded the receipt retention budget. Inspect the original report artifact.".into());
        record.admission.run_state = RunState::OutputLimit;
        bytes = serde_json::to_vec(&record)?;
    }
    let incomplete = Report {
        format: record.plan.format.clone(),
        observations: vec![],
        facts: BTreeMap::new(),
        complete: false,
        limitations: record.decode_error.iter().cloned().collect(),
    };
    let assessment = engine::assess(
        &record.plan.criterion,
        &record.admission,
        record.report.as_ref().unwrap_or(&incomplete),
        &record.admission.after,
    );
    let path = private_write(&artifact_dir(db), &bytes)?;
    binding.post = IdentityEdge::from_observation(&post);
    binding.exit_code = outcome.exit_code.map(i64::from);
    binding.term_signal = outcome.signal.map(i64::from);
    binding.stdout = outcome.stdout.capture();
    binding.stderr = outcome.stderr.capture();
    binding.bound_at = completed;
    if record.admission.run_state != RunState::Completed {
        binding.wrapper_defects.push(format!(
            "integration worker: {:?}",
            record.admission.run_state
        ));
    }
    binding.artifacts.push(ArtifactDigest {
        path: path.display().to_string(),
        sha256: Some(sha256_hex(&bytes)),
    });
    if let Some(artifact) = report_artifact.as_ref() {
        binding.artifacts.push(artifact.clone());
    }
    binding.artifacts.extend(mutation_artifacts);
    let stored = evb::store(&ledger, &binding)?;
    let (_, evidence_event) = evb::record_command_bound(&ledger, stored.stored())?;
    Ok(
        json!({"contract":engine::CONTRACT,"binding_id":stored.stored().id,"evidence_event":evidence_event,"artifact":path,"raw_report_artifact":report_artifact,"record":record,"assessment":assessment}),
    )
}
pub fn results(db: &Path, root: &Path) -> Result<Value> {
    let root = root.canonicalize()?;
    let ledger = Ledger::open(db)?;
    let now = wsid::observe(&root, Scope::everything())?;
    let repo = match &now {
        WObservation::Complete(fp) => fp
            .doc()
            .repo_id
            .clone()
            .context("repository identity unavailable")?,
        _ => bail!("current workspace observation is incomplete"),
    };
    let mut rows = vec![];
    let mut input_read = input_read::InputRead::default();
    for stored in evb::all_for_repo(&ledger, &repo)?
        .into_iter()
        .rev()
        .filter(|s| {
            Path::new(&s.binding.root) == root
                && s.binding
                    .source_adapter
                    .starts_with("thruline-integration:")
        })
        .take(100)
    {
        for a in stored.binding.artifacts.iter().take(1) {
            // Artifact paths come from a verified EVB; still restrict to this
            // store's content-addressed directory before opening any file.
            let path = Path::new(&a.path);
            let permitted = artifact_dir(db).canonicalize()?;
            anyhow::ensure!(
                path.canonicalize()?.parent() == Some(permitted.as_path()),
                "receipt artifact is outside this store"
            );
            let bytes = bounded_file(path, 8 * 1024 * 1024)?;
            if a.sha256.as_deref() != Some(sha256_hex(&bytes).as_str()) {
                bail!("integration receipt artifact was changed");
            }
            for raw in stored.binding.artifacts.iter().skip(1) {
                let p = Path::new(&raw.path);
                anyhow::ensure!(
                    p.canonicalize()?.parent() == Some(permitted.as_path()),
                    "report artifact is outside this store"
                );
                anyhow::ensure!(
                    raw.sha256.as_deref() == Some(hash_file(p)?.as_str()),
                    "original report artifact was changed"
                );
            }
            let record: Record = serde_json::from_value(engine::parse_json(&bytes)?)?;
            anyhow::ensure!(
                record.plan.root == root,
                "Receipt source does not match the selected project root"
            );
            let current = input_read.read(&record.plan, &now).unwrap_or_default();
            let incomplete = Report {
                format: record.plan.format.clone(),
                observations: vec![],
                facts: BTreeMap::new(),
                complete: false,
                limitations: record.decode_error.iter().cloned().collect(),
            };
            let assessment = engine::assess(
                &record.plan.criterion,
                &record.admission,
                record.report.as_ref().unwrap_or(&incomplete),
                &current,
            );
            rows.push(json!({"binding_id":stored.id,"artifact_digest":a.sha256,"execution_receipt":stored.binding,"record":record,"assessment":assessment}));
        }
    }
    Ok(
        json!({"contract":engine::CONTRACT,"root":root,"runs":rows,"limit":100,"coverage":"Latest 100 integration bindings for this repository; each result is re-evaluated on read."}),
    )
}

fn presentation_view(value: &mut Value) {
    fn omit_inline(value: &mut Value, count: &mut usize) {
        match value {
            Value::Object(fields) => {
                if fields.remove("bytes_base64").is_some() {
                    *count += 1;
                }
                for child in fields.values_mut() {
                    omit_inline(child, count);
                }
            }
            Value::Array(items) => {
                for child in items {
                    omit_inline(child, count);
                }
            }
            _ => {}
        }
    }
    let mut count = 0;
    omit_inline(value, &mut count);
    value["presentation"] = json!({"inline_artifact_payloads_omitted":count,"scope":"Only encoded file payloads are omitted from this UI projection. Source paths, hashes, sizes, native responses, diffs and observations remain. Original bytes are preserved in the receipt and native evidence export."});
}

fn plan_pack_is_secret(adapter: &str) -> bool {
    adapter == "thruline-integration:betterleaks"
}

fn platform_identity() -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        hash_json(&(
            hash_file(Path::new(
                "/System/Library/CoreServices/SystemVersion.plist",
            ))?,
            hash_file(Path::new("/usr/bin/sandbox-exec"))?,
        ))
    }
    #[cfg(target_os = "linux")]
    {
        hash_file(Path::new("/proc/sys/kernel/osrelease"))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Ok(String::new())
    }
}

#[cfg(test)]
mod allow_list_tests {
    use super::*;

    /// AI-REVIEW-OUT-OF-LAUNCH-R5: the ids the build ships are the catalog's,
    /// less the one AI-review row, and nothing else. A row added to the catalog
    /// is not shipped until it is added to `SHIPPED_IDS` on purpose, and this
    /// fails until then (`ui/src/workspace/ai-review-allow-list.test.js` holds
    /// the same list equal to the launch UI twin's).
    #[test]
    fn the_shipped_ids_are_the_catalogs_less_the_withheld_source() {
        let catalog: Value = serde_json::from_str(engine::CATALOG).expect("catalog json");
        let ids: Vec<&str> = catalog["projects"]
            .as_array()
            .expect("projects")
            .iter()
            .filter_map(|p| p["id"].as_str())
            .filter(|id| *id != AI_REVIEW_PACK)
            .collect();
        assert_eq!(SHIPPED_IDS.to_vec(), ids, "same ids, same order");
        assert!(!SHIPPED_IDS.contains(&AI_REVIEW_PACK));
        let unique: std::collections::BTreeSet<&str> = SHIPPED_IDS.iter().copied().collect();
        assert_eq!(unique.len(), SHIPPED_IDS.len(), "no id is listed twice");
    }

    #[test]
    fn an_id_is_shipped_only_when_it_is_listed_exactly() {
        if AI_REVIEW {
            assert!(shipped("pr-agent") && shipped("anything at all"));
            return;
        }
        assert!(shipped("ast-grep") && shipped("in-toto"));
        for id in [
            "pr-agent",
            "PR-Agent",
            " ast-grep",
            "ast-grep ",
            "AST-GREP",
            "ast_grep",
            "",
            "\u{1d429}\u{1d42b}-\u{1d41a}\u{1d420}\u{1d41e}\u{1d427}\u{1d42d}",
            "pr-agent-review",
        ] {
            assert!(!shipped(id), "{id:?}");
        }
        assert!(route_recipe_admitted("full") && route_recipe_admitted("change"));
        assert!(!route_recipe_admitted("Full") && !route_recipe_admitted(""));
    }

    #[test]
    fn a_count_is_said_in_the_singular_and_the_plural() {
        assert_eq!(selections_not_run(1), SELECTION_NOT_RUN_ONE);
        assert_eq!(
            selections_not_run(3),
            "3 registered selections are not ones this build runs, so they were left out of this review."
        );
    }
}
