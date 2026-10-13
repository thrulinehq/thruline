//! BUILD-SPEC §3.3 — the `Check` record, built from a captured `exec`.
//!
//! A [`CheckDraft`] is what §3.3 calls a `Check` minus the fields only the
//! ledger can supply (`id`, `exec_event_id`, `input_tree_hash`, the hashes of
//! `argv`/`cwd`/`env`). This crate deliberately cannot reach a ledger, a
//! process or a clock, so a draft is a pure function of
//! `(argv, exit_code, output)` and is testable as one.
//!
//! # A check is evidence that a command ran, not that it passed
//!
//! `CheckDraft` carries a [`RunnerOutcome`], which is either parsed numbers or
//! `Unverifiable`. It carries **no verdict**: §6.3 is M1, and the one mistake
//! this file must make impossible is letting "a test command ran and exited 0"
//! stand in for "the tests pass". Exit code 0 with an unparseable output is a
//! check with an unverifiable result, and it must read that way downstream.
//!
//! # Recognition goes through the argv, and through `sh -c` payloads
//!
//! §13: *"capture is at exec level, not by pattern on the prompt; parse `sh -c`
//! payloads"*. [`crate::shell`] does the parsing and refuses anything it cannot
//! expand, so an obfuscated command yields "not recognised" rather than an
//! invented runner.

use crate::parse::{parse, Runner, RunnerOutcome};
use crate::shell::{shell_payload, ShellPayload};

/// §3.3 `category`.
pub const CATEGORY_TEST: &str = "test";
pub const CATEGORY_BUILD: &str = "build";
pub const CATEGORY_LINT: &str = "lint";
pub const CATEGORY_TYPECHECK: &str = "typecheck";

/// A non-test check: §6.4's build, lint and typecheck commands.
///
/// Deliberately separate from [`CheckDraft`]. A test check's evidence is its
/// *parsed counts*; a build check's evidence is its **exit code and nothing
/// else**, and giving it a `parsed` block would invite a reader to find zeros
/// in it and conclude something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCheck {
    pub category: &'static str,
    /// The tool as recognised: `cargo build`, `mypy`, `eslint`.
    pub tool: String,
    pub argv: Vec<String>,
    pub exit_code: i64,
    /// A flag that suppresses the very check being claimed. §6.4: *"warn on
    /// `--no-verify`-style bypasses or narrowed paths."*
    pub bypass: Option<String>,
}

/// §6.4's recognised commands, per ecosystem.
///
/// Matched on **argv**, never on a command string — the same rule as
/// [`Runner::from_argv`], for the same §13 reason. A first token plus an
/// optional subcommand is enough to name every one of them, and anything not
/// listed is not a check rather than a guess.
const TOOL_TABLE: &[(&str, &[&str], &str, &str)] = &[
    // (program, subcommands that qualify — empty means any, category, name)
    ("cargo", &["build", "check"], CATEGORY_BUILD, "cargo build"),
    ("cargo", &["clippy"], CATEGORY_LINT, "cargo clippy"),
    ("cargo", &["fmt"], CATEGORY_LINT, "cargo fmt"),
    ("go", &["build"], CATEGORY_BUILD, "go build"),
    ("go", &["vet"], CATEGORY_LINT, "go vet"),
    ("gradle", &["build"], CATEGORY_BUILD, "gradle build"),
    (
        "mvn",
        &["compile", "package", "verify"],
        CATEGORY_BUILD,
        "mvn",
    ),
    ("dotnet", &["build"], CATEGORY_BUILD, "dotnet build"),
    ("tsc", &[], CATEGORY_TYPECHECK, "tsc"),
    ("mypy", &[], CATEGORY_TYPECHECK, "mypy"),
    ("pyright", &[], CATEGORY_TYPECHECK, "pyright"),
    ("eslint", &[], CATEGORY_LINT, "eslint"),
    ("ruff", &[], CATEGORY_LINT, "ruff"),
    ("flake8", &[], CATEGORY_LINT, "flake8"),
    ("black", &[], CATEGORY_LINT, "black"),
];

/// Flags that make a check check less than it appears to.
const BYPASS_FLAGS: &[&str] = &[
    "--no-verify",
    "--no-validate",
    "--skip-tests",
    "--ignore-errors",
    "--exit-zero",
    "--no-error-on-unmatched-pattern",
];

/// Is this exec a §6.4 build, lint or typecheck command?
///
/// `npm`/`npx`/`yarn` wrappers resolve through their own argv (`npx tsc`), and
/// a shell payload is parsed rather than matched.
pub fn tool_check_from_exec(argv: &[String], exit_code: i64) -> Option<ToolCheck> {
    if let Some(check) = direct_tool_check(argv, exit_code) {
        return Some(check);
    }
    // Through a runner wrapper: `npx tsc --noEmit`, `npm run lint -- --fix`.
    if let Some(first) = argv.first() {
        let base = first.rsplit('/').next().unwrap_or(first);
        if matches!(base, "npx" | "npm" | "pnpm" | "yarn") {
            let inner: Vec<String> = argv
                .iter()
                .skip(1)
                .skip_while(|a| a.starts_with('-') || matches!(a.as_str(), "run" | "exec" | "--"))
                .cloned()
                .collect();
            if !inner.is_empty() {
                if let Some(check) = direct_tool_check(&inner, exit_code) {
                    return Some(check);
                }
            }
        }
    }
    // Inside a shell payload.
    if let ShellPayload::Parsed(commands) = shell_payload(argv) {
        let found: Vec<ToolCheck> = commands
            .iter()
            .filter_map(|c| direct_tool_check(&c.argv, exit_code))
            .collect();
        // One exit code cannot be attributed to two commands, for the same
        // reason two test runners in one exec are refused.
        if found.len() == 1 {
            return found.into_iter().next();
        }
    }
    None
}

fn direct_tool_check(argv: &[String], exit_code: i64) -> Option<ToolCheck> {
    let program = argv.first()?;
    let base = program.rsplit('/').next().unwrap_or(program);
    let sub = argv
        .iter()
        .skip(1)
        .find(|a| !a.starts_with('-'))
        .map(String::as_str);
    for (prog, subs, category, name) in TOOL_TABLE {
        if base != *prog {
            continue;
        }
        if !subs.is_empty() && !sub.is_some_and(|s| subs.contains(&s)) {
            continue;
        }
        return Some(ToolCheck {
            category,
            tool: (*name).to_string(),
            argv: argv.to_vec(),
            exit_code,
            bypass: argv
                .iter()
                .find(|a| BYPASS_FLAGS.contains(&a.to_ascii_lowercase().as_str()))
                .cloned(),
        });
    }
    None
}

/// A §3.3 `Check`, minus the fields only the store can fill in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckDraft {
    pub category: &'static str,
    pub runner: Runner,
    /// The argv the runner itself was invoked with. For a shell payload this
    /// is the inner command, not the `sh -c …` wrapper — otherwise `argv_hash`
    /// would commit to the wrapper and two identical runs behind different
    /// shells would look like different checks.
    pub runner_argv: Vec<String>,
    /// The runner was found inside a shell payload rather than executed
    /// directly. Recorded because it changes what the exec's output contains:
    /// the payload's other commands wrote to the same stream.
    pub via_shell: bool,
    pub exit_code: i64,
    pub outcome: RunnerOutcome,
}

/// Whether an exec is a check at all.
///
/// **Not `Option<CheckDraft>`.** The reason a command was not recognised is
/// itself worth storing — "this ran no runner I know" and "this ran something I
/// could not parse" are different facts, and only the first is evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckOutcome {
    Check(Box<CheckDraft>),
    NotACheck { reason: String },
}

impl CheckOutcome {
    pub fn draft(&self) -> Option<&CheckDraft> {
        match self {
            CheckOutcome::Check(d) => Some(d),
            CheckOutcome::NotACheck { .. } => None,
        }
    }
}

/// Build a check from one captured exec.
///
/// `output` is the recorder's combined, already-redacted stdout+stderr.
pub fn check_from_exec(argv: &[String], exit_code: i64, output: &str) -> CheckOutcome {
    // 1. The command is the runner.
    if let Some(runner) = Runner::from_argv(argv) {
        return CheckOutcome::Check(Box::new(CheckDraft {
            category: CATEGORY_TEST,
            runner,
            runner_argv: argv.to_vec(),
            via_shell: false,
            exit_code,
            outcome: parse(Some(runner), output),
        }));
    }

    // 2. The command is a shell carrying the runner.
    match shell_payload(argv) {
        ShellPayload::NotShell => CheckOutcome::NotACheck {
            reason: format!(
                "no runner recognised in {:?}; §6.2 parsers exist for pytest and \
                 cargo test",
                argv.first().map(String::as_str).unwrap_or("")
            ),
        },
        ShellPayload::Unparsed { reason, .. } => CheckOutcome::NotACheck {
            // Loud, per Rule 12: this is not "no runner", it is "unreadable".
            reason: format!("the shell payload could not be parsed, so no runner can be attributed to it: {reason}"),
        },
        ShellPayload::Parsed(commands) => {
            let found: Vec<(Runner, Vec<String>)> = commands
                .into_iter()
                .filter_map(|c| Runner::from_argv(&c.argv).map(|r| (r, c.argv)))
                .collect();
            match found.len() {
                0 => CheckOutcome::NotACheck {
                    reason: "the shell payload parsed, and none of its commands is a \
                             recognised runner"
                        .to_string(),
                },
                1 => {
                    let (runner, runner_argv) = match found.into_iter().next() {
                        Some(pair) => pair,
                        // Unreachable: len() == 1. An error rather than a
                        // default, because a defaulted check is a fabricated
                        // one.
                        None => {
                            return CheckOutcome::NotACheck {
                                reason: "internal: a single recognised runner vanished"
                                    .to_string(),
                            }
                        }
                    };
                    CheckOutcome::Check(Box::new(CheckDraft {
                        category: CATEGORY_TEST,
                        runner,
                        runner_argv,
                        via_shell: true,
                        exit_code,
                        outcome: parse(Some(runner), output),
                    }))
                }
                // Two runs, one output stream, one exit code. Attributing the
                // numbers to either would be a guess, and attributing the exit
                // code to both would let a passing run cover a failing one —
                // which is the product's central failure mode committed by the
                // product. The shim captured each nested run separately; those
                // are the evidence.
                n => CheckOutcome::NotACheck {
                    reason: format!(
                        "{n} recognised runners share this exec's single output and \
                         exit code, so neither can be attributed; use the nested \
                         exec events the shim recorded for each"
                    ),
                },
            }
        }
    }
}

#[cfg(test)]
mod tests;
