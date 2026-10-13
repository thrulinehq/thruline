//! What exactly was examined, and is the target the intended running
//! application (brief `B6.1`–`B6.3`, `B4.6`, `Q4`; T-3-2).
//!
//! T-3-1 answered *what bears on this thing that counts as done*. It did not
//! answer the question underneath it, which is Sam's first question's second
//! clause — **one version**. A green run from yesterday's server, or from a
//! sibling worktree, is a green run about something else, and until this module
//! existed nothing in the record could say so.
//!
//! # 1. It extends the identity and snapshot model; it does not compete with it
//!
//! `B6.1` is a refusal to invent a second identity. Every field here is an
//! existing id or an existing digest:
//!
//! * `snapshot_id` — [`crate::snapshot`]'s own row id.
//! * `source_identity` — [`crate::workspace_identity`]'s `wsid1:…`, which is
//!   the tree hash **including uncommitted changes**: staged, dirty, deleted,
//!   renamed, untracked and the index digest are all inside it. This is the
//!   one field that decides "same version of the code".
//! * `base_revision` — `git HEAD`, as the snapshot reads it.
//! * `lockfiles_sha256` — over the snapshot's own lockfile digests.
//! * `worktree_path` — the snapshot's root.
//!
//! Nothing is copied: a row is ids and digests, never file bytes and never a
//! sentence. Rule 13's cost is measured, not estimated, in
//! `crates/core/tests/examined_inputs_cost.rs`, and the measured figure is at
//! the end of this comment.
//!
//! # 2. Unknown is a state, never a value
//!
//! [`Input`] has two shapes and no third. A probe that did not answer produces
//! [`Input::Unknown`], which is stored as SQL `NULL`, serialised as
//! `{"state":"unknown"}`, rendered as the copy file's "not written down", and
//! **never compares equal to anything, including another unknown**. The
//! database refuses the literal string `unknown` in every one of these columns,
//! so a caller that tries to write the word instead of the state is refused by
//! the store rather than by a convention.
//!
//! This is `B7.2` at the input layer: a missing input is missing. It is not a
//! match, and it is never quietly a pass.
//!
//! # 3. The predicate, in one function, with its call sites enumerated
//!
//! [`observation_applies`] is the whole rule, and
//! `crates/core/tests/examined_inputs_predicate.rs` re-derives its callers from
//! the sources and fails naming any new one — T-2-2's pattern, for the same
//! reason: a second reader that decides for itself what "applies" means is how
//! an old server's green run reaches a screen.
//!
//! An observation applies to a candidate only when every one of these holds:
//!
//! | class | what it means |
//! |---|---|
//! | `inputs_other_revision` | a different base revision |
//! | `inputs_other_worktree` | the same revision, a second checkout of it |
//! | `inputs_other_edits`    | the same revision and checkout, and one side has uncommitted changes the other does not |
//! | `inputs_other_folder_state` | the same revision and checkout, neither side has uncommitted changes, and the source identities still differ |
//! | `inputs_other_app`      | a different port owner, by port or by executable |
//! | `inputs_other_database` | the same executable, a different database |
//! | `inputs_unknown_version` / `inputs_unknown_app` | a probe that did not answer |
//! | `inputs_no_app_named`   | a running-app result, and the candidate names no app |
//!
//! The predicate gates **attachment to a Change**. It never touches a verdict:
//! nothing here is read by `premise`, the narrator, the claim verdicts or the
//! hash chain, and `crates/core/tests/examined_inputs_predicate.rs` pins that
//! this module is imported by nothing in those paths.
//!
//! # 4. A running-app examination proves what it looked at
//!
//! `B6.3`. For a browser or API examination the record carries the listening
//! port, the pid that owns it, that pid's executable path and the database file
//! that process has open — read with `lsof`, the same evidence
//! `docs/BASELINE-2026-09-16.md` used, and read only for a port the caller
//! named. Nothing here scans, and nothing here reads a port the caller did not
//! ask about.
//!
//! # Cost (Rule 13, `L1.1`)
//!
//! MEASURED on a fixture store by `crates/core/tests/examined_inputs_cost.rs`,
//! which re-measures on every run and fails if it has left the band. N = 1000,
//! `PRAGMA page_count × page_size`, checkpointed and vacuumed between readings:
//!
//! ```text
//! one examined-inputs record, source only        =   974 B
//! one examined-inputs record with a running app  =   974 B
//! ```
//!
//! The two are equal, and that is a fact about the measurement rather than
//! about the rows: the four running-app fields add about a hundred bytes and
//! the reading is `page_count × page_size`, so both land on the same number of
//! 4 KiB pages. The figure is an upper bound with every field at a realistic
//! length — a 40-character revision, a tagged workspace identity, three
//! 64-character digests, two real paths and a tool version string.
//!
//! For scale: one record per check run on the personal ledger's 76,205 sessions
//! — one check each, which is already more checks than that store holds — would
//! be about 74 MB against 7.79 GB already there, under 1 %.
//!
//! No `unwrap`, no `expect`, no network, no `unsafe`. Every read is masked at
//! the boundary and carries the record of what the masker did.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::chain::sha256_hex;
use crate::change::mask_text;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::redact::MaskRecord;

/// The served shape's version. Bumped when a field is added or renamed.
pub const INPUTS_VIEW_SCHEMA_VERSION: u32 = 1;

/// The name of the one function that decides whether an observation applies to
/// a candidate. If a second function in this workspace grows that power,
/// `crates/core/tests/examined_inputs_predicate.rs` fails and names it.
pub const THE_PREDICATE: &str = "observation_applies";

// ---------------------------------------------------------------------------
// Unknown is a state
// ---------------------------------------------------------------------------

/// One examined input: either a value the record actually holds, or the
/// explicit absence of one.
///
/// [`Input::Unknown`] never equals anything under [`Input::matches`], including
/// another unknown. Two probes that both failed did not agree about anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Input {
    Known { value: String },
    Unknown,
}

impl Input {
    pub fn known(value: impl Into<String>) -> Self {
        let value = value.into();
        if value.trim().is_empty() {
            return Self::Unknown;
        }
        Self::Known { value }
    }

    /// A probe's answer. `None` and the empty string are both "it did not
    /// answer" — never a value.
    pub fn from_option(value: Option<String>) -> Self {
        match value {
            Some(v) => Self::known(v),
            None => Self::Unknown,
        }
    }

    pub fn value(&self) -> Option<&str> {
        match self {
            Self::Known { value } => Some(value.as_str()),
            Self::Unknown => None,
        }
    }

    pub fn is_known(&self) -> bool {
        matches!(self, Self::Known { .. })
    }

    /// Do two inputs say the same thing? **Unknown matches nothing.**
    pub fn matches(&self, other: &Input) -> bool {
        match (self.value(), other.value()) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    /// The masked form, for serving.
    fn masked(&self, record: &mut MaskRecord) -> Input {
        match self {
            Self::Known { value } => Input::Known {
                value: mask_text(value, record),
            },
            Self::Unknown => Input::Unknown,
        }
    }
}

/// The word this module refuses to store as a value, in any column that could
/// carry one. Public so a test can assert the refusal without reading the DDL.
pub const NOT_A_VALUE: &str = "unknown";

// ---------------------------------------------------------------------------
// The record
// ---------------------------------------------------------------------------

/// Which examination a record belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectKind {
    /// What a Change says **will** be examined — the Prepare view's own row.
    Intended,
    /// A captured command execution (`evidence_bindings`).
    CheckRun,
    /// Something the recorder saw (`observations`).
    Observation,
}

impl SubjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Intended => "intended",
            Self::CheckRun => "check_run",
            Self::Observation => "observation",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "intended" => Ok(Self::Intended),
            "check_run" => Ok(Self::CheckRun),
            "observation" => Ok(Self::Observation),
            other => Err(CoreError::Config(format!("unknown subject kind {other:?}"))),
        }
    }
}

/// Whether the examined source carried edits that were not committed.
///
/// Three states and no fourth: clean is not unknown, and unknown is not clean.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Uncommitted {
    Clean,
    Present { files: u64, diff_sha256: Input },
    Unknown,
}

impl Uncommitted {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Present { .. } => "present",
            Self::Unknown => "unknown",
        }
    }
}

/// One tool the examination used or would use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct ToolInput {
    pub name: String,
    /// Its own `--version` string, where the record holds one.
    pub version: Input,
    /// False when the program is not on this machine. An absent tool is shown
    /// as absent; it is never a version of nothing.
    pub present: bool,
}

/// What a running-app examination looked at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Target {
    /// Not a browser or API examination: nothing was running to identify.
    None,
    RunningApp {
        port: u16,
        owner_pid: Input,
        /// The executable path of the process that owns the listening socket.
        owner_exe: Input,
        /// The database file that process has open.
        database_path: Input,
    },
}

impl Target {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::RunningApp { .. } => "running_app",
        }
    }

    pub fn is_running_app(&self) -> bool {
        matches!(self, Self::RunningApp { .. })
    }
}

/// Everything one examination looked at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExaminedInputs {
    pub subject_kind: SubjectKind,
    pub subject_id: String,
    pub snapshot_id: Input,
    /// The tree hash including uncommitted changes — `wsid1:…`.
    pub source_identity: Input,
    pub base_revision: Input,
    pub uncommitted: Uncommitted,
    pub worktree_path: Input,
    pub lockfiles_sha256: Input,
    /// A digest over the config and test files the check profile names, and
    /// the names themselves, so the technical layer can show which.
    pub config_sha256: Input,
    pub config_files: Vec<String>,
    pub tools: Vec<ToolInput>,
    pub target: Target,
}

impl ExaminedInputs {
    /// An empty record for one subject: everything unknown, nothing guessed.
    pub fn unknown_for(subject_kind: SubjectKind, subject_id: impl Into<String>) -> Self {
        Self {
            subject_kind,
            subject_id: subject_id.into(),
            snapshot_id: Input::Unknown,
            source_identity: Input::Unknown,
            base_revision: Input::Unknown,
            uncommitted: Uncommitted::Unknown,
            worktree_path: Input::Unknown,
            lockfiles_sha256: Input::Unknown,
            config_sha256: Input::Unknown,
            config_files: Vec::new(),
            tools: Vec::new(),
            target: Target::None,
        }
    }

    /// The masked form, for serving.
    pub fn masked(&self, record: &mut MaskRecord) -> ExaminedInputs {
        ExaminedInputs {
            subject_kind: self.subject_kind,
            subject_id: self.subject_id.clone(),
            snapshot_id: self.snapshot_id.masked(record),
            source_identity: self.source_identity.masked(record),
            base_revision: self.base_revision.masked(record),
            uncommitted: self.uncommitted.clone(),
            worktree_path: self.worktree_path.masked(record),
            lockfiles_sha256: self.lockfiles_sha256.masked(record),
            config_sha256: self.config_sha256.masked(record),
            config_files: self
                .config_files
                .iter()
                .map(|f| mask_text(f, record))
                .collect(),
            tools: self.tools.clone(),
            target: match &self.target {
                Target::None => Target::None,
                Target::RunningApp {
                    port,
                    owner_pid,
                    owner_exe,
                    database_path,
                } => Target::RunningApp {
                    port: *port,
                    owner_pid: owner_pid.clone(),
                    owner_exe: owner_exe.masked(record),
                    database_path: database_path.masked(record),
                },
            },
        }
    }

    /// The executable's own file name, for the Prepare sentence. The full path
    /// is the technical layer's; Sam reads a program name.
    pub fn app_program_name(&self) -> Option<String> {
        match &self.target {
            Target::RunningApp { owner_exe, .. } => owner_exe
                .value()
                .map(|p| p.rsplit('/').next().unwrap_or(p).to_string()),
            Target::None => None,
        }
    }
}

// ---------------------------------------------------------------------------
// THE PREDICATE
// ---------------------------------------------------------------------------

/// Whether one examination's result may be counted for one candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Applies {
    Yes,
    No {
        /// The copy-file slug. The sentence is looked up, never built here.
        reason_slug: &'static str,
    },
}

impl Applies {
    pub fn applies(&self) -> bool {
        matches!(self, Self::Yes)
    }

    pub fn slug(&self) -> &'static str {
        match self {
            Self::Yes => "inputs_applies",
            Self::No { reason_slug } => reason_slug,
        }
    }
}

/// **The predicate.** Does an observation's examined inputs describe the
/// candidate the Change is about?
///
/// `examined` is what a check run or an observation actually looked at.
/// `intended` is what the Change's Prepare view names for its candidate. The
/// order of the tests is the order a person would ask them in, and the first
/// failure is the answer: a result from a different revision is not also
/// reported as a database mismatch.
///
/// **Unknown never passes.** If either side cannot say what version it is
/// about, the answer is `inputs_unknown_version`, not a match.
///
/// # Why the folder is asked about before the source identity (T-3-2b)
///
/// `source_identity` is [`crate::workspace_identity`]'s `wsid1:`, and its
/// preimage opens with `root` and `worktree` — the canonical root and the git
/// common dir. Two **clean** sibling checkouts of the same commit therefore
/// carry different identities with not one uncommitted change between them.
/// Asking the identity first classified that as an edits difference, so a
/// person with two clean folders was told their unsaved changes differed when
/// neither folder had any. Nothing was wrongly accepted — the refusal was
/// right and stays — but the sentence was false, which is the one thing this
/// product may not do. The dedicated `worktree_path` field answers the folder
/// question exactly, so it is asked first and a second checkout is named as a
/// second checkout. The base revisions are already equal by the time control
/// reaches it, which is what lets its sentence say "of the same commit".
///
/// The identity check that follows is therefore about one folder at one
/// commit, and it still refuses everything it refused before. It only stops
/// blaming uncommitted changes for a difference neither side's uncommitted
/// state can account for: `inputs_other_edits` now requires that one side
/// actually reports uncommitted changes, and the residue — a different branch
/// name, a submodule, a sparse-checkout, a re-created git dir — is
/// `inputs_other_folder_state`, which is a refusal too.
pub fn observation_applies(examined: &ExaminedInputs, intended: &ExaminedInputs) -> Applies {
    let no = |slug: &'static str| Applies::No { reason_slug: slug };

    // The commit it was checked on.
    if !examined.base_revision.is_known() || !intended.base_revision.is_known() {
        return no("inputs_unknown_version");
    }
    if !examined.base_revision.matches(&intended.base_revision) {
        return no("inputs_other_revision");
    }

    // The copy of the project folder it happened in. Asked before the full
    // source identity, because the identity contains the answer to this
    // question and would report it as something else.
    if !examined.worktree_path.is_known() || !intended.worktree_path.is_known() {
        return no("inputs_unknown_worktree");
    }
    if !examined.worktree_path.matches(&intended.worktree_path) {
        return no("inputs_other_worktree");
    }

    // One folder, one commit: what is left is what is on disk in it.
    if !examined.source_identity.is_known() || !intended.source_identity.is_known() {
        return no("inputs_unknown_version");
    }
    if !examined.source_identity.matches(&intended.source_identity) {
        let dirty_somewhere = matches!(examined.uncommitted, Uncommitted::Present { .. })
            || matches!(intended.uncommitted, Uncommitted::Present { .. });
        return no(if dirty_somewhere {
            "inputs_other_edits"
        } else {
            "inputs_other_folder_state"
        });
    }

    // The running application, when this was a browser or API examination.
    if let Target::RunningApp {
        port,
        owner_exe,
        database_path,
        ..
    } = &examined.target
    {
        let Target::RunningApp {
            port: want_port,
            owner_exe: want_exe,
            database_path: want_db,
            ..
        } = &intended.target
        else {
            return no("inputs_no_app_named");
        };
        if !owner_exe.is_known() || !want_exe.is_known() {
            return no("inputs_unknown_app");
        }
        if port != want_port || !owner_exe.matches(want_exe) {
            return no("inputs_other_app");
        }
        if !database_path.is_known() || !want_db.is_known() {
            return no("inputs_unknown_app");
        }
        if !database_path.matches(want_db) {
            return no("inputs_other_database");
        }
    }

    Applies::Yes
}

// ---------------------------------------------------------------------------
// The store. Every write appends.
// ---------------------------------------------------------------------------

/// The id of one record: the SHA-256 of what it says.
///
/// Content-addressed, so recording the same examination twice inserts once and
/// a record that says something different is a different row rather than an
/// edit of the first.
pub fn record_id(inputs: &ExaminedInputs) -> String {
    let payload = format!(
        "exi/v1\u{1f}{}\u{1f}{}\u{1f}{}",
        inputs.subject_kind.as_str(),
        inputs.subject_id,
        serde_json::to_string(inputs).unwrap_or_default()
    );
    format!("exi_{}", sha256_hex(payload.as_bytes()))
}

/// What one write did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecordOutcome {
    pub id: String,
    /// False when this exact record was already there.
    pub inserted: bool,
}

/// Append one examined-inputs record.
///
/// Refused before the store is touched when a caller tries to write the word
/// `unknown` as a value: unknown is a state, and the database refuses it too.
pub fn record(ledger: &Ledger, inputs: &ExaminedInputs, at: &str) -> Result<RecordOutcome> {
    for field in [
        &inputs.snapshot_id,
        &inputs.source_identity,
        &inputs.base_revision,
        &inputs.worktree_path,
        &inputs.lockfiles_sha256,
        &inputs.config_sha256,
    ] {
        if field.value() == Some(NOT_A_VALUE) {
            return Err(refusal("inputs_unknown_is_not_a_value"));
        }
    }
    if let Target::RunningApp {
        owner_exe,
        database_path,
        ..
    } = &inputs.target
    {
        if owner_exe.value() == Some(NOT_A_VALUE) || database_path.value() == Some(NOT_A_VALUE) {
            return Err(refusal("inputs_unknown_is_not_a_value"));
        }
    }

    let id = record_id(inputs);
    let existing: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT id FROM examination_inputs WHERE id=?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?;
    if existing.is_some() {
        return Ok(RecordOutcome {
            id,
            inserted: false,
        });
    }

    let (uncommitted_files, uncommitted_sha) = match &inputs.uncommitted {
        Uncommitted::Present { files, diff_sha256 } => (
            Some(*files as i64),
            diff_sha256.value().map(|s| s.to_string()),
        ),
        _ => (None, None),
    };
    let (port, pid, exe, db) = match &inputs.target {
        Target::None => (None, None, None, None),
        Target::RunningApp {
            port,
            owner_pid,
            owner_exe,
            database_path,
        } => (
            Some(*port as i64),
            owner_pid.value().and_then(|p| p.parse::<i64>().ok()),
            owner_exe.value().map(|s| s.to_string()),
            database_path.value().map(|s| s.to_string()),
        ),
    };

    ledger.conn().execute(
        "INSERT INTO examination_inputs(id, subject_kind, subject_id, recorded_at,
             schema_version, snapshot_id, source_identity, base_revision,
             uncommitted_state, uncommitted_files, uncommitted_sha256, worktree_path,
             lockfiles_sha256, config_sha256, config_files_json, tools_json, target_kind,
             app_port, app_owner_pid, app_owner_exe, app_database)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)",
        params![
            id,
            inputs.subject_kind.as_str(),
            inputs.subject_id,
            at,
            INPUTS_VIEW_SCHEMA_VERSION,
            inputs.snapshot_id.value(),
            inputs.source_identity.value(),
            inputs.base_revision.value(),
            inputs.uncommitted.as_str(),
            uncommitted_files,
            uncommitted_sha,
            inputs.worktree_path.value(),
            inputs.lockfiles_sha256.value(),
            inputs.config_sha256.value(),
            serde_json::to_string(&inputs.config_files)?,
            serde_json::to_string(&inputs.tools)?,
            inputs.target.as_str(),
            port,
            pid,
            exe,
            db,
        ],
    )?;
    Ok(RecordOutcome { id, inserted: true })
}

/// Write the `intended` record for a Change at the moment it is bound to one
/// (T-3-2b).
///
/// **Why this exists.** Until this function was called from the two acts that
/// bind a Change to a working tree — the first-use click and `change new` —
/// the predicate had nothing to compare an examination against, so every piece
/// of evidence on every Change made on the ordinary path carried "the question
/// was not put" and no sentence. A guard that only protects the Changes
/// somebody happened to run a second command on is not a guard.
///
/// It observes the project's captured working folder when one is still on this
/// machine. The vendor's project directory key is lossy, so it is only used as
/// an exact legacy fallback; it is never re-expanded into a plausible path.
/// A command may also provide the directory its person explicitly selected.
/// It names **no running application and no tools**: binding is not a claim
/// that anything was examined, and a probe of a port nobody named would be this
/// product reading a machine on its own. `change prepare` is still the verb
/// that adds those, and it appends rather than replacing — the standing record
/// is the newest, and this one stays readable underneath it.
///
/// A project folder that cannot be established answers with no record. It
/// never guesses or chooses between reconstructed roots.
pub fn bind_intended(ledger: &Ledger, change_id: &str, at: &str) -> Result<Option<RecordOutcome>> {
    bind_intended_at(ledger, change_id, at, None)
}

/// [`bind_intended`] with an explicitly selected working directory.
///
/// The CLI may supply a person's explicit `--root` selection. Ambient process
/// cwd is not a selection: first-use and ordinary CLI use the captured project
/// identity unless the person selected a root through that flag.
pub fn bind_intended_at(
    ledger: &Ledger,
    change_id: &str,
    at: &str,
    selected_root: Option<&Path>,
) -> Result<Option<RecordOutcome>> {
    let project: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT c.project_id FROM changes c JOIN projects p ON p.id = c.project_id
              WHERE c.id = ?1",
            params![change_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(project_id) = project else {
        return Ok(None);
    };
    let root = crate::workspace::established_project_root(ledger, &project_id, selected_root)?;
    let Some(root) = root else {
        // The store names a folder that is not on this disk, so there is
        // nothing to observe and nothing is written. An all-unknown record
        // would read as "we looked and could not tell", which is a different
        // claim from "nobody has looked yet" — and it is the second one that
        // is true here. The Verify view then says nothing is bound to this
        // Change and names the one act that binds it, which is the act that
        // takes the folder from the person instead of from the store.
        //
        // A lossy vendor directory key is one real way to reach this state.
        // Binding requires captured workspace provenance or a person's
        // explicit root, never a reconstruction of that key.
        return Ok(None);
    };
    // T-S-2: the standing `intended` record before this write is what "the
    // tree version moved" is measured against. Read before the new one lands,
    // so a first binding (nothing standing yet) is correctly "nothing to
    // compare" rather than a move from an invented baseline.
    let previous = standing(ledger, SubjectKind::Intended, change_id)?;

    let (mut inputs, snapshot) = observe_source_with_snapshot(&root, at);
    if let Some(snap) = &snapshot {
        if crate::snapshot::store(ledger, snap).is_err() {
            inputs.snapshot_id = Input::Unknown;
        }
    }
    inputs.subject_kind = SubjectKind::Intended;
    inputs.subject_id = change_id.to_string();
    let outcome = record(ledger, &inputs, at)?;

    // Whether the tree actually moved (a new snapshot id) is decided inside
    // `on_intended_bound`; this call is unconditional so a re-bind onto the
    // same snapshot is correctly a no-op there rather than skipped here.
    // Failure here (e.g. no accepted scope yet) does not fail the binding
    // itself — the examined-inputs record this function exists to write is
    // the more load-bearing fact — but it is not swallowed: it is said,
    // loudly, once (Rule 12).
    if let Err(err) = crate::evidence_applicability::on_intended_bound(
        ledger,
        change_id,
        previous.as_ref(),
        &inputs,
        at,
    ) {
        tracing::warn!(
            change_id,
            error = %err,
            "freshness assessment did not run for this binding"
        );
    }

    Ok(Some(outcome))
}

/// The standing record for one examination: the latest one written.
pub fn standing(
    ledger: &Ledger,
    subject_kind: SubjectKind,
    subject_id: &str,
) -> Result<Option<ExaminedInputs>> {
    read_one(
        ledger,
        "WHERE subject_kind=?1 AND subject_id=?2 ORDER BY rowid DESC LIMIT 1",
        params![subject_kind.as_str(), subject_id],
    )
}

/// One record by its id — the exact record something was bound to (a review
/// names the `intended` record it looked at), which need not be the standing
/// one any more. `None` when no record has that id.
pub fn by_id(ledger: &Ledger, id: &str) -> Result<Option<ExaminedInputs>> {
    read_one(ledger, "WHERE id=?1", params![id])
}

/// The one reader behind [`standing`] and [`by_id`]: the first row the given
/// `WHERE …` tail selects, as a record.
fn read_one(
    ledger: &Ledger,
    tail: &str,
    args: &[&dyn rusqlite::ToSql],
) -> Result<Option<ExaminedInputs>> {
    let row = ledger
        .conn()
        .query_row(
            &format!(
                "SELECT snapshot_id, source_identity, base_revision, uncommitted_state,
                    uncommitted_files, uncommitted_sha256, worktree_path, lockfiles_sha256,
                    config_sha256, config_files_json, tools_json, target_kind, app_port,
                    app_owner_pid, app_owner_exe, app_database, subject_kind, subject_id
               FROM examination_inputs
              {tail}"
            ),
            args,
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, Option<i64>>(12)?,
                    row.get::<_, Option<i64>>(13)?,
                    row.get::<_, Option<String>>(14)?,
                    row.get::<_, Option<String>>(15)?,
                    row.get::<_, String>(16)?,
                    row.get::<_, String>(17)?,
                ))
            },
        )
        .optional()?;
    let Some(r) = row else { return Ok(None) };
    let subject_kind = SubjectKind::parse(&r.16)?;
    let uncommitted = match r.3.as_str() {
        "clean" => Uncommitted::Clean,
        "present" => Uncommitted::Present {
            files: r.4.unwrap_or(0).max(0) as u64,
            diff_sha256: Input::from_option(r.5),
        },
        _ => Uncommitted::Unknown,
    };
    let target = match r.11.as_str() {
        "running_app" => Target::RunningApp {
            port: r.12.unwrap_or(0).clamp(0, u16::MAX as i64) as u16,
            owner_pid: Input::from_option(r.13.map(|p| p.to_string())),
            owner_exe: Input::from_option(r.14),
            database_path: Input::from_option(r.15),
        },
        _ => Target::None,
    };
    Ok(Some(ExaminedInputs {
        subject_kind,
        subject_id: r.17,
        snapshot_id: Input::from_option(r.0),
        source_identity: Input::from_option(r.1),
        base_revision: Input::from_option(r.2),
        uncommitted,
        worktree_path: Input::from_option(r.6),
        lockfiles_sha256: Input::from_option(r.7),
        config_sha256: Input::from_option(r.8),
        config_files: serde_json::from_str(&r.9).unwrap_or_default(),
        tools: serde_json::from_str(&r.10).unwrap_or_default(),
        target,
    }))
}

/// How many records one examination has. The technical layer's count.
pub fn record_count(ledger: &Ledger, subject_kind: SubjectKind, subject_id: &str) -> Result<u64> {
    let n: i64 = ledger.conn().query_row(
        "SELECT COUNT(*) FROM examination_inputs WHERE subject_kind=?1 AND subject_id=?2",
        params![subject_kind.as_str(), subject_id],
        |row| row.get(0),
    )?;
    Ok(n.max(0) as u64)
}

/// A refusal, in the copy file's Plain words. Nothing in this module writes an
/// English sentence of its own.
fn refusal(slug: &'static str) -> CoreError {
    CoreError::Config(
        crate::voice::inputs_sentence(slug, &Fills::default())
            .map(|(plain, _)| plain)
            .unwrap_or_else(|| format!("refused: {slug}")),
    )
}

/// The closed set of things a copy line may have filled into it. No sentence
/// in this layer can be assembled out of free text.
#[derive(Debug, Clone, Default)]
pub struct Fills {
    pub port: Option<u16>,
    pub app: Option<String>,
    pub tool: Option<String>,
    pub value: Option<String>,
    pub count: Option<u64>,
}

// ---------------------------------------------------------------------------
// Observing the machine
// ---------------------------------------------------------------------------

/// Read the source side of an examined-inputs record out of a workspace.
///
/// Uses the existing snapshot capture for the tree, the lockfiles and the
/// revision, and the existing workspace identity for the tree hash that
/// includes uncommitted changes. Nothing is computed a second way.
///
/// The snapshot is **not stored** by this function: a caller that wants the
/// receipt on disk stores it and passes the id back in. A record whose snapshot
/// was never stored says `unknown` for the id rather than naming a row nobody
/// can open.
pub fn observe_source(root: &Path, snapshot_id: Option<String>) -> ExaminedInputs {
    let mut out = ExaminedInputs::unknown_for(SubjectKind::CheckRun, String::new());
    out.snapshot_id = Input::from_option(snapshot_id);
    out.worktree_path = Input::known(root.display().to_string());

    if let Ok(crate::workspace_identity::Observation::Complete(fp)) =
        crate::workspace_identity::observe(root, crate::workspace_identity::Scope::everything())
    {
        out.source_identity = Input::known(fp.identity().to_string());
        out.base_revision = Input::from_option(fp.doc().head.clone());
        out.worktree_path = Input::known(fp.doc().root.clone());
        let dirty = fp.doc().staged.len()
            + fp.doc().dirty.len()
            + fp.doc().deleted.len()
            + fp.doc().renamed.len()
            + fp.doc().untracked.len();
        out.uncommitted = if dirty == 0 {
            Uncommitted::Clean
        } else {
            Uncommitted::Present {
                files: dirty as u64,
                diff_sha256: Input::Unknown,
            }
        };
    }
    out
}

/// The same, plus the snapshot's lockfile digests and its dirty-diff digest.
///
/// One snapshot capture, reused for both halves, so the record and the stored
/// snapshot cannot disagree about the same tree.
pub fn observe_source_with_snapshot(
    root: &Path,
    taken_at: &str,
) -> (ExaminedInputs, Option<crate::snapshot::WorkspaceSnapshot>) {
    let snapshot = match crate::snapshot::capture(root, taken_at) {
        Ok(snapshot) => Some(snapshot),
        Err(err) => {
            // SNAPSHOT-CHILD-TIMEOUT (D-051): a tree read that did not finish
            // leaves the record's snapshot, lockfiles and revision `Unknown`.
            // The record has no column for why, so the reason is said here
            // rather than dropped (Rule 12).
            if crate::bounded_child::is_timeout(&err) {
                tracing::warn!(
                    root = %root.display(),
                    error = %err,
                    "no workspace snapshot: a read of the folder did not finish within the time limit"
                );
            }
            None
        }
    };
    let mut out = observe_source(root, snapshot.as_ref().map(|s| s.id.clone()));
    if let Some(snap) = &snapshot {
        out.lockfiles_sha256 = Input::known(lockfiles_digest(&snap.environment.lockfiles));
        if let (Uncommitted::Present { files, .. }, Some(diff)) =
            (&out.uncommitted, snap.tree.dirty_diff_sha256.clone())
        {
            out.uncommitted = Uncommitted::Present {
                files: *files,
                diff_sha256: Input::known(diff),
            };
        }
        if out.base_revision.value().is_none() {
            out.base_revision = Input::from_option(snap.git.as_ref().and_then(|g| g.head.clone()));
        }
    }
    (out, snapshot)
}

/// A digest over the lockfile digests the snapshot already computed. Ordered by
/// name (the map is a `BTreeMap`), so two machines agree.
pub fn lockfiles_digest(lockfiles: &BTreeMap<String, String>) -> String {
    let mut payload = String::from("lockfiles/v1");
    for (name, digest) in lockfiles {
        payload.push('\u{1f}');
        payload.push_str(name);
        payload.push('=');
        payload.push_str(digest);
    }
    sha256_hex(payload.as_bytes())
}

/// A digest over the named config and test files, and the names that went into
/// it. A file that is not there is named and contributes `absent`: a
/// configuration that disappeared is a different configuration.
pub fn config_digest(root: &Path, files: &[String]) -> (Input, Vec<String>) {
    if files.is_empty() {
        return (Input::Unknown, Vec::new());
    }
    let mut names: Vec<String> = files.to_vec();
    names.sort();
    names.dedup();
    let mut payload = String::from("config/v1");
    for name in &names {
        payload.push('\u{1f}');
        payload.push_str(name);
        payload.push('=');
        match std::fs::read(root.join(name)) {
            Ok(bytes) => payload.push_str(&sha256_hex(&bytes)),
            Err(_) => payload.push_str("absent"),
        }
    }
    (Input::known(sha256_hex(payload.as_bytes())), names)
}

/// A tool and the version it answers with, or the fact that it is not here.
///
/// Runs `<program> --version` and nothing else. A program that is not on this
/// machine is recorded absent; a program that answers nothing is recorded
/// present with an unknown version. Neither is guessed.
pub fn probe_tool(program: &str) -> ToolInput {
    let out = Command::new(program).arg("--version").output();
    match out {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            let line = text.lines().next().unwrap_or_default().trim().to_string();
            ToolInput {
                name: program.to_string(),
                version: Input::known(line),
                present: true,
            }
        }
        Ok(_) => ToolInput {
            name: program.to_string(),
            version: Input::Unknown,
            present: true,
        },
        Err(_) => ToolInput {
            name: program.to_string(),
            version: Input::Unknown,
            present: false,
        },
    }
}

/// Who owns a listening port, and what database that process has open.
///
/// `lsof` only, and only for the port the caller named. Nothing scans; nothing
/// reads a port nobody asked about. Every field that `lsof` does not answer is
/// `Unknown`.
pub fn probe_running_app(port: u16) -> Target {
    let mut owner_pid = Input::Unknown;
    let mut owner_exe = Input::Unknown;
    let mut database_path = Input::Unknown;

    if let Ok(out) = Command::new("lsof")
        .args(["-nP", "-sTCP:LISTEN", "-Fpn", &format!("-iTCP:{port}")])
        .output()
    {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout);
            for line in text.lines() {
                if let Some(pid) = line.strip_prefix('p') {
                    if pid.chars().all(|c| c.is_ascii_digit()) && !pid.is_empty() {
                        owner_pid = Input::known(pid.to_string());
                        break;
                    }
                }
            }
        }
    }

    if let Some(pid) = owner_pid.value() {
        if let Ok(out) = Command::new("lsof")
            .args(["-nP", "-p", pid, "-Fn"])
            .output()
        {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                let mut paths: Vec<&str> = Vec::new();
                for line in text.lines() {
                    if let Some(name) = line.strip_prefix('n') {
                        paths.push(name);
                    }
                }
                // The executable is the first `txt` path lsof reports; taking
                // the first absolute path it lists is the same fact and needs
                // no second format. `ps` is the fallback, and says so.
                if let Some(db) = paths.iter().find(|p| {
                    let lower = p.to_ascii_lowercase();
                    lower.ends_with(".sqlite3")
                        || lower.ends_with(".sqlite")
                        || lower.ends_with(".db")
                }) {
                    database_path = Input::known((*db).to_string());
                }
            }
        }
        if let Ok(out) = Command::new("ps").args(["-o", "comm=", "-p", pid]).output() {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout);
                owner_exe = Input::known(text.trim().to_string());
            }
        }
    }

    Target::RunningApp {
        port,
        owner_pid,
        owner_exe,
        database_path,
    }
}

// ---------------------------------------------------------------------------
// Reads. Masked at the boundary.
// ---------------------------------------------------------------------------

/// One line of the Prepare view: a copy-file slug and the one value it fills.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrepareLine {
    pub slug: &'static str,
    /// The value the sentence names, already masked. `None` where the record
    /// holds nothing — the surface fills the sentence with the
    /// `prepare_not_written_down` line rather than inventing a word.
    pub value: Option<String>,
    /// The program this line is about, where it is about one.
    pub tool: Option<String>,
}

/// What will be examined, in the order a person reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PrepareView {
    pub schema_version: u32,
    pub change_id: String,
    /// `None` when nothing has been recorded about what will be examined.
    pub inputs: Option<ExaminedInputs>,
    pub lines: Vec<PrepareLine>,
    /// The tools the record names, with their versions or their absence. The
    /// same facts are in `lines`; this is the typed form for the technical
    /// layer.
    pub tools: Vec<ToolInput>,
    /// The copy-file slug a surface uses where a line's value is `None`.
    pub missing_slug: &'static str,
    /// The running application this Change is about, when it names one.
    pub app_port: Option<u16>,
    pub app_program: Option<String>,
    pub record_count: u64,
    /// The copy-file slug for the count above. The sentence is looked up by the
    /// technical layer, never built there.
    pub record_count_slug: &'static str,
    pub masking: crate::redact::Treatment,
}

/// What the Prepare view renders from: the Change's own intended record.
pub fn prepare_view(ledger: &Ledger, change_id: &str) -> Result<PrepareView> {
    let mut record = MaskRecord::default();
    let stored = standing(ledger, SubjectKind::Intended, change_id)?;
    let count = record_count(ledger, SubjectKind::Intended, change_id)?;
    let Some(inputs) = stored else {
        return Ok(PrepareView {
            schema_version: INPUTS_VIEW_SCHEMA_VERSION,
            change_id: change_id.to_string(),
            inputs: None,
            lines: vec![PrepareLine {
                slug: "prepare_nothing_recorded",
                value: None,
                tool: None,
            }],
            tools: Vec::new(),
            missing_slug: "prepare_not_written_down",
            app_port: None,
            app_program: None,
            record_count: 0,
            record_count_slug: "inputs_record_count",
            masking: record.to_treatment(),
        });
    };
    let masked = inputs.masked(&mut record);
    let mut lines = vec![
        PrepareLine {
            slug: "prepare_repository",
            value: masked.worktree_path.value().map(|v| v.to_string()),
            tool: None,
        },
        PrepareLine {
            // The WHOLE revision. A surface that wants twelve characters cuts
            // twelve characters; this layer serves the fact, because a value
            // this module shortened would be a second, shorter truth about the
            // same thing and the masking census would be right to ask about it.
            slug: "prepare_base",
            value: masked.base_revision.value().map(|v| v.to_string()),
            tool: None,
        },
        PrepareLine {
            slug: match masked.uncommitted {
                Uncommitted::Present { .. } => "prepare_uncommitted_included",
                Uncommitted::Clean => "prepare_uncommitted_none",
                Uncommitted::Unknown => "prepare_uncommitted_unknown",
            },
            value: match &masked.uncommitted {
                Uncommitted::Present { files, .. } => Some(files.to_string()),
                _ => None,
            },
            tool: None,
        },
    ];
    let app_program = masked.app_program_name();
    if let Target::RunningApp { port, .. } = &masked.target {
        // A port with nothing identifiable on it says so. Nothing is inferred
        // from a probe that did not answer.
        lines.push(PrepareLine {
            slug: match app_program {
                Some(_) => "prepare_running_app",
                None => "prepare_app_unknown",
            },
            value: Some(port.to_string()),
            tool: None,
        });
    }
    for tool in &masked.tools {
        lines.push(PrepareLine {
            slug: match (tool.present, tool.version.value().is_some()) {
                (false, _) => "prepare_tool_absent",
                (true, false) => "prepare_tool_version_unknown",
                (true, true) => "prepare_tool_present",
            },
            value: tool.version.value().map(|v| v.to_string()),
            tool: Some(tool.name.clone()),
        });
    }
    let app_port = match &masked.target {
        Target::RunningApp { port, .. } => Some(*port),
        Target::None => None,
    };
    Ok(PrepareView {
        schema_version: INPUTS_VIEW_SCHEMA_VERSION,
        change_id: change_id.to_string(),
        tools: masked.tools.clone(),
        missing_slug: "prepare_not_written_down",
        inputs: Some(masked),
        lines,
        app_port,
        app_program,
        record_count: count,
        record_count_slug: "inputs_record_count",
        masking: record.to_treatment(),
    })
}
