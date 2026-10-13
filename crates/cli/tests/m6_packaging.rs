//! Every install path verifies before it executes, and nothing resolves a
//! floating version.
//!
//! There are now four ways to get this binary — the Action, `install.sh`, the
//! Homebrew formula, the `npx` shim — and each is a place where the wrong bytes
//! could be executed with the user's trust. No public release has been made.
//! These checks guard the source contract: **the checksum comparison happens
//! before the binary is put anywhere, and no path continues without it.**
//!
//! Written as a walk rather than as review notes for the usual reason: a rule
//! that lives only in a comment is a rule until someone is in a hurry.

use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The lines a shell actually runs.
///
/// **Three of this session's censuses matched their own explanation** — a
/// comment naming the forbidden idiom is not the forbidden idiom. Forcing the
/// explanation out of the file it is about is the wrong repair, so the walk
/// learns to tell a comment from a command instead.
fn runnable(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#') && !l.starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The installer refuses rather than continues, on every branch.
#[test]
fn install_sh_fails_closed() {
    let s = read("packaging/install.sh");
    assert!(s.contains("set -eu"), "install.sh does not fail on error");
    // The verification must precede the install.
    let compare = s
        .find(r#"[ "$HAVE" = "$WANT" ]"#)
        .expect("no checksum comparison");
    let install = s.find("install -m 0755").expect("no install step");
    assert!(
        compare < install,
        "the binary is installed before it is verified"
    );
    // And there is no way past a missing SHA256SUMS.
    assert!(
        s.contains("refusing to install an unverified binary"),
        "a missing SHA256SUMS does not stop the install"
    );
    assert!(
        s.contains("refusing to install unverified"),
        "a machine with no sha256 tool does not stop the install"
    );
    // Positive control: the comment-stripping walk still sees the script.
    assert!(
        runnable(&s).contains("shasum -a 256"),
        "the runnable walk found no commands, so the shrug check is vacuous"
    );
    // `|| true` and `|| :` are the idioms that turn a failed check into a
    // shrug. There must be none in a script people pipe into a shell.
    let running = runnable(&s);
    for shrug in ["|| true", "|| :", "2>/dev/null || echo"] {
        assert!(
            !running.contains(shrug),
            "install.sh contains `{shrug}`, which turns a failure into a continue"
        );
    }
}

/// The npm shim must not swallow an exit code: ThruLine's exit codes are its CI
/// interface, and a wrapper that returned 0 for a failing gate would turn every
/// red check green.
#[test]
fn the_npm_shim_preserves_the_exit_code() {
    let s = read("packaging/npm/bin/thruline.js");
    assert!(
        s.contains("process.exit(r.status === null ? 1 : r.status)"),
        "the npx shim does not forward the child's exit code:\n{s}"
    );
    let installer = read("packaging/npm/install.js");
    let compare = installer
        .find("have !== want[0]")
        .expect("no checksum comparison");
    let place = installer.find("copyFileSync").expect("no install step");
    assert!(
        compare < place,
        "the binary is placed before it is verified"
    );
}

/// The Homebrew formula's `test do` block asserts a behaviour, not a file's
/// existence — `brew test` runs it, and an install that produced a binary which
/// cannot refuse a broken policy is an install worth failing.
#[test]
fn the_homebrew_formula_tests_a_behaviour() {
    let s = read("packaging/homebrew/Formula/thruline.rb");
    assert!(s.contains("test do"), "no test block");
    assert!(
        s.contains("require_reciept"),
        "the formula's test does not exercise the policy refusal"
    );
    assert!(
        s.contains("license \"FSL-1.1-MIT\""),
        "the formula does not declare the FSL-1.1-MIT license"
    );
    // The template stays unfilled until the user publishes a release and the
    // separate tap PR is prepared from that release's checksums.
    assert!(
        s.contains("PLACEHOLDER"),
        "the private template contains checksums not taken from a published release"
    );
}

/// Read only the top-level `on:` block. A mention in a comment or a shell
/// command is not a workflow trigger.
fn on_block(workflow: &str) -> String {
    let (_, after) = workflow
        .split_once("\non:\n")
        .expect("workflow has no top-level on: block");
    after
        .lines()
        .take_while(|line| line.starts_with(' ') || line.is_empty() || line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

fn triggers(block: &str) -> Vec<&str> {
    block
        .lines()
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .filter(|line| !line.trim_start().starts_with('#'))
        .map(|line| {
            line.trim()
                .strip_suffix(':')
                .unwrap_or_else(|| panic!("unsupported inline workflow event: {line}"))
        })
        .collect()
}

/// No workflow may wake a release on a pushed tag. The two ordinary CI
/// workflows keep their branch filters; the release workflow is manual only.
#[test]
fn no_workflow_has_a_tag_trigger() {
    let directory = root().join(".github/workflows");
    let mut files = 0;
    for entry in std::fs::read_dir(&directory).expect("workflow directory") {
        let path = entry.expect("workflow entry").path();
        if !matches!(
            path.extension().and_then(|x| x.to_str()),
            Some("yml" | "yaml")
        ) {
            continue;
        }
        files += 1;
        let source = std::fs::read_to_string(&path).expect("workflow source");
        let block = on_block(&source);
        let events = triggers(&block);
        assert!(
            !events.is_empty(),
            "{} has no readable triggers",
            path.display()
        );
        assert!(
            !events
                .iter()
                .any(|event| *event == "release" || *event == "create"),
            "{} can trigger on a release or tag creation",
            path.display()
        );
        assert!(
            !block.lines().any(|line| {
                let line = line.trim_start();
                !line.starts_with('#')
                    && (line.starts_with("tags:") || line.starts_with("tags-ignore:"))
            }),
            "{} contains a tag filter in its trigger block",
            path.display()
        );
        if events.contains(&"push") {
            let (_, after_push) = block.split_once("  push:\n").expect("push trigger shape");
            let filters = after_push
                .lines()
                .take_while(|line| line.starts_with("    ") || line.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                filters
                    .lines()
                    .any(|line| line.trim_start().starts_with("branches:")),
                "{} has an unfiltered push trigger that can run on tags",
                path.display()
            );
            assert!(
                !filters.lines().any(|line| {
                    let line = line.trim_start();
                    line.starts_with("tags:") || line.starts_with("tags-ignore:")
                }),
                "{} has a tag-triggered push workflow",
                path.display()
            );
        }
        let running = runnable(&source);
        if path.file_name().and_then(|name| name.to_str()) != Some("release.yml") {
            assert!(
                !running.contains("gh release create")
                    && !running.contains("gh release edit")
                    && !running.contains("--draft=false"),
                "{} contains a release command outside release.yml",
                path.display()
            );
        }
    }
    assert!(
        files >= 3,
        "workflow census did not inspect every release lane"
    );
}

/// A real draft is a manual, source-confirmed act. The private workflow must
/// verify the five-file payload before its only GitHub release command.
#[test]
fn the_release_lane_is_manual_guarded_and_draft_only() {
    let release = read(".github/workflows/release.yml");
    let block = on_block(&release);
    assert_eq!(triggers(&block), ["workflow_dispatch"]);
    assert!(block.contains("      version:") && block.contains("      confirm_sha:"));
    assert!(block.contains("      dry_run:") && block.contains("        default: true"));
    assert!(release.contains("DEFAULT_BRANCH: ${{ github.event.repository.default_branch }}"));

    let preflight = release
        .split_once("\n  preflight:\n")
        .and_then(|(_, after)| after.split_once("\n  build:\n"))
        .map(|(section, _)| section)
        .expect("preflight must precede build");
    // The Apple secrets are all-or-nothing, not required: with none of them the
    // draft is still made, and says it is not notarized. No other secret is
    // used; the draft is made with the workflow's own token.
    for secret in [
        "APPLE_DEVELOPER_ID_P12_BASE64",
        "APPLE_DEVELOPER_ID_P12_PASSWORD",
        "APPLE_NOTARY_KEY_P8_BASE64",
        "APPLE_NOTARY_KEY_ID",
        "APPLE_NOTARY_ISSUER_ID",
    ] {
        assert!(
            preflight.contains(&format!("{secret}: ${{{{ secrets.{secret} }}}}"))
                && preflight.contains(&format!("\"{secret}\",")),
            "preflight does not look at secret {secret}"
        );
    }
    assert!(preflight.contains("Apple signing secrets are only partly set"));
    assert!(
        !release.contains("PUBLIC_RELEASE_TOKEN") && !release.contains("public_base_sha"),
        "the workflow still depends on a separate release repository"
    );
    let real_guard = preflight
        .split_once("          if not dry_run:\n")
        .and_then(|(_, after)| {
            after.split_once("          # No release may exist yet for this tag.")
        })
        .map(|(guard, _)| guard)
        .expect("real-run guard is missing");
    for required in [
        "default_ref = \"refs/heads/\" + os.environ[\"DEFAULT_BRANCH\"]",
        "os.environ[\"GITHUB_REF\"] != default_ref",
        "re.fullmatch(r\"[0-9a-f]{40}\", confirmed)",
        "confirmed != source_sha",
        "head.stdout.strip() != source_sha",
        "[\"git\", \"cat-file\", \"-t\", tag_ref]",
        "tag_type.stdout.strip() != \"tag\"",
        "[\"git\", \"rev-parse\", tag_ref + \"^{commit}\"]",
        "peeled.stdout.strip() != source_sha",
        "os.environ[\"GITHUB_REPOSITORY\"].lower() != \"thrulinehq/thruline\"",
        "^pub const PAID_GATE: bool = true;",
    ] {
        assert!(
            real_guard.contains(required),
            "real draft lost source guard: {required}"
        );
    }
    assert!(
        !real_guard.contains("TRIAL_DAYS"),
        "v0.1.1 release preflight still requires a trial constant"
    );
    assert!(preflight.contains("does not match Cargo.toml version"));
    assert!(preflight.contains("Release {tag} already exists"));
    // The repository holds the source, so it is no longer required to be bare.
    assert!(
        !preflight.contains("must contain only README.md and LICENSE.md")
            && !preflight.contains("(\"README.md\", \"blob\")"),
        "the preflight still demands an empty public default branch"
    );

    // Dependency policy is a blocking preflight, not a warning after artifacts
    // have left the private repository. The SBOM is a separate private Actions
    // artifact; the draft's five public assets never include it.
    let preflight_steps: Vec<&str> = preflight.split("      - ").collect();
    let deny_step = preflight_steps
        .iter()
        .find(|step| step.lines().any(|line| line.trim() == "cargo deny check"))
        .expect("preflight does not run cargo deny check");
    assert!(
        preflight.contains("cargo install cargo-deny --version 0.20.2 --locked"),
        "release audit tool version is not pinned"
    );
    assert!(
        !deny_step.contains("continue-on-error: true")
            && !runnable(deny_step).contains("||")
            && !runnable(deny_step).contains("; true"),
        "cargo deny check can fail without stopping the release"
    );
    let deny = preflight
        .find("cargo deny check")
        .expect("no dependency audit");
    let sbom = preflight
        .find("cargo cyclonedx")
        .expect("no CycloneDX SBOM generation");
    let private_upload = preflight
        .find("name: private-cyclonedx-sbom")
        .expect("CycloneDX SBOM is not retained as a private workflow artifact");
    assert!(
        deny < sbom && sbom < private_upload,
        "the audit and SBOM must complete before any artifact upload"
    );
    assert!(
        preflight.contains("cargo install cargo-cyclonedx --version 0.5.9 --locked")
            && preflight.contains("--manifest-path crates/cli/Cargo.toml")
            && preflight.contains("--no-default-features")
            && preflight.contains("--target all")
            && preflight.contains("--describe binaries")
            && preflight.contains("--all")
            && preflight.contains("--format json"),
        "the pinned CycloneDX command no longer describes the release CLI"
    );
    let sbom_upload_step = preflight_steps
        .iter()
        .find(|step| step.contains("name: private-cyclonedx-sbom"))
        .expect("private SBOM upload step is missing");
    assert!(
        sbom_upload_step.contains("if: ${{ !inputs.dry_run }}")
            && sbom_upload_step.contains("uses: actions/upload-artifact@v4")
            && sbom_upload_step.contains("path: private-sbom/"),
        "the SBOM is not an upload-only, private workflow artifact"
    );
    let first_upload = release
        .find("uses: actions/upload-artifact@v4")
        .expect("workflow has no private artifact upload");
    assert!(
        release.find("cargo deny check").unwrap() < first_upload,
        "an artifact can be uploaded before cargo deny check"
    );

    let build = release
        .split_once("\n  build:\n")
        .and_then(|(_, after)| after.split_once("\n  dry-summary:\n"))
        .map(|(section, _)| section)
        .expect("build job is missing");
    assert!(build.contains("    needs: preflight"));
    assert!(build.contains("python3 packaging/sign-and-notarize-macos.py \"$TARGET\""));
    // Signing runs only when the Apple secrets were all present at preflight.
    let sign_step = build
        .split("\n      - ")
        .find(|step| step.contains("name: Sign and notarize macOS executable"))
        .expect("signing step is missing");
    assert!(
        sign_step
            .contains("matrix.platform == 'macos' && needs.preflight.outputs.notarize == 'true'"),
        "macOS signing is not conditional on the Apple secrets"
    );
    assert!(build.contains(
        "file \"$RUNNER_TEMP/thruline-cli-$TARGET/thruline\" | grep -Eq 'statically linked|static-pie linked'"
    ));
    assert!(build.contains("shasum -a 256 -c") && build.contains("sha256sum -c"));
    assert!(build.contains("if: ${{ !inputs.dry_run }}\n        uses: actions/upload-artifact@v4"));
    let signing = read("packaging/sign-and-notarize-macos.py");
    for required in [
        "\"--options\", \"runtime\"",
        "\"--timestamp\"",
        "\"--verify\", \"--strict\"",
        "status != \"Accepted\"",
    ] {
        assert!(
            signing.contains(required),
            "macOS release check missing {required}"
        );
    }

    let assemble = release
        .split_once("\n  assemble:\n")
        .and_then(|(_, after)| after.split_once("\n  draft:\n"))
        .map(|(section, _)| section)
        .expect("assemble job is missing");
    assert!(assemble.contains("    if: ${{ !inputs.dry_run }}"));
    assert!(assemble.contains("    needs: [preflight, build]"));
    assert!(
        !assemble.contains("private-sbom") && !assemble.contains("cyclonedx"),
        "the private SBOM reached public payload assembly"
    );
    let staged = assemble
        .find("packaging/public-release/release_assets.py stage")
        .expect("no staging step");
    let verified = assemble
        .find("packaging/public-release/release_assets.py verify --dir release-payload")
        .expect("no staging verification");
    let uploaded = assemble
        .find("actions/upload-artifact@v4")
        .expect("no private payload transfer");
    assert!(
        staged < verified && verified < uploaded,
        "unverified assets can be uploaded"
    );

    let draft = release
        .split_once("\n  draft:\n")
        .map(|(_, section)| section)
        .expect("draft job is missing");
    assert!(draft.contains("    if: ${{ !inputs.dry_run }}"));
    assert!(draft.contains("    needs: [preflight, assemble]"));
    assert!(
        !draft.contains("private-sbom") && !draft.contains("cyclonedx"),
        "the private SBOM reached the public draft job"
    );
    for required in [
        "test \"$CONFIRM_SHA\" = \"$GITHUB_SHA\"",
        "test \"$(git cat-file -t \"refs/tags/$TAG\")\" = tag",
        "test \"$(git rev-parse \"refs/tags/$TAG^{commit}\")\" = \"$GITHUB_SHA\"",
        "Release $TAG appeared after preflight",
        "permissions:\n      contents: write",
        "Not notarized: this draft was built without Apple signing credentials",
    ] {
        assert!(
            draft.contains(required),
            "draft lost final guard: {required}"
        );
    }
    let download = draft
        .find("actions/download-artifact@v4")
        .expect("draft does not fetch staged payload");
    let verify = draft
        .find("packaging/public-release/release_assets.py verify --dir release-payload")
        .expect("draft does not recheck payload");
    let create = draft
        .find("gh release create")
        .expect("draft release is never created");
    assert!(
        download < verify && verify < create,
        "draft can create a release from unchecked assets"
    );
    let running = runnable(&release);
    assert_eq!(
        running.matches("gh release create").count(),
        1,
        "more than one release creation path"
    );
    assert!(!running.contains("gh release edit") && !running.contains("--draft=false"));
    let command = draft[create..]
        .lines()
        .take(6)
        .collect::<Vec<_>>()
        .join("\n");
    for required in [
        "release-payload/*",
        "--repo thrulinehq/thruline",
        "--verify-tag",
        "--draft",
    ] {
        assert!(
            command.contains(required),
            "release creation missing {required}"
        );
    }
    let commands = runnable(&release).to_ascii_lowercase();
    assert!(
        !commands.contains("sigstore") && !commands.contains("cosign"),
        "v0.1.0 release unexpectedly adds Sigstore signing"
    );
}

/// D-326 (a): a binary built with the paid gate off, or with no production key
/// compiled in, must never leave the build job. Every build target runs the
/// binary it is about to upload — taken out of the final archive, after macOS
/// signing — as `licence status --json` in a fresh, empty HOME and
/// THRULINE_HOME, and the job fails unless the document says the gate is on
/// and at least one built-in key is trusted.
///
/// The field names are read from the source, not remembered: the shape id from
/// `json_out.rs`, `paid_gate` and `technical.keys_built_in` from
/// `licence_cmd.rs`'s `status_body`. If either file renames them, this fails
/// before the workflow's check can go quietly vacuous.
#[test]
fn ci_refuses_a_built_binary_with_the_paid_gate_off_or_no_production_key() {
    // The names the check relies on are the names the binary prints.
    let json_out = read("crates/cli/src/json_out.rs");
    assert!(
        json_out.contains(
            "LICENCE_STATUS = (\"thruline.licence.status\", \"licence status\", \"--json\""
        ),
        "the `licence status --json` shape id moved; the release check must follow it"
    );
    let licence = read("crates/cli/src/licence_cmd.rs");
    let body = licence
        .split_once("pub(crate) fn status_body() -> Value {")
        .map(|(_, after)| after)
        .expect("licence_cmd.rs has no status_body");
    for field in [
        "\"paid_gate\": gate,",
        "\"technical\": technical,",
        "\"keys_built_in\": keys,",
        "let keys = trusted_keys().len();",
        "let gate = paid_gate_on();",
    ] {
        assert!(
            body.contains(field),
            "status_body no longer carries {field}; the release check reads it"
        );
    }

    let release = read(".github/workflows/release.yml");
    let build = release
        .split_once("\n  build:\n")
        .and_then(|(_, after)| after.split_once("\n  dry-summary:\n"))
        .map(|(section, _)| section)
        .expect("build job is missing");

    // Every target runs natively on its runner, so the step can run on every
    // target: the musl targets are static binaries on the Linux runner of
    // their own architecture. A cross target would need the step skipped
    // where it cannot execute, and must not be added without revisiting this.
    let matrix: Vec<(String, String)> = build
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- target: "))
        .map(|target| {
            let after = build
                .split_once(&format!("- target: {target}\n"))
                .map(|(_, rest)| rest)
                .expect("matrix entry");
            let arch = after
                .lines()
                .take_while(|l| !l.trim_start().starts_with("- "))
                .find_map(|l| l.trim().strip_prefix("host_arch: "))
                .unwrap_or_else(|| panic!("{target} has no host_arch"));
            (target.to_string(), arch.to_string())
        })
        .collect();
    let mut targets: Vec<&str> = matrix.iter().map(|(t, _)| t.as_str()).collect();
    targets.sort_unstable();
    assert_eq!(
        targets,
        [
            "aarch64-apple-darwin",
            "aarch64-unknown-linux-musl",
            "x86_64-apple-darwin",
            "x86_64-unknown-linux-musl",
        ],
        "the build matrix changed; every target must still run the licence check"
    );
    for (target, arch) in &matrix {
        let native = match target.split('-').next() {
            Some("aarch64") => arch == "arm64" || arch == "aarch64",
            Some("x86_64") => arch == "x86_64",
            _ => false,
        };
        assert!(
            native,
            "{target} is built on a {arch} runner, where its binary cannot run the licence check"
        );
    }
    assert!(
        build.contains("test \"$(uname -m)\" = \"$EXPECTED_ARCH\""),
        "the runner architecture is no longer pinned to the target's"
    );

    let steps: Vec<&str> = build.split("\n      - ").collect();
    let checks: Vec<&&str> = steps
        .iter()
        .filter(|step| runnable(step).contains("licence status --json"))
        .collect();
    assert_eq!(
        checks.len(),
        1,
        "the build job must run the built binary's licence status exactly once"
    );
    let step = checks[0];
    let step_head: Vec<&str> = step
        .lines()
        .take_while(|line| !line.trim_start().starts_with("run:"))
        .collect();
    assert!(
        !step_head
            .iter()
            .any(|line| line.trim_start().starts_with("if:")),
        "the licence check is skipped on some build target"
    );
    assert!(
        !step.contains("continue-on-error") && !step.contains("secrets."),
        "the licence check may not be soft and needs no secret"
    );

    let run = runnable(step);
    for required in [
        "set -euo pipefail",
        "mktemp -d",
        "tar -xzf \"target/distrib/thruline-cli-$TARGET.tar.gz\"",
        "/thruline-cli-$TARGET/thruline\"",
        "HOME=\"$check/home\"",
        "THRULINE_HOME=\"$check/thruline-home\"",
        "licence status --json > \"$check/status.json\"",
        ".shape == \"thruline.licence.status\"",
        ".paid_gate == true",
        ".technical.keys_built_in | type == \"number\" and . >= 1",
        "jq -e \"$filter\" \"$check/status.json\"",
        "exit 1",
    ] {
        assert!(
            run.contains(required),
            "the built-binary licence check lost: {required}"
        );
    }
    // A fresh, empty home: nothing on the runner can lend the binary a licence,
    // and the gate and keys it reports are its own.
    assert!(
        run.contains("mkdir \"$check/home\" \"$check/thruline-home\""),
        "the licence check does not run in freshly made, empty home directories"
    );
    for shrug in ["|| true", "|| :", "; true", "curl", "wget"] {
        assert!(!run.contains(shrug), "the licence check contains `{shrug}`");
    }

    // After the archive is final (built, and on macOS signed and notarized),
    // and before anything is uploaded.
    let built = build.find("dist build ").expect("no archive build");
    let signed = build
        .find("python3 packaging/sign-and-notarize-macos.py")
        .expect("no macOS signing");
    let checked = build.find(*step).expect("check step position");
    let uploaded = build
        .find("uses: actions/upload-artifact@v4")
        .expect("no build upload");
    assert!(
        built < checked && signed < checked && checked < uploaded,
        "the licence check must run on the final archive and before the upload"
    );
}

/// Exercise the stage helper on four tiny, real tar archives. A source search
/// for `SHA256SUMS` alone cannot prove that it covers the final archive bytes
/// or that an extra file is refused. The draft is exactly five assets: the four
/// archives and SHA256SUMS. The licence and the third-party notices are inside
/// each archive, as the source's own bytes, and are not separate assets.
#[test]
fn the_staged_public_payload_is_exactly_five_verified_files() {
    let fixture = r#"
import hashlib, importlib.util, io, shutil, sys, tarfile, tempfile
from pathlib import Path
repo = Path(sys.argv[1])
spec = importlib.util.spec_from_file_location('release_assets', repo / 'packaging/public-release/release_assets.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
expected_targets = (
    'aarch64-apple-darwin', 'x86_64-apple-darwin',
    'aarch64-unknown-linux-musl', 'x86_64-unknown-linux-musl',
)
expected_archives = {f'thruline-cli-{target}.tar.gz' for target in expected_targets}
assert tuple(module.TARGETS) == expected_targets
assert tuple(module.ARCHIVES) == tuple(f'thruline-cli-{target}.tar.gz' for target in expected_targets)
assert module.ASSETS == expected_archives | {'SHA256SUMS'} and len(module.ASSETS) == 5
with tempfile.TemporaryDirectory() as temp:
    root = Path(temp)
    artifacts, source, output = root/'artifacts', root/'source', root/'output'
    artifacts.mkdir()
    source.mkdir()
    approved_licence = (repo/'LICENSE.md').read_bytes()
    (source/'LICENSE.md').write_bytes(approved_licence)
    notices = b'fixture third-party notices\n'
    (source/'THIRD_PARTY_NOTICES.md').write_bytes(notices)
    contents = {'thruline': b'x', 'LICENSE.md': approved_licence, 'THIRD_PARTY_NOTICES.md': notices}
    def write_archive(path, target, names=('thruline', 'LICENSE.md', 'THIRD_PARTY_NOTICES.md'), licence=None):
        with tarfile.open(path, 'w:gz') as archive:
            for file_name in names:
                data = licence if (file_name == 'LICENSE.md' and licence is not None) else contents[file_name]
                member = tarfile.TarInfo(f'thruline-cli-{target}/{file_name}')
                member.size, member.mode = len(data), 0o755 if file_name == 'thruline' else 0o644
                archive.addfile(member, io.BytesIO(data))
    digests = {}
    for target, name in zip(module.TARGETS, module.ARCHIVES):
        path = artifacts/name
        write_archive(path, target)
        digests[name] = hashlib.sha256(path.read_bytes()).hexdigest()
        (artifacts/f'{name}.sha256').write_text(f'{digests[name]}  {name}\n')
    module.stage(artifacts, source, output)
    expected = expected_archives | {'SHA256SUMS'}
    assert len(expected) == 5 and {p.name for p in output.iterdir()} == expected
    assert module.sums(output/'SHA256SUMS') == digests
    assert module.verify(output) == digests
    # The approved licence travels inside every archive, byte for byte.
    for target, name in zip(module.TARGETS, module.ARCHIVES):
        assert module.archived_bytes(output/name, target, 'LICENSE.md') == approved_licence
        assert hashlib.sha256(module.archived_bytes(output/name, target, 'LICENSE.md')).hexdigest() == 'e81e5c0d1a2efe7549c8a5302854a1c1937e625997586651b9def451261b29de'
        assert module.archived_bytes(output/name, target, 'THIRD_PARTY_NOTICES.md') == notices
    # Either file missing is refused: from the source the payload is staged
    # from, and from inside an archive.
    for missing in ('LICENSE.md', 'THIRD_PARTY_NOTICES.md'):
        with tempfile.TemporaryDirectory() as bare:
            without = Path(bare)/'source'
            shutil.copytree(source, without)
            (without/missing).unlink()
            try: module.stage(artifacts, without, Path(bare)/'out')
            except SystemExit: pass
            else: raise AssertionError(f'staging ran without {missing} in the source')
        names = tuple(n for n in ('thruline', 'LICENSE.md', 'THIRD_PARTY_NOTICES.md') if n != missing)
        thin = root/'thin.tar.gz'
        write_archive(thin, module.TARGETS[0], names)
        try: module.verify_archive(thin, module.TARGETS[0])
        except SystemExit: pass
        else: raise AssertionError(f'an archive without {missing} was accepted')
        thin.unlink()
    # A licence inside an archive that is not the source's is refused at staging.
    other = root/'other'
    shutil.copytree(artifacts, other)
    write_archive(other/module.ARCHIVES[0], module.TARGETS[0], licence=b'some other licence')
    (other/f'{module.ARCHIVES[0]}.sha256').write_text(f'{hashlib.sha256((other/module.ARCHIVES[0]).read_bytes()).hexdigest()}  {module.ARCHIVES[0]}\n')
    try: module.stage(other, source, root/'other-out')
    except SystemExit: pass
    else: raise AssertionError('an archive carrying a different LICENSE.md was staged')
    # Nothing but the five names is an asset: the licence, the notices and a
    # README are not separate draft assets, and neither is anything else.
    for extra in ('LICENSE.md', 'THIRD_PARTY_NOTICES.md', 'README.md', 'unexpected'):
        (output/extra).write_text('extra')
        try: module.verify(output)
        except SystemExit: pass
        else: raise AssertionError(f'extra public asset {extra} was accepted')
        (output/extra).unlink()
    (output/'SHA256SUMS').write_text((output/'SHA256SUMS').read_text().splitlines()[0] + '\n')
    try: module.verify(output)
    except SystemExit: pass
    else: raise AssertionError('checksum file omitted three archives without refusal')
"#;
    let output = Command::new("python3")
        .arg("-c")
        .arg(fixture)
        .arg(root())
        .output()
        .expect("python3 is required by the release workflow");
    assert!(
        output.status.success(),
        "five-asset stage contract failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// No install path resolves a version that can change without a diff.
#[test]
fn nothing_installs_a_floating_version() {
    // `install.sh` is the one exception and it is a deliberate, narrow one: a
    // human running `curl | sh` with no version in mind gets the latest
    // release, and the script *prints* which one it resolved and how to pin it.
    // Every automated path must be pinned.
    for path in [
        "packaging/action/install.sh",
        "packaging/npm/install.js",
        "packaging/homebrew/Formula/thruline.rb",
    ] {
        let s = read(path);
        for floating in ["releases/latest", ":-latest", "=latest", "'latest'"] {
            assert!(
                !s.contains(floating),
                "{path} resolves a floating version via `{floating}`"
            );
        }
    }
    let curl = read("packaging/install.sh");
    assert!(
        curl.contains("THRULINE_VERSION") && curl.contains("set THRULINE_VERSION"),
        "install.sh does not tell the reader how to pin a version"
    );
}
