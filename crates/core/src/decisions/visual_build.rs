//! Exact-source design builds with a verified complete dist manifest.
use super::visual_cache::{key, VerifiedFile, VisualCache};
use super::visual_profile::VisualProfile;
use super::{build_caches, enough_space_for_build, git_text, tool_version};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignBuild {
    pub dist: PathBuf,
    pub ui_sha256: String,
    pub server_sha256: String,
    pub allowed_paths: Vec<String>,
    pub housekeeping: Vec<GeneratedHousekeeping>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedHousekeeping {
    pub path: String,
    pub status: u16,
    pub body_sha256: String,
}
/// Source branch plus runtime's closed asset guard makes the chosen mode
/// explicit. A changed source that reads records cannot obtain a valid capture.
pub fn supports_closed_design(repo: &Path, commit: &str) -> bool {
    let Some(main) = git_text(repo, &format!("{commit}:ui/src/main.jsx")) else {
        return false;
    };
    let Some(boards) = git_text(repo, &format!("{commit}:ui/src/hq/artboards.jsx")) else {
        return false;
    };
    main.contains("VITE_HQ_DESIGN")
        && main.contains("DESIGN.test(location.hash)")
        && main.contains("await import('./hq/artboards.jsx')")
        && main.contains("window.addEventListener('hashchange', draw)\n    return")
        && boards.contains("props={{}}")
}
pub(super) fn exact_design_build(
    repo: &Path,
    commit: &str,
    tree: &str,
    cache: &VisualCache,
    profile: &VisualProfile,
) -> Result<DesignBuild, &'static str> {
    let node = tool_version("node", "--version").ok_or("Node version unavailable")?;
    let npm = tool_version("npm", "--version").ok_or("npm version unavailable")?;
    let cache_key = key(&(tree, "design-v1-full-source", node, npm));
    if let Some(mut built) = cache.load::<DesignBuild>("build", &cache_key) {
        // UI artifacts can survive a capture transport revision, but the
        // current server implementation must bind every new capture key.
        let identity = current_server_identity();
        if built.server_sha256 != identity {
            built.server_sha256 = identity;
            built.housekeeping = generated_housekeeping();
            let mut files = Vec::new();
            walk(&built.dist, &built.dist, &mut files)?;
            let _ = cache.save("build", &cache_key, &built, &files);
        }
        return Ok(built);
    }
    if !supports_closed_design(repo, commit) {
        return Err("exact source does not prove a closed design mode");
    }
    if !enough_space_for_build(cache.root()) {
        return Err("visual builds require at least 25 GiB free");
    }
    let caches = build_caches().ok_or("offline build caches are unavailable")?;
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
        };
        let unpack = Command::new("tar")
            .args(["-xf"])
            .arg(&archive)
            .arg("-C")
            .arg(&source)
            .status()
            .map_err(|_| "exact source unpack unavailable")?;
        if !unpack.success() {
            return Err("exact source unpack failed");
        }
    }
    let ui = source.join("ui");
    if !ui.join("package-lock.json").is_file() {
        return Err("exact version has no pinned UI dependencies");
    }
    {
        let _stage = profile.span("npm_install");
        let out = Command::new("npm")
            .args(["ci", "--offline", "--no-audit", "--no-fund"])
            .env("npm_config_cache", &caches.npm_cache)
            .current_dir(&ui)
            .stdout(Stdio::null())
            .output()
            .map_err(|_| "UI dependency installer unavailable")?;
        if !out.status.success() {
            return Err("exact UI dependencies unavailable offline");
        }
    }
    {
        let _stage = profile.span("npm_build");
        let out = Command::new("npm")
            .args(["run", "build"])
            .env("VITE_HQ_DESIGN", "1")
            .current_dir(&ui)
            .stdout(Stdio::null())
            .output()
            .map_err(|_| "UI builder unavailable")?;
        if !out.status.success() || !ui.join("dist/index.html").is_file() {
            return Err("exact UI build failed");
        }
    }
    let stage = cache.root().join(format!("dist-{cache_key}"));
    if stage.exists() {
        std::fs::remove_dir_all(&stage).map_err(|_| "stale design build cannot be replaced")?
    }
    std::fs::rename(ui.join("dist"), &stage).map_err(|_| "design build cannot be preserved")?;
    let mut paths = Vec::new();
    walk(&stage, &stage, &mut paths)?;
    paths.sort();
    if paths.len() > 8192 {
        return Err("design asset list too large");
    }
    let files: Vec<_> = paths
        .iter()
        .map(|p| VerifiedFile::read(p).ok_or("design asset cannot be verified"))
        .collect::<Result<_, _>>()?;
    let digests: Vec<_> = files
        .iter()
        .map(|f| {
            (
                f.path
                    .strip_prefix(&stage)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                f.sha256.clone(),
            )
        })
        .collect();
    let allowed_paths = digests
        .iter()
        .map(|(p, _)| {
            if p == "index.html" {
                "/".into()
            } else {
                format!("/{p}")
            }
        })
        .collect();
    let built = DesignBuild {
        dist: stage,
        ui_sha256: key(&digests),
        server_sha256: current_server_identity(),
        allowed_paths,
        housekeeping: generated_housekeeping(),
    };
    let _ = cache.save_verified("build", &cache_key, &built, files);
    Ok(built)
}
fn walk(root: &Path, path: &Path, out: &mut Vec<PathBuf>) -> Result<(), &'static str> {
    for entry in std::fs::read_dir(path).map_err(|_| "design assets unavailable")? {
        let entry = entry.map_err(|_| "design assets unavailable")?;
        let p = entry.path();
        let kind = entry.file_type().map_err(|_| "design asset unavailable")?;
        if kind.is_symlink() {
            return Err("design asset is a symlink");
        }
        if kind.is_dir() {
            walk(root, &p, out)?
        } else if kind.is_file() {
            if !p.starts_with(root) {
                return Err("design asset leaves build");
            };
            out.push(p);
            if out.len() > 8192 {
                return Err("design asset list too large");
            }
        }
    }
    Ok(())
}

fn generated_housekeeping() -> Vec<GeneratedHousekeeping> {
    vec![GeneratedHousekeeping {
        path: "/favicon.ico".into(),
        status: 204,
        body_sha256: super::digest(b""),
    }]
}
fn current_server_identity() -> String {
    key(&(
        super::digest(super::visual_static::IDENTITY.as_bytes()),
        generated_housekeeping(),
    ))
}
