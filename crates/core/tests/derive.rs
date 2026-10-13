//! Derivation contract: determinism, state discipline, dedup, collapsing.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use thruline_core::derive::{
    derive_project, fingerprint, rebuild, Currency, DerivedGraph, DerivedNode, EvidenceState,
    Lifecycle, NodeDimensions, NodeState,
};
use thruline_core::export::{export_project, export_project_with_options, ExportOptions};
use thruline_core::ingest::ingest;
use thruline_core::ledger::Ledger;
use thruline_core::{EventKind, NormalizedEvent, ProjectRef, RawLineRef, SessionRef, Source};

const PROJ: &str = "-Users-dev-derive";

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

struct Scratch {
    _dir: tempfile::TempDir,
    root: PathBuf,
    db: PathBuf,
}

/// Build a transcript root from `(filename, contents)` pairs.
fn scratch(files: &[(&str, String)]) -> Scratch {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    let project = root.join(PROJ);
    fs::create_dir_all(&project).expect("mkdir");
    for (name, body) in files {
        fs::write(project.join(name), body).expect("write");
    }
    let db = dir.path().join("ledger.db");
    Scratch {
        _dir: dir,
        root,
        db,
    }
}

fn ingested(s: &Scratch) -> Ledger {
    let mut ledger = Ledger::open(&s.db).expect("open");
    ingest(
        &mut ledger,
        std::slice::from_ref(&s.root),
        &s.db.parent().unwrap().join("mirrors"),
    )
    .expect("ingest");
    ledger
}

/* ── transcript builders ─────────────────────────────────────────────── */

fn user_with_sidechain(
    uuid: &str,
    parent: Option<&str>,
    sid: &str,
    ts: &str,
    body: &str,
    is_sidechain: bool,
) -> String {
    let p = parent.map(|p| format!("\"{p}\"")).unwrap_or("null".into());
    format!(
        r#"{{"type":"user","uuid":"{uuid}","parentUuid":{p},"isSidechain":{is_sidechain},"sessionId":"{sid}","timestamp":"{ts}","message":{{"role":"user","content":"{body}"}}}}"#
    )
}

fn user(uuid: &str, parent: Option<&str>, sid: &str, ts: &str, body: &str) -> String {
    user_with_sidechain(uuid, parent, sid, ts, body, false)
}

fn text_with_sidechain(
    uuid: &str,
    parent: &str,
    sid: &str,
    ts: &str,
    body: &str,
    is_sidechain: bool,
) -> String {
    format!(
        r#"{{"type":"assistant","uuid":"{uuid}","parentUuid":"{parent}","isSidechain":{is_sidechain},"sessionId":"{sid}","timestamp":"{ts}","message":{{"role":"assistant","content":[{{"type":"text","text":"{body}"}}]}}}}"#
    )
}

fn text(uuid: &str, parent: &str, sid: &str, ts: &str, body: &str) -> String {
    text_with_sidechain(uuid, parent, sid, ts, body, false)
}

fn tool(
    uuid: &str,
    parent: &str,
    sid: &str,
    ts: &str,
    tid: &str,
    name: &str,
    input: &str,
) -> String {
    format!(
        r#"{{"type":"assistant","uuid":"{uuid}","parentUuid":"{parent}","isSidechain":false,"sessionId":"{sid}","timestamp":"{ts}","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"{tid}","name":"{name}","input":{input}}}]}}}}"#
    )
}

fn result(uuid: &str, parent: &str, sid: &str, ts: &str, tid: &str, out: &str) -> String {
    format!(
        r#"{{"type":"user","uuid":"{uuid}","parentUuid":"{parent}","isSidechain":false,"sessionId":"{sid}","timestamp":"{ts}","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{tid}","content":"{out}"}}]}}}}"#
    )
}

const S: &str = "aaaaaaaa-1111-4222-8333-444444444444";
fn u(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

fn node_by_anchor<'a>(graph: &'a DerivedGraph, uuid: &str) -> &'a DerivedNode {
    graph
        .nodes
        .iter()
        .find(|node| node.anchor_uuid.as_deref() == Some(uuid))
        .unwrap_or_else(|| panic!("missing node anchored at {uuid}"))
}

fn has_edge(graph: &DerivedGraph, from: &DerivedNode, to: &DerivedNode, kind: &str) -> bool {
    graph
        .edges
        .iter()
        .any(|edge| edge.from == from.id && edge.to == to.id && edge.kind == kind)
}

/// One turn: a proposal, then two edits to the same file, then an edit to a
/// different file, then a Bash command.
#[allow(clippy::vec_init_then_push)] // reads as a transcript, line by line
fn standard_transcript() -> String {
    let mut l = Vec::new();
    l.push(user(
        &u(1),
        None,
        S,
        "2026-07-29T00:00:00.000Z",
        "please do the thing",
    ));
    l.push(text(
        &u(2),
        &u(1),
        S,
        "2026-07-29T00:00:01.000Z",
        "I'll refactor the auth middleware.",
    ));
    l.push(tool(
        &u(3),
        &u(2),
        S,
        "2026-07-29T00:00:02.000Z",
        "t1",
        "Edit",
        r#"{"file_path":"/repo/src/auth.rs","old_string":"a","new_string":"b"}"#,
    ));
    l.push(result(
        &u(4),
        &u(3),
        S,
        "2026-07-29T00:00:03.000Z",
        "t1",
        "ok",
    ));
    l.push(tool(
        &u(5),
        &u(4),
        S,
        "2026-07-29T00:00:04.000Z",
        "t2",
        "Edit",
        r#"{"file_path":"/repo/src/auth.rs","old_string":"c","new_string":"d"}"#,
    ));
    l.push(result(
        &u(6),
        &u(5),
        S,
        "2026-07-29T00:00:05.000Z",
        "t2",
        "ok",
    ));
    l.push(tool(
        &u(7),
        &u(6),
        S,
        "2026-07-29T00:00:06.000Z",
        "t3",
        "Edit",
        r#"{"file_path":"/repo/src/lib.rs","old_string":"e","new_string":"f"}"#,
    ));
    l.push(result(
        &u(8),
        &u(7),
        S,
        "2026-07-29T00:00:07.000Z",
        "t3",
        "ok",
    ));
    l.push(tool(
        &u(9),
        &u(8),
        S,
        "2026-07-29T00:00:08.000Z",
        "t4",
        "Bash",
        r#"{"command":"cargo test --all"}"#,
    ));
    l.push(result(
        &u(10),
        &u(9),
        S,
        "2026-07-29T00:00:09.000Z",
        "t4",
        "214 passed",
    ));
    // Second turn: a proposal that never executes.
    l.push(user(
        &u(11),
        Some(&u(10)),
        S,
        "2026-07-29T00:01:00.000Z",
        "anything else?",
    ));
    l.push(text(
        &u(12),
        &u(11),
        S,
        "2026-07-29T00:01:01.000Z",
        "I'll add rate limiting next session.",
    ));
    // Third turn: a completed-work claim with no action at all.
    l.push(user(
        &u(13),
        Some(&u(12)),
        S,
        "2026-07-29T00:02:00.000Z",
        "and?",
    ));
    l.push(text(
        &u(14),
        &u(13),
        S,
        "2026-07-29T00:02:01.000Z",
        "I've updated `docs/plan.md` already.",
    ));
    l.join("\n") + "\n"
}

/* ── tests ───────────────────────────────────────────────────────────── */

#[test]
fn rebuild_twice_is_byte_identical() {
    let s = scratch(&[(&format!("{S}.jsonl"), standard_transcript())]);
    let ledger = ingested(&s);

    let first =
        rebuild(&ledger, PROJ, &thruline_core::config::Config::default()).expect("rebuild 1");
    let dump1 = dump_derived(&ledger);
    let second =
        rebuild(&ledger, PROJ, &thruline_core::config::Config::default()).expect("rebuild 2");
    let dump2 = dump_derived(&ledger);

    assert_eq!(
        fingerprint(&first),
        fingerprint(&second),
        "derivation must be a pure function of the ledger"
    );
    assert_eq!(dump1, dump2, "persisted nodes/edges rows must be identical");
    assert_eq!(first.nodes.len(), second.nodes.len());
    assert_eq!(first.edges, second.edges);
}

/// Every derived row, in a total order, with derived_rev excluded (it is
/// expected to advance) — everything else must be stable.
fn dump_derived(ledger: &Ledger) -> String {
    let mut out = String::new();
    let mut stmt = ledger
        .conn()
        .prepare(
            "SELECT id, label, sort_key, lifecycle, evidence_state, currency,
                    COALESCE(confidence,-1), session_id, anchor_event_id
             FROM nodes WHERE project_id = ?1 ORDER BY id",
        )
        .expect("prepare nodes");
    let rows = stmt
        .query_map([PROJ], |r| {
            Ok(format!(
                "N|{}|{}|{}|{}|{}|{}|{}|{}|{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, f64>(6)?,
                r.get::<_, String>(7)?,
                r.get::<_, String>(8)?
            ))
        })
        .expect("query nodes");
    for r in rows {
        out.push_str(&r.expect("row"));
        out.push('\n');
    }
    let mut stmt = ledger
        .conn()
        .prepare("SELECT from_node, to_node, kind FROM edges ORDER BY from_node, to_node, kind")
        .expect("prepare edges");
    let rows = stmt
        .query_map([], |r| {
            Ok(format!(
                "E|{}|{}|{}",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?
            ))
        })
        .expect("query edges");
    for r in rows {
        out.push_str(&r.expect("row"));
        out.push('\n');
    }
    out
}

fn publication_guard(ledger: &Ledger) -> (i64, i64, i64, i64, i64) {
    ledger
        .conn()
        .query_row(
            "SELECT
                 (SELECT count(*) FROM events WHERE project_id = ?1),
                 (SELECT count(*) FROM annotations a JOIN events e ON e.id = a.event_id
                    WHERE e.project_id = ?1),
                 (SELECT count(*) FROM session_generation_heads h JOIN sessions s
                    ON s.id = h.session_id WHERE s.project_id = ?1),
                 (SELECT COALESCE(MAX(derived_rev),0) FROM nodes WHERE project_id = ?1),
                 (SELECT count(DISTINCT derived_rev) FROM nodes WHERE project_id = ?1)",
            [PROJ],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .expect("publication guard")
}

#[test]
fn mid_node_publication_failure_keeps_the_previous_complete_revision() {
    let s = scratch(&[(&format!("{S}.jsonl"), standard_transcript())]);
    let ledger = ingested(&s);
    let first =
        rebuild(&ledger, PROJ, &thruline_core::config::Config::default()).expect("initial rebuild");
    let before_rows = dump_derived(&ledger);
    let before_guard = publication_guard(&ledger);

    ledger
        .conn()
        .execute_batch(
            "CREATE TEMP TRIGGER inject_mid_node_failure
             BEFORE INSERT ON nodes
             WHEN NEW.project_id = '-Users-dev-derive'
              AND (SELECT count(*) FROM nodes WHERE project_id = NEW.project_id) >= 1
             BEGIN SELECT RAISE(ABORT, 'injected mid-node publication failure'); END;",
        )
        .expect("install fault");
    let error = rebuild(&ledger, PROJ, &thruline_core::config::Config::default())
        .expect_err("fault must abort publication");
    assert!(error.to_string().contains("injected mid-node"), "{error}");
    assert_eq!(dump_derived(&ledger), before_rows);
    assert_eq!(publication_guard(&ledger), before_guard);

    ledger
        .conn()
        .execute_batch("DROP TRIGGER inject_mid_node_failure;")
        .expect("remove fault");
    let retry = rebuild(&ledger, PROJ, &thruline_core::config::Config::default())
        .expect("deterministic retry");
    assert_eq!(fingerprint(&retry), fingerprint(&first));
    assert_eq!(dump_derived(&ledger), before_rows);
    let after_retry = publication_guard(&ledger);
    assert_eq!(after_retry.0, before_guard.0, "events are authoritative");
    assert_eq!(
        after_retry.1, before_guard.1,
        "annotations are authoritative"
    );
    assert_eq!(
        after_retry.2, before_guard.2,
        "generation head is authoritative"
    );
    assert_eq!(after_retry.3, before_guard.3 + 1, "one revision published");
    assert_eq!(after_retry.4, 1, "only one complete revision is visible");
}

#[test]
fn mid_edge_publication_failure_never_exposes_new_nodes() {
    let s = scratch(&[(&format!("{S}.jsonl"), standard_transcript())]);
    let ledger = ingested(&s);
    let first =
        rebuild(&ledger, PROJ, &thruline_core::config::Config::default()).expect("initial rebuild");
    assert!(
        !first.edges.is_empty(),
        "fixture must exercise edge publication"
    );
    let before_rows = dump_derived(&ledger);
    let before_guard = publication_guard(&ledger);

    ledger
        .conn()
        .execute_batch(
            "CREATE TEMP TRIGGER inject_mid_edge_failure
             BEFORE INSERT ON edges
             BEGIN SELECT RAISE(ABORT, 'injected mid-edge publication failure'); END;",
        )
        .expect("install fault");
    let error = rebuild(&ledger, PROJ, &thruline_core::config::Config::default())
        .expect_err("fault must abort publication");
    assert!(error.to_string().contains("injected mid-edge"), "{error}");
    assert_eq!(dump_derived(&ledger), before_rows);
    assert_eq!(publication_guard(&ledger), before_guard);

    ledger
        .conn()
        .execute_batch("DROP TRIGGER inject_mid_edge_failure;")
        .expect("remove fault");
    let retry = rebuild(&ledger, PROJ, &thruline_core::config::Config::default())
        .expect("deterministic retry");
    assert_eq!(fingerprint(&retry), fingerprint(&first));
    assert_eq!(dump_derived(&ledger), before_rows);
}

#[test]
fn only_m2_states_are_ever_emitted() {
    let s = scratch(&[(&format!("{S}.jsonl"), standard_transcript())]);
    let ledger = ingested(&s);
    let g = derive_project(&ledger, PROJ).expect("derive");

    for n in &g.nodes {
        assert!(
            matches!(
                n.sort_key,
                NodeState::Recorded | NodeState::Pending | NodeState::Unknown
            ),
            "M3 states must not appear at M2: {:?}",
            n.sort_key
        );
    }
    // Explicitly: no contradicted / dropped / refused anywhere in the output.
    let states: Vec<&str> = g.nodes.iter().map(|n| n.sort_key.as_str()).collect();
    // `done` joins the forbidden list: at the deterministic tier it overclaims
    // exactly as "Verified" did. Only §5's entailment tier may award it.
    for forbidden in ["contradicted", "dropped", "refused", "supported"] {
        assert!(
            !states.contains(&forbidden),
            "{forbidden} leaked into the deterministic tier"
        );
    }
}

#[test]
fn states_follow_the_stated_rules() {
    let s = scratch(&[(&format!("{S}.jsonl"), standard_transcript())]);
    let ledger = ingested(&s);
    let g = derive_project(&ledger, PROJ).expect("derive");

    let by_label = |needle: &str| {
        g.nodes
            .iter()
            .find(|n| n.label.contains(needle))
            .unwrap_or_else(|| panic!("no node matching {needle:?} in {:?}", labels(&g)))
    };

    // Proposal with attached actions → done.
    let refactor = by_label("refactor");
    assert_eq!(refactor.sort_key, NodeState::Recorded);
    assert_eq!(
        refactor.dimensions,
        NodeDimensions::new(
            Lifecycle::Completed,
            EvidenceState::Observed,
            Currency::Current,
        )
    );
    assert!(
        refactor.evidence.len() >= 3,
        "clusters in the turn attach as evidence: {:?}",
        refactor.evidence
    );

    // Proposal with zero actions → pending.
    let rate = by_label("rate");
    assert_eq!(rate.sort_key, NodeState::Pending);
    assert_eq!(
        rate.dimensions,
        NodeDimensions::new(
            Lifecycle::Proposed,
            EvidenceState::Unknown,
            Currency::Current,
        )
    );
    assert!(rate.evidence.is_empty());

    // Completed-work claim with zero actions → unknown, never `supported`
    // and never an accusation. Accusations are the annotation layer's verdict
    // and require annotations, which this ledger has none of.
    let docs = by_label("updated");
    assert_eq!(docs.sort_key, NodeState::Unknown);
    assert_eq!(
        docs.dimensions,
        NodeDimensions::new(
            Lifecycle::Completed,
            EvidenceState::Unknown,
            Currency::Current,
        )
    );
    assert!(docs.evidence.is_empty());
}

#[test]
fn dimensions_are_independent_not_encoded_as_flat_state_variants() {
    let completed_proof_that_was_later_replaced = NodeDimensions::new(
        Lifecycle::Completed,
        EvidenceState::Supported,
        Currency::Superseded,
    );
    let abandoned_lifecycle_with_strong_current_proof = NodeDimensions::new(
        Lifecycle::Abandoned,
        EvidenceState::Verified,
        Currency::Current,
    );

    assert_eq!(
        completed_proof_that_was_later_replaced.currency,
        Currency::Superseded
    );
    assert_eq!(
        abandoned_lifecycle_with_strong_current_proof.evidence,
        EvidenceState::Verified
    );
}

#[test]
fn an_unanchored_completed_claim_never_becomes_a_node() {
    // Extractor v2's artifact-tying rule, seen from derivation: "I've updated
    // the docs already." names nothing a tool could have touched, so there is
    // no claim, no node, and nothing that could later be accused. The M2
    // extractor made a node here and it was exactly the false-⚠ surface.
    let s = scratch(&[(&format!("{S}.jsonl"), {
        let mut l = vec![user(&u(1), None, S, "2026-07-29T00:00:00.000Z", "go")];
        l.push(text(
            &u(2),
            &u(1),
            S,
            "2026-07-29T00:00:01.000Z",
            "I've updated the docs already.",
        ));
        l.join("\n") + "\n"
    })]);
    let ledger = ingested(&s);
    let g = derive_project(&ledger, PROJ).expect("derive");
    assert!(
        g.nodes.is_empty(),
        "an unanchored claim produced nodes: {:?}",
        labels(&g)
    );
}

fn labels(g: &thruline_core::DerivedGraph) -> Vec<String> {
    g.nodes.iter().map(|n| n.label.clone()).collect()
}

#[test]
fn consecutive_edits_to_one_file_collapse_but_a_different_file_does_not() {
    let s = scratch(&[(&format!("{S}.jsonl"), standard_transcript())]);
    let ledger = ingested(&s);
    let g = derive_project(&ledger, PROJ).expect("derive");

    let refactor = g
        .nodes
        .iter()
        .find(|n| n.label.contains("refactor"))
        .expect("proposal node");

    // The rule collapses clusters into one NODE, not into one evidence row:
    // two edits to the same file are two facts a receipt should show, and here
    // all four clusters already belong to the statement node anyway.
    assert_eq!(
        refactor.evidence.len(),
        4,
        "every cluster stays its own evidence row: {:?}",
        refactor
            .evidence
            .iter()
            .map(|e| &e.summary)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        g.stats.collapsed_clusters, 1,
        "the second auth.rs edit folded into the first cluster's node"
    );

    let summaries: Vec<&str> = refactor
        .evidence
        .iter()
        .map(|e| e.summary.as_str())
        .collect();
    assert!(
        summaries.iter().any(|s| s.contains("auth.rs")),
        "{summaries:?}"
    );
    assert!(
        summaries.iter().any(|s| s.contains("lib.rs")),
        "{summaries:?}"
    );
    assert!(
        summaries.iter().any(|s| s.contains("cargo")),
        "{summaries:?}"
    );

    // Evidence kinds map to §9's domain.
    for e in &refactor.evidence {
        assert!(matches!(e.kind, "edit" | "command"), "{}", e.kind);
    }
    // The Bash row carries what the command actually printed.
    let bash = refactor
        .evidence
        .iter()
        .find(|e| e.summary.contains("cargo"))
        .expect("bash row");
    assert_eq!(bash.result_excerpt.as_deref(), Some("214 passed"));
}

#[test]
fn standalone_clusters_collapse_into_one_node_per_target() {
    // No statement opens this turn, so each cluster would otherwise become its
    // own node. Consecutive same-target clusters must fold into one.
    let mut l = Vec::new();
    l.push(user(&u(1), None, S, "2026-07-29T00:00:00.000Z", "go"));
    for (i, (tid, path)) in [
        ("t1", "/repo/a.rs"),
        ("t2", "/repo/a.rs"),
        ("t3", "/repo/a.rs"),
        ("t4", "/repo/b.rs"),
    ]
    .iter()
    .enumerate()
    {
        let n = 10 + (i as u32) * 2;
        l.push(tool(
            &u(n),
            &u(if i == 0 { 1 } else { n - 1 }),
            S,
            &format!("2026-07-29T00:00:{:02}.000Z", i + 1),
            tid,
            "Edit",
            &format!(r#"{{"file_path":"{path}","old_string":"x","new_string":"y"}}"#),
        ));
        l.push(result(
            &u(n + 1),
            &u(n),
            S,
            &format!("2026-07-29T00:00:{:02}.000Z", i + 20),
            tid,
            "ok",
        ));
    }
    let s = scratch(&[(&format!("{S}.jsonl"), l.join("\n") + "\n")]);
    let ledger = ingested(&s);
    let g = derive_project(&ledger, PROJ).expect("derive");

    assert_eq!(g.stats.action_clusters, 4);
    assert_eq!(
        g.nodes.len(),
        2,
        "three a.rs edits fold into one node, b.rs is its own: {:?}",
        labels(&g)
    );
    assert_eq!(g.stats.collapsed_clusters, 2, "two folds");
    let first = &g.nodes[0];
    assert_eq!(first.evidence.len(), 3, "all three a.rs edits are evidence");
    assert!(g.nodes.iter().all(|n| n.sort_key == NodeState::Recorded));
}

#[test]
fn thinking_is_never_a_node_and_never_evidence() {
    let mut body = standard_transcript();
    body.push_str(&format!(
        r#"{{"type":"assistant","uuid":"{}","parentUuid":"{}","isSidechain":false,"sessionId":"{S}","timestamp":"2026-07-29T00:03:00.000Z","message":{{"role":"assistant","content":[{{"type":"thinking","thinking":"I'll fix everything and I have updated all the files.","signature":"x"}}]}}}}"#,
        u(90),
        u(14)
    ));
    body.push('\n');
    let s = scratch(&[(&format!("{S}.jsonl"), body)]);
    let ledger = ingested(&s);
    let g = derive_project(&ledger, PROJ).expect("derive");

    // The thinking text contains both a proposal and a completed-work claim.
    // Neither may produce a node, however tempting the prefixes look.
    assert!(
        !g.nodes.iter().any(|n| n.label.contains("fix everything")),
        "thinking became a node: {:?}",
        labels(&g)
    );
    assert!(!g
        .nodes
        .iter()
        .flat_map(|n| &n.evidence)
        .any(|e| e.summary.contains("everything")));
}

#[test]
fn kind_local_projection_omits_only_semantically_inert_columns() {
    let body = fs::read_to_string(fixture("claude_code_sessions.jsonl")).expect("fixture");
    let s = scratch(&[("00000000-0000-4000-8000-000000000000.jsonl", body)]);
    let ledger = ingested(&s);
    let before = derive_project(&ledger, PROJ).expect("derive before inert-field probe");
    let before_fingerprint = fingerprint(&before);
    let before_public = serde_json::to_vec(
        &export_project(&ledger, &before, "2026-08-02T00:00:00Z").expect("public export"),
    )
    .expect("serialize public");
    let before_diagnostics = serde_json::to_vec(
        &export_project_with_options(
            &ledger,
            &before,
            "2026-08-02T00:00:00Z",
            ExportOptions { diagnostics: true },
        )
        .expect("diagnostics export"),
    )
    .expect("serialize diagnostics");

    let kinds: Vec<String> = {
        let mut stmt = ledger
            .conn()
            .prepare("SELECT DISTINCT kind FROM events ORDER BY kind")
            .expect("prepare kinds");
        stmt.query_map([], |r| r.get(0))
            .expect("query kinds")
            .collect::<rusqlite::Result<_>>()
            .expect("collect kinds")
    };
    for required in [
        "assistant_text",
        "assistant_thinking",
        "meta",
        "tool_result",
        "tool_use",
        "user_msg",
    ] {
        assert!(
            kinds.iter().any(|kind| kind == required),
            "missing {required}"
        );
    }

    // This white-box corruption probe is confined to a temporary test ledger.
    // It changes exactly the fields the kind-local SELECT omits, then restores
    // the append-only trigger before deriving again. If any omitted field had
    // graph or receipt authority, the frozen graph/export bytes would change.
    ledger
        .conn()
        .execute_batch(
            "DROP TRIGGER events_no_update;
             UPDATE events SET
               -- A158: user_msg text gained export authority (the opening-ask
               -- candidate quotes it verbatim), so it leaves the inert set.
               text = CASE WHEN kind IN ('assistant_text','user_msg')
                           THEN text ELSE 'inert-text' END,
               tool_name = CASE WHEN kind = 'tool_use' THEN tool_name ELSE 'inert-tool' END,
               tool_use_id = CASE WHEN kind IN ('tool_use','tool_result')
                                  THEN tool_use_id ELSE 'inert-use-id' END,
               tool_input = CASE WHEN kind = 'tool_use' THEN tool_input ELSE 'inert-input' END,
               tool_result = CASE WHEN kind = 'tool_result'
                                  THEN tool_result ELSE 'inert-result' END,
               is_error = CASE WHEN kind = 'tool_result' THEN is_error ELSE 1 END,
               ts = CASE WHEN kind IN ('assistant_text','tool_use')
                         THEN ts ELSE '1900-01-01T00:00:00Z' END,
               raw_file = CASE WHEN kind IN ('assistant_text','tool_use')
                               THEN raw_file ELSE '/inert/path' END,
               raw_sha256 = CASE WHEN kind = 'tool_use' THEN raw_sha256
                                 ELSE 'inert-sha' END;
             CREATE TRIGGER events_no_update BEFORE UPDATE ON events
               BEGIN SELECT RAISE(ABORT,'events are append-only'); END;",
        )
        .expect("mutate inert fields and restore guard");

    let after = derive_project(&ledger, PROJ).expect("derive after inert-field probe");
    assert_eq!(fingerprint(&after), before_fingerprint);
    let after_public = serde_json::to_vec(
        &export_project(&ledger, &after, "2026-08-02T00:00:00Z").expect("public export"),
    )
    .expect("serialize public");
    let after_diagnostics = serde_json::to_vec(
        &export_project_with_options(
            &ledger,
            &after,
            "2026-08-02T00:00:00Z",
            ExportOptions { diagnostics: true },
        )
        .expect("diagnostics export"),
    )
    .expect("serialize diagnostics");
    assert_eq!(after.nodes, before.nodes);
    assert_eq!(after.edges, before.edges);
    assert_eq!(after_public, before_public);
    assert_eq!(after_diagnostics, before_diagnostics);
}

#[test]
fn a_resumed_session_contributes_no_duplicate_nodes() {
    let child = "bbbbbbbb-1111-4222-8333-444444444444";
    let parent_body = standard_transcript();
    // The resume copies the parent verbatim, then adds its own turn.
    let mut child_body = parent_body.clone();
    child_body.push_str(&user(
        &u(20),
        Some(&u(14)),
        child,
        "2026-07-29T01:00:00.000Z",
        "continue",
    ));
    child_body.push('\n');
    child_body.push_str(&text(
        &u(21),
        &u(20),
        child,
        "2026-07-29T01:00:01.000Z",
        "I'll finish the migration.",
    ));
    child_body.push('\n');

    let s = scratch(&[
        (&format!("{S}.jsonl"), parent_body),
        (&format!("{child}.jsonl"), child_body),
    ]);
    let ledger = ingested(&s);
    let g = derive_project(&ledger, PROJ).expect("derive");

    assert!(g.stats.duplicate_events > 0, "the copy must be detected");
    assert_eq!(
        g.stats.duplicate_events as usize,
        g.duplicate_events.len(),
        "every folded ledger row needs an audit relation"
    );
    assert_eq!(
        g.stats.canonical_events + g.stats.duplicate_events,
        g.stats.raw_events
    );

    // Every node id is unique and every canonical anchor appears once.
    let mut ids: Vec<&str> = g.nodes.iter().map(|n| n.id.as_str()).collect();
    ids.sort_unstable();
    let before = ids.len();
    ids.dedup();
    assert_eq!(before, ids.len(), "duplicate node ids");

    // The copied proposal is attributed to the parent session, not the copy.
    let refactor = g
        .nodes
        .iter()
        .find(|n| n.label.contains("refactor"))
        .expect("proposal node");
    assert_eq!(
        refactor.session_id, S,
        "canonical event must stay with the original session"
    );
    // The resumed session's own turn still produces its own node.
    assert!(
        g.nodes.iter().any(|n| n.session_id == child),
        "the resume's new work is missing"
    );
    assert!(g.duplicate_events.iter().all(|duplicate| {
        duplicate.duplicate_session_id == child
            && duplicate.canonical_session_id == S
            && duplicate.duplicate_event_id != duplicate.canonical_event_id
            && !duplicate.src_uuid.is_empty()
    }));
}

#[test]
fn deep_resume_relations_are_direct_chronological_stable_and_exported_once() {
    fn key_occurrences(value: &serde_json::Value, needle: &str) -> usize {
        match value {
            serde_json::Value::Object(object) => {
                usize::from(object.contains_key(needle))
                    + object
                        .values()
                        .map(|value| key_occurrences(value, needle))
                        .sum::<usize>()
            }
            serde_json::Value::Array(values) => values
                .iter()
                .map(|value| key_occurrences(value, needle))
                .sum(),
            _ => 0,
        }
    }

    let child = "bbbbbbbb-1111-4222-8333-444444444444";
    let grandchild = "cccccccc-1111-4222-8333-444444444444";
    let parent_user = u(1300);
    let parent_text = u(1301);
    let child_user = u(1302);
    let child_text = u(1303);
    let grandchild_user = u(1304);
    let grandchild_text = u(1305);

    let parent_body = [
        user(
            &parent_user,
            None,
            S,
            "2026-08-01T03:00:00.000Z",
            "begin the parent work",
        ),
        text(
            &parent_text,
            &parent_user,
            S,
            "2026-08-01T03:00:01.000Z",
            "I'll update `parent-secret.rs`.",
        ),
    ]
    .join("\n")
        + "\n";

    let mut child_body = parent_body.clone();
    child_body.push_str(&user(
        &child_user,
        Some(&parent_text),
        child,
        "2026-08-01T03:01:00.000Z",
        "resume in the child",
    ));
    child_body.push('\n');
    child_body.push_str(&text(
        &child_text,
        &child_user,
        child,
        "2026-08-01T03:01:01.000Z",
        "I'll update `child-secret.rs`.",
    ));
    child_body.push('\n');

    let mut grandchild_body = child_body.clone();
    grandchild_body.push_str(&user(
        &grandchild_user,
        Some(&child_text),
        grandchild,
        "2026-08-01T03:02:00.000Z",
        "resume in the grandchild",
    ));
    grandchild_body.push('\n');
    grandchild_body.push_str(&text(
        &grandchild_text,
        &grandchild_user,
        grandchild,
        "2026-08-01T03:02:01.000Z",
        "I'll update `grandchild-secret.rs`.",
    ));
    grandchild_body.push('\n');

    let forward = scratch(&[
        (&format!("{S}.jsonl"), parent_body.clone()),
        (&format!("{child}.jsonl"), child_body.clone()),
        (&format!("{grandchild}.jsonl"), grandchild_body.clone()),
    ]);
    let forward_ledger = ingested(&forward);
    let graph = derive_project(&forward_ledger, PROJ).expect("derive deep resume");

    assert_eq!(graph.stats.raw_events, 12, "the ledger keeps every copy");
    assert_eq!(graph.stats.canonical_events, 6);
    assert_eq!(graph.stats.duplicate_events, 6);
    assert_eq!(
        graph.stats.duplicate_events as usize,
        graph.duplicate_events.len()
    );
    let ledger_events: i64 = forward_ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE project_id = ?1",
            [PROJ],
            |row| row.get(0),
        )
        .expect("ledger event count");
    assert_eq!(ledger_events, 12);

    let duplicate_ids: BTreeSet<&str> = graph
        .duplicate_events
        .iter()
        .map(|duplicate| duplicate.duplicate_event_id.as_str())
        .collect();
    let mut copies_by_uuid: BTreeMap<&str, usize> = BTreeMap::new();
    for duplicate in &graph.duplicate_events {
        *copies_by_uuid
            .entry(duplicate.src_uuid.as_str())
            .or_default() += 1;
        assert_ne!(
            duplicate.duplicate_event_id, duplicate.canonical_event_id,
            "a folded row cannot be canonical"
        );
        assert!(
            !duplicate_ids.contains(duplicate.canonical_event_id.as_str()),
            "deep copies must point directly to the first canonical event"
        );
        let (expected_session, expected_seq) = match duplicate.src_uuid.as_str() {
            uuid if uuid == parent_user => (S, 1),
            uuid if uuid == parent_text => (S, 2),
            uuid if uuid == child_user => (child, 3),
            uuid if uuid == child_text => (child, 4),
            other => panic!("unexpected folded UUID {other}"),
        };
        assert_eq!(duplicate.canonical_session_id, expected_session);
        assert_eq!(duplicate.canonical_seq, expected_seq);
    }
    assert_eq!(copies_by_uuid.get(parent_user.as_str()), Some(&2));
    assert_eq!(copies_by_uuid.get(parent_text.as_str()), Some(&2));
    assert_eq!(copies_by_uuid.get(child_user.as_str()), Some(&1));
    assert_eq!(copies_by_uuid.get(child_text.as_str()), Some(&1));
    assert_eq!(
        graph
            .duplicate_events
            .iter()
            .map(|duplicate| duplicate.duplicate_session_id.as_str())
            .collect::<Vec<_>>(),
        vec![child, child, grandchild, grandchild, grandchild, grandchild],
        "relations are ordered by duplicate chronology"
    );

    let canonical_anchors: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|node| node.anchor_uuid.as_deref())
        .collect();
    assert_eq!(
        canonical_anchors,
        BTreeSet::from([
            parent_text.as_str(),
            child_text.as_str(),
            grandchild_text.as_str(),
        ]),
        "audit relations must not change canonical nodes"
    );

    let reversed = scratch(&[
        (&format!("{grandchild}.jsonl"), grandchild_body),
        (&format!("{child}.jsonl"), child_body),
        (&format!("{S}.jsonl"), parent_body),
    ]);
    let reversed_ledger = ingested(&reversed);
    let reversed_graph = derive_project(&reversed_ledger, PROJ).expect("derive reversed inputs");
    let relation_projection = |graph: &DerivedGraph| {
        graph
            .duplicate_events
            .iter()
            .map(|duplicate| {
                (
                    duplicate.src_uuid.clone(),
                    duplicate.duplicate_session_id.clone(),
                    duplicate.duplicate_seq,
                    duplicate.canonical_session_id.clone(),
                    duplicate.canonical_seq,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        relation_projection(&reversed_graph),
        relation_projection(&graph),
        "fixture creation order cannot change duplicate chronology or authority"
    );
    let node_projection = |graph: &DerivedGraph| {
        graph
            .nodes
            .iter()
            .map(|node| {
                (
                    node.id.clone(),
                    node.session_id.clone(),
                    node.anchor_seq,
                    node.anchor_uuid.clone(),
                    node.abandoned,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(node_projection(&reversed_graph), node_projection(&graph));
    assert_eq!(reversed_graph.edges, graph.edges);

    let retry = derive_project(&forward_ledger, PROJ).expect("repeat derive");
    assert_eq!(retry.duplicate_events, graph.duplicate_events);
    assert_eq!(fingerprint(&retry), fingerprint(&graph));

    let mut without_audit = graph.clone();
    without_audit.duplicate_events.clear();
    assert_ne!(
        fingerprint(&without_audit),
        fingerprint(&graph),
        "duplicate audit rows are part of the derive fingerprint"
    );

    let public =
        export_project(&forward_ledger, &graph, "2026-08-01T04:00:00Z").expect("public export");
    let diagnostics = export_project_with_options(
        &forward_ledger,
        &graph,
        "2026-08-01T04:00:00Z",
        ExportOptions { diagnostics: true },
    )
    .expect("diagnostics export");
    assert_eq!(public["duplicate_events"], diagnostics["duplicate_events"]);
    assert_eq!(
        public["duplicate_events"]
            .as_array()
            .expect("root duplicate events")
            .len(),
        6
    );
    assert_eq!(key_occurrences(&public, "duplicate_events"), 1);
    for relation in public["duplicate_events"]
        .as_array()
        .expect("duplicate event relations")
    {
        let mut keys: Vec<&str> = relation
            .as_object()
            .expect("relation object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "canonical_event_id",
                "canonical_seq",
                "canonical_session_id",
                "duplicate_event_id",
                "duplicate_seq",
                "duplicate_session_id",
                "src_uuid",
            ]
        );
    }
    let relation_json = serde_json::to_string(&public["duplicate_events"]).expect("relation JSON");
    for private in [
        "parent-secret.rs",
        "child-secret.rs",
        "grandchild-secret.rs",
        ".jsonl",
    ] {
        assert!(
            !relation_json.contains(private),
            "duplicate audit leaked transcript/path material: {private}"
        );
    }
}

#[test]
fn copied_rows_without_source_uuids_are_never_folded() {
    let child = "bbbbbbbb-1111-4222-8333-444444444444";
    let mut ledger = Ledger::open_in_memory().expect("ledger");
    ledger
        .upsert_project(
            &ProjectRef {
                id: PROJ.to_string(),
                root_path: "/synthetic/no-uuid".to_string(),
                source: Source::ClaudeCode,
                display_name: Some("no-uuid copies".to_string()),
            },
            "2026-08-01T05:00:00Z",
        )
        .expect("project");

    for (index, session_id) in [S, child].into_iter().enumerate() {
        ledger
            .upsert_session(&SessionRef {
                id: session_id.to_string(),
                project_id: PROJ.to_string(),
                file_path: format!("/synthetic/no-uuid/{session_id}.jsonl"),
                started_at: Some(format!("2026-08-01T05:0{index}:00Z")),
                raw_line_count: 1,
                raw_digest: None,
            })
            .expect("session");
        let mut event = NormalizedEvent::new(EventKind::AssistantText);
        event.role = Some("assistant".to_string());
        event.text = Some("I'll update `no-uuid.rs`.".to_string());
        event.ts = Some("2026-08-01T05:00:01Z".to_string());
        assert!(event.src_uuid.is_none());
        ledger
            .append_session_events(
                PROJ,
                session_id,
                &[(
                    RawLineRef {
                        file: format!("/synthetic/no-uuid/{session_id}.jsonl"),
                        line: 1,
                        sha256: format!("{index:064x}"),
                    },
                    event,
                )],
            )
            .expect("append no-uuid event");
    }
    let graph = derive_project(&ledger, PROJ).expect("derive no-uuid copies");

    let no_uuid_rows: i64 = ledger
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM events WHERE project_id = ?1 AND src_uuid IS NULL",
            [PROJ],
            |row| row.get(0),
        )
        .expect("no-uuid event count");
    assert_eq!(no_uuid_rows, 2);
    assert_eq!(graph.stats.raw_events, 2);
    assert_eq!(graph.stats.canonical_events, 2);
    assert_eq!(graph.stats.duplicate_events, 0);
    assert!(graph.duplicate_events.is_empty());
    assert_eq!(graph.nodes.len(), 2, "both no-uuid rows remain canonical");

    let exported = export_project(&ledger, &graph, "2026-08-01T05:01:00Z").expect("export");
    assert_eq!(exported["duplicate_events"], serde_json::json!([]));
}

#[test]
fn branch_selection_follows_the_newest_descendant_leaf() {
    let root = u(1000);
    let older_head = u(1001);
    let newer_head = u(1002);
    let older_continuation = u(1003);
    let newest_leaf = u(1004);
    let body = [
        user(
            &u(999),
            None,
            S,
            "2026-08-01T00:00:00.000Z",
            "build both approaches",
        ),
        text(
            &root,
            &u(999),
            S,
            "2026-08-01T00:00:01.000Z",
            "I'll update `root.rs`.",
        ),
        text(
            &older_head,
            &root,
            S,
            "2026-08-01T00:00:02.000Z",
            "I'll update `older.rs`.",
        ),
        text(
            &newer_head,
            &root,
            S,
            "2026-08-01T00:00:08.000Z",
            "I'll update `newer.rs`.",
        ),
        text(
            &older_continuation,
            &older_head,
            S,
            "2026-08-01T00:00:09.000Z",
            "I'll update `older-continuation.rs`.",
        ),
        text(
            &newest_leaf,
            &older_continuation,
            S,
            "2026-08-01T00:00:10.000Z",
            "I'll update `newest-leaf.rs`.",
        ),
    ]
    .join("\n")
        + "\n";

    let s = scratch(&[(&format!("{S}.jsonl"), body)]);
    let ledger = ingested(&s);
    let graph = derive_project(&ledger, PROJ).expect("derive");

    let root = node_by_anchor(&graph, &root);
    let older_head = node_by_anchor(&graph, &older_head);
    let newer_head = node_by_anchor(&graph, &newer_head);
    let older_continuation = node_by_anchor(&graph, &older_continuation);
    let newest_leaf = node_by_anchor(&graph, &newest_leaf);

    assert_eq!(graph.nodes.len(), 5, "branch rows must all be retained");
    assert_eq!(
        graph.edges.len(),
        4,
        "canonical branch edges must be retained"
    );
    assert_eq!(graph.stats.abandoned_heads, 1);
    assert!(!older_head.abandoned, "the continuing branch must win");
    assert_eq!(older_head.dimensions.currency, Currency::Current);
    assert!(
        newer_head.abandoned,
        "a newer immediate child must lose to a branch with the newest leaf"
    );
    assert_eq!(newer_head.dimensions.currency, Currency::Superseded);
    assert!(!older_continuation.abandoned);
    assert!(!newest_leaf.abandoned);

    assert!(has_edge(&graph, root, older_head, "seq"));
    assert!(has_edge(&graph, root, newer_head, "seq"));
    assert!(has_edge(&graph, older_head, older_continuation, "seq"));
    assert!(has_edge(&graph, older_continuation, newest_leaf, "seq"));
}

#[test]
fn branch_selection_ties_are_total_and_deterministic() {
    let child_b = "bbbbbbbb-1111-4222-8333-444444444444";
    let child_c = "cccccccc-1111-4222-8333-444444444444";
    let root = u(1100);
    let leaf_b = u(1101);
    let leaf_c = u(1102);
    let parent_body = [
        user(
            &u(1099),
            None,
            S,
            "2026-08-01T01:00:00.000Z",
            "choose one continuation",
        ),
        text(
            &root,
            &u(1099),
            S,
            "2026-08-01T01:00:01.000Z",
            "I'll update `root.rs`.",
        ),
    ]
    .join("\n")
        + "\n";
    let body_b = [
        user(
            &u(1111),
            Some(&root),
            child_b,
            "2026-08-01T01:00:02.000Z",
            "continue as B",
        ),
        text(
            &leaf_b,
            &u(1111),
            child_b,
            "2026-08-01T01:00:03.000Z",
            "I'll update `branch-b.rs`.",
        ),
    ]
    .join("\n")
        + "\n";
    let body_c = [
        user(
            &u(1121),
            Some(&root),
            child_c,
            "2026-08-01T01:00:02.000Z",
            "continue as C",
        ),
        text(
            &leaf_c,
            &u(1121),
            child_c,
            "2026-08-01T01:00:03.000Z",
            "I'll update `branch-c.rs`.",
        ),
    ]
    .join("\n")
        + "\n";

    let first = scratch(&[
        (&format!("{S}.jsonl"), parent_body.clone()),
        (&format!("{child_b}.jsonl"), body_b.clone()),
        (&format!("{child_c}.jsonl"), body_c.clone()),
    ]);
    let first_ledger = ingested(&first);
    let first_graph = derive_project(&first_ledger, PROJ).expect("first derive");
    let retry = derive_project(&first_ledger, PROJ).expect("repeat derive");
    assert_eq!(fingerprint(&retry), fingerprint(&first_graph));

    let b = node_by_anchor(&first_graph, &leaf_b);
    let c = node_by_anchor(&first_graph, &leaf_c);
    assert_eq!(b.ts, c.ts, "timestamp must tie in this fixture");
    assert_eq!(b.anchor_seq, c.anchor_seq, "sequence must also tie");
    let (winner, loser) = if b.id.as_str() > c.id.as_str() {
        (b, c)
    } else {
        (c, b)
    };
    assert!(!winner.abandoned, "greatest node id must break the tie");
    assert!(loser.abandoned, "the other immediate head must be retained");
    assert_eq!(first_graph.stats.abandoned_heads, 1);
    assert!(has_edge(
        &first_graph,
        node_by_anchor(&first_graph, &root),
        b,
        "inferred"
    ));
    assert!(has_edge(
        &first_graph,
        node_by_anchor(&first_graph, &root),
        c,
        "inferred"
    ));

    // Reversing fixture construction cannot change the branch projection.
    let reversed = scratch(&[
        (&format!("{child_c}.jsonl"), body_c),
        (&format!("{child_b}.jsonl"), body_b),
        (&format!("{S}.jsonl"), parent_body),
    ]);
    let reversed_ledger = ingested(&reversed);
    let reversed_graph = derive_project(&reversed_ledger, PROJ).expect("reversed derive");
    let projection = |graph: &DerivedGraph| {
        graph
            .nodes
            .iter()
            .map(|node| {
                (
                    node.id.clone(),
                    node.anchor_uuid.clone(),
                    node.abandoned,
                    node.dimensions.currency,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(projection(&reversed_graph), projection(&first_graph));
    assert_eq!(reversed_graph.edges, first_graph.edges);
}

#[test]
fn resumed_sidechain_metadata_does_not_change_branch_selection() {
    let child = "dddddddd-1111-4222-8333-444444444444";
    let root = u(1200);
    let older_head = u(1201);
    let resumed_leaf = u(1202);
    let parent_body = [
        user(
            &u(1199),
            None,
            S,
            "2026-08-01T02:00:00.000Z",
            "continue after the fork",
        ),
        text(
            &root,
            &u(1199),
            S,
            "2026-08-01T02:00:01.000Z",
            "I'll update `root.rs`.",
        ),
        text(
            &older_head,
            &root,
            S,
            "2026-08-01T02:00:02.000Z",
            "I'll update `older.rs`.",
        ),
    ]
    .join("\n")
        + "\n";
    let child_body = |is_sidechain| {
        let mut body = parent_body.clone();
        body.push_str(&user_with_sidechain(
            &u(1211),
            Some(&root),
            child,
            "2026-08-01T02:00:03.000Z",
            "resume",
            is_sidechain,
        ));
        body.push('\n');
        body.push_str(&text_with_sidechain(
            &resumed_leaf,
            &u(1211),
            child,
            "2026-08-01T02:00:04.000Z",
            "I'll update `resumed.rs`.",
            is_sidechain,
        ));
        body.push('\n');
        body
    };

    let ordinary_child = child_body(false);
    let sidechain_child = child_body(true);
    let ordinary = scratch(&[
        (&format!("{S}.jsonl"), parent_body.clone()),
        (&format!("{child}.jsonl"), ordinary_child),
    ]);
    let ordinary_ledger = ingested(&ordinary);
    let ordinary_graph = derive_project(&ordinary_ledger, PROJ).expect("ordinary derive");

    let sidechain = scratch(&[
        (&format!("{S}.jsonl"), parent_body),
        (&format!("{child}.jsonl"), sidechain_child),
    ]);
    let sidechain_ledger = ingested(&sidechain);
    let sidechain_graph = derive_project(&sidechain_ledger, PROJ).expect("sidechain derive");

    for graph in [&ordinary_graph, &sidechain_graph] {
        assert!(
            graph.stats.duplicate_events > 0,
            "copied prefix not deduped"
        );
        assert_eq!(
            graph.nodes.len(),
            3,
            "resume must not duplicate branch rows"
        );
        assert_eq!(graph.edges.len(), 2, "branch edges must be retained");
        assert_eq!(graph.stats.abandoned_heads, 1);
        let root = node_by_anchor(graph, &root);
        let older_head = node_by_anchor(graph, &older_head);
        let resumed_leaf = node_by_anchor(graph, &resumed_leaf);
        assert!(older_head.abandoned);
        assert_eq!(older_head.dimensions.currency, Currency::Superseded);
        assert!(!resumed_leaf.abandoned);
        assert_eq!(resumed_leaf.dimensions.currency, Currency::Current);
        assert!(has_edge(graph, root, older_head, "seq"));
        assert!(has_edge(graph, root, resumed_leaf, "inferred"));
    }

    let projection = |graph: &DerivedGraph| {
        graph
            .nodes
            .iter()
            .map(|node| {
                (
                    node.id.clone(),
                    node.anchor_uuid.clone(),
                    node.abandoned,
                    node.dimensions.currency,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(projection(&sidechain_graph), projection(&ordinary_graph));
    assert_eq!(sidechain_graph.edges, ordinary_graph.edges);
}

#[test]
fn derivation_over_the_committed_fixtures_is_snapshotted() {
    let body = fs::read_to_string(fixture("claude_code_sessions.jsonl")).expect("fixture");
    let s = scratch(&[("00000000-0000-4000-8000-000000000000.jsonl", body)]);
    let ledger = ingested(&s);
    let g = derive_project(&ledger, PROJ).expect("derive");

    let rendered: Vec<String> = g
        .nodes
        .iter()
        .map(|n| {
            format!(
                "{} | {} | {} | ev={} | abandoned={}",
                n.id,
                n.sort_key.as_str(),
                n.label,
                n.evidence.len(),
                n.abandoned
            )
        })
        .collect();
    insta::assert_debug_snapshot!("fixture_nodes", rendered);

    let edges: Vec<String> = g
        .edges
        .iter()
        .map(|e| format!("{} -{}-> {}", e.from, e.kind, e.to))
        .collect();
    insta::assert_debug_snapshot!("fixture_edges", edges);
}

#[test]
fn labels_are_bounded_and_deterministic() {
    let s = scratch(&[(&format!("{S}.jsonl"), standard_transcript())]);
    let ledger = ingested(&s);
    let g = derive_project(&ledger, PROJ).expect("derive");
    for n in &g.nodes {
        assert!(
            n.label.chars().count() <= 22,
            "label {:?} is {} chars",
            n.label,
            n.label.chars().count()
        );
        assert!(!n.label.is_empty());
        assert!(!n.label.contains('\n'));
    }
}

#[test]
fn an_empty_project_derives_to_an_empty_graph() {
    let s = scratch(&[(&format!("{S}.jsonl"), String::new())]);
    let ledger = ingested(&s);
    let g = derive_project(&ledger, PROJ).expect("derive must not fail on nothing");
    assert!(g.nodes.is_empty());
    assert!(g.edges.is_empty());
    assert_eq!(g.stats.canonical_events, 0);
}
