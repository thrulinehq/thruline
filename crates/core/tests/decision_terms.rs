use std::path::{Path, PathBuf};
use std::process::Command;

use thruline_core::decisions::checks::VerdictState;
use thruline_core::decisions::checks_terms::{
    check_forbidden_words, check_symbol_absent, check_term_everywhere, check_ui_copy_absent,
};
use thruline_core::decisions::params::{
    ForbiddenWordsParams, Surface, SymbolAbsentParams, TermEverywhereParams, UiCopyAbsentParams,
};

fn fixtures(kind: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/decisions")
        .join(kind)
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create destination");
    for entry in std::fs::read_dir(from).expect("read fixture") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy file");
        }
    }
}

fn repo_for(kind: &str, state: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp repo");
    copy_tree(&fixtures(kind).join(state), dir.path());
    for args in [
        vec!["init", "-q"],
        vec!["add", "-A"],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    ] {
        assert!(Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .expect("git")
            .success());
    }
    dir
}

#[test]
fn symbol_absent_distinguishes_code_tokens_from_string_mentions() {
    let params = SymbolAbsentParams {
        symbol: "LEGACY_FLAG".into(),
        language: None,
        surfaces: vec![Surface::Code],
        allowlist: Vec::new(),
    };
    let holds = repo_for("symbol_absent", "holds");
    let held = check_symbol_absent(holds.path(), "HEAD", &params);
    assert_eq!(held.state, VerdictState::Holds);
    assert!(held.findings.is_empty());

    let broken = repo_for("symbol_absent", "broken");
    let failed = check_symbol_absent(broken.path(), "HEAD", &params);
    assert_eq!(failed.state, VerdictState::Broken);
    assert_eq!(failed.findings.len(), 3);
    assert!(failed
        .findings
        .iter()
        .any(|f| f.path == "src/lib.rs" && f.line == 1));
    assert!(failed
        .findings
        .iter()
        .any(|f| f.path == "src/lib.rs" && f.line == 2));
    assert!(failed
        .findings
        .iter()
        .any(|f| f.path == "ui/app.js" && f.line == 1));
    assert!(!failed
        .findings
        .iter()
        .any(|f| f.path == "src/lib.rs" && f.line == 3));
    assert_eq!(failed.tree_hash.as_deref().map(str::len), Some(40));
    assert_eq!(
        check_symbol_absent(broken.path(), "not-a-version", &params).state,
        VerdictState::Unverifiable
    );
}

#[test]
fn route_and_flag_symbols_count_only_when_declared_in_code() {
    let holds = repo_for("symbol_absent", "holds");
    let broken = repo_for("symbol_absent", "broken");
    for (symbol, line) in [("/api/sync", 1), ("--force", 2)] {
        let params = SymbolAbsentParams {
            symbol: symbol.into(),
            language: Some("javascript".into()),
            surfaces: vec![Surface::Code],
            allowlist: Vec::new(),
        };
        assert_eq!(
            check_symbol_absent(holds.path(), "HEAD", &params).state,
            VerdictState::Holds
        );
        let failed = check_symbol_absent(broken.path(), "HEAD", &params);
        assert_eq!(failed.state, VerdictState::Broken);
        assert!(failed
            .findings
            .iter()
            .any(|f| f.path == "ui/router.js" && f.line == line));
    }
}

#[test]
fn ui_copy_absent_only_reads_visible_copy_at_the_named_version() {
    let params = UiCopyAbsentParams {
        terms: vec!["preview".into()],
        allowlist: vec!["ui/src/Legacy.jsx".into()],
    };
    let holds = repo_for("ui_copy_absent", "holds");
    assert_eq!(
        check_ui_copy_absent(holds.path(), "HEAD", &params).state,
        VerdictState::Holds
    );

    let broken = repo_for("ui_copy_absent", "broken");
    let before = check_ui_copy_absent(broken.path(), "HEAD", &params);
    assert_eq!(before.state, VerdictState::Broken);
    assert_eq!(before.findings.len(), 3);
    assert!(before
        .findings
        .iter()
        .any(|f| f.path == "ui/src/App.jsx" && f.line == 2));
    assert!(before
        .findings
        .iter()
        .any(|f| f.path == "ui/public/copy.json"));
    assert!(before
        .findings
        .iter()
        .any(|f| f.path == "ui/dist/assets/app.js"));
    assert!(!before
        .findings
        .iter()
        .any(|f| f.path == "ui/src/Legacy.jsx"));
    std::fs::write(
        broken.path().join("ui/src/App.jsx"),
        "export const App = () => <span>Ready</span>\n",
    )
    .expect("working tree edit");
    assert_eq!(check_ui_copy_absent(broken.path(), "HEAD", &params), before);
}

#[test]
fn term_everywhere_requires_canonical_copy_on_selected_surfaces() {
    let params = TermEverywhereParams {
        canonical: "ThruLine HQ".into(),
        forbidden_variants: vec!["Thru Line".into()],
        surfaces: vec![Surface::UiCopy, Surface::Docs],
        allowlist: Vec::new(),
    };
    let holds = repo_for("term_everywhere", "holds");
    let held = check_term_everywhere(holds.path(), "HEAD", &params);
    assert_eq!(held.state, VerdictState::Holds);
    let broken = repo_for("term_everywhere", "broken");
    let failed = check_term_everywhere(broken.path(), "HEAD", &params);
    assert_eq!(failed.state, VerdictState::Broken);
    assert_eq!(failed.findings.len(), 2);
    assert!(failed
        .surfaces
        .iter()
        .any(|s| s.surface == Surface::UiCopy && s.finding_count == 1));
    assert!(failed
        .surfaces
        .iter()
        .any(|s| s.surface == Surface::Docs && s.finding_count == 1));
}

#[test]
fn forbidden_words_checks_output_copy_not_code_comments() {
    let params = ForbiddenWordsParams {
        words: vec!["outdated".into()],
        surfaces: vec![Surface::UiCopy],
        allowlist: Vec::new(),
    };
    let holds = repo_for("forbidden_words", "holds");
    assert_eq!(
        check_forbidden_words(holds.path(), "HEAD", &params).state,
        VerdictState::Holds
    );
    let broken = repo_for("forbidden_words", "broken");
    let failed = check_forbidden_words(broken.path(), "HEAD", &params);
    assert_eq!(failed.state, VerdictState::Broken);
    assert_eq!(failed.findings.len(), 2);
    assert!(failed
        .findings
        .iter()
        .all(|f| f.path == "ui/src/App.jsx" && f.line == 2));
}

#[test]
fn unsupported_case_folding_cannot_read_as_a_pass() {
    let repo = repo_for("ui_copy_absent", "holds");
    let params = UiCopyAbsentParams {
        terms: vec!["Café".into()],
        allowlist: Vec::new(),
    };
    let outcome = check_ui_copy_absent(repo.path(), "HEAD", &params);
    assert_eq!(outcome.state, VerdictState::Unverifiable);
    assert!(outcome.reason.is_some());
}
