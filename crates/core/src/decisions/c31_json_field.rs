//! C31 exact Git JSON key exclusions. A scope covers one term inside one key,
//! never the containing file, sibling key, or value.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JsonFieldScope {
    pub path: String,
    pub pointer: String,
    pub term: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonKeySpan {
    pub path: String,
    pub pointer: String,
    pub term: String,
    pub line: u64,
    pub start_column: usize,
    pub end_column: usize,
}

impl JsonKeySpan {
    pub fn matches(&self, path: &str, line: u64, start: usize, end: usize, term: &str) -> bool {
        self.path == path
            && self.line == line
            && self.start_column == start
            && self.end_column == end
            && self.term.eq_ignore_ascii_case(term)
    }
}

pub fn json_field_spans_at(
    repo: &Path,
    commit: &str,
    scopes: &[JsonFieldScope],
) -> Result<Vec<JsonKeySpan>, &'static str> {
    validate_scopes(scopes)?;
    if scopes.is_empty() {
        return Ok(Vec::new());
    }
    if !git_oid(commit) {
        return Err("JSON field exclusion needs an exact commit");
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut spans = Vec::with_capacity(scopes.len());
    for scope in scopes {
        let blob = git_json_blob(repo, commit, &scope.path, deadline)?;
        // The semantic parse rejects malformed JSON. The lexical walk below
        // then locates the exact key bytes, including duplicate-key attacks
        // that a Value map would otherwise collapse.
        serde_json::from_slice::<serde_json::Value>(&blob)
            .map_err(|_| "JSON field exclusion source is malformed")?;
        let mut at = 0;
        let mut keys = Vec::new();
        scan_value(&blob, &mut at, "", &mut keys, 0)?;
        let selected = keys
            .into_iter()
            .filter(|key| key.pointer == scope.pointer)
            .collect::<Vec<_>>();
        if selected.len() != 1 {
            return Err("JSON field pointer does not name one unique key");
        }
        let key = &selected[0];
        let raw = &blob[key.start + 1..key.end - 1];
        if raw != key.name.as_bytes() {
            return Err("JSON field key uses an escaped spelling");
        }
        let positions = raw
            .windows(scope.term.len())
            .enumerate()
            .filter_map(|(at, bytes)| {
                bytes
                    .eq_ignore_ascii_case(scope.term.as_bytes())
                    .then_some(at)
            })
            .collect::<Vec<_>>();
        if positions.len() != 1 {
            return Err("JSON field key does not contain one exact searched term");
        }
        let absolute = key.start + 1 + positions[0];
        let preceding = &blob[..absolute];
        let line = preceding.iter().filter(|b| **b == b'\n').count() as u64 + 1;
        let line_start = preceding
            .iter()
            .rposition(|b| *b == b'\n')
            .map_or(0, |pos| pos + 1);
        let start_column = absolute - line_start;
        spans.push(JsonKeySpan {
            path: scope.path.clone(),
            pointer: scope.pointer.clone(),
            term: scope.term.clone(),
            line,
            start_column,
            end_column: start_column + scope.term.len(),
        });
    }
    Ok(spans)
}

fn git_oid(value: &str) -> bool {
    (value.len() == 40 || value.len() == 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Validate the accepted selector document before it can affect a census.
pub fn validate_scopes(scopes: &[JsonFieldScope]) -> Result<(), &'static str> {
    if scopes.len() > 32 {
        return Err("JSON field exclusion has too many key selectors");
    }
    let mut unique = BTreeSet::new();
    for scope in scopes {
        validate_scope(scope)?;
        if !unique.insert((&scope.path, &scope.pointer, &scope.term)) {
            return Err("JSON field exclusion repeats the same key and term");
        }
    }
    Ok(())
}

fn validate_scope(scope: &JsonFieldScope) -> Result<(), &'static str> {
    let path = &scope.path;
    if path.is_empty()
        || path.len() > 1024
        || !path.ends_with(".json")
        || path.starts_with(['/', '~'])
        || path.contains(['\\', ':', '*', '?', '['])
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err("JSON field exclusion needs one safe Git JSON path");
    }
    if scope.pointer.len() > 1024
        || !scope.pointer.starts_with('/')
        || scope.pointer.contains("//")
        || scope.pointer.chars().any(char::is_control)
        || scope
            .pointer
            .split('~')
            .skip(1)
            .any(|part| !part.starts_with(['0', '1']))
    {
        return Err("JSON field exclusion needs one valid JSON pointer");
    }
    if scope.term.is_empty()
        || scope.term.len() > 64
        || !scope.term.bytes().all(|b| b.is_ascii_alphabetic())
    {
        return Err("JSON field exclusion needs one ASCII word term");
    }
    Ok(())
}

fn git_json_blob(
    repo: &Path,
    commit: &str,
    path: &str,
    deadline: Instant,
) -> Result<Vec<u8>, &'static str> {
    let listing = git(repo, &["ls-tree", "-z", commit, "--", path], deadline)?;
    let row = listing.strip_suffix(&[0]).unwrap_or(&listing);
    let tab = row
        .iter()
        .position(|b| *b == b'\t')
        .ok_or("JSON field Git entry is unavailable")?;
    if &row[tab + 1..] != path.as_bytes()
        || !(row.starts_with(b"100644 blob ") || row.starts_with(b"100755 blob "))
        || row[..tab].contains(&0)
    {
        return Err("JSON field source is not the named regular Git blob");
    }
    let object = format!("{commit}:{path}");
    let size = git(repo, &["cat-file", "-s", &object], deadline)?;
    let size = std::str::from_utf8(&size)
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
        .ok_or("JSON field source size is unavailable")?;
    if size > 2 * 1024 * 1024 {
        return Err("JSON field source exceeds the 2 MiB bound");
    }
    let bytes = git(repo, &["cat-file", "blob", &object], deadline)?;
    if bytes.len() != size {
        return Err("JSON field source length differs from its Git object");
    }
    Ok(bytes)
}

fn git(repo: &Path, args: &[&str], deadline: Instant) -> Result<Vec<u8>, &'static str> {
    let mut command = Command::new("git");
    crate::git_isolation::isolate_config(&mut command)
        .args(args)
        .current_dir(repo);
    match crate::bounded_child::output_until(&mut command, deadline) {
        Ok(crate::bounded_child::Outcome::Finished(out)) if out.status.success() => Ok(out.stdout),
        _ => Err("JSON field exact Git source is unavailable"),
    }
}

struct KeyToken {
    pointer: String,
    name: String,
    start: usize,
    end: usize,
}

fn scan_value(
    bytes: &[u8],
    at: &mut usize,
    path: &str,
    keys: &mut Vec<KeyToken>,
    depth: usize,
) -> Result<(), &'static str> {
    if depth > 128 {
        return Err("JSON field source exceeds the nesting bound");
    }
    whitespace(bytes, at);
    match bytes.get(*at).copied() {
        Some(b'{') => {
            *at += 1;
            whitespace(bytes, at);
            while bytes.get(*at) != Some(&b'}') {
                let (name, start, end) = scan_string(bytes, at)?;
                let escaped = name.replace('~', "~0").replace('/', "~1");
                let pointer = format!("{path}/{escaped}");
                keys.push(KeyToken {
                    pointer: pointer.clone(),
                    name,
                    start,
                    end,
                });
                whitespace(bytes, at);
                if bytes.get(*at) != Some(&b':') {
                    return Err("JSON field source cannot locate key boundary");
                }
                *at += 1;
                scan_value(bytes, at, &pointer, keys, depth + 1)?;
                whitespace(bytes, at);
                if bytes.get(*at) != Some(&b',') {
                    break;
                }
                *at += 1;
                whitespace(bytes, at);
            }
            if bytes.get(*at) != Some(&b'}') {
                return Err("JSON field source cannot locate object end");
            }
            *at += 1;
        }
        Some(b'[') => {
            *at += 1;
            whitespace(bytes, at);
            let mut index = 0;
            while bytes.get(*at) != Some(&b']') {
                scan_value(bytes, at, &format!("{path}/{index}"), keys, depth + 1)?;
                index += 1;
                whitespace(bytes, at);
                if bytes.get(*at) != Some(&b',') {
                    break;
                }
                *at += 1;
                whitespace(bytes, at);
            }
            if bytes.get(*at) != Some(&b']') {
                return Err("JSON field source cannot locate array end");
            }
            *at += 1;
        }
        Some(b'"') => {
            scan_string(bytes, at)?;
        }
        Some(_) => {
            while bytes
                .get(*at)
                .is_some_and(|b| !matches!(*b, b',' | b'}' | b']' | b' ' | b'\n' | b'\r' | b'\t'))
            {
                *at += 1;
            }
        }
        None => return Err("JSON field source ended before the selected key"),
    }
    Ok(())
}

fn scan_string(bytes: &[u8], at: &mut usize) -> Result<(String, usize, usize), &'static str> {
    if bytes.get(*at) != Some(&b'"') {
        return Err("JSON field source expected a string token");
    }
    let start = *at;
    *at += 1;
    while let Some(byte) = bytes.get(*at) {
        if *byte == b'\\' {
            *at += 2;
        } else if *byte == b'"' {
            *at += 1;
            let name = serde_json::from_slice(&bytes[start..*at])
                .map_err(|_| "JSON field string token is invalid")?;
            return Ok((name, start, *at));
        } else {
            *at += 1;
        }
    }
    Err("JSON field string token has no end")
}

fn whitespace(bytes: &[u8], at: &mut usize) {
    while bytes
        .get(*at)
        .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
    {
        *at += 1;
    }
}
