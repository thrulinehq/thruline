//! What a test run said it ran: the runner's own summary line, as numbers.
//!
//! Parsed from the FULL stored result text of a test command (never from the
//! served excerpt), and only from a line in one of the shapes below. A run
//! whose output states no summary has no tally — `null`, never zeros.
//!
//! | runner | shape read |
//! |---|---|
//! | cargo test | `test result: ok. 12 passed; 0 failed; 1 ignored; …` — via [`crate::test_tally::parse_result_line`]; one line per test target, **summed** |
//! | pytest | `4 passed, 1 failed, 2 skipped, 1 error in 0.30s`, with or without `===` rules |
//! | jest | `Tests:       1 failed, 3 passed, 4 total` |
//! | vitest | `Tests  3 passed (3)` / `Tests  1 failed \| 3 passed (4)` |
//! | node --test (TAP) | `# pass 59`, `# fail 0`, `# skipped 1`, `# todo 0` |
//! | go test | one `ok  <pkg>` / `FAIL <pkg>` / `?  <pkg> [no test files]` line per package — **packages** are tallied |
//!
//! When several summaries are present (cargo's per-target lines, go's
//! per-package lines) the numbers are their sum and `line` is the last
//! summary line seen; `lines` says how many were summed.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Tally {
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub errors: u64,
    /// `tests`, or `packages` for go test.
    pub unit: &'static str,
    /// Which shape matched: `cargo` | `pytest` | `jest` | `vitest` | `tap` | `go`.
    pub runner: &'static str,
    /// The last summary line, verbatim (unmasked here; the caller masks).
    #[serde(skip)]
    pub line: String,
    /// How many summary lines were summed.
    pub lines: u64,
}

/// The tally a test run's output states, or `None`.
pub fn tally(text: &str) -> Option<Tally> {
    cargo(text)
        .or_else(|| jest_or_vitest(text))
        .or_else(|| pytest(text))
        .or_else(|| tap(text))
        .or_else(|| go(text))
}

fn cargo(text: &str) -> Option<Tally> {
    let mut out: Option<Tally> = None;
    for line in text.lines() {
        if let Some(t) = crate::test_tally::parse_result_line(line) {
            let o = out.get_or_insert(Tally {
                unit: "tests",
                runner: "cargo",
                ..Tally::default()
            });
            o.passed += t.passed;
            o.failed += t.failed;
            o.skipped += t.ignored;
            o.lines += 1;
            o.line = line.trim().to_string();
        }
    }
    out
}

/// `N word` pairs in a comma/pipe separated summary.
fn counts(s: &str) -> Vec<(u64, String)> {
    let mut out = Vec::new();
    let words: Vec<&str> = s
        .split(|c: char| c.is_whitespace() || c == ',' || c == '|' || c == '(' || c == ')')
        .filter(|w| !w.is_empty())
        .collect();
    for pair in words.windows(2) {
        if let Ok(n) = pair[0].parse::<u64>() {
            let word = pair[1]
                .trim_matches(|c: char| !c.is_alphabetic())
                .to_ascii_lowercase();
            out.push((n, word));
        }
    }
    out
}

fn apply(t: &mut Tally, pairs: &[(u64, String)]) -> bool {
    let mut any = false;
    for (n, w) in pairs {
        match w.as_str() {
            "passed" | "pass" => {
                t.passed += n;
                any = true;
            }
            "failed" | "fail" => {
                t.failed += n;
                any = true;
            }
            "skipped" | "skip" | "ignored" | "todo" | "deselected" | "xfailed" => {
                t.skipped += n;
                any = true;
            }
            "error" | "errors" => {
                t.errors += n;
                any = true;
            }
            _ => {}
        }
    }
    any
}

fn jest_or_vitest(text: &str) -> Option<Tally> {
    let mut found = None;
    for line in text.lines() {
        let t = line.trim();
        let (rest, runner) = if let Some(r) = t.strip_prefix("Tests:") {
            (r, "jest")
        } else if let Some(r) = t.strip_prefix("Tests ") {
            (r, "vitest")
        } else {
            continue;
        };
        let mut tally = Tally {
            unit: "tests",
            runner,
            line: t.to_string(),
            lines: 1,
            ..Tally::default()
        };
        if apply(&mut tally, &counts(rest)) {
            found = Some(tally);
        }
    }
    found
}

fn pytest(text: &str) -> Option<Tally> {
    let mut found = None;
    for line in text.lines() {
        let t = line.trim().trim_matches('=').trim();
        // pytest's final line always carries a duration: `… in 0.30s`.
        let Some((body, tail)) = t.rsplit_once(" in ") else {
            continue;
        };
        if !tail
            .trim()
            .trim_end_matches(|c: char| c == ')' || c.is_alphabetic() || c == ' ')
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_digit())
        {
            continue;
        }
        let mut tally = Tally {
            unit: "tests",
            runner: "pytest",
            line: line.trim().to_string(),
            lines: 1,
            ..Tally::default()
        };
        let pairs = counts(body);
        // Every pair must be a pytest outcome word, or this is some other
        // sentence that happens to end in a duration.
        let known = [
            "passed",
            "failed",
            "skipped",
            "error",
            "errors",
            "deselected",
            "xfailed",
            "xpassed",
            "warning",
            "warnings",
            "rerun",
        ];
        if pairs.is_empty() || !pairs.iter().all(|(_, w)| known.contains(&w.as_str())) {
            continue;
        }
        if apply(&mut tally, &pairs) {
            found = Some(tally);
        }
    }
    found
}

fn tap(text: &str) -> Option<Tally> {
    let mut tally = Tally {
        unit: "tests",
        runner: "tap",
        ..Tally::default()
    };
    let mut any = false;
    for line in text.lines() {
        let t = line.trim();
        let Some(rest) = t.strip_prefix("# ").or_else(|| t.strip_prefix("ℹ ")) else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let (Some(word), Some(n), None) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let Ok(n) = n.parse::<u64>() else {
            continue;
        };
        let hit = match word {
            "pass" => {
                tally.passed += n;
                true
            }
            "fail" => {
                tally.failed += n;
                true
            }
            "skipped" | "todo" => {
                tally.skipped += n;
                true
            }
            "cancelled" => {
                tally.errors += n;
                true
            }
            _ => false,
        };
        if hit {
            any = true;
            tally.lines += 1;
            tally.line = t.to_string();
        }
    }
    any.then_some(tally)
}

fn go(text: &str) -> Option<Tally> {
    let mut tally = Tally {
        unit: "packages",
        runner: "go",
        ..Tally::default()
    };
    let mut any = false;
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        let hit = match words.as_slice() {
            ["ok", pkg, ..] if !pkg.is_empty() => {
                tally.passed += 1;
                true
            }
            ["FAIL", pkg, ..] if !pkg.is_empty() => {
                tally.failed += 1;
                true
            }
            ["?", _, rest @ ..] if rest.join(" ").contains("[no test files]") => {
                tally.skipped += 1;
                true
            }
            _ => false,
        };
        if hit {
            any = true;
            tally.lines += 1;
            tally.line = line.trim().to_string();
        }
    }
    any.then_some(tally)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nums(t: &Tally) -> (u64, u64, u64, u64) {
        (t.passed, t.failed, t.skipped, t.errors)
    }

    #[test]
    fn cargo_sums_every_target() {
        let out = "running 12 tests\ntest result: ok. 12 passed; 0 failed; 1 ignored; 0 measured\n\ntest result: FAILED. 3 passed; 2 failed; 0 ignored; 0 measured\n";
        let t = tally(out).expect("tally");
        assert_eq!(nums(&t), (15, 2, 1, 0));
        assert_eq!(t.runner, "cargo");
        assert_eq!(t.lines, 2);
        assert!(t.line.starts_with("test result: FAILED."));
    }

    #[test]
    fn pytest_with_and_without_rules() {
        let t = tally("....F\n4 passed, 1 failed in 0.3s\n").expect("tally");
        assert_eq!(nums(&t), (4, 1, 0, 0));
        let t = tally("===== 5 passed, 2 skipped, 1 error in 1.20s =====").expect("tally");
        assert_eq!(nums(&t), (5, 0, 2, 1));
        assert_eq!(t.runner, "pytest");
        assert!(tally("finished the job in 3s").is_none());
    }

    #[test]
    fn jest_and_vitest() {
        let t = tally("Test Suites: 1 failed, 1 total\nTests:       1 failed, 3 passed, 4 total\n")
            .expect("tally");
        assert_eq!(nums(&t), (3, 1, 0, 0));
        assert_eq!(t.runner, "jest");
        let t = tally(" Test Files  2 passed (2)\n      Tests  3 passed (3)\n").expect("tally");
        assert_eq!(nums(&t), (3, 0, 0, 0));
        assert_eq!(t.runner, "vitest");
        let t = tally("      Tests  1 failed | 3 passed (4)").expect("tally");
        assert_eq!(nums(&t), (3, 1, 0, 0));
    }

    #[test]
    fn node_test_tap() {
        let t = tally("# tests 60\n# suites 4\n# pass 59\n# fail 0\n# skipped 1\n# todo 0\n")
            .expect("tally");
        assert_eq!(nums(&t), (59, 0, 1, 0));
        assert_eq!(t.runner, "tap");
    }

    #[test]
    fn go_tallies_packages() {
        let t = tally("ok  \texample.com/a\t0.01s\nFAIL\texample.com/b\t0.02s\n?   \texample.com/c\t[no test files]\n").expect("tally");
        assert_eq!(nums(&t), (1, 1, 1, 0));
        assert_eq!(t.unit, "packages");
    }

    #[test]
    fn no_summary_is_no_tally() {
        assert!(tally("compiling...\nerror[E0425]: cannot find value").is_none());
    }
}
