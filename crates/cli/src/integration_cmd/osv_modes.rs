//! Explicit source/artifact/image OSV selections and prepared runtime closure.
use super::*;
use std::path::Component;
pub const CONFIG: &str = ".thruline/osv.json";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    schema: String,
    mode: String,
    inputs: Vec<String>,
    formats: Vec<String>,
    policy: Option<String>,
    metadata: Option<Metadata>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    licenses: Vec<String>,
    deprecation: bool,
}
pub struct Recipe {
    pub argv: Vec<String>,
    pub rules: Vec<PathBuf>,
    pub runtime: Vec<PathBuf>,
    pub dataset: Vec<PathBuf>,
}
fn relative(s: &str) -> Result<()> {
    anyhow::ensure!(
        !s.is_empty()
            && !s.contains(['\\', '\0', '\n', '\r'])
            && s.split('/')
                .all(|p| !p.is_empty() && p != "." && p != ".." && p != ".git")
            && Path::new(s)
                .components()
                .all(|p| matches!(p, Component::Normal(_))),
        "OSV selections require ordinary project-relative files"
    );
    Ok(())
}
#[cfg(unix)]
fn read(root: &Path, name: &str, limit: u64) -> Result<Vec<u8>> {
    use rustix::fs::{open, openat, Mode, OFlags};
    use std::io::Read;
    relative(name)?;
    let mut directory = open(
        root,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW,
        Mode::empty(),
    )?;
    let parts: Vec<_> = Path::new(name).components().collect();
    for (i, part) in parts.iter().enumerate() {
        let Component::Normal(name) = part else {
            bail!("invalid relative file")
        };
        let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK;
        if i + 1 == parts.len() {
            let file = fs::File::from(openat(&directory, *name, flags, Mode::empty())?);
            anyhow::ensure!(
                file.metadata()?.is_file() && file.metadata()?.len() <= limit,
                "OSV selected file exceeds its byte bound or is not regular"
            );
            let mut bytes = Vec::new();
            file.take(limit + 1).read_to_end(&mut bytes)?;
            anyhow::ensure!(
                bytes.len() as u64 <= limit,
                "OSV selected input grew beyond its byte bound"
            );
            return Ok(bytes);
        }
        directory = openat(&directory, *name, flags | OFlags::DIRECTORY, Mode::empty())?;
    }
    bail!("missing selected file")
}
#[cfg(not(unix))]
fn read(_: &Path, _: &str, _: u64) -> Result<Vec<u8>> {
    bail!("OSV modes require qualified descriptor-relative input reads")
}
fn referenced(v: &Value) -> Result<PathBuf> {
    let path = PathBuf::from(
        v["path"]
            .as_str()
            .context("OSV runtime reference has no path")?,
    );
    anyhow::ensure!(
        path.is_absolute()
            && path.is_file()
            && !fs::symlink_metadata(&path)?.file_type().is_symlink(),
        "OSV runtime references require absolute ordinary files"
    );
    anyhow::ensure!(
        v["sha256"].as_str() == Some(hash_file(&path)?.as_str()),
        "OSV prepared runtime or dataset checksum changed"
    );
    if let Some(bytes) = v["bytes"].as_u64() {
        anyhow::ensure!(
            path.metadata()?.len() == bytes,
            "OSV prepared runtime byte count changed"
        )
    }
    Ok(path)
}
pub fn recipe(root: &Path, registered: &[PathBuf], output: &Path) -> Result<Recipe> {
    let value = engine::parse_json(&read(root, CONFIG, 65536)?)?;
    anyhow::ensure!(value.as_object().is_some_and(|o| o.len()==6 && ["schema","mode","inputs","formats","policy","metadata"].iter().all(|k|o.contains_key(*k))), "OSV mode configuration requires the exact schema, mode, inputs, formats, policy and metadata fields");
    let selection: Selection = serde_json::from_value(value)?;
    anyhow::ensure!(
        selection.schema == "thruline.osv-modes.v1"
            && ["sbom", "installed-npm", "image", "go"].contains(&selection.mode.as_str()),
        "Unsupported explicit OSV mode"
    );
    anyhow::ensure!(
        (1..=256).contains(&selection.inputs.len()),
        "OSV modes require 1-256 selected files"
    );
    let distinct: std::collections::BTreeSet<_> = selection.inputs.iter().collect();
    anyhow::ensure!(
        distinct.len() == selection.inputs.len(),
        "Repeated OSV input file"
    );
    anyhow::ensure!(
        selection.formats.len() <= 4
            && selection
                .formats
                .iter()
                .all(|s| ["sarif", "html", "cyclonedx-1-5", "spdx-2-3"].contains(&s.as_str())),
        "Unsupported native report format"
    );
    anyhow::ensure!(
        selection
            .formats
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            == selection.formats.len(),
        "Repeated native export format"
    );
    if ["sbom", "image"].contains(&selection.mode.as_str()) {
        anyhow::ensure!(
            selection.inputs.len() == 1,
            "Select exactly one SBOM or image archive"
        )
    }
    if selection.mode == "go" {
        anyhow::ensure!(selection.inputs.iter().any(|s|s=="go.mod") && selection.metadata.is_none(),"Go call analysis requires selected source/go.mod and does not support the npm metadata cache")
    }
    if let Some(metadata) = &selection.metadata {
        anyhow::ensure!(
            (1..=16).contains(&metadata.licenses.len())
                && metadata
                    .licenses
                    .iter()
                    .all(|s| !s.is_empty() && s.len() <= 80 && !s.contains(',')),
            "Invalid explicit native license allowlist"
        );
        let _ = metadata.deprecation;
    }
    let mut rules = vec![root.join(CONFIG)];
    let mut total = 0;
    for name in &selection.inputs {
        total += read(root, name, 8 * 1024 * 1024)?.len();
        anyhow::ensure!(
            total <= 64 * 1024 * 1024,
            "OSV selected files exceed 64 MiB"
        );
        rules.push(root.join(name));
    }
    if let Some(policy) = &selection.policy {
        read(root, policy, 65536)?;
        rules.push(root.join(policy));
    }
    let manifests: Vec<_> = registered
        .iter()
        .filter(|p| p.file_name().is_some_and(|s| s == "osv-mode-runtime.json"))
        .collect();
    anyhow::ensure!(
        manifests.len() == 1,
        "Provision one explicit osv-mode-runtime.json in the selected OSV runtime closure"
    );
    let manifest = manifests[0];
    let v = engine::parse_json(&bounded_file(manifest, 1024 * 1024)?)?;
    anyhow::ensure!(
        v["schema"] == "thruline.osv-mode-runtime.v1",
        "Unsupported OSV mode runtime manifest"
    );
    let python = referenced(&v["python"])?;
    let wrapper = referenced(&v["wrapper"])?;
    anyhow::ensure!(
        wrapper.file_name().is_some_and(|s| s == "osv-modes.py"),
        "Unexpected OSV mode controller"
    );
    let mut runtime = vec![
        manifest.to_path_buf(),
        python.clone(),
        wrapper.clone(),
        referenced(&v["executable"])?,
    ];
    if selection.metadata.is_some() {
        runtime.push(referenced(&v["cached_executable"])?);
        runtime.push(referenced(&v["metadata_manifest"])?);
        for value in v["metadata_files"]
            .as_array()
            .context("Prepared metadata closure missing")?
        {
            runtime.push(referenced(value)?);
        }
    }
    if selection.mode == "go" {
        runtime.push(referenced(&v["go_executable"])?);
    }
    for p in v["runtime_paths"]
        .as_array()
        .context("OSV runtime library closure missing")?
    {
        let path = PathBuf::from(p.as_str().context("Runtime path is not text")?);
        anyhow::ensure!(
            path.is_absolute() && path.exists(),
            "Missing OSV runtime library path"
        );
        runtime.push(path);
    }
    let mut dataset = vec![];
    for value in v["dataset_files"]
        .as_array()
        .context("OSV dataset closure missing")?
    {
        dataset.push(referenced(value)?);
    }
    if let Some(provenance) = v.get("provenance_files") {
        let records = provenance
            .as_array()
            .context("OSV provenance files must be an array")?;
        anyhow::ensure!(
            records.len() <= 32,
            "OSV provenance file count exceeds its bound"
        );
        for value in records {
            dataset.push(referenced(value)?);
        }
    }
    anyhow::ensure!(
        !dataset.is_empty() && v["dataset_scope"].as_array().is_some_and(|a| !a.is_empty()),
        "OSV modes require a dated explicit package/version dataset scope"
    );
    let argv = vec![
        python.display().to_string(),
        "-I".into(),
        "-B".into(),
        "-X".into(),
        "pycache_prefix=/dev/null".into(),
        wrapper.display().to_string(),
        "--runtime".into(),
        manifest.display().to_string(),
        "--output".into(),
        output.display().to_string(),
    ];
    Ok(Recipe {
        argv,
        rules,
        runtime,
        dataset,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    fn root(label: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "thruline-osv-selection-{}-{}",
            std::process::id(),
            label
        ));
        fs::create_dir(&p).unwrap();
        p
    }
    #[test]
    fn reads_exact_selected_bytes_and_refuses_symlink_components() {
        let root = root("links");
        fs::create_dir(root.join("ordinary")).unwrap();
        fs::write(root.join("ordinary/bom.json"), b"selected SBOM bytes").unwrap();
        assert_eq!(
            read(&root, "ordinary/bom.json", 64).unwrap(),
            b"selected SBOM bytes"
        );
        symlink("ordinary", root.join("alias")).unwrap();
        symlink("ordinary/bom.json", root.join("alias.json")).unwrap();
        assert!(read(&root, "alias/bom.json", 64).is_err());
        assert!(read(&root, "alias.json", 64).is_err());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn selected_file_limit_and_metadata_paths_are_explicit() {
        let root = root("limits");
        fs::write(root.join("bom.json"), b"12345").unwrap();
        assert!(read(&root, "bom.json", 4).is_err());
        for invalid in [
            "../bom.json",
            "/tmp/bom.json",
            ".git/config",
            "a//b",
            "a/./b",
            "a\\b",
        ] {
            assert!(relative(invalid).is_err(), "accepted {invalid}");
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn missing_required_configuration_fields_do_not_reach_runtime_lookup() {
        let root = root("config");
        fs::create_dir(root.join(".thruline")).unwrap();
        fs::write(root.join(".thruline/osv.json"),br#"{"schema":"thruline.osv-modes.v1","mode":"sbom","inputs":["bom.json"],"formats":[],"metadata":null}"#).unwrap();
        let error = recipe(&root, &[], &root.join("output"))
            .err()
            .unwrap()
            .to_string();
        assert!(error.contains("exact schema"), "{error}");
        fs::remove_dir_all(root).unwrap();
    }
}
