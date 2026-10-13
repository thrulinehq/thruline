//! Closed, versioned AST declarations. Coordinates select syntax, never a
//! computed title or filename. Accepted intent remains person-reviewed data.
use super::SpanScope;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;
use tree_sitter::Node;

pub(super) fn validate(
    source: &str,
    scope: &SpanScope,
    deadline: Instant,
) -> Result<(), &'static str> {
    if scope.path.ends_with(".rs") {
        rust(source, scope, deadline)
    } else {
        javascript(source, scope, deadline)
    }
}
fn exact(node: Node<'_>, scope: &SpanScope) -> bool {
    let start = node.start_position();
    let end = node.end_position();
    (
        start.row as u64 + 1,
        start.column,
        end.row as u64 + 1,
        end.column,
    ) == (
        scope.start_line,
        scope.start_column,
        scope.end_line,
        scope.end_column,
    )
}
fn rust(source: &str, scope: &SpanScope, deadline: Instant) -> Result<(), &'static str> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .map_err(|_| "test declaration parser is unavailable")?;
    let mut progress = |_: &tree_sitter::ParseState| Instant::now() >= deadline;
    let mut input =
        |offset: usize, _: tree_sitter::Point| source.as_bytes().get(offset..).unwrap_or_default();
    let tree = parser
        .parse_with_options(
            &mut input,
            None,
            Some(tree_sitter::ParseOptions::new().progress_callback(&mut progress)),
        )
        .ok_or("test declaration source could not be parsed")?;
    if tree.root_node().has_error() {
        return Err("test declaration source has malformed Rust syntax");
    }
    let mut pending = vec![(tree.root_node(), 0usize)];
    let mut nodes = 0usize;
    while let Some((node, depth)) = pending.pop() {
        nodes += 1;
        if nodes > 100_000 || depth > 128 || Instant::now() >= deadline {
            return Err("test declaration source exceeds its parse bound");
        }
        if node.kind() == "function_item" && exact(node, scope) && module_item(node) {
            let mut previous = node.prev_named_sibling();
            while let Some(attr) = previous {
                match attr.kind() {
                    "attribute_item" => {
                        let compact: String = text(attr, source)
                            .chars()
                            .filter(|c| !c.is_whitespace())
                            .collect();
                        if compact == "#[test]" {
                            return Ok(());
                        }
                    }
                    "line_comment" | "block_comment" => {}
                    _ => break,
                }
                previous = attr.prev_named_sibling();
            }
        }
        for i in (0..node.named_child_count()).rev() {
            if let Some(child) = node.named_child(i) {
                pending.push((child, depth + 1));
            }
        }
    }
    Err("exact declaration is not one complete Rust test function")
}
fn module_item(mut node: Node<'_>) -> bool {
    while let Some(parent) = node.parent() {
        match parent.kind() {
            "source_file" => return true,
            "declaration_list" => {
                let Some(module) = parent.parent() else {
                    return false;
                };
                if module.kind() != "mod_item" {
                    return false;
                }
                node = module;
            }
            _ => return false,
        }
    }
    false
}
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

fn javascript(source: &str, scope: &SpanScope, deadline: Instant) -> Result<(), &'static str> {
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
            "for_in_statement" => node.child_by_field_name("left"),
            "catch_clause" => node.child_by_field_name("parameter"),
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
    for call in nodes
        .iter()
        .filter(|n| n.kind() == "call_expression" && exact(**n, scope))
    {
        let Some(callee) = call.child_by_field_name("function") else {
            continue;
        };
        if callee.kind() != "identifier" || !aliases.contains(text(callee, source)) {
            continue;
        }
        let Some(statement) = call.parent() else {
            continue;
        };
        if statement.kind() != "expression_statement" {
            continue;
        }
        let bindings = if statement.parent().is_some_and(|p| p == root) {
            BTreeSet::new()
        } else {
            let Some(bindings) = loop_bindings(statement, root, &nodes, source) else {
                continue;
            };
            bindings
        };
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
        if title(args[0], source, &bindings) {
            return Ok(());
        }
    }
    Err("exact declaration is not one supported direct node:test callback")
}
fn loop_bindings(
    statement: Node<'_>,
    root: Node<'_>,
    nodes: &[Node<'_>],
    source: &str,
) -> Option<BTreeSet<String>> {
    let body = statement.parent()?;
    if body.kind() != "statement_block" {
        return None;
    }
    let loop_node = body.parent()?;
    if loop_node.kind() != "for_in_statement"
        || loop_node.parent()? != root
        || loop_node.child_by_field_name("body")? != body
        || text(loop_node.child_by_field_name("operator")?, source) != "of"
        || text(loop_node.child_by_field_name("kind")?, source) != "const"
    {
        return None;
    }
    let inventory = loop_node.child_by_field_name("right")?;
    if inventory.kind() != "identifier" {
        return None;
    }
    let definitions: Vec<_> = nodes
        .iter()
        .filter(|n| n.kind() == "variable_declarator")
        .filter(|n| {
            n.child_by_field_name("name")
                .is_some_and(|name| text(name, source) == text(inventory, source))
        })
        .collect();
    if definitions.len() != 1 {
        return None;
    }
    let definition = *definitions[0];
    let declaration = definition.parent()?;
    if declaration.kind() != "lexical_declaration"
        || declaration.parent()? != root
        || !text(declaration, source).starts_with("const ")
        || definition.child_by_field_name("value")?.kind() != "array"
        || definition.start_byte() >= loop_node.start_byte()
    {
        return None;
    }
    let left = loop_node.child_by_field_name("left")?;
    let mut pending = vec![left];
    let mut bindings = BTreeSet::new();
    while let Some(node) = pending.pop() {
        match node.kind() {
            "identifier" => {
                bindings.insert(text(node, source).to_owned());
            }
            "array_pattern" => {
                for i in 0..node.named_child_count() {
                    pending.push(node.named_child(i)?);
                }
            }
            "comment" => {}
            _ => return None,
        }
    }
    (!bindings.is_empty()).then_some(bindings)
}
fn title(node: Node<'_>, source: &str, bindings: &BTreeSet<String>) -> bool {
    match node.kind() {
        "string" => literal(node, source).is_some(),
        "identifier" => bindings.contains(text(node, source)),
        "binary_expression" => {
            node.child_by_field_name("operator")
                .is_some_and(|n| text(n, source) == "+")
                && node
                    .child_by_field_name("left")
                    .is_some_and(|n| title(n, source, bindings))
                && node
                    .child_by_field_name("right")
                    .is_some_and(|n| title(n, source, bindings))
        }
        _ => false,
    }
}
