//! T-S-2 — freshness and invalidation, the sprint slice of T-3-3: "the retry
//! test passed on the previous version; the affected code changed afterward."
//!
//! This module answers one question for one piece of linked evidence: is it
//! still about the tree the Change is now bound to, and if not, why not, in
//! one of Sam's five words? It never re-runs anything and never touches the
//! link itself (`change_evidence::links_for` is untouched) — it only records,
//! append-only, whether a link that already exists still counts.
//!
//! # The one documented function
//!
//! [`invalidating_paths`] is the whole dependency rule: where a check profile
//! declares which paths a check depends on, only those paths invalidate;
//! where it does not, any changed path under the criterion's accepted scope
//! invalidates (conservative). Nothing else in this module, or in the CLI or
//! the served route, re-implements it.
//!
//! Declared paths are not yet a thing the check profile can name in this
//! schema (`check_profile_json` is a list of check ids, not `(check, paths)`
//! pairs), so [`declared_paths_for`] returns `None` for every criterion today
//! and every assessment takes the conservative branch. The function exists
//! now so a later check profile that DOES declare paths does not need a new
//! rule, only a populated lookup — a decision recorded in the coder report.
//!
//! # Never relabel
//!
//! [`record`] only ever inserts; the table's own triggers refuse UPDATE and
//! DELETE. A record superseding an older one is a NEW row naming the same
//! criterion link — the old row, its snapshot and its time are untouched.
//! the test `no_writer_in_this_module_ever_issues_update_or_delete_against_the_table`
//! in `crates/core/tests/evidence_applicability_rule.rs` walks this module's
//! own source for one.

use std::collections::BTreeMap;

use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use crate::chain::sha256_hex;
use crate::change::mask_text;
use crate::error::Result;
use crate::ledger::Ledger;
use crate::redact::MaskRecord;

/// The name of the one function that decides which paths invalidate a piece
/// of evidence. If a second function in this crate grows that power, the
/// never-relabel test's sibling in this module fails and names it.
pub const THE_DEPENDENCY_RULE: &str = "invalidating_paths";

/// Sam's five reason classes, in the order the card lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonClass {
    SourceChanged,
    TestsChanged,
    DependenciesChanged,
    ConfigurationChanged,
    CriteriaChanged,
}

impl ReasonClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SourceChanged => "source_changed",
            Self::TestsChanged => "tests_changed",
            Self::DependenciesChanged => "dependencies_changed",
            Self::ConfigurationChanged => "configuration_changed",
            Self::CriteriaChanged => "criteria_changed",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "source_changed" => Some(Self::SourceChanged),
            "tests_changed" => Some(Self::TestsChanged),
            "dependencies_changed" => Some(Self::DependenciesChanged),
            "configuration_changed" => Some(Self::ConfigurationChanged),
            "criteria_changed" => Some(Self::CriteriaChanged),
            _ => None,
        }
    }

    /// The copy-file slug for the reason sentence, one level down from the
    /// "needs a re-check" headline.
    pub fn slug(self) -> &'static str {
        match self {
            Self::SourceChanged => "applicability_reason_source_changed",
            Self::TestsChanged => "applicability_reason_tests_changed",
            Self::DependenciesChanged => "applicability_reason_dependencies_changed",
            Self::ConfigurationChanged => "applicability_reason_configuration_changed",
            Self::CriteriaChanged => "applicability_reason_criteria_changed",
        }
    }
}

/// Whether a path names a dependency lockfile. Checked before the test and
/// configuration classes because a lockfile is also, incidentally, config.
fn is_dependency_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(
        name,
        "Cargo.lock" | "package-lock.json" | "pnpm-lock.yaml" | "yarn.lock" | "Gemfile.lock"
    )
}

/// Whether a path is a test file, by the two conventions this workspace
/// actually uses: a `tests/` directory, or a `.test.js`/`_test.rs` suffix.
fn is_test_path(path: &str) -> bool {
    if path.split('/').any(|segment| segment == "tests") {
        return true;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    name.ends_with(".test.js") || name.ends_with(".test.jsx") || name.ends_with("_test.rs")
}

/// Whether a path is build/tooling configuration rather than source.
fn is_configuration_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(
        name,
        "Cargo.toml" | "package.json" | "tsconfig.json" | "vite.config.js" | "vite.config.ts"
    ) || name.starts_with(".eslintrc")
}

/// One changed path's reason class, by the conventions above. Order matters:
/// a lockfile under `tests/` is still a dependency change, not a test change.
pub fn classify_path(path: &str) -> ReasonClass {
    if is_dependency_path(path) {
        ReasonClass::DependenciesChanged
    } else if is_configuration_path(path) {
        ReasonClass::ConfigurationChanged
    } else if is_test_path(path) {
        ReasonClass::TestsChanged
    } else {
        ReasonClass::SourceChanged
    }
}

/// A `**`/`*` glob, matched the way `allowed_scope_json` entries are written
/// today: `**` stands for any number of path segments, `*` for any run of
/// characters within one segment, everything else is literal.
pub fn glob_matches(pattern: &str, path: &str) -> bool {
    fn to_regex(pattern: &str) -> String {
        let mut out = String::from("^");
        let mut chars = pattern.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '*' => {
                    if chars.peek() == Some(&'*') {
                        chars.next();
                        out.push_str(".*");
                    } else {
                        out.push_str("[^/]*");
                    }
                }
                '.' | '+' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|' | '\\' => {
                    out.push('\\');
                    out.push(c);
                }
                other => out.push(other),
            }
        }
        out.push('$');
        out
    }
    match regex_lite_match(&to_regex(pattern), path) {
        Some(m) => m,
        None => path == pattern,
    }
}

/// A minimal `^...$` matcher over the two constructs [`glob_matches`] emits
/// (`.*` and `[^/]*`) plus literals — this workspace does not otherwise carry
/// a regex dependency in `crates/core`, and pulling one in for two wildcard
/// forms would be more machinery than the milestone allows (Rule 2).
fn regex_lite_match(pattern: &str, text: &str) -> Option<bool> {
    let body = pattern.strip_prefix('^')?.strip_suffix('$')?;
    fn rec(pat: &[u8], txt: &[u8]) -> bool {
        if pat.is_empty() {
            return txt.is_empty();
        }
        if pat.starts_with(b".*") {
            for i in 0..=txt.len() {
                if rec(&pat[2..], &txt[i..]) {
                    return true;
                }
            }
            return false;
        }
        if pat.starts_with(b"[^/]*") {
            for i in 0..=txt.len() {
                if txt[..i].contains(&b'/') {
                    break;
                }
                if rec(&pat[5..], &txt[i..]) {
                    return true;
                }
            }
            return false;
        }
        if pat[0] == b'\\' {
            return !txt.is_empty() && pat.get(1) == txt.first() && rec(&pat[2..], &txt[1..]);
        }
        !txt.is_empty() && pat[0] == txt[0] && rec(&pat[1..], &txt[1..])
    }
    Some(rec(body.as_bytes(), text.as_bytes()))
}

/// Whether a path matches any pattern in a list.
fn matches_any(path: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|p| glob_matches(p, path))
}

/// Declared paths for one criterion's check profile, today. `None` always:
/// `check_profile_json` names checks, not `(check, paths)` pairs, in this
/// schema. See the module doc for why this is a real function and not a
/// placeholder.
pub fn declared_paths_for(_check_profile: &[String], _criterion_index: u32) -> Option<Vec<String>> {
    None
}

/// **The dependency rule.** Which of `changed_paths` invalidate a piece of
/// evidence linked to a criterion whose check profile declares `declared`
/// (when it declares any) and whose accepted scope allows `allowed_scope`.
///
/// Declared and non-empty: only the overlap between `changed_paths` and
/// `declared` counts. Otherwise: every changed path under `allowed_scope`
/// counts (conservative — Q5's "relevant source/tests/dependencies/
/// configuration change" is read as "anything in scope, because nothing
/// narrower was declared").
///
/// Same inputs, same output: no clock, no randomness, no I/O.
pub fn invalidating_paths(
    changed_paths: &[String],
    declared: Option<&[String]>,
    allowed_scope: &[String],
) -> Vec<String> {
    let scope: &[String] = match declared {
        Some(paths) if !paths.is_empty() => paths,
        _ => allowed_scope,
    };
    let mut out: Vec<String> = changed_paths
        .iter()
        .filter(|p| matches_any(p, scope))
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Whether a piece of evidence still applies, and why not when it does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicabilityState {
    Applies,
    NoLongerApplies,
}

impl ApplicabilityState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Applies => "applies",
            Self::NoLongerApplies => "no_longer_applies",
        }
    }
}

/// One row of `change_criterion_freshness`, masked and ready to serve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApplicabilityRecord {
    pub id: String,
    pub change_id: String,
    pub criterion_evidence_id: String,
    pub criterion_index: u32,
    pub scope_version: u32,
    pub old_snapshot: String,
    pub new_snapshot: String,
    pub state: ApplicabilityState,
    pub reason_class: Option<ReasonClass>,
    /// The copy-file slug for the headline sentence: "still applies" or
    /// "needs a re-check", looked up from `state` and never built here.
    pub state_slug: &'static str,
    /// The copy-file slug for the reason, one level down. `None` exactly
    /// when `reason_class` is `None`.
    pub reason_slug: Option<&'static str>,
    pub reason_paths: Vec<String>,
    /// **What the rule compared** — the check profile's declared paths for
    /// this criterion when it declares any, otherwise the accepted scope
    /// (the conservative branch of `invalidating_paths`). This is the subject
    /// of the "still applies" sentence: an `Applies` record has no
    /// `reason_paths` by design (nothing invalidating to name), and a
    /// sentence that names neither the path checked nor the version it is
    /// unchanged since carries no claim at all (T-F-1-F1).
    ///
    /// DERIVED AT READ, not stored: it is a function of the standing accepted
    /// scope, which is already in the ledger. Empty when the scope cannot be
    /// read, and a surface that finds it empty says so rather than rendering
    /// a hole.
    #[serde(default)]
    pub checked_paths: Vec<String>,
    pub old_input_hash: Option<String>,
    pub new_input_hash: Option<String>,
    pub recorded_at: String,
}

impl ApplicabilityRecord {
    pub fn masked(&self, record: &mut MaskRecord) -> ApplicabilityRecord {
        ApplicabilityRecord {
            id: self.id.clone(),
            change_id: self.change_id.clone(),
            criterion_evidence_id: self.criterion_evidence_id.clone(),
            criterion_index: self.criterion_index,
            scope_version: self.scope_version,
            old_snapshot: self.old_snapshot.clone(),
            new_snapshot: self.new_snapshot.clone(),
            state: self.state,
            reason_class: self.reason_class,
            state_slug: self.state_slug,
            reason_slug: self.reason_slug,
            reason_paths: self
                .reason_paths
                .iter()
                .map(|p| mask_text(p, record))
                .collect(),
            checked_paths: self
                .checked_paths
                .iter()
                .map(|p| mask_text(p, record))
                .collect(),
            old_input_hash: self.old_input_hash.clone(),
            new_input_hash: self.new_input_hash.clone(),
            recorded_at: self.recorded_at.clone(),
        }
    }
}

fn state_slug(state: ApplicabilityState) -> &'static str {
    match state {
        ApplicabilityState::Applies => "applicability_still_applies",
        ApplicabilityState::NoLongerApplies => "applicability_needs_a_recheck",
    }
}

/// The id of one record: content-addressed, so writing the same conclusion
/// twice about the same criterion link inserts once (the replay-determinism
/// acceptance criterion) and a record that says something different from the
/// last one is a new row rather than an edit of it.
fn record_id(
    criterion_evidence_id: &str,
    old_snapshot: &str,
    new_snapshot: &str,
    state: ApplicabilityState,
    reason_class: Option<ReasonClass>,
    reason_paths: &[String],
) -> String {
    let payload = format!(
        "eap/v1\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        criterion_evidence_id,
        old_snapshot,
        new_snapshot,
        state.as_str(),
        reason_class.map(ReasonClass::as_str).unwrap_or(""),
        reason_paths.join(",")
    );
    format!("eap_{}", sha256_hex(payload.as_bytes()))
}

/// One assessment input: what changed between the old and the new whole-tree
/// snapshot, as a path → content digest map. A path present on only one side
/// counts as changed (added or removed).
pub fn changed_paths(
    old: &BTreeMap<String, String>,
    new: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut out: Vec<String> = old
        .iter()
        .filter(|(path, hash)| new.get(*path) != Some(*hash))
        .map(|(path, _)| path.clone())
        .chain(new.keys().filter(|path| !old.contains_key(*path)).cloned())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The whole-tree fingerprint the record carries as `old_input_hash`/
/// `new_input_hash` — the sha256 of the sorted `path:digest` pairs, so an
/// auditor never has to trust the classifier's word for what moved.
pub fn tree_fingerprint(tree: &BTreeMap<String, String>) -> String {
    let joined = tree
        .iter()
        .map(|(path, hash)| format!("{path}:{hash}"))
        .collect::<Vec<_>>()
        .join("\u{1f}");
    sha256_hex(joined.as_bytes())
}

/// **T-3-4 (`B6.8`) — the tree moved WHILE the run was going.**
///
/// A run that reached the end and whose inputs changed under it did not
/// examine what is on disk now. `change_evidence`'s freshness rule compares
/// snapshots after the fact and would only notice at the next comparison, so
/// the record would carry a fresh pass in the meantime — which is exactly what
/// `B6.8` forbids. This writes the `no_longer_applies` row **at completion**,
/// for every standing link that points at this run's binding, so the evidence
/// is stale from birth rather than from whenever somebody next looks.
///
/// The reason class is `source_changed`: the wrapper compares whole-tree
/// examined-inputs fingerprints and knows *that* the tree moved, not which
/// path did. Claiming a narrower class would be inventing a fact, and naming
/// no paths is the honest form of "the tree moved and the run did not see it".
///
/// Returns how many rows it wrote. Zero is an ordinary answer: a run nobody
/// has linked to a criterion yet has nothing to mark, and the same comparison
/// will be made again by the rule once a link exists.
pub fn mark_inputs_changed(
    ledger: &Ledger,
    change_id: &str,
    binding_id: &str,
    old_snapshot: &str,
    new_snapshot: &str,
    at: &str,
) -> Result<usize> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, criterion_index, scope_version FROM change_criterion_evidence
          WHERE change_id=?1 AND evidence_id=?2 AND evidence_kind='check_run'
            AND basis <> 'not-bearing'",
    )?;
    let rows = statement
        .query_map(params![change_id, binding_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, u32>(1)?,
                row.get::<_, u32>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    drop(statement);
    let mut written = 0;
    for (link_id, criterion_index, scope_version) in rows {
        record(
            ledger,
            change_id,
            &link_id,
            criterion_index,
            scope_version,
            old_snapshot,
            new_snapshot,
            ApplicabilityState::NoLongerApplies,
            Some(ReasonClass::SourceChanged),
            &[],
            Some(old_snapshot),
            Some(new_snapshot),
            at,
        )?;
        written += 1;
    }
    Ok(written)
}

/// Append one applicability record. Returns the id whether or not this exact
/// conclusion was already stored — the table's own append-only triggers are
/// what make a second write with different content a new row rather than an
/// edit, so this function itself contains no update path to test.
#[allow(clippy::too_many_arguments)]
pub fn record(
    ledger: &Ledger,
    change_id: &str,
    criterion_evidence_id: &str,
    criterion_index: u32,
    scope_version: u32,
    old_snapshot: &str,
    new_snapshot: &str,
    state: ApplicabilityState,
    reason_class: Option<ReasonClass>,
    reason_paths: &[String],
    old_input_hash: Option<&str>,
    new_input_hash: Option<&str>,
    at: &str,
) -> Result<String> {
    let id = record_id(
        criterion_evidence_id,
        old_snapshot,
        new_snapshot,
        state,
        reason_class,
        reason_paths,
    );
    let existing: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT id FROM change_criterion_freshness WHERE id=?1",
            params![id],
            |row| row.get(0),
        )
        .optional()?;
    if existing.is_some() {
        return Ok(id);
    }
    let reason_paths_json = serde_json::to_string(reason_paths).unwrap_or_else(|_| "[]".into());
    ledger.conn().execute(
        "INSERT INTO change_criterion_freshness(
            id, change_id, criterion_evidence_id, criterion_index, scope_version,
            old_snapshot, new_snapshot, state, reason_class, reason_paths_json,
            old_input_hash, new_input_hash, recorded_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            id,
            change_id,
            criterion_evidence_id,
            criterion_index,
            scope_version,
            old_snapshot,
            new_snapshot,
            state.as_str(),
            reason_class.map(ReasonClass::as_str),
            reason_paths_json,
            old_input_hash,
            new_input_hash,
            at,
        ],
    )?;
    Ok(id)
}

fn row_to_record(row: &rusqlite::Row) -> rusqlite::Result<ApplicabilityRecord> {
    let state_raw: String = row.get("state")?;
    let state = if state_raw == "applies" {
        ApplicabilityState::Applies
    } else {
        ApplicabilityState::NoLongerApplies
    };
    let reason_class: Option<String> = row.get("reason_class")?;
    let reason_class = reason_class.as_deref().and_then(ReasonClass::parse);
    let reason_paths_raw: String = row.get("reason_paths_json")?;
    let reason_paths: Vec<String> = serde_json::from_str(&reason_paths_raw).unwrap_or_default();
    Ok(ApplicabilityRecord {
        id: row.get("id")?,
        change_id: row.get("change_id")?,
        criterion_evidence_id: row.get("criterion_evidence_id")?,
        criterion_index: row.get("criterion_index")?,
        scope_version: row.get("scope_version")?,
        old_snapshot: row.get("old_snapshot")?,
        new_snapshot: row.get("new_snapshot")?,
        state,
        reason_class,
        state_slug: state_slug(state),
        reason_slug: reason_class.map(ReasonClass::slug),
        reason_paths,
        // Not a stored column: the caller that knows the standing scope fills
        // it (`fill_checked_paths`), and a raw row read leaves it empty.
        checked_paths: Vec::new(),
        old_input_hash: row.get("old_input_hash")?,
        new_input_hash: row.get("new_input_hash")?,
        recorded_at: row.get("recorded_at")?,
    })
}

/// The latest applicability record for every criterion-evidence link of one
/// Change, keyed by `criterion_evidence_id`. History (every earlier row) is
/// never deleted; this is the standing read, not the whole ledger.
///
/// "Latest" means **last appended**, not "largest id". `recorded_at` is
/// second-granular (`ingest::iso8601_utc_now`), so two assessments of the
/// same link inside one second tie on it; the tie used to be broken by `id
/// ASC`, which is a sha256 of the record's content and therefore an arbitrary
/// order. On a machine fast enough to run two checks in the same second that
/// made a stale item read as still-applying roughly half the time — found by
/// the T-3-3-F1 gate. `rowid` is this table's own insertion order, and the
/// table is append-only by trigger, so it is the true order of the record.
///
/// `rowid` is therefore the **only** order (NEVER-ASSESSED-NOT-READY-R3).
/// `recorded_at` used to be the primary key of the sort with `rowid` breaking
/// ties, so a backward clock step between two `change prepare` runs made an
/// OLDER "applies" row stand over the latest "no longer applies" one, and the
/// criterion read Ready. `recorded_at` is the caller's wall-clock read and is
/// kept on every record as data; it never chooses which record stands.
/// `(allowed_scope, check_profile)` of one scope version.
type ScopeAtVersion = (Vec<String>, Vec<String>);

/// The accepted scope and check profile of ONE scope version (T-F-1-F1).
///
/// Not [`standing_scope`]: a record recorded at scope version 1 was compared
/// against version 1's scope, and naming today's scope on that sentence would
/// be this module saying it compared something it did not.
fn scope_at_version(
    ledger: &Ledger,
    change_id: &str,
    scope_version: u32,
) -> Result<Option<ScopeAtVersion>> {
    let row = ledger
        .conn()
        .query_row(
            "SELECT allowed_scope_json, check_profile_json
               FROM change_scope_revisions
              WHERE change_id=?1 AND scope_version=?2",
            params![change_id, scope_version],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let Some((allowed_raw, profile_raw)) = row else {
        return Ok(None);
    };
    let parse = |raw: &str| serde_json::from_str::<Vec<String>>(raw).unwrap_or_default();
    Ok(Some((parse(&allowed_raw), parse(&profile_raw))))
}

/// Fill [`ApplicabilityRecord::checked_paths`] on records read back out of the
/// table, which does not store them: what the rule compared is a function of
/// the accepted scope at the record's own scope version, and is re-derived
/// here by the same expression [`assess_tree_move`] used to choose it.
///
/// A record whose scope revision cannot be read keeps an empty list — the
/// surface then says it cannot name the path rather than naming a wrong one.
fn fill_checked_paths(ledger: &Ledger, records: &mut [ApplicabilityRecord]) -> Result<()> {
    let mut cache: BTreeMap<(String, u32), Option<ScopeAtVersion>> = BTreeMap::new();
    for record in records.iter_mut() {
        let key = (record.change_id.clone(), record.scope_version);
        let scope = match cache.get(&key) {
            Some(hit) => hit.clone(),
            None => {
                let looked_up = scope_at_version(ledger, &record.change_id, record.scope_version)?;
                cache.insert(key, looked_up.clone());
                looked_up
            }
        };
        let Some((allowed_scope, check_profile)) = scope else {
            continue;
        };
        let declared = declared_paths_for(&check_profile, record.criterion_index);
        record.checked_paths = match declared {
            Some(paths) if !paths.is_empty() => paths,
            _ => allowed_scope,
        };
    }
    Ok(())
}

pub fn standing_for_change(
    ledger: &Ledger,
    change_id: &str,
) -> Result<BTreeMap<String, ApplicabilityRecord>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, change_id, criterion_evidence_id, criterion_index, scope_version,
                old_snapshot, new_snapshot, state, reason_class, reason_paths_json,
                old_input_hash, new_input_hash, recorded_at
           FROM change_criterion_freshness
          WHERE change_id=?1
          ORDER BY criterion_evidence_id, rowid ASC",
    )?;
    let mut out: BTreeMap<String, ApplicabilityRecord> = BTreeMap::new();
    let rows = statement.query_map(params![change_id], row_to_record)?;
    for row in rows {
        let record = row?;
        out.insert(record.criterion_evidence_id.clone(), record);
    }
    let mut values: Vec<ApplicabilityRecord> = out.into_values().collect();
    fill_checked_paths(ledger, &mut values)?;
    Ok(values
        .into_iter()
        .map(|r| (r.criterion_evidence_id.clone(), r))
        .collect())
}

/// Every historical row for one criterion-evidence link, in append order
/// (`rowid`, never `recorded_at` — NEVER-ASSESSED-NOT-READY-R3) — what
/// `standing_for_change` folds down to one, kept in full for the record.
pub fn history_for_link(
    ledger: &Ledger,
    criterion_evidence_id: &str,
) -> Result<Vec<ApplicabilityRecord>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, change_id, criterion_evidence_id, criterion_index, scope_version,
                old_snapshot, new_snapshot, state, reason_class, reason_paths_json,
                old_input_hash, new_input_hash, recorded_at
           FROM change_criterion_freshness
          WHERE criterion_evidence_id=?1
          ORDER BY rowid ASC",
    )?;
    let rows = statement.query_map(params![criterion_evidence_id], row_to_record)?;
    let mut out = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    fill_checked_paths(ledger, &mut out)?;
    Ok(out)
}

/// One still-linked criterion-evidence row, read directly (this module does
/// not depend on `change_evidence`'s private link-reading helpers).
struct LinkedRow {
    id: String,
    criterion_index: u32,
    evidence_kind: String,
    evidence_id: String,
}

fn linked_rows(ledger: &Ledger, change_id: &str, scope_version: u32) -> Result<Vec<LinkedRow>> {
    let mut statement = ledger.conn().prepare(
        "SELECT id, criterion_index, evidence_kind, evidence_id FROM change_criterion_evidence
          WHERE change_id=?1 AND scope_version=?2 AND basis <> 'not_bearing'
          ORDER BY id",
    )?;
    let rows = statement
        .query_map(params![change_id, scope_version], |row| {
            Ok(LinkedRow {
                id: row.get(0)?,
                criterion_index: row.get(1)?,
                evidence_kind: row.get(2)?,
                evidence_id: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// The tree **that piece of evidence was itself produced against** — its own
/// examined-inputs record (T-3-2 writes one per check run, keyed by the
/// binding id), resolved to the stored workspace snapshot and its manifest.
///
/// This is the whole of T-3-3-F1's fix. Assessing against the Change's
/// *previous standing Intended* made a second `change prepare` on an
/// unchanged tree compare the post-edit tree to itself, find nothing changed,
/// and write `applies` over a correct `no_longer_applies` — laundering stale
/// evidence back to current. Anchored to the evidence's own snapshot instead,
/// an item that stopped applying can only start applying again when the
/// current tree's invalidating paths match that snapshot's digests once more.
///
/// `None` when the evidence carries no examined-inputs record or no stored
/// snapshot (a finding, an observation, or a check run recorded before T-3-2)
/// — the caller then falls back to the tree-move pair it was given, which is
/// the pre-fix behaviour and is no worse than it was.
type OwnTree = (String, BTreeMap<String, String>, Option<String>);

fn evidence_own_tree(ledger: &Ledger, row: &LinkedRow) -> Result<Option<OwnTree>> {
    let subject_kind = match row.evidence_kind.as_str() {
        "check_run" => crate::examined_inputs::SubjectKind::CheckRun,
        "observation" => crate::examined_inputs::SubjectKind::Observation,
        _ => return Ok(None),
    };
    let Some(inputs) = crate::examined_inputs::standing(ledger, subject_kind, &row.evidence_id)?
    else {
        return Ok(None);
    };
    let Some(snapshot_id) = inputs.snapshot_id.value() else {
        return Ok(None);
    };
    let Some((snapshot, _)) = crate::snapshot::load(ledger, snapshot_id)? else {
        return Ok(None);
    };
    let head = snapshot.git.as_ref().and_then(|g| g.head.clone());
    Ok(Some((
        snapshot_id.to_string(),
        manifest_from_snapshot(&snapshot),
        head,
    )))
}

/// NEVER-ASSESSED-NOT-READY-R2: whether a binding of this Change's
/// `intended` record, written AFTER this evidence's own examined-inputs
/// record, has no stored snapshot (D-051, SNAPSHOT-CHILD-TIMEOUT, or a store
/// that failed). Across such a gap the Change's tree chain no longer says
/// what the files were when the evidence was produced, so it cannot stand in
/// for the evidence's own tree.
///
/// Records are ordered by the append-only table's own insertion order. An
/// evidence with no examined-inputs record of its own (a finding, or a run
/// recorded before T-3-2) has nothing to order against and reads `false` —
/// exactly what it read before this rule.
fn capture_gap_since(ledger: &Ledger, change_id: &str, row: &LinkedRow) -> Result<bool> {
    let gap: bool = ledger.conn().query_row(
        "SELECT EXISTS(
            SELECT 1 FROM examination_inputs i
             WHERE i.subject_kind='intended' AND i.subject_id=?1
               AND i.snapshot_id IS NULL
               AND i.rowid > (SELECT MAX(e.rowid) FROM examination_inputs e
                               WHERE e.subject_kind=?2 AND e.subject_id=?3))",
        params![change_id, row.evidence_kind, row.evidence_id],
        |r| r.get(0),
    )?;
    Ok(gap)
}

/// NEVER-ASSESSED-NOT-READY-R2 — **read side of [`assess_rows`]**: would the
/// next binding of this Change's `intended` record (`thruline change
/// prepare`) be able to assess this piece of evidence? True when the
/// evidence carries its own stored tree, or when the Change's standing
/// binding has a stored snapshot that loads and no binding since the
/// evidence was recorded went without one. The standing binding is what the
/// next bind reads as its previous, so this is the sweep's own rule asked
/// one bind early — not a second rule.
///
/// It assumes the next bind's own capture succeeds; if it does not, nothing
/// is compared against it and the line reads as it did.
pub fn assessable_by_next_bind(
    ledger: &Ledger,
    change_id: &str,
    evidence_kind: &str,
    evidence_id: &str,
) -> Result<bool> {
    let row = LinkedRow {
        id: String::new(),
        criterion_index: 0,
        evidence_kind: evidence_kind.to_string(),
        evidence_id: evidence_id.to_string(),
    };
    if evidence_own_tree(ledger, &row)?.is_some() {
        return Ok(true);
    }
    let standing = crate::examined_inputs::standing(
        ledger,
        crate::examined_inputs::SubjectKind::Intended,
        change_id,
    )?;
    let Some(chain_id) = standing
        .as_ref()
        .and_then(|s| s.snapshot_id.value())
        .map(str::to_string)
    else {
        return Ok(false);
    };
    if crate::snapshot::load(ledger, &chain_id)?.is_none() {
        return Ok(false);
    }
    Ok(!capture_gap_since(ledger, change_id, &row)?)
}

/// `(scope_version, allowed_scope, check_profile)` of the standing accepted
/// scope.
type StandingScope = (u32, Vec<String>, Vec<String>);

fn standing_scope(ledger: &Ledger, change_id: &str) -> Result<Option<StandingScope>> {
    let row = ledger
        .conn()
        .query_row(
            "SELECT r.scope_version, r.allowed_scope_json, r.check_profile_json
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
    let Some((version, allowed_raw, profile_raw)) = row else {
        return Ok(None);
    };
    let parse = |raw: &str| serde_json::from_str::<Vec<String>>(raw).unwrap_or_default();
    Ok(Some((version, parse(&allowed_raw), parse(&profile_raw))))
}

/// Assess every still-linked criterion-evidence row of the standing accepted
/// scope against a tree move from `old_tree` to `new_tree`, and record what
/// each one reads. Written when the Change's tree version moves — the caller
/// supplies the two whole-tree manifests (a mapping from path to content
/// digest); this module derives no manifest of its own so it stays a pure
/// function of what it is given (replay-deterministic, testable without a
/// worktree).
///
/// Returns one record per still-linked row, in id order.
#[allow(clippy::too_many_arguments)]
pub fn assess_tree_move(
    ledger: &Ledger,
    change_id: &str,
    old_snapshot: &str,
    new_snapshot: &str,
    old_tree: &BTreeMap<String, String>,
    new_tree: &BTreeMap<String, String>,
    moves: Option<&CommitMoves>,
    at: &str,
) -> Result<Vec<ApplicabilityRecord>> {
    assess_rows(
        ledger,
        change_id,
        Some((old_snapshot, old_tree)),
        new_snapshot,
        new_tree,
        moves,
        at,
    )
}

/// **NEVER-ASSESSED-NOT-READY-R2 — the one place the sweep is decided.**
/// [`assess_tree_move`] and [`on_intended_bound`] both come here.
///
/// `chain` is the Change's previous standing `intended` tree, when one was
/// stored and loads. It is only ever the baseline for a row that has no tree
/// of its own ([`evidence_own_tree`]), and only when no binding since that
/// row's evidence was recorded went without a snapshot
/// ([`capture_gap_since`]): across such a gap, nothing on record says what
/// the files were when the evidence was produced, and comparing two trees
/// captured after the gap would call it "still applies" without ever having
/// looked at the tree it was checked on.
///
/// A row with no baseline is **not assessed and gets no record** — there is
/// no third state in `change_criterion_freshness` (no DDL), so the reason is
/// served at read by [`assessable_by_next_bind`], the same rule, and the
/// line reads `readiness_could_not_assess` rather than promising a
/// `change prepare` that cannot settle it. Every row that IS assessed is
/// compared against `new_tree`, the tree the new binding captured.
fn assess_rows(
    ledger: &Ledger,
    change_id: &str,
    chain: Option<(&str, &BTreeMap<String, String>)>,
    new_snapshot: &str,
    new_tree: &BTreeMap<String, String>,
    moves: Option<&CommitMoves>,
    at: &str,
) -> Result<Vec<ApplicabilityRecord>> {
    let Some((scope_version, allowed_scope, check_profile)) = standing_scope(ledger, change_id)?
    else {
        return Ok(Vec::new());
    };
    let new_hash = tree_fingerprint(new_tree);
    let rows = linked_rows(ledger, change_id, scope_version)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        // T-3-3-F1: each item is assessed against the tree IT was produced
        // against, not against the Change's previous standing Intended.
        let own = evidence_own_tree(ledger, &row)?;
        let (base_snapshot, base_tree, base_head) = match (&own, chain) {
            (Some((id, tree, head)), _) => (id.as_str(), tree, head.as_deref()),
            (None, Some((old_snapshot, old_tree)))
                if !capture_gap_since(ledger, change_id, &row)? =>
            {
                (
                    old_snapshot,
                    old_tree,
                    moves.and_then(CommitMoves::fallback_head),
                )
            }
            // No baseline: could not be assessed. Said at read, never
            // recorded as a conclusion (see the function doc).
            _ => continue,
        };
        // T-F-1-F5: what moved by being committed is in neither manifest, so
        // it is asked for by commit id, from the snapshot this row was
        // produced against to the one being bound now.
        let mut changed = changed_paths(base_tree, new_tree);
        changed.extend(moves.map(|m| m.paths_since(base_head)).unwrap_or_default());
        changed.sort();
        changed.dedup();
        let old_hash = tree_fingerprint(base_tree);
        let declared = declared_paths_for(&check_profile, row.criterion_index);
        let invalidating = invalidating_paths(&changed, declared.as_deref(), &allowed_scope);
        let (state, reason_class, reason_paths) = if invalidating.is_empty() {
            (ApplicabilityState::Applies, None, Vec::new())
        } else {
            // Priority order matches the module doc: a lockfile or config
            // file among the invalidating paths outranks a plain source or
            // test path, because "the lockfile moved" is the more specific
            // and more actionable fact.
            let mut by_class: BTreeMap<ReasonClass, Vec<String>> = BTreeMap::new();
            for path in &invalidating {
                by_class
                    .entry(classify_path(path))
                    .or_default()
                    .push(path.clone());
            }
            // `invalidating` is non-empty and `classify_path` is total, so one
            // of the four buckets holds it; the `unwrap_or` arm is otherwise
            // unreachable and exists only so this stays panic-free (no
            // `expect` in `crates/core`) rather than because it runs.
            let chosen = [
                ReasonClass::DependenciesChanged,
                ReasonClass::ConfigurationChanged,
                ReasonClass::TestsChanged,
                ReasonClass::SourceChanged,
            ]
            .into_iter()
            .find_map(|class| by_class.remove(&class).map(|paths| (class, paths)))
            .unwrap_or((ReasonClass::SourceChanged, invalidating.clone()));
            (
                ApplicabilityState::NoLongerApplies,
                Some(chosen.0),
                chosen.1,
            )
        };
        let id = record(
            ledger,
            change_id,
            &row.id,
            row.criterion_index,
            scope_version,
            base_snapshot,
            new_snapshot,
            state,
            reason_class,
            &reason_paths,
            Some(&old_hash),
            Some(&new_hash),
            at,
        )?;
        out.push(ApplicabilityRecord {
            id,
            change_id: change_id.to_string(),
            criterion_evidence_id: row.id,
            criterion_index: row.criterion_index,
            scope_version,
            old_snapshot: base_snapshot.to_string(),
            new_snapshot: new_snapshot.to_string(),
            state,
            reason_class,
            state_slug: state_slug(state),
            reason_slug: reason_class.map(ReasonClass::slug),
            reason_paths,
            checked_paths: match &declared {
                Some(paths) if !paths.is_empty() => paths.clone(),
                _ => allowed_scope.clone(),
            },
            old_input_hash: Some(old_hash.clone()),
            new_input_hash: Some(new_hash.clone()),
            recorded_at: at.to_string(),
        });
    }
    Ok(out)
}

/// Reassess when a new scope version is accepted (item 4): for every
/// criterion index present in both the old and the new criteria list, whose
/// text changed, every still-linked criterion-evidence row from the OLD scope
/// version at that index is marked `criteria_changed — needs reassessment`.
/// A criterion whose text is unchanged keeps its existing records untouched —
/// this function writes nothing for it. A criterion that is new in the
/// accepted version has no old evidence to mark; its evidence starts fresh
/// once `derive_links` links something to it.
pub fn reassess_for_scope_change(
    ledger: &Ledger,
    change_id: &str,
    old_scope_version: u32,
    new_scope_version: u32,
    at: &str,
) -> Result<Vec<ApplicabilityRecord>> {
    let get_criteria = |version: u32| -> Result<Vec<String>> {
        let raw: Option<String> = ledger
            .conn()
            .query_row(
                "SELECT criteria_json FROM change_scope_revisions
                  WHERE change_id=?1 AND scope_version=?2",
                params![change_id, version],
                |row| row.get(0),
            )
            .optional()?;
        Ok(raw
            .map(|r| serde_json::from_str::<Vec<String>>(&r).unwrap_or_default())
            .unwrap_or_default())
    };
    let old_criteria = get_criteria(old_scope_version)?;
    let new_criteria = get_criteria(new_scope_version)?;
    let rows = linked_rows(ledger, change_id, old_scope_version)?;
    let mut out = Vec::new();
    for row in rows {
        let idx = row.criterion_index as usize;
        let old_text = old_criteria.get(idx.saturating_sub(1));
        let new_text = new_criteria.get(idx.saturating_sub(1));
        let changed = match (old_text, new_text) {
            (Some(o), Some(n)) => o != n,
            _ => false,
        };
        if !changed {
            continue;
        }
        let old_snapshot = format!("scope_version:{old_scope_version}");
        let new_snapshot = format!("scope_version:{new_scope_version}");
        let id = record(
            ledger,
            change_id,
            &row.id,
            row.criterion_index,
            old_scope_version,
            &old_snapshot,
            &new_snapshot,
            ApplicabilityState::NoLongerApplies,
            Some(ReasonClass::CriteriaChanged),
            &[],
            None,
            None,
            at,
        )?;
        out.push(ApplicabilityRecord {
            id,
            change_id: change_id.to_string(),
            criterion_evidence_id: row.id,
            criterion_index: row.criterion_index,
            scope_version: old_scope_version,
            old_snapshot,
            new_snapshot,
            state: ApplicabilityState::NoLongerApplies,
            reason_class: Some(ReasonClass::CriteriaChanged),
            state_slug: state_slug(ApplicabilityState::NoLongerApplies),
            reason_slug: Some(ReasonClass::CriteriaChanged.slug()),
            reason_paths: Vec::new(),
            checked_paths: Vec::new(),
            old_input_hash: None,
            new_input_hash: None,
            recorded_at: at.to_string(),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// The trigger: called when the Change's tree version moves.
// ---------------------------------------------------------------------------

/// What moved by being **committed**, which no tree manifest can see.
///
/// T-F-1-F5. A [`crate::snapshot::WorkspaceSnapshot`]'s manifests list the
/// dirty working tree only, so an edit that has been committed is invisible
/// to [`changed_paths`]: both snapshots are clean and compare as identical.
/// `FOUNDER-TEST.md` step 7 is exactly that sequence — check, edit, commit,
/// `change prepare` — and before this it left a passing run reading "still
/// applies" while the screen told the reader nothing had ever been checked.
///
/// Held as a separate, caller-supplied reader so [`assess_tree_move`] stays
/// a function of the trees it is given: pass `None` and it behaves exactly
/// as it did, which is what its unit tests (hand-made manifests, no
/// repository) do.
///
/// **Stated limit.** When git cannot answer — either commit unknown here, a
/// shallow clone, a rewritten history — this names no paths. It does not
/// invent one and it does not upgrade silence into a verdict; such a move
/// reads the same as before this fix.
pub struct CommitMoves {
    root: std::path::PathBuf,
    new_head: String,
    fallback_head: Option<String>,
}

impl CommitMoves {
    /// `None` when either side has no recorded HEAD (not a git worktree), or
    /// when both sides are the same commit — nothing was committed between
    /// them, so there is nothing this reader could add.
    pub fn between(
        old_snap: &crate::snapshot::WorkspaceSnapshot,
        new_snap: &crate::snapshot::WorkspaceSnapshot,
    ) -> Option<Self> {
        let old_head = old_snap.git.as_ref().and_then(|g| g.head.clone());
        let new_head = new_snap.git.as_ref()?.head.as_deref()?;
        if old_head.as_deref() == Some(new_head) {
            return None;
        }
        Self::onto(new_snap, old_head)
    }

    /// Commits moved onto `new_snap`, with no previous tree of the Change's
    /// own to fall back to (NEVER-ASSESSED-NOT-READY-R2): only a row that
    /// carries its own recorded HEAD can ask it anything.
    fn onto(
        new_snap: &crate::snapshot::WorkspaceSnapshot,
        fallback_head: Option<String>,
    ) -> Option<Self> {
        let new_git = new_snap.git.as_ref()?;
        let new_head = new_git.head.clone()?;
        let root = if new_git.worktree_root.is_empty() {
            new_snap.root.clone()
        } else {
            new_git.worktree_root.clone()
        };
        Some(Self {
            root: std::path::PathBuf::from(root),
            new_head,
            fallback_head,
        })
    }

    fn fallback_head(&self) -> Option<&str> {
        self.fallback_head.as_deref()
    }

    /// The paths committed between `base_head` and the commit being bound
    /// now. Empty when the base is unknown, or is already the new commit —
    /// evidence produced on the commit being bound is not stale because of
    /// that commit.
    fn paths_since(&self, base_head: Option<&str>) -> Vec<String> {
        let Some(base) = base_head else {
            return Vec::new();
        };
        if base == self.new_head {
            return Vec::new();
        }
        crate::snapshot::committed_changed_paths(&self.root, base, &self.new_head)
            .unwrap_or_default()
    }
}

/// A path → content-digest manifest built from a stored [`crate::snapshot::WorkspaceSnapshot`]'s
/// own tree manifests — nothing re-observed, nothing scanned: staged,
/// unstaged and untracked entries carry the worktree's own sha256; a deleted
/// path carries the literal marker `deleted` as its digest, so its absence
/// still reads as a change against a snapshot where it was present.
pub fn manifest_from_snapshot(
    snapshot: &crate::snapshot::WorkspaceSnapshot,
) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for entry in snapshot
        .tree
        .staged
        .iter()
        .chain(&snapshot.tree.unstaged)
        .chain(&snapshot.tree.untracked)
    {
        if let Some(sha) = &entry.sha256 {
            out.insert(entry.path.clone(), sha.clone());
        }
    }
    for path in &snapshot.tree.deleted {
        out.insert(path.clone(), "deleted".to_string());
    }
    out
}

/// Called when a new `intended` examined-inputs record is bound for a Change
/// (T-3-2's `examined_inputs::bind_intended_at`, and `change prepare`) — the
/// moment this module answers "did the Change's tree version just move?" It
/// reads the PREVIOUS standing `intended` record (before the new one was
/// written) and the new one just written, loads the stored workspace
/// snapshots already on disk (`crate::snapshot::load`, T-2's own recorder)
/// and diffs their own tree manifests. No git is run here beyond the commit
/// reader, and no path is inferred beyond what was recorded.
///
/// Nothing is assessed when the NEW binding has no stored snapshot: there is
/// no tree to compare anything against. When the PREVIOUS binding has none
/// (or there is none), the sweep still runs (NEVER-ASSESSED-NOT-READY-R2):
/// every row that carries its own recorded tree is assessed from it, and a
/// row that has neither is left unassessed, which [`assessable_by_next_bind`]
/// serves as the reason. At 3476eb1f this case returned before looking at a
/// single row, silently, for the whole Change.
pub fn on_intended_bound(
    ledger: &Ledger,
    change_id: &str,
    previous: Option<&crate::examined_inputs::ExaminedInputs>,
    current: &crate::examined_inputs::ExaminedInputs,
    at: &str,
) -> Result<Vec<ApplicabilityRecord>> {
    let Some(new_id) = current.snapshot_id.value() else {
        return Ok(Vec::new());
    };
    let new_id = new_id.to_string();
    let old_id = previous
        .and_then(|p| p.snapshot_id.value())
        .map(str::to_string);
    if old_id.as_deref() == Some(new_id.as_str()) {
        return Ok(Vec::new());
    }
    let Some((new_snap, _)) = crate::snapshot::load(ledger, &new_id)? else {
        return Ok(Vec::new());
    };
    let old_snap = match &old_id {
        Some(id) => crate::snapshot::load(ledger, id)?.map(|(snap, _)| snap),
        None => None,
    };
    let new_tree = manifest_from_snapshot(&new_snap);
    let (old_tree, moves) = match &old_snap {
        Some(old) => (
            Some(manifest_from_snapshot(old)),
            CommitMoves::between(old, &new_snap),
        ),
        None => (None, CommitMoves::onto(&new_snap, None)),
    };
    let chain = match (&old_id, &old_tree) {
        (Some(id), Some(tree)) => Some((id.as_str(), tree)),
        _ => None,
    };
    assess_rows(
        ledger,
        change_id,
        chain,
        &new_id,
        &new_tree,
        moves.as_ref(),
        at,
    )
}
