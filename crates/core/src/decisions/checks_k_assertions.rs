//! C28 F1: exact-tree direct assertion structure, independent of B2 execution.
//! Holds is limited to direct standard assertions with runtime call operands.
//! Semantic tautologies and execution are not certified; K4 remains Partial.
//! Unknown helpers, closures, attributes and control flow abstain. This pure
//! checker grants no acceptance or standing authority.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use crate::git_isolation::isolate_config;
use crate::redact::Masked;

use super::checks::{CheckOutcome, Finding, SurfaceResult, VerdictState};
use super::params::Surface;

const MAX_BLOB_BYTES: usize = 256 * 1024;
const MAX_AST_NODES: usize = 20_000;
const MAX_AST_DEPTH: usize = 128;
const MAX_ASSERTIONS: usize = 128;
const MAX_ARGUMENT_BYTES: usize = 4096;
const GIT_TIMEOUT: Duration = Duration::from_secs(5);
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RustNamedTestParams {
    pub path: String,
    pub test_name: String,
}

pub fn check_at(
    repo: &Path,
    version: &str,
    tree_hash: &str,
    params: &RustNamedTestParams,
) -> CheckOutcome {
    let deadline = Instant::now() + CHECK_TIMEOUT;
    let mut tree = None;
    match check_inner(repo, version, tree_hash, params, deadline, &mut tree) {
        Ok(result) => result,
        Err(reason) => CheckOutcome::unavailable(reason, tree),
    }
}

fn check_inner(
    repo: &Path,
    version: &str,
    claimed_tree: &str,
    params: &RustNamedTestParams,
    deadline: Instant,
    bound: &mut Option<String>,
) -> Result<CheckOutcome, &'static str> {
    if !is_oid(version) || !is_oid(claimed_tree) {
        return Err("K4 requires an exact full commit and tree identity");
    }
    let spec = format!("{version}^{{commit}}");
    let commit = git(
        repo,
        &["rev-parse", "--verify", "--end-of-options", &spec],
        128,
        deadline,
    )
    .ok_or("K4 exact commit cannot be read")?;
    if std::str::from_utf8(&commit).ok().map(str::trim) != Some(version) {
        return Err("K4 version is not the exact commit object");
    }
    let spec = format!("{version}^{{tree}}");
    let raw_tree = git(
        repo,
        &["rev-parse", "--verify", "--end-of-options", &spec],
        128,
        deadline,
    )
    .ok_or("K4 exact tree cannot be read")?;
    let tree = std::str::from_utf8(&raw_tree)
        .map_err(|_| "K4 tree identity is malformed")?
        .trim();
    if tree != claimed_tree {
        return Err("K4 claimed tree does not match the commit");
    }
    *bound = Some(tree.into());
    if !safe_path(&params.path) || !identifier(&params.test_name) {
        return Err("K4 needs one literal Rust path and one exact test name");
    }
    let listing = git(
        repo,
        &["ls-tree", "-z", tree, "--", &params.path],
        4096,
        deadline,
    )
    .ok_or("K4 source entry cannot be read")?;
    let entries = listing
        .split(|byte| *byte == 0)
        .filter(|row| !row.is_empty())
        .collect::<Vec<_>>();
    if entries.len() != 1 {
        return Err("K4 source entry is absent or ambiguous");
    }
    let tab = entries[0]
        .iter()
        .position(|byte| *byte == b'\t')
        .ok_or("K4 tree entry is malformed")?;
    if &entries[0][tab + 1..] != params.path.as_bytes() {
        return Err("K4 source path does not match exactly");
    }
    let header =
        std::str::from_utf8(&entries[0][..tab]).map_err(|_| "K4 tree entry is malformed")?;
    let parts = header.split_ascii_whitespace().collect::<Vec<_>>();
    if parts.len() != 3
        || !matches!(parts[0], "100644" | "100755")
        || parts[1] != "blob"
        || !is_oid(parts[2])
    {
        return Err("K4 source is not a regular Git blob");
    }
    let size = git(repo, &["cat-file", "-s", parts[2]], 128, deadline)
        .and_then(|raw| String::from_utf8(raw).ok())
        .and_then(|raw| raw.trim().parse::<usize>().ok())
        .ok_or("K4 source size cannot be read")?;
    if size > MAX_BLOB_BYTES {
        return Err("K4 source exceeds the 256 KiB byte limit");
    }
    let raw = git(repo, &["cat-file", "blob", parts[2]], size, deadline)
        .ok_or("K4 source blob cannot be read")?;
    if raw.len() != size {
        return Err("K4 source size does not match");
    }
    let source = std::str::from_utf8(&raw).map_err(|_| "K4 source is not UTF-8")?;
    let syntax = syntax(source, deadline)?;
    let nodes = bounded_nodes(syntax.root_node())?;
    if Instant::now() >= deadline {
        return Err("K4 check exceeded its time budget");
    }
    if nodes.iter().any(|node| {
        matches!(
            node.kind(),
            "macro_definition"
                | "inner_attribute_item"
                | "use_declaration"
                | "extern_crate_declaration"
        ) || (node.kind() == "mod_item"
            && node
                .child_by_field_name("name")
                .is_some_and(|name| text(name, source) == "std"))
    }) {
        return Err("K4 assertion or test bindings are unsupported");
    }
    let functions = nodes
        .iter()
        .copied()
        .filter(|node| {
            node.kind() == "function_item"
                && node
                    .child_by_field_name("name")
                    .is_some_and(|name| text(name, source) == params.test_name)
        })
        .collect::<Vec<_>>();
    if functions.len() != 1 {
        return Err("K4 named test is absent or duplicated");
    }
    let function = functions[0];
    let mut previous = function.prev_named_sibling();
    let mut attributes = Vec::new();
    while let Some(node) = previous {
        match node.kind() {
            "attribute_item" => attributes.push(compact(text(node, source))),
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        previous = node.prev_named_sibling();
    }
    if attributes != ["#[test]"] {
        return Err("K4 needs one plain #[test] attribute without conditions");
    }
    if nodes.iter().any(|node| {
        node.kind() == "attribute_item" && {
            let attr = compact(text(*node, source));
            attr != "#[test]" && !attr.starts_with("#[path=")
        }
    }) {
        return Err("K4 conditional or custom attributes are unsupported");
    }
    let body = function
        .child_by_field_name("body")
        .ok_or("K4 test body is unavailable")?;
    if compact(&source[function.start_byte()..body.start_byte()])
        != format!("fn{}()", params.test_name)
    {
        return Err("K4 supports only a synchronous unit test function");
    }
    let mut assertions = 0;
    let mut broken = Vec::new();
    for index in 0..body.named_child_count() {
        let statement = body
            .named_child(index)
            .ok_or("K4 test statement is unavailable")?;
        if matches!(statement.kind(), "line_comment" | "block_comment") {
            continue;
        }
        if statement.kind() != "expression_statement" || statement.named_child_count() != 1 {
            return Err("K4 body has helpers, control flow or unsupported statements");
        }
        let invocation = statement
            .named_child(0)
            .ok_or("K4 assertion statement is unavailable")?;
        if invocation.kind() != "macro_invocation" {
            return Err("K4 body has a helper or unsupported expression");
        }
        if assertions >= MAX_ASSERTIONS {
            return Err("K4 exceeds the 128 assertion limit");
        }
        if assertion(invocation, source, deadline)? {
            broken.push(invocation);
        }
        assertions += 1;
    }
    if Instant::now() >= deadline {
        return Err("K4 check exceeded its time budget");
    }
    if assertions == 0 {
        return Ok(outcome(
            VerdictState::Broken,
            tree,
            "named test has no assertions",
            vec![finding(params, function)],
        ));
    }
    if !broken.is_empty() {
        return Ok(outcome(
            VerdictState::Broken,
            tree,
            "named test contains a supported literal tautological assertion",
            broken
                .into_iter()
                .map(|node| finding(params, node))
                .collect(),
        ));
    }
    Ok(outcome(VerdictState::Holds, tree, "named test has direct nonliteral standard assertions in the supported structural scope; semantic tautologies and execution are not certified", Vec::new()))
}

fn outcome(state: VerdictState, tree: &str, reason: &str, findings: Vec<Finding>) -> CheckOutcome {
    CheckOutcome {
        state,
        tree_hash: Some(tree.into()),
        reason: Some(reason.into()),
        checked_places: 1,
        surfaces: vec![SurfaceResult {
            surface: Surface::Tests,
            checked_places: 1,
            finding_count: findings.len() as u64,
            state,
        }],
        findings,
        not_looked_at: Vec::new(),
        visual: Vec::new(),
    }
}
fn finding(params: &RustNamedTestParams, node: Node<'_>) -> Finding {
    Finding {
        surface: Surface::Tests,
        path: Masked::mask(&params.path).into_text(),
        line: node.start_position().row as u64 + 1,
        span: Masked::mask(&params.test_name).into_text(),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Literal {
    Bool(bool),
    Integer(i128),
}
#[derive(Clone, Copy)]
enum Operand {
    Literal(Literal),
    Runtime,
}

fn assertion(node: Node<'_>, source: &str, deadline: Instant) -> Result<bool, &'static str> {
    let name = node
        .child_by_field_name("macro")
        .ok_or("K4 assertion macro is unavailable")?;
    let operation = match compact(text(name, source)).as_str() {
        "::std::assert" => "assert",
        "::std::assert_eq" => "eq",
        "::std::assert_ne" => "ne",
        _ => return Err("K4 requires an absolute standard assertion macro"),
    };
    let tokens = (0..node.named_child_count())
        .filter_map(|i| node.named_child(i))
        .find(|child| child.kind() == "token_tree")
        .ok_or("K4 assertion arguments are unavailable")?;
    let raw = text(tokens, source).trim();
    let args = raw
        .strip_prefix('(')
        .and_then(|value| value.strip_suffix(')'))
        .ok_or("K4 supports parenthesized assertion macros only")?
        .trim();
    if args.len() > MAX_ARGUMENT_BYTES {
        return Err("K4 assertion exceeds the argument byte limit");
    }
    let args = args.strip_suffix(',').unwrap_or(args);
    let wrapped = format!("fn __k4() {{ let __args = ({args},); }}");
    let parsed = syntax(&wrapped, deadline)?;
    let nodes = bounded_nodes(parsed.root_node())?;
    let tuple = nodes
        .iter()
        .copied()
        .find(|node| node.kind() == "tuple_expression")
        .ok_or("K4 assertion operands cannot be parsed")?;
    let operands = (0..tuple.named_child_count())
        .filter_map(|i| tuple.named_child(i))
        .filter(|node| !matches!(node.kind(), "line_comment" | "block_comment"))
        .collect::<Vec<_>>();
    let value = if operation == "assert" && operands.len() == 1 {
        operand(operands[0], &wrapped, 0)?
    } else if operation != "assert" && operands.len() == 2 {
        comparison(
            operand(operands[0], &wrapped, 0)?,
            operand(operands[1], &wrapped, 0)?,
            operation == "eq",
        )?
    } else {
        return Err("K4 assertion argument shape is unsupported");
    };
    match value {
        Operand::Literal(Literal::Bool(true)) => Ok(true),
        Operand::Runtime => Ok(false),
        _ => Err("K4 constant assertion is not a supported literal-true witness"),
    }
}

fn operand(node: Node<'_>, source: &str, depth: usize) -> Result<Operand, &'static str> {
    if depth > 32 {
        return Err("K4 expression depth exceeds its limit");
    }
    match node.kind() {
        "boolean_literal" => Ok(Operand::Literal(Literal::Bool(
            text(node, source) == "true",
        ))),
        "integer_literal" if text(node, source).bytes().all(|byte| byte.is_ascii_digit()) => {
            text(node, source)
                .parse()
                .map(|value| Operand::Literal(Literal::Integer(value)))
                .map_err(|_| "K4 integer is outside the supported range")
        }
        "parenthesized_expression" if node.named_child_count() == 1 => {
            operand(node.named_child(0).unwrap(), source, depth + 1)
        }
        "call_expression" => {
            let function = node
                .child_by_field_name("function")
                .ok_or("K4 call target is unavailable")?;
            if !matches!(function.kind(), "identifier" | "scoped_identifier") {
                return Err("K4 dynamic call target is unsupported");
            }
            let args = node
                .child_by_field_name("arguments")
                .ok_or("K4 call arguments are unavailable")?;
            for index in 0..args.named_child_count() {
                if !matches!(
                    operand(args.named_child(index).unwrap(), source, depth + 1)?,
                    Operand::Literal(_)
                ) {
                    return Err("K4 call input is outside the literal scope");
                }
            }
            Ok(Operand::Runtime)
        }
        "unary_expression" => {
            if !text(node, source).trim_start().starts_with('!') || node.named_child_count() != 1 {
                return Err("K4 unary operand is unsupported");
            }
            match operand(node.named_child(0).unwrap(), source, depth + 1)? {
                Operand::Literal(Literal::Bool(value)) => {
                    Ok(Operand::Literal(Literal::Bool(!value)))
                }
                Operand::Runtime => Ok(Operand::Runtime),
                _ => Err("K4 unary operand is not boolean"),
            }
        }
        "binary_expression" => {
            let op = node
                .child_by_field_name("operator")
                .ok_or("K4 operator is unavailable")?;
            let equals = match text(op, source) {
                "==" => true,
                "!=" => false,
                _ => return Err("K4 operator is unsupported"),
            };
            let left = node
                .child_by_field_name("left")
                .ok_or("K4 comparison operand is unavailable")?;
            let right = node
                .child_by_field_name("right")
                .ok_or("K4 comparison operand is unavailable")?;
            comparison(
                operand(left, source, depth + 1)?,
                operand(right, source, depth + 1)?,
                equals,
            )
        }
        _ => Err("K4 operand is outside the supported structural scope"),
    }
}
fn comparison(left: Operand, right: Operand, equals: bool) -> Result<Operand, &'static str> {
    match (left, right) {
        (Operand::Literal(left), Operand::Literal(right))
            if std::mem::discriminant(&left) == std::mem::discriminant(&right) =>
        {
            Ok(Operand::Literal(Literal::Bool((left == right) == equals)))
        }
        (Operand::Runtime, Operand::Literal(_)) | (Operand::Literal(_), Operand::Runtime) => {
            Ok(Operand::Runtime)
        }
        _ => Err("K4 comparison is outside the one-call literal scope"),
    }
}

fn syntax(source: &str, deadline: Instant) -> Result<tree_sitter::Tree, &'static str> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .map_err(|_| "K4 Rust parser is unavailable")?;
    let mut progress = |_: &tree_sitter::ParseState| Instant::now() >= deadline;
    let mut input =
        |offset: usize, _: tree_sitter::Point| source.as_bytes().get(offset..).unwrap_or_default();
    let tree = parser
        .parse_with_options(
            &mut input,
            None,
            Some(tree_sitter::ParseOptions::new().progress_callback(&mut progress)),
        )
        .ok_or("K4 Rust source cannot be parsed")?;
    if tree.root_node().has_error() {
        return Err("K4 Rust source has malformed syntax");
    }
    Ok(tree)
}
fn bounded_nodes(root: Node<'_>) -> Result<Vec<Node<'_>>, &'static str> {
    let mut pending = vec![(root, 0)];
    let mut nodes = Vec::new();
    while let Some((node, depth)) = pending.pop() {
        if depth > MAX_AST_DEPTH
            || nodes.len() + pending.len() + node.child_count() >= MAX_AST_NODES
        {
            return Err("K4 source exceeds its AST node or depth limit");
        }
        nodes.push(node);
        for index in (0..node.child_count()).rev() {
            pending.push((node.child(index).unwrap(), depth + 1));
        }
    }
    Ok(nodes)
}
fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}
fn compact(value: &str) -> String {
    value.chars().filter(|ch| !ch.is_whitespace()).collect()
}
fn is_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        })
}
fn safe_path(path: &str) -> bool {
    path.len() <= 1024
        && path.ends_with(".rs")
        && !path.starts_with(['/', '~'])
        && !path.contains(['\\', '*', '?', '[', ':'])
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn git(repo: &Path, args: &[&str], limit: usize, deadline: Instant) -> Option<Vec<u8>> {
    let timeout = deadline
        .checked_duration_since(Instant::now())?
        .min(GIT_TIMEOUT);
    let mut child = isolate_config(&mut Command::new("git"))
        .args(args)
        .current_dir(repo)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let (send, receive) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut raw = Vec::new();
        let read = stdout.take(limit as u64 + 1).read_to_end(&mut raw);
        let _ = send.send(read.ok().map(|_| raw));
    });
    let started = Instant::now();
    let raw = match receive.recv_timeout(timeout) {
        Ok(Some(raw)) if raw.len() <= limit => raw,
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
    };
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success().then_some(raw),
            Ok(None) if started.elapsed() < timeout => std::thread::sleep(Duration::from_millis(1)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}
