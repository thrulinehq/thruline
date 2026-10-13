//! Native-worker filesystem contract. Read scope and managed write scope are
//! explicit; generic host home and arbitrary target writes are never inherited.
use anyhow::{Context, Result};
use std::path::{Component, Path, PathBuf};

fn quoted(path: &Path) -> Result<String> {
    let text = path.to_str().context("sandbox path is not UTF-8")?;
    anyhow::ensure!(!text.contains(['\0', '\n', '\r']), "invalid sandbox path");
    Ok(serde_json::to_string(text)?)
}

/// Resolve an existing ancestor before creating only the requested managed
/// output. This rejects `..` and symlink aliases into the protected project.
pub fn resolved(path: &Path) -> Result<PathBuf> {
    anyhow::ensure!(path.is_absolute(), "worker access paths must be absolute");
    anyhow::ensure!(
        path.components()
            .all(|c| !matches!(c, Component::ParentDir | Component::CurDir)),
        "worker access paths must be normalized"
    );
    let mut ancestor = path;
    let mut suffix = Vec::new();
    while !ancestor.exists() {
        suffix.push(ancestor.file_name().context("invalid worker output path")?);
        ancestor = ancestor
            .parent()
            .context("worker output has no existing ancestor")?;
    }
    let mut result = ancestor.canonicalize()?;
    for part in suffix.into_iter().rev() {
        result.push(part);
    }
    Ok(result)
}

pub fn writable(path: &Path, protected: &[PathBuf]) -> Result<PathBuf> {
    let path = resolved(path)?;
    anyhow::ensure!(
        path.components().count() >= 4,
        "worker write scope is too broad"
    );
    for source in protected {
        let source = resolved(source)?;
        anyhow::ensure!(
            !path.starts_with(&source) && !source.starts_with(&path),
            "worker output overlaps protected source or runtime: {}",
            path.display()
        );
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        anyhow::ensure!(path != home, "worker cannot write the host home");
        for sensitive in [
            ".ssh",
            ".codex",
            ".aws",
            ".config/gcloud",
            ".azure",
            "Library/Keychains",
        ] {
            anyhow::ensure!(
                !path.starts_with(home.join(sensitive)),
                "worker output overlaps a sensitive host store"
            );
        }
    }
    std::fs::create_dir_all(&path)?;
    Ok(path.canonicalize()?)
}

pub fn profile(
    read_paths: &[PathBuf],
    write_paths: &[PathBuf],
    port: Option<u16>,
) -> Result<String> {
    // Tahoe's dyld ignition opens the root directory itself. The literal
    // grant is necessary for process startup and does not grant descendants.
    // Measured with real /bin/echo and interpreter startup probes.
    let mut profile = "(version 1)(allow default)(deny network*)(deny file-read*)(deny file-write*)(allow file-read-metadata)(allow file-read* (literal \"/\"))".to_string();
    match port {
        None => {}
        Some(11435) => profile.push_str("(allow network-outbound (remote ip \"127.0.0.1:11435\"))"),
        Some(11436) => profile.push_str("(allow network-outbound (remote ip \"127.0.0.1:11436\"))"),
        Some(_) => anyhow::bail!("unregistered local model port"),
    }
    // Operating-system executables, dynamic loaders, locale/timezone data and
    // installed tool libraries. No blanket /Users, /private/tmp, /Library or
    // host-home read grant; target scripts cannot read neighboring projects.
    for path in [
        "/System",
        "/usr/bin",
        "/usr/lib",
        "/usr/share",
        "/bin",
        "/sbin",
        "/dev",
        "/Library/Developer/CommandLineTools",
        "/Applications/Xcode.app",
        "/Library/Frameworks/Python.framework",
        "/opt/homebrew/Cellar",
        "/opt/homebrew/opt",
        "/opt/homebrew/lib",
        "/opt/homebrew/share",
    ] {
        let path = Path::new(path);
        if path.exists() {
            profile.push_str(&format!("(allow file-read* (subpath {}))", quoted(path)?));
        }
    }
    for path in [
        "/private/etc/localtime",
        "/private/etc/passwd",
        "/private/etc/group",
        "/private/etc/protocols",
        "/private/etc/services",
        // Cargo initializes LibreSSL even for offline metadata/builds.
        // This system config contains format defaults, not private keys.
        "/private/etc/ssl/openssl.cnf",
    ] {
        let path = Path::new(path);
        if path.exists() {
            profile.push_str(&format!("(allow file-read* (literal {}))", quoted(path)?));
            profile.push_str(&format!(
                "(allow file-read* (literal {}))",
                quoted(&path.canonicalize()?)?
            ));
        }
    }
    for path in read_paths.iter().chain(write_paths) {
        let path = path
            .canonicalize()
            .with_context(|| format!("worker read path missing: {}", path.display()))?;
        anyhow::ensure!(
            path.components().count() > 1,
            "worker cannot read the entire filesystem"
        );
        let kind = if path.is_dir() { "subpath" } else { "literal" };
        profile.push_str(&format!("(allow file-read* ({kind} {}))", quoted(&path)?));
        // Runtime discovery (for example Joern's InstallConfig) enumerates
        // containing directories. Literal grants permit those directory
        // entries, never neighboring file contents or descendant trees.
        for ancestor in path.ancestors().skip(1) {
            profile.push_str(&format!(
                "(allow file-read* (literal {}))",
                quoted(ancestor)?
            ));
        }
    }
    for path in write_paths {
        let path = path.canonicalize()?;
        anyhow::ensure!(
            path.is_dir() && path.components().count() >= 4,
            "invalid worker write scope"
        );
        profile.push_str(&format!("(allow file-write* (subpath {}))", quoted(&path)?));
    }
    profile.push_str("(allow file-write* (literal \"/dev/null\") (literal \"/dev/tty\"))");
    // Keep these denied even when a caller selects an overly broad project.
    // Metadata remains visible; credential contents and modifications do not.
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        for sensitive in [
            ".ssh",
            ".codex",
            ".aws",
            ".azure",
            ".config/gcloud",
            "Library/Keychains",
            ".cargo/credentials",
            ".cargo/credentials.toml",
        ] {
            let path = home.join(sensitive);
            profile.push_str(&format!(
                "(deny file-read-data file-write* (subpath {}))",
                quoted(&path)?
            ));
        }
    }
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_runtime_and_symlink_output_overlap_are_rejected() {
        let temp = tempfile::tempdir().expect("test workspace");
        let source = temp.path().join("project");
        std::fs::create_dir(&source).expect("project");
        let source = source.canonicalize().expect("canonical");
        assert!(writable(&source.join("output"), std::slice::from_ref(&source)).is_err());
        assert!(writable(
            source.parent().expect("parent"),
            std::slice::from_ref(&source)
        )
        .is_err());
        #[cfg(unix)]
        {
            let alias = temp.path().join("alias");
            std::os::unix::fs::symlink(&source, &alias).expect("test symlink");
            assert!(writable(&alias.join("output"), std::slice::from_ref(&source)).is_err());
        }
        assert!(resolved(&source.join("../escape")).is_err());
    }
}
