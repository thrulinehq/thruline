//! B4 reads Actions workflow objects without executing them.

#[path = "support/person_action.rs"]
mod person_action;

use std::fs;
use std::path::Path;
use std::process::Command;

use person_action::PersonRecorder;
use rusqlite::params;
use serde_json::json;
use tempfile::TempDir;
use thruline_core::decisions::checks::{CheckOutcome, VerdictState};
use thruline_core::decisions::checks_preservation::CiStepPresentParams;
use thruline_core::decisions::{ci_adapter, engine};
use thruline_core::ledger::Ledger;

const WORKFLOW: &str = ".github/workflows/ci.yml";
const BASE: &str = "name: Proof\non: [push]\njobs:\n  check:\n    runs-on: ubuntu-latest\n    steps:\n      - name: proof\n        run: coverage report --fail-under=80\n";

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git runs");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("Git text")
        .trim()
        .into()
}

struct Fixture {
    repo: TempDir,
    baseline: String,
    params: CiStepPresentParams,
}

impl Fixture {
    fn new(source: &str, selector: &str, threshold: Option<u32>) -> Self {
        let repo = tempfile::tempdir().expect("scratch repo");
        git(repo.path(), &["init", "-q"]);
        git(repo.path(), &["config", "user.name", "Fixture"]);
        git(
            repo.path(),
            &["config", "user.email", "fixture@example.invalid"],
        );
        fs::create_dir_all(repo.path().join(".github/workflows")).expect("workflow folder");
        fs::write(repo.path().join(WORKFLOW), source).expect("baseline workflow");
        git(repo.path(), &["add", "."]);
        git(
            repo.path(),
            &["commit", "-qm", "accepted workflow baseline"],
        );
        let baseline = git(repo.path(), &["rev-parse", "HEAD"]);
        Self {
            repo,
            baseline,
            params: CiStepPresentParams {
                workflow: WORKFLOW.into(),
                step: selector.into(),
                threshold_pct: threshold,
            },
        }
    }

    fn version(&self, source: &str) -> String {
        fs::write(self.repo.path().join(WORKFLOW), source).expect("target workflow");
        git(self.repo.path(), &["add", "-A"]);
        git(
            self.repo.path(),
            &["commit", "--allow-empty", "-qm", "target workflow"],
        );
        git(self.repo.path(), &["rev-parse", "HEAD"])
    }

    fn check(&self, version: &str) -> CheckOutcome {
        let tree = git(
            self.repo.path(),
            &["rev-parse", &format!("{version}^{{tree}}")],
        );
        ci_adapter::check_at(
            self.repo.path(),
            version,
            &tree,
            &self.baseline,
            &self.params,
        )
    }
}

#[test]
fn b4_accepted_engine_holds_then_breaks_and_replays_identically() {
    let f = Fixture::new(BASE, "proof", Some(80));
    let held = f.version(&BASE.replace("=80", "=85"));
    let broken = f.version(&BASE.replace("=80", "=70"));
    let ledger =
        Ledger::open(f.repo.path().join(".git/decision-ledger.db")).expect("scratch ledger");
    ledger.conn().execute("INSERT INTO projects(id,root_path,source,created_at) VALUES('b4-project',?1,'codex','2026-10-07T00:00:00Z')", [f.repo.path().to_string_lossy().as_ref()]).expect("project");
    ledger.conn().execute("INSERT INTO decisions(id,project_id,created_at) VALUES('b4-decision','b4-project','2026-10-07T00:00:00Z')", []).expect("decision");
    ledger.conn().execute("INSERT INTO decision_revisions(decision_id,rev,kind,params_schema_version,params_json,text,sources_json,author_actor,recorded_at) VALUES('b4-decision',1,'ci_step_present',1,?1,'Keep proof at eighty percent','[]','person','2026-10-07T00:00:00Z')", [json!({"kind":"ci_step_present","workflow":WORKFLOW,"step":"proof","threshold_pct":80}).to_string()]).expect("revision");
    ledger.conn().execute("INSERT INTO decision_acceptances(id,decision_id,rev,actor,accepted_at,rationale,severity,applies_from_version) VALUES('b4-accept','b4-decision',1,'person','2026-10-07T00:01:00Z','keep the proof','blocks',?1)", params![f.baseline]).expect("acceptance");
    let mut person = PersonRecorder::new(&ledger, "b4-project");
    person.bind_acceptance(&ledger, "b4-accept");
    for (version, expected) in [
        (&held, VerdictState::Holds),
        (&broken, VerdictState::Broken),
    ] {
        let stored = engine::check_decision_version(
            &ledger,
            "b4-project",
            f.repo.path(),
            version,
            "2026-10-07T00:02:00Z",
            false,
            "b4-decision",
        )
        .expect("check");
        assert_eq!(stored[0].state, expected, "{:?}", stored[0]);
        let replay = engine::check_decision_version(
            &ledger,
            "b4-project",
            f.repo.path(),
            version,
            "2026-10-07T00:03:00Z",
            true,
            "b4-decision",
        )
        .expect("replay");
        assert_eq!(stored[0].outcome, replay[0].outcome);
    }
}

#[test]
fn b4_removed_named_step_or_workflow_is_broken() {
    let f = Fixture::new(BASE, "proof", Some(80));
    let renamed = f.version(&BASE.replace("name: proof", "name: other"));
    assert_eq!(f.check(&renamed).state, VerdictState::Broken);
    fs::remove_file(f.repo.path().join(WORKFLOW)).expect("remove workflow");
    git(f.repo.path(), &["add", "-A"]);
    git(f.repo.path(), &["commit", "-qm", "remove workflow"]);
    let removed = git(f.repo.path(), &["rev-parse", "HEAD"]);
    let result = f.check(&removed);
    assert_eq!(result.state, VerdictState::Broken);
    assert_eq!(result.findings[0].path, WORKFLOW);
}

#[test]
fn b4_new_bypass_kinds_break_and_existing_other_bypass_cannot_mask_a_new_one() {
    for source in [
        BASE.replace("=80", "=80 || true"),
        BASE.replace(
            "        run:",
            "        continue-on-error: true\n        run:",
        ),
        BASE.replace("    runs-on:", "    continue-on-error: true\n    runs-on:"),
    ] {
        let f = Fixture::new(BASE, "proof", Some(80));
        let version = f.version(&source);
        assert_eq!(f.check(&version).state, VerdictState::Broken, "{source}");
    }
    let base = format!(
        "{BASE}      - name: helper\n        continue-on-error: true\n        run: echo helper\n"
    );
    let f = Fixture::new(&base, "proof", Some(80));
    let relocated = base.replace(
        "      - name: helper\n",
        "      - name: harmless\n        run: echo harmless\n      - name: helper\n",
    );
    let version = f.version(&relocated);
    assert_eq!(
        f.check(&version).state,
        VerdictState::Holds,
        "moving an unchanged named helper does not introduce a bypass"
    );
    let target = base.replace(
        "      - name: proof\n",
        "      - name: proof\n        continue-on-error: true\n",
    );
    let version = f.version(&target);
    assert_eq!(f.check(&version).state, VerdictState::Broken);
}

#[test]
fn b4_comments_and_quoted_bypass_words_are_inert() {
    let base = format!("{BASE}      - name: helper\n        run: echo '|| true'\n# coverageThreshold: 1\n# continue-on-error: true\n");
    let f = Fixture::new(&base, "proof", Some(80));
    let target = f.version(&base.replace("# coverageThreshold: 1", "# coverageThreshold: 99"));
    assert_eq!(f.check(&target).state, VerdictState::Holds);
    let comment_only = BASE.replace(
        "coverage report --fail-under=80",
        "cargo test\n# fail_under: 80",
    );
    let f = Fixture::new(&comment_only, "proof", Some(80));
    assert_eq!(f.check(&f.baseline).state, VerdictState::Unverifiable);
}

#[test]
fn b4_literal_fail_under_and_coverage_threshold_inputs_are_preserved() {
    for key in ["fail_under", "coverageThreshold"] {
        let base = BASE.replace(
            "run: coverage report --fail-under=80",
            &format!("uses: example/coverage@v1\n        with:\n          {key}: 80"),
        );
        let f = Fixture::new(&base, "proof", Some(80));
        let same = f.version(&base);
        assert_eq!(f.check(&same).state, VerdictState::Holds);
        let lowered = f.version(&base.replace(&format!("{key}: 80"), &format!("{key}: 70")));
        assert_eq!(f.check(&lowered).state, VerdictState::Broken);
        let removed =
            f.version(&base.replace(&format!("        with:\n          {key}: 80\n"), ""));
        assert_eq!(f.check(&removed).state, VerdictState::Broken);
    }
}

#[test]
fn b4_unique_job_selector_is_supported_and_job_removal_breaks() {
    let f = Fixture::new(BASE, "check", Some(80));
    assert_eq!(f.check(&f.baseline).state, VerdictState::Holds);
    let renamed = f.version(&BASE.replace("  check:", "  replacement:"));
    assert_eq!(f.check(&renamed).state, VerdictState::Broken);
}

#[test]
fn b4_duplicate_keys_anchors_documents_and_ambiguous_steps_are_unverifiable() {
    for source in [
        BASE.replace("    runs-on:", "    runs-on: macos-latest\n    runs-on:"),
        BASE.replace("  check:", "  check: &job"),
        format!("{BASE}\n---\nname: second\n"),
        format!("{BASE}      - name: proof\n        run: cargo test\n"),
    ] {
        let f = Fixture::new(BASE, "proof", Some(80));
        let version = f.version(&source);
        assert_eq!(
            f.check(&version).state,
            VerdictState::Unverifiable,
            "{source}"
        );
    }
}

#[test]
fn b4_dynamic_threshold_conditions_and_shell_constructs_are_unverifiable() {
    for source in [
        BASE.replace("--fail-under=80", "--fail-under=${{ vars.PERCENT }}"),
        BASE.replace("        run:", "        if: success()\n        run:"),
        BASE.replace(
            "coverage report --fail-under=80",
            "sh -c 'coverage report --fail-under=80 || true'",
        ),
        BASE.replace(
            "coverage report --fail-under=80",
            "coverage report --fail-under=80 | tee proof.log",
        ),
    ] {
        let f = Fixture::new(BASE, "proof", Some(80));
        let version = f.version(&source);
        assert_eq!(
            f.check(&version).state,
            VerdictState::Unverifiable,
            "{source}"
        );
    }
}

#[test]
fn b4_changed_execution_body_cannot_hold_under_an_unchanged_step_label() {
    let f = Fixture::new(BASE, "proof", None);
    let version = f.version(&BASE.replace("coverage report --fail-under=80", "echo proof"));
    assert_eq!(f.check(&version).state, VerdictState::Unverifiable);
}

#[test]
fn b4_worktree_changes_do_not_change_exact_git_verdict() {
    let f = Fixture::new(BASE, "proof", Some(80));
    fs::write(f.repo.path().join(WORKFLOW), "malformed: [\n").expect("working tree differs");
    assert_eq!(f.check(&f.baseline).state, VerdictState::Holds);
}

#[test]
fn b4_missing_baseline_wrong_tree_refs_and_non_regular_blobs_abstain() {
    let f = Fixture::new(BASE, "proof", Some(80));
    let tree = git(f.repo.path(), &["rev-parse", "HEAD^{tree}"]);
    for (version, claimed_tree, baseline) in [
        (&f.baseline, "0".repeat(40), f.baseline.clone()),
        (&f.baseline, tree.clone(), "0".repeat(40)),
        (&f.baseline, tree.clone(), "HEAD".into()),
    ] {
        assert_eq!(
            ci_adapter::check_at(f.repo.path(), version, &claimed_tree, &baseline, &f.params).state,
            VerdictState::Unverifiable
        );
    }
    assert_eq!(
        ci_adapter::check_at(f.repo.path(), "HEAD", &tree, &f.baseline, &f.params).state,
        VerdictState::Unverifiable
    );
    #[cfg(unix)]
    {
        fs::remove_file(f.repo.path().join(WORKFLOW)).expect("replace workflow");
        std::os::unix::fs::symlink("elsewhere.yml", f.repo.path().join(WORKFLOW)).expect("symlink");
        git(f.repo.path(), &["add", "-A"]);
        git(f.repo.path(), &["commit", "-qm", "workflow symlink"]);
        let version = git(f.repo.path(), &["rev-parse", "HEAD"]);
        assert_eq!(f.check(&version).state, VerdictState::Unverifiable);
    }
}

#[test]
fn b4_oversized_workflow_and_unknown_threshold_shapes_are_unverifiable() {
    let f = Fixture::new(BASE, "proof", Some(80));
    let version = f.version(&" ".repeat(2 * 1024 * 1024 + 1));
    assert_eq!(f.check(&version).state, VerdictState::Unverifiable);
    let dynamic = BASE.replace("run: coverage report --fail-under=80", "uses: example/coverage@v1\n        with:\n          coverageThreshold:\n            statements: 80");
    let version = f.version(&dynamic);
    assert_eq!(f.check(&version).state, VerdictState::Unverifiable);
}

#[test]
fn b4_unrelated_threshold_keys_do_not_supply_an_execution_threshold() {
    let source = BASE.replace(
        "run: coverage report --fail-under=80",
        "run: cargo test\n        env:\n          fail_under: 80",
    );
    let f = Fixture::new(&source, "proof", Some(80));
    assert_eq!(f.check(&f.baseline).state, VerdictState::Unverifiable);
    let f = Fixture::new(BASE, "proof", Some(80));
    let source = BASE
        .replace("coverage report --fail-under=80", "coverage report")
        .replace(
            "        run:",
            "        env:\n          coverageThreshold: 80\n        run:",
        );
    let version = f.version(&source);
    assert_eq!(f.check(&version).state, VerdictState::Broken);
}

#[test]
fn b4_changed_parent_conditions_matrix_or_triggers_cannot_hold() {
    let f = Fixture::new(BASE, "proof", Some(80));
    let disabled = f.version(&BASE.replace("    runs-on:", "    if: false\n    runs-on:"));
    assert_eq!(f.check(&disabled).state, VerdictState::Broken);
    for source in [BASE.replace("on: [push]", "on: [workflow_dispatch]"), BASE.replace("    runs-on:", "    strategy:\n      matrix:\n        os: [ubuntu-latest]\n        exclude:\n          - os: ubuntu-latest\n    runs-on:")] {
        let version = f.version(&source);
        assert_eq!(f.check(&version).state, VerdictState::Unverifiable);
    }
}

#[test]
fn b4_dangling_logical_operators_are_unverifiable() {
    let f = Fixture::new(BASE, "proof", Some(80));
    for suffix in [
        " &&",
        " ||",
        " &&\n          # no next command",
        " || # no next command",
    ] {
        let version =
            f.version(&BASE.replace("--fail-under=80", &format!("--fail-under=80{suffix}")));
        assert_eq!(
            f.check(&version).state,
            VerdictState::Unverifiable,
            "dangling {suffix:?}"
        );
    }
}

#[test]
fn b4_selector_collisions_and_retargeting_cannot_fabricate_a_break_or_hold() {
    let f = Fixture::new(BASE, "check", Some(80));
    let colliding = format!("{BASE}  elsewhere:\n    runs-on: ubuntu-latest\n    steps:\n      - name: check\n        if: false\n        run: coverage report --fail-under=80\n");
    let version = f.version(&colliding);
    assert_eq!(
        f.check(&version).state,
        VerdictState::Unverifiable,
        "accepted job remains; another disabled same-name step is ambiguous"
    );
    let retargeted = BASE
        .replace("  check:", "  elsewhere:")
        .replace("name: proof", "name: check")
        .replace("        run:", "        if: false\n        run:");
    let version = f.version(&retargeted);
    assert_eq!(
        f.check(&version).state,
        VerdictState::Unverifiable,
        "bare selector cannot change from accepted job id to a step name"
    );
}

#[test]
fn b4_unnamed_bypass_moves_abstain_instead_of_fabricating_introduction() {
    let base = format!("{BASE}      - continue-on-error: true\n        run: echo helper\n");
    let f = Fixture::new(&base, "proof", Some(80));
    let source = base.replace(
        "      - continue-on-error: true\n",
        "      - run: echo harmless\n      - continue-on-error: true\n",
    );
    let version = f.version(&source);
    assert_eq!(f.check(&version).state, VerdictState::Unverifiable);
}

#[test]
fn b4_allow_failure_has_no_proved_actions_semantics_and_abstains() {
    let f = Fixture::new(BASE, "proof", Some(80));
    for source in [
        BASE.replace("        run:", "        allow_failure: true\n        run:"),
        BASE.replace("    runs-on:", "    allow_failure: true\n    runs-on:"),
    ] {
        let version = f.version(&source);
        assert_eq!(f.check(&version).state, VerdictState::Unverifiable);
    }
}
