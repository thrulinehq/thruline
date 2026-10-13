//! Pure K13–K18 checks over source-backed, exact-version receipts.
//!
//! The adapter must prove complete project and version censuses. These checks
//! do not infer actors from role labels, inspect mutable files, or emit raw
//! credential values. They are not wired into the Decisions verdict engine yet.

use crate::redact::Masked;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KState {
    Holds,
    Broken,
    Unverifiable,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KOutcome {
    pub state: KState,
    pub reason: Option<&'static str>,
    pub checked_places: u64,
    pub findings: Vec<KFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KFinding {
    pub evidence_id: String,
    pub path: String,
    pub line: u64,
    pub marker: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExactVersion {
    pub commit: String,
    pub tree: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRef {
    pub raw_file: String,
    pub raw_line: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KActor {
    Person,
    Agent,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KEdit {
    pub id: String,
    pub session_id: String,
    pub version: ExactVersion,
    pub actor: KActor,
    /// A validated person-action receipt, never a role label alone.
    pub person_action_id: Option<String>,
    pub path: String,
    pub line: u64,
    pub source: SourceRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellOperator {
    Start,
    Sequence,
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellInvocation {
    pub preceded_by: ShellOperator,
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KCommand {
    pub id: String,
    pub session_id: String,
    pub version: ExactVersion,
    pub actor: KActor,
    pub source: SourceRef,
    /// A parser receipt must preserve shell operators and argv boundaries.
    pub parser_id: String,
    pub parse_complete: bool,
    pub invocations: Vec<ShellInvocation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KClaimKind {
    TestsPass,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KClaim {
    pub id: String,
    pub session_id: String,
    pub version: ExactVersion,
    pub seq: u64,
    pub kind: KClaimKind,
    pub bound_run_id: Option<String>,
    pub source: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KRun {
    pub id: String,
    pub version: ExactVersion,
    pub seq: u64,
    pub complete: bool,
    pub exit_code: Option<i32>,
    pub passed: Option<u64>,
    pub failed: Option<u64>,
    pub errors: Option<u64>,
    pub source: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialShape {
    pub id: String,
    pub path: String,
    pub line: u64,
    pub source: SourceRef,
    pub masker_fired: bool,
    pub known_credential_prefix: bool,
    /// A named detector family, never the matched value.
    pub family: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialScan {
    pub version: ExactVersion,
    pub complete: bool,
    pub detector_id: String,
    pub shapes: Vec<CredentialShape>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleKind {
    RuleFileEdit,
    ForbiddenCommand,
    ForbiddenText,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KCompaction {
    pub id: String,
    pub session_id: String,
    /// Project-global event order, comparable to rule-check `seq` even when
    /// this compaction occurred on an earlier version.
    pub seq: u64,
    pub version: ExactVersion,
    pub source: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KAcceptedRule {
    pub id: String,
    pub kind: RuleKind,
    /// Must resolve to a validated person-action receipt.
    pub person_action_id: String,
    pub source: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KRuleCheck {
    pub id: String,
    pub rule_id: String,
    pub kind: RuleKind,
    pub session_id: String,
    pub seq: u64,
    pub version: ExactVersion,
    pub deterministic: bool,
    pub state: KState,
    pub source: SourceRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KCensus {
    pub version: ExactVersion,
    pub complete: bool,
    pub edits: Vec<KEdit>,
    pub commands: Vec<KCommand>,
    pub claims: Vec<KClaim>,
    pub runs: Vec<KRun>,
    pub credential_scan: Option<CredentialScan>,
    pub compactions: Vec<KCompaction>,
    pub accepted_rules: Vec<KAcceptedRule>,
    pub rule_checks: Vec<KRuleCheck>,
}

fn unavailable(reason: &'static str) -> KOutcome {
    KOutcome {
        state: KState::Unverifiable,
        reason: Some(reason),
        checked_places: 0,
        findings: Vec::new(),
    }
}

fn outcome(
    state: KState,
    reason: Option<&'static str>,
    checked_places: u64,
    findings: Vec<KFinding>,
) -> KOutcome {
    KOutcome {
        state,
        reason,
        checked_places,
        findings,
    }
}

fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_version(version: &ExactVersion) -> bool {
    valid_oid(&version.commit) && valid_oid(&version.tree)
}

fn valid_source(source: &SourceRef) -> bool {
    !source.raw_file.trim().is_empty() && source.raw_line > 0
}

fn valid_census(census: &KCensus) -> Result<(), &'static str> {
    if !census.complete || !valid_version(&census.version) {
        return Err("exact-version project census is incomplete");
    }
    Ok(())
}

fn valid_edits(census: &KCensus) -> Result<(), &'static str> {
    let mut ids = std::collections::BTreeSet::new();
    for edit in &census.edits {
        if edit.version != census.version
            || edit.id.is_empty()
            || !ids.insert(&edit.id)
            || edit.session_id.is_empty()
            || edit.path.is_empty()
            || edit.line == 0
            || !valid_source(&edit.source)
        {
            return Err("edit provenance is incomplete or not on this exact version");
        }
    }
    Ok(())
}

fn is_rule_file(path: &str) -> bool {
    let basename = path.rsplit('/').next().unwrap_or("");
    path == ".thruline"
        || path.starts_with(".thruline/")
        || matches!(basename, "CLAUDE.md" | "AGENTS.md")
        || matches!(
            path,
            ".claude/settings.json"
                | ".claude/settings.local.json"
                | ".pre-commit-config.yaml"
                | "lefthook.yml"
        )
        || path.starts_with(".husky/")
        || path.starts_with(".githooks/")
        || path.starts_with(".git/hooks/")
}

fn is_spec_plan(path: &str) -> bool {
    let basename = path.rsplit('/').next().unwrap_or("");
    path.starts_with("spec/")
        || basename == "PLAN.md"
        || (path.starts_with("docs/")
            && basename.starts_with("SPEC")
            && (basename.ends_with(".md") || basename.ends_with(".mdx")))
}

fn check_agent_edits(
    census: &KCensus,
    relevant: fn(&str) -> bool,
    marker: &'static str,
) -> KOutcome {
    if let Err(reason) = valid_census(census).and_then(|_| valid_edits(census)) {
        return unavailable(reason);
    }
    let scoped = census
        .edits
        .iter()
        .filter(|edit| relevant(&edit.path))
        .collect::<Vec<_>>();
    if scoped.iter().any(|edit| {
        edit.actor == KActor::Unknown
            || (edit.actor == KActor::Person
                && edit.person_action_id.as_deref().is_none_or(str::is_empty))
    }) {
        return unavailable("relevant edit actor or person-action receipt is unverified");
    }
    let mut findings = Vec::new();
    for edit in scoped.iter().filter(|edit| edit.actor == KActor::Agent) {
        let person_touched_same_path = scoped.iter().any(|other| {
            other.actor == KActor::Person
                && other.path == edit.path
                && other
                    .person_action_id
                    .as_deref()
                    .is_some_and(|id| !id.is_empty())
        });
        if !person_touched_same_path {
            findings.push(KFinding {
                evidence_id: masked_text(&edit.id),
                path: masked_text(&edit.path),
                line: edit.line,
                marker,
            });
        }
    }
    let state = if findings.is_empty() {
        KState::Holds
    } else {
        KState::Broken
    };
    outcome(state, None, scoped.len() as u64, findings)
}

/// K13 is the B5 tamper guard over exact-version edits. The caller must have
/// validated each person-action ID against the source event before projecting
/// this census; a role string does not make an edit a person edit.
pub fn check_k13_rule_files(census: &KCensus) -> KOutcome {
    check_agent_edits(census, is_rule_file, "agent_rule_file_edit")
}

/// K14 exposes only source locations and detector families. The existing
/// `secrets_absent` check never confirms absence of a secret from a clean
/// scanner result, so this candidate cannot return Holds. A conjunctive
/// masker and known-prefix signal is still only a credential *shape*.
pub fn check_k14_credentials(census: &KCensus) -> KOutcome {
    if let Err(reason) = valid_census(census) {
        return unavailable(reason);
    }
    let Some(scan) = &census.credential_scan else {
        return unavailable("tracked-tree credential scan receipt is missing");
    };
    if scan.version != census.version
        || !scan.complete
        || scan.detector_id != "capture-masker-and-known-prefix-v1"
    {
        return unavailable(
            "credential scan is not complete on this exact tree with a known detector",
        );
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut findings = Vec::new();
    for shape in &scan.shapes {
        if shape.id.is_empty()
            || !ids.insert(&shape.id)
            || shape.path.is_empty()
            || shape.line == 0
            || shape.family.is_empty()
            || !valid_source(&shape.source)
        {
            return unavailable("credential-shape provenance is incomplete");
        }
        if shape.masker_fired && shape.known_credential_prefix {
            findings.push(KFinding {
                evidence_id: masked_text(&shape.id),
                path: masked_text(&shape.path),
                line: shape.line,
                marker: "credential_shape_candidate",
            });
        }
    }
    outcome(
        KState::Unverifiable,
        Some(if findings.is_empty() {
            "shape scan cannot establish that no secret exists"
        } else {
            "credential-shaped values need person review; matched values are withheld"
        }),
        scan.shapes.len() as u64,
        findings,
    )
}

fn is_opaque_invocation(argv: &[String]) -> bool {
    let Some(first) = argv.first() else {
        return true;
    };
    let executable = first.rsplit('/').next().unwrap_or(first);
    matches!(
        executable,
        "eval"
            | "xargs"
            | "env"
            | "sudo"
            | "command"
            | "nohup"
            | "time"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
    ) || (matches!(executable, "python" | "python3" | "node" | "ruby" | "perl")
        && argv
            .iter()
            .skip(1)
            .any(|arg| matches!(arg.as_str(), "-c" | "-e")))
}

fn bypass(invocations: &[ShellInvocation]) -> bool {
    invocations.iter().any(|part| {
        let argv = &part.argv;
        let executable = argv
            .first()
            .and_then(|first| first.rsplit('/').next())
            .unwrap_or("");
        (executable == "git" && argv.iter().skip(1).any(|arg| arg == "--no-verify"))
            || (executable == "set" && argv.iter().skip(1).any(|arg| arg == "+e"))
            || (part.preceded_by == ShellOperator::Or && argv.len() == 1 && argv[0] == "true")
    })
}

/// K15 reads parsed command receipts, not a substring in quoted prose.
pub fn check_k15_bypass_commands(census: &KCensus) -> KOutcome {
    if let Err(reason) = valid_census(census) {
        return unavailable(reason);
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut agent_commands = Vec::new();
    for command in &census.commands {
        if command.version != census.version
            || command.id.is_empty()
            || !ids.insert(&command.id)
            || command.session_id.is_empty()
            || !valid_source(&command.source)
        {
            return unavailable("command provenance is incomplete or not on this exact version");
        }
        if command.actor == KActor::Unknown {
            return unavailable("command actor is unverified");
        }
        if command.actor == KActor::Agent {
            if !command.parse_complete
                || command.parser_id != "shell-parser-v1"
                || command.invocations.is_empty()
                || command
                    .invocations
                    .first()
                    .is_none_or(|part| part.preceded_by != ShellOperator::Start)
                || command
                    .invocations
                    .iter()
                    .any(|part| part.argv.is_empty() || is_opaque_invocation(&part.argv))
            {
                return unavailable("agent command parser receipt is incomplete or opaque");
            }
            agent_commands.push(command);
        }
    }
    if agent_commands.is_empty() {
        return outcome(
            KState::NotApplicable,
            Some("no agent command in exact-version census"),
            0,
            Vec::new(),
        );
    }
    let findings = agent_commands
        .iter()
        .filter(|command| bypass(&command.invocations))
        .map(|command| KFinding {
            evidence_id: masked_text(&command.id),
            path: masked_text(&command.source.raw_file),
            line: command.source.raw_line,
            marker: "verification_bypass_command",
        })
        .collect::<Vec<_>>();
    let state = if findings.is_empty() {
        KState::Holds
    } else {
        KState::Broken
    };
    outcome(state, None, agent_commands.len() as u64, findings)
}

/// K16 requires a bound, completed run before this claim on this exact tree.
/// No run is Unverifiable, as E1 requires; a failed bound run is Broken.
pub fn check_k16_tests_pass_claim(census: &KCensus, claim_id: &str) -> KOutcome {
    if let Err(reason) = valid_census(census) {
        return unavailable(reason);
    }
    let mut claim_ids = std::collections::BTreeSet::new();
    if census
        .claims
        .iter()
        .any(|claim| claim.id.is_empty() || !claim_ids.insert(&claim.id))
    {
        return unavailable("claim census has missing or duplicate IDs");
    }
    let mut run_ids = std::collections::BTreeSet::new();
    if census
        .runs
        .iter()
        .any(|run| run.id.is_empty() || !run_ids.insert(&run.id))
    {
        return unavailable("run census has missing or duplicate IDs");
    }
    let Some(claim) = census.claims.iter().find(|claim| claim.id == claim_id) else {
        return unavailable("tests-pass claim receipt is missing");
    };
    if claim.id.is_empty()
        || claim.session_id.is_empty()
        || claim.seq == 0
        || claim.version != census.version
        || !valid_source(&claim.source)
    {
        return unavailable("claim provenance is incomplete or not on this exact version");
    }
    if claim.kind != KClaimKind::TestsPass {
        return outcome(
            KState::NotApplicable,
            Some("claim is not a tests-pass claim"),
            0,
            Vec::new(),
        );
    }
    let Some(run_id) = claim.bound_run_id.as_deref().filter(|id| !id.is_empty()) else {
        return unavailable("tests-pass claim has no bound run");
    };
    let Some(run) = census.runs.iter().find(|run| run.id == run_id) else {
        return unavailable("bound test run is absent from the complete census");
    };
    if run.version != census.version
        || run.seq == 0
        || run.seq >= claim.seq
        || !valid_source(&run.source)
    {
        return unavailable("bound test run is not a preceding run on this exact tree");
    }
    let (Some(exit_code), Some(passed), Some(failed), Some(errors)) =
        (run.exit_code, run.passed, run.failed, run.errors)
    else {
        return unavailable("bound test run result is incomplete");
    };
    if !run.complete {
        return unavailable("bound test run did not complete");
    }
    if exit_code != 0 || failed > 0 || errors > 0 {
        return outcome(
            KState::Broken,
            Some("bound exact-tree test run failed"),
            1,
            vec![KFinding {
                evidence_id: masked_text(&run.id),
                path: masked_text(&run.source.raw_file),
                line: run.source.raw_line,
                marker: "failed_bound_test_run",
            }],
        );
    }
    if passed == 0 {
        return unavailable("bound run records no passing tests");
    }
    outcome(KState::Holds, None, 1, Vec::new())
}

/// K17 implements the observable B5 proxy: an agent edit to a spec or plan
/// without a verified person edit to that same path on the same version. It
/// makes no claim about why the agent made the edit.
pub fn check_k17_spec_plan_edits(census: &KCensus) -> KOutcome {
    check_agent_edits(census, is_spec_plan, "agent_spec_plan_edit")
}

/// K18 requires each accepted K13/F1/G2 rule to have a deterministic check
/// after every recorded compaction in its session, on the exact named tree.
pub fn check_k18_after_compaction(census: &KCensus) -> KOutcome {
    if let Err(reason) = valid_census(census) {
        return unavailable(reason);
    }
    if census.compactions.is_empty() || census.accepted_rules.is_empty() {
        return outcome(
            KState::NotApplicable,
            Some("no compaction or accepted rule in scope"),
            0,
            Vec::new(),
        );
    }
    let mut ids = std::collections::BTreeSet::new();
    for compaction in &census.compactions {
        if compaction.id.is_empty()
            || !ids.insert(&compaction.id)
            || compaction.session_id.is_empty()
            || compaction.seq == 0
            || !valid_version(&compaction.version)
            || !valid_source(&compaction.source)
        {
            return unavailable("compaction provenance is incomplete or on another version");
        }
    }
    ids.clear();
    for rule in &census.accepted_rules {
        if rule.id.is_empty()
            || !ids.insert(&rule.id)
            || rule.person_action_id.is_empty()
            || !valid_source(&rule.source)
        {
            return unavailable("accepted rule lacks a verified person-action source");
        }
    }
    ids.clear();
    for check in &census.rule_checks {
        if check.id.is_empty()
            || !ids.insert(&check.id)
            || check.rule_id.is_empty()
            || check.session_id.is_empty()
            || check.seq == 0
            || check.version != census.version
            || !valid_source(&check.source)
        {
            return unavailable("rule check provenance is incomplete or on another version");
        }
    }
    let mut findings = Vec::new();
    let mut missing = false;
    let mut places = 0u64;
    for compaction in &census.compactions {
        for rule in &census.accepted_rules {
            let matching = census
                .rule_checks
                .iter()
                .filter(|check| {
                    check.rule_id == rule.id
                        && check.kind == rule.kind
                        && check.session_id == compaction.session_id
                        && check.seq > compaction.seq
                })
                .collect::<Vec<_>>();
            if matching.is_empty() {
                missing = true;
                continue;
            }
            places += 1;
            if matching.iter().any(|check| {
                !check.deterministic || !matches!(check.state, KState::Holds | KState::Broken)
            }) {
                missing = true;
                continue;
            }
            for check in matching
                .into_iter()
                .filter(|check| check.state == KState::Broken)
            {
                findings.push(KFinding {
                    evidence_id: masked_text(&check.id),
                    path: masked_text(&check.source.raw_file),
                    line: check.source.raw_line,
                    marker: "post_compaction_rule_broken",
                });
            }
        }
    }
    if !findings.is_empty() {
        outcome(
            KState::Broken,
            Some("accepted rule check is broken after compaction"),
            places,
            findings,
        )
    } else if missing {
        outcome(
            KState::Unverifiable,
            Some("post-compaction deterministic rule check is missing"),
            places,
            Vec::new(),
        )
    } else {
        outcome(KState::Holds, None, places, Vec::new())
    }
}

fn masked_text(text: &str) -> String {
    Masked::mask(text).into_text()
}
