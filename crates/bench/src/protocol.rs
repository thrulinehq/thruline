//! §12's provenance classes, and the commitment that makes class 2 admissible.
//!
//! # The impasse this resolves
//!
//! §12's targets need labelled trajectories. No source publishes them — they
//! are produced by running a benchmark harness against a model. But a corpus
//! the measured party generated is exactly what a self-authorship refusal
//! exists to reject, so the only route to data was the route the safeguard
//! forbade.
//!
//! The operator's amendment breaks that without weakening the safeguard: a
//! self-run corpus is admissible **only under pre-registration**. The run's
//! parameters — task ids, harness and model versions, prompts, seeds, exact
//! commands, and the holdout — are serialised, hashed, and recorded **before
//! the first trajectory exists**. After that, neither the task selection, nor
//! the labels, nor the inclusion decision can move in response to results.
//!
//! That is the whole mechanism. Everything else here exists to stop it being a
//! promise:
//!
//! - the commitment is a **hash over canonical bytes**, so "the protocol we ran
//!   under" and "the protocol we committed" are comparable rather than
//!   asserted;
//! - a class-2 measurement **carries** the hash of the protocol it ran under
//!   and is refused when the two differ;
//! - the corpus must **account for every task the protocol names** — a run that
//!   declared N tasks produces N trajectories, and a crashed one is recorded as
//!   a trajectory rather than omitted;
//! - labels come from the benchmark's oracle and **nowhere else**, with no
//!   default for the ones it did not produce.
//!
//! # Why the classes carry their evidence
//!
//! The previous `Provenance` was a plain field, so moving a corpus between
//! classes was a one-token edit. That was tolerable while the two classes were
//! "counts" and "does not count", because the edit was a visible lie in a
//! committed line. It is **not** tolerable now: classes 1 and 2 both count
//! toward the targets, so an edit between them is a lie that changes which
//! discipline a number claims to have been produced under.
//!
//! So each class holds the evidence its claim implies — a third-party label
//! source for class 1, a protocol commitment for class 2 — and neither is
//! constructible without it. Class 3 needs no evidence, because it claims
//! nothing.

use std::collections::BTreeMap;

/// Where a corpus came from, and therefore what a number measured on it means.
///
/// **Not a label. Each variant carries the evidence its class implies**, so a
/// corpus cannot be moved between classes by editing a word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provenance {
    /// **Class 1.** Third-party trajectories with third-party labels.
    ExternalLabelled(ExternalSource),
    /// **Class 2.** A third-party harness, agent and task set, run here under a
    /// protocol committed and hashed before the first trajectory.
    GeneratedUnderProtocol(Commitment),
    /// **Class 3.** Hand-written fixtures. Never counted toward §12.
    SelfAuthored,
}

impl Provenance {
    /// Does §12's target apply to a corpus of this class?
    pub fn counts_toward_targets(&self) -> bool {
        !matches!(self, Provenance::SelfAuthored)
    }

    pub fn class(&self) -> u8 {
        match self {
            Provenance::ExternalLabelled(_) => 1,
            Provenance::GeneratedUnderProtocol(_) => 2,
            Provenance::SelfAuthored => 3,
        }
    }
}

/// Class 1's evidence: a published dataset, and the field its labels came from.
///
/// Fields are private and the constructor is fallible, so "this is third-party
/// labelled" cannot be asserted without saying **whose** and **from which
/// field**. A reviewer who cannot resolve the four values cannot check the
/// claim, and a claim nobody can check is the thing this crate is against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalSource {
    publisher: String,
    dataset: String,
    version: String,
    /// The field in the published record that carries the ground truth. Named
    /// because "it has labels" is not checkable and "labels are in
    /// `resolved_status`" is.
    label_field: String,
}

/// Why a class could not be constructed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassError {
    /// A required piece of the class's evidence is missing.
    MissingEvidence { class: u8, field: &'static str },
    /// A protocol was committed with nothing in it to commit to.
    EmptyProtocol { field: &'static str },
    /// A protocol that does not require a bindable workspace.
    PreconditionNotRequired { field: &'static str },
    /// The holdout is not a subset of the tasks the protocol declares.
    HoldoutNotInTasks { stray: Vec<String> },
}

impl std::fmt::Display for ClassError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClassError::MissingEvidence { class, field } => write!(
                f,
                "class {class} requires `{field}`, and a class that cannot show \
                 its evidence is a class nobody can check"
            ),
            ClassError::EmptyProtocol { field } => write!(
                f,
                "a protocol commitment with no `{field}` commits to nothing; the \
                 point of hashing it before the run is that it constrains the run"
            ),
            ClassError::PreconditionNotRequired { field } => write!(
                f,
                "this protocol does not require `{field}`. §6.1 binds every check \
                 to a git tree hash, and a task workspace with no tree produces \
                 `unverifiable` for everything in it — a fully pre-registered run \
                 whose every result is uncountable, discovered after the holdout \
                 is already spent. The precondition belongs in the commitment, \
                 not in a runbook"
            ),
            ClassError::HoldoutNotInTasks { stray } => write!(
                f,
                "the holdout names {} task(s) the protocol does not declare \
                 ({}); a holdout outside the task set is a holdout chosen \
                 somewhere other than the protocol",
                stray.len(),
                stray.join(", ")
            ),
        }
    }
}

impl std::error::Error for ClassError {}

impl ExternalSource {
    pub fn new(
        publisher: &str,
        dataset: &str,
        version: &str,
        label_field: &str,
    ) -> Result<ExternalSource, ClassError> {
        for (value, field) in [
            (publisher, "publisher"),
            (dataset, "dataset"),
            (version, "version"),
            (label_field, "label_field"),
        ] {
            if value.trim().is_empty() {
                return Err(ClassError::MissingEvidence { class: 1, field });
            }
        }
        Ok(ExternalSource {
            publisher: publisher.trim().to_string(),
            dataset: dataset.trim().to_string(),
            version: version.trim().to_string(),
            label_field: label_field.trim().to_string(),
        })
    }

    pub fn publisher(&self) -> &str {
        &self.publisher
    }
    pub fn dataset(&self) -> &str {
        &self.dataset
    }
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn label_field(&self) -> &str {
        &self.label_field
    }
}

/// **What must be true of a task workspace before the agent's first action.**
///
/// # The precondition that would otherwise have wasted an entire run
///
/// §6.1 binds every check to a **git tree hash**. Terminal-Bench tasks are
/// Docker workspaces and have no reason to be git repositories at all — so an
/// agent can run, a check can execute, `run-tests.sh` can pass, and every
/// verdict is still `unverifiable`, because there was no tree to bind to. That
/// is a technically valid, fully pre-registered run whose every result is
/// uncountable, discovered only after the holdout is spent.
///
/// So the precondition lives **in the commitment**, not in a runbook. It is
/// hashed with everything else, [`Protocol::commit`] refuses a protocol that
/// does not require it, and a task whose workspace has no tree is recorded as a
/// precondition failure rather than producing a result nobody can count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preconditions {
    /// `git init` in every task workspace before the agent starts.
    pub workspace_git_init: bool,
    /// And a tree hash must exist **before the agent's first action**, asserted
    /// rather than assumed: `git init` alone leaves an unborn repository with
    /// no tree at all.
    pub tree_hash_before_first_action: bool,
}

impl Default for Preconditions {
    /// The only setting `commit()` accepts. A `Default` that produced an
    /// uncommittable protocol would be a trap.
    fn default() -> Preconditions {
        Preconditions {
            workspace_git_init: true,
            tree_hash_before_first_action: true,
        }
    }
}

/// The run parameters, as they stood **before** the first trajectory.
///
/// Everything a later run could be tempted to adjust in response to results
/// lives here, because the commitment is a hash of exactly these bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Protocol {
    /// The benchmark harness, at a version.
    pub harness: String,
    pub harness_version: String,
    /// The agent under test, at a version.
    pub model: String,
    /// Every task the run will attempt. **The accounting denominator**: a run
    /// that declares these must produce a trajectory for each.
    pub tasks: Vec<String>,
    /// The subset the targets are computed over. A subset of `tasks`, fixed
    /// here so it cannot be chosen after seeing results.
    pub holdout: Vec<String>,
    /// The exact commands, prompts and seeds. Free-form because harnesses
    /// differ; hashed because it is what makes the run reproducible.
    pub parameters: BTreeMap<String, String>,
    /// **The measuring instrument, pre-registered with the corpus.**
    ///
    /// Enabled detectors, their thresholds, and the severity mapping. Without
    /// this the corpus is pre-registered and the instrument is not, and every
    /// constant this build has moved — the assertion-shape classifier, the
    /// rename threshold, `MIN_HONEST_SHAPES`, `MAX_SHAPE_CONCENTRATION` — is a
    /// dial that could be turned after seeing the holdout. Pre-registering the
    /// data while leaving the detector free is the loophole that would make the
    /// whole protocol decorative.
    pub detectors: BTreeMap<String, String>,
    /// **Bindability, pre-registered.** See [`Preconditions`].
    pub preconditions: Preconditions,
}

impl Protocol {
    /// Hash it, and get back the thing a class-2 corpus must carry.
    ///
    /// **Call this before the first trajectory exists.** Nothing here can
    /// enforce *when* it was called — that is a property of the process, not of
    /// a type — but everything downstream can enforce that what ran matches
    /// what was committed, which is what removes the freedom to adjust.
    pub fn commit(&self) -> Result<Commitment, ClassError> {
        if self.harness.trim().is_empty() {
            return Err(ClassError::EmptyProtocol { field: "harness" });
        }
        if self.model.trim().is_empty() {
            return Err(ClassError::EmptyProtocol { field: "model" });
        }
        if self.tasks.is_empty() {
            return Err(ClassError::EmptyProtocol { field: "tasks" });
        }
        if self.holdout.is_empty() {
            return Err(ClassError::EmptyProtocol { field: "holdout" });
        }
        if self.detectors.is_empty() {
            return Err(ClassError::EmptyProtocol { field: "detectors" });
        }
        // A run that cannot bind a check to a tree yields `unverifiable` for
        // every task, and the holdout is spent finding that out. Refused here
        // rather than warned about.
        if !self.preconditions.workspace_git_init {
            return Err(ClassError::PreconditionNotRequired {
                field: "workspace_git_init",
            });
        }
        if !self.preconditions.tree_hash_before_first_action {
            return Err(ClassError::PreconditionNotRequired {
                field: "tree_hash_before_first_action",
            });
        }
        let declared: std::collections::BTreeSet<&String> = self.tasks.iter().collect();
        let stray: Vec<String> = self
            .holdout
            .iter()
            .filter(|t| !declared.contains(t))
            .cloned()
            .collect();
        if !stray.is_empty() {
            return Err(ClassError::HoldoutNotInTasks { stray });
        }
        Ok(Commitment {
            hash: self.hash(),
            tasks: self.tasks.clone(),
            holdout: self.holdout.clone(),
        })
    }

    /// **Derives the holdout from the protocol's own hash**, in two stages.
    ///
    /// The coordinator's instruction is that the holdout comes from the
    /// protocol hash. Taken literally that is circular — the holdout is part of
    /// what is hashed — so it is done in two stages and the seam is named
    /// rather than hidden:
    ///
    /// 1. hash everything **except** the holdout, which is fixed the moment the
    ///    tasks, harness, model, parameters and detectors are;
    /// 2. use that as the salt for a nested split;
    /// 3. commit the whole protocol, holdout included.
    ///
    /// Nothing here is chosen. Anyone with the protocol recomputes the same
    /// split, and changing any field to move the holdout also moves the
    /// committed hash — which is exactly the edit the commitment exists to make
    /// visible.
    ///
    /// The split is **nested**: a task in the holdout at 20% is in the holdout
    /// at 30%, so a fraction raised after seeing results cannot quietly become
    /// a different sample wearing the same rule.
    pub fn derive_holdout(&mut self, fraction: f64) -> Result<(), ClassError> {
        if !(fraction > 0.0 && fraction < 1.0) {
            return Err(ClassError::EmptyProtocol { field: "holdout" });
        }
        let salt = self.hash_without_holdout();
        let cut = (fraction * (u64::MAX as f64)) as u64;
        let mut tasks = self.tasks.clone();
        tasks.sort();
        tasks.dedup();
        self.holdout = tasks
            .into_iter()
            .filter(|t| bucket(&salt, t) < cut)
            .collect();
        if self.holdout.is_empty() {
            return Err(ClassError::EmptyProtocol { field: "holdout" });
        }
        Ok(())
    }

    /// The salt for [`Protocol::derive_holdout`]: every committed field except
    /// the holdout itself.
    pub fn hash_without_holdout(&self) -> String {
        let mut probe = self.clone();
        probe.holdout = Vec::new();
        probe.hash()
    }

    /// `sha256` over canonical bytes.
    ///
    /// Canonical, not `Debug`: two protocols that differ only in map iteration
    /// order or list order are the same protocol, and a hash that said
    /// otherwise would refuse honest re-runs. Lists are sorted and deduplicated
    /// before hashing for the same reason — and the sort is why a task added
    /// after the fact changes the hash, which is the point.
    pub fn hash(&self) -> String {
        let mut tasks = self.tasks.clone();
        tasks.sort();
        tasks.dedup();
        let mut holdout = self.holdout.clone();
        holdout.sort();
        holdout.dedup();
        let mut material = String::new();
        // v2: the separators are escaped. See `esc`.
        material.push_str("thruline-bench-protocol-v2\n");
        for (k, v) in [
            ("harness", &self.harness),
            ("harness_version", &self.harness_version),
            ("model", &self.model),
        ] {
            material.push_str(&format!("{}\u{1}{}\n", esc(k), esc(v)));
        }
        material.push_str(&format!(
            "tasks\u{1}{}\n",
            tasks
                .iter()
                .map(|t| esc(t))
                .collect::<Vec<_>>()
                .join("\u{2}")
        ));
        material.push_str(&format!(
            "holdout\u{1}{}\n",
            holdout
                .iter()
                .map(|t| esc(t))
                .collect::<Vec<_>>()
                .join("\u{2}")
        ));
        // `BTreeMap` iterates in key order, so the parameters are already
        // canonical.
        for (k, v) in &self.parameters {
            material.push_str(&format!("param\u{1}{}\u{1}{}\n", esc(k), esc(v)));
        }
        for (k, v) in &self.detectors {
            material.push_str(&format!("detector\u{1}{}\u{1}{}\n", esc(k), esc(v)));
        }
        material.push_str(&format!(
            "precondition\u{1}workspace_git_init\u{1}{}\n",
            self.preconditions.workspace_git_init
        ));
        material.push_str(&format!(
            "precondition\u{1}tree_hash_before_first_action\u{1}{}\n",
            self.preconditions.tree_hash_before_first_action
        ));
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(material.as_bytes());
        let digest = hasher.finalize();
        let mut hex = String::from("sha256:");
        for byte in digest {
            hex.push_str(&format!("{byte:02x}"));
        }
        hex
    }
}

/// Class 2's evidence: the hash of a protocol, and what it committed to.
///
/// Private hash with no setter, obtainable only from [`Protocol::commit`], so
/// a commitment cannot be conjured to match a run that happened first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commitment {
    hash: String,
    tasks: Vec<String>,
    holdout: Vec<String>,
}

impl Commitment {
    pub fn hash(&self) -> &str {
        &self.hash
    }
    /// Every task the run must account for.
    pub fn tasks(&self) -> &[String] {
        &self.tasks
    }
    /// The subset the targets are computed over.
    pub fn holdout(&self) -> &[String] {
        &self.holdout
    }
}

/// What a run says about itself afterwards.
///
/// Separate from [`Commitment`] on purpose: the commitment is what was promised
/// and this is what happened, and the whole discipline is that the two are
/// **compared** rather than assumed equal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunManifest {
    protocol_hash: String,
    run_id: String,
    prior_runs: Vec<String>,
}

impl RunManifest {
    // **There is deliberately no public constructor.**
    //
    // There used to be `RunManifest::first(hash, run_id)`, which hardcoded
    // `prior_runs: Vec::new()`. It was documented as "for a genuinely first run
    // and for tests" and it was, in fact, a one-token bypass of the entire
    // holdout ledger: a holdout spent three times through `HoldoutLedger` and
    // then declared through `first()` produced a clean headline over a
    // thrice-spent holdout. Four of this crate's own tests took that path.
    //
    // "The ledger enforces it across processes" was only ever true if the
    // caller chose the ledger, and with two public constructors — one of them
    // named `first` — the caller was a human making a judgement call. So the
    // judgement is gone: [`HoldoutLedger::spend`] is the only way to obtain a
    // `RunManifest`, and obtaining one **records the spend**. You cannot claim a
    // first run without making it one.

    pub fn protocol_hash(&self) -> &str {
        &self.protocol_hash
    }
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
    /// Runs that consumed this holdout before this one. **Non-empty is a
    /// refusal**, not a note.
    pub fn prior_runs(&self) -> &[String] {
        &self.prior_runs
    }
}

/// An append-only record of which runs have consumed which holdout.
///
/// # Why this is a file and not a flag
///
/// A holdout is fixed before the run so it cannot be chosen after results. That
/// stops one attack and not the next one: the same holdout consumed by ten
/// tuning runs is a validation set wearing a holdout's name, and the tenth
/// number is fitted to it however carefully the split was drawn.
///
/// Nothing in a single process can see the nine earlier runs, so the record has
/// to outlive the process. `prior_runs` on a manifest is then **derived from a
/// file** rather than asserted by the caller, and the refusal fires without
/// anyone having to remember it. The ledger is append-only text — one
/// `hash<TAB>run_id` line per spend — so it diffs, and a run removed from it is
/// a visible deletion rather than a silent one.
pub struct HoldoutLedger {
    path: std::path::PathBuf,
    spent: Vec<(String, String)>,
}

impl HoldoutLedger {
    /// Reads the ledger at `path`. A missing file is an empty ledger — the
    /// first spend creates it.
    pub fn open(path: &std::path::Path) -> std::io::Result<HoldoutLedger> {
        let spent = match std::fs::read_to_string(path) {
            Ok(text) => text
                .lines()
                .filter_map(|l| l.split_once('\t'))
                .map(|(h, r)| (h.to_string(), r.to_string()))
                .collect(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e),
        };
        Ok(HoldoutLedger {
            path: path.to_path_buf(),
            spent,
        })
    }

    /// Re-reads the file into the in-memory snapshot.
    fn reread(&mut self) -> std::io::Result<()> {
        let fresh = HoldoutLedger::open(&self.path)?;
        self.spent = fresh.spent;
        Ok(())
    }

    /// Runs that already consumed this protocol's holdout.
    pub fn prior(&self, protocol_hash: &str) -> Vec<String> {
        self.spent
            .iter()
            .filter(|(h, _)| h == protocol_hash)
            .map(|(_, r)| r.clone())
            .collect()
    }

    /// Records this run's consumption and returns the manifest that describes
    /// it — **including the runs that got there first**, which is what makes
    /// the second use of a holdout refuse itself.
    pub fn spend(&mut self, protocol_hash: &str, run_id: &str) -> std::io::Result<RunManifest> {
        // **Re-read before appending.** Two processes that opened the ledger
        // before either spent would otherwise both see an empty history: the
        // in-memory copy is a snapshot, and the file is the record. This does
        // not make the append atomic against a truly concurrent writer — that
        // needs a lock, and is recorded as a residual — but it does close the
        // realistic case, where one run finishes and another starts from a
        // handle opened earlier.
        self.reread()?;
        let prior_runs = self.prior(protocol_hash);
        use std::io::Write as _;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(f, "{protocol_hash}\t{run_id}")?;
        self.spent
            .push((protocol_hash.to_string(), run_id.to_string()));
        Ok(RunManifest {
            protocol_hash: protocol_hash.to_string(),
            run_id: run_id.to_string(),
            prior_runs,
        })
    }
}

#[cfg(test)]
mod tests;

/// Escapes the field and list separators, so a value containing one cannot
/// impersonate a field boundary.
///
/// # The collision this removes
///
/// The preimage joined fields with a raw `\u{1}` and list items with a raw
/// `\u{2}`, so `{"cmd": "run\u{1}seed\u{1}7"}` and
/// `{"cmd\u{1}run": "seed\u{1}7"}` produced **the same commitment hash**, as
/// did `["a\u{2}b", "c"]` and `["a", "b\u{2}c"]`. Two different protocols, one
/// hash — which is the whole thing a commitment is supposed to make impossible.
///
/// Unreachable with Terminal-Bench task ids, and `parameters` is free-form **by
/// design**: it holds prompts and exact commands, which is precisely where an
/// arbitrary byte arrives. This is also the class RFC 8785 canonicalisation was
/// adopted for elsewhere in this product, so it is a known-and-fixed-once
/// failure appearing in a preimage written by hand.
///
/// `\u{0}` is the escape, and it is itself escaped, which is what makes the
/// mapping injective.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\u{0}' => out.push_str("\u{0}0"),
            '\u{1}' => out.push_str("\u{0}1"),
            '\u{2}' => out.push_str("\u{0}2"),
            _ => out.push(c),
        }
    }
    out
}

/// A stable 64-bit bucket for `task` under `salt`.
///
/// The same construction `tau2::holdout` uses, for the same reason: a split
/// anyone can recompute and nobody can redraw.
pub(crate) fn bucket(salt: &str, task: &str) -> u64 {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update([0u8]);
    h.update(task.as_bytes());
    let d = h.finalize();
    let mut b = [0u8; 8];
    b.copy_from_slice(&d[..8]);
    u64::from_be_bytes(b)
}
