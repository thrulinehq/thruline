//! A187 — the adjudication store: append-only, human-provenanced, and unable
//! to invent or overwrite a judgment.
//!
//! The CLI loop itself cannot be exercised here — it refuses to record without
//! an interactive terminal, which is the point of it — so these drive the store
//! directly on an explicitly synthetic specimen. The operator's real batch is
//! never touched by this file: every specimen below is constructed in a
//! throwaway ledger and is marked as a test specimen in its own subject.

use thruline_core::adjudicate::{
    counted_by_class, record, unreachable_batches, verdict_for, Specimen, Verdict,
};
use thruline_core::ledger::Ledger;

const WHO: &str = "test-operator";
const ROUTE: &str = "test:a187";

fn ledger() -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    (dir, ledger)
}

fn specimen(subject: &str) -> Specimen {
    Specimen::new(
        "crash_candidate",
        subject,
        "no-close-and-quiet",
        "a test specimen, constructed here and belonging to no real batch",
        vec!["TEST SPECIMEN — not drawn from the operator's pending batch".into()],
    )
}

#[test]
fn a_verdict_carries_who_when_and_by_what_route() {
    let (_d, l) = ledger();
    let s = specimen("TEST-SPECIMEN-provenance");
    assert!(record(&l, &s, Verdict::Agree, WHO, ROUTE, 1_700_000_000_000).expect("record"));

    let got = verdict_for(&l, &s.id).expect("read").expect("a verdict");
    assert_eq!(got.verdict, Verdict::Agree);
    assert_eq!(
        got.operator, WHO,
        "a verdict with no human on it is not one"
    );
    assert_eq!(
        got.route, ROUTE,
        "the route must be recorded, never inferred"
    );
    assert_eq!(got.adjudicated_at_unix_ms, 1_700_000_000_000);
    assert_eq!(
        got.class, "crash_candidate",
        "the class travels with the row"
    );
}

#[test]
fn a_recorded_verdict_is_never_silently_overwritten() {
    // Idempotent by REFUSAL, not by overwrite. Re-running the tool must not
    // re-ask what was answered, and must never quietly replace what a person
    // decided with a later answer.
    let (_d, l) = ledger();
    let s = specimen("TEST-SPECIMEN-idempotence");
    assert!(record(&l, &s, Verdict::Agree, WHO, ROUTE, 1).expect("first"));
    assert!(
        !record(&l, &s, Verdict::Disagree, "someone-else", ROUTE, 2).expect("second"),
        "a second verdict on the same specimen must be refused, not applied"
    );
    let got = verdict_for(&l, &s.id).expect("read").expect("a verdict");
    assert_eq!(got.verdict, Verdict::Agree, "the original judgment stands");
    assert_eq!(got.operator, WHO);
}

#[test]
fn the_store_is_append_only_under_the_triggers() {
    let (_d, l) = ledger();
    let s = specimen("TEST-SPECIMEN-append-only");
    record(&l, &s, Verdict::Agree, WHO, ROUTE, 1).expect("record");

    let update = l
        .conn()
        .execute("UPDATE adjudications SET verdict = 'disagree'", []);
    assert!(update.is_err(), "an adjudication was updated");
    let delete = l.conn().execute("DELETE FROM adjudications", []);
    assert!(delete.is_err(), "an adjudication was deleted");

    // And the row is still exactly as it was.
    let got = verdict_for(&l, &s.id).expect("read").expect("a verdict");
    assert_eq!(got.verdict, Verdict::Agree);
}

#[test]
fn abstention_is_recorded_as_itself_and_is_never_a_pass() {
    // C4.3's precedent. "I cannot tell from this" is a real human answer about
    // the evidence: it is stored, it is distinguishable from agree and from
    // disagree, and it does NOT count toward a graduation bar — a bar of 300
    // met by 300 abstentions would be a bar met by looking away.
    let (_d, l) = ledger();
    let abstained = specimen("TEST-SPECIMEN-abstained");
    let agreed = specimen("TEST-SPECIMEN-agreed");
    record(&l, &abstained, Verdict::InsufficientEvidence, WHO, ROUTE, 1).expect("record");
    record(&l, &agreed, Verdict::Agree, WHO, ROUTE, 2).expect("record");

    let stored = verdict_for(&l, &abstained.id)
        .expect("read")
        .expect("abstention is stored, not dropped");
    assert_eq!(stored.verdict, Verdict::InsufficientEvidence);

    let counted = counted_by_class(&l).expect("counts");
    assert_eq!(
        counted.get("crash_candidate").copied(),
        Some(1),
        "only the agreement counts toward graduation; the abstention must not"
    );
}

#[test]
fn an_abstention_still_stops_the_tool_re_asking() {
    // Abstention is an answer, so it is not the same as skipping: a specimen
    // that was abstained on is no longer pending, and the operator is not asked
    // it again on the next run.
    let (_d, l) = ledger();
    let s = specimen("TEST-SPECIMEN-abstention-is-answered");
    record(&l, &s, Verdict::InsufficientEvidence, WHO, ROUTE, 1).expect("record");
    assert!(
        verdict_for(&l, &s.id).expect("read").is_some(),
        "an abstained specimen must count as answered for resumability"
    );
}

#[test]
fn the_batch_it_cannot_serve_is_named_rather_than_omitted() {
    let named = unreachable_batches();
    assert!(
        !named.is_empty(),
        "silence about an unreachable batch is the false calm"
    );
    for (batch, why) in named {
        assert!(!batch.is_empty() && !why.is_empty());
        assert!(
            why.len() > 60,
            "an unreachable batch states its reason, not a shrug: {batch}"
        );
    }
}
