use super::*;

/// §9's shipped example, verbatim. If this stops parsing, the documented policy
/// and the implemented policy have separated.
const EXAMPLE: &str = r#"
version: 1
require_receipt: true
claims:
  tests_pass:
    required_when: { paths: ["src/**", "lib/**"] }
    require_reexecuted: true
    min_changed_line_coverage: 70
    allow_skips: false
  behavior_unchanged:
    require_evidence: true
  test_file_changes:
    require_human_ack: true          # label `tests-changed-ok` or approving review
promises:
  block_on_open: false
  comment_open: true
stop_gate:
  enabled: true
  max_stop_blocks: 2
reexec:
  mode: claimed                      # claimed|full
  reruns: 2
  image: auto
"#;

fn example() -> Policy {
    Policy::parse(EXAMPLE).expect("§9's own example must parse")
}

#[test]
fn the_specs_example_parses_to_what_it_says() {
    let p = example();
    assert_eq!(p.version, 1);
    assert!(p.require_receipt);
    assert_eq!(p.claims.len(), 3);
    let tp = &p.claims["tests_pass"];
    assert_eq!(
        tp.required_when_paths.as_deref(),
        Some(["src/**".to_string(), "lib/**".to_string()].as_slice())
    );
    assert!(tp.require_reexecuted);
    assert_eq!(tp.min_changed_line_coverage, Some(70));
    assert!(!tp.allow_skips);
    assert!(p.claims["behavior_unchanged"].require_evidence);
    assert!(p.claims["test_file_changes"].require_human_ack);
    assert!(p.stop_gate.enabled);
    assert_eq!(p.reexec.mode, ReexecMode::Claimed);
    assert_eq!(p.reexec.image, "auto");
    // The trailing comment after `claimed` is a comment, not part of the value.
    assert_eq!(p.reexec.reruns, 2);
}

// ---------------------------------------------------------------------------
// The closed schema, which is the security property.
// ---------------------------------------------------------------------------

/// **[BUILD-PLAN C.8.3]** No policy key may reach the extractor, a score or a
/// confidence. A future field that did would look like a feature and would put
/// the model back into enforcement.
#[test]
fn no_policy_key_can_reference_an_extractor_or_a_score() {
    let forbidden = [
        "extractor",
        "score",
        "confidence",
        "probability",
        "model",
        "llm",
        "nli",
        "threshold",
        "annotator",
        "judge",
    ];
    for key in parse::SCHEMA {
        let lower = key.to_ascii_lowercase();
        for bad in forbidden {
            assert!(
                !lower.contains(bad),
                "the policy schema gained `{key}`, which references `{bad}`. \
                 Policy evaluates rules-extracted claims only; a key that can \
                 name the extractor or a score lets a model's output decide a \
                 merge."
            );
        }
    }
    // Positive control: the walk is looking at a real, non-empty schema.
    assert!(parse::SCHEMA.len() > 15, "{}", parse::SCHEMA.len());
    assert!(parse::SCHEMA.contains(&"require_receipt"));
}

#[test]
fn an_unknown_key_is_refused_rather_than_ignored() {
    let err = Policy::parse("version: 1\nrequire_reciept: true\n").expect_err("typo is refused");
    match err {
        ParseError::UnknownKey { key, line, .. } => {
            assert_eq!(key, "require_reciept");
            assert_eq!(line, 2);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn done_templates_are_named_lines_without_changing_the_gate_rules() {
    let text = "version: 1\nrequire_receipt: true\ntemplates:\n  bug_fix:\n    lines: [\"Handle empty input, with an error.\", \"Preserve the bytes.\"]\n";
    let policy = Policy::parse(text).expect("template policy");
    assert!(policy.require_receipt);
    assert_eq!(policy.templates.len(), 1);
    assert_eq!(
        policy.templates["bug_fix"].lines,
        ["Handle empty input, with an error.", "Preserve the bytes."]
    );
    assert_eq!(
        policy.evaluate(&Subject::default()).conclusion,
        Conclusion::Failure
    );
}

#[test]
fn done_templates_refuse_unknown_fields_and_unusable_lines() {
    for text in [
        "version: 1\ntemplates:\n  bug_fix:\n    line: [\"one\"]\n",
        "version: 1\ntemplates:\n  bug_fix:\n    lines: []\n",
        "version: 1\ntemplates:\n  bug_fix:\n    lines: [\"one\", \"\"]\n",
        "version: 1\ntemplates:\n  bug_fix:\n    lines: one\n",
        "version: 1\ntemplates:\n  bug.fix:\n    lines: [\"one\"]\n",
        "version: 1\ntemplates:\n  bug_fix:\n    lines: [\"one\"]\n  bug_fix:\n    lines: [\"two\"]\n",
        "version: 1\ntemplates: { bug_fix: { lines: [\"one\"], ignored: true } }\n",
        "version: 1\ntemplates:\n  bug_fix: { lines: [\"one\"], ignored: true }\n",
        "version: 1\ntemplates:\n  bug_fix:\n    lines: [\"one\"]\n    lines: [\"two\"]\n",
    ] {
        assert!(Policy::parse(text).is_err(), "accepted {text}");
    }
    let overlong = format!(
        "version: 1\ntemplates:\n  bug_fix:\n    lines: [\"{}\"]\n",
        "x".repeat(241)
    );
    assert!(Policy::parse(&overlong).is_err());
}

/// The typo above is the whole argument for a closed schema: under a permissive
/// reader this file silently means `require_receipt: false`.
#[test]
fn the_typo_would_otherwise_have_silently_disabled_the_requirement() {
    let correct = Policy::parse("version: 1\nrequire_receipt: true\n").expect("parses");
    assert!(correct.require_receipt);
    assert!(Policy::parse("version: 1\nrequire_reciept: true\n").is_err());
}

#[test]
fn a_key_that_could_name_the_extractor_is_refused_by_the_schema_itself() {
    for attempt in [
        "version: 1\nclaims:\n  tests_pass:\n    extractor: model\n",
        "version: 1\nclaims:\n  tests_pass:\n    min_confidence: 0.9\n",
        "version: 1\nmin_score: 0.8\n",
    ] {
        assert!(
            matches!(Policy::parse(attempt), Err(ParseError::UnknownKey { .. })),
            "a policy reached the model's output: {attempt}"
        );
    }
}

// ---------------------------------------------------------------------------
// The subset's refusals. Each names the construct.
// ---------------------------------------------------------------------------

#[test]
fn yamls_implicit_booleans_are_refused_by_name() {
    for value in ["yes", "no", "on", "off", "True", "y"] {
        let err = Policy::parse(&format!("version: 1\nrequire_receipt: {value}\n"))
            .expect_err("implicit boolean is refused");
        assert!(matches!(err, ParseError::Type { .. }), "{value}: {err:?}");
    }
}

#[test]
fn constructs_outside_the_subset_are_named_not_guessed() {
    let cases = [
        ("version: 1\nclaims:\n\t  tests_pass:\n", "a tab character"),
        (
            "version: 1\nclaims:\n  tests_pass:\n    required_when:\n      paths:\n        - src/**\n",
            "a block sequence item",
        ),
        ("version: 1\n---\nversion: 1\n", "a document separator"),
    ];
    for (text, want) in cases {
        match Policy::parse(text) {
            Err(ParseError::Unsupported { construct, .. }) => assert_eq!(construct, want),
            other => panic!("{want}: {other:?}"),
        }
    }
}

#[test]
fn a_missing_or_future_version_is_refused() {
    assert!(matches!(
        Policy::parse("require_receipt: true\n"),
        Err(ParseError::Version { .. })
    ));
    assert!(matches!(
        Policy::parse("version: 2\n"),
        Err(ParseError::Version { .. })
    ));
}

#[test]
fn a_hash_inside_a_quoted_glob_is_not_a_comment() {
    let p = Policy::parse(
        "version: 1\nclaims:\n  tests_pass:\n    required_when: { paths: [\"src/#gen/**\"] }\n",
    )
    .expect("parses");
    assert_eq!(
        p.claims["tests_pass"].required_when_paths.as_deref(),
        Some(["src/#gen/**".to_string()].as_slice())
    );
}

// ---------------------------------------------------------------------------
// Globs.
// ---------------------------------------------------------------------------

#[test]
fn globs_match_the_way_the_docs_say() {
    assert!(glob("src/**", "src/a/b/c.rs"));
    assert!(glob("src/**", "src/a.rs"));
    assert!(glob("src/*.rs", "src/a.rs"));
    // `*` does not cross a separator, which is the whole reason `**` exists.
    assert!(!glob("src/*.rs", "src/a/b.rs"));
    assert!(!glob("src/**", "lib/a.rs"));
    assert!(glob("**/test_*.py", "a/b/test_x.py"));
}

// ---------------------------------------------------------------------------
// Evaluation.
// ---------------------------------------------------------------------------

fn claim(kind: &str, verdict: Verdict) -> Claim {
    Claim {
        kind: kind.to_string(),
        extractor: "rules".to_string(),
        verdict,
        reexecuted: true,
        evidence_present: true,
        human_acked: Some(true),
        changed_line_coverage: Some(90),
        skipped_tests: Some(0),
    }
}

fn subject(claims: Vec<Claim>) -> Subject {
    Subject {
        receipt_present: true,
        changed_paths: Some(vec!["src/main.rs".to_string()]),
        open_promises: 0,
        stop_blocks: Some(0),
        claims,
    }
}

#[test]
fn a_supported_claim_under_a_satisfied_rule_passes() {
    let d = example().evaluate(&subject(vec![claim("tests_pass", Verdict::Supported)]));
    assert_eq!(d.conclusion, Conclusion::Success, "{:?}", d.reasons);
    assert!(d.reasons.is_empty());
}

#[test]
fn an_unsupported_claim_fails_the_check() {
    let d = example().evaluate(&subject(vec![claim("tests_pass", Verdict::Unsupported)]));
    assert_eq!(d.conclusion, Conclusion::Failure);
    assert_eq!(d.reasons[0].rule, "verdict");
}

/// §6's third state surviving to the merge button: `unverifiable` is neutral,
/// which is neither a block nor a green check.
#[test]
fn an_unverifiable_claim_is_neutral_and_never_green() {
    let d = example().evaluate(&subject(vec![claim("tests_pass", Verdict::Unverifiable)]));
    assert_eq!(d.conclusion, Conclusion::Neutral);
    assert!(d.reasons[0].detail.contains("reexec"));
}

/// The filter that keeps the model out of enforcement, and the count that keeps
/// it visible.
#[test]
fn a_model_extracted_unsupported_claim_cannot_fail_a_gate_and_is_still_reported() {
    let mut c = claim("tests_pass", Verdict::Unsupported);
    c.extractor = "model".to_string();
    let d = example().evaluate(&subject(vec![c]));
    assert_eq!(
        d.conclusion,
        Conclusion::Success,
        "a model-sourced claim reached the gate: {:?}",
        d.reasons
    );
    assert_eq!(d.advisory.get("model"), Some(&1));
}

/// D10 (Track Q) — a rule that does not apply must say so, and an UNPROVEN
/// claim it declined to judge is not a pass.
///
/// The demo this product ships edits `shipping.py` and `test_shipping.py` at
/// the repository root; the documented example policy scopes `tests_pass` to
/// `src/**` and `lib/**`. So a session whose only claim was an *unsupported*
/// `tests_pass` passed the gate at exit 0, and nothing in the output named the
/// rule that had stood down. Q: *"The path scoping is defensible on its own;
/// shipping it beside a demo it green-lights is not."*
///
/// This test asserted `Success` and was the pin holding the silence in place.
/// It is rewritten rather than deleted, because the *skip* is still correct —
/// only the silence and the green were not.
#[test]
fn a_rule_that_does_not_apply_is_reported_and_an_unproven_claim_is_not_a_pass() {
    let mut s = subject(vec![claim("tests_pass", Verdict::Unsupported)]);
    s.changed_paths = Some(vec!["docs/readme.md".to_string()]);
    let d = example().evaluate(&s);

    assert_eq!(
        d.conclusion,
        Conclusion::Neutral,
        "an unsupported claim the gate declined to judge concluded {:?}: {:?}",
        d.conclusion,
        d.reasons
    );
    let r = d
        .reasons
        .iter()
        .find(|r| r.rule == "required_when")
        .unwrap_or_else(|| panic!("the skipped rule is not in the report: {:?}", d.reasons));
    assert_eq!(r.claim.as_deref(), Some("tests_pass"));
    assert!(r.detail.contains("did not apply"), "{}", r.detail);
    assert!(
        r.detail.contains("unsupported"),
        "the reason does not say what the claim's own verdict was, which is the \
         thing a reader needs: {}",
        r.detail
    );
}

/// The control, and the reason the rule above is verdict-sensitive: a
/// scoped-out rule over a claim the record SUPPORTS is not worth an amber
/// badge. Making every docs-only pull request neutral is how people learn to
/// ignore neutral.
#[test]
fn a_rule_that_does_not_apply_over_a_supported_claim_is_still_a_pass() {
    let mut s = subject(vec![claim("tests_pass", Verdict::Supported)]);
    s.changed_paths = Some(vec!["docs/readme.md".to_string()]);
    let d = example().evaluate(&s);
    assert_eq!(d.conclusion, Conclusion::Success, "{:?}", d.reasons);
    // Reported all the same: the gate says which rules it did not apply,
    // whatever it concluded.
    assert!(
        d.reasons.iter().any(|r| r.rule == "required_when"),
        "the skip was silent for a supported claim: {:?}",
        d.reasons
    );
}

#[test]
fn a_missing_receipt_fails_when_policy_requires_one() {
    let mut s = subject(vec![]);
    s.receipt_present = false;
    let d = example().evaluate(&s);
    assert_eq!(d.conclusion, Conclusion::Failure);
    assert_eq!(d.reasons[0].rule, "require_receipt");
}

/// **Absent coverage is not a satisfied minimum.** A policy asking for 70% that
/// receives no measurement has not been met, and calling that green is the
/// collapse §6 forbids one layer up.
#[test]
fn an_unmeasured_coverage_is_neutral_not_a_pass() {
    let mut c = claim("tests_pass", Verdict::Supported);
    c.changed_line_coverage = None;
    let d = example().evaluate(&subject(vec![c]));
    assert_eq!(d.conclusion, Conclusion::Neutral, "{:?}", d.reasons);
    assert_eq!(d.reasons[0].rule, "min_changed_line_coverage");
}

#[test]
fn coverage_under_the_minimum_fails() {
    let mut c = claim("tests_pass", Verdict::Supported);
    c.changed_line_coverage = Some(69);
    let d = example().evaluate(&subject(vec![c]));
    assert_eq!(d.conclusion, Conclusion::Failure);
}

#[test]
fn a_skipped_test_fails_when_policy_allows_none() {
    let mut c = claim("tests_pass", Verdict::Supported);
    c.skipped_tests = Some(1);
    let d = example().evaluate(&subject(vec![c]));
    assert_eq!(d.conclusion, Conclusion::Failure);
    assert_eq!(d.reasons[0].rule, "allow_skips");
}

#[test]
fn a_claim_that_was_not_reexecuted_fails_when_policy_requires_it() {
    let mut c = claim("tests_pass", Verdict::Supported);
    c.reexecuted = false;
    let d = example().evaluate(&subject(vec![c]));
    assert_eq!(d.conclusion, Conclusion::Failure);
    assert_eq!(d.reasons[0].rule, "require_reexecuted");
}

#[test]
fn failure_dominates_neutral() {
    let mut unver = claim("behavior_unchanged", Verdict::Unverifiable);
    unver.evidence_present = true;
    let d = example().evaluate(&subject(vec![
        unver,
        claim("tests_pass", Verdict::Unsupported),
    ]));
    assert_eq!(d.conclusion, Conclusion::Failure);
}

/// A repository with no policy file must not be gated by one.
#[test]
fn the_default_policy_gates_nothing() {
    let d = Policy::default().evaluate(&subject(vec![claim("tests_pass", Verdict::Unsupported)]));
    assert_eq!(d.conclusion, Conclusion::Success, "{:?}", d.reasons);
}

/// Evaluation is a pure function of its inputs — no clock, no environment, no
/// filesystem — so a check run is reproducible from the receipt and the policy.
#[test]
fn evaluation_is_deterministic() {
    let p = example();
    let s = subject(vec![
        claim("tests_pass", Verdict::Unsupported),
        claim("behavior_unchanged", Verdict::Unverifiable),
    ]);
    let first = p.evaluate(&s);
    for _ in 0..8 {
        assert_eq!(p.evaluate(&s), first);
    }
}

/// **An undetermined path set must not switch a rule off.** The failure mode a
/// silent `Vec::new()` would produce is a green check on a change nobody could
/// see, which is the most expensive kind of wrong this crate can be.
#[test]
fn a_rule_still_applies_when_the_changed_paths_are_unknown() {
    let mut s = subject(vec![claim("tests_pass", Verdict::Unsupported)]);
    s.changed_paths = None;
    assert_eq!(example().evaluate(&s).conclusion, Conclusion::Failure);
}

/// The three unmeasurables are neutral, and each says which one it is.
#[test]
fn an_undetermined_input_is_neutral_and_names_itself() {
    for (mutate, want_rule) in [
        (
            Box::new(|c: &mut Claim| c.skipped_tests = None) as Box<dyn Fn(&mut Claim)>,
            "allow_skips",
        ),
        (
            Box::new(|c: &mut Claim| c.changed_line_coverage = None),
            "min_changed_line_coverage",
        ),
    ] {
        let mut c = claim("tests_pass", Verdict::Supported);
        mutate(&mut c);
        let d = example().evaluate(&subject(vec![c]));
        assert_eq!(
            d.conclusion,
            Conclusion::Neutral,
            "{want_rule}: {:?}",
            d.reasons
        );
        assert!(
            d.reasons.iter().any(|r| r.rule == want_rule),
            "{want_rule}: {:?}",
            d.reasons
        );
    }
    // `human_acked` belongs to a different claim kind in §9's example.
    let mut c = claim("test_file_changes", Verdict::Supported);
    c.human_acked = None;
    let d = example().evaluate(&subject(vec![c]));
    assert_eq!(d.conclusion, Conclusion::Neutral, "{:?}", d.reasons);
    assert!(d.reasons.iter().any(|r| r.rule == "require_human_ack"));
}

// ---------------------------------------------------------------------------
// §8.1's policy digest — the thing a receipt binds itself to
// ---------------------------------------------------------------------------

/// **The mutation experiment, as a unit test.** Every rule in the shipped
/// example, flipped one at a time, must move the digest. A digest that survives
/// a rule change is a digest bound to nothing — which is exactly what
/// `policy_hash: String::new()` was.
#[test]
fn flipping_any_single_rule_moves_the_digest() {
    let base = example();
    let base_digest = base.digest();

    let mut mutations: Vec<(&str, Policy)> = Vec::new();
    let mut p = base.clone();
    p.version += 1;
    mutations.push(("version", p));
    let mut p = base.clone();
    p.require_receipt = !p.require_receipt;
    mutations.push(("require_receipt", p));
    let mut p = base.clone();
    p.promises.block_on_open = !p.promises.block_on_open;
    mutations.push(("promises.block_on_open", p));
    let mut p = base.clone();
    p.promises.comment_open = !p.promises.comment_open;
    mutations.push(("promises.comment_open", p));
    let mut p = base.clone();
    p.stop_gate.enabled = !p.stop_gate.enabled;
    mutations.push(("stop_gate.enabled", p));
    let mut p = base.clone();
    p.stop_gate.max_stop_blocks += 1;
    mutations.push(("stop_gate.max_stop_blocks", p));
    let mut p = base.clone();
    p.reexec.mode = ReexecMode::Full;
    mutations.push(("reexec.mode", p));
    let mut p = base.clone();
    p.reexec.reruns += 1;
    mutations.push(("reexec.reruns", p));
    let mut p = base.clone();
    p.reexec.image = "python:3.12".into();
    mutations.push(("reexec.image", p));

    // Every field of every claim rule, too.
    for field in [
        "required_when_paths",
        "require_reexecuted",
        "require_evidence",
        "require_human_ack",
        "min_changed_line_coverage",
        "allow_skips",
    ] {
        let mut p = base.clone();
        let rule = p.claims.get_mut("tests_pass").expect("tests_pass rule");
        match field {
            "required_when_paths" => rule.required_when_paths = Some(vec!["docs/**".into()]),
            "require_reexecuted" => rule.require_reexecuted = !rule.require_reexecuted,
            "require_evidence" => rule.require_evidence = !rule.require_evidence,
            "require_human_ack" => rule.require_human_ack = !rule.require_human_ack,
            "min_changed_line_coverage" => rule.min_changed_line_coverage = Some(1),
            "allow_skips" => rule.allow_skips = !rule.allow_skips,
            _ => unreachable!(),
        }
        mutations.push((field, p));
    }
    // A rule REMOVED is a policy change too — this is the shape of the
    // "rewrite it to its opposite" attack that produced byte-identical
    // `Verified.` output on 2026-09-04.
    let mut p = base.clone();
    p.claims.remove("tests_pass");
    mutations.push(("claims.tests_pass removed", p));

    for (what, mutated) in mutations {
        assert_ne!(
            base_digest,
            mutated.digest(),
            "changing {what} left the policy digest unchanged"
        );
    }
}

#[test]
fn the_digest_is_stable_across_formatting_and_glob_order() {
    // The digest is over the PARSED rules, so a comment or an indentation
    // change must not move it — a false failure teaches people to ignore the
    // check.
    let a = Policy::parse(EXAMPLE).expect("example");
    let commented = format!("# a header comment\n{EXAMPLE}\n");
    let b = Policy::parse(&commented).expect("commented example");
    assert_eq!(a.digest(), b.digest());

    // Glob order inside one rule is not a rule change.
    let mut c = a.clone();
    let rule = c.claims.get_mut("tests_pass").expect("rule");
    let mut paths = rule.required_when_paths.clone().expect("paths");
    paths.reverse();
    rule.required_when_paths = Some(paths);
    assert_eq!(a.digest(), c.digest());
}

#[test]
fn absent_is_a_digest_and_it_is_not_the_default_policys_digest() {
    // "No policy file" is an answer, not a blank. The empty string was the old
    // representation, and the verifier read it as "no policy claim".
    let absent = absent_digest();
    assert!(absent.starts_with("sha256:"));
    assert_ne!(absent, "");
    // Deliberately distinct from a file that parses to the defaults: deleting
    // `.thruline/policy.yml` is precisely the mutation this binding exists to
    // catch, and reporting it as unchanged would be the same collapse one hop
    // away.
    assert_ne!(absent, Policy::default().digest());
}

#[test]
fn a_repository_resolves_a_digest_and_an_unparseable_policy_is_an_error() {
    let dir = std::env::temp_dir().join(format!("thruline-policy-digest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".thruline")).expect("mkdir");

    // No file → the absent digest, not an error and not a blank.
    let empty_repo = dir.join("nothing");
    std::fs::create_dir_all(&empty_repo).expect("mkdir");
    assert_eq!(
        digest_for_repo(&empty_repo).expect("absent resolves"),
        absent_digest()
    );

    // A real file → that policy's digest, resolved through the same path the
    // producer uses.
    std::fs::write(policy_path(&dir), EXAMPLE).expect("write");
    assert_eq!(
        digest_for_repo(&dir).expect("parses"),
        Policy::parse(EXAMPLE).expect("example").digest()
    );

    // A file that does not parse is an ERROR, never silently downgraded to
    // `absent_digest()` — a repository that intended to enforce rules must not
    // be able to issue a receipt saying it had none.
    std::fs::write(policy_path(&dir), "version: 1\nnonsense_key: 3\n").expect("write");
    let err = digest_for_repo(&dir).expect_err("an unparseable policy must not resolve");
    assert!(matches!(err, DigestError::Parse { .. }), "{err:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_canonical_rendering_mentions_every_field_of_the_policy() {
    // A field added to `Policy` and not added to `canonical` would drop out of
    // the digest silently — the guard that watches nothing. This is a literal
    // name census against the struct's own source.
    // Exercise optional blocks in their enabled form. Empty templates and
    // absent Decisions keep older policies' canonical digest unchanged.
    let text = Policy::parse(&format!(
        "{EXAMPLE}decisions:\ntemplates:\n  bug_fix:\n    lines: [\"Preserve the bytes.\"]\n"
    ))
    .expect("example with Decisions gate and a done template")
    .canonical();
    let src = include_str!("lib.rs");
    let mut missing = Vec::new();
    let mut in_struct = false;
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("pub struct Policy ")
            || t.starts_with("pub struct ClaimRule ")
            || t.starts_with("pub struct Promises ")
            || t.starts_with("pub struct StopGate ")
            || t.starts_with("pub struct Reexec ")
        {
            in_struct = true;
            continue;
        }
        if in_struct && t == "}" {
            in_struct = false;
            continue;
        }
        if !in_struct {
            continue;
        }
        if let Some(name) = t.strip_prefix("pub ").and_then(|r| r.split(':').next()) {
            if !text.contains(name) {
                missing.push(name.to_string());
            }
        }
    }
    assert!(
        missing.is_empty(),
        "these policy fields never reach the digest: {missing:?}\n{text}"
    );
}

// ---------------------------------------------------------------------------
// Track R, F9 — a determined-and-empty path set.
// ---------------------------------------------------------------------------

/// **`Some(vec![])` is not "the change touched nothing".**
///
/// An agent that commits before the session's last captured event leaves the
/// head tree and the final snapshot tree as the same git object, so
/// `git diff-tree` prints nothing. The diff was computed, so the path set is
/// `Some`; it is empty, so under a glob match no `required_when` rule can ever
/// apply — and every path-scoped rule in the file is silently skipped on
/// exactly the runs where the change set could not be seen.
#[test]
fn a_determined_but_empty_path_set_does_not_switch_a_rule_off() {
    let mut s = subject(vec![claim("tests_pass", Verdict::Unsupported)]);
    s.changed_paths = Some(Vec::new());
    let d = example().evaluate(&s);
    assert_eq!(
        d.conclusion,
        Conclusion::Failure,
        "an empty path set skipped a path-scoped rule: {:?}",
        d.reasons
    );
}

/// And an undetermined path set — either form of it — can never be a silent
/// green. The rule is evaluated *and* the reader is told the scope could not be
/// checked, because "this rule applied" and "this rule was in scope" are
/// different claims and only one of them is true here.
#[test]
fn an_undetermined_path_set_is_never_a_silent_green() {
    for undetermined in [None, Some(Vec::new())] {
        let mut s = subject(vec![claim("tests_pass", Verdict::Supported)]);
        s.changed_paths = undetermined.clone();
        let d = example().evaluate(&s);
        assert_eq!(
            d.conclusion,
            Conclusion::Neutral,
            "{undetermined:?} produced a green check: {:?}",
            d.reasons
        );
        assert!(
            d.reasons.iter().any(|r| r.rule == "required_when"),
            "nothing named the undetermined scope: {:?}",
            d.reasons
        );
    }
}

/// The control: a *determined, non-empty* path set that matches keeps passing,
/// so the neutral above is about the undetermined case and not about every run.
#[test]
fn a_determined_matching_path_set_still_passes_cleanly() {
    let d = example().evaluate(&subject(vec![claim("tests_pass", Verdict::Supported)]));
    assert_eq!(d.conclusion, Conclusion::Success, "{:?}", d.reasons);
    assert!(d.reasons.is_empty(), "{:?}", d.reasons);
}

// ---------------------------------------------------------------------------
// Track R, F11 — `max_stop_blocks` must be able to fire.
// ---------------------------------------------------------------------------

#[test]
fn a_stop_block_count_over_the_limit_fails() {
    let mut s = subject(vec![]);
    s.stop_blocks = Some(3);
    let d = example().evaluate(&s);
    assert_eq!(d.conclusion, Conclusion::Failure, "{:?}", d.reasons);
    assert!(d
        .reasons
        .iter()
        .any(|r| r.rule == "stop_gate.max_stop_blocks"));
}

/// **An unknown block count is not a count of zero**, and this is the whole of
/// F11: `stop_blocks` was a literal `0` in the caller, so the comparison could
/// not fire for any input and the rule read as coverage it never provided.
#[test]
fn an_unknown_stop_block_count_is_neutral_not_a_pass() {
    let mut s = subject(vec![]);
    s.stop_blocks = None;
    let d = example().evaluate(&s);
    assert_eq!(d.conclusion, Conclusion::Neutral, "{:?}", d.reasons);
    let r = d
        .reasons
        .iter()
        .find(|r| r.rule == "stop_gate.max_stop_blocks")
        .expect("the rule names itself");
    assert_eq!(r.conclusion, Conclusion::Neutral);
}

/// A disabled stop gate says nothing at all, either way.
#[test]
fn a_disabled_stop_gate_is_silent_on_an_unknown_count() {
    let mut p = example();
    p.stop_gate.enabled = false;
    let mut s = subject(vec![]);
    s.stop_blocks = None;
    let d = p.evaluate(&s);
    assert!(
        !d.reasons
            .iter()
            .any(|r| r.rule == "stop_gate.max_stop_blocks"),
        "{:?}",
        d.reasons
    );
}
