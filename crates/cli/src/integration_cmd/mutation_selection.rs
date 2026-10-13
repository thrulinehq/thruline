//! A bounded accepted native mutation selection; never arbitrary native argv.
use super::*;

pub(super) const CONFIG: &str = ".thruline/mutants.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Selection {
    schema: String,
    #[serde(default)]
    workspace: bool,
    #[serde(default)]
    packages: Vec<String>,
    #[serde(default)]
    test_workspace: bool,
    #[serde(default)]
    test_packages: Vec<String>,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    all_features: bool,
    #[serde(default)]
    no_default_features: bool,
    #[serde(default)]
    in_diff: Option<String>,
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    exclude_files: Vec<String>,
    #[serde(default)]
    examine_re: Vec<String>,
    #[serde(default)]
    exclude_re: Vec<String>,
    #[serde(default = "one")]
    jobs: u64,
    #[serde(default)]
    shard: Option<Shard>,
    #[serde(default = "cargo")]
    pub(super) test_runner: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Shard {
    index: u64,
    count: u64,
    algorithm: String,
}
fn one() -> u64 {
    1
}
fn cargo() -> String {
    "cargo".into()
}

pub(super) fn configuration(root: &Path) -> Result<Option<Selection>> {
    let directory = root.join(".thruline");
    match fs::symlink_metadata(&directory) {
        Ok(metadata) => anyhow::ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "Mutation selection directory must be a real project directory"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let path = root.join(CONFIG);
    match fs::symlink_metadata(&path) {
        Ok(metadata) => anyhow::ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Mutation selection must be a regular project file without symlinks"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let selection: Selection =
        serde_json::from_value(engine::parse_json(&bounded_file(&path, 16384)?)?)?;
    selection.args(root)?;
    Ok(Some(selection))
}
impl Selection {
    pub(super) fn args(&self, root: &Path) -> Result<Vec<String>> {
        anyhow::ensure!(
            self.schema == "thruline.mutation-selection.v1",
            "Unsupported mutation selection schema"
        );
        anyhow::ensure!(
            !(self.workspace && !self.packages.is_empty()),
            "Choose workspace or named mutation packages"
        );
        anyhow::ensure!(
            !(self.test_workspace && !self.test_packages.is_empty()),
            "Choose workspace tests or named test packages"
        );
        anyhow::ensure!(
            !(self.all_features && (!self.features.is_empty() || self.no_default_features)),
            "All features conflicts with explicit/default feature selection"
        );
        anyhow::ensure!(
            (1..=2).contains(&self.jobs),
            "Mutation jobs must be 1 or 2 within the worker budget"
        );
        anyhow::ensure!(
            ["cargo", "nextest"].contains(&self.test_runner.as_str()),
            "Mutation test runner must be cargo or nextest"
        );
        let mut args = vec![
            "--jobs".into(),
            self.jobs.to_string(),
            "--test-tool".into(),
            self.test_runner.clone(),
        ];
        for (flag, enabled) in [
            ("--workspace", self.workspace),
            ("--test-workspace=true", self.test_workspace),
            ("--all-features", self.all_features),
            ("--no-default-features", self.no_default_features),
        ] {
            if enabled {
                args.push(flag.into());
            }
        }
        for (flag, values, identifier) in [
            ("--package", &self.packages, true),
            ("--test-package", &self.test_packages, true),
            ("--features", &self.features, true),
            ("--file", &self.files, false),
            ("--exclude", &self.exclude_files, false),
            ("--re", &self.examine_re, false),
            ("--exclude-re", &self.exclude_re, false),
        ] {
            anyhow::ensure!(values.len() <= 32, "Mutation selector count exceeds 32");
            let mut seen = std::collections::BTreeSet::new();
            for value in values {
                anyhow::ensure!(
                    !value.is_empty()
                        && value.len() <= 256
                        && !value.starts_with('-')
                        && !value.chars().any(char::is_control)
                        && seen.insert(value),
                    "Invalid or duplicate mutation selector"
                );
                anyhow::ensure!(
                    !identifier
                        || value
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"_-/".contains(&c)),
                    "Invalid package or feature selector"
                );
                args.extend([flag.into(), value.clone()]);
            }
        }
        if let Some(diff) = &self.in_diff {
            let path = Path::new(diff);
            anyhow::ensure!(
                !diff.is_empty()
                    && !path.is_absolute()
                    && path
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_)))
                    && !path.components().any(|c| c.as_os_str() == ".git"),
                "Mutation diff must be a contained project file"
            );
            let mut current = root.to_path_buf();
            for part in path.components() {
                current.push(part);
                anyhow::ensure!(
                    !fs::symlink_metadata(&current)?.file_type().is_symlink(),
                    "Mutation diff symlinks are unsupported"
                );
            }
            anyhow::ensure!(
                !bounded_file(&current, 1024 * 1024)?.is_empty(),
                "Mutation diff is empty"
            );
            args.extend(["--in-diff".into(), current.display().to_string()]);
        }
        if let Some(shard) = &self.shard {
            anyhow::ensure!(
                (1..=16).contains(&shard.count) && shard.index < shard.count,
                "Invalid mutation shard index or count"
            );
            anyhow::ensure!(
                ["slice", "round-robin"].contains(&shard.algorithm.as_str()),
                "Unsupported mutation sharding algorithm"
            );
            args.extend([
                "--shard".into(),
                format!("{}/{}", shard.index, shard.count),
                "--sharding".into(),
                shard.algorithm.clone(),
            ]);
        }
        Ok(args)
    }
    pub(super) fn rule_files(&self, root: &Path) -> Vec<PathBuf> {
        let mut files = vec![root.join(CONFIG)];
        if let Some(diff) = &self.in_diff {
            files.push(root.join(diff));
        }
        files
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(value: Value) -> Result<Selection> {
        Ok(serde_json::from_value(value)?)
    }
    #[test]
    fn selected_packages_features_jobs_and_shards_are_exact_native_arguments() {
        let value = json!({"schema":"thruline.mutation-selection.v1","packages":["arithmetic"],"features":["strong"],"no_default_features":true,"jobs":2,"shard":{"index":1,"count":2,"algorithm":"round-robin"},"exclude_re":["untested"]});
        let selection = parse(value).unwrap();
        let args = selection.args(Path::new(".")).unwrap();
        assert_eq!(
            args,
            vec![
                "--jobs",
                "2",
                "--test-tool",
                "cargo",
                "--no-default-features",
                "--package",
                "arithmetic",
                "--features",
                "strong",
                "--exclude-re",
                "untested",
                "--shard",
                "1/2",
                "--sharding",
                "round-robin"
            ]
        );
    }
    #[test]
    fn arbitrary_commands_conflicting_modes_and_unbounded_jobs_are_refused() {
        for extra in [
            json!({"argv":["--in-place"]}),
            json!({"jobs":3}),
            json!({"jobs":0}),
            json!({"test_runner":"shell"}),
            json!({"workspace":true,"packages":["a"]}),
            json!({"all_features":true,"features":["a"]}),
            json!({"packages":["--all"]}),
            json!({"shard":{"index":2,"count":2,"algorithm":"slice"}}),
        ] {
            let mut v = json!({"schema":"thruline.mutation-selection.v1"});
            v.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            assert!(parse(v).and_then(|s| s.args(Path::new("."))).is_err());
        }
    }
    #[test]
    fn diff_requires_bounded_present_project_file_without_symlinks() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("change.diff"), "diff --git a/x b/x\n").unwrap();
        for name in ["../other", "/tmp/other", ".git/HEAD", "missing"] {
            let s =
                parse(json!({"schema":"thruline.mutation-selection.v1","in_diff":name})).unwrap();
            assert!(s.args(root.path()).is_err());
        }
        let s = parse(json!({"schema":"thruline.mutation-selection.v1","in_diff":"change.diff"}))
            .unwrap();
        assert!(s.args(root.path()).is_ok());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                root.path().join("change.diff"),
                root.path().join("linked.diff"),
            )
            .unwrap();
            let s =
                parse(json!({"schema":"thruline.mutation-selection.v1","in_diff":"linked.diff"}))
                    .unwrap();
            assert!(s.args(root.path()).is_err());
        }
    }
    #[test]
    fn explicit_selection_symlinks_do_not_silently_fall_back_to_defaults() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join(".thruline")).unwrap();
        assert!(configuration(root.path()).unwrap().is_none());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.path().join("missing"), root.path().join(CONFIG))
                .unwrap();
            assert!(configuration(root.path()).is_err());
        }
    }
}
