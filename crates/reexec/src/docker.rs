//! §7's mandatory local backend: Docker.
//!
//! # What "clean" means here, concretely
//!
//! - The working directory is a **freshly materialised tree**, written blob by
//!   blob from `git cat-file` (never `git checkout` — see [`crate::materialize`]).
//!   Nothing of the developer's working copy is visible: no `.git`, no
//!   untracked files, no environment, no caches.
//! - `--network none` unless the repository explicitly asked otherwise in
//!   `.thruline/reexec.yml`. §7 says network off by default and §0.8 says
//!   nothing leaves the machine unless the user enables it; those agree.
//! - `--env` carries only what the caller passes. The host environment is not
//!   forwarded, so a secret in the developer's shell cannot reach the
//!   container and a `CI=1` in it cannot change what the tests do.
//!
//! # The image digest is the evidence
//!
//! A tag is mutable: `python:3-slim` today and `python:3-slim` tomorrow are two
//! different images. So the record carries `docker image inspect --format
//! {{.Id}}`, the content digest of the image as this machine holds it, and a
//! run that cannot resolve one **fails** rather than proceeding — because
//! `reexecuted: true` is derived from that digest, and a re-execution with
//! nothing to bind it to is not evidence of anything.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::image::ImageSource;
use crate::{ExecResult, PrepareRequest, Prepared, RunRequest, Sandbox, SandboxError};

/// A local Docker sandbox.
#[derive(Debug, Default)]
pub struct DockerSandbox {
    /// Where the tree was materialised. Owned so `destroy` can remove it.
    workdir: Option<PathBuf>,
    prepared: Option<Prepared>,
    /// The binary to drive. `docker` unless `THRULINE_DOCKER` overrides it,
    /// which is how a `podman` user points this at their runtime without a code
    /// change.
    binary: Option<String>,
    /// The name of the container the last `run` started, so a timeout or a
    /// `destroy` can reach the container itself and not merely the client.
    container: Option<String>,
}

impl DockerSandbox {
    pub fn new() -> DockerSandbox {
        DockerSandbox::default()
    }

    fn bin(&self) -> String {
        self.binary
            .clone()
            .or_else(|| std::env::var("THRULINE_DOCKER").ok())
            .unwrap_or_else(|| "docker".to_string())
    }

    /// Is a container runtime usable right now?
    ///
    /// Answered by asking the daemon, not by looking for the binary on `PATH`:
    /// a `docker` CLI with no daemon behind it is the common state on a
    /// developer laptop and it fails several seconds later, deep inside a run.
    pub fn available(&self) -> Result<String, SandboxError> {
        let out = Command::new(self.bin())
            .args(["info", "--format", "{{.ServerVersion}}"])
            .stdin(Stdio::null())
            .output()
            .map_err(|e| SandboxError::NoRuntime {
                detail: format!("{}: {e}", self.bin()),
            })?;
        if !out.status.success() {
            return Err(SandboxError::NoRuntime {
                detail: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// `docker image inspect --format {{.Id}}`.
    fn image_id(&self, reference: &str) -> Option<String> {
        let out = Command::new(self.bin())
            .args(["image", "inspect", "--format", "{{.Id}}", reference])
            .stdin(Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let id = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!id.is_empty()).then_some(id)
    }

    fn repo_digest(&self, reference: &str) -> Option<String> {
        let out = Command::new(self.bin())
            .args([
                "image",
                "inspect",
                "--format",
                "{{if .RepoDigests}}{{index .RepoDigests 0}}{{end}}",
                reference,
            ])
            .stdin(Stdio::null())
            .output()
            .ok()?;
        let d = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!d.is_empty()).then_some(d)
    }

    fn run_docker(&self, args: &[&str]) -> Result<String, SandboxError> {
        let out = Command::new(self.bin())
            .args(args)
            .stdin(Stdio::null())
            .output()
            .map_err(SandboxError::Spawn)?;
        if !out.status.success() {
            return Err(SandboxError::Docker {
                argv: args.join(" "),
                status: out.status.to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// Stop a container by name, best effort and synchronously.
    ///
    /// `rm -f` rather than `kill`: the run carries `--rm`, so a killed
    /// container is reaped by the daemon anyway, and `rm -f` returns only once
    /// the container is gone — which is the property the caller needs before it
    /// deletes the mount.
    fn kill_container(&self, name: &str) {
        let _ = Command::new(self.bin())
            .args(["rm", "-f", name])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    /// Pull an image only when it is not already present.
    ///
    /// A pull is the one network access §7 needs, and it happens at *prepare*
    /// time, never during the run — the run itself is `--network none`.
    fn ensure_image(&self, reference: &str) -> Result<(), SandboxError> {
        if self.image_id(reference).is_some() {
            return Ok(());
        }
        self.run_docker(&["pull", reference])?;
        Ok(())
    }

    /// Build the lockfile-keyed dependency layer, or reuse it.
    ///
    /// §7: *"cache layers keyed by lockfile hash"*. Keyed by the lockfile hash
    /// **and the base image's content digest**, and `FROM` that digest rather
    /// than the mutable reference — the two halves of the same rule. A tag that
    /// named only the lockfile was served on presence alone, so the first base
    /// to build a layer owned it: a repository declaring
    /// `python:3.13.7-slim-bookworm` ran Python 3.14.7 on Debian 13 and said
    /// `image_source: reexec-yml` while doing it.
    ///
    /// `tree` is the **materialised tree**, never the worktree. The lockfiles
    /// that go into the layer are the bytes the tree names; reading them from
    /// the working copy installed dependencies from an uncommitted file and
    /// called the result a re-execution of the tree.
    fn ensure_deps_layer(
        &self,
        base_digest: &str,
        deps: &crate::image::DependencyLayer,
        tree: &Path,
    ) -> Result<String, SandboxError> {
        let tag = deps.tag_for_base(base_digest);
        if self.image_id(&tag).is_some() {
            return Ok(tag);
        }
        // `FROM <image id>` does not resolve: BuildKit reads a bare hex digest
        // as a repository name and goes to the registry for it (measured —
        // `pull access denied ... docker.io/library/sha256:adafcc…`). So the
        // exact image is given a content-addressed alias first, and the build
        // is FROM that. Naming the mutable reference instead would leave a
        // window in which the tag moved between the digest going into the key
        // and the layer being built on it.
        //
        // The alias is left in place: it is a tag on an image the machine
        // already holds, it costs nothing, and removing the last tag of an
        // image is how a tool deletes something a user wanted.
        let alias = base_alias(base_digest);
        if self.image_id(&alias).as_deref() != Some(base_digest) {
            self.run_docker(&["tag", base_digest, &alias])?;
        }
        let context = tempfile::tempdir().map_err(SandboxError::Workspace)?;
        for name in &deps.lockfiles {
            // `tree_read`, never `fs::copy`: copy follows symlinks, so a tree
            // that named its lockfile as a link to a host file copied THAT
            // file's bytes into the build context and `COPY . .` put them in
            // the image. Measured 2026-09-04 by reading `HOST-SECRET-IN-NO-TREE`
            // out of a built layer.
            let Some(bytes) =
                crate::tree_read::read(tree, name).map_err(|e| SandboxError::Image(e.into()))?
            else {
                return Err(SandboxError::Image(crate::image::ImageError::Io {
                    path: tree.join(name),
                    source: std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "the dependency layer names a lockfile the materialised tree does not \
                         hold as a regular file",
                    ),
                }));
            };
            let to = context.path().join(name);
            if let Some(parent) = to.parent() {
                std::fs::create_dir_all(parent).map_err(SandboxError::Workspace)?;
            }
            std::fs::write(&to, &bytes).map_err(SandboxError::Workspace)?;
        }
        let dockerfile = format!(
            "FROM {alias}\nWORKDIR /thruline-deps\nCOPY . .\nRUN {}\nWORKDIR /w\n",
            deps.install
        );
        std::fs::write(context.path().join("Dockerfile"), dockerfile)
            .map_err(SandboxError::Workspace)?;
        let path = context.path().to_string_lossy().to_string();
        // No `-f`: docker resolves a relative `-f` against the CURRENT
        // directory rather than the context, so `-f Dockerfile` names a file in
        // whatever directory the CLI happened to be run from. The default —
        // `<context>/Dockerfile` — is the one that is a function of the context.
        self.run_docker(&["build", "-t", &tag, &path])?;
        Ok(tag)
    }
}

impl Sandbox for DockerSandbox {
    fn prepare(&mut self, req: &PrepareRequest) -> Result<Prepared, SandboxError> {
        self.available()?;

        // 1. The tree, written blob by blob. Not a checkout — see the module
        //    docs of `materialize`.
        //
        //    **It comes first now.** Everything below that reads a file reads it
        //    from here, because the worktree is not the thing being re-executed:
        //    it was measured on 2026-09-04 installing dependencies from an
        //    uncommitted `pinned==9.9.9` while the tree said `pinned==1.0.0`.
        //    The directory is held by the `TempDir` guard until every check has
        //    passed, so a refusal leaves nothing behind.
        let dir = tempfile::Builder::new()
            .prefix("thruline-reexec-")
            .tempdir()
            .map_err(SandboxError::Workspace)?;
        let materialized = crate::materialize::materialize(&req.repo, &req.tree_hash, dir.path())?;

        // 2. The dependency layer, **re-derived from the tree**. The caller
        //    derived one from the worktree; where the two disagree the run is
        //    refused rather than quietly given either, because a receipt cannot
        //    say which of the two it meant.
        let tree_deps = crate::image::dependency_layer(dir.path())?;
        if tree_deps != req.image.deps {
            return Err(SandboxError::TreeDivergence {
                what: "the dependency layer".to_string(),
                worktree: describe_deps(&req.image.deps),
                tree: describe_deps(&tree_deps),
            });
        }

        // 3. The image. A Dockerfile is built **from the materialised tree**,
        //    not from the working copy: a build context is content, and the
        //    tag is keyed to the Dockerfile's bytes, so building the worktree's
        //    would stamp the tree's tag on the worktree's image.
        let mut reference = req.image.reference.clone();
        if req.image.source == ImageSource::Dockerfile {
            let tree_reference = crate::image::dockerfile_reference(dir.path())?;
            if tree_reference.as_deref() != Some(reference.as_str()) {
                return Err(SandboxError::TreeDivergence {
                    what: "the Dockerfile".to_string(),
                    worktree: reference.clone(),
                    tree: tree_reference
                        .unwrap_or_else(|| "no Dockerfile in the tree at all".to_string()),
                });
            }
            let path = dir.path().to_string_lossy().to_string();
            self.run_docker(&["build", "-t", &reference, &path])?;
        } else {
            self.ensure_image(&reference)?;
        }

        // 4. The dependency layer, keyed by the lockfile hash AND the base's
        //    content digest — resolved here, before anything is built on it.
        let base_digest = self
            .image_id(&reference)
            .ok_or_else(|| SandboxError::NoImageDigest {
                image: reference.clone(),
            })?;
        if let Some(deps) = &tree_deps {
            reference = self.ensure_deps_layer(&base_digest, deps, dir.path())?;
        }

        let image_digest =
            self.image_id(&reference)
                .ok_or_else(|| SandboxError::NoImageDigest {
                    image: reference.clone(),
                })?;
        let repo_digest = self.repo_digest(&reference);

        // Kept, not dropped: the directory has to outlive `prepare` because the
        // container mounts it. `destroy` removes it.
        let workdir = dir.keep();

        let prepared = Prepared {
            image: reference,
            image_digest,
            repo_digest,
            image_source: req.image.source,
            workdir: workdir.clone(),
            materialized,
            network: req.network,
        };
        self.workdir = Some(workdir);
        self.prepared = Some(prepared.clone());
        Ok(prepared)
    }

    fn run(&mut self, req: &RunRequest) -> Result<ExecResult, SandboxError> {
        let prepared = self.prepared.clone().ok_or(SandboxError::NotPrepared)?;
        let mount = format!("{}:/w", prepared.workdir.to_string_lossy());
        // A name, so the container can be reached after the client is gone.
        // Without one the timeout below killed the `docker run` process and
        // left the container running against a mount that `destroy` then
        // deleted underneath it — observed end to end on 2026-09-04.
        let name = container_name(&prepared.workdir, &req.argv);
        self.container = Some(name.clone());
        let workdir = if req.cwd.is_empty() || req.cwd == "." {
            "/w".to_string()
        } else {
            format!("/w/{}", req.cwd.trim_start_matches('/'))
        };
        let mut args: Vec<String> = vec![
            "run".into(),
            "--rm".into(),
            // §7 and §0.8. `host` only when the repository asked for it.
            "--network".into(),
            if prepared.network { "host" } else { "none" }.into(),
            "-v".into(),
            mount,
            "-w".into(),
            workdir,
            "--name".into(),
            name.clone(),
        ];
        for (k, v) in &req.env {
            args.push("-e".into());
            args.push(format!("{k}={v}"));
        }
        args.push(prepared.image.clone());
        args.extend(req.argv.iter().cloned());

        let started = Instant::now();
        let mut child = Command::new(self.bin())
            .args(&args)
            // The child gets pipes, exactly as the M0.8 recorder's children do,
            // so the two runs are comparable on the one axis that matters. The
            // `stdio` field below records it rather than leaving it implied.
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(SandboxError::Spawn)?;

        // The timeout is enforced by killing the docker CLI and then the
        // container: killing only the client leaves the container running.
        let deadline = started + req.timeout;
        let mut timed_out = false;
        loop {
            match child.try_wait().map_err(SandboxError::Spawn)? {
                Some(_) => break,
                None => {
                    if Instant::now() >= deadline {
                        timed_out = true;
                        // The container first, then the client. Killing the
                        // client alone leaves the container running: the run
                        // keeps writing to the mounted tree, and `destroy`
                        // removes that mount out from under it.
                        self.kill_container(&name);
                        let _ = child.kill();
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        let out = child.wait_with_output().map_err(SandboxError::Spawn)?;
        Ok(ExecResult {
            exit_code: out.status.code().map(i64::from).unwrap_or(-1),
            stdout: out.stdout,
            stderr: out.stderr,
            duration_ms: started.elapsed().as_millis() as u64,
            stdio: "pipe",
            timed_out,
        })
    }

    fn destroy(&mut self) -> Result<(), SandboxError> {
        // The container before the directory it has mounted. The other order
        // deletes a running container's working tree, which is how a timed-out
        // run went on executing against half a tree.
        if let Some(name) = self.container.take() {
            self.kill_container(&name);
        }
        if let Some(dir) = self.workdir.take() {
            // Best effort: a leftover temp directory is a nuisance, an error
            // here would mask the run's own result.
            let _ = std::fs::remove_dir_all(&dir);
        }
        self.prepared = None;
        Ok(())
    }
}

/// A stable local alias for an image content digest.
///
/// `thruline-reexec:base-<12 hex>` — the same shape the layer tag ends with, so
/// `docker images` shows a layer and the base it sits on side by side.
fn base_alias(base_digest: &str) -> String {
    let hex: String = base_digest
        .rsplit(':')
        .next()
        .unwrap_or(base_digest)
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(12)
        .collect();
    format!("thruline-reexec:base-{hex}")
}

/// A container name that is unique to this workdir and command.
///
/// Derived rather than random for the same reason `reexec_check_id` is: two
/// runs of the same command in the same sandbox should collide loudly rather
/// than leave an unnamed second container behind. `--name` requires
/// `[a-zA-Z0-9][a-zA-Z0-9_.-]*`, so the material is hashed rather than quoted.
fn container_name(workdir: &Path, argv: &[String]) -> String {
    let mut material = workdir.to_string_lossy().as_bytes().to_vec();
    for arg in argv {
        material.push(0);
        material.extend_from_slice(arg.as_bytes());
    }
    format!(
        "thruline-reexec-{}",
        &thruline_core::chain::sha256_hex(&material)[..16]
    )
}

/// One line naming a dependency layer, for a divergence message.
fn describe_deps(deps: &Option<crate::image::DependencyLayer>) -> String {
    match deps {
        None => "no dependency layer".to_string(),
        Some(d) => format!(
            "{} over [{}] with key {} and install {:?}",
            d.ecosystem.as_str(),
            d.lockfiles.join(", "),
            &d.key[..16],
            d.install
        ),
    }
}

/// The environment a re-execution is given.
///
/// Deliberately tiny and explicit: everything not listed here is absent inside
/// the container, so a value in the developer's shell can neither leak in nor
/// change what the tests do.
pub fn minimal_env() -> BTreeMap<String, String> {
    let mut env = BTreeMap::new();
    // Suppress interactive/paging behaviour without pretending to be CI, which
    // §6.8 treats as a signal a test might key on.
    env.insert("PYTHONDONTWRITEBYTECODE".into(), "1".into());
    env.insert("PYTHONUNBUFFERED".into(), "1".into());
    env.insert("TERM".into(), "dumb".into());
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sandbox_that_was_never_prepared_refuses_to_run() {
        let mut sandbox = DockerSandbox::new();
        let err = sandbox
            .run(&RunRequest {
                argv: vec!["true".into()],
                cwd: String::new(),
                env: BTreeMap::new(),
                timeout: std::time::Duration::from_secs(1),
            })
            .expect_err("must refuse");
        assert!(matches!(err, SandboxError::NotPrepared));
    }

    #[test]
    fn the_minimal_environment_does_not_announce_ci() {
        // §6.8 flags newly introduced `CI` sniffing in source. Setting `CI=1`
        // here would make ThruLine's own re-execution take whichever branch the
        // agent wrote for CI — the environment would decide the evidence.
        let env = minimal_env();
        for forbidden in ["CI", "GITHUB_ACTIONS", "PYTEST_CURRENT_TEST", "NODE_ENV"] {
            assert!(
                !env.contains_key(forbidden),
                "{forbidden} is set in the re-execution environment"
            );
        }
    }
}
