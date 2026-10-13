//! Approved renderer identity, cached only with verified executable metadata.
use super::visual_cache::{key, VerifiedFile, VisualCache};
use super::visual_inputs::file_stamp;
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RendererIdentity {
    pub sha256: String,
    pub product: String,
}
pub fn verify_renderer_with(
    renderer: &Path,
    pin: &str,
    approved_sha: &str,
    approved_product: &str,
    cache: &VisualCache,
) -> Result<RendererIdentity, &'static str> {
    verify_renderer_with_framework(
        renderer,
        pin,
        approved_sha,
        approved_product,
        Some(super::APPROVED_RENDERER_FRAMEWORK_SHA256),
        cache,
    )
}

pub fn verify_renderer_with_framework(
    renderer: &Path,
    pin: &str,
    approved_sha: &str,
    approved_product: &str,
    approved_framework: Option<&str>,
    cache: &VisualCache,
) -> Result<RendererIdentity, &'static str> {
    if !pin.eq_ignore_ascii_case(approved_sha) {
        return Err("renderer pin does not match the approved renderer");
    }
    let launcher = renderer.canonicalize().map_err(|_| "renderer not found")?;
    let mut paths = vec![launcher.clone()];
    if let Some(contents) = launcher.parent().and_then(Path::parent) {
        let framework = contents.join(
            "Frameworks/Google Chrome Framework.framework/Versions/Current/Google Chrome Framework",
        );
        if framework.exists() {
            paths.push(
                framework
                    .canonicalize()
                    .map_err(|_| "renderer framework identity unavailable")?,
            );
        }
    }
    if approved_framework.is_some() && paths.len() != 2 {
        return Err("approved renderer framework is missing");
    }
    let stamps: Vec<_> = paths
        .iter()
        .map(|p| file_stamp(p).ok_or("renderer identity unavailable"))
        .collect::<Result<_, _>>()?;
    let identity = key(&(
        paths.clone(),
        stamps,
        approved_sha,
        approved_product,
        approved_framework,
    ));
    if let Some(value) = cache.load::<RendererIdentity>("renderer", &identity) {
        return Ok(value);
    }
    let files: Vec<_> = paths
        .iter()
        .map(|p| VerifiedFile::read(p).ok_or("renderer identity unavailable"))
        .collect::<Result<_, _>>()?;
    if !files[0].sha256.eq_ignore_ascii_case(approved_sha) {
        return Err("renderer pin does not match");
    }
    if approved_framework.is_some_and(|pin| !files[1].sha256.eq_ignore_ascii_case(pin)) {
        return Err("renderer framework does not match the approved installation");
    }
    let output = match crate::bounded_child::output_until(
        std::process::Command::new(&launcher).arg("--version"),
        std::time::Instant::now() + std::time::Duration::from_secs(10),
    )
    .map_err(|_| "renderer version unavailable")?
    {
        crate::bounded_child::Outcome::Finished(out) => out,
        crate::bounded_child::Outcome::TimedOut => {
            return Err("renderer version verification exceeded its deadline")
        }
    };
    if !output.status.success() || output.stdout.len() > 1024 {
        return Err("renderer version unavailable");
    }
    let expected = approved_product
        .strip_prefix("Chrome/")
        .map(|v| format!("Google Chrome {v}"))
        .ok_or("renderer product pin is invalid")?;
    if String::from_utf8_lossy(&output.stdout).trim() != expected {
        return Err("renderer version does not match the approved renderer");
    }
    if !files.iter().all(VerifiedFile::remains_verified) {
        return Err("renderer changed during identity verification");
    }
    let value = RendererIdentity {
        sha256: files[0].sha256.clone(),
        product: approved_product.into(),
    };
    let _ = cache.save_verified("renderer", &identity, &value, files);
    Ok(value)
}
