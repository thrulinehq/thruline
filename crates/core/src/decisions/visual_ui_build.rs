//! External generated UI workspace; exact tracked source stays internal.
pub use super::visual_build_legacy::{supports_closed_design, DesignBuild, GeneratedHousekeeping};
use super::visual_cache::{key, VerifiedFile, VisualCache};
use super::visual_cargo_target::{CargoTarget, DirectoryPin};
use super::visual_profile::VisualProfile;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
const GIB: u64 = 1 << 30;

pub(super) fn build_external_ui(
    repo: &Path,
    ui: &Path,
    target: &CargoTarget,
    npm_cache: &Path,
    profile: &VisualProfile,
    design: bool,
) -> Result<UiWorkspace, &'static str> {
    let ui = ui
        .canonicalize()
        .map_err(|_| "exact UI source unavailable")?;
    let source = ui.parent().ok_or("exact source parent unavailable")?;
    let source_volume = DirectoryPin::open(source)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let source_metadata = std::fs::metadata(source).map_err(|_| "source volume unavailable")?;
        let repository = std::fs::metadata(repo).map_err(|_| "repository volume unavailable")?;
        if source_metadata.dev() != repository.dev() {
            return Err("exact source must stay on the internal repository volume");
        }
    }
    if source_volume.free()? < 15 * GIB {
        return Err("internal source volume is below 15 GiB");
    }
    let mut source_files = verified_files(&generated_files(source)?)?;
    let mut workspace = UiWorkspace::prepare(
        &target.target,
        &ui,
        &key(&("generated-ui-v1", &target.target)),
    )?;
    workspace.remap_preserved_source(&mut source_files)?;
    workspace.source_files = source_files;
    {
        let _stage = profile.span("npm_install");
        let destination =
            DirectoryPin::open(&workspace.npm_cache)?.child(OsStr::new("_cacache"))?;
        copy_cache(
            &npm_cache.join("_cacache"),
            &destination,
            &mut (0u64, 0usize),
            &mut || {
                workspace.verify_staging()?;
                target.check_running()?;
                if source_volume.free()? < 15 * GIB {
                    return Err("internal source volume fell below 15 GiB");
                }
                Ok(())
            },
        )?;
        let mut command = Command::new("npm");
        command
            .args(["ci", "--offline", "--no-audit", "--no-fund", "--prefix"])
            .arg(&workspace.root)
            .current_dir(&ui);
        workspace.configure(&mut command);
        let out = target.run_with_checks(&mut command, || workspace.verify_staging())?;
        if !out.status.success() {
            return Err("exact UI dependencies unavailable offline");
        }
    }
    {
        let _stage = profile.span("npm_build");
        let mut command = Command::new("npm");
        command
            .args(["run", "build", "--", "--outDir"])
            .arg(&workspace.dist)
            .arg("--emptyOutDir")
            .env("VITE_HQ_DESIGN", if design { "1" } else { "0" })
            .current_dir(&ui);
        workspace.configure(&mut command);
        let out = target.run_with_checks(&mut command, || workspace.verify_staging())?;
        if !out.status.success() {
            return Err("exact UI build failed");
        }
    }
    workspace.verify()?;
    let paths = generated_files(&workspace.dist)?;
    if !paths
        .iter()
        .any(|path| path == &workspace.dist.join("index.html"))
    {
        return Err("exact UI build has no regular index.html");
    }
    workspace.artifact_files = verified_files(&paths)?;
    workspace.verify()?;
    Ok(workspace)
}

pub(super) fn exact_design_build(
    repo: &Path,
    commit: &str,
    tree: &str,
    cache: &VisualCache,
    profile: &VisualProfile,
) -> Result<DesignBuild, &'static str> {
    if std::env::var_os("THRULINE_VISUAL_CARGO_TARGET_ROOT").is_none() {
        return super::visual_build_legacy::exact_design_build(repo, commit, tree, cache, profile);
    }
    let node = super::tool_version("node", "--version").ok_or("Node version unavailable")?;
    let npm = super::tool_version("npm", "--version").ok_or("npm version unavailable")?;
    let mut target = CargoTarget::from_env(commit)?;
    let cache_key = key(&(
        tree,
        "design-v2-external-generated",
        node,
        npm,
        &target.target,
    ));
    if let Some(built) = cache.load::<DesignBuild>("build", &cache_key) {
        if cached_build_matches(&target.target, &built)? {
            return Ok(built);
        }
    }
    target.check_start()?;
    if !supports_closed_design(repo, commit) {
        return Err("exact source does not prove a closed design mode");
    }
    let source_volume = DirectoryPin::open(cache.root())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let repository = std::fs::metadata(repo).map_err(|_| "repository volume unavailable")?;
        let scratch =
            std::fs::metadata(cache.root()).map_err(|_| "source cache volume unavailable")?;
        if repository.dev() != scratch.dev() {
            return Err("source cache must stay on the internal repository volume");
        }
    }
    if source_volume.free()? < 15 * GIB {
        return Err("internal source volume is below 15 GiB");
    }
    target.prepare()?;
    let caches = super::build_caches().ok_or("offline build caches are unavailable")?;
    let work =
        tempfile::tempdir_in(cache.root()).map_err(|_| "design build scratch unavailable")?;
    let source = work.path().join("source");
    std::fs::create_dir(&source).map_err(|_| "design build scratch unavailable")?;
    let archive = work.path().join("source.tar");
    {
        let _stage = profile.span("archive");
        let status = crate::git_isolation::isolate_config(&mut Command::new("git"))
            .args(["archive", "--format=tar", "--output"])
            .arg(&archive)
            .arg(commit)
            .current_dir(repo)
            .status()
            .map_err(|_| "exact source archive unavailable")?;
        if !status.success() {
            return Err("exact source archive failed");
        }
        let unpack = Command::new("tar")
            .arg("-xf")
            .arg(&archive)
            .arg("-C")
            .arg(&source)
            .status()
            .map_err(|_| "exact source unpack unavailable")?;
        if !unpack.success() {
            return Err("exact source unpack failed");
        }
    }
    let workspace = build_external_ui(
        repo,
        &source.join("ui"),
        &target,
        &caches.npm_cache,
        profile,
        true,
    )?;
    let paths = generated_files(&workspace.dist)?;
    if !paths
        .iter()
        .any(|p| p == &workspace.dist.join("index.html"))
    {
        return Err("exact UI build has no regular index.html");
    }
    let files = verified_files(&paths)?;
    let built = build_manifest(&workspace.dist, &files)?;
    cache
        .save_verified("build", &cache_key, &built, files)
        .map_err(|_| "design build manifest cannot be preserved")?;
    Ok(built)
}

pub(super) struct UiWorkspace {
    root: PathBuf,
    pub(super) dist: PathBuf,
    npm_cache: PathBuf,
    temporary: PathBuf,
    source_ui: PathBuf,
    pins: Vec<DirectoryPin>,
    manifests: Vec<VerifiedFile>,
    preserved_dist: Option<PathBuf>,
    preserved_files: Vec<VerifiedFile>,
    configs: Vec<VerifiedFile>,
    source_files: Vec<VerifiedFile>,
    artifact_files: Vec<VerifiedFile>,
}
impl UiWorkspace {
    fn prepare(target: &Path, source_ui: &Path, cache_key: &str) -> Result<Self, &'static str> {
        if cache_key.len() != 64 || !cache_key.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("UI build key is invalid");
        }
        let source_ui = source_ui
            .canonicalize()
            .map_err(|_| "exact UI source is unavailable")?;
        let source_pin = DirectoryPin::open(&source_ui)?;
        let manifests = ["package.json", "package-lock.json"]
            .iter()
            .map(|name| {
                VerifiedFile::read(&source_ui.join(name))
                    .ok_or("exact UI source manifests must be regular files")
            })
            .collect::<Result<Vec<_>, _>>()?;
        if std::fs::symlink_metadata(source_ui.join("node_modules")).is_ok() {
            return Err("exact source already contains generated UI dependencies");
        }
        let tracked_dist = source_ui.join("dist");
        let mut preserved_files = if std::fs::symlink_metadata(&tracked_dist).is_ok() {
            verified_files(&generated_files(&tracked_dist)?)?
        } else {
            Vec::new()
        };
        let target_pin = DirectoryPin::open(target)?;
        let parent = target_pin.child(OsStr::new("ui-build"))?;
        let root_pin = parent.child(OsStr::new(&format!(
            "{cache_key}-{}",
            ulid::Ulid::generate()
        )))?;
        let root = root_pin.path().to_path_buf();
        let dist_pin = root_pin.child(OsStr::new("dist"))?;
        let cache_pin = root_pin.child(OsStr::new("npm-cache"))?;
        let tmp_pin = root_pin.child(OsStr::new("tmp"))?;
        let dist = dist_pin.path().to_path_buf();
        let npm_cache = cache_pin.path().to_path_buf();
        let temporary = tmp_pin.path().to_path_buf();
        let mut configs = Vec::new();
        for name in ["npm-user.conf", "npm-global.conf"] {
            drop(root_pin.create_file(OsStr::new(name))?);
            configs.push(
                VerifiedFile::read(&root.join(name))
                    .ok_or("generated npm configuration cannot be pinned")?,
            );
        }
        let mut pins = vec![
            source_pin, target_pin, parent, root_pin, dist_pin, cache_pin, tmp_pin,
        ];
        let preserved_dist = if std::fs::symlink_metadata(&tracked_dist).is_ok() {
            let source_parent =
                DirectoryPin::open(source_ui.parent().ok_or("source parent unavailable")?)?;
            let preserved = source_parent.path().join(format!(
                ".thruline-c1-preserved-ui-dist-{}",
                ulid::Ulid::generate()
            ));
            source_parent.verify()?;
            std::fs::rename(&tracked_dist, &preserved)
                .map_err(|_| "tracked UI dist cannot be preserved internally")?;
            source_parent.verify()?;
            for file in &mut preserved_files {
                let relative = file
                    .path
                    .strip_prefix(&tracked_dist)
                    .map_err(|_| "tracked UI dist path leaves source")?;
                file.path = preserved.join(relative);
                if !file.remains_verified() {
                    return Err("tracked UI dist changed while preserved");
                }
            }
            pins.push(DirectoryPin::open(&preserved)?);
            pins.push(source_parent);
            Some(preserved)
        } else {
            None
        };
        let workspace = Self {
            dist,
            npm_cache,
            temporary,
            root,
            source_ui,
            manifests,
            pins,
            preserved_dist,
            preserved_files,
            configs,
            source_files: Vec::new(),
            artifact_files: Vec::new(),
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            for name in ["package.json", "package-lock.json"] {
                symlink(workspace.source_ui.join(name), workspace.root.join(name))
                    .map_err(|_| "UI manifest link cannot be created")?;
            }
            for name in ["node_modules", "dist"] {
                symlink(workspace.root.join(name), workspace.source_ui.join(name))
                    .map_err(|_| "generated UI staging link cannot be created")?;
            }
        }
        #[cfg(not(unix))]
        {
            return Err("external UI staging is unavailable on this platform");
        }
        workspace.verify()?;
        Ok(workspace)
    }
    pub(super) fn verify(&self) -> Result<(), &'static str> {
        self.verify_staging()?;
        if !self
            .preserved_files
            .iter()
            .all(VerifiedFile::remains_verified)
        {
            return Err("preserved tracked UI dist changed");
        }
        if !self.source_files.iter().all(VerifiedFile::remains_verified) {
            return Err("exact archived source changed during build");
        }
        if !self
            .artifact_files
            .iter()
            .all(VerifiedFile::remains_verified)
        {
            return Err("generated UI artifacts changed after build");
        }
        Ok(())
    }
    pub(super) fn verify_staging(&self) -> Result<(), &'static str> {
        for pin in &self.pins {
            pin.verify()?;
        }
        if !self.manifests.iter().all(VerifiedFile::remains_verified) {
            return Err("exact UI source manifest changed");
        }
        if !self.configs.iter().all(VerifiedFile::remains_verified) {
            return Err("generated npm configuration changed");
        }
        for name in ["package.json", "package-lock.json"] {
            verify_link(&self.root.join(name), &self.source_ui.join(name))?;
        }
        for name in ["node_modules", "dist"] {
            verify_link(&self.source_ui.join(name), &self.root.join(name))?;
        }
        Ok(())
    }
    fn remap_preserved_source(&self, files: &mut [VerifiedFile]) -> Result<(), &'static str> {
        let original = self.source_ui.join("dist");
        for file in files {
            if let Ok(relative) = file.path.strip_prefix(&original) {
                file.path = self
                    .preserved_dist
                    .as_ref()
                    .ok_or("tracked dist preservation is missing")?
                    .join(relative);
                if !file.remains_verified() {
                    return Err("tracked dist source pin changed");
                }
            }
        }
        Ok(())
    }
    fn configure(&self, command: &mut Command) {
        command
            .env("npm_config_cache", &self.npm_cache)
            .env("npm_config_logs_dir", self.npm_cache.join("_logs"))
            .env("TMPDIR", &self.temporary)
            .env("TMP", &self.temporary)
            .env("TEMP", &self.temporary)
            .env("npm_config_userconfig", self.root.join("npm-user.conf"))
            .env("npm_config_globalconfig", self.root.join("npm-global.conf"));
    }
}
fn verify_link(path: &Path, expected: &Path) -> Result<(), &'static str> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| "UI staging link unavailable")?;
    if !meta.file_type().is_symlink() || std::fs::read_link(path).ok().as_deref() != Some(expected)
    {
        return Err("UI staging link changed");
    }
    Ok(())
}
fn generated_files(root: &Path) -> Result<Vec<PathBuf>, &'static str> {
    let pin = DirectoryPin::open(root)?;
    let mut result = Vec::new();
    walk(&pin, &mut result)?;
    result.sort();
    Ok(result)
}
fn walk(pin: &DirectoryPin, files: &mut Vec<PathBuf>) -> Result<(), &'static str> {
    pin.verify()?;
    for entry in std::fs::read_dir(pin.path()).map_err(|_| "design assets unavailable")? {
        let entry = entry.map_err(|_| "design asset unavailable")?;
        let kind = entry.file_type().map_err(|_| "design asset unavailable")?;
        if kind.is_dir() {
            let child = DirectoryPin::open(&entry.path())?;
            walk(&child, files)?;
        } else if kind.is_file() {
            files.push(entry.path());
            if files.len() > 8192 {
                return Err("design asset list too large");
            }
        } else {
            return Err("design asset is not a regular file or directory");
        }
    }
    pin.verify()
}
fn verified_files(paths: &[PathBuf]) -> Result<Vec<VerifiedFile>, &'static str> {
    paths
        .iter()
        .map(|p| VerifiedFile::read(p).ok_or("design asset cannot be verified"))
        .collect()
}
fn copy_cache(
    source: &Path,
    target: &DirectoryPin,
    copied: &mut (u64, usize),
    guard: &mut impl FnMut() -> Result<(), &'static str>,
) -> Result<(), &'static str> {
    guard()?;
    let source_pin = DirectoryPin::open(source)?;
    for entry in
        std::fs::read_dir(source).map_err(|_| "offline npm package cache is unavailable")?
    {
        guard()?;
        source_pin.verify()?;
        let entry = entry.map_err(|_| "offline npm cache entry is unavailable")?;
        let kind = entry
            .file_type()
            .map_err(|_| "offline npm cache entry is unavailable")?;
        if kind.is_dir() {
            let child = target.child(&entry.file_name())?;
            copy_cache(&entry.path(), &child, copied, guard)?;
        } else if kind.is_file() {
            copied.1 += 1;
            if copied.1 > 65536 {
                return Err("offline npm cache exceeds file bound");
            }
            let mut output = target.create_file(&entry.file_name())?;
            let fd = rustix::fs::open(
                entry.path(),
                rustix::fs::OFlags::RDONLY
                    | rustix::fs::OFlags::NONBLOCK
                    | rustix::fs::OFlags::NOFOLLOW
                    | rustix::fs::OFlags::CLOEXEC,
                rustix::fs::Mode::empty(),
            )
            .map_err(|_| "offline npm cache entry cannot be read")?;
            let mut input = std::fs::File::from(fd);
            if !input
                .metadata()
                .map_err(|_| "offline npm cache entry identity unavailable")?
                .is_file()
            {
                return Err("offline npm cache entry is not regular");
            }
            let mut buffer = [0u8; 65536];
            loop {
                guard()?;
                let bytes = input
                    .read(&mut buffer)
                    .map_err(|_| "offline npm cache cannot be read")?;
                if bytes == 0 {
                    break;
                }
                copied.0 = copied
                    .0
                    .checked_add(bytes as u64)
                    .ok_or("offline npm cache size overflowed")?;
                if copied.0 > 2 * GIB {
                    return Err("offline npm cache exceeds byte bound");
                }
                output
                    .write_all(&buffer[..bytes])
                    .map_err(|_| "offline npm cache cannot be routed externally")?;
            }
        } else {
            return Err("offline npm cache entry is a symlink or special file");
        }
    }
    source_pin.verify()?;
    guard()
}
fn build_manifest(dist: &Path, files: &[VerifiedFile]) -> Result<DesignBuild, &'static str> {
    let mut digests = files
        .iter()
        .map(|file| {
            file.path
                .strip_prefix(dist)
                .map(|path| (path.to_string_lossy().into_owned(), file.sha256.clone()))
                .map_err(|_| "design asset leaves build")
        })
        .collect::<Result<Vec<_>, _>>()?;
    digests.sort();
    let housekeeping = vec![GeneratedHousekeeping {
        path: "/favicon.ico".into(),
        status: 204,
        body_sha256: super::digest(b""),
    }];
    let server_sha256 = key(&(
        super::digest(super::visual_static::IDENTITY.as_bytes()),
        &housekeeping,
    ));
    Ok(DesignBuild {
        dist: dist.to_path_buf(),
        ui_sha256: key(&digests),
        server_sha256,
        allowed_paths: digests
            .iter()
            .map(|(path, _)| {
                if path == "index.html" {
                    "/".into()
                } else {
                    format!("/{path}")
                }
            })
            .collect(),
        housekeeping,
    })
}
fn cached_build_matches(target: &Path, built: &DesignBuild) -> Result<bool, &'static str> {
    if !built.dist.starts_with(target.join("ui-build"))
        || built.dist.canonicalize().ok().as_deref() != Some(&built.dist)
    {
        return Ok(false);
    }
    let files = verified_files(&generated_files(&built.dist)?)?;
    let actual = build_manifest(&built.dist, &files)?;
    Ok(actual.ui_sha256 == built.ui_sha256
        && actual.server_sha256 == built.server_sha256
        && actual.allowed_paths == built.allowed_paths)
}
#[cfg(test)]
#[path = "visual_ui_build_tests.rs"]
mod tests;
