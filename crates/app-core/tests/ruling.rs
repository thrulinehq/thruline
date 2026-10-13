//! The ruling, branch by branch, and the parsers that feed it.
//!
//! `services/github-app/selftest.py` observes the ruling end to end over really
//! signed receipts, which is the stronger evidence and is also slow, needs a
//! built CLI, and covers the branches it happens to reach. This covers the
//! matrix — including the combinations a receipt fixture cannot easily be made
//! to produce, like a policy that *fails* over an `unsupported` claim.

use thruline_app_core::fetch;
use thruline_app_core::ruling::{decide, Ground};
use thruline_app_core::verify_json::{self, Claims, VerifyJson};
use thruline_policy::{Conclusion, Decision, Reason};

fn receipt(conclusion: Conclusion, unsupported: u64, unverifiable: u64) -> VerifyJson {
    let total = 1 + unsupported + unverifiable;
    VerifyJson {
        conclusion,
        integrity: "verified".to_string(),
        claims: Claims {
            total,
            supported: 1,
            unsupported,
            unverifiable,
            other: 0,
            all_supported: unsupported == 0 && unverifiable == 0,
            unsupported_claim_ids: (0..unsupported).map(|i| format!("clm_u{i}")).collect(),
            unverifiable_claim_ids: (0..unverifiable).map(|i| format!("clm_v{i}")).collect(),
        },
        checks: Vec::new(),
    }
}

fn policy(conclusion: Conclusion) -> Decision {
    Decision {
        conclusion,
        reasons: vec![Reason {
            claim: Some("tests_pass".to_string()),
            rule: "required_when",
            detail: "a rule fired".to_string(),
            conclusion,
        }],
        advisory: Default::default(),
    }
}

// ---------------------------------------------------------------------------
// "without one, any unsupported claim yields neutral … never success"
// ---------------------------------------------------------------------------

#[test]
fn without_a_policy_an_unproved_claim_is_neutral_and_never_success() {
    for (unsupported, unverifiable) in [(1, 0), (0, 1), (2, 3)] {
        let r = receipt(Conclusion::Neutral, unsupported, unverifiable);
        let out = decide(Some(&r), None, false);
        assert_eq!(
            out.conclusion,
            Conclusion::Neutral,
            "{unsupported} unsupported / {unverifiable} unverifiable"
        );
        assert_eq!(out.ground, Ground::UnprovedNoPolicy);
        assert!(out.carried, "the conclusion must be the verifier's word");
    }
}

/// **The clause's exact words**, as an assertion over every combination the
/// claim axis can be in with no policy present.
#[test]
fn the_claim_axis_alone_can_never_produce_failure() {
    for unsupported in 0..3u64 {
        for unverifiable in 0..3u64 {
            let carried = if unsupported + unverifiable > 0 {
                Conclusion::Neutral
            } else {
                Conclusion::Success
            };
            let r = receipt(carried, unsupported, unverifiable);
            let out = decide(Some(&r), None, false);
            assert_ne!(
                out.conclusion,
                Conclusion::Failure,
                "an unproved claim produced `failure` with no policy to require it. \
                 Deciding that an unproved claim may not merge is the repository's \
                 call, written in .thruline/policy.yml."
            );
        }
    }
}

// ---------------------------------------------------------------------------
// "with a policy the policy decides" — downward only
// ---------------------------------------------------------------------------

#[test]
fn a_policy_can_lower_the_conclusion_and_never_raise_it() {
    // Failing policy over a clean receipt: failure.
    let clean = receipt(Conclusion::Success, 0, 0);
    let out = decide(Some(&clean), Some(&policy(Conclusion::Failure)), false);
    assert_eq!(out.conclusion, Conclusion::Failure);
    assert_eq!(out.ground, Ground::PolicyRefused);

    // Passing policy over an unproved claim: **still neutral**. The policy said
    // it may merge, which is a different sentence from "it was proved".
    let unproved = receipt(Conclusion::Neutral, 1, 0);
    let out = decide(Some(&unproved), Some(&policy(Conclusion::Success)), false);
    assert_eq!(
        out.conclusion,
        Conclusion::Neutral,
        "a passing policy greened an unproved claim"
    );
    assert_eq!(out.ground, Ground::UnprovedPolicyAllows);

    // Passing policy over a clean receipt: the one green path.
    let out = decide(Some(&clean), Some(&policy(Conclusion::Success)), false);
    assert_eq!(out.conclusion, Conclusion::Success);
    assert_eq!(out.ground, Ground::Verified);

    // A policy that could not decide is not a policy that passed.
    let out = decide(Some(&clean), Some(&policy(Conclusion::Neutral)), false);
    assert_eq!(out.conclusion, Conclusion::Neutral);
    assert_eq!(out.ground, Ground::PolicyUndecided);
}

/// A contradicted receipt is `failure` from the integrity axis, and **no policy
/// can lift it**. The policy was written about claims and knows nothing about
/// whether the receipt is even about this repository.
#[test]
fn a_contradicted_receipt_cannot_be_greened_by_any_policy() {
    let bad = receipt(Conclusion::Failure, 0, 0);
    for p in [
        None,
        Some(policy(Conclusion::Success)),
        Some(policy(Conclusion::Neutral)),
        Some(policy(Conclusion::Failure)),
    ] {
        let out = decide(Some(&bad), p.as_ref(), false);
        assert_eq!(out.conclusion, Conclusion::Failure);
        assert_eq!(out.ground, Ground::ReceiptContradicted);
    }
}

// ---------------------------------------------------------------------------
// no receipt
// ---------------------------------------------------------------------------

#[test]
fn an_absent_receipt_blocks_only_when_the_repository_asked_it_to() {
    let out = decide(None, None, false);
    assert_eq!(out.conclusion, Conclusion::Neutral);
    assert_eq!(out.ground, Ground::NoReceipt);
    assert!(!out.carried, "there was no verifier to carry a word from");

    let out = decide(None, Some(&policy(Conclusion::Failure)), true);
    assert_eq!(out.conclusion, Conclusion::Failure);
    assert_eq!(out.ground, Ground::NoReceiptRequired);
}

// ---------------------------------------------------------------------------
// verify --json
// ---------------------------------------------------------------------------

/// The App refuses its own verifier's output rather than defaulting.
#[test]
fn an_unreadable_verification_is_refused_and_never_treated_as_neutral() {
    for bad in [
        &b"not json"[..],
        br#"{"integrity":{"status":"verified"},"claims":{}}"#,
        br#"{"conclusion":"probably fine","integrity":{"status":"x"},"claims":{}}"#,
        br#"{"conclusion":"neutral","claims":{"total":0}}"#,
    ] {
        assert!(
            verify_json::parse(bad).is_err(),
            "accepted unreadable verifier output: {}",
            String::from_utf8_lossy(bad)
        );
    }
}

/// The App-side half of the one rule: `success` beside an unproved claim is
/// refused **even though the core produced it**.
#[test]
fn a_self_contradictory_report_is_refused() {
    let body = br#"{"conclusion":"success","integrity":{"status":"verified"},
        "claims":{"total":1,"supported":0,"unsupported":1,"unverifiable":0,"other":0,
                  "all_supported":false,"unsupported_claim_ids":["clm_1"],
                  "unverifiable_claim_ids":[]}}"#;
    assert!(verify_json::parse(body).is_err());
}

/// And a receipt that claims nothing is a legitimate `success`. Keying the
/// contradiction check on `!all_supported` — which is `false` for a claimless
/// receipt — would have refused every one of them, and read as the guard
/// working.
#[test]
fn a_receipt_with_no_claims_is_not_a_contradiction() {
    let body = br#"{"conclusion":"success","integrity":{"status":"verified"},
        "claims":{"total":0,"supported":0,"unsupported":0,"unverifiable":0,"other":0,
                  "all_supported":false,"unsupported_claim_ids":[],
                  "unverifiable_claim_ids":[]}}"#;
    let v = verify_json::parse(body).expect("a claimless receipt must parse");
    assert_eq!(v.conclusion, Conclusion::Success);
    assert!(!v.claims.anything_unproved());
}

/// The verifier is told there is no ledger here. Without it, a host that
/// happens to have one reports `chain_head: fail` for every receipt from
/// another machine — `failure`, on a pull request, about a genuine receipt.
#[test]
fn the_verifier_is_always_pointed_at_an_absent_ledger() {
    let args = verify_json::argv("/tmp/r.json", "/tmp/repo", "/tmp/none.db", None);
    let at = args.iter().position(|a| a == "--exec-db").expect(
        "the App must pass --exec-db, or it inherits the host's ledger and accuses \
         every receipt it did not produce",
    );
    assert_eq!(args.get(at + 1).map(String::as_str), Some("/tmp/none.db"));
    assert!(args.contains(&"--offline".to_string()));
    assert!(args.contains(&"json".to_string()));
}

// ---------------------------------------------------------------------------
// the parsers
// ---------------------------------------------------------------------------

#[test]
fn base64_round_trips_including_the_wrapping_github_applies() {
    for raw in [
        &b""[..],
        b"a",
        b"ab",
        b"abc",
        b"abcd",
        b"{\"payloadType\":\"application/vnd.in-toto+json\"}",
    ] {
        let mut enc = String::new();
        {
            // Reference encoder, so the test does not rely on the decoder to
            // check itself.
            const T: &[u8; 64] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            for c in raw.chunks(3) {
                let b = (u32::from(c[0]) << 16)
                    | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
                    | u32::from(*c.get(2).unwrap_or(&0));
                enc.push(T[(b >> 18) as usize & 63] as char);
                enc.push(T[(b >> 12) as usize & 63] as char);
                enc.push(if c.len() > 1 {
                    T[(b >> 6) as usize & 63] as char
                } else {
                    '='
                });
                enc.push(if c.len() > 2 {
                    T[b as usize & 63] as char
                } else {
                    '='
                });
            }
        }
        assert_eq!(fetch::base64_decode(&enc).expect("decode"), raw, "{enc}");
        // GitHub's contents API wraps at 60 columns.
        let wrapped: String = enc
            .as_bytes()
            .chunks(60)
            .map(|c| String::from_utf8_lossy(c).to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(fetch::base64_decode(&wrapped).expect("wrapped"), raw);
    }
}

/// **Refuses rather than skipping.** A decoder that silently drops what it does
/// not understand turns a corrupted receipt into a different, well-formed one.
#[test]
fn base64_refuses_malformed_input() {
    for bad in ["a", "ab!cd", "abcde", "====", "ab=c", "a=bcd"] {
        assert!(
            fetch::base64_decode(bad).is_err(),
            "accepted malformed base64 {bad:?}"
        );
    }
}

#[test]
fn a_receipt_path_that_could_escape_the_checkout_is_never_requested() {
    let body = br#"[
      {"filename":".thruline/receipts/../../.github/workflows/x.json","status":"modified"},
      {"filename":".thruline/receipts/../../../etc/passwd.json","status":"modified"},
      {"filename":"/etc/passwd","status":"modified"},
      {"filename":".thruline/receipts/nested/deep.json","status":"modified"},
      {"filename":".thruline/receipts/.hidden.json","status":"modified"},
      {"filename":".thruline/receipts/gone.json","status":"removed"},
      {"filename":".thruline/receipts/ok.json","status":"modified"}
    ]"#;
    let paths = fetch::parse_pr_files(body).expect("parse");
    assert_eq!(paths, vec![".thruline/receipts/ok.json".to_string()]);
    // A receipt this pull request DELETES is not a receipt it has.
    assert!(!paths.iter().any(|p| p.contains("gone")));
}

#[test]
fn a_comment_attachment_is_only_read_behind_its_marker_and_only_if_it_is_json() {
    let no_marker = br#"[{"id":1,"body":"```json\n{\"a\":1}\n```"}]"#;
    assert_eq!(
        fetch::parse_comment_attachment(no_marker).expect("parse"),
        None,
        "a fenced block without the marker became a candidate receipt; a reviewer \
         pasting an example would change what the gate verifies"
    );

    let not_json = br#"[{"id":1,"body":"<!-- thruline-receipt -->\n```json\nnot json\n```"}]"#;
    assert_eq!(
        fetch::parse_comment_attachment(not_json).expect("parse"),
        None
    );

    let good = br#"[{"id":1,"body":"x"},
                    {"id":2,"body":"<!-- thruline-receipt -->\n```json\n{\"payload\":\"e30\"}\n```"}]"#;
    let got = fetch::parse_comment_attachment(good)
        .expect("parse")
        .expect("some");
    assert_eq!(String::from_utf8_lossy(&got), r#"{"payload":"e30"}"#);
}

#[test]
fn the_request_plans_are_the_paths_github_documents() {
    assert_eq!(
        fetch::pr_files("acme/widgets", 7).path,
        "/repos/acme/widgets/pulls/7/files?per_page=100"
    );
    assert_eq!(
        fetch::contents("acme/widgets", ".thruline/policy.yml", "abc").path,
        "/repos/acme/widgets/contents/.thruline/policy.yml?ref=abc"
    );
    assert_eq!(
        fetch::installation_token(12345).path,
        "/app/installations/12345/access_tokens"
    );
    assert_eq!(
        fetch::post_check_run("a/b", serde_json::json!({})).method,
        "POST"
    );
    assert_eq!(fetch::patch_comment("a/b", 9, "hi").method, "PATCH");
}

#[test]
fn a_response_over_the_cap_is_refused_rather_than_allocated() {
    let big = "A".repeat(4 * 1024 * 1024);
    let body = format!(r#"{{"encoding":"base64","content":"{big}"}}"#);
    assert!(matches!(
        fetch::parse_contents(body.as_bytes()),
        Err(fetch::FetchError::TooLarge(_, _))
    ));
}

// ---------------------------------------------------------------------------
// D21 — one badge vocabulary
// ---------------------------------------------------------------------------

/// The three shared states must read exactly as `attest::report::badge_label`
/// spells them, word and colour.
///
/// # Why this is a pinned table and not a call
///
/// The right shape is for `app-core` to **call** the core's `badge_label`, so
/// there is one definition rather than two that happen to agree. That symbol
/// does not exist at this branch's base — it arrives with the core's merge —
/// so the call cannot compile here and the de-duplication is a step in the
/// merge resolution, written out in `docs/tracks/C/INTERFACE-REQUESTS.md` R6.
///
/// Until then this is the next best thing: the values are pinned against the
/// core's table copied in as literals, so **this side cannot drift** while the
/// two definitions coexist. It is deliberately not a substitute for the call,
/// and R6 says so.
///
/// # The divergence this closes, which was real
///
/// `app-core` used to spell `Neutral` as "not verified" and `Failure` as
/// "unsupported", while `attest` spelled `Neutral` "partial" and `Failure`
/// "not verified". Two badges for one product, colliding on the one phrase a
/// reader would then have to disambiguate — and `Success` had two different
/// greens. Fixed on this branch before D21 was filed; this keeps it fixed.
#[test]
fn the_badge_vocabulary_matches_the_cores_table_exactly() {
    use thruline_app_core::badge::{label_and_colour, State};

    // Copied from `attest::report::badge_label` at core `fad0e00`. If the core
    // changes a word or a hex value, this fails and names which — which is the
    // whole point of copying it in rather than paraphrasing it.
    for (state, want) in [
        (State::Known(Conclusion::Success), ("verified", "#2ea043")),
        (State::Known(Conclusion::Neutral), ("partial", "#bf8700")),
        (
            State::Known(Conclusion::Failure),
            ("not verified", "#cf222e"),
        ),
    ] {
        assert_eq!(
            label_and_colour(state),
            want,
            "{state:?} disagrees with attest::report::badge_label"
        );
    }

    // The fourth state is app-core's alone and has no counterpart in the core's
    // table: a hosted badge can be asked about a ref this App has never
    // concluded anything for, and "no record" is a different sentence from
    // "something went unproved". Grey, not amber, so an unconfigured repository
    // cannot be mistaken for a partially verified one.
    assert_eq!(label_and_colour(State::Unknown), ("no receipt", "#6e7781"));

    // And the one rule that outranks the vocabulary: exactly one state says
    // "verified", whatever the words become.
    let verified = [
        State::Known(Conclusion::Success),
        State::Known(Conclusion::Neutral),
        State::Known(Conclusion::Failure),
        State::Unknown,
    ]
    .into_iter()
    .filter(|s| label_and_colour(*s).0 == "verified")
    .count();
    assert_eq!(verified, 1, "more than one badge state claims verification");
}

/// The badge guard's needle must still find what the badge renders.
///
/// `respond` refuses to serve a badge when the state is not `success` and the
/// rendered bytes contain `">verified<"` — a real branch, deliberately not a
/// `debug_assert!`, because that compiles out of the build that gets deployed.
/// But the guard matches **rendered markup** while the test above checks
/// `label_and_colour`, and those are two representations. Put an attribute
/// between the `>` and the text, escape the label, split the tspan — the needle
/// stops matching, the branch stops firing, and **the guard becomes a no-op that
/// still reads as present in the source.** That is the shape this session has
/// found five times, so the link gets a test rather than an argument.
///
/// This asserts the biconditional, not just one side: exactly the success state
/// renders the needle. One direction would let a build where NO state renders it
/// pass — which is the silent no-op itself.
#[test]
fn the_badge_guards_needle_matches_what_the_badge_actually_renders() {
    use thruline_app_core::badge::{svg, State};

    for state in [
        State::Known(Conclusion::Success),
        State::Known(Conclusion::Neutral),
        State::Known(Conclusion::Failure),
        State::Unknown,
    ] {
        let rendered = svg(state).contains(">verified<");
        let is_success = state == State::Known(Conclusion::Success);
        assert_eq!(
            rendered, is_success,
            "{state:?}: svg() contains \">verified<\" = {rendered}, but the guard in \
             `respond` assumes that is true only for success. Either the markup moved \
             and the guard is now a no-op, or a non-success state renders the word."
        );
    }
}

// ---------------------------------------------------------------------------
// D30 — what may reach the rendered comment
// ---------------------------------------------------------------------------

/// **The agent's free text never reaches the PR comment**, and this checks it
/// rather than asserting it.
///
/// # Why this is the question the a209 census is actually asking
///
/// `comment::inert` masks markup — backtick to quote, pipe to a box-drawing
/// bar — and that is *not* credential masking. The census refused an entry
/// claiming otherwise and was right to. What makes this site safe is not what
/// `inert` does to the text, it is **what text can arrive**: this App renders
/// structured fields it selected — claim ids, check names, policy rule names
/// and the verifier's own sentences — and never the agent's prose.
///
/// That is a property of `receipt::subject` and `verify_json::parse`, not of
/// the truncation, and a property is checkable. A receipt is fetched over the
/// network from a pull request; **nothing in this App has redacted it**, and
/// nothing can — whatever redaction ran, ran on the machine that signed it. So
/// the safety cannot come from trusting the bytes, and has to come from
/// reading only fields whose shape is ours.
///
/// The canary is credential-shaped on purpose: if a future edit starts
/// rendering `claims[].text`, an agent that pasted a token into its final
/// message would put that token in a public pull request comment.
#[test]
fn the_agents_free_text_never_reaches_the_rendered_comment() {
    use thruline_app_core::{comment, receipt, ruling::Ground};

    const CANARY: &str = "ghp_CANARYcanary0123456789abcdefghijkl";

    // A receipt whose every free-text field carries the canary.
    let statement = serde_json::json!({
        "predicate": {
            "session": { "id": CANARY, "harness": CANARY, "model": CANARY },
            "claims": [{
                "id": "clm_01",
                "kind": "tests_pass",
                "text": format!("All tests pass. My token is {CANARY}"),
                "extractor": "rules",
                "asserted_by": { "harness": CANARY }
            }],
            "verdicts": [{
                "claim_id": "clm_01",
                "verdict": "unsupported",
                "reasons": [format!("the record does not support {CANARY}")],
                "evidence": []
            }],
            "checks": [{ "id": "chk_01", "stdout_hash": CANARY }],
            "promises": { "open": [ { "text": CANARY } ] }
        }
    });

    let (subj, _) = receipt::subject(&statement, None, None);
    let decision = thruline_policy::Policy::default().evaluate(&subj);

    let verified = verify_json::parse(
        br#"{"conclusion":"neutral","integrity":{"status":"partial"},
             "claims":{"total":1,"supported":0,"unsupported":1,"unverifiable":0,
                       "other":0,"all_supported":false,
                       "unsupported_claim_ids":["clm_01"],"unverifiable_claim_ids":[]},
             "checks":[{"name":"signature","outcome":{"Pass":{"detail":"verifies"}}}]}"#,
    )
    .expect("parse");

    let rendered = comment::render(
        &thruline_app_core::ruling::Ruling {
            conclusion: Conclusion::Neutral,
            ground: Ground::UnprovedNoPolicy,
            carried: true,
        },
        Some(thruline_app_core::receipt_source::Source::PullRequestFile),
        Some(&verified),
        Some(&decision),
    );

    assert!(
        !rendered.contains(CANARY),
        "the agent's free text reached the PR comment:\n{rendered}"
    );
    // Positive control: the canary really is in what was handed in, so this
    // test cannot pass by rendering nothing or by the canary never existing.
    assert!(statement.to_string().contains(CANARY));
    assert!(rendered.contains("clm_01"), "nothing was rendered at all");
}

/// A receipt with no claims is not a receipt whose claims were proved.
///
/// `anything_unproved` is three `> 0` comparisons, so an empty claim set answers
/// "nothing unproved" in the same words a fully proved one uses, and the check
/// run rendered *"Claims supported by the execution record"* over a receipt with
/// none. The conclusion is deliberately unchanged — `success` in both cases is
/// the operator's ruling — but the ground must not say claims were supported
/// when there were no claims.
///
/// The helper above hardcodes `supported: 1`, which is why no existing test
/// reached this: every fixture had a claim, so the empty case was unreachable
/// from the matrix that was supposed to cover the matrix.
#[test]
fn a_receipt_with_no_claims_does_not_claim_its_claims_were_supported() {
    let mut empty = receipt(Conclusion::Success, 0, 0);
    empty.claims = Claims {
        total: 0,
        supported: 0,
        unsupported: 0,
        unverifiable: 0,
        other: 0,
        all_supported: true,
        unsupported_claim_ids: Vec::new(),
        unverifiable_claim_ids: Vec::new(),
    };

    // With no policy, and with one: both paths chose `Verified` before.
    for policy_arg in [None, Some(policy(Conclusion::Success))] {
        let out = decide(Some(&empty), policy_arg.as_ref(), false);
        assert_eq!(
            out.conclusion,
            Conclusion::Success,
            "the ruling is unchanged; only the sentence is"
        );
        assert_eq!(out.ground, Ground::NoClaimsToCheck);
        assert!(
            !out.ground.title().contains("supported"),
            "a zero-claim receipt must not render a sentence about supported claims: {}",
            out.ground.title()
        );
    }

    // And the ordinary case is untouched: one supported claim still reads as one.
    let proved = receipt(Conclusion::Success, 0, 0);
    assert_eq!(decide(Some(&proved), None, false).ground, Ground::Verified);
}

/// A claim the verifier counted but did not name still appears in the table.
///
/// The verdict turns on the counts; the table is built from the id lists. When
/// they diverge — `unsupported: 2` with no ids — every unproved claim dropped
/// out of the table and the only row left was the green one, so a `neutral`
/// conclusion rendered as a single ✅. The summary sentence still said "1 of 3",
/// which is why this was survivable rather than fatal, and also why it lasted:
/// the fact was present somewhere, just not where it is read.
#[test]
fn a_claim_counted_but_not_named_still_gets_a_row() {
    let v = verify_json::parse(
        br#"{"conclusion":"neutral","integrity":{"status":"verified"},
             "claims":{"total":3,"supported":1,"unsupported":2,"unverifiable":0,
                       "other":0,"all_supported":false,
                       "unsupported_claim_ids":[],"unverifiable_claim_ids":[]},
             "checks":[]}"#,
    )
    .expect("parse");
    let out = thruline_app_core::comment::render(
        &thruline_app_core::ruling::Ruling {
            conclusion: Conclusion::Neutral,
            ground: Ground::UnprovedNoPolicy,
            carried: true,
        },
        Some(thruline_app_core::receipt_source::Source::PullRequestFile),
        Some(&v),
        None,
    );

    let rows: Vec<&str> = out.lines().filter(|l| l.starts_with("| ")).collect();
    assert!(
        rows.iter()
            .any(|l| l.contains("not itemised by the verifier")),
        "2 unsupported claims produced no row naming them. Rows were:\n{}",
        rows.join("\n")
    );
    // The specific shape this guards: the table must not consist only of green.
    assert!(
        !rows
            .iter()
            .all(|l| l.contains("supported |") && !l.contains("unsupported")),
        "every row in the table is a supported row, under a neutral conclusion"
    );
}
