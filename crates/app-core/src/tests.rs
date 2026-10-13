use super::*;

const SECRET: &[u8] = b"a-webhook-secret";

fn sign(secret: &[u8], body: &[u8]) -> String {
    use hmac::Mac as _;
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(secret).expect("hmac");
    mac.update(body);
    let d = mac.finalize().into_bytes();
    let mut hex = String::from("sha256=");
    for b in d {
        hex.push_str(&format!("{b:02x}"));
    }
    hex
}

fn body() -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({
        // `action` is load-bearing since the filter went in: a delivery with no
        // action is not actionable, which is the right default and is why this
        // fixture has to state one.
        "action": "opened",
        "repository": { "full_name": "acme/widgets" },
        "pull_request": { "number": 7, "head": { "sha": "a".repeat(40) } },
        "installation": { "id": 12345 }
    }))
    .expect("json")
}

#[test]
fn a_correctly_signed_delivery_authenticates() {
    let b = body();
    let w = Webhook::authenticate(Some("pull_request"), Some(&sign(SECRET, &b)), SECRET, &b)
        .expect("a correct signature must authenticate");
    assert!(w.is_actionable());
    let s = w.subject().expect("subject");
    assert_eq!(s.repo, "acme/widgets");
    assert_eq!(s.installation, 12345);
    assert_eq!(s.pull_request, Some(7));
}

/// One flipped byte in the body must fail, and the error must not say which
/// byte.
#[test]
fn a_tampered_body_is_refused() {
    let b = body();
    let sig = sign(SECRET, &b);
    let mut tampered = b.clone();
    let at = tampered.len() / 2;
    tampered[at] ^= 0x01;
    assert_eq!(
        Webhook::authenticate(Some("pull_request"), Some(&sig), SECRET, &tampered)
            .expect_err("must be refused"),
        WebhookError::BadSignature
    );
}

#[test]
fn the_wrong_secret_is_refused() {
    let b = body();
    let sig = sign(b"someone-elses-secret", &b);
    assert_eq!(
        Webhook::authenticate(Some("pull_request"), Some(&sig), SECRET, &b)
            .expect_err("must be refused"),
        WebhookError::BadSignature
    );
}

/// **The body is not parsed until the signature is checked.** An
/// unauthenticated JSON parse is a public attack surface, and the ordering is
/// the defence — so a body that is not JSON at all must still fail on the
/// signature, not on the parse.
#[test]
fn an_unsigned_delivery_is_never_parsed() {
    let junk = b"{{{{ not json at all".to_vec();
    for (sig, want) in [
        (None, WebhookError::MissingSignature),
        (
            Some("garbage".to_string()),
            WebhookError::MalformedSignature,
        ),
        (
            Some(format!("sha256={}", "z".repeat(64))),
            WebhookError::MalformedSignature,
        ),
        (Some(sign(b"wrong", &junk)), WebhookError::BadSignature),
    ] {
        assert_eq!(
            Webhook::authenticate(Some("pull_request"), sig.as_deref(), SECRET, &junk)
                .expect_err("must be refused"),
            want
        );
    }
    // And with the right signature it reaches the parser and fails there —
    // which proves the four cases above stopped earlier.
    let e = Webhook::authenticate(
        Some("pull_request"),
        Some(&sign(SECRET, &junk)),
        SECRET,
        &junk,
    )
    .expect_err("junk is not JSON");
    assert!(matches!(e, WebhookError::NotJson(_)), "{e:?}");
}

#[test]
fn a_missing_event_header_is_refused() {
    let b = body();
    assert_eq!(
        Webhook::authenticate(None, Some(&sign(SECRET, &b)), SECRET, &b)
            .expect_err("must be refused"),
        WebhookError::MissingEvent
    );
}

/// Events the App did not ask for are acknowledged and ignored. An App that
/// errors on them gets its deliveries disabled by GitHub.
#[test]
fn unrelated_events_are_not_actionable_but_are_not_errors() {
    let b = body();
    for event in ["push", "issues", "star", "ping"] {
        let w = Webhook::authenticate(Some(event), Some(&sign(SECRET, &b)), SECRET, &b)
            .expect("authenticates");
        assert!(!w.is_actionable(), "{event}");
    }
}

/// A webhook body is written by whoever sent it. Every field the App uses is
/// checked against the shape GitHub can actually produce, **here**, rather than
/// at whatever joins it to a path later.
#[test]
fn payload_fields_that_could_be_paths_or_arguments_are_refused() {
    for repo in [
        "../../etc/passwd",
        "acme/../widgets",
        "acme/widgets/extra",
        "/acme/widgets",
        "acme/",
        "",
        "acme/widgets;rm -rf /",
        "-flag/widgets",
    ] {
        assert!(!is_repo_full_name(repo), "{repo:?} was accepted");
    }
    assert!(is_repo_full_name("acme/widgets"));
    assert!(is_repo_full_name("a-b_c.d/e_f-g.h"));

    for sha in ["", "zzzz", "a".repeat(39).as_str(), "../../x"] {
        assert!(!is_sha(sha), "{sha:?} was accepted");
    }
    assert!(is_sha(&"a".repeat(40)));
    assert!(is_sha(&"F".repeat(64)));
}

#[test]
fn a_delivery_naming_a_repository_that_could_be_a_path_is_refused() {
    let evil = serde_json::to_vec(&serde_json::json!({
        "repository": { "full_name": "../../../etc" },
        "check_suite": { "head_sha": "b".repeat(40) },
        "installation": { "id": 1 }
    }))
    .expect("json");
    let w = Webhook::authenticate(
        Some("check_suite"),
        Some(&sign(SECRET, &evil)),
        SECRET,
        &evil,
    )
    .expect("signature is valid; the content is not");
    let e = w
        .subject()
        .expect_err("a path-shaped repository must be refused");
    assert!(
        matches!(
            e,
            WebhookError::Untrusted {
                field: "repository.full_name",
                ..
            }
        ),
        "{e:?}"
    );
}

// ---------------------------------------------------------------------------
// Receipt sources.
// ---------------------------------------------------------------------------

#[test]
fn the_strongest_receipt_source_is_searched_first() {
    use receipt_source::{Source, SEARCH_ORDER};
    assert_eq!(SEARCH_ORDER[0], Source::Attestation);
    // Looking in the pull request first would let an author shadow an
    // attestation by committing a file.
    assert!(
        SEARCH_ORDER
            .iter()
            .position(|s| *s == Source::Attestation)
            .unwrap()
            < SEARCH_ORDER
                .iter()
                .position(|s| *s == Source::PullRequestFile)
                .unwrap()
    );
    // Only the source GitHub vouches for goes without a caveat.
    assert!(Source::Attestation.caveat().is_none());
    assert!(Source::PullRequestFile.caveat().is_some());
    assert!(Source::Comment.caveat().is_some());
}

#[test]
fn a_receipt_path_that_could_escape_the_checkout_is_refused() {
    use receipt_source::is_receipt_path;
    for bad in [
        ".thruline/receipts/../../etc/passwd",
        ".thruline/receipts/sub/dir.json",
        ".thruline/receipts/.hidden.json",
        ".thruline/receipts/x.json/../y",
        "/.thruline/receipts/x.json",
        ".thruline/receipts/",
        ".thruline/receipts/.json",
        "receipts/x.json",
        ".thruline/receipts/x.txt",
    ] {
        assert!(!is_receipt_path(bad), "{bad:?} was accepted");
    }
    assert!(is_receipt_path(".thruline/receipts/latest.json"));
    assert!(is_receipt_path(
        ".thruline/receipts/2026-09-03T10-00-00Z.json"
    ));
}

// ---------------------------------------------------------------------------
// The check run.
// ---------------------------------------------------------------------------

#[test]
fn two_neutrals_cannot_make_a_success() {
    use check_run::combine;
    use thruline_policy::Conclusion::*;
    assert_eq!(combine(Neutral, Neutral), Neutral);
    assert_eq!(combine(Neutral, Success), Neutral);
    assert_eq!(combine(Success, Neutral), Neutral);
    assert_eq!(combine(Failure, Success), Failure);
    assert_eq!(combine(Neutral, Failure), Failure);
    assert_eq!(combine(Success, Success), Success);
}

#[test]
fn the_summary_never_drops_a_weak_sources_caveat() {
    use check_run::summary;
    use receipt_source::Source;
    use thruline_policy::Conclusion;
    for source in [Source::PullRequestFile, Source::Comment] {
        let s = summary(source, Conclusion::Success, None);
        assert!(
            s.contains("signature"),
            "{}: the caveat is missing from\n{s}",
            source.as_str()
        );
    }
    let s = summary(Source::Attestation, Conclusion::Success, None);
    assert!(s.contains("attestation"), "{s}");
}

#[test]
fn a_pipe_in_a_reason_cannot_break_the_table() {
    use check_run::summary;
    use receipt_source::Source;
    use thruline_policy::{Conclusion, Decision, Reason};
    let d = Decision {
        conclusion: Conclusion::Failure,
        reasons: vec![Reason {
            claim: Some("tests_pass".into()),
            rule: "verdict",
            detail: "the command was `a | b | c`".into(),
            conclusion: Conclusion::Failure,
        }],
        advisory: Default::default(),
    };
    let s = summary(Source::Attestation, Conclusion::Success, Some(&d));
    let row = s
        .lines()
        .find(|l| l.contains("tests_pass"))
        .expect("the row is missing");
    assert_eq!(row.matches("\\|").count(), 2, "{row}");
}

#[test]
fn the_check_run_payload_is_the_shape_the_api_wants() {
    use thruline_policy::Conclusion;
    let p = check_run::payload(&"a".repeat(40), Conclusion::Neutral, "t", "s");
    assert_eq!(p["name"], "thruline/receipt");
    assert_eq!(p["status"], "completed");
    assert_eq!(p["conclusion"], "neutral");
    assert_eq!(p["output"]["title"], "t");
}

// ---------------------------------------------------------------------------
// The badge.
// ---------------------------------------------------------------------------

#[test]
fn only_one_badge_state_says_verified() {
    use thruline_policy::Conclusion;
    let mut verified = 0;
    // **Four states**, including the one for a repository this App has never
    // concluded anything about. `Unknown` is in the loop because it is the
    // state a brand-new installation is in, and it is the one nobody
    // remembers to check.
    for c in [
        badge::State::Known(Conclusion::Success),
        badge::State::Known(Conclusion::Neutral),
        badge::State::Known(Conclusion::Failure),
        badge::State::Unknown,
    ] {
        let svg = badge::svg(c);
        assert!(svg.starts_with("<svg"), "{svg}");
        // No external FETCH: a badge is the most-copied surface there is, and
        // an external reference in one is a request from every README it lands
        // in. Checked in fetching positions rather than for the substring
        // `http`, because `xmlns="http://www.w3.org/2000/svg"` is a namespace
        // identifier that is never retrieved — the first version of this
        // assertion failed on it.
        for fetch in [
            "href=",
            "src=",
            "url(http",
            "url('http",
            "@import",
            "<script",
            "<image",
            "<foreignObject",
        ] {
            assert!(!svg.contains(fetch), "{c:?} badge contains `{fetch}`");
        }
        // Positive control: the rule would catch a real one.
        let injected = svg.replace(
            "<title>",
            "<image href=\"https://evil.invalid/x.png\"/><title>",
        );
        assert!(injected.contains("href="), "the fetch check is vacuous");
        if svg.contains(">verified<") {
            verified += 1;
        }
    }
    assert_eq!(verified, 1, "more than one badge state claims verification");
    // **One vocabulary with the binary developers run.** This module used to
    // spell `Neutral` as "not verified" and `Failure` as "unsupported", while
    // `attest::report::badge_of` spelled `Neutral` "partial" and `Failure`
    // "not verified". Two badges for one product, colliding on the one phrase
    // a reader would have to disambiguate.
    assert!(badge::svg(badge::State::Known(Conclusion::Neutral)).contains(">partial<"));
    assert!(badge::svg(badge::State::Known(Conclusion::Failure)).contains(">not verified<"));
    assert!(badge::svg(badge::State::Unknown).contains(">no receipt<"));
}

/// The hosted endpoint's whole reason for existing: **nothing a caller sends
/// can raise the badge's colour.**
///
/// `attest::report::badge_route` takes its state from `?status=`, which is
/// correct there and fatal on a public URL. This walks the shapes an attacker
/// would try and asserts every one of them lands on a state the store chose.
#[test]
fn no_query_parameter_can_make_the_hosted_badge_green() {
    use thruline_policy::Conclusion;

    struct Empty;
    impl badge::Store for Empty {
        fn conclusion(&self, _repo: &str, _r: &str) -> Option<Conclusion> {
            None
        }
    }
    struct SaysNeutral;
    impl badge::Store for SaysNeutral {
        fn conclusion(&self, _repo: &str, _r: &str) -> Option<Conclusion> {
            Some(Conclusion::Neutral)
        }
    }

    for q in [
        "",
        "?status=verified",
        "?ref=main&status=verified",
        "?status=success",
        "?conclusion=success",
        "?ref=verified",
    ] {
        let req = badge::route(&format!("/badge/acme/widgets.svg{q}"))
            .unwrap_or_else(|| panic!("route must parse {q:?}"));
        for (store, name) in [
            (&Empty as &dyn badge::Store, "empty"),
            (&SaysNeutral as &dyn badge::Store, "neutral"),
        ] {
            let (status, _, _, body) = badge::respond(store, &req);
            assert_eq!(status, 200, "{name} {q}");
            assert!(
                !body.contains(">verified<"),
                "the {name} store served a green badge for {q:?}"
            );
        }
    }

    // Positive control: a store that really did conclude success is green, so
    // the assertions above are not passing because the badge is never green.
    struct SaysSuccess;
    impl badge::Store for SaysSuccess {
        fn conclusion(&self, _repo: &str, _r: &str) -> Option<Conclusion> {
            Some(Conclusion::Success)
        }
    }
    let req = badge::route("/badge/acme/widgets.svg").expect("route");
    let (_, _, _, body) = badge::respond(&SaysSuccess, &req);
    assert!(body.contains(">verified<"), "the green path is unreachable");
}

/// A path this endpoint does not own must not be served a badge, and a
/// repository name that could be a traversal must not be looked up.
#[test]
fn the_badge_route_refuses_what_it_does_not_own() {
    for bad in [
        "/badge/acme.svg",
        "/badge/acme/widgets.png",
        "/badge/../../etc/passwd.svg",
        "/badge/acme/../widgets.svg",
        "/badge//widgets.svg",
        "/badge/verified-by-thruline.svg?status=verified",
        "/acme/widgets.svg",
    ] {
        assert!(badge::route(bad).is_none(), "{bad} was routed");
    }
    let ok = badge::route("/badge/acme/widgets.svg?ref=abc123").expect("a real one");
    assert_eq!(ok.repo, "acme/widgets");
    assert_eq!(ok.git_ref, "abc123");
    // An unusable ref falls back to `main` rather than being normalised into
    // something that looks safe.
    assert_eq!(
        badge::route("/badge/acme/widgets.svg?ref=../../x")
            .expect("route")
            .git_ref,
        "main"
    );
}

#[test]
fn badge_text_is_escaped() {
    assert_eq!(badge::escape("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&apos;");
}

/// The `action` filter, in both directions.
///
/// Getting it wrong one way re-verifies on every title edit and posts checks
/// onto closed pull requests; getting it wrong the other way ignores the label
/// that §9's `require_human_ack` is satisfied by, so a reviewer applies
/// `tests-changed-ok` and nothing happens.
#[test]
fn the_action_decides_whether_a_pull_request_delivery_is_acted_on() {
    let hook = |event: &str, action: &str| {
        let body = serde_json::to_vec(&serde_json::json!({
            "action": action,
            "repository": { "full_name": "acme/widgets" },
            "pull_request": { "number": 7, "head": { "sha": "a".repeat(40) } },
            "check_suite": { "head_sha": "a".repeat(40) },
            "installation": { "id": 1 }
        }))
        .expect("json");
        Webhook::authenticate(Some(event), Some(&sign(SECRET, &body)), SECRET, &body)
            .expect("authenticate")
    };

    for action in ["opened", "synchronize", "reopened", "ready_for_review"] {
        assert!(hook("pull_request", action).is_actionable(), "{action}");
    }
    // The label §9's `require_human_ack` reads. Ignoring it would make applying
    // the label a no-op with nothing saying why.
    for action in ["labeled", "unlabeled"] {
        assert!(
            hook("pull_request", action).is_actionable(),
            "{action} is ignored, so `require_human_ack` can never be satisfied by \
             the label §9 names"
        );
    }
    for action in ["closed", "edited", "assigned", "milestoned", "locked", ""] {
        assert!(
            !hook("pull_request", action).is_actionable(),
            "{action:?} triggers a full re-verification and a check run, on a pull \
             request where nothing that could change the answer has changed"
        );
    }
    for action in ["requested", "rerequested"] {
        assert!(hook("check_suite", action).is_actionable(), "{action}");
    }
    // `completed` is GitHub telling us a suite finished, not asking for one.
    assert!(!hook("check_suite", "completed").is_actionable());
    assert!(!hook("issues", "opened").is_actionable());
}
