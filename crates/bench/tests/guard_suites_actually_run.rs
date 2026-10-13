//! The four shell/python guard suites are RUN, by `cargo test`, or they are
//! not guards.
//!
//! # Why this file exists
//!
//! `scripts/pilot-report-coverage-test.py` holds 118 checks — every refusal in
//! the coverage generator, exercised as a subprocess, including the two
//! fail-opens closed in this batch. `scripts/pilot-run-guard-test.sh` holds 28,
//! covering the runner's credential and host-binary gates, the lock under
//! three-way concurrency with a mutation control, and — since ruling 6 — the
//! musl staging block and the fold, both of which sit past the prologue this
//! suite used to stop at. `scripts/compose-instrument-test.py` holds 140,
//! covering the patcher that writes the container setup chain.
//! `scripts/classify-workspace-test.py` holds 16, covering the classifier that
//! decides which tasks leave a coverage denominator — the last of the four to
//! be wired, and the hole this crate's gate spent a day naming rather than
//! closing.
//!
//! **Counts go stale.** They are here because a reader deserves a sense of what
//! is behind each name, not as an assertion — the suites' own output is the
//! measurement, and it is printed on failure.
//!
//! **None of them had a caller.** Not a CI job, not a `cargo test`, not another
//! script: a repository-wide search for their names found only their own
//! usage strings and a comment in `pilot-run.sh`. They ran when a human typed
//! the command and at no other time. (This sentence said "Neither" while two
//! suites were driven from here; the list has grown twice since, and a count
//! embedded in a word is the kind that goes stale without anyone noticing.)
//!
//! That is the third and fourth instance of the shape this batch exists to
//! catch, and the fourth was found by the machinery built for the first: the
//! split-enforcement census stopped counting `*-test.py` and `*-test.sh` as
//! shipped callers, which is what made "who runs these?" a question worth
//! asking. A test suite nobody runs is in the same state as a guard nobody
//! calls — green in every summary, and enforcing nothing.
//!
//! # These FAIL when the interpreter is missing; they do not skip
//!
//! `python3` and `bash` are hard dependencies of the pilot path already —
//! `pilot-run.sh` is bash and shells into `pilot-report.py` on every run. A
//! skip here would report "the guards are fine" on a machine that could not
//! check them, which is the fail-open inversion in miniature.
//!
//! The runner suite takes about 70 s: it stages a scratch host binary and
//! drives the script once per case. That is the price of watching the gates
//! that stand between a pilot run and money, and it is paid once per
//! `cargo test`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/bench -> repo root")
        .to_path_buf()
}

/// Runs a guard suite and fails with its whole output, which is the only form
/// in which one of these is diagnosable.
fn run_suite(program: &str, script: &str, what: &str) {
    let root = repo_root();
    let path = root.join(script);
    assert!(
        path.is_file(),
        "{script} is gone. It is the suite that watches {what}; deleting it is \
         not a refactor."
    );
    let out = Command::new(program)
        .arg(&path)
        .current_dir(&root)
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "could not run `{program} {script}`: {e}\n\
                 This test does NOT skip when the interpreter is missing. \
                 `{program}` is a hard dependency of the pilot path — \
                 scripts/pilot-run.sh is bash and shells into pilot-report.py \
                 on every run — so an unrunnable guard suite is an unchecked \
                 guard, and an unchecked guard must not report green."
            )
        });
    if !out.status.success() {
        panic!(
            "{script} FAILED (exit {:?}) — {what} is not holding.\n\
             \n--- stdout ---\n{}\n--- stderr ---\n{}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn the_coverage_generator_guard_suite_runs_and_passes() {
    run_suite(
        "python3",
        "scripts/pilot-report-coverage-test.py",
        "every refusal in scripts/pilot-report.py — the unread instrument-\
         failures set, the unattested and stale attestation, the empty \
         denominator, the vocabulary refusal that keeps an oracle number from \
         being called coverage, and the empty-capture predicate",
    );
}

#[test]
fn the_runner_guard_suite_runs_and_passes() {
    run_suite(
        "bash",
        "scripts/pilot-run-guard-test.sh",
        "scripts/pilot-run.sh's credential gate, its model allowlist and its \
         host-binary resolution — the gates that stand between a pilot run and \
         spending money",
    );
}

/// **The suite that reaches `scripts/classify-workspace.py`.**
///
/// Wired on 2026-09-06, and it closes the hole this crate's gate had been
/// naming in writing for a day: *"it still does not execute
/// classify-workspace.py, which has no suite."* Saying so in every gate
/// statement was better than hiding it and was never a substitute for closing
/// it.
///
/// The classifier decides which tasks leave a **coverage denominator** — the
/// same authority `pilot-report.py` and `instrument-failures.json` hold, and
/// both of those have been driven from here for a day. It works on constructed
/// task directories, so it needs no corpus, no network and no Docker, and it
/// cannot go red because someone re-provisioned 164 MB of fixtures.
#[test]
fn the_workspace_classifier_guard_suite_runs_and_passes() {
    run_suite(
        "python3",
        "scripts/classify-workspace-test.py",
        "scripts/classify-workspace.py — every stratum, the predicate pin's \
         refusal to emit a verdict when it drifts, the empty- and missing-corpus \
         refusals, and the calibration reported in BOTH directions",
    );
}

/// An interpreter that can import BOTH `ruamel.yaml` and `yaml`, or `None`.
///
/// `compose-instrument.py` requires ruamel and **exits 4 rather than falling
/// back** to a loader that strips the corpus's canary comments and accepts a
/// duplicate-key compose file. Its guard suite therefore needs the same
/// interpreter, and Apple's `/usr/bin/python3` has neither module.
///
/// The candidates are probed by IMPORTING, not by looking at a path: a venv
/// that exists and lacks the modules must not be selected on the strength of
/// its name.
fn python_with_yaml_loaders() -> Option<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    // The same variable `scripts/pilot-run.sh` reads, so an operator who has
    // pointed one at a provisioned tree has pointed both.
    if let Some(sp) = std::env::var_os("THRULINE_PILOT_SP") {
        candidates.push(PathBuf::from(sp).join("pilot/venv/bin/python"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".thruline-bench/pilot/venv/bin/python"));
    }
    candidates.push(PathBuf::from("python3"));

    candidates
        .into_iter()
        .find(|c| {
            Command::new(c)
                .args(["-c", "import ruamel.yaml, yaml"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        })?
        .to_str()
        .map(str::to_string)
}

/// **The suite that reaches `scripts/compose-instrument.py`.**
///
/// Wired on 2026-09-06 by operator ruling 6, and the gap it closes was found by
/// applying Track C's §4c to this crate's own gate: `cargo test -p
/// thruline-bench` reported 183 passing while executing **none** of
/// `compose-instrument.py`. The patcher generates the container setup chain —
/// the workspace resolution, the per-architecture binary selection, the shim
/// install and the pack assertion — so an unreached patcher is an unreached
/// instrument, and every capture number downstream is taken through it.
///
/// 140 checks run without a corpus and exercise the patcher unconditionally,
/// including `sh -n` over every generated chain. Point `THRULINE_TB_CORPUS` at
/// an `original-tasks` directory and three more run over all 241 tasks.
#[test]
fn the_compose_patcher_guard_suite_runs_and_passes() {
    let Some(py) = python_with_yaml_loaders() else {
        panic!(
            "no interpreter on this machine can import both `ruamel.yaml` and \
             `yaml`, so scripts/compose-instrument-test.py CANNOT RUN and the \
             compose patcher is UNCHECKED.\n\
             \n\
             This is a failure and not a skip, for the reason this file's \
             header gives: a skip would report `the guards are fine` on a \
             machine that could not check them. It is stated as an environment \
             gap rather than a code defect — nothing is wrong with the patcher, \
             and nothing here has verified that it is right either.\n\
             \n\
             Tried, in order: $THRULINE_PILOT_SP/pilot/venv/bin/python, \
             ~/.thruline-bench/pilot/venv/bin/python, python3.\n\
             \n\
             Provide one of them. `fixtures/bench/download.sh`'s pinned \
             Terminal-Bench archive ships a pyproject that hard-requires both, \
             so the pilot venv has them; any venv with `ruamel.yaml` and \
             `PyYAML` will do."
        )
    };
    run_suite(
        &py,
        "scripts/compose-instrument-test.py",
        "scripts/compose-instrument.py — the structural patcher that writes the \
         container setup chain: the workspace the repository is initialised in, \
         the per-architecture binary selection, the shim install and the \
         one-spool pack assertion. Every capture figure this programme reports \
         is taken through it",
    );
}
