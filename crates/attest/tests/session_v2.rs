//! D22 protocol/presentation cases. Integrity is controlled explicitly here;
//! the CLI integration suite separately exercises signature and local custody.
use thruline_attest::report::PolicyConclusion;
use thruline_attest::session_v2::{self, ClaimOrigin, Extractor, SessionStatement};
use thruline_attest::statement::*;
use thruline_attest::verify::{Check, Outcome, Report};

fn session(verdict: &str, extractor: Extractor, degraded: bool) -> SessionStatement {
    let claim = ClaimEntry {
        id: "clm_1".into(),
        kind: "tests_pass".into(),
        text: "original text omitted".into(),
        asserted_by: AssertedBy {
            harness: "test-fixture".into(),
            model: None,
        },
    };
    let statement = Statement::new(
        vec![Subject {
            name: "fixture".into(),
            digest: [("gitTree".into(), "a".repeat(40))].into(),
        }],
        Predicate {
            session: Session {
                id: "fixture-session".into(),
                event_chain_head: format!("sha256:{}", "b".repeat(64)),
                ..Default::default()
            },
            claims: vec![claim],
            checks: vec![CheckEntry {
                id: "check-1".into(),
                category: "test".into(),
                runner: "pytest".into(),
                argv_hash: "c".repeat(64),
                env_hash: "d".repeat(64),
                cwd_hash: "e".repeat(64),
                input_tree_hash: "a".repeat(40),
                exit_code: 0,
                parsed: serde_json::json!({"passed":1,"failed":0}),
                reexecuted: false,
                container_image_digest: None,
            }],
            verdicts: vec![VerdictEntry {
                claim_id: "clm_1".into(),
                verdict: verdict.into(),
                verifier: "tests_pass".into(),
                evidence: vec!["check-1".into()],
                reasons: vec![],
                warnings: vec![],
                findings: vec![],
            }],
            capture: Capture {
                capture_degraded: degraded,
                degraded_causes: if degraded {
                    vec!["missing_stop: capture incomplete".into()]
                } else {
                    vec![]
                },
                ..Default::default()
            },
            ..Default::default()
        },
    );
    SessionStatement::new(
        statement,
        vec![ClaimOrigin {
            claim_id: "clm_1".into(),
            extractor,
        }],
    )
    .unwrap()
}

fn integrity(outcome: Outcome) -> Report {
    Report {
        checks: vec![Check {
            name: "explicit integrity fixture".into(),
            outcome,
        }],
    }
}
fn complete() -> Report {
    integrity(Outcome::Pass {
        detail: "fixture only".into(),
    })
}

#[test]
fn d22_rules_unsupported_requires_action_and_supported_control_can_be_green() {
    let unsupported = session("unsupported", Extractor::Rules, false);
    let body = session_v2::presentation(&complete(), &unsupported, None);
    assert_eq!(body["conclusion"], "action_required");
    assert_eq!(body["ground"], "rules_unsupported");
    assert_eq!(body["verified"], false);
    assert_eq!(
        session_v2::presentation(
            &complete(),
            &session("supported", Extractor::Rules, false),
            None
        )["conclusion"],
        "success"
    );
}

#[test]
fn model_origin_never_blocks_even_under_explicit_policy_failure() {
    for verdict in ["supported", "unsupported", "unverifiable"] {
        let record = session(verdict, Extractor::LocalModel, false);
        for policy in [
            None,
            Some(PolicyConclusion::Success),
            Some(PolicyConclusion::Failure),
        ] {
            let body = session_v2::presentation(&complete(), &record, policy);
            assert!(!["failure", "action_required"].contains(&body["conclusion"].as_str().unwrap()));
        }
    }
}

#[test]
fn degraded_and_incomplete_integrity_have_a_neutral_ceiling_and_cause_title() {
    let record = session("unsupported", Extractor::Rules, true);
    let body = session_v2::presentation(&complete(), &record, None);
    assert_eq!(body["conclusion"], "neutral");
    // R8-VOCABULARY: a degraded capture is a recording gap, so its word is
    // Fog, not Recorded. The conclusion ceiling above is unchanged.
    assert_eq!(body["badge_label"], "Fog");
    assert!(body["cause_title"].as_str().unwrap().contains("capture"));
    let report = integrity(Outcome::Skipped {
        reason: "missing local history".into(),
    });
    assert_eq!(
        session_v2::presentation(
            &report,
            &session("unsupported", Extractor::Rules, false),
            None
        )["conclusion"],
        "neutral"
    );
}

#[test]
fn new_decoder_rejects_missing_provenance_ground_and_required_accounting() {
    let value = serde_json::to_value(session("supported", Extractor::Rules, false)).unwrap();
    assert!(session_v2::decode(&serde_json::to_vec(&value).unwrap()).is_ok());
    for mutation in [
        "missing-origin",
        "duplicate-origin",
        "unknown-origin",
        "false-ground",
        "unknown-field",
        "missing-verdict",
        "duplicate-claim",
    ] {
        let mut v = value.clone();
        match mutation {
            "missing-origin" => v["predicate"]["claim_origins"] = serde_json::json!([]),
            "duplicate-origin" => {
                let origin = v["predicate"]["claim_origins"][0].clone();
                v["predicate"]["claim_origins"]
                    .as_array_mut()
                    .unwrap()
                    .push(origin);
            }
            "unknown-origin" => {
                v["predicate"]["claim_origins"][0]["extractor"] = serde_json::json!("trusted-model")
            }
            "false-ground" => v["predicate"]["ground"] = serde_json::json!("rules_unsupported"),
            "unknown-field" => v["predicate"]["capture"]["trusted"] = serde_json::json!(true),
            "missing-verdict" => v["predicate"]["verdicts"] = serde_json::json!([]),
            "duplicate-claim" => {
                let claim = v["predicate"]["claims"][0].clone();
                v["predicate"]["claims"].as_array_mut().unwrap().push(claim);
            }
            _ => unreachable!(),
        }
        assert!(
            session_v2::decode(&serde_json::to_vec(&v).unwrap()).is_err(),
            "{mutation}"
        );
    }
}
