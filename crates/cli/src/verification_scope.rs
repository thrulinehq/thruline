//! Bounded, explicit input manifests and private source copies. No isolation claim.
use crate::acceptance_exec::{self as exec, Inputs};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, process::Command};
use thruline_policy::acceptance::{Check, PackPlan};
pub const MAX_FILES: usize = 4096;
pub const MAX_BYTES: u64 = 128 * 1024 * 1024;

fn rust_manifest_inputs(repo: &Path, check: &Check) -> Result<Vec<String>> {
    // Reuse the owned Cargo collector to discover the exact declared closure,
    // including ignored workspace members and path dependencies. This avoids
    // traversing unrelated node_modules/target trees to scope a package graph.
    // It never imports or executes the examined project.
    let store = tempfile::tempdir()?;
    let mut command = Command::new(&check.argv[0]);
    command
        .current_dir(repo)
        .args([
            "-I",
            "-B",
            "-c",
            include_str!("verification_architecture.py"),
        ])
        .arg(serde_json::to_string(
            &check.verification.as_ref().and_then(|v| v.pack.as_ref()),
        )?)
        .arg("--manifest-inventory");
    let report = thruline_capture::bounded::run(
        command,
        std::time::Duration::from_secs(10),
        thruline_capture::redaction::Redactor::from_repo(repo)?,
        store.path(),
    )?;
    anyhow::ensure!(
        report.status.success() && !report.timed_out && !report.cancelled,
        "Cargo input inventory did not complete within 10 seconds"
    );
    let output = report
        .stdout
        .context("Cargo input inventory was not captured")?;
    anyhow::ensure!(
        output.complete
            && output.captured.redactions_applied == 0
            && !output.captured.invalid_utf8
            && !output.captured.block_cap_hit
            && output.captured.store_error.is_none(),
        "Cargo input inventory is incomplete or redacted"
    );
    let bytes = std::fs::read(
        output
            .captured
            .stored_path
            .context("Cargo input inventory bytes unavailable")?,
    )?;
    let value: Value = serde_json::from_slice(&bytes)?;
    anyhow::ensure!(
        value["protocol"] == "thruline-cargo-inputs/1",
        "Cargo input inventory protocol is unsupported"
    );
    let files = value["files"]
        .as_array()
        .context("Cargo input closure could not be established")?;
    anyhow::ensure!(
        files.len() <= 513,
        "Cargo input inventory exceeds 513 manifests"
    );
    files
        .iter()
        .map(|file| {
            let path = file
                .as_str()
                .context("Cargo input inventory path is invalid")?;
            anyhow::ensure!(
                Path::new(path).is_relative()
                    && Path::new(path)
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_)))
                    && path.ends_with("Cargo.toml"),
                "Cargo input inventory path escapes the project"
            );
            Ok(path.to_string())
        })
        .collect()
}

pub fn tree_files(repo: &Path, tree: &str) -> Result<Vec<String>> {
    let mut command = Command::new("git");
    thruline_core::git_isolation::isolate_config(&mut command);
    let out = command
        .current_dir(repo)
        .args(["ls-tree", "-r", "-z", "--name-only", tree])
        .output()?;
    anyhow::ensure!(
        out.status.success() && out.stdout.len() <= 1024 * 1024,
        "source inventory is unavailable or exceeds 1 MiB"
    );
    let paths: Vec<_> = String::from_utf8(out.stdout)?
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    anyhow::ensure!(
        paths.len() <= MAX_FILES,
        "source inventory exceeds 4096 files; select a smaller local project"
    );
    Ok(paths)
}
pub fn role(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.ends_with(".lock")
        || matches!(
            name,
            "package-lock.json" | "go.sum" | "uv.lock" | "Pipfile.lock"
        )
    {
        "dependency lockfile"
    } else if path.contains("/tests/")
        || path.starts_with("tests/")
        || name.starts_with("test_")
        || name.contains(".test.")
    {
        "test"
    } else if path.contains(".thruline/")
        || matches!(
            name,
            "pyproject.toml"
                | "package.json"
                | "Cargo.toml"
                | "pytest.ini"
                | "setup.cfg"
                | "tsconfig.json"
                | ".gitignore"
                | ".thrulineignore"
        )
    {
        "configuration"
    } else {
        "source or declared dependency"
    }
}
pub fn stamp(path: &Path) -> Result<Value> {
    let m = std::fs::metadata(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(json!([
            m.len(),
            m.dev().to_string(),
            m.ino().to_string(),
            m.mtime(),
            m.mtime_nsec(),
            m.ctime(),
            m.ctime_nsec()
        ]))
    }
    #[cfg(not(unix))]
    {
        Ok(json!([
            m.len(),
            m.modified()?
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
                .to_string()
        ]))
    }
}
pub fn manifest(repo: &Path, tree: &str, check: &Check) -> Result<Value> {
    let v = check
        .verification
        .as_ref()
        .context("scope declaration missing")?;
    let mut paths = tree_files(repo, tree)?;
    let rust_root = match v.pack.as_ref() {
        Some(PackPlan::Architecture {
            root,
            graph: Some(thruline_policy::acceptance::ArchitectureGraph::RustPackages),
            ..
        }) => Some(root),
        _ => None,
    };
    if rust_root.is_some() {
        for path in rust_manifest_inputs(repo, check)? {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    for p in &v.scope {
        if repo.join(p).is_file() && !paths.contains(p) {
            paths.push(p.clone());
        }
        // An accepted directory means its actual files, including ignored
        // source. Git alone must not hide an offending import from this scope.
        if repo.join(p).is_dir() && rust_root != Some(p) {
            let mut todo = vec![repo.join(p)];
            let mut visited = 0;
            while let Some(dir) = todo.pop() {
                for entry in std::fs::read_dir(dir)? {
                    let entry = entry?;
                    if matches!(entry.file_name().to_str(), Some(".git" | "__pycache__")) {
                        continue;
                    }
                    visited += 1;
                    anyhow::ensure!(
                        visited <= MAX_FILES,
                        "declared directory scope exceeds 4096 entries"
                    );
                    let kind = entry.file_type()?;
                    anyhow::ensure!(
                        !kind.is_symlink(),
                        "declared directory contains a symlink; scope cannot be closed"
                    );
                    if kind.is_dir() {
                        todo.push(entry.path());
                    } else if kind.is_file() {
                        let name = entry
                            .path()
                            .strip_prefix(repo)?
                            .components()
                            .filter_map(|c| match c {
                                std::path::Component::Normal(n) => {
                                    Some(n.to_string_lossy().into_owned())
                                }
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("/");
                        if !paths.contains(&name) {
                            paths.push(name);
                        }
                    } else {
                        anyhow::bail!("declared directory contains an unsupported special file");
                    }
                }
            }
        }
    }
    // Common root configuration is relevant even if ignored by Git.
    for p in [
        "package-lock.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "Cargo.lock",
        "uv.lock",
        "poetry.lock",
        "Pipfile.lock",
        "requirements.txt",
        "pyproject.toml",
        "package.json",
        "Cargo.toml",
        "pytest.ini",
        "setup.cfg",
        "tsconfig.json",
        ".thruline/acceptance.json",
    ] {
        if repo.join(p).is_file() && !paths.iter().any(|x| x == p) {
            paths.push(p.into());
        }
    }
    for p in &v.environment_files {
        if !paths.contains(p) {
            paths.push(p.clone());
        }
    }
    anyhow::ensure!(
        paths.len() <= MAX_FILES + 256,
        "declared input count exceeds scope limit"
    );
    let mut files = BTreeMap::new();
    let mut total = 0;
    for name in paths {
        let path = repo.join(&name);
        // Symlinks escaping the captured source cannot establish a closed copy.
        let resolved = path.canonicalize()?;
        anyhow::ensure!(
            resolved.starts_with(repo) || v.environment_files.contains(&name),
            "undeclared external symlink input: {name}"
        );
        let before = stamp(&resolved)?;
        total += std::fs::metadata(&resolved)?.len();
        anyhow::ensure!(
            total <= MAX_BYTES,
            "declared inputs exceed 128 MiB materialization budget"
        );
        let digest = exec::file_digest(&resolved, 16 * 1024 * 1024)?;
        anyhow::ensure!(
            stamp(&resolved)? == before,
            "input changed while fingerprinting: {name}"
        );
        files.insert(name.clone(),json!({"sha256":digest,"stamp":before,"role":role(&name),"external":!resolved.starts_with(repo)}));
    }
    Ok(
        json!({"files":files,"bytes":total,"closure":"explicit local declaration; unknown external services and undeclared dependencies cannot support broader claims","declared_complete":v.environment_closed,"environment":v.environment_description}),
    )
}

pub struct SourceCopy {
    pub directory: tempfile::TempDir,
    baseline: BTreeMap<String, Value>,
}
fn copy_inventory(root: &Path, check: &Check) -> Result<BTreeMap<String, Value>> {
    let mut todo = vec![root.to_path_buf()];
    let mut out = BTreeMap::new();
    let mut bytes = 0;
    while let Some(dir) = todo.pop() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(root)?.to_string_lossy().into_owned();
            if entry.file_name() == "__pycache__" || check.artifacts.iter().any(|a| a.path == rel) {
                continue;
            }
            let kind = entry.file_type()?;
            anyhow::ensure!(
                !kind.is_symlink(),
                "source copy contains a symlink; consistent read cannot be established: {rel}"
            );
            if kind.is_dir() {
                todo.push(path);
                continue;
            }
            bytes += entry.metadata()?.len();
            anyhow::ensure!(
                out.len() < MAX_FILES + 256 && bytes <= MAX_BYTES,
                "execution copy exceeded its input inventory budget"
            );
            out.insert(
                rel,
                json!({"sha256":exec::file_digest(&path,16*1024*1024)?,"stamp":stamp(&path)?}),
            );
        }
    }
    Ok(out)
}
impl SourceCopy {
    pub fn prepare(repo: &Path, before: &Inputs, check: &Check) -> Result<Self> {
        let directory = tempfile::tempdir()?;
        // Reuse the audited blob materializer, without smudge filters.
        let m = thruline_reexec::materialize::materialize(repo, &before.tree, directory.path())?;
        anyhow::ensure!(
            m.complete()
                && m.symlinks.is_empty()
                && m.files.len() <= MAX_FILES
                && m.bytes <= MAX_BYTES,
            "source tree cannot be materialized completely within the local budget"
        );
        let manifest = before
            .scope_manifest
            .as_ref()
            .context("source manifest missing")?;
        for (name, entry) in manifest["files"]
            .as_object()
            .context("file manifest missing")?
        {
            let path = repo.join(name).canonicalize()?;
            if !path.starts_with(repo) {
                continue;
            }
            let rel = path.strip_prefix(repo)?;
            let dst = directory.path().join(rel);
            if !dst.exists() {
                if let Some(p) = dst.parent() {
                    std::fs::create_dir_all(p)?;
                }
                std::fs::copy(&path, &dst)?;
            }
            anyhow::ensure!(
                exec::file_digest(&dst, 16 * 1024 * 1024)?
                    == entry["sha256"].as_str().unwrap_or(""),
                "source copy differs from examined input: {name}"
            );
        }
        for a in &check.artifacts {
            let output = directory.path().join(&a.path);
            if output.exists() {
                std::fs::remove_file(output)?;
            }
        }
        let baseline = copy_inventory(directory.path(), check)?;
        Ok(Self {
            directory,
            baseline,
        })
    }
    pub fn unchanged(&self, check: &Check) -> Result<bool> {
        Ok(copy_inventory(self.directory.path(), check)? == self.baseline)
    }
    pub fn command(&self, repo: &Path, before: &Inputs, check: &Check) -> Result<Command> {
        let map = |s: &str| -> Result<String> {
            let p = Path::new(s);
            if p.is_absolute() && p.starts_with(repo) {
                return Ok(self
                    .directory
                    .path()
                    .join(p.strip_prefix(repo)?)
                    .to_string_lossy()
                    .into_owned());
            }
            anyhow::ensure!(
                !s.contains(&repo.to_string_lossy().to_string()),
                "embedded source path cannot be safely remapped; use relative accepted arguments"
            );
            Ok(s.to_string())
        };
        let mut c = Command::new(map(&before.executable)?);
        c.args(
            check
                .argv
                .iter()
                .skip(1)
                .map(|s| map(s))
                .collect::<Result<Vec<_>>>()?,
        )
        .current_dir(
            self.directory
                .path()
                .join(Path::new(&before.cwd).strip_prefix(repo)?),
        );
        Ok(c)
    }
}

pub fn completion_dependencies(repo: &Path, db: &Path, check: &Check) -> Result<Option<String>> {
    if !matches!(
        check.verification.as_ref().and_then(|v| v.pack.as_ref()),
        Some(PackPlan::Completion)
    ) {
        return Ok(None);
    }
    if !db.exists() {
        return Ok(Some("no observed runs".into()));
    }
    let l = thruline_ledger::Ledger::open_read_only(db)?;
    let repo_id = thruline_ledger::RepoId::from_key(&repo.to_string_lossy());
    let mut q=l.conn().prepare("SELECT e.session_id,e.payload,(SELECT hash FROM events tail WHERE tail.session_id=e.session_id ORDER BY tail.seq DESC LIMIT 1) FROM events e JOIN sessions s ON s.id=e.session_id WHERE s.repo_id=?1 AND s.harness='acceptance-local-v1' AND e.seq=1 ORDER BY e.rowid DESC LIMIT 4097")?;
    let rows = q
        .query_map([repo_id.as_str()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    anyhow::ensure!(
        rows.len() <= 4096,
        "completion evidence history exceeds reader bound"
    );
    let mut latest = BTreeMap::new();
    for (run, row, head) in rows {
        let v: Value = serde_json::from_str(&row)?;
        if v["check_id"] != check.id {
            if let Some(id) = v["check_id"].as_str() {
                latest
                    .entry(id.to_string())
                    .or_insert(json!({"run_id":run,"head":head,"start":v}));
            }
        }
    }
    Ok(Some(thruline_core::chain::sha256_hex(&serde_json::to_vec(
        &latest,
    )?)))
}
