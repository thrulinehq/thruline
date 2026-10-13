//! §7's image-selection order, and the lockfile-keyed dependency layer.
//!
//! > *image selection order: `.thruline/reexec.yml` → `devcontainer.json` →
//! > `Dockerfile` (build) → ecosystem default images*
//!
//! Each step is asserted **with the steps below it also present**, so the test
//! shows precedence rather than showing that each source works in isolation —
//! four independent "this source is read" tests would all pass under a wrong
//! order.

use std::path::Path;

use thruline_reexec::image::{select, ImageSource};

fn write(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, text).expect("write");
}

/// A repository carrying every source at once, so precedence is what is tested.
fn everything(root: &Path) {
    write(
        root,
        ".thruline/reexec.yml",
        "version: 1\nimage: from-yml:1\n",
    );
    write(
        root,
        ".devcontainer/devcontainer.json",
        "{\n  // comment\n  \"image\": \"from-devcontainer:1\",\n}\n",
    );
    write(root, "Dockerfile", "FROM scratch\n");
    write(root, "requirements.txt", "pytest==9.0.1\n");
}

#[test]
fn reexec_yml_wins_over_everything_else() {
    let dir = tempfile::tempdir().expect("tempdir");
    everything(dir.path());
    let spec = select(dir.path()).expect("select");
    assert_eq!(spec.reference, "from-yml:1");
    assert_eq!(spec.source, ImageSource::ReexecYml);
}

#[test]
fn devcontainer_wins_when_the_yml_names_no_image() {
    let dir = tempfile::tempdir().expect("tempdir");
    everything(dir.path());
    // The file exists and is valid; it just does not answer this question.
    write(
        dir.path(),
        ".thruline/reexec.yml",
        "version: 1\nnetwork: none\n",
    );
    let spec = select(dir.path()).expect("select");
    assert_eq!(spec.reference, "from-devcontainer:1");
    assert_eq!(spec.source, ImageSource::Devcontainer);
}

#[test]
fn a_dockerfile_is_built_when_nothing_names_an_image() {
    let dir = tempfile::tempdir().expect("tempdir");
    everything(dir.path());
    std::fs::remove_file(dir.path().join(".thruline/reexec.yml")).expect("rm");
    std::fs::remove_file(dir.path().join(".devcontainer/devcontainer.json")).expect("rm");
    let spec = select(dir.path()).expect("select");
    assert_eq!(spec.source, ImageSource::Dockerfile);
    assert_eq!(spec.build_context.as_deref(), Some(dir.path()));
    // The tag carries the Dockerfile's hash, so an edited Dockerfile cannot
    // silently reuse the image built from the old one.
    let first = spec.reference.clone();
    write(dir.path(), "Dockerfile", "FROM scratch\nENV X=1\n");
    let second = select(dir.path()).expect("select").reference;
    assert_ne!(first, second, "an edited Dockerfile reused its old tag");
}

#[test]
fn the_ecosystem_default_is_last_and_comes_from_the_lockfiles_present() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "requirements.txt", "pytest==9.0.1\n");
    let spec = select(dir.path()).expect("select");
    assert_eq!(spec.source, ImageSource::EcosystemDefault);
    assert_eq!(spec.reference, "python:3-slim");

    let node = tempfile::tempdir().expect("tempdir");
    write(node.path(), "package-lock.json", "{}\n");
    assert_eq!(
        select(node.path()).expect("select").reference,
        "node:22-slim"
    );
}

#[test]
fn a_repository_that_says_nothing_is_undecidable_rather_than_guessed_at() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "README.md", "hello\n");
    let err = select(dir.path()).expect_err("must refuse to guess");
    // Guessing would make the receipt say the tests ran in "a clean container"
    // while naming an environment nobody chose.
    assert!(
        format!("{err}").contains("no image could be selected"),
        "{err}"
    );
}

#[test]
fn the_dependency_layer_key_moves_with_the_lockfile_and_not_with_anything_else() {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "requirements.txt", "pytest==9.0.1\n");
    let a = select(dir.path()).expect("select").deps.expect("a layer");
    assert!(a
        .tag_for_base(BASE_A)
        .starts_with("thruline-reexec:python-"));
    assert_eq!(a.lockfiles, vec!["requirements.txt".to_string()]);
    assert!(a.install.contains("pip install"));

    // An unrelated source edit must NOT invalidate the cached layer: that is
    // the whole value of keying on the lockfile.
    write(dir.path(), "shipping.py", "x = 1\n");
    let b = select(dir.path()).expect("select").deps.expect("a layer");
    assert_eq!(
        a.key, b.key,
        "a source edit changed the dependency layer key"
    );

    // A lockfile edit must.
    write(dir.path(), "requirements.txt", "pytest==9.0.0\n");
    let c = select(dir.path()).expect("select").deps.expect("a layer");
    assert_ne!(
        a.key, c.key,
        "a lockfile edit reused the old dependency layer"
    );

    // And so must a changed install command, or a repository that edited only
    // its `install:` line would run the old one.
    write(dir.path(), "requirements.txt", "pytest==9.0.1\n");
    write(
        dir.path(),
        ".thruline/reexec.yml",
        "version: 1\ninstall: pip install -r requirements.txt --pre\n",
    );
    let d = select(dir.path()).expect("select").deps.expect("a layer");
    assert_ne!(
        a.key, d.key,
        "a changed install command reused the old layer"
    );
}

/// Two real image ids, as `docker image inspect --format {{.Id}}` prints them.
const BASE_A: &str = "sha256:adafcc17694d0da3e1a4b0e0ba9e6b0b8e5d4d5f6a7b8c9d0e1f2a3b4c5d6e7f";
const BASE_B: &str = "sha256:cad9a2c871760f1e2d3c4b5a69788796a5b4c3d2e1f0a9b8c7d6e5f4a3b2c1d0";

#[test]
fn the_same_lockfile_on_a_different_base_image_is_a_different_layer() {
    // The measured defect this closes: the tag was a function of (ecosystem,
    // lockfile bytes, install command) only, so `ensure_deps_layer` served a
    // cached layer on presence alone and the caller then REPLACED the requested
    // base with it. A repository declaring `python:3.13.7-slim-bookworm` ran
    // Python 3.14.7 on Debian 13 while the record read `image_source:
    // reexec-yml`.
    //
    // Asserted in the failure direction: the two tags must DIFFER.
    //
    // **This test alone does not hold that property, and the comment that used
    // to stand here said it did.** Deleting `base_digest` from the hashed key
    // in `tag_for_base` leaves all seven tests in this file green — witnessed
    // 2026-09-04 — because the tag also carries the digest's first twelve
    // characters as *text*, and BASE_A and BASE_B differ there. The assertion
    // below therefore passes on a key that ignores the base entirely.
    // `the_tag_is_a_function_of_the_whole_base_digest` is the test that goes
    // red on that deletion.
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "requirements.txt", "pytest==9.0.1\n");
    let layer = select(dir.path()).expect("select").deps.expect("a layer");

    let on_a = layer.tag_for_base(BASE_A);
    let on_b = layer.tag_for_base(BASE_B);
    assert_ne!(
        on_a, on_b,
        "one tag names the layer on two different base images, so a presence check on it \
         serves whichever base built it first"
    );
    assert_eq!(on_a, layer.tag_for_base(BASE_A), "the tag is not stable");

    // The digest is in the tag TEXT as well as in its hash, so `docker images`
    // shows a human which base a layer sits on.
    assert!(on_a.contains("adafcc17694d"), "{on_a}");
    assert!(on_b.contains("cad9a2c87176"), "{on_b}");
    // And a docker tag is still a legal docker tag.
    let (_, tag) = on_a.split_once(':').expect("a tag");
    assert!(tag.len() <= 128, "{tag}");
    assert!(
        tag.chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)),
        "{tag} is not a legal docker tag"
    );
}

/// Two base digests that agree on the twelve characters the tag shows a human
/// and differ only after them. Docker image ids are full sha256; a tag keyed on
/// a twelve-character prefix is keyed on 48 bits of one.
const BASE_C: &str = "sha256:adafcc17694d0000000000000000000000000000000000000000000000000001";
const BASE_D: &str = "sha256:adafcc17694d0000000000000000000000000000000000000000000000000002";

#[test]
fn the_tag_is_a_function_of_the_whole_base_digest() {
    // The property `the_same_lockfile_on_a_different_base_image_is_a_different_layer`
    // is *stated* to hold but does not: that test's two digests differ in the
    // twelve characters the tag prints, so it passes even when the hashed key
    // ignores the base completely. This one cannot: the printed characters are
    // identical, so only the hash can tell the two tags apart.
    //
    // What the deletion costs, if it is ever made: `ensure_deps_layer` serves a
    // cached layer on a presence check of this tag, and the caller then REPLACES
    // the requested base with whatever built it first. That is the measured
    // defect the base went into the key for — a repository declaring
    // `python:3.13.7-slim-bookworm` running Python 3.14.7 on Debian 13 while the
    // record read `image_source: reexec-yml`.
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "requirements.txt", "pytest==9.0.1\n");
    let layer = select(dir.path()).expect("select").deps.expect("a layer");

    let on_c = layer.tag_for_base(BASE_C);
    let on_d = layer.tag_for_base(BASE_D);
    assert_ne!(
        on_c, on_d,
        "two different base images share one layer tag, so a presence check on it serves \
         whichever base built the layer first — and the run then happens on a base nobody \
         asked for, with the receipt still naming the one that was requested"
    );
    // And the difference is in the hashed half, not in the twelve characters
    // printed for humans: those are equal by construction here.
    let head = |t: &str| t.split("-base-").next().unwrap_or(t).to_string();
    let tail = |t: &str| t.split("-base-").nth(1).unwrap_or("").to_string();
    assert_eq!(
        tail(&on_c),
        tail(&on_d),
        "the fixture digests must print alike"
    );
    assert_ne!(
        head(&on_c),
        head(&on_d),
        "the base digest is not in the key"
    );
}
