//! `ran ∩ holdout = ∅`, checked against the run records **on disk**.
//!
//! # The direction nothing else checks
//!
//! Everything else that guards the split is PROSPECTIVE. `scripts/pilot-run.sh`
//! refuses a live-holdout id before it runs one. `TerminalBenchSplit::parse`
//! re-derives all 241 sides and refuses a hand-edited one.
//! `thruline_bench::drive::prepare` refuses one in Rust. Those stop the next
//! breach.
//!
//! **None of them looks backwards**, and until this file there was nothing in
//! the repository that intersected the holdout with what has actually been run.
//! That gap is not theoretical: `ancient-puzzle` was live holdout and it was
//! spent 26 minutes and 38 seconds after the split fixture was committed, and
//! what caught it was a person reading timestamps — not a check. A guard that
//! only faces forward cannot tell you whether it was installed in time.
//!
//! # What counts as "ran"
//!
//! Three independent traces, deliberately over-inclusive, because the cost of a
//! false positive here is a conversation and the cost of a false negative is a
//! spent holdout nobody noticed:
//!
//! * `mount-<prefix><task>/`  — a capture mount was created for it
//! * `staged-<prefix><task>/` — its task directory was copied out of the corpus
//! * a `results.json` naming it — Terminal-Bench graded it
//!
//! Staging is included even without a result. A task that was staged and never
//! graded still had its content read by the harness, and "we pointed the
//! instrument at it but the run died" is not a holdout that survived.
//!
//! # It is IGNORED, not skipped — and the difference is the whole point
//!
//! The run records live outside the repository, so the sweep cannot be
//! unconditional. This file's first version handled that by printing "SKIPPED,
//! and this is not a pass" and returning — **and a Rust test that returns is a
//! pass.** The message argued with the harness; the harness won. Every gate
//! figure this crate published counted it among the passing while it swept
//! nothing.
//!
//! It is now `#[ignore]`, so an unrun sweep lands in the ignored column, which
//! is true. The consequence is stated rather than implied: **`ran ∩ holdout = ∅`
//! is verified when someone asks, not on every gate.** A gate cannot verify it
//! — the records are not in the repository — and what a gate can do is stop
//! claiming to.
//!
//! A failed lookup reported as a clean result is the exact defect this program
//! found in its own tooling when a search of three directories was published as
//! a fact about the whole filesystem. This was that defect, in the guard written
//! against it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use thruline_bench::pilot::TERMINAL_BENCH_SPLIT;

/// Ids on the holdout side that carry no `observed` row: the part of the
/// holdout there is still something to protect about.
///
/// Parsed from the compiled-in manifest rather than from the file on disk, so
/// this test is checking the committed split and not whatever is in the working
/// tree.
fn live_holdout() -> BTreeSet<String> {
    let observed: BTreeSet<&str> = TERMINAL_BENCH_SPLIT
        .lines()
        .filter(|l| l.starts_with("observed\t"))
        .filter_map(|l| l.split('\t').nth(1))
        .collect();
    TERMINAL_BENCH_SPLIT
        .lines()
        .filter(|l| l.starts_with("task\t"))
        .filter_map(|l| {
            let mut f = l.split('\t');
            let _ = f.next();
            let id = f.next()?;
            let side = f.next()?;
            (side == "holdout" && !observed.contains(id)).then(|| id.to_string())
        })
        .collect()
}

/// Task ids with any trace of having been run, from a pilot directory.
fn ran(pilot: &Path) -> std::io::Result<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    for entry in std::fs::read_dir(pilot)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        // `mount-<run-prefix><task>` and `staged-<run-prefix><task>`. The run
        // prefix is not known here, so the suffix is matched against the split's
        // ids rather than parsed — an id that is a suffix of the directory name
        // is the only reliable reading when the prefix is arbitrary.
        for lead in ["mount-", "staged-"] {
            if let Some(rest) = name.strip_prefix(lead) {
                out.insert(rest.to_string());
            }
        }
    }
    Ok(out)
}

/// Every `task_id` mentioned by a `results.json` anywhere under `pilot`.
fn graded(pilot: &Path, depth: usize, out: &mut BTreeSet<String>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(pilot) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            graded(&p, depth - 1, out);
        } else if p.file_name().and_then(|n| n.to_str()) == Some("results.json") {
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            // Deliberately a text scan and not a JSON walk: the harness's schema
            // is not this crate's to track, and a parse that failed on a shape
            // change would report "no tasks ran" — the unsafe direction.
            for (i, _) in text.match_indices("\"task_id\"") {
                let tail = &text[i..];
                if let Some(colon) = tail.find(':') {
                    let after = &tail[colon + 1..];
                    if let Some(open) = after.find('"') {
                        if let Some(close) = after[open + 1..].find('"') {
                            out.insert(after[open + 1..open + 1 + close].to_string());
                        }
                    }
                }
            }
        }
    }
}

/// **`#[ignore]`, and that is the fix rather than a retreat.**
///
/// This test used to run in every gate and REPORT A PASS whenever
/// `THRULINE_PILOT_DIR` was unset — which is always, in the default gate. It
/// printed "SKIPPED, and this is not a pass" and then returned, and a Rust test
/// that returns is a pass. The message argued with the harness and the harness
/// won: the crate's figure counted it among the passing while it swept nothing.
///
/// Track C named the shape (DECISIONS #173): **a check that passes by not
/// running looks correct on the page**, and is caught only by running it under
/// the condition it was written for and counting what comes out. Two of C's own
/// were guards gated on something they did not need; this one is the same fact
/// from the third side — a guard whose *precondition* was unmet, reporting the
/// colour of a guard whose condition was met.
///
/// `#[ignore]` moves it from the passed column to the ignored column, which is
/// where an unrun check honestly belongs, and matches how this crate already
/// reports its corpus-dependent tests. **Run it deliberately:**
///
/// ```text
/// THRULINE_PILOT_DIR=<pilot dir> cargo test -p thruline-bench \
///     --test holdout_intersection -- --ignored --nocapture
/// ```
///
/// It is not enough on its own and the honest consequence is stated here rather
/// than left implied: **`ran ∩ holdout = ∅` is now verified when someone asks,
/// not on every gate.** A gate cannot verify it, because the run records live
/// outside the repository; what a gate CAN do is stop claiming to.
#[ignore = "needs THRULINE_PILOT_DIR: the run records live outside the repository.             Ignored rather than silently passing — see the doc comment."]
#[test]
fn no_live_holdout_task_has_ever_been_run() {
    let Some(dir) = std::env::var_os("THRULINE_PILOT_DIR") else {
        let live = live_holdout();
        eprintln!(
            "SKIPPED, and this is not a pass: THRULINE_PILOT_DIR is unset, so \
             NOTHING was swept.\n\
             \n\
             What was NOT checked: whether any of the {} live-holdout task ids \
             has a mount-*, staged-* or results.json trace on this machine.\n\
             \n\
             Set THRULINE_PILOT_DIR to the pilot directory holding the run \
             records (the one with mount-*/ and staged-*/ in it) and re-run. A \
             failed lookup reported as a clean result is the defect this suite \
             exists to refuse — see PILOT-REPORT.md, where a sweep of three \
             directories was published as a fact about the whole filesystem.",
            live.len()
        );
        return;
    };
    let pilot = PathBuf::from(dir);
    assert!(
        pilot.is_dir(),
        "THRULINE_PILOT_DIR is set to {} and that is not a directory. Refusing \
         to report an empty intersection over a path that does not exist.",
        pilot.display()
    );

    let live = live_holdout();
    assert!(
        !live.is_empty(),
        "the committed split yielded NO live holdout ids. Either every holdout \
         task is now observed — which is a finding, not a pass — or the parse \
         above is wrong. An empty needle set makes any intersection empty."
    );

    let mut seen = ran(&pilot).expect("reading the pilot directory");
    graded(&pilot, 6, &mut seen);
    assert!(
        !seen.is_empty(),
        "no run records at all under {}. An intersection with nothing is empty \
         for the wrong reason.",
        pilot.display()
    );

    // The intersection is over SUFFIXES: a directory is `mount-<prefix><task>`
    // and the prefix is chosen per run, so the id has to be matched at the end.
    let breached: Vec<&String> = live
        .iter()
        .filter(|id| {
            seen.iter()
                .any(|d| d == *id || d.ends_with(&format!("-{id}")) || d.ends_with(id.as_str()))
        })
        .collect();

    assert!(
        breached.is_empty(),
        "ran ∩ live-holdout is NOT empty. These holdout tasks have been run:\n  \
         {}\n\n\
         A holdout task is worth exactly one measurement and these are spent. \
         Do not fix this by moving them to the observed list — that is how a \
         holdout becomes a validation set one exception at a time. Record the \
         spend, and say what it cost.",
        breached
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );

    eprintln!(
        "swept {} run-record ids under {} against {} live-holdout ids: \
         intersection empty",
        seen.len(),
        pilot.display(),
        live.len()
    );
}

#[test]
fn the_live_holdout_is_not_silently_empty() {
    let live = live_holdout();
    // 118 holdout-side minus 25 observed-and-holdout-side = 93, the figure the
    // fixture's own `holdout_size` header declares. Asserted here so that a
    // change to either number has to be deliberate: a parse that silently
    // returned nothing would make the intersection test above vacuous.
    assert_eq!(
        live.len(),
        93,
        "the live holdout is {} ids, and the committed fixture's `holdout_size` \
         header says 93. If the split changed, this number changes WITH a \
         re-measurement and a note — not on its own.",
        live.len()
    );
}
