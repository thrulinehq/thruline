//! Exact-version term and symbol checks. Every read is from one Git tree;
//! working-tree edits cannot change a verdict about a named version.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use crate::redact::Masked;

use super::checks::{CheckOutcome, Finding, SurfaceResult, VerdictState};
use super::params::{
    ForbiddenWordsParams, Surface, SymbolAbsentParams, TermEverywhereParams, UiCopyAbsentParams,
};

#[derive(Debug)]
struct Candidate {
    path: String,
    line: u64,
    text: String,
}

struct Tree {
    hash: String,
    paths: Vec<String>,
}

fn git(repo: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    crate::git_isolation::isolate_config(&mut Command::new("git"))
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|error| format!("could not read the version tree: {error}"))
}

fn tree(repo: &Path, version: &str) -> Result<Tree, String> {
    if version.trim().is_empty() || version.chars().any(char::is_control) {
        return Err("version is missing".into());
    }
    let object = format!("{version}^{{tree}}");
    let resolved = git(
        repo,
        &["rev-parse", "--verify", "--end-of-options", &object],
    )?;
    if !resolved.status.success() {
        return Err(format!("version {version} has no readable tree"));
    }
    let hash = String::from_utf8(resolved.stdout)
        .map_err(|_| "version tree id is not text".to_string())?
        .trim()
        .to_string();
    if hash.len() != 40 && hash.len() != 64 {
        return Err("version tree id has an unexpected shape".into());
    }
    let listing = git(repo, &["ls-tree", "-r", "-z", "--name-only", &hash])?;
    if !listing.status.success() {
        return Err("version tree could not be listed".into());
    }
    let paths = listing
        .stdout
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect();
    Ok(Tree { hash, paths })
}

fn candidates(repo: &Path, tree: &Tree, needles: &[String]) -> Result<Vec<Candidate>, String> {
    if needles.is_empty()
        || needles
            .iter()
            .any(|term| term.trim().is_empty() || term.chars().any(char::is_control))
    {
        return Err("term list contains an empty or invalid term".into());
    }
    if needles.iter().any(|term| !term.is_ascii()) {
        return Err("case folding for this term is not supported".into());
    }
    let mut args = vec!["grep", "-n", "-z", "-I", "-F", "-i"];
    for needle in needles {
        args.push("-e");
        args.push(needle);
    }
    args.push(&tree.hash);
    args.push("--");
    let output = git(repo, &args)?;
    if output.status.code() == Some(1) {
        return Ok(Vec::new());
    }
    if !output.status.success() {
        return Err("the version tree could not be searched".into());
    }
    let mut found = Vec::new();
    let bytes = &output.stdout;
    let mut at = 0usize;
    while at < bytes.len() {
        let path_end = bytes[at..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|position| at + position)
            .ok_or("search returned an incomplete path")?;
        let path = &bytes[at..path_end];
        at = path_end + 1;
        let line_end = bytes[at..]
            .iter()
            .position(|byte| *byte == 0)
            .map(|position| at + position)
            .ok_or("search returned an incomplete line number")?;
        let line = std::str::from_utf8(&bytes[at..line_end])
            .map_err(|_| "search line number is not text")?
            .parse::<u64>()
            .map_err(|_| "search line number is invalid")?;
        at = line_end + 1;
        let text_end = bytes[at..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|position| at + position)
            .ok_or("search returned an incomplete line")?;
        let text = String::from_utf8_lossy(&bytes[at..text_end])
            .trim_end_matches('\r')
            .to_string();
        at = text_end + 1;
        let prefix = format!("{}:", tree.hash);
        if !path.starts_with(prefix.as_bytes()) {
            return Err("search result came from another version".into());
        }
        found.push(Candidate {
            path: String::from_utf8_lossy(&path[prefix.len()..]).into_owned(),
            line,
            text,
        });
    }
    Ok(found)
}

fn glob_matches(pattern: &str, path: &str) -> bool {
    fn at(pattern: &[u8], path: &[u8]) -> bool {
        if pattern.is_empty() {
            return path.is_empty();
        }
        if pattern[0] == b'*' {
            let double = pattern.get(1) == Some(&b'*');
            let rest = &pattern[if double { 2 } else { 1 }..];
            if at(rest, path) {
                return true;
            }
            return !path.is_empty() && (double || path[0] != b'/') && at(pattern, &path[1..]);
        }
        !path.is_empty() && pattern[0] == path[0] && at(&pattern[1..], &path[1..])
    }
    at(pattern.as_bytes(), path.as_bytes())
}

fn allowed(path: &str, allowlist: &[String]) -> bool {
    allowlist.iter().any(|pattern| {
        glob_matches(pattern, path)
            || pattern
                .strip_suffix("/**")
                .is_some_and(|folder| path == folder || path.starts_with(&format!("{folder}/")))
    })
}

fn test_path(path: &str) -> bool {
    path.split('/')
        .any(|part| matches!(part, "test" | "tests" | "__tests__"))
        || path.contains(".test.")
        || path.contains(".spec.")
        || path.ends_with("_test.rs")
}

fn code_extension(path: &str) -> bool {
    [".rs", ".js", ".jsx", ".ts", ".tsx", ".py", ".go"]
        .iter()
        .any(|extension| path.ends_with(extension))
}

fn ui_copy_path(path: &str) -> bool {
    path.starts_with("ui/")
        && !test_path(path)
        && [".js", ".jsx", ".ts", ".tsx", ".json", ".html"]
            .iter()
            .any(|extension| path.ends_with(extension))
}

fn surface(path: &str) -> Surface {
    if path.starts_with(".github/") {
        Surface::Ci
    } else if test_path(path) {
        Surface::Tests
    } else if path.starts_with("docs/") || path.ends_with(".md") {
        Surface::Docs
    } else if ui_copy_path(path) {
        Surface::UiCopy
    } else if code_extension(path) {
        Surface::Code
    } else {
        Surface::Other
    }
}

fn symbol_surface(path: &str) -> Surface {
    if test_path(path) {
        Surface::Tests
    } else if path.starts_with(".github/") {
        Surface::Ci
    } else {
        Surface::Code
    }
}

fn language_matches(path: &str, language: Option<&str>) -> bool {
    match language {
        None => code_extension(path),
        Some("rust") => path.ends_with(".rs"),
        Some("javascript" | "js") => path.ends_with(".js") || path.ends_with(".jsx"),
        Some("typescript" | "ts") => path.ends_with(".ts") || path.ends_with(".tsx"),
        Some("python" | "py") => path.ends_with(".py"),
        Some("go") => path.ends_with(".go"),
        Some(_) => false,
    }
}

fn term_positions(line: &str, term: &str, case_sensitive: bool) -> Vec<(usize, usize)> {
    let haystack = if case_sensitive {
        line.to_string()
    } else {
        line.to_ascii_lowercase()
    };
    let needle = if case_sensitive {
        term.to_string()
    } else {
        term.to_ascii_lowercase()
    };
    let mut positions = Vec::new();
    let mut offset = 0;
    while offset <= haystack.len() {
        let Some(found) = haystack[offset..].find(&needle) else {
            break;
        };
        let start = offset + found;
        let end = start + needle.len();
        let before = line[..start].chars().next_back();
        let after = line[end..].chars().next();
        let boundary = |ch: char| !ch.is_alphanumeric() && ch != '_';
        if before.is_none_or(boundary) && after.is_none_or(boundary) {
            positions.push((start, end));
        }
        offset = end;
    }
    positions
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Context {
    Code,
    String,
    Comment,
}

/// Classify one byte position without turning identifier and comment text
/// into visible copy or a symbol definition.
fn context_at(line: &str, target: usize) -> Context {
    let bytes = line.as_bytes();
    let mut at = 0;
    let mut quote: Option<u8> = None;
    let mut block = false;
    while at < target && at < bytes.len() {
        if block {
            if bytes[at..].starts_with(b"*/") {
                block = false;
                at += 2;
            } else {
                at += 1;
            }
            continue;
        }
        if let Some(mark) = quote {
            if bytes[at] == b'\\' {
                at += 2;
            } else if bytes[at] == mark {
                quote = None;
                at += 1;
            } else {
                at += 1;
            }
            continue;
        }
        if bytes[at..].starts_with(b"//") {
            return Context::Comment;
        }
        if bytes[at..].starts_with(b"/*") {
            block = true;
            at += 2;
            continue;
        }
        if matches!(bytes[at], b'"' | b'\'' | b'`') {
            quote = Some(bytes[at]);
        }
        at += 1;
    }
    if block {
        Context::Comment
    } else if quote.is_some() {
        Context::String
    } else {
        Context::Code
    }
}

fn file_context_at(
    content: &str,
    path: &str,
    line_number: u64,
    column: usize,
) -> Result<Context, String> {
    let mut from = 0usize;
    let mut target = None;
    for (index, line) in content.split_inclusive('\n').enumerate() {
        if index as u64 + 1 == line_number {
            if column > line.len() {
                return Err("search position is outside its source line".into());
            }
            target = Some(from + column);
            break;
        }
        from += line.len();
    }
    let Some(target) = target else {
        return Err("search line is absent from its source file".into());
    };
    let bytes = content.as_bytes();
    let mut at = 0usize;
    let mut block_depth = 0u32;
    let mut line_comment = false;
    let mut quote: Option<u8> = None;
    while at < target {
        if bytes[at] == b'\n' {
            line_comment = false;
            at += 1;
            continue;
        }
        if line_comment {
            at += 1;
            continue;
        }
        if block_depth > 0 {
            if bytes[at..].starts_with(b"/*") {
                block_depth += 1;
                at += 2;
            } else if bytes[at..].starts_with(b"*/") {
                block_depth -= 1;
                at += 2;
            } else {
                at += 1;
            }
            continue;
        }
        if let Some(mark) = quote {
            if bytes[at] == b'\\' {
                at += 2;
            } else if bytes[at] == mark {
                quote = None;
                at += 1;
            } else {
                at += 1;
            }
            continue;
        }
        if bytes[at..].starts_with(b"//") || (path.ends_with(".py") && bytes[at] == b'#') {
            line_comment = true;
            at += 1;
            continue;
        }
        if bytes[at..].starts_with(b"/*") {
            block_depth = 1;
            at += 2;
            continue;
        }
        if matches!(bytes[at], b'"' | b'\'' | b'`') {
            let rust_lifetime =
                path.ends_with(".rs") && bytes[at] == b'\'' && bytes.get(at + 2) != Some(&b'\'');
            if !rust_lifetime {
                quote = Some(bytes[at]);
            }
        }
        at += 1;
    }
    if line_comment || block_depth > 0 {
        Ok(Context::Comment)
    } else if quote.is_some() {
        Ok(Context::String)
    } else {
        Ok(Context::Code)
    }
}

fn source_file(repo: &Path, tree: &Tree, path: &str) -> Result<String, String> {
    let object = format!("{}:{path}", tree.hash);
    let output = git(repo, &["show", &object])?;
    if !output.status.success() {
        return Err(format!("could not read source at {path}"));
    }
    String::from_utf8(output.stdout).map_err(|_| format!("source at {path} is not text"))
}

fn route_or_flag_declaration(line: &str, symbol: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    if symbol.starts_with("--") {
        [".option(", "option(", ".flag(", "flag(", ".arg(", "arg("]
            .iter()
            .any(|marker| lower.contains(marker))
    } else if symbol.starts_with('/') {
        [
            ".route(", "route(", ".get(", ".post(", ".put(", ".delete(", "router.", "app.",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
    } else {
        false
    }
}

fn visible_copy(path: &str, line: &str, start: usize) -> bool {
    match context_at(line, start) {
        Context::Comment => false,
        Context::Code => {
            let left = &line[..start];
            left.rfind('>')
                .is_some_and(|close| left.rfind('<').is_none_or(|open| close > open))
        }
        Context::String => {
            let trimmed = line.trim_start();
            if trimmed.starts_with("import ") || trimmed.starts_with("from ") {
                return false;
            }
            if path.ends_with(".json") {
                let after = &line[start..];
                if after
                    .find('"')
                    .is_some_and(|end| after[end + 1..].trim_start().starts_with(':'))
                {
                    return false;
                }
            }
            let before = &line[..start];
            !["className=", "id=", "src=", "href="]
                .iter()
                .any(|attribute| {
                    before
                        .trim_end_matches(|ch: char| ch != '=')
                        .ends_with(attribute)
                })
        }
    }
}

fn finding(surface: Surface, candidate: &Candidate, start: usize, end: usize) -> Finding {
    Finding {
        surface,
        path: candidate.path.clone(),
        line: candidate.line,
        span: Masked::mask(&candidate.text[start..end]).into_text(),
    }
}

fn outcome(
    tree_hash: Option<String>,
    requested: &[Surface],
    eligible: &[u64],
    mut findings: Vec<Finding>,
    reason: Option<String>,
    empty_is_not_applicable: bool,
) -> CheckOutcome {
    findings.sort_by(|a, b| (&a.path, a.line, &a.span).cmp(&(&b.path, b.line, &b.span)));
    findings.dedup();
    let surfaces: Vec<SurfaceResult> = requested
        .iter()
        .enumerate()
        .map(|(index, selected)| {
            let count = findings
                .iter()
                .filter(|found| found.surface == *selected)
                .count() as u64;
            let state = if count > 0 {
                VerdictState::Broken
            } else if eligible.get(index).copied().unwrap_or(0) == 0 {
                VerdictState::Unverifiable
            } else if empty_is_not_applicable {
                VerdictState::NotApplicable
            } else {
                VerdictState::Holds
            };
            SurfaceResult {
                surface: *selected,
                checked_places: eligible.get(index).copied().unwrap_or(0),
                finding_count: count,
                state,
            }
        })
        .collect();
    let state = if !findings.is_empty() {
        VerdictState::Broken
    } else if surfaces
        .iter()
        .any(|entry| entry.state == VerdictState::Unverifiable)
    {
        VerdictState::Unverifiable
    } else if empty_is_not_applicable {
        VerdictState::NotApplicable
    } else {
        VerdictState::Holds
    };
    CheckOutcome {
        state,
        tree_hash,
        reason,
        checked_places: eligible.iter().copied().sum(),
        findings,
        surfaces,
        not_looked_at: Vec::new(),
        visual: Vec::new(),
    }
}

fn unreadable(reason: String) -> CheckOutcome {
    CheckOutcome {
        state: VerdictState::Unverifiable,
        tree_hash: None,
        reason: Some(reason),
        checked_places: 0,
        findings: Vec::new(),
        surfaces: Vec::new(),
        not_looked_at: Vec::new(),
        visual: Vec::new(),
    }
}

pub fn check_symbol_absent(
    repo: &Path,
    version: &str,
    params: &SymbolAbsentParams,
) -> CheckOutcome {
    let tree = match tree(repo, version) {
        Ok(tree) => tree,
        Err(reason) => return unreadable(reason),
    };
    let selected = &params.surfaces;
    let eligible: Vec<u64> = selected
        .iter()
        .map(|surface| {
            tree.paths
                .iter()
                .filter(|path| {
                    !allowed(path, &params.allowlist)
                        && language_matches(path, params.language.as_deref())
                        && symbol_surface(path) == *surface
                })
                .count() as u64
        })
        .collect();
    if params.language.as_deref().is_some_and(|language| {
        !matches!(
            language,
            "rust" | "javascript" | "js" | "typescript" | "ts" | "python" | "py" | "go"
        )
    }) {
        return unreadable("symbol language is not supported".into());
    }
    let matches = match candidates(repo, &tree, std::slice::from_ref(&params.symbol)) {
        Ok(lines) => lines,
        Err(reason) => return unreadable(reason),
    };
    let mut findings = Vec::new();
    let mut mentions = 0u64;
    let mut files: HashMap<String, String> = HashMap::new();
    for candidate in &matches {
        let s = symbol_surface(&candidate.path);
        if !selected.contains(&s)
            || allowed(&candidate.path, &params.allowlist)
            || !language_matches(&candidate.path, params.language.as_deref())
        {
            continue;
        }
        if !files.contains_key(&candidate.path) {
            let content = match source_file(repo, &tree, &candidate.path) {
                Ok(content) => content,
                Err(reason) => return unreadable(reason),
            };
            files.insert(candidate.path.clone(), content);
        }
        let Some(content) = files.get(&candidate.path) else {
            return unreadable("source file was not retained".into());
        };
        for (start, end) in term_positions(&candidate.text, &params.symbol, true) {
            let context = match file_context_at(content, &candidate.path, candidate.line, start) {
                Ok(context) => context,
                Err(reason) => return unreadable(reason),
            };
            if context == Context::Code
                || (context == Context::String
                    && route_or_flag_declaration(&candidate.text, &params.symbol))
            {
                findings.push(finding(s, candidate, start, end));
            } else {
                mentions += 1;
            }
        }
    }
    let reason = if mentions > 0 {
        Some(format!("{mentions} text mentions are not code occurrences"))
    } else {
        None
    };
    outcome(
        Some(tree.hash),
        selected,
        &eligible,
        findings,
        reason,
        false,
    )
}

pub fn check_ui_copy_absent(
    repo: &Path,
    version: &str,
    params: &UiCopyAbsentParams,
) -> CheckOutcome {
    let tree = match tree(repo, version) {
        Ok(tree) => tree,
        Err(reason) => return unreadable(reason),
    };
    let selected = [Surface::UiCopy];
    let eligible = [tree
        .paths
        .iter()
        .filter(|path| ui_copy_path(path) && !allowed(path, &params.allowlist))
        .count() as u64];
    let matches = match candidates(repo, &tree, &params.terms) {
        Ok(lines) => lines,
        Err(reason) => return unreadable(reason),
    };
    let mut findings = Vec::new();
    for candidate in &matches {
        if !ui_copy_path(&candidate.path) || allowed(&candidate.path, &params.allowlist) {
            continue;
        }
        for term in &params.terms {
            for (start, end) in term_positions(&candidate.text, term, false) {
                if visible_copy(&candidate.path, &candidate.text, start) {
                    findings.push(finding(Surface::UiCopy, candidate, start, end));
                }
            }
        }
    }
    outcome(Some(tree.hash), &selected, &eligible, findings, None, false)
}

pub fn check_term_everywhere(
    repo: &Path,
    version: &str,
    params: &TermEverywhereParams,
) -> CheckOutcome {
    let tree = match tree(repo, version) {
        Ok(tree) => tree,
        Err(reason) => return unreadable(reason),
    };
    let selected = &params.surfaces;
    let eligible: Vec<u64> = selected
        .iter()
        .map(|surface| {
            tree.paths
                .iter()
                .filter(|path| !allowed(path, &params.allowlist) && self::surface(path) == *surface)
                .count() as u64
        })
        .collect();
    let mut needles = params.forbidden_variants.clone();
    needles.push(params.canonical.clone());
    let matches = match candidates(repo, &tree, &needles) {
        Ok(lines) => lines,
        Err(reason) => return unreadable(reason),
    };
    let mut findings = Vec::new();
    let mut canonical_count = 0u64;
    for candidate in &matches {
        let s = surface(&candidate.path);
        if !selected.contains(&s) || allowed(&candidate.path, &params.allowlist) {
            continue;
        }
        for (start, _) in term_positions(&candidate.text, &params.canonical, false) {
            if s != Surface::UiCopy || visible_copy(&candidate.path, &candidate.text, start) {
                canonical_count += 1;
            }
        }
        for variant in &params.forbidden_variants {
            for (start, end) in term_positions(&candidate.text, variant, false) {
                if s != Surface::UiCopy || visible_copy(&candidate.path, &candidate.text, start) {
                    findings.push(finding(s, candidate, start, end));
                }
            }
        }
    }
    let absent = canonical_count == 0 && findings.is_empty();
    outcome(Some(tree.hash), selected, &eligible, findings, None, absent)
}

pub fn check_forbidden_words(
    repo: &Path,
    version: &str,
    params: &ForbiddenWordsParams,
) -> CheckOutcome {
    let tree = match tree(repo, version) {
        Ok(tree) => tree,
        Err(reason) => return unreadable(reason),
    };
    let selected = &params.surfaces;
    let eligible: Vec<u64> = selected
        .iter()
        .map(|surface| {
            tree.paths
                .iter()
                .filter(|path| !allowed(path, &params.allowlist) && self::surface(path) == *surface)
                .count() as u64
        })
        .collect();
    let matches = match candidates(repo, &tree, &params.words) {
        Ok(lines) => lines,
        Err(reason) => return unreadable(reason),
    };
    let mut findings = Vec::new();
    for candidate in &matches {
        let s = surface(&candidate.path);
        if !selected.contains(&s) || allowed(&candidate.path, &params.allowlist) {
            continue;
        }
        for word in &params.words {
            for (start, end) in term_positions(&candidate.text, word, false) {
                if s != Surface::UiCopy || visible_copy(&candidate.path, &candidate.text, start) {
                    findings.push(finding(s, candidate, start, end));
                }
            }
        }
    }
    outcome(Some(tree.hash), selected, &eligible, findings, None, false)
}
