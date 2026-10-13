//! Explicit Infer checker selection and pinned runtime identity.
use super::*;
use std::path::Component;
pub const CONFIG: &str = ".thruline/infer-modes.json";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    schema: String,
    mode: String,
    sources: Vec<String>,
    java_release: Option<u64>,
    procedures: Vec<String>,
    taint_model: Option<String>,
    baseline_sources: Vec<String>,
    require_pure: bool,
}
pub struct Recipe {
    pub argv: Vec<String>,
    pub rules: Vec<PathBuf>,
    pub runtime: Vec<PathBuf>,
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
        "Infer selections require ordinary project-relative files"
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
                "Infer selected file exceeds its byte bound or is not regular"
            );
            let mut bytes = Vec::new();
            file.take(limit + 1).read_to_end(&mut bytes)?;
            anyhow::ensure!(
                bytes.len() as u64 <= limit,
                "Infer selected input grew beyond its byte bound"
            );
            return Ok(bytes);
        }
        directory = openat(&directory, *name, flags | OFlags::DIRECTORY, Mode::empty())?;
    }
    bail!("missing selected file")
}
#[cfg(not(unix))]
fn read(_: &Path, _: &str, _: u64) -> Result<Vec<u8>> {
    bail!("Infer modes require qualified descriptor-relative input reads")
}
fn referenced(v: &Value) -> Result<PathBuf> {
    let path = PathBuf::from(
        v["path"]
            .as_str()
            .context("Infer runtime reference has no path")?,
    );
    anyhow::ensure!(
        path.is_absolute()
            && path.is_file()
            && !fs::symlink_metadata(&path)?.file_type().is_symlink(),
        "Infer runtime references require absolute ordinary files"
    );
    anyhow::ensure!(
        v["sha256"].as_str() == Some(hash_file(&path)?.as_str()),
        "Infer prepared runtime or dataset checksum changed"
    );
    if let Some(bytes) = v["bytes"].as_u64() {
        anyhow::ensure!(
            path.metadata()?.len() == bytes,
            "Infer prepared runtime byte count changed"
        )
    }
    Ok(path)
}

fn class_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 256
        && s.split('.').all(|part| {
            let mut bytes = part.bytes();
            bytes
                .next()
                .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_' || b == b'$')
                && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$')
        })
}
pub fn recipe(root: &Path, registered: &[PathBuf], output: &Path) -> Result<Recipe> {
    let value = engine::parse_json(&read(root, CONFIG, 65536)?)?;
    anyhow::ensure!(
        value.as_object().is_some_and(|o| o.len() == 8
            && [
                "schema",
                "mode",
                "sources",
                "java_release",
                "procedures",
                "taint_model",
                "baseline_sources",
                "require_pure"
            ]
            .iter()
            .all(|k| o.contains_key(*k))),
        "Infer mode configuration requires all eight explicit fields"
    );
    let c: Selection = serde_json::from_value(value)?;
    anyhow::ensure!(
        c.schema == "thruline.infer-modes.v1"
            && [
                "racerd",
                "cost",
                "purity",
                "impurity",
                "loop-hoisting",
                "pulse",
                "taint",
                "rust-ir",
                "erlang",
                "python"
            ]
            .contains(&c.mode.as_str()),
        "Unsupported explicit Infer mode"
    );
    let java = !["rust-ir", "erlang", "python"].contains(&c.mode.as_str());
    anyhow::ensure!(
        c.java_release == if java { Some(17) } else { None },
        "Only explicit Java17 compilation or selected language capture is supported"
    );
    anyhow::ensure!(
        !c.require_pure || c.mode == "impurity",
        "A purity requirement applies only to the explicit impurity checker"
    );
    anyhow::ensure!(
        c.procedures.len() <= 32
            && java != c.procedures.is_empty()
            && c.procedures.iter().all(|s| class_name(s))
            && c.procedures
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == c.procedures.len(),
        "Java summaries require distinct literal qualified class names"
    );
    anyhow::ensure!(!c.sources.is_empty() && c.sources.len()<=128 && c.baseline_sources.len()<=128 && (c.baseline_sources.is_empty() || c.mode=="cost"),"Infer requires 1-128 sources; explicit baseline sources apply only to native cost comparison");
    let mut rules = vec![root.join(CONFIG)];
    let mut total = 0;
    for names in [&c.sources, &c.baseline_sources] {
        anyhow::ensure!(
            names
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == names.len(),
            "Repeated Infer source selection"
        );
        for name in names {
            let suffix = Path::new(name)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            anyhow::ensure!(
                if java {
                    suffix == "java"
                } else {
                    match c.mode.as_str() {
                        "rust-ir" => ["rs", "ullbc"].contains(&suffix),
                        "erlang" => suffix == "erl",
                        _ => suffix == "py",
                    }
                },
                "Source extension is outside the selected native mode"
            );
            total += read(root, name, 8 * 1024 * 1024)?.len();
            anyhow::ensure!(
                total <= 32 * 1024 * 1024,
                "Infer selected source bytes exceed 32 MiB"
            );
            rules.push(root.join(name));
        }
    }
    anyhow::ensure!(
        c.mode != "python" || c.sources.len() == 1,
        "Python availability capture requires exactly one selected source"
    );
    anyhow::ensure!(
        c.mode != "rust-ir" || c.sources.iter().any(|s| s.ends_with(".ullbc")),
        "Experimental Rust mode requires explicit existing ULLBC inputs"
    );
    anyhow::ensure!(
        (c.mode == "taint") == c.taint_model.is_some(),
        "A reviewed taint model is required only for the selected taint mode"
    );
    if let Some(name) = &c.taint_model {
        engine::parse_json(&read(root, name, 65536)?)?;
        rules.push(root.join(name));
    }
    let manifests: Vec<_> = registered
        .iter()
        .filter(|p| {
            p.file_name()
                .is_some_and(|s| s == "infer-mode-runtime.json")
        })
        .collect();
    anyhow::ensure!(
        manifests.len() == 1,
        "Provision exactly one infer-mode-runtime.json in the selected Infer runtime closure"
    );
    let manifest = manifests[0];
    let v = engine::parse_json(&bounded_file(manifest, 1024 * 1024)?)?;
    anyhow::ensure!(
        v["schema"] == "thruline.infer-mode-runtime.v1" && v["version"] == "1.3.0",
        "Unsupported prepared Infer mode runtime"
    );
    let python = referenced(&v["python"])?;
    let wrapper = referenced(&v["wrapper"])?;
    anyhow::ensure!(
        wrapper.file_name().is_some_and(|s| s == "infer-modes.py"),
        "Unexpected Infer mode controller"
    );
    let mut runtime = vec![
        manifest.to_path_buf(),
        python.clone(),
        wrapper.clone(),
        referenced(&v["executable"])?,
        referenced(&v["javac"])?,
    ];
    if c.mode == "rust-ir" {
        runtime.push(referenced(&v["rust_executable"])?);
    }
    let paths = v["runtime_paths"]
        .as_array()
        .context("Infer runtime closure missing")?;
    anyhow::ensure!(paths.len() <= 32, "Infer runtime path count exceeds bound");
    for item in paths {
        let path = PathBuf::from(item.as_str().context("Runtime path not text")?);
        anyhow::ensure!(
            path.is_absolute() && path.exists(),
            "Missing Infer runtime library path"
        );
        runtime.push(path);
    }
    let provenance = v["provenance_files"]
        .as_array()
        .context("Infer provenance closure missing")?;
    anyhow::ensure!(provenance.len() <= 32, "Infer provenance exceeds bound");
    for item in provenance {
        runtime.push(referenced(item)?);
    }
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
    })
}
#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn class_identifiers_do_not_enable_native_regex_injection() {
        for valid in ["Calc", "com.owned.ValueFlow", "Outer$Inner"] {
            assert!(class_name(valid));
        }
        for invalid in [".*", "X|Y", "X\\|Y", "a..b", "9Test", "", "A\nB"] {
            assert!(!class_name(invalid));
        }
    }
    #[test]
    fn source_components_refuse_git_and_parent_paths() {
        for bad in [
            "../Calc.java",
            "/tmp/Calc.java",
            ".git/config",
            "a//b",
            "a/./b",
            "a\\b",
        ] {
            assert!(relative(bad).is_err());
        }
        assert!(relative("src/Calc.java").is_ok());
    }
    #[test]
    fn selected_source_reader_refuses_directory_symlink() {
        let root =
            std::env::temp_dir().join(format!("thruline-infer-mode-reader-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("real")).unwrap();
        fs::write(root.join("real/Calc.java"), b"class Calc {}\n").unwrap();
        std::os::unix::fs::symlink("real", root.join("alias")).unwrap();
        assert!(read(&root, "alias/Calc.java", 100).is_err());
        assert_eq!(
            read(&root, "real/Calc.java", 100).unwrap(),
            b"class Calc {}\n"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
