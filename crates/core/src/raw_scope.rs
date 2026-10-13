//! Hash-validated metadata from one immutable transcript line.
//!
//! `cwd` and `gitBranch` are ambient per-line fields in Claude Code JSONL.
//! Codex can carry an explicit `cwd`/`workdir` inside the same tool envelope;
//! session-level `turn_context` never lends it to another line. These fields
//! are intentionally not copied into append-only `events`, but proof scope
//! still needs them. This cache reads each authoritative file once and accepts
//! metadata only when the line matches the hash frozen at ingest.

use std::collections::HashMap;
use std::io::BufRead;

use crate::chain::sha256_hex;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RawScope {
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    /// False means the source line was absent, unreadable, or no longer
    /// matched the immutable event hash. Unknown scope must not create an
    /// identity with conviction authority.
    pub trusted: bool,
}

#[derive(Debug, Clone, Default)]
struct CachedLine {
    sha256: String,
    cwd: Option<String>,
    git_branch: Option<String>,
}

#[derive(Default)]
pub(crate) struct RawScopeCache {
    files: HashMap<String, Vec<CachedLine>>,
}

impl RawScopeCache {
    pub(crate) fn scope_for(
        &mut self,
        raw_file: &str,
        raw_line: i64,
        expected_sha256: &str,
    ) -> RawScope {
        if !self.files.contains_key(raw_file) {
            self.files
                .insert(raw_file.to_string(), read_lines(raw_file));
        }
        let Some(index) = usize::try_from(raw_line)
            .ok()
            .and_then(|line| line.checked_sub(1))
        else {
            return RawScope::default();
        };
        let Some(line) = self.files.get(raw_file).and_then(|lines| lines.get(index)) else {
            return RawScope::default();
        };
        if expected_sha256.is_empty() || line.sha256 != expected_sha256 {
            return RawScope::default();
        }
        RawScope {
            cwd: line.cwd.clone(),
            git_branch: line.git_branch.clone(),
            trusted: true,
        }
    }
}

fn read_lines(raw_file: &str) -> Vec<CachedLine> {
    let Ok(file) = std::fs::File::open(raw_file) else {
        return Vec::new();
    };
    let mut reader = std::io::BufReader::new(file);
    let mut lines = Vec::new();
    loop {
        let mut bytes = Vec::new();
        let Ok(read) = reader.read_until(b'\n', &mut bytes) else {
            return Vec::new();
        };
        if read == 0 {
            break;
        }
        if bytes.ends_with(b"\n") {
            bytes.pop();
            if bytes.ends_with(b"\r") {
                bytes.pop();
            }
        }
        let sha256 = sha256_hex(&bytes);
        let parsed = std::str::from_utf8(&bytes)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
        let cwd = parsed.as_ref().and_then(same_line_cwd);
        let git_branch = parsed
            .as_ref()
            .and_then(|value| string_at(value, &["gitBranch", "git_branch"]));
        lines.push(CachedLine {
            sha256,
            cwd,
            git_branch,
        });
    }
    lines
}

fn same_line_cwd(value: &serde_json::Value) -> Option<String> {
    if let Some(cwd) = string_at(value, &["cwd", "workdir"]) {
        return Some(cwd);
    }
    let payload = value.get("payload")?;
    if let Some(cwd) = string_at(payload, &["cwd", "workdir"]) {
        return Some(cwd);
    }
    for key in ["execution", "input", "arguments"] {
        let Some(candidate) = payload.get(key) else {
            continue;
        };
        if let Some(cwd) = string_at(candidate, &["cwd", "workdir"]) {
            return Some(cwd);
        }
        if let Some(text) = candidate.as_str() {
            if let Ok(decoded) = serde_json::from_str::<serde_json::Value>(text) {
                if let Some(cwd) = string_at(&decoded, &["cwd", "workdir"]) {
                    return Some(cwd);
                }
            }
        }
    }
    None
}

fn string_at(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(serde_json::Value::as_str))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn scope_is_admitted_only_for_the_frozen_raw_line() {
        let mut file = tempfile::NamedTempFile::new().expect("temp raw file");
        let line = br#"{"cwd":"/repo","gitBranch":"main"}"#;
        file.write_all(line).expect("write line");
        file.write_all(b"\n").expect("write newline");
        let path = file.path().to_string_lossy().into_owned();
        let expected = sha256_hex(line);

        let mut cache = RawScopeCache::default();
        let scope = cache.scope_for(&path, 1, &expected);
        assert!(scope.trusted);
        assert_eq!(scope.cwd.as_deref(), Some("/repo"));
        assert_eq!(scope.git_branch.as_deref(), Some("main"));

        let unknown = cache.scope_for(&path, 1, &sha256_hex(b"rewritten"));
        assert_eq!(unknown, RawScope::default());
    }

    #[test]
    fn codex_scope_uses_only_explicit_same_line_workdir() {
        let mut file = tempfile::NamedTempFile::new().expect("temp raw file");
        let turn = br#"{"type":"turn_context","payload":{"cwd":"/ambient","git_branch":"topic"}}"#;
        let call = br#"{"type":"response_item","payload":{"type":"function_call","arguments":"{\"cwd\":\"/exact\"}"}}"#;
        file.write_all(turn).expect("turn");
        file.write_all(b"\n").expect("newline");
        file.write_all(call).expect("call");
        file.write_all(b"\n").expect("newline");
        let path = file.path().to_string_lossy().into_owned();

        let mut cache = RawScopeCache::default();
        let call_scope = cache.scope_for(&path, 2, &sha256_hex(call));
        assert!(call_scope.trusted);
        assert_eq!(call_scope.cwd.as_deref(), Some("/exact"));
        assert_eq!(call_scope.git_branch, None);
    }
}
