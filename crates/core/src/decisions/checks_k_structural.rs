//! Pure advisory Family K structural candidates. Never a Decisions verdict.
//!
//! Callers must bind each supplied census to exact Git objects first. A
//! `NoSignal` here means only that this narrow scanner saw no candidate.

use std::collections::{BTreeMap, BTreeSet};

use crate::redact::Masked;

use super::checks_k_candidates::{KCandidateObservation, KCandidateOutcome, KCandidateState};
use super::checks_preservation::{AssertionCensus, CiStepCensus};

const K_CODES: [&str; 5] = ["K1", "K2", "K3", "K8", "K9"];

#[derive(Debug, Clone, Default)]
pub struct KStructuralInput {
    pub source: Option<SourceCensus>,
    pub files: Option<TestFileCensus>,
    pub assertions: Option<AssertionEvidence>,
    pub ci: Option<CiEvidence>,
}

#[derive(Debug, Clone)]
pub struct SourceCensus {
    pub complete: bool,
    pub changes: Vec<SourceDelta>,
}

#[derive(Debug, Clone)]
pub struct SourceDelta {
    pub path: String,
    pub baseline: Option<String>,
    pub target: Option<String>,
}

#[derive(Debug, Clone)]
pub struct TestFileCensus {
    pub complete: bool,
    pub baseline_test_paths: Vec<String>,
    pub target_test_paths: Vec<String>,
    pub target_source_paths: Vec<String>,
    pub links: Vec<TestSourceLink>,
}

#[derive(Debug, Clone)]
pub struct TestSourceLink {
    pub test_path: String,
    pub source_paths: Vec<String>,
    pub moved_to: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AssertionEvidence {
    pub census: AssertionCensus,
    pub detector_complete: bool,
    pub detector_findings: Vec<AssertionDetectorFinding>,
}

#[derive(Debug, Clone)]
pub struct AssertionDetectorFinding {
    pub detector: String,
    pub path: String,
    pub line: u64,
}

#[derive(Debug, Clone)]
pub struct CiEvidence {
    pub complete: bool,
    pub steps: Vec<CiStepCensus>,
}

pub fn scan_k_structural(code: &str, input: &KStructuralInput) -> KCandidateOutcome {
    match code {
        "K1" => scan_skips(input.source.as_ref()),
        "K2" => scan_test_deletions(input.files.as_ref()),
        "K3" => scan_assertions(input.assertions.as_ref()),
        "K8" | "K9" => scan_ci(code, input.ci.as_ref()),
        _ => unavailable(code, "structural candidate kind is unsupported", 0),
    }
}

pub fn scan_k_structural_candidates(input: &KStructuralInput) -> Vec<KCandidateOutcome> {
    K_CODES
        .iter()
        .map(|code| scan_k_structural(code, input))
        .collect()
}

fn unavailable(code: &str, reason: &'static str, examined: usize) -> KCandidateOutcome {
    KCandidateOutcome {
        code: code.into(),
        state: KCandidateState::Unverifiable,
        observations: Vec::new(),
        reason: Some(reason),
        examined_source_files: examined,
    }
}

fn candidate(
    code: &str,
    observations: Vec<KCandidateObservation>,
    signal_reason: &'static str,
    no_signal_reason: &'static str,
    examined: usize,
) -> KCandidateOutcome {
    let state = if observations.is_empty() {
        KCandidateState::NoSignal
    } else {
        KCandidateState::Signal
    };
    KCandidateOutcome {
        code: code.into(),
        state,
        observations,
        reason: Some(if state == KCandidateState::Signal {
            signal_reason
        } else {
            no_signal_reason
        }),
        examined_source_files: examined,
    }
}

fn observation(path: &str, line: u64, marker: &'static str) -> KCandidateObservation {
    KCandidateObservation {
        path: Masked::mask(path).into_text(),
        line,
        marker,
    }
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(':')
        && !path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
}

fn unique_safe_paths(paths: &[String]) -> bool {
    let mut seen = BTreeSet::new();
    paths
        .iter()
        .all(|path| safe_path(path) && seen.insert(path))
}

fn scan_skips(source: Option<&SourceCensus>) -> KCandidateOutcome {
    let Some(source) = source else {
        return unavailable("K1", "changed test-source census is missing", 0);
    };
    if !source.complete {
        return unavailable("K1", "changed test-source census is incomplete", 0);
    }
    let mut seen = BTreeSet::new();
    if source.changes.iter().any(|change| {
        !safe_path(&change.path)
            || !seen.insert(change.path.as_str())
            || (change.baseline.is_none() && change.target.is_none())
    }) {
        return unavailable("K1", "changed test-source paths or blobs are invalid", 0);
    }
    let examined = source
        .changes
        .iter()
        .filter(|change| is_test_source(&change.path))
        .count();
    let mut prior_lines = BTreeMap::<String, usize>::new();
    for change in &source.changes {
        if is_test_source(&change.path) {
            if let Some(before) = &change.baseline {
                for line in before.lines() {
                    *prior_lines.entry(line.trim().into()).or_default() += 1;
                }
            }
        }
    }
    let mut observations = Vec::new();
    for change in &source.changes {
        if !is_test_source(&change.path) {
            continue;
        }
        let Some(after) = &change.target else {
            continue;
        };
        let lines = after.lines().collect::<Vec<_>>();
        for (index, line) in lines.iter().enumerate() {
            if let Some(count) = prior_lines.get_mut(line.trim()) {
                if *count > 0 {
                    *count -= 1;
                    continue;
                }
            }
            if let Some(marker) = skip_focus_marker(&change.path, &lines, index) {
                observations.push(observation(&change.path, (index + 1) as u64, marker));
            }
        }
    }
    candidate(
        "K1",
        observations,
        "added skip/focus syntax needs review; intent to get green is unproved",
        "no supported added skip/focus syntax; run scope and intent are unobserved",
        examined,
    )
}

fn is_test_source(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    if lower.ends_with(".rs") {
        return true; // Rust unit tests can be in production source files.
    }
    if lower.ends_with("_test.go")
        || lower.ends_with("_test.dart")
        || lower.ends_with("_test.exs")
        || name.starts_with("test_")
        || name.ends_with("test.java")
        || name.ends_with("test.kt")
        || name.ends_with("tests.cs")
        || name.ends_with("test.php")
        || name.ends_with("tests.swift")
        || name.ends_with("_spec.rb")
    {
        return true;
    }
    let recognized = [".js", ".jsx", ".ts", ".tsx", ".py", ".rb", ".php", ".swift"]
        .iter()
        .any(|ext| lower.ends_with(ext));
    recognized
        && (lower.contains("/test/")
            || lower.contains("/tests/")
            || lower.contains("/__tests__/")
            || lower.contains("/spec/")
            || lower.contains(".test.")
            || lower.contains(".spec."))
}

fn skip_focus_marker(path: &str, lines: &[&str], index: usize) -> Option<&'static str> {
    let line = lines.get(index)?.trim();
    if line.is_empty()
        || line.starts_with("//")
        || line.starts_with("/*")
        || line.starts_with('*')
        || (path.ends_with(".py") && line.starts_with('#'))
    {
        return None;
    }
    let lower = path.to_ascii_lowercase();
    if [".js", ".jsx", ".ts", ".tsx"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        if platform_guard_near(lines, index) {
            return None;
        }
        let focus = [
            "test.only(",
            "it.only(",
            "describe.only(",
            "fit(",
            "fdescribe(",
        ];
        let skip = ["test.skip(", "it.skip(", "describe.skip(", "xit(", "xtest("];
        return focus
            .iter()
            .any(|prefix| line.starts_with(prefix))
            .then_some("test_focused")
            .or_else(|| {
                skip.iter()
                    .any(|prefix| line.starts_with(prefix))
                    .then_some("test_skipped")
            });
    }
    if lower.ends_with(".py") {
        if line.starts_with("@pytest.mark.skipif")
            || line.starts_with("@unittest.skipIf")
            || line.starts_with("@unittest.skipUnless")
        {
            return None;
        }
        return (line.starts_with("@pytest.mark.skip") || line.starts_with("@unittest.skip"))
            .then_some("test_skipped");
    }
    if lower.ends_with(".go") {
        let code = code_without_strings_and_comments(line);
        if !(code.contains("t.Skip(") || code.contains("t.SkipNow("))
            || platform_guard_near(lines, index)
        {
            return None;
        }
        return Some("test_skipped");
    }
    if lower.ends_with(".rs") {
        return (line == "#[ignore]" || line.starts_with("#[ignore =")).then_some("test_skipped");
    }
    if lower.ends_with(".java") || lower.ends_with(".kt") {
        return (line.starts_with("@Disabled") || line.starts_with("@Ignore"))
            .then_some("test_skipped");
    }
    if lower.ends_with(".cs") {
        return (line.starts_with("[Ignore")
            || (line.starts_with("[Fact(") || line.starts_with("[Theory("))
                && line.contains("Skip"))
        .then_some("test_skipped");
    }
    if lower.ends_with(".rb") {
        return (line.starts_with("skip ")
            || line == "skip"
            || line.starts_with("pending ")
            || line.starts_with("xit "))
        .then_some("test_skipped")
        .or_else(|| line.starts_with("fit ").then_some("test_focused"));
    }
    if lower.ends_with(".php") {
        return line
            .starts_with("$this->markTestSkipped(")
            .then_some("test_skipped");
    }
    if lower.ends_with(".swift") {
        return line.starts_with("throw XCTSkip(").then_some("test_skipped");
    }
    if lower.ends_with(".dart") {
        return ((line.starts_with("test(") || line.starts_with("group("))
            && line.contains("skip: true"))
        .then_some("test_skipped");
    }
    if lower.ends_with(".exs") {
        return (line == "@tag :skip" || line.starts_with("@tag skip: true"))
            .then_some("test_skipped");
    }
    None
}

fn platform_guard_near(lines: &[&str], index: usize) -> bool {
    lines[index.saturating_sub(3)..=index].iter().any(|near| {
        let near = near.trim_start();
        (near.starts_with("if ") || near.starts_with("if("))
            && [
                "runtime.GOOS",
                "runtime.GOARCH",
                "process.platform",
                "navigator.platform",
                "Deno.build.os",
                "Bun.platform",
            ]
            .iter()
            .any(|guard| near.contains(guard))
    })
}

fn code_without_strings_and_comments(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut quote = None;
    let mut escaped = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        if let Some(delimiter) = quote {
            out.push(' ');
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == delimiter {
                quote = None;
            }
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            break;
        }
        if matches!(ch, '\'' | '"' | '`') {
            quote = Some(ch);
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out
}

fn scan_test_deletions(files: Option<&TestFileCensus>) -> KCandidateOutcome {
    let Some(files) = files else {
        return unavailable("K2", "test-file and source-path census is missing", 0);
    };
    if !files.complete {
        return unavailable("K2", "test-file and source-path census is incomplete", 0);
    }
    if !unique_safe_paths(&files.baseline_test_paths)
        || !unique_safe_paths(&files.target_test_paths)
        || !unique_safe_paths(&files.target_source_paths)
        || !unique_safe_paths(
            &files
                .links
                .iter()
                .map(|link| link.test_path.clone())
                .collect::<Vec<_>>(),
        )
    {
        return unavailable("K2", "test/source census has invalid or duplicate paths", 0);
    }
    let target_tests = files
        .target_test_paths
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let target_source = files
        .target_source_paths
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let links = files
        .links
        .iter()
        .map(|link| (link.test_path.as_str(), link))
        .collect::<BTreeMap<_, _>>();
    let mut observations = Vec::new();
    for test_path in &files.baseline_test_paths {
        if target_tests.contains(test_path.as_str()) {
            continue;
        }
        let Some(link) = links.get(test_path.as_str()) else {
            return unavailable("K2", "deleted test has no accepted source link", 0);
        };
        if let Some(new_test) = &link.moved_to {
            if !safe_path(new_test) || !target_tests.contains(new_test.as_str()) {
                return unavailable("K2", "test moved_to receipt has no target file", 0);
            }
            continue;
        }
        if link.source_paths.is_empty() || !unique_safe_paths(&link.source_paths) {
            return unavailable("K2", "deleted test has no valid linked source paths", 0);
        }
        if link
            .source_paths
            .iter()
            .any(|source| target_source.contains(source.as_str()))
        {
            observations.push(observation(test_path, 0, "test_deleted_source_remains"));
        }
    }
    candidate(
        "K2",
        observations,
        "linked source remains after test deletion; person review is needed",
        "no linked test deletion found; source renames outside accepted links are unobserved",
        0,
    )
}

fn scan_assertions(assertions: Option<&AssertionEvidence>) -> KCandidateOutcome {
    let Some(assertions) = assertions else {
        return unavailable("K3", "assertion and weakening censuses are missing", 0);
    };
    let census = &assertions.census;
    if !census.complete
        || census.baseline_version.is_empty()
        || census.target_version.is_empty()
        || census.baseline_tree_hash.is_empty()
        || census.target_tree_hash.is_empty()
    {
        return unavailable("K3", "assertion census is incomplete or unbound", 0);
    }
    let mut baseline_total = 0u64;
    let mut target_total = 0u64;
    for file in &census.files {
        if file.baseline_path.is_none() && file.target_path.is_none() {
            return unavailable("K3", "assertion row has no file on either version", 0);
        }
        if let Some(path) = &file.baseline_path {
            if !safe_path(path) || file.baseline_blob.as_deref().is_none_or(str::is_empty) {
                return unavailable("K3", "assertion baseline file receipt is missing", 0);
            }
        } else if file.baseline_blob.is_some() || file.baseline_count != 0 {
            return unavailable("K3", "new assertion file has a fabricated baseline", 0);
        }
        if let Some(path) = &file.target_path {
            if !safe_path(path) || file.target_blob.as_deref().is_none_or(str::is_empty) {
                return unavailable("K3", "assertion target file receipt is missing", 0);
            }
        } else if file.target_blob.is_some() || file.target_count != 0 {
            return unavailable("K3", "deleted assertion file has a fabricated target", 0);
        }
        if let (Some(old), Some(new)) = (&file.baseline_path, &file.target_path) {
            if old != new && file.moved_to.as_deref() != Some(new) {
                return unavailable("K3", "test move lacks a matching moved_to receipt", 0);
            }
        }
        let Some(next_base) = baseline_total.checked_add(file.baseline_count) else {
            return unavailable("K3", "assertion baseline count overflowed", 0);
        };
        let Some(next_target) = target_total.checked_add(file.target_count) else {
            return unavailable("K3", "assertion target count overflowed", 0);
        };
        baseline_total = next_base;
        target_total = next_target;
    }
    let mut observations = Vec::new();
    if target_total < baseline_total {
        let path = census
            .files
            .iter()
            .find(|file| file.target_count < file.baseline_count)
            .and_then(|file| {
                file.target_path
                    .as_deref()
                    .or(file.baseline_path.as_deref())
            })
            .unwrap_or("(test tree)");
        observations.push(observation(path, 0, "assertion_count_decreased"));
    }
    for finding in &assertions.detector_findings {
        let marker = match finding.detector.as_str() {
            "assertion_removed" => "assertion_removed",
            "assertion_weakened" => "assertion_weakened",
            "assertion_count_decreased" => "assertion_count_decreased",
            _ => continue,
        };
        if !safe_path(&finding.path) {
            return unavailable("K3", "assertion detector path is invalid", 0);
        }
        observations.push(observation(&finding.path, finding.line, marker));
    }
    if !assertions.detector_complete && observations.is_empty() {
        return unavailable("K3", "assertion-strength detector census is missing", 0);
    }
    candidate(
        "K3",
        observations,
        "assertion count or strength declined; person review is needed",
        "no supplied assertion-loss signal; Git receipts are not checked by this pure scanner",
        0,
    )
}

fn scan_ci(code: &str, ci: Option<&CiEvidence>) -> KCandidateOutcome {
    let Some(ci) = ci else {
        return unavailable(code, "complete workflow parser census is missing", 0);
    };
    if !ci.complete || ci.steps.iter().any(|step| !step.complete) {
        return unavailable(code, "workflow parser census is incomplete", 0);
    }
    let identity = ci.steps.first().map(|first| {
        (
            first.baseline_version.as_str(),
            first.baseline_tree_hash.as_str(),
            first.target_version.as_str(),
            first.target_tree_hash.as_str(),
        )
    });
    let mut observed_steps = BTreeSet::new();
    for step in &ci.steps {
        let row_identity = (
            step.baseline_version.as_str(),
            step.baseline_tree_hash.as_str(),
            step.target_version.as_str(),
            step.target_tree_hash.as_str(),
        );
        if identity != Some(row_identity)
            || row_identity.0.is_empty()
            || row_identity.1.is_empty()
            || row_identity.2.is_empty()
            || row_identity.3.is_empty()
            || !safe_path(&step.workflow)
            || step.step.trim().is_empty()
            || step.baseline_blob.is_empty()
            || step.target_blob.is_empty()
            || !observed_steps.insert((step.workflow.as_str(), step.step.as_str()))
            || step.baseline_threshold_pct.is_some_and(|pct| pct > 100)
            || step.target_threshold_pct.is_some_and(|pct| pct > 100)
            || step
                .baseline_bypasses
                .iter()
                .chain(&step.target_bypasses)
                .any(|value| {
                    !matches!(
                        value.as_str(),
                        "|| true" | "continue-on-error" | "allow_failure"
                    )
                })
        {
            return unavailable(code, "CI row identities or typed observations disagree", 0);
        }
    }
    let mut observations = Vec::new();
    for step in &ci.steps {
        if code == "K8" {
            if step.baseline_step_present && !step.target_step_present {
                observations.push(observation(&step.workflow, 0, "ci_step_removed"));
            }
            for bypass in &step.target_bypasses {
                if !step.baseline_bypasses.contains(bypass) {
                    observations.push(observation(&step.workflow, 0, "ci_bypass_added"));
                }
            }
        } else if let Some(baseline) = step.baseline_threshold_pct {
            if step
                .target_threshold_pct
                .is_none_or(|target| target < baseline)
            {
                observations.push(observation(&step.workflow, 0, "coverage_threshold_lowered"));
            }
        }
    }
    candidate(
        code,
        observations,
        "typed CI weakening delta needs review; workflow blobs are not checked here",
        "no supplied CI weakening delta; workflow parser coverage is not independently proved",
        0,
    )
}
