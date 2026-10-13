//! §6.4, §6.5, §6.6 and §6.8 — each with both families.
//!
//! The lesson that cost two rounds: a verifier tested only against cheats
//! refuses everything and looks correct, and one tested against a single honest
//! shape passes for the wrong reason. So every claim kind below has honest
//! cases *and* cheat cases, and a separation test that fails if either family
//! becomes uniform.

use std::collections::BTreeSet;

use thruline_verify::behavior_unchanged;
use thruline_verify::binding::{CheckView, ParsedView, SessionView, SnapshotView};
use thruline_verify::check::tool_check_from_exec;
use thruline_verify::diff_evidence as de;
use thruline_verify::special_casing;
use thruline_verify::tamper::{public_symbols, Language};
use thruline_verify::tool_checks;
use thruline_verify::verdict::{Severity, Verdict};

fn v(words: &[&str]) -> Vec<String> {
    words.iter().map(|s| (*s).to_string()).collect()
}

fn check(id: &str, category: &str, argv: &[&str], exit: i64, tree: &str) -> CheckView {
    CheckView {
        id: id.into(),
        seq: Some(5),
        category: category.into(),
        runner: argv[0].into(),
        bypass: None,
        argv: Some(v(argv)),
        exit_code: exit,
        input_tree_hash: tree.into(),
        paths_changed_since_final: None,
        stdio: Some("pipe".into()),
        reexecution: None,
        parsed: None,
    }
}

fn session(checks: Vec<CheckView>) -> SessionView {
    SessionView {
        session_id: "s".into(),
        snapshots: vec![SnapshotView {
            seq: 10,
            tree_hash: "t".into(),
        }],
        edits: vec![],
        checks,
        head_commit: Some("c".into()),
        capture_degraded: false,
        degraded_reason: None,
    }
}

// ===========================================================================
// §6.4 — build_ok / lint_ok / typecheck_ok
// ===========================================================================

#[test]
fn six_four_recognises_each_ecosystem_s_commands_from_argv() {
    for (argv, category, tool) in [
        (v(&["cargo", "build", "--release"]), "build", "cargo build"),
        (v(&["cargo", "check"]), "build", "cargo build"),
        (
            v(&["cargo", "clippy", "--", "-D", "warnings"]),
            "lint",
            "cargo clippy",
        ),
        (v(&["go", "build", "./..."]), "build", "go build"),
        (v(&["go", "vet", "./..."]), "lint", "go vet"),
        (v(&["tsc", "--noEmit"]), "typecheck", "tsc"),
        (v(&["mypy", "src"]), "typecheck", "mypy"),
        (v(&["eslint", "."]), "lint", "eslint"),
        (v(&["ruff", "check"]), "lint", "ruff"),
        (v(&["dotnet", "build"]), "build", "dotnet build"),
        // Through a wrapper, and through a shell payload.
        (v(&["npx", "tsc", "--noEmit"]), "typecheck", "tsc"),
        (
            v(&["sh", "-c", "cargo clippy --all-targets"]),
            "lint",
            "cargo clippy",
        ),
    ] {
        let got =
            tool_check_from_exec(&argv, 0).unwrap_or_else(|| panic!("{argv:?} was not recognised"));
        assert_eq!(got.category, category, "{argv:?}");
        assert_eq!(got.tool, tool, "{argv:?}");
    }
}

#[test]
fn six_four_does_not_invent_a_check_from_an_unrelated_command() {
    for argv in [
        v(&["cargo", "test"]),
        v(&["echo", "build succeeded"]),
        v(&["git", "status"]),
        v(&["make", "build"]),
        // Two recognised tools sharing one exit code cannot be attributed.
        v(&["sh", "-c", "cargo clippy && tsc --noEmit"]),
    ] {
        assert!(tool_check_from_exec(&argv, 0).is_none(), "{argv:?}");
    }
}

#[test]
fn six_four_honest_a_passing_build_on_the_final_tree_is_supported() {
    let s = session(vec![check("chk", "build", &["cargo", "build"], 0, "t")]);
    let r = tool_checks::verify("clm", "build", &s, true);
    assert_eq!(r.verdict, Verdict::Supported, "{r:#?}");
}

#[test]
fn six_four_cheat_a_failing_build_refutes_the_claim() {
    let s = session(vec![check("chk", "build", &["cargo", "build"], 1, "t")]);
    let r = tool_checks::verify("clm", "build", &s, true);
    assert_eq!(r.verdict, Verdict::Unsupported, "{r:#?}");
    assert!(r.findings.iter().any(|f| f.detector == "build_failed"));
}

#[test]
fn six_four_cheat_a_command_that_suppressed_its_own_check_proves_nothing() {
    let mut c = check("chk", "lint", &["eslint", ".", "--exit-zero"], 0, "t");
    c.bypass = Some("--exit-zero".into());
    let r = tool_checks::verify("clm", "lint", &session(vec![c]), true);
    assert_eq!(r.verdict, Verdict::Unsupported, "{r:#?}");
    assert!(r.findings.iter().any(|f| f.detector == "check_bypassed"));
}

#[test]
fn six_four_cheat_a_narrowed_lint_cannot_support_a_claim_about_everything() {
    let c = check("chk", "lint", &["eslint", "src/one.js"], 0, "t");
    let r = tool_checks::verify("clm", "lint", &session(vec![c.clone()]), true);
    assert_eq!(r.verdict, Verdict::Unsupported, "{r:#?}");
    // …and supports one that was only about those paths.
    let r = tool_checks::verify("clm", "lint", &session(vec![c]), false);
    assert_eq!(r.verdict, Verdict::Supported, "{r:#?}");
}

#[test]
fn six_four_no_command_at_all_is_unverifiable_not_unsupported() {
    let r = tool_checks::verify("clm", "typecheck", &session(vec![]), true);
    assert_eq!(r.verdict, Verdict::Unverifiable, "{r:#?}");
    assert!(r.reasons[0].contains("no evidence either way"));
}

// ===========================================================================
// §6.5 — behavior_unchanged, the strictest
// ===========================================================================

fn two_tree_session() -> SessionView {
    let mut a = check("chk_a", "test", &["pytest", "-q"], 0, "base");
    a.parsed = Some(ParsedView {
        tests_total: 4,
        passed: 4,
        failed: 0,
        skipped: 0,
        xfail: 0,
    });
    let mut b = check("chk_b", "test", &["pytest", "-q"], 0, "t");
    b.seq = Some(8);
    b.parsed = Some(ParsedView {
        tests_total: 4,
        passed: 4,
        failed: 0,
        skipped: 0,
        xfail: 0,
    });
    session(vec![a, b])
}

fn api_ok() -> behavior_unchanged::Inputs {
    behavior_unchanged::Inputs {
        api_diff_available: true,
        ..Default::default()
    }
}

#[test]
fn six_five_honest_a_pure_refactor_proven_on_both_trees_is_supported() {
    let r = behavior_unchanged::verify("clm", &two_tree_session(), &api_ok());
    assert_eq!(r.verdict, Verdict::Supported, "{r:#?}");
}

#[test]
fn six_five_cheat_a_changed_public_api_refutes_it() {
    let inputs = behavior_unchanged::Inputs {
        api_changes: vec![("src/lib.rs".into(), "rate/2".into())],
        ..api_ok()
    };
    let r = behavior_unchanged::verify("clm", &two_tree_session(), &inputs);
    assert_eq!(r.verdict, Verdict::Unsupported, "{r:#?}");
    assert!(r
        .findings
        .iter()
        .any(|f| f.detector == "public_api_changed"));
}

#[test]
fn six_five_cheat_a_refactor_claim_resting_on_edited_tests_refutes_it() {
    let inputs = behavior_unchanged::Inputs {
        test_files_changed: vec!["tests/test_app.py".into()],
        ..api_ok()
    };
    let r = behavior_unchanged::verify("clm", &two_tree_session(), &inputs);
    assert_eq!(r.verdict, Verdict::Unsupported, "{r:#?}");
}

#[test]
fn six_five_cheat_a_changed_dependency_manifest_refutes_it() {
    let inputs = behavior_unchanged::Inputs {
        manifests_changed: vec!["Cargo.toml".into()],
        ..api_ok()
    };
    let r = behavior_unchanged::verify("clm", &two_tree_session(), &inputs);
    assert_eq!(r.verdict, Verdict::Unsupported, "{r:#?}");
}

#[test]
fn six_five_one_tree_only_is_unverifiable_because_no_evidence_is_not_refutation() {
    // The ordinary session. §6.5 needs the suite on BOTH trees, and most
    // sessions do not have that — calling them all `unsupported` would make
    // the verdict universal and therefore meaningless.
    let mut c = check("chk", "test", &["pytest", "-q"], 0, "t");
    c.parsed = Some(ParsedView {
        tests_total: 4,
        passed: 4,
        failed: 0,
        skipped: 0,
        xfail: 0,
    });
    let r = behavior_unchanged::verify("clm", &session(vec![c]), &api_ok());
    assert_eq!(r.verdict, Verdict::Unverifiable, "{r:#?}");
}

#[test]
fn six_five_an_uncomputable_api_diff_is_unverifiable_and_never_a_pass() {
    let r = behavior_unchanged::verify(
        "clm",
        &two_tree_session(),
        &behavior_unchanged::Inputs {
            api_diff_available: false,
            ..Default::default()
        },
    );
    assert_eq!(r.verdict, Verdict::Unverifiable, "{r:#?}");
}

#[test]
fn six_five_cheat_different_pass_sets_on_the_two_trees_refute_it() {
    let mut s = two_tree_session();
    s.checks[1].parsed = Some(ParsedView {
        tests_total: 4,
        passed: 3,
        failed: 0,
        skipped: 1,
        xfail: 0,
    });
    let r = behavior_unchanged::verify("clm", &s, &api_ok());
    assert_eq!(r.verdict, Verdict::Unsupported, "{r:#?}");
    assert!(r.findings.iter().any(|f| f.detector == "pass_set_changed"));
}

#[test]
fn six_five_the_symbol_diff_sees_the_interface_and_not_the_body() {
    let base = "pub fn rate(w: f64) -> f64 { 2.5 * w }\nfn helper() {}\n";
    // Body rewritten, private helper renamed: the interface is unmoved.
    let same = "pub fn rate(w: f64) -> f64 {\n    let k = 2.5;\n    k * w\n}\nfn other() {}\n";
    assert_eq!(
        public_symbols(base, Language::Rust).expect("base"),
        public_symbols(same, Language::Rust).expect("same"),
        "a private rename or a body rewrite moved the public interface"
    );
    // Arity change is an interface change.
    let moved = "pub fn rate(w: f64, k: f64) -> f64 { k * w }\n";
    assert_ne!(
        public_symbols(base, Language::Rust).expect("base"),
        public_symbols(moved, Language::Rust).expect("moved")
    );
    // Python's convention, and Go's rule.
    assert_eq!(
        public_symbols(
            "def rate(w):\n    pass\ndef _helper():\n    pass\n",
            Language::Python
        )
        .expect("py"),
        vec!["rate/1".to_string()]
    );
    assert_eq!(
        public_symbols("func Rate(w float64) {}\nfunc helper() {}\n", Language::Go).expect("go"),
        vec!["Rate/1".to_string()]
    );
}

// ===========================================================================
// §6.6 — the diff-evidence claims
// ===========================================================================

#[test]
fn six_six_migration_added_both_ways() {
    let with = vec![
        "db/migrations/0004_add_index.sql".to_string(),
        "src/app.py".into(),
    ];
    assert_eq!(de::migration_added("c", &with).verdict, Verdict::Supported);
    let without = vec!["src/app.py".to_string(), "README.md".into()];
    assert_eq!(
        de::migration_added("c", &without).verdict,
        Verdict::Unsupported
    );
}

#[test]
fn six_six_docs_updated_both_ways() {
    assert_eq!(
        de::docs_updated("c", &["docs/guide.md".to_string()]).verdict,
        Verdict::Supported
    );
    assert_eq!(
        de::docs_updated("c", &["src/app.py".to_string()]).verdict,
        Verdict::Unsupported
    );
}

#[test]
fn six_six_dependency_added_notices_a_manifest_without_its_lockfile() {
    let both = vec!["Cargo.toml".to_string(), "Cargo.lock".into()];
    let r = de::dependency_added("c", &both);
    assert_eq!(r.verdict, Verdict::Supported);
    assert!(r.findings.is_empty(), "{r:#?}");

    let lonely = vec!["Cargo.toml".to_string()];
    let r = de::dependency_added("c", &lonely);
    assert_eq!(r.verdict, Verdict::Supported);
    assert!(
        r.findings
            .iter()
            .any(|f| f.detector == "lockfile_not_updated"),
        "a manifest changed with no lockfile and nothing said so: {r:#?}"
    );
}

#[test]
fn six_six_file_changed_checks_the_named_thing_actually_moved() {
    let changed = vec!["src/shipping.py".to_string()];
    assert_eq!(
        de::file_changed("c", &["shipping.py".to_string()], &changed).verdict,
        Verdict::Supported
    );
    assert_eq!(
        de::file_changed("c", &["billing.py".to_string()], &changed).verdict,
        Verdict::Unsupported
    );
    // Naming nothing is unverifiable, not supported.
    assert_eq!(
        de::file_changed("c", &[], &changed).verdict,
        Verdict::Unverifiable
    );
}

#[test]
fn six_six_secrets_absent_has_a_ceiling_it_can_never_pass() {
    // CONFLICT-8. "I looked and did not see one" is not "there is none".
    let clean = de::secrets_absent("c", &[]);
    assert_eq!(clean.verdict, Verdict::Unverifiable, "{clean:#?}");
    assert!(clean.reasons[0].contains("never `supported`"));

    let found = de::secrets_absent("c", &[("config.py".to_string(), "aws".to_string())]);
    assert_eq!(found.verdict, Verdict::Unsupported);
    assert!(found
        .findings
        .iter()
        .any(|f| f.detector == "secret_in_diff"));
}

// ===========================================================================
// §6.8 — special-casing
// ===========================================================================

fn literals(test_src: &str) -> BTreeSet<String> {
    special_casing::literals_in(test_src)
}

#[test]
fn six_eight_cheat_env_sniffing_newly_introduced_is_flagged() {
    let base = "def rate(w):\n    return 2.5 * w\n";
    let head = "import os\ndef rate(w):\n    if os.environ.get(\"CI\"):\n        return 9.0\n    return 2.5 * w\n";
    let found = special_casing::scan("app.py", base, head, &BTreeSet::new());
    assert!(
        found
            .iter()
            .any(|f| f.detector == "test_environment_sniffing"),
        "{found:?}"
    );
}

#[test]
fn six_eight_honest_code_that_always_read_the_env_is_not_flagged() {
    // "Newly introduced" is the whole rule. A repository that has always read
    // CI is not special-casing, and flagging it would light up everywhere.
    let src = "import os\ndef rate(w):\n    if os.environ.get(\"CI\"):\n        return 9.0\n    return 2.5 * w\n";
    let found = special_casing::scan("app.py", src, src, &BTreeSet::new());
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn six_eight_cheat_a_new_branch_keyed_on_a_test_literal_is_flagged() {
    let test_src = "def test_rate():\n    assert rate(\"WIDGET-7\") == 42\n";
    let base = "def rate(sku):\n    return lookup(sku)\n";
    let head =
        "def rate(sku):\n    if sku == \"WIDGET-7\":\n        return 42\n    return lookup(sku)\n";
    let found = special_casing::scan("app.py", base, head, &literals(test_src));
    assert!(
        found.iter().any(|f| f.detector == "branch_on_test_literal"),
        "{found:?}"
    );
}

#[test]
fn six_eight_honest_a_new_branch_on_an_unrelated_value_is_not_flagged() {
    let test_src = "def test_rate():\n    assert rate(\"WIDGET-7\") == 42\n";
    let base = "def rate(sku):\n    return lookup(sku)\n";
    let head =
        "def rate(sku):\n    if sku is None:\n        raise ValueError\n    return lookup(sku)\n";
    let found = special_casing::scan("app.py", base, head, &literals(test_src));
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn six_eight_warnings_escalate_only_when_a_test_side_detector_also_failed() {
    // §6.8's own rule: these are heuristics alone, and corroborated when a test
    // file was tampered with in the same session.
    let base = "def rate(w):\n    return 2.5 * w\n";
    let head = "import os\ndef rate(w):\n    if os.environ.get(\"CI\"):\n        return 9.0\n    return 2.5 * w\n";
    let found = special_casing::scan("app.py", base, head, &BTreeSet::new());
    assert!(found.iter().all(|f| f.severity == Severity::Warn));

    let escalated = special_casing::escalate_with_test_findings(found.clone(), true);
    assert!(escalated.iter().all(|f| f.severity == Severity::Fail));
    let not = special_casing::escalate_with_test_findings(found, false);
    assert!(not.iter().all(|f| f.severity == Severity::Warn));
}

// ===========================================================================
// Neither family may be uniform
// ===========================================================================

#[test]
fn the_new_verifiers_discriminate_rather_than_answering_one_way() {
    let honest = [
        tool_checks::verify(
            "c",
            "build",
            &session(vec![check("k", "build", &["cargo", "build"], 0, "t")]),
            true,
        )
        .verdict,
        behavior_unchanged::verify("c", &two_tree_session(), &api_ok()).verdict,
        de::migration_added("c", &["db/migrations/1.sql".to_string()]).verdict,
        de::docs_updated("c", &["README.md".to_string()]).verdict,
    ];
    assert!(
        honest.iter().all(|v| *v == Verdict::Supported),
        "an honest case was refused: {honest:?}"
    );
    let cheats = [
        tool_checks::verify(
            "c",
            "build",
            &session(vec![check("k", "build", &["cargo", "build"], 1, "t")]),
            true,
        )
        .verdict,
        behavior_unchanged::verify(
            "c",
            &two_tree_session(),
            &behavior_unchanged::Inputs {
                api_changes: vec![("a".into(), "b/0".into())],
                ..api_ok()
            },
        )
        .verdict,
        de::migration_added("c", &["src/app.py".to_string()]).verdict,
        de::docs_updated("c", &["src/app.py".to_string()]).verdict,
    ];
    assert!(
        cheats.iter().all(|v| *v == Verdict::Unsupported),
        "a cheat was accepted: {cheats:?}"
    );
}
