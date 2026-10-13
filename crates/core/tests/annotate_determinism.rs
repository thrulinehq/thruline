//! M3.5: with annotations frozen, derivation stays byte-identical.
//!
//! Runs in the default suite — the scorer here is a deterministic stub, so no
//! model download is needed. That is the point: determinism is a property of
//! derivation, not of the model, and it must be provable without one.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use thruline_core::annotate::{annotate, extract_heuristic_claims};
use thruline_core::config::Config;
use thruline_core::derive::{derive_project_with, fingerprint};
use thruline_core::export::export_project;
use thruline_core::ingest::ingest;
use thruline_core::ledger::Ledger;
use thruline_core::nli::{EntailmentScorer, Scores};
use thruline_core::Result;

const PROJ: &str = "-Users-dev-annot";
const S: &str = "bbbbbbbb-1111-4222-8333-444444444444";

/// Scores from the premise's own bytes: no randomness, no model, but not a
/// constant either — different premises must produce different scores or the
/// determinism claim would be vacuous.
struct StubScorer;

impl EntailmentScorer for StubScorer {
    fn score(&mut self, premise: &str, hypothesis: &str) -> Result<Scores> {
        let n = premise.len() as f32 + hypothesis.len() as f32;
        let e = ((n * 7.0) % 100.0) / 100.0;
        let c = ((n * 13.0) % 100.0) / 100.0;
        let rest = (1.0 - e).max(0.0);
        Ok(Scores {
            entailment: e,
            contradiction: c.min(rest),
            neutral: (rest - c.min(rest)).max(0.0),
        })
    }
}

fn u(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

fn transcript() -> String {
    let mut l: Vec<String> = Vec::new();
    l.push(format!(
        r#"{{"type":"user","uuid":"{}","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-29T00:00:00.000Z","message":{{"role":"user","content":"run it"}}}}"#,
        u(1)
    ));
    l.push(format!(
        r#"{{"type":"assistant","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-29T00:00:01.000Z","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"t1","name":"Bash","input":{{"command":"cargo test"}}}}]}}}}"#,
        u(2),
        u(1)
    ));
    l.push(format!(
        r#"{{"type":"user","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-29T00:00:02.000Z","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t1","content":"test result: ok. 12 passed; 0 failed"}}]}}}}"#,
        u(3),
        u(2)
    ));
    l.push(format!(
        r#"{{"type":"assistant","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-29T00:00:03.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":"All 12 tests pass. I've also updated `src/db/schema.sql`."}}]}}}}"#,
        u(4),
        u(3)
    ));
    l.join("\n") + "\n"
}

fn setup() -> (tempfile::TempDir, Ledger) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    let project = root.join(PROJ);
    fs::create_dir_all(&project).expect("mkdir");
    fs::write(project.join(format!("{S}.jsonl")), transcript()).expect("write");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    ingest(
        &mut ledger,
        std::slice::from_ref(&PathBuf::from(&root)),
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    (dir, ledger)
}

#[test]
fn derivation_with_frozen_annotations_is_byte_identical() {
    let (_d, ledger) = setup();
    let cfg = Config::default();

    let first = derive_project_with(&ledger, PROJ, &cfg).expect("derive");
    let before = fingerprint(&first);
    // No annotations yet: nothing may be accused.
    for n in &first.nodes {
        assert!(
            !n.sort_key.is_accusation(),
            "the deterministic tier accused something without annotations: {:?}",
            n.sort_key
        );
    }

    let r = annotate(&ledger, PROJ, &cfg, &mut StubScorer, "stub-v1").expect("annotate");
    assert!(r.claims_written > 0, "no claims extracted from the fixture");
    assert!(r.links_written > 0, "no evidence links written");

    let after_first = fingerprint(&derive_project_with(&ledger, PROJ, &cfg).expect("derive"));
    assert_ne!(
        before, after_first,
        "annotations changed nothing — the entailment tier is not wired into derivation"
    );

    // The actual invariant: same events + same annotations → same bytes.
    let after_second = fingerprint(&derive_project_with(&ledger, PROJ, &cfg).expect("derive"));
    assert_eq!(
        after_first, after_second,
        "derivation is not deterministic with frozen annotations"
    );

    // And re-running annotate must be a no-op, not an append.
    let again = annotate(&ledger, PROJ, &cfg, &mut StubScorer, "stub-v1").expect("re-annotate");
    let after_third = fingerprint(&derive_project_with(&ledger, PROJ, &cfg).expect("derive"));
    assert_eq!(
        after_first, after_third,
        "re-running annotate with the same extractor and model changed the graph — \
         idempotency on (event, extractor, model, input-hash) is broken"
    );
    assert_eq!(
        again.claims_written, r.claims_written,
        "claim count drifted across identical runs"
    );
}

#[test]
fn a_changed_model_appends_rather_than_colliding() {
    // §5: "changed model appends". The scorer is part of the evidence_links
    // key, so a second model's verdicts sit beside the first's instead of
    // silently overwriting them and making the old verdict unexplainable.
    let (_d, ledger) = setup();
    let cfg = Config::default();
    // These tags deliberately hash in reverse chronological order for this
    // project (`scorer-0`'s annotation id sorts after `scorer-1`'s). A query
    // ordered by timestamp/id would choose the wrong run when both land in one
    // second; the explicit projection head must choose the actual later run.
    annotate(&ledger, PROJ, &cfg, &mut StubScorer, "scorer-0").expect("annotate v1");
    let after_v1: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM evidence_links", [], |r| r.get(0))
        .expect("count");
    annotate(&ledger, PROJ, &cfg, &mut StubScorer, "scorer-1").expect("annotate v2");
    let after_v2: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM evidence_links", [], |r| r.get(0))
        .expect("count");
    assert_eq!(
        after_v2,
        after_v1 * 2,
        "a second model must append its own links, not overwrite the first's"
    );
    assert_eq!(
        ledger
            .active_entailment_scorer(PROJ)
            .expect("active scorer"),
        Some("scorer-1".to_string())
    );
    let graph = derive_project_with(&ledger, PROJ, &cfg).expect("derive active scorer");
    let export = export_project(&ledger, &graph, "2026-07-31T03:00:00Z").expect("export");
    let receipts = export["receipts"].as_object().expect("receipts");
    assert!(!receipts.is_empty());
    assert!(receipts
        .values()
        .all(|receipt| receipt["verdict"]["model"] == "scorer-1"));
}

#[test]
fn a_new_same_extractor_claim_invalidates_the_old_entailment_head() {
    let (dir, mut ledger) = setup();
    let cfg = Config::default();
    annotate(&ledger, PROJ, &cfg, &mut StubScorer, "before-append").expect("initial annotate");
    assert!(ledger
        .active_entailment_scorer(PROJ)
        .expect("initial head")
        .is_some());

    let root = dir.path().join("projects");
    let source = root.join(PROJ).join(format!("{S}.jsonl"));
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(source)
        .expect("open transcript append");
    writeln!(
        file,
        r#"{{"type":"assistant","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-29T00:00:04.000Z","message":{{"role":"assistant","content":[{{"type":"text","text":"All 13 integration tests pass."}}]}}}}"#,
        u(5),
        u(4),
    )
    .expect("append claim");
    ingest(&mut ledger, &[root], &dir.path().join("mirrors")).expect("ingest appended claim");
    let appended_events: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE project_id = ?1 AND text LIKE '%13 integration%'",
            [PROJ],
            |row| row.get(0),
        )
        .expect("appended event count");
    assert_eq!(appended_events, 1);
    let claims_before: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM claims", [], |row| row.get(0))
        .expect("claims before refresh");
    extract_heuristic_claims(&ledger, PROJ).expect("refresh deterministic projection");
    let claims_after: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM claims", [], |row| row.get(0))
        .expect("claims after refresh");
    assert!(claims_after > claims_before, "new claim was not projected");

    assert_eq!(
        ledger.active_entailment_scorer(PROJ).expect("cleared head"),
        None
    );
    let links: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM evidence_links", [], |row| row.get(0))
        .expect("link count");
    assert_eq!(links, 0, "partial old scores survived a new claim");
}
