//! Narrow K6 advisory associations from bounded, immutable Python blobs.
//!
//! A positive receipt proves a new literal dictionary lookup and a stable
//! literal input/expected pair in a uniquely linked test source. It establishes
//! neither execution, intent, acceptance nor a Decisions verdict.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crate::git_isolation::isolate_config;
use crate::redact::Masked;

use super::checks_k_candidates::{KCandidateObservation, KCandidateOutcome, KCandidateState};

const MAX_FILES: usize = 10_000;
const MAX_BLOB_BYTES: usize = 2 * 1024 * 1024;
const MAX_TOTAL_BYTES: usize = 32 * 1024 * 1024;
const MAX_TREE_BYTES: usize = 16 * 1024 * 1024;
const MAX_NODES: usize = 300_000;
const MAX_WITNESSES: usize = 4096;
const MAX_SIGNATURE_ENTRIES: usize = 4096;
const MAX_PATH_BYTES: usize = 1024;
const MAX_LITERAL_BYTES: usize = 1024;
const GIT_TIMEOUT: Duration = Duration::from_secs(5);
const UNLINKED: &str = "fixture-input values are not linked to source lookup keys";

type Signature = BTreeMap<String, String>;
type PairKey = (String, String, String, String);
type TableIdentity = (String, String, Signature, Option<String>);

#[derive(Clone)]
struct Table {
    path: String,
    function: String,
    line: u64,
    signature: Signature,
    identity: TableIdentity,
}

#[derive(Clone)]
struct Pair {
    key: PairKey,
    path: String,
    line: u64,
}

struct Snapshot {
    tables: Vec<Table>,
    pairs: Vec<Pair>,
    files: usize,
}

/// Evaluate only bounded Python source forms. `NoSignal` is scoped structural
/// absence; unsupported syntax or absent fixture links remain Unverifiable.
pub fn scan(repo: &Path, baseline: &str, target: &str) -> KCandidateOutcome {
    let result = scan_inner(repo, baseline, target);
    match result {
        Ok(outcome) => outcome,
        Err(reason) => outcome(KCandidateState::Unverifiable, reason, Vec::new(), 0),
    }
}

fn outcome(
    state: KCandidateState,
    reason: &'static str,
    observations: Vec<KCandidateObservation>,
    examined_source_files: usize,
) -> KCandidateOutcome {
    KCandidateOutcome {
        code: "K6".into(),
        state,
        observations,
        reason: Some(reason),
        examined_source_files,
    }
}

fn scan_inner(
    repo: &Path,
    baseline: &str,
    target: &str,
) -> Result<KCandidateOutcome, &'static str> {
    if !exact_commit(repo, baseline) || !exact_commit(repo, target) {
        return Err("exact full commit identity is unavailable");
    }
    let changed = git(
        repo,
        &[
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            baseline,
            target,
        ],
        MAX_TREE_BYTES,
    )
    .ok_or("exact changed-path census cannot be read")?;
    let mut python_changed = false;
    for raw in changed
        .split(|byte| *byte == 0)
        .filter(|raw| !raw.is_empty())
    {
        let path = std::str::from_utf8(raw).map_err(|_| "source path is not UTF-8")?;
        if excluded(path) {
            continue;
        }
        if path.ends_with(".py") {
            python_changed = true;
        } else if [
            ".rs", ".js", ".jsx", ".ts", ".tsx", ".go", ".java", ".rb", ".php", ".swift", ".kt",
            ".cs",
        ]
        .iter()
        .any(|suffix| path.ends_with(suffix))
        {
            return Err("K6 fixture lookup parsing supports Python source only");
        }
    }
    if !python_changed {
        return Err("no changed Python production slice is available for K6");
    }
    let mut blobs = BTreeMap::new();
    let mut total = 0;
    let before_sources = sources(repo, baseline, &mut blobs, &mut total)?;
    let after_sources = sources(repo, target, &mut blobs, &mut total)?;
    let before = snapshot(&before_sources)?;
    let after = snapshot(&after_sources)?;
    let moved_files = exact_unique_file_moves(&before_sources, &after_sources);
    let mut prior: BTreeMap<String, BTreeSet<TableIdentity>> = BTreeMap::new();
    for table in before.tables {
        prior.entry(table.path).or_default().insert(table.identity);
    }
    let stable = before
        .pairs
        .into_iter()
        .map(|pair| pair.key)
        .collect::<BTreeSet<_>>();
    let mut observations = Vec::new();
    let mut unlinked = false;
    for table in after.tables {
        let prior_path = moved_files.get(&table.path).unwrap_or(&table.path);
        if prior
            .get(prior_path)
            .is_some_and(|identities| identities.contains(&table.identity))
        {
            continue;
        }
        let matching = after
            .pairs
            .iter()
            .filter(|pair| {
                pair.key.0 == table.path
                    && pair.key.1 == table.function
                    && stable.contains(&pair.key)
                    && table.signature.get(&pair.key.2) == Some(&pair.key.3)
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            unlinked = true;
            continue;
        }
        observations.push(KCandidateObservation {
            path: Masked::mask(&table.path).into_text(),
            line: table.line,
            marker: "new_literal_fixture_answer_table",
        });
        for pair in matching {
            observations.push(KCandidateObservation {
                path: Masked::mask(&pair.path).into_text(),
                line: pair.line,
                marker: "stable_literal_test_input_and_expected_output",
            });
        }
    }
    observations.sort_by(|a, b| (&a.path, a.line, a.marker).cmp(&(&b.path, b.line, b.marker)));
    observations.dedup();
    if !observations.is_empty() {
        Ok(outcome(KCandidateState::Signal, "a new literal Python lookup table shares stable input/output pairs with uniquely linked test source; execution and intent are unproved", observations, after.files))
    } else if unlinked {
        Ok(outcome(
            KCandidateState::Unverifiable,
            UNLINKED,
            Vec::new(),
            after.files,
        ))
    } else {
        Ok(outcome(KCandidateState::NoSignal, "no net new supported literal Python lookup structure was found; this is not a full K6 verdict", Vec::new(), after.files))
    }
}

fn exact_unique_file_moves(
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut prior: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut current: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (path, source) in before {
        prior
            .entry(source.as_str())
            .or_default()
            .push(path.as_str());
    }
    for (path, source) in after {
        current
            .entry(source.as_str())
            .or_default()
            .push(path.as_str());
    }
    let mut moves = BTreeMap::new();
    for (source, paths) in current {
        if paths.len() != 1 || before.contains_key(paths[0]) {
            continue;
        }
        let Some(old) = prior.get(source) else {
            continue;
        };
        if old.len() == 1 && !after.contains_key(old[0]) {
            moves.insert(paths[0].to_owned(), old[0].to_owned());
        }
    }
    moves
}

fn sources(
    repo: &Path,
    commit: &str,
    cache: &mut BTreeMap<String, String>,
    total: &mut usize,
) -> Result<BTreeMap<String, String>, &'static str> {
    let listing = git(
        repo,
        &["ls-tree", "-r", "-z", "--full-tree", commit],
        MAX_TREE_BYTES,
    )
    .ok_or("bounded exact Python tree census is unavailable")?;
    let mut result = BTreeMap::new();
    for row in listing
        .split(|byte| *byte == 0)
        .filter(|row| !row.is_empty())
    {
        let tab = row
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or("tree entry is malformed")?;
        let path = std::str::from_utf8(&row[tab + 1..]).map_err(|_| "tree path is not UTF-8")?;
        if !safe_path(path) {
            return Err("tree path is unsafe");
        }
        if !path.ends_with(".py") || excluded(path) {
            continue;
        }
        if result.len() >= MAX_FILES {
            return Err("Python tree exceeds the file budget");
        }
        let header = std::str::from_utf8(&row[..tab]).map_err(|_| "tree entry is malformed")?;
        let fields = header.split_ascii_whitespace().collect::<Vec<_>>();
        if fields.len() != 3
            || !matches!(fields[0], "100644" | "100755")
            || fields[1] != "blob"
            || !full_oid(fields[2])
        {
            return Err("Python tree has an unsupported mode or object");
        }
        let oid = fields[2];
        let source = if let Some(source) = cache.get(oid) {
            *total = total
                .checked_add(source.len())
                .filter(|bytes| *bytes <= MAX_TOTAL_BYTES)
                .ok_or("Python trees exceed the total byte budget")?;
            source.clone()
        } else {
            let size = git(repo, &["cat-file", "-s", oid], 32)
                .and_then(|raw| String::from_utf8(raw).ok())
                .and_then(|raw| raw.trim().parse::<usize>().ok())
                .ok_or("Python blob size is unavailable")?;
            if size > MAX_BLOB_BYTES {
                return Err("Python blob exceeds the byte budget");
            }
            *total = total
                .checked_add(size)
                .filter(|bytes| *bytes <= MAX_TOTAL_BYTES)
                .ok_or("Python trees exceed the total byte budget")?;
            let raw =
                git(repo, &["cat-file", "blob", oid], size).ok_or("Python blob is unavailable")?;
            if raw.len() != size {
                return Err("Python blob size disagrees with its exact object");
            }
            let source = String::from_utf8(raw).map_err(|_| "Python blob is not UTF-8")?;
            cache.insert(oid.into(), source.clone());
            source
        };
        if result.insert(path.to_owned(), source).is_some() {
            return Err("Python tree repeats a source path");
        }
    }
    Ok(result)
}

fn parse_python(source: &str) -> Result<tree_sitter::Tree, &'static str> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .map_err(|_| "Python grammar is unavailable")?;
    let tree = parser
        .parse(source, None)
        .ok_or("Python syntax is unavailable")?;
    let root = tree.root_node();
    if root.has_error() {
        return Err("Python source has malformed syntax");
    }
    if nodes(root)?.iter().any(|node| node.is_missing()) {
        return Err("Python source has missing syntax");
    }
    Ok(tree)
}

fn snapshot(sources: &BTreeMap<String, String>) -> Result<Snapshot, &'static str> {
    let mut tables = Vec::new();
    let mut pairs = Vec::new();
    let mut importable: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (path, source) in sources.iter().filter(|(path, _)| !is_test_path(path)) {
        let tree = parse_python(source)?;
        let root = tree.root_node();
        let possible = nodes(root)?
            .iter()
            .filter(|node| dictionary_get(**node, source).is_some())
            .count();
        let passive = straightline_module(root, source, false);
        if possible > 0 && !passive {
            return Err("production lookup module has unsupported binding syntax");
        }
        let mut count = 0;
        let bindings = binding_counts(root, source);
        for function in children(root)
            .into_iter()
            .filter(|node| node.kind() == "function_definition")
        {
            if passive {
                if let (Some(name), Some(parameters)) = (
                    function.child_by_field_name("name"),
                    function.child_by_field_name("parameters"),
                ) {
                    let name = text(name, source);
                    if children(parameters).len() == 1 && bindings.get(name) == Some(&1) {
                        importable
                            .entry(path.clone())
                            .or_default()
                            .insert(name.into());
                    }
                }
            }
            if let Some(table) = table(path, source, function)? {
                if bindings.get(&table.function) != Some(&1) {
                    return Err("production lookup function binding is not unique");
                }
                if tables.len() >= MAX_WITNESSES {
                    return Err("Python tables exceed the witness budget");
                }
                count += 1;
                tables.push(table);
            }
        }
        if count != possible {
            return Err("Python dictionary lookup is outside the bounded function form");
        }
    }
    for (path, source) in sources.iter().filter(|(path, _)| is_test_path(path)) {
        let tree = parse_python(source)?;
        let file_pairs = test_pairs(path, source, tree.root_node(), sources, &importable)?;
        if pairs.len() + file_pairs.len() > MAX_WITNESSES {
            return Err("Python test associations exceed the witness budget");
        }
        pairs.extend(file_pairs);
    }
    Ok(Snapshot {
        tables,
        pairs,
        files: sources.len(),
    })
}

fn table(
    path: &str,
    source: &str,
    function: tree_sitter::Node<'_>,
) -> Result<Option<Table>, &'static str> {
    let Some(body) = function.child_by_field_name("body") else {
        return Err("function body is unavailable");
    };
    if !nodes(body)?
        .iter()
        .any(|node| dictionary_get(*node, source).is_some())
    {
        return Ok(None);
    }
    let parameters = function
        .child_by_field_name("parameters")
        .ok_or("lookup parameters are unavailable")?;
    let parameters = children(parameters);
    if parameters.len() != 1 || parameters[0].kind() != "identifier" {
        return Err("lookup requires one literal identifier parameter");
    }
    let statements = children(body);
    if statements.len() != 1 || statements[0].kind() != "return_statement" {
        return Err("lookup function requires one direct return");
    }
    let values = children(statements[0]);
    if values.len() != 1 {
        return Err("lookup return is ambiguous");
    }
    let call = values[0];
    let dictionary =
        dictionary_get(call, source).ok_or("lookup return is outside the bounded form")?;
    let arguments = call
        .child_by_field_name("arguments")
        .map(children)
        .ok_or("lookup arguments are unavailable")?;
    if !matches!(arguments.len(), 1 | 2)
        || arguments[0].kind() != "identifier"
        || text(arguments[0], source) != text(parameters[0], source)
        || arguments.get(1).is_some_and(|node| {
            !matches!(node.kind(), "integer" | "none") && plain_string(*node, source).is_none()
        })
    {
        return Err("lookup arguments are outside the literal parameter form");
    }
    let mut signature = BTreeMap::new();
    for pair in children(dictionary) {
        if pair.kind() != "pair" {
            return Err("lookup dictionary has a dynamic entry");
        }
        let key = pair
            .child_by_field_name("key")
            .and_then(|node| plain_string(node, source))
            .ok_or("lookup key is not a plain string literal")?;
        let value = pair
            .child_by_field_name("value")
            .and_then(|node| integer(node, source))
            .ok_or("lookup output is not a bounded integer literal")?;
        if signature.len() >= MAX_SIGNATURE_ENTRIES {
            return Err("lookup dictionary exceeds the entry budget");
        }
        if signature.insert(key, value).is_some() {
            return Err("lookup dictionary has duplicate keys");
        }
    }
    if signature.is_empty() {
        return Err("lookup dictionary has no literal entries");
    }
    let name = function
        .child_by_field_name("name")
        .map(|node| text(node, source))
        .ok_or("lookup name is unavailable")?;
    let fallback = arguments.get(1).map(|node| text(*node, source).to_owned());
    let identity = (
        name.to_owned(),
        text(parameters[0], source).to_owned(),
        signature.clone(),
        fallback,
    );
    Ok(Some(Table {
        path: path.into(),
        function: name.into(),
        line: call.start_position().row as u64 + 1,
        signature,
        identity,
    }))
}

fn dictionary_get<'a>(node: tree_sitter::Node<'a>, source: &str) -> Option<tree_sitter::Node<'a>> {
    if node.kind() != "call" {
        return None;
    }
    let function = node.child_by_field_name("function")?;
    if function.kind() != "attribute"
        || text(function.child_by_field_name("attribute")?, source) != "get"
    {
        return None;
    }
    let object = function.child_by_field_name("object")?;
    (object.kind() == "dictionary").then_some(object)
}

fn test_pairs(
    path: &str,
    source: &str,
    root: tree_sitter::Node<'_>,
    sources: &BTreeMap<String, String>,
    importable: &BTreeMap<String, BTreeSet<String>>,
) -> Result<Vec<Pair>, &'static str> {
    if !straightline_module(root, source, true) {
        return Ok(Vec::new());
    }
    let root_imports = children(root)
        .into_iter()
        .filter(|node| node.kind() == "import_from_statement")
        .collect::<Vec<_>>();
    if root_imports.len() != 1 {
        return Ok(Vec::new());
    }
    let bindings = binding_counts(root, source);
    let mut imports = BTreeMap::new();
    for import in children(root)
        .into_iter()
        .filter(|node| node.kind() == "import_from_statement")
    {
        let Some(module) = import.child_by_field_name("module_name") else {
            continue;
        };
        if module.kind() != "dotted_name" {
            continue;
        }
        let module_name = text(module, source);
        if !module_name.split('.').all(identifier) {
            continue;
        }
        let target = format!("{}.py", module_name.replace('.', "/"));
        let package = format!("{}/__init__.py", module_name.replace('.', "/"));
        let parts = module_name.split('.').collect::<Vec<_>>();
        let has_initializer = (1..parts.len()).any(|count| {
            sources.contains_key(&format!("{}/__init__.py", parts[..count].join("/")))
        });
        if !sources.contains_key(&target)
            || sources.contains_key(&package)
            || is_test_path(&target)
            || has_initializer
        {
            continue;
        }
        let named_imports = (0..import.child_count())
            .filter(|index| import.field_name_for_child(*index as u32) == Some("name"))
            .count();
        if named_imports != 1 {
            continue;
        }
        for index in 0..import.child_count() {
            if import.field_name_for_child(index as u32) != Some("name") {
                continue;
            }
            let Some(name) = import.child(index) else {
                continue;
            };
            let name_text = text(name, source);
            if name.kind() == "dotted_name"
                && identifier(name_text)
                && bindings.get(name_text) == Some(&1)
                && importable
                    .get(&target)
                    .is_some_and(|functions| functions.contains(name_text))
            {
                imports.insert(name_text.to_owned(), target.clone());
            }
        }
    }
    let mut pairs = Vec::new();
    'functions: for function in children(root)
        .into_iter()
        .filter(|node| node.kind() == "function_definition")
    {
        let Some(name) = function.child_by_field_name("name") else {
            continue;
        };
        if !text(name, source).starts_with("test_") {
            continue;
        }
        let Some(parameters) = function.child_by_field_name("parameters") else {
            continue;
        };
        if !children(parameters).is_empty() {
            continue;
        }
        let Some(body) = function.child_by_field_name("body") else {
            continue;
        };
        let statements = children(body);
        if statements
            .iter()
            .any(|node| node.kind() != "assert_statement")
        {
            continue;
        }
        let mut function_pairs = Vec::new();
        for statement in statements {
            let expressions = children(statement);
            if expressions.len() != 1 || expressions[0].kind() != "comparison_operator" {
                continue 'functions;
            }
            let comparison = expressions[0];
            let operators = (0..comparison.child_count())
                .filter_map(|index| {
                    (comparison.field_name_for_child(index as u32) == Some("operators"))
                        .then(|| comparison.child(index))
                        .flatten()
                })
                .collect::<Vec<_>>();
            let operands = children(comparison);
            if operators.len() != 1 || text(operators[0], source) != "==" || operands.len() != 2 {
                continue 'functions;
            }
            let call = operands[0];
            if call.kind() != "call" {
                continue 'functions;
            }
            let Some(callee) = call.child_by_field_name("function") else {
                continue 'functions;
            };
            if callee.kind() != "identifier" {
                continue 'functions;
            }
            let callee = text(callee, source);
            let Some(module) = imports.get(callee) else {
                continue 'functions;
            };
            let Some(arguments) = call.child_by_field_name("arguments") else {
                continue 'functions;
            };
            let arguments = children(arguments);
            if arguments.len() != 1 {
                continue 'functions;
            }
            let Some(input) = plain_string(arguments[0], source) else {
                continue 'functions;
            };
            let Some(expected) = integer(operands[1], source) else {
                continue 'functions;
            };
            if pairs.len() + function_pairs.len() >= MAX_WITNESSES {
                return Err("Python test associations exceed the witness budget");
            }
            function_pairs.push(Pair {
                key: (module.clone(), callee.into(), input, expected),
                path: path.into(),
                line: statement.start_position().row as u64 + 1,
            });
        }
        pairs.extend(function_pairs);
    }
    Ok(pairs)
}

fn straightline_module(root: tree_sitter::Node<'_>, source: &str, test: bool) -> bool {
    children(root).into_iter().all(|node| match node.kind() {
        "function_definition" => {
            !text(node, source).starts_with("async ")
                && node.child_by_field_name("return_type").is_none()
                && node.child_by_field_name("type_parameters").is_none()
                && node
                    .child_by_field_name("parameters")
                    .is_some_and(|parameters| {
                        children(parameters)
                            .iter()
                            .all(|parameter| parameter.kind() == "identifier")
                    })
        }
        "import_from_statement" => {
            test && !children(node)
                .iter()
                .any(|child| matches!(child.kind(), "wildcard_import" | "aliased_import"))
        }
        "import_statement" => false,
        "expression_statement" => {
            let values = children(node);
            values.len() == 1
                && (plain_string(values[0], source).is_some()
                    || (!test
                        && values[0].kind() == "assignment"
                        && values[0]
                            .child_by_field_name("left")
                            .is_some_and(|left| left.kind() == "identifier")
                        && values[0].child_by_field_name("right").is_some_and(|right| {
                            matches!(right.kind(), "integer" | "none")
                                || plain_string(right, source).is_some()
                        })))
        }
        _ => false,
    })
}

fn binding_counts(root: tree_sitter::Node<'_>, source: &str) -> BTreeMap<String, usize> {
    let mut bindings = BTreeMap::new();
    for node in children(root) {
        match node.kind() {
            "function_definition" | "class_definition" => {
                if let Some(name) = node.child_by_field_name("name") {
                    *bindings.entry(text(name, source).into()).or_default() += 1;
                }
            }
            "decorated_definition" => {
                if let Some(definition) = node.child_by_field_name("definition") {
                    if let Some(name) = definition.child_by_field_name("name") {
                        *bindings.entry(text(name, source).into()).or_default() += 1;
                    }
                }
            }
            "import_from_statement" | "import_statement" => {
                for index in 0..node.child_count() {
                    if node.field_name_for_child(index as u32) != Some("name") {
                        continue;
                    }
                    if let Some(name) = node.child(index) {
                        let binding = if name.kind() == "aliased_import" {
                            name.child_by_field_name("alias")
                                .map(|alias| text(alias, source))
                        } else {
                            Some(text(name, source).split('.').next().unwrap_or(""))
                        };
                        if let Some(binding) = binding {
                            *bindings.entry(binding.into()).or_default() += 1;
                        }
                    }
                }
            }
            "expression_statement" => {
                for expression in children(node) {
                    if matches!(expression.kind(), "assignment" | "augmented_assignment") {
                        if let Some(left) = expression.child_by_field_name("left") {
                            for binding in nodes(left)
                                .unwrap_or_default()
                                .into_iter()
                                .filter(|node| node.kind() == "identifier")
                            {
                                *bindings.entry(text(binding, source).into()).or_default() += 1;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    bindings
}

fn children(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|node| node.kind() != "comment")
        .collect()
}

fn nodes(root: tree_sitter::Node<'_>) -> Result<Vec<tree_sitter::Node<'_>>, &'static str> {
    let mut stack = vec![root];
    let mut result = Vec::new();
    while let Some(node) = stack.pop() {
        if result.len() >= MAX_NODES {
            return Err("Python syntax exceeds the node budget");
        }
        result.push(node);
        stack.extend(children(node));
    }
    Ok(result)
}

fn text<'a>(node: tree_sitter::Node<'_>, source: &'a str) -> &'a str {
    source.get(node.byte_range()).unwrap_or("")
}

fn plain_string(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    if node.kind() != "string" {
        return None;
    }
    let raw = text(node, source);
    let quote = *raw.as_bytes().first()?;
    if !matches!(quote, b'\'' | b'"') || raw.len() < 2 || raw.as_bytes().last() != Some(&quote) {
        return None;
    }
    let inner = &raw[1..raw.len() - 1];
    if inner.len() > MAX_LITERAL_BYTES || inner.contains(['\\', '\n', '\r', '\'', '"']) {
        return None;
    }
    Some(inner.into())
}

fn integer(node: tree_sitter::Node<'_>, source: &str) -> Option<String> {
    if node.kind() != "integer" {
        return None;
    }
    let raw = text(node, source);
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    raw.parse::<u64>().ok().map(|number| number.to_string())
}

fn identifier(value: &str) -> bool {
    value.bytes().enumerate().all(|(index, byte)| {
        byte.is_ascii_alphabetic() || byte == b'_' || (index > 0 && byte.is_ascii_digit())
    }) && !value.is_empty()
}

fn full_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn exact_commit(repo: &Path, value: &str) -> bool {
    if !full_oid(value) {
        return false;
    }
    git(
        repo,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{value}^{{commit}}"),
        ],
        128,
    )
    .and_then(|raw| String::from_utf8(raw).ok())
    .is_some_and(|actual| actual.trim() == value)
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH_BYTES
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn excluded(path: &str) -> bool {
    path.split('/').any(|part| {
        matches!(
            part,
            "dist"
                | "build"
                | "out"
                | "target"
                | "node_modules"
                | "vendor"
                | ".next"
                | ".vite"
                | "coverage"
        )
    }) || path.contains(".min.")
}

fn is_test_path(path: &str) -> bool {
    path.split('/').any(|part| {
        matches!(
            part,
            "test" | "tests" | "__tests__" | "spec" | "specs" | "fixtures"
        )
    }) || path
        .rsplit('/')
        .next()
        .is_some_and(|name| name.starts_with("test_") || name.ends_with("_test.py"))
}

fn git(repo: &Path, args: &[&str], limit: usize) -> Option<Vec<u8>> {
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
    let raw = match receive.recv_timeout(GIT_TIMEOUT) {
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
            Ok(None) if started.elapsed() < GIT_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(1))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}
