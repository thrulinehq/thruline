//! Explicit native views retain an uncached full companion and confined Git preparation.
use super::*;
const POLICY: &str = ".thruline/dependency-review-modes.json";
const PREPARATION: &str = "THRULINE_DEPENDENCY_PREPARATION";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    schema: String,
    baseline: Option<PathBuf>,
    affected_base: Option<String>,
    cache: bool,
}

fn selected_file(root: &Path, path: &Path, limit: u64) -> Result<Vec<u8>> {
    anyhow::ensure!(
        !path.is_absolute()
            && path
                .components()
                .all(|p| matches!(p, std::path::Component::Normal(_))),
        "Dependency mode inputs must be contained regular source files"
    );
    let mut current = root.to_path_buf();
    for part in path.components() {
        current.push(part);
        anyhow::ensure!(
            !fs::symlink_metadata(&current)?.file_type().is_symlink(),
            "Dependency mode input contains a link"
        );
    }
    bounded_file(&current, limit)
}
fn selection(root: &Path) -> Result<Option<Selection>> {
    match fs::symlink_metadata(root.join(POLICY)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let bytes = selected_file(root, Path::new(POLICY), 16384)?;
    let value = engine::parse_json(&bytes)?;
    anyhow::ensure!(
        value.as_object().is_some_and(|v| v.len() == 4),
        "Dependency mode policy requires all four explicit fields"
    );
    let selected: Selection = serde_json::from_value(value)?;
    anyhow::ensure!(
        selected.schema == "thruline.dependency-review-modes.v1"
            && (selected.baseline.is_some() || selected.affected_base.is_some() || selected.cache),
        "Unsupported or empty dependency mode policy"
    );
    if let Some(base) = &selected.affected_base {
        anyhow::ensure!(
            base.len() == 40
                && base
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Affected scope needs an immutable complete commit identity"
        );
    }
    if let Some(path) = &selected.baseline {
        let bytes = selected_file(root, path, 8 * 1024 * 1024)?;
        anyhow::ensure!(
            engine::parse_json(&bytes)?.is_array(),
            "Selected known-violation baseline must be a native JSON array"
        );
    }
    // Uses the existing confined ordinary Git boundary for source identity too.
    semgrep_mode::snapshot_configuration(root)?;
    semgrep_mode::git_identity(root)?;
    Ok(Some(selected))
}

fn proof(root: &Path, reference: &Value) -> Result<(Value, Option<String>, String)> {
    let value = semgrep_mode::verified_preparation(reference)?;
    anyhow::ensure!(
        value["root"] == json!(root),
        "Dependency preparation belongs to a different source root"
    );
    let stream = value["stdout"]["text"]
        .as_str()
        .context("Missing native snapshot stdout")?;
    anyhow::ensure!(
        value["stdout"]["sha256"] == sha256_hex(stream.as_bytes())
            && value["stdout"]["size"] == json!(stream.len()),
        "Snapshot stdout differs from its native capture"
    );
    let child = engine::parse_json(stream.as_bytes())?;
    let tree = child["tree_hash"]
        .as_str()
        .context("Missing snapshot tree")?;
    anyhow::ensure!(
        child["schema"] == "thruline.semgrep-snapshot-child.v1"
            && child["complete"] == true
            && child["root"] == json!(root)
            && tree.len() == 40
            && tree.bytes().all(|b| b.is_ascii_hexdigit()),
        "Incomplete or foreign native snapshot identity"
    );
    Ok((
        value.clone(),
        child["head"].as_str().map(str::to_owned),
        tree.to_owned(),
    ))
}

pub fn configure(
    db: &Path,
    root: &Path,
    runtime: &[PathBuf],
    rules: &mut Vec<PathBuf>,
    dependencies: &mut Vec<PathBuf>,
    environment: &mut BTreeMap<String, String>,
) -> Result<bool> {
    let Some(selected) = selection(root)? else {
        return Ok(false);
    };
    selected_file(
        root,
        Path::new(".thruline/dependency-cruiser-report.json"),
        16384,
    )?;
    let git = runtime.iter().find(|p| p.is_file() && p.file_name().is_some_and(|n| n == "git") && **p != Path::new("/usr/bin/git")).context("Prepare and bind the actual Git executable, not the macOS launcher, in this dependency runtime")?;
    anyhow::ensure!(
        git.is_absolute() && git.canonicalize()? == *git,
        "Prepared Git runtime must be an absolute regular binary"
    );
    rules.push(root.join(POLICY));
    if let Some(baseline) = selected.baseline {
        rules.push(root.join(baseline));
    }
    environment.insert("THRULINE_DEPENDENCY_GIT".into(), git.display().to_string());
    environment.insert(
        "PATH".into(),
        format!(
            "{}:/usr/bin:/bin",
            git.parent().context("Git parent absent")?.display()
        ),
    );
    environment.insert("GIT_CONFIG_NOSYSTEM".into(), "1".into());
    environment.insert("GIT_CONFIG_GLOBAL".into(), "/dev/null".into());
    environment.insert("GIT_TERMINAL_PROMPT".into(), "0".into());
    environment.insert(
        "THRULINE_DEPENDENCY_RUNTIME_SHA256".into(),
        input_read::hash_runtime(root, runtime)?,
    );
    environment.insert(
        "THRULINE_DEPENDENCY_STATE_DIRECTORY".into(),
        artifact_dir(db)
            .join("dependency-cruiser")
            .display()
            .to_string(),
    );
    if selected.affected_base.is_some() {
        // Materialize objects inside the fixed worker before sealing the complete .git closure.
        let reference = semgrep_mode::prepare_snapshot(db, root)?;
        proof(root, &reference)?;
        rules.push(PathBuf::from(
            reference["path"]
                .as_str()
                .context("Missing managed preparation path")?,
        ));
        dependencies.push(root.join(".git"));
        environment.insert(PREPARATION.into(), serde_json::to_string(&reference)?);
    }
    Ok(true)
}

pub fn accepted_snapshot(db: &Path, root: &Path) -> Result<Option<(Option<String>, String)>> {
    if selection(root)?.is_none_or(|selected| selected.affected_base.is_none()) {
        return Ok(None);
    }
    let reference = semgrep_mode::prepare_snapshot(db, root)?;
    let (_, head, tree) = proof(root, &reference)?;
    Ok(Some((head, tree)))
}

pub fn annotate_preparation(plan: &Plan, report: &mut Report) -> Result<()> {
    let Some(reference) = plan.environment.get(PREPARATION) else {
        return Ok(());
    };
    let reference = engine::parse_json(reference.as_bytes())?;
    let (value, _, _) = proof(&plan.root, &reference)?;
    let graph = report
        .facts
        .get_mut("dependency_graph")
        .context("Missing dependency graph facts")?;
    anyhow::ensure!(
        graph["review_modes"].is_object(),
        "Missing selected dependency review modes"
    );
    graph["review_modes"]["preparation"] = value;
    Ok(())
}
