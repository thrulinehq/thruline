//! The licence is the official FSL-1.1-MIT text (fsl.software), and it travels
//! with the source and the binary as `LICENSE.md`. First-party metadata must
//! name it, and must not offer a Cargo publish.

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

const LICENCE: &str = "FSL-1.1-MIT";
// The official template (fsl.software, FSL-1.1-MIT.template.md, sha256
// 0905af4c021cb64aff11a80aae7f25f0d7bafc69c14e97992273873d175a718b) with
// exactly one line changed: `Copyright ${year} ${licensor name}` became
// `Copyright 2026 RAAS Labs, LLC`. This pin is that file's digest. The earlier
// proprietary text (d0d568f1…, and 0b89ef30… in v0.1.0) lives in git history.
const APPROVED_SHA256: &str = "e81e5c0d1a2efe7549c8a5302854a1c1937e625997586651b9def451261b29de";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .to_path_buf()
}

fn read(path: &str) -> String {
    std::fs::read_to_string(root().join(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn manifest(path: &str) -> toml::Value {
    toml::from_str(&read(path)).unwrap_or_else(|e| panic!("{path}: {e}"))
}

#[test]
fn root_licence_is_the_official_fsl_text_naming_the_licensor() {
    let bytes = std::fs::read(root().join("LICENSE.md")).expect("root LICENSE.md");
    assert_eq!(
        thruline_licence::to_hex(&Sha256::digest(&bytes)),
        APPROVED_SHA256
    );
    let text = std::str::from_utf8(&bytes).expect("UTF-8 licence");
    assert!(text.starts_with("# Functional Source License, Version 1.1, MIT Future License\n"));
    assert!(text.contains("\nCopyright 2026 RAAS Labs, LLC\n"));
    // The template placeholders must all have been filled.
    assert!(!text.contains("${"), "an unfilled FSL template placeholder");
    // The old proprietary file is gone and must not come back beside the new one.
    assert!(
        !root().join("LICENSE").exists(),
        "a root LICENSE (the retired proprietary text) is back beside LICENSE.md"
    );
}

#[test]
fn third_party_notices_exist_and_cargo_dist_puts_them_in_every_archive() {
    let notices = std::fs::read(root().join("THIRD_PARTY_NOTICES.md")).expect(
        "root THIRD_PARTY_NOTICES.md: every archive, dmg and release carries it beside LICENSE.md",
    );
    assert!(!notices.is_empty(), "THIRD_PARTY_NOTICES.md is empty");
    // cargo-dist adds README*, LICENSE* and CHANGELOG* to an archive by itself;
    // the notices file is not among them, so the config must name it.
    let dist = manifest("dist-workspace.toml");
    let include = dist["dist"]["include"]
        .as_array()
        .expect("dist-workspace.toml `include`");
    assert!(
        include
            .iter()
            .any(|value| value.as_str() == Some("THIRD_PARTY_NOTICES.md")),
        "cargo-dist would build archives without the third-party notices"
    );
}

#[test]
fn every_first_party_crate_names_the_licence_and_is_not_publishable() {
    let workspace = manifest("Cargo.toml");
    let defaults = &workspace["workspace"]["package"];
    assert_eq!(defaults["license"].as_str(), Some(LICENCE));
    assert_eq!(defaults["publish"].as_bool(), Some(false));

    let mut paths: Vec<String> = std::fs::read_dir(root().join("crates"))
        .expect("crate directory")
        .map(|entry| entry.expect("crate entry").path().join("Cargo.toml"))
        .filter(|path| path.is_file())
        .map(|path| {
            path.strip_prefix(root())
                .expect("first-party manifest")
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    paths.sort();
    assert!(paths.len() >= 18, "first-party census is incomplete");
    for path in paths {
        let doc = manifest(&path);
        let package = &doc["package"];
        let licence = package.get("license").expect("package licence");
        let effective_licence =
            if licence.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
                defaults["license"].as_str()
            } else {
                licence.as_str()
            };
        assert_eq!(effective_licence, Some(LICENCE), "{path}");
        let publish = package.get("publish").expect("package publish restriction");
        let effective_publish =
            if publish.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
                defaults["publish"].as_bool()
            } else {
                publish.as_bool()
            };
        assert_eq!(effective_publish, Some(false), "{path}");
    }
    // cargo-dist otherwise excludes a non-publishable CLI from the release.
    let cli = manifest("crates/cli/Cargo.toml");
    assert_eq!(
        cli["package"]["metadata"]["dist"]["dist"].as_bool(),
        Some(true)
    );
}

#[test]
fn auxiliary_first_party_manifests_name_no_other_licence() {
    // The one auxiliary first-party manifest in the public tree.
    let path = "fuzz/Cargo.toml";
    let doc = manifest(path);
    assert_eq!(doc["package"]["publish"].as_bool(), Some(false), "{path}");
    if let Some(licence) = doc["package"].get("license") {
        assert_eq!(licence.as_str(), Some(LICENCE), "{path}");
    }
}

#[test]
fn cargo_deny_ignores_private_crates_without_allowing_the_first_party_licence() {
    let deny = manifest("deny.toml");
    assert_eq!(deny["licenses"]["private"]["ignore"].as_bool(), Some(true));
    let allow = deny["licenses"]["allow"]
        .as_array()
        .expect("dependency allow list");
    assert!(!allow.iter().any(|value| value.as_str() == Some(LICENCE)));
}

#[test]
fn npm_metadata_names_the_licence() {
    let package: serde_json::Value =
        serde_json::from_str(&read("packaging/npm/package.json")).expect("npm metadata");
    assert_eq!(package["license"].as_str(), Some(LICENCE));
}

#[test]
fn public_readme_names_the_licence_the_pro_requirement_and_third_party_notices() {
    let readme = read("packaging/public-release/README.md");
    let section = "## Licence\n\nThruLine is licensed under the Functional Source License, Version 1.1, MIT Future License (FSL-1.1-MIT). Copyright 2026 RAAS Labs, LLC. The terms are in `LICENSE.md`; each version converts to the MIT license on the second anniversary of the date it is first made available. ThruLine Pro features, as distributed, require a paid licence from RAAS Labs, LLC. The open-source components ThruLine includes are listed, with their licences, in `THIRD_PARTY_NOTICES.md`.\n";
    let licence = readme
        .find(section)
        .expect("approved public licence paragraph");
    let last_paragraph = readme
        .rfind("The release contains the four archives")
        .expect("public asset explanation");
    assert!(licence < last_paragraph);
}

#[test]
fn rendered_homebrew_formula_keeps_the_fsl_licence() {
    let fixture = r#"
import importlib.util, sys
from pathlib import Path
repo = Path(sys.argv[1])
sys.path.insert(0, str(repo/'packaging/public-release'))
spec = importlib.util.spec_from_file_location('open_tap_pr', repo/'packaging/public-release/open_tap_pr.py')
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
checksums = {name: f'{i:064x}' for i, name in enumerate(module.ARCHIVES, start=1)}
formula = module.render_formula('v0.1.0', checksums)
assert [line.strip() for line in formula.splitlines() if line.strip().startswith('license ')] == ['license "FSL-1.1-MIT"']
"#;
    let output = Command::new("python3")
        .args(["-c", fixture])
        .arg(root())
        .output()
        .expect("Python is required by release staging");
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
