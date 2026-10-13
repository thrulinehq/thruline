//! The last presentation boundary for a person's path exclusions. This walks
//! already masked view/export JSON; it never alters the ledger, an event, or
//! any judgment derived from them. A miss returns the original bytes exactly.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use serde_json::{Map, Value};
use thruline_core::capture_exclusion::{
    command_path_arguments, marker, path_without_line_suffix, ExclusionPolicy, LEFT_OUT_FIELD,
};
use thruline_core::capture_hold;
use thruline_core::ledger::Ledger;

/// Capture holds name an HQ repository root. Export and Change reads may name
/// one of its ledger members instead; for Claude Code that member's stored
/// root can be the transcript directory. Map through HQ membership first,
/// retaining the stored root only for legacy or scratch members not in HQ.
pub(crate) fn project_roots(ledger: &Ledger) -> Result<BTreeMap<String, String>> {
    let mut roots: BTreeMap<_, _> = ledger.project_roots()?.into_iter().collect();
    for project in thruline_core::hq::hq_projects(ledger)? {
        for member in project.members {
            // A synthetic or legacy Claude member may not use a mangled
            // absolute folder name at all. Its decoded HQ root is then
            // relative and cannot be a policy root. Otherwise use the same
            // HQ root the capture-hold write address records, including when
            // it came from a lossy folder-name decode.
            if project.root.starts_with('/') {
                roots.insert(member.ledger_project_id, project.root.clone());
            }
        }
    }
    Ok(roots)
}

/// Read the standing exclusions for one project folder. An unreadable record
/// is an error: silently serving unfiltered material would break the promise.
pub(crate) fn policy(ledger: &Ledger, root: &str) -> Result<ExclusionPolicy> {
    let holds = capture_hold::holds_view(ledger, root)?;
    Ok(ExclusionPolicy::from_exclusions(root, &holds.exclusions)?)
}

/// A byte-preserving no-op when there is nothing to leave out. When entries
/// are replaced, add the number to the document, beside its other fields.
pub(crate) fn bytes<'a>(body: &'a [u8], policy: &ExclusionPolicy) -> Result<Cow<'a, [u8]>> {
    if policy.is_empty() {
        return Ok(Cow::Borrowed(body));
    }
    let mut doc: Value = serde_json::from_slice(body)?;
    if checked_apply(&mut doc, policy)? == 0 {
        return Ok(Cow::Borrowed(body));
    }
    Ok(Cow::Owned(serde_json::to_vec(&doc)?))
}

/// Replace path-bearing entries in a JSON document. The entry keeps its
/// identity, shape and numeric facts, while every display string associated
/// with the excluded file becomes the same marker. The count is per entry,
/// rather than per string field inside an entry.
pub(crate) fn apply(doc: &mut Value, policy: &ExclusionPolicy) -> u64 {
    apply_with_generated(doc, policy, &HashSet::new())
}

/// The production walk, including files the repository marks generated with
/// `linguist-generated` in Git attributes. A failed lookup refuses the view
/// instead of silently exposing a category the person turned on.
pub(crate) fn checked_apply(doc: &mut Value, policy: &ExclusionPolicy) -> Result<u64> {
    if !policy.has_generated() {
        return Ok(apply(doc, policy));
    }
    let generated = generated_paths(doc, policy)?;
    Ok(apply_with_generated(doc, policy, &generated))
}

/// The search corpus is not JSON until after matching. Batch the same Git
/// attribute lookup over its original searchable strings before hiding them.
pub(crate) fn generated_for_texts<'a>(
    policy: &ExclusionPolicy,
    texts: impl IntoIterator<Item = &'a str>,
) -> Result<HashSet<String>> {
    if !policy.has_generated() {
        return Ok(HashSet::new());
    }
    let doc = Value::Array(
        texts
            .into_iter()
            .map(|s| Value::String(s.to_owned()))
            .collect(),
    );
    generated_paths(&doc, policy)
}

fn apply_with_generated(
    doc: &mut Value,
    policy: &ExclusionPolicy,
    generated: &HashSet<String>,
) -> u64 {
    if policy.is_empty() {
        return 0;
    }
    let count = visit(doc, policy, generated, None);
    if count > 0 {
        if let Value::Object(fields) = doc {
            fields.insert(LEFT_OUT_FIELD.into(), Value::from(count));
        }
    }
    count
}

fn visit(
    value: &mut Value,
    policy: &ExclusionPolicy,
    generated: &HashSet<String>,
    parent: Option<&str>,
) -> u64 {
    match value {
        Value::Object(fields) => {
            if let Some(key) = direct_key(fields, policy, generated) {
                replace_entry(fields, &marker(key));
                return 1;
            }
            let mut left_out = 0;
            let mut hidden_keys = BTreeSet::new();
            let mut removed_keys = Vec::new();
            for (name, child) in fields.iter_mut() {
                if path_key_candidate(name, parent) {
                    if let Some(key) = policy.key_for_path(name, generated.contains(name)) {
                        let mark = marker(key);
                        // The keyed subtree is opaque: arbitrary nested
                        // object keys can themselves carry excluded content.
                        // Several excluded keys can share one fixed marker;
                        // count each omission and keep unrelated entries.
                        hidden_keys.insert(mark);
                        removed_keys.push(name.clone());
                        left_out += 1;
                        continue;
                    }
                }
                if !identity_scalar(name, child) && !direct_path_scalar(name, child) {
                    left_out += visit(child, policy, generated, Some(name));
                }
            }
            for name in removed_keys {
                fields.remove(&name);
            }
            for mark in hidden_keys {
                fields.insert(mark.clone(), Value::String(mark));
            }
            left_out
        }
        Value::Array(items) => items
            .iter_mut()
            .map(|item| visit(item, policy, generated, parent))
            .sum(),
        Value::String(s) if !s.starts_with("left out by your exclusion (`") => {
            if let Some(key) = policy
                .key_for_path(s, generated.contains(s))
                .or_else(|| text_key(s, policy, generated))
            {
                *s = marker(key);
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}

fn direct_key<'a>(
    fields: &Map<String, Value>,
    policy: &'a ExclusionPolicy,
    generated: &HashSet<String>,
) -> Option<&'a str> {
    for name in ["file", "path", "moved_to", "file_path", "record_path"] {
        if let Some(path) = fields.get(name).and_then(Value::as_str) {
            if let Some(key) = policy.key_for_path(path, generated.contains(path)) {
                return Some(key);
            }
        }
    }
    for name in ["command", "quote", "text", "sentence", "summary"] {
        if let Some(text) = fields.get(name).and_then(Value::as_str) {
            if !text.starts_with("left out by your exclusion (`") {
                let key = if name == "command" {
                    policy.key_in_command_with_generated(text, |path| generated.contains(path))
                } else {
                    None
                };
                if let Some(key) = key.or_else(|| text_key(text, policy, generated)) {
                    return Some(key);
                }
            }
        }
    }
    // Session tool rows hold the operation and its output as sibling arrays.
    // If a summary names an excluded file, its output can contain the file's
    // contents without repeating its path.
    if let Some(lines) = fields.get("summary").and_then(Value::as_array) {
        for line in lines {
            if let Some(text) = line.as_str() {
                if let Some(key) = policy
                    .key_in_command_with_generated(text, |path| generated.contains(path))
                    .or_else(|| text_key(text, policy, generated))
                {
                    return Some(key);
                }
            }
        }
    }
    None
}

fn text_key<'a>(
    text: &str,
    policy: &'a ExclusionPolicy,
    generated: &HashSet<String>,
) -> Option<&'a str> {
    policy.key_in_text_with_generated(text, |path| generated.contains(path))
}

fn path_tokens(text: &str) -> Vec<&str> {
    text.split(|ch: char| {
        !(ch.is_alphanumeric()
            || matches!(
                ch,
                '/' | '\\' | '.' | '-' | '_' | '~' | '+' | '%' | '@' | ':' | '$'
            ))
    })
    .map(|token| token.trim_end_matches(['.', ':']))
    .filter(|token| {
        (token.contains('/') || token.contains('\\') || token.contains('.'))
            && !token.contains("://")
            && !token.contains('$')
            && !token.starts_with('~')
    })
    .collect()
}

fn path_key_candidate(key: &str, parent: Option<&str>) -> bool {
    !key.starts_with("left out by your exclusion (`")
        && (key.contains('/')
            || key.contains('\\')
            || key.starts_with('.')
            || matches!(parent, Some("files" | "paths" | "file_map" | "file_list")))
}

fn generated_paths(doc: &Value, policy: &ExclusionPolicy) -> Result<HashSet<String>> {
    if !policy.has_generated() {
        return Ok(HashSet::new());
    }
    let mut candidates = BTreeSet::new();
    collect_candidates(doc, None, &mut candidates);
    let mut by_relative: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for original in candidates {
        if let Some(relative) = relative_to_root(&original, policy.project_root())? {
            by_relative.entry(relative).or_default().push(original);
        }
    }
    if by_relative.is_empty() {
        return Ok(HashSet::new());
    }

    let mut git = Command::new("git");
    thruline_core::git_isolation::isolate_config(&mut git);
    let mut child = git
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .arg("-C")
        .arg(policy.project_root())
        .args(["check-attr", "-z", "--stdin", "linguist-generated"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("reading generated-file Git attributes")?;
    let mut input = Vec::new();
    for relative in by_relative.keys() {
        input.extend_from_slice(relative.as_bytes());
        input.push(0);
    }
    let mut stdin = child.stdin.take().context("opening Git attribute input")?;
    // Read Git's output while writing its input. A large project can fill
    // both pipes; writing all input first would then deadlock.
    let writer = std::thread::spawn(move || -> std::io::Result<()> { stdin.write_all(&input) });
    let output = child
        .wait_with_output()
        .context("waiting for generated-file Git attributes")?;
    writer
        .join()
        .map_err(|_| anyhow::anyhow!("Git attribute input writer stopped"))??;
    if !output.status.success() {
        bail!(
            "generated-file Git attribute lookup failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let chunks: Vec<&[u8]> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|chunk| !chunk.is_empty())
        .collect();
    if !chunks.len().is_multiple_of(3) {
        bail!("generated-file Git attribute output was incomplete");
    }
    let mut generated = HashSet::new();
    let mut seen = BTreeSet::new();
    for triple in chunks.chunks_exact(3) {
        let path = std::str::from_utf8(triple[0])?;
        let attribute = std::str::from_utf8(triple[1])?;
        let value = std::str::from_utf8(triple[2])?;
        if attribute != "linguist-generated"
            || !by_relative.contains_key(path)
            || !seen.insert(path)
        {
            bail!("generated-file Git attribute output did not match its input");
        }
        if matches!(value, "set" | "true" | "1") {
            if let Some(originals) = by_relative.get(path) {
                generated.extend(originals.iter().cloned());
            }
        }
    }
    if seen.len() != by_relative.len() {
        bail!("generated-file Git attribute output was incomplete");
    }
    Ok(generated)
}

fn collect_candidates(value: &Value, name: Option<&str>, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(fields) => {
            for (field, child) in fields {
                if path_key_candidate(field, name) {
                    out.insert(field.clone());
                }
                if !identity_scalar(field, child) {
                    collect_candidates(child, Some(field), out);
                }
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_candidates(child, name, out);
            }
        }
        Value::String(s) if !s.starts_with("left out by your exclusion (`") => {
            for argument in command_path_arguments(s) {
                out.insert(argument.to_owned());
                if let Some(path) = path_without_line_suffix(argument) {
                    out.insert(path.to_owned());
                }
            }
            if matches!(
                name,
                Some("file" | "path" | "moved_to" | "file_path" | "record_path")
            ) {
                out.insert(s.clone());
            }
            if !s.chars().any(char::is_whitespace)
                && !s.contains("://")
                && !s.contains('$')
                && !s.starts_with('~')
            {
                out.insert(s.clone());
            }
            for token in path_tokens(s) {
                out.insert(token.to_owned());
                if let Some(path) = path_without_line_suffix(token) {
                    out.insert(path.to_owned());
                }
            }
            for quoted in s.split(['\'', '"', '`']).skip(1).step_by(2) {
                if quoted.contains('/') || quoted.contains('.') {
                    out.insert(quoted.to_owned());
                    if let Some(path) = path_without_line_suffix(quoted) {
                        out.insert(path.to_owned());
                    }
                }
            }
        }
        _ => {}
    }
}

fn relative_to_root(path: &str, root: &str) -> Result<Option<String>> {
    // The matcher resolves `.` and an in-root `..` lexically. Git must receive
    // that same name, otherwise `src/../generated/a.rs` can be served even
    // though `generated/a.rs` has the repository's generated attribute.
    if path.contains('\0') || root.contains('\0') {
        bail!("generated-file path cannot contain a NUL byte");
    }
    if path.starts_with('\\') {
        return Ok(None);
    }
    let Some(parts) = lexical_components(path) else {
        return Ok(None);
    };
    let relative = if path.starts_with('/') {
        let root_parts =
            lexical_components(root).context("invalid project root for Git attributes")?;
        let Some(relative) = parts.strip_prefix(root_parts.as_slice()) else {
            return Ok(None);
        };
        relative
    } else {
        parts.as_slice()
    };
    Ok((!relative.is_empty()).then(|| relative.join("/")))
}

fn lexical_components(path: &str) -> Option<Vec<&str>> {
    let mut parts = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts)
}

/// These are identity and classification fields, not presentation prose. In
/// particular `raw_file` names the recording, not a source file in the repo.
fn structural(name: &str) -> bool {
    matches!(
        name,
        "id" | "event_id"
            | "session_id"
            | "version_id"
            | "thread_id"
            | "ledger_project_id"
            | "hq_project_id"
            | "raw_file"
            | "raw_line"
            | "ts"
            | "started_at"
            | "last_at"
            | "committed_at"
            | "at"
            | "kind"
            | "source"
            | "op"
            | "via"
            | "outcome"
            | "verdict"
            | "verdict_basis"
            | "marker"
            | "source_name"
            | "tool_name"
            | "title_basis"
            | "schema_version"
            | "contract_version"
            | "format"
            | "commit"
            | "short"
            | "project_id"
            | "change_id"
            | "decision_id"
            | "review_id"
            | "act"
            | "standing"
            | "status"
            | "state"
            | "target_revision"
            | "scope_version"
            | "version"
    )
}

fn identity_scalar(name: &str, value: &Value) -> bool {
    structural(name) && !matches!(value, Value::Object(_) | Value::Array(_))
}

fn direct_path_scalar(name: &str, value: &Value) -> bool {
    matches!(
        name,
        "file" | "path" | "moved_to" | "file_path" | "record_path"
    ) && value.is_string()
}

fn replace_entry(fields: &mut Map<String, Value>, mark: &str) {
    replace_object(fields, mark, None);
}

fn replace_object(fields: &mut Map<String, Value>, mark: &str, parent: Option<&str>) {
    let mut hidden_keys = BTreeSet::new();
    for (name, mut child) in std::mem::take(fields) {
        if path_key_candidate(&name, parent) {
            hidden_keys.insert(mark.to_owned());
            continue;
        }
        if !identity_scalar(&name, &child) {
            match (&name[..], &mut child) {
                // Patch lines and prompt segments belong to the excluded
                // entry even when they do not repeat the file's path.
                ("hunks" | "segments", Value::Array(items)) => items.clear(),
                (_, child) => replace_content(child, mark, Some(&name)),
            }
        }
        fields.insert(name, child);
    }
    // A pre-existing marker key must not restore an unsanitized subtree.
    for key in hidden_keys {
        fields.insert(key, Value::String(mark.to_owned()));
    }
}

fn replace_content(value: &mut Value, mark: &str, parent: Option<&str>) {
    match value {
        Value::String(s) => *s = mark.to_owned(),
        Value::Array(items) => {
            for item in items {
                replace_content(item, mark, parent);
            }
        }
        Value::Object(fields) => replace_object(fields, mark, parent),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thruline_core::capture_hold::ExclusionView;

    fn policy_for(key: &str) -> ExclusionPolicy {
        let entry = ExclusionView {
            key: key.into(),
            covers: key.into(),
            note: None,
            built_in: false,
            on: true,
            source: "recorded",
            basis: None,
            since: None,
            by: None,
        };
        ExclusionPolicy::from_exclusions("/work/app", &[entry]).unwrap()
    }

    #[test]
    fn a_miss_is_the_same_bytes_and_an_edit_does_not_leak_its_patch_lines() {
        let body = br#"{"schema_version":1,"edits":[{"id":"e1","file":"src/a.rs","hunks":[{"new":"private code"}],"lines_added":1}],"files":[{"path":"src/a.rs"}]}"#;
        let empty = ExclusionPolicy::from_exclusions("/work/app", &[]).unwrap();
        assert!(matches!(bytes(body, &empty), Ok(Cow::Borrowed(b)) if b == body));
        assert!(matches!(bytes(body, &policy_for("other")), Ok(Cow::Borrowed(b)) if b == body));
        let hidden = bytes(body, &policy_for("src/a.rs")).unwrap();
        let doc: Value = serde_json::from_slice(&hidden).unwrap();
        let mark = marker("src/a.rs");
        assert_eq!(doc["edits"][0]["file"], mark);
        assert_eq!(doc["edits"][0]["hunks"], serde_json::json!([]));
        assert_eq!(doc["files"][0]["path"], mark);
        assert_eq!(doc[LEFT_OUT_FIELD], 2);
        assert!(!String::from_utf8_lossy(&hidden).contains("private code"));
    }

    #[test]
    fn a_command_that_names_an_excluded_path_hides_its_output_too() {
        let mut doc = serde_json::json!({
            "commands": [{"id":"c1","command":"cat secrets/key","output_excerpt":["the key's value"]}],
            "prompts": [{"event_id":"p1","text":"Inspect secrets/key","segments":[{"kind":"typed","text":"Inspect secrets/key"}]}]
        });
        assert_eq!(apply(&mut doc, &policy_for("secrets")), 2);
        let body = serde_json::to_string(&doc).unwrap();
        assert!(!body.contains("the key's value"));
        assert_eq!(doc["commands"][0]["command"], marker("secrets"));
        assert_eq!(doc["prompts"][0]["segments"], serde_json::json!([]));
    }

    #[test]
    fn a_nested_object_under_an_identity_field_cannot_keep_the_excluded_content() {
        let mut doc = serde_json::json!({
            "edits": [{"file":"src/a.rs","source":{"detail":"private patch"}}]
        });
        assert_eq!(apply(&mut doc, &policy_for("src/a.rs")), 1);
        assert_eq!(doc["edits"][0]["source"]["detail"], marker("src/a.rs"));
    }

    #[test]
    fn a_directly_excluded_entry_scrubs_nested_path_keys_and_their_content() {
        let mut doc = serde_json::json!({
            "id": "e1",
            "file": "src/a.rs",
            "files": {
                "src/a.rs": {"body": "PRIVATE CONTENT", "nested/path": "PRIVATE KEY"},
                "src/b.rs": {"body": "other associated content"}
            }
        });
        assert_eq!(apply(&mut doc, &policy_for("src/a.rs")), 1);
        let mark = marker("src/a.rs");
        assert_eq!(doc["id"], "e1");
        assert_eq!(doc["file"], mark);
        assert!(doc["files"].get("src/a.rs").is_none());
        assert!(doc["files"].get("src/b.rs").is_none());
        assert_eq!(doc["files"][&mark], mark);
        let body = serde_json::to_string(&doc).unwrap();
        assert!(!body.contains("PRIVATE CONTENT"));
        assert!(!body.contains("PRIVATE KEY"));
    }

    #[test]
    fn a_path_used_as_an_object_key_hides_its_nested_content_without_hiding_other_files() {
        let body = br#"{"files":{"src/a.rs":{"sha256":"private digest","detail":"PRIVATE CONTENT"},"src/b.rs":{"sha256":"public digest"}},"count":2}"#;
        let hidden = bytes(body, &policy_for("src/a.rs")).expect("present keyed file");
        let doc: Value = serde_json::from_slice(&hidden).expect("JSON");
        let mark = marker("src/a.rs");
        assert!(doc["files"].get("src/a.rs").is_none());
        assert_eq!(doc["files"][&mark], mark);
        assert_eq!(doc["files"]["src/b.rs"]["sha256"], "public digest");
        assert_eq!(doc["count"], 2);
        assert_eq!(doc[LEFT_OUT_FIELD], 1);
        assert!(!String::from_utf8_lossy(&hidden).contains("PRIVATE CONTENT"));
        assert!(matches!(
            bytes(body, &policy_for("unrelated")),
            Ok(Cow::Borrowed(original)) if original == body
        ));
    }

    #[test]
    fn change_side_views_hide_paths_but_keep_their_verdicts_and_ids() {
        let mut doc = serde_json::json!({
            "items": [{"id":"a1","standing":"applies","checked_paths":["src/a.rs"]}],
            "prepare": {"config_files":["src/a.rs"],"ready":true},
            "findings": [{"id":"f1","path":"src/a.rs","message":"private finding"}],
            "proposals": [{"id":"p1","text":"Read src/a.rs:42","kind":"criterion"}]
        });
        assert_eq!(apply(&mut doc, &policy_for("src/a.rs")), 4);
        let body = serde_json::to_string(&doc).unwrap();
        assert!(!body.contains("private finding"));
        assert!(!body.contains("Read src/a.rs:42"));
        assert_eq!(doc["items"][0]["standing"], "applies");
        assert_eq!(doc["findings"][0]["id"], "f1");
        assert_eq!(doc["prepare"]["ready"], true);
        assert_eq!(doc[LEFT_OUT_FIELD], 4);
    }

    #[test]
    fn generated_means_the_repository_marked_it_in_git_attributes() {
        let temp = tempfile::tempdir().unwrap();
        let status = Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(temp.path())
            .status()
            .unwrap();
        assert!(status.success());
        std::fs::write(
            temp.path().join(".gitattributes"),
            "generated/** linguist-generated\nBUILDLOG linguist-generated\n",
        )
        .unwrap();
        let entry = ExclusionView {
            key: "generated".into(),
            covers: "generated files".into(),
            note: None,
            built_in: true,
            on: true,
            source: "recorded",
            basis: None,
            since: None,
            by: None,
        };
        let policy =
            ExclusionPolicy::from_exclusions(temp.path().to_str().unwrap(), &[entry]).unwrap();
        let mut doc = serde_json::json!({
            "edits": [
                {"file":"generated/a.rs","hunks":[{"text":"a secret"}]},
                {"file":"src/a.rs","hunks":[{"text":"public"}]}
            ],
            "commands": [
                {"command":"cat 'generated/my file.rs'","output_excerpt":["another secret"]},
                {"command":"cat BUILDLOG","output_excerpt":["bare generated content"]}
            ],
            "tools": [{"summary":["cat BUILDLOG"],"output_excerpt":["tool result secret"]}],
            "prompts": [{"text":"Read generated/a.rs:42","segments":[{"text":"private quote"}]}]
            ,"files": {"generated/file.rs": {"sha256":"private generated digest"},
                        "src/a.rs": {"sha256":"public digest"}}
        });
        assert_eq!(checked_apply(&mut doc, &policy).unwrap(), 6);
        assert_eq!(doc["edits"][0]["file"], marker("generated"));
        assert_eq!(doc["edits"][0]["hunks"], serde_json::json!([]));
        assert_eq!(doc["edits"][1]["file"], "src/a.rs");
        assert_eq!(doc["commands"][0]["command"], marker("generated"));
        assert_eq!(doc["commands"][1]["command"], marker("generated"));
        assert!(!serde_json::to_string(&doc)
            .unwrap()
            .contains("another secret"));
        assert!(!serde_json::to_string(&doc)
            .unwrap()
            .contains("bare generated content"));
        assert_eq!(doc["tools"][0]["summary"][0], marker("generated"));
        assert!(!serde_json::to_string(&doc)
            .unwrap()
            .contains("tool result secret"));
        assert_eq!(doc["prompts"][0]["text"], marker("generated"));
        assert_eq!(doc["prompts"][0]["segments"], serde_json::json!([]));
        assert!(doc["files"].get("generated/file.rs").is_none());
        assert_eq!(doc["files"][&marker("generated")], marker("generated"));
        assert_eq!(doc["files"]["src/a.rs"]["sha256"], "public digest");
        assert_eq!(doc[LEFT_OUT_FIELD], 6);
    }

    #[test]
    fn generated_lookup_normalizes_in_root_paths_without_accepting_escape() {
        let temp = tempfile::tempdir().unwrap();
        assert!(Command::new("git")
            .arg("init")
            .arg("--quiet")
            .arg(temp.path())
            .status()
            .unwrap()
            .success());
        std::fs::write(
            temp.path().join(".gitattributes"),
            "generated/** linguist-generated\n",
        )
        .unwrap();
        let root = temp.path().to_str().unwrap();
        let policy = ExclusionPolicy::from_exclusions(
            root,
            &[ExclusionView {
                key: "generated".into(),
                covers: "generated files".into(),
                note: None,
                built_in: true,
                on: true,
                source: "recorded",
                basis: None,
                since: None,
                by: None,
            }],
        )
        .unwrap();
        let absolute = format!("{root}/src/../generated/a.rs");
        let mut doc = serde_json::json!({
            "edits": [
                {"file": "src/../generated/a.rs", "hunks": [{"text": "relative private"}]},
                {"file": absolute, "hunks": [{"text": "absolute private"}]},
                {"file": "src/../../generated/a.rs", "hunks": [{"text": "outside root"}]}
            ]
        });
        assert_eq!(checked_apply(&mut doc, &policy).unwrap(), 2);
        assert_eq!(doc["edits"][0]["file"], marker("generated"));
        assert_eq!(doc["edits"][1]["file"], marker("generated"));
        assert_eq!(doc["edits"][0]["hunks"], serde_json::json!([]));
        assert_eq!(doc["edits"][1]["hunks"], serde_json::json!([]));
        assert_eq!(doc["edits"][2]["file"], "src/../../generated/a.rs");
        assert_eq!(doc["edits"][2]["hunks"][0]["text"], "outside root");

        let untouched = br#"{"edits":[{"file":"src/public.rs","hunks":[{"text":"public"}]}]}"#;
        assert!(matches!(bytes(untouched, &policy), Ok(Cow::Borrowed(raw)) if raw == untouched));
    }

    #[test]
    fn direct_file_fields_keep_a_literal_colon_filename_when_only_a_line_reference_would_match() {
        let mut doc = serde_json::json!({
            "edits": [{"file":"src/a.rs:42","hunks":[{"text":"literal colon file"}]}],
            "prompts": [{"text":"Read src/a.rs:42","segments":[{"text":"private quote"}]}]
        });
        assert_eq!(apply(&mut doc, &policy_for("src/a.rs")), 1);
        assert_eq!(doc["edits"][0]["file"], "src/a.rs:42");
        assert_eq!(doc["edits"][0]["hunks"][0]["text"], "literal colon file");
        assert_eq!(doc["prompts"][0]["text"], marker("src/a.rs"));
    }
}
