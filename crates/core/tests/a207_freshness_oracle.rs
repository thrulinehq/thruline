//! A207 (MANDATE-v4 §13 / §33 / §34) — the freshness oracle.
//!
//! Real temporary git repositories, real observations through the Stage 3
//! observer, bindings built from really-observed identities, and every
//! expectation derived from the manipulation performed — never from the
//! engine under test. Stale-evidence invalidation is a §34 hard gate:
//! every case here must pass, with no percentage tolerance.
//!
//! Each mutation case asserts a positive control first (unchanged state ⇒
//! `CURRENT_STATE_CONFIRMED`), so the flip is attributable to the
//! manipulation alone. The v1 conservative rule is the law under test: ANY
//! workspace-identity change is relevant; there is no dependency-aware
//! narrowing to reach for.

use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use thruline_core::evidence_binding::{
    classify_command, evaluate, evaluate_with, scope_story, store, EvidenceBinding, EvidenceState,
    ExecMode, Freshness, IdentityEdge, StoreOutcome, StoredBinding, StreamCapture, WsidScopeDecl,
};
use thruline_core::ledger::Ledger;
use thruline_core::observation::{of_type, verify_chain, ObservationType};
use thruline_core::workspace_identity::{observe, Observation, Scope, WorkspaceFingerprint};

const EMPTY_SHA: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_AUTHOR_NAME", "oracle")
        .env("GIT_AUTHOR_EMAIL", "oracle@test")
        .env("GIT_COMMITTER_NAME", "oracle")
        .env("GIT_COMMITTER_EMAIL", "oracle@test")
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?} in {cwd:?}: {out:?}");
}

fn write(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, content).expect("write");
}

/// A repository with two committed files, on branch `main`.
fn seeded_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    write(root, "base.txt", "base\n");
    write(root, "second.txt", "second\n");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "seed"]);
    dir
}

fn fingerprint(root: &Path) -> WorkspaceFingerprint {
    match observe(root, Scope::everything()).expect("observe") {
        Observation::Complete(fp) => *fp,
        Observation::Incomplete { reason, .. } => {
            panic!("expected a complete observation: {reason}")
        }
    }
}

static TICK: AtomicU64 = AtomicU64::new(0);

/// Build and store a binding from a REAL observation of `root` — the
/// pre/post bracket is the really-observed identity, never an invention.
fn bind_now(ledger: &Ledger, root: &Path, command: &str) -> StoredBinding {
    let fp = fingerprint(root);
    let canonical_root = fp.doc().root.clone();
    let repo_id = fp
        .doc()
        .repo_id
        .clone()
        .expect("a committed repo has an identity");
    let tick = TICK.fetch_add(1, Ordering::SeqCst);
    let scope = classify_command(command, &canonical_root);
    let binding = EvidenceBinding {
        argv: command.split_whitespace().map(str::to_owned).collect(),
        mode: ExecMode::Direct,
        cwd: canonical_root.clone(),
        root: canonical_root,
        repository_identity: repo_id,
        scope,
        wsid_scope: WsidScopeDecl::everything(),
        pre: IdentityEdge::complete(fp.identity().to_string()),
        post: IdentityEdge::complete(fp.identity().to_string()),
        exit_code: Some(0),
        term_signal: None,
        stdout: StreamCapture::Captured {
            sha256: EMPTY_SHA.into(),
            bytes: 0,
        },
        stderr: StreamCapture::Captured {
            sha256: EMPTY_SHA.into(),
            bytes: 0,
        },
        artifacts: vec![],
        wrapper_defects: vec![],
        started_at: format!("2026-09-01T13:00:{:02}Z", tick % 60),
        bound_at: format!("2026-09-01T13:01:{:02}.{:03}Z", tick % 60, tick),
        source_adapter: "thruline".into(),
        source_session: format!("cli:check:oracle-{tick}"),
    };
    match store(ledger, &binding).expect("store") {
        StoreOutcome::Stored(s) => s,
        StoreOutcome::AlreadyStored(_) => panic!("each oracle binding is distinct by bound_at"),
    }
}

fn freshness_of(ledger: &Ledger, stored: &StoredBinding, at: &str) -> Freshness {
    evaluate(ledger, stored, at).expect("evaluate").freshness
}

#[test]
fn passing_evidence_becomes_stale_after_mutation_and_the_trail_deduplicates() {
    let dir = seeded_repo();
    let root = dir.path();
    let ledger = Ledger::open_in_memory().expect("ledger");
    let stored = bind_now(&ledger, root, "cargo test");

    // Positive control: the unchanged state confirms, and nothing appends.
    let eval = evaluate(&ledger, &stored, "2026-09-01T14:00:00Z").expect("evaluate");
    assert_eq!(eval.freshness, Freshness::CurrentStateConfirmed);
    assert_eq!(eval.freshness.state().as_str(), "CURRENT_STATE_CONFIRMED");
    assert!(eval.appended_observations.is_empty());
    assert_eq!(eval.bound_at, stored.binding.bound_at, "T1 carried whole");

    // The mutation: an unstaged tracked edit.
    write(root, "base.txt", "mutated\n");
    let eval = evaluate(&ledger, &stored, "2026-09-01T14:05:00Z").expect("evaluate");
    let Freshness::Stale { bound, current } = &eval.freshness else {
        panic!("a mutated workspace must be STALE: {:?}", eval.freshness);
    };
    assert_ne!(bound, current);
    assert_eq!(eval.freshness.state().as_str(), "STALE");
    assert_eq!(
        eval.appended_observations.len(),
        1,
        "the detection is recorded on the chain"
    );
    let stale_rows = of_type(&ledger, ObservationType::EvidenceStale).expect("read");
    assert_eq!(stale_rows.len(), 1);
    assert_eq!(
        stale_rows[0].observed_at, "2026-09-01T14:05:00Z",
        "the detection carries its own moment"
    );

    // Re-evaluating the same staleness is not a second detection.
    let again = evaluate(&ledger, &stored, "2026-09-01T14:06:00Z").expect("evaluate");
    assert!(matches!(again.freshness, Freshness::Stale { .. }));
    assert!(
        again.appended_observations.is_empty(),
        "the same detected state appends nothing new"
    );
    assert_eq!(
        of_type(&ledger, ObservationType::EvidenceStale)
            .expect("read")
            .len(),
        1
    );

    // A FURTHER mutation is a genuinely novel detection.
    write(root, "base.txt", "mutated again\n");
    let further = evaluate(&ledger, &stored, "2026-09-01T14:07:00Z").expect("evaluate");
    assert!(matches!(further.freshness, Freshness::Stale { .. }));
    assert_eq!(further.appended_observations.len(), 1);
    assert_eq!(
        of_type(&ledger, ObservationType::EvidenceStale)
            .expect("read")
            .len(),
        2,
        "each detected current state is one row, once"
    );
    assert_eq!(verify_chain(&ledger).expect("verify"), 2);
}

/// §33's mutation matrix, each class in a fresh repository with its own
/// positive control. Every §34 case must flip — no tolerance.
#[test]
fn every_workspace_mutation_class_flips_freshness_to_stale() {
    type Mutation<'a> = (&'a str, Box<dyn Fn(&Path)>);
    let mutations: Vec<Mutation<'_>> = vec![
        (
            "staged change",
            Box::new(|root| {
                write(root, "base.txt", "staged\n");
                git(root, &["add", "base.txt"]);
            }),
        ),
        (
            "unstaged tracked edit",
            Box::new(|root| write(root, "base.txt", "dirty\n")),
        ),
        (
            "rename",
            Box::new(|root| git(root, &["mv", "base.txt", "renamed.txt"])),
        ),
        (
            "deleted tracked file",
            Box::new(|root| std::fs::remove_file(root.join("base.txt")).expect("rm")),
        ),
        (
            "relevant untracked file",
            Box::new(|root| write(root, "novel.txt", "untracked\n")),
        ),
        (
            "new commit (different HEAD)",
            Box::new(|root| {
                write(root, "base.txt", "advanced\n");
                git(root, &["add", "."]);
                git(root, &["commit", "-q", "-m", "advance"]);
            }),
        ),
        #[cfg(unix)]
        (
            "file mode (bare chmod +x)",
            Box::new(|root| {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    root.join("base.txt"),
                    std::fs::Permissions::from_mode(0o755),
                )
                .expect("chmod");
            }),
        ),
        #[cfg(unix)]
        (
            "symlink retarget",
            Box::new(|root| {
                std::fs::remove_file(root.join("link")).expect("unlink");
                std::os::unix::fs::symlink("second.txt", root.join("link")).expect("relink");
            }),
        ),
    ];

    for (name, mutate) in mutations {
        let dir = seeded_repo();
        let root = dir.path();
        #[cfg(unix)]
        {
            // The symlink case needs a link to retarget; it is part of the
            // bound state for every case, harmlessly.
            std::os::unix::fs::symlink("base.txt", root.join("link")).expect("symlink");
        }
        let ledger = Ledger::open_in_memory().expect("ledger");
        let stored = bind_now(&ledger, root, "cargo test");

        // Positive control first: the flip must be the manipulation's.
        assert_eq!(
            freshness_of(&ledger, &stored, "2026-09-01T15:00:00Z"),
            Freshness::CurrentStateConfirmed,
            "positive control failed before the {name} mutation"
        );
        mutate(root);
        let after = freshness_of(&ledger, &stored, "2026-09-01T15:01:00Z");
        assert!(
            matches!(after, Freshness::Stale { .. }),
            "{name} must flip freshness to STALE, got {after:?}"
        );
    }
}

#[test]
fn a_fresh_rerun_on_the_final_state_supersedes_the_stale_binding() {
    let dir = seeded_repo();
    let root = dir.path();
    let ledger = Ledger::open_in_memory().expect("ledger");

    // T0: bind at S1.
    let b1 = bind_now(&ledger, root, "cargo test");

    // S1 → S2; T1: the old evidence is detected stale, and the record of
    // that detection is permanent.
    write(root, "base.txt", "final state\n");
    let t1 = evaluate(&ledger, &b1, "2026-09-01T16:00:00Z").expect("evaluate");
    assert!(matches!(t1.freshness, Freshness::Stale { .. }));

    // The fresh rerun on the final state binds new and confirms.
    let b2 = bind_now(&ledger, root, "cargo test");
    assert_eq!(
        b1.binding.scope.key, b2.binding.scope.key,
        "same suite, same scope key — that is what supersession means"
    );
    let e2 = evaluate(&ledger, &b2, "2026-09-01T16:05:00Z").expect("evaluate");
    assert_eq!(e2.freshness, Freshness::CurrentStateConfirmed);

    // The old binding is now invalidated by the later change — a later
    // binding covers the current state.
    let t2 = evaluate(&ledger, &b1, "2026-09-01T16:06:00Z").expect("evaluate");
    let Freshness::InvalidatedByLaterChange { superseded_by } = &t2.freshness else {
        panic!("superseded evidence must say so: {:?}", t2.freshness);
    };
    assert_eq!(superseded_by, &b2.id);
    assert_eq!(t2.freshness.state().as_str(), "INVALIDATED_BY_LATER_CHANGE");
    assert_eq!(
        of_type(&ledger, ObservationType::EvidenceInvalidated)
            .expect("read")
            .len(),
        1
    );

    // Temporal truth: T1's STALE row is still on the chain, untouched.
    let stale_rows = of_type(&ledger, ObservationType::EvidenceStale).expect("read");
    assert_eq!(stale_rows.len(), 1, "T1 is never rewritten");
    assert_eq!(stale_rows[0].observed_at, "2026-09-01T16:00:00Z");

    // The scope story rolls up as §9's SUBSEQUENTLY_CONFIRMED: unsupported
    // (stale) at T1, confirmed at T2 — both moments carried, distinct from
    // a plain CURRENT_STATE_CONFIRMED.
    let story = scope_story(&ledger, &b2, "2026-09-01T16:07:00Z").expect("story");
    assert_eq!(story.state, EvidenceState::SubsequentlyConfirmed);
    assert_eq!(story.state.as_str(), "SUBSEQUENTLY_CONFIRMED");
    assert!(story.prior_detections >= 1);
    assert_eq!(
        story.first_detection_at.as_deref(),
        Some("2026-09-01T16:00:00Z"),
        "the story names T1"
    );
    assert_eq!(story.latest.evaluated_at, "2026-09-01T16:07:00Z");
}

/// The revert arm: the same binding goes stale at T1 and the workspace
/// later returns byte-for-byte to the bound state. The binding confirms
/// again at T2 — and the story still says SUBSEQUENTLY_CONFIRMED, because
/// T1 happened and stays recorded.
#[test]
fn a_reverted_workspace_reconfirms_without_rewriting_t1() {
    let dir = seeded_repo();
    let root = dir.path();
    let ledger = Ledger::open_in_memory().expect("ledger");
    let stored = bind_now(&ledger, root, "./test.sh");

    write(root, "base.txt", "storm\n");
    let t1 = evaluate(&ledger, &stored, "2026-09-01T17:00:00Z").expect("evaluate");
    assert!(matches!(t1.freshness, Freshness::Stale { .. }));

    write(root, "base.txt", "base\n"); // byte-identical revert
    let t2 = evaluate(&ledger, &stored, "2026-09-01T17:10:00Z").expect("evaluate");
    assert_eq!(t2.freshness, Freshness::CurrentStateConfirmed);

    let story = scope_story(&ledger, &stored, "2026-09-01T17:11:00Z").expect("story");
    assert_eq!(story.state, EvidenceState::SubsequentlyConfirmed);
    assert_eq!(
        of_type(&ledger, ObservationType::EvidenceStale)
            .expect("read")
            .len(),
        1,
        "the T1 detection survives the reconfirmation"
    );

    // A scope with NO recorded failure reads plain CURRENT_STATE_CONFIRMED —
    // the two states stay distinct.
    let clean_dir = seeded_repo();
    let clean = bind_now(&ledger, clean_dir.path(), "./test.sh");
    let clean_story = scope_story(&ledger, &clean, "2026-09-01T17:12:00Z").expect("story");
    assert_eq!(clean_story.state, EvidenceState::CurrentStateConfirmed);
    assert_eq!(clean_story.prior_detections, 0);
}

#[test]
fn a_binding_that_never_bracketed_one_state_is_unknown() {
    let dir = seeded_repo();
    let root = dir.path();
    let ledger = Ledger::open_in_memory().expect("ledger");

    // Two REAL observations straddling a real mutation: pre ≠ post.
    let pre = fingerprint(root);
    write(root, "base.txt", "moved between observations\n");
    let post = fingerprint(root);
    assert_ne!(pre.identity(), post.identity());

    let mut seed = bind_now(&ledger, root, "cargo test").binding;
    seed.pre = IdentityEdge::complete(pre.identity().to_string());
    seed.post = IdentityEdge::complete(post.identity().to_string());
    seed.bound_at = "2026-09-01T18:00:00.500Z".into();
    let StoreOutcome::Stored(straddled) = store(&ledger, &seed).expect("store") else {
        panic!("distinct content");
    };
    let eval = evaluate(&ledger, &straddled, "2026-09-01T18:01:00Z").expect("evaluate");
    let Freshness::Unknown { reason } = &eval.freshness else {
        panic!("a straddled run confirms nothing: {:?}", eval.freshness);
    };
    assert!(
        reason.contains("straddled two workspace states"),
        "the reason names the straddle: {reason}"
    );
    assert!(
        eval.appended_observations.is_empty(),
        "UNKNOWN establishes nothing and appends nothing"
    );

    // An incomplete edge is equally unbindable, with its reason carried.
    let mut seed2 = seed.clone();
    seed2.post = IdentityEdge::incomplete("the tree moved mid-observation".into());
    seed2.bound_at = "2026-09-01T18:02:00.500Z".into();
    let StoreOutcome::Stored(incomplete) = store(&ledger, &seed2).expect("store") else {
        panic!("distinct content");
    };
    let eval = evaluate(&ledger, &incomplete, "2026-09-01T18:03:00Z").expect("evaluate");
    let Freshness::Unknown { reason } = &eval.freshness else {
        panic!("an incomplete edge confirms nothing: {:?}", eval.freshness);
    };
    assert!(reason.contains("the tree moved mid-observation"));
}

#[test]
fn an_interrupted_re_observation_is_an_incomplete_search_not_a_verdict() {
    let dir = seeded_repo();
    let root = dir.path().to_path_buf();
    let ledger = Ledger::open_in_memory().expect("ledger");
    let stored = bind_now(&ledger, &root, "cargo test");

    // A REAL interleaved write through the Stage 3 seam — no sleeps.
    let eval = evaluate_with(&ledger, &stored, "2026-09-01T19:00:00Z", || {
        write(&root, "mid-evaluation.txt", "moved\n");
    })
    .expect("evaluate");
    let Freshness::IncompleteSearch { reason } = &eval.freshness else {
        panic!(
            "a moving tree yields INCOMPLETE_SEARCH, never STALE or a \
             confirmation: {:?}",
            eval.freshness
        );
    };
    assert!(reason.contains("changed during observation"), "{reason}");
    assert_eq!(eval.freshness.state().as_str(), "INCOMPLETE_SEARCH");
    assert!(
        eval.appended_observations.is_empty(),
        "nothing was established, so nothing is recorded"
    );

    // And the settled tree still evaluates: the interruption verdict was
    // about the search, not the workspace. (The interleaved write made the
    // tree genuinely stale — the untracked file stayed.)
    let settled = evaluate(&ledger, &stored, "2026-09-01T19:01:00Z").expect("evaluate");
    assert!(matches!(settled.freshness, Freshness::Stale { .. }));
}

#[test]
fn a_vanished_workspace_is_an_incomplete_search_with_the_reason_stated() {
    let dir = seeded_repo();
    let ledger = Ledger::open_in_memory().expect("ledger");
    let stored = bind_now(&ledger, dir.path(), "cargo test");
    drop(dir); // the workspace is gone

    let eval = evaluate(&ledger, &stored, "2026-09-01T20:00:00Z").expect("evaluate");
    let Freshness::IncompleteSearch { reason } = &eval.freshness else {
        panic!(
            "a missing workspace cannot be inspected — INCOMPLETE_SEARCH, \
             never a confirmation or a staleness verdict: {:?}",
            eval.freshness
        );
    };
    assert!(
        reason.contains("could not be observed"),
        "the reason says what failed: {reason}"
    );
}
