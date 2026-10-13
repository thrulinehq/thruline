//! Private C1 capture and comparison cache. A verified digest is recorded once
//! and remains usable only while its inode, size, mtime and ctime stay identical.
use super::visual_inputs::{file_digest, file_stamp, FileStamp};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureKey {
    pub implementation: String,
    pub ui_sha256: String,
    pub server_sha256: String,
    pub server_mode: String,
    pub capture_tool_sha256: String,
    pub fixture_sha256: String,
    pub renderer_sha256: String,
    pub renderer_version: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub capture_clock: String,
    pub screen: String,
    pub route: String,
    pub scenario: String,
}
impl CaptureKey {
    pub fn key(&self) -> String {
        key(self)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultKey {
    pub capture_key: String,
    pub decision_id: String,
    pub revision: i64,
    pub reference_sha256: String,
    pub tolerance_bits: u64,
    pub compare_tool_sha256: String,
}
impl ResultKey {
    pub fn key(&self) -> String {
        key(self)
    }
    pub fn comparison_key(&self) -> String {
        key(&(
            self.capture_key.as_str(),
            self.reference_sha256.as_str(),
            self.tolerance_bits,
            self.compare_tool_sha256.as_str(),
        ))
    }
}
pub fn key(value: &impl Serialize) -> String {
    digest(&serde_json::to_vec(value).unwrap_or_default())
}
pub fn digest(body: &[u8]) -> String {
    Sha256::digest(body)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedFile {
    pub path: PathBuf,
    pub stamp: FileStamp,
    pub sha256: String,
}
impl VerifiedFile {
    pub fn read(path: &Path) -> Option<Self> {
        file_stamp(path)?;
        let path = path.canonicalize().ok()?;
        let stamp = file_stamp(&path)?;
        let sha256 = file_digest(&path)?;
        if file_stamp(&path).as_ref() != Some(&stamp) {
            return None;
        }
        Some(Self {
            path,
            stamp,
            sha256,
        })
    }
    pub fn remains_verified(&self) -> bool {
        valid_key(&self.sha256) && file_stamp(&self.path).as_ref() == Some(&self.stamp)
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema: u32,
    kind: String,
    key: String,
    value: serde_json::Value,
    value_sha256: String,
    files: Vec<VerifiedFile>,
}
#[derive(Clone)]
pub struct VisualCache {
    root: PathBuf,
}
impl VisualCache {
    pub fn from_env() -> Option<Self> {
        Self::at(
            std::env::var_os("THRULINE_VISUAL_CACHE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| std::env::temp_dir().join("thruline-c28-c1-cache")),
        )
    }
    pub fn at(root: PathBuf) -> Option<Self> {
        if !root.is_absolute() {
            return None;
        }
        let mut b = std::fs::DirBuilder::new();
        b.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            b.mode(0o700);
        }
        b.create(&root).ok()?;
        if !private(&root, false) {
            return None;
        }
        Some(Self {
            root: root.canonicalize().ok()?,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn path(&self, kind: &str, key: &str) -> Option<PathBuf> {
        if !valid_kind(kind) || !valid_key(key) {
            return None;
        }
        Some(self.root.join(format!("{kind}-{key}.json")))
    }
    pub fn load<T: serde::de::DeserializeOwned>(&self, kind: &str, key: &str) -> Option<T> {
        if !private(&self.root, false) {
            return None;
        }
        let path = self.path(kind, key)?;
        if !private(&path, true) || std::fs::metadata(&path).ok()?.len() > 4 * 1024 * 1024 {
            return None;
        }
        let body = std::fs::read(path).ok()?;
        let envelope: Envelope = serde_json::from_slice(&body).ok()?;
        if envelope.schema != 1
            || envelope.kind != kind
            || envelope.key != key
            || envelope.files.len() > 8192
            || digest(&serde_json::to_vec(&envelope.value).ok()?) != envelope.value_sha256
            || !envelope.files.iter().all(VerifiedFile::remains_verified)
        {
            return None;
        }
        serde_json::from_value(envelope.value).ok()
    }
    pub fn save<T: Serialize>(
        &self,
        kind: &str,
        key: &str,
        value: &T,
        files: &[PathBuf],
    ) -> Result<(), String> {
        let files: Vec<_> = files
            .iter()
            .map(|p| {
                VerifiedFile::read(p).ok_or_else(|| "cache artifact cannot be verified".to_owned())
            })
            .collect::<Result<_, _>>()?;
        self.save_verified(kind, key, value, files)
    }
    /// Only the caller that has just hashed these exact file bytes may use this.
    pub fn save_verified<T: Serialize>(
        &self,
        kind: &str,
        key: &str,
        value: &T,
        files: Vec<VerifiedFile>,
    ) -> Result<(), String> {
        use std::io::Write;
        if !private(&self.root, false)
            || files.len() > 8192
            || !files.iter().all(VerifiedFile::remains_verified)
        {
            return Err("cache inputs changed".into());
        }
        let path = self.path(kind, key).ok_or("cache key is invalid")?;
        let value = serde_json::to_value(value).map_err(|_| "cache value invalid")?;
        let envelope = Envelope {
            schema: 1,
            kind: kind.into(),
            key: key.into(),
            value_sha256: digest(&serde_json::to_vec(&value).map_err(|_| "cache value invalid")?),
            value,
            files,
        };
        let body = serde_json::to_vec(&envelope).map_err(|_| "cache value invalid")?;
        if body.len() > 4 * 1024 * 1024 {
            return Err("cache receipt too large".into());
        }
        let mut file =
            tempfile::NamedTempFile::new_in(&self.root).map_err(|_| "cache write failed")?;
        file.write_all(&body).map_err(|_| "cache write failed")?;
        file.persist(path).map_err(|_| "cache save failed")?;
        Ok(())
    }
}
fn valid_kind(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 40
        && value.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')
}
fn valid_key(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn private(path: &Path, file: bool) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if meta.file_type().is_symlink()
        || if file {
            !meta.is_file()
        } else {
            !meta.is_dir()
        }
    {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return false;
        }
    }
    true
}
