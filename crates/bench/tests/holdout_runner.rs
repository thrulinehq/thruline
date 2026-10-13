//! `scripts/pilot-run.sh` must refuse a live-holdout task id.
//!
//! # The event these tests exist for
//!
//! `fixtures/terminal-bench-holdout-v1.tsv` was committed at 03:10:42 on
//! 2026-09-04. At 03:37:20 — twenty-seven minutes later — `pilot-run.sh` ran
//! `ancient-puzzle`, which was on the holdout side with no `observed` row, and
//! wrote a graded `results.json` for it. Nothing refused the run.
//!
//! The reason was not that a check was weak. **`pilot-run.sh` contained zero
//! occurrences of the word "holdout"**, and `pilot::plan`, `check_from_tune`
//! and `TerminalBenchSplit` had no caller outside this crate's own tests. The
//! whole anti-contamination construction was library code nothing shipping
//! called, which is the same shape as the coverage gate that was also wired to
//! nothing.
//!
//! # Why these drive the script instead of asserting on its text
//!
//! A grep for `holdout` in `pilot-run.sh` would have gone green the moment the
//! word appeared in a comment. These tests run the script and read its exit
//! code, its message and the manifest on disk, in all six directions —
//! including the two that must *not* refuse, because a guard that refuses
//! everything proves as little as one that refuses nothing.
//!
//! `crates/bench/tests/split_enforcement_census.rs` is the other half: it drives
//! the same guard, then drives a copy with the enforcement excised between its
//! mutation sentinels, and fails when a registered enforcement symbol is left
//! leaning on a refusal that no longer happens.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/bench -> repo root")
        .to_path_buf()
}

fn script() -> PathBuf {
    repo_root().join("scripts/pilot-run.sh")
}

/// A scratch `THRULINE_PILOT_SP` with a fake pinned dataset holding `tasks`.
///
/// The guard sits after the "does this task exist in the dataset" check, so a
/// task directory has to exist for the refusal under test to be the one that
/// fires. Nothing here reaches Docker: every assertion below is decided before
/// the preflight.
struct Sp {
    dir: tempfile::TempDir,
}

impl Sp {
    fn with(tasks: &[&str]) -> Sp {
        let dir = tempfile::tempdir().expect("tmp");
        for t in tasks {
            std::fs::create_dir_all(dir.path().join("pilot/tb-src/original-tasks").join(t))
                .expect("mkdir task");
        }
        Sp { dir }
    }

    fn manifest(&self) -> PathBuf {
        self.dir.path().join("holdout-spend.tsv")
    }

    /// Runs `pilot-run.sh` with `task`, `run_id` and any extra argv.
    fn run(&self, task: &str, run_id: &str, args: &[&str]) -> Output {
        self.run_with_split(task, run_id, args, &repo_root().join(SPLIT))
    }

    fn run_with_split(&self, task: &str, run_id: &str, args: &[&str], split: &Path) -> Output {
        Command::new("bash")
            .arg(script())
            .args(args)
            .arg("oracle")
            .env("THRULINE_PILOT_SP", self.dir.path())
            .env("PILOT_TASK_ID", task)
            .env("PILOT_RUN_ID", run_id)
            .env("PILOT_SPLIT_FILE", split)
            .output()
            .expect("run pilot-run.sh")
    }
}

const SPLIT: &str = "fixtures/terminal-bench-holdout-v1.tsv";

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

/// **Why a failure here is not what it looks like.**
///
/// Every assertion below used to print raw stderr and nothing else. On a tree
/// with the split guard removed, six of these seven tests go red — correctly —
/// and every one of them reads `pilot-run: tb not found at …/pilot/venv/bin/tb`.
/// A reader meeting that goes looking for a broken Terminal-Bench install.
///
/// There is no broken install. `tb` is absent **on purpose**: [`Sp`] stages a
/// scratch `THRULINE_PILOT_SP` with no pinned venv precisely so that nothing
/// here can reach Docker, and every case in this file is decided BEFORE the
/// preflight. So reaching the preflight at all is not the environment failing —
/// it is the run having sailed past the split guard without being refused, and
/// that is what the message must say.
fn diagnosis(o: &Output) -> String {
    let err = stderr(o);
    let reached_preflight = err.contains("tb not found");
    let guard_spoke = err.contains("REFUSED")
        || err.contains("REFUSING")
        || err.contains("DELIBERATE HOLDOUT SPEND")
        || err.contains("already spent");
    let head = if reached_preflight && !guard_spoke {
        concat!(
            "  CAUSE: the runner reached the Terminal-Bench PREFLIGHT, which means it got\n",
            "  PAST the split guard without saying anything. `tb not found` is EXPECTED in\n",
            "  these tests and is NOT the failure — the scratch THRULINE_PILOT_SP has no\n",
            "  pinned venv by design, so every case here is decided before the preflight.\n",
            "  Reaching it is the finding: THE SPLIT GUARD IN scripts/pilot-run.sh IS NOT\n",
            "  FIRING — deleted, moved out from between its mutation sentinels, or\n",
            "  short-circuited. See crates/bench/tests/split_enforcement_census.rs.\n\n",
        )
    } else {
        ""
    };
    format!("{head}  --- scripts/pilot-run.sh stderr ---\n{err}")
}

/// A live-holdout id and a tune id, read out of the committed split itself so
/// these tests cannot drift away from it by naming constants.
fn one_live_holdout_and_one_tune() -> (String, String) {
    use thruline_bench::pilot::TerminalBenchSplit;
    let s = TerminalBenchSplit::load().expect("the committed split must parse");
    let scratch = tempfile::tempdir().expect("tmp");
    let mut ledger =
        thruline_bench::HoldoutLedger::open(&scratch.path().join("spend")).expect("open");
    let m = ledger.spend(&s.split_hash(), "census").expect("spend");
    let held = s.holdout(&m).expect("released")[0].clone();
    // `tune` is `corpus \ holdout`, so pick one the hash also files as tune to
    // keep this case unambiguous — an observed holdout-side id is its own case.
    let tune = s
        .tune()
        .iter()
        .find(|t| !s.observed().iter().any(|o| &&o.task == t))
        .expect("a tune id that was never observed")
        .clone();
    (held, tune)
}

// ---------------------------------------------------------------------------
// The four failure directions the brief names, plus two the guard needs.
// ---------------------------------------------------------------------------

#[test]
fn a_holdout_id_with_no_flag_is_refused_by_name_and_side() {
    let (held, _) = one_live_holdout_and_one_tune();
    let sp = Sp::with(&[&held]);
    let out = sp.run(&held, "hr1", &[]);
    let err = stderr(&out);

    assert!(
        !out.status.success(),
        "a live holdout id ran with no flag. exit={:?}\n{}",
        out.status.code(),
        diagnosis(&out)
    );
    assert!(err.contains("REFUSED"), "{}", diagnosis(&out));
    assert!(
        err.contains(&held),
        "the refusal does not name the id:\n{}",
        diagnosis(&out)
    );
    assert!(
        err.contains("HOLDOUT side"),
        "the refusal does not name the side:\n{}",
        diagnosis(&out)
    );
    assert!(
        !sp.manifest().exists(),
        "a refused run wrote a spend manifest — a refusal that records a spend \
         would retire a holdout nobody spent"
    );
}

#[test]
fn a_holdout_id_with_the_flag_runs_and_leaves_a_manifest_naming_it() {
    let (held, _) = one_live_holdout_and_one_tune();
    let sp = Sp::with(&[&held]);
    let out = sp.run(&held, "hr2", &["--holdout-run"]);
    let err = stderr(&out);

    // It gets past the guard. It then dies at the preflight because this
    // scratch SP has no pinned Terminal-Bench venv, which is the *point*: the
    // guard is not what stopped it.
    assert!(
        err.contains("DELIBERATE HOLDOUT SPEND"),
        "the flag did not release the run:\n{}",
        diagnosis(&out)
    );
    assert!(
        !err.contains("REFUSED"),
        "the flag was accepted and the run still refused:\n{}",
        diagnosis(&out)
    );

    let m = sp.manifest();
    assert!(m.exists(), "the deliberate spend left no manifest");
    let text = std::fs::read_to_string(&m).expect("read manifest");
    assert!(
        text.lines()
            .any(|l| l.starts_with(&format!("run\t{held}\tholdout\t"))),
        "the manifest does not record the id that was spent:\n{text}"
    );
    // Written before the run, so a run that dies after this line still burns
    // the id. That is the safe direction for a holdout and it is deliberate.
    assert!(text.contains("hr2"), "the run-id is not recorded:\n{text}");
}

#[test]
fn a_second_run_of_the_same_holdout_id_is_refused_from_the_manifest() {
    let (held, _) = one_live_holdout_and_one_tune();
    let sp = Sp::with(&[&held]);
    let first = sp.run(&held, "hr3a", &["--holdout-run"]);
    assert!(
        stderr(&first).contains("DELIBERATE HOLDOUT SPEND"),
        "the first deliberate spend did not happen, so the second-spend refusal \
         under test was never armed:\n{}",
        diagnosis(&first)
    );

    // Same id, a fresh run-id, the flag again. The flag is permission to spend
    // a holdout once; it is not permission to spend the same one twice.
    let out = sp.run(&held, "hr3b", &["--holdout-run"]);
    let err = stderr(&out);
    assert!(
        !out.status.success(),
        "a second spend was allowed:\n{}",
        diagnosis(&out)
    );
    assert!(err.contains("ALREADY been spent"), "{}", diagnosis(&out));
    assert!(err.contains(&held), "{}", diagnosis(&out));

    let text = std::fs::read_to_string(sp.manifest()).expect("read");
    assert_eq!(
        text.lines().filter(|l| l.starts_with("run\t")).count(),
        1,
        "the refused second spend appended a row anyway:\n{text}"
    );
}

#[test]
fn a_tune_id_is_unaffected() {
    let (_, tune) = one_live_holdout_and_one_tune();
    let sp = Sp::with(&[&tune]);
    let out = sp.run(&tune, "hr4", &[]);
    let err = stderr(&out);

    assert!(
        !err.contains("REFUSED"),
        "a tune id was refused by the split guard:\n{}",
        diagnosis(&out)
    );
    assert!(
        !err.contains("holdout side"),
        "a tune id drew a holdout message:\n{}",
        diagnosis(&out)
    );
    assert!(
        !sp.manifest().exists(),
        "a tune run wrote a holdout spend manifest"
    );
    // Positive control that it really did run: it reaches the preflight, which
    // is the next gate after the guard.
    assert!(
        err.contains("tb not found") || String::from_utf8_lossy(&out.stdout).contains("preflight"),
        "the tune run did not get past the split guard at all:\n{}",
        diagnosis(&out)
    );
}

/// The inversion this program has already shipped once: a guard that cannot
/// read its input and lets the run through.
#[test]
fn a_missing_split_file_refuses_rather_than_falling_through_to_allow() {
    let (_, tune) = one_live_holdout_and_one_tune();
    let sp = Sp::with(&[&tune]);
    let missing = sp.dir.path().join("no-such-split.tsv");
    let out = sp.run_with_split(&tune, "hr5", &[], &missing);
    let err = stderr(&out);

    assert!(
        !out.status.success(),
        "a missing split file let the run proceed. THIS IS THE FAIL-OPEN \
         INVERSION: a split guard that allows when it cannot read its own input \
         reports as enforcement and enforces nothing.\n{}",
        diagnosis(&out)
    );
    assert!(err.contains("missing or unreadable"), "{}", diagnosis(&out));
    assert!(err.contains("REFUSING"), "{}", diagnosis(&out));
}

/// An unreadable file is a different syscall failure from a missing one, and a
/// `[ -f ]`-only check passes it.
#[test]
fn an_unreadable_or_empty_split_file_also_refuses() {
    let (_, tune) = one_live_holdout_and_one_tune();
    let sp = Sp::with(&[&tune]);

    // Empty: present, readable, and says nothing about any task.
    let empty = sp.dir.path().join("empty-split.tsv");
    std::fs::write(&empty, "# no task rows at all\n").expect("write");
    let out = sp.run_with_split(&tune, "hr6", &[], &empty);
    assert!(
        !out.status.success(),
        "an empty split allowed the run:\n{}",
        diagnosis(&out)
    );
    assert!(stderr(&out).contains("task rows"), "{}", diagnosis(&out));

    // Present and non-empty, but the id is not in it: no side can be derived,
    // and "no side" must not resolve to "tune".
    let other = sp.dir.path().join("other-corpus.tsv");
    std::fs::write(&other, "task\tsome-other-task\ttune\n").expect("write");
    let out = sp.run_with_split(&tune, "hr7", &[], &other);
    assert!(
        !out.status.success(),
        "a task absent from the split was treated as spendable:\n{}",
        diagnosis(&out)
    );
    assert!(
        stderr(&out).contains("NOT in the committed split"),
        "{}",
        diagnosis(&out)
    );
}

/// `ancient-puzzle` is the breach. It is holdout-side by hash — a fact that
/// cannot be edited, because `SplitError::AssignmentDrift` refuses an edited
/// assignment — and it now carries a `run-record` row, which is what actually
/// retires it. The runner must let it through *without* the flag, because there
/// is nothing left to protect.
#[test]
fn the_breached_task_is_retired_and_no_longer_needs_the_flag() {
    use thruline_bench::pilot::TerminalBenchSplit;
    let s = TerminalBenchSplit::load().expect("parse");
    assert!(
        s.tune().iter().any(|t| t == "ancient-puzzle"),
        "`ancient-puzzle` was run and must no longer be spendable holdout"
    );
    assert!(
        s.observed()
            .iter()
            .any(|o| o.task == "ancient-puzzle" && o.tier.is_execution()),
        "`ancient-puzzle` carries no execution-tier record of the run that spent it"
    );

    let sp = Sp::with(&["ancient-puzzle"]);
    let out = sp.run("ancient-puzzle", "hr8", &[]);
    let err = stderr(&out);
    assert!(
        !err.contains("REFUSED"),
        "a task that is already spent still demands the flag:\n{}",
        diagnosis(&out)
    );
    assert!(
        err.contains("already spent"),
        "the runner does not say why it let a holdout-side id through:\n{}",
        diagnosis(&out)
    );
    assert!(
        !sp.manifest().exists(),
        "an observed task wrote a spend row"
    );
}
