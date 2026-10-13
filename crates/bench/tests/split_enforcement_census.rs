//! Split enforcement must be reachable from something that ships **and firing**.
//! This census drives it and fails when it stops.
//!
//! # The state this exists to catch, stated exactly
//!
//! On 2026-09-04 the committed holdout split was breached twenty-seven minutes
//! after it was committed: `fixtures/terminal-bench-holdout-v1.tsv` landed at
//! 03:10:42, and at 03:37:20 `scripts/pilot-run.sh` produced a graded
//! `results.json` for `ancient-puzzle` — holdout side, no `observed` row.
//! Every guard that should have stopped it existed, was tested, and was green.
//! **None of it had a caller outside this crate's own tests.**
//!
//! # The state THIS FILE was in, which is the same failure one level up
//!
//! The first version of this census keyed on **text**, and it did not work.
//! Measured, not argued: with the entire split guard deleted from
//! `scripts/pilot-run.sh` — 135 lines, every refusal, the whole block — this
//! census still passed 6 of 6 and exited 0. Three separate reasons, all of them
//! the same reason:
//!
//! 1. The fixture-consumer check needed only the **basename** to appear on one
//!    executable line. A single surviving `SPLIT_FILE=…` assignment, by then a
//!    dead variable nothing read, satisfied it.
//! 2. The "a consumer must refuse" checks tested `["REFUSED", "REFUSING",
//!    "die ", "exit 1"]` against **whole-file text**. `pilot-run.sh` carries 44
//!    unrelated `die `/`exit 1` occurrences, so those checks passed on every
//!    possible version of the file, gutted or not.
//! 3. `MAX_UNWIRED` capped the exception list, but **nothing floored
//!    `ENFORCEMENT`**. Deleting a `Site` row silenced the census for that
//!    symbol with no test going red.
//!
//! A census that greps for words is a census of vocabulary. So this one keys on
//! **behaviour**: it runs the shipped runner against a live-holdout id and
//! watches the refusal happen, then runs the same script with the enforcement
//! **excised between its mutation sentinels** and watches the refusal stop. The
//! first half proves the guard fires. The second half proves the refusal came
//! from *the guard* and not from somewhere else — without it, a script that
//! refused everything would look identical to one that enforces the split.
//!
//! # What is still read as text, and why that is sound now
//!
//! Text answers exactly one question here: *does any shipped file name this
//! symbol?* That is a **caller search**, and its only job is to make a
//! registered exception go stale the day the symbol acquires a real caller. No
//! refusal, no coverage and no enforcement is inferred from text any more.
//!
//! The executable surface was also wrong and is corrected. It accepted only
//! `scripts/`, `*/main.rs`, `/src/bin/` and `*.sh`, so **all 21 modules of the
//! shipped `thruline` binary were invisible** — including
//! `crates/cli/src/pilot_cmd.rs`, which `main.rs` reaches and which
//! `pilot-run.sh` shells back into for the money ceiling. That file alone
//! refutes the old register's stated rationale ("a shell script cannot call a
//! Rust function"): a shell script calls Rust functions in this repository
//! today, through a subcommand. The surface is now **derived** — every `.rs`
//! under a crate that actually builds a binary — rather than pattern-matched,
//! and test drivers under `scripts/` (`*-test.py`, `*-test.sh`) are excluded,
//! because a test that drives a flag is not a shipped caller of it. That
//! exclusion is what surfaces the `--coverage` entry below.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

// ---------------------------------------------------------------------------
// The register.
// ---------------------------------------------------------------------------

/// How a site's enforcement is proven to be connected.
enum Cover {
    /// A shipped caller names the symbol, AND a probe watches the enforcement
    /// fire, with a mutation control proving the refusal came from it.
    Driven { probe: &'static str },
    /// No shipped caller. Registered — with a reason, with the **hazard it
    /// leaves open** (a reason without a hazard is how an exception list turns
    /// into a permission slip), and **still required to name a probe**.
    ///
    /// That last part is the type doing the work a test used to fail at. There
    /// is no way to spell "registered and watched by nothing": every variant
    /// carries a probe, so an entry cannot be added that is merely excused.
    Unwired {
        why: &'static str,
        hazard: &'static str,
        probe: &'static str,
    },
}

impl Cover {
    /// The probe in [`Observed`] that decides whether this site's enforcement
    /// is actually happening.
    fn probe(&self) -> &'static str {
        match self {
            Cover::Driven { probe } => probe,
            Cover::Unwired { probe, .. } => probe,
        }
    }
}

/// A split-enforcement symbol that must be reachable from something that ships.
struct Site {
    /// The distinctive text an external caller must write. Qualified where the
    /// bare name is a common English word: `plan` appears in prose, and a
    /// census that counts prose as a caller is worse than no census.
    symbol: &'static str,
    /// Where it is defined. Its own definition is not a call site.
    defined_in: &'static str,
    /// What it refuses, in one line.
    enforces: &'static str,
    /// Driven, or registered as unwired.
    cover: &'static Cover,
}

/// The probe that drives `scripts/pilot-run.sh` against a live-holdout id.
const SPLIT_GUARD: &str = "split-guard";
/// The probe that drives `scripts/pilot-report.py --coverage`.
const COVERAGE_FLAG: &str = "coverage-flag";

/// The reason every one of the six split symbols is unwired is the same single
/// fact, so it is stated once rather than paraphrased six times.
const NO_RUST_DRIVER: Cover = Cover::Unwired {
    why: "there is still no `thruline bench` SUBCOMMAND that stages and runs a \
          task — `BenchArgs` is flat and `bench_cmd::run` always exits 3. What \
          has changed, and what this text said until 2026-09-05, is the claim \
          that no Rust entry point stages a task at all: \
          `thruline_bench::drive::prepare` now does, and it calls these symbols \
          itself (TerminalBenchSplit::load, check_against_corpus, \
          tune_protocol, check_from_tune, HoldoutLedger::spend). It is not yet \
          reachable from anything that ships, because crates/bench declares no \
          [[bin]] and `is_executable_surface` therefore cannot see a library \
          module — so the caller search below will NOT go stale on its own when \
          `drive` lands. It goes stale when crates/cli dispatches to it, which \
          is filed as R1 in docs/tracks/E/INTERFACE-REQUESTS.md",
    hazard: "TWO runners now enforce this split and only one of them is watched \
             here. The shell guard in scripts/pilot-run.sh is driven by the \
             `split-guard` probe every run; `drive`'s refusal of a live-holdout \
             id is proven only by crates/bench/tests/drive.rs, which this census \
             does not execute and cannot vouch for. That gap is stated rather \
             than papered over: a second runner was the exact hazard the \
             previous text named, it has arrived, and the automatic staleness \
             check does not fire for it. Delete these entries the day \
             `thruline bench drive` is dispatched from the CLI, and give them \
             the drive probe then",
    probe: SPLIT_GUARD,
};

const ENFORCEMENT: &[Site] = &[
    Site {
        symbol: "TerminalBenchSplit",
        defined_in: "crates/bench/src/pilot.rs",
        enforces: "re-derives all 241 assignments from bucket(salt,id) < cut and \
                   refuses a hand-edited side (SplitError::AssignmentDrift)",
        cover: &NO_RUST_DRIVER,
    },
    Site {
        symbol: "TERMINAL_BENCH_SPLIT",
        defined_in: "crates/bench/src/pilot.rs",
        enforces: "the compiled-in manifest bytes — pinned rather than read from \
                   a path a caller could point elsewhere",
        cover: &NO_RUST_DRIVER,
    },
    Site {
        symbol: "pilot::plan",
        defined_in: "crates/bench/src/pilot.rs",
        enforces: "draws only from tune and refuses a protocol with an empty \
                   holdout (PilotError::UncommittedHoldout)",
        cover: &NO_RUST_DRIVER,
    },
    Site {
        symbol: "check_from_tune",
        defined_in: "crates/bench/src/pilot.rs",
        enforces: "refuses a pilot whose tasks reach into the committed holdout",
        cover: &NO_RUST_DRIVER,
    },
    Site {
        symbol: "tune_protocol",
        defined_in: "crates/bench/src/pilot.rs",
        enforces: "the only sanctioned way to seed a Protocol from the split: \
                   tasks come from tune, so a sampler cannot reach a holdout id",
        cover: &NO_RUST_DRIVER,
    },
    Site {
        symbol: "HoldoutLedger",
        defined_in: "crates/bench/src/protocol.rs",
        enforces: "makes reading the holdout names an append-only spend and \
                   refuses the second read",
        cover: &NO_RUST_DRIVER,
    },
    // -----------------------------------------------------------------------
    // THE THIRD INSTANCE OF THE SAME FAMILY, registered rather than left where
    // this whole line of work exists to catch it.
    // -----------------------------------------------------------------------
    Site {
        symbol: "--coverage",
        defined_in: "scripts/pilot-report.py",
        enforces: "refuses to emit any coverage figure it has not read AND \
                   matched an attested instrument-failures set for (exits 8/11/ \
                   12), and refuses to call an oracle number `coverage` at all",
        cover: &Cover::Unwired {
            why: "scripts/pilot-run.sh:~1428 invokes pilot-report.py in per-task \
                  mode with no flag, and it is right not to: coverage is a \
                  figure over a run SET and that script runs ONE task. Called \
                  per task it would either refuse for want of an attestation or \
                  emit a 1-run denominator, and a 1-run denominator is worse \
                  than no call. No shipped surface aggregates a run set today",
            hazard: "an operator can still compute a coverage-shaped number by \
                     hand and publish it WITHOUT this tool — which is exactly \
                     what happened to the first chain-coverage figure: it came \
                     out of an ad-hoc aggregator, not out of a bypassed guard. \
                     This entry does not close that; nothing in a repository \
                     can. What it does is refuse to let the flag sit unwatched: \
                     the `coverage-flag` probe drives it every run and fails if \
                     it stops dispatching or stops refusing",
            probe: COVERAGE_FLAG,
        },
    },
];

/// **The floor the first version did not have.** `MAX_UNWIRED` capped the
/// exception list, and nothing at all stopped a `Site` row being deleted:
/// removing one silenced the census for that symbol and every test stayed
/// green. Lowering this is a deliberate edit with a diff, the same way raising
/// `MAX_UNWIRED` is.
const ENFORCEMENT_FLOOR: usize = 7;

/// Raising this is a deliberate edit with a diff, which is the only protection
/// an exception list has against becoming the default.
const MAX_UNWIRED: usize = 7;

/// The artefact the enforcement is about.
const SPLIT_FIXTURE: &str = "terminal-bench-holdout-v1.tsv";

/// The shipped surface the split guard lives in.
const RUNNER: &str = "scripts/pilot-run.sh";

/// The mutation boundary inside [`RUNNER`]. Excising everything between these
/// is how the census proves the refusal it watched is load-bearing.
const GUARD_BEGIN: &str = "# >>> THRULINE-SPLIT-GUARD-BEGIN >>>";
const GUARD_END: &str = "# <<< THRULINE-SPLIT-GUARD-END <<<";

// ---------------------------------------------------------------------------
// What the census learned by DRIVING. Not text.
// ---------------------------------------------------------------------------

/// Every field is a fact obtained by running something and reading what it did.
/// `Ok(())` means the property held; `Err(why)` carries the observation that
/// refutes it, so the census's message can name a cause rather than a rule.
struct Observed {
    /// The shipped runner refused a live-holdout id, naming the id and the side.
    guard_fires: Result<(), String>,
    /// The same runner, with the guard excised between its sentinels, STOPPED
    /// refusing — so the refusal above came from the guard.
    guard_is_load_bearing: Result<(), String>,
    /// `pilot-report.py --coverage` dispatches to the coverage path and refuses
    /// rather than falling through to the per-task path or emitting a figure.
    coverage_flag_dispatches: Result<(), String>,
}

impl Observed {
    /// Everything held. Used by the negative controls, which vary one fact.
    fn all_good() -> Observed {
        Observed {
            guard_fires: Ok(()),
            guard_is_load_bearing: Ok(()),
            coverage_flag_dispatches: Ok(()),
        }
    }

    fn probe(&self, name: &str) -> &Result<(), String> {
        match name {
            SPLIT_GUARD => &self.guard_fires,
            COVERAGE_FLAG => &self.coverage_flag_dispatches,
            other => panic!("no probe named `{other}` — a Site names a probe that does not exist"),
        }
    }
}

// ---------------------------------------------------------------------------
// The probes.
// ---------------------------------------------------------------------------

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crates/bench -> repo root")
        .to_path_buf()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

/// One live-holdout id, read out of the committed split itself so this cannot
/// drift away from it by naming a constant.
fn a_live_holdout_id() -> String {
    use thruline_bench::pilot::TerminalBenchSplit;
    let s = TerminalBenchSplit::load().expect("the committed split must parse");
    let scratch = tempfile::tempdir().expect("tmp");
    let mut ledger =
        thruline_bench::HoldoutLedger::open(&scratch.path().join("spend")).expect("open");
    let m = ledger.spend(&s.split_hash(), "census").expect("spend");
    s.holdout(&m).expect("released")[0].clone()
}

/// Runs `script` with a live-holdout id and no `--holdout-run`.
///
/// `THRULINE_PILOT_SP` is a scratch directory with the task staged and **no
/// pinned venv**, so the run cannot reach Docker: past the guard it dies at the
/// preflight, on a `tb` path inside the temp directory that never exists.
fn drive_runner(script: &Path, task: &str, run_id: &str) -> (Output, tempfile::TempDir) {
    let sp = tempfile::tempdir().expect("tmp");
    std::fs::create_dir_all(sp.path().join("pilot/tb-src/original-tasks").join(task))
        .expect("mkdir task");
    let out = Command::new("bash")
        .arg(script)
        .arg("oracle")
        .env("THRULINE_PILOT_SP", sp.path())
        .env("PILOT_TASK_ID", task)
        .env("PILOT_RUN_ID", run_id)
        .env(
            "PILOT_SPLIT_FILE",
            repo_root().join("fixtures").join(SPLIT_FIXTURE),
        )
        .output()
        .expect("run the pilot runner");
    (out, sp)
}

/// **Probe 1a.** The shipped runner must refuse a live-holdout id.
fn probe_guard_fires(task: &str) -> Result<(), String> {
    let (out, _sp) = drive_runner(&repo_root().join(RUNNER), task, "census-fires");
    let err = stderr(&out);
    if out.status.success() {
        return Err(format!(
            "{RUNNER} EXITED 0 on live-holdout id `{task}`. It said:\n      {}",
            first_lines(&err, 4)
        ));
    }
    if !err.contains("REFUSED") || !err.contains("HOLDOUT side") || !err.contains(task) {
        return Err(format!(
            "{RUNNER} exited {:?} on live-holdout id `{task}` but NOT with the \
             split refusal (it must say REFUSED, name the id, and name the \
             HOLDOUT side). It said:\n      {}",
            out.status.code(),
            first_lines(&err, 4)
        ));
    }
    Ok(())
}

/// **Probe 1b — the negative control, and the half the text census had no
/// equivalent of.** Excise the guard between its sentinels and drive the copy:
/// it must now get past and die downstream. If it still refuses, the refusal
/// probe 1a watched did not come from the guard, and 1a proves nothing.
fn probe_guard_is_load_bearing(task: &str) -> Result<(), String> {
    let src_path = repo_root().join(RUNNER);
    let src =
        std::fs::read_to_string(&src_path).map_err(|e| format!("cannot read {RUNNER}: {e}"))?;
    let (Some(b), Some(e)) = (src.find(GUARD_BEGIN), src.find(GUARD_END)) else {
        return Err(format!(
            "{RUNNER} has no mutation boundary. The census excises everything \
             between `{GUARD_BEGIN}` and `{GUARD_END}` to prove the refusal it \
             watches is load-bearing, and it cannot find them. Either the \
             sentinels were deleted, or the guard was moved out from between \
             them. Without this control a script that refused EVERYTHING would \
             read as split enforcement."
        ));
    };
    if b >= e {
        return Err(format!(
            "{RUNNER}'s mutation sentinels are inverted (`{GUARD_END}` precedes \
             `{GUARD_BEGIN}`), so the census cannot excise the guard."
        ));
    }
    let mut mutant = String::with_capacity(src.len());
    mutant.push_str(&src[..b]);
    mutant.push_str(&src[e + GUARD_END.len()..]);
    let removed = src[b..e].lines().count();

    let dir = tempfile::tempdir().expect("tmp");
    let mpath = dir.path().join("pilot-run-mutant.sh");
    std::fs::write(&mpath, &mutant).expect("write mutant");

    let (out, _sp) = drive_runner(&mpath, task, "census-mutant");
    let err = stderr(&out);
    if err.contains("REFUSED") && err.contains("HOLDOUT side") {
        return Err(format!(
            "{RUNNER} STILL refuses live-holdout id `{task}` with {removed} lines \
             of split guard excised. The refusal the census watched therefore \
             does not come from the guard, and watching it proves nothing about \
             split enforcement. It said:\n      {}",
            first_lines(&err, 4)
        ));
    }
    Ok(())
}

/// **Probe 2.** `pilot-report.py --coverage` must dispatch to the coverage path
/// and refuse — not fall through to per-task mode, and not emit a figure.
///
/// Driven over an EMPTY pilot directory, whose only correct answer is the
/// no-denominator refusal.
fn probe_coverage_flag() -> Result<(), String> {
    let dir = tempfile::tempdir().expect("tmp");
    let out = Command::new("python3")
        .arg(repo_root().join("scripts/pilot-report.py"))
        .args(["--coverage", "--pilot-dir"])
        .arg(dir.path())
        .args(["--run-prefix", "census-no-such-prefix-"])
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", dir.path())
        .output()
        .map_err(|e| format!("cannot run scripts/pilot-report.py: {e}"))?;
    let err = stderr(&out);
    let so = String::from_utf8_lossy(&out.stdout).to_string();
    if out.status.success() {
        return Err(format!(
            "`pilot-report.py --coverage` EXITED 0 over an empty pilot \
             directory. It must refuse: a figure over no runs is not a figure. \
             stdout:\n      {}",
            first_lines(&so, 3)
        ));
    }
    if so.contains("COVERAGE REPORT") || so.contains("INSTRUMENT-REACH REPORT") {
        return Err(format!(
            "`pilot-report.py --coverage` printed a report over an empty pilot \
             directory:\n      {}",
            first_lines(&so, 3)
        ));
    }
    // Exit 1 is what a Python traceback exits with, and per-task mode would
    // KeyError on the absent MOUNT env var. Either means the flag did not
    // dispatch to `coverage_main` at all.
    if out.status.code() == Some(1) || err.contains("Traceback") {
        return Err(format!(
            "`pilot-report.py --coverage` did not dispatch to the coverage \
             path: it exited {:?} with a traceback, which is what per-task mode \
             does when MOUNT is unset. The `--coverage` dispatch is gone.\n      {}",
            out.status.code(),
            first_lines(&err, 4)
        ));
    }
    if !err.contains("no run records") {
        return Err(format!(
            "`pilot-report.py --coverage` exited {:?} over an empty pilot \
             directory without saying there are no run records:\n      {}",
            out.status.code(),
            first_lines(&err, 4)
        ));
    }
    Ok(())
}

fn first_lines(s: &str, n: usize) -> String {
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .take(n)
        .collect::<Vec<_>>()
        .join("\n      ")
}

// ---------------------------------------------------------------------------
// The walk. Text answers exactly one question: does a shipped file NAME this?
// ---------------------------------------------------------------------------

/// Crates that actually build a binary, derived from the manifests rather than
/// listed. Every `.rs` under such a crate's `src/` ships inside that binary.
fn binary_crates(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for base in ["crates", "services"] {
        let Ok(entries) = std::fs::read_dir(root.join(base)) else {
            continue;
        };
        for c in entries.flatten() {
            let dir = c.path();
            let manifest = dir.join("Cargo.toml");
            let has_bin = dir.join("src/main.rs").is_file()
                || std::fs::read_to_string(&manifest)
                    .map(|t| t.lines().any(|l| l.trim() == "[[bin]]"))
                    .unwrap_or(false);
            if has_bin {
                if let Some(name) = dir.file_name() {
                    out.push(format!("{base}/{}/src/", name.to_string_lossy()));
                }
            }
        }
    }
    out.sort();
    out
}

/// A file the product executes.
///
/// Three corrections over the version that did not work:
/// * every `.rs` under a crate that BUILDS A BINARY counts, because it ships
///   inside that binary. The old rule saw only `main.rs`, so the 21 other
///   modules of the `thruline` binary — `pilot_cmd.rs` among them — were
///   invisible, and the register drew a false conclusion from their absence.
/// * a library crate's `src/` still does not count. `crates/bench` builds no
///   binary, and `crates/bench/src/pilot.rs` `include_str!`s the fixture, so
///   accepting any `src/` would make this census green on the commit that lost
///   the holdout.
/// * a **test driver** under `scripts/` does not count. `pilot-report-coverage-
///   test.py` runs `--coverage` as a subprocess; that is a watcher, not a
///   shipped caller, and counting it would excuse the flag from the very
///   question this file asks.
fn is_executable_surface(rel: &str, bin_crate_srcs: &[String]) -> bool {
    if is_test_code(rel) {
        return false;
    }
    rel.starts_with("scripts/")
        || rel.ends_with("/main.rs")
        || rel.contains("/src/bin/")
        || rel.ends_with(".sh")
        || bin_crate_srcs.iter().any(|p| rel.starts_with(p))
}

fn is_test_code(rel: &str) -> bool {
    rel.contains("/tests/")
        || rel.starts_with("tests/")
        || rel.ends_with("tests.rs")
        || rel.contains("/benches/")
        // Test drivers living beside the scripts they drive.
        || rel.ends_with("-test.py")
        || rel.ends_with("-test.sh")
        || rel.ends_with("_test.py")
        || rel.ends_with("_test.sh")
}

/// Removes `#[cfg(test)]` blocks by brace matching, keeping everything after.
///
/// Truncating at the first one is wrong and has been measured wrong in this
/// repository before: files carry inline test modules long before the shipped
/// code that matters.
fn strip_test_modules(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("#[cfg(test)]") {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let Some(open) = after.find('{') else {
            break;
        };
        if after[..open].contains(';') {
            rest = &after[after.find(';').expect("checked") + 1..];
            continue;
        }
        let mut depth = 0usize;
        let mut end = None;
        for (i, b) in after.as_bytes().iter().enumerate().skip(open) {
            match b {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(i + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        match end {
            Some(e) => rest = &after[e..],
            None => break,
        }
    }
    out.push_str(rest);
    out
}

/// `(repo-relative path, shipped text)` for every Rust source and every script.
///
/// `packaging/` and `.github/` are **not walked**, and the omission is
/// deliberate rather than an oversight: they are owned by another worker in
/// this batch, and a census whose result depends on a concurrently edited tree
/// reports someone else's timing as this repository's state.
fn shipped() -> Vec<(String, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else {
                out.push(p);
            }
        }
    }

    let root = repo_root();
    let mut files: Vec<PathBuf> = Vec::new();
    for base in ["crates", "services"] {
        let Ok(entries) = std::fs::read_dir(root.join(base)) else {
            continue;
        };
        for c in entries.flatten() {
            let src = c.path().join("src");
            if src.is_dir() {
                walk(&src, &mut files);
            }
        }
    }
    walk(&root.join("scripts"), &mut files);
    files.sort();

    let mut out = Vec::new();
    for p in files {
        let rel = p
            .strip_prefix(&root)
            .expect("prefix")
            .to_string_lossy()
            .replace('\\', "/");
        let ext_ok = p
            .extension()
            .is_some_and(|x| x == "rs" || x == "sh" || x == "py");
        if !ext_ok || is_test_code(&rel) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        let text = if rel.ends_with(".rs") {
            strip_test_modules(&text)
        } else {
            text
        };
        out.push((rel, text));
    }
    out
}

// ---------------------------------------------------------------------------
// The census itself: a pure function, so the negative controls can drive it.
// ---------------------------------------------------------------------------

fn names(text: &str, needle: &str) -> bool {
    text.lines()
        .filter(|l| {
            let t = l.trim_start();
            !t.starts_with("//") && !t.starts_with('#') && !t.starts_with("*")
        })
        .any(|l| {
            if !needle.starts_with("--") {
                return l.contains(needle);
            }
            // Options are tokens: --coverage-reporter belongs to another
            // command and is not a caller of pilot-report.py --coverage.
            let option_char = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
            l.match_indices(needle).any(|(at, matched)| {
                !l[..at].chars().next_back().is_some_and(option_char)
                    && !l[at + matched.len()..]
                        .chars()
                        .next()
                        .is_some_and(option_char)
            })
        })
}

#[test]
fn option_census_distinguishes_complete_flags_from_shared_prefixes() {
    assert!(!names(
        "parser.add_argument('--coverage-reporter')",
        "--coverage"
    ));
    assert!(!names("run --coverage_suffix", "--coverage"));
    assert!(names("run --coverage --pilot-dir tmp", "--coverage"));
    assert!(names("['--coverage', '--pilot-dir']", "--coverage"));
    assert!(names("run --coverage=true", "--coverage"));
}

/// Every way split enforcement can be present and not connected.
fn census(files: &[(String, String)], sites: &[Site], observed: &Observed) -> Vec<String> {
    let mut problems = Vec::new();

    // --- 0. The register itself. A row that can vanish is not a register. ---
    if sites.len() < ENFORCEMENT_FLOOR {
        problems.push(format!(
            "ENFORCEMENT holds {} sites and the floor is {ENFORCEMENT_FLOOR}. A \
             deleted `Site` row silences this census for that symbol with \
             nothing going red, which is how the first version of this file \
             could be disarmed one row at a time. Lowering the floor is a \
             deliberate edit with a diff.",
            sites.len()
        ));
    }
    let unwired = sites
        .iter()
        .filter(|s| matches!(s.cover, Cover::Unwired { .. }))
        .count();
    if unwired > MAX_UNWIRED {
        problems.push(format!(
            "{unwired} sites are registered UNWIRED and MAX_UNWIRED is \
             {MAX_UNWIRED}. Raising the cap is a deliberate edit, which is the \
             only thing standing between an exception list and a permission slip."
        ));
    }

    let bin_srcs = binary_crates(&repo_root());
    let executables: Vec<&(String, String)> = files
        .iter()
        .filter(|(rel, _)| is_executable_surface(rel, &bin_srcs))
        .collect();

    // --- 1. BEHAVIOUR. Every site names a probe, and every probe must hold. --
    //
    // This is the whole of the rebuild. `Driven` and `Unwired` differ only in
    // whether a shipped file NAMES the symbol; both are answerable to a probe
    // that ran. A site whose probe did not hold is enforcement that has stopped
    // happening, and it is reported by name.
    let mut leans_on_guard = false;
    for site in sites {
        let probe = site.cover.probe();
        if probe == SPLIT_GUARD {
            leans_on_guard = true;
        }
        if let Err(why) = observed.probe(probe) {
            problems.push(format!(
                "`{}` ({}) leans on the `{probe}` probe, and THE PROBE DID NOT \
                 HOLD.\n   {why}\n   `{}` {} — and on this tree nothing that \
                 ships is making that refusal happen.",
                site.symbol, site.defined_in, site.symbol, site.enforces
            ));
        }
    }
    // The mutation control is not tied to one site: it is what makes every
    // `split-guard` observation mean anything at all.
    if leans_on_guard {
        if let Err(why) = &observed.guard_is_load_bearing {
            problems.push(format!(
                "THE MUTATION CONTROL FAILED, so every split-guard observation \
                 in this run is uninterpretable. {why}"
            ));
        }
    }
    for site in sites {
        if let Cover::Unwired { why, hazard, .. } = site.cover {
            if why.len() < 40 {
                problems.push(format!("`{}` is excused without a reason", site.symbol));
            }
            if hazard.len() < 40 {
                problems.push(format!(
                    "`{}` is excused without naming the hazard the exception \
                     leaves open. A reason says why there is no caller; a hazard \
                     says what is therefore unprotected, and an exception list \
                     that carries only the first is a list of apologies.",
                    site.symbol
                ));
            }
        }
    }

    // --- 2. CALLER SEARCH. Text, for the one question text can answer. ------
    for site in sites {
        let callers: Vec<&str> = executables
            .iter()
            .filter(|(rel, text)| rel.as_str() != site.defined_in && names(text, site.symbol))
            .map(|(rel, _)| rel.as_str())
            .collect();
        match (callers.is_empty(), &site.cover) {
            (true, Cover::Driven { probe }) => problems.push(format!(
                "`{}` ({}) is registered as Driven by the `{probe}` probe, and \
                 no shipped file names it at all. `Driven` asserts a shipped \
                 caller; register it Unwired, with its hazard, or wire it.",
                site.symbol, site.defined_in
            )),
            (false, Cover::Unwired { .. }) => problems.push(format!(
                "`{}` is registered UNWIRED but IS now named by {}. A stale \
                 exception is how the list stops meaning anything — delete the \
                 entry and give it a probe.",
                site.symbol,
                callers.join(", ")
            )),
            _ => {}
        }
    }

    // --- 3. The artefact must still be read by something that runs. ---------
    // Kept, and DEMOTED to what it is: a cheap corroboration, never the proof.
    // Left alone it is satisfied by one dead assignment, which is exactly how
    // this census passed 6/6 on a tree with the whole guard deleted.
    let consumers: Vec<&str> = executables
        .iter()
        .filter(|(_, text)| names(text, SPLIT_FIXTURE))
        .map(|(rel, _)| rel.as_str())
        .collect();
    if consumers.is_empty() {
        problems.push(format!(
            "`{SPLIT_FIXTURE}` has NO shipped executable consumer. The split \
             exists, the parser exists, the tests are green, and nothing that \
             runs a task reads it — which is the exact state in which \
             `ancient-puzzle` was spent 27 minutes after the split was committed"
        ));
    }

    problems
}

// ---------------------------------------------------------------------------
// Tests.
// ---------------------------------------------------------------------------

#[test]
fn the_walk_is_looking_at_the_source() {
    let files = shipped();
    let bin_srcs = binary_crates(&repo_root());
    assert!(
        files.len() > 40,
        "the walk found {} files; it is not looking at the source",
        files.len()
    );
    assert!(
        files.iter().any(|(r, _)| r == RUNNER),
        "the walk misses the runner, which is the file the census is about"
    );
    // The correction: the shipped binary's non-main modules are visible now.
    assert!(
        bin_srcs.iter().any(|p| p == "crates/cli/src/"),
        "no binary crate was derived; the executable surface is back to \
         pattern-matching. derived: {bin_srcs:?}"
    );
    assert!(
        is_executable_surface("crates/cli/src/pilot_cmd.rs", &bin_srcs),
        "crates/cli/src/pilot_cmd.rs is invisible again. It is reached from \
         main.rs and scripts/pilot-run.sh shells back into it, which is the \
         file that refutes `a shell script cannot call a Rust function`."
    );
    assert!(
        !is_executable_surface("crates/bench/src/pilot.rs", &bin_srcs),
        "a library crate's src counted as shipped; that is the loophole that \
         would have made this census green at the breaching commit"
    );
    // Positive control: each symbol really is defined where the register says.
    for site in ENFORCEMENT {
        let def = files
            .iter()
            .find(|(r, _)| r == site.defined_in)
            .unwrap_or_else(|| panic!("{} is not in the walk", site.defined_in));
        let bare = site.symbol.rsplit("::").next().expect("symbol");
        assert!(
            def.1.contains(bare),
            "ENFORCEMENT says `{}` lives in {}, and it does not",
            site.symbol,
            site.defined_in
        );
    }
}

/// **The behavioural half, run against the real tree.**
///
/// This is the test that goes red when the guard is deleted. The text census
/// did not: with all 135 lines of guard removed it passed 6 of 6.
#[test]
fn the_shipped_runner_refuses_a_live_holdout_and_the_guard_is_what_does_it() {
    let held = a_live_holdout_id();
    if let Err(why) = probe_guard_fires(&held) {
        panic!(
            "SPLIT ENFORCEMENT IS NOT FIRING FROM THE SHIPPED RUNNER.\n  {why}\n\n\
             The split guard in {RUNNER} (between {GUARD_BEGIN} and \
             {GUARD_END}) is what must refuse a live-holdout id, and it did \
             not. This is the state `ancient-puzzle` was spent in: the \
             enforcement in crates/bench/src/pilot.rs — TerminalBenchSplit, \
             pilot::plan, check_from_tune, tune_protocol, HoldoutLedger — is \
             library code, and this script is the only thing that ships which \
             refuses on its behalf."
        );
    }
    if let Err(why) = probe_guard_is_load_bearing(&held) {
        panic!(
            "THE MUTATION CONTROL FAILED.\n  {why}\n\n\
             The refusal watched above cannot be attributed to the split guard \
             without this, and a census that cannot attribute a refusal is a \
             census of vocabulary."
        );
    }
}

#[test]
fn the_coverage_flag_dispatches_and_refuses() {
    if let Err(why) = probe_coverage_flag() {
        panic!(
            "`scripts/pilot-report.py --coverage` is registered in ENFORCEMENT \
             as the guard on publishing a coverage figure, and driving it \
             showed otherwise.\n  {why}"
        );
    }
}

#[test]
fn every_split_enforcement_symbol_is_covered_or_registered_with_its_hazard() {
    let held = a_live_holdout_id();
    let observed = Observed {
        guard_fires: probe_guard_fires(&held),
        guard_is_load_bearing: probe_guard_is_load_bearing(&held),
        coverage_flag_dispatches: probe_coverage_flag(),
    };
    let problems = census(&shipped(), ENFORCEMENT, &observed);
    assert!(
        problems.is_empty(),
        "split-enforcement census:\n  {}\n\n\
         Split enforcement that nothing calls is not enforcement. The committed \
         holdout was breached 27 minutes after it was committed by exactly this \
         state: `pilot-run.sh` had zero occurrences of the word `holdout`, and \
         `pilot::plan`, `check_from_tune` and `TerminalBenchSplit` existed, were \
         tested, and were called by nothing that ships.",
        problems.join("\n  ")
    );
}

// ---------------------------------------------------------------------------
// Negative controls. Each varies ONE thing and watches the census report it.
// ---------------------------------------------------------------------------

fn head_shaped_corpus() -> Vec<(String, String)> {
    vec![
        (
            "crates/bench/src/pilot.rs".to_string(),
            format!(
                "pub const TERMINAL_BENCH_SPLIT: &str = include_str!(\"../../../fixtures/{SPLIT_FIXTURE}\");\n\
                 pub struct TerminalBenchSplit {{}}\n\
                 pub fn plan() {{}}\n\
                 pub fn check_from_tune() {{}}\n\
                 pub fn tune_protocol() {{}}\n"
            ),
        ),
        (
            "crates/bench/src/protocol.rs".to_string(),
            "pub struct HoldoutLedger {}\n".to_string(),
        ),
        (
            // The runner as it actually stood: task id from the environment,
            // no mention of the split, nothing to refuse.
            RUNNER.to_string(),
            "TASK_ID=\"${PILOT_TASK_ID:-broken-python}\"\ntb run --task-id \"$TASK_ID\"\n"
                .to_string(),
        ),
    ]
}

/// **THE REGRESSION FOR THE DEFECT THIS FILE WAS REBUILT FOR.**
///
/// A corpus in which the fixture basename still appears on one executable line
/// — a dead `SPLIT_FILE=` assignment nothing reads — and the guard is gone.
/// The old census passed this. This one must name the cause.
#[test]
fn a_dead_assignment_naming_the_fixture_does_not_satisfy_the_census() {
    let mut gutted = head_shaped_corpus();
    gutted[2].1 = format!(
        // Every text check the old census had, satisfied, with no guard:
        // the basename on an executable line, and 44 unrelated refusal tokens.
        "SPLIT_FILE=\"${{PILOT_SPLIT_FILE:-$REPO_ROOT/fixtures/{SPLIT_FIXTURE}}}\"\n\
         die() {{ printf '%s\\n' \"$*\" >&2; exit 1; }}\n\
         [ -n \"$TASK_ID\" ] || die \"REFUSED: no task id\"\n\
         tb run --task-id \"$TASK_ID\"\n"
    );
    let observed = Observed {
        // What the real probe returns on such a tree.
        guard_fires: Err("scripts/pilot-run.sh exited Some(1) on live-holdout \
                          id `x` but NOT with the split refusal"
            .to_string()),
        guard_is_load_bearing: Ok(()),
        coverage_flag_dispatches: Ok(()),
    };
    let problems = census(&gutted, ENFORCEMENT, &observed);

    assert!(
        !problems.is_empty(),
        "the census passed on a tree whose only trace of the split is a dead \
         assignment. That is the measured defect: 135 lines of guard deleted, \
         6 of 6 green, exit 0."
    );
    let joined = problems.join("\n");
    assert!(
        joined.contains("THE PROBE DID NOT HOLD"),
        "the census did not name the behaviour that stopped happening:\n{joined}"
    );
    assert!(
        joined.contains("TerminalBenchSplit"),
        "the census did not name the enforcement that is no longer firing:\n{joined}"
    );
    // And the corroboration it USED to rest on is, on this corpus, satisfied —
    // which is the whole point of demoting it.
    assert!(
        !joined.contains("NO shipped executable consumer"),
        "this corpus is supposed to satisfy the old text check; if it does not, \
         the regression is not reproducing the defect:\n{joined}"
    );
}

/// **The floor.** Deleting a `Site` row must fail, and it did not before.
#[test]
fn deleting_a_site_row_fails_the_census() {
    let short: Vec<Site> = ENFORCEMENT
        .iter()
        .skip(1) // one row deleted, exactly as an edit would
        .map(|s| Site {
            symbol: s.symbol,
            defined_in: s.defined_in,
            enforces: s.enforces,
            cover: s.cover,
        })
        .collect();
    let problems = census(&shipped(), &short, &Observed::all_good());
    assert!(
        problems
            .iter()
            .any(|p| p.contains("ENFORCEMENT holds") && p.contains("the floor is")),
        "a deleted Site row did not fail the census. That is how the first \
         version of this file could be disarmed one row at a time:\n{problems:#?}"
    );
}

/// The mutation control failing must be reported as such — not silently.
#[test]
fn a_missing_mutation_boundary_is_reported_and_not_shrugged_off() {
    let observed = Observed {
        guard_fires: Ok(()),
        guard_is_load_bearing: Err(format!(
            "{RUNNER} has no mutation boundary; the sentinels were deleted"
        )),
        coverage_flag_dispatches: Ok(()),
    };
    let problems = census(&shipped(), ENFORCEMENT, &observed);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("MUTATION CONTROL FAILED") && p.contains("uninterpretable")),
        "a census that cannot attribute its refusal reported nothing:\n{problems:#?}"
    );
}

/// The other direction an exception list rots in: the symbol gets wired, and
/// the entry stays.
#[test]
fn the_census_reports_a_stale_exception() {
    let wired = vec![
        (
            "crates/bench/src/pilot.rs".to_string(),
            "pub struct TerminalBenchSplit {}\n".to_string(),
        ),
        (
            "crates/cli/src/main.rs".to_string(),
            "let s = TerminalBenchSplit::load();\n".to_string(),
        ),
        (RUNNER.to_string(), format!("split={SPLIT_FIXTURE}\n")),
    ];
    let sites = [Site {
        symbol: "TerminalBenchSplit",
        defined_in: "crates/bench/src/pilot.rs",
        enforces: "re-derives the split",
        cover: &NO_RUST_DRIVER,
    }];
    let problems = census(&wired, &sites, &Observed::all_good());
    assert!(
        problems.iter().any(|p| p.contains("stale exception")),
        "a wired symbol kept its exception:\n{problems:#?}"
    );
}

/// An exception with no hazard named is an apology, and must not be accepted.
#[test]
fn an_exception_without_a_hazard_is_refused() {
    let sites = [Site {
        symbol: "TerminalBenchSplit",
        defined_in: "crates/bench/src/pilot.rs",
        enforces: "re-derives the split",
        cover: &Cover::Unwired {
            why: "a reason long enough to pass the length floor on registered exceptions",
            hazard: "shrug",
            probe: SPLIT_GUARD,
        },
    }];
    let problems = census(&head_shaped_corpus(), &sites, &Observed::all_good());
    assert!(
        problems
            .iter()
            .any(|p| p.contains("without naming the hazard")),
        "an exception with no hazard was accepted:\n{problems:#?}"
    );
}

/// `Driven` asserts a shipped caller exists. When none does, the register is
/// lying about the symbol's state and must say so — the mirror image of the
/// stale-exception check.
#[test]
fn a_driven_site_with_no_shipped_caller_is_reported() {
    let sites = [Site {
        symbol: "tune_protocol",
        defined_in: "crates/bench/src/pilot.rs",
        enforces: "seeds a Protocol only from tune",
        cover: &Cover::Driven { probe: SPLIT_GUARD },
    }];
    let problems = census(&head_shaped_corpus(), &sites, &Observed::all_good());
    assert!(
        problems
            .iter()
            .any(|p| p.contains("registered as Driven") && p.contains("no shipped file names it")),
        "a Driven site with no caller anywhere was accepted:\n{problems:#?}"
    );
}

/// A library file mentioning the fixture must not satisfy the consumer rule.
/// This is the precise loophole that would have made the census green at HEAD.
#[test]
fn a_library_file_that_embeds_the_fixture_is_not_a_consumer() {
    let bin_srcs = binary_crates(&repo_root());
    assert!(!is_executable_surface(
        "crates/bench/src/pilot.rs",
        &bin_srcs
    ));
    assert!(is_executable_surface(RUNNER, &bin_srcs));
    assert!(is_executable_surface("crates/cli/src/main.rs", &bin_srcs));
    assert!(is_test_code("crates/bench/tests/pilot.rs"));
    // A test driver beside the script it drives is a watcher, not a caller.
    assert!(is_test_code("scripts/pilot-report-coverage-test.py"));
    assert!(!is_executable_surface(
        "scripts/pilot-report-coverage-test.py",
        &bin_srcs
    ));
    assert!(is_executable_surface("scripts/pilot-report.py", &bin_srcs));

    let only_lib = vec![(
        "crates/bench/src/pilot.rs".to_string(),
        format!("include_str!(\"../../../fixtures/{SPLIT_FIXTURE}\")\n"),
    )];
    let problems = census(&only_lib, &[], &Observed::all_good());
    assert!(
        problems
            .iter()
            .any(|p| p.contains("NO shipped executable consumer")),
        "an `include_str!` in a library counted as reading the split:\n{problems:#?}"
    );
}

/// `--coverage` really has no shipped caller — the fact the register asserts.
/// If one appears, the entry above is stale and must be deleted.
#[test]
fn the_coverage_flag_has_no_shipped_caller_and_the_register_says_so() {
    let bin_srcs = binary_crates(&repo_root());
    let callers: Vec<String> = shipped()
        .into_iter()
        .filter(|(rel, text)| {
            rel != "scripts/pilot-report.py"
                && is_executable_surface(rel, &bin_srcs)
                && names(text, "--coverage")
        })
        .map(|(rel, _)| rel)
        .collect();
    assert!(
        callers.is_empty(),
        "`--coverage` now has shipped caller(s) {callers:?}. Delete its UNWIRED \
         registration in ENFORCEMENT and give it a Driven cover."
    );
    let site = ENFORCEMENT
        .iter()
        .find(|s| s.symbol == "--coverage")
        .expect("`--coverage` must stay registered while it has no caller");
    assert!(
        matches!(site.cover, Cover::Unwired { .. }),
        "`--coverage` is registered as Driven and has no shipped caller"
    );
}
