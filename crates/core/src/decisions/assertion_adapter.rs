//! B3's production assertion census from two immutable Git trees.
//!
//! The verifier owns assertion recognition. This adapter only chooses accepted
//! paths, rejects unreadable syntax and modes, and binds every count to a
//! bounded blob read. A source comment or caller-provided count has no role.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use thruline_verify::tamper::parse_tests;

use crate::change_link::{pattern_matches, PatternVerdict};
use crate::git_isolation::isolate_config;

use super::checks::CheckOutcome;
use super::checks_preservation::{
    check_assertion_floor, AssertionCensus, AssertionFile, AssertionFloorParams,
};

const MAX_BLOB_BYTES: u64 = 2 * 1024 * 1024;
const MAX_TOTAL_BLOB_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TREE_LIST_BYTES: usize = 16 * 1024 * 1024;
const MAX_SELECTED_FILES: usize = 10_000;

type SourceDialect = thruline_verify::tamper::Language;

#[derive(Clone)]
struct ParsedFile {
    blob: String,
    count: u64,
}

/// Check a person's assertion floor at one named target version and tree.
/// `tree_hash` is verified against Git before any result can hold or break.
pub fn check_at(
    repo: &Path,
    version: &str,
    tree_hash: &str,
    params: &AssertionFloorParams,
) -> CheckOutcome {
    let unbound = |reason: &str| CheckOutcome::unavailable(reason, None);
    if !is_exact_commit(repo, &params.baseline_version) {
        return unbound("assertion baseline must be an exact commit object id");
    }
    if !is_exact_commit(repo, version) {
        return unbound("assertion target must be an exact commit object id");
    }
    let Some(target_tree) = resolve_tree(repo, version) else {
        return unbound("assertion target version cannot be resolved");
    };
    if target_tree != tree_hash {
        return unbound("assertion target tree does not match the named version");
    }
    let unavailable = |reason: &str| CheckOutcome::unavailable(reason, Some(target_tree.clone()));
    let Some(baseline_tree) = resolve_tree(repo, &params.baseline_version) else {
        return unavailable("assertion baseline version cannot be resolved");
    };
    if params.paths.is_empty()
        || params.paths.iter().any(|path| {
            !safe_path(path)
                || pattern_matches(path, "__scope_probe__") == PatternVerdict::Unreadable
        })
    {
        return unavailable("assertion scope is invalid");
    }

    let mut total_bytes = 0;
    let baseline = match parsed_files(repo, &baseline_tree, &params.paths, &mut total_bytes) {
        Ok(files) => files,
        Err(reason) => return unavailable(reason),
    };
    let target = match parsed_files(repo, &target_tree, &params.paths, &mut total_bytes) {
        Ok(files) => files,
        Err(reason) => return unavailable(reason),
    };
    let files = join_files(baseline, target);
    let census = AssertionCensus {
        baseline_version: params.baseline_version.clone(),
        baseline_tree_hash: baseline_tree,
        target_version: version.into(),
        target_tree_hash: target_tree,
        complete: true,
        files,
    };
    check_assertion_floor(repo, version, params, &census)
}

fn parsed_files(
    repo: &Path,
    tree: &str,
    patterns: &[String],
    total_bytes: &mut u64,
) -> Result<BTreeMap<String, ParsedFile>, &'static str> {
    let listing = git_bounded(
        repo,
        &["ls-tree", "-r", "-z", "--full-tree", tree],
        MAX_TREE_LIST_BYTES,
    )
    .ok_or("assertion tree cannot be listed")?;
    let mut files = BTreeMap::new();
    for record in listing
        .split(|byte| *byte == 0)
        .filter(|row| !row.is_empty())
    {
        let tab = record
            .iter()
            .position(|byte| *byte == b'\t')
            .ok_or("assertion tree entry is malformed")?;
        let (header, path_with_tab) = record.split_at(tab);
        let path_bytes = &path_with_tab[1..];
        let path =
            std::str::from_utf8(path_bytes).map_err(|_| "assertion tree has a non-UTF-8 path")?;
        if !safe_path(path) {
            return Err("assertion tree has an unsafe path");
        }
        let selected = patterns.iter().try_fold(false, |hit, pattern| {
            match pattern_matches(pattern, path) {
                PatternVerdict::Matched => Ok(true),
                PatternVerdict::DidNotMatch => Ok(hit),
                PatternVerdict::Unreadable => Err("assertion scope cannot be matched"),
            }
        })?;
        if !selected {
            continue;
        }
        if files.len() >= MAX_SELECTED_FILES {
            return Err("assertion scope exceeds the file limit");
        }
        let header =
            std::str::from_utf8(header).map_err(|_| "assertion tree entry is malformed")?;
        let mut parts = header.split_ascii_whitespace();
        let (Some(mode), Some(kind), Some(blob), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err("assertion tree entry is malformed");
        };
        if kind != "blob" || !matches!(mode, "100644" | "100755") || !is_oid(blob) {
            return Err("assertion path has an unsupported Git mode or object");
        }
        // The shared verifier selects the TypeScript grammar for .tsx, so a
        // JSX assertion count cannot be certified by that parser yet.
        if path
            .rsplit_once('.')
            .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("tsx"))
        {
            return Err("TSX assertion parser is unavailable");
        }
        let language =
            SourceDialect::from_path(path).ok_or("assertion path has an unsupported language")?;
        let bytes = bounded_blob(repo, blob, total_bytes)?;
        let source = std::str::from_utf8(&bytes).map_err(|_| "assertion blob is not UTF-8")?;
        strict_syntax(source, language)?;
        let tests = parse_tests(source, language)
            .map_err(|_| "assertion verifier could not parse the blob")?;
        let count = tests
            .iter()
            .try_fold(0u64, |total, test| {
                total.checked_add(test.assertions.len() as u64)
            })
            .ok_or("assertion count overflowed")?;
        if files
            .insert(
                path.into(),
                ParsedFile {
                    blob: blob.into(),
                    count,
                },
            )
            .is_some()
        {
            return Err("assertion tree repeats a selected path");
        }
    }
    Ok(files)
}

fn join_files(
    mut baseline: BTreeMap<String, ParsedFile>,
    mut target: BTreeMap<String, ParsedFile>,
) -> Vec<AssertionFile> {
    let mut files = Vec::new();
    let shared: Vec<_> = baseline
        .keys()
        .filter(|path| target.contains_key(*path))
        .cloned()
        .collect();
    for path in shared {
        let base = baseline.remove(&path).expect("shared baseline path");
        let now = target.remove(&path).expect("shared target path");
        files.push(AssertionFile {
            baseline_path: Some(path.clone()),
            target_path: Some(path),
            baseline_blob: Some(base.blob),
            target_blob: Some(now.blob),
            baseline_count: base.count,
            target_count: now.count,
            moved_to: None,
        });
    }

    // An exact-blob one-to-one rename is unambiguous. Changed-content moves
    // remain two rows; B3 still nets the counts without claiming identity.
    let baseline_blob_counts = baseline.values().fold(BTreeMap::new(), |mut counts, file| {
        *counts.entry(file.blob.clone()).or_insert(0usize) += 1;
        counts
    });
    for (old_path, base) in baseline {
        let matches: Vec<_> = target
            .iter()
            .filter(|(_, now)| now.blob == base.blob)
            .map(|(path, _)| path.clone())
            .collect();
        if matches.len() == 1 && baseline_blob_counts.get(&base.blob) == Some(&1) {
            let new_path = matches[0].clone();
            let now = target.remove(&new_path).expect("unique target blob");
            files.push(AssertionFile {
                baseline_path: Some(old_path),
                target_path: Some(new_path.clone()),
                baseline_blob: Some(base.blob),
                target_blob: Some(now.blob),
                baseline_count: base.count,
                target_count: now.count,
                moved_to: Some(new_path),
            });
        } else {
            files.push(AssertionFile {
                baseline_path: Some(old_path),
                target_path: None,
                baseline_blob: Some(base.blob),
                target_blob: None,
                baseline_count: base.count,
                target_count: 0,
                moved_to: None,
            });
        }
    }
    for (path, now) in target {
        files.push(AssertionFile {
            baseline_path: None,
            target_path: Some(path),
            baseline_blob: None,
            target_blob: Some(now.blob),
            baseline_count: 0,
            target_count: now.count,
            moved_to: None,
        });
    }
    files.sort_by(|a, b| {
        (&a.baseline_path, &a.target_path).cmp(&(&b.baseline_path, &b.target_path))
    });
    files
}

fn strict_syntax(source: &str, language: SourceDialect) -> Result<(), &'static str> {
    let grammar: tree_sitter::Language = match language {
        SourceDialect::Python => tree_sitter_python::LANGUAGE.into(),
        SourceDialect::Rust => tree_sitter_rust::LANGUAGE.into(),
        SourceDialect::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        // Match the verifier's TypeScript grammar; .tsx was rejected earlier.
        SourceDialect::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        SourceDialect::Go => tree_sitter_go::LANGUAGE.into(),
    };
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&grammar)
        .map_err(|_| "assertion grammar could not be loaded")?;
    let tree = parser
        .parse(source, None)
        .ok_or("assertion syntax could not be parsed")?;
    if tree.root_node().has_error() {
        return Err("assertion blob has malformed syntax");
    }
    Ok(())
}

fn bounded_blob(repo: &Path, oid: &str, total_bytes: &mut u64) -> Result<Vec<u8>, &'static str> {
    let size = git(repo, &["cat-file", "-s", oid]).ok_or("assertion blob size cannot be read")?;
    let size = std::str::from_utf8(&size)
        .ok()
        .and_then(|text| text.trim().parse::<u64>().ok())
        .ok_or("assertion blob size is invalid")?;
    if size > MAX_BLOB_BYTES {
        return Err("assertion blob exceeds the byte limit");
    }
    *total_bytes = total_bytes
        .checked_add(size)
        .filter(|total| *total <= MAX_TOTAL_BLOB_BYTES)
        .ok_or("assertion scope exceeds the total byte limit")?;
    let bytes = git_bounded(repo, &["cat-file", "blob", oid], size as usize)
        .ok_or("assertion blob cannot be read")?;
    if bytes.len() as u64 != size {
        return Err("assertion blob size changed during read");
    }
    Ok(bytes)
}

fn resolve_tree(repo: &Path, version: &str) -> Option<String> {
    if version.is_empty() || version.chars().any(char::is_control) {
        return None;
    }
    let spec = format!("{version}^{{tree}}");
    let bytes = git(repo, &["rev-parse", "--verify", "--end-of-options", &spec])?;
    let oid = std::str::from_utf8(&bytes).ok()?.trim();
    is_oid(oid).then(|| oid.into())
}

fn is_exact_commit(repo: &Path, version: &str) -> bool {
    if !is_oid(version) {
        return false;
    }
    let spec = format!("{version}^{{commit}}");
    git(repo, &["rev-parse", "--verify", "--end-of-options", &spec])
        .and_then(|raw| String::from_utf8(raw).ok())
        .is_some_and(|resolved| resolved.trim().eq_ignore_ascii_case(version))
}

fn is_oid(value: &str) -> bool {
    (value.len() == 40 || value.len() == 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
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

fn git(repo: &Path, args: &[&str]) -> Option<Vec<u8>> {
    git_bounded(repo, args, 1024)
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
