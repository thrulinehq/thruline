//! Explicit accepted paths and exact-tree named Rust/node:test scopes.
//! A source label or a matching filename cannot grant an exclusion.
use crate::error::{CoreError, Result};
use crate::redact::Masked;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestScope {
    pub path: String,
    pub name_pattern: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NotLookedAt {
    pub path: String,
    pub source: String,
    pub pattern: String,
    pub test_name: Option<String>,
    pub start_line: Option<u64>,
    pub end_line: Option<u64>,
    pub start_column: Option<usize>,
    pub end_column: Option<usize>,
}
impl NotLookedAt {
    pub(crate) fn path(path: &str, source: &str, pattern: &str) -> Self {
        Self {
            path: Masked::mask(path).into_text(),
            source: source.into(),
            pattern: Masked::mask(pattern).into_text(),
            test_name: None,
            start_line: None,
            end_line: None,
            start_column: None,
            end_column: None,
        }
    }
}
pub(crate) struct TestSpan {
    pub path: String,
    pub receipt: NotLookedAt,
    pub excluded_lines: Vec<u64>,
}
impl TestSpan {
    pub(crate) fn contains(&self, path: &str, line: u64, start: usize, end: usize) -> bool {
        let lo = self.receipt.start_line.unwrap_or(0);
        let hi = self.receipt.end_line.unwrap_or(0);
        path == self.path
            && line >= lo
            && line <= hi
            && (line != lo || start >= self.receipt.start_column.unwrap_or(0))
            && (line != hi || end <= self.receipt.end_column.unwrap_or(0))
    }
}
pub(crate) fn validate_scopes(scopes: &[TestScope]) -> Result<()> {
    let invalid = || {
        CoreError::Config("test exclusions need exact source paths and bounded test names".into())
    };
    if scopes.len() > 128 {
        return Err(invalid());
    }
    for scope in scopes {
        let path = &scope.path;
        let prefix = scope
            .name_pattern
            .strip_suffix('*')
            .unwrap_or(&scope.name_pattern);
        let rust = path.ends_with(".rs");
        let javascript = [".js", ".jsx", ".mjs", ".cjs"]
            .iter()
            .any(|ext| path.ends_with(ext));
        let name_valid = if rust {
            !prefix.is_empty()
                && prefix.len() <= 128
                && prefix.bytes().enumerate().all(|(i, b)| {
                    b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit())
                })
        } else {
            !scope.name_pattern.is_empty()
                && scope.name_pattern.len() <= 512
                && !scope.name_pattern.contains('*')
                && !scope.name_pattern.chars().any(char::is_control)
        };
        if path.len() > 1024
            || (!rust && !javascript)
            || path.starts_with(['/', '~'])
            || path.contains(['\\', '*', '?', '[', ':'])
            || path.chars().any(char::is_control)
            || path
                .split('/')
                .any(|p| p.is_empty() || matches!(p, "." | ".."))
            || !name_valid
        {
            return Err(invalid());
        }
    }
    Ok(())
}

pub(crate) fn test_spans(
    repo: &Path,
    commit: &str,
    scopes: &[TestScope],
) -> std::result::Result<Vec<TestSpan>, &'static str> {
    validate_scopes(scopes).map_err(|_| "test exclusion parameters are invalid")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut spans = Vec::new();
    let mut sources = std::collections::BTreeMap::<String, String>::new();
    for scope in scopes {
        if !sources.contains_key(&scope.path) {
            let entry = git(
                repo,
                &["ls-tree", "-z", commit, "--", &scope.path],
                deadline,
            )?;
            let entries = entry
                .split(|b| *b == 0)
                .filter(|row| !row.is_empty())
                .collect::<Vec<_>>();
            if entries.len() != 1
                || !entries[0].starts_with(b"100644 blob ")
                    && !entries[0].starts_with(b"100755 blob ")
            {
                return Err("test exclusion source is not a regular Git file");
            }
            let tab = entries[0]
                .iter()
                .position(|b| *b == b'\t')
                .ok_or("test exclusion source identity is invalid")?;
            if &entries[0][tab + 1..] != scope.path.as_bytes() {
                return Err("test exclusion source path differs");
            }
            let object = format!("{commit}:{}", scope.path);
            let size = git(repo, &["cat-file", "-s", &object], deadline)?;
            let size = std::str::from_utf8(&size)
                .ok()
                .and_then(|s| s.trim().parse::<usize>().ok())
                .ok_or("test exclusion source size is unavailable")?;
            if size > 2 * 1024 * 1024 {
                return Err("test exclusion source exceeds its byte limit");
            }
            let source = git(repo, &["cat-file", "blob", &object], deadline)?;
            if source.len() != size {
                return Err("test exclusion source identity differs");
            }
            let source =
                String::from_utf8(source).map_err(|_| "test exclusion source is not UTF-8")?;
            sources.insert(scope.path.clone(), source);
        }
        let source = sources
            .get(&scope.path)
            .ok_or("test exclusion source is unavailable")?;
        if !scope.path.ends_with(".rs") {
            spans.extend(super::text_exclusions_js::spans(
                source,
                &scope.path,
                &scope.name_pattern,
                deadline,
            )?);
            continue;
        }
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .map_err(|_| "test exclusion parser is unavailable")?;
        let mut progress = |_: &tree_sitter::ParseState| Instant::now() >= deadline;
        let mut input = |offset: usize, _: tree_sitter::Point| {
            source.as_bytes().get(offset..).unwrap_or_default()
        };
        let tree = parser
            .parse_with_options(
                &mut input,
                None,
                Some(tree_sitter::ParseOptions::new().progress_callback(&mut progress)),
            )
            .ok_or("test exclusion source could not be parsed")?;
        if tree.root_node().has_error() {
            return Err("test exclusion source has malformed Rust syntax");
        }
        let mut pending = vec![(tree.root_node(), 0usize)];
        let mut nodes = 0usize;
        let mut matched = false;
        while let Some((node, depth)) = pending.pop() {
            nodes += 1;
            if nodes > 100_000 || depth > 128 || Instant::now() >= deadline {
                return Err("test exclusion source exceeds its parse bound");
            }
            if node.kind() == "function_item" {
                if let Some(name) = node.child_by_field_name("name") {
                    let name = &source[name.start_byte()..name.end_byte()];
                    let matches = scope
                        .name_pattern
                        .strip_suffix('*')
                        .map_or(name == scope.name_pattern, |p| name.starts_with(p));
                    if matches {
                        let mut prev = node.prev_named_sibling();
                        let mut is_test = false;
                        while let Some(attr) = prev {
                            match attr.kind() {
                                "attribute_item" => {
                                    let compact = source[attr.start_byte()..attr.end_byte()]
                                        .chars()
                                        .filter(|c| !c.is_whitespace())
                                        .collect::<String>();
                                    is_test |= compact == "#[test]";
                                }
                                "line_comment" | "block_comment" => {}
                                _ => break,
                            }
                            prev = attr.prev_named_sibling();
                        }
                        if is_test {
                            matched = true;
                            let start = node.start_position();
                            let end = node.end_position();
                            let excluded_lines = source
                                .lines()
                                .enumerate()
                                .filter_map(|(row, text)| {
                                    if row < start.row
                                        || row > end.row
                                        || (row == start.row
                                            && !text.get(..start.column)?.trim().is_empty())
                                        || (row == end.row
                                            && !text.get(end.column..)?.trim().is_empty())
                                    {
                                        None
                                    } else {
                                        Some(row as u64 + 1)
                                    }
                                })
                                .collect();
                            spans.push(TestSpan {
                                path: scope.path.clone(),
                                excluded_lines,
                                receipt: NotLookedAt {
                                    path: Masked::mask(&scope.path).into_text(),
                                    source: "accepted_test_scope".into(),
                                    pattern: scope.name_pattern.clone(),
                                    test_name: Some(name.into()),
                                    start_line: Some(start.row as u64 + 1),
                                    end_line: Some(end.row as u64 + 1),
                                    start_column: Some(start.column),
                                    end_column: Some(end.column),
                                },
                            });
                        }
                    }
                }
            }
            for index in (0..node.named_child_count()).rev() {
                if let Some(child) = node.named_child(index) {
                    pending.push((child, depth + 1));
                }
            }
        }
        if !matched {
            return Err("accepted named test exclusion has no parsed test on this version");
        }
    }
    spans.sort_by(|a, b| a.receipt.cmp(&b.receipt));
    spans.dedup_by(|a, b| a.receipt == b.receipt);
    Ok(spans)
}
fn git(
    repo: &Path,
    args: &[&str],
    deadline: Instant,
) -> std::result::Result<Vec<u8>, &'static str> {
    let mut cmd = Command::new("git");
    crate::git_isolation::isolate_config(&mut cmd)
        .args(args)
        .current_dir(repo);
    match crate::bounded_child::output_until(&mut cmd, deadline) {
        Ok(crate::bounded_child::Outcome::Finished(output)) if output.status.success() => {
            Ok(output.stdout)
        }
        _ => Err("test exclusion exact source is unavailable"),
    }
}

/// Plain summary names every distinct path pattern or exact test scope.
/// The technical receipt retains each matched path and byte column.
pub fn summary(rows: &[NotLookedAt]) -> Option<String> {
    let entries = rows
        .iter()
        .map(|row| match &row.test_name {
            Some(name) => format!("{}::{name}", row.path),
            None => format!(
                "{} ({})",
                row.pattern,
                if row.source == "recorded_ignore" {
                    "recorded path exclusion"
                } else {
                    "accepted path exclusion"
                }
            ),
        })
        .collect::<std::collections::BTreeSet<_>>();
    (!entries.is_empty()).then(|| {
        format!(
            "Not looked at: {}.",
            entries.into_iter().collect::<Vec<_>>().join("; ")
        )
    })
}
