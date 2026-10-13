//! BUILD-SPEC §7 image selection and the lockfile-keyed dependency layer.
//!
//! > *image selection order: `.thruline/reexec.yml` → `devcontainer.json` →
//! > `Dockerfile` (build) → ecosystem default images (rust, node, python, go,
//! > jdk). Network off by default; dependency install steps run from lockfiles;
//! > cache layers keyed by lockfile hash.*
//!
//! # The order is data, not control flow
//!
//! [`select`] returns the first source that answered, **and says which one**.
//! A selection whose provenance is not recorded is a selection nobody can
//! check: "the tests ran in a clean container" is a different claim from "the
//! tests ran in the image the repository asked for", and the receipt has to be
//! able to tell them apart.
//!
//! # Why the config parser is hand-written
//!
//! `crates/claims/src/rules.rs` parses `.thruline/claims.yml` as a documented
//! subset rather than pulling a YAML crate in, for the same reason repeated
//! here: the file is three keys, and a full YAML implementation is a large
//! parser in the trusted path of a security tool. Anything outside the
//! documented subset is an **error**, never a silent default — the reason
//! someone writes this file is that the defaults are wrong for them.

use std::path::{Path, PathBuf};

use thruline_core::chain::sha256_hex;

/// Where the image came from. Ordered as §7 orders them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ImageSource {
    /// `.thruline/reexec.yml`, the explicit answer.
    ReexecYml,
    /// `.devcontainer/devcontainer.json` or `devcontainer.json`.
    Devcontainer,
    /// A `Dockerfile` in the repository root — built, not pulled.
    Dockerfile,
    /// Nothing said, so the ecosystem was detected from its lockfiles.
    EcosystemDefault,
}

impl ImageSource {
    pub fn as_str(self) -> &'static str {
        match self {
            ImageSource::ReexecYml => "reexec-yml",
            ImageSource::Devcontainer => "devcontainer",
            ImageSource::Dockerfile => "dockerfile",
            ImageSource::EcosystemDefault => "ecosystem-default",
        }
    }
}

/// What to run the checks in.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ImageSpec {
    /// A pullable/local reference, or the tag a build will produce.
    pub reference: String,
    pub source: ImageSource,
    /// Set when the reference has to be produced by `docker build` first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_context: Option<PathBuf>,
    /// The dependency-install layer, when one was derived. Keyed by lockfile
    /// hash so a repeated run reuses it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deps: Option<DependencyLayer>,
}

/// A generated image layer that installs the repository's declared
/// dependencies, and nothing else.
///
/// § 7: *"dependency install steps run from lockfiles; cache layers keyed by
/// lockfile hash"*. The key is the hash, so two runs over the same lockfile
/// reuse one image and a changed lockfile builds a new one — which is the only
/// caching rule that cannot serve stale dependencies.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct DependencyLayer {
    /// The lockfile(s) the key is over, repo-relative.
    pub lockfiles: Vec<String>,
    /// `sha256` over the lockfile bytes, in path order, then the install line.
    ///
    /// **Not a tag.** A layer is `FROM` a base image, so its identity is
    /// (base, lockfiles, install) and not (lockfiles, install) — see
    /// [`DependencyLayer::tag_for_base`], which is the only thing that names an
    /// image.
    pub key: String,
    /// The install command run at build time, as a shell line.
    pub install: String,
    pub ecosystem: Ecosystem,
}

impl DependencyLayer {
    /// The tag this layer gets **when built on `base_digest`**.
    ///
    /// # The bug this signature exists to make unwriteable
    ///
    /// The tag used to be a field, keyed on (ecosystem, lockfile bytes, install
    /// command) alone. A presence check on that tag then served a layer built
    /// on **whatever base image happened to be current the first time** — and
    /// the caller replaced the requested base with it. Measured on 2026-09-04:
    /// a repository declaring `python:3.13.7-slim-bookworm` was handed a layer
    /// built on `python:3-slim` and ran Python 3.14.7 on Debian 13, while the
    /// record still read `image_source: reexec-yml`. A receipt naming an image
    /// the run did not use is a false provenance claim, which §0.3 forbids more
    /// plainly than it forbids a wrong result.
    ///
    /// The base's **content digest** goes into the key, not its reference: a
    /// tag is mutable, so `python:3-slim` before and after a pull are two
    /// images and only the digest can tell. The digest also appears in the tag
    /// text, so a human reading `docker images` can see which base a layer sits
    /// on without inspecting it.
    pub fn tag_for_base(&self, base_digest: &str) -> String {
        let mut material = self.key.as_bytes().to_vec();
        material.push(0);
        material.extend_from_slice(base_digest.as_bytes());
        let key = sha256_hex(&material);
        let base = base_digest
            .rsplit(':')
            .next()
            .unwrap_or(base_digest)
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .take(12)
            .collect::<String>();
        format!(
            "thruline-reexec:{}-{}-base-{base}",
            self.ecosystem.as_str(),
            &key[..16]
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Ecosystem {
    Python,
    Node,
    Rust,
    Go,
    Jvm,
}

/// Every ecosystem, so a scan over the candidate lockfiles cannot silently miss
/// one when a variant is added.
const ALL_ECOSYSTEMS: [Ecosystem; 5] = [
    Ecosystem::Python,
    Ecosystem::Node,
    Ecosystem::Rust,
    Ecosystem::Go,
    Ecosystem::Jvm,
];

impl Ecosystem {
    /// §7's "ecosystem default images (rust, node, python, go, jdk)".
    pub fn default_image(self) -> &'static str {
        match self {
            Ecosystem::Python => "python:3-slim",
            Ecosystem::Node => "node:22-slim",
            Ecosystem::Rust => "rust:1-slim",
            Ecosystem::Go => "golang:1",
            Ecosystem::Jvm => "eclipse-temurin:21",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Ecosystem::Python => "python",
            Ecosystem::Node => "node",
            Ecosystem::Rust => "rust",
            Ecosystem::Go => "go",
            Ecosystem::Jvm => "jvm",
        }
    }

    /// The lockfiles that identify this ecosystem, most specific first, and the
    /// install command each implies.
    fn lockfiles(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Ecosystem::Python => &[
                (
                    "requirements.txt",
                    "pip install --no-cache-dir -r requirements.txt",
                ),
                (
                    "requirements-dev.txt",
                    "pip install --no-cache-dir -r requirements-dev.txt",
                ),
            ],
            Ecosystem::Node => &[
                ("package-lock.json", "npm ci"),
                (
                    "pnpm-lock.yaml",
                    "corepack enable && pnpm install --frozen-lockfile",
                ),
                ("yarn.lock", "yarn install --frozen-lockfile"),
            ],
            Ecosystem::Rust => &[("Cargo.lock", "cargo fetch --locked")],
            Ecosystem::Go => &[("go.sum", "go mod download")],
            Ecosystem::Jvm => &[],
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("{path}: {message}")]
    Config { path: String, message: String },
    #[error(
        "no image could be selected for {root}: `.thruline/reexec.yml` names none, there is \
         no devcontainer.json and no Dockerfile, and no lockfile identifies an ecosystem. \
         Name one in `.thruline/reexec.yml` — guessing an image would make the re-execution \
         a statement about an environment nobody chose."
    )]
    Undecidable { root: String },
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// A read of the tree that would have left it. See `tree_read`.
    #[error(transparent)]
    TreeRead(#[from] crate::tree_read::TreeReadError),
}

/// `.thruline/reexec.yml`, the documented subset.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReexecConfig {
    pub image: Option<String>,
    pub install: Option<String>,
    /// `none` (the default) or `host`. Anything else is an error.
    pub network: Option<String>,
}

impl ReexecConfig {
    pub const PATH: &'static str = ".thruline/reexec.yml";

    pub fn load(root: &Path) -> Result<Option<ReexecConfig>, ImageError> {
        // `tree_read`, never `fs::read_to_string`: this file names the image
        // AND the install command that is executed at build time, so a symlink
        // pointing out of the tree would let a host file choose both.
        let Some(text) = crate::tree_read::read_to_string(root, Self::PATH)? else {
            return Ok(None);
        };
        Ok(Some(Self::parse(
            &text,
            &root.join(Self::PATH).display().to_string(),
        )?))
    }

    pub fn parse(text: &str, path: &str) -> Result<ReexecConfig, ImageError> {
        let mut config = ReexecConfig::default();
        let err = |message: String| ImageError::Config {
            path: path.to_string(),
            message,
        };
        for (i, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once(':') else {
                return Err(err(format!("line {}: expected `key: value`", i + 1)));
            };
            let value = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            match key.trim() {
                "version" => {
                    if value != "1" {
                        return Err(err(format!("version {value:?} is not 1")));
                    }
                }
                "image" => config.image = Some(value),
                "install" => config.install = Some(value),
                "network" => {
                    if value != "none" && value != "host" {
                        return Err(err(format!(
                            "line {}: network must be `none` or `host`, not {value:?}",
                            i + 1
                        )));
                    }
                    config.network = Some(value);
                }
                other => {
                    return Err(err(format!(
                        "line {}: unknown key {other:?}. The documented keys are \
                         version, image, install and network — an unknown key is an \
                         error rather than a default, because a typo would otherwise \
                         silently run the wrong environment.",
                        i + 1
                    )))
                }
            }
        }
        Ok(config)
    }
}

/// §7's selection order, applied to `root`, reading the config itself.
pub fn select(root: &Path) -> Result<ImageSpec, ImageError> {
    let config = ReexecConfig::load(root)?;
    select_with(root, config.as_ref())
}

/// §7's selection order against a config the caller has **already read**.
///
/// This exists so that a caller which needs the config for its own reasons —
/// `reexec` needs `network` — reads `.thruline/reexec.yml` exactly ONCE and
/// gets one answer about it. It used to be read three times per run (here, in
/// [`dependency_layer`], and again in the command), and the command's own read
/// was written `unwrap_or(None)`: a parse error there became "no config",
/// which became `network: false`.
///
/// That swallow was invisible because *this* function read the same file with
/// `?` a line earlier and refused first. A guard riding a stronger veto is not
/// a guard. Passing the config in removes the second and third reads rather
/// than adding a fourth check, so there is no longer a weaker reader for a
/// stronger one to mask — and it closes a TOCTOU besides: three reads of one
/// file can disagree.
pub fn select_with(root: &Path, config: Option<&ReexecConfig>) -> Result<ImageSpec, ImageError> {
    let deps = dependency_layer_with(root, config.and_then(|c| c.install.clone()))?;

    // 1. `.thruline/reexec.yml` — the explicit answer.
    if let Some(reference) = config.and_then(|c| c.image.clone()) {
        return Ok(ImageSpec {
            reference,
            source: ImageSource::ReexecYml,
            build_context: None,
            deps,
        });
    }

    // 2. `devcontainer.json`.
    if let Some(reference) = devcontainer_image(root)? {
        return Ok(ImageSpec {
            reference,
            source: ImageSource::Devcontainer,
            build_context: None,
            deps,
        });
    }

    // 3. A `Dockerfile` in the root: build it.
    if let Some(reference) = dockerfile_reference(root)? {
        return Ok(ImageSpec {
            reference,
            source: ImageSource::Dockerfile,
            build_context: Some(root.to_path_buf()),
            deps,
        });
    }

    // 4. Ecosystem default, from the lockfiles that are actually present.
    if let Some(ecosystem) = detect_ecosystem(root) {
        return Ok(ImageSpec {
            reference: ecosystem.default_image().to_string(),
            source: ImageSource::EcosystemDefault,
            build_context: None,
            deps,
        });
    }

    Err(ImageError::Undecidable {
        root: root.display().to_string(),
    })
}

/// The `image` a devcontainer declares, if it declares one directly.
///
/// A devcontainer that builds from a Dockerfile is deliberately **not**
/// followed here: `build.dockerfile` carries args, context and features whose
/// semantics belong to the devcontainer CLI, and reimplementing a subset of
/// them would produce an image that is *nearly* the declared one. Falls through
/// to the Dockerfile and ecosystem rules instead, which are honest about what
/// they are.
fn devcontainer_image(root: &Path) -> Result<Option<String>, ImageError> {
    for rel in [".devcontainer/devcontainer.json", "devcontainer.json"] {
        let path = root.join(rel);
        let Some(text) = crate::tree_read::read_to_string(root, rel)? else {
            continue;
        };
        // devcontainer.json is JSONC: comments and trailing commas are legal.
        let stripped = strip_jsonc(&text);
        let value: serde_json::Value =
            serde_json::from_str(&stripped).map_err(|e| ImageError::Config {
                path: path.display().to_string(),
                message: format!("not readable as JSON (comments are stripped first): {e}"),
            })?;
        if let Some(image) = value.get("image").and_then(serde_json::Value::as_str) {
            return Ok(Some(image.to_string()));
        }
    }
    Ok(None)
}

/// Remove `//` and `/* */` comments and trailing commas from JSONC.
///
/// String-aware: a `//` inside a string literal is content, not a comment.
fn strip_jsonc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            '/' if chars.peek() == Some(&'/') => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                let _ = chars.next();
                let mut prev = '\0';
                for n in chars.by_ref() {
                    if prev == '*' && n == '/' {
                        break;
                    }
                    prev = n;
                }
            }
            _ => out.push(c),
        }
    }
    // Trailing commas.
    let mut cleaned = String::with_capacity(out.len());
    let bytes: Vec<char> = out.chars().collect();
    let mut i = 0;
    let mut in_string = false;
    let mut escaped = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            cleaned.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if c == '"' {
            in_string = true;
            cleaned.push(c);
            i += 1;
            continue;
        }
        if c == ',' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_whitespace() {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == '}' || bytes[j] == ']') {
                i += 1;
                continue;
            }
        }
        cleaned.push(c);
        i += 1;
    }
    cleaned
}

/// Which ecosystem's lockfiles are present, if any.
pub fn detect_ecosystem(root: &Path) -> Option<Ecosystem> {
    for eco in [
        Ecosystem::Python,
        Ecosystem::Node,
        Ecosystem::Rust,
        Ecosystem::Go,
    ] {
        if eco
            .lockfiles()
            .iter()
            .any(|(f, _)| crate::tree_read::is_regular_file(root, f))
        {
            return Some(eco);
        }
    }
    // Python without a lockfile is still Python if it has a pyproject or a
    // conftest — the common case for a repository whose only dependency is the
    // test runner.
    let is_file = |rel| crate::tree_read::is_regular_file(root, rel);
    if is_file("pyproject.toml") || is_file("conftest.py") {
        return Some(Ecosystem::Python);
    }
    if is_file("pom.xml") || is_file("build.gradle") {
        return Some(Ecosystem::Jvm);
    }
    None
}

/// The dependency layer for `root`, if any lockfile declares one, reading
/// `.thruline/reexec.yml` for the `install:` override itself.
pub fn dependency_layer(root: &Path) -> Result<Option<DependencyLayer>, ImageError> {
    let override_install = ReexecConfig::load(root)?.and_then(|c| c.install);
    dependency_layer_with(root, override_install)
}

/// The dependency layer against an `install:` override the caller already read.
/// See [`select_with`] for why the read is hoisted out.
pub fn dependency_layer_with(
    root: &Path,
    override_install: Option<String>,
) -> Result<Option<DependencyLayer>, ImageError> {
    // Loud before quiet. `detect_ecosystem` reads a symlinked lockfile as
    // *absent*, which on its own would drop the tree's entry in silence; here
    // the whole candidate set is checked first so the refusal names the file
    // (Rule 12). This is the path that decides what gets built and installed,
    // and it is the one the escape was measured on.
    for eco in ALL_ECOSYSTEMS {
        for (name, _) in eco.lockfiles() {
            crate::tree_read::refuse_symlink(root, name)?;
        }
    }
    let Some(ecosystem) = detect_ecosystem(root) else {
        return Ok(None);
    };
    let mut lockfiles = Vec::new();
    let mut installs = Vec::new();
    let mut bytes_of = Vec::new();
    for (name, install) in ecosystem.lockfiles() {
        if let Some(bytes) = crate::tree_read::read(root, name)? {
            lockfiles.push((*name).to_string());
            installs.push((*install).to_string());
            bytes_of.push(bytes);
        }
    }
    if lockfiles.is_empty() {
        return Ok(None);
    }
    let install = override_install.unwrap_or_else(|| installs.join(" && "));
    // One hashing routine and one hex writer for the whole project
    // (`thruline_core::chain::sha256_hex`). A second one here would be a second
    // answer to one question, which is the shape that produced this milestone's
    // redaction leak.
    let mut material: Vec<u8> = Vec::new();
    for (name, bytes) in lockfiles.iter().zip(&bytes_of) {
        material.extend_from_slice(name.as_bytes());
        material.push(0);
        material.extend_from_slice(bytes);
        material.push(0);
    }
    // The install command is part of the key: a repository that changed only
    // its `install:` line must not reuse a layer built by the old one.
    material.extend_from_slice(install.as_bytes());
    let key = sha256_hex(&material);
    Ok(Some(DependencyLayer {
        lockfiles,
        key,
        install,
        ecosystem,
    }))
}

/// The tag a `Dockerfile` in `dir` will be built under, or `None` if there is
/// none.
///
/// Public because the sandbox has to be able to ask the **materialised tree**
/// the same question the worktree was asked: a build context is content, not
/// configuration, and building the worktree's Dockerfile under a tag keyed to
/// the worktree's bytes is how a re-execution stops being a re-execution of the
/// tree. The tag carries the Dockerfile's hash, so an edited Dockerfile gets a
/// new tag rather than silently reusing the old image.
pub fn dockerfile_reference(dir: &Path) -> Result<Option<String>, ImageError> {
    let Some(bytes) = crate::tree_read::read(dir, "Dockerfile")? else {
        return Ok(None);
    };
    Ok(Some(format!(
        "thruline-reexec:dockerfile-{}",
        &sha256_hex(&bytes)[..16]
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_key_is_an_error_not_a_default() {
        let e = ReexecConfig::parse("version: 1\nimages: python:3\n", "x").unwrap_err();
        assert!(format!("{e}").contains("unknown key"), "{e}");
    }

    #[test]
    fn network_accepts_only_none_or_host() {
        assert!(ReexecConfig::parse("network: none\n", "x").is_ok());
        assert!(ReexecConfig::parse("network: host\n", "x").is_ok());
        assert!(ReexecConfig::parse("network: bridge\n", "x").is_err());
    }

    #[test]
    fn jsonc_comments_and_trailing_commas_are_stripped_but_strings_are_not() {
        let text = r#"{
  // a comment
  "image": "python:3", /* another */
  "name": "https://example.invalid/x", // not a comment
}"#;
        let value: serde_json::Value = serde_json::from_str(&strip_jsonc(text)).expect("parses");
        assert_eq!(value["image"], "python:3");
        assert_eq!(value["name"], "https://example.invalid/x");
    }
}
