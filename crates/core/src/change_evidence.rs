//! Criterion ↔ evidence: which recorded thing bears on which thing that counts
//! as done, and where the record holds nothing (brief `B4.6`, `B5.6`,
//! `B7.5`–`B7.7`, `B10.2`, `Q3`; T-3-1).
//!
//! T-2-1 built the Change, T-2-2 built the link between a Change and a piece of
//! recorded work, T-2-3 built what counts as done. None of them answers the
//! question Sam actually asks second — *what still needs evidence?* — because
//! "this Change has two confirmed recordings" says nothing about whether
//! anything in them bears on criterion 3. This module is that link, and the
//! four things that make it trustworthy are the four sections below.
//!
//! # 1. It copies nothing
//!
//! A row of `change_criterion_evidence` is *(which criterion, which stored
//! evidence row, why)*. It holds no outcome, no count and no prose out of the
//! thing it points at. "What did this show" is a join, so there is no second
//! store to disagree with the first (`B2.5`). The count of rows per Change is
//! served, once, in the technical layer.
//!
//! # 2. The rule, in one place, and nothing else links automatically
//!
//! [`links_for`] is the whole rule. For each accepted criterion of the standing
//! scope version, over evidence from sessions that are **confirmed and in
//! scope** (the gate is [`crate::change_link::applicability`], called here and
//! never re-implemented):
//!
//! * **`declared`** — the accepted check profile names a check whose category
//!   is this criterion's class, and the evidence is a check run of that class.
//! * **`named`** — the evidence's own typed name field carries, as a whole
//!   token, one of the criterion's subject tokens. The subject comes from
//!   [`crate::change_propose::observable_shape`], the same cut of a sentence
//!   the proposer and the weakening check already use; the token is compared
//!   after the same normalisation, so the two cannot drift.
//!
//! **Nothing else.** In particular there is no timing basis: a run that
//! happened in the same session as the work is not evidence about a criterion,
//! and the database's own CHECK has no word for it. Only check runs have a
//! typed name field that could carry a criterion's subject, so only check runs
//! are ever linked by the rule; a finding or an observation reaches a criterion
//! only when a person puts it there, and says so with their name on it.
//!
//! Same inputs, same rows: the link id is the SHA-256 of the tuple, so
//! re-running [`derive_links`] over an unchanged record inserts nothing.
//!
//! # 3. Missing evidence is missing, and a run that proved nothing is not a pass
//!
//! [`observed_of`] reads the observed result off **typed columns only**. The
//! important arm is the one that looks like a win and is not: a test-class run
//! that exited 0. An exit code cannot tell a full suite from a suite that ran
//! zero tests or skipped every one of them, and calling it Proven is exactly
//! the accidental satisfaction `Q3` forbids. A build- or lint-class run is
//! different and says so: a build check's evidence **is** its exit code and
//! nothing else (the rule `crates/verify/src/check.rs` states), so a zero exit
//! there is Proven.
//!
//! **T-3-6-F1 (`Q1`) — the exit code is still not the evidence; the count is.**
//! Until this card the store held no count of what a test run ran, so a suite
//! that passed and a suite that ran nothing were the same record and both read
//! *ran, checked nothing*. Sam ran two real suites, both green, and every
//! screen said no test had run. The wrapper now reads the run's own
//! `test result:` line off the stream as it captures it and stores the count as
//! columns ([`crate::test_tally`]); a test-class run that COMPLETED, exited 0,
//! captured both streams whole and counted **at least one pass and no
//! failures** is Proven for the criteria it is linked to. Everything short of
//! that keeps the old answer in the old words: no count at all, or a count of
//! zero, still reads *ran, checked nothing*, and the two vetoes above it —
//! an execution that did not finish (T-3-4) and evidence the freshness layer
//! has invalidated (T-S-2) — still settle nothing whatever they counted.
//!
//! # 4. Unlinking never deletes
//!
//! A person who removes a wrong link appends a `not-bearing` row. The row it
//! answers is still there, still readable, still attributable; the standing
//! state of a pair is its latest row. Re-linking appends again. Nothing in this
//! module updates or deletes, and the triggers refuse it anyway.
//!
//! Every read here is masked at the boundary and carries the record of what the
//! masker did. No `unwrap`, no `expect`, no network, no `unsafe`.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::chain::sha256_hex;
use crate::change::{mask_text, AcceptedScope};
use crate::change_link::applicability;
use crate::change_propose::{
    accepted_check_availability, observable_shape, Availability, ProfileEnvironment,
};
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::redact::MaskRecord;

/// The served shape's version. Bumped when a field is added or renamed.
pub const COVERAGE_VIEW_SCHEMA_VERSION: u32 = 1;

/// The name of the one function that decides what links automatically. If a
/// second function in this workspace grows that power,
/// `crates/core/tests/change_evidence_rule.rs` fails and names it.
pub const THE_RULE: &str = "links_for";

// ---------------------------------------------------------------------------
// The vocabulary
// ---------------------------------------------------------------------------

/// Which stored table a piece of evidence lives in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    /// A bracketed command execution — `evidence_bindings`.
    CheckRun,
    /// A completion claim with no proof in the record — `speaker_findings`.
    Finding,
    /// Something the recorder saw — `observations`.
    Observation,
}

impl EvidenceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CheckRun => "check_run",
            Self::Finding => "finding",
            Self::Observation => "observation",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "check_run" => Ok(Self::CheckRun),
            "finding" => Ok(Self::Finding),
            "observation" => Ok(Self::Observation),
            other => Err(CoreError::Config(format!(
                "unknown evidence kind {other:?}"
            ))),
        }
    }
}

/// Why a link exists. Four words, and the absence of a fifth is the point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkBasis {
    Declared,
    Named,
    Manual,
    NotBearing,
}

impl LinkBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Named => "named",
            Self::Manual => "manual",
            Self::NotBearing => "not-bearing",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "declared" => Ok(Self::Declared),
            "named" => Ok(Self::Named),
            "manual" => Ok(Self::Manual),
            "not-bearing" => Ok(Self::NotBearing),
            other => Err(CoreError::Config(format!("unknown link basis {other:?}"))),
        }
    }

    /// Only a person may write these two, and the database agrees.
    pub fn needs_actor(self) -> bool {
        matches!(self, Self::Manual | Self::NotBearing)
    }

    /// The copy-file slug for this basis.
    pub fn label_slug(self) -> &'static str {
        match self {
            Self::Declared => "basis_declared",
            Self::Named => "basis_named",
            Self::Manual => "basis_manual",
            Self::NotBearing => "basis_not_bearing",
        }
    }
}

/// The class of work a criterion asks about, in the vocabulary the store
/// already uses for a captured command (`evidence_bindings.scope_class`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunClass {
    Test,
    Build,
    Unclassified,
}

impl RunClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Build => "build",
            Self::Unclassified => "unclassified",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "test" => Self::Test,
            "build" => Self::Build,
            _ => Self::Unclassified,
        }
    }
}

/// The five state words, as the surfaces already spell them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Observed {
    Proven,
    NotProven,
    Couldnt,
    Loose,
    Fog,
}

impl Observed {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Proven => "proven",
            Self::NotProven => "not_proven",
            Self::Couldnt => "couldnt",
            Self::Loose => "loose",
            Self::Fog => "fog",
        }
    }
}

/// What one criterion's coverage is, as a closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    /// At least one linked item bears on it.
    Evidence,
    /// Nothing is linked at all.
    None,
    /// Nothing is linked, and the check this criterion's class asks for cannot
    /// run on this machine.
    AbsentTool,
    /// Something is linked and none of it satisfies anything.
    RanNothing,
    /// Something is linked, and none of what bears on this line was examined on
    /// the version this Change is bound to (T-3-2). A result about another
    /// version is not a smaller result; it is a result about something else.
    OtherVersion,
}

impl CoverageState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Evidence => "evidence",
            Self::None => "none",
            Self::AbsentTool => "absent_tool",
            Self::RanNothing => "ran_nothing",
            Self::OtherVersion => "other_version",
        }
    }

    /// The copy-file slug for the sentence. The sentence is looked up by a
    /// surface, never built here.
    pub fn slug(self) -> &'static str {
        match self {
            Self::Evidence => "coverage_evidence",
            Self::None => "coverage_no_test_ran",
            Self::AbsentTool => "coverage_tool_absent",
            Self::RanNothing => "coverage_ran_checked_nothing",
            Self::OtherVersion => "coverage_other_version",
        }
    }
}

// ---------------------------------------------------------------------------
// Reading the criterion side
// ---------------------------------------------------------------------------

/// Words that carry no subject of their own. The same idea as
/// `change_propose::SUBJECT_STOPWORDS`, restated for the token test rather than
/// the pairing key; short tokens are dropped by length below.
const TOKEN_STOPWORDS: &[&str] = &[
    "after", "again", "also", "and", "any", "are", "been", "before", "both", "case", "does",
    "done", "each", "every", "from", "have", "into", "must", "never", "not", "once", "only",
    "over", "same", "should", "still", "than", "that", "the", "their", "then", "there", "these",
    "they", "this", "those", "under", "when", "which", "while", "with", "work", "your",
];

/// Words that say a criterion is about running the tests.
const TEST_WORDS: &[&str] = &[
    "test",
    "tests",
    "tested",
    "testing",
    "suite",
    "suites",
    "spec",
    "specs",
    "regression",
    "regressions",
    "coverage",
];

/// Words that say a criterion is about the thing building, linting or
/// typechecking cleanly.
const BUILD_WORDS: &[&str] = &[
    "build",
    "builds",
    "building",
    "compile",
    "compiles",
    "compiled",
    "lint",
    "lints",
    "linting",
    "clippy",
    "fmt",
    "format",
    "formatting",
    "typecheck",
    "typechecks",
    "types",
];

/// Lower-case alphanumeric tokens of a piece of text, in order, deduplicated.
///
/// Pure, locale-free and allocation-simple: any character that is not
/// alphanumeric separates. Nothing here depends on the machine.
pub fn tokens_of(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split(|c: char| !c.is_alphanumeric()) {
        if raw.is_empty() {
            continue;
        }
        let token = raw.to_lowercase();
        if !out.contains(&token) {
            out.push(token);
        }
    }
    out
}

/// The tokens of a criterion's **subject** — what the sentence is about.
///
/// The subject comes from [`observable_shape`], the existing cut of a criterion
/// into subject / condition / outcome, so this module and the proposer agree on
/// what a criterion is about. A sentence that cannot be cut keeps its whole
/// text as the subject rather than losing its tokens: refusing to link a
/// criterion because our parser could not read it would hide it, and a
/// criterion nobody can see is worse than one linked loosely — the basis is on
/// the row either way.
///
/// Tokens shorter than four characters are dropped. "the", "is" and "run"
/// appear in almost every command line on a machine, and a token that matches
/// everything is a timing basis wearing a different hat.
pub fn subject_tokens(criterion: &str) -> Vec<String> {
    let subject = match observable_shape(criterion) {
        Ok(shape) if !shape.subject.trim().is_empty() => shape.subject,
        _ => criterion.to_string(),
    };
    tokens_of(&subject)
        .into_iter()
        .filter(|t| t.chars().count() >= 4 && !TOKEN_STOPWORDS.contains(&t.as_str()))
        .collect()
}

/// What class of check a criterion asks for, from its own words.
///
/// Test words win over build words, because "the tests build and pass" is a
/// sentence about tests. A criterion that names neither is `Unclassified` and
/// the `declared` basis never fires for it — an unknown is not a class.
pub fn criterion_class(criterion: &str) -> RunClass {
    let tokens = tokens_of(criterion);
    if tokens.iter().any(|t| TEST_WORDS.contains(&t.as_str())) {
        return RunClass::Test;
    }
    if tokens.iter().any(|t| BUILD_WORDS.contains(&t.as_str())) {
        return RunClass::Build;
    }
    RunClass::Unclassified
}

/// The standing accepted scope version and its content, unmasked.
///
/// Internal: the rule runs over the record as the record holds it, and the
/// masker is applied where text is served, not where it is compared.
fn standing_accepted(ledger: &Ledger, change_id: &str) -> Result<Option<(u32, AcceptedScope)>> {
    let row = ledger
        .conn()
        .query_row(
            "SELECT r.scope_version, r.criteria_json, r.check_profile_json
               FROM change_scope_revisions r
               JOIN change_scope_acceptances a
                 ON a.change_id=r.change_id AND a.scope_version=r.scope_version
              WHERE r.change_id=?1
              ORDER BY r.scope_version DESC LIMIT 1",
            params![change_id],
            |row| {
                Ok((
                    row.get::<_, u32>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((version, criteria, profile)) = row else {
        return Ok(None);
    };
    let parse = |raw: &str| serde_json::from_str::<Vec<String>>(raw).unwrap_or_default();
    Ok(Some((
        version,
        AcceptedScope {
            criteria: parse(&criteria),
            check_profile: parse(&profile),
            ..AcceptedScope::default()
        },
    )))
}

// ---------------------------------------------------------------------------
// Reading the evidence side
// ---------------------------------------------------------------------------

/// One piece of already-stored evidence that a Change is allowed to look at.
///
/// Built from typed columns of the table it lives in and nothing else. `name`
/// is that table's own typed name field — the one the `named` basis reads —
/// and it is the only free-ish text here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub kind: EvidenceKind,
    pub id: String,
    pub session_id: String,
    pub name: String,
    pub class: RunClass,
    pub observed: Observed,
    /// True when this evidence satisfies nothing, whatever it is attached to.
    pub checked_nothing: bool,
    /// T-3-4 (`B7.1`/`B7.6`) — what became of the EXECUTION, as against what
    /// it observed. `NotRun` for evidence that is not an execution at all (a
    /// completion claim, an observation) and for a run recorded before this
    /// dimension existed; the veto below only ever fires on a state the record
    /// positively holds, so an old binding keeps reading exactly as it did.
    pub execution: crate::run_execution::ExecutionState,
    /// T-3-6-F3 (`Q2`) — the failing assertion's own words, read out of the
    /// stream at record time. `None` for evidence that is not a captured run,
    /// and for a run whose output carried no failure block this product
    /// recognised. An absent line is never rendered as "it passed".
    pub failure_line: Option<String>,
    /// T-3-6-F3 (`B16.10`) — what this run was DECLARED to be. `None` when
    /// nobody said; never guessed from a branch name and never inferred from
    /// the failure.
    pub evidence_class: Option<crate::run_record::EvidenceClass>,
    pub recorded_at: String,
}

impl Candidate {
    /// Does this bear on anything at all? The inverse of `checked_nothing`,
    /// named the positive way because that is how the coverage state reads.
    pub fn bears(&self) -> bool {
        !self.checked_nothing
    }
}

/// The observed result and whether it satisfies anything, from typed columns.
///
/// The table is in this module's docs, section 3, and both halves of every arm
/// are here rather than split across callers.
fn observed_of(
    kind: EvidenceKind,
    class: RunClass,
    exit_code: Option<i64>,
    term_signal: Option<i64>,
    streams_complete: bool,
    tally: Option<crate::test_tally::TestTally>,
    dependencies_unavailable: bool,
) -> (Observed, bool) {
    match kind {
        // A completion claim with nothing behind it: promised, and the record
        // holds no follow-through. It bears — it is exactly the kind of thing a
        // person needs to see under a criterion — and it is never a pass.
        EvidenceKind::Finding => (Observed::Loose, false),
        // Something the recorder saw. It is not a check and never satisfies a
        // criterion on its own; a person may still want it beside one.
        EvidenceKind::Observation => (Observed::Couldnt, true),
        EvidenceKind::CheckRun => {
            if term_signal.is_some() {
                return (Observed::Couldnt, true);
            }
            // OFFLINE-BUILD-OUTCOME (D-051): the build could not fetch what it
            // needs, so it never reached the project's code. Its non-zero exit
            // is about this machine, not the code: it checked nothing, and it
            // is never Not proven. The run sheet reads the same record field
            // through the same function (`run_execution::
            // dependencies_unavailable`).
            if dependencies_unavailable {
                return (Observed::Couldnt, true);
            }
            let Some(exit) = exit_code else {
                return (Observed::Couldnt, true);
            };
            if exit != 0 {
                return (Observed::NotProven, false);
            }
            if !streams_complete {
                // It finished, and the record does not hold what it said.
                return (Observed::Couldnt, true);
            }
            match class {
                // A build or lint check's evidence IS its exit code and nothing
                // else (`crates/verify/src/check.rs`). Zero is the evidence.
                RunClass::Build => (Observed::Proven, false),
                // A test command that exited 0 proves that a command exited 0,
                // and nothing more: zero tests, every test filtered out and a
                // full green suite are one exit code, and `Q3` forbids
                // guessing in the direction of satisfaction.
                //
                // T-3-6-F1 (`Q1`): the exit code is still not the evidence —
                // the COUNT is. When the wrapper read a `test result:` line off
                // the stream it captured, the record holds a typed count
                // ([`crate::test_tally`]) and can tell those three apart. At
                // least one test passed and none failed is a suite that ran;
                // anything else — no count at all, or a count of zero — keeps
                // the old answer, in the old words.
                RunClass::Test => match tally {
                    Some(t) if t.counts_a_suite() => (Observed::Proven, false),
                    _ => (Observed::Couldnt, true),
                },
                RunClass::Unclassified => (Observed::Couldnt, true),
            }
        }
    }
}

/// Whether a stored binding's streams were both captured whole.
///
/// Read off the stored document's typed enum, not off a string search: a
/// `Truncated` capture means the record does not hold what the command said.
fn streams_complete(doc: &str) -> bool {
    #[derive(serde::Deserialize)]
    struct Shape {
        stdout: crate::evidence_binding::StreamCapture,
        stderr: crate::evidence_binding::StreamCapture,
    }
    let Ok(shape) = serde_json::from_str::<Shape>(doc) else {
        return false;
    };
    matches!(
        shape.stdout,
        crate::evidence_binding::StreamCapture::Captured { .. }
    ) && matches!(
        shape.stderr,
        crate::evidence_binding::StreamCapture::Captured { .. }
    )
}

/// Every piece of evidence one Change may look at, in a stable order.
///
/// **The gate is the gate.** A session reaches this list only when
/// [`applicability`] says it applies: confirmed by a person, and changing at
/// least one path inside the accepted scope. Nothing here re-decides that.
pub fn candidates(ledger: &Ledger, change_id: &str) -> Result<Vec<Candidate>> {
    Ok(candidates_and_why(ledger, change_id)?.0)
}

/// The same list, with the reason it is empty when it is.
///
/// One function rather than two because the gate must have **one** caller in
/// this module: `crates/core/tests/change_scope_gate.rs` enumerates every
/// caller of `applicability` from the sources and fails when a second one
/// appears, and a second reader of the gate is exactly how two ideas of
/// "linked" start. The reason is the first refusal in session order, in the
/// link layer's own vocabulary, so the sentence a disabled control shows is
/// looked up rather than built here.
pub fn candidates_and_why(
    ledger: &Ledger,
    change_id: &str,
) -> Result<(Vec<Candidate>, Option<&'static str>)> {
    let sessions = crate::change_link::linked_sessions(ledger, change_id)?;
    let mut out = Vec::new();
    let mut refused: Option<&'static str> = None;
    for session_id in sessions {
        let verdict = applicability(ledger, change_id, &session_id)?;
        if !verdict.applies() {
            if refused.is_none() {
                refused = Some(verdict.slug());
            }
            continue;
        }
        out.extend(check_runs_of(ledger, &session_id)?);
        out.extend(findings_of(ledger, &session_id)?);
        out.extend(observations_of(ledger, &session_id)?);
    }
    out.sort_by(|a, b| (a.kind, &a.recorded_at, &a.id).cmp(&(b.kind, &b.recorded_at, &b.id)));
    let why = if out.is_empty() {
        Some(refused.unwrap_or("not_applicable_no_link"))
    } else {
        None
    };
    Ok((out, why))
}

fn check_runs_of(ledger: &Ledger, session_id: &str) -> Result<Vec<Candidate>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, scope_class, scope_key, exit_code, term_signal, bound_at, doc
           FROM evidence_bindings WHERE source_session=?1 ORDER BY seq",
    )?;
    let rows = statement.query_map(params![session_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    // T-3-4 (`B7.6`): what became of each run's EXECUTION, read once for the
    // whole store rather than per row. A run the record holds no execution
    // state for reads exactly as it did before this dimension existed.
    let executions = crate::run_execution::by_binding(ledger)?;
    // T-3-6-F1 (`Q1`): what each run COUNTED, read once for the whole store
    // the same way. A run with no row here is a run whose output carried no
    // parsable count, and it reads exactly as it did before this field
    // existed — an absent count is never a zero and never a pass.
    let tallies = crate::test_tally::by_binding(ledger)?;
    // T-3-6-F3 (`Q2`): what each run SAID when it failed, and what class of
    // evidence it was declared to be, read once for the whole store the same
    // way. Neither field moves a verdict; both are things a person reads
    // beside a finding the record already holds.
    let run_records = crate::run_record::by_binding(ledger)?;
    // **T-3-5 verifier finding 2, half one.** `by_binding` only finds an
    // execution that named the binding it produced, and a run a PERSON stopped
    // never does: `cancel-run` appends `cancelled` while the wrapper is still
    // inside `wait()`, with no binding to name yet, and the wrapper's own
    // ending — the one carrying `binding_id` — is then refused, correctly,
    // because a run may end once. The stopped run therefore produced a binding
    // that no execution row pointed at, and the coverage layer read it as a
    // run nothing is recorded about.
    //
    // The join that survives both orderings is the run's own key: `check`
    // starts the execution under `run_key = source_session` and stores the
    // binding under that same `source_session`, one binding per invocation. So
    // the binding lookup is asked first, and this is the fallback — never an
    // override.
    let by_run_key = crate::run_execution::latest(ledger, session_id)?;
    let mut out = Vec::new();
    for row in rows {
        let (id, scope_class, scope_key, exit_code, term_signal, bound_at, doc) = row?;
        let class = RunClass::parse(&scope_class);
        let record = executions.get(&id).or(by_run_key.as_ref());
        let execution = record
            .map(|r| r.state)
            .unwrap_or(crate::run_execution::ExecutionState::NotRun);
        let (mut observed, mut checked_nothing) = observed_of(
            EvidenceKind::CheckRun,
            class,
            exit_code,
            term_signal,
            streams_complete(&doc),
            tallies.get(&id).copied(),
            record
                .is_some_and(|r| crate::run_execution::dependencies_unavailable(r.state, r.reason)),
        );
        // **NEVER A PASS (`B7.6`).** An execution that did not reach the end
        // cannot satisfy anything, however its last captured line reads. The
        // run stays listed and stays readable — a check stopped half-way is a
        // fact a person needs — and it settles nothing. `NotRun` is the
        // absence of a record rather than a claim about one, so it does not
        // fire: the veto only ever acts on a state the store positively holds.
        if execution != crate::run_execution::ExecutionState::NotRun && !execution.settles() {
            observed = Observed::Couldnt;
            checked_nothing = true;
        }
        let stored = run_records.get(&id);
        out.push(Candidate {
            kind: EvidenceKind::CheckRun,
            id,
            session_id: session_id.to_string(),
            name: scope_key,
            class,
            observed,
            checked_nothing,
            execution,
            failure_line: stored.and_then(|r| r.failure_line.clone()),
            evidence_class: stored.and_then(|r| r.evidence_class),
            recorded_at: bound_at,
        });
    }
    Ok(out)
}

fn findings_of(ledger: &Ledger, session_id: &str) -> Result<Vec<Candidate>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, claim_marker, recorded_at FROM speaker_findings
          WHERE session_id=?1 ORDER BY seq",
    )?;
    let rows = statement.query_map(params![session_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, marker, recorded_at) = row?;
        let (observed, checked_nothing) = observed_of(
            EvidenceKind::Finding,
            RunClass::Unclassified,
            None,
            None,
            true,
            None,
            false,
        );
        out.push(Candidate {
            kind: EvidenceKind::Finding,
            id,
            session_id: session_id.to_string(),
            name: marker,
            class: RunClass::Unclassified,
            observed,
            checked_nothing,
            execution: crate::run_execution::ExecutionState::NotRun,
            failure_line: None,
            evidence_class: None,
            recorded_at,
        });
    }
    Ok(out)
}

fn observations_of(ledger: &Ledger, session_id: &str) -> Result<Vec<Candidate>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, observation_type, observed_at FROM observations
          WHERE source_session=?1 ORDER BY seq",
    )?;
    let rows = statement.query_map(params![session_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, kind_word, observed_at) = row?;
        let (observed, checked_nothing) = observed_of(
            EvidenceKind::Observation,
            RunClass::Unclassified,
            None,
            None,
            true,
            None,
            false,
        );
        out.push(Candidate {
            kind: EvidenceKind::Observation,
            id,
            session_id: session_id.to_string(),
            name: kind_word,
            class: RunClass::Unclassified,
            observed,
            checked_nothing,
            execution: crate::run_execution::ExecutionState::NotRun,
            failure_line: None,
            evidence_class: None,
            recorded_at: observed_at,
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// THE RULE
// ---------------------------------------------------------------------------

/// One link the rule would write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleLink {
    pub criterion_index: u32,
    pub kind: EvidenceKind,
    pub evidence_id: String,
    pub session_id: String,
    pub basis: LinkBasis,
    /// What matched: the profile entry, or the subject token. Never a sentence.
    pub detail: String,
}

/// **The rule.** Every automatic link, for one accepted scope, over one
/// candidate list. Pure: no store, no clock, no machine.
///
/// The order is fixed — criterion, then candidate, then `declared` before
/// `named` — so the same inputs produce the same rows in the same order on
/// every machine. A pair that qualifies both ways gets one row, `declared`,
/// because that is the stronger statement about why it is there.
pub fn links_for(
    criteria: &[String],
    check_profile: &[String],
    candidates: &[Candidate],
) -> Vec<RuleLink> {
    let declared_classes = profile_classes(check_profile);
    let mut out = Vec::new();
    for (index, criterion) in criteria.iter().enumerate() {
        let number = index as u32 + 1;
        let class = criterion_class(criterion);
        let tokens = subject_tokens(criterion);
        for candidate in candidates {
            // Only a check run has a typed field the rule can read. A finding
            // or an observation needs a person, and says whose name it is on.
            if candidate.kind != EvidenceKind::CheckRun {
                continue;
            }
            if class != RunClass::Unclassified && candidate.class == class {
                if let Some(entry) = declared_classes.get(&class) {
                    out.push(RuleLink {
                        criterion_index: number,
                        kind: candidate.kind,
                        evidence_id: candidate.id.clone(),
                        session_id: candidate.session_id.clone(),
                        basis: LinkBasis::Declared,
                        detail: entry.clone(),
                    });
                    continue;
                }
            }
            let name_tokens = tokens_of(named_text(&candidate.name));
            if let Some(hit) = tokens.iter().find(|t| name_tokens.contains(t)) {
                out.push(RuleLink {
                    criterion_index: number,
                    kind: candidate.kind,
                    evidence_id: candidate.id.clone(),
                    session_id: candidate.session_id.clone(),
                    basis: LinkBasis::Named,
                    detail: hit.clone(),
                });
            }
        }
    }
    out
}

/// The part of a run's name the `named` basis may read: the command, never the
/// folder it ran in (NAMED-LINK-CWD).
///
/// A check run's name is its §5i identity key, `bash:{cwd}:{cmd}`. The folder
/// is where the run happened, not a statement about what it checked: a project
/// that lives in `…/shop/` must not have every passing `cargo test` linked to a
/// line about the shop. The split is the one every other reader of the key
/// already uses (`identity::is_test_invocation`,
/// `evidence_binding::establishes_all_tests`): after `bash:`, the first `:`.
/// A command's own colons (`importer::keeps`) stay in the command. A name that
/// is not a bash key carries no folder and is read whole, as before.
fn named_text(name: &str) -> &str {
    name.strip_prefix("bash:")
        .and_then(|rest| rest.split_once(':'))
        .map_or(name, |(_cwd, command)| command)
}

/// Which classes the accepted check profile declares a check for, and the
/// profile entry that declares each. First entry wins, in the profile's own
/// stored order, so the detail is a fact about the record rather than a choice.
fn profile_classes(check_profile: &[String]) -> BTreeMap<RunClass, String> {
    let mut out: BTreeMap<RunClass, String> = BTreeMap::new();
    for id in check_profile {
        let Some(category) = crate::change_propose::profile_check_category(id) else {
            continue;
        };
        let class = class_of_category(category);
        if class == RunClass::Unclassified {
            continue;
        }
        out.entry(class).or_insert_with(|| id.clone());
    }
    out
}

/// A profile check's category, in the run-class vocabulary. Stated here, in
/// one table, because the two vocabularies are different lists written for
/// different reasons and a silent guess between them is how a lint run ends up
/// under a criterion about tests.
pub fn class_of_category(category: &str) -> RunClass {
    match category {
        "tests" | "behaviour" | "mutation" => RunClass::Test,
        "lint" | "static analysis" | "secrets" => RunClass::Build,
        _ => RunClass::Unclassified,
    }
}

/// The id of one link row: the SHA-256 of the tuple it is.
///
/// Content-addressed so re-running the rule over an unchanged record inserts
/// nothing, and so a link, a tombstone and a re-link are three distinguishable
/// rows rather than one row edited three times.
pub fn link_id(
    change_id: &str,
    scope_version: u32,
    criterion_index: u32,
    kind: EvidenceKind,
    evidence_id: &str,
    basis: LinkBasis,
    asserted_at: &str,
) -> String {
    // The automatic bases carry no time in their id: the same record must
    // produce the same row whenever the rule runs. A person's act does carry
    // one, because two acts by the same person at different times are two
    // facts.
    let time = if basis.needs_actor() { asserted_at } else { "" };
    let payload = format!(
        "cce/v1\u{1f}{change_id}\u{1f}{scope_version}\u{1f}{criterion_index}\u{1f}{}\u{1f}{evidence_id}\u{1f}{}\u{1f}{time}",
        kind.as_str(),
        basis.as_str()
    );
    format!("cce_{}", sha256_hex(payload.as_bytes()))
}

// ---------------------------------------------------------------------------
// Writes. Every one appends.
// ---------------------------------------------------------------------------

/// What one run of the rule wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeriveReport {
    pub change_id: String,
    pub scope_version: u32,
    pub considered: u32,
    pub inserted: u32,
    /// Rows the rule would have written that a person has already said do not
    /// bear. They are not written again: a tombstone is an answer, not a race.
    pub tombstoned: u32,
}

/// The declaration used to attach an unsuccessful dependency-fetch attempt.
///
/// The binding still describes `cargo fetch`, and every normal reader keeps
/// that actual class and command. Only the rule's input view uses the recorded
/// requested command: it explains *which check could not start or was stopped*,
/// not what the fetch proved. The existing Couldn't-check veto remains unchanged.
fn requests_for_linking(ledger: &Ledger, pool: &[Candidate]) -> Result<Vec<Candidate>> {
    use crate::observation::{self, ObservationType, ReceiptTerminal};
    use crate::run_execution::ExecutionState;
    let mut requests = pool.to_vec();
    for request in &mut requests {
        if request.kind != EvidenceKind::CheckRun
            || request.observed != Observed::Couldnt
            || !request.checked_nothing
        {
            continue;
        }
        let Some(execution) = crate::run_execution::latest(ledger, &request.session_id)? else {
            continue;
        };
        let stopped = matches!(
            execution.state,
            ExecutionState::Cancelled | ExecutionState::Interrupted
        );
        // Cancellation can land before the wrapper writes its binding. The
        // same run-key join used by check_runs_of preserves that ordering;
        // an execution that does name a binding must still name this one.
        let binding_matches = match execution.binding_id.as_deref() {
            Some(id) => id == request.id,
            None => stopped,
        };
        if !(stopped
            || crate::run_execution::dependencies_unavailable(execution.state, execution.reason))
            || !binding_matches
        {
            continue;
        }
        let Some(stored) = crate::evidence_binding::load(ledger, &request.id)? else {
            continue;
        };
        let actual_command = crate::evidence_binding::rendered_command(&stored.binding.argv);
        for row in observation::for_session(ledger, &request.session_id)? {
            if row.source_adapter != "thruline"
                || row.observation_type() != Some(ObservationType::CommandFinished)
            {
                continue;
            }
            let receipt = row.receipt()?;
            if receipt.extra.get("phase").and_then(|value| value.as_str())
                != Some("dependency_fetch")
                || !matches!(&receipt.terminal, ReceiptTerminal::ProcessExecution {
                    command, exit_code,
                } if command == &actual_command
                    && *exit_code == stored.binding.exit_code
                    && (stopped || exit_code.is_some_and(|code| code != 0)))
            {
                continue;
            }
            let Some(argv) = receipt
                .extra
                .get("requested_argv")
                .and_then(|value| value.as_array())
            else {
                continue;
            };
            let Some(argv) = argv
                .iter()
                .map(|arg| arg.as_str())
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let intent =
                crate::evidence_binding::classify_command(&argv.join(" "), &stored.binding.root);
            request.class = RunClass::parse(intent.class.as_str());
            request.name = execution.name.clone();
            break;
        }
    }
    Ok(requests)
}

/// Run the rule and append the rows it produces that are not already there.
///
/// Idempotent by construction: the ids are content-addressed, so a second run
/// over an unchanged record inserts nothing and reports it.
pub fn derive_links(ledger: &Ledger, change_id: &str, at: &str) -> Result<DeriveReport> {
    let Some((scope_version, scope)) = standing_accepted(ledger, change_id)? else {
        return Ok(DeriveReport {
            change_id: change_id.to_string(),
            scope_version: 0,
            considered: 0,
            inserted: 0,
            tombstoned: 0,
        });
    };
    let pool = candidates(ledger, change_id)?;
    let requests = requests_for_linking(ledger, &pool)?;
    let wanted = links_for(&scope.criteria, &scope.check_profile, &requests);
    let standing = standing_rows(ledger, change_id, scope_version)?;

    let mut inserted = 0u32;
    let mut tombstoned = 0u32;
    for link in &wanted {
        let key = (link.criterion_index, link.kind, link.evidence_id.clone());
        if standing.get(&key).map(|r| r.basis) == Some(LinkBasis::NotBearing) {
            tombstoned += 1;
            continue;
        }
        let id = link_id(
            change_id,
            scope_version,
            link.criterion_index,
            link.kind,
            &link.evidence_id,
            link.basis,
            at,
        );
        let existing: Option<String> = ledger
            .conn()
            .query_row(
                "SELECT id FROM change_criterion_evidence WHERE id=?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        if existing.is_some() {
            continue;
        }
        ledger.conn().execute(
            "INSERT INTO change_criterion_evidence(id, change_id, scope_version,
                 criterion_index, evidence_kind, evidence_id, session_id, basis,
                 detail, actor, asserted_at, reason)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,NULL,?10,NULL)",
            params![
                id,
                change_id,
                scope_version,
                link.criterion_index,
                link.kind.as_str(),
                link.evidence_id,
                link.session_id,
                link.basis.as_str(),
                link.detail,
                at,
            ],
        )?;
        inserted += 1;
    }
    Ok(DeriveReport {
        change_id: change_id.to_string(),
        scope_version,
        considered: wanted.len() as u32,
        inserted,
        tombstoned,
    })
}

/// One person's act on one link: `manual` to add, `not-bearing` to remove.
///
/// Refused without a named actor, before the store is touched — an act nobody
/// signed is not an act (`B5.6`). Refused when the evidence is not something
/// this Change may look at, which is the same gate everything else goes
/// through: a person cannot hand-link a run from work they never confirmed.
/// A refusal, in the copy file's Plain words. Nothing in this module writes an
/// English sentence of its own: a person who is told "no" is told it in the
/// same words the surface would use, and the slug is the only thing crossing
/// the boundary.
fn refusal(slug: &'static str) -> CoreError {
    CoreError::Config(
        crate::voice::coverage_sentence(slug, "", 0)
            .map(|(plain, _)| plain)
            .unwrap_or_else(|| format!("refused: {slug}")),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkAct {
    pub criterion_index: u32,
    pub kind: EvidenceKind,
    pub evidence_id: String,
    pub basis: LinkBasis,
    pub actor: String,
    pub reason: String,
    pub at: String,
}

pub fn act(ledger: &Ledger, change_id: &str, request: &LinkAct) -> Result<String> {
    let LinkAct {
        criterion_index,
        kind,
        evidence_id,
        basis,
        actor,
        reason,
        at,
    } = request;
    let (criterion_index, kind, basis) = (*criterion_index, *kind, *basis);
    let (evidence_id, actor, reason, at) = (
        evidence_id.as_str(),
        actor.as_str(),
        reason.as_str(),
        at.as_str(),
    );
    if !basis.needs_actor() {
        return Err(refusal("link_needs_actor"));
    }
    if actor.trim().is_empty() {
        return Err(refusal("link_needs_actor"));
    }
    let Some((scope_version, scope)) = standing_accepted(ledger, change_id)? else {
        return Err(refusal("link_no_criteria"));
    };
    if criterion_index < 1 || criterion_index as usize > scope.criteria.len() {
        return Err(refusal("link_unknown_criterion"));
    }
    let pool = candidates(ledger, change_id)?;
    let Some(found) = pool.iter().find(|c| c.kind == kind && c.id == evidence_id) else {
        return Err(refusal("link_not_in_reach"));
    };
    let id = link_id(
        change_id,
        scope_version,
        criterion_index,
        kind,
        evidence_id,
        basis,
        at,
    );
    ledger.conn().execute(
        "INSERT INTO change_criterion_evidence(id, change_id, scope_version,
             criterion_index, evidence_kind, evidence_id, session_id, basis,
             detail, actor, asserted_at, reason)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,NULL,?9,?10,?11)",
        params![
            id,
            change_id,
            scope_version,
            criterion_index,
            kind.as_str(),
            evidence_id,
            found.session_id,
            basis.as_str(),
            actor,
            at,
            reason,
        ],
    )?;
    Ok(id)
}

// ---------------------------------------------------------------------------
// Reads. Masked at the boundary.
// ---------------------------------------------------------------------------

/// One stored link row, as the store holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredRow {
    id: String,
    criterion_index: u32,
    kind: EvidenceKind,
    evidence_id: String,
    session_id: String,
    basis: LinkBasis,
    detail: Option<String>,
    actor: Option<String>,
    asserted_at: String,
    reason: Option<String>,
}

type PairKey = (u32, EvidenceKind, String);

/// Every row for one Change and scope version, oldest first.
///
/// **Ordered by `rowid`, not by `asserted_at`.** Two acts inside one second
/// carry the same recorded time — the stamps are whole seconds — and resolving
/// them by id would resolve them by a hash, which is to say arbitrarily: a
/// person who linked something and then took it off again would sometimes see
/// it still linked. `rowid` is the order the store actually accepted them in,
/// and it is the same tie-break `change_link::standing_link` already uses for
/// the same reason. Found by `linking_and_unlinking_both_append_and_the_screen_follows`
/// on the second run of the break-and-restore, which put both acts in one
/// second where the hand walkthrough had put them minutes apart.
fn all_rows(ledger: &Ledger, change_id: &str, scope_version: u32) -> Result<Vec<StoredRow>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, criterion_index, evidence_kind, evidence_id, session_id, basis,
                detail, actor, asserted_at, reason
           FROM change_criterion_evidence
          WHERE change_id=?1 AND scope_version=?2
          ORDER BY rowid",
    )?;
    let rows = statement.query_map(params![change_id, scope_version], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, u32>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, String>(8)?,
            row.get::<_, Option<String>>(9)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, index, kind, evidence_id, session_id, basis, detail, actor, at, reason) = row?;
        out.push(StoredRow {
            id,
            criterion_index: index,
            kind: EvidenceKind::parse(&kind)?,
            evidence_id,
            session_id,
            basis: LinkBasis::parse(&basis)?,
            detail,
            actor,
            asserted_at: at,
            reason,
        });
    }
    Ok(out)
}

/// The standing row for each (criterion, evidence) pair: the latest one.
fn standing_rows(
    ledger: &Ledger,
    change_id: &str,
    scope_version: u32,
) -> Result<BTreeMap<PairKey, StoredRow>> {
    let mut out: BTreeMap<PairKey, StoredRow> = BTreeMap::new();
    for row in all_rows(ledger, change_id, scope_version)? {
        out.insert(
            (row.criterion_index, row.kind, row.evidence_id.clone()),
            row,
        );
    }
    Ok(out)
}

/// One linked piece of evidence, as it is served.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceItemView {
    pub link_id: String,
    pub kind: EvidenceKind,
    pub evidence_id: String,
    pub session_id: String,
    pub basis: LinkBasis,
    /// The copy-file slug for the basis sentence.
    pub basis_slug: &'static str,
    pub detail: Option<String>,
    pub actor: Option<String>,
    pub asserted_at: String,
    pub reason: Option<String>,
    /// The evidence's own typed name, masked.
    pub name: String,
    pub observed: Observed,
    /// False when this satisfies nothing, whatever it is attached to.
    pub bears: bool,
    /// The copy-file slug for what the item itself says.
    pub note_slug: Option<&'static str>,
    /// Whether this examination is about the version this Change is bound to
    /// (T-3-2, `B6.3`/`B6.4`). `None` for a piece of evidence that is not an
    /// examination — a completion claim is not a thing that was run — and for
    /// every item on a Change that has not written down what it is about,
    /// because a mismatch against nothing is not a fact.
    pub applies: Option<bool>,
    /// The copy-file slug for "checked on this version" or the mismatch
    /// sentence. `None` exactly when `applies` is `None`.
    pub applies_slug: Option<&'static str>,
    /// T-S-2: whether the latest `change_criterion_freshness` row for this
    /// link says `no_longer_applies` — a relevant path changed since this
    /// evidence was produced, or the criterion's own text changed under it.
    /// `None` when no freshness row exists yet (unassessed, not stale); a
    /// `Some(true)` reads "still applies" and is otherwise uninformative.
    pub fresh: Option<bool>,
    /// The copy-file slug for the freshness sentence (`applicability_*`).
    /// `None` exactly when `fresh` is `None`.
    pub fresh_slug: Option<&'static str>,
    /// NEVER-ASSESSED-NOT-READY-R2: when `fresh` is `None`, whether the
    /// next `change prepare` can assess this item at all
    /// (`evidence_applicability::assessable_by_next_bind`, the sweep's own
    /// rule). `Some(false)` means nothing on record says what the files were
    /// when it was produced, so no prepare will settle it. `None` exactly
    /// when `fresh` is `Some` — it has been assessed.
    pub assessable: Option<bool>,
    /// T-3-4 (`B7.1`) — what became of the EXECUTION behind this item, served
    /// beside the observed result and **never merged with it** (design record
    /// §3.8: a screen shows two fields, never one badge).
    pub execution: crate::run_execution::ExecutionState,
    /// The copy-file slug for the execution sentence.
    pub execution_slug: &'static str,
    /// T-3-6-F3 (`Q2`, `B16.8`) — the failing assertion's own words, masked
    /// like every other piece of text served from this layer. `None` when the
    /// record holds no line from the failure; the screens say that in those
    /// words and never invent one.
    pub failure_line: Option<String>,
    /// T-3-6-F3 (`B16.10`) — the class this run was declared to be, as the
    /// store holds it. `None` when nobody said.
    pub evidence_class: Option<crate::run_record::EvidenceClass>,
    /// The copy-file slug for the evidence-class sentence. `None` exactly when
    /// `evidence_class` is `None`.
    pub evidence_class_slug: Option<&'static str>,
}

/// One accepted criterion and what the record holds for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CriterionCoverage {
    pub index: u32,
    pub text: String,
    pub class: RunClass,
    pub state: CoverageState,
    pub state_slug: &'static str,
    /// The program that is not on this machine, when that is the state.
    pub absent_tool: Option<String>,
    pub items: Vec<EvidenceItemView>,
    /// Links a person has said do not bear. Kept and shown: unlinking is an
    /// act with a name on it, not a disappearance.
    pub removed: Vec<EvidenceItemView>,
}

/// Something a person could link by hand, with enough to recognise it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CandidateView {
    pub kind: EvidenceKind,
    pub evidence_id: String,
    pub session_id: String,
    pub name: String,
    pub observed: Observed,
    pub bears: bool,
    /// T-3-4 (`B7.1`) — the execution half, here too, so a candidate a person
    /// is being offered to link cannot look finished when it was stopped.
    pub execution: crate::run_execution::ExecutionState,
    pub execution_slug: &'static str,
}

/// What the record holds for every accepted criterion of one Change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoverageView {
    pub schema_version: u32,
    pub change_id: String,
    /// `None` when nothing has been accepted as counting as done.
    pub scope_version: Option<u32>,
    pub criteria: Vec<CriterionCoverage>,
    pub candidates: Vec<CandidateView>,
    /// T-3-4 (`B7.1`, `Q12`) — every run this Change has an execution record
    /// for, latest row per run, **including the ones that never produced a
    /// binding at all**. An interrupted run is exactly that case: the process
    /// died before the wrapper could store anything, so it is invisible to the
    /// candidate pool and would be invisible to the screen. A run that was cut
    /// short has to be visible or the surface silently reads as if it never
    /// happened, which is the `Q12` failure in its purest form.
    pub executions: Vec<crate::run_execution::ExecutionRecord>,
    /// The technical layer's count: how many link rows this Change has.
    pub link_rows: u64,
    /// Why there is nothing a person could link by hand, when there is
    /// nothing. One of `change_link`'s own refusal slugs, so the sentence a
    /// disabled control shows is the same sentence the link layer would give,
    /// looked up rather than built. `None` when there IS something to link.
    ///
    /// Rule 6 is why this field exists: "Confirm the work that belongs to this
    /// Change" is the wrong enabler on a Change whose work is already
    /// confirmed and whose accepted scope names no files, and a control that
    /// asks a person to do what they have already done is worse than one that
    /// says nothing.
    pub nothing_to_link_slug: Option<&'static str>,
    /// Why no piece of evidence here says which version it was checked on
    /// (T-3-2b). `Some` exactly when this Change has no record of what it is
    /// about, which is the one case where `applies` is `null` on every item:
    /// a Change opened by an earlier build, before binding wrote one. The
    /// sentence names the act that fills it; `None` on every Change opened on
    /// this build, and a test asserts that.
    pub nothing_bound_slug: Option<&'static str>,
    /// Which of this Change's linked sessions are one captured execution
    /// rather than a recorded conversation (T-3-2b). A reader asks a
    /// transcript for its story and an execution for its exit code; served so
    /// a screen does not ask an execution for a story it never had.
    pub execution_sessions: Vec<String>,
    pub masking: crate::redact::Treatment,
}

/// Everything the Verify view and the packet render from.
///
/// Reads. It never derives, so opening a screen cannot write a row; a caller
/// that wants the rule run asks [`derive_links`] first, and says so.
pub fn coverage(
    ledger: &Ledger,
    change_id: &str,
    environment: &ProfileEnvironment,
) -> Result<CoverageView> {
    let mut record = MaskRecord::default();
    let Some((scope_version, scope)) = standing_accepted(ledger, change_id)? else {
        return Ok(CoverageView {
            schema_version: COVERAGE_VIEW_SCHEMA_VERSION,
            change_id: change_id.to_string(),
            scope_version: None,
            criteria: Vec::new(),
            candidates: Vec::new(),
            executions: crate::run_execution::all(ledger, Some(change_id))?,
            link_rows: 0,
            nothing_to_link_slug: Some("not_applicable_no_scope"),
            nothing_bound_slug: nothing_bound(ledger, change_id)?,
            execution_sessions: crate::change_link::execution_sessions(ledger, change_id)?,
            masking: record.to_treatment(),
        });
    };
    let intended = crate::examined_inputs::standing(
        ledger,
        crate::examined_inputs::SubjectKind::Intended,
        change_id,
    )?;
    let (pool, why_nothing) = candidates_and_why(ledger, change_id)?;
    let by_id: BTreeMap<(EvidenceKind, String), &Candidate> =
        pool.iter().map(|c| ((c.kind, c.id.clone()), c)).collect();
    let rows = all_rows(ledger, change_id, scope_version)?;
    let link_rows = rows.len() as u64;
    let standing = standing_rows(ledger, change_id, scope_version)?;
    // T-S-2: the latest freshness row per link, so a criterion whose only
    // evidence has since been invalidated does not read Proven off a run the
    // record itself says no longer applies to this version.
    let freshness = crate::evidence_applicability::standing_for_change(ledger, change_id)?;

    let absent = absent_tools(&scope.check_profile, environment);

    let mut criteria = Vec::new();
    for (index, text) in scope.criteria.iter().enumerate() {
        let number = index as u32 + 1;
        let class = criterion_class(text);
        let mut items = Vec::new();
        let mut removed = Vec::new();
        for (key, row) in &standing {
            if key.0 != number {
                continue;
            }
            let candidate = by_id.get(&(row.kind, row.evidence_id.clone()));
            let applicability =
                applicability_of(ledger, intended.as_ref(), row.kind, &row.evidence_id)?;
            let fresh = freshness.get(&row.id);
            let mut view = item_view(row, candidate.copied(), applicability, fresh, &mut record);
            if fresh.is_none() {
                view.assessable = Some(crate::evidence_applicability::assessable_by_next_bind(
                    ledger,
                    change_id,
                    row.kind.as_str(),
                    &row.evidence_id,
                )?);
            }
            if row.basis == LinkBasis::NotBearing {
                removed.push(view);
            } else {
                items.push(view);
            }
        }
        let state = if items.is_empty() {
            match absent.get(&class) {
                Some(_) => CoverageState::AbsentTool,
                None => CoverageState::None,
            }
        } else if items
            .iter()
            .any(|i| i.bears && i.applies != Some(false) && i.fresh != Some(false))
        {
            CoverageState::Evidence
        } else if items.iter().any(|i| i.applies != Some(false)) {
            // Something that was examined on THIS version is linked here, and
            // it settled nothing. That is the more informative sentence, so it
            // wins over the version one: a person is not told their evidence is
            // about another version when some of it is not.
            CoverageState::RanNothing
        } else {
            // Everything linked here was examined on something else.
            CoverageState::OtherVersion
        };
        criteria.push(CriterionCoverage {
            index: number,
            text: mask_text(text, &mut record),
            class,
            state,
            state_slug: state.slug(),
            absent_tool: if state == CoverageState::AbsentTool {
                absent.get(&class).cloned()
            } else {
                None
            },
            items,
            removed,
        });
    }

    let candidate_views = pool
        .iter()
        .map(|c| CandidateView {
            kind: c.kind,
            evidence_id: c.id.clone(),
            session_id: c.session_id.clone(),
            name: mask_text(&c.name, &mut record),
            observed: c.observed,
            bears: c.bears(),
            execution: c.execution,
            execution_slug: c.execution.slug(),
        })
        .collect();

    Ok(CoverageView {
        schema_version: COVERAGE_VIEW_SCHEMA_VERSION,
        change_id: change_id.to_string(),
        scope_version: Some(scope_version),
        criteria,
        candidates: candidate_views,
        executions: crate::run_execution::all(ledger, Some(change_id))?,
        link_rows,
        nothing_to_link_slug: why_nothing,
        nothing_bound_slug: match intended {
            Some(_) => None,
            None => Some("inputs_nothing_bound"),
        },
        execution_sessions: crate::change_link::execution_sessions(ledger, change_id)?,
        masking: record.to_treatment(),
    })
}

/// The same answer for the early return above, asked of the store directly.
fn nothing_bound(ledger: &Ledger, change_id: &str) -> Result<Option<&'static str>> {
    let intended = crate::examined_inputs::standing(
        ledger,
        crate::examined_inputs::SubjectKind::Intended,
        change_id,
    )?;
    Ok(match intended {
        Some(_) => None,
        None => Some("inputs_nothing_bound"),
    })
}

fn item_view(
    row: &StoredRow,
    candidate: Option<&Candidate>,
    applicability: Option<crate::examined_inputs::Applies>,
    freshness: Option<&crate::evidence_applicability::ApplicabilityRecord>,
    record: &mut MaskRecord,
) -> EvidenceItemView {
    // Evidence the gate no longer lets this Change look at: the link is still
    // a fact, and what it points at is no longer readable here. Said, never
    // dressed up as a result.
    let (name, observed, bears, note) = match candidate {
        Some(c) => (
            mask_text(&c.name, record),
            c.observed,
            c.bears(),
            if c.checked_nothing {
                Some("item_checked_nothing")
            } else {
                None
            },
        ),
        None => (
            String::new(),
            Observed::Couldnt,
            false,
            Some("item_out_of_reach"),
        ),
    };
    let execution = candidate
        .map(|c| c.execution)
        .unwrap_or(crate::run_execution::ExecutionState::NotRun);
    EvidenceItemView {
        link_id: row.id.clone(),
        kind: row.kind,
        evidence_id: row.evidence_id.clone(),
        session_id: row.session_id.clone(),
        basis: row.basis,
        basis_slug: row.basis.label_slug(),
        detail: row.detail.clone(),
        actor: row.actor.as_deref().map(|a| mask_text(a, record)),
        asserted_at: row.asserted_at.clone(),
        reason: row.reason.as_deref().map(|r| mask_text(r, record)),
        name,
        observed,
        bears,
        note_slug: note,
        applies: applicability.as_ref().map(|a| a.applies()),
        applies_slug: applicability.as_ref().map(|a| a.slug()),
        fresh: freshness
            .map(|f| f.state == crate::evidence_applicability::ApplicabilityState::Applies),
        fresh_slug: freshness.map(|f| f.state_slug),
        // Filled by the caller, which holds the ledger.
        assessable: None,
        execution,
        execution_slug: execution.slug(),
        // Masked at the boundary, like the name beside it: this is text the
        // run printed, so it goes through the masker before it is served.
        failure_line: candidate
            .and_then(|c| c.failure_line.as_deref())
            .map(|line| mask_text(line, record)),
        evidence_class: candidate.and_then(|c| c.evidence_class),
        evidence_class_slug: candidate.and_then(|c| c.evidence_class).map(|c| c.slug()),
    }
}

/// Whether one linked piece of evidence is about the version this Change is
/// bound to.
///
/// **The one caller of the predicate in this module.** `None` means the
/// question was not asked: either the evidence is not an examination (a
/// completion claim was never run), or this Change has not written down what
/// it is about, and a mismatch against nothing is not a fact.
pub(crate) fn applicability_of(
    ledger: &Ledger,
    intended: Option<&crate::examined_inputs::ExaminedInputs>,
    kind: EvidenceKind,
    evidence_id: &str,
) -> Result<Option<crate::examined_inputs::Applies>> {
    use crate::examined_inputs::{observation_applies, Applies, SubjectKind};
    let Some(intended) = intended else {
        return Ok(None);
    };
    let subject = match kind {
        EvidenceKind::CheckRun => SubjectKind::CheckRun,
        EvidenceKind::Observation => SubjectKind::Observation,
        // A completion claim with no proof behind it is not an examination:
        // nothing ran, so there is no version it was run against.
        EvidenceKind::Finding => return Ok(None),
    };
    let Some(examined) = crate::examined_inputs::standing(ledger, subject, evidence_id)? else {
        return Ok(Some(Applies::No {
            reason_slug: "inputs_missing",
        }));
    };
    Ok(Some(observation_applies(&examined, intended)))
}

/// Which classes the accepted profile asks for a check this machine cannot
/// run, and the program that is missing.
fn absent_tools(
    check_profile: &[String],
    environment: &ProfileEnvironment,
) -> BTreeMap<RunClass, String> {
    let mut out: BTreeMap<RunClass, String> = BTreeMap::new();
    for entry in accepted_check_availability(check_profile, environment) {
        let Availability::ToolAbsent { tool } = &entry.availability else {
            continue;
        };
        let class = class_of_category(entry.category);
        if class == RunClass::Unclassified {
            continue;
        }
        out.entry(class).or_insert_with(|| tool.clone());
    }
    out
}

/// Every stored link row for one Change, across every scope version — the
/// technical layer's own count, so "how many links does this Change have" is
/// one query and not a sum of screens.
pub fn link_row_count(ledger: &Ledger, change_id: &str) -> Result<u64> {
    let n: i64 = ledger.conn().query_row(
        "SELECT COUNT(*) FROM change_criterion_evidence WHERE change_id=?1",
        params![change_id],
        |row| row.get(0),
    )?;
    Ok(n.max(0) as u64)
}

/// The set of bases the store will accept. Public so a test can assert that
/// `timing` is not one of them without reading the DDL by hand.
pub fn known_bases() -> BTreeSet<&'static str> {
    ["declared", "named", "manual", "not-bearing"]
        .into_iter()
        .collect()
}
