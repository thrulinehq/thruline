//! Verified capture reuse for historical exact-backend replays.
use super::visual_cache::VisualCache;
use super::CaptureReceipt;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoricalScreen {
    pub screen: String,
    pub route: String,
    pub scenario: String,
    pub legacy_route: Option<String>,
    pub legacy_scenario: Option<String>,
    pub width: u32,
    pub height: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoricalInput {
    pub implementation: String,
    pub commit_oid: String,
    pub tree_hash: String,
    pub capture_tool_sha256: String,
    pub fixture_sha256: String,
    pub renderer_sha256: String,
    pub renderer_framework_sha256: String,
    pub renderer_version: String,
    pub capture_clock: String,
    pub screens: Vec<HistoricalScreen>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoricalBuild {
    pub ui_sha256: String,
    pub server_sha256: String,
    pub files: Vec<PathBuf>,
}
pub fn preserve_build(
    cache: &VisualCache,
    dist: &Path,
    server: &Path,
) -> Result<HistoricalBuild, String> {
    let artifact = tempfile::Builder::new()
        .prefix("history-build-")
        .tempdir_in(cache.root())
        .map_err(|_| "historical artifact storage unavailable")?;
    let root = artifact.path();
    let mut source = Vec::new();
    walk(dist, dist, &mut source)?;
    if source.is_empty() || !dist.join("index.html").is_file() {
        return Err("historical UI build is incomplete".into());
    }
    source.sort();
    let server_copy = root.join("server");
    copy_stable(server, &server_copy)?;
    let mut files = vec![server_copy];
    for original in source {
        let relative = original
            .strip_prefix(dist)
            .map_err(|_| "historical asset leaves dist")?;
        let copy = root.join("dist").join(relative);
        std::fs::create_dir_all(copy.parent().unwrap())
            .map_err(|_| "historical asset storage unavailable")?;
        copy_stable(&original, &copy)?;
        files.push(copy);
    }
    let mut build = HistoricalBuild {
        ui_sha256: String::new(),
        server_sha256: String::new(),
        files,
    };
    let (ui, server, _) = build_identity(cache, &build)?;
    build.ui_sha256 = ui;
    build.server_sha256 = server;
    let _ = artifact.keep();
    Ok(build)
}
pub fn save_captures(
    cache: &VisualCache,
    input: &HistoricalInput,
    build: &HistoricalBuild,
    receipts: &[CaptureReceipt],
) -> Result<(), String> {
    if !valid_input(input) || receipts.len() != input.screens.len() {
        return Err("historical input or receipt count is invalid".into());
    }
    let (ui, server, proof) = build_identity(cache, build)?;
    if ui != build.ui_sha256 || server != build.server_sha256 {
        return Err("historical build digest does not match exact artifact bytes".into());
    }
    let mut keys = Vec::new();
    for (screen, receipt) in input.screens.iter().zip(receipts) {
        if !receipt_matches(input, screen, receipt) {
            return Err("historical receipt is missing, failed or mismatched".into());
        }
        let capture_key = capture_key(input, screen, build);
        let mut saved = receipt.clone();
        let mut files = Vec::new();
        if receipt.route_found {
            let png = receipt
                .screenshot_path
                .as_ref()
                .ok_or("historical pixels are missing")?;
            let dir = cache.root().join("history-pixels");
            std::fs::create_dir_all(&dir).map_err(|_| "historical pixel storage unavailable")?;
            let copy = dir.join(format!("{capture_key}.png"));
            copy_stable(png, &copy)?;
            saved.screenshot_path = Some(copy.clone());
            files.push(copy);
        }
        cache.save("history-capture", &capture_key, &saved, &files)?;
        keys.push(capture_key);
    }
    let index = HistoryIndex {
        build: build.clone(),
        capture_keys: keys,
    };
    cache.save_verified(
        "history-index",
        &super::visual_cache::key(input),
        &index,
        proof,
    )
}
pub fn load_captures(cache: &VisualCache, input: &HistoricalInput) -> Option<Vec<CaptureReceipt>> {
    if !valid_input(input) {
        return None;
    }
    let index: HistoryIndex = cache.load("history-index", &super::visual_cache::key(input))?;
    if index.capture_keys.len() != input.screens.len() {
        return None;
    }
    let mut receipts = Vec::new();
    for (screen, key) in input.screens.iter().zip(&index.capture_keys) {
        if key != &capture_key(input, screen, &index.build) {
            return None;
        }
        let receipt: CaptureReceipt = cache.load("history-capture", key)?;
        if !receipt_matches(input, screen, &receipt) {
            return None;
        }
        receipts.push(receipt);
    }
    Some(receipts)
}
/// These keys are available only while the exact UI/server and every capture
/// artifact remain verified. Result keys add the accepted revision/reference.
pub fn bound_capture_keys(cache: &VisualCache, input: &HistoricalInput) -> Option<Vec<String>> {
    load_captures(cache, input)?;
    let index: HistoryIndex = cache.load("history-index", &super::visual_cache::key(input))?;
    Some(index.capture_keys)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryIndex {
    build: HistoricalBuild,
    capture_keys: Vec<String>,
}
fn valid_input(i: &HistoricalInput) -> bool {
    let mut names = std::collections::BTreeSet::new();
    !i.screens.is_empty()
        && i.screens.len() <= 64
        && i.screens.iter().all(|s| {
            names.insert(&s.screen)
                && s.width > 0
                && s.height > 0
                && s.width <= 16384
                && s.height <= 16384
        })
}
fn receipt_matches(i: &HistoricalInput, s: &HistoricalScreen, c: &CaptureReceipt) -> bool {
    if c.screen != s.screen
        || c.route != s.route
        || c.scenario != s.scenario
        || c.commit_oid != i.commit_oid
        || c.tree_hash != i.tree_hash
        || c.failure.is_some()
    {
        return false;
    }
    // A source-proved absent route contains no invented browser capture.
    if !c.route_found {
        return c.screenshot_path.is_none();
    }
    let actual_route = c.actual_route.as_deref().unwrap_or(&c.route);
    let actual_scenario = c.actual_scenario.as_deref().unwrap_or(&c.scenario);
    let accepted = (actual_route == s.route && actual_scenario == s.scenario)
        || (s.legacy_route.as_deref() == Some(actual_route)
            && s.legacy_scenario.as_deref() == Some(actual_scenario));
    accepted
        && c.renderer_sha256 == i.renderer_sha256
        && c.renderer_version.as_deref() == Some(i.renderer_version.as_str())
        && c.capture_clock.as_deref() == Some(i.capture_clock.as_str())
        && c.fixture_sha256.as_deref() == Some(i.fixture_sha256.as_str())
        && c.frame_width == Some(s.width)
        && c.frame_height == Some(s.height)
        && c.content_width.is_some()
        && c.content_height.is_some()
        && c.overflow_x.is_some()
        && c.overflow_y.is_some()
        && c.screenshot_path.is_some()
}
fn capture_key(i: &HistoricalInput, s: &HistoricalScreen, b: &HistoricalBuild) -> String {
    super::visual_cache::key(&(
        super::visual_cache::CaptureKey {
            implementation: i.implementation.clone(),
            ui_sha256: b.ui_sha256.clone(),
            server_sha256: b.server_sha256.clone(),
            server_mode: "exact-historical-backend-v1".into(),
            capture_tool_sha256: i.capture_tool_sha256.clone(),
            fixture_sha256: i.fixture_sha256.clone(),
            renderer_sha256: i.renderer_sha256.clone(),
            renderer_version: i.renderer_version.clone(),
            frame_width: s.width,
            frame_height: s.height,
            capture_clock: i.capture_clock.clone(),
            screen: s.screen.clone(),
            route: s.route.clone(),
            scenario: s.scenario.clone(),
        },
        &i.renderer_framework_sha256,
        &i.tree_hash,
        &i.commit_oid,
        &s.legacy_route,
        &s.legacy_scenario,
    ))
}
fn copy_stable(source: &Path, target: &Path) -> Result<(), String> {
    let before =
        super::visual_inputs::file_stamp(source).ok_or("historical artifact unavailable")?;
    if before.bytes > 256 * 1024 * 1024 {
        return Err("historical artifact exceeds size bound".into());
    }
    std::fs::copy(source, target).map_err(|_| "historical artifact copy failed")?;
    if super::visual_inputs::file_stamp(source).as_ref() != Some(&before) {
        return Err("historical artifact changed while preserving it".into());
    }
    Ok(())
}
fn build_identity(
    cache: &VisualCache,
    b: &HistoricalBuild,
) -> Result<(String, String, Vec<super::visual_cache::VerifiedFile>), String> {
    let server = b
        .files
        .first()
        .ok_or("historical server artifact missing")?;
    let root = server
        .parent()
        .ok_or("historical artifact directory missing")?;
    if !root.starts_with(cache.root())
        || server.file_name().and_then(|n| n.to_str()) != Some("server")
        || b.files.len() < 2
        || b.files.len() > 8193
    {
        return Err("historical artifact binding is invalid".into());
    }
    let server_proof = super::visual_cache::VerifiedFile::read(server)
        .ok_or("historical server digest unavailable")?;
    let server_sha = server_proof.sha256.clone();
    let mut proof = vec![server_proof];
    let mut digests = Vec::new();
    let mut count = 0;
    for p in &b.files[1..] {
        let relative = p
            .strip_prefix(root.join("dist"))
            .map_err(|_| "historical asset leaves bound dist")?;
        let stamp = super::visual_inputs::file_stamp(p).ok_or("historical asset unavailable")?;
        count += stamp.bytes;
        if count > 512 * 1024 * 1024 {
            return Err("historical UI size exceeds bound".into());
        }
        let file =
            super::visual_cache::VerifiedFile::read(p).ok_or("historical UI digest unavailable")?;
        let sha = file.sha256.clone();
        proof.push(file);
        digests.push((relative.to_string_lossy().into_owned(), sha));
    }
    digests.sort();
    if digests.iter().filter(|(p, _)| p == "index.html").count() != 1
        || digests.windows(2).any(|w| w[0].0 == w[1].0)
    {
        return Err("historical UI inventory is invalid".into());
    }
    if !proof
        .iter()
        .all(super::visual_cache::VerifiedFile::remains_verified)
    {
        return Err("historical build changed during verification".into());
    }
    Ok((super::visual_cache::key(&digests), server_sha, proof))
}
fn walk(root: &Path, path: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(path).map_err(|_| "historical dist unavailable")? {
        let entry = entry.map_err(|_| "historical asset unavailable")?;
        let p = entry.path();
        let kind = entry
            .file_type()
            .map_err(|_| "historical asset unavailable")?;
        if kind.is_symlink() {
            return Err("historical asset is a symlink".into());
        }
        if kind.is_dir() {
            walk(root, &p, out)?;
        } else if kind.is_file() && p.starts_with(root) {
            out.push(p);
            if out.len() > 8192 {
                return Err("historical asset count exceeds bound".into());
            }
        }
    }
    Ok(())
}
