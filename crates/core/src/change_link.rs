//! Session ↔ Change links: proposing them, deciding them, and the one gate a
//! result has to pass to reach a Change (brief `B3.8`–`B3.14`, `B8.4`, `L1.1`).
//!
//! T-2-1 built the table. This module builds the three things that make it
//! mean something, and the order of the three is the whole design:
//!
//! 1. **A suggester that only ever suggests.** [`suggest`] proposes sessions
//!    from two mechanical bases — the Change's activity window overlapping the
//!    session's capture window (`timing`), and the session's changed paths
//!    falling inside the Change's allowed scope (`filename`). Every proposal
//!    carries the basis and the evidence for it, and nothing it writes is
//!    anything other than a `suggested` row. No model, no ranking, no
//!    tie-break: a session that matches both ways produces two proposals and
//!    is named as a tie rather than resolved (`B8.4`).
//! 2. **A decision only a person can make.** [`decide`] appends a `confirmed`
//!    or `rejected` row with the actor, the time, basis `user` and the words
//!    the person gave. It never edits or deletes the suggestion it answers —
//!    the table refuses that — so "what did the tool think, and what did the
//!    person say" stay two separate readable facts.
//! 3. **A gate nothing goes around.** [`applicability`] is the only function
//!    in this workspace that decides whether a session's recorded outcomes may
//!    be read under a Change, and [`change_results`] is the only caller of it
//!    that serves results. A confirmed link is not enough: the session's
//!    changed paths must also fall inside the accepted scope's allowed paths
//!    (`B3.13`). Either missing, and the answer is a refusal sentence from the
//!    copy file and no numbers at all.
//!
//! **Nothing here writes a result anywhere.** The association table has no
//! column to hold one (T-2-1 pinned its exact column list), and this module
//! adds no second place. A Change "has" a session's outcome only in the sense
//! that a query may be permitted to read it, and the permission is recomputed
//! every time rather than stored.
//!
//! **Nothing here fabricates a criterion.** Associating a decade-old session to
//! a Change appends one row to one table. `change_scope_revisions` and
//! `change_scope_acceptances` are never written from this module — `B3.16`,
//! and there is a test that reads both tables before and after every act here.
//!
//! No `unwrap`, no `expect`, no network, no `unsafe`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path};

use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use ulid::Ulid;

use crate::chain::sha256_hex;
use crate::change::{
    mask_list, mask_text, AssociationBasis, AssociationProvenance, NewAssociation,
};
use crate::error::{CoreError, Result};
use crate::ledger::Ledger;
use crate::redact::MaskRecord;

/// The served shape's version for everything in this module.
pub const LINK_VIEW_SCHEMA_VERSION: u32 = 1;

/// The name of the one function that decides whether a result may be read
/// under a Change.
///
/// Exported so the call-site census can name what it is enumerating instead of
/// hard-coding a string in the test file, which is how a census quietly stops
/// matching.
pub const SCOPE_GATE: &str = "applicability";

// ---------------------------------------------------------------------------
// Time, handled explicitly rather than trusted
// ---------------------------------------------------------------------------

/// A closed interval of canonical UTC time, both ends inclusive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Window {
    pub from: String,
    pub to: String,
}

impl Window {
    /// Do these two intervals share at least one instant?
    pub fn overlaps(&self, other: &Window) -> bool {
        self.from <= other.to && other.from <= self.to
    }
}

/// Is this the exact UTC form the store writes, so that string comparison is
/// time comparison?
///
/// Deliberately strict. A timestamp in any other shape sorts wrongly against
/// these, and a suggester that silently mis-sorts is worse than one that says
/// it could not tell (Rule 12).
///
/// This is the *target* form, not the gate. What a vendor wrote goes through
/// [`normalise_utc`] first; only a value that cannot be brought to this form
/// without guessing makes the session "undated", and undated sessions are
/// listed by name in the report.
pub fn is_canonical_utc(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 20 || b[19] != b'Z' || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    if b[10] != b'T' || b[13] != b':' || b[16] != b':' {
        return false;
    }
    [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
        .iter()
        .all(|i| b[*i].is_ascii_digit())
}

/// The canonical form of a UTC timestamp a vendor actually wrote, or `None`
/// when this cannot be read as one without guessing.
///
/// `is_canonical_utc` above describes the form **this store** writes. It is not
/// the form every recorder writes, and treating "not our spelling" as "no time"
/// is what `T-2-8-F2` found on real capture: Claude Code stamps milliseconds
/// (`2026-09-18T08:42:07.914Z`, 24 bytes), the length check rejected it, and
/// every real session was filed as undated — so the timing basis could not fire
/// on a single real recording.
///
/// Two spellings are accepted beside the canonical one, and no others:
///
/// * **fractional seconds** — `…:07.914Z` — truncated to the whole second.
///   Truncation, not rounding: the floor of a stamp is a real instant the work
///   was already under way at, and a rounded one can be an instant that had not
///   happened yet. It is also the only choice that keeps string comparison and
///   time comparison the same thing, which is this module's whole sort order.
/// * **an explicit zero offset** — `+00:00` or `-00:00` — rewritten to `Z`.
///
/// A non-zero offset is NOT accepted: converting it needs arithmetic on a
/// calendar, and a suggester that quietly shifts somebody's clock is worse than
/// one that says it could not tell (Rule 12). It stays undated and is named.
///
/// Pure: same string in, same string out, no clock, no locale. Nothing is
/// written anywhere — the normalisation happens at read, so no store grows by
/// one byte for it (`03-USER-LENS.md` rule 13).
pub fn normalise_utc(value: &str) -> Option<String> {
    let value = value.trim();
    if is_canonical_utc(value) {
        return Some(value.to_string());
    }
    // Strip an explicit zero offset, leaving the same instant.
    let body = match value
        .strip_suffix("+00:00")
        .or_else(|| value.strip_suffix("-00:00"))
    {
        Some(rest) => rest,
        None => value.strip_suffix('Z')?,
    };
    // Whole seconds, with anything after the decimal point dropped.
    let seconds = match body.split_once('.') {
        Some((whole, fraction)) => {
            if fraction.is_empty() || !fraction.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            whole
        }
        None => body,
    };
    let candidate = format!("{seconds}Z");
    is_canonical_utc(&candidate).then_some(candidate)
}

fn canonical(value: Option<String>) -> Option<String> {
    value.as_deref().and_then(normalise_utc)
}

// ---------------------------------------------------------------------------
// What a session touched
// ---------------------------------------------------------------------------

/// The tool names whose calls mean "a file was changed", lowercased.
///
/// Pinned as a constant with a test rather than written inline, because the
/// set is a claim about the capture vocabulary and a silent addition to it
/// would silently widen what counts as in scope. A tool outside this list is
/// not evidence that a file changed, and is not counted.
pub const EDIT_TOOLS: &[&str] = &[
    "edit",
    "write",
    "multiedit",
    "notebookedit",
    "str_replace_editor",
    "apply_patch",
];

/// The JSON keys a changed path can arrive under. Same list `derive` and
/// `identity` already use, so one vocabulary, not three.
pub const PATH_KEYS: &[&str] = &["file_path", "filePath", "path", "notebook_path"];

/// The paths one session changed, project-relative where the project's root is
/// a prefix, deduplicated and sorted.
///
/// Reads `events` only, by id, and copies nothing into any Change table.
pub fn session_changed_paths(ledger: &Ledger, session_id: &str) -> Result<Vec<String>> {
    let project_id: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT project_id FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .optional()?;
    let root = project_id
        .as_deref()
        .map(|project| crate::workspace::established_project_root(ledger, project, None))
        .transpose()?
        .flatten();
    changed_paths_with_root(ledger, session_id, root.as_deref())
}

/// The changed paths for a session under the root this Change already bound.
/// An explicit sibling-worktree binding remains the source of scope, rather
/// than being replaced later by a project-level captured root.
pub fn session_changed_paths_for_change(
    ledger: &Ledger,
    change_id: &str,
    session_id: &str,
) -> Result<Vec<String>> {
    let change_project: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT project_id FROM changes WHERE id=?1",
            params![change_id],
            |row| row.get(0),
        )
        .optional()?;
    let session_project: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT project_id FROM sessions WHERE id=?1",
            params![session_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(project_id) = change_project else {
        return Ok(Vec::new());
    };
    if session_project.as_deref() != Some(project_id.as_str()) {
        return Ok(Vec::new());
    }
    let bound = crate::examined_inputs::standing(
        ledger,
        crate::examined_inputs::SubjectKind::Intended,
        change_id,
    )?
    .and_then(|row| row.worktree_path.value().map(std::path::PathBuf::from));
    let root = match bound {
        Some(root) if root.is_dir() => Some(root),
        _ => crate::workspace::established_project_root(ledger, &project_id, None)?,
    };
    changed_paths_with_root(ledger, session_id, root.as_deref())
}

fn changed_paths_with_root(
    ledger: &Ledger,
    session_id: &str,
    root: Option<&Path>,
) -> Result<Vec<String>> {
    let mut statement = ledger.conn().prepare(
        "SELECT tool_name, tool_input FROM events
          WHERE session_id = ?1 AND kind = 'tool_use' AND tool_input IS NOT NULL
          ORDER BY seq",
    )?;
    let rows = statement.query_map(params![session_id], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?,
            row.get::<_, Option<String>>(1)?,
        ))
    })?;
    let mut out = BTreeSet::new();
    for row in rows {
        let (tool, input) = row?;
        let (Some(tool), Some(input)) = (tool, input) else {
            continue;
        };
        if !EDIT_TOOLS.contains(&tool.to_ascii_lowercase().as_str()) {
            continue;
        }
        for key in PATH_KEYS {
            if let Some(value) = crate::derive::json_string_field(&input, key) {
                if let Some(path) = relative_to(&value, root) {
                    out.insert(path);
                }
                break;
            }
        }
    }
    Ok(out.into_iter().collect())
}

/// A path as the scope would spell it: relative to an established workspace
/// root. Absolute paths without that provenance, paths outside the root, and
/// parent traversal abstain instead of becoming a broad machine-path scope.
/// `strip_prefix` keeps the required directory-boundary behavior. The one
/// `/var` to `/private/var` conversion is a verified macOS filesystem alias;
/// no punctuation or stored-root reconstruction is attempted.
fn relative_to(path: &str, root: Option<&Path>) -> Option<String> {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        let root = root?;
        if let Ok(inside) = candidate.strip_prefix(root) {
            return clean_relative(inside);
        }
        // macOS exposes `/var` through `/private/var`. This is a verified
        // operating-system alias of the established root, not punctuation
        // recovery, and still works when the changed file was deleted.
        if root.starts_with("/private/var") && path.starts_with("/var/") {
            let private = Path::new("/private").join(path.trim_start_matches('/'));
            if let Ok(inside) = private.strip_prefix(root) {
                return clean_relative(inside);
            }
        }
        let observed = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.to_path_buf());
        let inside = observed.strip_prefix(root).ok()?;
        return clean_relative(inside);
    }
    clean_relative(candidate)
}

fn clean_relative(path: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

// ---------------------------------------------------------------------------
// Scope matching
// ---------------------------------------------------------------------------

/// What an allowed-scope entry did with one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternVerdict {
    Matched,
    DidNotMatch,
    /// The entry is not a shape this matcher understands. It matches nothing
    /// and is reported, never treated as "matches everything" or as "matches
    /// nothing" silently.
    Unreadable,
}

/// Three shapes, and only three: the whole tree (`**`), a subtree
/// (`dir/**` or `dir/`), and one exact path.
///
/// Everything else is [`PatternVerdict::Unreadable`]. A half-implemented glob
/// is how a scope check starts letting things through that nobody agreed to;
/// a matcher that says "I do not understand this line" is one a person can fix.
pub fn pattern_matches(pattern: &str, path: &str) -> PatternVerdict {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return PatternVerdict::Unreadable;
    }
    if pattern == "**" {
        return PatternVerdict::Matched;
    }
    if let Some(prefix) = pattern.strip_suffix("/**").or_else(|| {
        pattern
            .strip_suffix('/')
            .map(|p| p.trim_end_matches('/'))
            .filter(|p| !p.is_empty())
    }) {
        if prefix.contains('*') || prefix.contains('?') {
            return PatternVerdict::Unreadable;
        }
        let prefix = prefix.trim_end_matches('/');
        return if path == prefix || path.starts_with(&format!("{prefix}/")) {
            PatternVerdict::Matched
        } else {
            PatternVerdict::DidNotMatch
        };
    }
    if pattern.contains('*') || pattern.contains('?') {
        return PatternVerdict::Unreadable;
    }
    if pattern == path {
        PatternVerdict::Matched
    } else {
        PatternVerdict::DidNotMatch
    }
}

/// The standing scope's allowed paths, with its version.
fn standing_allowed_scope(ledger: &Ledger, change_id: &str) -> Result<Option<(u32, Vec<String>)>> {
    let row: Option<(u32, String)> = ledger
        .conn()
        .query_row(
            "SELECT scope_version, allowed_scope_json FROM change_scope_revisions
              WHERE change_id = ?1 ORDER BY scope_version DESC LIMIT 1",
            params![change_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((version, json)) = row else {
        return Ok(None);
    };
    let allowed: Vec<String> = serde_json::from_str(&json).unwrap_or_default();
    Ok(Some((version, allowed)))
}

/// Split one session's paths against one scope's allowed paths.
struct ScopeSplit {
    inside: Vec<String>,
    outside: Vec<String>,
    unreadable_patterns: Vec<String>,
}

fn split_by_scope(paths: &[String], allowed: &[String]) -> ScopeSplit {
    let mut unreadable = BTreeSet::new();
    let mut inside = Vec::new();
    let mut outside = Vec::new();
    for path in paths {
        let mut hit = false;
        for pattern in allowed {
            match pattern_matches(pattern, path) {
                PatternVerdict::Matched => hit = true,
                PatternVerdict::DidNotMatch => {}
                PatternVerdict::Unreadable => {
                    unreadable.insert(pattern.clone());
                }
            }
        }
        if hit {
            inside.push(path.clone());
        } else {
            outside.push(path.clone());
        }
    }
    ScopeSplit {
        inside,
        outside,
        unreadable_patterns: unreadable.into_iter().collect(),
    }
}

// ---------------------------------------------------------------------------
// The suggester
// ---------------------------------------------------------------------------

/// One proposal, with the evidence that produced it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Suggestion {
    pub session_id: String,
    pub basis: AssociationBasis,
    /// The instants both windows share. Present on a `timing` proposal only.
    pub overlap: Option<Window>,
    /// The paths that are both in the session and inside the scope. Present on
    /// a `filename` proposal only.
    pub intersecting_paths: Vec<String>,
    /// The copy-file slug for the sentence this proposal is shown with.
    pub sentence_slug: &'static str,
}

/// What the suggester found, including what it could not decide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SuggestionReport {
    pub schema_version: u32,
    pub change_id: String,
    /// `None` when the Change's own timestamps are not canonical UTC, in which
    /// case the timing basis was skipped entirely and this says so.
    pub change_window: Option<Window>,
    pub scope_version: Option<u32>,
    pub sessions_considered: u64,
    /// Sorted by `(session_id, basis)`. Same store, same list, always.
    pub suggestions: Vec<Suggestion>,
    /// Sessions that matched on both bases. Reported, never resolved.
    pub ties: Vec<String>,
    /// The copy-file slug for the tie sentence, when there is a tie.
    pub ties_slug: Option<&'static str>,
    /// Sessions with no usable timestamp, named rather than dropped.
    pub undated_sessions: Vec<String>,
    /// The copy-file slug for what an undated session means.
    pub undated_slug: Option<&'static str>,
    /// The copy-file slug for the one sentence that says how much recorded work
    /// was **not considered** because it could not be placed in time.
    ///
    /// Separate from `undated_slug` because it is a different sentence for a
    /// different job: that one explains one session, this one says what the
    /// whole answer leaves out. `T-2-8-F3`: with no suggestions, a surface that
    /// printed only "nothing looks like it belongs" told a person an empty
    /// result where the truth was an unread one, while this report already held
    /// the reason. The count that fills it is `undated_sessions.len()`, a typed
    /// field, never a number a surface arrives at on its own.
    pub undated_summary_slug: Option<&'static str>,
    /// Allowed-scope entries this matcher does not understand.
    pub unreadable_scope_patterns: Vec<String>,
    /// The copy-file slug for a scope line the matcher cannot read.
    pub unreadable_scope_slug: Option<&'static str>,
    /// The copy-file slug for the one sentence that says how many lines of what
    /// this Change covers could not be read, and so matched nothing.
    pub unreadable_scope_summary_slug: Option<&'static str>,
    /// The copy-file slug when there is nothing to show, `None` otherwise.
    pub empty_slug: Option<&'static str>,
    pub masking: crate::redact::Treatment,
}

/// The Change's activity window: from when it was opened to the last moment
/// anything was written down about it.
///
/// Every end-point is a stored field — `opened_at`, a scope's `proposed_at`, an
/// acceptance's `accepted_at`, a binding's `bound_at`. Nothing is inferred from
/// a clock, so the same store always produces the same window.
pub fn change_window(ledger: &Ledger, change_id: &str) -> Result<Option<Window>> {
    let opened: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT opened_at FROM changes WHERE id = ?1",
            params![change_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(from) = canonical(opened) else {
        return Ok(None);
    };
    let mut to = from.clone();
    for sql in [
        "SELECT MAX(proposed_at) FROM change_scope_revisions WHERE change_id = ?1",
        "SELECT MAX(accepted_at) FROM change_scope_acceptances WHERE change_id = ?1",
        "SELECT MAX(bound_at) FROM change_implementations WHERE change_id = ?1",
    ] {
        let value: Option<String> = ledger
            .conn()
            .query_row(sql, params![change_id], |row| row.get(0))
            .optional()?
            .flatten();
        if let Some(value) = canonical(value) {
            if value > to {
                to = value;
            }
        }
    }
    Ok(Some(Window { from, to }))
}

/// One session's capture window, from the events themselves, falling back to
/// the session row's own start.
pub fn session_window(ledger: &Ledger, session_id: &str) -> Result<Option<Window>> {
    let bounds: Option<(Option<String>, Option<String>)> = ledger
        .conn()
        .query_row(
            "SELECT MIN(ts), MAX(ts) FROM events WHERE session_id = ?1 AND ts IS NOT NULL",
            params![session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((Some(lo), Some(hi))) = bounds {
        if let (Some(from), Some(to)) = (canonical(Some(lo)), canonical(Some(hi))) {
            return Ok(Some(Window { from, to }));
        }
    }
    let started: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT started_at FROM sessions WHERE id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(canonical(started).map(|t| Window {
        from: t.clone(),
        to: t,
    }))
}

/// Propose sessions for one Change. Reads only; writes nothing.
///
/// Determinism is the contract: the same store produces the same report, byte
/// for byte, because every input is a stored field and the output is sorted by
/// `(session_id, basis)`. There is no clock in it and no randomness.
pub fn suggest(ledger: &Ledger, change_id: &str) -> Result<SuggestionReport> {
    let project_id: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT project_id FROM changes WHERE id = ?1",
            params![change_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(project_id) = project_id else {
        return Err(CoreError::Config(format!(
            "no change with id {change_id:?}"
        )));
    };

    let window = change_window(ledger, change_id)?;
    let scope = standing_allowed_scope(ledger, change_id)?;

    let mut statement = ledger
        .conn()
        .prepare("SELECT id FROM sessions WHERE project_id = ?1 ORDER BY id")?;
    let session_ids: Vec<String> = statement
        .query_map(params![project_id], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut record = MaskRecord::default();
    let mut timing = BTreeSet::new();
    let mut filename = BTreeSet::new();
    let mut suggestions = Vec::new();
    let mut undated = Vec::new();
    let mut unreadable = BTreeSet::new();

    for session_id in &session_ids {
        // Timing.
        match session_window(ledger, session_id)? {
            Some(session_span) => {
                if let Some(change_span) = &window {
                    if session_span.overlaps(change_span) {
                        timing.insert(session_id.clone());
                        suggestions.push(Suggestion {
                            session_id: session_id.clone(),
                            basis: AssociationBasis::Timing,
                            overlap: Some(Window {
                                from: session_span.from.max(change_span.from.clone()),
                                to: session_span.to.min(change_span.to.clone()),
                            }),
                            intersecting_paths: Vec::new(),
                            sentence_slug: "suggestion_timing",
                        });
                    }
                }
            }
            None => undated.push(session_id.clone()),
        }

        // Filenames.
        if let Some((_, allowed)) = &scope {
            let paths = session_changed_paths_for_change(ledger, change_id, session_id)?;
            let split = split_by_scope(&paths, allowed);
            for pattern in split.unreadable_patterns {
                unreadable.insert(pattern);
            }
            if !split.inside.is_empty() {
                filename.insert(session_id.clone());
                suggestions.push(Suggestion {
                    session_id: session_id.clone(),
                    basis: AssociationBasis::Filename,
                    overlap: None,
                    intersecting_paths: mask_list(split.inside, &mut record),
                    sentence_slug: "suggestion_filename",
                });
            }
        }
    }

    suggestions.sort_by(|a, b| {
        (a.session_id.as_str(), a.basis.as_str()).cmp(&(b.session_id.as_str(), b.basis.as_str()))
    });
    let ties: Vec<String> = timing.intersection(&filename).cloned().collect();
    let empty_slug = if suggestions.is_empty() {
        Some("suggestion_none")
    } else {
        None
    };

    let unreadable: Vec<String> = unreadable.into_iter().collect();
    // Every sentence this report can be shown with is named here, by the
    // engine, so no surface has to decide which words to use for a tie, an
    // undated session or a scope line nobody can read.
    let slug_if = |present: bool, slug: &'static str| present.then_some(slug);
    Ok(SuggestionReport {
        schema_version: LINK_VIEW_SCHEMA_VERSION,
        change_id: change_id.to_string(),
        change_window: window,
        scope_version: scope.map(|(v, _)| v),
        sessions_considered: session_ids.len() as u64,
        suggestions,
        ties_slug: slug_if(!ties.is_empty(), "suggestion_tie"),
        ties,
        undated_slug: slug_if(!undated.is_empty(), "session_undated"),
        undated_summary_slug: slug_if(!undated.is_empty(), "session_undated_summary"),
        undated_sessions: undated,
        unreadable_scope_slug: slug_if(!unreadable.is_empty(), "scope_pattern_unreadable"),
        unreadable_scope_summary_slug: slug_if(
            !unreadable.is_empty(),
            "scope_pattern_unreadable_summary",
        ),
        unreadable_scope_patterns: unreadable,
        empty_slug,
        masking: record.to_treatment(),
    })
}

/// The id a proposal wears, derived from its content so that re-running the
/// suggester on an unchanged store cannot produce a second row for the same
/// proposal.
pub fn suggestion_id(change_id: &str, session_id: &str, basis: AssociationBasis) -> String {
    let digest =
        sha256_hex(format!("{change_id}\u{1f}{session_id}\u{1f}{}", basis.as_str()).as_bytes());
    format!("csa_{}", &digest[..32])
}

/// What recording a set of proposals did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct RecordOutcome {
    pub written: Vec<String>,
    /// Pairs left alone because a person has already decided them. A
    /// suggestion cannot argue with a decision.
    pub skipped_decided: Vec<String>,
    /// Proposals already on file, unchanged.
    pub skipped_existing: Vec<String>,
}

/// Append `suggested` rows for a report's proposals.
///
/// Three rules, each one a row this refuses to write: a proposal is never
/// written twice; a pair a person has confirmed or rejected is left alone; and
/// nothing written here carries an actor, because a machine basis may not name
/// one — the table's CHECK refuses that anyway, and this does not rely on it.
pub fn record_suggestions(
    ledger: &Ledger,
    report: &SuggestionReport,
    asserted_at: &str,
) -> Result<RecordOutcome> {
    let mut outcome = RecordOutcome::default();
    for suggestion in &report.suggestions {
        let id = suggestion_id(&report.change_id, &suggestion.session_id, suggestion.basis);
        if let Some(standing) = standing_link(ledger, &report.change_id, &suggestion.session_id)? {
            if standing.provenance != AssociationProvenance::Suggested {
                outcome.skipped_decided.push(id);
                continue;
            }
        }
        let exists: Option<i64> = ledger
            .conn()
            .query_row(
                "SELECT 1 FROM change_session_associations WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        if exists.is_some() {
            outcome.skipped_existing.push(id);
            continue;
        }
        crate::change::append_association(
            ledger,
            &NewAssociation {
                id: id.clone(),
                change_id: report.change_id.clone(),
                session_id: suggestion.session_id.clone(),
                provenance: AssociationProvenance::Suggested,
                basis: suggestion.basis,
                actor: None,
                asserted_at: asserted_at.to_string(),
                reason: None,
            },
        )?;
        outcome.written.push(id);
    }
    Ok(outcome)
}

// ---------------------------------------------------------------------------
// The session one captured execution is recorded under (T-3-2b)
// ---------------------------------------------------------------------------

/// One session `thruline check` wrote for one captured execution.
///
/// Three facts the gate cannot otherwise know, and nothing else: this session
/// is an execution rather than a transcript, which execution it is, and which
/// Change the person who typed the command ran it for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckSession {
    pub session_id: String,
    pub binding_id: String,
    pub change_id: String,
    /// The folder the command ran in, as the wrapper recorded it.
    pub ran_in: String,
    pub actor: String,
    pub recorded_at: String,
}

/// What `thruline check --for-change` asks this module to write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCheckSession {
    pub session_id: String,
    pub project_id: String,
    pub binding_id: String,
    pub change_id: String,
    pub ran_in: String,
    pub actor: String,
    pub recorded_at: String,
}

/// Write the session one captured execution belongs to, and confirm it onto
/// the Change it was run for.
///
/// **One transaction, three rows, no fourth.** The `sessions` row (so the
/// join every piece of evidence travels through has something to join to), the
/// `change_check_sessions` row (the typed fact that says what kind of session it is),
/// and the person's `confirmed` association with basis `user` and their name on
/// it. It writes no scope version, no criterion link and no result: this is the
/// same act `change associate --confirm` is, made by the same kind of person,
/// and the id they typed after `--for-change` is what they asserted.
///
/// **And one file, written before the rows** (ADV-run-ledger-5, lane `B5`):
/// the run's own record, one line, beside the store (see
/// [`CHECK_RECORD_DIR`]). Every other session row names a file and stores
/// that file's whole-file digest, and `thruline verify` recomputes it; a check
/// run has no vendor transcript, so before this its row named the folder the
/// command ran in and stored no digest, and verify said FAIL after every run
/// (from the run sheet and from `thruline check --for-change` alike). The row
/// now names the record and carries its digest, taken by the same function
/// verify uses. Where the command ran stays in `change_check_sessions.ran_in`.
///
/// A check run for no Change never reaches here at all: the wrapper calls this
/// only when a Change was named, so the capture stays in `evidence_bindings`,
/// discoverable by `thruline evidence`, and attached to nothing.
pub fn record_check_session(ledger: &Ledger, new: &NewCheckSession) -> Result<String> {
    if new.actor.trim().is_empty() {
        return Err(CoreError::Config(
            crate::voice::association_line("confirm_needs_actor")
                .map(|l| l.plain.to_string())
                .unwrap_or_else(|| "a link decision must name who made it".to_string()),
        ));
    }
    let (record, lines, digest) = write_check_record(ledger, new)?;
    let transaction = ledger.conn().unchecked_transaction()?;
    ledger.conn().execute(
        "INSERT INTO sessions(id, project_id, file_path, started_at, raw_line_count, raw_digest)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            new.session_id,
            new.project_id,
            record,
            new.recorded_at,
            lines as i64,
            digest
        ],
    )?;
    ledger.conn().execute(
        "INSERT INTO change_check_sessions(session_id, binding_id, change_id, ran_in, actor, recorded_at)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            new.session_id,
            new.binding_id,
            new.change_id,
            new.ran_in,
            new.actor,
            new.recorded_at
        ],
    )?;
    let id = decide(
        ledger,
        &new.change_id,
        &new.session_id,
        AssociationProvenance::Confirmed,
        &new.actor,
        "ran this check for this Change",
        &new.recorded_at,
    )?;
    transaction.commit()?;
    Ok(id)
}

/// The folder, beside the store, that holds one record file per check run
/// recorded for a Change (ADV-run-ledger-5).
pub const CHECK_RECORD_DIR: &str = "check-sessions";

/// The one line a check session's record file holds.
#[derive(Serialize)]
struct CheckRecordLine<'a> {
    record: &'static str,
    session_id: &'a str,
    project_id: &'a str,
    binding_id: &'a str,
    change_id: &'a str,
    ran_in: &'a str,
    actor: &'a str,
    recorded_at: &'a str,
}

/// Write one check session's record — once, never replaced — beside the store
/// this ledger is, and answer `(path, raw_line_count, raw_digest)` taken over
/// the file on disk by [`crate::ingest::whole_file_digest`], the function
/// `verify` recomputes it with. A ledger with no folder (in memory) has
/// nowhere to keep one and is refused, loudly.
///
/// Written before the rows: if they are then refused, the file stays and
/// names a session the store does not hold — verify never reads it, and the
/// storage figure counts it. The reverse order could leave a row naming a file
/// that was never written.
fn write_check_record(ledger: &Ledger, new: &NewCheckSession) -> Result<(String, u64, String)> {
    let store = ledger
        .conn()
        .path()
        .filter(|p| !p.is_empty())
        .and_then(|p| Path::new(p).parent())
        .ok_or_else(|| {
            CoreError::Config(
                "a check run's record is kept beside the store, and this store has no folder"
                    .to_string(),
            )
        })?;
    let dir = store.join(CHECK_RECORD_DIR);
    crate::private_store::create_private_dir(&dir).map_err(|e| CoreError::io(&dir, e))?;
    let name: String = new
        .session_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let path = dir.join(format!("{name}.jsonl"));
    let mut line = serde_json::to_string(&CheckRecordLine {
        record: "thruline.check-session.v1",
        session_id: &new.session_id,
        project_id: &new.project_id,
        binding_id: &new.binding_id,
        change_id: &new.change_id,
        ran_in: &new.ran_in,
        actor: &new.actor,
        recorded_at: &new.recorded_at,
    })?;
    line.push('\n');
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    {
        use std::io::Write as _;
        let mut file = options.open(&path).map_err(|e| CoreError::io(&path, e))?;
        file.write_all(line.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|e| CoreError::io(&path, e))?;
    }
    let (lines, digest) = crate::ingest::whole_file_digest(&path)?;
    Ok((path.display().to_string(), lines, digest))
}

/// Every session under one Change that is an execution rather than a
/// transcript, in id order.
///
/// Served so a reader knows not to ask a captured command for a transcript: a
/// check run has no prompts, no tool calls and no story, and a screen that
/// asks for one and prints the refusal has put an engine word in front of a
/// person for no reason. Found on the ordinary path in T-3-2b's own
/// walkthrough, where the Change screen read `refused: invalid_session` under
/// the covered folders.
pub fn execution_sessions(ledger: &Ledger, change_id: &str) -> Result<Vec<String>> {
    let mut statement = ledger.conn().prepare(
        "SELECT c.session_id FROM change_check_sessions c
           JOIN change_session_associations a ON a.session_id = c.session_id
          WHERE a.change_id = ?1 GROUP BY c.session_id ORDER BY c.session_id",
    )?;
    let ids: Vec<String> = statement
        .query_map(params![change_id], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(ids)
}

/// The execution one session is, or `None` when it is a transcript.
pub fn check_session(ledger: &Ledger, session_id: &str) -> Result<Option<CheckSession>> {
    let row = ledger
        .conn()
        .query_row(
            "SELECT session_id, binding_id, change_id, ran_in, actor, recorded_at
               FROM change_check_sessions WHERE session_id = ?1",
            params![session_id],
            |row| {
                Ok(CheckSession {
                    session_id: row.get(0)?,
                    binding_id: row.get(1)?,
                    change_id: row.get(2)?,
                    ran_in: row.get(3)?,
                    actor: row.get(4)?,
                    recorded_at: row.get(5)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

// ---------------------------------------------------------------------------
// The decision
// ---------------------------------------------------------------------------

/// Append a person's decision about one pair.
///
/// `provenance` must be `Confirmed` or `Rejected`; a caller asking to "decide"
/// something as a suggestion is refused here rather than quietly writing a
/// machine row with a person's name on it. The actor is required before the
/// database is touched so the refusal is a sentence rather than a constraint
/// error, and the database's CHECK still stands behind it.
pub fn decide(
    ledger: &Ledger,
    change_id: &str,
    session_id: &str,
    provenance: AssociationProvenance,
    actor: &str,
    reason: &str,
    decided_at: &str,
) -> Result<String> {
    if provenance == AssociationProvenance::Suggested {
        return Err(CoreError::Config(
            "a person's decision is `confirmed` or `rejected`; `suggested` is what the \
             tool proposes, and it is not a decision"
                .to_string(),
        ));
    }
    if actor.trim().is_empty() {
        return Err(CoreError::Config(
            crate::voice::association_line("confirm_needs_actor")
                .map(|l| l.plain.to_string())
                .unwrap_or_else(|| "a link decision must name who made it".to_string()),
        ));
    }
    // A decision's id is a fresh ULID, not a digest of its content.
    //
    // The first version derived it from `(change, session, provenance, actor,
    // time)`, and a hand walkthrough broke it in under a minute: confirm,
    // reject, confirm again inside one second produced the same id twice and
    // the append-only trigger refused it with a raw constraint error. A person
    // changing their mind twice is not a programming error, and a suggestion
    // is the only thing here that should be idempotent — a decision is an act,
    // and two identical acts a second apart are two acts.
    let id = format!("csa_{}", Ulid::generate());
    crate::change::append_association(
        ledger,
        &NewAssociation {
            id: id.clone(),
            change_id: change_id.to_string(),
            session_id: session_id.to_string(),
            provenance,
            basis: AssociationBasis::User,
            actor: Some(actor.to_string()),
            asserted_at: decided_at.to_string(),
            reason: Some(reason.to_string()),
        },
    )?;
    Ok(id)
}

/// The latest thing anybody said about one pair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StandingLink {
    pub id: String,
    pub provenance: AssociationProvenance,
    pub basis: AssociationBasis,
    pub actor: Option<String>,
    pub asserted_at: String,
    pub reason: Option<String>,
}

/// The standing provenance for one pair: **the row appended last**.
///
/// The earlier rows are still there and still readable; this is "what is true
/// now", not "what is left". Free text comes back masked, because this is a
/// public read.
///
/// Ordering is by insertion (`rowid`), not by `asserted_at`, and that is a
/// correction rather than a preference. `asserted_at` is second-granularity,
/// and two decisions about one pair inside one second is not a corner case —
/// it is what a person changing their mind on a screen produces. The first
/// version of this function ordered by `asserted_at DESC, id DESC`, and a
/// hand walkthrough showed a rejection silently losing to the confirmation it
/// answered, because the tie fell to whichever content hash sorted higher.
/// A decision decided by a hash is not a decision. On an append-only table the
/// last row appended IS the last thing anybody said.
pub fn standing_link(
    ledger: &Ledger,
    change_id: &str,
    session_id: &str,
) -> Result<Option<StandingLink>> {
    let mut record = MaskRecord::default();
    standing_link_masked(ledger, change_id, session_id, &mut record)
}

/// The same read, masking into a caller's record so one view can state what
/// the masker did across all of its text at once.
fn standing_link_masked(
    ledger: &Ledger,
    change_id: &str,
    session_id: &str,
    record: &mut MaskRecord,
) -> Result<Option<StandingLink>> {
    /// `(id, provenance, basis, actor, asserted_at, reason)` — the row exactly
    /// as the table spells it, before any of it is parsed or masked.
    type AssociationRow = (
        String,
        String,
        String,
        Option<String>,
        String,
        Option<String>,
    );
    let row: Option<AssociationRow> = ledger
        .conn()
        .query_row(
            "SELECT id, provenance, basis, actor, asserted_at, reason
               FROM change_session_associations
              WHERE change_id = ?1 AND session_id = ?2
              ORDER BY rowid DESC LIMIT 1",
            params![change_id, session_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?;
    let Some((id, provenance, basis, actor, asserted_at, reason)) = row else {
        return Ok(None);
    };
    Ok(Some(StandingLink {
        id,
        provenance: AssociationProvenance::parse(&provenance)?,
        basis: AssociationBasis::parse(&basis)?,
        actor: actor.map(|a| mask_text(&a, record)),
        asserted_at,
        reason: reason.map(|r| mask_text(&r, record)),
    }))
}

// ---------------------------------------------------------------------------
// The gate
// ---------------------------------------------------------------------------

/// Whether a session's recorded outcomes may be read under a Change.
///
/// `Applies` names the scope version it was checked against and the paths that
/// put it inside; `NotApplicable` names one copy-file slug and nothing else.
/// There is no third answer and no "probably".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Applicability {
    Applies {
        scope_version: u32,
        in_scope_paths: Vec<String>,
    },
    /// A session `thruline check` wrote for one captured execution, run for
    /// THIS Change. A check run changed no files, so the changed-path test has
    /// no input; what stands in its place is the Change the person named when
    /// they ran it, and the standing scope they accepted.
    AppliesAsExamination {
        scope_version: u32,
        binding_id: String,
    },
    NotApplicable {
        /// The copy-file slug. The sentence is looked up, never built here.
        reason_slug: &'static str,
        /// The paths that put it outside, when that is the reason. Empty
        /// otherwise; never a summary sentence.
        outside_paths: Vec<String>,
    },
}

impl Applicability {
    pub fn applies(&self) -> bool {
        matches!(
            self,
            Self::Applies { .. } | Self::AppliesAsExamination { .. }
        )
    }

    /// The copy-file slug for whichever answer this is.
    pub fn slug(&self) -> &'static str {
        match self {
            Self::Applies { .. } => "applies_in_scope",
            Self::AppliesAsExamination { .. } => "applies_examination",
            Self::NotApplicable { reason_slug, .. } => reason_slug,
        }
    }
}

/// **The gate.** The only function that may authorize reading a session's
/// outcomes under a Change (`B3.13`).
///
/// Two conditions, both required:
///
/// 1. the standing provenance for the pair is `confirmed` — a suggestion is
///    not a confirmation, and a rejection is not an absence;
/// 2. at least one path the session changed falls inside the accepted scope's
///    allowed paths.
///
/// A Change with no scope at all fails the second condition rather than
/// passing it vacuously, and a session whose changed paths cannot be read from
/// the record fails it too: "we could not check" is not "it is fine".
///
/// If a second function in this workspace ever grows the power to make this
/// decision, `crates/core/tests/change_scope_gate.rs` fails and names it.
///
/// **Not a serving surface.** The paths it returns are as the record holds
/// them. [`change_results`] is the only caller that turns them into an answer,
/// and it masks them there; the census test pins that it is the only one.
pub fn applicability(ledger: &Ledger, change_id: &str, session_id: &str) -> Result<Applicability> {
    let standing = standing_link(ledger, change_id, session_id)?;
    match standing.as_ref().map(|s| s.provenance) {
        None => {
            return Ok(Applicability::NotApplicable {
                reason_slug: "not_applicable_no_link",
                outside_paths: Vec::new(),
            })
        }
        Some(AssociationProvenance::Suggested) => {
            return Ok(Applicability::NotApplicable {
                reason_slug: "not_applicable_only_suggested",
                outside_paths: Vec::new(),
            })
        }
        Some(AssociationProvenance::Rejected) => {
            return Ok(Applicability::NotApplicable {
                reason_slug: "not_applicable_rejected",
                outside_paths: Vec::new(),
            })
        }
        Some(AssociationProvenance::Confirmed) => {}
    }

    let Some((scope_version, allowed)) = standing_allowed_scope(ledger, change_id)? else {
        return Ok(Applicability::NotApplicable {
            reason_slug: "not_applicable_no_scope",
            outside_paths: Vec::new(),
        });
    };
    if allowed.is_empty() {
        return Ok(Applicability::NotApplicable {
            reason_slug: "not_applicable_no_scope",
            outside_paths: Vec::new(),
        });
    }

    // One captured execution, recorded under its own session (T-3-2b). A check
    // run edited nothing, so asking which of the files it changed fall inside
    // the scope has no input and would refuse every execution this product
    // ever records. The question that IS answerable is asked instead, and it
    // is not weaker: which Change did the person who typed the command run it
    // for? A run named for another Change is refused here rather than read
    // across, and one named for this Change still has to pass the
    // examined-inputs predicate item by item before it counts for anything.
    if let Some(check) = check_session(ledger, session_id)? {
        if check.change_id != change_id {
            return Ok(Applicability::NotApplicable {
                reason_slug: "not_applicable_check_for_another_change",
                outside_paths: Vec::new(),
            });
        }
        return Ok(Applicability::AppliesAsExamination {
            scope_version,
            binding_id: check.binding_id,
        });
    }

    let paths = session_changed_paths_for_change(ledger, change_id, session_id)?;
    if paths.is_empty() {
        return Ok(Applicability::NotApplicable {
            reason_slug: "not_applicable_no_changed_paths",
            outside_paths: Vec::new(),
        });
    }
    let split = split_by_scope(&paths, &allowed);
    if split.inside.is_empty() {
        return Ok(Applicability::NotApplicable {
            reason_slug: "not_applicable_outside_scope",
            outside_paths: split.outside,
        });
    }
    Ok(Applicability::Applies {
        scope_version,
        in_scope_paths: split.inside,
    })
}

// ---------------------------------------------------------------------------
// The one place results are served under a Change
// ---------------------------------------------------------------------------

/// One session under a Change: either its recorded outcomes, or the reason
/// they are not being shown here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionUnderChange {
    pub session_id: String,
    pub applicability: Applicability,
    /// The copy-file slug for the sentence to show.
    pub sentence_slug: &'static str,
    /// Counts of this session's recorded verdicts by their stored word.
    ///
    /// **Empty unless the gate said `Applies`.** Every number here is a
    /// `COUNT(*)` over `claim_verdicts` rows keyed by the stored `verdict`
    /// column; none is computed, estimated or carried from anywhere else.
    pub verdict_counts: BTreeMap<String, u64>,
}

/// Every session anybody has linked to this Change, each with its verdict or
/// its refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangeResultsView {
    pub schema_version: u32,
    pub change_id: String,
    pub sessions: Vec<SessionUnderChange>,
    pub masking: crate::redact::Treatment,
}

/// Every session anybody has said anything about for one Change, in id order.
///
/// The one reading of "which sessions are named here", so a second caller cannot
/// grow a second idea of it. T-2-3's proposer needs the same list and asks for it
/// through this function rather than writing the query again — which is what
/// `the_association_table_is_read_in_exactly_two_modules` exists to prevent, and
/// what it caught.
///
/// Says nothing about provenance: a rejected pair is in this list. The caller
/// asks [`standing_link`] what is true now.
pub fn linked_sessions(ledger: &Ledger, change_id: &str) -> Result<Vec<String>> {
    let mut statement = ledger.conn().prepare(
        "SELECT DISTINCT session_id FROM change_session_associations
          WHERE change_id = ?1 ORDER BY session_id",
    )?;
    let ids: Vec<String> = statement
        .query_map(params![change_id], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(ids)
}

/// Serve the outcomes of the sessions linked to one Change.
///
/// Sessions that fail the gate are **listed**, not hidden: a refusal Sam cannot
/// see is a refusal Sam will not believe. What they do not carry is a number.
pub fn change_results(ledger: &Ledger, change_id: &str) -> Result<ChangeResultsView> {
    let session_ids = linked_sessions(ledger, change_id)?;

    let mut record = MaskRecord::default();
    let mut sessions = Vec::new();
    for session_id in session_ids {
        let mut verdict = applicability(ledger, change_id, &session_id)?;
        if let Applicability::Applies { in_scope_paths, .. } = &mut verdict {
            *in_scope_paths = mask_list(std::mem::take(in_scope_paths), &mut record);
        }
        if let Applicability::NotApplicable { outside_paths, .. } = &mut verdict {
            *outside_paths = mask_list(std::mem::take(outside_paths), &mut record);
        }
        let counts = if verdict.applies() {
            read_verdict_counts(ledger, &session_id)?
        } else {
            BTreeMap::new()
        };
        sessions.push(SessionUnderChange {
            session_id,
            sentence_slug: verdict.slug(),
            applicability: verdict,
            verdict_counts: counts,
        });
    }
    Ok(ChangeResultsView {
        schema_version: LINK_VIEW_SCHEMA_VERSION,
        change_id: change_id.to_string(),
        sessions,
        masking: record.to_treatment(),
    })
}

/// Counts from the typed `verdict` column. No free text is read, so no free
/// text can become a number.
fn read_verdict_counts(ledger: &Ledger, session_id: &str) -> Result<BTreeMap<String, u64>> {
    let mut statement = ledger.conn().prepare(
        "SELECT verdict, COUNT(*) FROM claim_verdicts WHERE session_id = ?1
          GROUP BY verdict ORDER BY verdict",
    )?;
    let rows = statement.query_map(params![session_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut out = BTreeMap::new();
    for row in rows {
        let (verdict, count) = row?;
        out.insert(verdict, count.max(0) as u64);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// A session's own read-only view of its links (B3.16)
// ---------------------------------------------------------------------------

/// One Change a session is linked to, from the session's side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionChangeLink {
    pub change_id: String,
    pub change_title: String,
    pub provenance: AssociationProvenance,
    pub basis: AssociationBasis,
    pub actor: Option<String>,
    pub asserted_at: String,
    pub reason: Option<String>,
    /// The copy-file slug for the provenance word.
    pub label_slug: &'static str,
}

/// A session's links, or the fact that it has none.
///
/// This is the **one field** T-2-2 adds to a session's read-only view. Nothing
/// else about a session changes shape, and an unassociated session answers
/// here with an empty list and the `link_none` sentence — which is what
/// `B3.16` asks for: old work stays discoverable and nothing is invented for
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionLinksView {
    pub schema_version: u32,
    pub session_id: String,
    pub changes: Vec<SessionChangeLink>,
    /// The copy-file slug when there are none, `None` otherwise.
    pub none_slug: Option<&'static str>,
    pub masking: crate::redact::Treatment,
}

/// Read one session's links. Read-only; writes nothing, creates nothing.
pub fn session_links(ledger: &Ledger, session_id: &str) -> Result<SessionLinksView> {
    let mut statement = ledger.conn().prepare(
        "SELECT DISTINCT a.change_id, c.title FROM change_session_associations a
           JOIN changes c ON c.id = a.change_id
          WHERE a.session_id = ?1 ORDER BY a.change_id",
    )?;
    let rows: Vec<(String, String)> = statement
        .query_map(params![session_id], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut record = MaskRecord::default();
    let mut changes = Vec::new();
    for (change_id, title) in rows {
        let Some(standing) = standing_link_masked(ledger, &change_id, session_id, &mut record)?
        else {
            continue;
        };
        changes.push(SessionChangeLink {
            change_id,
            change_title: mask_text(&title, &mut record),
            provenance: standing.provenance,
            basis: standing.basis,
            actor: standing.actor,
            asserted_at: standing.asserted_at,
            reason: standing.reason,
            label_slug: standing.provenance.label_slug(),
        });
    }
    let none_slug = if changes.is_empty() {
        Some("link_none")
    } else {
        None
    };
    Ok(SessionLinksView {
        schema_version: LINK_VIEW_SCHEMA_VERSION,
        session_id: session_id.to_string(),
        changes,
        none_slug,
        masking: record.to_treatment(),
    })
}
