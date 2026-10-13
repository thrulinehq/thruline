//! Bounded, read-only Git adapter for advisory Family K history candidates.
//!
//! The K1/K2 structural evaluators receive exact first-parent tree data.
//! Other K evaluators require actor, assertion, workflow, command, or run
//! receipts that Git cannot establish, so their rows abstain. These are not
//! accepted Decisions verdicts.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::decisions::checks_k_candidates::{
    KCandidateObservation, KCandidateOutcome, KCandidateState,
};
use crate::decisions::checks_k_structural::{
    scan_k_structural, KStructuralInput, SourceCensus, SourceDelta, TestFileCensus,
};
use crate::redact::Masked;

const CODES: [&str; 10] = [
    "K1", "K2", "K3", "K8", "K9", "K13", "K14", "K15", "K16", "K17",
];
const MAX_HISTORY_PAIRS: usize = 200;
const MAX_TRACKED_FILES: usize = 50_000;
const MAX_CHANGED_FILES: usize = 10_000;
const MAX_TREE_OUTPUT: usize = 16 * 1024 * 1024;
const MAX_DIFF_OUTPUT: usize = 4 * 1024 * 1024;
const MAX_FILE_BYTES: usize = 2 * 1024 * 1024;
const MAX_SOURCE_BYTES: usize = 32 * 1024 * 1024;
type PathSelector = fn(&str) -> bool;

#[derive(Debug, Clone)]
pub struct KHistoryPair {
    pub baseline: String,
    pub target: String,
    pub outcomes: Vec<KCandidateOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TreeEntry {
    mode: String,
    kind: String,
    oid: String,
}

fn unavailable(code: &str, reason: &'static str) -> KCandidateOutcome {
    KCandidateOutcome {
        code: code.into(),
        state: KCandidateState::Unverifiable,
        observations: Vec::new(),
        reason: Some(reason),
        examined_source_files: 0,
    }
}

fn unavailable_all(reason: &'static str) -> Vec<KCandidateOutcome> {
    CODES.iter().map(|code| unavailable(code, reason)).collect()
}

fn full_oid(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn git(repo: &Path, args: &[&str], max_bytes: usize) -> Option<Vec<u8>> {
    let mut child = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .current_dir(repo)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    let mut output = Vec::new();
    let read = child
        .stdout
        .take()?
        .take((max_bytes as u64) + 1)
        .read_to_end(&mut output);
    if read.is_err() || output.len() > max_bytes {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    }
    child.wait().ok()?.success().then_some(output)
}

fn git_text(repo: &Path, args: &[&str], max_bytes: usize) -> Option<String> {
    String::from_utf8(git(repo, args, max_bytes)?).ok()
}

fn exact_commit(repo: &Path, commit: &str) -> bool {
    full_oid(commit)
        && git_text(repo, &["cat-file", "-t", commit], 32)
            .is_some_and(|kind| kind.trim() == "commit")
}

fn exact_first_parent(repo: &Path, baseline: &str, target: &str) -> bool {
    exact_commit(repo, baseline)
        && exact_commit(repo, target)
        && git_text(repo, &["rev-parse", &format!("{target}^1")], 128)
            .is_some_and(|parent| parent.trim() == baseline)
}

fn tree(repo: &Path, commit: &str) -> Option<BTreeMap<String, TreeEntry>> {
    let raw = git(repo, &["ls-tree", "-r", "-z", commit], MAX_TREE_OUTPUT)?;
    let mut entries = BTreeMap::new();
    for row in raw.split(|byte| *byte == 0).filter(|row| !row.is_empty()) {
        let tab = row.iter().position(|byte| *byte == b'\t')?;
        let header = std::str::from_utf8(&row[..tab]).ok()?;
        let path = std::str::from_utf8(&row[tab + 1..]).ok()?;
        if path.is_empty() || path.starts_with('/') || path.split('/').any(|part| part == "..") {
            return None;
        }
        let mut fields = header.split_ascii_whitespace();
        let mode = fields.next()?;
        let kind = fields.next()?;
        let oid = fields.next()?;
        if fields.next().is_some() || !full_oid(oid) || entries.len() >= MAX_TRACKED_FILES {
            return None;
        }
        if entries
            .insert(
                path.into(),
                TreeEntry {
                    mode: mode.into(),
                    kind: kind.into(),
                    oid: oid.into(),
                },
            )
            .is_some()
        {
            return None;
        }
    }
    Some(entries)
}

fn changed_paths(repo: &Path, baseline: &str, target: &str) -> Option<BTreeSet<String>> {
    let raw = git(
        repo,
        &[
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            baseline,
            target,
        ],
        MAX_DIFF_OUTPUT,
    )?;
    let mut paths = BTreeSet::new();
    for row in raw.split(|byte| *byte == 0).filter(|row| !row.is_empty()) {
        let path = std::str::from_utf8(row).ok()?;
        if path.is_empty() || !paths.insert(path.into()) || paths.len() > MAX_CHANGED_FILES {
            return None;
        }
    }
    Some(paths)
}

fn supported_source(path: &str) -> bool {
    [
        ".rs", ".js", ".jsx", ".ts", ".tsx", ".py", ".go", ".java", ".kt", ".cs", ".rb", ".php",
        ".swift", ".dart", ".exs",
    ]
    .iter()
    .any(|suffix| path.ends_with(suffix))
}

fn source_blob(repo: &Path, entry: Option<&TreeEntry>, used: &mut usize) -> Option<Option<String>> {
    let Some(entry) = entry else {
        return Some(None);
    };
    if entry.kind != "blob" || !entry.mode.starts_with("100") {
        return None;
    }
    let size = git_text(repo, &["cat-file", "-s", &entry.oid], 32)?
        .trim()
        .parse::<usize>()
        .ok()?;
    if size > MAX_FILE_BYTES || used.checked_add(size)? > MAX_SOURCE_BYTES {
        return None;
    }
    let raw = git(repo, &["cat-file", "blob", &entry.oid], MAX_FILE_BYTES)?;
    if raw.len() != size {
        return None;
    }
    *used += size;
    Some(Some(String::from_utf8(raw).ok()?))
}

fn scan_k1(
    repo: &Path,
    changed: &BTreeSet<String>,
    before: &BTreeMap<String, TreeEntry>,
    after: &BTreeMap<String, TreeEntry>,
) -> KCandidateOutcome {
    let mut source = SourceCensus {
        complete: true,
        changes: Vec::new(),
    };
    let mut used = 0usize;
    for path in changed.iter().filter(|path| supported_source(path)) {
        let Some(baseline) = source_blob(repo, before.get(path), &mut used) else {
            return unavailable(
                "K1",
                "changed source baseline blob is unreadable or over budget",
            );
        };
        let Some(target) = source_blob(repo, after.get(path), &mut used) else {
            return unavailable(
                "K1",
                "changed source target blob is unreadable or over budget",
            );
        };
        source.changes.push(SourceDelta {
            path: path.clone(),
            baseline,
            target,
        });
    }
    scan_k_structural(
        "K1",
        &KStructuralInput {
            source: Some(source),
            ..KStructuralInput::default()
        },
    )
}

fn scan_k2(
    before: &BTreeMap<String, TreeEntry>,
    after: &BTreeMap<String, TreeEntry>,
) -> KCandidateOutcome {
    // Treat every tracked path as a possible test path. No removed path is a
    // sound narrow NoSignal. A removed path has no accepted source/move link.
    let baseline_paths = before.keys().cloned().collect::<Vec<_>>();
    let target_paths = after.keys().cloned().collect::<Vec<_>>();
    let mut outcome = scan_k_structural(
        "K2",
        &KStructuralInput {
            files: Some(TestFileCensus {
                complete: true,
                baseline_test_paths: baseline_paths,
                target_test_paths: target_paths.clone(),
                target_source_paths: target_paths,
                links: Vec::new(),
            }),
            ..KStructuralInput::default()
        },
    );
    outcome.reason = Some(if outcome.state == KCandidateState::NoSignal {
        "no tracked file path was removed; test/source links are unavailable"
    } else {
        "a tracked path was removed; accepted test/source and move links are unavailable"
    });
    outcome
}

fn is_rule_file(path: &str) -> bool {
    let basename = path.rsplit('/').next().unwrap_or("");
    path == ".thruline"
        || path.starts_with(".thruline/")
        || matches!(basename, "CLAUDE.md" | "AGENTS.md")
        || matches!(
            path,
            ".claude/settings.json"
                | ".claude/settings.local.json"
                | ".pre-commit-config.yaml"
                | "lefthook.yml"
        )
        || path.starts_with(".husky/")
        || path.starts_with(".githooks/")
        || path.starts_with(".git/hooks/")
}

fn is_spec_plan(path: &str) -> bool {
    let basename = path.rsplit('/').next().unwrap_or("");
    path.starts_with("spec/")
        || basename == "PLAN.md"
        || (path.starts_with("docs/")
            && basename.starts_with("SPEC")
            && (basename.ends_with(".md") || basename.ends_with(".mdx")))
}

fn record_gap(code: &str, changed: &BTreeSet<String>, reason: &'static str) -> KCandidateOutcome {
    let selector: Option<(PathSelector, &'static str)> = match code {
        "K13" => Some((is_rule_file, "rule_file_changed_actor_unverified")),
        "K17" => Some((is_spec_plan, "spec_plan_changed_actor_unverified")),
        _ => None,
    };
    let observations = selector.map_or_else(Vec::new, |(relevant, marker)| {
        changed
            .iter()
            .filter(|path| relevant(path))
            .map(|path| KCandidateObservation {
                path: Masked::mask(path).into_text(),
                line: 0,
                marker,
            })
            .collect()
    });
    KCandidateOutcome {
        code: code.into(),
        state: KCandidateState::Unverifiable,
        observations,
        reason: Some(reason),
        examined_source_files: 0,
    }
}

/// Scan one exact target commit against its exact first parent. `NoSignal`
/// means only that a supported candidate was absent, never a Decisions Hold.
pub fn scan_k_history_pair(repo: &Path, baseline: &str, target: &str) -> Vec<KCandidateOutcome> {
    if !exact_first_parent(repo, baseline, target) {
        return unavailable_all("full adjacent first-parent commit identities are unavailable");
    }
    let (Some(before), Some(after), Some(changed)) = (
        tree(repo, baseline),
        tree(repo, target),
        changed_paths(repo, baseline, target),
    ) else {
        return unavailable_all("exact tracked tree or diff census is unavailable or over budget");
    };
    let expected = before
        .iter()
        .filter(|(path, entry)| after.get(*path) != Some(*entry))
        .map(|(path, _)| path.clone())
        .chain(
            after
                .keys()
                .filter(|path| !before.contains_key(*path))
                .cloned(),
        )
        .collect::<BTreeSet<_>>();
    if expected != changed {
        return unavailable_all("Git diff paths disagree with exact tree objects");
    }
    CODES
        .iter()
        .map(|code| match *code {
            "K1" => scan_k1(repo, &changed, &before, &after),
            "K2" => scan_k2(&before, &after),
            "K3" => record_gap(
                code,
                &changed,
                "complete assertion and strength parser receipts are absent",
            ),
            "K8" | "K9" => record_gap(
                code,
                &changed,
                "complete workflow parser receipts are absent",
            ),
            "K13" | "K17" => record_gap(
                code,
                &changed,
                "exact edit actor and verified person-action receipts are absent",
            ),
            "K14" => record_gap(
                code,
                &changed,
                "exact-tree masked credential scan receipt is absent",
            ),
            "K15" => record_gap(
                code,
                &changed,
                "complete parsed agent-command census is absent from Git",
            ),
            "K16" => record_gap(
                code,
                &changed,
                "tests-pass claim and bound exact-tree run receipts are absent",
            ),
            _ => unavailable(code, "history candidate kind is unsupported"),
        })
        .collect()
}

/// Inspect at most 200 adjacent first-parent pairs, newest target first.
pub fn scan_k_history_first_parent(
    repo: &Path,
    head: &str,
    max_pairs: usize,
) -> Result<Vec<KHistoryPair>, &'static str> {
    if max_pairs > MAX_HISTORY_PAIRS || !exact_commit(repo, head) {
        return Err("history request is over 200 pairs or head is not a full commit");
    }
    let max_count = (max_pairs + 1).to_string();
    let raw = git_text(
        repo,
        &[
            "rev-list",
            "--first-parent",
            "--max-count",
            &max_count,
            head,
        ],
        16 * 1024,
    )
    .ok_or("first-parent history is unavailable")?;
    let commits = raw.lines().collect::<Vec<_>>();
    if commits.first() != Some(&head) || commits.iter().any(|commit| !full_oid(commit)) {
        return Err("first-parent history identities are incomplete");
    }
    Ok(commits
        .windows(2)
        .map(|pair| KHistoryPair {
            baseline: pair[1].into(),
            target: pair[0].into(),
            outcomes: scan_k_history_pair(repo, pair[1], pair[0]),
        })
        .collect())
}
