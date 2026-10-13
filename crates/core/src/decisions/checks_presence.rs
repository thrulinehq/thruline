//! B1 and B6: deterministic checks against immutable Git tree objects.
//!
//! The working tree is never read. A ref is resolved once to a tree object;
//! every path and blob is then read by object id from that tree.

use std::path::Path;
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::decisions::checks::{CheckOutcome, Finding, SurfaceResult, VerdictState};
use crate::decisions::params::{FeatureDefaultPreservedParams, PathPresentParams, Surface};
use crate::redact::Masked;

struct Entry {
    mode: String,
    kind: String,
    oid: String,
}

fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>, ()> {
    let output = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|_| ())?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(())
    }
}

fn is_object_id(value: &str) -> bool {
    (value.len() == 40 || value.len() == 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn tree_id(repo: &Path, version: &str) -> Result<String, ()> {
    if version.trim().is_empty() || version.chars().any(char::is_control) {
        return Err(());
    }
    let object = format!("{version}^{{tree}}");
    let raw = git(
        repo,
        &["rev-parse", "--verify", "--end-of-options", &object],
    )?;
    let id = std::str::from_utf8(&raw).map_err(|_| ())?.trim();
    if is_object_id(id) {
        Ok(id.to_ascii_lowercase())
    } else {
        Err(())
    }
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

fn entry(repo: &Path, tree: &str, path: &str) -> Result<Option<Entry>, ()> {
    let raw = git(repo, &["ls-tree", "-z", "--full-tree", tree, "--", path])?;
    for record in raw
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let Some(tab) = record.iter().position(|byte| *byte == b'\t') else {
            return Err(());
        };
        if &record[tab + 1..] != path.as_bytes() {
            continue;
        }
        let header = std::str::from_utf8(&record[..tab]).map_err(|_| ())?;
        let mut parts = header.split_ascii_whitespace();
        let (Some(mode), Some(kind), Some(oid), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(());
        };
        if !is_object_id(oid) {
            return Err(());
        }
        return Ok(Some(Entry {
            mode: mode.to_string(),
            kind: kind.to_string(),
            oid: oid.to_string(),
        }));
    }
    Ok(None)
}

fn regular_blob(entry: &Entry) -> bool {
    entry.kind == "blob" && matches!(entry.mode.as_str(), "100644" | "100755")
}

fn blob(repo: &Path, oid: &str) -> Result<Vec<u8>, ()> {
    git(repo, &["cat-file", "blob", oid])
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn surface(path: &str) -> Surface {
    if path.starts_with("tests/") || path.starts_with("test/") {
        Surface::Tests
    } else if path.starts_with(".github/") {
        Surface::Ci
    } else if path.starts_with("docs/") || path.ends_with(".md") {
        Surface::Docs
    } else if path.starts_with("ui/") {
        Surface::UiCopy
    } else if path.ends_with(".rs") || path.ends_with(".toml") || path.ends_with(".js") {
        Surface::Code
    } else {
        Surface::Other
    }
}

fn outcome(
    state: VerdictState,
    tree_hash: Option<String>,
    reason: Option<&str>,
    findings: Vec<Finding>,
    surface: Surface,
) -> CheckOutcome {
    let checked_places = u64::from(state != VerdictState::Unverifiable);
    CheckOutcome {
        state,
        tree_hash,
        reason: reason.map(str::to_string),
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

fn finding(path: &str, line: u64, span: &str, surface: Surface) -> Finding {
    Finding {
        surface,
        path: Masked::mask(path).into_text(),
        line,
        span: Masked::mask(span).into_text(),
    }
}

pub fn check_path_present(repo: &Path, version: &str, params: &PathPresentParams) -> CheckOutcome {
    let s = surface(&params.path);
    if !safe_path(&params.path) {
        return outcome(
            VerdictState::Unverifiable,
            None,
            Some("invalid project path"),
            Vec::new(),
            s,
        );
    }
    let tree = match tree_id(repo, version) {
        Ok(tree) => tree,
        Err(()) => {
            return outcome(
                VerdictState::Unverifiable,
                None,
                Some("version cannot be resolved to a Git tree"),
                Vec::new(),
                s,
            );
        }
    };
    let found = match entry(repo, &tree, &params.path) {
        Ok(found) => found,
        Err(()) => {
            return outcome(
                VerdictState::Unverifiable,
                Some(tree),
                Some("version tree could not be read"),
                Vec::new(),
                s,
            );
        }
    };
    let Some(found) = found else {
        return outcome(
            VerdictState::Broken,
            Some(tree),
            None,
            vec![finding(&params.path, 0, "path absent from version", s)],
            s,
        );
    };
    if !regular_blob(&found) {
        return outcome(
            VerdictState::Broken,
            Some(tree),
            None,
            vec![finding(&params.path, 0, "path is not a regular file", s)],
            s,
        );
    }
    let Some(expected) = params.sha256.as_deref() else {
        return outcome(VerdictState::Holds, Some(tree), None, Vec::new(), s);
    };
    if expected.len() != 64 || !expected.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return outcome(
            VerdictState::Unverifiable,
            Some(tree),
            Some("accepted sha256 is invalid"),
            Vec::new(),
            s,
        );
    }
    let contents = match blob(repo, &found.oid) {
        Ok(contents) => contents,
        Err(()) => {
            return outcome(
                VerdictState::Unverifiable,
                Some(tree),
                Some("version blob could not be read"),
                Vec::new(),
                s,
            );
        }
    };
    let actual = sha256(&contents);
    if actual.eq_ignore_ascii_case(expected) {
        outcome(VerdictState::Holds, Some(tree), None, Vec::new(), s)
    } else {
        outcome(
            VerdictState::Broken,
            Some(tree),
            None,
            vec![finding(&params.path, 0, &format!("sha256 {actual}"), s)],
            s,
        )
    }
}

fn assignment_value<'a>(line: &'a str, symbol: &str) -> Option<&'a str> {
    let line = line.trim_start();
    if line.starts_with('#') || line.starts_with("//") {
        return None;
    }
    let (left, right) = line.split_once('=')?;
    if right.starts_with('=') {
        return None;
    }
    let name_part = left.split(':').next()?.trim_end();
    let prefix = name_part.strip_suffix(symbol)?.trim();
    if !(prefix.is_empty()
        || prefix.ends_with("const")
        || prefix.ends_with("static")
        || prefix.ends_with("let")
        || prefix.ends_with("var"))
    {
        return None;
    }
    let token = right
        .trim_start()
        .split(|ch: char| ch.is_whitespace() || ch == ';' || ch == ',')
        .next()?;
    (!token.is_empty()).then_some(token)
}

pub fn check_feature_default_preserved(
    repo: &Path,
    version: &str,
    params: &FeatureDefaultPreservedParams,
) -> CheckOutcome {
    let s = surface(&params.path);
    if !safe_path(&params.path) || params.symbol.is_empty() || params.expected_value.is_empty() {
        return outcome(
            VerdictState::Unverifiable,
            None,
            Some("invalid accepted default parameters"),
            Vec::new(),
            s,
        );
    }
    let tree = match tree_id(repo, version) {
        Ok(tree) => tree,
        Err(()) => {
            return outcome(
                VerdictState::Unverifiable,
                None,
                Some("version cannot be resolved to a Git tree"),
                Vec::new(),
                s,
            );
        }
    };
    let found = match entry(repo, &tree, &params.path) {
        Ok(found) => found,
        Err(()) => {
            return outcome(
                VerdictState::Unverifiable,
                Some(tree),
                Some("version tree could not be read"),
                Vec::new(),
                s,
            );
        }
    };
    let Some(found) = found else {
        return outcome(
            VerdictState::Broken,
            Some(tree),
            None,
            vec![finding(
                &params.path,
                0,
                "default file absent from version",
                s,
            )],
            s,
        );
    };
    if !regular_blob(&found) {
        return outcome(
            VerdictState::Broken,
            Some(tree),
            None,
            vec![finding(
                &params.path,
                0,
                "default path is not a regular file",
                s,
            )],
            s,
        );
    }
    let contents = match blob(repo, &found.oid) {
        Ok(contents) => contents,
        Err(()) => {
            return outcome(
                VerdictState::Unverifiable,
                Some(tree),
                Some("version blob could not be read"),
                Vec::new(),
                s,
            );
        }
    };
    let text = match String::from_utf8(contents) {
        Ok(text) => text,
        Err(_) => {
            return outcome(
                VerdictState::Unverifiable,
                Some(tree),
                Some("default file is not UTF-8 text"),
                Vec::new(),
                s,
            );
        }
    };
    let mut definitions = 0u64;
    let mut findings = Vec::new();
    let expect_absent = params.expected_value == "<absent>";
    for (index, line) in text.lines().enumerate() {
        if let Some(value) = assignment_value(line, &params.symbol) {
            definitions += 1;
            if expect_absent || value != params.expected_value {
                findings.push(finding(&params.path, index as u64 + 1, line, s));
            }
        }
    }
    if definitions == 0 && !expect_absent {
        findings.push(finding(
            &params.path,
            0,
            "default symbol absent from version",
            s,
        ));
    }
    let state = if findings.is_empty() {
        VerdictState::Holds
    } else {
        VerdictState::Broken
    };
    outcome(state, Some(tree), None, findings, s)
}
