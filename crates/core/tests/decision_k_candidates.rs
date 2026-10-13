//! Family K source-delta signals are advisory. A signal is never a Decisions
//! verdict, and absent fixture/actor provenance remains Unverifiable.

use std::fs;
use std::path::Path;
use std::process::Command;

use serde::Deserialize;
use thruline_core::decisions::checks_k_candidates::{
    scan_k_candidate, scan_k_candidates, KCandidateState,
};

#[derive(Debug, Deserialize)]
struct Case {
    code: String,
    path: String,
    baseline: String,
    signal: String,
    benign_path: String,
    benign: String,
}

fn cases() -> Vec<Case> {
    serde_json::from_str(include_str!("fixtures/decisions/k_candidates/cases.json"))
        .expect("valid fixture cases")
}

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .expect("git available");
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git utf8")
        .trim()
        .into()
}

fn fixture_repo(
    path: &str,
    baseline: &str,
    next_path: &str,
    next: &str,
) -> (tempfile::TempDir, String, String) {
    let temp = tempfile::tempdir().expect("scratch repo");
    let repo = temp.path();
    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.name", "K Fixture"]);
    git(repo, &["config", "user.email", "fixture@example.invalid"]);
    let baseline_path = repo.join(path);
    fs::create_dir_all(baseline_path.parent().expect("parent")).expect("baseline dir");
    fs::write(&baseline_path, baseline).expect("baseline source");
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "baseline"]);
    let base = git(repo, &["rev-parse", "HEAD"]);
    let next_file = repo.join(next_path);
    fs::create_dir_all(next_file.parent().expect("parent")).expect("next dir");
    fs::write(next_file, next).expect("next source");
    git(repo, &["add", "."]);
    git(repo, &["commit", "--allow-empty", "-qm", "target"]);
    let target = git(repo, &["rev-parse", "HEAD"]);
    (temp, base, target)
}

fn check_case(code: &str, signal_expected: KCandidateState, benign_expected: KCandidateState) {
    let case = cases()
        .into_iter()
        .find(|case| case.code == code)
        .expect("case");
    let (temp, baseline, target) =
        fixture_repo(&case.path, &case.baseline, &case.path, &case.signal);
    let signal = scan_k_candidate(temp.path(), &baseline, &target, code);
    assert_eq!(signal.state, signal_expected, "{code} signal: {signal:?}");
    if signal.state == KCandidateState::Signal {
        assert!(
            !signal.observations.is_empty(),
            "{code} needs a source receipt"
        );
        assert!(signal.observations.iter().all(|row| row.line > 0));
    } else {
        assert!(signal.observations.is_empty());
        assert!(signal.reason.is_some());
    }
    let (temp, baseline, target) =
        fixture_repo(&case.path, &case.baseline, &case.benign_path, &case.benign);
    let benign = scan_k_candidate(temp.path(), &baseline, &target, code);
    assert_eq!(benign.state, benign_expected, "{code} benign: {benign:?}");
    assert!(benign.observations.is_empty());
}

#[test]
fn k4_tautology_signal_and_real_assertion_control() {
    check_case("K4", KCandidateState::Signal, KCandidateState::NoSignal);
}

#[test]
fn k5_production_test_branch_signal_and_test_path_control() {
    check_case("K5", KCandidateState::Signal, KCandidateState::NoSignal);
}

#[test]
fn k6_lookup_needs_recorded_fixture_input_link() {
    check_case(
        "K6",
        KCandidateState::Unverifiable,
        KCandidateState::NoSignal,
    );
}

#[test]
fn k6_visible_fixture_key_and_source_table_do_not_fabricate_an_accepted_verdict() {
    let temp = tempfile::tempdir().expect("scratch repo");
    let repo = temp.path();
    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.name", "K Fixture"]);
    git(repo, &["config", "user.email", "fixture@example.invalid"]);
    fs::create_dir_all(repo.join("src")).expect("source dir");
    fs::create_dir_all(repo.join("tests/fixtures")).expect("fixture dir");
    fs::write(
        repo.join("src/answer.py"),
        "def answer(value):\n    return compute(value)\n",
    )
    .expect("baseline source");
    fs::write(
        repo.join("tests/fixtures/answers.json"),
        "{\"input\":\"fixture-a\",\"expected\":42}\n",
    )
    .expect("visible input fixture");
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "baseline"]);
    let baseline = git(repo, &["rev-parse", "HEAD"]);
    fs::write(
        repo.join("src/answer.py"),
        "def answer(value):\n    return {'fixture-a': 42}.get(value, compute(value))\n",
    )
    .expect("source lookup table");
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "source lookup"]);
    let target = git(repo, &["rev-parse", "HEAD"]);

    assert_eq!(baseline.len(), 40);
    assert_eq!(target.len(), 40);
    let outcome = scan_k_candidate(repo, &baseline, &target, "K6");
    assert_eq!(outcome.state, KCandidateState::Unverifiable);
    assert_eq!(
        outcome.reason,
        Some("lookup arguments are outside the literal parameter form")
    );
    assert!(outcome.observations.is_empty());
}

#[test]
fn k7_constant_equality_is_a_candidate_not_intent_proof() {
    check_case("K7", KCandidateState::Signal, KCandidateState::NoSignal);
}

#[test]
fn k10_suppression_signal_and_quoted_text_control() {
    check_case("K10", KCandidateState::Signal, KCandidateState::NoSignal);
}

#[test]
fn k11_empty_catch_signal_and_reporting_handler_control() {
    check_case("K11", KCandidateState::Signal, KCandidateState::NoSignal);
}

#[test]
fn k12_stub_signal_and_todo_comment_control() {
    check_case("K12", KCandidateState::Signal, KCandidateState::NoSignal);
}

#[test]
fn k18_rule_text_cannot_prove_cross_session_enforcement() {
    check_case(
        "K18",
        KCandidateState::Unverifiable,
        KCandidateState::Unverifiable,
    );
}

#[test]
fn moved_suppression_line_is_netted_across_source_paths() {
    let temp = tempfile::tempdir().expect("scratch repo");
    let repo = temp.path();
    git(repo, &["init", "-q"]);
    git(repo, &["config", "user.name", "K Fixture"]);
    git(repo, &["config", "user.email", "fixture@example.invalid"]);
    fs::create_dir(repo.join("src")).expect("src");
    fs::write(
        repo.join("src/old.ts"),
        "// @ts-ignore\nexport const x = 1;\n",
    )
    .expect("old");
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "baseline"]);
    let baseline = git(repo, &["rev-parse", "HEAD"]);
    git(repo, &["mv", "src/old.ts", "src/new.ts"]);
    git(repo, &["commit", "-qm", "move"]);
    let target = git(repo, &["rev-parse", "HEAD"]);
    let outcome = scan_k_candidate(repo, &baseline, &target, "K10");
    assert_eq!(outcome.state, KCandidateState::NoSignal);
}

#[test]
fn partial_commit_id_cannot_bind_a_candidate_scan() {
    let case = cases().into_iter().next().expect("case");
    let (temp, baseline, target) =
        fixture_repo(&case.path, &case.baseline, &case.path, &case.signal);
    let outcome = scan_k_candidate(temp.path(), &baseline[..7], &target, &case.code);
    assert_eq!(outcome.state, KCandidateState::Unverifiable);
    assert_eq!(
        outcome.reason,
        Some("exact full commit identity is unavailable")
    );
}

#[test]
fn quoted_candidate_text_does_not_become_a_source_signal() {
    for (code, path, baseline, quoted) in [
        (
            "K5",
            "src/app.ts",
            "export const mode = 'normal';\n",
            "if (label === \"process.env.NODE_ENV === 'test'\") return label;\n",
        ),
        (
            "K10",
            "src/app.ts",
            "export const mode = 'normal';\n",
            "export const guidance = 'as any is prohibited';\n",
        ),
        (
            "K11",
            "src/app.ts",
            "export const mode = 'normal';\n",
            "export const guidance = 'catch {} is prohibited';\n",
        ),
        (
            "K12",
            "src/lib.rs",
            "pub fn answer() -> i32 { 42 }\n",
            "pub fn guidance() -> &'static str { \"todo!() is prohibited\" }\n",
        ),
    ] {
        let (temp, baseline, target) = fixture_repo(path, baseline, path, quoted);
        let outcome = scan_k_candidate(temp.path(), &baseline, &target, code);
        assert_eq!(
            outcome.state,
            KCandidateState::NoSignal,
            "{code}: {outcome:?}"
        );
    }
}

#[test]
fn k10_real_as_any_expression_is_a_candidate_signal() {
    let (temp, baseline, target) = fixture_repo(
        "src/app.ts",
        "export const value = compute();\n",
        "src/app.ts",
        "export const value = compute() as any;\n",
    );
    let outcome = scan_k_candidate(temp.path(), &baseline, &target, "K10");
    assert_eq!(outcome.state, KCandidateState::Signal);
}

#[test]
fn one_exact_git_delta_produces_all_eight_advisory_candidates() {
    let (temp, baseline, target) = fixture_repo(
        "src/app.ts",
        "export const value: number = compute();\n",
        "src/app.ts",
        "// @ts-ignore\nexport const value: number = compute();\n",
    );
    let outcomes = scan_k_candidates(temp.path(), &baseline, &target);
    assert_eq!(
        outcomes
            .iter()
            .map(|row| row.code.as_str())
            .collect::<Vec<_>>(),
        ["K4", "K5", "K6", "K7", "K10", "K11", "K12", "K18"]
    );
    assert_eq!(
        outcomes.iter().map(|row| row.state).collect::<Vec<_>>(),
        [
            KCandidateState::NoSignal,
            KCandidateState::NoSignal,
            KCandidateState::Unverifiable,
            KCandidateState::NoSignal,
            KCandidateState::Signal,
            KCandidateState::NoSignal,
            KCandidateState::NoSignal,
            KCandidateState::Unverifiable,
        ]
    );
}

#[test]
fn generated_dist_bundle_is_outside_source_candidate_scope() {
    let (temp, baseline, target) = fixture_repo(
        "ui/src/app.js",
        "export function load() { return 1; }\n",
        "ui/dist/assets/app.js",
        "try { load(); } catch {}\n",
    );
    let outcome = scan_k_candidate(temp.path(), &baseline, &target, "K11");
    assert_eq!(outcome.state, KCandidateState::NoSignal);
}

#[test]
fn prose_has_any_inside_block_comment_is_not_as_any_suppression() {
    let (temp, baseline, target) = fixture_repo(
        "ui/src/change-view.js",
        "export const ready = true;\n",
        "ui/src/change-view.js",
        "/**\n * The list reports whether any row has any evidence.\n */\nexport const ready = true;\n",
    );
    let outcome = scan_k_candidate(temp.path(), &baseline, &target, "K10");
    assert_eq!(outcome.state, KCandidateState::NoSignal);
}
