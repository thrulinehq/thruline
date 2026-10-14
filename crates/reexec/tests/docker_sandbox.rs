//! What the Docker backend does when a container runtime is actually there.
//!
//! Every test here is `#[ignore]`d: it needs a daemon, and a skip that reported
//! itself as a pass would be exactly the failure mode this file exists to
//! catch. Run with `cargo test -p thruline-reexec --test docker_sandbox --
//! --ignored`.
//!
//! # The three measured defects these reproduce
//!
//! 1. **The layer-cache tag excluded the base image.** Keyed on (ecosystem,
//!    lockfile bytes, install command) alone, so a cached tag was served on
//!    presence and the requested base was replaced by it. Measured 2026-09-04:
//!    a repository declaring `python:3.13.7-slim-bookworm` ran Python 3.14.7 on
//!    Debian 13, and the record still read `image_source: reexec-yml`.
//! 2. **Lockfiles were read from the live worktree**, so a layer could be built
//!    from an uncommitted file while the run claimed to be a re-execution of the
//!    tree.
//! 3. **The timeout killed only the docker client**, leaving the container
//!    running against a mount that `destroy` then deleted underneath it.
//!
//! Each is asserted in the failure direction: the images must *differ*, the
//! layer must hold the *tree's* bytes, and the container must be *gone*.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use thruline_reexec::docker::{minimal_env, DockerSandbox};
use thruline_reexec::image::{dependency_layer, select};
use thruline_reexec::{PrepareRequest, RunRequest, Sandbox, SandboxError};

/// Two images that are both present on a machine that has run this project, and
/// that differ in a way a human can see instantly.
const DECLARED: &str = "python:3.13.7-slim-bookworm";
// Pinned to a minor line, not `python:3-slim`: the floating tag moved to 3.15
// when it was released, and the test asserts this image reports 3.14.
const OTHER: &str = "python:3.14-slim";

fn docker_available() -> bool {
    Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn image_present(reference: &str) -> bool {
    Command::new("docker")
        .args(["image", "inspect", "--format", "{{.Id}}", reference])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .unwrap_or_else(|e| panic!("git {args:?}: {e}"));
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, text).expect("write");
}

/// A repository whose re-execution image is `image` and whose dependency layer
/// installs nothing (`/bin/true`) — the cache machinery is what is under test,
/// not pip. Returns (repo dir, final tree hash).
fn repo_declaring(image: &str, lockfile: &str) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    write(root, "requirements.txt", lockfile);
    write(
        root,
        ".thruline/reexec.yml",
        &format!("version: 1\nimage: {image}\ninstall: /bin/true\n"),
    );
    write(root, "test_x.py", "def test_x():\n    assert True\n");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "one"]);
    let tree = git(root, &["rev-parse", "HEAD^{tree}"]);
    (dir, tree)
}

/// `docker rmi` for tags this test built, and nothing else. Only ever called
/// with a `thruline-reexec:` tag that a `prepare` in this file produced.
fn remove_built_image(reference: &str) {
    assert!(
        reference.starts_with("thruline-reexec:"),
        "this helper removes only images this test built, not {reference}"
    );
    let _ = Command::new("docker")
        .args(["rmi", "-f", reference])
        .output();
}

fn run_in(sandbox: &mut DockerSandbox, argv: &[&str]) -> (i64, String) {
    let result = sandbox
        .run(&RunRequest {
            argv: argv.iter().map(|s| (*s).to_string()).collect(),
            cwd: String::new(),
            env: minimal_env(),
            timeout: Duration::from_secs(120),
        })
        .expect("run");
    (
        result.exit_code,
        String::from_utf8_lossy(&result.stdout).trim().to_string(),
    )
}

// ---------------------------------------------------------------------------
// 1. The layer cache and the base image.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires a running Docker daemon. Run with --ignored."]
fn a_cached_dependency_layer_is_never_served_across_a_different_base_image() {
    assert!(docker_available(), "no Docker daemon");
    for image in [DECLARED, OTHER] {
        assert!(
            image_present(image),
            "{image} is not present locally, and pulling it here would make this test a \
             network test. `docker pull {image}` first."
        );
    }

    // The same lockfile and the same install command, under two different base
    // images. Under the old key these two repositories share one tag.
    let lockfile = "# thruline base-key test\npinned==1.0.0\n";
    let (repo_a, tree_a) = repo_declaring(DECLARED, lockfile);
    let (repo_b, tree_b) = repo_declaring(OTHER, lockfile);

    let deps_a = dependency_layer(repo_a.path())
        .expect("layer")
        .expect("a layer");
    let deps_b = dependency_layer(repo_b.path())
        .expect("layer")
        .expect("a layer");
    assert_eq!(
        deps_a.key, deps_b.key,
        "the two repositories must share the lockfile key — that is the collision the base \
         digest has to break. If they differ, this test proves nothing."
    );

    let mut built: Vec<String> = Vec::new();
    let run = |repo: &Path, tree: &str, declared: &str| -> (String, String, String) {
        let image = select(repo).expect("select");
        assert_eq!(image.reference, declared, "the yml was not read");
        let mut sandbox = DockerSandbox::new();
        let prepared = sandbox
            .prepare(&PrepareRequest {
                repo: repo.to_path_buf(),
                tree_hash: tree.to_string(),
                image,
                network: false,
            })
            .expect("prepare");
        let (code, version) = run_in(&mut sandbox, &["python", "-VV"]);
        assert_eq!(code, 0, "python did not run: {version}");
        sandbox.destroy().expect("destroy");
        (prepared.image, prepared.image_digest, version)
    };

    let (tag_a, digest_a, version_a) = run(repo_a.path(), &tree_a, DECLARED);
    built.push(tag_a.clone());
    let (tag_b, digest_b, version_b) = run(repo_b.path(), &tree_b, OTHER);
    built.push(tag_b.clone());
    for tag in &built {
        remove_built_image(tag);
    }

    // The control: the two BASES really are distinguishable, so a wrong answer
    // is visible. If they were the same Python, this test could not fail.
    assert_ne!(
        version_a, version_b,
        "the two base images report the same Python, so serving one for the other would be \
         undetectable and this test is vacuous: {version_a} / {version_b}"
    );
    assert!(
        version_a.contains("3.13.7"),
        "the repository declared {DECLARED} and the container reported {version_a:?}. The \
         cached dependency layer was served on presence alone and REPLACED the requested \
         base — while the record still says image_source: reexec-yml."
    );
    assert!(version_b.contains("3.14"), "{version_b}");
    assert_ne!(
        tag_a, tag_b,
        "one tag names the layer on two different base images"
    );
    assert_ne!(tag_a, digest_a, "the tag is not the digest");
    assert_ne!(digest_a, digest_b, "the two runs used one image");
    eprintln!("base A {tag_a} -> {version_a}\nbase B {tag_b} -> {version_b}");
}

// ---------------------------------------------------------------------------
// 2. The tree, not the working copy.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "requires a running Docker daemon. Run with --ignored."]
fn a_dependency_layer_comes_from_the_tree_and_a_divergent_worktree_is_refused() {
    assert!(docker_available(), "no Docker daemon");
    assert!(image_present(DECLARED), "{DECLARED} is not present locally");

    let (repo, tree) = repo_declaring(DECLARED, "pinned==1.0.0\n");
    // The worktree now says something the tree does not. This is the state the
    // defect was measured in: `thruline reexec` derives its ImageSpec from the
    // working copy and hands it to a sandbox that materialises the tree.
    write(repo.path(), "requirements.txt", "pinned==9.9.9\n");
    let from_worktree = select(repo.path()).expect("select");

    let mut sandbox = DockerSandbox::new();
    let err = sandbox
        .prepare(&PrepareRequest {
            repo: repo.path().to_path_buf(),
            tree_hash: tree.clone(),
            image: from_worktree.clone(),
            network: false,
        })
        .expect_err(
            "the layer was built from an uncommitted lockfile and the run was called a \
             re-execution of the tree",
        );
    let message = format!("{err}");
    assert!(
        matches!(err, SandboxError::TreeDivergence { .. }),
        "refused, but not as a divergence: {message}"
    );
    assert!(
        message.contains("dependency layer"),
        "the refusal does not say what diverged: {message}"
    );
    eprintln!("REFUSED: {message}");

    // And when the caller does pass the tree's own layer, the bytes that reach
    // the image are the tree's. `install: /bin/true` leaves the lockfile in
    // /thruline-deps, so the container can be asked directly.
    let materialised = tempfile::tempdir().expect("tempdir");
    thruline_reexec::materialize(repo.path(), &tree, materialised.path()).expect("materialise");
    let from_tree = thruline_reexec::ImageSpec {
        deps: dependency_layer(materialised.path()).expect("layer"),
        ..from_worktree
    };
    let prepared = sandbox
        .prepare(&PrepareRequest {
            repo: repo.path().to_path_buf(),
            tree_hash: tree.clone(),
            image: from_tree,
            network: false,
        })
        .expect("prepare with the tree's own layer");
    let (code, lock) = run_in(&mut sandbox, &["cat", "/thruline-deps/requirements.txt"]);
    let workdir = prepared.workdir.clone();
    let tag = prepared.image.clone();
    sandbox.destroy().expect("destroy");
    remove_built_image(&tag);

    assert_eq!(code, 0, "the lockfile is not in the layer at all: {lock}");
    assert!(
        lock.contains("pinned==1.0.0"),
        "the dependency layer was installed from the WORKING COPY: the tree names \
         pinned==1.0.0 and the layer holds {lock:?}"
    );
    assert!(!lock.contains("9.9.9"), "{lock}");
    // The materialised tree agrees with the layer, so both halves came from the
    // same place.
    let on_disk = std::fs::read_to_string(materialised.path().join("requirements.txt"))
        .expect("read materialised lockfile");
    assert_eq!(on_disk, "pinned==1.0.0\n");
    assert!(
        !workdir.exists(),
        "destroy left the materialised tree behind"
    );
}

// ---------------------------------------------------------------------------
// 3. The timeout, and what it leaves running.
// ---------------------------------------------------------------------------

fn live_reexec_containers() -> Vec<String> {
    let out = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            "name=thruline-reexec-",
            "--format",
            "{{.Names}} {{.Status}}",
        ])
        .output()
        .expect("docker ps");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .filter(|l| !l.trim().is_empty())
        .collect()
}

#[test]
#[ignore = "requires a running Docker daemon. Run with --ignored."]
fn a_timed_out_run_kills_the_container_and_not_only_the_client() {
    assert!(docker_available(), "no Docker daemon");
    assert!(image_present(DECLARED), "{DECLARED} is not present locally");
    assert!(
        live_reexec_containers().is_empty(),
        "a re-execution container is already running, so this test cannot attribute one to \
         itself: {:?}",
        live_reexec_containers()
    );

    let (repo, tree) = repo_declaring(DECLARED, "pinned==1.0.0\n");
    let mut sandbox = DockerSandbox::new();
    let prepared = sandbox
        .prepare(&PrepareRequest {
            repo: repo.path().to_path_buf(),
            tree_hash: tree.clone(),
            image: select(repo.path()).expect("select"),
            network: false,
        })
        .expect("prepare");
    let workdir: PathBuf = prepared.workdir.clone();
    let tag = prepared.image.clone();

    let result = sandbox
        .run(&RunRequest {
            // Long enough that only a kill can end it.
            argv: vec!["sleep".into(), "300".into()],
            cwd: String::new(),
            env: minimal_env(),
            timeout: Duration::from_secs(3),
        })
        .expect("run");
    assert!(result.timed_out, "the run did not time out at all");

    // The container, not the client. Killing `docker run` alone leaves the
    // container `Up` — and then `destroy` removes the mount underneath it.
    let left = live_reexec_containers();
    assert!(
        left.is_empty(),
        "the timeout killed the docker client and left the container running: {left:?}. \
         `destroy` is about to delete the tree it has mounted."
    );

    sandbox.destroy().expect("destroy");
    remove_built_image(&tag);
    assert!(
        !workdir.exists(),
        "destroy left the materialised tree behind"
    );
    assert!(live_reexec_containers().is_empty());
}

// ---------------------------------------------------------------------------
// 4. The read side of the tree: what a symlink the tree names can drag in.
// ---------------------------------------------------------------------------

/// The end-to-end form of `tests/tree_reads.rs`: not "the two answers differ"
/// but **the host's bytes, read out of the built image.**
///
/// `TreeDivergence` is blind to this by construction — it re-derives the layer
/// from the tree and compares it with the worktree's, and both sides follow the
/// same symlink to the same host file and agree. The refusal has to come from
/// the read, not from the comparison.
#[test]
#[cfg(unix)]
#[ignore = "requires a running Docker daemon. Run with --ignored."]
fn a_host_file_a_symlink_names_never_reaches_the_built_layer() {
    assert!(docker_available(), "no Docker daemon");
    assert!(image_present(DECLARED), "{DECLARED} is not present locally");

    // A file outside the repository entirely, holding bytes no tree names.
    let host = tempfile::tempdir().expect("host");
    let outside = host.path().join("outside.txt");
    std::fs::write(&outside, "HOST-SECRET-IN-NO-TREE\n").expect("write");

    let repo = tempfile::tempdir().expect("repo");
    let root = repo.path();
    git(root, &["init", "-q", "-b", "main"]);
    std::os::unix::fs::symlink(&outside, root.join("requirements.txt")).expect("symlink");
    write(
        root,
        ".thruline/reexec.yml",
        &format!("version: 1\nimage: {DECLARED}\ninstall: /bin/true\n"),
    );
    write(root, "test_x.py", "def test_x():\n    assert True\n");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "one"]);
    let tree = git(root, &["rev-parse", "HEAD^{tree}"]);

    // Every outcome is asserted. An earlier draft of this test returned early
    // when selection refused — a skip that reports itself as a pass, which is
    // the exact defect this program keeps finding. There is no early return
    // here: a refusal must name the symlink, and a build must not carry the
    // bytes.
    enum Outcome {
        Refused(String),
        Built(String),
    }

    let mut sandbox = DockerSandbox::new();
    // The caller's spec, derived from the worktree exactly as `thruline reexec`
    // derives it.
    let outcome = match select(root) {
        Err(e) => Outcome::Refused(format!("{e}")),
        Ok(from_worktree) => match sandbox.prepare(&PrepareRequest {
            repo: root.to_path_buf(),
            tree_hash: tree.clone(),
            image: from_worktree,
            network: false,
        }) {
            Err(e) => Outcome::Refused(format!("{e}")),
            Ok(prepared) => {
                // `install: /bin/true` leaves the lockfile in /thruline-deps,
                // so the image can be asked what it holds.
                let (_, lock) = run_in(
                    &mut sandbox,
                    &["sh", "-c", "cat /thruline-deps/requirements.txt 2>&1"],
                );
                let tag = prepared.image.clone();
                sandbox.destroy().expect("destroy");
                remove_built_image(&tag);
                Outcome::Built(lock)
            }
        },
    };

    match outcome {
        Outcome::Refused(message) => {
            assert!(
                message.contains("symlink"),
                "refused, but not for the symlink: {message}"
            );
            eprintln!("REFUSED: {message}");
        }
        Outcome::Built(lock) => assert!(
            !lock.contains("HOST-SECRET-IN-NO-TREE"),
            "the built layer holds bytes from a file no tree names: {lock:?}"
        ),
    }
}

// ---------------------------------------------------------------------------
// The build-context symlink route, pinned — and whose mechanism it is.
// ---------------------------------------------------------------------------

/// A `Dockerfile` in the tree, plus a tracked symlink pointing OUT of it, must
/// not put the host file's bytes into the built image.
///
/// # This property is the BUILDER's, not ThruLine's, and that is the point
///
/// Everywhere else this crate closes a read-side symlink escape it does so
/// itself: `tree_read` refuses the link, `materialize` writes mode `120000` as
/// a link rather than following it, and `ensure_deps_layer` copies lockfile
/// *bytes from the tree* instead of `fs::copy`ing a path. The
/// `ImageSource::Dockerfile` route is different: `prepare` hands the whole
/// materialised tree to `docker build` as a build context, and what happens to
/// a symlink in it is decided by the builder.
///
/// Measured 2026-09-04, Docker server 29.6.2, on three shapes:
///
/// | shape | BuildKit (default) | legacy (`DOCKER_BUILDKIT=0`) |
/// |---|---|---|
/// | `COPY payload.txt /`, link → absolute host path | `"/payload.txt": not found` | `stat payload.txt: file does not exist` |
/// | `COPY payload.txt /`, link → `../secret.txt` | same | same |
/// | `COPY . /ctx/` | link copied as a DANGLING link; `cat` → `No such file` | — |
///
/// So both builders refuse, and the whole-context copy preserves the link
/// rather than resolving it. **Neither refusal is ours.** Nothing in this
/// repository would go red if a future builder resolved context symlinks
/// against the host — which is why this test exists and why it is written
/// against the property (*the bytes are not in the image*) rather than against
/// the error message.
///
/// It asserts on BOTH builders deliberately. `DOCKER_BUILDKIT=0` is one
/// environment variable away in any CI runner or developer shell, and a test
/// that pinned only the default would say nothing about the machine that sets
/// it. If either half ever goes green-with-the-secret, the enforcement has to
/// move to our side of the line — `refuse_symlink` over the context before the
/// build, in `DockerSandbox::prepare`.
#[test]
#[ignore = "requires a running Docker daemon. Run with --ignored."]
fn a_symlink_in_the_dockerfile_build_context_cannot_carry_a_host_file_into_the_image() {
    assert!(docker_available(), "no Docker daemon");
    assert!(
        image_present(OTHER),
        "{OTHER} is not present. Pulling it here would make this a network test; the \
         acceptance harness pulls it."
    );

    // The host file that must never reach an image. It lives outside the
    // repository, which is the whole shape of the escape.
    let outside = tempfile::tempdir().expect("tempdir");
    let secret_path = outside.path().join("host-secret.txt");
    const SECRET: &str = "HOST-SECRET-IN-NO-TREE-AND-IN-NO-IMAGE";
    std::fs::write(&secret_path, format!("{SECRET}\n")).expect("write secret");

    // A repository whose tree holds a Dockerfile and a symlink to that file.
    // Committed, so the link is a real mode-`120000` entry and `materialize`
    // reproduces it in the context exactly as the tree names it.
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    write(root, "requirements.txt", "pytest==9.0.1\n");
    write(
        root,
        "Dockerfile",
        &format!("FROM {OTHER}\nCOPY payload.txt /payload.txt\n"),
    );
    std::os::unix::fs::symlink(&secret_path, root.join("payload.txt")).expect("symlink");
    git(root, &["add", "-A"]);
    git(root, &["commit", "-qm", "one"]);
    let tree = git(root, &["rev-parse", "HEAD^{tree}"]);
    assert!(
        git(root, &["ls-tree", "HEAD", "payload.txt"]).starts_with("120000"),
        "the fixture did not commit a symlink, so it is not about this at all"
    );

    // 1. The product's own path. `prepare` builds the Dockerfile from the
    //    materialised tree; the build must fail rather than produce an image.
    let spec = select(root).expect("image selection");
    assert_eq!(
        spec.source.as_str(),
        "dockerfile",
        "the fixture did not take the Dockerfile route: {spec:?}"
    );
    let mut sandbox = DockerSandbox::new();
    let prepared = sandbox.prepare(&PrepareRequest {
        repo: root.to_path_buf(),
        tree_hash: tree.clone(),
        image: spec.clone(),
        network: false,
    });
    let _ = sandbox.destroy();
    match prepared {
        Err(e) => {
            let text = e.to_string();
            assert!(
                !text.contains(SECRET),
                "the failure text carries the host file's contents: {text}"
            );
        }
        Ok(p) => {
            // Not a pass by default. If a builder ever accepts this, the image
            // must still not hold the bytes — and the tag has to be cleaned up.
            let holds = Command::new("docker")
                .args([
                    "run",
                    "--rm",
                    "--network",
                    "none",
                    &p.image,
                    "cat",
                    "/payload.txt",
                ])
                .output()
                .expect("docker run");
            let text = String::from_utf8_lossy(&holds.stdout).to_string();
            remove_built_image(&spec.reference);
            panic!(
                "`prepare` built an image from a context containing an escaping symlink. \
                 /payload.txt in it: {text:?}. If this is the builder resolving context \
                 symlinks against the host, the refusal must move to ThruLine's side."
            );
        }
    }

    // 2. Both builders, directly, on the same context — because the refusal
    //    above is the builder's and this is the only place that says so. The
    //    materialised tree is rebuilt here rather than reused: `prepare` holds
    //    its workspace in a `TempDir` guard that is dropped on refusal, which
    //    is correct and leaves nothing to inspect.
    let ctx = tempfile::tempdir().expect("tempdir");
    thruline_reexec::materialize::materialize(root, &tree, ctx.path()).expect("materialize");
    assert!(
        std::fs::symlink_metadata(ctx.path().join("payload.txt"))
            .expect("stat")
            .file_type()
            .is_symlink(),
        "materialize did not reproduce the link, so the context under test is not the \
         one the product builds"
    );
    for buildkit in ["1", "0"] {
        let tag = format!("thruline-reexec:context-symlink-probe-{buildkit}");
        let out = Command::new("docker")
            .args(["build", "-t", &tag])
            .arg(ctx.path())
            .env("DOCKER_BUILDKIT", buildkit)
            .output()
            .expect("docker build");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if out.status.success() {
            let holds = Command::new("docker")
                .args([
                    "run",
                    "--rm",
                    "--network",
                    "none",
                    &tag,
                    "cat",
                    "/payload.txt",
                ])
                .output()
                .expect("docker run");
            let got = String::from_utf8_lossy(&holds.stdout).to_string();
            remove_built_image(&tag);
            assert!(
                !got.contains(SECRET),
                "DOCKER_BUILDKIT={buildkit} resolved a context symlink against the host \
                 and put {SECRET} into an image. This is the case that requires ThruLine \
                 to refuse escaping symlinks in the build context itself, before the \
                 build: /payload.txt held {got:?}"
            );
            panic!(
                "DOCKER_BUILDKIT={buildkit} built the image without refusing the escaping \
                 symlink. It did not leak the secret, but the refusal this test relies on \
                 is gone: {text}"
            );
        }
        assert!(
            !text.contains(SECRET),
            "DOCKER_BUILDKIT={buildkit} printed the host file's contents: {text}"
        );
    }
}
