//! **The receipt predicate, held to its published JSON Schema.**
//!
//! `spec/execution-check-v0.1.schema.json` is a contract four external teams
//! will pin fixtures against. This suite is the thing that keeps it true: it
//! validates documents produced by the real serializer — and a real receipt the
//! product actually emitted — against the committed file, and it fails when
//! either side moves without the other.
//!
//! # What "fails when the producer drifts" means here, mechanically
//!
//! Three walks, in opposite directions, because a schema can drift from a
//! producer in three different ways:
//!
//! 1. **The producer grows a field.** The maximal document carries a key the
//!    schema does not declare; `additionalProperties: false` rejects it.
//! 2. **The producer loses or renames a field.** `every_declared_property_is_
//!    produced` walks the schema against the maximal document and reports any
//!    property the schema declares that the producer no longer emits.
//! 3. **A field changes from required to optional or back.** The minimal
//!    document — every `Option` `None`, every skippable `Vec` empty — has
//!    exactly the key set the schema marks `required`, asserted as an equality
//!    at every object.
//!
//! Plus the value domains: the verdict, claim-kind, severity and category
//! vocabularies in the schema are compared against the vocabularies **in the
//! code**, re-derived rather than transcribed (A205). Adding a variant to
//! `Verdict` or `ClaimKind` is a compile error in this file until the schema is
//! republished.
//!
//! # The one property that is checked adversarially rather than structurally
//!
//! §6: *"Never collapse `unverifiable` into `supported`."* A schema that lets a
//! verdict field go missing, go null, or carry an unrecognised string is a
//! schema that permits a document in which that distinction is gone — and
//! `attest::verify` decides `unverifiable` by **string comparison**, so an
//! unrecognised verdict is silently neither. `unverifiable_cannot_be_lost`
//! asserts the schema refuses every one of those documents.

mod schema_subset;

use serde_json::{json, Value};
use std::collections::BTreeSet;

use thruline_attest::statement::{
    AssertedBy, Capture, CheckEntry, ClaimEntry, Predicate, Promises, Session, Statement, Subject,
    VerdictEntry, VerifierInfo, PAYLOAD_TYPE, PREDICATE_TYPE, SCHEMA_ID, STATEMENT_TYPE,
};

/// The committed contract. `include_str!` on purpose: a missing or moved file
/// is a build failure here, not a test that quietly finds nothing to check.
const SCHEMA_SRC: &str = include_str!("../../../spec/execution-check-v0.1.schema.json");

/// A receipt this product actually emitted, kept beside the schema it must
/// satisfy. Provenance is in `fixtures/receipt-real.provenance.txt`.
const REAL_RECEIPT: &str = include_str!("fixtures/receipt-real.json");

fn schema() -> Value {
    serde_json::from_str(SCHEMA_SRC).expect("the committed schema is not valid JSON")
}

fn statement_json(s: &Statement) -> Value {
    serde_json::to_value(s).expect("the producer's own serializer")
}

fn assert_valid(doc: &Value, what: &str) {
    let root = schema();
    let errs = schema_subset::validate(&root, &root, doc);
    assert!(
        errs.is_empty(),
        "{what} does not satisfy the committed schema:\n  {}",
        errs.join("\n  ")
    );
}

fn assert_invalid(doc: &Value, what: &str) {
    let root = schema();
    let errs = schema_subset::validate(&root, &root, doc);
    assert!(
        !errs.is_empty(),
        "{what} was ACCEPTED by the committed schema, and must not be"
    );
}

// ---------------------------------------------------------------------------
// The two reference documents, built through the producer's own types.
// ---------------------------------------------------------------------------

/// Every optional field present, every skippable collection non-empty.
///
/// This is the document that proves the schema declares nothing the producer
/// does not emit, and nothing it does.
fn maximal() -> Statement {
    let mut digest = std::collections::BTreeMap::new();
    digest.insert("gitTree".into(), "a".repeat(40));
    digest.insert("gitCommit".into(), "b".repeat(40));
    Statement::new(
        vec![Subject {
            name: "git+/repo".into(),
            digest,
        }],
        Predicate {
            session: Session {
                id: "sess-1".into(),
                harness: "claude-code".into(),
                model: Some("a-model".into()),
                started: Some("2026-09-05T00:00:00Z".into()),
                ended: Some("2026-09-05T00:01:00Z".into()),
                event_chain_head: format!("sha256:{}", "c".repeat(64)),
                event_count: 42,
            },
            claims: vec![ClaimEntry {
                id: "clm_1".into(),
                kind: "tests_pass".into(),
                text: "All tests pass".into(),
                asserted_by: AssertedBy {
                    harness: "claude-code".into(),
                    model: Some("a-model".into()),
                },
            }],
            checks: vec![CheckEntry {
                id: "chk_1".into(),
                category: "test".into(),
                runner: "pytest".into(),
                argv_hash: format!("sha256:{}", "d".repeat(64)),
                env_hash: format!("sha256:{}", "e".repeat(64)),
                cwd_hash: format!("sha256:{}", "f".repeat(64)),
                input_tree_hash: "a".repeat(40),
                exit_code: 0,
                parsed: json!({"tests_total": 3, "passed": 3, "failed": 0}),
                reexecuted: true,
                container_image_digest: Some(format!("sha256:{}", "1".repeat(64))),
            }],
            verdicts: vec![VerdictEntry {
                claim_id: "clm_1".into(),
                verdict: "supported".into(),
                verifier: "tests_pass@1".into(),
                evidence: vec!["chk_1".into()],
                reasons: vec!["ran on final tree".into()],
                warnings: vec!["1 test skipped".into()],
                findings: vec![json!({
                    "severity": "warn",
                    "detector": "narrowed_run",
                    "path": "tests/test_x.py",
                    "detail": "-k was passed"
                })],
            }],
            promises: Promises {
                open: vec!["prm_1".into()],
                fulfilled: vec!["prm_2".into()],
            },
            verifier: VerifierInfo {
                thruline_version: "0.1.0".into(),
                mode: "clean-sandbox-reexec".into(),
                policy_hash: Some(format!("sha256:{}", "2".repeat(64))),
            },
            capture: Capture {
                sources: vec!["claude-code-hook".into()],
                transcript_hash: Some(format!("sha256:{}", "3".repeat(64))),
                redactions_applied: 3,
                capture_degraded: true,
                content_filters: Some("filter=lfs on *.bin".into()),
                // §8.1's causes. Non-empty here BECAUSE `capture_degraded` is
                // true: the schema now requires the pair in both directions,
                // so a fixture that stated the flag and no cause would be a
                // receipt this build cannot emit.
                degraded_causes: vec!["content_filters: filter=lfs on *.bin".into()],
                git_env_digest: Some(format!("sha256:{}", "4".repeat(64))),
            },
        },
    )
}

/// Every optional field absent, every skippable collection empty. This is the
/// smallest document the producer can emit.
fn minimal() -> Statement {
    let mut digest = std::collections::BTreeMap::new();
    digest.insert("gitTree".into(), "a".repeat(40));
    Statement::new(
        vec![Subject {
            name: "git+/repo".into(),
            digest,
        }],
        Predicate {
            session: Session {
                id: "sess-1".into(),
                harness: "claude-code".into(),
                model: None,
                started: None,
                ended: None,
                event_chain_head: format!("sha256:{}", "c".repeat(64)),
                event_count: 0,
            },
            claims: vec![ClaimEntry {
                id: "clm_1".into(),
                kind: "tests_pass".into(),
                text: "All tests pass".into(),
                asserted_by: AssertedBy {
                    harness: "claude-code".into(),
                    model: None,
                },
            }],
            checks: vec![CheckEntry {
                id: "chk_1".into(),
                category: "test".into(),
                runner: "pytest".into(),
                // Empty is MEANINGFUL and must validate: an unbound check
                // (no snapshot could be taken) stores the empty string, which
                // can never equal a tree hash. A schema that demanded a hash
                // here would reject the honest record of a failed snapshot.
                argv_hash: String::new(),
                env_hash: String::new(),
                cwd_hash: String::new(),
                input_tree_hash: String::new(),
                exit_code: 1,
                parsed: json!({"unverifiable": {"reason": "unknown runner", "hint": "…"}}),
                reexecuted: false,
                container_image_digest: None,
            }],
            verdicts: vec![VerdictEntry {
                claim_id: "clm_1".into(),
                verdict: "unverifiable".into(),
                verifier: "tests_pass@1".into(),
                evidence: vec![],
                reasons: vec!["no test check was captured".into()],
                warnings: vec![],
                findings: vec![],
            }],
            promises: Promises::default(),
            verifier: VerifierInfo {
                thruline_version: "0.1.0".into(),
                mode: "local".into(),
                policy_hash: Some(format!("sha256:{}", "2".repeat(64))),
            },
            capture: Capture {
                sources: vec![],
                transcript_hash: None,
                redactions_applied: 0,
                capture_degraded: false,
                content_filters: None,
                // Empty exactly because the flag is false; the schema enforces
                // that direction too.
                degraded_causes: vec![],
                git_env_digest: None,
            },
        },
    )
}

// ---------------------------------------------------------------------------
// 1. The schema is the crate's constants, not a copy of them.
// ---------------------------------------------------------------------------

#[test]
fn the_schema_names_the_crate_s_own_identifiers() {
    let s = schema();
    assert_eq!(
        s["$id"].as_str(),
        Some(SCHEMA_ID),
        "the schema's $id and `statement::SCHEMA_ID` must be the same string; the \
         predicate domain was ruled once and both halves move together"
    );
    assert_eq!(
        s["properties"]["predicateType"]["const"].as_str(),
        Some(PREDICATE_TYPE),
        "the schema pins a predicateType that is not the one this build issues"
    );
    assert_eq!(
        s["properties"]["_type"]["const"].as_str(),
        Some(STATEMENT_TYPE)
    );
    assert_eq!(
        s["$defs"]["dsseEnvelope"]["properties"]["payloadType"]["const"].as_str(),
        Some(PAYLOAD_TYPE)
    );
}

#[test]
fn every_pattern_in_the_schema_is_implemented_here() {
    let s = schema();
    let mut used = Vec::new();
    schema_subset::patterns_used(&s, &mut used);
    let implemented: BTreeSet<&str> = schema_subset::patterns().iter().map(|(k, _)| *k).collect();
    let used: BTreeSet<&str> = used.iter().map(String::as_str).collect();
    assert!(
        used.is_subset(&implemented),
        "the schema uses pattern(s) this evaluator does not implement, so they would be \
         validated by nothing: {:?}",
        used.difference(&implemented).collect::<Vec<_>>()
    );
}

#[test]
fn the_schema_uses_no_keyword_this_suite_cannot_evaluate() {
    let s = schema();
    let mut errs = Vec::new();
    schema_subset::check_keywords(&s, "#", &mut errs);
    assert!(
        errs.is_empty(),
        "a keyword nobody evaluates validates nothing:\n  {}",
        errs.join("\n  ")
    );
}

/// Direction 1 is enforced by `additionalProperties: false`, and by nothing
/// else. Loosening one — to `true`, or by deleting the keyword — would disarm
/// the only guard that catches a producer growing a field, and no other test in
/// this file would notice. So the closure itself is asserted.
#[test]
fn every_object_in_the_schema_is_closed() {
    /// The two objects that are open ON PURPOSE, each with the reason. A third
    /// entry here is a decision, not a fix.
    const OPEN_BY_DESIGN: &[(&str, &str)] = &[
        (
            "#/$defs/subject/properties/digest",
            "in-toto permits further digest algorithms; the values are still constrained to strings",
        ),
        (
            "#/$defs/checkEntry/properties/parsed",
            "a growing union of runner-output shapes; constrained instead by `minProperties: 1`, because the failure mode here is an EMPTY block reading as a clean run",
        ),
    ];

    fn walk(node: &Value, path: String, out: &mut Vec<String>) {
        let Some(map) = node.as_object() else { return };
        if map.get("type").and_then(Value::as_str) == Some("object")
            && !OPEN_BY_DESIGN.iter().any(|(p, _)| *p == path)
            && map.get("additionalProperties") != Some(&Value::Bool(false))
        {
            out.push(format!(
                "{path} is an object schema that is not closed (additionalProperties = {})",
                map.get("additionalProperties")
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "absent".into())
            ));
        }
        for (k, v) in map {
            match k.as_str() {
                "properties" | "$defs" => {
                    if let Some(m) = v.as_object() {
                        for (name, sub) in m {
                            walk(sub, format!("{path}/{k}/{name}"), out);
                        }
                    }
                }
                "items" | "then" | "additionalProperties" => walk(v, format!("{path}/{k}"), out),
                "allOf" => {
                    if let Some(a) = v.as_array() {
                        for (i, sub) in a.iter().enumerate() {
                            walk(sub, format!("{path}/allOf/{i}"), out);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let mut open = Vec::new();
    walk(&schema(), "#".to_string(), &mut open);
    assert!(
        open.is_empty(),
        "an open object accepts a field the producer grew without anyone noticing, which \
         is the drift this suite exists to catch:\n  {}",
        open.join("\n  ")
    );
    // And the allowlist must not rot into a list of things that are now closed.
    for (p, _) in OPEN_BY_DESIGN {
        assert!(
            schema().pointer(p.trim_start_matches('#')).is_some(),
            "{p} is allowlisted as open and no longer exists in the schema"
        );
    }
}

// ---------------------------------------------------------------------------
// 2 & 3. The producer's own documents, both directions.
// ---------------------------------------------------------------------------

#[test]
fn the_maximal_receipt_validates() {
    assert_valid(&statement_json(&maximal()), "the maximal statement");
}

#[test]
fn the_minimal_receipt_validates() {
    assert_valid(&statement_json(&minimal()), "the minimal statement");
}

/// Direction 2: the schema declares nothing the producer stopped emitting.
#[test]
fn every_declared_property_is_produced() {
    let root = schema();
    let doc = statement_json(&maximal());
    let mut missing = Vec::new();
    declared_but_absent(&root, &root, &doc, "$", &mut missing);
    assert!(
        missing.is_empty(),
        "the schema declares propert(ies) the producer no longer emits — a consumer \
         reading the schema would wait for a field that never arrives:\n  {}",
        missing.join("\n  ")
    );
}

/// Direction 3: `required` is exactly the minimal document's key set.
#[test]
fn required_is_exactly_what_the_minimal_receipt_carries() {
    let root = schema();
    let doc = statement_json(&minimal());
    let mut errs = Vec::new();
    required_equals_keys(&root, &root, &doc, "$", &mut errs);
    assert!(
        errs.is_empty(),
        "`required` and the smallest document the producer can emit disagree:\n  {}",
        errs.join("\n  ")
    );
}

fn deref<'a>(root: &'a Value, s: &'a Value) -> &'a Value {
    match s.get("$ref").and_then(Value::as_str) {
        Some(r) => {
            let mut node = root;
            for seg in r.trim_start_matches("#/").split('/') {
                match node.get(seg) {
                    Some(n) => node = n,
                    None => return s,
                }
            }
            node
        }
        None => s,
    }
}

fn declared_but_absent(root: &Value, s: &Value, doc: &Value, path: &str, out: &mut Vec<String>) {
    let s = deref(root, s);
    if let (Some(props), Some(o)) = (
        s.get("properties").and_then(Value::as_object),
        doc.as_object(),
    ) {
        for (name, sub) in props {
            match o.get(name) {
                Some(child) => {
                    declared_but_absent(root, sub, child, &format!("{path}.{name}"), out)
                }
                None => out.push(format!("{path}.{name} is declared and not produced")),
            }
        }
    }
    if let (Some(items), Some(a)) = (s.get("items"), doc.as_array()) {
        for (i, child) in a.iter().enumerate() {
            declared_but_absent(root, items, child, &format!("{path}[{i}]"), out);
        }
        if a.is_empty() {
            out.push(format!(
                "{path} is empty in the maximal document, so its item schema is checked by \
                 nothing"
            ));
        }
    }
}

fn required_equals_keys(root: &Value, s: &Value, doc: &Value, path: &str, out: &mut Vec<String>) {
    let s = deref(root, s);
    if let Some(o) = doc.as_object() {
        // Only objects the schema actually describes with `properties` are
        // compared; open objects (a digest map, a `parsed` block) have no
        // declared key set by design.
        if s.get("properties").is_some() {
            let required: BTreeSet<&str> = s
                .get("required")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).collect())
                .unwrap_or_default();
            let present: BTreeSet<&str> = o.keys().map(String::as_str).collect();
            for k in present.difference(&required) {
                out.push(format!(
                    "{path}.{k} is emitted by the minimal receipt and is not `required`"
                ));
            }
            for k in required.difference(&present) {
                out.push(format!(
                    "{path}.{k} is `required` and the minimal receipt does not carry it"
                ));
            }
        }
        if let Some(props) = s.get("properties").and_then(Value::as_object) {
            for (name, sub) in props {
                if let Some(child) = o.get(name) {
                    required_equals_keys(root, sub, child, &format!("{path}.{name}"), out);
                }
            }
        }
    }
    if let (Some(items), Some(a)) = (s.get("items"), doc.as_array()) {
        for (i, child) in a.iter().enumerate() {
            required_equals_keys(root, items, child, &format!("{path}[{i}]"), out);
        }
    }
}

// ---------------------------------------------------------------------------
// 4. Value domains, re-derived from the code rather than transcribed.
// ---------------------------------------------------------------------------

fn schema_enum(s: &Value, pointer: &str) -> BTreeSet<String> {
    s.pointer(pointer)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("no enum at {pointer}"))
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

#[test]
fn the_verdict_domain_is_the_code_s_domain() {
    use thruline_verify::Verdict;
    // Exhaustive by construction: adding a variant is a compile error here, and
    // renaming one of the wire strings fails the equality below.
    fn wire(v: Verdict) -> &'static str {
        match v {
            Verdict::Supported => "supported",
            Verdict::Unsupported => "unsupported",
            Verdict::Unverifiable => "unverifiable",
        }
    }
    let code: BTreeSet<String> = [
        Verdict::Supported,
        Verdict::Unsupported,
        Verdict::Unverifiable,
    ]
    .into_iter()
    .map(|v| {
        assert_eq!(
            wire(v),
            v.as_str(),
            "Verdict::as_str changed under this test"
        );
        v.as_str().to_string()
    })
    .collect();
    assert_eq!(
        schema_enum(&schema(), "/$defs/verdictEntry/properties/verdict/enum"),
        code,
        "the schema's verdict vocabulary is not `thruline_verify::Verdict`'s. §6: a \
         verdict string the reader does not know is neither `unverifiable` nor \
         `unsupported` to `attest::verify`, which decides by string comparison."
    );
}

#[test]
fn the_claim_kind_domain_is_the_code_s_domain() {
    use thruline_claims::ClaimKind::*;
    let all = [
        TestsPass,
        BuildOk,
        LintOk,
        TypecheckOk,
        BehaviorUnchanged,
        MigrationAdded,
        SecretsAbsent,
        DocsUpdated,
        DependencyAdded,
        FileChanged,
        DidX,
        TaskComplete,
    ];
    // Exhaustiveness guard: a new variant breaks this match.
    for k in all {
        let _: &str = match k {
            TestsPass => "tests_pass",
            BuildOk => "build_ok",
            LintOk => "lint_ok",
            TypecheckOk => "typecheck_ok",
            BehaviorUnchanged => "behavior_unchanged",
            MigrationAdded => "migration_added",
            SecretsAbsent => "secrets_absent",
            DocsUpdated => "docs_updated",
            DependencyAdded => "dependency_added",
            FileChanged => "file_changed",
            DidX => "did_x",
            TaskComplete => "task_complete",
        };
    }
    let code: BTreeSet<String> = all.iter().map(|k| k.as_str().to_string()).collect();
    assert_eq!(
        schema_enum(&schema(), "/$defs/claimEntry/properties/kind/enum"),
        code,
        "the schema's claim-kind vocabulary is not `thruline_claims::ClaimKind`'s"
    );
}

#[test]
fn the_finding_severity_domain_is_the_code_s_domain() {
    use thruline_verify::Severity;
    let all = [Severity::Info, Severity::Warn, Severity::Fail];
    for s in all {
        let _: &str = match s {
            Severity::Info => "info",
            Severity::Warn => "warn",
            Severity::Fail => "fail",
        };
    }
    let code: BTreeSet<String> = all
        .iter()
        .map(|s| {
            serde_json::to_value(s)
                .expect("severity serialises")
                .as_str()
                .expect("as a string")
                .to_string()
        })
        .collect();
    assert_eq!(
        schema_enum(&schema(), "/$defs/finding/properties/severity/enum"),
        code
    );
}

#[test]
fn the_check_category_domain_is_the_code_s_domain() {
    use thruline_verify::check::{
        CATEGORY_BUILD, CATEGORY_LINT, CATEGORY_TEST, CATEGORY_TYPECHECK,
    };
    let imported: BTreeSet<String> = [
        CATEGORY_TEST,
        CATEGORY_BUILD,
        CATEGORY_LINT,
        CATEGORY_TYPECHECK,
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    // The categories are four separate `pub const`s, not an enum, so importing
    // them proves no variant was renamed but cannot prove none was ADDED. The
    // census closes that half by reading the declaration site.
    let src = include_str!("../../verify/src/check.rs");
    let declared: BTreeSet<String> = src
        .lines()
        .filter_map(|l| l.trim().strip_prefix("pub const CATEGORY_"))
        .filter_map(|l| l.split('"').nth(1))
        .map(str::to_string)
        .collect();
    assert_eq!(
        declared, imported,
        "`crates/verify/src/check.rs` declares a CATEGORY_ constant this test does not \
         import — the schema's category enum is now narrower than the producer's"
    );
    assert_eq!(
        schema_enum(&schema(), "/$defs/checkEntry/properties/category/enum"),
        imported
    );
}

// ---------------------------------------------------------------------------
// 5. §6 — the distinction the schema is not allowed to lose.
// ---------------------------------------------------------------------------

#[test]
fn unverifiable_cannot_be_lost() {
    let base = statement_json(&minimal());
    let with_verdict = |v: Value| {
        let mut d = base.clone();
        match v {
            Value::Null => {
                d["predicate"]["verdicts"][0]["verdict"] = Value::Null;
            }
            _ => d["predicate"]["verdicts"][0]["verdict"] = v,
        }
        d
    };

    // Each of the three real verdicts must be expressible.
    for v in ["supported", "unsupported", "unverifiable"] {
        assert_valid(&with_verdict(json!(v)), &format!("verdict {v:?}"));
    }

    // An unknown verdict string is the collapse in its quietest form:
    // `attest::verify` counts `unverifiable` by `v.verdict == "unverifiable"`,
    // so anything it does not recognise is silently NOT unverifiable and the
    // receipt reports "none left unverifiable".
    for v in ["SUPPORTED", "Unverifiable", "ok", "pass", "unverified", ""] {
        assert_invalid(&with_verdict(json!(v)), &format!("verdict {v:?}"));
    }
    assert_invalid(&with_verdict(Value::Null), "a null verdict");

    // Absent entirely.
    let mut d = base.clone();
    d["predicate"]["verdicts"][0]
        .as_object_mut()
        .expect("verdict entry")
        .remove("verdict");
    assert_invalid(&d, "a verdict entry with no verdict");

    // A verdict may not become a claim's optional decoration either: the entry
    // must name the claim it is about.
    let mut d = base.clone();
    d["predicate"]["verdicts"][0]
        .as_object_mut()
        .expect("verdict entry")
        .remove("claim_id");
    assert_invalid(&d, "a verdict entry with no claim_id");
}

// ---------------------------------------------------------------------------
// 6. The cross-field invariants `attest::verify` enforces, enforced here too.
// ---------------------------------------------------------------------------

#[test]
fn a_reexecution_claim_without_its_digest_is_refused() {
    // `verify.rs` §5: "a bare boolean is a claim, not evidence."
    let mut d = statement_json(&maximal());
    d["predicate"]["checks"][0]
        .as_object_mut()
        .expect("check")
        .remove("container_image_digest");
    assert_invalid(&d, "`reexecuted: true` with no container image digest");

    // And the inverse, or the digest becomes decorative.
    let mut d = statement_json(&maximal());
    d["predicate"]["checks"][0]["reexecuted"] = json!(false);
    assert_invalid(&d, "a container image digest with `reexecuted: false`");

    // A digest that is not a digest.
    let mut d = statement_json(&maximal());
    d["predicate"]["checks"][0]["container_image_digest"] = json!("sha256:short");
    assert_invalid(&d, "a truncated container image digest");
}

#[test]
fn content_filters_without_degraded_capture_are_refused() {
    // `verify.rs` §7: the two halves may not disagree about whether the tree
    // hash names the bytes on disk.
    let mut d = statement_json(&maximal());
    d["predicate"]["capture"]["capture_degraded"] = json!(false);
    assert_invalid(&d, "content filters with `capture_degraded: false`");
}

#[test]
fn a_receipt_that_cannot_name_its_policy_is_refused() {
    // `verify.rs` §6: absent and empty are different failures, and neither is
    // a pass. A schema that let either through would publish, as valid, a
    // document this build refuses.
    let mut d = statement_json(&minimal());
    d["predicate"]["verifier"]
        .as_object_mut()
        .expect("verifier")
        .remove("policy_hash");
    assert_invalid(&d, "a receipt with no policy hash");

    let mut d = statement_json(&minimal());
    d["predicate"]["verifier"]["policy_hash"] = json!("");
    assert_invalid(&d, "a receipt with an empty policy hash");
}

#[test]
fn an_empty_parsed_block_is_refused() {
    // §3.3: never `{}` — "a defaulted parsed block reads as a clean run".
    // `null` is the same failure with a different spelling, and the producer
    // can emit it (`serde_json::from_str(..).unwrap_or(Value::Null)`).
    for empty in [json!({}), Value::Null] {
        let mut d = statement_json(&minimal());
        d["predicate"]["checks"][0]["parsed"] = empty.clone();
        assert_invalid(&d, &format!("a check whose parsed block is {empty}"));
    }
}

#[test]
fn a_statement_about_nothing_is_refused() {
    // `Statement::validate`: no subject, and no gitTree on a subject.
    let mut d = statement_json(&minimal());
    d["subject"] = json!([]);
    assert_invalid(&d, "a statement with no subject");

    let mut d = statement_json(&minimal());
    d["subject"][0]["digest"]
        .as_object_mut()
        .expect("digest")
        .remove("gitTree");
    assert_invalid(&d, "a subject with no gitTree digest");

    let mut d = statement_json(&minimal());
    d["predicate"]["session"]["event_chain_head"] = json!("");
    assert_invalid(&d, "a predicate with no event chain head");

    let mut d = statement_json(&minimal());
    d["predicateType"] = json!("https://example.invalid/other/v1");
    assert_invalid(&d, "a foreign predicate type");
}

// ---------------------------------------------------------------------------
// 7. A receipt the product actually emitted.
// ---------------------------------------------------------------------------

#[test]
fn a_real_receipt_from_the_product_validates() {
    use base64::Engine as _;
    let root = schema();
    let envelope: Value = serde_json::from_str(REAL_RECEIPT).expect("the fixture is JSON");

    let errs = schema_subset::validate(&root, &root["$defs"]["dsseEnvelope"], &envelope);
    assert!(
        errs.is_empty(),
        "the DSSE envelope the product wrote does not satisfy the published envelope \
         shape:\n  {}",
        errs.join("\n  ")
    );

    let payload = base64::engine::general_purpose::STANDARD
        .decode(envelope["payload"].as_str().expect("payload"))
        .expect("payload is base64");
    let statement: Value = serde_json::from_slice(&payload).expect("payload is JSON");

    assert_eq!(
        statement["predicateType"].as_str(),
        Some(PREDICATE_TYPE),
        "the committed fixture receipt was issued under a different predicate type than \
         this build emits — regenerate it (see fixtures/receipt-real.provenance.txt) \
         rather than editing the string, or the fixture stops being a real receipt"
    );
    assert_valid(&statement, "a receipt the product emitted");

    // The fixture must also be a receipt this build can still parse into its
    // own types: schema-valid and type-invalid would mean the schema had
    // drifted away from the producer while still describing the fixture.
    let parsed: Statement = serde_json::from_slice(&payload)
        .expect("the fixture no longer deserialises into `Statement`");
    assert!(parsed.validate().is_ok());
}
