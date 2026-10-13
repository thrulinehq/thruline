//! The Change record and its components (brief `B3.3`–`B3.7`, `B3.12`,
//! `B6.1`, `B11.4`).
//!
//! A **Change** is one bounded piece of work someone is being asked to accept.
//! It lives in the core ledger beside the session, as a child of the project,
//! because a Change may span sessions and a session may contribute to several
//! Changes (`B3.10`, `B3.11`). The DDL for every table this module reads and
//! writes is in [`crate::ledger`], under the `T-2-1` comment.
//!
//! Three properties are the point of the module, and each has a test:
//!
//! 1. **It copies nothing.** A Change references a session, an event and a
//!    snapshot by the existing id. No prose, timestamp or digest is duplicated
//!    out of `events`, so there is no second evidence store to disagree with
//!    the first (`B2.5`).
//! 2. **Nothing is inherited.** No verdict, claim or check row carries a change
//!    id, in the schema or in this module. A session's successful run cannot
//!    arrive on a Change through an association, because an association is two
//!    ids, a word and a time and carries no outcome field at all (`B3.13`).
//! 3. **Reads are masked.** Every reader here returns text that has been
//!    through [`crate::redact::Masked`], and the record of what the masker did
//!    travels with it. Nothing in this module hands out raw stored text.
//!
//! No `unwrap`, no `expect`, no network, no `unsafe` — the crate's rules,
//! restated because this module is new.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::chain::sha256_hex;
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::redact::{MaskRecord, Masked};

/// The served shape's version. Bumped when a field is added or renamed, so a
/// reader can tell a new field from a missing one.
///
/// Version 2 (T-2-2): every association gains `reason`, the words the person
/// gave when they confirmed or rejected it, and `rejected` joins the
/// provenance vocabulary.
///
/// Version 3 (T-S-4): `decisions` arrives — the approvals and withdrawals this
/// Change carries, and the version the record stands at now. The existing
/// `decision` placeholder is untouched beside it, because the six-stage strip
/// reads that one and a stage is not a decision.
///
/// Version 4 (T-4-1): `reviews` arrives — every review of this Change, oldest
/// round first, each naming the version it looked at and whether that is still
/// the version the record stands at. A review is never a decision.
///
/// Version 5 (STEP-7-GIT-LINE, R-2026-09-28-42): `code_moved` arrives — on
/// the one-Change route only, the commit recorded with the version against the
/// recorded folder's HEAD when the page was read
/// ([`crate::change_code_moved`]). `null` means nothing is said: equal heads,
/// nothing to read, a git that did not answer, or a reader (the index, the
/// export records) that does not read git at all. It never means "unchanged".
///
/// Version 6 (QUAL-F2-R2, Q7): `retrospective` arrives — whether the standing
/// list of what counts as done was accepted after the work it covers was
/// already in the record ([`RetrospectiveAcceptance`]). Served on every Change
/// and on every route, because it is read from the ledger alone: `null` means
/// nothing is said, never "accepted first".
///
/// Version 7 (C30 R-LEDGER): `notes` arrives — the words people kept on this
/// Change, oldest first, each with who and when and, when it was kept with an
/// approval or a withdrawal, which one ([`crate::change_note`]). Words, never a
/// result. A decision row in `decisions` also gains a derived `note`, absent
/// when there is none, so every approval recorded without one serves the bytes
/// it always did.
///
/// Version 8 (C30 R-LEDGER): `seconds` arrives — every second person's approval
/// of an approved version, and every taking-back, oldest first, masked, each
/// with a derived `counting` ([`crate::change_decision_second`]). The standing
/// approval in `decisions` also gains a derived `seconds_standing`, absent when
/// no second approval counts on it, so an approval nobody seconded serves the
/// bytes it always did. A second is a name, never a result.
pub const CHANGE_VIEW_SCHEMA_VERSION: u32 = 8;

fn require_nonempty(field: &'static str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(CoreError::Config(format!(
            "a change {field} must not be empty"
        )));
    }
    Ok(())
}

/// Where a request came from. `imported` is work that predates the Change
/// layer: `B3.16` forbids inventing acceptance criteria for it, so it is
/// marked rather than dressed up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestSource {
    Person,
    Agent,
    Imported,
}

impl RequestSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Person => "person",
            Self::Agent => "agent",
            Self::Imported => "imported",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "person" => Ok(Self::Person),
            "agent" => Ok(Self::Agent),
            "imported" => Ok(Self::Imported),
            other => Err(CoreError::Config(format!(
                "unknown change request source {other:?}"
            ))),
        }
    }
}

/// What was asked for (`B3.4`). `prompt_event_id` is the original prompt or
/// specification **by id**: the prose stays in `events` under the append-only
/// triggers, and the receipt path from a Change to raw JSONL is the one a
/// session already has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeRequest {
    pub title: String,
    pub purpose: String,
    pub prompt_event_id: Option<String>,
    pub requester: String,
    pub source: RequestSource,
}

/// A new Change, as a caller supplies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChange {
    pub id: String,
    pub project_id: String,
    pub opened_at: String,
    pub request: ChangeRequest,
}

/// One version of what counts as done (`B3.5`). Versioned because `B6.13`
/// attaches an approval to the version it was given, which needs the old
/// version to still be there.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AcceptedScope {
    pub criteria: Vec<String>,
    pub exclusions: Vec<String>,
    pub allowed_scope: Vec<String>,
    pub constraints: Vec<String>,
    pub check_profile: Vec<String>,
}

impl AcceptedScope {
    /// The digest over the five lists, canonically encoded. "Is this the scope
    /// that was accepted" is then an equality test rather than a diff.
    pub fn content_sha256(&self) -> String {
        #[derive(Serialize)]
        struct Canonical<'a> {
            allowed_scope: &'a [String],
            check_profile: &'a [String],
            constraints: &'a [String],
            criteria: &'a [String],
            exclusions: &'a [String],
        }
        let canonical = Canonical {
            allowed_scope: &self.allowed_scope,
            check_profile: &self.check_profile,
            constraints: &self.constraints,
            criteria: &self.criteria,
            exclusions: &self.exclusions,
        };
        match serde_json::to_vec(&canonical) {
            Ok(bytes) => sha256_hex(&bytes),
            // Five `Vec<String>` cannot fail to serialize; the arm exists so
            // this crate keeps its no-unwrap rule rather than because it runs.
            Err(_) => sha256_hex(b"change-scope-serialization-unavailable"),
        }
    }
}

/// Who agreed to a scope version, and when (`B3.5`). **This is not an
/// approval.** Accepting a scope says "this is what counts as done"; approving
/// says "the evidence supports this version". They are different tables so a
/// surface cannot read one as the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeAcceptance {
    pub actor: String,
    pub accepted_at: String,
    pub rationale: String,
}

/// The implementation binding (`B3.7`, `B6.1`). One snapshot per binding
/// version, deliberately: a Change that points at two snapshots at once is the
/// screen that says "approved" for the wrong version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplementationBinding {
    pub bound_at: String,
    pub repository_identity: String,
    pub base_revision: String,
    pub snapshot_id: Option<String>,
    pub workspace_identity: Option<String>,
}

/// Confirmed, rejected, or merely suggested (`B3.12`).
///
/// `Rejected` is T-2-2's addition and is held to the confirmation's bar, not
/// the suggestion's: only a named person on a `user` basis may say no. A
/// rejection expressed as the mere absence of a confirmation would be a
/// decision nobody could see, attribute or re-open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssociationProvenance {
    Suggested,
    Confirmed,
    Rejected,
}

impl AssociationProvenance {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Suggested => "suggested",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "suggested" => Ok(Self::Suggested),
            "confirmed" => Ok(Self::Confirmed),
            "rejected" => Ok(Self::Rejected),
            other => Err(CoreError::Config(format!(
                "unknown association provenance {other:?}"
            ))),
        }
    }

    /// The copy-file slug for this provenance. Every served word for an
    /// association comes from here, so no surface can invent one.
    pub fn label_slug(self) -> &'static str {
        match self {
            Self::Suggested => "suggested",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected_link",
        }
    }
}

/// Why the link exists. Only `User` may confirm; the database enforces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssociationBasis {
    User,
    Timing,
    Filename,
    AgentDeclared,
}

impl AssociationBasis {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Timing => "timing",
            Self::Filename => "filename",
            Self::AgentDeclared => "agent-declared",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "user" => Ok(Self::User),
            "timing" => Ok(Self::Timing),
            "filename" => Ok(Self::Filename),
            "agent-declared" => Ok(Self::AgentDeclared),
            other => Err(CoreError::Config(format!(
                "unknown association basis {other:?}"
            ))),
        }
    }
}

/// One assertion that a session belongs to a Change. There is no outcome field
/// here, and that absence is the `B3.13` guarantee: an association cannot carry
/// a result onto a Change because it has nowhere to put one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAssociation {
    pub id: String,
    pub change_id: String,
    pub session_id: String,
    pub provenance: AssociationProvenance,
    pub basis: AssociationBasis,
    pub actor: Option<String>,
    pub asserted_at: String,
    /// The words the person gave. `None` on a machine suggestion, whose reason
    /// is its basis and is not prose.
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// Writes. Every one appends; none updates.
// ---------------------------------------------------------------------------

/// Record a Change. Append-only: a second insert of the same id is refused by
/// the trigger, not by a caller remembering to check.
pub fn insert_change(ledger: &Ledger, change: &NewChange) -> Result<()> {
    require_nonempty("id", &change.id)?;
    require_nonempty("title", &change.request.title)?;
    require_nonempty("requester", &change.request.requester)?;
    ledger.conn().execute(
        "INSERT INTO changes(id, project_id, opened_at, title, purpose,
                             request_event_id, requester, request_source)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            change.id,
            change.project_id,
            change.opened_at,
            change.request.title,
            change.request.purpose,
            change.request.prompt_event_id,
            change.request.requester,
            change.request.source.as_str(),
        ],
    )?;
    Ok(())
}

/// Append the next scope version. The version is chosen here rather than by the
/// caller so two writers cannot both believe they wrote version 3.
pub fn append_scope(
    ledger: &Ledger,
    change_id: &str,
    proposed_at: &str,
    scope: &AcceptedScope,
) -> Result<u32> {
    let next: u32 = ledger
        .conn()
        .query_row(
            "SELECT COALESCE(MAX(scope_version), 0) + 1 FROM change_scope_revisions
              WHERE change_id=?1",
            params![change_id],
            |row| row.get(0),
        )
        .unwrap_or(1);
    let json = |list: &[String]| -> Result<String> { Ok(serde_json::to_string(list)?) };
    ledger.conn().execute(
        "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at,
             criteria_json, exclusions_json, allowed_scope_json, constraints_json,
             check_profile_json, content_sha256)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        params![
            change_id,
            next,
            proposed_at,
            json(&scope.criteria)?,
            json(&scope.exclusions)?,
            json(&scope.allowed_scope)?,
            json(&scope.constraints)?,
            json(&scope.check_profile)?,
            scope.content_sha256(),
        ],
    )?;
    Ok(next)
}

/// Record that a named actor accepted one scope version. One-shot per version.
pub fn accept_scope(
    ledger: &Ledger,
    change_id: &str,
    scope_version: u32,
    acceptance: &ScopeAcceptance,
) -> Result<()> {
    require_nonempty("scope acceptance actor", &acceptance.actor)?;
    ledger.conn().execute(
        "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by,
             accepted_at, rationale)
         VALUES(?1,?2,?3,?4,?5)",
        params![
            change_id,
            scope_version,
            acceptance.actor,
            acceptance.accepted_at,
            acceptance.rationale
        ],
    )?;
    Ok(())
}

/// Append the next implementation binding.
pub fn append_implementation(
    ledger: &Ledger,
    change_id: &str,
    binding: &ImplementationBinding,
) -> Result<u32> {
    require_nonempty("repository identity", &binding.repository_identity)?;
    require_nonempty("base revision", &binding.base_revision)?;
    let next: u32 = ledger
        .conn()
        .query_row(
            "SELECT COALESCE(MAX(binding_version), 0) + 1 FROM change_implementations
              WHERE change_id=?1",
            params![change_id],
            |row| row.get(0),
        )
        .unwrap_or(1);
    ledger.conn().execute(
        "INSERT INTO change_implementations(change_id, binding_version, bound_at,
             repository_identity, base_revision, snapshot_id, workspace_identity)
         VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![
            change_id,
            next,
            binding.bound_at,
            binding.repository_identity,
            binding.base_revision,
            binding.snapshot_id,
            binding.workspace_identity,
        ],
    )?;
    Ok(next)
}

/// Append one session ↔ Change assertion.
///
/// A `Confirmed` row with no actor, or with any basis other than `user`, is
/// refused by the table's CHECK. This function does not re-implement that rule
/// — it lets the database answer, so a caller that bypasses this function is
/// refused identically.
pub fn append_association(ledger: &Ledger, association: &NewAssociation) -> Result<()> {
    require_nonempty("association id", &association.id)?;
    ledger.conn().execute(
        "INSERT INTO change_session_associations(id, change_id, session_id,
             provenance, basis, actor, asserted_at, reason)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            association.id,
            association.change_id,
            association.session_id,
            association.provenance.as_str(),
            association.basis.as_str(),
            association.actor,
            association.asserted_at,
            association.reason,
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Reads. Masked at the boundary, so nothing downstream can forget.
// ---------------------------------------------------------------------------

/// A scope version as it is served.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScopeView {
    pub version: u32,
    pub proposed_at: String,
    pub criteria: Vec<String>,
    pub exclusions: Vec<String>,
    pub allowed_scope: Vec<String>,
    pub constraints: Vec<String>,
    pub check_profile: Vec<String>,
    pub content_sha256: String,
    /// `None` means nobody has agreed this list yet — shown as missing, never
    /// as agreed.
    pub acceptance: Option<AcceptanceView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AcceptanceView {
    pub actor: String,
    pub accepted_at: String,
    pub rationale: String,
}

/// QUAL-F2-R2 (`Q7`, brief §16 row 7) — the standing list of what counts as
/// done was accepted **after** the work it covers was already in the record.
///
/// A caution, not evidence: it says something about the ORDER of two things
/// the record holds, and nothing about whether the work meets the list. It is
/// derived at read from append-only facts and stored nowhere (no table, no
/// column, no write address), like [`derived_implementation`]:
///
/// * the acceptance row's own time, and
/// * the Change's own `intended` examined-inputs records — written by
///   `change new`, first use, `change prepare` and `check --for-change` — in
///   the order they were written.
///
/// The first of those is the tree the Change was opened on. The work is a
/// **different revision** recorded strictly before the acceptance, and it is
/// the tree standing at the acceptance (the last record before it) that must
/// differ from the opened one: a commit landed, and had not been taken back.
///
/// What is deliberately NOT the work, so the sentence it drives is true every
/// time it is said:
///
/// * a different set of uncommitted files at the same revision. The tree
///   fingerprint counts untracked files, so a baseline `cargo test` that leaves
///   a `Cargo.lock` behind would otherwise read as somebody's work;
/// * git's own commit times: a committer's claim, and a pinned history carries
///   its original dates, which would put an acceptance made first *after* them;
/// * the edits inside a confirmed session.
///
/// A Change opened on a tree that already held the work shows no move, so this
/// stays `None` there — silence, never "accepted before". It is also `None`
/// when the revision moves again at, or (by a time that cannot be read) maybe
/// after, the acceptance: the work was then not done when the list was
/// accepted, and "after the work was done" would not be true.
///
/// Times are compared as instants ([`instant_nanos`]), never as text. Ties are
/// not "before": stamps are whole seconds, and a record in the same second as
/// the acceptance does not say which came first. A time that cannot be read is
/// ordered against nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RetrospectiveAcceptance {
    /// The scope version whose acceptance this is about: the standing one.
    /// Earlier versions keep their own acceptances, with their own times, in
    /// `scope_history`.
    pub scope_version: u32,
    /// The acceptance row's own time, unchanged.
    pub accepted_at: String,
    /// The UTC date of that instant, for the sentence that carries the date.
    /// It is not the front of `accepted_at`'s spelling, which is another zone's
    /// date for a stamp that has an offset.
    pub accepted_on: String,
    /// The first record that holds a revision other than the opened one, by
    /// id, and when it was written.
    pub work_record_id: String,
    pub work_recorded_at: String,
    /// The revision the Change was opened on, and the one the work was first
    /// recorded at.
    pub opened_revision: String,
    pub work_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImplementationView {
    pub version: u32,
    pub bound_at: String,
    pub repository_identity: String,
    pub base_revision: String,
    pub snapshot_id: Option<String>,
    pub workspace_identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssociationView {
    pub id: String,
    pub session_id: String,
    pub provenance: AssociationProvenance,
    pub basis: AssociationBasis,
    pub actor: Option<String>,
    pub asserted_at: String,
    /// The person's own words for confirming or rejecting. Masked like every
    /// other free text here.
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RequestView {
    pub title: String,
    pub purpose: String,
    pub prompt_event_id: Option<String>,
    pub requester: String,
    pub source: RequestSource,
}

/// What the store holds for one Change, masked, with the two later stages
/// present as explicit placeholders rather than absent keys.
///
/// `examination` and `decision` are `"none_yet"` at this task. They are served
/// rather than omitted so a reader never has to decide whether a missing key
/// means "nothing happened" or "this build is older than that field" — missing
/// evidence renders as missing, never as pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangeView {
    pub schema_version: u32,
    pub id: String,
    pub project_id: String,
    pub opened_at: String,
    pub request: RequestView,
    /// The standing scope: the highest version. `None` means nothing has been
    /// written down about what counts as done.
    pub accepted_scope: Option<ScopeView>,
    /// Every earlier version, oldest first. Superseded, never deleted.
    pub scope_history: Vec<ScopeView>,
    pub implementation: Option<ImplementationView>,
    pub implementation_history: Vec<ImplementationView>,
    pub associations: Vec<AssociationView>,
    pub examination: &'static str,
    /// The placeholder the six-stage strip has always carried. It stays a
    /// placeholder on purpose: the strip is a view, not a decision, and
    /// `decisions` below is where the record speaks.
    pub decision: &'static str,
    /// T-S-4 — what the record holds about approving this Change, and where
    /// the record stands now. Never a screen state: every field is a row.
    pub decisions: crate::change_decision::DecisionsView,
    /// C30 R-LEDGER — the words people kept on this Change, oldest first,
    /// masked. Never a result: no count, verdict or approval reads them.
    pub notes: Vec<crate::change_note::NoteView>,
    /// C30 R-LEDGER — second approvals: a second person's name on the version
    /// an approval stands for, and every taking-back, oldest first, masked.
    /// Never a result: no count, verdict, readiness or choice of which approval
    /// stands reads them.
    pub seconds: Vec<crate::change_decision_second::SecondView>,
    /// T-4-1 — every review of this Change, kept apart from checks and from
    /// decisions. Masked like every other free text here.
    pub reviews: Vec<crate::change_review::ReviewView>,
    /// STEP-7-GIT-LINE — what moved since the version was recorded, read from
    /// git when this view was read, with its receipt. Only
    /// [`read_change_live`] fills it; `None` is silence, never "unchanged".
    pub code_moved: Option<crate::change_code_moved::CodeMoved>,
    /// QUAL-F2-R2 — `Some` when the standing list of what counts as done was
    /// accepted after the work was already in the record. Always served, on
    /// every route; `null` is silence, never "accepted first". See
    /// [`RetrospectiveAcceptance`].
    pub retrospective: Option<RetrospectiveAcceptance>,
    /// What the masker did to the text above, so a consumer can say so.
    pub masking: crate::redact::Treatment,
}

/// The placeholder both later stages carry until their tasks land.
pub const NONE_YET: &str = "none_yet";

fn parse_list(raw: &str) -> Vec<String> {
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}

pub(crate) fn mask_list(list: Vec<String>, record: &mut MaskRecord) -> Vec<String> {
    list.into_iter()
        .map(|item| {
            let masked = Masked::mask(&item);
            record.merge(masked.record());
            masked.into_text()
        })
        .collect()
}

pub(crate) fn mask_text(raw: &str, record: &mut MaskRecord) -> String {
    let masked = Masked::mask(raw);
    record.merge(masked.record());
    masked.into_text()
}

fn read_scopes(
    ledger: &Ledger,
    change_id: &str,
    record: &mut MaskRecord,
) -> Result<Vec<ScopeView>> {
    let mut statement = ledger.conn().prepare(
        "SELECT r.scope_version, r.proposed_at, r.criteria_json, r.exclusions_json,
                r.allowed_scope_json, r.constraints_json, r.check_profile_json,
                r.content_sha256, a.accepted_by, a.accepted_at, a.rationale
           FROM change_scope_revisions r
           LEFT JOIN change_scope_acceptances a
                  ON a.change_id=r.change_id AND a.scope_version=r.scope_version
          WHERE r.change_id=?1
          ORDER BY r.scope_version",
    )?;
    let rows = statement.query_map(params![change_id], |row| {
        Ok((
            row.get::<_, u32>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, Option<String>>(10)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (
            version,
            proposed_at,
            criteria,
            exclusions,
            allowed,
            constraints,
            profile,
            digest,
            by,
            at,
            why,
        ) = row?;
        let acceptance = match (by, at) {
            (Some(actor), Some(accepted_at)) => Some(AcceptanceView {
                actor: mask_text(&actor, record),
                accepted_at,
                rationale: mask_text(why.as_deref().unwrap_or_default(), record),
            }),
            _ => None,
        };
        out.push(ScopeView {
            version,
            proposed_at,
            criteria: mask_list(parse_list(&criteria), record),
            exclusions: mask_list(parse_list(&exclusions), record),
            allowed_scope: mask_list(parse_list(&allowed), record),
            constraints: mask_list(parse_list(&constraints), record),
            check_profile: mask_list(parse_list(&profile), record),
            content_sha256: digest,
            acceptance,
        });
    }
    Ok(out)
}

fn read_implementations(
    ledger: &Ledger,
    change_id: &str,
    record: &mut MaskRecord,
) -> Result<Vec<ImplementationView>> {
    let mut statement = ledger.conn().prepare(
        "SELECT binding_version, bound_at, repository_identity, base_revision,
                snapshot_id, workspace_identity
           FROM change_implementations WHERE change_id=?1 ORDER BY binding_version",
    )?;
    let rows = statement.query_map(params![change_id], |row| {
        Ok(ImplementationView {
            version: row.get(0)?,
            bound_at: row.get(1)?,
            repository_identity: row.get(2)?,
            base_revision: row.get(3)?,
            snapshot_id: row.get(4)?,
            workspace_identity: row.get(5)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        let mut view = row?;
        view.repository_identity = mask_text(&view.repository_identity, record);
        out.push(view);
    }
    Ok(out)
}

fn read_associations(
    ledger: &Ledger,
    change_id: &str,
    record: &mut MaskRecord,
) -> Result<Vec<AssociationView>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, session_id, provenance, basis, actor, asserted_at, reason
           FROM change_session_associations WHERE change_id=?1
          ORDER BY asserted_at, id",
    )?;
    let rows = statement.query_map(params![change_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, session_id, provenance, basis, actor, asserted_at, reason) = row?;
        out.push(AssociationView {
            id,
            session_id,
            provenance: AssociationProvenance::parse(&provenance)?,
            basis: AssociationBasis::parse(&basis)?,
            actor: actor.map(|a| mask_text(&a, record)),
            asserted_at,
            reason: reason.map(|r| mask_text(&r, record)),
        });
    }
    Ok(out)
}

/// One Change, masked and whole. `Ok(None)` means no such Change.
pub fn read_change(ledger: &Ledger, id: &str) -> Result<Option<ChangeView>> {
    read_change_with(ledger, id, None)
}

/// [`read_change`], plus the read-time git line (STEP-7-GIT-LINE): the one
/// place a served Change carries `code_moved`. It reads git and writes
/// nothing — no row, no re-binding of the version.
pub fn read_change_live(ledger: &Ledger, id: &str) -> Result<Option<ChangeView>> {
    read_change_with(
        ledger,
        id,
        Some(&crate::change_code_moved::GitReader::system()),
    )
}

fn read_change_with(
    ledger: &Ledger,
    id: &str,
    git: Option<&crate::change_code_moved::GitReader>,
) -> Result<Option<ChangeView>> {
    let head = ledger
        .conn()
        .query_row(
            "SELECT id, project_id, opened_at, title, purpose, request_event_id,
                    requester, request_source
               FROM changes WHERE id=?1",
            params![id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((id, project_id, opened_at, title, purpose, prompt_event_id, requester, source)) =
        head
    else {
        return Ok(None);
    };

    let mut record = MaskRecord::default();
    let request = RequestView {
        title: mask_text(&title, &mut record),
        purpose: mask_text(&purpose, &mut record),
        prompt_event_id,
        requester: mask_text(&requester, &mut record),
        source: RequestSource::parse(&source)?,
    };
    let mut scopes = read_scopes(ledger, &id, &mut record)?;
    let accepted_scope = scopes.pop();
    let mut implementations = read_implementations(ledger, &id, &mut record)?;
    /* T-S-4-F1 — the version the screen names is the version the product
     * records. `change_implementations` is written by nothing this product
     * ships, so a Change reachable by the documented flow had no version and
     * the screen said so forever. The standing `intended` examined-inputs
     * record (T-3-2) IS that version, written by `change new`, first use,
     * `change prepare` and `check --for-change`. An explicit binding still
     * wins; this is the ordinary path, and it is DERIVED rather than inserted,
     * so `change_implementations` keeps meaning "someone bound this on
     * purpose" and no row is invented in an append-only table. */
    let implementation = match implementations.pop() {
        Some(bound) => Some(bound),
        None => derived_implementation(ledger, &id, &mut record)?,
    };
    let associations = read_associations(ledger, &id, &mut record)?;
    let decisions = crate::change_decision::read(ledger, &id)?;
    let notes = crate::change_note::read(ledger, &id, &mut record)?;
    let seconds = crate::change_decision_second::read(
        ledger,
        &id,
        decisions.standing.as_ref().map(|row| row.id.as_str()),
        &mut record,
    )?;
    let reviews = crate::change_review::read_change_reviews(ledger, &id, &mut record)?;
    let code_moved = match git {
        Some(git) => crate::change_code_moved::read(ledger, &id, git)?.map(|(moved, masked)| {
            record.merge(&masked);
            moved
        }),
        None => None,
    };
    let retrospective = retrospective_acceptance(ledger, &id, accepted_scope.as_ref())?;

    Ok(Some(ChangeView {
        schema_version: CHANGE_VIEW_SCHEMA_VERSION,
        id,
        project_id,
        opened_at,
        request,
        accepted_scope,
        scope_history: scopes,
        implementation,
        implementation_history: implementations,
        associations,
        examination: NONE_YET,
        decision: NONE_YET,
        decisions,
        notes,
        seconds,
        reviews,
        code_moved,
        retrospective,
        masking: record.to_treatment(),
    }))
}

/// One `intended` record, as far as "did a commit land" needs.
struct IntendedRecord {
    id: String,
    recorded_at: String,
    revision: Option<String>,
}

/// QUAL-F2-R2 — see [`RetrospectiveAcceptance`] for the rule and its limits.
fn retrospective_acceptance(
    ledger: &Ledger,
    change_id: &str,
    standing: Option<&ScopeView>,
) -> Result<Option<RetrospectiveAcceptance>> {
    let Some(scope) = standing else {
        return Ok(None);
    };
    let Some(acceptance) = &scope.acceptance else {
        return Ok(None);
    };
    let Some(accepted) = instant_nanos(&acceptance.accepted_at) else {
        return Ok(None);
    };
    let mut statement = ledger.conn().prepare(
        "SELECT id, recorded_at, base_revision
           FROM examination_inputs
          WHERE subject_kind='intended' AND subject_id=?1
          ORDER BY rowid",
    )?;
    let rows = statement.query_map(params![change_id], |row| {
        Ok(IntendedRecord {
            id: row.get(0)?,
            recorded_at: row.get(1)?,
            // A blank revision is one nobody read, not a commit: the same
            // reading `derived_implementation` gives it.
            revision: row
                .get::<_, Option<String>>(2)?
                .filter(|revision| !revision.trim().is_empty()),
        })
    })?;
    let mut records = rows
        .collect::<std::result::Result<Vec<_>, _>>()?
        .into_iter();

    // The tree the Change was opened on, recorded before it was accepted.
    // Without that baseline there is nothing to say the work moved from.
    let Some(opened) = records.next() else {
        return Ok(None);
    };
    let Some(opened_revision) = opened.revision.clone() else {
        return Ok(None);
    };
    if !instant_nanos(&opened.recorded_at).is_some_and(|at| at < accepted) {
        return Ok(None);
    }

    let mut standing_revision = opened_revision.clone();
    let mut work: Option<(IntendedRecord, String)> = None;
    // Revisions of the records that are not shown to be before the
    // acceptance: at or after it, or with a time that cannot be read.
    let mut afterwards: Vec<String> = Vec::new();
    for record in records {
        let before = instant_nanos(&record.recorded_at).is_some_and(|at| at < accepted);
        // An unread revision moves nothing, before or after.
        let Some(revision) = record.revision.clone() else {
            continue;
        };
        if !before {
            afterwards.push(revision);
            continue;
        }
        standing_revision = revision.clone();
        if work.is_none() && revision != opened_revision {
            work = Some((record, revision));
        }
    }

    let Some((work, work_revision)) = work else {
        return Ok(None);
    };
    // Taken back to the opened commit before the acceptance: at that moment
    // the record shows the tree the Change was opened on, not the work.
    if standing_revision == opened_revision {
        return Ok(None);
    }
    // Moved again at, or maybe after, the acceptance: the work was not done.
    if afterwards
        .iter()
        .any(|revision| *revision != standing_revision)
    {
        return Ok(None);
    }
    Ok(Some(RetrospectiveAcceptance {
        scope_version: scope.version,
        accepted_at: acceptance.accepted_at.clone(),
        accepted_on: utc_date(accepted),
        work_record_id: work.id,
        work_recorded_at: work.recorded_at,
        opened_revision,
        work_revision,
    }))
}

/// An RFC 3339 stamp — `YYYY-MM-DDTHH:MM:SS`, an optional fraction of up to
/// nine digits, then `Z` or a numeric offset — as an instant: nanoseconds since
/// the Unix epoch, UTC. `None` for any other shape, or for a date or time that
/// does not exist (`2026-02-30`, `25:00:00`, a leap second), which is then
/// ordered against nothing.
///
/// Needed because stamps that spell in different ways are the same instant or
/// different ones in a way their text does not say: `…:00.500Z` sorts BELOW
/// `…:00Z` and is later, and `09:05:00-01:00` is 10:05 UTC.
fn instant_nanos(stamp: &str) -> Option<i128> {
    let bytes = stamp.as_bytes();
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        let digits = bytes.get(range)?;
        if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
            return None;
        }
        Some(
            digits
                .iter()
                .fold(0_i64, |n, d| n * 10 + i64::from(d - b'0')),
        )
    };
    let punctuation = [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')];
    if bytes.len() < 20 || punctuation.iter().any(|&(at, c)| bytes.get(at) != Some(&c)) {
        return None;
    }
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let days = days_from_civil(year, month, day);
    // A day that does not exist in its month comes back as another date.
    if crate::ingest::iso8601_utc(days * 86_400).get(..10) != stamp.get(..10) {
        return None;
    }

    let mut rest = stamp.get(19..)?;
    let mut fraction: i128 = 0;
    if let Some(after_dot) = rest.strip_prefix('.') {
        let end = after_dot
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after_dot.len());
        let (digits, tail) = after_dot.split_at(end);
        if digits.is_empty() || digits.len() > 9 {
            return None;
        }
        fraction =
            digits.parse::<i128>().ok()? * 10_i128.pow(9 - u32::try_from(digits.len()).ok()?);
        rest = tail;
    }
    let offset_seconds: i64 = if rest == "Z" {
        0
    } else {
        let sign = match rest.as_bytes().first()? {
            b'+' => 1,
            b'-' => -1,
            _ => return None,
        };
        let zone = rest.get(1..)?.as_bytes();
        let two = |at: usize| -> Option<i64> {
            match (zone.get(at)?, zone.get(at + 1)?) {
                (a, b) if a.is_ascii_digit() && b.is_ascii_digit() => {
                    Some(i64::from(a - b'0') * 10 + i64::from(b - b'0'))
                }
                _ => None,
            }
        };
        if zone.len() != 5 || zone.get(2) != Some(&b':') {
            return None;
        }
        let (zone_hours, zone_minutes) = (two(0)?, two(3)?);
        if zone_hours > 23 || zone_minutes > 59 {
            return None;
        }
        sign * (zone_hours * 3_600 + zone_minutes * 60)
    };

    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second - offset_seconds;
    Some(i128::from(seconds) * 1_000_000_000 + fraction)
}

/// The UTC calendar date (`YYYY-MM-DD`) of an instant from [`instant_nanos`].
fn utc_date(nanos: i128) -> String {
    let seconds = i64::try_from(nanos.div_euclid(1_000_000_000)).unwrap_or(0);
    // The date is the part of the stamp before its `T`: nothing is cut off.
    let stamp = crate::ingest::iso8601_utc(seconds);
    stamp.split('T').next().unwrap_or_default().to_string()
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`, the inverse of the `civil_from_days` in
/// [`crate::ingest::iso8601_utc`]).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The implementation view derived from the standing `intended` examined-inputs
/// record, when nothing was bound on purpose (T-S-4-F1).
///
/// `version` is 0 deliberately: a derived reading is not binding version 1, and
/// a reader that cares about the difference can see it. Everything else is the
/// record's own, masked like every other field this module hands out.
fn derived_implementation(
    ledger: &Ledger,
    change_id: &str,
    record: &mut MaskRecord,
) -> Result<Option<ImplementationView>> {
    let Some(inputs) = crate::examined_inputs::standing(
        ledger,
        crate::examined_inputs::SubjectKind::Intended,
        change_id,
    )?
    else {
        return Ok(None);
    };
    let base = inputs.base_revision.value().unwrap_or_default().to_string();
    let snapshot = inputs
        .snapshot_id
        .value()
        .map(|v| v.to_string())
        .filter(|v| !v.trim().is_empty());
    if base.trim().is_empty() && snapshot.is_none() {
        return Ok(None);
    }
    let bound_at: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT recorded_at FROM examination_inputs
              WHERE subject_kind='intended' AND subject_id=?1
              ORDER BY rowid DESC LIMIT 1",
            params![change_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(Some(ImplementationView {
        version: 0,
        bound_at: bound_at.unwrap_or_default(),
        repository_identity: mask_text(inputs.source_identity.value().unwrap_or_default(), record),
        base_revision: base,
        snapshot_id: snapshot,
        workspace_identity: inputs
            .worktree_path
            .value()
            .map(|v| mask_text(v, record))
            .filter(|v| !v.is_empty()),
    }))
}

/// Every Change, oldest first, optionally narrowed to one project.
pub fn read_changes(ledger: &Ledger, project_id: Option<&str>) -> Result<Vec<ChangeView>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id FROM changes WHERE (?1 IS NULL OR project_id=?1) ORDER BY opened_at, id",
    )?;
    let ids = statement.query_map(params![project_id], |row| row.get::<_, String>(0))?;
    let mut out = Vec::new();
    for id in ids {
        if let Some(view) = read_change(ledger, &id?)? {
            out.push(view);
        }
    }
    Ok(out)
}
