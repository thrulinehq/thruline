//! BUILD-PLAN C.7's **S3 — declared environment**, and §8.1's
//! `capture.git_env_digest`.
//!
//! > *Every snapshot records a `git_env_digest` over: the effective attribute
//! > sources (in-tree `.gitattributes`, `$GIT_DIR/info/attributes`,
//! > `core.attributesFile`), all `filter.*` config,
//! > `core.autocrlf`/`safecrlf`/`eol`/`precomposeunicode`/`symlinks`/
//! > `ignorecase`/`fsmonitor`/`sparseCheckout`/`excludesFile`, the content hash
//! > of `.git/info/exclude`, and the `.gitmodules` set. **The digest goes in
//! > the receipt** (§8.1 `capture`), so a verifier on another machine can see
//! > the conditions the evidence was produced under.*
//!
//! # What this is for, stated plainly
//!
//! S1 and S2 detect that the tree hash stopped naming the worktree. S3 does not
//! detect anything. It **records the conditions**, so that a reviewer holding
//! only the receipt can see that the evidence was produced under a repository
//! configured a particular way — and can see that it *changed* mid-session,
//! which C.7 makes a `warn`, or a `fail` when the change is to the measurement
//! apparatus itself.
//!
//! A meaningful part of C.7's attack surface (rows 6, 7, 10, 11) is **outside
//! the tree**, so no diff-based detector can see it. The digest is how those
//! rows reach a receipt at all.
//!
//! # Everything here is read-only
//!
//! `config --list`, `rev-parse`, `ls-tree`, `cat-file`. A source-walking test
//! (`crates/core/tests/spec_git_env_verbs.rs`) forbids every mutating verb —
//! including the two §4.4 permits in `snapshot.rs`, because nothing here needs
//! them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

/// The config keys whose values change what a tree hash means.
///
/// Ordered as C.7 lists them. Anything not here is deliberately out: the digest
/// has to be **stable across irrelevant configuration**, or a receipt would
/// carry a different digest on every machine and the field would say nothing.
const WATCHED_KEYS: &[&str] = &[
    "core.autocrlf",
    "core.safecrlf",
    "core.eol",
    "core.precomposeunicode",
    "core.symlinks",
    "core.ignorecase",
    "core.fsmonitor",
    "core.sparsecheckout",
    "core.excludesfile",
    "core.attributesfile",
    "core.hookspath",
    "core.quotepath",
];

/// The recorded git environment, and its digest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitEnv {
    /// `sha256:…` over the RFC 8785 canonicalisation of everything below.
    pub digest: String,
    /// `filter.<name>.<clean|smudge|process|required>` → value. **Values, not
    /// just names**: a filter whose command changed is a different filter, and
    /// the whole point of C.7 row 1 is that the command is arbitrary code.
    pub filters: BTreeMap<String, String>,
    /// [`WATCHED_KEYS`] that are set.
    pub config: BTreeMap<String, String>,
    /// Attribute sources and the sha256 of their contents, so a changed
    /// `.gitattributes` changes the digest even when its *set* of filters does
    /// not.
    pub attribute_sources: BTreeMap<String, String>,
    /// `sha256` of `$GIT_DIR/info/exclude`, which is per-clone, never
    /// committed, and can hide a path from the tree with no diff at all
    /// (C.7 row 6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_hash: Option<String>,
    /// Submodule paths from `.gitmodules` (C.7 row 8): each one is worktree
    /// content the parent tree cannot see.
    pub submodules: Vec<String>,
    /// Whether the clone is shallow or sparse — both mean paths are absent
    /// from the worktree for reasons no diff records.
    pub shallow: bool,
    pub sparse: bool,
    /// Sources named but not readable. Recorded rather than dropped: a
    /// `.gitattributes` this build could not read is not the same fact as one
    /// that declares nothing (Rule 12).
    pub unreadable: Vec<String>,
}

impl GitEnv {
    /// Does the environment carry anything that can make the tree hash stop
    /// naming the worktree?
    ///
    /// Advisory. The *detection* is S1/S2 in `snapshot.rs`; this is the
    /// standing `warn` C.7 attaches to a non-empty filter set.
    pub fn has_content_transform(&self) -> bool {
        self.filters
            .keys()
            .any(|k| k.ends_with(".clean") || k.ends_with(".smudge") || k.ends_with(".process"))
    }
}

/// Read the environment and digest it.
///
/// Never fails on a missing piece: a repository with no `.gitattributes`, no
/// `info/exclude` and no submodules has a perfectly good digest over the empty
/// record. It fails only when `git` itself cannot be run, which is the same
/// condition `snapshot_tree` already reports.
pub fn git_env(repo_root: &Path) -> Result<GitEnv, GitEnvError> {
    let root = repo_root.to_path_buf();
    let mut env = GitEnv::default();

    // --- config -----------------------------------------------------------
    //
    // `--list` rather than one `--get` per key: the effective value across
    // system, global, local and worktree scopes, in one process.
    let listing = git(&root, &["config", "--list", "-z"])?;
    for entry in listing.split('\0').filter(|e| !e.is_empty()) {
        // `key\nvalue` under `-z`.
        let (key, value) = match entry.split_once('\n') {
            Some((k, v)) => (k, v),
            // A key with no value (`[section] key`) is `true` to git.
            None => (entry, "true"),
        };
        let key = key.to_ascii_lowercase();
        if key.starts_with("filter.") {
            env.filters.insert(key, value.to_string());
        } else if WATCHED_KEYS.contains(&key.as_str()) {
            env.config.insert(key, value.to_string());
        }
    }
    env.sparse = env
        .config
        .get("core.sparsecheckout")
        .is_some_and(|v| v == "true");

    // --- attribute sources -------------------------------------------------
    let git_dir = git(&root, &["rev-parse", "--absolute-git-dir"])
        .map(|s| PathBuf::from(s.trim()))
        .unwrap_or_else(|_| root.join(".git"));

    // In-tree, from the worktree rather than from a tree object: this function
    // describes the CONDITIONS, and an uncommitted `.gitattributes` is in force
    // right now. An agent installing a laundering filter does not commit it
    // first.
    let tracked = git(
        &root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )
    .unwrap_or_default();
    for path in tracked.split('\0').filter(|p| !p.is_empty()) {
        if path != ".gitattributes" && !path.ends_with("/.gitattributes") {
            continue;
        }
        match std::fs::read(root.join(path)) {
            Ok(bytes) => {
                env.attribute_sources
                    .insert(path.to_string(), crate::chain::sha256_hex(&bytes));
            }
            Err(e) => env.unreadable.push(format!("{path}: {e}")),
        }
    }
    for (label, path) in [
        ("(info/attributes)", git_dir.join("info/attributes")),
        (
            "(core.attributesFile)",
            expand_tilde(
                env.config
                    .get("core.attributesfile")
                    .map(String::as_str)
                    .unwrap_or_default(),
            ),
        ),
    ] {
        if path.as_os_str().is_empty() {
            continue;
        }
        match std::fs::read(&path) {
            Ok(bytes) => {
                env.attribute_sources
                    .insert(label.to_string(), crate::chain::sha256_hex(&bytes));
            }
            // Absent is normal and is not recorded — an absent file and an
            // unreadable one are different facts, and only the second is a gap.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => env.unreadable.push(format!("{}: {e}", path.display())),
        }
    }

    // --- excludes ----------------------------------------------------------
    match std::fs::read(git_dir.join("info/exclude")) {
        Ok(bytes) => env.exclude_hash = Some(crate::chain::sha256_hex(&bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => env.unreadable.push(format!("info/exclude: {e}")),
    }

    // --- submodules ---------------------------------------------------------
    if let Ok(text) = std::fs::read_to_string(root.join(".gitmodules")) {
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("path") {
                if let Some(value) = rest.trim_start().strip_prefix('=') {
                    env.submodules.push(value.trim().to_string());
                }
            }
        }
        env.submodules.sort();
        env.submodules.dedup();
    }

    // --- shallow ------------------------------------------------------------
    env.shallow = git(&root, &["rev-parse", "--is-shallow-repository"])
        .map(|s| s.trim() == "true")
        .unwrap_or(false);

    // --- the digest ---------------------------------------------------------
    //
    // Over the RFC 8785 canonicalisation of the record with `digest` itself
    // empty — the same "hash the object minus its own hash field" shape the
    // event chain uses, so there is one habit rather than two.
    env.digest = String::new();
    let bytes = crate::spec::canonical::canonical_bytes(&env)
        .map_err(|e| GitEnvError::Canonical(e.to_string()))?;
    env.digest = format!("sha256:{}", crate::chain::sha256_hex(&bytes));
    Ok(env)
}

fn expand_tilde(raw: &str) -> PathBuf {
    if raw.is_empty() {
        return PathBuf::new();
    }
    match raw.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home).join(rest),
            None => PathBuf::from(raw),
        },
        None => PathBuf::from(raw),
    }
}

/// A read-only git call. See the module docs on the verb guard.
fn git(root: &Path, args: &[&str]) -> Result<String, GitEnvError> {
    let out = Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .env_remove("GIT_INDEX_FILE")
        .args(args)
        .output()
        .map_err(|e| GitEnvError::Spawn(e.to_string()))?;
    if !out.status.success() {
        return Err(GitEnvError::Git {
            argv: args.join(" "),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[derive(Debug, thiserror::Error)]
pub enum GitEnvError {
    #[error("running git: {0}")]
    Spawn(String),
    #[error("`git {argv}` failed: {stderr}")]
    Git { argv: String, stderr: String },
    #[error("canonicalising the environment record: {0}")]
    Canonical(String),
}
