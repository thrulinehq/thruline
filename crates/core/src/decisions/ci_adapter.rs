//! B4's bounded, immutable GitHub Actions workflow adapter.
//!
//! This is a structural preservation check. It reads no working-tree files,
//! executes no workflow, and resolves no external action or expression. YAML
//! anchors, duplicate keys, dynamic conditions and unsupported shell syntax
//! are evidence gaps. A stable label cannot certify a replaced execution body.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use yaml_rust2::parser::{Event, Parser};
use yaml_rust2::scanner::TScalarStyle;

use crate::git_isolation::isolate_config;
use crate::redact::Masked;

use super::checks::{CheckOutcome, Finding, SurfaceResult, VerdictState};
use super::checks_preservation::{check_ci_step_present, CiStepCensus, CiStepPresentParams};
use super::params::Surface;

const MAX_BLOB_BYTES: usize = 2 * 1024 * 1024;
const MAX_YAML_EVENTS: usize = 30_000;
const MAX_YAML_DEPTH: usize = 64;

type Checked<T> = Result<T, &'static str>;

#[derive(Clone, Debug)]
struct Node {
    line: u64,
    value: NodeValue,
}

#[derive(Clone, Debug)]
enum NodeValue {
    Scalar { text: String, plain: bool },
    Sequence(Vec<Node>),
    Mapping(BTreeMap<String, Node>),
}

impl Node {
    fn scalar(&self) -> Checked<&str> {
        match &self.value {
            NodeValue::Scalar { text, .. } => Ok(text),
            _ => Err("CI field must be a literal scalar"),
        }
    }

    fn mapping(&self) -> Checked<&BTreeMap<String, Node>> {
        match &self.value {
            NodeValue::Mapping(map) => Ok(map),
            _ => Err("CI mapping is malformed"),
        }
    }

    fn sequence(&self) -> Checked<&[Node]> {
        match &self.value {
            NodeValue::Sequence(items) => Ok(items),
            _ => Err("CI steps must be a sequence"),
        }
    }

    fn bool_literal(&self) -> Checked<bool> {
        match &self.value {
            NodeValue::Scalar { text, plain: true } if text == "true" => Ok(true),
            NodeValue::Scalar { text, plain: true } if text == "false" => Ok(false),
            _ => Err("CI condition or bypass must be a literal boolean"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Bypass {
    location: String,
    kind: &'static str,
    line: u64,
}

struct Observation {
    selected: Option<Selected>,
    present: bool,
    threshold: Option<u32>,
    bypasses: Vec<Bypass>,
}

struct Selected {
    node: Node,
    job: bool,
    job_id: String,
    selector_kind: &'static str,
    workflow_context: Node,
    job_context: Option<Node>,
}

/// The baseline is the verified acceptance's immutable applies_from_version.
/// Both versions must be full commit object ids, never movable refs.
pub fn check_at(
    repo: &Path,
    version: &str,
    tree_hash: &str,
    baseline_version: &str,
    params: &CiStepPresentParams,
) -> CheckOutcome {
    if !is_exact_commit(repo, version) {
        return CheckOutcome::unavailable("CI target must be an exact commit object id", None);
    }
    let Some(target_tree) = tree(repo, version) else {
        return CheckOutcome::unavailable("CI target tree cannot be resolved", None);
    };
    if target_tree != tree_hash {
        return CheckOutcome::unavailable("CI target tree does not match the named version", None);
    }
    let unavailable = |reason: &str| CheckOutcome::unavailable(reason, Some(target_tree.clone()));
    if !is_exact_commit(repo, baseline_version) {
        return unavailable("CI accepted baseline must be an exact commit object id");
    }
    let Some(baseline_tree) = tree(repo, baseline_version) else {
        return unavailable("CI accepted baseline tree cannot be resolved");
    };
    if !safe_path(&params.workflow)
        || !params.workflow.starts_with(".github/workflows/")
        || !params.workflow.ends_with(".yml") && !params.workflow.ends_with(".yaml")
        || params.step.trim().is_empty()
        || params.step.chars().any(char::is_control)
        || params.threshold_pct.is_some_and(|value| value > 100)
    {
        return unavailable("CI accepted selector is outside the supported Actions subset");
    }
    let (baseline_blob, baseline_bytes) =
        match workflow_blob(repo, &baseline_tree, &params.workflow) {
            Ok(Some(blob)) => blob,
            Ok(None) => return unavailable("CI accepted baseline workflow is absent"),
            Err(reason) => return unavailable(reason),
        };
    let baseline = match observe(&baseline_bytes, &params.step) {
        Ok(observation) => observation,
        Err(reason) => return unavailable(reason),
    };
    if !baseline.present
        || params
            .threshold_pct
            .is_some_and(|value| baseline.threshold != Some(value))
    {
        return unavailable("CI accepted baseline does not contain the named step or threshold");
    }
    let (target_blob, target_bytes) = match workflow_blob(repo, &target_tree, &params.workflow) {
        Ok(Some(blob)) => blob,
        Ok(None) => {
            return broken(
                &target_tree,
                vec![finding(
                    &params.workflow,
                    0,
                    "accepted CI workflow is absent",
                )],
            );
        }
        Err(reason) => return unavailable(reason),
    };
    let target = match observe(&target_bytes, &params.step) {
        Ok(observation) => observation,
        Err(reason) => return unavailable(reason),
    };
    if let (Some(before), Some(after)) = (&baseline.selected, &target.selected) {
        if before.job != after.job
            || before.job_id != after.job_id
            || before.selector_kind != after.selector_kind
        {
            return unavailable("CI selector changed its accepted class or job location");
        }
    }

    // Bind bypass deltas to their syntactic location. Comparing only the kind
    // would let an old bypass in another step mask a newly introduced one.
    let prior = baseline
        .bypasses
        .iter()
        .map(|b| (&b.location, b.kind))
        .collect::<BTreeSet<_>>();
    let changed_locations = target
        .bypasses
        .iter()
        .filter(|b| !prior.contains(&(&b.location, b.kind)))
        .collect::<Vec<_>>();
    let current = target
        .bypasses
        .iter()
        .map(|b| (&b.location, b.kind))
        .collect::<BTreeSet<_>>();
    let possibly_moved_kinds = baseline
        .bypasses
        .iter()
        .filter(|b| !current.contains(&(&b.location, b.kind)))
        .map(|b| b.kind)
        .collect::<BTreeSet<_>>();
    let introduced = changed_locations
        .iter()
        .filter(|b| !possibly_moved_kinds.contains(b.kind))
        .map(|b| finding(&params.workflow, b.line, "CI bypass setting was introduced"))
        .collect::<Vec<_>>();
    if !introduced.is_empty() {
        return broken(&target_tree, introduced);
    }
    if !changed_locations.is_empty() {
        return unavailable(
            "CI bypass locations changed and a move cannot be distinguished from introduction",
        );
    }
    let census = CiStepCensus {
        baseline_version: baseline_version.into(),
        baseline_tree_hash: baseline_tree,
        target_version: version.into(),
        target_tree_hash: target_tree.clone(),
        workflow: params.workflow.clone(),
        step: params.step.clone(),
        baseline_blob,
        target_blob,
        complete: true,
        baseline_step_present: baseline.present,
        target_step_present: target.present,
        baseline_threshold_pct: baseline.threshold,
        target_threshold_pct: target.threshold,
        baseline_bypasses: baseline.bypasses.iter().map(|b| b.kind.into()).collect(),
        target_bypasses: target.bypasses.iter().map(|b| b.kind.into()).collect(),
    };
    let outcome = check_ci_step_present(repo, version, params, &census);
    if outcome.state != VerdictState::Holds {
        return outcome;
    }
    let (Some(before), Some(after)) = (&baseline.selected, &target.selected) else {
        return unavailable("CI selected execution receipt is missing");
    };
    let normalize_threshold = params.threshold_pct.is_some();
    match (
        execution_signature(before, normalize_threshold),
        execution_signature(after, normalize_threshold),
    ) {
        (Ok(before), Ok(after)) if before == after => outcome,
        _ => unavailable(
            "CI selected execution body changed outside the accepted threshold or bypass check",
        ),
    }
}

fn yaml(source: &str) -> Checked<Node> {
    let mut parser = Parser::new_from_str(source);
    let mut events = Vec::new();
    let mut documents = 0;
    let mut depth = 0usize;
    loop {
        let (event, mark) = parser
            .next_token()
            .map_err(|_| "CI workflow YAML is malformed")?;
        match &event {
            Event::DocumentStart => {
                documents += 1;
                if documents > 1 {
                    return Err("CI workflow has multiple YAML documents");
                }
            }
            Event::Alias(_) => return Err("CI workflow YAML aliases are unsupported"),
            Event::Scalar(_, _, anchor, tag)
            | Event::SequenceStart(anchor, tag)
            | Event::MappingStart(anchor, tag)
                if *anchor != 0 || tag.is_some() =>
            {
                return Err("CI workflow YAML anchors or tags are unsupported")
            }
            _ => {}
        }
        if matches!(event, Event::SequenceStart(..) | Event::MappingStart(..)) {
            depth += 1;
            if depth > MAX_YAML_DEPTH {
                return Err("CI workflow YAML exceeds the nesting limit");
            }
        } else if matches!(event, Event::SequenceEnd | Event::MappingEnd) {
            depth = depth
                .checked_sub(1)
                .ok_or("CI workflow YAML nesting is malformed")?;
        }
        let ended = event == Event::StreamEnd;
        events.push((event, mark.line() as u64));
        if events.len() > MAX_YAML_EVENTS {
            return Err("CI workflow YAML exceeds the event limit");
        }
        if ended {
            break;
        }
    }
    if documents != 1 || depth != 0 {
        return Err("CI workflow needs one complete YAML document");
    }
    let mut index = 0;
    expect_event(&events, &mut index, Event::StreamStart)?;
    expect_event(&events, &mut index, Event::DocumentStart)?;
    let root = yaml_node(&events, &mut index)?;
    expect_event(&events, &mut index, Event::DocumentEnd)?;
    expect_event(&events, &mut index, Event::StreamEnd)?;
    if index != events.len() {
        return Err("CI workflow YAML has trailing events");
    }
    Ok(root)
}

fn expect_event(events: &[(Event, u64)], index: &mut usize, expected: Event) -> Checked<()> {
    if events
        .get(*index)
        .is_none_or(|(event, _)| *event != expected)
    {
        return Err("CI workflow YAML structure is malformed");
    }
    *index += 1;
    Ok(())
}

fn yaml_node(events: &[(Event, u64)], index: &mut usize) -> Checked<Node> {
    let (event, line) = events
        .get(*index)
        .ok_or("CI workflow YAML node is missing")?;
    *index += 1;
    let value = match event {
        Event::Scalar(text, style, 0, None) => NodeValue::Scalar {
            text: text.clone(),
            plain: *style == TScalarStyle::Plain,
        },
        Event::SequenceStart(0, None) => {
            let mut items = Vec::new();
            while events
                .get(*index)
                .is_some_and(|(event, _)| *event != Event::SequenceEnd)
            {
                items.push(yaml_node(events, index)?);
            }
            expect_event(events, index, Event::SequenceEnd)?;
            NodeValue::Sequence(items)
        }
        Event::MappingStart(0, None) => {
            let mut map = BTreeMap::new();
            while events
                .get(*index)
                .is_some_and(|(event, _)| *event != Event::MappingEnd)
            {
                let key_node = yaml_node(events, index)?;
                let key = key_node.scalar()?;
                if key.is_empty() || key == "<<" || key.chars().any(char::is_control) {
                    return Err("CI workflow YAML key is unsupported");
                }
                let value = yaml_node(events, index)?;
                if map.insert(key.to_owned(), value).is_some() {
                    return Err("CI workflow YAML has duplicate keys");
                }
            }
            expect_event(events, index, Event::MappingEnd)?;
            NodeValue::Mapping(map)
        }
        _ => return Err("CI workflow YAML node is unsupported"),
    };
    Ok(Node { line: *line, value })
}

fn observe(bytes: &[u8], selector: &str) -> Checked<Observation> {
    let source = std::str::from_utf8(bytes).map_err(|_| "CI workflow blob is not UTF-8")?;
    let root = yaml(source)?;
    let root_fields = root.mapping()?;
    validate_default_shell(root_fields)?;
    let jobs = root_fields
        .get("jobs")
        .ok_or("CI Actions jobs mapping is missing")?
        .mapping()?;
    if jobs.is_empty() {
        return Err("CI Actions jobs mapping is empty");
    }
    let mut step_matches = Vec::new();
    let mut job_matches = Vec::new();
    let mut bypasses = Vec::new();
    for (job_id, job) in jobs {
        let fields = job.mapping()?;
        if fields.contains_key("uses") {
            return Err("CI reusable workflow cannot be resolved from this blob");
        }
        validate_default_shell(fields)?;
        let enabled = enabled(fields)?;
        let job_location = serde_json::json!({"job":job_id}).to_string();
        collect_bypass(fields, &job_location, &mut bypasses)?;
        if job_id == selector || literal_name(fields, "name")? == Some(selector) {
            let kind = if job_id == selector {
                "job_id"
            } else {
                "job_name"
            };
            job_matches.push((job, enabled, true, job_id.as_str(), job, kind));
        }
        let steps = fields
            .get("steps")
            .ok_or("CI Actions job steps are missing")?
            .sequence()?;
        if steps.is_empty() {
            return Err("CI Actions job steps are empty");
        }
        let mut name_counts = BTreeMap::new();
        for step in steps {
            let fields = step.mapping()?;
            if let Some(name) = literal_name(fields, "id")?.or(literal_name(fields, "name")?) {
                *name_counts.entry(name).or_insert(0usize) += 1;
            }
        }
        for (index, step) in steps.iter().enumerate() {
            let fields = step.mapping()?;
            let step_enabled = enabled && enabled_field(fields)?;
            let stable_name = literal_name(fields, "id")?.or(literal_name(fields, "name")?);
            let unique_name = stable_name.filter(|name| name_counts.get(name) == Some(&1));
            let identity = unique_name.map_or_else(
                || serde_json::json!({"index":index}),
                |name| serde_json::json!({"name":name}),
            );
            let location = serde_json::json!({"job":job_id,"step":identity}).to_string();
            collect_bypass(fields, &location, &mut bypasses)?;
            match (fields.get("run"), fields.get("uses")) {
                (Some(run), None) => {
                    if let Some(shell) = fields.get("shell") {
                        valid_shell(shell.scalar()?)?;
                    }
                    let commands = shell_commands(run.scalar()?)?;
                    for (index, command) in commands.iter().enumerate() {
                        if command.or {
                            bypasses.push(Bypass {
                                location: format!("{location}/run/{index}"),
                                kind: "|| true",
                                line: run.line,
                            });
                        }
                    }
                }
                (None, Some(uses)) => {
                    if uses.scalar()?.contains("${{") {
                        return Err("CI action identity is dynamic");
                    }
                }
                _ => return Err("CI Actions step must have one literal run or uses field"),
            }
            if literal_name(fields, "id")? == Some(selector)
                || literal_name(fields, "name")? == Some(selector)
            {
                let kind = if literal_name(fields, "id")? == Some(selector) {
                    "step_id"
                } else {
                    "step_name"
                };
                step_matches.push((step, step_enabled, false, job_id.as_str(), job, kind));
            }
        }
    }
    if step_matches.len() + job_matches.len() > 1 {
        return Err("CI accepted step or job selector is ambiguous");
    }
    let matches = if step_matches.is_empty() {
        job_matches
    } else {
        step_matches
    };
    if matches.len() > 1 {
        return Err("CI accepted step or job selector is ambiguous");
    }
    let (selected, present) = matches
        .into_iter()
        .next()
        .map(|(node, enabled, is_job, job_id, job, selector_kind)| {
            let workflow_context = without_field(&root, "jobs")?;
            let job_context = (!is_job).then(|| without_field(job, "steps")).transpose()?;
            Ok::<_, &'static str>((
                Some(Selected {
                    node: node.clone(),
                    job: is_job,
                    job_id: job_id.into(),
                    selector_kind,
                    workflow_context,
                    job_context,
                }),
                enabled,
            ))
        })
        .transpose()?
        .unwrap_or((None, false));
    let threshold = match &selected {
        Some(selected) => {
            let mut thresholds = Vec::new();
            if selected.job {
                let fields = selected.node.mapping()?;
                for step in fields
                    .get("steps")
                    .ok_or("CI selected job steps are missing")?
                    .sequence()?
                {
                    thresholds_in(step, &mut thresholds)?;
                }
            } else {
                thresholds_in(&selected.node, &mut thresholds)?;
            }
            if thresholds.len() > 1 {
                return Err("CI accepted threshold has multiple literal locations");
            }
            thresholds.first().copied()
        }
        None => None,
    };
    bypasses.sort();
    Ok(Observation {
        selected,
        present,
        threshold,
        bypasses,
    })
}

fn literal_name<'a>(fields: &'a BTreeMap<String, Node>, key: &str) -> Checked<Option<&'a str>> {
    fields
        .get(key)
        .map(|value| value.scalar())
        .transpose()
        .and_then(|value| {
            if value.is_some_and(|value| value.contains("${{")) {
                Err("CI selector identity is dynamic")
            } else {
                Ok(value)
            }
        })
}

fn enabled(fields: &BTreeMap<String, Node>) -> Checked<bool> {
    enabled_field(fields)
}

fn enabled_field(fields: &BTreeMap<String, Node>) -> Checked<bool> {
    fields
        .get("if")
        .map(|value| value.bool_literal())
        .transpose()
        .map(|value| value.unwrap_or(true))
}

fn validate_default_shell(fields: &BTreeMap<String, Node>) -> Checked<()> {
    if let Some(defaults) = fields.get("defaults") {
        if let Some(run) = defaults.mapping()?.get("run") {
            if let Some(shell) = run.mapping()?.get("shell") {
                valid_shell(shell.scalar()?)?;
            }
        }
    }
    Ok(())
}

fn valid_shell(shell: &str) -> Checked<()> {
    if matches!(shell, "bash" | "sh") {
        Ok(())
    } else {
        Err("CI shell dialect is unsupported")
    }
}

fn collect_bypass(
    fields: &BTreeMap<String, Node>,
    location: &str,
    bypasses: &mut Vec<Bypass>,
) -> Checked<()> {
    if fields.contains_key("allow_failure") {
        return Err("CI allow_failure requires a supported non-Actions dialect");
    }
    if let Some(value) = fields.get("continue-on-error") {
        if value.bool_literal()? {
            bypasses.push(Bypass {
                location: format!("{location}/continue-on-error"),
                kind: "continue-on-error",
                line: value.line,
            });
        }
    }
    Ok(())
}

fn threshold_key(key: &str) -> bool {
    matches!(key, "fail_under" | "coverageThreshold")
}

fn percent(text: &str) -> Checked<u32> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("CI coverage threshold must be an integer literal");
    }
    text.parse::<u32>()
        .ok()
        .filter(|value| *value <= 100)
        .ok_or("CI coverage threshold is outside zero to one hundred")
}

fn thresholds_in(step: &Node, values: &mut Vec<u32>) -> Checked<()> {
    let fields = step.mapping()?;
    if let Some(run) = fields.get("run") {
        for command in shell_commands(run.scalar()?)? {
            command_thresholds(&command.argv, values)?;
        }
    }
    if fields.contains_key("uses") {
        if let Some(with) = fields.get("with") {
            for (key, value) in with.mapping()? {
                if threshold_key(key) {
                    values.push(percent(value.scalar()?)?);
                }
            }
        }
    }
    Ok(())
}

fn threshold_flag(argv: &[String], flag: &str) -> bool {
    match argv.first().map(String::as_str) {
        Some("coverage") if argv.get(1).map(String::as_str) == Some("report") => {
            flag == "--fail-under"
        }
        Some("pytest") => flag == "--cov-fail-under",
        Some("cargo") if argv.get(1).map(String::as_str) == Some("llvm-cov") => matches!(
            flag,
            "--fail-under-lines" | "--fail-under-functions" | "--fail-under-regions"
        ),
        _ => false,
    }
}

fn command_thresholds(argv: &[String], values: &mut Vec<u32>) -> Checked<()> {
    let mut index = 0;
    while index < argv.len() {
        let (flag, inline) = argv[index]
            .split_once('=')
            .map_or((argv[index].as_str(), None), |(flag, value)| {
                (flag, Some(value))
            });
        if threshold_flag(argv, flag) {
            let value = if let Some(value) = inline {
                value
            } else {
                index += 1;
                argv.get(index)
                    .map(String::as_str)
                    .ok_or("CI threshold flag has no value")?
            };
            values.push(percent(value)?);
        }
        index += 1;
    }
    Ok(())
}

fn without_field(node: &Node, key: &str) -> Checked<Node> {
    let mut fields = node.mapping()?.clone();
    fields.remove(key);
    Ok(Node {
        line: node.line,
        value: NodeValue::Mapping(fields),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExecutionScope {
    Literal,
    Job,
    Step,
    With,
}

fn execution_signature(
    selected: &Selected,
    normalize_threshold: bool,
) -> Checked<serde_json::Value> {
    Ok(serde_json::json!({
        "job_id":selected.job_id,
        "is_job":selected.job,
        "workflow":execution_node(&selected.workflow_context, ExecutionScope::Literal, false)?,
        "job_context":selected.job_context.as_ref().map(|node| execution_node(node, ExecutionScope::Job, false)).transpose()?,
        "body":execution_node(&selected.node, if selected.job { ExecutionScope::Job } else { ExecutionScope::Step }, normalize_threshold)?,
    }))
}

fn execution_node(
    node: &Node,
    scope: ExecutionScope,
    normalize_threshold: bool,
) -> Checked<serde_json::Value> {
    Ok(match &node.value {
        NodeValue::Scalar { text, plain } => serde_json::json!({"scalar":text,"plain":plain}),
        NodeValue::Sequence(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|item| execution_node(item, scope, normalize_threshold))
                .collect::<Checked<Vec<_>>>()?,
        ),
        NodeValue::Mapping(fields) => {
            let mut object = serde_json::Map::new();
            for (key, value) in fields {
                if matches!(scope, ExecutionScope::Job | ExecutionScope::Step)
                    && matches!(key.as_str(), "continue-on-error" | "allow_failure")
                {
                    continue;
                }
                let value = if scope == ExecutionScope::Step && key == "run" {
                    let commands = shell_commands(value.scalar()?)?;
                    let mut normalized = Vec::new();
                    for command in commands {
                        if command.or {
                            continue;
                        }
                        let mut argv = command.argv;
                        if normalize_threshold {
                            let mut index = 0;
                            while index < argv.len() {
                                let (flag, inline) = argv[index]
                                    .split_once('=')
                                    .map_or((argv[index].as_str(), false), |(flag, _)| {
                                        (flag, true)
                                    });
                                if threshold_flag(&argv, flag) {
                                    if inline {
                                        argv[index] = format!("{flag}=__accepted_threshold__");
                                    } else {
                                        index += 1;
                                        let slot = argv
                                            .get_mut(index)
                                            .ok_or("CI threshold flag has no value")?;
                                        *slot = "__accepted_threshold__".into();
                                    }
                                }
                                index += 1;
                            }
                        }
                        normalized
                            .push(serde_json::json!({"operator":command.operator,"argv":argv}));
                    }
                    serde_json::Value::Array(normalized)
                } else if scope == ExecutionScope::With && normalize_threshold && threshold_key(key)
                {
                    serde_json::json!("__accepted_threshold__")
                } else {
                    let child_scope = if scope == ExecutionScope::Job && key == "steps" {
                        ExecutionScope::Step
                    } else if scope == ExecutionScope::Step
                        && fields.contains_key("uses")
                        && key == "with"
                    {
                        ExecutionScope::With
                    } else {
                        ExecutionScope::Literal
                    };
                    execution_node(value, child_scope, normalize_threshold)?
                };
                object.insert(key.clone(), value);
            }
            serde_json::Value::Object(object)
        }
    })
}

struct ShellCommand {
    argv: Vec<String>,
    operator: &'static str,
    or: bool,
}

/// A deliberately closed shell grammar: literal argv, quotes, comments,
/// newlines, semicolons, && and ||. No shell expansion or nested program text.
fn shell_commands(source: &str) -> Checked<Vec<ShellCommand>> {
    let mut chars = source.chars().peekable();
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote = None;
    let mut commands = Vec::new();
    let mut operator = "start";
    while let Some(ch) = chars.next() {
        if let Some(delimiter) = quote {
            if ch == delimiter {
                quote = None;
            } else if delimiter == '"' && matches!(ch, '$' | '`') {
                return Err("CI shell expansion is unsupported");
            } else if delimiter == '"' && ch == '\\' {
                let escaped = chars.next().ok_or("CI shell quote escape is incomplete")?;
                if escaped != '\n' {
                    if !matches!(escaped, '$' | '`' | '"' | '\\') {
                        word.push('\\');
                    }
                    word.push(escaped);
                }
            } else {
                word.push(ch);
            }
            continue;
        }
        match ch {
            '\'' | '"' => {
                started = true;
                quote = Some(ch);
            }
            '\\' => {
                let escaped = chars.next().ok_or("CI shell escape is incomplete")?;
                if escaped != '\n' {
                    started = true;
                    word.push(escaped);
                }
            }
            '#' if !started => {
                while chars.peek().is_some_and(|ch| *ch != '\n') {
                    chars.next();
                }
            }
            ' ' | '\t' | '\r' => flush_word(&mut words, &mut word, &mut started),
            '\n' | ';' | '&' | '|' => {
                flush_word(&mut words, &mut word, &mut started);
                if ch == ';' && words.is_empty() {
                    return Err("CI shell empty statement is unsupported");
                }
                let next_operator = match ch {
                    '&' if chars.next_if_eq(&'&').is_some() => "and",
                    '|' if chars.next_if_eq(&'|').is_some() => "or",
                    '&' | '|' => {
                        return Err("CI shell pipelines or background jobs are unsupported")
                    }
                    _ => "sequence",
                };
                if !words.is_empty() {
                    commands.push(shell_command(std::mem::take(&mut words), operator)?);
                    operator = next_operator;
                } else if next_operator != "sequence" {
                    return Err("CI shell operator has no command");
                }
            }
            '$' | '`' | '(' | ')' | '{' | '}' | '<' | '>' | '*' | '?' | '[' | ']' | '~' => {
                return Err("CI shell expansion or control syntax is unsupported")
            }
            ch if ch.is_control() => {
                return Err("CI shell text has unsupported control characters")
            }
            _ => {
                started = true;
                word.push(ch);
            }
        }
    }
    if quote.is_some() {
        return Err("CI shell quote is incomplete");
    }
    flush_word(&mut words, &mut word, &mut started);
    if !words.is_empty() {
        commands.push(shell_command(words, operator)?);
    } else if matches!(operator, "and" | "or") {
        return Err("CI shell operator has no command");
    }
    if commands.is_empty() {
        return Err("CI shell has no literal command");
    }
    Ok(commands)
}

fn flush_word(words: &mut Vec<String>, word: &mut String, started: &mut bool) {
    if *started {
        words.push(std::mem::take(word));
        *started = false;
    }
}

fn shell_command(argv: Vec<String>, operator: &'static str) -> Checked<ShellCommand> {
    let name = argv.first().ok_or("CI shell command is empty")?;
    let base = name.rsplit('/').next().unwrap_or(name);
    if matches!(
        base,
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "pwsh"
            | "powershell"
            | "eval"
            | "source"
            | "."
            | "exec"
            | "exit"
            | "set"
            | "trap"
            | "if"
            | "then"
            | "else"
            | "fi"
            | "for"
            | "while"
            | "until"
            | "case"
            | "do"
            | "done"
            | "function"
            | "!"
            | ":"
            | "env"
            | "command"
            | "sudo"
            | "timeout"
            | "nohup"
    ) || name.contains('=')
        || name.is_empty()
    {
        return Err("CI shell nested commands or control statements are unsupported");
    }
    if operator == "or" && base != "true" {
        return Err("CI shell OR branch is outside the closed bypass grammar");
    }
    Ok(ShellCommand {
        argv,
        operator,
        or: operator == "or",
    })
}

fn finding(path: &str, line: u64, span: &str) -> Finding {
    Finding {
        surface: Surface::Ci,
        path: Masked::mask(path).into_text(),
        line,
        span: span.into(),
    }
}

fn broken(tree: &str, findings: Vec<Finding>) -> CheckOutcome {
    CheckOutcome {
        state: VerdictState::Broken,
        tree_hash: Some(tree.into()),
        reason: None,
        checked_places: 1,
        surfaces: vec![SurfaceResult {
            surface: Surface::Ci,
            checked_places: 1,
            finding_count: findings.len() as u64,
            state: VerdictState::Broken,
        }],
        findings,
        not_looked_at: Vec::new(),
        visual: Vec::new(),
    }
}

fn workflow_blob(repo: &Path, tree: &str, path: &str) -> Checked<Option<(String, Vec<u8>)>> {
    let listing = git_bounded(
        repo,
        &["ls-tree", "-z", "--full-tree", tree, "--", path],
        16 * 1024,
    )
    .ok_or("CI workflow tree entry cannot be read")?;
    for record in listing
        .split(|byte| *byte == 0)
        .filter(|row| !row.is_empty())
    {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or("CI tree entry is malformed")?;
        if &record[tab + 1..] != path.as_bytes() {
            continue;
        }
        let header =
            std::str::from_utf8(&record[..tab]).map_err(|_| "CI tree entry is malformed")?;
        let mut fields = header.split_ascii_whitespace();
        let (Some(mode), Some(kind), Some(oid), None) =
            (fields.next(), fields.next(), fields.next(), fields.next())
        else {
            return Err("CI tree entry is malformed");
        };
        if !matches!(mode, "100644" | "100755") || kind != "blob" || !is_oid(oid) {
            return Err("CI workflow has an unsupported Git mode or object");
        }
        let size = git_bounded(repo, &["cat-file", "-s", oid], 1024)
            .and_then(|raw| String::from_utf8(raw).ok())
            .and_then(|text| text.trim().parse::<usize>().ok())
            .ok_or("CI workflow blob size cannot be read")?;
        if size > MAX_BLOB_BYTES {
            return Err("CI workflow blob exceeds the byte limit");
        }
        let bytes = git_bounded(repo, &["cat-file", "blob", oid], size)
            .ok_or("CI workflow blob cannot be read")?;
        if bytes.len() != size {
            return Err("CI workflow blob size differs from the receipt");
        }
        return Ok(Some((oid.into(), bytes)));
    }
    Ok(None)
}

fn tree(repo: &Path, version: &str) -> Option<String> {
    let spec = format!("{version}^{{tree}}");
    let raw = git_bounded(
        repo,
        &["rev-parse", "--verify", "--end-of-options", &spec],
        1024,
    )?;
    let oid = std::str::from_utf8(&raw).ok()?.trim();
    is_oid(oid).then(|| oid.into())
}

fn is_exact_commit(repo: &Path, version: &str) -> bool {
    if !is_oid(version) {
        return false;
    }
    let spec = format!("{version}^{{commit}}");
    git_bounded(
        repo,
        &["rev-parse", "--verify", "--end-of-options", &spec],
        1024,
    )
    .and_then(|raw| String::from_utf8(raw).ok())
    .is_some_and(|resolved| resolved.trim().eq_ignore_ascii_case(version))
}

fn is_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.starts_with('~')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn git_bounded(repo: &Path, args: &[&str], limit: usize) -> Option<Vec<u8>> {
    let mut child = isolate_config(&mut Command::new("git"))
        .args(args)
        .current_dir(repo)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() > limit {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    child.wait().ok()?.success().then_some(bytes)
}
