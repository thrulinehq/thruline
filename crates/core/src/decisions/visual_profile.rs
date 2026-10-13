//! Optional private timing log. Timings are diagnostics, never verdict inputs.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct VisualProfile {
    path: Option<PathBuf>,
    started: Instant,
    record_total: bool,
}
pub struct VisualSpan<'a> {
    profile: &'a VisualProfile,
    stage: &'static str,
    started: Instant,
}
impl VisualProfile {
    pub fn from_env() -> Self {
        let path = std::env::var_os("THRULINE_VISUAL_PROFILE")
            .map(PathBuf::from)
            .filter(|path| prepare(path));
        Self {
            path,
            started: Instant::now(),
            record_total: true,
        }
    }
    pub fn for_stage() -> Self {
        let mut profile = Self::from_env();
        profile.record_total = false;
        profile
    }
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub fn span(&self, stage: &'static str) -> VisualSpan<'_> {
        VisualSpan {
            profile: self,
            stage,
            started: Instant::now(),
        }
    }
    pub fn record(&self, stage: &str, elapsed_ms: u128) {
        if let Some(path) = &self.path {
            if let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(path) {
                let _ = writeln!(
                    file,
                    "{}",
                    serde_json::json!({"stage": stage, "elapsed_ms": elapsed_ms})
                );
            }
        }
    }
}
impl Drop for VisualProfile {
    fn drop(&mut self) {
        if self.record_total {
            self.record("check_total", self.started.elapsed().as_millis());
        }
    }
}
impl Drop for VisualSpan<'_> {
    fn drop(&mut self) {
        self.profile
            .record(self.stage, self.started.elapsed().as_millis());
    }
}
fn prepare(path: &Path) -> bool {
    if !path.is_absolute() {
        return false;
    }
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        if !meta.is_file() || meta.file_type().is_symlink() {
            return false;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o077 != 0 {
                return false;
            }
        }
        return true;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).is_ok()
}
