//! M6.R1.2 / M6.R1.3 — the two questions.
//!
//! *What needs me?* and *why should I believe it?*, over a real ledger built by
//! a real ingest. These assert the properties A80 gates: the first-level voice
//! over rendered output, shadow containment under every flag, acknowledgment
//! semantics end to end, receipt reachability, refusal of ambiguity, and the
//! structural honesty of the suggested actions.
//!
//! No model is loaded. `nodes` is a derived projection, so the contradiction
//! fixture writes one directly — that is what "derived" means, and it lets the
//! contradiction path be exercised without the scorer.

use std::fs;
use std::path::PathBuf;

use thruline_core::attention::{acknowledge, AcknowledgmentAction, AttentionKind};
use thruline_core::ingest::ingest;
use thruline_core::ledger::Ledger;
use thruline_core::now::{now, why, Coverage, Resolution};
use thruline_core::voice;

const PROJ: &str = "-Users-dev-nowwhy";
const S: &str = "bbbbbbbb-2222-4333-8444-555555555555";

fn u(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

fn transcript() -> String {
    let mut l: Vec<String> = Vec::new();
    l.push(format!(
        r#"{{"type":"user","uuid":"{}","parentUuid":null,"isSidechain":false,"sessionId":"{S}","timestamp":"2026-08-01T00:00:00.000Z","cwd":"/repo","message":{{"role":"user","content":"run the suite"}}}}"#,
        u(1)
    ));
    l.push(format!(
        r#"{{"type":"assistant","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-08-01T00:00:01.000Z","cwd":"/repo","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"t1","name":"Bash","input":{{"command":"cargo test --all"}}}}]}}}}"#,
        u(2),
        u(1)
    ));
    l.push(format!(
        r#"{{"type":"user","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-08-01T00:00:02.000Z","cwd":"/repo","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t1","content":"running 33 tests\ntest result: FAILED. 31 passed; 2 failed; 0 ignored"}}]}}}}"#,
        u(3),
        u(2)
    ));
    l.push(format!(
        r#"{{"type":"assistant","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-08-01T00:00:03.000Z","cwd":"/repo","message":{{"role":"assistant","content":[{{"type":"text","text":"33/33 tests pass."}}]}}}}"#,
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

/// A current, contradicted claim in the derived projection. `label` is the
/// project's own words, which is exactly what `why` quotes.
fn with_contradiction(ledger: &Ledger, evidence: &str) -> String {
    let anchor: String = ledger
        .conn()
        .query_row(
            "SELECT id FROM events WHERE project_id=?1 AND kind='assistant_text'
             ORDER BY seq DESC LIMIT 1",
            [PROJ],
            |r| r.get(0),
        )
        .expect("an assistant_text event to anchor to");
    ledger
        .conn()
        .execute(
            "INSERT OR REPLACE INTO nodes(id, project_id, label, sort_key, lifecycle,
                 evidence_state, currency, confidence, session_id, anchor_event_id, derived_rev)
             VALUES('n1', ?1, '33/33 tests pass.', 'contradicted', 'completed',
                    ?2, 'current', 0.9, ?3, ?4, 1)",
            rusqlite::params![PROJ, evidence, S, anchor],
        )
        .expect("write the derived node");
    anchor
}

/// Every string a person actually reads, from both surfaces, under both flags.
fn first_level_strings(ledger: &Ledger) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for all in [false, true] {
        for p in now(ledger, None, all, Coverage::Complete).expect("now") {
            out.push((format!("headline (all={all})"), p.headline));
            if let Some(note) = p.hidden_note {
                out.push((format!("hidden note (all={all})"), note));
            }
            for item in p.items {
                out.push((format!("sentence {}", item.id), item.sentence));
                out.push((format!("action {}", item.id), item.action));
                if let Resolution::One(w) = why(ledger, &item.id).expect("why") {
                    out.push((format!("why level 1 {}", w.id), w.sentence));
                    out.push((format!("why action {}", w.id), w.action));
                    out.push((format!("why level 2 {}", w.id), w.what_happened));
                }
            }
        }
    }
    out
}

#[test]
fn now_speaks_plain_english_with_one_sentence_and_one_action_per_item() {
    let (_d, ledger) = setup();
    with_contradiction(&ledger, "contradicted");
    let view = now(&ledger, Some(PROJ), false, Coverage::Complete).expect("now");
    assert_eq!(view.len(), 1, "one project, one headline");
    let p = &view[0];
    assert!(
        p.headline.contains("1 thing worth a look"),
        "the headline must carry its noun: {:?}",
        p.headline
    );
    assert!(
        p.headline.ends_with("Everything else checks out."),
        "the doctrine's headline shape: {:?}",
        p.headline
    );
    assert_eq!(p.items.len(), 1);
    let item = &p.items[0];
    assert_eq!(item.kind, AttentionKind::Contradiction);
    assert_eq!(
        item.sentence,
        voice::verdict("contradicted").unwrap().sentence
    );
    assert_eq!(item.action, "Look at the receipt.");
    assert!(item.dismissed_at_unix_ms.is_none());
    assert!(p.hidden_note.is_none(), "no zero-count line ever renders");
}

#[test]
fn the_calm_state_renders_an_item_never_a_blank_screen() {
    let (_d, ledger) = setup();
    // Calm must be EARNED: build the projection first. Before A94 this test
    // passed without it, which is how "everything checks out" came to ship for
    // projects nothing had ever read.
    thruline_core::derive::rebuild(&ledger, PROJ, &thruline_core::config::Config::default())
        .expect("rebuild");
    let view = now(&ledger, Some(PROJ), false, Coverage::Complete).expect("now");
    let p = &view[0];
    assert!(p.headline.ends_with("everything checks out."));
    assert_eq!(p.items.len(), 1, "calm is an item: {:?}", p.items);
    assert_eq!(p.items[0].kind, AttentionKind::AllClear);
    assert_eq!(p.items[0].action, "Nothing to do.");
}

#[test]
fn calm_is_refused_when_the_record_was_not_fully_watched() {
    // The same earned-calm fixture as above, differing only in what the caller
    // was able to say about coverage. Measured before this gate existed: 90 of
    // 92 exported projects read "everything checks out." while `doctor`
    // reported the shim was not on PATH, so nothing invoked it — the product
    // asserting a clean record over a machine it was barely reading.
    let (_d, ledger) = setup();
    thruline_core::derive::rebuild(&ledger, PROJ, &thruline_core::config::Config::default())
        .expect("rebuild");

    let calm = now(&ledger, Some(PROJ), false, Coverage::Complete).expect("now");
    assert!(
        calm[0].headline.ends_with("everything checks out."),
        "control: with complete coverage the calm sentence is still available"
    );

    let gated = now(&ledger, Some(PROJ), false, Coverage::Incomplete).expect("now");
    let h = &gated[0].headline;
    assert!(
        !h.contains("everything checks out"),
        "the calm claim must be unavailable when coverage is incomplete: {h}"
    );
    assert!(
        h.contains("not everything was watched"),
        "and the reason must be said, not merely omitted: {h}"
    );
    // Coverage changes what may be CLAIMED, never what was FOUND: the same
    // items, in the same order, with the same ids and kinds. Only the calm
    // item's wording is allowed to move, and it must not restate the claim the
    // headline just refused.
    assert_eq!(
        gated[0].items.len(),
        calm[0].items.len(),
        "a coverage gap must not invent or hide an attention item"
    );
    for (g, c) in gated[0].items.iter().zip(calm[0].items.iter()) {
        assert_eq!(
            g.id, c.id,
            "ids are derived from the record, not from coverage"
        );
        assert_eq!(g.kind, c.kind, "kinds are derived from the record");
    }
    let calm_item = &gated[0].items[0];
    assert!(
        !calm_item.sentence.contains("Everything checks out"),
        "the item must not restate the refused claim: {}",
        calm_item.sentence
    );
    assert_ne!(
        calm_item.action, "Nothing to do.",
        "there IS something to do: close the coverage gap"
    );
}

#[test]
fn dismissal_hides_by_default_shows_under_all_and_never_yields_a_blank_screen() {
    let (_d, ledger) = setup();
    with_contradiction(&ledger, "contradicted");
    let id = now(&ledger, Some(PROJ), false, Coverage::Complete).expect("now")[0].items[0]
        .id
        .clone();
    acknowledge(
        &ledger,
        &id,
        AcknowledgmentAction::Dismissed,
        1_785_000_000_000,
    )
    .expect("ack");

    // Default: hidden, counted, and the calm item takes its place.
    let default = now(&ledger, Some(PROJ), false, Coverage::Complete).expect("now");
    let p = &default[0];
    assert_eq!(p.dismissed_hidden, 1);
    assert_eq!(
        p.hidden_note.as_deref(),
        Some("1 dismissed item hidden — `thruline now --all` shows them."),
        "a calm headline over a hidden conflict must say so"
    );
    assert_eq!(p.items.len(), 1);
    assert_eq!(
        p.items[0].kind,
        AttentionKind::AllClear,
        "dismissing the last item must not produce a blank screen"
    );
    assert!(p.headline.ends_with("everything checks out."));

    // --all: shown, with its dismissal record.
    let all = now(&ledger, Some(PROJ), true, Coverage::Complete).expect("now --all");
    let p = &all[0];
    assert_eq!(p.items.len(), 1);
    assert_eq!(p.items[0].id, id);
    assert_eq!(p.items[0].dismissed_at_unix_ms, Some(1_785_000_000_000));
    assert!(
        p.headline.ends_with("everything checks out."),
        "a dismissed item is not a thing worth a look: {:?}",
        p.headline
    );
}

#[test]
fn a_changed_subject_resurfaces_as_a_new_id() {
    let (_d, ledger) = setup();
    with_contradiction(&ledger, "contradicted");
    let before = now(&ledger, Some(PROJ), false, Coverage::Complete).expect("now")[0].items[0]
        .id
        .clone();
    acknowledge(
        &ledger,
        &before,
        AcknowledgmentAction::Dismissed,
        1_785_000_000_000,
    )
    .expect("ack");
    assert_eq!(
        now(&ledger, Some(PROJ), false, Coverage::Complete).expect("now")[0].items[0].kind,
        AttentionKind::AllClear
    );

    // The subject changes: same node, different label. The digest changes, so
    // the id changes, so the dismissal no longer covers it.
    ledger
        .conn()
        .execute(
            "UPDATE nodes SET label='all 40 tests pass.' WHERE id='n1'",
            [],
        )
        .expect("re-derive the node");
    let after = now(&ledger, Some(PROJ), false, Coverage::Complete).expect("now");
    assert_eq!(after[0].items.len(), 1);
    assert_ne!(
        after[0].items[0].id, before,
        "id did not follow the subject"
    );
    assert_eq!(after[0].items[0].kind, AttentionKind::Contradiction);
    assert_eq!(after[0].dismissed_hidden, 0);
}

#[test]
fn why_discloses_three_levels_and_the_receipt_reaches_a_raw_line() {
    let (_d, ledger) = setup();
    let anchor = with_contradiction(&ledger, "contradicted");
    let id = now(&ledger, Some(PROJ), false, Coverage::Complete).expect("now")[0].items[0]
        .id
        .clone();

    let Resolution::One(view) = why(&ledger, &id).expect("why") else {
        panic!("an exact id must resolve to exactly one item");
    };
    // Level one is the same sentence `now` printed — one voice, not two.
    assert_eq!(
        view.sentence,
        voice::verdict("contradicted").unwrap().sentence
    );
    assert_eq!(view.action, "Look at the receipt.");
    // Level two is our prose plus the project's own words, kept apart.
    assert!(view
        .what_happened
        .starts_with("The record holds both the claim and a run that disagrees with it."));
    assert!(
        view.what_happened.contains("1 receipt"),
        "the receipt count must carry its noun: {:?}",
        view.what_happened
    );
    assert_eq!(view.quoted.as_deref(), Some("33/33 tests pass."));
    // Level three reaches the raw line on disk.
    assert_eq!(view.receipts.len(), 1);
    let r = &view.receipts[0];
    assert_eq!(r.event_id, anchor);
    assert!(
        r.raw_file.ends_with(&format!("{S}.jsonl")),
        "{}",
        r.raw_file
    );
    assert!(r.raw_line > 0, "a receipt without a line is not a receipt");
    assert_eq!(r.raw_sha256.len(), 64);
    assert_eq!(r.chain_hash.len(), 64);
    let line = fs::read_to_string(&r.raw_file)
        .expect("the receipt's file")
        .lines()
        .nth(r.raw_line as usize - 1)
        .expect("the receipt's line")
        .to_string();
    assert!(
        line.contains("33/33 tests pass."),
        "the receipt did not land on the claimed line"
    );
}

#[test]
fn why_takes_a_prefix_and_refuses_ambiguity_rather_than_guessing() {
    let (_d, ledger) = setup();
    with_contradiction(&ledger, "contradicted");
    let id = now(&ledger, Some(PROJ), false, Coverage::Complete).expect("now")[0].items[0]
        .id
        .clone();

    // An unambiguous prefix resolves.
    match why(&ledger, &id[..12]).expect("why by prefix") {
        Resolution::One(v) => assert_eq!(v.id, id),
        other => panic!("a unique prefix must resolve: {other:?}"),
    }
    // `att_` matches every item that exists. With a second item present that is
    // ambiguous, and ambiguity is refused with the candidates named.
    acknowledge(
        &ledger,
        "att_deadbeefdeadbeefdeadbeef",
        AcknowledgmentAction::Dismissed,
        1,
    )
    .expect("ack a second id");
    match why(&ledger, "att_").expect("why ambiguous") {
        Resolution::Ambiguous(ids) => {
            assert!(
                ids.len() >= 2,
                "ambiguity must name every candidate: {ids:?}"
            );
            assert!(ids.contains(&id));
        }
        other => panic!("an ambiguous prefix must be refused, not guessed: {other:?}"),
    }
    assert!(matches!(
        why(&ledger, "att_nothing_matches_this").expect("why missing"),
        Resolution::NotFound
    ));
    assert!(matches!(
        why(&ledger, "   ").expect("why blank"),
        Resolution::NotFound
    ));
}

#[test]
fn why_still_answers_for_dismissed_and_no_longer_deriving_items() {
    let (_d, ledger) = setup();
    with_contradiction(&ledger, "contradicted");
    let id = now(&ledger, Some(PROJ), false, Coverage::Complete).expect("now")[0].items[0]
        .id
        .clone();
    acknowledge(
        &ledger,
        &id,
        AcknowledgmentAction::Dismissed,
        1_785_000_000_000,
    )
    .expect("ack");

    // Dismissed: answered, with the dismissal on the record.
    let Resolution::One(view) = why(&ledger, &id).expect("why dismissed") else {
        panic!("a dismissed item must still be answerable");
    };
    assert_eq!(view.dismissed_at_unix_ms, Some(1_785_000_000_000));
    assert!(!view.no_longer_derives);

    // The subject changes, so this id stops deriving — and stays answerable.
    ledger
        .conn()
        .execute("UPDATE nodes SET label='something else' WHERE id='n1'", [])
        .expect("re-derive");
    let Resolution::One(view) = why(&ledger, &id).expect("why expired") else {
        panic!("history is never rewritten to make an answer unavailable");
    };
    assert!(view.no_longer_derives);
    assert_eq!(
        view.kind, None,
        "a kind is not recoverable — do not guess one"
    );
    assert_eq!(view.sentence, voice::no_longer_derives());
    assert_eq!(view.dismissed_at_unix_ms, Some(1_785_000_000_000));
}

#[test]
fn no_first_level_string_in_now_or_why_carries_engine_lexicon() {
    // §5v over RENDERED output rather than over the table: this is the check
    // that would have caught the four DOM-only leaks M6.R1.1 found by hand.
    let (_d, ledger) = setup();
    with_contradiction(&ledger, "contradicted");
    let id = now(&ledger, Some(PROJ), false, Coverage::Complete).expect("now")[0].items[0]
        .id
        .clone();
    acknowledge(
        &ledger,
        &id,
        AcknowledgmentAction::Dismissed,
        1_785_000_000_000,
    )
    .expect("ack");

    let strings = first_level_strings(&ledger);
    assert!(
        strings.len() >= 12,
        "the walk covered only {} strings",
        strings.len()
    );
    for (where_, text) in strings {
        if let Some(term) = voice::banned_in(&text) {
            panic!("engine lexicon {term:?} reached a first-level string.\n  where: {where_}\n  text:  {text:?}");
        }
    }
}

#[test]
fn shadow_classes_never_reach_now_or_why_under_any_flag() {
    // The shadow classes are the two the holdout has not graduated (A79/A80).
    // A node carrying one must produce no item at all — not a quieter item.
    let (_d, ledger) = setup();
    for shadow in ["unknown", "supported"] {
        // `nodes.evidence_state` cannot hold 'unsupported'/'dropped' (the CHECK
        // forbids them), which is containment at the schema. These stand in as
        // non-contradicted states: only 'contradicted' may raise an item.
        with_contradiction(&ledger, shadow);
        for all in [false, true] {
            let view = now(&ledger, None, all, Coverage::Complete).expect("now");
            let blob = serde_json::to_string(&view).expect("json");
            for term in ["unsupported", "dropped"] {
                assert!(
                    !blob.contains(term),
                    "a shadow class reached `now` (all={all}): {blob}"
                );
            }
            for p in &view {
                for item in &p.items {
                    assert_ne!(
                        item.kind,
                        AttentionKind::Contradiction,
                        "{shadow} must not raise a contradiction item"
                    );
                    let Resolution::One(w) = why(&ledger, &item.id).expect("why") else {
                        panic!("every listed item must be answerable");
                    };
                    let wb = serde_json::to_string(&w).expect("json");
                    for term in ["unsupported", "dropped"] {
                        assert!(!wb.contains(term), "a shadow class reached `why`: {wb}");
                    }
                }
            }
        }
    }
}

#[test]
fn both_surfaces_are_byte_identical_across_two_reads() {
    let (_d, ledger) = setup();
    with_contradiction(&ledger, "contradicted");
    for all in [false, true] {
        let a = serde_json::to_string(&now(&ledger, None, all, Coverage::Complete).expect("now"))
            .expect("json");
        let b = serde_json::to_string(&now(&ledger, None, all, Coverage::Complete).expect("now"))
            .expect("json");
        assert_eq!(a, b, "`now` is not deterministic (all={all})");
    }
    let id = now(&ledger, Some(PROJ), false, Coverage::Complete).expect("now")[0].items[0]
        .id
        .clone();
    let one = |()| match why(&ledger, &id).expect("why") {
        Resolution::One(v) => serde_json::to_string(&v).expect("json"),
        other => panic!("{other:?}"),
    };
    assert_eq!(one(()), one(()), "`why` is not deterministic");
}

#[test]
fn the_attention_path_cannot_execute_anything_or_probe_the_live_system() {
    // §5u structurally, at the source. A suggested action that could execute
    // would make the receipt proof of our own action rather than of the
    // project's history — so the two modules behind those actions are asserted
    // to contain no way to execute, spawn, poll, or reach the filesystem or
    // network. `recorded_readiness` (the bounded reconstruction) is required;
    // the CLI's live-probing `inspect_readiness` is forbidden here.
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut saw_bounded = false;
    for name in ["attention.rs", "now.rs"] {
        let src = fs::read_to_string(root.join(name)).expect("read the module");
        // Comments name the forbidden things on purpose; strip them first so
        // the scan reads code, not prose about code.
        let code: String = src
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in [
            "std::process",
            "Command::new",
            "std::net",
            "TcpStream",
            "ureq",
            "reqwest",
            "thread::spawn",
            "std::fs",
            "File::open",
            "inspect_readiness",
            "sleep",
            "interval",
        ] {
            assert!(
                !code.contains(forbidden),
                "{name} reaches for {forbidden:?} — the attention path must stay a \
                 read-only derivation over accepted records (§5u)"
            );
        }
        saw_bounded |= code.contains("recorded_readiness");
    }
    assert!(
        saw_bounded,
        "the bounded-reconstruction call is gone; readiness must not be probed live"
    );
}
