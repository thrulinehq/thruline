//! K13–K18 typed-evidence fixtures. Each test was first run against
//! Unverifiable stubs; the red log is recorded in the phase report.

use serde::Deserialize;
use thruline_core::decisions::checks_k_provenance::*;

#[derive(Deserialize)]
struct Fixture {
    rule_file: String,
    ordinary_file: String,
    spec_file: String,
    ordinary_doc: String,
    safe_command: Vec<String>,
    bypass_command: Vec<String>,
    safe_shape_family: String,
    credential_shape_family: String,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("fixtures/decisions/k_provenance/cases.json"))
        .expect("valid fixture")
}

fn version() -> ExactVersion {
    ExactVersion {
        commit: "a".repeat(40),
        tree: "b".repeat(40),
    }
}

fn source() -> SourceRef {
    SourceRef {
        raw_file: "record/session.jsonl".into(),
        raw_line: 7,
    }
}

fn census() -> KCensus {
    KCensus {
        version: version(),
        complete: true,
        edits: Vec::new(),
        commands: Vec::new(),
        claims: Vec::new(),
        runs: Vec::new(),
        credential_scan: None,
        compactions: Vec::new(),
        accepted_rules: Vec::new(),
        rule_checks: Vec::new(),
    }
}

fn edit(id: &str, path: &str, actor: KActor) -> KEdit {
    KEdit {
        id: id.into(),
        session_id: "s1".into(),
        version: version(),
        actor,
        person_action_id: (actor == KActor::Person).then(|| "verified-person-act-1".into()),
        path: path.into(),
        line: 4,
        source: source(),
    }
}

fn command(id: &str, argv: Vec<String>) -> KCommand {
    KCommand {
        id: id.into(),
        session_id: "s1".into(),
        version: version(),
        actor: KActor::Agent,
        source: source(),
        parser_id: "shell-parser-v1".into(),
        parse_complete: true,
        invocations: vec![ShellInvocation {
            preceded_by: ShellOperator::Start,
            argv,
        }],
    }
}

fn claim() -> KClaim {
    KClaim {
        id: "claim-1".into(),
        session_id: "s1".into(),
        version: version(),
        seq: 9,
        kind: KClaimKind::TestsPass,
        bound_run_id: Some("run-1".into()),
        source: source(),
    }
}

fn run() -> KRun {
    KRun {
        id: "run-1".into(),
        version: version(),
        seq: 8,
        complete: true,
        exit_code: Some(0),
        passed: Some(3),
        failed: Some(0),
        errors: Some(0),
        source: source(),
    }
}

fn compaction() -> KCompaction {
    KCompaction {
        id: "compact-1".into(),
        session_id: "s1".into(),
        seq: 5,
        version: version(),
        source: source(),
    }
}

fn accepted_rule() -> KAcceptedRule {
    KAcceptedRule {
        id: "rule-1".into(),
        kind: RuleKind::ForbiddenCommand,
        person_action_id: "verified-person-act-1".into(),
        source: source(),
    }
}

fn rule_check(state: KState) -> KRuleCheck {
    KRuleCheck {
        id: "check-1".into(),
        rule_id: "rule-1".into(),
        kind: RuleKind::ForbiddenCommand,
        session_id: "s1".into(),
        seq: 6,
        version: version(),
        deterministic: true,
        state,
        source: source(),
    }
}

#[test]
fn k13_agent_rule_file_edit_breaks_but_person_edit_same_version_controls() {
    let f = fixture();
    let mut c = census();
    c.edits.push(edit("agent-1", &f.rule_file, KActor::Agent));
    let broken = check_k13_rule_files(&c);
    assert_eq!(broken.state, KState::Broken);
    assert_eq!(broken.findings.len(), 1);
    assert_eq!(broken.findings[0].path, f.rule_file);
    c.edits.push(edit("person-1", &f.rule_file, KActor::Person));
    assert_eq!(check_k13_rule_files(&c).state, KState::Holds);
    c.edits.clear();
    c.edits
        .push(edit("ordinary", &f.ordinary_file, KActor::Agent));
    assert_eq!(check_k13_rule_files(&c).state, KState::Holds);
}

#[test]
fn k13_unknown_person_authority_and_incomplete_census_abstain() {
    let f = fixture();
    let mut c = census();
    c.edits.push(edit("agent-1", &f.rule_file, KActor::Agent));
    let mut person = edit("person-1", &f.rule_file, KActor::Person);
    person.person_action_id = None;
    c.edits.push(person);
    assert_eq!(check_k13_rule_files(&c).state, KState::Unverifiable);
    c.edits.clear();
    c.complete = false;
    assert_eq!(check_k13_rule_files(&c).state, KState::Unverifiable);
}

#[test]
fn k13_person_edit_to_another_file_does_not_exempt_agent_edit() {
    let f = fixture();
    let mut c = census();
    c.edits.push(edit("agent-1", &f.rule_file, KActor::Agent));
    c.edits.push(edit("person-1", "CLAUDE.md", KActor::Person));
    assert_eq!(check_k13_rule_files(&c).state, KState::Broken);
}

#[test]
fn k14_shape_is_masked_advisory_and_clean_scan_has_no_absence_proof() {
    let f = fixture();
    let mut c = census();
    c.credential_scan = Some(CredentialScan {
        version: version(),
        complete: true,
        detector_id: "capture-masker-and-known-prefix-v1".into(),
        shapes: vec![CredentialShape {
            id: "shape-1".into(),
            path: "src/config.rs".into(),
            line: 4,
            source: source(),
            masker_fired: true,
            known_credential_prefix: true,
            family: f.credential_shape_family,
        }],
    });
    let signal = check_k14_credentials(&c);
    assert_eq!(signal.state, KState::Unverifiable);
    assert_eq!(signal.findings.len(), 1);
    assert_eq!(signal.findings[0].marker, "credential_shape_candidate");
    assert!(!format!("{signal:?}").contains("secret-value"));
    c.credential_scan.as_mut().expect("scan").shapes.clear();
    c.credential_scan.as_mut().expect("scan").detector_id = f.safe_shape_family;
    assert_eq!(check_k14_credentials(&c).state, KState::Unverifiable);
}

#[test]
fn k15_parsed_verification_bypass_breaks_and_safe_command_holds() {
    let f = fixture();
    let mut c = census();
    c.commands.push(command("safe-1", f.safe_command));
    assert_eq!(check_k15_bypass_commands(&c).state, KState::Holds);
    c.commands.push(command("bypass-1", f.bypass_command));
    let broken = check_k15_bypass_commands(&c);
    assert_eq!(broken.state, KState::Broken);
    assert_eq!(broken.findings.len(), 1);
    c.commands[1].invocations = vec![
        ShellInvocation {
            preceded_by: ShellOperator::Start,
            argv: vec!["cargo".into(), "test".into()],
        },
        ShellInvocation {
            preceded_by: ShellOperator::Or,
            argv: vec!["true".into()],
        },
    ];
    assert_eq!(check_k15_bypass_commands(&c).state, KState::Broken);
    c.commands[1].invocations = vec![ShellInvocation {
        preceded_by: ShellOperator::Start,
        argv: vec!["set".into(), "+e".into()],
    }];
    assert_eq!(check_k15_bypass_commands(&c).state, KState::Broken);
}

#[test]
fn k15_opaque_parser_and_unknown_actor_abstain() {
    let mut c = census();
    c.commands
        .push(command("opaque", vec!["sh".into(), "-c".into()]));
    c.commands[0].parse_complete = false;
    assert_eq!(check_k15_bypass_commands(&c).state, KState::Unverifiable);
    c.commands[0].parse_complete = true;
    c.commands[0].actor = KActor::Unknown;
    assert_eq!(check_k15_bypass_commands(&c).state, KState::Unverifiable);
}

#[test]
fn k15_quoted_argument_control_and_shell_wrapper_abstention() {
    let mut c = census();
    c.commands
        .push(command("quoted", vec!["echo".into(), "--no-verify".into()]));
    assert_eq!(check_k15_bypass_commands(&c).state, KState::Holds);
    c.commands[0].invocations[0].argv = vec!["sh".into(), "./script.sh".into()];
    assert_eq!(check_k15_bypass_commands(&c).state, KState::Unverifiable);
    c.commands[0].invocations[0].argv = vec!["env".into(), "git".into(), "status".into()];
    assert_eq!(check_k15_bypass_commands(&c).state, KState::Unverifiable);
}

#[test]
fn k16_bound_exact_tree_pass_holds_failed_run_breaks() {
    let mut c = census();
    c.claims.push(claim());
    c.runs.push(run());
    assert_eq!(
        check_k16_tests_pass_claim(&c, "claim-1").state,
        KState::Holds
    );
    c.runs[0].exit_code = Some(1);
    c.runs[0].failed = Some(1);
    assert_eq!(
        check_k16_tests_pass_claim(&c, "claim-1").state,
        KState::Broken
    );
}

#[test]
fn k16_missing_or_other_tree_run_abstains() {
    let mut c = census();
    c.claims.push(claim());
    assert_eq!(
        check_k16_tests_pass_claim(&c, "claim-1").state,
        KState::Unverifiable
    );
    c.runs.push(run());
    c.runs[0].version.tree = "c".repeat(40);
    assert_eq!(
        check_k16_tests_pass_claim(&c, "claim-1").state,
        KState::Unverifiable
    );
}

#[test]
fn k16_duplicate_run_id_and_zero_test_tally_abstain() {
    let mut c = census();
    c.claims.push(claim());
    c.runs.push(run());
    c.runs.push(run());
    assert_eq!(
        check_k16_tests_pass_claim(&c, "claim-1").state,
        KState::Unverifiable
    );
    c.runs.pop();
    c.runs[0].passed = Some(0);
    assert_eq!(
        check_k16_tests_pass_claim(&c, "claim-1").state,
        KState::Unverifiable
    );
}

#[test]
fn k17_agent_spec_edit_breaks_with_person_same_path_control() {
    let f = fixture();
    let mut c = census();
    c.edits.push(edit("agent-1", &f.spec_file, KActor::Agent));
    assert_eq!(check_k17_spec_plan_edits(&c).state, KState::Broken);
    c.edits.push(edit("person-1", &f.spec_file, KActor::Person));
    assert_eq!(check_k17_spec_plan_edits(&c).state, KState::Holds);
    c.edits.clear();
    c.edits.push(edit("note", &f.ordinary_doc, KActor::Agent));
    assert_eq!(check_k17_spec_plan_edits(&c).state, KState::Holds);
}

#[test]
fn k17_other_version_and_unknown_actor_abstain() {
    let f = fixture();
    let mut c = census();
    c.edits.push(edit("unknown", &f.spec_file, KActor::Unknown));
    assert_eq!(check_k17_spec_plan_edits(&c).state, KState::Unverifiable);
    c.edits[0].actor = KActor::Agent;
    c.edits[0].version.tree = "c".repeat(40);
    assert_eq!(check_k17_spec_plan_edits(&c).state, KState::Unverifiable);
}

#[test]
fn k18_post_compaction_deterministic_rule_check_holds_or_breaks() {
    let mut c = census();
    c.compactions.push(compaction());
    c.accepted_rules.push(accepted_rule());
    c.rule_checks.push(rule_check(KState::Holds));
    assert_eq!(check_k18_after_compaction(&c).state, KState::Holds);
    c.rule_checks[0].state = KState::Broken;
    let broken = check_k18_after_compaction(&c);
    assert_eq!(broken.state, KState::Broken);
    assert_eq!(broken.findings.len(), 1);
    assert_eq!(broken.findings[0].evidence_id, "check-1");
    assert_eq!(broken.findings[0].path, "record/session.jsonl");
    assert_eq!(broken.findings[0].line, 7);
    assert_eq!(broken.findings[0].marker, "post_compaction_rule_broken");
}

#[test]
fn k18_missing_post_compaction_check_or_person_acceptance_abstains() {
    let mut c = census();
    c.compactions.push(compaction());
    c.accepted_rules.push(accepted_rule());
    assert_eq!(check_k18_after_compaction(&c).state, KState::Unverifiable);
    c.rule_checks.push(rule_check(KState::Holds));
    c.rule_checks[0].deterministic = false;
    assert_eq!(check_k18_after_compaction(&c).state, KState::Unverifiable);
    c.rule_checks[0].deterministic = true;
    c.accepted_rules[0].person_action_id.clear();
    assert_eq!(check_k18_after_compaction(&c).state, KState::Unverifiable);
}

#[test]
fn k18_pre_compaction_or_other_tree_check_never_proves_following() {
    let mut c = census();
    c.compactions.push(compaction());
    c.accepted_rules.push(accepted_rule());
    c.rule_checks.push(rule_check(KState::Holds));
    c.rule_checks[0].seq = 4;
    assert_eq!(check_k18_after_compaction(&c).state, KState::Unverifiable);
    c.rule_checks[0].seq = 6;
    c.rule_checks[0].version.tree = "c".repeat(40);
    assert_eq!(check_k18_after_compaction(&c).state, KState::Unverifiable);
}

#[test]
fn k18_other_session_check_cannot_supply_post_compaction_receipt() {
    let mut c = census();
    c.compactions.push(compaction());
    c.accepted_rules.push(accepted_rule());
    c.rule_checks.push(rule_check(KState::Holds));
    c.rule_checks[0].session_id = "s2".into();
    let outcome = check_k18_after_compaction(&c);
    assert_eq!(outcome.state, KState::Unverifiable);
    assert_eq!(
        outcome.reason,
        Some("post-compaction deterministic rule check is missing")
    );
    assert!(outcome.findings.is_empty());
}

#[test]
fn k18_prior_version_compaction_is_checked_on_current_exact_tree() {
    let mut c = census();
    let mut prior = compaction();
    prior.version.commit = "c".repeat(40);
    prior.version.tree = "d".repeat(40);
    c.compactions.push(prior);
    c.accepted_rules.push(accepted_rule());
    c.rule_checks.push(rule_check(KState::Holds));
    assert_eq!(check_k18_after_compaction(&c).state, KState::Holds);
}
