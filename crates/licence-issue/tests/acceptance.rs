//! T-DIST-6 acceptance A (in process), B, C and D.
//!
//! Every signing key and webhook secret is generated here, at run time. The
//! webhook bodies are the crate's fixtures, **SYNTHESISED FROM DOCUMENTATION,
//! NOT RECORDED — unverified against a live account.** Acceptance A against
//! the real `thruline` binary is `crates/cli/tests/licence_cli.rs`; E is
//! `tools/licence-issuer/tests/shell.rs`.

use std::path::PathBuf;

use ed25519_dalek::SigningKey;
use rand_core::{OsRng, RngCore};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thruline_licence::{verify, Fingerprint, Plan, Problem, TrustedKey, Verdict};
use thruline_licence_issue::fixtures::{
    FIXTURE_MAPPING, LEMONSQUEEZY_ORDER_CREATED, PADDLE_TRANSACTION_COMPLETED, PROVENANCE,
};
use thruline_licence_issue::{
    lemonsqueezy, paddle, Config, Headers, Issued, Issuer, Refusal, Store, WebhookSecret,
};

/* ── the rig ────────────────────────────────────────────────────────────── */

fn now() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    i64::try_from(secs).expect("secs")
}

/// A webhook secret, generated now: 32 random bytes as hex text.
fn fresh_secret() -> Vec<u8> {
    let mut b = [0u8; 32];
    OsRng.fill_bytes(&mut b);
    thruline_licence::to_hex(&b).into_bytes()
}

struct Rig {
    _dir: tempfile::TempDir,
    db: PathBuf,
    issuer: Issuer,
    public: [u8; 32],
    secret: Vec<u8>,
}

impl Rig {
    fn new(mapping: &str) -> Rig {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("issued.sqlite");
        let signing = SigningKey::generate(&mut OsRng);
        let public = signing.verifying_key().to_bytes();
        let config = Config::from_toml(mapping).expect("mapping");
        let store = Store::open(&db).expect("store");
        Rig {
            _dir: dir,
            db,
            issuer: Issuer::new(config, store, signing),
            public,
            secret: fresh_secret(),
        }
    }

    fn fixtures() -> Rig {
        Rig::new(FIXTURE_MAPPING)
    }

    fn live(&self) -> WebhookSecret {
        WebhookSecret::live(self.secret.clone())
    }

    fn ls(&mut self, body: &[u8], at: i64) -> Result<Issued, Refusal> {
        let headers = lemonsqueezy::sign(&self.secret, body);
        let secret = self.live();
        self.issuer
            .handle("lemonsqueezy", &secret, &headers, body, at)
    }

    fn paddle(&mut self, body: &[u8], at: i64) -> Result<Issued, Refusal> {
        let headers = paddle::sign(&self.secret, body, at);
        let secret = self.live();
        self.issuer.handle("paddle", &secret, &headers, body, at)
    }

    /// (licences, events, row digest, file SHA-256): "the store is unchanged"
    /// is this tuple before == after.
    fn census(&self) -> (u64, u64, String, String) {
        let s = self.issuer.store();
        (
            s.licences().expect("count"),
            s.events().expect("count"),
            s.digest().expect("digest"),
            thruline_licence::to_hex(&Sha256::digest(std::fs::read(&self.db).expect("db"))),
        )
    }

    fn verdict(&self, licence: &[u8], at: i64, machine: &Fingerprint) -> Verdict {
        let key = TrustedKey::from_bytes(&self.public).expect("key");
        verify(licence, &[key], at, machine)
    }
}

/// A fixture with one edit applied, re-serialised (so it must be re-signed).
fn edited(fixture: &str, edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut v: Value = serde_json::from_str(fixture).expect("fixture");
    edit(&mut v);
    serde_json::to_vec(&v).expect("json")
}

fn ls_body() -> Vec<u8> {
    LEMONSQUEEZY_ORDER_CREATED.as_bytes().to_vec()
}

fn paddle_body() -> Vec<u8> {
    PADDLE_TRANSACTION_COMPLETED.as_bytes().to_vec()
}

/// The same body with exactly one byte changed, the signature left as it was.
fn one_byte_altered(body: &[u8], from: &str, to: &str) -> Vec<u8> {
    assert_eq!(from.len(), to.len());
    let text = std::str::from_utf8(body).expect("utf8");
    assert!(text.contains(from), "{from}");
    let out = text.replacen(from, to, 1).into_bytes();
    let differing = body.iter().zip(&out).filter(|(a, b)| a != b).count();
    assert_eq!(differing, 1, "exactly one byte differs");
    out
}

#[test]
fn every_fixture_is_labelled_as_synthesised_and_carries_no_secret_shape() {
    for f in [LEMONSQUEEZY_ORDER_CREATED, PADDLE_TRANSACTION_COMPLETED] {
        let v: Value = serde_json::from_str(f).expect("json");
        let label = v["_provenance"].as_str().expect("label");
        assert!(label.starts_with(PROVENANCE), "{label}");
        assert!(label.contains("unverified against a live account"));
        for shape in [
            "whsec_",
            "pdl_ntfset_",
            "pdl_live_",
            "pdl_sdbx_",
            "PRIVATE KEY",
            "signature",
        ] {
            assert!(!f.contains(shape), "a fixture carries `{shape}`");
        }
    }
}

/* ── A: a documented-shape payload yields a licence T-DIST-4 accepts ───── */

#[test]
fn a_each_provider_yields_a_licence_that_t_dist_4_verify_accepts() {
    let mut rig = Rig::fixtures();
    let t = now();
    for (issued, plan) in [
        (rig.ls(&ls_body(), t).expect("lemonsqueezy"), Plan::Pro),
        (rig.paddle(&paddle_body(), t).expect("paddle"), Plan::Team),
    ] {
        assert!(!issued.replayed);
        match rig.verdict(&issued.licence, t, &Fingerprint::Unavailable) {
            Verdict::Valid(l) => {
                assert_eq!(l.plan, plan);
                assert_eq!(l.licence_id, issued.licence_id);
                assert!(
                    !l.machine_bound,
                    "no fingerprint was given, so none is bound"
                );
                assert_eq!(l.issued_at, thruline_licence::timestamp(t));
                assert_eq!(l.expires_at, thruline_licence::timestamp(t + 365 * 86_400));
            }
            other => panic!("{:?} for {}", other, issued.provider),
        }
        // A year and a day later, the same file has expired: the duration is real.
        assert!(matches!(
            rig.verdict(&issued.licence, t + 366 * 86_400, &Fingerprint::Unavailable),
            Verdict::Refused(Problem::Expired { .. })
        ));
    }
    // A key the verifier does not hold is refused: the key is the real one.
    let stranger = SigningKey::generate(&mut OsRng).verifying_key().to_bytes();
    let only_stranger = TrustedKey::from_bytes(&stranger).expect("key");
    let issued = rig.ls(&ls_body(), t).expect("replay");
    assert_eq!(
        verify(
            &issued.licence,
            &[only_stranger],
            t,
            &Fingerprint::Unavailable
        ),
        Verdict::Refused(Problem::UnknownKey)
    );
}

#[test]
fn a_a_fingerprint_given_at_checkout_ties_the_licence_to_that_machine() {
    let mut rig = Rig::fixtures();
    let t = now();
    let mine = "ab".repeat(32);
    let body = edited(LEMONSQUEEZY_ORDER_CREATED, |v| {
        v["meta"]["custom_data"]["machine"] = Value::String(mine.clone());
    });
    let issued = rig.ls(&body, t).expect("issued");
    match rig.verdict(&issued.licence, t, &Fingerprint::Available(mine.clone())) {
        Verdict::Valid(l) => assert!(l.machine_bound),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        rig.verdict(&issued.licence, t, &Fingerprint::Available("cd".repeat(32))),
        Verdict::Refused(Problem::WrongMachine)
    );
    // A fingerprint that is not one is refused, and nothing is written.
    let before = rig.census();
    let body = edited(PADDLE_TRANSACTION_COMPLETED, |v| {
        v["data"]["custom_data"] = serde_json::json!({ "machine": "not-a-fingerprint" });
    });
    assert_eq!(rig.paddle(&body, t), Err(Refusal::BadMachine));
    assert_eq!(rig.census(), before);
}

/* ── B: one byte altered fails, and writes nothing ─────────────────────── */

#[test]
fn b_one_byte_altered_fails_the_signature_and_the_store_is_unchanged() {
    let mut rig = Rig::fixtures();
    let t = now();
    // Something already in the store, so "unchanged" is not "still empty".
    let seed_ls = edited(LEMONSQUEEZY_ORDER_CREATED, |v| {
        v["data"]["id"] = Value::from("100002");
        v["meta"]["webhook_id"] = Value::from("cccccccc-0000-4000-8000-00000000a0aa");
    });
    let seed_paddle = edited(PADDLE_TRANSACTION_COMPLETED, |v| {
        v["data"]["id"] = Value::from("txn_01j8synthesised0000000002");
        v["event_id"] = Value::from("evt_01j8synthesised0000000002");
    });
    rig.ls(&seed_ls, t).expect("seed ls");
    rig.paddle(&seed_paddle, t).expect("seed paddle");
    let before = rig.census();
    assert_eq!((before.0, before.1), (2, 2));

    // Lemon Squeezy: signed over the real body, delivered with one byte changed.
    let body = ls_body();
    let headers = lemonsqueezy::sign(&rig.secret, &body);
    let altered = one_byte_altered(&body, "\"total\": 9900", "\"total\": 9901");
    let secret = rig.live();
    assert_eq!(
        rig.issuer
            .handle("lemonsqueezy", &secret, &headers, &altered, t),
        Err(Refusal::BadSignature)
    );
    assert_eq!(rig.census(), before, "a bad signature wrote to the store");

    // Paddle: the same, over `ts:body`.
    let body = paddle_body();
    let headers = paddle::sign(&rig.secret, &body, t);
    let altered = one_byte_altered(&body, "\"total\": \"29900\"", "\"total\": \"29901\"");
    assert_eq!(
        rig.issuer.handle("paddle", &secret, &headers, &altered, t),
        Err(Refusal::BadSignature)
    );
    assert_eq!(rig.census(), before, "a bad signature wrote to the store");

    // The unaltered bodies, with the same headers, do issue: the refusals
    // above were the altered byte and nothing else.
    let body = ls_body();
    let headers = lemonsqueezy::sign(&rig.secret, &body);
    let issued = rig
        .issuer
        .handle("lemonsqueezy", &secret, &headers, &body, t)
        .expect("the unaltered body issues");
    assert_eq!(
        (issued.order_id.as_str(), issued.replayed),
        ("100001", false)
    );
}

#[test]
fn b_a_missing_wrong_or_stale_signature_writes_nothing() {
    let mut rig = Rig::fixtures();
    let t = now();
    let before = rig.census();
    let secret = rig.live();
    let body = ls_body();
    let wrong_key = lemonsqueezy::sign(&fresh_secret(), &body);
    let cases: Vec<(&str, Headers, Refusal)> = vec![
        (
            "lemonsqueezy",
            Headers::new(),
            Refusal::MissingSignature("X-Signature"),
        ),
        ("lemonsqueezy", wrong_key, Refusal::BadSignature),
        (
            "lemonsqueezy",
            Headers::new().with("X-Signature", "zz"),
            Refusal::BadSignature,
        ),
        (
            "paddle",
            Headers::new(),
            Refusal::MissingSignature("Paddle-Signature"),
        ),
        (
            "paddle",
            paddle::sign(&rig.secret, &paddle_body(), t - paddle::TOLERANCE_SECS - 1),
            Refusal::StaleSignature(paddle::TOLERANCE_SECS),
        ),
        (
            "paddle",
            paddle::sign(&rig.secret, &paddle_body(), t + paddle::TOLERANCE_SECS + 1),
            Refusal::StaleSignature(paddle::TOLERANCE_SECS),
        ),
        (
            "paddle",
            Headers::new().with("Paddle-Signature", "h1=00"),
            Refusal::BadSignature,
        ),
    ];
    for (provider, headers, want) in cases {
        let body = if provider == "paddle" {
            paddle_body()
        } else {
            ls_body()
        };
        assert_eq!(
            rig.issuer.handle(provider, &secret, &headers, &body, t),
            Err(want)
        );
        assert_eq!(rig.census(), before);
    }
    // An empty secret verifies nothing.
    let headers = lemonsqueezy::sign(b"", &body);
    assert_eq!(
        rig.issuer.handle(
            "lemonsqueezy",
            &WebhookSecret::live(Vec::new()),
            &headers,
            &body,
            t
        ),
        Err(Refusal::BadSignature)
    );
    assert_eq!(rig.census(), before);
}

/* ── C: replay returns the first licence, byte for byte ───────────────── */

#[test]
fn c_replaying_the_same_event_returns_byte_identical_bytes_and_leaves_one_row() {
    let mut rig = Rig::fixtures();
    let t = now();
    let first = rig.ls(&ls_body(), t).expect("first");
    let again = rig.ls(&ls_body(), t + 90).expect("replay");
    assert!(again.replayed);
    assert_eq!(again.licence, first.licence, "a replay minted new bytes");
    assert_eq!(again.licence_id, first.licence_id);
    let (licences, events, _, _) = rig.census();
    assert_eq!((licences, events), (1, 1));

    let first = rig.paddle(&paddle_body(), t).expect("first");
    let again = rig.paddle(&paddle_body(), t + 90).expect("replay");
    assert_eq!(again.licence, first.licence, "a replay minted new bytes");
    let (licences, events, _, _) = rig.census();
    assert_eq!((licences, events), (2, 2));
}

#[test]
fn c_a_second_event_for_the_same_order_returns_the_first_licence() {
    let mut rig = Rig::fixtures();
    let t = now();

    let first = rig.ls(&ls_body(), t).expect("first");
    let second_event = edited(LEMONSQUEEZY_ORDER_CREATED, |v| {
        v["meta"]["webhook_id"] = Value::from("cccccccc-0000-4000-8000-00000000a0ff");
    });
    // An hour later: a second mint would carry a different issued_at, so
    // byte-identity here can only mean "the first licence, returned".
    let second = rig.ls(&second_event, t + 3_600).expect("second event");
    assert!(second.replayed);
    assert_eq!(
        second.licence, first.licence,
        "one order was issued two licences"
    );
    assert_eq!(second.licence_id, first.licence_id);
    assert_eq!(rig.issuer.store().licences().expect("count"), 1);
    assert_eq!(rig.issuer.store().events().expect("count"), 2);

    let first = rig.paddle(&paddle_body(), t).expect("first");
    let second_event = edited(PADDLE_TRANSACTION_COMPLETED, |v| {
        v["event_id"] = Value::from("evt_01j8synthesised00000000ff");
    });
    let second = rig.paddle(&second_event, t + 3_600).expect("second event");
    assert_eq!(
        second.licence, first.licence,
        "one order was issued two licences"
    );
    assert_eq!(rig.issuer.store().licences().expect("count"), 2);

    // And the LS event with no webhook_id falls back to the body's hash: the
    // same bytes are the same event, and the order key still holds.
    let no_id = edited(LEMONSQUEEZY_ORDER_CREATED, |v| {
        v["meta"]
            .as_object_mut()
            .expect("meta")
            .remove("webhook_id");
    });
    let third = rig.ls(&no_id, t + 7_200).expect("no webhook id");
    assert_eq!(third.licence, rig.ls(&ls_body(), t).expect("again").licence);
    assert_eq!(rig.issuer.store().licences().expect("count"), 2);
}

/* ── D: unknown product and test mode, each refused in its own words ──── */

#[test]
fn d_an_unknown_product_is_refused_never_defaulted_and_writes_nothing() {
    let mut rig = Rig::fixtures();
    let t = now();
    let before = rig.census();
    let body = edited(LEMONSQUEEZY_ORDER_CREATED, |v| {
        v["data"]["attributes"]["first_order_item"]["variant_id"] = Value::from(59_999);
    });
    let err = rig
        .ls(&body, t)
        .expect_err("an unknown variant was issued a licence");
    assert_eq!(
        err,
        Refusal::UnknownProduct {
            provider: "lemonsqueezy".into(),
            id: "59999".into()
        }
    );
    assert_eq!(
        err.to_string(),
        "No plan is set up for lemonsqueezy product \"59999\", so nothing was issued. \
         An unknown product is never given a plan by default; add it to products.toml."
    );
    let body = edited(PADDLE_TRANSACTION_COMPLETED, |v| {
        v["data"]["items"][0]["price"]["id"] = Value::from("pri_01j8synthesisedgold000001");
    });
    assert!(matches!(
        rig.paddle(&body, t),
        Err(Refusal::UnknownProduct { .. })
    ));
    assert_eq!(rig.census(), before);
    // Under the production mapping every fixture id is unknown, too.
    let mut prod = Rig::new(thruline_licence_issue::config::PRODUCTION);
    assert!(matches!(
        prod.ls(&ls_body(), t),
        Err(Refusal::UnknownProduct { .. })
    ));
    assert!(matches!(
        prod.paddle(&paddle_body(), t),
        Err(Refusal::UnknownProduct { .. })
    ));
}

#[test]
fn d_a_test_mode_event_is_refused_under_the_production_mapping() {
    let mut prod = Rig::new(thruline_licence_issue::config::PRODUCTION);
    let t = now();
    let before = prod.census();
    // Lemon Squeezy marks test mode in the body. The id is a mapped one, so
    // test mode is the only reason left to refuse.
    let body = edited(LEMONSQUEEZY_ORDER_CREATED, |v| {
        v["meta"]["test_mode"] = Value::Bool(true);
        v["data"]["attributes"]["test_mode"] = Value::Bool(true);
        v["data"]["attributes"]["first_order_item"]["variant_id"] =
            Value::from("PLACEHOLDER-lemonsqueezy-variant-pro-one-year");
    });
    let err = prod
        .ls(&body, t)
        .expect_err("a test-mode payment was issued");
    assert_eq!(err, Refusal::TestMode);
    assert_eq!(
        err.to_string(),
        "This is a test-mode payment, and this issuer is set up for real payments only, \
         so nothing was issued."
    );
    // The same body, not in test mode, IS issued: the refusal was test mode.
    let live = edited(LEMONSQUEEZY_ORDER_CREATED, |v| {
        v["data"]["attributes"]["first_order_item"]["variant_id"] =
            Value::from("PLACEHOLDER-lemonsqueezy-variant-pro-one-year");
    });
    assert_eq!(prod.census(), before);
    assert!(prod.ls(&live, t).is_ok());

    // Paddle has no body flag; an event verified with a sandbox secret is test mode.
    let body = edited(PADDLE_TRANSACTION_COMPLETED, |v| {
        v["data"]["items"][0]["price"]["id"] =
            Value::from("PLACEHOLDER-paddle-price-team-one-year");
    });
    let headers = paddle::sign(&prod.secret, &body, t);
    let sandbox = WebhookSecret::sandbox(prod.secret.clone());
    let before = prod.census();
    assert_eq!(
        prod.issuer.handle("paddle", &sandbox, &headers, &body, t),
        Err(Refusal::TestMode)
    );
    assert_eq!(prod.census(), before);
    assert_ne!(
        Refusal::TestMode.to_string(),
        Refusal::UnknownProduct {
            provider: "paddle".into(),
            id: "x".into()
        }
        .to_string()
    );
}

/* ── the rest of the path ──────────────────────────────────────────────── */

#[test]
fn an_event_that_is_not_a_payment_issues_nothing_and_writes_nothing() {
    let mut rig = Rig::fixtures();
    let t = now();
    let before = rig.census();
    let refunded = edited(LEMONSQUEEZY_ORDER_CREATED, |v| {
        v["meta"]["event_name"] = Value::from("order_refunded");
    });
    assert_eq!(
        rig.ls(&refunded, t),
        Err(Refusal::NotAPayment("order_refunded".into()))
    );
    let pending = edited(LEMONSQUEEZY_ORDER_CREATED, |v| {
        v["data"]["attributes"]["status"] = Value::from("pending");
    });
    assert!(matches!(rig.ls(&pending, t), Err(Refusal::NotAPayment(_))));
    let created = edited(PADDLE_TRANSACTION_COMPLETED, |v| {
        v["event_type"] = Value::from("transaction.created");
    });
    assert_eq!(
        rig.paddle(&created, t),
        Err(Refusal::NotAPayment("transaction.created".into()))
    );
    assert_eq!(Refusal::NotAPayment("x".into()).http_status(), 202);
    assert_eq!(rig.census(), before);
}

#[test]
fn a_provider_off_the_providers_line_is_refused() {
    let only_paddle = "providers = [\"paddle\"]\naccept_test_mode = false\n";
    let mut rig = Rig::new(only_paddle);
    let t = now();
    assert_eq!(
        rig.ls(&ls_body(), t),
        Err(Refusal::UnknownProvider("lemonsqueezy".into()))
    );
    let secret = rig.live();
    assert_eq!(
        rig.issuer
            .handle("stripe", &secret, &Headers::new(), b"{}", t),
        Err(Refusal::UnknownProvider("stripe".into()))
    );
}

/* ── the store: versioned, append-only ─────────────────────────────────── */

#[test]
fn the_store_refuses_a_newer_schema_and_reads_a_hand_built_v1() {
    let dir = tempfile::tempdir().expect("tempdir");
    let newer = dir.path().join("newer.sqlite");
    {
        let c = rusqlite::Connection::open(&newer).expect("open");
        c.pragma_update(None, "user_version", 2).expect("version");
    }
    let err = Store::open(&newer).err().expect("a newer store was opened");
    assert!(err.to_string().contains("newer issuer"), "{err}");

    // Compatibility: a v1 file built by hand from the committed DDL, with a
    // row in it, opens and answers.
    let v1 = dir.path().join("v1.sqlite");
    {
        let c = rusqlite::Connection::open(&v1).expect("open");
        c.execute_batch(thruline_licence_issue::store::SCHEMA_V1)
            .expect("ddl");
        c.execute(
            "INSERT INTO licences VALUES ('paddle','txn_x','01J8ZQ4Y6T3M2N1P0R9S8T7V6W','evt_x',X'4142',1)",
            [],
        )
        .expect("row");
        c.pragma_update(None, "user_version", 1).expect("version");
    }
    let store = Store::open(&v1).expect("v1 opens");
    assert_eq!(store.licences().expect("count"), 1);
    assert_eq!(
        store.licence_for("paddle", "txn_x").expect("read"),
        Some(b"AB".to_vec())
    );
}

#[test]
fn the_store_is_append_only() {
    let mut rig = Rig::fixtures();
    rig.ls(&ls_body(), now()).expect("issued");
    let c = rusqlite::Connection::open(&rig.db).expect("open");
    for sql in [
        "UPDATE licences SET licence = X'00'",
        "DELETE FROM licences",
        "UPDATE events SET order_id = 'x'",
        "DELETE FROM events",
    ] {
        assert!(c.execute(sql, []).is_err(), "{sql} succeeded");
    }
    assert_eq!(rig.issuer.store().licences().expect("count"), 1);
}
