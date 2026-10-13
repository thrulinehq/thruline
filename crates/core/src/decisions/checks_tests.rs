//! B2: a named test remains in the exact tree and in an exact-version run.
//!
//! Aggregate runner summaries do not name tests. An adapter must supply a
//! complete named-result census beside the runner tally; absent evidence is
//! `Unverifiable`. No command is executed while computing this verdict.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::redact::Masked;
use crate::test_tally::TestTally;

use super::checks::{CheckOutcome, Finding, SurfaceResult, VerdictState};
use super::params::{Surface, TestPresentAndRunsParams};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamedTestState {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NamedTest {
    pub name: String,
    pub state: NamedTestState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedTestRun {
    pub run_id: String,
    pub seq: u64,
    pub version_id: String,
    pub tree_hash: String,
    pub command: String,
    pub exit_code: Option<i64>,
    pub tally: Option<TestTally>,
    pub errors: Option<u64>,
    pub complete: bool,
    /// True only when the adapter captured every named result from this run.
    pub named_results_complete: bool,
    pub tests: Vec<NamedTest>,
}

/// Optional accepted scope for the K1/K2 feature-removal case. The caller
/// obtains these paths from a person's accepted decision. This checker also
/// verifies their baseline presence and target absence in Git objects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureScope {
    pub accepted_baseline_version: String,
    pub feature_paths: Vec<String>,
    pub test_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestRunCensus {
    pub version_id: String,
    /// Git commit that supplied the exact tree; the HQ version id may differ.
    pub commit_oid: String,
    pub tree_hash: String,
    pub complete: bool,
    pub runs: Vec<RecordedTestRun>,
    pub feature_scope: Option<FeatureScope>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceState {
    Runnable,
    Disabled,
    PlatformConditional,
}

#[derive(Debug, Clone)]
struct TestDefinition {
    name: String,
    path: String,
    line: u64,
    state: SourceState,
}

pub fn check_test_present_and_runs(
    repo: &Path,
    version: &str,
    params: &TestPresentAndRunsParams,
    census: Option<&TestRunCensus>,
) -> CheckOutcome {
    let Some(commit) = git_oid(repo, &format!("{version}^{{commit}}")) else {
        return CheckOutcome::unavailable("version cannot be resolved to a commit", None);
    };
    let Some(tree_hash) = git_oid(repo, &format!("{commit}^{{tree}}")) else {
        return CheckOutcome::unavailable("version tree cannot be resolved", None);
    };
    if !supported_pattern(&params.name_pattern) || params.command.trim().is_empty() {
        return CheckOutcome::unavailable("test name or command is not supported", Some(tree_hash));
    }
    let definitions = match definitions_at(repo, &commit) {
        Ok(defs) => defs,
        Err(reason) => return CheckOutcome::unavailable(reason, Some(tree_hash)),
    };
    let matches: Vec<_> = definitions
        .iter()
        .filter(|d| name_matches(&params.name_pattern, &d.name))
        .collect();
    if matches.is_empty() {
        if let Some(scope) = census.and_then(|c| c.feature_scope.as_ref()) {
            let Some(census) = census else {
                return CheckOutcome::unavailable("feature evidence is missing", Some(tree_hash));
            };
            if !census.complete || census.commit_oid != commit || census.tree_hash != tree_hash {
                return CheckOutcome::unavailable(
                    "feature evidence is not on this exact version",
                    Some(tree_hash),
                );
            }
            match feature_removed_with_tests(repo, &commit, scope) {
                Ok(true) => {
                    return outcome(
                        VerdictState::Holds,
                        &tree_hash,
                        Some("feature and its tests were removed together"),
                        Vec::new(),
                    );
                }
                Ok(false) => {}
                Err(reason) => return CheckOutcome::unavailable(reason, Some(tree_hash)),
            }
        }
        return outcome(
            VerdictState::Broken,
            &tree_hash,
            Some("named test is absent from this version"),
            vec![Finding {
                surface: Surface::Tests,
                path: "(test tree)".into(),
                line: 0,
                span: Masked::mask(&params.name_pattern).into_text(),
            }],
        );
    }
    let disabled: Vec<Finding> = matches
        .iter()
        .filter(|d| d.state == SourceState::Disabled)
        .map(|d| Finding {
            surface: Surface::Tests,
            path: Masked::mask(&d.path).into_text(),
            line: d.line,
            span: Masked::mask(&d.name).into_text(),
        })
        .collect();
    if !disabled.is_empty() {
        return outcome(
            VerdictState::Broken,
            &tree_hash,
            Some("named test is disabled or focused in source"),
            disabled,
        );
    }
    let Some(census) = census else {
        return CheckOutcome::unavailable("no named test run on this version", Some(tree_hash));
    };
    if !census.complete || census.commit_oid != commit || census.tree_hash != tree_hash {
        return CheckOutcome::unavailable(
            "named test run census is not on this exact version",
            Some(tree_hash),
        );
    }
    let run = census
        .runs
        .iter()
        .filter(|run| {
            run.version_id == census.version_id
                && run.tree_hash == tree_hash
                && run.command.trim() == params.command.trim()
        })
        .max_by_key(|run| run.seq);
    let Some(run) = run else {
        return CheckOutcome::unavailable("no named test run on this version", Some(tree_hash));
    };
    if !run.complete || !run.named_results_complete {
        return CheckOutcome::unavailable(
            "last test run has incomplete named results",
            Some(tree_hash),
        );
    }
    let Some(tally) = run.tally else {
        return CheckOutcome::unavailable("last test run has no tally", Some(tree_hash));
    };
    let (Some(exit_code), Some(errors)) = (run.exit_code, run.errors) else {
        return CheckOutcome::unavailable("last test run has incomplete outcome", Some(tree_hash));
    };
    if tally.suites == 0 || tally.passed + tally.failed + tally.ignored == 0 {
        return outcome(
            VerdictState::Broken,
            &tree_hash,
            Some("last test run contains no tests"),
            findings_for(&matches),
        );
    }
    let named: Vec<_> = run
        .tests
        .iter()
        .filter(|test| name_matches(&params.name_pattern, &test.name))
        .collect();
    if named.is_empty() {
        return outcome(
            VerdictState::Broken,
            &tree_hash,
            Some("named test is absent from the last recorded tally"),
            findings_for(&matches),
        );
    }
    if named.iter().any(|test| match test.state {
        NamedTestState::Passed => tally.passed == 0,
        NamedTestState::Failed => tally.failed == 0,
        NamedTestState::Skipped => tally.ignored == 0,
    }) {
        return CheckOutcome::unavailable("named results and tally do not agree", Some(tree_hash));
    }
    if exit_code != 0
        || errors > 0
        || named
            .iter()
            .any(|test| test.state == NamedTestState::Failed)
    {
        return outcome(
            VerdictState::Broken,
            &tree_hash,
            Some("named test run did not pass"),
            findings_for(&matches),
        );
    }
    if named
        .iter()
        .any(|test| test.state == NamedTestState::Passed)
    {
        return outcome(VerdictState::Holds, &tree_hash, None, Vec::new());
    }
    if matches
        .iter()
        .all(|d| d.state == SourceState::PlatformConditional)
    {
        return outcome(
            VerdictState::Holds,
            &tree_hash,
            Some("named test has a platform condition and appears in the tally"),
            Vec::new(),
        );
    }
    outcome(
        VerdictState::Broken,
        &tree_hash,
        Some("named test was skipped in the last run"),
        findings_for(&matches),
    )
}

fn findings_for(defs: &[&TestDefinition]) -> Vec<Finding> {
    defs.iter()
        .map(|d| Finding {
            surface: Surface::Tests,
            path: Masked::mask(&d.path).into_text(),
            line: d.line,
            span: Masked::mask(&d.name).into_text(),
        })
        .collect()
}

fn outcome(
    state: VerdictState,
    tree_hash: &str,
    reason: Option<&str>,
    findings: Vec<Finding>,
) -> CheckOutcome {
    let finding_count = findings.len() as u64;
    let checked_places = 1;
    CheckOutcome {
        state,
        tree_hash: Some(tree_hash.to_owned()),
        reason: reason.map(str::to_owned),
        checked_places,
        findings,
        surfaces: vec![SurfaceResult {
            surface: Surface::Tests,
            checked_places,
            finding_count,
            state,
        }],
        not_looked_at: Vec::new(),
        visual: Vec::new(),
    }
}

fn git_oid(repo: &Path, revision: &str) -> Option<String> {
    let out = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["rev-parse", "--verify", "--end-of-options", revision])
        .current_dir(repo)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let oid = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (oid.len() == 40 || oid.len() == 64)
        .then_some(oid)
        .filter(|oid| oid.bytes().all(|c| c.is_ascii_hexdigit()))
}

fn definitions_at(repo: &Path, commit: &str) -> Result<Vec<TestDefinition>, &'static str> {
    let out = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["ls-tree", "-r", "-z", "--name-only", commit])
        .current_dir(repo)
        .output()
        .map_err(|_| "version test tree could not be read")?;
    if !out.status.success() {
        return Err("version test tree could not be read");
    }
    let mut defs = Vec::new();
    for raw in out
        .stdout
        .split(|b| *b == 0)
        .filter(|entry| !entry.is_empty())
    {
        let path = std::str::from_utf8(raw).map_err(|_| "test path is not UTF-8")?;
        if !test_source_path(path) {
            continue;
        }
        let object = format!("{commit}:{path}");
        let blob = crate::git_isolation::isolate_config(&mut Command::new("git"))
            .args(["cat-file", "blob", &object])
            .current_dir(repo)
            .output()
            .map_err(|_| "test source could not be read")?;
        if !blob.status.success() {
            return Err("test source could not be read");
        }
        let source = std::str::from_utf8(&blob.stdout).map_err(|_| "test source is not UTF-8")?;
        defs.extend(parse_definitions(path, source));
    }
    Ok(defs)
}

fn test_source_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".rs") {
        // Rust unit tests often live in src/lib.rs under #[cfg(test)].
        return true;
    }
    let supported = [".rs", ".js", ".jsx", ".ts", ".tsx", ".py", ".go"]
        .iter()
        .any(|ext| lower.ends_with(ext));
    supported
        && (lower.contains("/test")
            || lower.contains("/spec")
            || lower.ends_with("_test.go")
            || lower.starts_with("tests/"))
}

fn parse_definitions(path: &str, source: &str) -> Vec<TestDefinition> {
    if path.ends_with(".rs") {
        rust_tests(path, source)
    } else if path.ends_with(".py") {
        python_tests(path, source)
    } else if path.ends_with(".go") {
        go_tests(path, source)
    } else {
        javascript_tests(path, source)
    }
}

fn rust_tests(path: &str, source: &str) -> Vec<TestDefinition> {
    let mut defs = Vec::new();
    let mut has_test = false;
    let mut state = SourceState::Runnable;
    for (index, line) in source.lines().enumerate() {
        let s = line.trim();
        if s.starts_with("//") || s.starts_with("/*") || s.starts_with('*') || s.is_empty() {
            continue;
        }
        if s.starts_with("#[") {
            if s.starts_with("#[test]") || s.contains("::test]") {
                has_test = true;
            }
            if s.starts_with("#[ignore") {
                state = SourceState::Disabled;
            } else if s.starts_with("#[cfg_attr(")
                && s.contains("ignore")
                && state != SourceState::Disabled
            {
                state = SourceState::PlatformConditional;
            }
            continue;
        }
        if let Some(after_fn) = s.split_once("fn ").map(|(_, rest)| rest) {
            if has_test {
                let name = after_fn
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect::<String>();
                if !name.is_empty() {
                    defs.push(TestDefinition {
                        name,
                        path: path.into(),
                        line: (index + 1) as u64,
                        state,
                    });
                }
            }
        }
        has_test = false;
        state = SourceState::Runnable;
    }
    defs
}

fn python_tests(path: &str, source: &str) -> Vec<TestDefinition> {
    let mut defs = Vec::new();
    let mut state = SourceState::Runnable;
    for (index, line) in source.lines().enumerate() {
        let s = line.trim();
        if s.starts_with('@') {
            if s.starts_with("@pytest.mark.skipif")
                || s.starts_with("@unittest.skipIf")
                || s.starts_with("@unittest.skipUnless")
            {
                state = SourceState::PlatformConditional;
            } else if s.starts_with("@pytest.mark.skip") || s.starts_with("@unittest.skip") {
                state = SourceState::Disabled;
            }
            continue;
        }
        let s = s.strip_prefix("async ").unwrap_or(s);
        if let Some(name) = s
            .strip_prefix("def ")
            .and_then(|rest| rest.split_once('(').map(|(name, _)| name))
        {
            if name.starts_with("test_") {
                defs.push(TestDefinition {
                    name: name.into(),
                    path: path.into(),
                    line: (index + 1) as u64,
                    state,
                });
            }
            state = SourceState::Runnable;
        } else if !s.is_empty() && !s.starts_with('#') {
            state = SourceState::Runnable;
        }
    }
    defs
}

fn go_tests(path: &str, source: &str) -> Vec<TestDefinition> {
    let lines: Vec<&str> = source.lines().collect();
    let mut defs = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let s = line.trim();
        let Some(name) = s
            .strip_prefix("func Test")
            .and_then(|rest| rest.split_once('(').map(|(name, _)| name))
        else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let end = lines
            .iter()
            .enumerate()
            .skip(index + 1)
            .find(|(_, next)| next.trim_start().starts_with("func "))
            .map(|(i, _)| i)
            .unwrap_or(lines.len());
        let body = &lines[index..end];
        let state = if body
            .iter()
            .any(|line| line.contains("t.Skip(") || line.contains("t.SkipNow("))
        {
            if body
                .iter()
                .any(|line| line.contains("runtime.GOOS") || line.contains("runtime.GOARCH"))
            {
                SourceState::PlatformConditional
            } else {
                SourceState::Disabled
            }
        } else {
            SourceState::Runnable
        };
        defs.push(TestDefinition {
            name: format!("Test{name}"),
            path: path.into(),
            line: (index + 1) as u64,
            state,
        });
    }
    defs
}

fn javascript_tests(path: &str, source: &str) -> Vec<TestDefinition> {
    let mut defs = Vec::new();
    let focused_suite = source.lines().any(|line| {
        let s = line.trim_start();
        !s.starts_with("//") && (s.contains("fdescribe(") || s.contains("describe.only("))
    });
    for (index, line) in source.lines().enumerate() {
        let s = line.trim_start();
        if s.starts_with("//") || s.starts_with("/*") {
            continue;
        }
        for (marker, state) in [
            ("test.skip(", SourceState::Disabled),
            ("it.skip(", SourceState::Disabled),
            ("xtest(", SourceState::Disabled),
            ("xit(", SourceState::Disabled),
            ("test.only(", SourceState::Disabled),
            ("it.only(", SourceState::Disabled),
            ("ftest(", SourceState::Disabled),
            ("fit(", SourceState::Disabled),
            ("test(", SourceState::Runnable),
            ("it(", SourceState::Runnable),
        ] {
            let Some((_, tail)) = s.split_once(marker) else {
                continue;
            };
            let tail = tail.trim_start();
            let Some(quote) = tail
                .chars()
                .next()
                .filter(|c| *c == '\'' || *c == '"' || *c == '`')
            else {
                continue;
            };
            let Some(name) = tail[1..].split(quote).next() else {
                continue;
            };
            if !name.is_empty() {
                defs.push(TestDefinition {
                    name: name.into(),
                    path: path.into(),
                    line: (index + 1) as u64,
                    state: if focused_suite {
                        SourceState::Disabled
                    } else {
                        state
                    },
                });
            }
            break;
        }
    }
    defs
}

fn supported_pattern(pattern: &str) -> bool {
    !pattern.trim().is_empty() && pattern.is_ascii() && !pattern.contains('?')
}

fn canonical(name: &str) -> String {
    let last = name.rsplit("::").next().unwrap_or(name);
    let last = last
        .strip_prefix("Test")
        .or_else(|| last.strip_prefix("test_"))
        .unwrap_or(last);
    last.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '*')
        .flat_map(char::to_lowercase)
        .collect()
}

fn name_matches(pattern: &str, name: &str) -> bool {
    let pattern = canonical(pattern);
    let name = canonical(name);
    if !pattern.contains('*') {
        return pattern == name;
    }
    let mut rest = name.as_str();
    let mut pieces = pattern.split('*').peekable();
    if let Some(first) = pieces.next() {
        if !rest.starts_with(first) {
            return false;
        }
        rest = &rest[first.len()..];
    }
    let suffix = pattern.rsplit('*').next().unwrap_or("");
    for piece in pieces {
        if piece.is_empty() {
            continue;
        }
        let Some(position) = rest.find(piece) else {
            return false;
        };
        rest = &rest[position + piece.len()..];
    }
    suffix.is_empty() || name.ends_with(suffix)
}

fn feature_removed_with_tests(
    repo: &Path,
    target: &str,
    scope: &FeatureScope,
) -> Result<bool, &'static str> {
    if scope.feature_paths.is_empty() || scope.test_paths.is_empty() {
        return Err("feature scope needs feature and test paths");
    }
    if scope
        .feature_paths
        .iter()
        .chain(scope.test_paths.iter())
        .any(|path| !safe_path(path))
    {
        return Err("feature scope has an unsupported path");
    }
    let Some(baseline) = git_oid(
        repo,
        &format!("{}^{{commit}}", scope.accepted_baseline_version),
    ) else {
        return Err("accepted baseline cannot be resolved");
    };
    let ancestor = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["merge-base", "--is-ancestor", &baseline, target])
        .current_dir(repo)
        .status()
        .map_err(|_| "feature history could not be read")?;
    if !ancestor.success() {
        return Err("accepted baseline is not an ancestor of this version");
    }
    for path in scope.feature_paths.iter().chain(scope.test_paths.iter()) {
        if !object_exists(repo, &baseline, path)? || object_exists(repo, target, path)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains(':')
        && !path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
}

fn object_exists(repo: &Path, commit: &str, path: &str) -> Result<bool, &'static str> {
    let object = format!("{commit}:{path}");
    let out = crate::git_isolation::isolate_config(&mut Command::new("git"))
        .args(["cat-file", "-e", &object])
        .current_dir(repo)
        .output()
        .map_err(|_| "feature tree could not be read")?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) | Some(128) => Ok(false),
        _ => Err("feature tree could not be read"),
    }
}
