//! Narrow, advisory Family K signals from two immutable Git trees.
//!
//! These are candidates for a person to inspect, never Decisions verdicts.
//! A missing signal does not establish that a full K rule holds. The scanner
//! K6 uses a bounded Python fixture lookup association. K18 abstains while
//! accepted rule and cross-session enforcement receipts are unavailable.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::process::Command;

use crate::redact::Masked;

const MAX_FILES: usize = 10_000;
const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 32 * 1024 * 1024;
const K_CODES: [&str; 8] = ["K4", "K5", "K6", "K7", "K10", "K11", "K12", "K18"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KCandidateState {
    /// A narrow lexical or structural source-delta signal for human review.
    Signal,
    /// No supported signal in examined tracked additions; never a Hold.
    NoSignal,
    /// Required exact source or provenance was unavailable.
    Unverifiable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KCandidateObservation {
    pub path: String,
    pub line: u64,
    pub marker: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KCandidateOutcome {
    pub code: String,
    pub state: KCandidateState,
    pub observations: Vec<KCandidateObservation>,
    pub reason: Option<&'static str>,
    pub examined_source_files: usize,
}

/// Scan supported tracked source additions between full, immutable commits.
///
/// Identical lines moved between files are netted before matching. K5, K7
/// and K12 keep test and production paths separate so moving test code into
/// production is still visible. No model, shell interpreter or mutable
/// working-tree read participates in this candidate scan.
pub fn scan_k_candidate(
    repo: &Path,
    baseline_version: &str,
    target_version: &str,
    code: &str,
) -> KCandidateOutcome {
    scan_k_candidates(repo, baseline_version, target_version)
        .into_iter()
        .find(|outcome| outcome.code == code)
        .unwrap_or_else(|| KCandidateOutcome {
            code: code.into(),
            state: KCandidateState::Unverifiable,
            observations: Vec::new(),
            reason: Some("candidate kind is unsupported"),
            examined_source_files: 0,
        })
}

/// Read one exact Git delta and evaluate all eight advisory candidates.
/// Changed paths include both sides of a rename, so a moved line nets out.
pub fn scan_k_candidates(
    repo: &Path,
    baseline_version: &str,
    target_version: &str,
) -> Vec<KCandidateOutcome> {
    let mut outcomes = K_CODES
        .iter()
        .map(|code| KCandidateOutcome {
            code: (*code).into(),
            state: KCandidateState::Unverifiable,
            observations: Vec::new(),
            reason: None,
            examined_source_files: 0,
        })
        .collect::<Vec<_>>();
    if !exact_commit(repo, baseline_version) || !exact_commit(repo, target_version) {
        for outcome in &mut outcomes {
            outcome.reason = Some("exact full commit identity is unavailable");
        }
        return outcomes;
    }
    for outcome in &mut outcomes {
        if outcome.code == "K6" {
            *outcome = super::checks_k_fixture_lookup::scan(repo, baseline_version, target_version);
            continue;
        }
        outcome.reason = match outcome.code.as_str() {
            "K18" => Some("accepted rule and cross-session enforcement receipts are absent"),
            _ => None,
        };
    }
    let Some(changed) = changed_source_paths(repo, baseline_version, target_version) else {
        set_source_gap(&mut outcomes, "exact changed-path census cannot be read");
        return outcomes;
    };
    let Some(before) = source_tree(repo, baseline_version, &changed) else {
        set_source_gap(
            &mut outcomes,
            "baseline source tree cannot be read completely",
        );
        return outcomes;
    };
    let Some(after) = source_tree(repo, target_version, &changed) else {
        set_source_gap(
            &mut outcomes,
            "target source tree cannot be read completely",
        );
        return outcomes;
    };
    for outcome in &mut outcomes {
        if !matches!(outcome.code.as_str(), "K6" | "K18") {
            scan_one(outcome, &before, &after);
        }
    }
    outcomes
}

fn set_source_gap(outcomes: &mut [KCandidateOutcome], reason: &'static str) {
    for outcome in outcomes {
        if !matches!(outcome.code.as_str(), "K6" | "K18") {
            outcome.reason = Some(reason);
        }
    }
}

fn scan_one(
    outcome: &mut KCandidateOutcome,
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) {
    let code = outcome.code.as_str();
    outcome.examined_source_files = after.len();
    let mut prior_lines: HashMap<(bool, String), usize> = HashMap::new();
    for (path, content) in before {
        let group = separate_test_paths(code) && is_test_path(path);
        for line in content.lines() {
            *prior_lines.entry((group, line.to_owned())).or_default() += 1;
        }
    }
    for (path, content) in after {
        let group = separate_test_paths(code) && is_test_path(path);
        let lines = content.lines().collect::<Vec<_>>();
        for (index, line) in lines.iter().enumerate() {
            let key = (group, (*line).to_owned());
            if let Some(count) = prior_lines.get_mut(&key) {
                if *count > 0 {
                    *count -= 1;
                    continue;
                }
            }
            if let Some(marker) = marker_for(code, path, &lines, index) {
                outcome.observations.push(KCandidateObservation {
                    path: Masked::mask(path).into_text(),
                    line: (index + 1) as u64,
                    marker,
                });
            }
        }
    }
    outcome.state = if outcome.observations.is_empty() {
        KCandidateState::NoSignal
    } else {
        KCandidateState::Signal
    };
    if code == "K7" && outcome.state == KCandidateState::Signal {
        outcome.reason = Some("constant equality syntax does not establish test-only intent");
    }
}

fn changed_source_paths(repo: &Path, baseline: &str, target: &str) -> Option<BTreeSet<String>> {
    let paths = git_output(
        repo,
        &[
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            baseline,
            target,
        ],
    )?;
    let mut changed = BTreeSet::new();
    for raw in paths
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
    {
        let path = std::str::from_utf8(raw).ok()?;
        if supported_source(path) {
            changed.insert(path.to_owned());
        }
    }
    (changed.len() <= MAX_FILES).then_some(changed)
}

fn separate_test_paths(code: &str) -> bool {
    matches!(code, "K5" | "K7" | "K12")
}

fn marker_for(code: &str, path: &str, lines: &[&str], index: usize) -> Option<&'static str> {
    let line = lines.get(index)?.trim();
    let compact = line
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect::<String>();
    let code_only = outside_literals_and_comments(line);
    let compact_code = code_only
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace())
        .collect::<String>();
    let ext = Path::new(path).extension().and_then(|ext| ext.to_str())?;
    let is_js = matches!(ext, "js" | "jsx" | "ts" | "tsx");
    let is_py = ext == "py";
    let is_rs = ext == "rs";
    match code {
        "K4" if is_py && matches!(compact.as_str(), "assertTrue" | "assertTrue#noqa") => {
            Some("tautological_assertion")
        }
        "K4" if is_js
            && matches!(
                compact.as_str(),
                "expect(true).toBe(true);" | "expect(true).toBe(true)"
            ) =>
        {
            Some("tautological_assertion")
        }
        "K4" if is_rs && matches!(compact.as_str(), "assert!(true);" | "assert!(true)") => {
            Some("tautological_assertion")
        }
        "K5" if !is_test_path(path) && is_js && line.starts_with("if") && {
            (code_only.contains("process.env.NODE_ENV")
                && (compact.contains("process.env.NODE_ENV==='test'")
                    || compact.contains("process.env.NODE_ENV===\"test\"")))
                || (code_only.contains("import.meta.env.MODE")
                    && (compact.contains("import.meta.env.MODE==='test'")
                        || compact.contains("import.meta.env.MODE===\"test\"")))
        } =>
        {
            Some("production_test_environment_branch")
        }
        "K5" if !is_test_path(path)
            && is_py
            && line.starts_with("if ")
            && code_only.contains("sys.modules")
            && (compact.contains("\"pytest\"insys.modules")
                || compact.contains("'pytest'insys.modules")) =>
        {
            Some("production_test_environment_branch")
        }
        "K5" if !is_test_path(path) && is_rs && compact_code.contains("cfg!(test)") => {
            Some("production_test_environment_branch")
        }
        "K7" if !is_test_path(path)
            && is_py
            && matches!(compact.as_str(), "returnTrue" | "returnFalse")
            && nearest_function(lines, index) == Some("__eq__") =>
        {
            Some("constant_equality")
        }
        "K10"
            if is_js
                && (line.starts_with("// @ts-ignore")
                    || line.starts_with("// @ts-expect-error")
                    || line.starts_with("// eslint-disable")
                    || line.starts_with("/* eslint-disable")
                    || (!line.starts_with('*') && has_as_any_expression(&code_only))) =>
        {
            Some("type_or_lint_suppression")
        }
        "K10" if is_py && line.starts_with("# noqa") => Some("type_or_lint_suppression"),
        "K10" if is_rs && (line.starts_with("#[allow(") || line.starts_with("#![allow(")) => {
            Some("type_or_lint_suppression")
        }
        "K11" if is_js && compact_code.contains("catch{}") => Some("empty_error_handler"),
        "K11" if is_py && line.starts_with("except") && compact.ends_with(":pass") => {
            Some("empty_error_handler")
        }
        "K11"
            if is_py
                && line == "pass"
                && index > 0
                && lines[index - 1].trim().starts_with("except")
                && lines[index - 1].trim().ends_with(':') =>
        {
            Some("empty_error_handler")
        }
        "K12"
            if !is_test_path(path)
                && is_rs
                && (compact_code.contains("todo!()")
                    || compact_code.contains("unimplemented!()")) =>
        {
            Some("runtime_placeholder")
        }
        "K12" if !is_test_path(path) && is_py && line.starts_with("raise NotImplementedError") => {
            Some("runtime_placeholder")
        }
        _ => None,
    }
}

fn has_as_any_expression(code: &str) -> bool {
    let tokens = code
        .split_whitespace()
        .map(|part| part.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_'))
        .collect::<Vec<_>>();
    tokens
        .windows(2)
        .any(|pair| pair[0] == "as" && pair[1] == "any")
}

/// A line-level guard against treating a quoted example or comment as code.
/// This is deliberately narrower than a language parser.
fn outside_literals_and_comments(line: &str) -> String {
    let mut result = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(ch) = chars.next() {
        if let Some(delimiter) = quote {
            result.push(' ');
            if ch == '\\' {
                if chars.next().is_some() {
                    result.push(' ');
                }
            } else if ch == delimiter {
                quote = None;
            }
            continue;
        }
        if matches!(ch, '\'' | '"' | '\u{60}') {
            quote = Some(ch);
            result.push(' ');
        } else if ch == '#' || (ch == '/' && matches!(chars.peek(), Some('/' | '*'))) {
            break;
        } else {
            result.push(ch);
        }
    }
    result
}

fn nearest_function<'a>(lines: &'a [&'a str], index: usize) -> Option<&'a str> {
    for prior in lines[..index].iter().rev() {
        let trimmed = prior.trim();
        if let Some(rest) = trimmed.strip_prefix("def ") {
            return rest.split('(').next();
        }
    }
    None
}

fn is_test_path(path: &str) -> bool {
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.iter().any(|part| {
        matches!(
            *part,
            "test" | "tests" | "__tests__" | "spec" | "specs" | "fixtures"
        )
    }) {
        return true;
    }
    let name = parts.last().copied().unwrap_or("");
    name.starts_with("test_")
        || name.ends_with("_test.py")
        || name.contains(".test.")
        || name.contains(".spec.")
}

fn source_tree(
    repo: &Path,
    commit: &str,
    changed: &BTreeSet<String>,
) -> Option<BTreeMap<String, String>> {
    if changed.is_empty() {
        return Some(BTreeMap::new());
    }
    let paths = git_output(repo, &["ls-tree", "-r", "-z", "--name-only", commit])?;
    let mut source_paths = BTreeSet::new();
    for raw in paths
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
    {
        let path = std::str::from_utf8(raw).ok()?;
        if changed.contains(path) {
            source_paths.insert(path.to_owned());
        }
    }
    if source_paths.len() > MAX_FILES {
        return None;
    }
    let mut total_bytes = 0usize;
    let mut result = BTreeMap::new();
    for path in source_paths {
        let spec = format!("{commit}:{path}");
        let contents = git_output(repo, &["show", &spec])?;
        if contents.len() > MAX_FILE_BYTES {
            return None;
        }
        total_bytes = total_bytes.checked_add(contents.len())?;
        if total_bytes > MAX_TOTAL_BYTES {
            return None;
        }
        result.insert(path, String::from_utf8(contents).ok()?);
    }
    Some(result)
}

fn supported_source(path: &str) -> bool {
    if path.split('/').any(|part| {
        matches!(
            part,
            "dist"
                | "build"
                | "out"
                | "target"
                | "node_modules"
                | "vendor"
                | ".next"
                | ".vite"
                | "coverage"
        )
    }) || path.contains(".min.")
    {
        return false;
    }
    Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext, "js" | "jsx" | "ts" | "tsx" | "py" | "rs"))
}

fn exact_commit(repo: &Path, version: &str) -> bool {
    if !matches!(version.len(), 40 | 64) || !version.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return false;
    }
    let spec = format!("{version}^{{commit}}");
    git_output(repo, &["rev-parse", "--verify", "--end-of-options", &spec])
        .and_then(|output| String::from_utf8(output).ok())
        .is_some_and(|actual| actual.trim() == version)
}

fn git_output(repo: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args(args)
        .current_dir(repo)
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}
