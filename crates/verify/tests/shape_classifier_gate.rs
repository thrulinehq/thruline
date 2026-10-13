//! The gate on §6.7's shape classifier.
//!
//! The classifier reads an assertion's *shape* from its own source text. That
//! was measured across **15,835 real assertions in 833 files**: 13 ambiguous
//! cases, 0.082% — Python 1/9,429, Rust 5/5,368, JS/TS 7/1,038, **Go
//! unmeasured, no corpus on this machine**. Every one is a message string, e.g.
//! `assert!(…, "git {args:?} in {cwd:?}")` reading as a containment because of
//! the word `in` inside the *message*.
//!
//! That was tolerable while the finding was warn-only. It is not tolerable now
//! that a weakening to `Truthy` is a `fail`, so string literals are stripped
//! before the shape is read — and this file is the **gate**, not a note: it
//! fails if the stripping is ever removed while a shape-derived finding can
//! still reach `fail`.

use thruline_verify::binding::{CheckView, ParsedView, SessionView, SnapshotView};
use thruline_verify::tamper::{
    diff_test_file, strength_of, strip_string_literals, Language, Strength,
};
use thruline_verify::tests_pass::{verify, Inputs};
use thruline_verify::verdict::Severity;

#[test]
fn the_measured_ambiguous_shape_is_reclassified_by_stripping() {
    // The verbatim example from the measurement. Without stripping, the `in`
    // in the message makes this a Containment; a preceding `assert_eq!` would
    // then read as a weakening that is not one.
    let ambiguous = r#"assert!(out.status.success(), "git {args:?} in {cwd:?} failed")"#;
    assert_eq!(
        strength_of(ambiguous),
        Strength::Truthy,
        "a message string still decides the shape"
    );

    // And the shapes that are real are unaffected.
    assert_eq!(
        strength_of("assert_eq!(a, b, \"in the parser\")"),
        Strength::Equality
    );
    assert_eq!(
        strength_of("assert \"12\" in str(rate(2))"),
        Strength::Containment
    );
    assert_eq!(strength_of("assert rate(2) == 9.0"), Strength::Equality);
    assert_eq!(
        strength_of("expect(x).toContain('needle')"),
        Strength::Containment
    );
}

#[test]
fn stripping_handles_every_quote_form_without_eating_the_expression() {
    for (input, must_keep) in [
        (r#"assert x == "a in b""#, "=="),
        (r#"assert x == 'a in b'"#, "=="),
        (r#"assert_eq!(a, b, "it's fine")"#, "assert_eq!"),
        (r#"assert x == "escaped \" quote in it""#, "=="),
    ] {
        let stripped = strip_string_literals(input);
        assert!(
            stripped.contains(must_keep),
            "stripping ate the expression: {input:?} -> {stripped:?}"
        );
        assert!(
            !stripped.contains(" in b"),
            "a literal survived: {input:?} -> {stripped:?}"
        );
    }
}

#[test]
fn the_gate_itself_a_shape_derived_finding_can_reach_fail_so_stripping_is_mandatory() {
    // The two halves of the gate, asserted together so neither can be relaxed
    // without the other being noticed.
    //
    // Half 1: a shape-derived finding CAN reach `fail`.
    let base = "def test_a():\n    assert rate(2) == 9.0\n";
    let head = "def test_a():\n    assert rate(2) > 0\n";
    let findings = diff_test_file("test_x.py", base, head, Language::Python).expect("diff");
    assert!(
        findings
            .iter()
            .any(|f| f.detector == "assertion_weakened" && f.severity == Severity::Fail),
        "no shape-derived finding reaches fail, so this gate is dormant — if that \
         is deliberate, say so here rather than deleting the test: {findings:?}"
    );

    // Half 2: therefore the classifier must not be decidable by a message
    // string. A weakening that exists ONLY because of a literal must not fire.
    let base = "def test_a():\n    assert check(x), \"value == expected\"\n";
    let head = "def test_a():\n    assert check(x), \"value in expected\"\n";
    let findings = diff_test_file("test_x.py", base, head, Language::Python).expect("diff");
    assert!(
        !findings.iter().any(|f| f.detector == "assertion_weakened"),
        "changing only a MESSAGE string was read as a weakened assertion, and that \
         weakening can now refute a claim: {findings:?}"
    );
}

#[test]
fn a_message_only_change_cannot_refute_a_claim_end_to_end() {
    // The same gate at the verifier, because that is where the cost lands.
    let base = "def test_a():\n    assert check(x), \"a == b\"\n";
    let head = "def test_a():\n    assert check(x), \"a in b\"\n";
    let findings = diff_test_file("test_x.py", base, head, Language::Python).expect("diff");
    let session = SessionView {
        session_id: "s".into(),
        snapshots: vec![SnapshotView {
            seq: 10,
            tree_hash: "t".into(),
        }],
        edits: vec![],
        checks: vec![CheckView {
            id: "chk".into(),
            seq: Some(5),
            category: "test".into(),
            runner: "pytest".into(),
            bypass: None,
            argv: Some(vec!["pytest".into(), "-q".into()]),
            exit_code: 0,
            input_tree_hash: "t".into(),
            paths_changed_since_final: None,
            stdio: Some("pipe".into()),
            reexecution: None,
            parsed: Some(ParsedView {
                tests_total: 1,
                passed: 1,
                failed: 0,
                skipped: 0,
                xfail: 0,
            }),
        }],
        head_commit: Some("c".into()),
        capture_degraded: false,
        degraded_reason: None,
    };
    let record = verify(
        "clm",
        &session,
        &Inputs {
            scope_is_all: true,
            tamper_findings: findings,
            source_changed: true,
            test_files_changed: vec!["test_x.py".into()],
            coverage: None,
        },
    );
    assert_eq!(
        record.verdict,
        thruline_verify::verdict::Verdict::Supported,
        "a message-string edit refuted a claim: {record:#?}"
    );
}

#[test]
fn the_classifier_strips_before_it_matches_and_the_source_says_so() {
    // A source-level control, in the repo's own census idiom: the stripping
    // must happen INSIDE `strength_of`, not at some call site that a new
    // caller could skip.
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tamper.rs"),
    )
    .expect("read tamper.rs");
    let at = src
        .find("pub fn strength_of(")
        .expect("strength_of is gone; the gate needs rewriting");
    let body: String = src[at..].chars().take(600).collect();
    assert!(
        body.contains("strip_string_literals("),
        "`strength_of` no longer strips string literals, and a weakening can \
         still reach `fail` — the 0.082% ambiguous cases are message strings, \
         and every one of them can now refute a claim"
    );
}

// ---------------------------------------------------------------------------
// The residual vector: a NEW reader of assertion shapes
// ---------------------------------------------------------------------------

/// Walk every workspace member's `src/`, in the idiom
/// `crates/core/tests/a209_masking_corpus.rs` established.
fn workspace_sources() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("workspace manifest");
    // `members` is one line, by the same constraint that keeps the redaction
    // census honest.
    let line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("members"))
        .expect("a members key");
    let inner = line
        .split('[')
        .nth(1)
        .and_then(|s| s.split(']').next())
        .expect("a members array");
    inner
        .split(',')
        .map(|m| root.join(m.trim().trim_matches('"')).join("src"))
        .filter(|p| p.is_dir())
        .collect()
}

fn walk_rs(dir: &std::path::Path, f: &mut impl FnMut(&std::path::Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_rs(&path, f);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                f(&path, &text);
            }
        }
    }
}

/// The enclosing `fn` name for a byte offset.
fn enclosing_fn(text: &str, at: usize) -> String {
    text[..at]
        .rmatch_indices("fn ")
        .find_map(|(i, _)| {
            let rest = &text[i + 3..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            (!name.is_empty()).then_some(name)
        })
        .unwrap_or_else(|| "<unknown>".to_string())
}

#[test]
fn every_reader_of_assertion_shapes_is_declared() {
    // The escalation gate proves things about `strength_of`. A NEW caller is
    // the one vector it cannot see — so callers are enumerated, and an
    // undeclared one fails the build.
    let declared: std::collections::BTreeSet<(String, String)> =
        thruline_verify::tamper::SHAPE_CALL_SITES
            .iter()
            .map(|(f, n, _)| ((*f).to_string(), (*n).to_string()))
            .collect();

    let mut found: std::collections::BTreeSet<(String, String)> = Default::default();
    for src in workspace_sources() {
        walk_rs(&src, &mut |path, text| {
            // `#[cfg(test)]` inline blocks and out-of-line test modules are not
            // production callers.
            if path
                .file_name()
                .is_some_and(|f| f == "tests.rs" || f == "mod.rs" && text.contains("#[cfg(test)]"))
            {
                return;
            }
            let file = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default();
            for (at, _) in text.match_indices("strength_of(") {
                // The definition itself is not a call site.
                let before = &text[at.saturating_sub(10)..at];
                if before.contains("fn ") {
                    continue;
                }
                found.insert((file.clone(), enclosing_fn(text, at)));
            }
        });
    }

    assert_eq!(
        found, declared,
        "readers of assertion shapes drifted from SHAPE_CALL_SITES.\n\
         A new caller is outside everything `shape_classifier_gate` proves — it \n\
         may read a shape without stripping literals, or compare strengths \n\
         without going through `diff_test_file`. Declare it, with a reason."
    );
}

#[test]
fn the_shape_census_can_actually_fail() {
    // The negative control. A census that finds nothing passes trivially, and
    // this project has shipped exactly that mistake before: a truncation census
    // whose assertion was `assert_eq!(documented.len(), 4)`.
    let sources = workspace_sources();
    assert!(
        sources.len() > 5,
        "the walk found {} member src/ trees; it is not covering the workspace",
        sources.len()
    );
    let mut sites = 0usize;
    for src in &sources {
        walk_rs(src, &mut |_, text| {
            sites += text.matches("strength_of(").count();
        });
    }
    assert!(
        sites >= 2,
        "the walk found {sites} mentions of `strength_of(` — it should see at \
         least the definition and its one caller, so it is not reading the files"
    );
    assert_eq!(
        thruline_verify::tamper::SHAPE_CALL_SITES.len(),
        1,
        "the declared list changed; if that is deliberate, update this control \
         so it still says what the expected shape is"
    );
}

#[test]
fn the_shape_vocabulary_is_not_reimplemented_outside_the_classifier() {
    // The census above catches a new CALLER of `strength_of`. It does not
    // catch an independent REIMPLEMENTATION that never calls it — someone
    // writing their own `if text.contains(" in ")` beside it.
    //
    // Closing that needs the idiom `the_honest_failure_markers_appear_in_no_
    // crate_but_explain` established: grep for distinctive strings. A single
    // one is useless here — `" in "` appears in every third doc comment — so
    // the test requires the **conjunction**, which is what makes it
    // low-noise: three shape tokens together only make sense in a shape
    // classifier.
    const VOCABULARY: &[&str] = &["\"tocontain\"", "\"assertin\"", "\"startswith\""];

    let mut hits: Vec<String> = Vec::new();
    for src in workspace_sources() {
        walk_rs(&src, &mut |path, text| {
            let file = path.file_name().map(|f| f.to_string_lossy().to_string());
            // `tamper.rs` is the classifier itself.
            if file.as_deref() == Some("tamper.rs") {
                return;
            }
            let lower = text.to_ascii_lowercase();
            if VOCABULARY.iter().all(|v| lower.contains(v)) {
                hits.push(path.display().to_string());
            }
        });
    }
    assert!(
        hits.is_empty(),
        "the assertion-shape vocabulary was reimplemented outside `tamper.rs`, \
         where the literal-stripping gate does not reach it: {hits:?}"
    );
}

#[test]
fn the_vocabulary_census_can_actually_fail() {
    // Negative control: the conjunction must be satisfiable, or the test above
    // passes because it can never match anything.
    let text = "let a = \"tocontain\"; let b = \"assertin\"; let c = \"startswith\";";
    const VOCABULARY: &[&str] = &["\"tocontain\"", "\"assertin\"", "\"startswith\""];
    assert!(
        VOCABULARY
            .iter()
            .all(|v| text.to_ascii_lowercase().contains(v)),
        "the census's own predicate cannot match its own vocabulary"
    );
    // And `tamper.rs` really does contain all three — so the exemption is
    // load-bearing rather than decorative.
    let tamper = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tamper.rs"),
    )
    .expect("read tamper.rs")
    .to_ascii_lowercase();
    assert!(
        VOCABULARY.iter().all(|v| tamper.contains(v)),
        "tamper.rs no longer contains the vocabulary; the census exempts a file \
         that would not have matched anyway"
    );
}
