//! Every install path names the artifacts the release actually publishes.
//!
//! # The failure this exists against, which had already happened
//!
//! `packaging/install.sh`, `packaging/npm/install.js` and
//! `packaging/action/install.sh` once named archives the build did not make.
//! The private workflow now stages four `thruline-cli-<target>.tar.gz` archives
//! and `SHA256SUMS` for a draft in `thrulinehq/thruline`; cargo-dist's local
//! sidecars are inputs to that staging step, not public release assets. The
//! archive keeps the executable one directory down. A release could publish
//! perfectly while every installer still 404s if these names drift again.
//!
//! That is the whole shape: the release side and the install side are written
//! months apart by people looking at different files, and the only thing that
//! connects them is a string. This test is that connection.
//!
//! # It reads the config rather than a copy of it
//!
//! The target list comes from `dist-workspace.toml` itself, so adding a target
//! there and forgetting an installer fails here. The *shape* of the name is
//! asserted against the local cargo-dist plan and public staging allowlist —
//! `dist` is not installed on every machine that runs `cargo test`, so shelling
//! out to it would skip this guard where it is needed. `RELEASING.md` rechecks
//! the plan before a draft run.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    let p = root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn runnable(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.starts_with('#') && !line.starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The targets `dist-workspace.toml` says will be built.
fn targets() -> Vec<String> {
    let text = read("dist-workspace.toml");
    let start = text
        .find("targets = [")
        .expect("dist-workspace.toml has no targets");
    let rest = &text[start..];
    let end = rest.find(']').expect("unterminated targets list");
    let list: Vec<String> = rest[..end]
        .split('"')
        .filter(|s| s.contains("-") && !s.contains('=') && !s.contains('\n'))
        .map(str::to_string)
        .collect();
    assert_eq!(
        list.len(),
        4,
        "the release must ship exactly four targets: {list:?}"
    );
    list
}

/// cargo-dist's artifact name for a target. **One definition.**
fn archive(target: &str) -> String {
    format!("thruline-cli-{target}.tar.gz")
}

/// The four targets §15 and the operator's brief require, by name.
#[test]
fn the_release_builds_macos_arm64_and_x86_and_linux_x86_and_arm64_musl() {
    let t = targets();
    for want in [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "x86_64-unknown-linux-musl",
        "aarch64-unknown-linux-musl",
    ] {
        assert!(
            t.iter().any(|x| x == want),
            "{want} is not a release target: {t:?}"
        );
    }
    // **Both Linux architectures.** Static musl closes the libc half of the
    // bench's failure class and not the arch half: task images that pin
    // `--platform=linux/amd64` cannot execute an aarch64 binary however it is
    // linked, which is five of the six failures there. Shipping one Linux arch
    // would leave them exactly as broken.
    let linux: Vec<&String> = t.iter().filter(|x| x.contains("linux")).collect();
    assert_eq!(
        linux.len(),
        2,
        "the release ships {} Linux target(s): {linux:?}. Both x86_64 and aarch64 are \
         load-bearing — an aarch64 binary cannot run in an amd64-pinned container \
         however it is linked.",
        linux.len()
    );

    // **Linux is musl and nothing else.** A `-gnu` target reintroduces the
    // failure the bench measured — `libc.so.6: version 'GLIBC_2.39' not found`
    // — and it reintroduces it as an artifact that runs fine on the machine
    // that built it.
    for target in &t {
        if target.contains("linux") {
            assert!(
                target.ends_with("-musl"),
                "{target} is a dynamically linked Linux target. glibc is backward \
                 compatible and not forward, so it will not start on any image older \
                 than the builder's — measured by the bench, under an error string \
                 (\"asciinema is not installed in the container\") that named the \
                 wrong cause."
            );
        }
    }
}

/// The install paths and the release agree about what is published.
#[test]
fn every_install_path_fetches_an_artifact_the_release_produces() {
    let sh = read("packaging/install.sh");
    let js = read("packaging/npm/install.js");
    let action = read("packaging/action/install.sh");
    let brew = read("packaging/homebrew/Formula/thruline.rb");

    // The archive name is built from `$TARGET` in the shell paths and from
    // `${target}` in Node, so the assertion is on the template, not on four
    // expanded strings.
    assert!(
        sh.contains("thruline-cli-$TARGET.tar.gz"),
        "packaging/install.sh does not name cargo-dist's archive"
    );
    assert!(
        js.contains("`thruline-cli-${target}.tar.gz`"),
        "packaging/npm/install.js does not name cargo-dist's archive"
    );
    assert!(
        action.contains("thruline-cli-${TARGET}.tar.gz"),
        "packaging/action/install.sh does not name cargo-dist's archive"
    );
    // The formula names each target explicitly, so each one is checked.
    for target in targets() {
        let name = archive(&target);
        if target.contains("linux") || target.contains("darwin") {
            assert!(
                brew.contains(&name),
                "the Homebrew formula does not name {name}"
            );
        }
    }

    // The public payload uses SHA256SUMS for all four final archives. The
    // local cargo-dist sidecars become one public checksum file at staging.
    //
    // **Over the running lines only.** REPORT.md records five instances in this
    // build of a guard that matched the comment *explaining* why something is
    // wrong — `|| true` inside the comment forbidding it, `latest` inside the
    // sentence explaining its absence, `.checks` inside `inputs.checksum`,
    // `^test result` matching a test named `result_only_…`, and `musl` inside
    // the comment saying musl is wrong. This assertion was the sixth: it fired
    // on `packaging/install.sh`'s own explanation of the bug it had been fixed
    // for. A guard is a search, and a search has a space.
    for (what, text) in [
        ("packaging/install.sh", &sh),
        ("packaging/npm/install.js", &js),
    ] {
        let running = runnable(text);
        assert!(
            running.contains("SHA256SUMS"),
            "{what} does not fetch the public SHA256SUMS"
        );
        assert!(
            !running.contains("sha256.sum"),
            "{what} fetches a cargo-dist sidecar instead of public SHA256SUMS"
        );
        // Positive control: the walk is reading real code, not an empty string
        // left behind by a comment filter that ate the file.
        assert!(
            running.contains("thruline-cli-"),
            "the comment filter removed everything from {what}"
        );
    }
    assert!(
        runnable(&sh).contains("\"$BASE/SHA256SUMS\""),
        "install.sh does not download the checksums from its archive base"
    );
    assert!(
        runnable(&js).contains("fetchBuffer(`${base}/SHA256SUMS`)"),
        "npm does not download the checksums from its archive base"
    );

    // The binary is one directory down inside the archive.
    assert!(
        sh.contains("thruline-cli-$TARGET/thruline"),
        "packaging/install.sh installs from the archive root; the binary is one \
         directory down"
    );
    assert!(
        js.contains("`thruline-cli-${target}`"),
        "packaging/npm/install.js installs from the archive root"
    );
}

/// The source is public (FSL-1.1-MIT) at the same repository that hosts the
/// release downloads, and the default binary download is that release. An
/// explicit test-harness or mirror override is warned about before curl. The
/// manifest slug and the download slug are therefore the same one, and neither
/// may be the old private origin or the never-existing `thruline/thruline`.
#[test]
fn install_paths_resolve_to_the_public_release_repository() {
    let manifest = read("Cargo.toml");
    let source_repo = manifest
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("repository = \"https://github.com/")
        })
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("Cargo.toml names its source repository on github.com");
    assert_eq!(
        source_repo, "thrulinehq/thruline",
        "the Cargo source repository is not the public repository the downloads come from"
    );
    let sh = runnable(&read("packaging/install.sh"));
    assert!(sh.contains("REPO=\"thrulinehq/thruline\""));
    assert!(
        sh.contains("BASE=\"${THRULINE_BASE_URL:-https://github.com/$REPO/releases/download/$VERSION}\""),
        "curl install default is not the public GitHub release, or the explicit override disappeared"
    );
    assert!(sh.contains("\"$BASE/$ARCHIVE\""));
    assert!(sh.contains("\"$BASE/SHA256SUMS\""));

    let npm = runnable(&read("packaging/npm/install.js"));
    assert!(npm.contains("const REPO = 'thrulinehq/thruline'"));
    assert!(npm.contains("https://github.com/${REPO}/releases/download/v${VERSION}"));
    assert!(npm.contains("fetchBuffer(`${base}/${name}`)"));
    assert!(npm.contains("fetchBuffer(`${base}/SHA256SUMS`)"));
    let package = read("packaging/npm/package.json");
    assert!(package.contains("git+https://github.com/thrulinehq/thruline.git"));

    let action = runnable(&read("packaging/action/install.sh"));
    assert!(action.contains(
        "URL=\"https://github.com/thrulinehq/thruline/releases/download/${TL_VERSION}/${ARCHIVE}\""
    ));
    let brew = runnable(&read("packaging/homebrew/Formula/thruline.rb"));
    for target in targets() {
        let url = format!(
            "url \"https://github.com/thrulinehq/thruline/releases/download/v#{{version}}/{}\"",
            archive(&target)
        );
        assert!(
            brew.contains(&url),
            "Homebrew does not use the public URL for {target}"
        );
    }
    let public_readme = read("packaging/public-release/README.md");
    assert!(public_readme
        .contains("base=\"https://github.com/thrulinehq/thruline/releases/download/v0.2.0\""));
    for (path, text, expected_urls) in [
        ("packaging/install.sh", sh, 1),
        ("packaging/npm/install.js", npm, 1),
        ("packaging/action/install.sh", action, 1),
        ("packaging/homebrew/Formula/thruline.rb", brew, 4),
    ] {
        assert_eq!(
            text.matches("releases/download/").count(),
            expected_urls,
            "{path} gained another release download base"
        );
        assert!(
            !text.contains("github.com/example/thruline-private/releases/download")
                && !text.contains("github.com/thruline/thruline/releases/download"),
            "{path} has a release download outside thrulinehq/thruline"
        );
    }
}

/// The opt-in source override is required by the acceptance harness. It must
/// be visible to a human before the first download and must not disable the
/// checksum refusal. The curl stub makes the URLs and stderr order observable
/// without a network request or a release build.
#[test]
fn curl_installer_warns_before_override_and_checksums_both_sources() {
    let fixture = r#"
import os, subprocess, sys, tempfile
from pathlib import Path

installer = Path(sys.argv[1]) / 'packaging/install.sh'
archive = 'thruline-cli-aarch64-apple-darwin.tar.gz'
version = 'v0.1.0'
official = f'https://github.com/thrulinehq/thruline/releases/download/{version}'
mirror = 'https://mirror.example.invalid/artifacts/v0.1.0'
with tempfile.TemporaryDirectory() as temp:
    root = Path(temp)
    stub = root / 'stub'
    prefix = root / 'prefix'
    stub.mkdir()
    prefix.mkdir()
    curl = stub / 'curl'
    curl.write_text('''#!/bin/sh
output=
url=
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) output="$2"; shift 2 ;;
    --retry) shift 2 ;;
    -*) shift ;;
    *) url="$1"; shift ;;
  esac
done
printf 'download:%s\\n' "$url" >&2
case "$url" in
  */SHA256SUMS) printf '%064d  thruline-cli-aarch64-apple-darwin.tar.gz\\n' 0 > "$output" ;;
  *) printf 'deliberately wrong archive bytes\\n' > "$output" ;;
esac
''')
    uname = stub / 'uname'
    uname.write_text('''#!/bin/sh
case "$1" in
  -s) printf 'Darwin\\n' ;;
  -m) printf 'arm64\\n' ;;
  *) exit 1 ;;
esac
''')
    curl.chmod(0o755)
    uname.chmod(0o755)
    for base, overridden in ((official, False), (mirror, True)):
        env = os.environ.copy()
        env.update(PATH=str(stub) + os.pathsep + env['PATH'],
                   THRULINE_VERSION=version, THRULINE_PREFIX=str(prefix))
        env.pop('THRULINE_BASE_URL', None)
        if overridden:
            env['THRULINE_BASE_URL'] = base
        result = subprocess.run(['sh', str(installer)], env=env,
                                capture_output=True, text=True, timeout=20)
        stderr = result.stderr
        downloads = [line.removeprefix('download:') for line in stderr.splitlines()
                     if line.startswith('download:')]
        assert downloads == [f'{base}/{archive}', f'{base}/SHA256SUMS'], (downloads, stderr)
        if overridden:
            warning = f'thruline: WARNING: downloading from non-official source {base}'
            assert warning in stderr, stderr
            assert stderr.index(warning) < stderr.index('download:'), stderr
        else:
            assert 'non-official source' not in stderr, stderr
        assert result.returncode != 0, stderr
        assert 'checksum mismatch' in stderr, stderr
        assert not (prefix / 'thruline').exists(), stderr
"#;
    let output = Command::new("python3")
        .arg("-c")
        .arg(fixture)
        .arg(root())
        .output()
        .expect("python3 is required by the release workflow");
    assert!(
        output.status.success(),
        "curl source and checksum contract failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// cargo-dist must not also generate a shell installer.
///
/// Its generated one prints *"skipping sha256 checksum verification (it
/// requires the 'sha256sum' command)"* and installs anyway. macOS has `shasum`,
/// not `sha256sum`, so that path is every Mac. Two installers on one release is
/// also how one of them goes stale.
#[test]
fn the_release_ships_one_installer_and_it_is_the_one_that_refuses() {
    let cfg = read("dist-workspace.toml");
    let line = cfg
        .lines()
        .find(|l| l.trim_start().starts_with("installers = "))
        .expect("dist-workspace.toml has no `installers` key");
    assert!(
        line.contains("[]"),
        "cargo-dist is generating an installer again ({line}). Its checksum \
         verification is skippable and macOS skips it."
    );
    let sh = read("packaging/install.sh");
    assert!(
        sh.contains("refusing to install unverified"),
        "packaging/install.sh no longer refuses when it cannot checksum"
    );
    assert!(
        sh.contains("refusing to install an unverified binary"),
        "packaging/install.sh no longer refuses when the checksum file is absent"
    );
}

/// A Homebrew tap is a repository whose formulae live in `Formula/`.
///
/// The formula was a loose `packaging/homebrew/thruline.rb`. Homebrew does not
/// read that: it clones `homebrew-<tap>` and globs `Formula/*.rb`. A tap
/// published from the old layout would have installed nothing and said the
/// formula did not exist — a failure that only appears after the tap is public.
#[test]
fn the_formula_is_in_the_layout_a_tap_actually_reads() {
    let formula = root().join("packaging/homebrew/Formula/thruline.rb");
    assert!(
        formula.is_file(),
        "the formula is not at packaging/homebrew/Formula/thruline.rb; Homebrew globs \
         Formula/*.rb in the tap repository and will not find it anywhere else"
    );
    let text = std::fs::read_to_string(&formula).expect("formula");
    // The placeholders must still be placeholders. A formula that reaches a tap
    // with a real-looking but wrong checksum is worse than one that obviously
    // has not been filled in.
    assert!(
        text.contains("PLACEHOLDER"),
        "the formula's checksums have been filled in on the branch. They belong in \
         the tap repository, filled from the published release's SHA256SUMS — not \
         committed here, where they go stale against the next release."
    );
}
