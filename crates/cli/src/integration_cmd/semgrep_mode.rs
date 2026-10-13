//! Explicit local Semgrep modes and bounded, no-follow ordinary Git closure.
//! Identity reads do not execute Git. Source-snapshot preparation runs only
//! inside the fixed bounded product worker; native analysis uses its pinned container.
use super::*;
use std::path::Component;

pub const CONFIG: &str = ".thruline/semgrep-mode.json";
pub const PROTOCOL: &str = "/thruline/semgrep-modes.py";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub schema: String,
    pub mode: String,
    pub rules: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_commit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_fix_tests: Option<bool>,
}

fn relative(value: &str, dot: bool) -> Result<()> {
    anyhow::ensure!(
        (dot && value == ".") || (!value.is_empty() && !value.contains(['\\', '\0', '\n', '\r'])
        && value.split('/').all(|p| !p.is_empty() && p != "." && p != ".." && p != ".git")
        && Path::new(value).components().all(|p| matches!(p, Component::Normal(_)))),
        "Semgrep selections require bounded project-relative paths; external, Git metadata and alias paths are not targets"
    );
    Ok(())
}

#[cfg(unix)]
fn open_regular(root: &Path, relative: &Path) -> Result<fs::File> {
    use rustix::fs::{open, openat, Mode, OFlags};
    let mut directory = open(
        root,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW,
        Mode::empty(),
    )?;
    let parts: Vec<_> = relative.components().collect();
    anyhow::ensure!(!parts.is_empty(), "missing bound file");
    for (index, part) in parts.iter().enumerate() {
        let Component::Normal(name) = part else {
            bail!("noncanonical bound path")
        };
        let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK;
        if index + 1 == parts.len() {
            let file = fs::File::from(openat(&directory, *name, flags, Mode::empty())?);
            anyhow::ensure!(
                file.metadata()?.is_file(),
                "bound input is not a regular file"
            );
            return Ok(file);
        }
        directory = openat(&directory, *name, flags | OFlags::DIRECTORY, Mode::empty())?;
    }
    bail!("missing file component")
}
#[cfg(not(unix))]
fn open_regular(_root: &Path, _relative: &Path) -> Result<fs::File> {
    bail!("Semgrep modes require a qualified directory-relative source reader")
}

fn read(root: &Path, path: &str, limit: u64) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut file = open_regular(root, Path::new(path))?;
    anyhow::ensure!(
        file.metadata()?.len() <= limit,
        "Semgrep input exceeds its explicit byte budget"
    );
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= limit,
        "Semgrep input grew beyond its byte budget"
    );
    Ok(bytes)
}

fn selected_path(root: &Path, value: &str, directory: bool) -> Result<PathBuf> {
    relative(value, false)?;
    let mut current = root.to_path_buf();
    for part in Path::new(value).components() {
        current.push(part);
        let metadata = fs::symlink_metadata(&current)?;
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "Semgrep selection crosses a symlink"
        );
    }
    anyhow::ensure!(
        if directory {
            current.is_dir()
        } else {
            current.is_file()
        },
        "Semgrep selected path has the wrong kind"
    );
    Ok(current)
}

pub fn configuration(root: &Path) -> Result<Option<Selection>> {
    match fs::symlink_metadata(root.join(CONFIG)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(metadata) => anyhow::ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Semgrep mode configuration must be a regular file"
        ),
    }
    let value = engine::parse_json(&read(root, CONFIG, 16384)?)?;
    let keys: std::collections::BTreeSet<&str> = value
        .as_object()
        .context("mode configuration must be an object")?
        .keys()
        .map(String::as_str)
        .collect();
    let mut expected = std::collections::BTreeSet::from(["schema", "mode", "rules"]);
    match value["mode"].as_str() {
        Some("targeted") => {
            expected.insert("targets");
        }
        Some("baseline") => {
            expected.extend(["targets", "baseline_commit"]);
        }
        Some("rule-test") => {
            expected.extend(["tests", "require_fix_tests"]);
        }
        _ => bail!("unsupported Semgrep mode"),
    }
    anyhow::ensure!(
        keys == expected,
        "Semgrep mode configuration has missing or unrelated fields"
    );
    let selected: Selection = serde_json::from_value(value)?;
    anyhow::ensure!(
        selected.schema == "thruline.semgrep-mode.v1",
        "unsupported Semgrep mode schema"
    );
    match selected.mode.as_str() {
        "targeted" | "baseline" => {
            selected_path(root, &selected.rules, false)?;
            anyhow::ensure!(
                selected.tests.is_none()
                    && selected.require_fix_tests.is_none()
                    && !selected.targets.is_empty()
                    && selected.targets.len() <= 32,
                "scan mode requires only explicit local rules and 1–32 targets"
            );
            let mut distinct = std::collections::BTreeSet::new();
            for target in &selected.targets {
                relative(target, true)?;
                anyhow::ensure!(distinct.insert(target), "repeated Semgrep target");
                if target != "." {
                    let file = root.join(target);
                    selected_path(root, target, file.is_dir())?;
                }
            }
            if selected.mode == "baseline" {
                let baseline = selected
                    .baseline_commit
                    .as_deref()
                    .context("baseline mode requires an exact ancestor commit")?;
                anyhow::ensure!(
                    baseline.len() == 40
                        && baseline
                            .bytes()
                            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                    "baseline must be an exact lowercase SHA-1 commit, not a movable branch name"
                );
            } else {
                anyhow::ensure!(
                    selected.baseline_commit.is_none(),
                    "targeted mode cannot select a baseline"
                );
            }
        }
        "rule-test" => {
            selected_path(root, &selected.rules, true)?;
            selected_path(
                root,
                selected
                    .tests
                    .as_deref()
                    .context("rule-test requires a fixture directory")?,
                true,
            )?;
            anyhow::ensure!(selected.targets.is_empty() && selected.baseline_commit.is_none() && selected.require_fix_tests==Some(true),
                "the qualified rule-test mode requires explicit fixed-fixture assertions and no scan targets or baseline");
        }
        _ => bail!("select targeted, baseline or rule-test Semgrep mode"),
    }
    Ok(Some(selected))
}

pub fn is_plan(plan: &Plan) -> bool {
    plan.pack == "semgrep"
        && plan.format == "semgrep-json-v1"
        && plan.container.is_some()
        && matches!(plan.argv.len(), 5 | 7)
        && plan.argv[..5]
            == [
                "/opt/python/bin/python",
                "-I",
                PROTOCOL,
                "--thruline-mode",
                CONFIG,
            ]
        && (plan.argv.len() == 5
            || (plan.argv[5] == "--git-identity"
                && plan.argv[6].len() == 64
                && plan.argv[6]
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())))
}

/// Source rules/fixtures never use the more permissive external-runtime alias reader.
pub fn rules_identity(plan: &Plan) -> Result<String> {
    let mut hashes = BTreeMap::new();
    let mut total = 0u64;
    for selected in &plan.rules {
        let relative = selected
            .strip_prefix(&plan.root)
            .context("mode rule input is outside its project")?;
        for entry in walkdir::WalkDir::new(selected).follow_links(false) {
            let entry = entry?;
            anyhow::ensure!(entry.depth() <= 32, "mode rule input depth exceeded");
            if entry.file_type().is_dir() {
                continue;
            }
            anyhow::ensure!(
                entry.file_type().is_file(),
                "mode rules/fixtures contain a symlink or special file"
            );
            let path = entry.path().strip_prefix(&plan.root)?;
            let name = path.to_str().context("rule path is not UTF-8")?;
            let data = read(&plan.root, name, 16 * 1024 * 1024)?;
            total += data.len() as u64;
            anyhow::ensure!(
                hashes.len() < 8192 && total <= 128 * 1024 * 1024,
                "mode rule/fixture input budget exceeded"
            );
            hashes.insert(name.to_owned(), sha256_hex(&data));
        }
        anyhow::ensure!(
            !relative.as_os_str().is_empty(),
            "project root cannot be an implicit rule input"
        );
    }
    if let Some(reference) = plan.environment.get(PREPARATION_ENV) {
        let reference = engine::parse_json(reference.as_bytes())?;
        verified_preparation(&reference)?;
        hashes.insert(
            "@bounded-source-snapshot".into(),
            reference["sha256"]
                .as_str()
                .context("preparation digest missing")?
                .into(),
        );
    }
    hash_json(&hashes)
}

const PREPARATION_ENV: &str = "THRULINE_SEMGREP_PREPARATION";

pub fn snapshot_child(root: &Path) -> Result<Value> {
    let root = root.canonicalize()?;
    snapshot_configuration(&root)?;
    let snapshot = thruline_core::spec::snapshot::snapshot_tree(&root)?;
    anyhow::ensure!(
        !snapshot.content_filters.degrades_capture() && snapshot.omitted.is_empty(),
        "Baseline source snapshot is incomplete or filtered"
    );
    Ok(
        json!({"schema":"thruline.semgrep-snapshot-child.v1","tree_hash":snapshot.tree_hash,
        "head":snapshot.head_commit,"root":root,"complete":true}),
    )
}

fn source_identity(root: &Path) -> Result<String> {
    let mut hashes = BTreeMap::new();
    let mut total = 0u64;
    let entries = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || e.file_name() != ".git");
    for (index, entry) in entries.enumerate() {
        let entry = entry?;
        anyhow::ensure!(
            index < 8192 && entry.depth() <= 32,
            "snapshot source inventory exceeds bounded scope"
        );
        if entry.file_type().is_dir() {
            continue;
        }
        anyhow::ensure!(
            entry.file_type().is_file(),
            "snapshot source contains a symlink or special file"
        );
        let relative = entry
            .path()
            .strip_prefix(root)?
            .to_str()
            .context("source path is not UTF-8")?;
        let bytes = read(root, relative, 16 * 1024 * 1024)?;
        total += bytes.len() as u64;
        anyhow::ensure!(
            total <= 128 * 1024 * 1024,
            "snapshot source bytes exceed128MiB"
        );
        hashes.insert(relative.to_owned(), sha256_hex(&bytes));
    }
    hash_json(&hashes)
}

fn git_runtime_identity() -> Result<Value> {
    let mut files = Vec::new();
    for value in [
        "/usr/bin/git",
        "/Applications/Xcode.app/Contents/Developer/usr/bin/git",
    ] {
        let path = Path::new(value);
        if path.is_file() {
            files.push(json!({"path":path.canonicalize()?,"sha256":hash_file(path)?}));
        }
    }
    anyhow::ensure!(
        !files.is_empty(),
        "native Git executable identity unavailable"
    );
    Ok(
        json!({"files":files,"scope":"System Git launcher and selected Xcode Git executable bytes; operating-system libraries remain the existing worker platform boundary, not a fully redistributed Git runtime."}),
    )
}

pub(super) fn verified_preparation(reference: &Value) -> Result<Value> {
    let path = Path::new(
        reference["path"]
            .as_str()
            .context("snapshot preparation path missing")?,
    );
    let bytes = bounded_file(path, 2 * 1024 * 1024)?;
    anyhow::ensure!(
        reference["sha256"] == sha256_hex(&bytes),
        "Snapshot preparation evidence changed"
    );
    let value = engine::parse_json(&bytes)?;
    anyhow::ensure!(
        value["schema"] == "thruline.semgrep-snapshot-preparation.v1" && value["complete"] == true,
        "Snapshot preparation did not complete"
    );
    Ok(value)
}

/// Materialize the existing product snapshot under the native worker boundary,
/// then cache only its immutable proof at the exact source/Git after-identity.
/// Every read checks source and Git bytes again; this is not a freshness cache.
pub fn prepare_snapshot(db: &Path, root: &Path) -> Result<Value> {
    snapshot_configuration(root)?;
    let before_git = git_identity(root)?;
    let source = source_identity(root)?;
    let store = artifact_dir(db);
    fs::create_dir_all(&store)?;
    let cache = store.join("semgrep-snapshot-index");
    fs::create_dir_all(&cache)?;
    let exe = std::env::current_exe()?.canonicalize()?;
    let candidate_sha = hash_file(&exe)?;
    let git_runtime = git_runtime_identity()?;
    let git_bin = if Path::new("/Applications/Xcode.app/Contents/Developer/usr/bin/git").is_file() {
        "/Applications/Xcode.app/Contents/Developer/usr/bin:/usr/bin:/bin"
    } else {
        "/usr/bin:/bin"
    };
    let key = hash_json(&(root, &source, &before_git, &candidate_sha, &git_runtime))?;
    let index = cache.join(format!("{key}.json"));
    if index.exists() {
        let reference = engine::parse_json(&bounded_file(&index, 16384)?)?;
        let value = verified_preparation(&reference)?;
        anyhow::ensure!(
            value["root"] == json!(root)
                && value["source_identity"] == source
                && value["after_git_identity"] == before_git
                && value["candidate_sha256"] == candidate_sha
                && value["git_runtime"] == git_runtime,
            "Snapshot proof no longer matches the selected source/Git closure"
        );
        return Ok(reference);
    }
    let exe = std::env::current_exe()?.canonicalize()?;
    let plan=Plan{schema:"thruline.integration-plan.v1".into(),pack:"semgrep-snapshot".into(),format:"thruline.semgrep-snapshot-preparation.v1".into(),support:None,root:root.to_owned(),
        argv:vec![exe.display().to_string(),"--db".into(),"/dev/null".into(),"integration".into(),"semgrep-snapshot".into(),"--root".into(),root.display().to_string()],
        container:None,local_model_port:None,writable_paths:vec![],criterion:Criterion{id:"semgrep-source-snapshot".into(),revision:"v1".into(),requirement:"Materialize the bounded selected source tree before binding its Git object closure".into(),success:"Source/index/refs/config unchanged; snapshot complete inside declared worker filesystem/network scope".into(),accepted_by:"explicit local baseline selection".into(),condition:engine::Condition::NoObservations},
        acceptance:None,approved_setup:None,rules:vec![],runtime:vec![],dependencies:vec![],dataset:vec![],environment:BTreeMap::from([("PATH".into(),git_bin.into()),("HOME".into(),"/dev/null".into()),("GIT_CONFIG_NOSYSTEM".into(),"1".into()),("GIT_CONFIG_GLOBAL".into(),"/dev/null".into())]),input_closure_complete:true,timeout_seconds:30,output_limit_bytes:1024*1024,cpu_seconds:20,memory_mb:512,report:ReportSource::Stdout};
    let mut prepared = worker::prepare_semgrep_snapshot(&plan)?;
    let actual = prepared.argv.clone();
    let before_metadata = metadata_identity(root)?;
    let outcome = worker::execute(&plan, &mut prepared, |_| Ok(()))?;
    let after_git = git_identity(root);
    let after_source = source_identity(root);
    let after_metadata = metadata_identity(root);
    let complete = outcome.state == RunState::Completed
        && outcome.exit_code == Some(0)
        && outcome.stdout.complete
        && outcome.stderr.complete
        && after_source.as_ref().ok() == Some(&source)
        && after_metadata.as_ref().ok() == Some(&before_metadata)
        && after_git.is_ok();
    let value = json!({"schema":"thruline.semgrep-snapshot-preparation.v1","root":root,"complete":complete,"source_identity":source,"before_git_identity":before_git,"after_git_identity":after_git.as_ref().ok(),"metadata_identity_before":before_metadata,"metadata_identity_after":after_metadata.as_ref().ok(),"argv":actual,"worker_state":outcome.state,"exit_code":outcome.exit_code,"duration_ms":outcome.duration_ms,"sampled_peak_rss_bytes":outcome.sampled_peak_rss_bytes,"resource_error":outcome.resource_error,"isolation":outcome.isolation,
        "stdout":{"text":String::from_utf8_lossy(&outcome.stdout.bytes),"sha256":sha256_hex(&outcome.stdout.bytes),"size":outcome.stdout.bytes.len()},"stderr":{"text":String::from_utf8_lossy(&outcome.stderr.bytes),"sha256":sha256_hex(&outcome.stderr.bytes),"size":outcome.stderr.bytes.len()},"candidate_sha256":hash_file(&exe)?,"git_runtime":git_runtime});
    let data = serde_json::to_vec(&value)?;
    let artifact = private_write(&store, &data)?;
    let reference = json!({"path":artifact,"sha256":sha256_hex(&data),"bytes":data.len()});
    anyhow::ensure!(
        complete,
        "Bounded baseline source-snapshot worker failed; preserved preparation evidence: {}",
        reference
    );
    let after_key = hash_json(&(root, &source, &after_git?, &candidate_sha, &git_runtime))?;
    fs::write(
        cache.join(format!("{after_key}.json")),
        serde_json::to_vec(&reference)?,
    )?;
    Ok(reference)
}
fn metadata_identity(root: &Path) -> Result<String> {
    let mut values = BTreeMap::new();
    for entry in walkdir::WalkDir::new(root.join(".git"))
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.file_name() != "objects")
    {
        let entry = entry?;
        if entry.file_type().is_dir() {
            continue;
        }
        anyhow::ensure!(
            entry.file_type().is_file(),
            "Git metadata contains an alias or special file"
        );
        let path = entry
            .path()
            .strip_prefix(root)?
            .to_str()
            .context("Git metadata is not UTF-8")?;
        values.insert(
            path.to_owned(),
            sha256_hex(&read(root, path, 16 * 1024 * 1024)?),
        );
    }
    hash_json(&values)
}
pub fn accepted_snapshot(db: &Path, root: &Path) -> Result<Option<(Option<String>, String)>> {
    if configuration(root)?.is_none_or(|selection| selection.mode != "baseline") {
        return Ok(None);
    }
    let reference = prepare_snapshot(db, root)?;
    let proof = verified_preparation(&reference)?;
    let child = engine::parse_json(
        proof["stdout"]["text"]
            .as_str()
            .context("snapshot child stream absent")?
            .as_bytes(),
    )?;
    anyhow::ensure!(
        child["schema"] == "thruline.semgrep-snapshot-child.v1"
            && child["complete"] == true
            && child["root"] == json!(root),
        "snapshot child identity differs"
    );
    let tree = child["tree_hash"]
        .as_str()
        .context("snapshot tree absent")?;
    anyhow::ensure!(
        tree.len() == 40 && tree.bytes().all(|b| b.is_ascii_hexdigit()),
        "snapshot tree is not a complete Git identity"
    );
    Ok(Some((
        child["head"].as_str().map(str::to_owned),
        tree.to_owned(),
    )))
}

pub fn annotate_preparation(plan: &Plan, report: &mut Report) -> Result<()> {
    if let Some(reference) = plan.environment.get(PREPARATION_ENV) {
        let reference = engine::parse_json(reference.as_bytes())?;
        let proof = verified_preparation(&reference)?;
        anyhow::ensure!(
            proof["root"] == json!(plan.root),
            "Snapshot preparation belongs to a different source root"
        );
        report
            .facts
            .get_mut("semgrep_mode")
            .context("Semgrep mode facts missing")?["preparation"] = proof.clone();
        report
            .facts
            .insert("semgrep_snapshot_preparation".into(), proof);
    }
    Ok(())
}

pub fn bind_preparation(
    environment: &mut BTreeMap<String, String>,
    reference: &Value,
) -> Result<()> {
    environment.insert(PREPARATION_ENV.into(), serde_json::to_string(reference)?);
    Ok(())
}

/// The initial baseline route uses the product's ordinary source snapshot.
/// Refuse executable/local attribute configuration before that preparation.
/// Full configurable Git attributes/filter support is outside this mode.
pub fn snapshot_configuration(root: &Path) -> Result<()> {
    let config = String::from_utf8(read(root, ".git/config", 65536)?)?;
    let mut section = String::new();
    for line in config.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(['#', ';']) {
            continue;
        }
        if line.starts_with('[') {
            anyhow::ensure!(line=="[core]","Baseline snapshot preparation supports only the ordinary core Git configuration; extensions, filters, includes, aliases and executable settings require separate qualification");
            section = "core".into();
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .context("unsupported baseline Git configuration syntax")?;
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim().to_ascii_lowercase();
        anyhow::ensure!(
            section == "core"
                && matches!(
                    key.as_str(),
                    "repositoryformatversion"
                        | "filemode"
                        | "bare"
                        | "logallrefupdates"
                        | "ignorecase"
                        | "precomposeunicode"
                )
                && match key.as_str() {
                    "repositoryformatversion" => value == "0",
                    "bare" => value == "false",
                    _ => matches!(value.as_str(), "true" | "false"),
                },
            "Baseline snapshot configuration is outside the qualified ordinary core settings"
        );
    }
    for (index, entry) in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .enumerate()
    {
        let entry = entry?;
        anyhow::ensure!(
            index < 16384 && entry.depth() <= 32,
            "Baseline source/Git preparation inventory exceeds its bounded file or depth scope"
        );
        anyhow::ensure!(
            entry.file_name() != ".gitattributes"
                && entry.path() != root.join(".git/info/attributes"),
            "Baseline snapshot attributes are outside the qualified mode"
        );
    }
    Ok(())
}

pub fn git_identity(root: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let git = root.join(".git");
    let meta = fs::symlink_metadata(&git)?;
    anyhow::ensure!(
        meta.is_dir() && !meta.file_type().is_symlink(),
        "baseline mode supports an ordinary private .git directory, not linked worktrees"
    );
    for name in [
        "objects/info/alternates",
        "objects/info/http-alternates",
        "info/grafts",
        "shallow",
        "commondir",
        "worktrees",
        "modules",
    ] {
        anyhow::ensure!(
            !git.join(name).try_exists()?,
            "unsupported external, shallow, grafted or nested Git object layout: {name}"
        );
    }
    let config = String::from_utf8(read(root, ".git/config", 65536)?)?;
    anyhow::ensure!(
        !config.to_ascii_lowercase().contains("[extensions")
            && !config.to_ascii_lowercase().contains("[include"),
        "baseline Git extensions and included configuration are not qualified"
    );
    let mut identities = BTreeMap::new();
    let mut total = 0u64;
    let mut count = 0;
    for entry in walkdir::WalkDir::new(&git).follow_links(false) {
        let entry = entry?;
        anyhow::ensure!(
            entry.depth() <= 32,
            "baseline object path exceeds 32 components"
        );
        if entry.file_type().is_dir() {
            continue;
        }
        anyhow::ensure!(
            entry.file_type().is_file(),
            "baseline metadata contains a nonregular or symlink entry"
        );
        count += 1;
        anyhow::ensure!(count <= 8192, "baseline closure exceeds 8192 files");
        let relative = entry.path().strip_prefix(root)?;
        let mut file = open_regular(root, relative)?;
        let before = file.metadata()?;
        anyhow::ensure!(
            before.len() <= 16 * 1024 * 1024,
            "baseline object/pack exceeds 16 MiB; qualify a larger closure separately"
        );
        let mut hasher = Sha256::new();
        let mut bytes = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            bytes += n as u64;
            total += n as u64;
            anyhow::ensure!(
                bytes <= 16 * 1024 * 1024 && total <= 128 * 1024 * 1024,
                "baseline object closure exceeds 128 MiB"
            );
            hasher.update(&buffer[..n]);
        }
        let after = file.metadata()?;
        anyhow::ensure!(
            before.len() == after.len()
                && before.modified()? == after.modified()?
                && bytes == before.len(),
            "Git object changed during identity capture"
        );
        identities.insert(
            relative
                .to_str()
                .context("Git object path is not UTF-8")?
                .to_owned(),
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>(),
        );
    }
    anyhow::ensure!(
        identities.contains_key(".git/HEAD")
            && identities.contains_key(".git/config")
            && identities.keys().any(|p| p.starts_with(".git/objects/")),
        "baseline object closure is empty or missing HEAD/configuration"
    );
    hash_json(&identities)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn root() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join(".git/objects/aa")).unwrap();
        fs::write(
            d.path().join(".git/config"),
            "[core]\nrepositoryformatversion = 0\n",
        )
        .unwrap();
        fs::write(d.path().join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(
            d.path().join(".git/objects/aa/object"),
            b"inert object bytes",
        )
        .unwrap();
        d
    }
    #[cfg(unix)]
    #[test]
    fn explicit_configuration_rejects_external_aliases_and_unknown_fields() {
        use std::os::unix::fs::symlink;
        let d = root();
        let outside = tempfile::tempdir().unwrap();
        fs::create_dir(d.path().join(".thruline")).unwrap();
        fs::write(d.path().join("rule.yml"), "rules: []\n").unwrap();
        let config = json!({"schema":"thruline.semgrep-mode.v1","mode":"targeted","rules":"rule.yml","targets":["."]});
        fs::write(d.path().join(CONFIG), serde_json::to_vec(&config).unwrap()).unwrap();
        assert_eq!(configuration(d.path()).unwrap().unwrap().mode, "targeted");
        let mut changed = config.clone();
        changed["baseline_commit"] = Value::Null;
        fs::write(d.path().join(CONFIG), serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(configuration(d.path()).is_err());
        fs::remove_file(d.path().join(CONFIG)).unwrap();
        fs::write(
            outside.path().join("inert-config"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        symlink(outside.path().join("inert-config"), d.path().join(CONFIG)).unwrap();
        assert!(configuration(d.path()).is_err());
        fs::remove_file(d.path().join(CONFIG)).unwrap();
        fs::write(d.path().join(CONFIG), serde_json::to_vec(&config).unwrap()).unwrap();
        fs::remove_file(d.path().join("rule.yml")).unwrap();
        fs::write(outside.path().join("inert-rule"), "inert external policy").unwrap();
        symlink(outside.path().join("inert-rule"), d.path().join("rule.yml")).unwrap();
        assert!(configuration(d.path()).is_err());
    }
    #[test]
    fn snapshot_preparation_rejects_executable_configuration_and_attributes() {
        let d = root();
        assert!(snapshot_configuration(d.path()).is_ok());
        fs::write(
            d.path().join(".git/config"),
            "[core]\nrepositoryformatversion = 0\nfsmonitor = /tmp/inert-never-executed\n",
        )
        .unwrap();
        assert!(snapshot_configuration(d.path()).is_err());
        fs::write(d.path().join(".git/config"),"[core]\nrepositoryformatversion = 0\n[filter \"inert\"]\nclean = /tmp/inert-never-executed\n").unwrap();
        assert!(snapshot_configuration(d.path()).is_err());
        fs::write(
            d.path().join(".git/config"),
            "[core]\nrepositoryformatversion = 0\n",
        )
        .unwrap();
        fs::write(d.path().join(".gitattributes"), "*.py filter=inert\n").unwrap();
        assert!(snapshot_configuration(d.path()).is_err());
    }
    #[test]
    fn object_bytes_and_head_are_live_identity_inputs() {
        let d = root();
        let original = git_identity(d.path()).unwrap();
        fs::write(
            d.path().join(".git/objects/aa/object"),
            b"changed object bytes",
        )
        .unwrap();
        assert_ne!(original, git_identity(d.path()).unwrap());
        fs::write(
            d.path().join(".git/objects/aa/object"),
            b"inert object bytes",
        )
        .unwrap();
        assert_eq!(original, git_identity(d.path()).unwrap());
        fs::write(d.path().join(".git/HEAD"), "ref: refs/heads/other\n").unwrap();
        assert_ne!(original, git_identity(d.path()).unwrap());
    }
    #[cfg(unix)]
    #[test]
    fn does_not_read_external_objects_or_linked_metadata() {
        use std::os::unix::fs::symlink;
        let d = root();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("inert"), "inert secret").unwrap();
        symlink(
            outside.path().join("inert"),
            d.path().join(".git/objects/leak"),
        )
        .unwrap();
        assert!(git_identity(d.path()).is_err());
        fs::remove_file(d.path().join(".git/objects/leak")).unwrap();
        fs::create_dir_all(d.path().join(".git/objects/info")).unwrap();
        fs::write(
            d.path().join(".git/objects/info/alternates"),
            outside.path().display().to_string(),
        )
        .unwrap();
        assert!(git_identity(d.path()).is_err());
    }
    #[test]
    fn refuses_unsupported_layout_and_unbounded_objects() {
        let d = root();
        fs::write(d.path().join(".git/shallow"), "missing history").unwrap();
        assert!(git_identity(d.path()).is_err());
        fs::remove_file(d.path().join(".git/shallow")).unwrap();
        fs::File::create(d.path().join(".git/objects/large"))
            .unwrap()
            .set_len(16 * 1024 * 1024 + 1)
            .unwrap();
        assert!(git_identity(d.path()).is_err());
    }
}
