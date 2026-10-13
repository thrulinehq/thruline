use super::*;
use crate::extract::Source;

fn src(text: &str) -> Vec<Source> {
    vec![Source {
        event_id: "evt_01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
        session_id: "s1".to_string(),
        text: text.to_string(),
    }]
}

fn kinds(text: &str) -> Vec<ClaimKind> {
    extract(&src(text), &Table::default())
        .claims
        .into_iter()
        .map(|c| c.kind)
        .collect()
}

// ---------------------------------------------------------------------------
// §5.2 minimum coverage: a fixture per phrase family
// ---------------------------------------------------------------------------

#[test]
fn every_phrase_family_in_the_spec_s_minimum_list_is_covered() {
    for (text, want) in [
        ("All tests pass.", ClaimKind::TestsPass),
        ("The test suite passes.", ClaimKind::TestsPass),
        ("Tests pass.", ClaimKind::TestsPass),
        ("Everything is all green now.", ClaimKind::TestsPass),
        ("The build succeeds.", ClaimKind::BuildOk),
        ("It compiles.", ClaimKind::BuildOk),
        ("Lint clean.", ClaimKind::LintOk),
        ("No clippy warnings.", ClaimKind::LintOk),
        ("It type checks.", ClaimKind::TypecheckOk),
        ("mypy clean.", ClaimKind::TypecheckOk),
        ("This is purely a refactor.", ClaimKind::BehaviorUnchanged),
        ("No functional changes.", ClaimKind::BehaviorUnchanged),
        ("Added a migration.", ClaimKind::MigrationAdded),
        ("Updated the docs.", ClaimKind::DocsUpdated),
        ("Added a dependency.", ClaimKind::DependencyAdded),
        ("No secrets committed.", ClaimKind::SecretsAbsent),
        ("Ready to merge.", ClaimKind::TaskComplete),
    ] {
        assert!(
            kinds(text).contains(&want),
            "{text:?} did not yield {:?}",
            want.as_str()
        );
    }
}

// ---------------------------------------------------------------------------
// Negation: the ruling C.9 moved here from §6.9
// ---------------------------------------------------------------------------

#[test]
fn the_case_ruling_c9_moved_here_yields_no_claim() {
    // "I failed to make all tests pass" is the sentence §6.9 reached for. It is
    // a negation bug in the extractor, and fixing it HERE changes whether a
    // claim exists; fixing it at verdict would have changed whether an existing
    // claim's failure counts — which is prose excusing missing evidence.
    let e = extract(&src("I failed to make all tests pass."), &Table::default());
    assert!(
        e.claims.is_empty(),
        "an honest failure produced a claim: {:?}",
        e.claims
    );
    // And it is COUNTED, not silently dropped: "found nothing" and "found
    // something and declined it" are different facts.
    assert_eq!(e.negated.len(), 1);
    assert_eq!(e.negated[0].1, ClaimKind::TestsPass);
}

#[test]
fn a_negation_counter_fixture_for_every_claim_kind_the_table_carries() {
    for (text, kind) in [
        ("Tests do not pass.", ClaimKind::TestsPass),
        ("I couldn't get the tests to pass.", ClaimKind::TestsPass),
        ("Not all tests pass.", ClaimKind::TestsPass),
        (
            "I skipped tests, so I can't say all tests pass.",
            ClaimKind::TestsPass,
        ),
        ("I can't yet say the build succeeds.", ClaimKind::BuildOk),
        (
            "It is not true that the build succeeds.",
            ClaimKind::BuildOk,
        ),
        ("Lint is not clean.", ClaimKind::LintOk),
        ("mypy is not clean.", ClaimKind::TypecheckOk),
        (
            "This is not purely a refactor.",
            ClaimKind::BehaviorUnchanged,
        ),
        ("I have not updated the docs.", ClaimKind::DocsUpdated),
        ("This is not ready to merge.", ClaimKind::TaskComplete),
    ] {
        let got = kinds(text);
        assert!(
            !got.contains(&kind),
            "{text:?} yielded {:?}, which it denies",
            kind.as_str()
        );
    }
}

#[test]
fn post_phrase_negation_is_a_known_limit_and_is_recorded_as_one() {
    // A phrase matcher can see the clause BEFORE a phrase; it cannot see a
    // denial that arrives after one. "All tests pass" followed by ", or they
    // would if the fixture loaded" still yields a claim.
    //
    // Asserted rather than left implicit, because the alternative is a reader
    // assuming a coverage this extractor does not have. The exposure is small
    // and the direction is the honest one: the claim IS extracted, so it goes
    // to a verifier, and a verifier that finds no evidence says `unsupported`.
    // A missed extraction would instead be silence.
    let e = extract(
        &src("All tests pass, or they would if the fixture loaded."),
        &Table::default(),
    );
    assert_eq!(
        e.claims.len(),
        1,
        "post-phrase negation is now handled; update this test and the report"
    );
}

#[test]
fn negation_does_not_reach_across_a_clause_boundary() {
    // The precision half. A cue anywhere in the sentence would make this
    // negated and drop a real claim — which is the direction that hides an
    // overclaim.
    for text in [
        "I fixed the failing test and now all tests pass.",
        "The build was broken, but the build succeeds now.",
        "There were no tests before; now all tests pass.",
        "Two tests failed at first, then all tests pass.",
    ] {
        assert!(
            !kinds(text).is_empty(),
            "{text:?} was wrongly treated as negated"
        );
    }
}

#[test]
fn a_future_tense_sentence_is_not_a_claim_about_the_present() {
    for text in [
        "I will make sure all tests pass.",
        "Next I need to check that all tests pass.",
        "Please verify that all tests pass.",
        "If all tests pass, we can merge.",
    ] {
        assert!(
            kinds(text).is_empty(),
            "{text:?} was read as a completed claim"
        );
    }
}

// ---------------------------------------------------------------------------
// Spans, scope, ids
// ---------------------------------------------------------------------------

#[test]
fn a_claim_carries_the_exact_span_of_the_sentence_it_came_from() {
    let text = "I updated the rate function. All tests pass. Shipping it.";
    let e = extract(&src(text), &Table::default());
    let claim = e
        .claims
        .iter()
        .find(|c| c.kind == ClaimKind::TestsPass)
        .expect("a claim");
    assert_eq!(claim.text, "All tests pass.");
    let (start, end) = claim.span.offset;
    assert_eq!(&text[start..end], "All tests pass");
    assert_eq!(claim.span.event_id, "evt_01ARZ3NDEKTSV4RRFFQ69G5FAV");
}

#[test]
fn an_unqualified_claim_is_scoped_to_everything_and_a_qualified_one_is_not() {
    // The distinction §6.3 turns on. Reading an unqualified sentence as
    // `Named` would quietly make a narrowed run sufficient for it.
    let all = extract(&src("All tests pass."), &Table::default());
    assert_eq!(all.claims[0].args.scope, Scope::All);

    let named = extract(&src("That test passes now."), &Table::default());
    assert_eq!(
        named.claims[0].args.scope,
        Scope::Named,
        "a qualified sentence was read as a claim about the whole suite"
    );
}

#[test]
fn extraction_is_deterministic_including_the_ids() {
    let a = extract(&src("All tests pass."), &Table::default());
    let b = extract(&src("All tests pass."), &Table::default());
    assert_eq!(a, b);
    assert!(a.claims[0].id.starts_with("clm_"));
}

#[test]
fn one_sentence_yields_at_most_one_claim_per_kind() {
    // The table has several `tests_pass` phrases and a sentence can contain
    // two of them; two rows for one sentence would double-count in any report.
    let e = extract(
        &src("All tests pass and the test suite passes."),
        &Table::default(),
    );
    let n = e
        .claims
        .iter()
        .filter(|c| c.kind == ClaimKind::TestsPass)
        .count();
    assert_eq!(n, 1, "{:?}", e.claims);
}

// ---------------------------------------------------------------------------
// The type boundary (§0.1 / ruling R3)
// ---------------------------------------------------------------------------

#[test]
fn the_projection_verifiers_receive_cannot_carry_prose_or_provenance() {
    let e = extract(&src("All tests pass."), &Table::default());
    let claim = &e.claims[0];
    let v = claim.verifiable();
    assert_eq!(v.kind, claim.kind);
    assert_eq!(v.args, claim.args);

    // The proof is structural, and it is checked as text because a field that
    // does not exist cannot be asserted about any other way: the serialised
    // projection has exactly three keys, and none of them is prose.
    let json = serde_json::to_value(&v).expect("serialise");
    let keys: Vec<&str> = json
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(keys, vec!["args", "id", "kind"]);
    for forbidden in ["text", "span", "extractor", "confidence"] {
        assert!(
            json.get(forbidden).is_none(),
            "the verdict-path projection carries {forbidden}"
        );
    }
}

#[test]
fn a_claim_has_no_confidence_field_at_all() {
    // Ruling R3. Not "defaults to 1.0" — absent, so nothing can threshold it.
    let e = extract(&src("All tests pass."), &Table::default());
    let json = serde_json::to_value(&e.claims[0]).expect("serialise");
    assert!(json.get("confidence").is_none());
    assert_eq!(json["extractor"], serde_json::json!("rules"));
}

// ---------------------------------------------------------------------------
// The table itself
// ---------------------------------------------------------------------------

#[test]
fn the_built_in_table_parses_and_is_ordered() {
    let t = Table::parse(rules::DEFAULT_TABLE, "<built-in>").expect("the built-in table parses");
    assert!(t.patterns().len() > 30, "{}", t.patterns().len());
    // Order is meaningful: the longer, more specific `tests_pass` phrase must
    // come before the shorter one it contains, or the shorter always wins and
    // the span is wrong.
    let phrases: Vec<&str> = t
        .patterns()
        .iter()
        .filter(|p| p.kind == ClaimKind::TestsPass)
        .map(|p| p.phrase.as_str())
        .collect();
    let all_at = phrases.iter().position(|p| *p == "all tests pass");
    let short_at = phrases.iter().position(|p| *p == "tests pass");
    assert!(all_at < short_at, "{phrases:?}");
}

#[test]
fn a_malformed_override_is_an_error_and_never_a_silent_fallback() {
    // The reason someone writes this file is that the built-in table misses
    // something of theirs. Ignoring it quietly drops exactly the claims they
    // asked to have caught.
    for (bad, why) in [
        ("version: 2\npatterns:\n  - kind: tests_pass\n", "version"),
        (
            "patterns:\n  - kind: tests_pass\n    phrases:\n      - x\n",
            "no version",
        ),
        (
            "version: 1\npatterns:\n  - kind: nonsense\n",
            "unknown kind",
        ),
        ("version: 1\npatterns:\n", "empty"),
        (
            "version: 1\npatterns:\n  - kind: tests_pass\n    phrases:\n      - \"\"\n",
            "empty phrase",
        ),
    ] {
        assert!(
            Table::parse(bad, "t.yml").is_err(),
            "a {why} table parsed instead of erroring"
        );
    }
}

#[test]
fn an_override_table_replaces_the_built_in_one() {
    let t = Table::parse(
        "version: 1\npatterns:\n  - kind: tests_pass\n    scope: all\n    phrases:\n      - \"la suite passe\"\n",
        "t.yml",
    )
    .expect("parse");
    let e = extract(&src("La suite passe."), &t);
    assert_eq!(e.claims.len(), 1);
    assert_eq!(e.claims[0].kind, ClaimKind::TestsPass);
}

// ---------------------------------------------------------------------------
// Promises (§3.5 / §5.2)
// ---------------------------------------------------------------------------

#[test]
fn the_promise_cues_the_spec_lists_are_all_recognised() {
    for (text, kind) in [
        ("TODO: wire up the retry path.", PromiseKind::Todo),
        (
            "I'll add integration tests in a follow-up.",
            PromiseKind::FollowUp,
        ),
        (
            "This should be added in a future PR.",
            PromiseKind::FollowUp,
        ),
        ("I will refactor the parser later.", PromiseKind::Deferred),
        (
            "Error handling is left as an exercise.",
            PromiseKind::Deferred,
        ),
        ("The migration is not yet written.", PromiseKind::Deferred),
        ("I assumed the ids are unique.", PromiseKind::Assumption),
    ] {
        let e = extract(&src(text), &Table::default());
        assert_eq!(e.promises.len(), 1, "{text:?} → {:?}", e.promises);
        assert_eq!(e.promises[0].kind, kind, "{text:?}");
        assert_eq!(e.promises[0].status, PromiseStatus::Open);
    }
}

#[test]
fn a_sentence_with_several_promise_cues_is_one_promise() {
    let e = extract(
        &src("I'll follow up on the remaining cases later."),
        &Table::default(),
    );
    assert_eq!(e.promises.len(), 1, "{:?}", e.promises);
}

#[test]
fn a_claim_and_a_promise_can_come_from_one_message_without_interfering() {
    let e = extract(
        &src("All tests pass. I'll add a regression test in a follow-up."),
        &Table::default(),
    );
    assert!(e.claims.iter().any(|c| c.kind == ClaimKind::TestsPass));
    assert_eq!(e.promises.len(), 1);
    assert!(e.promises[0].text.contains("follow-up"));
}

#[test]
fn sentence_splitting_does_not_break_a_decimal_or_a_version() {
    let e = extract(
        &src("Bumped to 1.2.3 and all tests pass."),
        &Table::default(),
    );
    assert_eq!(e.claims.len(), 1, "{:?}", e.claims);
    assert!(e.claims[0].text.contains("1.2.3"));
}
