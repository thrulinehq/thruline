//! BUILD-SPEC §6.8 — special-casing and hardcoding heuristics, source side.
//!
//! > *Flag (`warn`, escalate to `fail` when combined with a `fail` test
//! > finding): branches keyed on values that appear verbatim in test inputs;
//! > `if "test" in sys.argv` / env sniffing (`PYTEST_CURRENT_TEST`, `CI`,
//! > `NODE_ENV=='test'`) newly introduced in source; functions returning
//! > constants that match test expectations; new global mutable state read by a
//! > function under test.*
//!
//! # Newly introduced is the whole rule
//!
//! Every heuristic here compares **base against head**. A repository that has
//! always read `CI` is not special-casing; a repository that started reading it
//! in the same session that claims its tests pass is the shape §6.8 exists for.
//! Flagging the presence rather than the *arrival* would light up on most real
//! codebases and be ignored within a week.
//!
//! # Severity is `warn` here and escalates elsewhere
//!
//! §6.8's own instruction. These are heuristics over source text, and the
//! evidence that turns one into a refusal is a *test-side* `fail` finding — so
//! the combination is applied by the caller, which can see both.

use std::collections::BTreeSet;

use crate::verdict::{Finding, Severity};

/// Environment and argv probes that mean "am I under test?".
const TEST_SNIFFS: &[&str] = &[
    "pytest_current_test",
    "\"pytest\"",
    "'pytest'",
    "sys.argv",
    "unittest",
    "node_env",
    "nodeenv",
    "process.env.ci",
    "getenv(\"ci\")",
    "getenv('ci')",
    "env::var(\"ci\")",
    "environ[\"ci\"]",
    "environ['ci']",
    "environ.get(\"ci\")",
    "environ.get('ci')",
    "cfg!(test)",
    "#[cfg(test)]",
    "testing.testing()",
    "jest.isrunning",
    "vitest",
];

/// Lines that make a branch conditional on something.
fn is_branch(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("if ")
        || t.starts_with("if(")
        || t.starts_with("elif ")
        || t.starts_with("} else if")
        || t.starts_with("else if")
        || t.starts_with("match ")
        || t.starts_with("switch ")
        || t.starts_with("case ")
        || t.starts_with("when ")
}

/// Lines added between two versions of a file.
///
/// A set difference rather than a real diff: it over-reports a moved line,
/// which is the safe direction for a `warn`, and it needs no diff engine.
fn added_lines<'a>(base: &str, head: &'a str) -> Vec<&'a str> {
    let old: BTreeSet<&str> = base.lines().map(str::trim).collect();
    head.lines()
        .filter(|l| !l.trim().is_empty())
        .filter(|l| !old.contains(l.trim()))
        .collect()
}

/// §6.8 over one changed **non-test** source file.
///
/// `test_literals` are string and numeric literals appearing in the session's
/// test files, so a new source branch keyed on one of them can be recognised.
pub fn scan(path: &str, base: &str, head: &str, test_literals: &BTreeSet<String>) -> Vec<Finding> {
    let mut out = Vec::new();
    let added = added_lines(base, head);

    // 1. Env or argv sniffing newly introduced.
    for line in &added {
        let lower = line.to_ascii_lowercase();
        if !is_branch(&lower) {
            continue;
        }
        if let Some(sniff) = TEST_SNIFFS.iter().find(|s| lower.contains(*s)) {
            out.push(
                Finding::new(
                    Severity::Warn,
                    "test_environment_sniffing",
                    format!(
                        "a branch reading `{sniff}` was added to source in this session: \
                         `{}` — code that behaves differently under test proves nothing \
                         when tested",
                        line.trim()
                    ),
                )
                .at(path),
            );
            break;
        }
    }

    // 2. A new branch keyed on a value that appears verbatim in a test.
    for line in &added {
        if !is_branch(line) {
            continue;
        }
        if let Some(lit) = test_literals.iter().find(|l| line.contains(l.as_str())) {
            out.push(
                Finding::new(
                    Severity::Warn,
                    "branch_on_test_literal",
                    format!(
                        "a branch keyed on `{lit}` — a literal that appears verbatim in \
                         this session's tests — was added to source: `{}`",
                        line.trim()
                    ),
                )
                .at(path),
            );
            break;
        }
    }

    // 3. A function that now returns a constant a test expects.
    for line in &added {
        let t = line.trim();
        let returned = t
            .strip_prefix("return ")
            .or_else(|| t.strip_prefix("return("))
            .map(|r| r.trim_end_matches(&[';', ')'][..]).trim());
        let Some(value) = returned else { continue };
        if value.is_empty() || value.len() > 40 {
            continue;
        }
        if test_literals.contains(value) {
            out.push(
                Finding::new(
                    Severity::Warn,
                    "returns_test_expected_constant",
                    format!(
                        "a `return {value}` was added, and `{value}` is a value this \
                         session's tests expect"
                    ),
                )
                .at(path),
            );
            break;
        }
    }

    out
}

/// Escalate §6.8's warnings when a test-side detector already failed.
///
/// §6.8's own rule: these are heuristics on their own, and corroborated when a
/// test file was tampered with in the same session. Applied by the caller
/// because only it sees both sides.
///
/// # R8's second site
///
/// Track G established that a capped finding cannot make `test_side_failed`
/// true **today**, because `crates/cli/src/claims_cmd.rs:614-615` computes the
/// predicate from the findings as emitted, before `tests_pass::escalate` runs.
/// That is an ordering, and the operator ruled that a cap resting on one "is a
/// cap that will be silently lost on the next refactor". So this site does not
/// rely on it: the raise goes through [`Finding::raised_to`], which consults
/// the ceiling the finding carries. If `test_side_failed` is ever computed
/// from post-escalation severities, every §6.8 finding here is still clamped
/// to its own pack's cap.
pub fn escalate_with_test_findings(findings: Vec<Finding>, test_side_failed: bool) -> Vec<Finding> {
    if !test_side_failed {
        return findings;
    }
    findings
        .into_iter()
        .map(|f| {
            let mut out = f.raised_to(Severity::Fail);
            out.detail = format!(
                "{} — and a test-side detector failed in the same session, which is \
                 §6.8's escalation condition",
                out.detail
            );
            out
        })
        .collect()
}

/// Literals a test file asserts on, for heuristic 2 and 3.
pub fn literals_in(source: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut chars = source.chars().peekable();
    let mut current = String::new();
    let mut in_string: Option<char> = None;
    while let Some(c) = chars.next() {
        match in_string {
            Some(q) => {
                if c == '\\' {
                    chars.next();
                    continue;
                }
                if c == q {
                    if current.len() >= 2 && current.len() <= 40 {
                        out.insert(format!("{q}{current}{q}"));
                        out.insert(current.clone());
                    }
                    current.clear();
                    in_string = None;
                } else {
                    current.push(c);
                }
            }
            None => {
                if c == '"' || c == '\'' {
                    in_string = Some(c);
                    current.clear();
                } else if c.is_ascii_digit() {
                    current.push(c);
                    while chars
                        .peek()
                        .is_some_and(|n| n.is_ascii_digit() || *n == '.')
                    {
                        current.push(chars.next().unwrap_or('0'));
                    }
                    // Single digits are everywhere and mean nothing.
                    if current.len() >= 2 {
                        out.insert(std::mem::take(&mut current));
                    }
                    current.clear();
                }
            }
        }
    }
    out
}
