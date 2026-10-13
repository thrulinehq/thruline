//! Exact top-level node:test scopes. Unsupported binding/call shapes grant none.
use super::text_exclusions::{NotLookedAt, TestSpan};
use crate::redact::Masked;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;
use tree_sitter::Node;

fn text<'a>(node: Node<'_>, source: &'a str) -> &'a str {
    &source[node.start_byte()..node.end_byte()]
}
fn literal(node: Node<'_>, source: &str) -> Option<String> {
    if node.kind() != "string" {
        return None;
    }
    let raw = text(node, source);
    let quote = *raw.as_bytes().first()?;
    if !matches!(quote, b'\'' | b'"') || raw.as_bytes().last() != Some(&quote) {
        return None;
    }
    let value = raw.get(1..raw.len().checked_sub(1)?)?;
    (!value.contains('\\') && !value.chars().any(char::is_control)).then(|| value.to_owned())
}
fn tokens_contain(value: &str, alias: &str) -> bool {
    value
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
        .any(|p| p == alias)
}

pub(super) fn spans(
    source: &str,
    path: &str,
    name: &str,
    deadline: Instant,
) -> Result<Vec<TestSpan>, &'static str> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_javascript::LANGUAGE.into())
        .map_err(|_| "test exclusion parser is unavailable")?;
    let mut progress = |_: &tree_sitter::ParseState| Instant::now() >= deadline;
    let mut input =
        |offset: usize, _: tree_sitter::Point| source.as_bytes().get(offset..).unwrap_or_default();
    let tree = parser
        .parse_with_options(
            &mut input,
            None,
            Some(tree_sitter::ParseOptions::new().progress_callback(&mut progress)),
        )
        .ok_or("test exclusion source could not be parsed")?;
    let root = tree.root_node();
    if root.has_error() {
        return Err("test exclusion source has malformed JavaScript syntax");
    }
    let mut pending = vec![(root, 0usize)];
    let mut nodes = Vec::new();
    while let Some((node, depth)) = pending.pop() {
        if nodes.len() >= 100_000 || depth > 128 || Instant::now() >= deadline {
            return Err("test exclusion source exceeds its parse bound");
        }
        nodes.push(node);
        for i in (0..node.named_child_count()).rev() {
            if let Some(child) = node.named_child(i) {
                pending.push((child, depth + 1));
            }
        }
    }
    let mut aliases = BTreeSet::new();
    for statement in nodes
        .iter()
        .filter(|n| n.kind() == "import_statement" && n.parent().is_some_and(|p| p == root))
    {
        if statement
            .child_by_field_name("source")
            .and_then(|n| literal(n, source))
            .as_deref()
            != Some("node:test")
        {
            continue;
        }
        let Some(clause) = (0..statement.named_child_count())
            .filter_map(|i| statement.named_child(i))
            .find(|n| n.kind() == "import_clause")
        else {
            continue;
        };
        for i in 0..clause.named_child_count() {
            let Some(child) = clause.named_child(i) else {
                continue;
            };
            if child.kind() == "identifier" {
                aliases.insert(text(child, source).to_owned());
            }
            if child.kind() == "named_imports" {
                for j in 0..child.named_child_count() {
                    let Some(specifier) = child.named_child(j) else {
                        continue;
                    };
                    let Some(imported) = specifier.child_by_field_name("name") else {
                        continue;
                    };
                    if text(imported, source) == "test" {
                        aliases.insert(
                            text(
                                specifier.child_by_field_name("alias").unwrap_or(imported),
                                source,
                            )
                            .to_owned(),
                        );
                    }
                }
            }
        }
    }
    if aliases.is_empty() {
        return Err("accepted named test has no trusted node:test import");
    }
    let mut import_bindings = BTreeMap::<String, usize>::new();
    for node in &nodes {
        let binding = match node.kind() {
            "identifier"
                if node.parent().is_some_and(|p| {
                    p.kind() == "import_clause" || p.kind() == "namespace_import"
                }) =>
            {
                Some(*node)
            }
            "import_specifier" => node
                .child_by_field_name("alias")
                .or_else(|| node.child_by_field_name("name")),
            _ => None,
        };
        if let Some(binding) = binding {
            *import_bindings
                .entry(text(binding, source).into())
                .or_default() += 1;
        }
    }
    if aliases
        .iter()
        .any(|alias| import_bindings.get(alias) != Some(&1))
    {
        return Err("test import has a competing import binding");
    }
    // Conservatively refuse any competing declaration or parameter binding.
    // Exact top-level callbacks need no inference about JavaScript resolution.
    for node in &nodes {
        let binding = match node.kind() {
            "variable_declarator"
            | "function_declaration"
            | "function_expression"
            | "generator_function_declaration"
            | "generator_function"
            | "class_declaration" => node.child_by_field_name("name"),
            "assignment_expression" | "augmented_assignment_expression" => {
                node.child_by_field_name("left")
            }
            "update_expression" => node.child_by_field_name("argument"),
            "arrow_function" => node.child_by_field_name("parameter"),
            "formal_parameters" => Some(*node),
            _ => None,
        };
        if binding.is_some_and(|b| {
            aliases
                .iter()
                .any(|alias| tokens_contain(text(b, source), alias))
        }) {
            return Err("test import has a competing or shadowed binding");
        }
    }
    let mut selected = Vec::new();
    for call in nodes.iter().filter(|n| n.kind() == "call_expression") {
        let Some(callee) = call.child_by_field_name("function") else {
            continue;
        };
        if callee.kind() != "identifier" || !aliases.contains(text(callee, source)) {
            continue;
        }
        let Some(statement) = call.parent() else {
            continue;
        };
        if statement.kind() != "expression_statement"
            || !statement.parent().is_some_and(|p| p == root)
        {
            continue;
        }
        let Some(arguments) = call.child_by_field_name("arguments") else {
            continue;
        };
        let args: Vec<_> = (0..arguments.named_child_count())
            .filter_map(|i| arguments.named_child(i))
            .filter(|n| n.kind() != "comment")
            .collect();
        if args.len() != 2 || !matches!(args[1].kind(), "arrow_function" | "function_expression") {
            continue;
        }
        if args.first().and_then(|n| literal(*n, source)).as_deref() == Some(name) {
            selected.push(*call);
        }
    }
    if selected.len() != 1 {
        return Err("accepted named test must identify one direct literal node:test callback");
    }
    let node = selected[0];
    let start = node.start_position();
    let end = node.end_position();
    let excluded_lines = source
        .lines()
        .enumerate()
        .filter_map(|(row, line)| {
            if row < start.row
                || row > end.row
                || (row == start.row && !line.get(..start.column)?.trim().is_empty())
                || (row == end.row && !line.get(end.column..)?.trim_matches(';').trim().is_empty())
            {
                None
            } else {
                Some(row as u64 + 1)
            }
        })
        .collect();
    Ok(vec![TestSpan {
        path: path.to_owned(),
        excluded_lines,
        receipt: NotLookedAt {
            path: Masked::mask(path).into_text(),
            source: "accepted_test_scope".into(),
            pattern: name.into(),
            test_name: Some(name.into()),
            start_line: Some(start.row as u64 + 1),
            end_line: Some(end.row as u64 + 1),
            start_column: Some(start.column),
            end_column: Some(end.column),
        },
    }])
}
