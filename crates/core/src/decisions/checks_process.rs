//! F1–F4: deterministic process checks over a complete recorded census.
//!
//! These functions never execute a command or infer intent from prose. A caller
//! must prove the census covers the named version and relevant sessions. A
//! missing command, edit, answer, run or interval receipt is not a hold.

use serde::{Deserialize, Serialize};

use crate::change_link::{pattern_matches, PatternVerdict};
use crate::redact::Masked;

use super::checks::{CheckOutcome, Finding, SurfaceResult, VerdictState};
use super::params::Surface;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessActor {
    Person,
    Agent,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessCommand {
    pub id: String,
    pub session_id: String,
    pub actor: ProcessActor,
    /// A globally comparable order within the supplied census, not a local
    /// per-session counter.
    pub seq: u64,
    pub version_id: String,
    pub tree_hash: String,
    pub command: String,
    pub complete: bool,
    pub started_ms: Option<i64>,
    pub ended_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessEdit {
    pub change_id: String,
    pub session_id: String,
    pub seq: u64,
    pub path: String,
    pub actor: ProcessActor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessRun {
    pub change_id: String,
    pub session_id: String,
    pub seq: u64,
    pub version_id: String,
    pub tree_hash: String,
    pub exit_code: Option<i64>,
    pub passed: Option<u64>,
    pub failed: Option<u64>,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessAnswer {
    pub id: String,
    pub session_id: String,
    pub seq: u64,
    pub event_kind: String,
    pub actor: ProcessActor,
    pub granted: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessAction {
    pub id: String,
    pub session_id: String,
    pub seq: u64,
    pub action_class: String,
}

/// The adapter's completeness promise covers all sessions and events needed
/// for the accepted rule, including actions that would fail it. If it
/// cannot make that promise, set `complete=false`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessCensus {
    pub version_id: String,
    pub tree_hash: String,
    pub complete: bool,
    pub commands: Vec<ProcessCommand>,
    pub edits: Vec<ProcessEdit>,
    pub runs: Vec<ProcessRun>,
    pub answers: Vec<ProcessAnswer>,
    pub actions: Vec<ProcessAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandForbiddenParams {
    /// Literal shell token sequences, not regular expressions or shell code.
    pub patterns: Vec<String>,
    /// None checks every agent session in the complete census.
    pub session_scope: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestsBeforeCodeParams {
    pub change_id: String,
    pub test_paths: Vec<String>,
    pub source_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsentRecordedParams {
    pub action_class: String,
    pub consent_event_kind: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessWindow {
    /// UTC Unix milliseconds; the adapter must use a common clock basis.
    pub start_ms: i64,
    pub end_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OneHeavyJobParams {
    /// Literal shell token sequences.
    pub command_patterns: Vec<String>,
    pub window: ProcessWindow,
}

/// F1: agent invocations matching a person-accepted literal token pattern.
/// Quoted words in an `echo` argument do not become a second invocation.
pub fn check_command_forbidden(
    version_id: &str,
    tree_hash: &str,
    params: &CommandForbiddenParams,
    census: &ProcessCensus,
) -> CheckOutcome {
    if let Err(reason) = valid_census(version_id, tree_hash, census) {
        return unavailable(reason, tree_hash);
    }
    let patterns = match parsed_patterns(&params.patterns) {
        Ok(patterns) => patterns,
        Err(reason) => return unavailable(reason, tree_hash),
    };
    if params.session_scope.as_deref().is_some_and(str::is_empty) {
        return unavailable("accepted session scope is empty", tree_hash);
    }
    let commands = census
        .commands
        .iter()
        .filter(|row| {
            row.actor != ProcessActor::Person
                && row.version_id == version_id
                && row.tree_hash == tree_hash
                && params
                    .session_scope
                    .as_deref()
                    .is_none_or(|scope| row.session_id == scope)
        })
        .collect::<Vec<_>>();
    if commands.is_empty() {
        return outcome(
            VerdictState::NotApplicable,
            tree_hash,
            Some("no agent command in accepted scope"),
            Vec::new(),
            0,
            Surface::Other,
        );
    }
    let mut findings = Vec::new();
    for row in &commands {
        if row.actor == ProcessActor::Unknown {
            return unavailable("command actor is unknown", tree_hash);
        }
        if !valid_command_receipt(row) {
            return unavailable("agent command receipt is incomplete", tree_hash);
        }
        let Some(tokens) = tokenize_shell(&row.command) else {
            return unavailable("agent command syntax cannot be read", tree_hash);
        };
        if opaque_shell_invocation(&tokens) {
            return unavailable("nested command invocation cannot be inspected", tree_hash);
        }
        if patterns
            .iter()
            .any(|pattern| token_pattern_matches(&tokens, pattern))
        {
            findings.push(command_finding(row));
        }
    }
    let state = if findings.is_empty() {
        VerdictState::Holds
    } else {
        VerdictState::Broken
    };
    outcome(
        state,
        tree_hash,
        None,
        findings,
        commands.len() as u64,
        Surface::Other,
    )
}

/// F2: one Change's first test edit, red run, source edit and exact-tree green
/// run must occur in that order. All sequence numbers are globally comparable.
pub fn check_tests_before_code(
    version_id: &str,
    tree_hash: &str,
    params: &TestsBeforeCodeParams,
    census: &ProcessCensus,
) -> CheckOutcome {
    if let Err(reason) = valid_census(version_id, tree_hash, census) {
        return unavailable(reason, tree_hash);
    }
    if params.change_id.trim().is_empty()
        || !valid_paths(&params.test_paths)
        || !valid_paths(&params.source_paths)
    {
        return unavailable("accepted Change or path scope is invalid", tree_hash);
    }
    let edits = census
        .edits
        .iter()
        .filter(|row| row.change_id == params.change_id)
        .collect::<Vec<_>>();
    let runs = census
        .runs
        .iter()
        .filter(|row| row.change_id == params.change_id)
        .collect::<Vec<_>>();
    if edits
        .iter()
        .any(|row| row.seq == 0 || row.path.is_empty() || row.session_id.is_empty())
        || runs
            .iter()
            .any(|row| row.seq == 0 || row.session_id.is_empty())
    {
        return unavailable("Change edit or run order is missing", tree_hash);
    }
    let mut all_seq = edits
        .iter()
        .map(|row| row.seq)
        .chain(runs.iter().map(|row| row.seq))
        .collect::<Vec<_>>();
    all_seq.sort_unstable();
    if all_seq.windows(2).any(|pair| pair[0] == pair[1]) {
        return unavailable("Change edit and run order is ambiguous", tree_hash);
    }
    let mut first_test = None::<&ProcessEdit>;
    let mut first_source = None::<&ProcessEdit>;
    for row in &edits {
        if matches_any(&params.test_paths, &row.path).is_none()
            || matches_any(&params.source_paths, &row.path).is_none()
        {
            return unavailable("accepted path pattern cannot be read", tree_hash);
        }
        let test = matches_any(&params.test_paths, &row.path) == Some(true);
        let source = matches_any(&params.source_paths, &row.path) == Some(true);
        if test && source {
            return unavailable("test and source scopes overlap", tree_hash);
        }
        if test && first_test.is_none_or(|old| row.seq < old.seq) {
            first_test = Some(row);
        }
        if source && first_source.is_none_or(|old| row.seq < old.seq) {
            first_source = Some(row);
        }
    }
    let Some(source) = first_source else {
        return outcome(
            VerdictState::NotApplicable,
            tree_hash,
            Some("no source edit in this Change"),
            Vec::new(),
            edits.len() as u64,
            Surface::Tests,
        );
    };
    let Some(test) = first_test else {
        return outcome(
            VerdictState::Broken,
            tree_hash,
            Some("source edit preceded any test edit"),
            vec![edit_finding(source)],
            edits.len() as u64,
            Surface::Tests,
        );
    };
    if test.seq >= source.seq {
        return outcome(
            VerdictState::Broken,
            tree_hash,
            Some("source edit preceded test edit"),
            vec![edit_finding(source)],
            edits.len() as u64,
            Surface::Tests,
        );
    }
    let Some(first_run) = runs.iter().min_by_key(|row| row.seq) else {
        return unavailable("no recorded test run for this Change", tree_hash);
    };
    if !first_run.complete || first_run.exit_code.is_none() || first_run.failed.is_none() {
        return unavailable("first test run has incomplete result", tree_hash);
    }
    if first_run.seq <= test.seq
        || first_run.seq >= source.seq
        || first_run.exit_code == Some(0)
        || first_run.failed == Some(0)
    {
        return outcome(
            VerdictState::Broken,
            tree_hash,
            Some("a failing red run did not precede the source edit"),
            vec![edit_finding(source)],
            (edits.len() + runs.len()) as u64,
            Surface::Tests,
        );
    }
    let target_green = runs
        .iter()
        .filter(|row| {
            row.seq > source.seq && row.version_id == version_id && row.tree_hash == tree_hash
        })
        .min_by_key(|row| row.seq);
    let Some(green) = target_green else {
        return unavailable(
            "no green run recorded on this exact tree after the source edit",
            tree_hash,
        );
    };
    if !green.complete
        || green.exit_code.is_none()
        || green.passed.is_none()
        || green.failed.is_none()
    {
        return unavailable("green run has incomplete result", tree_hash);
    }
    if green.exit_code != Some(0) || green.passed == Some(0) || green.failed != Some(0) {
        return outcome(
            VerdictState::Broken,
            tree_hash,
            Some("first exact-tree run after source edit is not green"),
            vec![edit_finding(source)],
            (edits.len() + runs.len()) as u64,
            Surface::Tests,
        );
    }
    outcome(
        VerdictState::Holds,
        tree_hash,
        None,
        Vec::new(),
        (edits.len() + runs.len()) as u64,
        Surface::Tests,
    )
}

/// F3: an action needs a preceding, granted person answer in the same session.
pub fn check_consent_recorded(
    version_id: &str,
    tree_hash: &str,
    params: &ConsentRecordedParams,
    census: &ProcessCensus,
) -> CheckOutcome {
    if let Err(reason) = valid_census(version_id, tree_hash, census) {
        return unavailable(reason, tree_hash);
    }
    if params.action_class.trim().is_empty() || params.consent_event_kind.trim().is_empty() {
        return unavailable("accepted action or consent kind is empty", tree_hash);
    }
    let actions = census
        .actions
        .iter()
        .filter(|row| row.action_class == params.action_class)
        .collect::<Vec<_>>();
    if actions.is_empty() {
        return outcome(
            VerdictState::NotApplicable,
            tree_hash,
            Some("no action of the accepted class"),
            Vec::new(),
            0,
            Surface::Other,
        );
    }
    let mut findings = Vec::new();
    for action in &actions {
        if action.id.is_empty() || action.session_id.is_empty() || action.seq == 0 {
            return unavailable("action receipt is incomplete", tree_hash);
        }
        let answer = census
            .answers
            .iter()
            .filter(|row| {
                row.session_id == action.session_id
                    && row.event_kind == params.consent_event_kind
                    && row.actor == ProcessActor::Person
                    && row.seq < action.seq
            })
            .max_by_key(|row| row.seq);
        match answer {
            Some(row) if row.id.is_empty() || row.seq == 0 || row.granted.is_none() => {
                return unavailable("person answer receipt is incomplete", tree_hash)
            }
            Some(row) if row.granted == Some(true) => {}
            _ => findings.push(action_finding(action)),
        }
    }
    let state = if findings.is_empty() {
        VerdictState::Holds
    } else {
        VerdictState::Broken
    };
    outcome(
        state,
        tree_hash,
        None,
        findings,
        actions.len() as u64,
        Surface::Other,
    )
}

/// F4: two matching jobs in different sessions must not occupy overlapping
/// UTC millisecond intervals. Adjacent half-open intervals do not overlap.
pub fn check_one_heavy_job(
    version_id: &str,
    tree_hash: &str,
    params: &OneHeavyJobParams,
    census: &ProcessCensus,
) -> CheckOutcome {
    if let Err(reason) = valid_census(version_id, tree_hash, census) {
        return unavailable(reason, tree_hash);
    }
    if params.window.start_ms >= params.window.end_ms {
        return unavailable("accepted heavy-job window is invalid", tree_hash);
    }
    let patterns = match parsed_patterns(&params.command_patterns) {
        Ok(patterns) => patterns,
        Err(reason) => return unavailable(reason, tree_hash),
    };
    let mut jobs = Vec::new();
    for row in census
        .commands
        .iter()
        .filter(|row| row.actor != ProcessActor::Person)
    {
        if row.actor == ProcessActor::Unknown {
            return unavailable("heavy-job command actor is unknown", tree_hash);
        }
        if !valid_command_receipt(row) {
            return unavailable("heavy-job command receipt is incomplete", tree_hash);
        }
        let Some(tokens) = tokenize_shell(&row.command) else {
            return unavailable("heavy-job command syntax cannot be read", tree_hash);
        };
        if opaque_shell_invocation(&tokens) {
            return unavailable("nested heavy-job invocation cannot be inspected", tree_hash);
        }
        if patterns
            .iter()
            .any(|pattern| token_pattern_matches(&tokens, pattern))
        {
            jobs.push(row);
        }
    }
    if jobs.is_empty() {
        return outcome(
            VerdictState::NotApplicable,
            tree_hash,
            Some("no matching heavy job in accepted scope"),
            Vec::new(),
            0,
            Surface::Other,
        );
    }
    let mut in_window = Vec::new();
    for row in jobs {
        let (Some(start), Some(end)) = (row.started_ms, row.ended_ms) else {
            return unavailable("heavy-job interval is incomplete", tree_hash);
        };
        if start >= end {
            return unavailable("heavy-job interval is invalid", tree_hash);
        }
        if start < params.window.end_ms && end > params.window.start_ms {
            in_window.push((row, start, end));
        }
    }
    let mut findings = Vec::new();
    for (index, left) in in_window.iter().enumerate() {
        for right in in_window.iter().skip(index + 1) {
            if left.0.session_id != right.0.session_id && left.1 < right.2 && right.1 < left.2 {
                findings.push(command_finding(right.0));
            }
        }
    }
    let state = if findings.is_empty() {
        VerdictState::Holds
    } else {
        VerdictState::Broken
    };
    outcome(
        state,
        tree_hash,
        None,
        findings,
        in_window.len() as u64,
        Surface::Other,
    )
}

fn valid_census(
    version_id: &str,
    tree_hash: &str,
    census: &ProcessCensus,
) -> Result<(), &'static str> {
    if version_id.is_empty() || tree_hash.is_empty() {
        return Err("exact version identity is missing");
    }
    if !census.complete || census.version_id != version_id || census.tree_hash != tree_hash {
        return Err("process census is not complete for this exact version");
    }
    Ok(())
}

fn valid_command_receipt(row: &ProcessCommand) -> bool {
    row.complete
        && !row.id.is_empty()
        && !row.session_id.is_empty()
        && row.seq > 0
        && !row.command.trim().is_empty()
}

fn valid_paths(paths: &[String]) -> bool {
    !paths.is_empty()
        && paths.iter().all(|path| {
            !path.is_empty()
                && pattern_matches(path, "__decisions_scope_probe__") != PatternVerdict::Unreadable
        })
}

fn matches_any(patterns: &[String], path: &str) -> Option<bool> {
    let mut hit = false;
    for pattern in patterns {
        match pattern_matches(pattern, path) {
            PatternVerdict::Matched => hit = true,
            PatternVerdict::DidNotMatch => {}
            PatternVerdict::Unreadable => return None,
        }
    }
    Some(hit)
}

#[derive(Clone)]
struct Token {
    value: String,
    segment_start: bool,
}

fn parsed_patterns(patterns: &[String]) -> Result<Vec<Vec<Token>>, &'static str> {
    if patterns.is_empty() {
        return Err("accepted command patterns are empty");
    }
    patterns
        .iter()
        .map(|pattern| {
            let tokens =
                tokenize_shell(pattern).ok_or("accepted command pattern syntax cannot be read")?;
            if tokens.is_empty()
                || tokens
                    .iter()
                    .any(|token| token.value == "*" || token.value == "?")
            {
                return Err("accepted command pattern is empty or unsupported");
            }
            Ok(tokens)
        })
        .collect()
}

/// A deliberately small shell lexer. It handles quotes, escapes and segment
/// separators; unsupported redirection/substitution yields no checkable fact.
fn tokenize_shell(input: &str) -> Option<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    let mut chars = input.chars().peekable();
    let mut quote = None::<char>;
    let mut segment_start = true;
    let mut word_start = true;
    while let Some(ch) = chars.next() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            } else if ch == '\\' && q == '"' {
                word.push(chars.next()?);
            } else {
                word.push(ch);
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                quote = Some(ch);
            }
            '\\' => word.push(chars.next()?),
            '$' | '`' | '<' | '>' => return None,
            ' ' | '\t' | '\r' => {
                if !word.is_empty() {
                    tokens.push(Token {
                        value: std::mem::take(&mut word),
                        segment_start: word_start,
                    });
                    segment_start = false;
                }
                word_start = segment_start;
            }
            ';' | '|' | '&' | '\n' => {
                if !word.is_empty() {
                    tokens.push(Token {
                        value: std::mem::take(&mut word),
                        segment_start: word_start,
                    });
                }
                let operator = if ch == '|' && chars.peek() == Some(&'|') {
                    chars.next();
                    "||"
                } else if ch == '&' && chars.peek() == Some(&'&') {
                    chars.next();
                    "&&"
                } else if ch == ';' || ch == '\n' {
                    ";"
                } else {
                    return None;
                };
                tokens.push(Token {
                    value: operator.into(),
                    segment_start: false,
                });
                segment_start = true;
                word_start = true;
            }
            _ => {
                if word.is_empty() {
                    word_start = segment_start;
                }
                word.push(ch);
            }
        }
    }
    if quote.is_some() {
        return None;
    }
    if !word.is_empty() {
        tokens.push(Token {
            value: word,
            segment_start: word_start,
        });
    }
    Some(tokens)
}

fn token_pattern_matches(command: &[Token], pattern: &[Token]) -> bool {
    if pattern.len() > command.len() {
        return false;
    }
    command.windows(pattern.len()).any(|window| {
        let first = &pattern[0].value;
        let boundary =
            first.starts_with('-') || first == "||" || first == "&&" || window[0].segment_start;
        boundary
            && window
                .iter()
                .zip(pattern)
                .all(|(actual, expected)| actual.value == expected.value)
    })
}

fn opaque_shell_invocation(tokens: &[Token]) -> bool {
    for (index, token) in tokens.iter().enumerate() {
        if token.segment_start && matches!(token.value.as_str(), "eval" | "source" | ".") {
            return true;
        }
        if matches!(
            token.value.as_str(),
            "sh" | "bash" | "zsh" | "dash" | "ksh" | "fish"
        ) && tokens
            .get(index + 1)
            .is_some_and(|next| next.value == "-c" || next.value == "--command")
        {
            return true;
        }
    }
    false
}

fn command_finding(row: &ProcessCommand) -> Finding {
    Finding {
        surface: Surface::Other,
        path: String::new(),
        line: row.seq,
        span: Masked::mask(&row.command).into_text(),
    }
}

fn edit_finding(row: &ProcessEdit) -> Finding {
    Finding {
        surface: Surface::Tests,
        path: Masked::mask(&row.path).into_text(),
        line: row.seq,
        span: "source edit occurred before the required red/green sequence".into(),
    }
}

fn action_finding(row: &ProcessAction) -> Finding {
    Finding {
        surface: Surface::Other,
        path: String::new(),
        line: row.seq,
        span: format!(
            "action {} lacked preceding person consent",
            Masked::mask(&row.action_class).text()
        ),
    }
}

fn unavailable(reason: &'static str, tree_hash: &str) -> CheckOutcome {
    CheckOutcome::unavailable(reason, (!tree_hash.is_empty()).then(|| tree_hash.into()))
}

fn outcome(
    state: VerdictState,
    tree_hash: &str,
    reason: Option<&str>,
    findings: Vec<Finding>,
    checked_places: u64,
    surface: Surface,
) -> CheckOutcome {
    CheckOutcome {
        state,
        tree_hash: Some(tree_hash.into()),
        reason: reason.map(str::to_owned),
        checked_places,
        surfaces: vec![SurfaceResult {
            surface,
            checked_places,
            finding_count: findings.len() as u64,
            state,
        }],
        findings,
        not_looked_at: Vec::new(),
        visual: Vec::new(),
    }
}
