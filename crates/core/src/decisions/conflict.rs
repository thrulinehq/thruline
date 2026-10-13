//! Exact-version, advisory conflicts between person-accepted decisions.
use super::{
    checks::{check_at, CheckOutcome, VerdictState},
    params::{parse_params, CheckParams},
    storage,
};
use crate::{chain::sha256_hex, error::Result, ledger::Ledger, redact::Masked};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::{
    collections::BTreeMap,
    process::Command,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictState {
    Conflict,
    Superseded,
    Unverifiable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityWitness {
    pub acceptance_id: String,
    pub event_id: String,
    pub raw_file: String,
    pub raw_line: u64,
    pub raw_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionConflict {
    pub left_decision_id: String,
    pub left_revision: u32,
    pub right_decision_id: String,
    pub right_revision: u32,
    pub left_authority: Option<AuthorityWitness>,
    pub right_authority: Option<AuthorityWitness>,
    pub state: ConflictState,
    pub reason: String,
    pub checkable_if: Option<String>,
    pub path: Option<String>,
    pub proposed_superseded_by: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictReport {
    pub schema_version: u32,
    pub project_id: String,
    pub version_id: String,
    pub tree_hash: Option<String>,
    pub conflicts: Vec<DecisionConflict>,
    pub missing_evidence: Vec<String>,
}

pub fn project_conflicts(
    ledger: &Ledger,
    project_id: &str,
    repo: &Path,
    version_id: &str,
) -> Result<ConflictReport> {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut report = ConflictReport {
        schema_version: 1,
        project_id: project_id.into(),
        version_id: version_id.into(),
        tree_hash: None,
        conflicts: vec![],
        missing_evidence: vec![],
    };
    let root: Option<String> = ledger
        .conn()
        .query_row(
            "SELECT root_path FROM projects WHERE id=?1",
            [project_id],
            |r| r.get(0),
        )
        .optional()?;
    let root_matches = root
        .as_deref()
        .and_then(|p| Path::new(p).canonicalize().ok())
        .zip(repo.canonicalize().ok())
        .is_some_and(|(a, b)| a == b);
    if root_matches {
        report.tree_hash = exact_version(repo, version_id, deadline);
    }
    if report.tree_hash.is_none() {
        report
            .missing_evidence
            .push("exact project repository and version are not verified".into());
    }
    let mut statement = ledger
        .conn()
        .prepare("SELECT id FROM decisions WHERE project_id=?1 ORDER BY id LIMIT 257")?;
    let ids = statement
        .query_map([project_id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if ids.len() > 256 {
        report
            .missing_evidence
            .push("decision pair census exceeds 256 decisions".into());
    }
    let mut candidates = vec![];
    for id in ids.into_iter().take(256) {
        if Instant::now() >= deadline {
            report
                .missing_evidence
                .push("conflict source budget exhausted".into());
            break;
        }
        if let Some(accepted) = storage::verified_acceptance_for_decision_read_only(
            ledger,
            &id,
            deadline.saturating_duration_since(Instant::now()),
        )? {
            let raw: String = ledger.conn().query_row(
                "SELECT params_json FROM decision_revisions WHERE decision_id=?1 AND rev=?2",
                params![id, accepted.revision],
                |r| r.get(0),
            )?;
            let check = parse_params(&accepted.kind, accepted.params_schema_version, &raw)?;
            let witness = authority_witness(ledger, &id, accepted.revision, deadline)?;
            let retired = storage::verified_retirement_for_decision_read_only(
                ledger,
                &id,
                deadline.saturating_duration_since(Instant::now()),
            )?;
            candidates.push((accepted, check, witness, retired));
        } else {
            let attempted: bool = ledger.conn().query_row(
                "SELECT EXISTS(SELECT 1 FROM decision_acceptances WHERE decision_id=?1)",
                [&id],
                |r| r.get(0),
            )?;
            if attempted {
                report.missing_evidence.push(format!(
                    "person acceptance evidence is unavailable for {id}"
                ));
            }
        }
    }
    let mut cache = BTreeMap::<String, CheckOutcome>::new();
    for left in 0..candidates.len() {
        for right in left + 1..candidates.len() {
            let (a, ap, aw, ar) = &candidates[left];
            let (b, bp, bw, br) = &candidates[right];
            let superseded = ar.as_ref().and_then(|r| r.superseded_by.as_deref())
                == Some(b.id.as_str())
                || br.as_ref().and_then(|r| r.superseded_by.as_deref()) == Some(a.id.as_str());
            if (ar.is_some() || br.is_some()) && !superseded {
                continue;
            }
            let proof = opposite(repo, version_id, &a.id, ap, &b.id, bp, &mut cache, deadline);
            let Some((path, mut reason, mut state)) = proof else {
                continue;
            };
            let complete = report.tree_hash.is_some()
                && aw.is_some()
                && bw.is_some()
                && Instant::now() < deadline;
            if !complete {
                state = ConflictState::Unverifiable;
                reason = "exact version or person source evidence is unavailable".into();
            } else if !ancestor(repo, &a.applies_from_version, version_id, deadline)
                || !ancestor(repo, &b.applies_from_version, version_id, deadline)
            {
                // A proved earlier version is outside a decision's applicability.
                if exact_version(repo, &a.applies_from_version, deadline).is_some()
                    && exact_version(repo, &b.applies_from_version, deadline).is_some()
                {
                    continue;
                }
                state = ConflictState::Unverifiable;
                reason = "accepted start-version ancestry is unavailable".into();
            } else if superseded {
                state = ConflictState::Superseded;
                reason =
                    "an exact verified person act supersedes this predecessor; both rows remain"
                        .into();
            }
            let proposal = if state == ConflictState::Conflict {
                let ao = storage::acceptance_order(ledger, &a.id, a.revision, Some(deadline))?;
                let bo = storage::acceptance_order(ledger, &b.id, b.revision, Some(deadline))?;
                match (ao, bo) {
                    (Some(x), Some(y)) if x < y => Some(b.id.clone()),
                    (Some(x), Some(y)) if y < x => Some(a.id.clone()),
                    _ => None,
                }
            } else {
                None
            };
            report.conflicts.push(DecisionConflict {
                left_decision_id: a.id.clone(),
                left_revision: a.revision,
                right_decision_id: b.id.clone(),
                right_revision: b.revision,
                left_authority: aw.clone(),
                right_authority: bw.clone(),
                checkable_if: (state == ConflictState::Unverifiable)
                    .then(|| "restore exact Git objects and verified person receipts".into()),
                state,
                reason: Masked::mask(&reason).into_text(),
                path: path.map(|p| Masked::mask(&p).into_text()),
                proposed_superseded_by: proposal,
            });
        }
    }
    Ok(report)
}

fn authority_witness(
    ledger: &Ledger,
    id: &str,
    revision: u32,
    deadline: Instant,
) -> Result<Option<AuthorityWitness>> {
    let Some(order) = storage::acceptance_order(ledger, id, revision, Some(deadline))? else {
        return Ok(None);
    };
    let row:Option<(String,String,String,i64,String)>=ledger.conn().query_row("SELECT a.id,p.event_id,p.raw_file,p.raw_line,p.raw_sha256 FROM decision_acceptances a JOIN decision_person_acts p ON p.action='accept' AND p.act_row_id=a.id WHERE a.rowid=?1",[order],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
    Ok(
        row.and_then(|(acceptance_id, event_id, raw_file, line, raw_sha256)| {
            u64::try_from(line).ok().map(|raw_line| AuthorityWitness {
                acceptance_id,
                event_id,
                raw_file: Masked::mask(&raw_file).into_text(),
                raw_line,
                raw_sha256,
            })
        }),
    )
}

#[allow(clippy::too_many_arguments)]
fn opposite(
    repo: &Path,
    version: &str,
    aid: &str,
    a: &CheckParams,
    bid: &str,
    b: &CheckParams,
    cache: &mut BTreeMap<String, CheckOutcome>,
    deadline: Instant,
) -> Option<(Option<String>, String, ConflictState)> {
    match (a, b) {
        (CheckParams::FeatureDefaultPreserved(a), CheckParams::FeatureDefaultPreserved(b))
            if a.path == b.path && a.symbol == b.symbol && a.expected_value != b.expected_value =>
        {
            Some((
                Some(a.path.clone()),
                "accepted defaults require different values for the same symbol".into(),
                ConflictState::Conflict,
            ))
        }
        (CheckParams::PathPresent(a), CheckParams::PathPresent(b))
            if a.path == b.path
                && a.sha256.is_some()
                && b.sha256.is_some()
                && a.sha256 != b.sha256 =>
        {
            Some((
                Some(a.path.clone()),
                "accepted file predicates require different full hashes".into(),
                ConflictState::Conflict,
            ))
        }
        (CheckParams::TextAbsent(_), CheckParams::PathPresent(p))
        | (CheckParams::PathPresent(p), CheckParams::TextAbsent(_))
            if p.sha256.is_some() =>
        {
            let (id, absent) = if matches!(a, CheckParams::TextAbsent(_)) {
                (aid, a)
            } else {
                (bid, b)
            };
            let Some(bytes) = blob_at(repo, version, &p.path, deadline) else {
                return Some((
                    Some(p.path.clone()),
                    "required exact blob cannot be read".into(),
                    ConflictState::Unverifiable,
                ));
            };
            if p.sha256.as_deref() != Some(sha256_hex(&bytes).as_str()) {
                return Some((
                    Some(p.path.clone()),
                    "required pinned blob is unavailable on this version".into(),
                    ConflictState::Unverifiable,
                ));
            }
            let outcome = cache
                .entry(id.into())
                .or_insert_with(|| check_at(repo, version, absent));
            if outcome.reason.is_some() || outcome.state == VerdictState::Unverifiable {
                return Some((
                    Some(p.path.clone()),
                    "absence scope could not be fully checked".into(),
                    ConflictState::Unverifiable,
                ));
            }
            outcome.findings.iter().any(|f| f.path == p.path).then(|| {
                (
                    Some(p.path.clone()),
                    "required pinned file contains a forbidden unwaived span".into(),
                    ConflictState::Conflict,
                )
            })
        }
        _ => None,
    }
}

pub(crate) fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64)
        && oid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn git_output(repo: &Path, args: &[&str], deadline: Instant) -> Option<Vec<u8>> {
    let mut command = Command::new("git");
    crate::git_isolation::isolate_config(&mut command)
        .args(args)
        .current_dir(repo);
    match crate::bounded_child::output_until(&mut command, deadline).ok()? {
        crate::bounded_child::Outcome::Finished(out)
            if out.status.success() && out.stdout.len() <= 2 * 1024 * 1024 =>
        {
            Some(out.stdout)
        }
        _ => None,
    }
}
pub(crate) fn exact_version(repo: &Path, version: &str, deadline: Instant) -> Option<String> {
    if !valid_oid(version) {
        return None;
    }
    let commit = git_output(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{version}^{{commit}}"),
        ],
        deadline,
    )?;
    if std::str::from_utf8(&commit).ok()?.trim() != version {
        return None;
    }
    let tree = git_output(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{version}^{{tree}}"),
        ],
        deadline,
    )?;
    let tree = std::str::from_utf8(&tree).ok()?.trim();
    valid_oid(tree).then(|| tree.into())
}
pub(crate) fn ancestor(repo: &Path, before: &str, after: &str, deadline: Instant) -> bool {
    valid_oid(before)
        && valid_oid(after)
        && git_output(
            repo,
            &["merge-base", "--is-ancestor", before, after],
            deadline,
        )
        .is_some()
}
pub(crate) fn blob_at(
    repo: &Path,
    version: &str,
    path: &str,
    deadline: Instant,
) -> Option<Vec<u8>> {
    if !valid_oid(version)
        || path.is_empty()
        || Path::new(path).is_absolute()
        || path.chars().any(char::is_control)
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return None;
    }
    let entry = git_output(
        repo,
        &["ls-tree", "-z", version, "--", &format!(":(literal){path}")],
        deadline,
    )?;
    let row = std::str::from_utf8(&entry).ok()?.strip_suffix('\0')?;
    let (meta, name) = row.split_once('\t')?;
    let fields = meta.split_whitespace().collect::<Vec<_>>();
    if name != path
        || fields.len() != 3
        || !matches!(fields[0], "100644" | "100755")
        || fields[1] != "blob"
        || !valid_oid(fields[2])
    {
        return None;
    }
    let size = git_output(repo, &["cat-file", "-s", fields[2]], deadline)?;
    if std::str::from_utf8(&size)
        .ok()?
        .trim()
        .parse::<usize>()
        .ok()?
        > 2 * 1024 * 1024
    {
        return None;
    }
    git_output(repo, &["cat-file", "blob", fields[2]], deadline)
}
