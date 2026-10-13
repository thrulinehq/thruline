//! C31 future dry-run archive condition for a docs path exclusion.
//! No proof means no exclusion; the caller decides Broken versus Unverifiable.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const PROOF_PATH: &str = "target/distrib/decisions-ship-proof.json";
const RELEASE_VERSION: &str = "v0.2.0";
const MAX_PROOF_BYTES: u64 = 64 * 1024;
const MAX_ARCHIVE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_LIST_BYTES: usize = 4 * 1024 * 1024;
const ARCHIVES: [&str; 4] = [
    "thruline-cli-aarch64-apple-darwin.tar.gz",
    "thruline-cli-x86_64-apple-darwin.tar.gz",
    "thruline-cli-aarch64-unknown-linux-musl.tar.gz",
    "thruline-cli-x86_64-unknown-linux-musl.tar.gz",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveAbsentScope {
    pub pattern: String,
    pub release_version: String,
    pub proof_path: String,
    #[serde(default)]
    pub required_source_line: Option<RequiredSourceLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequiredSourceLine {
    pub path: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveResolution {
    pub patterns: Vec<String>,
    pub missing: Vec<String>,
}

pub fn resolve_archive_absences(
    repo: &Path,
    commit: &str,
    scopes: &[ArchiveAbsentScope],
) -> ArchiveResolution {
    if scopes.is_empty() {
        return ArchiveResolution {
            patterns: Vec::new(),
            missing: Vec::new(),
        };
    }
    if let Err(reason) = validate_scopes(scopes) {
        return ArchiveResolution {
            patterns: Vec::new(),
            missing: scopes
                .iter()
                .map(|scope| format!("{}: {reason}", scope.pattern))
                .collect(),
        };
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    let inventory = verify_inventory(repo, commit, deadline);
    let mut resolution = ArchiveResolution {
        patterns: Vec::new(),
        missing: Vec::new(),
    };
    for scope in scopes {
        let entries = match &inventory {
            Ok(entries) => entries,
            Err(reason) => {
                resolution.missing.push(format!(
                    "{} v0.2.0 dry-run archive condition: {reason}",
                    scope.pattern
                ));
                continue;
            }
        };
        if let Some(marker) = &scope.required_source_line {
            if let Err(reason) = verify_source_line(repo, commit, marker, deadline) {
                resolution.missing.push(format!(
                    "{} v0.2.0 source condition: {reason}",
                    scope.pattern
                ));
                continue;
            }
        }
        if entries
            .iter()
            .any(|entry| archive_path_matches(&scope.pattern, entry))
        {
            resolution.missing.push(format!(
                "{} is present in a v0.2.0 dry-run archive",
                scope.pattern
            ));
            continue;
        }
        resolution.patterns.push(scope.pattern.clone());
    }
    resolution
}

/// Validate the accepted condition without reading mutable archive evidence.
pub fn validate_scopes(scopes: &[ArchiveAbsentScope]) -> Result<(), &'static str> {
    if scopes.len() > 16 {
        return Err("shipping condition has too many path scopes");
    }
    let mut unique = BTreeSet::new();
    for scope in scopes {
        if scope.release_version != RELEASE_VERSION || scope.proof_path != PROOF_PATH {
            return Err("shipping condition needs the named v0.2.0 dry-run proof path");
        }
        if scope.pattern != "docs/**"
            && (!safe_relative(&scope.pattern)
                || !scope.pattern.ends_with(".md")
                || scope.pattern.contains('*'))
        {
            return Err("shipping condition needs one exact Markdown path or docs/**");
        }
        if !unique.insert(&scope.pattern) {
            return Err("shipping condition repeats a path scope");
        }
        if let Some(marker) = &scope.required_source_line {
            if !safe_relative(&marker.path)
                || !marker.path.ends_with(".md")
                || marker.text.is_empty()
                || marker.text.len() > 512
                || marker.text.trim() != marker.text
                || marker.text.chars().any(char::is_control)
            {
                return Err("shipping condition has an invalid exact source line");
            }
        }
    }
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    schema_version: u32,
    release_version: String,
    checked_commit: String,
    source: String,
    archives: Vec<ArchiveProof>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArchiveProof {
    file_name: String,
    sha256: String,
}

fn verify_inventory(
    repo: &Path,
    commit: &str,
    deadline: Instant,
) -> Result<Vec<String>, &'static str> {
    if !git_oid(commit) {
        return Err("checked commit is not one exact Git object id");
    }
    let (path_base, target) = proof_target(repo)?;
    let dist = target.join("distrib");
    let proof_path = dist.join("decisions-ship-proof.json");
    let bytes = read_regular_bounded(&path_base, &proof_path, MAX_PROOF_BYTES)?;
    let proof: Proof = serde_json::from_slice(&bytes)
        .map_err(|_| "v0.2.0 dry-run archive proof is missing or malformed")?;
    if proof.schema_version != 1
        || proof.release_version != RELEASE_VERSION
        || proof.checked_commit != commit
        || proof.source != "private-release-dry-run"
        || proof.archives.len() != ARCHIVES.len()
    {
        return Err("v0.2.0 dry-run archive proof does not bind this commit and four outputs");
    }
    verify_source_package_version(repo, commit, deadline)?;
    // A fifth tarball cannot silently sit beside four named outputs while a
    // four-archive proof claims to describe the complete release set. Other
    // ordinary dist files (checksums, JSON, manifests) are not tar archives.
    for entry in
        fs::read_dir(&dist).map_err(|_| "v0.2.0 dry-run distribution inventory cannot be listed")?
    {
        let entry = entry.map_err(|_| "v0.2.0 dry-run distribution inventory is incomplete")?;
        let file_name = entry
            .file_name()
            .into_string()
            .map_err(|_| "v0.2.0 dry-run distribution has a non-UTF-8 name")?;
        if file_name.ends_with(".tar.gz") && !ARCHIVES.contains(&file_name.as_str()) {
            return Err("v0.2.0 dry-run distribution has an unproved platform archive");
        }
    }
    let mut seen = BTreeSet::new();
    let mut entries = Vec::new();
    for item in &proof.archives {
        if !ARCHIVES.contains(&item.file_name.as_str())
            || !seen.insert(item.file_name.as_str())
            || item.sha256.len() != 64
            || !item
                .sha256
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err("v0.2.0 dry-run archive list has a missing or duplicate output");
        }
        let archive = dist.join(&item.file_name);
        let actual = sha256_regular_archive(&path_base, &archive, deadline)?;
        if actual != item.sha256 {
            return Err("v0.2.0 dry-run archive SHA-256 differs from the proof");
        }
        let listed = tar_listing(&archive, deadline)?;
        // Rehash after tar's read to detect archive replacement during listing.
        if sha256_regular_archive(&path_base, &archive, deadline)? != item.sha256 {
            return Err("v0.2.0 dry-run archive changed while its members were listed");
        }
        entries.extend(listed);
    }
    if seen.len() != ARCHIVES.len() {
        return Err("v0.2.0 dry-run proof omits a platform archive");
    }
    Ok(entries)
}

/// Only this dedicated override relocates mutable shipping evidence. An ambient
/// Cargo build target never changes the rule's default repository proof path.
fn proof_target(repo: &Path) -> Result<(PathBuf, PathBuf), &'static str> {
    let Some(value) = std::env::var_os("THRULINE_SHIP_PROOF_TARGET_DIR") else {
        return Ok((repo.to_path_buf(), repo.join("target")));
    };
    let raw = value.to_str().ok_or("shipping proof target is not UTF-8")?;
    let target = PathBuf::from(raw);
    if !target.is_absolute()
        || raw.len() > 4096
        || raw.chars().any(char::is_control)
        || raw
            .split('/')
            .skip(1)
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Err("shipping proof target must be an absolute ordinary directory");
    }
    let mut current = PathBuf::new();
    for part in target.components() {
        if !matches!(part, Component::RootDir | Component::Normal(_)) {
            return Err("shipping proof target has a nonregular component");
        }
        current.push(part);
        let metadata = fs::symlink_metadata(&current)
            .map_err(|_| "shipping proof target directory is missing")?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("shipping proof target uses a symlink or nonregular directory");
        }
    }
    // Recheck the complete ancestor chain for each proof/archive read, rather
    // than treating the external directory as a trusted repository root.
    Ok((PathBuf::from("/"), target))
}

fn safe_relative(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && !value.contains(['\\', '*', '?', '[', ':'])
        && !value.chars().any(char::is_control)
        && Path::new(value)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        && value
            .split('/')
            .all(|part| !part.is_empty() && !matches!(part, "." | ".."))
}

fn git_oid(value: &str) -> bool {
    (value.len() == 40 || value.len() == 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn read_regular_bounded(repo: &Path, path: &Path, limit: u64) -> Result<Vec<u8>, &'static str> {
    let metadata = check_regular_path(repo, path, limit)?;
    let file = File::open(path).map_err(|_| "v0.2.0 dry-run proof cannot be read")?;
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "v0.2.0 dry-run proof cannot be read")?;
    if bytes.len() as u64 > limit || bytes.len() as u64 != metadata.len() {
        return Err("v0.2.0 dry-run proof changed or exceeds its byte bound");
    }
    Ok(bytes)
}

fn check_regular_path(repo: &Path, path: &Path, limit: u64) -> Result<fs::Metadata, &'static str> {
    if !path.starts_with(repo) {
        return Err("shipping proof path is outside the repository");
    }
    let mut current = repo.to_path_buf();
    let relative = path
        .strip_prefix(repo)
        .map_err(|_| "shipping proof path is outside the repository")?;
    let components = relative.components().collect::<Vec<_>>();
    let mut leaf = None;
    for (index, part) in components.iter().enumerate() {
        if !matches!(part, Component::Normal(_)) {
            return Err("shipping proof path has a nonregular component");
        }
        current.push(part);
        let metadata = fs::symlink_metadata(&current)
            .map_err(|_| "v0.2.0 dry-run archive proof or output is missing")?;
        if metadata.file_type().is_symlink()
            || (index + 1 == components.len() && !metadata.is_file())
            || (index + 1 < components.len() && !metadata.is_dir())
        {
            return Err("v0.2.0 dry-run proof or archive uses a symlink or nonregular path");
        }
        if index + 1 == components.len() && metadata.len() > limit {
            return Err("v0.2.0 dry-run proof or archive exceeds its byte bound");
        }
        #[cfg(unix)]
        if index + 1 == components.len() {
            use std::os::unix::fs::MetadataExt;
            if metadata.nlink() != 1 {
                return Err("v0.2.0 dry-run proof or archive is hard-linked");
            }
        }
        if index + 1 == components.len() {
            leaf = Some(metadata);
        }
    }
    leaf.ok_or("shipping proof path has no regular file")
}

fn sha256_regular_archive(
    repo: &Path,
    path: &Path,
    deadline: Instant,
) -> Result<String, &'static str> {
    // The same regular-path and byte bound is applied before streaming. This
    // is local byte consistency, not a signed attestation of who built it.
    let metadata = check_regular_path(repo, path, MAX_ARCHIVE_BYTES)?;
    let mut file = File::open(path).map_err(|_| "v0.2.0 platform archive cannot be opened")?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut read = 0u64;
    loop {
        if Instant::now() >= deadline {
            return Err("v0.2.0 archive verification exceeded its time bound");
        }
        let amount = file
            .read(&mut buffer)
            .map_err(|_| "v0.2.0 platform archive cannot be read")?;
        if amount == 0 {
            break;
        }
        read += amount as u64;
        if read > MAX_ARCHIVE_BYTES {
            return Err("v0.2.0 platform archive exceeds its byte bound");
        }
        digest.update(&buffer[..amount]);
    }
    if read != metadata.len() {
        return Err("v0.2.0 platform archive changed while read");
    }
    Ok(crate::chain::to_hex(&digest.finalize()))
}

fn tar_listing(path: &Path, deadline: Instant) -> Result<Vec<String>, &'static str> {
    // `head` hard-caps stdout while pipefail makes a truncated tar fail. The
    // fixed shell text receives the archive solely as a positional argument.
    let mut command = Command::new("/bin/bash");
    command
        .args([
            "-o",
            "pipefail",
            "-c",
            "/usr/bin/tar -tzf \"$1\" | /usr/bin/head -c 4194305",
            "c31-archive-list",
        ])
        .arg(path);
    let bytes = match crate::bounded_child::output_until(&mut command, deadline) {
        Ok(crate::bounded_child::Outcome::Finished(output)) if output.status.success() => {
            output.stdout
        }
        _ => return Err("v0.2.0 dry-run archive listing is unavailable or incomplete"),
    };
    if bytes.is_empty() || bytes.len() > MAX_LIST_BYTES {
        return Err("v0.2.0 dry-run archive listing exceeds its byte bound");
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| "v0.2.0 dry-run archive listing has non-UTF-8 paths")?;
    let entries = text
        .lines()
        .map(|line| line.trim_start_matches("./").trim_end_matches('/'))
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if entries.is_empty() {
        return Err("v0.2.0 dry-run archive has no listed members");
    }
    let root = entries[0]
        .split('/')
        .next()
        .ok_or("v0.2.0 dry-run archive has no package root")?;
    if !root.starts_with("thruline")
        || entries.iter().any(|entry| {
            !(entry == root || entry.starts_with(&format!("{root}/")))
                || entry.contains(['\\', '\0', '\r'])
                || entry.chars().any(char::is_control)
                || entry
                    .split('/')
                    .any(|part| part.is_empty() || matches!(part, "." | ".."))
        })
        || !entries
            .iter()
            .any(|entry| entry.rsplit('/').next() == Some("thruline"))
    {
        return Err("v0.2.0 dry-run archive listing has an invalid package member");
    }
    Ok(entries)
}

fn archive_path_matches(pattern: &str, entry: &str) -> bool {
    let components = entry.split('/').skip(1).collect::<Vec<_>>();
    if pattern == "docs/**" {
        components.contains(&"docs")
    } else {
        let exact = pattern.split('/').collect::<Vec<_>>();
        components.ends_with(&exact)
    }
}

fn verify_source_line(
    repo: &Path,
    commit: &str,
    marker: &RequiredSourceLine,
    deadline: Instant,
) -> Result<(), &'static str> {
    let blob = committed_regular_blob(repo, commit, &marker.path, deadline)?;
    let text =
        std::str::from_utf8(&blob).map_err(|_| "exact source marker Git blob is not UTF-8")?;
    if !text.lines().any(|line| {
        let line = line.trim_end_matches('\r').trim_start();
        let hashes = line.bytes().take_while(|b| *b == b'#').count();
        let content = if (1..=6).contains(&hashes) {
            line[hashes..].trim_start()
        } else {
            line
        };
        content == marker.text
    }) {
        return Err("exact committed C20 supersession line is missing");
    }
    Ok(())
}

fn verify_source_package_version(
    repo: &Path,
    commit: &str,
    deadline: Instant,
) -> Result<(), &'static str> {
    let read_manifest = |path| -> Result<toml::Value, &'static str> {
        let bytes = committed_regular_blob(repo, commit, path, deadline)
            .map_err(|_| "source package version manifest is unavailable or nonregular")?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| "source package version manifest is not UTF-8")?;
        toml::from_str::<toml::Value>(text)
            .map_err(|_| "source package version manifest is malformed or ambiguous")
    };
    let root = read_manifest("Cargo.toml")?;
    let expected = RELEASE_VERSION
        .strip_prefix('v')
        .ok_or("source package version cannot be resolved from the accepted release version")?;
    if root
        .get("workspace")
        .and_then(|workspace| workspace.get("package"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        != Some(expected)
    {
        return Err("source package version does not match the accepted release version");
    }
    let cli = read_manifest("crates/cli/Cargo.toml")?;
    if cli.get("workspace").is_some() {
        return Err("source package version CLI has an ambiguous nested workspace");
    }
    let package = cli
        .get("package")
        .and_then(toml::Value::as_table)
        .ok_or("source package version CLI package is missing")?;
    if package.contains_key("workspace") {
        return Err("source package version CLI redirects its workspace");
    }
    if package.get("name").and_then(toml::Value::as_str) != Some("thruline-cli") {
        return Err("source package version does not identify the release CLI");
    }
    let matches = match package.get("version") {
        Some(toml::Value::String(version)) => version == expected,
        Some(toml::Value::Table(inherited)) => {
            inherited.len() == 1
                && inherited.get("workspace").and_then(toml::Value::as_bool) == Some(true)
        }
        _ => false,
    };
    if !matches {
        return Err(
            "source package version CLI is missing, ambiguous, or differs from the release",
        );
    }
    Ok(())
}

fn committed_regular_blob(
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
        .ok_or("exact source marker Git entry is missing")?;
    if &row[tab + 1..] != path.as_bytes()
        || !(row.starts_with(b"100644 blob ") || row.starts_with(b"100755 blob "))
    {
        return Err("exact source marker is not a regular Git file");
    }
    let object = format!("{commit}:{path}");
    let size = git(repo, &["cat-file", "-s", &object], deadline)?;
    let size = std::str::from_utf8(&size)
        .ok()
        .and_then(|text| text.trim().parse::<usize>().ok())
        .ok_or("exact source marker size is unavailable")?;
    if size > 2 * 1024 * 1024 {
        return Err("exact source marker file exceeds its byte bound");
    }
    let blob = git(repo, &["cat-file", "blob", &object], deadline)?;
    if blob.len() != size {
        return Err("exact source marker Git blob length differs");
    }
    Ok(blob)
}

fn git(repo: &Path, args: &[&str], deadline: Instant) -> Result<Vec<u8>, &'static str> {
    let mut command = Command::new("git");
    crate::git_isolation::isolate_config(&mut command)
        .args(args)
        .current_dir(repo);
    match crate::bounded_child::output_until(&mut command, deadline) {
        Ok(crate::bounded_child::Outcome::Finished(output)) if output.status.success() => {
            Ok(output.stdout)
        }
        _ => Err("exact committed source marker cannot be read"),
    }
}
