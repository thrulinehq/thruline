//! Deterministic checks against a named project version.
//!
//! This module reads exact Git objects through fixed, read-only Git commands.
//! An unreadable tree or unsupported term yields `Unverifiable`, never a hold.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::redact::Masked;

use super::checks_visual::VisualScreenResult;
use super::params::{CheckParams, Surface, TextAbsentParams};
use super::{checks_presence, checks_terms, checks_tests};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictState {
    Holds,
    Broken,
    Unverifiable,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub surface: Surface,
    pub path: String,
    pub line: u64,
    pub span: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SurfaceResult {
    pub surface: Surface,
    pub checked_places: u64,
    pub finding_count: u64,
    pub state: VerdictState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckOutcome {
    pub state: VerdictState,
    pub tree_hash: Option<String>,
    pub reason: Option<String>,
    pub checked_places: u64,
    pub findings: Vec<Finding>,
    pub surfaces: Vec<SurfaceResult>,
    #[serde(default)]
    pub visual: Vec<VisualScreenResult>,
    #[serde(default)]
    pub not_looked_at: Vec<super::text_exclusions::NotLookedAt>,
}

impl CheckOutcome {
    pub(crate) fn unavailable(reason: &str, tree_hash: Option<String>) -> Self {
        Self {
            state: VerdictState::Unverifiable,
            tree_hash,
            reason: Some(reason.to_owned()),
            checked_places: 0,
            findings: Vec::new(),
            surfaces: Vec::new(),
            not_looked_at: Vec::new(),
            visual: Vec::new(),
        }
    }
}

pub fn check_at(repo: &Path, version: &str, params: &CheckParams) -> CheckOutcome {
    match params {
        CheckParams::TextAbsent(p) => check_text_absent_at(repo, version, p),
        CheckParams::PathPresent(p) => checks_presence::check_path_present(repo, version, p),
        CheckParams::FeatureDefaultPreserved(p) => {
            checks_presence::check_feature_default_preserved(repo, version, p)
        }
        CheckParams::TestPresentAndRuns(p) => {
            checks_tests::check_test_present_and_runs(repo, version, p, None)
        }
        CheckParams::SymbolAbsent(p) => checks_terms::check_symbol_absent(repo, version, p),
        CheckParams::UiCopyAbsent(p) => checks_terms::check_ui_copy_absent(repo, version, p),
        CheckParams::TermEverywhere(p) => checks_terms::check_term_everywhere(repo, version, p),
        CheckParams::ForbiddenWords(p) => checks_terms::check_forbidden_words(repo, version, p),
        _ => CheckOutcome::unavailable("check kind is not implemented", None),
    }
}

pub fn check_text_absent_at(repo: &Path, version: &str, params: &TextAbsentParams) -> CheckOutcome {
    let terms = params
        .terms
        .iter()
        .chain(params.synonyms.iter())
        .collect::<Vec<_>>();
    if terms.is_empty() || terms.iter().any(|t| t.is_empty() || !t.is_ascii()) {
        return CheckOutcome::unavailable("check requires nonempty ASCII terms", None);
    }
    let Some(commit_oid) = git_oid(repo, &format!("{version}^{{commit}}")) else {
        return CheckOutcome::unavailable("version cannot be resolved to a commit", None);
    };
    let Some(tree_hash) = git_oid(repo, &format!("{commit_oid}^{{tree}}")) else {
        return CheckOutcome::unavailable("version tree cannot be resolved", None);
    };
    let ignore = match ignore_patterns(repo, &commit_oid) {
        Ok(patterns) => patterns,
        Err(reason) => return CheckOutcome::unavailable(reason, Some(tree_hash)),
    };
    let mut found = BTreeMap::<(String, u64, usize, usize), Finding>::new();
    let paths = match tracked_paths(repo, &commit_oid) {
        Ok(paths) => paths,
        Err(reason) => return CheckOutcome::unavailable(reason, Some(tree_hash)),
    };
    let span_deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut scope_missing = Vec::new();
    let test_spans =
        match super::text_exclusions::test_spans(repo, &commit_oid, &params.test_allowlist) {
            Ok(spans) => spans,
            Err(reason) if !params.span_allowlist.is_empty() => {
                scope_missing.push(format!("Accepted test exclusions not applied: {reason}"));
                Vec::new()
            }
            Err(reason) => return CheckOutcome::unavailable(reason, Some(tree_hash)),
        };
    let json_spans = match super::c31_json_field::json_field_spans_at(
        repo,
        &commit_oid,
        &params.json_field_allowlist,
    ) {
        Ok(spans) => spans,
        Err(reason) => return CheckOutcome::unavailable(reason, Some(tree_hash)),
    };
    let archive_resolution = super::c31_archive::resolve_archive_absences(
        repo,
        &commit_oid,
        &params.conditional_path_allowlist,
    );
    let span_resolution = super::c32_span_scope::resolve_spans_at(
        repo,
        &commit_oid,
        &params.span_allowlist,
        &params.test_allowlist,
        &test_spans,
        span_deadline,
    );
    scope_missing.extend(span_resolution.missing.iter().cloned());
    scope_missing.extend(archive_resolution.missing.iter().cloned());
    let mut allowlist = params.allowlist.clone();
    allowlist.extend(archive_resolution.patterns.iter().cloned());
    let mut not_looked_at = test_spans
        .iter()
        .map(|span| span.receipt.clone())
        .collect::<Vec<_>>();
    not_looked_at.extend(
        json_spans
            .iter()
            .map(|span| super::text_exclusions::NotLookedAt {
                path: Masked::mask(&span.path).into_text(),
                source: "accepted_json_key_scope".into(),
                pattern: Masked::mask(&format!("{}#{} ({})", span.path, span.pointer, span.term))
                    .into_text(),
                test_name: None,
                start_line: Some(span.line),
                end_line: Some(span.line),
                start_column: Some(span.start_column),
                end_column: Some(span.end_column),
            }),
    );
    not_looked_at.extend(
        span_resolution
            .spans
            .iter()
            .map(|span| span.receipt.clone()),
    );
    let mut checked_paths = BTreeMap::<String, Surface>::new();
    for path in paths {
        for (source, patterns) in [
            ("accepted_allowlist", &params.allowlist),
            ("accepted_conditional_archive", &archive_resolution.patterns),
            ("recorded_ignore", &ignore),
        ] {
            for pattern in patterns {
                if glob_path(pattern, &path) {
                    not_looked_at.push(super::text_exclusions::NotLookedAt::path(
                        &path, source, pattern,
                    ));
                }
            }
        }
        if ignored(&path, &allowlist, &ignore) {
            continue;
        }
        let surface = surface_of(&path);
        if !params.surfaces.contains(&surface) {
            continue;
        }
        checked_paths.insert(path.clone(), surface);
        for term in &terms {
            for (start, end) in term_spans(&path, term) {
                if span_resolution
                    .spans
                    .iter()
                    .any(|span| span.contains(&path, 0, start, end))
                {
                    continue;
                }
                let Some(span) = path.get(start..end) else {
                    continue;
                };
                found
                    .entry((path.clone(), 0, start, end))
                    .or_insert_with(|| Finding {
                        surface,
                        path: Masked::mask(&path).into_text(),
                        line: 0,
                        span: Masked::mask(span).into_text(),
                    });
            }
        }
    }
    let line_counts = match tracked_text_line_counts(repo, &commit_oid) {
        Ok(counts) => counts,
        Err(reason) => return CheckOutcome::unavailable(reason, Some(tree_hash)),
    };
    let mut checked_by_surface = BTreeMap::<&'static str, u64>::new();
    for (path, surface) in &checked_paths {
        let skipped = test_spans
            .iter()
            .filter(|span| span.path == *path)
            .flat_map(|span| span.excluded_lines.iter().copied())
            .chain(
                span_resolution
                    .spans
                    .iter()
                    .filter(|span| span.path == *path)
                    .flat_map(|span| span.excluded_lines.iter().copied()),
            )
            .collect::<BTreeSet<_>>()
            .len() as u64;
        let lines = line_counts
            .get(path)
            .copied()
            .unwrap_or(0)
            .saturating_sub(skipped);
        let pathname_places = u64::from(
            !span_resolution
                .spans
                .iter()
                .any(|span| span.path == *path && span.pathname()),
        );
        let Some(places) = lines.checked_add(pathname_places) else {
            return CheckOutcome::unavailable("source census is too large", Some(tree_hash));
        };
        let count = checked_by_surface.entry(surface.as_str()).or_default();
        let Some(next) = count.checked_add(places) else {
            return CheckOutcome::unavailable("source census is too large", Some(tree_hash));
        };
        *count = next;
    }
    let Some(checked_places) = checked_by_surface
        .values()
        .try_fold(0u64, |sum, count| sum.checked_add(*count))
    else {
        return CheckOutcome::unavailable("source census is too large", Some(tree_hash));
    };
    for term in terms {
        let output = match crate::git_isolation::isolate_config(&mut Command::new("git"))
            .args(["grep", "--no-color", "-z", "-n", "-I", "-i", "-F", "-e"])
            .arg(term)
            .arg(&commit_oid)
            .arg("--")
            .current_dir(repo)
            .output()
        {
            Ok(output) => output,
            Err(_) => {
                return CheckOutcome::unavailable("source census could not run", Some(tree_hash));
            }
        };
        match output.status.code() {
            Some(1) => continue,
            Some(0) => {}
            _ => {
                return CheckOutcome::unavailable("source census failed", Some(tree_hash));
            }
        }
        let lines = match grep_lines(&output.stdout, &commit_oid) {
            Some(lines) => lines,
            None => {
                return CheckOutcome::unavailable(
                    "source census output could not be read",
                    Some(tree_hash),
                );
            }
        };
        for (path, line_number, line) in lines {
            if ignored(&path, &allowlist, &ignore) {
                continue;
            }
            let surface = surface_of(&path);
            if !params.surfaces.contains(&surface) {
                continue;
            }
            for (start, end) in term_spans(&line, term) {
                if test_spans
                    .iter()
                    .any(|span| span.contains(&path, line_number, start, end))
                {
                    continue;
                }
                if span_resolution
                    .spans
                    .iter()
                    .any(|span| span.contains(&path, line_number, start, end))
                {
                    continue;
                }
                if json_spans
                    .iter()
                    .any(|span| span.matches(&path, line_number, start, end, term))
                {
                    continue;
                }
                let Some(span) = line.get(start..end) else {
                    continue;
                };
                found
                    .entry((path.clone(), line_number, start, end))
                    .or_insert_with(|| Finding {
                        surface,
                        path: Masked::mask(&path).into_text(),
                        line: line_number,
                        span: Masked::mask(span).into_text(),
                    });
            }
        }
    }
    let findings: Vec<Finding> = found.into_values().collect();
    let mut surfaces = params.surfaces.clone();
    surfaces.sort_by_key(|surface| surface.as_str());
    surfaces.dedup();
    let surfaces = surfaces
        .into_iter()
        .map(|surface| {
            let finding_count = findings.iter().filter(|f| f.surface == surface).count() as u64;
            SurfaceResult {
                surface,
                checked_places: checked_by_surface
                    .get(surface.as_str())
                    .copied()
                    .unwrap_or(0),
                finding_count,
                state: if finding_count == 0 && !scope_missing.is_empty() {
                    VerdictState::Unverifiable
                } else if finding_count == 0 {
                    VerdictState::Holds
                } else {
                    VerdictState::Broken
                },
            }
        })
        .collect();
    not_looked_at.sort();
    not_looked_at.dedup();
    CheckOutcome {
        state: if findings.is_empty() && !scope_missing.is_empty() {
            VerdictState::Unverifiable
        } else if findings.is_empty() {
            VerdictState::Holds
        } else {
            VerdictState::Broken
        },
        tree_hash: Some(tree_hash),
        reason: (!scope_missing.is_empty()).then(|| {
            format!(
                "Conditional exclusions not applied: {}",
                scope_missing.join("; ")
            )
        }),
        checked_places,
        findings,
        surfaces,
        not_looked_at,
        visual: Vec::new(),
    }
}

/// Count the text lines Git actually searches with `-I`, by tracked path.
/// `^` matches every line once, including blank lines. Path names themselves
/// are counted separately so an empty or binary tracked file is still checked
/// for a forbidden term in its name.
fn tracked_text_line_counts(
    repo: &Path,
    commit_oid: &str,
) -> std::result::Result<BTreeMap<String, u64>, &'static str> {
    let output = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args([
            "grep",
            "--no-color",
            "-c",
            "-z",
            "-I",
            "-e",
            "^",
            commit_oid,
            "--",
        ])
        .current_dir(repo)
        .output()
        .map_err(|_| "tracked line census could not run")?;
    match output.status.code() {
        Some(1) => return Ok(BTreeMap::new()),
        Some(0) => {}
        _ => return Err("tracked line census failed"),
    }
    let prefix = format!("{commit_oid}:");
    let mut counts = BTreeMap::new();
    let mut cursor = 0usize;
    while cursor < output.stdout.len() {
        let path_end = cursor
            + output.stdout[cursor..]
                .iter()
                .position(|byte| *byte == 0)
                .ok_or("tracked line census has an incomplete path")?;
        let header = std::str::from_utf8(&output.stdout[cursor..path_end])
            .map_err(|_| "tracked line census path is not UTF-8")?;
        let path = header
            .strip_prefix(&prefix)
            .ok_or("tracked line census came from another version")?;
        cursor = path_end + 1;
        let count_end = cursor
            + output.stdout[cursor..]
                .iter()
                .position(|byte| *byte == b'\n')
                .ok_or("tracked line census has an incomplete count")?;
        let count = std::str::from_utf8(&output.stdout[cursor..count_end])
            .map_err(|_| "tracked line census count is not text")?
            .parse::<u64>()
            .map_err(|_| "tracked line census count is invalid")?;
        if counts.insert(path.to_owned(), count).is_some() {
            return Err("tracked line census repeats a path");
        }
        cursor = count_end + 1;
    }
    Ok(counts)
}

fn tracked_paths(repo: &Path, commit_oid: &str) -> std::result::Result<Vec<String>, &'static str> {
    let output = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["ls-tree", "-r", "-z", "--name-only", commit_oid])
        .current_dir(repo)
        .output()
        .map_err(|_| "tracked path census could not run")?;
    if !output.status.success() {
        return Err("tracked path census failed");
    }
    let mut paths = Vec::new();
    for bytes in output.stdout.split(|byte| *byte == 0) {
        if bytes.is_empty() {
            continue;
        }
        let path = std::str::from_utf8(bytes).map_err(|_| "tracked path is not UTF-8")?;
        paths.push(path.to_owned());
    }
    Ok(paths)
}

/// Recheck an absence decision across two exact commits. When the accepted
/// term disappeared in a changed line, a new phrase in that line is a
/// candidate for person review, never an automatically accepted synonym.
pub fn check_text_absent_transition(
    repo: &Path,
    before_version: &str,
    after_version: &str,
    params: &TextAbsentParams,
) -> CheckOutcome {
    let mut outcome = check_text_absent_at(repo, after_version, params);
    if outcome.state != VerdictState::Holds {
        return outcome;
    }
    let Some(before) = git_oid(repo, &format!("{before_version}^{{commit}}")) else {
        return CheckOutcome::unavailable("baseline version cannot be resolved", outcome.tree_hash);
    };
    let Some(after) = git_oid(repo, &format!("{after_version}^{{commit}}")) else {
        return CheckOutcome::unavailable("target version cannot be resolved", outcome.tree_hash);
    };
    let output = match crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["diff", "--no-ext-diff", "--no-color", "--unified=0"])
        .args([&before, &after, "--"])
        .current_dir(repo)
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => {
            return CheckOutcome::unavailable("version delta could not be read", outcome.tree_hash)
        }
    };
    let Some(diff) = std::str::from_utf8(&output.stdout).ok() else {
        return CheckOutcome::unavailable("version delta is not UTF-8", outcome.tree_hash);
    };
    let mut candidates = std::collections::BTreeSet::<String>::new();
    let mut removed = Vec::<&str>::new();
    let mut added = Vec::<&str>::new();
    for line in diff.lines().chain(std::iter::once("@@ end")) {
        if line.starts_with("@@") || line.starts_with("diff --git ") {
            collect_rename_candidates(&removed, &added, params, &mut candidates);
            removed.clear();
            added.clear();
        } else if let Some(text) = line.strip_prefix('-') {
            if !line.starts_with("---") {
                removed.push(text);
            }
        } else if let Some(text) = line.strip_prefix('+') {
            if !line.starts_with("+++") {
                added.push(text);
            }
        }
    }
    if !candidates.is_empty() {
        let joined = candidates.into_iter().collect::<Vec<_>>().join(", ");
        outcome.state = VerdictState::Unverifiable;
        outcome.reason = Some(format!(
            "term may have been renamed — confirm or dismiss: {}",
            Masked::mask(&joined).into_text()
        ));
        for surface in &mut outcome.surfaces {
            surface.state = VerdictState::Unverifiable;
        }
    }
    outcome
}

fn collect_rename_candidates(
    removed: &[&str],
    added: &[&str],
    params: &TextAbsentParams,
    candidates: &mut std::collections::BTreeSet<String>,
) {
    for old in removed {
        if !params
            .terms
            .iter()
            .chain(params.synonyms.iter())
            .any(|term| !term_spans(old, term).is_empty())
        {
            continue;
        }
        for new in added {
            if let Some(candidate) = replacement_phrase(old, new) {
                if !params
                    .terms
                    .iter()
                    .chain(params.synonyms.iter())
                    .any(|term| candidate.eq_ignore_ascii_case(term))
                {
                    candidates.insert(candidate);
                }
            }
        }
    }
}

fn replacement_phrase(old: &str, new: &str) -> Option<String> {
    let mut prefix = 0;
    for (a, b) in old.chars().zip(new.chars()) {
        if a != b {
            break;
        }
        prefix += a.len_utf8();
    }
    let old_tail = old.get(prefix..)?;
    let new_tail = new.get(prefix..)?;
    let mut old_suffix = 0;
    let mut new_suffix = 0;
    for (a, b) in old_tail.chars().rev().zip(new_tail.chars().rev()) {
        if a != b {
            break;
        }
        old_suffix += a.len_utf8();
        new_suffix += b.len_utf8();
    }
    let end = new.len().checked_sub(new_suffix)?;
    let phrase = new
        .get(prefix..end)?
        .trim_matches(|c: char| !c.is_alphanumeric() && !matches!(c, ' ' | '-' | '_'));
    (!phrase.is_empty() && phrase.len() <= 80 && prefix >= 4 && old_suffix >= 4)
        .then(|| phrase.to_owned())
}

fn git_oid(repo: &Path, rev: &str) -> Option<String> {
    let output = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["rev-parse", "--verify", "--end-of-options", rev])
        .current_dir(repo)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let oid = std::str::from_utf8(&output.stdout).ok()?.trim();
    (oid.len() >= 40 && oid.chars().all(|c| c.is_ascii_hexdigit())).then(|| oid.to_owned())
}

fn grep_lines(bytes: &[u8], commit_oid: &str) -> Option<Vec<(String, u64, String)>> {
    let mut out = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let name_end = cursor + bytes.get(cursor..)?.iter().position(|b| *b == 0)?;
        let header = std::str::from_utf8(bytes.get(cursor..name_end)?).ok()?;
        let path = header.strip_prefix(&format!("{commit_oid}:"))?.to_owned();
        cursor = name_end + 1;
        let number_end = cursor + bytes.get(cursor..)?.iter().position(|b| *b == 0)?;
        let number = std::str::from_utf8(bytes.get(cursor..number_end)?)
            .ok()?
            .parse::<u64>()
            .ok()?;
        cursor = number_end + 1;
        let text_end = cursor + bytes.get(cursor..)?.iter().position(|b| *b == b'\n')?;
        let line = std::str::from_utf8(bytes.get(cursor..text_end)?)
            .ok()?
            .to_owned();
        cursor = text_end + 1;
        out.push((path, number, line));
    }
    Some(out)
}

fn term_spans(line: &str, term: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if term.is_empty() || term.len() > line.len() {
        return out;
    }
    for start in 0..=line.len() - term.len() {
        let end = start + term.len();
        let Some(candidate) = line.get(start..end) else {
            continue;
        };
        if !candidate.eq_ignore_ascii_case(term) {
            continue;
        }
        let before_word = line
            .get(..start)
            .and_then(|s| s.chars().next_back())
            .is_some_and(char::is_alphanumeric);
        let after_word = line
            .get(end..)
            .and_then(|s| s.chars().next())
            .is_some_and(char::is_alphanumeric);
        if !before_word && !after_word {
            out.push((start, end));
        }
    }
    out
}

fn surface_of(path: &str) -> Surface {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.contains(&"tests")
        || parts.contains(&"test")
        || path.contains(".test.")
        || path.contains(".spec.")
    {
        Surface::Tests
    } else if path.starts_with(".github/workflows/") || path == ".gitlab-ci.yml" {
        Surface::Ci
    } else if path.starts_with("docs/")
        || path.ends_with(".md")
        || path.ends_with(".mdx")
        || path.ends_with(".rst")
    {
        Surface::Docs
    } else if path.starts_with("ui/")
        || path.ends_with(".jsx")
        || path.ends_with(".tsx")
        || path.ends_with(".html")
    {
        Surface::UiCopy
    } else {
        Surface::Code
    }
}

fn ignore_patterns(
    repo: &Path,
    commit_oid: &str,
) -> std::result::Result<Vec<String>, &'static str> {
    let listed = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args([
            "ls-tree",
            "--name-only",
            commit_oid,
            "--",
            ".thrulineignore",
        ])
        .current_dir(repo)
        .output()
        .map_err(|_| "exclusion file could not be checked")?;
    if !listed.status.success() {
        return Err("exclusion file could not be checked");
    }
    if listed.stdout.is_empty() {
        return Ok(Vec::new());
    }
    let output = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .arg("show")
        .arg(format!("{commit_oid}:.thrulineignore"))
        .current_dir(repo)
        .output()
        .map_err(|_| "exclusion file could not be read")?;
    if !output.status.success() {
        return Err("exclusion file could not be read");
    }
    let text = std::str::from_utf8(&output.stdout).map_err(|_| "exclusion file is not UTF-8")?;
    let mut patterns = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('!') || line.contains('[') || line.contains('?') {
            return Err("exclusion pattern is not supported");
        }
        patterns.push(line.trim_start_matches('/').to_owned());
    }
    Ok(patterns)
}

fn ignored(path: &str, allowlist: &[String], ignore: &[String]) -> bool {
    allowlist
        .iter()
        .chain(ignore.iter())
        .any(|p| glob_path(p, path))
}

fn glob_path(pattern: &str, path: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix("/**") {
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }
    if pattern.ends_with('/') {
        return path.starts_with(pattern);
    }
    if !pattern.contains('*') {
        return path == pattern;
    }
    let pats: Vec<&str> = pattern.split('/').collect();
    let parts: Vec<&str> = path.split('/').collect();
    glob_parts(&pats, &parts)
}

fn glob_parts(pattern: &[&str], path: &[&str]) -> bool {
    if pattern.is_empty() {
        return path.is_empty();
    }
    if pattern[0] == "**" {
        return glob_parts(&pattern[1..], path)
            || (!path.is_empty() && glob_parts(pattern, &path[1..]));
    }
    !path.is_empty() && glob_segment(pattern[0], path[0]) && glob_parts(&pattern[1..], &path[1..])
}

fn glob_segment(pattern: &str, path: &str) -> bool {
    let mut states = vec![false; path.len() + 1];
    states[0] = true;
    for p in pattern.bytes() {
        let mut next = vec![false; path.len() + 1];
        if p == b'*' {
            next[0] = states[0];
            for i in 1..=path.len() {
                next[i] = states[i] || next[i - 1];
            }
        } else {
            for (i, c) in path.bytes().enumerate() {
                next[i + 1] = states[i] && p == c;
            }
        }
        states = next;
    }
    states[path.len()]
}
