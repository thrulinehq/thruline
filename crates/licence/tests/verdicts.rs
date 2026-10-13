//! T-DIST-4 acceptance C — every verdict, with its exact sentence pair.
//!
//! Every key pair here is generated at run time, in memory. Nothing signed or
//! secret is committed, and no fixture file holds a licence.

use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use thruline_licence::{
    check_file, decide, fingerprint, seal, verify, Entitlement, Fingerprint, Plan, Problem, Source,
    Surface, TrustedKey, Verdict, PAID, PLANS,
};

/// 2026-09-23T12:00:00Z — the tests' "now". Passed in; the crate reads no clock.
const NOW: i64 = 1_790_164_800;

const LICENCE_ID: &str = "01J8ZQ4Y6T3M2N1P0R9S8T7V6W";

fn keypair() -> (SigningKey, TrustedKey) {
    let signing = SigningKey::generate(&mut OsRng);
    let trusted = TrustedKey::from_bytes(&signing.verifying_key().to_bytes()).expect("key");
    (signing, trusted)
}

fn payload(product: &str, plan: &str, expires: &str, machine: Option<&str>) -> String {
    let machine = machine
        .map(|m| format!(",\"machine\":\"{m}\""))
        .unwrap_or_default();
    format!(
        "{{\"format\":1,\"licence_id\":\"{LICENCE_ID}\",\"product\":\"{product}\",\"plan\":\"{plan}\",\"issued_at\":\"2026-09-01T00:00:00Z\",\"expires_at\":\"{expires}\"{machine}}}"
    )
}

fn sealed(signing: &SigningKey, body: &str) -> Vec<u8> {
    seal(body.as_bytes(), signing).expect("seal").into_bytes()
}

fn here() -> Fingerprint {
    fingerprint::of_platform_id("cccccccc-1111-2222-3333-444444444444")
}

fn refused(v: Verdict) -> Problem {
    match v {
        Verdict::Refused(p) => p,
        Verdict::Valid(l) => panic!("expected a refusal, got a valid licence: {l:?}"),
    }
}

fn pair(p: &Problem) -> (String, String) {
    (p.problem(), p.fix())
}

#[test]
fn valid() {
    let (signing, trusted) = keypair();
    let bytes = sealed(
        &signing,
        &payload("thruline", "pro", "2027-09-23T00:00:00Z", None),
    );
    let v = verify(&bytes, std::slice::from_ref(&trusted), NOW, &here());
    match &v {
        Verdict::Valid(l) => {
            assert_eq!(l.plan, Plan::Pro);
            assert_eq!(l.licence_id, LICENCE_ID);
            assert_eq!(l.key_id, trusted.id());
            assert!(!l.machine_bound);
        }
        Verdict::Refused(p) => panic!("a good licence was refused: {p:?}"),
    }
    assert_eq!(
        v.sentence(),
        "This machine has a ThruLine Pro licence, valid until 23 Sep 2027."
    );
}

#[test]
fn expired() {
    let (signing, trusted) = keypair();
    let bytes = sealed(
        &signing,
        &payload("thruline", "team", "2026-09-01T00:00:00Z", None),
    );
    let p = refused(verify(&bytes, &[trusted], NOW, &here()));
    assert_eq!(
        pair(&p),
        ("This licence expired on 1 Sep 2026.".into(), RENEW.into())
    );
    // The instant of expiry is already expired: `now >= expires_at`.
    let (signing, trusted) = keypair();
    let bytes = sealed(
        &signing,
        &payload("thruline", "team", "2026-09-23T12:00:00Z", None),
    );
    assert_eq!(
        refused(verify(&bytes, &[trusted], NOW, &here())).slug(),
        "expired"
    );
}

#[test]
fn tampered_one_byte_of_payload_flipped() {
    let (signing, trusted) = keypair();
    let good = sealed(
        &signing,
        &payload("thruline", "pro", "2027-09-23T00:00:00Z", None),
    );
    let text = String::from_utf8(good.clone()).expect("utf8");
    let start = text.find("payload: ").expect("payload") + "payload: ".len();
    let end = start + text[start..].find('\n').expect("eol");
    // A byte at the start, the middle and the end of the payload, each flipped
    // on its own, is caught.
    for at in [start, start + (end - start) / 2, end - 1] {
        let mut bad = good.clone();
        bad[at] ^= 0x01;
        let p = refused(verify(&bad, std::slice::from_ref(&trusted), NOW, &here()));
        assert_eq!(
            pair(&p),
            (
                "This licence has been changed, or wasn't made by the maker it names, so it can't be trusted."
                    .into(),
                "Download it again from the licence page on ThruLine's website, with your licence key.".into()
            ),
            "byte {at} flipped"
        );
    }
    // The untouched original still verifies: the flip is what was caught.
    assert!(matches!(
        verify(&good, &[trusted], NOW, &here()),
        Verdict::Valid(_)
    ));
}

#[test]
fn wrong_product() {
    let (signing, trusted) = keypair();
    let bytes = sealed(
        &signing,
        &payload("otherthing", "pro", "2027-09-23T00:00:00Z", None),
    );
    let p = refused(verify(&bytes, &[trusted], NOW, &here()));
    assert_eq!(
        pair(&p),
        (
            "This licence is for a different product, not ThruLine.".into(),
            "Check you picked the right file, or download it again from the licence page on ThruLine's website, with your licence key."
                .into()
        )
    );
}

#[test]
fn wrong_key_is_told_apart_from_tampered() {
    let (signing, _) = keypair();
    let (_, other) = keypair();
    let bytes = sealed(
        &signing,
        &payload("thruline", "pro", "2027-09-23T00:00:00Z", None),
    );
    let p = refused(verify(&bytes, &[other], NOW, &here()));
    assert_eq!(
        pair(&p),
        (
            "This licence names a maker this version of ThruLine doesn't know, so it can't check it."
                .into(),
            "Update ThruLine, then install the licence again; if it's still refused, download it again from the licence page on ThruLine's website, with your licence key."
                .into()
        )
    );
    assert_eq!(p, Problem::UnknownKey);
}

#[test]
fn unknown_plan() {
    let (signing, trusted) = keypair();
    let bytes = sealed(
        &signing,
        &payload("thruline", "enterprise", "2027-09-23T00:00:00Z", None),
    );
    let p = refused(verify(&bytes, &[trusted], NOW, &here()));
    assert_eq!(
        pair(&p),
        (
            "This licence names a plan this version of ThruLine doesn't know.".into(),
            "Update ThruLine, then install the licence again.".into()
        )
    );
}

#[test]
fn wrong_machine() {
    let (signing, trusted) = keypair();
    let elsewhere = match fingerprint::of_platform_id("another-machine") {
        Fingerprint::Available(h) => h,
        Fingerprint::Unavailable => panic!("hash"),
    };
    let bytes = sealed(
        &signing,
        &payload("thruline", "pro", "2027-09-23T00:00:00Z", Some(&elsewhere)),
    );
    let p = refused(verify(&bytes, std::slice::from_ref(&trusted), NOW, &here()));
    assert_eq!(
        pair(&p),
        (
            "This licence is for a different machine.".into(),
            "Install the licence issued for this machine, or ask for one issued for it.".into()
        )
    );
    // Positive control: the same licence on the machine it names is valid.
    let mine = match here() {
        Fingerprint::Available(h) => h,
        Fingerprint::Unavailable => panic!("hash"),
    };
    let bytes = sealed(
        &signing,
        &payload("thruline", "pro", "2027-09-23T00:00:00Z", Some(&mine)),
    );
    match verify(&bytes, &[trusted], NOW, &here()) {
        Verdict::Valid(l) => assert!(l.machine_bound),
        Verdict::Refused(p) => panic!("the named machine was refused: {p:?}"),
    }
}

#[test]
fn fingerprint_unavailable() {
    let (signing, trusted) = keypair();
    let bytes = sealed(
        &signing,
        &payload("thruline", "pro", "2027-09-23T00:00:00Z", Some("ab")),
    );
    let p = refused(verify(
        &bytes,
        std::slice::from_ref(&trusted),
        NOW,
        &Fingerprint::Unavailable,
    ));
    assert_eq!(
        pair(&p),
        (
            "This licence is for one machine, and ThruLine can't tell which machine this is."
                .into(),
            "Ask for a licence that isn't tied to one machine.".into()
        )
    );
    // A licence tied to no machine does not need to know which one this is.
    let bytes = sealed(
        &signing,
        &payload("thruline", "pro", "2027-09-23T00:00:00Z", None),
    );
    assert!(matches!(
        verify(&bytes, &[trusted], NOW, &Fingerprint::Unavailable),
        Verdict::Valid(_)
    ));
}

#[test]
fn absent() {
    let (_, trusted) = keypair();
    let dir = tempfile::tempdir().expect("tempdir");
    let p = refused(check_file(
        &dir.path().join("licence.txt"),
        &[trusted],
        NOW,
        &here(),
    ));
    assert_eq!(
        pair(&p),
        (
            "No licence is installed on this machine.".into(),
            "Get Pro at https://thrulinehq.com/pricing, then install your licence with: thruline licence install <file>".into()
        )
    );
}

#[test]
fn unreadable() {
    let (_, trusted) = keypair();
    let dir = tempfile::tempdir().expect("tempdir");
    // A directory where the file should be: it exists, and cannot be read as
    // a file by anybody, root included.
    let path = dir.path().join("licence.txt");
    std::fs::create_dir(&path).expect("mkdir");
    let p = refused(check_file(&path, &[trusted], NOW, &here()));
    assert_eq!(
        pair(&p),
        (
            "There is a licence file, but ThruLine can't open it.".into(),
            "Check the file can be read by your user, or install the licence again.".into()
        )
    );
}

#[test]
fn not_a_licence_and_newer_format_and_no_key() {
    let (signing, trusted) = keypair();
    for junk in [&b""[..], b"hello", b"ThruLine licence\nkey: 00\n"] {
        assert_eq!(
            refused(verify(junk, std::slice::from_ref(&trusted), NOW, &here())),
            Problem::NotALicence
        );
    }
    assert_eq!(
        pair(&Problem::NotALicence),
        (
            "This file isn't a ThruLine licence, or part of it is missing.".into(),
            "Download it again from the licence page on ThruLine's website, with your licence key."
                .into()
        )
    );
    let newer = payload("thruline", "pro", "2027-09-23T00:00:00Z", None).replacen(
        "\"format\":1",
        "\"format\":2",
        1,
    );
    let p = refused(verify(
        &sealed(&signing, &newer),
        std::slice::from_ref(&trusted),
        NOW,
        &here(),
    ));
    assert_eq!(
        pair(&p),
        (
            "This licence was made for a newer version of ThruLine.".into(),
            "Update ThruLine, then install the licence again.".into()
        )
    );
    // With no trusted key at all, the binary says so, whatever the file — and,
    // being the build that ships until a key exists, says nothing alarming
    // (ADV-dist-copy-3): such a build gates nothing, so nothing needs doing.
    let good = sealed(
        &signing,
        &payload("thruline", "pro", "2027-09-23T00:00:00Z", None),
    );
    let p = refused(verify(&good, &[], NOW, &here()));
    assert_eq!(
        pair(&p),
        (
            "This copy of ThruLine doesn't check licences, and nothing in it needs one.".into(),
            "You don't need to do anything.".into()
        )
    );
    let dir = tempfile::tempdir().expect("tempdir");
    assert_eq!(
        refused(check_file(&dir.path().join("none"), &[], NOW, &here())),
        Problem::NoTrustedKey
    );
}

#[test]
fn every_problem_has_its_own_sentence() {
    let all = [
        Problem::Absent,
        Problem::Unreadable,
        Problem::NotALicence,
        Problem::NewerFormat,
        Problem::NoTrustedKey,
        Problem::UnknownKey,
        Problem::Tampered,
        Problem::WrongProduct,
        Problem::UnknownPlan,
        Problem::Expired {
            on: "1 Sep 2026".into(),
        },
        Problem::WrongMachine,
        Problem::MachineUnknown,
    ];
    let sentences: std::collections::BTreeSet<String> = all.iter().map(Problem::problem).collect();
    assert_eq!(sentences.len(), all.len(), "two problems share a sentence");
    let slugs: Vec<&str> = all.iter().map(Problem::slug).collect();
    assert_eq!(slugs, Problem::SLUGS);
    for p in &all {
        for word in ["signature", "key id", "fingerprint"] {
            assert!(
                !p.problem().to_lowercase().contains(word)
                    && !p.fix().to_lowercase().contains(word),
                "{p:?} says the technical word {word:?} on the plain layer"
            );
        }
    }
}

/* ── entitlement ───────────────────────────────────────────────────────── */

#[test]
fn exactly_one_paid_surface_and_both_plans_unlock_it() {
    assert_eq!(
        PAID.len(),
        1,
        "the paid set is PROVISIONAL and exactly one surface"
    );
    assert_eq!(PAID[0].0, Surface::ApproveVersion);
    assert_eq!(PAID[0].1, &[Plan::Pro, Plan::Team]);
    assert_eq!(PLANS.len(), 2);
}

#[test]
fn decide_opens_on_a_valid_licence_and_refuses_with_both_halves_otherwise() {
    let (signing, trusted) = keypair();
    for plan in ["pro", "team"] {
        let bytes = sealed(
            &signing,
            &payload("thruline", plan, "2027-09-23T00:00:00Z", None),
        );
        let v = verify(&bytes, std::slice::from_ref(&trusted), NOW, &here());
        assert_eq!(
            decide(
                Surface::ApproveVersion,
                &[Source::Licence {
                    verdict: v,
                    installed: true
                }]
            ),
            Entitlement::Allowed
        );
    }
    assert_eq!(
        decide(
            Surface::ApproveVersion,
            &[Source::Licence {
                verdict: Verdict::Refused(Problem::Absent),
                installed: false
            }]
        ),
        Entitlement::Refused {
            reason: "Approving a version needs a Pro licence. No licence is installed on this machine.".into(),
            enable: "Get Pro at https://thrulinehq.com/pricing, then install your licence with: thruline licence install <file>".into(),
            status: "absent".into(),
        }
    );
    // No source at all is absence, never a pass.
    assert!(matches!(
        decide(Surface::ApproveVersion, &[]),
        Entitlement::Refused { .. }
    ));
}

/* ── C20: approving has one paid licence source ─────────────────────────── */

/// An expired paid licence points to the renewal path for a lapsed payment,
/// or to the current period's download after a successful automatic renewal.
const RENEW: &str = "Go to the licence page on ThruLine's website with your licence key, renew there if your subscription has ended, then download the new licence and install it.";

#[test]
fn no_installed_licence_never_opens_approving() {
    let expected = Entitlement::Refused {
        reason: "Approving a version needs a Pro licence. No licence is installed on this machine."
            .into(),
        enable: "Get Pro at https://thrulinehq.com/pricing, then install your licence with: thruline licence install <file>".into(),
        status: "absent".into(),
    };
    assert_eq!(decide(Surface::ApproveVersion, &[]), expected);

    let sources = [Source::Licence {
        verdict: Verdict::Refused(Problem::Absent),
        installed: false,
    }];
    assert_eq!(decide(Surface::ApproveVersion, &sources), expected);
    assert!(matches!(
        thruline_licence::speaking(&sources),
        Some(Source::Licence {
            verdict: Verdict::Refused(Problem::Absent),
            installed: false,
        })
    ));
}

#[test]
fn signed_paid_expiry_includes_issuer_renewal_grace_and_refuses_at_boundary() {
    // The issuer has included renewal grace in expires_at. Verification uses
    // the signed boundary directly, with no grace calculation in this crate.
    const PAID_PERIOD_END: i64 = 1_790_726_400; // 30 Sep 2026 00:00 UTC
    const IN_RENEWAL_GRACE: i64 = 1_791_331_200; // 7 Oct 2026 00:00 UTC
    const SIGNED_EXPIRY: i64 = 1_791_936_000; // 14 Oct 2026 00:00 UTC

    let (signing, trusted) = keypair();
    let bytes = sealed(
        &signing,
        &payload("thruline", "pro", "2026-10-14T00:00:00Z", None),
    );
    for now in [PAID_PERIOD_END, IN_RENEWAL_GRACE, SIGNED_EXPIRY - 1] {
        let verdict = verify(&bytes, std::slice::from_ref(&trusted), now, &here());
        assert!(matches!(&verdict, Verdict::Valid(_)), "refused at {now}");
        assert_eq!(
            decide(
                Surface::ApproveVersion,
                &[Source::Licence {
                    verdict,
                    installed: true,
                }]
            ),
            Entitlement::Allowed,
            "approving closed before signed expiry at {now}"
        );
    }

    let verdict = verify(&bytes, &[trusted], SIGNED_EXPIRY, &here());
    assert_eq!(
        verdict,
        Verdict::Refused(Problem::Expired {
            on: "14 Oct 2026".into(),
        })
    );
    let sources = [Source::Licence {
        verdict,
        installed: true,
    }];
    assert_eq!(
        decide(Surface::ApproveVersion, &sources),
        Entitlement::Refused {
            reason: "Approving a version needs a Pro licence. This licence expired on 14 Oct 2026."
                .into(),
            enable: RENEW.into(),
            status: "expired".into(),
        }
    );
    assert!(matches!(
        thruline_licence::speaking(&sources),
        Some(Source::Licence {
            verdict: Verdict::Refused(Problem::Expired { .. }),
            installed: true,
        })
    ));
}

#[test]
fn the_expired_fix_names_renewing_on_the_licence_page_and_installing_the_new_licence() {
    let p = Problem::Expired {
        on: "1 Sep 2026".into(),
    };
    assert_eq!(p.fix(), RENEW);
    let fix = p.fix();
    assert!(fix.contains("renew"), "{fix}");
    assert!(fix.contains("licence page"), "{fix}");
    assert!(fix.contains("install"), "{fix}");
}

#[test]
fn an_unusable_installed_licence_refuses_and_explains_its_own_problem() {
    for p in [
        Problem::Unreadable,
        Problem::NotALicence,
        Problem::NewerFormat,
        Problem::NoTrustedKey,
        Problem::UnknownKey,
        Problem::Tampered,
        Problem::WrongProduct,
        Problem::UnknownPlan,
        Problem::WrongMachine,
        Problem::MachineUnknown,
    ] {
        let sources = [Source::Licence {
            verdict: Verdict::Refused(p.clone()),
            installed: true,
        }];
        assert_eq!(
            decide(Surface::ApproveVersion, &sources),
            Entitlement::Refused {
                reason: format!("Approving a version needs a Pro licence. {}", p.problem()),
                enable: p.fix(),
                status: p.slug().into(),
            },
            "{p:?} opened approving"
        );
        match thruline_licence::speaking(&sources) {
            Some(Source::Licence {
                verdict: Verdict::Refused(problem),
                installed: true,
            }) => assert_eq!(problem, &p),
            other => panic!("{p:?} did not speak for its installed file: {other:?}"),
        }
    }
}

/* ── ruling 2: Pro is the only plan anyone can buy or see ──────────────── */

/// Founder ruling 2 (D-298): "Pro is the only purchasable plan. Team licence
/// code may stay dormant, but Team must not be buyable or shown anywhere a
/// user sees." A Team licence still parses, verifies and unlocks approving —
/// that is the dormancy — but the sentence a person reads names no plan: not
/// "Team", which is not sold, and not "Pro", which this licence is not.
#[test]
fn ruling_2_a_dormant_team_licence_verifies_and_is_named_by_no_plan() {
    assert_eq!(
        Plan::parse("team"),
        Some(Plan::Team),
        "Team stays parseable"
    );
    assert_eq!(Plan::Team.as_str(), "team");
    let (signing, trusted) = keypair();
    let bytes = sealed(
        &signing,
        &payload("thruline", "team", "2027-09-23T00:00:00Z", None),
    );
    let v = verify(&bytes, std::slice::from_ref(&trusted), NOW, &here());
    match &v {
        Verdict::Valid(l) => assert_eq!(l.plan, Plan::Team),
        Verdict::Refused(p) => panic!("a dormant Team licence was refused: {p:?}"),
    }
    assert_eq!(
        v.sentence(),
        "This machine has a ThruLine licence, valid until 23 Sep 2027."
    );
    assert_eq!(
        decide(
            Surface::ApproveVersion,
            &[Source::Licence {
                verdict: v,
                installed: true
            }]
        ),
        Entitlement::Allowed,
        "a dormant Team licence keeps approving open"
    );
}

/// Every sentence the crate can say of a licence — every verdict, for every
/// plan it knows — names no plan but Pro, and Pro only of a Pro licence.
#[test]
fn ruling_2_no_sentence_names_a_plan_that_is_not_sold() {
    for (wire, plan, _) in PLANS {
        let (signing, trusted) = keypair();
        for expires in ["2027-09-23T00:00:00Z", "2026-09-01T00:00:00Z"] {
            let bytes = sealed(&signing, &payload("thruline", wire, expires, None));
            let v = verify(&bytes, std::slice::from_ref(&trusted), NOW, &here());
            let mut said = vec![v.sentence()];
            if let Verdict::Refused(p) = &v {
                said.push(p.fix());
            }
            for s in said {
                assert!(!s.contains("Team"), "{wire}: {s}");
                if *plan != Plan::Pro {
                    assert!(!s.contains("Pro"), "{wire}: {s}");
                }
            }
        }
    }
}
