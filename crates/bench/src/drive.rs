//! `thruline bench drive <task>` — one operator-launched task, under the
//! committed protocol, recorded as class 2.
//!
//! # Why a human pastes the prompt, and why that is the point rather than a gap
//!
//! Every run this crate has measured so far was **scripted**: Claude Code
//! against a local fake endpoint replaying a canned tool-call plan whose entire
//! body is one Bash call to the task's own test script. That configuration
//! costs $0 and it measures the plumbing honestly, but it cannot measure an
//! agent, because there is no agent in it. `drive` is the other half: a real
//! agent, launched by a person, with the run's terms fixed **before** they
//! launch it.
//!
//! The operator is in the loop on purpose. A run this crate started itself
//! would be class 2 by construction and worth very little — the thing that
//! makes an operator-launched run evidence is that a human, holding the
//! credentials, chose to spend them on a task and a prompt they did not pick
//! after seeing the result. So this module **prepares and refuses**; it does
//! not launch. It hands back a prompt to paste, exactly once, and the prompt is
//! part of the hash.
//!
//! # The prompt is in the commitment, which is the whole mechanism
//!
//! [`OPERATOR_PROMPT`] is a constant, and [`prepare`] writes it and its SHA-256
//! into the protocol's `parameters` before [`Protocol::commit`] hashes them. A
//! prompt outside the commitment is a dial: the operator runs the task, reads
//! the trajectory, rewords the instruction and runs it again, and every
//! individual run is honest while the sequence is fitted. Inside the
//! commitment, a reworded prompt is a **different protocol hash**, and
//! [`crate::class_two::ingest`] will not let its results be counted under the
//! old one.
//!
//! # What this refuses, and why the refusals are the feature
//!
//! * A task that is not on the **tune** side of the committed split. The
//!   holdout was breached once already — `ancient-puzzle`, 27 minutes after the
//!   split fixture was committed — by a runner that took its task id from an
//!   environment variable and never looked at the file. This one looks.
//! * A corpus whose task ids no longer match the committed split.
//! * A second spend of the same protocol's holdout, because the only way to get
//!   a [`RunManifest`] is [`HoldoutLedger::spend`] and obtaining one records it.
//! * A workspace with no bindable git tree — recorded as
//!   [`TerminalState::NoBindableTree`] rather than as a result, because a
//!   verdict bound to nothing is not a verdict.
//!
//! # This is a SECOND runner, and that is a hazard the census names
//!
//! `scripts/pilot-run.sh` enforces the split with a shell guard.
//! `crates/bench/tests/split_enforcement_census.rs` excuses six Rust symbols
//! from needing a caller precisely because no Rust entry point stages and runs
//! a task, and it warns that a second runner "would inherit none of it". This
//! module is that second runner, so it does the enforcement itself, in
//! [`prepare`], through the same symbols — and the census entries are updated
//! to say so rather than left describing a world where this file does not
//! exist.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::class_two::{is_tree_hash, TaskResult, TerminalState};
use crate::pilot::{check_from_tune, Harness, TerminalBenchSplit};
use crate::protocol::{Commitment, HoldoutLedger, Protocol, Provenance, RunManifest};

/// **The fixed prompt, pasted once, hashed into the commitment.**
///
/// Deliberately says nothing about how to solve anything: the task's own
/// `task.yaml` instruction is what the agent is being measured on, and a prompt
/// that adds hints measures the prompt. What it does say is the part an
/// operator would otherwise improvise differently every time — where the work
/// happens, and that the session must not be abandoned half-way, because a
/// truncated session produces a trajectory that reads as a failed task.
pub const OPERATOR_PROMPT: &str = "\
You are working in this container's task workspace. Read the task description \
that is already present and complete the work it asks for. Work only inside the \
workspace directory. When you believe the task is complete, stop — do not \
summarise, do not ask what to do next, and do not exit before the work is \
written to disk.";

/// **The instrument, pre-registered with the corpus.**
///
/// [`Protocol::commit`] refuses an empty `detectors` map, and the refusal is the
/// point rather than a nuisance: pre-registering which tasks will run while
/// leaving the detectors free is the loophole that makes the whole protocol
/// decorative — run, look at the holdout, turn a threshold, report. Committing
/// these means a later run with a different detector set is a different protocol
/// hash and its results cannot be counted under this one.
///
/// **This list is short because the crate's detector set is short, and naming
/// one that does not exist would be worse than naming none.** `narrowed_run` is
/// the detector `report::ablation_table` actually ablates. When more ship, they
/// are added here, and adding one is deliberately a protocol change.
pub const DRIVE_DETECTORS: &[(&str, &str)] = &[("narrowed_run", "enabled")];

/// **What the staged workspace must NOT contain, and why each one.**
///
/// A Terminal-Bench task directory is not a workspace. It is the task's whole
/// definition — content, build, grader and reference solution in one directory —
/// and the harness separates them: the Dockerfile puts *content* in the
/// workspace, `tests/` is mounted at `$TEST_DIR` only after the agent stops, and
/// the solution is the oracle's and never enters an agent's container at all.
///
/// `drive` copied the directory whole. So the agent it measured could read the
/// grader it would be graded by and the reference solution to the task it was
/// set — and any number taken from such a session measures nothing. This was
/// found before any session ran, which is the only reason it is a defect and not
/// a retraction.
///
/// **The names are withheld at every depth**, not just the top level, and
/// [`prepare`] re-walks the staged tree afterwards and refuses
/// [`DriveError::OracleLeaked`] if one survived. A filter that skipped a name
/// and a workspace that does not contain it are different claims.
pub const WITHHELD: &[(&str, &str)] = &[
    ("solution.sh", "the reference solution"),
    ("solution.yaml", "the reference solution"),
    (
        "tests",
        "the grader, which the harness mounts only after the agent stops",
    ),
    (
        "run-tests.sh",
        "the oracle invocation, which names the grader",
    ),
    (
        "Dockerfile",
        "the build definition; it also states the task's premise",
    ),
    ("docker-compose.yaml", "the run definition"),
];

/// What the operator asked for.
#[derive(Debug, Clone)]
pub struct DriveRequest {
    /// The Terminal-Bench task id.
    pub task: String,
    /// The pinned corpus (`.../original-tasks`).
    pub corpus: PathBuf,
    /// Where the prepared workspace is written.
    pub root: PathBuf,
    /// This run's id. Appears in the holdout ledger.
    pub run_id: String,
    pub harness: Harness,
    /// The agent under test, at a version. Hashed, so "which model" cannot be
    /// decided after the result.
    pub model: String,
    /// **Who launched this run.** Hashed with everything else.
    ///
    /// `drive` used to write the literal `"operator"` here, which was true of
    /// the only launcher it imagined and false the moment anyone else ran it
    /// under the operator's standing authority. A record that says the operator
    /// launched a session the operator did not launch is exactly the kind of
    /// fact this crate exists to refuse, and it would have been invisible —
    /// nothing downstream could have contradicted it.
    ///
    /// **It is hashed, so it is not a label.** Two runs of the same task with
    /// different launchers are different protocols and their results cannot be
    /// pooled. That is the intended consequence: who pressed the key is part of
    /// what a class-2 corpus is claiming.
    pub launched_by: String,
    /// The append-only holdout ledger.
    pub ledger: PathBuf,
}

/// Why a drive refused. **Every variant is a refusal to produce a number**, not
/// a warning beside one.
#[derive(Debug)]
pub enum DriveError {
    /// The committed split did not parse or does not describe this corpus.
    Split(crate::pilot::SplitError),
    /// The task is not on the tune side. Named, with the side it is on.
    NotFromTune(crate::pilot::PilotError),
    /// The task id is not in the corpus at all.
    UnknownTask {
        id: String,
        corpus: PathBuf,
    },
    /// A protocol that could not be committed — see [`crate::protocol::ClassError`].
    Uncommittable(crate::protocol::ClassError),
    /// This protocol's holdout has been spent before, by these runs.
    AlreadySpent {
        protocol_hash: String,
        prior: Vec<String>,
    },
    /// No launcher was named. Refused rather than defaulted: a default here
    /// would put a name in the record that nobody chose.
    NoLauncher,
    /// A withheld name survived into the staged workspace. **The run is void**,
    /// not degraded: an agent that can read the grader is not being measured.
    OracleLeaked {
        paths: Vec<String>,
    },
    Io(std::io::Error),
}

impl std::fmt::Display for DriveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriveError::Split(e) => write!(f, "the committed split refused: {e}"),
            DriveError::NotFromTune(e) => write!(f, "{e}"),
            DriveError::UnknownTask { id, corpus } => write!(
                f,
                "no task `{id}` in {}. The corpus is the pinned Terminal-Bench \
                 archive, not a directory of whatever is lying around.",
                corpus.display()
            ),
            DriveError::Uncommittable(e) => write!(f, "the protocol will not commit: {e}"),
            DriveError::AlreadySpent {
                protocol_hash,
                prior,
            } => write!(
                f,
                "this protocol's holdout ({protocol_hash}) was already spent by \
                 {}. A holdout consumed twice is a validation set wearing a \
                 holdout's name, and the second number is fitted to it.",
                prior.join(", ")
            ),
            DriveError::NoLauncher => write!(
                f,
                "no launcher named. `launched_by` records WHO ran this session \
                 and it is hashed into the protocol, so it cannot be defaulted: \
                 a default would put a name in the record that nobody chose. \
                 Pass the principal that actually pressed the key — `operator`, \
                 `coordinator`, or whoever it was."
            ),
            DriveError::OracleLeaked { paths } => write!(
                f,
                "the staged workspace still contains {}. The agent would be able \
                 to read the grader it is graded by, or the reference solution to \
                 the task it is set, and a number taken from that session measures \
                 nothing. The workspace has been removed. This is a bug in \
                 `stage_workspace`, not something a caller can pass differently.",
                paths.join(", ")
            ),
            DriveError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for DriveError {}

impl From<std::io::Error> for DriveError {
    fn from(e: std::io::Error) -> Self {
        DriveError::Io(e)
    }
}

/// A prepared, uncommenced run. **Holds the commitment, so what happens next
/// can be checked against what was promised.**
#[derive(Debug)]
pub struct Prepared {
    /// The staged workspace the operator points the agent at.
    pub workspace: PathBuf,
    /// The exact text to paste, once.
    pub prompt: &'static str,
    /// The commitment every result must carry.
    pub commitment: Commitment,
    /// The manifest returned by the ledger — obtaining it recorded the spend.
    pub manifest: RunManifest,
    /// **Class 2, carrying its commitment.** Not a label: the variant holds the
    /// [`Commitment`], so a corpus cannot be moved into class 2 by editing a
    /// word. Built inside [`prepare`], after the committed split, the corpus
    /// directory and the holdout ledger have all been read — which is the rule
    /// `tests/construction_census.rs` enforces, and the reason `drive.rs` is
    /// named in its PROVENANCE_SITES rather than slipping past its classifier.
    pub provenance: Provenance,
    pub task: String,
    pub run_id: String,
    /// Who launched it, echoed from the request so the brief does not have to
    /// restate a constant. It is already hashed into `commitment`.
    pub launched_by: String,
}

impl Prepared {
    /// What an operator is shown. Kept here rather than in the CLI so the
    /// wording is testable and cannot drift per call site.
    pub fn operator_brief(&self) -> String {
        format!(
            "task        : {}\n\
             run id      : {}\n\
             workspace   : {}\n\
             protocol    : {}\n\
             provenance  : class 2, {}-launched\n\
             \n\
             Paste this prompt to the agent ONCE, then let it work:\n\
             \n{}\n\n\
             The prompt above is hashed into the protocol. Rewording it is a \
             different protocol and its results cannot be counted under this \
             one.\n",
            self.task,
            self.run_id,
            self.workspace.display(),
            self.commitment.hash(),
            self.launched_by,
            self.prompt,
        )
    }
}

/// The protocol a drive commits to, before anything runs.
///
/// Separate from [`prepare`] so a caller — and a test — can see exactly what is
/// hashed without staging a workspace or spending a ledger entry.
pub fn protocol_for(req: &DriveRequest, split: &TerminalBenchSplit) -> Protocol {
    let mut parameters = BTreeMap::new();
    parameters.insert("launch".to_string(), req.launched_by.clone());
    parameters.insert("prompt".to_string(), OPERATOR_PROMPT.to_string());
    // The digest as well as the text. The text is what makes the run
    // reproducible; the digest is what a reader can compare against a
    // transcript without diffing a paragraph by eye.
    parameters.insert("prompt_sha256".to_string(), prompt_digest());
    parameters.insert("harness".to_string(), req.harness.as_str().to_string());
    // WHAT THE AGENT COULD NOT SEE IS PART OF WHAT THE RUN MEANS. Two sessions
    // that differ in whether the grader was readable are not comparable, so the
    // withheld set is hashed rather than left as a property of whichever build
    // happened to run.
    parameters.insert(
        "withheld".to_string(),
        WITHHELD
            .iter()
            .map(|(n, _)| *n)
            .collect::<Vec<_>>()
            .join(","),
    );

    let base = Protocol {
        harness: req.harness.as_str().to_string(),
        harness_version: "terminal-bench-1-d28711d0".to_string(),
        model: req.model.clone(),
        tasks: Vec::new(),
        holdout: Vec::new(),
        parameters,
        detectors: DRIVE_DETECTORS
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        preconditions: crate::protocol::Preconditions::default(),
    };
    // THE SPLIT SEEDS THE PROTOCOL, not the caller. `tune_protocol` is the only
    // sanctioned way to do it, and it exists so a sampler cannot reach a
    // holdout id by construction rather than by care.
    let mut p = split.tune_protocol(&base);
    p.parameters = base.parameters.clone();
    p
}

/// Stage one task and commit to the terms, refusing before anything is spent.
///
/// **Order matters and is not incidental.** Every refusal that can be made from
/// the committed files is made before the ledger is touched, so a refused drive
/// costs no holdout entry. The spend is the last thing that happens, because
/// the spend is the irreversible thing.
/// The commitment for a request, **derived without staging or spending**.
///
/// [`prepare`] calls this and then stages and spends; the `record` example calls
/// it alone, because recording a finished session must not append a second line
/// to the holdout ledger for a run that was already spent. Every refusal that
/// does not depend on the filesystem being written happens here, so a caller who
/// only wants the hash still gets the holdout check.
///
/// **It is deterministic.** The same request yields the same commitment, which
/// is what lets a launcher re-derive the hash after the fact and compare it to
/// the one their brief printed, instead of retyping a hash and hoping.
pub fn commitment_for(req: &DriveRequest) -> Result<Commitment, DriveError> {
    if req.launched_by.trim().is_empty() {
        return Err(DriveError::NoLauncher);
    }
    let split = TerminalBenchSplit::load().map_err(DriveError::Split)?;
    split
        .check_against_corpus(&req.corpus)
        .map_err(DriveError::Split)?;
    if !req.corpus.join(&req.task).is_dir() {
        return Err(DriveError::UnknownTask {
            id: req.task.clone(),
            corpus: req.corpus.clone(),
        });
    }
    let base = protocol_for(req, &split);
    let mut one = base.clone();
    one.tasks = vec![req.task.clone()];
    one.holdout = vec![req.task.clone()];
    check_from_tune(&one, &base).map_err(DriveError::NotFromTune)?;
    one.commit().map_err(DriveError::Uncommittable)
}

pub fn prepare(req: &DriveRequest) -> Result<Prepared, DriveError> {
    // Every refusal that can be made from the committed files, made before the
    // filesystem is touched and long before the ledger is.
    let commitment = commitment_for(req)?;
    let task_dir = req.corpus.join(&req.task);

    // 5. Stage the workspace. COPIED, never referenced in place: the pinned
    //    archive is the corpus and a run that edits it has changed the
    //    measuring stick.
    let workspace = req.root.join(&req.run_id).join(&req.task);
    if workspace.exists() {
        std::fs::remove_dir_all(&workspace)?;
    }
    std::fs::create_dir_all(&workspace)?;
    stage_workspace(&task_dir, &workspace)?;

    // THE POST-CONDITION. `stage_workspace` skipping a name and the workspace
    // not containing it are different claims; this checks the second.
    let leaked = leaked_oracle(&workspace);
    if !leaked.is_empty() {
        std::fs::remove_dir_all(&workspace).ok();
        return Err(DriveError::OracleLeaked { paths: leaked });
    }

    // 6. The spend, last. Obtaining a RunManifest IS the spend — there is no
    //    other constructor — and a second one for the same protocol comes back
    //    carrying its priors.
    let mut ledger = HoldoutLedger::open(&req.ledger)?;
    let manifest = ledger.spend(commitment.hash(), &req.run_id)?;
    if !manifest.prior_runs().is_empty() {
        return Err(DriveError::AlreadySpent {
            protocol_hash: commitment.hash().to_string(),
            prior: manifest.prior_runs().to_vec(),
        });
    }

    // The class is constructed HERE, where the evidence for it was read: the
    // committed split (from its own bytes), the corpus on disk, and the
    // append-only ledger. Written as a plain construction on purpose — the
    // census's classifier ignores a line that begins with `let`, and reaching
    // for that would be a bypass rather than a justification.
    let provenance = build_provenance(&commitment);

    Ok(Prepared {
        workspace,
        prompt: OPERATOR_PROMPT,
        commitment,
        provenance,
        manifest,
        task: req.task.clone(),
        run_id: req.run_id.clone(),
        launched_by: req.launched_by.clone(),
    })
}

/// What the operator's session actually did, as the drive observed it.
#[derive(Debug, Clone)]
pub struct Observed {
    pub state: TerminalState,
    pub oracle_exit: Option<i32>,
    pub duration_ms: u64,
    pub executed_checks: usize,
    /// The workspace tree before the agent's first action, if there was one.
    pub tree_before_first_action: Option<String>,
}

/// Turn an observed session into the row a class-2 corpus can carry.
///
/// **The precondition is enforced here and not trusted from the caller.** A
/// `tree_before_first_action` that is not shaped like a tree hash is not a
/// precondition met by a different spelling — it is a precondition failed, and
/// the row becomes [`TerminalState::NoBindableTree`] with no verdict and no
/// checks, whatever the session reported. `"ok"` must not satisfy this.
pub fn record(prepared: &Prepared, observed: &Observed) -> TaskResult {
    record_under(&prepared.task, &prepared.commitment, observed)
}

/// [`record`] for a launcher who no longer holds the [`Prepared`].
///
/// A session is prepared in one process and recorded in another with a human in
/// between, and `Prepared` cannot survive that gap by design — it carries the
/// [`RunManifest`], and the only way to obtain one is to spend the ledger.
/// Re-preparing to record would append a second spend for a run already made.
///
/// So the commitment is **re-derived** by [`commitment_for`] rather than
/// reconstructed or retyped, and this takes it directly.
pub fn record_under(task: &str, commitment: &Commitment, observed: &Observed) -> TaskResult {
    let bindable = observed
        .tree_before_first_action
        .as_deref()
        .filter(|t| is_tree_hash(t));

    if bindable.is_none() {
        return TaskResult {
            task_id: task.to_string(),
            state: TerminalState::NoBindableTree,
            oracle_exit: observed.oracle_exit,
            verdict: None,
            shape: None,
            duration_ms: observed.duration_ms,
            detectors: Vec::new(),
            impossible_type: None,
            executed_checks: 0,
            // ITS OWN hash, never the commitment's by reference. A row that
            // borrows the commitment's hash cannot disagree with it, and the
            // whole discipline is that the two are compared.
            protocol_hash: commitment.hash().to_string(),
            tree_before_first_action: None,
        };
    }

    TaskResult {
        task_id: task.to_string(),
        state: observed.state.clone(),
        oracle_exit: observed.oracle_exit,
        verdict: None,
        shape: Some(task.to_string()),
        duration_ms: observed.duration_ms,
        detectors: Vec::new(),
        impossible_type: None,
        executed_checks: observed.executed_checks,
        protocol_hash: commitment.hash().to_string(),
        tree_before_first_action: bindable.map(str::to_string),
    }
}

/// SHA-256 of the fixed prompt, lowercase hex.
///
/// `sha2` directly rather than `thruline_core::chain`: this crate takes
/// `thruline-core` as a DEV dependency only, deliberately, so that the engine's
/// crates do not enter a bench build. A convenience import here would reverse
/// that on the shipped side for one hash.
pub fn prompt_digest() -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(OPERATOR_PROMPT.as_bytes());
    let mut hex = String::new();
    for byte in h.finalize() {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

/// The class-2 value, at the one call site that has read the artefacts.
fn build_provenance(commitment: &Commitment) -> Provenance {
    Provenance::GeneratedUnderProtocol(commitment.clone())
}

/// Recursive copy that **withholds the oracle**. See [`WITHHELD`].
fn stage_workspace(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if WITHHELD
            .iter()
            .any(|(w, _)| std::ffi::OsStr::new(w) == name)
        {
            continue;
        }
        let src = entry.path();
        let dst = to.join(&name);
        if src.is_dir() {
            stage_workspace(&src, &dst)?;
        } else {
            std::fs::copy(&src, &dst)?;
        }
    }
    Ok(())
}

/// Every withheld name still present under `root`, relative to it.
///
/// A post-condition, not a second filter. `stage_workspace` skipping a name and
/// the workspace not containing it are different claims, and only the second is
/// the one that matters.
fn leaked_oracle(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if WITHHELD
                .iter()
                .any(|(w, _)| std::ffi::OsStr::new(w) == e.file_name())
            {
                out.push(
                    p.strip_prefix(base)
                        .unwrap_or(&p)
                        .to_string_lossy()
                        .into_owned(),
                );
            }
            if p.is_dir() {
                walk(&p, base, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}
