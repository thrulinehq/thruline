//! Bounded to one results read. Reuses byte-derived identities among at most
//! 100 historical receipts; never caches an external execution or survives a
//! refresh. Each file closure is read at its sampling point in this request.
use super::*;
#[derive(Default)]
pub struct InputRead {
    files: BTreeMap<String, String>,
    runtimes: BTreeMap<String, String>,
    runtime_bytes: RuntimeBytes,
    executables: BTreeMap<String, String>,
    platform: Option<String>,
    suppression_sources: BTreeMap<String, suppressions::Snapshot>,
}
impl InputRead {
    fn runtime(&mut self, root: &Path, paths: &[PathBuf]) -> Result<String> {
        let key = hash_json(&(root, paths))?;
        if let Some(identity) = self.runtimes.get(&key) {
            return Ok(identity.clone());
        }
        let identity = runtime_graph_with_cache(
            root,
            paths,
            RuntimeLimits::default(),
            &mut self.runtime_bytes,
        )?
        .0;
        self.runtimes.insert(key, identity.clone());
        Ok(identity)
    }
    fn files(&mut self, root: &Path, paths: &[PathBuf]) -> Result<String> {
        let key = hash_json(&(root, paths))?;
        if let Some(identity) = self.files.get(&key) {
            return Ok(identity.clone());
        }
        let identity = hash_files(root, paths)?;
        self.files.insert(key, identity.clone());
        Ok(identity)
    }
    fn executable(&mut self, plan: &Plan) -> Result<String> {
        let key = hash_json(&(&plan.argv[0], &plan.container))?;
        if let Some(identity) = self.executables.get(&key) {
            return Ok(identity.clone());
        }
        let identity = match &plan.container {
            Some(spec) => container::identity(spec)?,
            None => hash_file(Path::new(&plan.argv[0]))?,
        };
        self.executables.insert(key, identity.clone());
        Ok(identity)
    }
    pub(super) fn suppression_snapshot(&mut self, plan: &Plan) -> Result<&suppressions::Snapshot> {
        let key = hash_json(&(&plan.root, &plan.pack, &plan.format, &plan.argv))?;
        if !self.suppression_sources.contains_key(&key) {
            self.suppression_sources
                .insert(key.clone(), suppressions::Snapshot::capture(plan)?);
        }
        Ok(self
            .suppression_sources
            .get(&key)
            .expect("inserted source snapshot"))
    }
    pub fn read(&mut self, plan: &Plan, obs: &WObservation) -> Result<Inputs> {
        let executable = hash_json(&(
            self.executable(plan)?,
            self.runtime(&plan.root, &plan.runtime)?,
        ))?;
        let platform = match &self.platform {
            Some(v) => v.clone(),
            None => {
                let v = platform_identity()?;
                self.platform = Some(v.clone());
                v
            }
        };
        let mut dependencies = self.files(&plan.root, &plan.dependencies)?;
        if ["ast-grep", "semgrep"].contains(&plan.pack.as_str()) {
            dependencies = hash_json(&(&dependencies, &self.suppression_snapshot(plan)?.identity))?;
        }
        Ok(Inputs {
            workspace: IdentityEdge::from_observation(obs)
                .identity
                .unwrap_or_default(),
            executable,
            environment: hash_json(
                &json!({"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"platform":platform,"env":plan.environment}),
            )?,
            rules: if semgrep_mode::is_plan(plan) {
                semgrep_mode::rules_identity(plan)?
            } else {
                self.files(&plan.root, &plan.rules)?
            },
            dependencies,
            dataset: if semgrep_mode::is_plan(plan) && plan.argv.len() == 7 {
                hash_json(&(
                    self.files(&plan.root, &plan.dataset)?,
                    semgrep_mode::git_identity(&plan.root)?,
                ))?
            } else {
                self.files(&plan.root, &plan.dataset)?
            },
            // Acceptance is authoritative and cheap; it is never cached here.
            requirement: workflow::requirement_identity(plan)?,
            policy: if plan.acceptance.is_some() {
                hash_json(&(plan, workflow::runtime_configuration_identity(&plan.pack)?))?
            } else {
                hash_json(plan)?
            },
            adapter: hash_json(&(
                engine::ADAPTER_VERSION,
                crate::CODE_DIGEST,
                include_str!("../../../verify/src/integrations/mod.rs"),
                include_str!("../../../verify/src/integrations/decode.rs"),
                include_str!("../../../verify/src/integrations/ruff.rs"),
                include_str!("../../../verify/src/integrations/semgrep_modes.rs"),
                include_str!("../../../verify/src/integrations/infer.rs"),
                include_str!("../../../verify/src/integrations/clippy.rs"),
                include_str!("../../../verify/src/integrations/specialists.rs"),
                include_str!("../../../verify/src/integrations/sonar.rs"),
                include_str!("../../../verify/src/integrations/megalinter.rs"),
                include_str!("../../../verify/src/integrations/local_agent.rs"),
                include_str!("../../../verify/src/integrations/strict_json.rs"),
                include_str!("../../../verify/src/integrations/coverage.rs"),
                include_str!("../../../verify/src/integrations/suppressions.rs"),
                engine::CATALOG,
            ))?,
        })
    }
}

/// Runtime installations contain framework directory aliases and executable
/// aliases. Bind the complete reachable graph, reading each physical file once
/// per request. Rules, dependencies, datasets and source readers keep their
/// existing stricter limits and directory-link refusal.
pub(super) fn hash_runtime(root: &Path, paths: &[PathBuf]) -> Result<String> {
    runtime_graph(root, paths, RuntimeLimits::default()).map(|result| result.0)
}

#[derive(Clone, Copy)]
struct RuntimeLimits {
    entries: usize,
    bytes: u64,
    file_bytes: u64,
}
impl Default for RuntimeLimits {
    fn default() -> Self {
        // Measured full Clippy/Xcode closure: 4.0 GiB, 56k graph entries;
        // Infer: 1.25 GiB after eliminating repeated executable aliases.
        // Only runtime closures use this 6 GiB aggregate ceiling. The 1 GiB
        // single-file ceiling is unchanged; no assets are excluded to fit.
        Self {
            entries: 100_000,
            bytes: 6 * 1024 * 1024 * 1024,
            file_bytes: 1024 * 1024 * 1024,
        }
    }
}

/// Shared only by closures sampled within a single `results` request. Graphs
/// still enumerate every path, bind aliases, enforce their own byte budgets,
/// and recheck metadata after traversal. The cached value is a byte digest,
/// never a saved assessment or execution. A later request starts empty.
#[derive(Default)]
struct RuntimeBytes {
    #[cfg(unix)]
    files: BTreeMap<(u64, u64), (RuntimeStamp, String)>,
    #[cfg(test)]
    bytes_read: u64,
}
#[cfg(unix)]
type RuntimeStamp = (u64, u64, u64, u32, i64, i64, i64, i64);

#[cfg(unix)]
fn stamp(metadata: &fs::Metadata) -> RuntimeStamp {
    use std::os::unix::fs::MetadataExt;
    (
        metadata.dev(),
        metadata.ino(),
        metadata.len(),
        metadata.mode(),
        metadata.mtime(),
        metadata.mtime_nsec(),
        metadata.ctime(),
        metadata.ctime_nsec(),
    )
}

#[cfg(unix)]
fn runtime_graph(root: &Path, paths: &[PathBuf], limits: RuntimeLimits) -> Result<(String, u64)> {
    runtime_graph_with_cache(root, paths, limits, &mut RuntimeBytes::default())
}

#[cfg(unix)]
fn runtime_graph_with_cache(
    root: &Path,
    paths: &[PathBuf],
    limits: RuntimeLimits,
    cache: &mut RuntimeBytes,
) -> Result<(String, u64)> {
    use rustix::fs::{open, Mode, OFlags};
    use sha2::{Digest, Sha256};
    use std::collections::BTreeSet;
    use std::io::Read;
    use std::os::unix::fs::MetadataExt;

    struct Graph {
        nodes: BTreeMap<PathBuf, Value>,
        resolving: BTreeSet<PathBuf>,
        resolved: BTreeMap<PathBuf, PathBuf>,
        watched: BTreeMap<PathBuf, fs::Metadata>,
        limits: RuntimeLimits,
    }
    impl Graph {
        fn insert(&mut self, path: PathBuf, value: Value) -> Result<()> {
            self.nodes.insert(path, value);
            anyhow::ensure!(
                self.nodes.len() <= self.limits.entries,
                "runtime closure exceeds {} graph entries",
                self.limits.entries
            );
            Ok(())
        }
        // Resolve one component at a time so intermediate aliases, including
        // Versions/Current and a declared SDK alias, are themselves bound.
        fn resolve(&mut self, path: &Path, depth: usize) -> Result<PathBuf> {
            anyhow::ensure!(
                depth <= 256,
                "runtime path or link chain exceeds 256 components"
            );
            anyhow::ensure!(path.is_absolute(), "runtime paths must be absolute");
            if let Some(found) = self.resolved.get(path) {
                return Ok(found.clone());
            }
            if path == Path::new("/") {
                return Ok(path.into());
            }
            let parent =
                self.resolve(path.parent().context("runtime parent missing")?, depth + 1)?;
            let name = path.file_name().context("runtime filename missing")?;
            let physical = parent.join(name);
            let metadata = fs::symlink_metadata(&physical)
                .with_context(|| format!("cannot bind runtime {}", physical.display()))?;
            let found = if metadata.file_type().is_symlink() {
                anyhow::ensure!(
                    self.resolving.insert(physical.clone()),
                    "runtime symbolic-link cycle: {}",
                    physical.display()
                );
                let target = fs::read_link(&physical)?;
                let destination = if target.is_absolute() {
                    target.clone()
                } else {
                    parent.join(&target)
                };
                // Normalize only lexical dot components. Resolution of each
                // preceding symlink remains authoritative for `link/..`.
                let destination = self.resolve_components(&destination, depth + 1)?;
                self.insert(
                    physical.clone(),
                    json!({"kind":"symlink","target":target,"resolved":destination}),
                )?;
                self.watched.insert(physical.clone(), metadata);
                self.resolving.remove(&physical);
                destination
            } else {
                physical
            };
            self.resolved.insert(path.into(), found.clone());
            Ok(found)
        }
        fn resolve_components(&mut self, path: &Path, depth: usize) -> Result<PathBuf> {
            use std::path::Component;
            let mut current = PathBuf::from("/");
            for part in path.components() {
                match part {
                    Component::RootDir => {}
                    Component::CurDir => {
                        anyhow::ensure!(
                            fs::metadata(&current)?.is_dir(),
                            "runtime dot traversal through a non-directory"
                        );
                    }
                    Component::ParentDir => {
                        anyhow::ensure!(
                            fs::metadata(&current)?.is_dir(),
                            "runtime parent traversal through a non-directory"
                        );
                        current.pop();
                        if current.as_os_str().is_empty() {
                            current.push("/");
                        }
                    }
                    Component::Normal(name) => {
                        current = self.resolve(&current.join(name), depth + 1)?;
                    }
                    _ => bail!("unsupported runtime path prefix"),
                }
            }
            Ok(current)
        }
    }
    let mut graph = Graph {
        nodes: BTreeMap::new(),
        resolving: BTreeSet::new(),
        resolved: BTreeMap::new(),
        watched: BTreeMap::new(),
        limits,
    };
    let mut declared = BTreeMap::new();
    let mut pending = BTreeSet::new();
    for path in paths {
        let full = path_at(root, path);
        let resolved = graph.resolve_components(&full, 0)?;
        declared.insert(full, resolved.clone());
        pending.insert(resolved);
    }
    let mut visited = BTreeSet::new();
    let mut contents: BTreeMap<(u64, u64), (fs::Metadata, String, PathBuf)> = BTreeMap::new();
    let mut total = 0u64;
    while let Some(path) = pending.pop_first() {
        if !visited.insert(path.clone()) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            graph.resolved.remove(&path);
            pending.insert(graph.resolve(&path, 0)?);
            continue;
        }
        let value = if metadata.is_dir() {
            let mut children = BTreeSet::new();
            for child in fs::read_dir(&path)? {
                let child = child?;
                children.insert(child.file_name());
                // Bound enumeration before collecting arbitrarily wide dirs.
                anyhow::ensure!(
                    children.len() + graph.nodes.len() <= limits.entries,
                    "runtime directory exceeds graph-entry budget"
                );
                pending.insert(child.path());
            }
            json!({"kind":"directory","mode":metadata.mode(),"children":children})
        } else {
            anyhow::ensure!(
                metadata.is_file(),
                "runtime input is not a regular file: {}",
                path.display()
            );
            anyhow::ensure!(
                metadata.len() <= limits.file_bytes,
                "runtime file exceeds {} bytes: {}",
                limits.file_bytes,
                path.display()
            );
            let key = (metadata.dev(), metadata.ino());
            let (identity, anchor) = if let Some((prior, identity, anchor)) = contents.get(&key) {
                anyhow::ensure!(
                    stamp(prior) == stamp(&metadata),
                    "runtime hardlink changed while binding"
                );
                (identity.clone(), anchor.clone())
            } else {
                total = total
                    .checked_add(metadata.len())
                    .context("runtime byte count overflow")?;
                anyhow::ensure!(
                    total <= limits.bytes,
                    "runtime closure exceeds {} unique bytes",
                    limits.bytes
                );
                let cached = cache
                    .files
                    .get(&key)
                    .filter(|(before, _)| *before == stamp(&metadata))
                    .map(|(_, identity)| identity.clone());
                let identity = if let Some(identity) = cached {
                    identity
                } else {
                    // NONBLOCK prevents a file-to-FIFO replacement from hanging;
                    // NOFOLLOW rejects a final-component link replacement.
                    let mut file = fs::File::from(open(
                        &path,
                        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK,
                        Mode::empty(),
                    )?);
                    anyhow::ensure!(
                        file.metadata()?.is_file() && stamp(&file.metadata()?) == stamp(&metadata),
                        "runtime input replaced before read"
                    );
                    let mut hash = Sha256::new();
                    let mut buffer = [0u8; 65536];
                    let mut bytes = 0u64;
                    loop {
                        let n = file.read(&mut buffer)?;
                        if n == 0 {
                            break;
                        }
                        bytes += n as u64;
                        anyhow::ensure!(
                            bytes <= metadata.len() && bytes <= limits.file_bytes,
                            "runtime input grew during read"
                        );
                        hash.update(&buffer[..n]);
                    }
                    anyhow::ensure!(
                        bytes == metadata.len() && stamp(&file.metadata()?) == stamp(&metadata),
                        "runtime input changed during read"
                    );
                    let identity: String = hash
                        .finalize()
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect();
                    #[cfg(test)]
                    {
                        cache.bytes_read += bytes;
                    }
                    // Bound cache storage independently of how many different
                    // historical closures this one read happens to encounter.
                    if cache.files.len() < 100_000 || cache.files.contains_key(&key) {
                        cache
                            .files
                            .insert(key, (stamp(&metadata), identity.clone()));
                    }
                    identity
                };
                contents.insert(key, (metadata.clone(), identity.clone(), path.clone()));
                (identity, path.clone())
            };
            json!({"kind":"file","mode":metadata.mode(),"bytes":metadata.len(),"sha256":identity,"hardlink_anchor":anchor})
        };
        graph.watched.insert(path.clone(), metadata);
        graph.insert(path, value)?;
        anyhow::ensure!(
            pending.len() + graph.nodes.len() <= limits.entries * 2,
            "runtime pending graph exceeds bounded traversal"
        );
    }
    // Reject changes during this graph sample, including cache hits whose
    // bytes were read earlier in this same request. No cache survives a new
    // refresh; directory and alias membership is always freshly enumerated.
    for (path, before) in &graph.watched {
        anyhow::ensure!(
            stamp(&fs::symlink_metadata(path)?) == stamp(before),
            "runtime graph changed while binding: {}",
            path.display()
        );
    }
    Ok((
        hash_json(&("thruline.runtime-graph.v1", declared, graph.nodes))?,
        total,
    ))
}

#[cfg(not(unix))]
fn runtime_graph(
    _root: &Path,
    _paths: &[PathBuf],
    _limits: RuntimeLimits,
) -> Result<(String, u64)> {
    bail!("This host has no qualified bounded runtime graph reader")
}
#[cfg(not(unix))]
fn runtime_graph_with_cache(
    root: &Path,
    paths: &[PathBuf],
    limits: RuntimeLimits,
    _cache: &mut RuntimeBytes,
) -> Result<(String, u64)> {
    runtime_graph(root, paths, limits)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn overlapping_runtime_closures_read_shared_bytes_once_without_changing_identity() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::create_dir(root.join("common")).unwrap();
        fs::write(root.join("common/library"), "shared library").unwrap();
        fs::write(root.join("tool-a"), "first tool").unwrap();
        fs::write(root.join("tool-b"), "second tool").unwrap();
        let a = vec![root.join("common"), root.join("tool-a")];
        let b = vec![root.join("common"), root.join("tool-b")];
        let mut cache = RuntimeBytes::default();
        let first =
            runtime_graph_with_cache(&root, &a, RuntimeLimits::default(), &mut cache).unwrap();
        let second =
            runtime_graph_with_cache(&root, &b, RuntimeLimits::default(), &mut cache).unwrap();
        assert_eq!(
            first,
            runtime_graph(&root, &a, RuntimeLimits::default()).unwrap()
        );
        assert_eq!(
            second,
            runtime_graph(&root, &b, RuntimeLimits::default()).unwrap()
        );
        assert_eq!(
            cache.bytes_read,
            ("shared library".len() + "first tool".len() + "second tool".len()) as u64
        );
        // A cache hit still consumes the closure's unique-byte budget.
        assert!(runtime_graph_with_cache(
            &root,
            &b,
            RuntimeLimits {
                bytes: second.1 - 1,
                ..RuntimeLimits::default()
            },
            &mut cache
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn runtime_cache_rechecks_changed_files_links_and_next_request() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::write(root.join("library-a"), "aaaa").unwrap();
        fs::write(root.join("library-b"), "bbbb").unwrap();
        symlink("library-a", root.join("current")).unwrap();
        let paths = vec![root.join("current")];
        let mut cache = RuntimeBytes::default();
        let before =
            runtime_graph_with_cache(&root, &paths, RuntimeLimits::default(), &mut cache).unwrap();
        let old_time = fs::metadata(root.join("library-a"))
            .unwrap()
            .modified()
            .unwrap();
        fs::write(root.join("library-a"), "cccc").unwrap();
        fs::File::open(root.join("library-a"))
            .unwrap()
            .set_modified(old_time)
            .unwrap();
        let changed =
            runtime_graph_with_cache(&root, &paths, RuntimeLimits::default(), &mut cache).unwrap();
        assert_ne!(before.0, changed.0);
        assert_eq!(
            changed,
            runtime_graph(&root, &paths, RuntimeLimits::default()).unwrap()
        );
        fs::remove_file(root.join("current")).unwrap();
        symlink("library-b", root.join("current")).unwrap();
        let retargeted =
            runtime_graph_with_cache(&root, &paths, RuntimeLimits::default(), &mut cache).unwrap();
        assert_ne!(changed.0, retargeted.0);
        assert_eq!(
            retargeted,
            runtime_graph(&root, &paths, RuntimeLimits::default()).unwrap()
        );
        let mut next_request = RuntimeBytes::default();
        assert_eq!(
            retargeted,
            runtime_graph_with_cache(&root, &paths, RuntimeLimits::default(), &mut next_request)
                .unwrap()
        );
        assert_eq!(next_request.bytes_read, 4);
    }

    #[test]
    fn another_read_rehashes_changed_bytes_without_trusting_mtime() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rules");
        fs::write(&path, "first").unwrap();
        let paths = vec![path.clone()];
        let mut read = InputRead::default();
        let first = read.files(dir.path(), &paths).unwrap();
        assert_eq!(first, hash_files(dir.path(), &paths).unwrap());
        fs::write(&path, "other").unwrap(); // same byte length
        let mut next = InputRead::default();
        let changed = next.files(dir.path(), &paths).unwrap();
        assert_ne!(first, changed);
        assert_eq!(changed, hash_files(dir.path(), &paths).unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn runtime_alias_graph_terminates_directory_cycles_and_binds_targets() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::create_dir(root.join("v1")).unwrap();
        fs::create_dir(root.join("v2")).unwrap();
        fs::write(root.join("v1/lib"), "same").unwrap();
        fs::write(root.join("v2/lib"), "same").unwrap();
        symlink("v1", root.join("Current")).unwrap();
        symlink("..", root.join("v1/parent")).unwrap();
        let paths = vec![root.join("Current")];
        let first = hash_runtime(&root, &paths).unwrap();
        assert_eq!(first, hash_runtime(&root, &paths).unwrap());
        fs::remove_file(root.join("Current")).unwrap();
        symlink("v2", root.join("Current")).unwrap();
        assert_ne!(first, hash_runtime(&root, &paths).unwrap());
        let before = hash_runtime(&root, &[root.join("Current/lib")]).unwrap();
        fs::write(root.join("v2/lib"), "edit").unwrap();
        assert_ne!(
            before,
            hash_runtime(&root, &[root.join("Current/lib")]).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn runtime_deduplicates_physical_content_but_preserves_alias_identity() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::write(root.join("data"), "12345").unwrap();
        fs::hard_link(root.join("data"), root.join("hardlink")).unwrap();
        symlink("data", root.join("symlink")).unwrap();
        let limits = RuntimeLimits {
            entries: 20,
            bytes: 5,
            file_bytes: 5,
        };
        let (before, bytes) = runtime_graph(&root, std::slice::from_ref(&root), limits).unwrap();
        assert_eq!(bytes, 5);
        fs::remove_file(root.join("hardlink")).unwrap();
        fs::write(root.join("hardlink"), "12345").unwrap();
        assert!(runtime_graph(&root, std::slice::from_ref(&root), limits).is_err());
        let (after, bytes) = runtime_graph(
            &root,
            std::slice::from_ref(&root),
            RuntimeLimits {
                bytes: 10,
                ..limits
            },
        )
        .unwrap();
        assert_eq!(bytes, 10);
        assert_ne!(before, after);
        assert!(runtime_graph(
            &root,
            std::slice::from_ref(&root),
            RuntimeLimits {
                entries: 2,
                bytes: 10,
                ..limits
            }
        )
        .is_err());
        assert!(runtime_graph(
            &root,
            std::slice::from_ref(&root),
            RuntimeLimits {
                file_bytes: 4,
                bytes: 10,
                ..limits
            }
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn runtime_reader_rejects_broken_links_link_cycles_and_special_files() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        symlink("absent", root.join("broken")).unwrap();
        assert!(hash_runtime(&root, &[root.join("broken")]).is_err());
        symlink("b", root.join("a")).unwrap();
        symlink("a", root.join("b")).unwrap();
        assert!(hash_runtime(&root, &[root.join("a")]).is_err());
        assert!(std::process::Command::new("mkfifo")
            .arg(root.join("fifo"))
            .status()
            .unwrap()
            .success());
        assert!(hash_runtime(&root, &[root.join("fifo")]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn source_and_rules_readers_remain_strict_and_runtime_cache_is_request_local() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::create_dir(root.join("runtime")).unwrap();
        fs::write(root.join("runtime/lib"), "first").unwrap();
        symlink("runtime", root.join("alias")).unwrap();
        assert!(hash_files(&root, std::slice::from_ref(&root)).is_err());
        let mut first_read = InputRead::default();
        let before = first_read
            .runtime(&root, std::slice::from_ref(&root))
            .unwrap();
        fs::write(root.join("runtime/lib"), "other").unwrap();
        let mut next_read = InputRead::default();
        let after = next_read
            .runtime(&root, std::slice::from_ref(&root))
            .unwrap();
        assert_ne!(before, after);
    }

    /// Opt-in actual installation audit: never a replacement for tool execution.
    #[cfg(unix)]
    #[test]
    #[ignore = "reads each explicitly selected installed runtime; set THRULINE_RUNTIME_AUDIT_REGISTRY"]
    fn registered_runtime_graphs_are_bounded() {
        let registry = std::env::var("THRULINE_RUNTIME_AUDIT_REGISTRY").expect("explicit registry");
        let value: Value = serde_json::from_slice(&fs::read(registry).unwrap()).unwrap();
        for (pack, tool) in value["tools"].as_object().unwrap() {
            let paths: Vec<PathBuf> = serde_json::from_value(tool["runtime"].clone()).unwrap();
            let started = std::time::Instant::now();
            let (identity, bytes) = runtime_graph(
                &std::env::current_dir().unwrap(),
                &paths,
                RuntimeLimits::default(),
            )
            .unwrap_or_else(|error| panic!("{pack}: {error:#}"));
            println!(
                "{}",
                json!({"pack":pack,"sha256":identity,"unique_bytes":bytes,"duration_ms":started.elapsed().as_millis()})
            );
        }
    }
}
