//! Source-adapter boundary for transcript discovery and identity.
//!
//! Source identity and logical-project association are intentionally separate
//! (A71). Nothing in this module merges providers from cwd/root equality.

use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::chain::to_hex;
use crate::error::{CoreError, Result};
use crate::model::Source;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SourceSelection {
    Auto,
    ClaudeCode,
    Codex,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub source: Source,
    pub format_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPlan {
    pub source: Source,
    pub path: PathBuf,
    pub owner_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogicalRoute {
    pub owner_key: String,
    pub source_project_key: String,
    pub source_session_key: String,
    pub normalized_root: String,
    pub display_name: Option<String>,
    pub started_at: Option<String>,
}

pub trait TranscriptAdapter {
    fn source(&self) -> Source;
    fn probe(&self, path: &Path) -> Result<Probe>;
    fn discover(&self, roots: &[PathBuf]) -> Result<Vec<SessionPlan>>;
    fn inspect(&self, plan: &SessionPlan) -> Result<LogicalRoute>;
}

pub fn stable_source_project_id(
    owner_key: &str,
    source: Source,
    source_project_key: &str,
) -> String {
    format!(
        "project_{}",
        identity_hash(&[
            "source-project-v1",
            owner_key,
            source.as_str(),
            source_project_key,
        ])
    )
}

pub fn stable_source_session_id(
    owner_key: &str,
    source: Source,
    source_session_key: &str,
) -> String {
    format!(
        "session_{}",
        identity_hash(&[
            "source-session-v1",
            owner_key,
            source.as_str(),
            source_session_key,
        ])
    )
}

pub fn stable_owner_key(owner_root: &Path) -> Result<String> {
    let normalized = lexical_absolute(owner_root)?;
    #[cfg(unix)]
    let platform_owner = {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(owner_root).map_err(|e| CoreError::io(owner_root, e))?;
        format!("unix:{}", metadata.uid())
    };
    #[cfg(not(unix))]
    let platform_owner = "platform-owner-unavailable".to_string();
    Ok(format!(
        "owner_{}",
        identity_hash(&["source-owner-v1", &platform_owner, &normalized])
    ))
}

pub fn lexical_absolute(path: &Path) -> Result<String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| CoreError::io(".", e))?
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Path::new("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    return Err(CoreError::Config(
                        "path normalization escaped its absolute root".to_string(),
                    ));
                }
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
        .to_str()
        .map(str::to_owned)
        .ok_or_else(|| CoreError::Config("source path is not valid UTF-8".to_string()))
}

pub fn display_name_from_root(root: &str) -> Option<String> {
    Path::new(root)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
}

fn identity_hash(fields: &[&str]) -> String {
    let mut hash = Sha256::new();
    for field in fields {
        hash.update((field.len() as u64).to_le_bytes());
        hash.update(field.as_bytes());
    }
    to_hex(&hash.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identities_are_owner_and_source_scoped_not_root_merged() {
        let codex = stable_source_project_id("owner-a", Source::Codex, "/same/root");
        let claude = stable_source_project_id("owner-a", Source::ClaudeCode, "/same/root");
        let other_owner = stable_source_project_id("owner-b", Source::Codex, "/same/root");
        assert_ne!(codex, claude);
        assert_ne!(codex, other_owner);
        assert_eq!(
            codex,
            stable_source_project_id("owner-a", Source::Codex, "/same/root")
        );
    }

    #[test]
    fn lexical_normalization_does_not_resolve_symlinks() {
        let normalized = lexical_absolute(Path::new("/tmp/one/../two/./three"))
            .expect("normalize absolute path");
        assert_eq!(normalized, "/tmp/two/three");
    }
}
