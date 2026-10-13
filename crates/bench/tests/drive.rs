//! `thruline bench drive` — the refusals, and the commitment.
//!
//! # This file exists because `drive` is a SECOND runner
//!
//! `scripts/pilot-run.sh` enforces the committed split with a shell guard, and
//! `split_enforcement_census.rs` excuses six Rust symbols from needing a caller
//! on the grounds that no Rust entry point stages and runs a task. It names the
//! hazard exactly: *"a SECOND runner — a Rust one, or another script — would
//! inherit none of it."* `drive` is that second runner. So the first thing
//! tested here is not that it works; it is that it **refuses a live-holdout
//! id**, in Rust, without the shell guard anywhere in the path.
//!
//! The holdout was breached once — `ancient-puzzle`, 27 minutes after the split
//! fixture was committed — by a runner that took its task id from an
//! environment variable and never opened the file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use thruline_bench::class_two::TerminalState;
use thruline_bench::drive::{self, DriveError, DriveRequest, Observed};
use thruline_bench::pilot::{Harness, TerminalBenchSplit, TERMINAL_BENCH_SPLIT};
use thruline_bench::protocol::Provenance;

// ---------------------------------------------------------------------------
// A corpus, built from the SPLIT'S OWN IDS.
//
// Not from a directory listing and not from a hand-typed list: `prepare` calls
// `check_against_corpus`, which fails on ANY difference between the committed
// ids and the directory, so a corpus assembled any other way would make these
// tests fail for a reason that has nothing to do with what they assert.
// ---------------------------------------------------------------------------

fn split_rows() -> Vec<(String, String)> {
    TERMINAL_BENCH_SPLIT
        .lines()
        .filter(|l| l.starts_with("task\t"))
        .filter_map(|l| {
            let mut f = l.split('\t');
            let _ = f.next();
            Some((f.next()?.to_string(), f.next()?.to_string()))
        })
        .collect()
}

fn observed_ids() -> Vec<String> {
    TERMINAL_BENCH_SPLIT
        .lines()
        .filter(|l| l.starts_with("observed\t"))
        .filter_map(|l| l.split('\t').nth(1).map(str::to_string))
        .collect()
}

/// An id on the holdout side that carries no `observed` row — i.e. **live**
/// holdout, the thing there is still something to protect about.
fn a_live_holdout_id() -> String {
    let observed = observed_ids();
    split_rows()
        .into_iter()
        .find(|(id, side)| side == "holdout" && !observed.contains(id))
        .map(|(id, _)| id)
        .expect("the committed split has a live holdout id")
}

fn a_tune_id() -> String {
    split_rows()
        .into_iter()
        .find(|(_, side)| side == "tune")
        .map(|(id, _)| id)
        .expect("the committed split has a tune id")
}

/// Every committed id as a directory, with one file in the task under test so
/// the staging copy has something to move.
fn corpus_at(root: &Path, populate: &str) -> PathBuf {
    let corpus = root.join("original-tasks");
    for (id, _) in split_rows() {
        std::fs::create_dir_all(corpus.join(&id)).expect("task dir");
    }
    std::fs::write(
        corpus.join(populate).join("task.yaml"),
        "instruction: do the thing\n",
    )
    .expect("task.yaml");
    // A nested CONTENT directory, not `tests/`. This used to be `tests/`, and
    // the assertion that it survived staging was asserting the defect: the
    // grader must never reach the agent's workspace. See WITHHELD.
    std::fs::create_dir_all(corpus.join(populate).join("data")).expect("data dir");
    std::fs::write(
        corpus.join(populate).join("data").join("input.txt"),
        "the task's own nested content\n",
    )
    .expect("nested file");
    corpus
}

/// **A leak I wrote today.** `_tmp` held the path and nothing removed it: the
/// underscore says "unused field", not "owns the directory", and `PathBuf` has
/// no `Drop` that deletes anything. 758 `thruline-drive-*` directories on this
/// machine before it was noticed — and the `tl-*` sweep that found the fourth
/// and fifth leak sites did not see it, because I chose a different prefix here.
///
/// A sweep for the names you know about is a sweep for the leaks you have
/// already fixed.
struct Fixture {
    tmp: PathBuf,
    req: DriveRequest,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.tmp);
    }
}

fn fixture(task: &str, label: &str) -> Fixture {
    let tmp = std::env::temp_dir().join(format!("thruline-drive-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).expect("tmp");
    let corpus = corpus_at(&tmp, task);
    Fixture {
        req: DriveRequest {
            task: task.to_string(),
            corpus,
            root: tmp.join("runs"),
            run_id: format!("drive-{label}"),
            harness: Harness::ClaudeCode,
            model: "claude-opus-5".to_string(),
            launched_by: "operator".to_string(),
            ledger: tmp.join("holdout-spend.tsv"),
        },
        tmp,
    }
}

// ---------------------------------------------------------------------------
// The refusal that matters.
// ---------------------------------------------------------------------------

#[test]
fn a_live_holdout_id_is_refused_by_the_rust_runner_with_no_shell_guard_in_the_path() {
    let held = a_live_holdout_id();
    let f = fixture(&held, "holdout");
    match drive::prepare(&f.req) {
        Err(DriveError::NotFromTune(e)) => {
            let msg = e.to_string();
            assert!(
                msg.contains(&held),
                "the refusal must NAME the id it refused, or an operator cannot \
                 tell which of their arguments was the problem: {msg}"
            );
        }
        Err(other) => panic!(
            "refused, but for the WRONG REASON: {other}. A holdout id that fails \
             on a missing directory or an unparseable split is not protected — \
             it is lucky, and the next refactor removes the luck."
        ),
        Ok(_) => panic!(
            "`drive` PREPARED a live-holdout task ({held}). This is the exact \
             state that spent `ancient-puzzle`: the enforcement existed, and the \
             runner did not call it."
        ),
    }
    // AND NOTHING WAS SPENT. A refusal that still charges the holdout ledger has
    // consumed the thing it refused to consume.
    assert!(
        !f.req.ledger.exists(),
        "the holdout ledger was written by a REFUSED drive at {}",
        f.req.ledger.display()
    );
}

#[test]
fn the_refusal_happens_before_the_workspace_is_staged() {
    let held = a_live_holdout_id();
    let f = fixture(&held, "holdout-nostage");
    let _ = drive::prepare(&f.req);
    let staged = f.req.root.join(&f.req.run_id);
    assert!(
        !staged.exists(),
        "a refused drive left a staged workspace at {} — an operator who lists \
         the run directory would see a run that was never sanctioned",
        staged.display()
    );
}

// ---------------------------------------------------------------------------
// The accepted path.
// ---------------------------------------------------------------------------

#[test]
fn a_tune_task_prepares_stages_and_commits_as_class_two() {
    let task = a_tune_id();
    let f = fixture(&task, "tune");
    let p = drive::prepare(&f.req).expect("a tune task must prepare");

    assert_eq!(p.task, task);
    assert!(
        p.workspace.join("task.yaml").is_file(),
        "the task's own files were not staged into {}",
        p.workspace.display()
    );
    assert!(
        p.workspace.join("data").join("input.txt").is_file(),
        "the staging copy is not recursive, so a task's nested content is lost"
    );

    // Class 2, carrying its commitment rather than asserting a label.
    match &p.provenance {
        Provenance::GeneratedUnderProtocol(c) => assert_eq!(c.hash(), p.commitment.hash()),
        other => panic!("expected class 2, got class {}", other.class()),
    }
    assert_eq!(p.provenance.class(), 2);

    // The spend was recorded, and this run has no priors.
    assert!(f.req.ledger.exists(), "the holdout ledger was not written");
    assert!(p.manifest.prior_runs().is_empty());
    assert_eq!(p.manifest.run_id(), f.req.run_id);
}

#[test]
fn the_corpus_must_still_be_the_committed_one() {
    let task = a_tune_id();
    let f = fixture(&task, "drift");
    // One extra directory is a different corpus, and a run measured on it is
    // not a run of this protocol.
    std::fs::create_dir_all(f.req.corpus.join("a-task-nobody-committed")).expect("extra");
    match drive::prepare(&f.req) {
        Err(DriveError::Split(_)) => {}
        other => panic!(
            "a corpus with an uncommitted task was accepted ({other:?}). The \
             split's hash proves the FILE is consistent; only the corpus check \
             proves it still describes these tasks."
        ),
    }
}

#[test]
fn an_unknown_task_is_refused_by_name() {
    let task = a_tune_id();
    let mut f = fixture(&task, "unknown");
    f.req.task = "no-such-task-anywhere".to_string();
    match drive::prepare(&f.req) {
        // It is not in the corpus AND not in tune; either refusal is correct,
        // and both must name the id.
        Err(e) => assert!(
            e.to_string().contains("no-such-task-anywhere"),
            "the refusal does not name the task: {e}"
        ),
        Ok(_) => panic!("prepared a task that does not exist"),
    }
}

// ---------------------------------------------------------------------------
// The oracle must not reach the agent.
// ---------------------------------------------------------------------------

#[test]
fn the_staged_workspace_withholds_the_grader_and_the_reference_solution() {
    let task = a_tune_id();
    let f = fixture(&task, "withhold");
    // Plant the whole shape a real task directory has, at top level AND nested,
    // because the first version of this staging copied the directory whole.
    let td = f.req.corpus.join(&task);
    std::fs::write(td.join("solution.sh"), "#!/bin/sh\n# the answer\n").expect("solution");
    std::fs::write(td.join("run-tests.sh"), "pytest tests/\n").expect("run-tests");
    std::fs::write(td.join("Dockerfile"), "FROM scratch\n").expect("dockerfile");
    std::fs::write(td.join("docker-compose.yaml"), "services: {}\n").expect("compose");
    std::fs::create_dir_all(td.join("inner/tests")).expect("nested tests");
    std::fs::write(td.join("inner/tests/test_nested.py"), "assert True\n").expect("nested");
    std::fs::write(td.join("content.py"), "# the task's own content\n").expect("content");

    let p = drive::prepare(&f.req).expect("prepares");

    // G's arm of the day's rule (DECISIONS #180): a check that runs today but
    // could go empty needs a guard that fails when it does. Both loops below
    // iterate WITHHELD, so emptying that list makes them pass while NOTHING is
    // withheld — and the staged workspace would hand the agent its grader
    // again, silently, with the suite green.
    assert!(
        !drive::WITHHELD.is_empty(),
        "WITHHELD is empty, so this test and \
         `withholding_is_part_of_the_commitment` both iterate nothing and pass \
         while the agent's workspace contains the reference solution and the \
         grader."
    );

    for (name, why) in drive::WITHHELD {
        let hit = walk_names(&p.workspace);
        assert!(
            !hit.iter().any(|n| n == name),
            "the staged workspace contains `{name}` — {why}. An agent that can \
             read it is not being measured."
        );
    }
    assert!(
        !p.workspace.join("inner/tests").exists(),
        "a withheld name survived at depth. The harness separates these at every \
         level, not just the top one."
    );
    // ...and the task's OWN content is still there. A staging that withholds
    // everything withholds the task.
    assert!(
        p.workspace.join("content.py").is_file(),
        "the task's own content was withheld along with the oracle"
    );
    assert!(
        p.workspace.join("task.yaml").is_file(),
        "task.yaml is the instruction the prompt tells the agent to read"
    );
}

/// Every file and directory name under `root`, at any depth.
fn walk_names(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            out.push(e.file_name().to_string_lossy().into_owned());
            if e.path().is_dir() {
                stack.push(e.path());
            }
        }
    }
    out
}

#[test]
fn withholding_is_part_of_the_commitment() {
    let split = TerminalBenchSplit::load().expect("the committed split");
    let task = a_tune_id();
    let f = fixture(&task, "withheldhash");
    let p = drive::protocol_for(&f.req, &split);
    let declared = p.parameters.get("withheld").expect("withheld is committed");
    assert!(
        !drive::WITHHELD.is_empty(),
        "WITHHELD is empty: the loop below checks nothing and the protocol \
         would commit to withholding nothing while claiming a withheld set."
    );
    assert!(
        !declared.is_empty(),
        "the committed `withheld` parameter is an empty string. Two sessions \
         that differ in whether the grader was readable would then hash the \
         same."
    );
    for (name, _) in drive::WITHHELD {
        assert!(
            declared.contains(name),
            "`{name}` is withheld but not declared in the protocol. Two sessions \
             that differ in whether the grader was readable are not comparable, \
             so what was withheld has to be in the hash."
        );
    }
}

// ---------------------------------------------------------------------------
// The prompt is IN the commitment. This is the whole anti-fitting mechanism.
// ---------------------------------------------------------------------------

#[test]
fn rewording_the_prompt_changes_the_protocol_hash() {
    let split = TerminalBenchSplit::load().expect("the committed split");
    let task = a_tune_id();
    let f = fixture(&task, "prompt");

    let mut a = drive::protocol_for(&f.req, &split);
    a.tasks = vec![task.clone()];
    a.holdout = vec![task.clone()];
    let committed = a.commit().expect("commits").hash().to_string();

    let mut b = a.clone();
    b.parameters.insert(
        "prompt".to_string(),
        format!("{} Also, try harder.", drive::OPERATOR_PROMPT),
    );
    let reworded = b.commit().expect("commits").hash().to_string();

    assert_ne!(
        committed, reworded,
        "a reworded prompt produced the SAME protocol hash. The prompt would \
         then be a dial: run, read the trajectory, reword, run again — every \
         run honest, the sequence fitted."
    );
}

#[test]
fn the_prompt_and_its_digest_are_both_in_the_parameters() {
    let split = TerminalBenchSplit::load().expect("the committed split");
    let task = a_tune_id();
    let f = fixture(&task, "params");
    let p = drive::protocol_for(&f.req, &split);

    assert_eq!(
        p.parameters.get("prompt").map(String::as_str),
        Some(drive::OPERATOR_PROMPT)
    );
    assert_eq!(
        p.parameters.get("prompt_sha256"),
        Some(&drive::prompt_digest())
    );
    assert_eq!(
        p.parameters.get("launch").map(String::as_str),
        Some("operator"),
        "a drive that does not record HOW it was launched cannot be told apart \
         from a scripted run afterwards"
    );
    // 64 lowercase hex, not a debug-formatted digest.
    let d = drive::prompt_digest();
    assert_eq!(d.len(), 64, "digest is not 64 hex characters: {d}");
    assert!(d
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
}

#[test]
fn an_unnamed_launcher_is_refused_and_costs_nothing() {
    let task = a_tune_id();
    let mut f = fixture(&task, "nolauncher");
    f.req.launched_by = "   ".to_string();
    match drive::prepare(&f.req) {
        Err(DriveError::NoLauncher) => {}
        other => panic!(
            "an unnamed launcher was accepted ({other:?}). `launch` is hashed \
             into the protocol, so defaulting it writes a name into the record \
             that nobody chose — and nothing downstream could contradict it."
        ),
    }
    assert!(
        !f.req.ledger.exists(),
        "a refused drive charged the holdout ledger"
    );
    assert!(
        !f.req.root.join(&f.req.run_id).exists(),
        "a refused drive staged a workspace"
    );
}

#[test]
fn who_launched_it_is_part_of_the_protocol_hash() {
    let split = TerminalBenchSplit::load().expect("the committed split");
    let task = a_tune_id();
    let mut f = fixture(&task, "launcher");

    let mut hashes = Vec::new();
    for who in ["operator", "coordinator"] {
        f.req.launched_by = who.to_string();
        let mut p = drive::protocol_for(&f.req, &split);
        p.tasks = vec![task.clone()];
        p.holdout = vec![task.clone()];
        assert_eq!(p.parameters.get("launch").map(String::as_str), Some(who));
        hashes.push(p.commit().expect("commits").hash().to_string());
    }
    assert_ne!(
        hashes[0], hashes[1],
        "an operator-launched and a coordinator-launched run of the same task \
         produced the SAME protocol hash. Who pressed the key is part of what a \
         class-2 corpus claims, and two runs that differ in it must not pool."
    );
}

// ---------------------------------------------------------------------------
// The precondition, enforced on the way IN rather than trusted.
// ---------------------------------------------------------------------------

#[test]
fn a_tree_hash_shaped_like_anything_else_is_a_precondition_failure() {
    let task = a_tune_id();
    let f = fixture(&task, "tree");
    let p = drive::prepare(&f.req).expect("prepares");

    for bogus in [
        "ok",
        "",
        "true",
        "4b825dc6",
        "ZZZZ25dc642cb6eb9a060e54bf8d69288fbee4904",
    ] {
        let row = drive::record(
            &p,
            &Observed {
                state: TerminalState::Completed,
                oracle_exit: Some(0),
                duration_ms: 1,
                executed_checks: 7,
                tree_before_first_action: Some(bogus.to_string()),
            },
        );
        assert_eq!(
            row.state,
            TerminalState::NoBindableTree,
            "`{bogus}` was accepted as a tree hash. A precondition satisfied by \
             an arbitrary string is not a precondition."
        );
        assert_eq!(
            row.executed_checks, 0,
            "checks survived a precondition failure — they would be counted as \
             evidence bound to a tree that does not exist"
        );
        assert!(row.verdict.is_none());
        assert!(row.tree_before_first_action.is_none());
    }
}

#[test]
fn a_real_tree_hash_is_kept_and_the_row_carries_its_own_protocol_hash() {
    let task = a_tune_id();
    let f = fixture(&task, "realtree");
    let p = drive::prepare(&f.req).expect("prepares");
    let tree = "c95e98ed6e6d1f0a2b3c4d5e6f708192a3b4c5d6";

    let row = drive::record(
        &p,
        &Observed {
            state: TerminalState::Completed,
            oracle_exit: Some(0),
            duration_ms: 42,
            executed_checks: 3,
            tree_before_first_action: Some(tree.to_string()),
        },
    );
    assert_eq!(row.state, TerminalState::Completed);
    assert_eq!(row.tree_before_first_action.as_deref(), Some(tree));
    assert_eq!(row.executed_checks, 3);
    assert_eq!(
        row.protocol_hash,
        p.commitment.hash(),
        "the row must carry its OWN protocol hash — a row that cannot disagree \
         with the commitment cannot be compared against it"
    );
}

// ---------------------------------------------------------------------------
// The ledger.
// ---------------------------------------------------------------------------

#[test]
fn the_same_protocol_cannot_be_spent_twice() {
    let task = a_tune_id();
    let f = fixture(&task, "twice");
    let first = drive::prepare(&f.req).expect("first prepare");

    // Same everything but the run id: the protocol hash is identical, so this
    // is the same holdout being consumed a second time.
    let mut again = f.req.clone();
    again.run_id = "drive-twice-b".to_string();
    match drive::prepare(&again) {
        Err(DriveError::AlreadySpent { prior, .. }) => {
            assert!(
                prior.contains(&first.run_id),
                "the refusal must name the run that got there first: {prior:?}"
            );
        }
        other => panic!(
            "a second spend of the same protocol was allowed ({other:?}). Ten \
             tuning runs over one holdout is a validation set wearing a \
             holdout's name."
        ),
    }
}

#[test]
fn the_operator_brief_names_the_task_the_workspace_and_the_protocol() {
    let task = a_tune_id();
    let f = fixture(&task, "brief");
    let p = drive::prepare(&f.req).expect("prepares");
    let brief = p.operator_brief();

    for needle in [
        task.as_str(),
        p.commitment.hash(),
        "class 2, operator-launched", // the fixture launches as `operator`
        drive::OPERATOR_PROMPT,
    ] {
        assert!(
            brief.contains(needle),
            "the operator brief omits {needle:?}. It is the only thing the \
             operator reads before spending real credentials.\n{brief}"
        );
    }
}

// ---------------------------------------------------------------------------
// The declared set is the accounting denominator.
// ---------------------------------------------------------------------------

#[test]
fn ingest_produces_exactly_one_outcome_for_the_declared_task() {
    let task = a_tune_id();
    let f = fixture(&task, "ingest");
    let p = drive::prepare(&f.req).expect("prepares");

    let row = drive::record(
        &p,
        &Observed {
            state: TerminalState::Completed,
            oracle_exit: Some(0),
            duration_ms: 5,
            executed_checks: 1,
            tree_before_first_action: Some("c95e98ed6e6d1f0a2b3c4d5e6f708192a3b4c5d6".to_string()),
        },
    );
    let (outcomes, undeclared) = thruline_bench::class_two::ingest(
        &p.commitment,
        thruline_bench::class_two::Corpus::TerminalBench,
        &[row],
    );
    assert_eq!(outcomes.len(), 1);
    assert!(
        undeclared.is_empty(),
        "a result for a task the protocol never declared: {undeclared:?}"
    );
}

#[test]
fn a_run_that_produced_nothing_still_accounts_for_its_declared_task() {
    let task = a_tune_id();
    let f = fixture(&task, "missing");
    let p = drive::prepare(&f.req).expect("prepares");

    let (outcomes, undeclared) = thruline_bench::class_two::ingest(
        &p.commitment,
        thruline_bench::class_two::Corpus::TerminalBench,
        &[],
    );
    assert_eq!(
        outcomes.len(),
        1,
        "the declared task vanished from the accounting when the operator's \
         session produced no result. A denominator that shrinks when a run \
         fails reports the failure as an absence."
    );
    assert!(undeclared.is_empty());
}

// A compile-time reminder that the module's error type is exhaustive enough to
// match on, which is what lets a CLI map refusals to exit codes.
#[test]
fn every_refusal_renders_a_message_a_person_can_act_on() {
    let e = DriveError::UnknownTask {
        id: "x".into(),
        corpus: PathBuf::from("/nowhere"),
    };
    assert!(e.to_string().contains("/nowhere"));
    let e = DriveError::AlreadySpent {
        protocol_hash: "sha256:abc".into(),
        prior: vec!["run-1".into()],
    };
    assert!(e.to_string().contains("run-1"));
    let _: BTreeMap<String, String> = BTreeMap::new();
}
