//! R17b-1a — the PROBE. Discovery only: no fixture, no binding, no verdict.
//!
//! Every time the attested-run path has bitten (A102, A103) it was an identity
//! ASSUMPTION. So this file authors a minimal codex rollout, ingests it through
//! the real sealed-mirror path, and prints what the engine actually recorded —
//! before anything is built on it.

mod scratch;

use std::fs;
use std::path::PathBuf;

use scratch::TempTree;
use thruline_core::codex::source_tool_identity;
use thruline_core::ingest::{ingest_codex, CodexIngestOptions};
use thruline_core::Ledger;

const OWNER: &str = "probe-owner";

/// Two shapes for `arguments`, because codex writes it as a JSON STRING in some
/// versions and an object in others, and `source_native_command` reads a string
/// field named `command` out of the stored `tool_input`. Which one survives
/// ingest readably is exactly what this probe exists to answer.
fn transcript() -> String {
    [
        r#"{"timestamp":"2030-01-01T00:00:00Z","type":"session_meta","payload":{"id":"probe-session","cwd":"/probe/project"}}"#,
        r#"{"timestamp":"2030-01-01T00:00:01Z","type":"turn_context","payload":{"turn_id":"probe-turn","cwd":"/probe/project","branch":"main"}}"#,
        r#"{"timestamp":"2030-01-01T00:00:02Z","type":"response_item","payload":{"type":"message","id":"probe-msg","role":"assistant","content":[{"type":"output_text","text":"The build succeeded."}]}}"#,
        // arguments as an OBJECT
        r#"{"timestamp":"2030-01-01T00:00:03Z","type":"response_item","payload":{"type":"function_call","id":"probe-fc-obj","call_id":"call-obj","name":"shell","arguments":{"command":"git status --porcelain"}}}"#,
        // arguments as a JSON STRING
        r#"{"timestamp":"2030-01-01T00:00:04Z","type":"response_item","payload":{"type":"function_call","id":"probe-fc-str","call_id":"call-str","name":"shell","arguments":"{\"command\":\"git diff --stat\"}"}}"#,
    ]
    .join("\n")
        + "\n"
}

struct Probe {
    _tmp: TempTree,
    db: PathBuf,
}

fn ingest_probe() -> Probe {
    let tmp = TempTree::new();
    let source = tmp.path().join("source").join("rollout-probe.jsonl");
    fs::create_dir_all(source.parent().expect("parent")).expect("mkdir");
    fs::write(&source, transcript()).expect("write transcript");
    let db = tmp.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    let report = ingest_codex(
        &mut ledger,
        std::slice::from_ref(&source),
        &CodexIngestOptions {
            mirror_root: tmp.path().join("private-mirror"),
            owner_key: Some(OWNER.to_string()),
        },
    )
    .expect("codex ingest");
    println!("PROBE ingest report: {report:?}");
    drop(ledger);
    Probe { _tmp: tmp, db }
}

fn events(ledger: &Ledger) -> Vec<(i64, String, String, String, String, String)> {
    ledger
        .conn()
        .prepare(
            "SELECT seq, kind, COALESCE(tool_use_id,''), COALESCE(source_turn_id,''),
                    COALESCE(tool_input,''), COALESCE(text,'')
             FROM events ORDER BY seq",
        )
        .expect("prepare")
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
            ))
        })
        .expect("query")
        .flatten()
        .collect()
}

fn session_ids(ledger: &Ledger) -> (String, String) {
    ledger
        .conn()
        .query_row(
            "SELECT session_id, source_session_key FROM source_sessions LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("source session")
}

/// PIN 1 — the identity that would have caused an A102-class failure.
///
/// `tool_use_id` is `source_tool_identity(**session_id**, call_id)` — the
/// DERIVED session id, not the raw `source_session_key` from the transcript.
/// Anything binding a wrapper run to an ingested event must key on the derived
/// id; using the raw key produces a different hash and binds nothing.
#[test]
fn the_tool_identity_is_keyed_on_the_derived_session_id_not_the_raw_key() {
    let probe = ingest_probe();
    let ledger = Ledger::open(&probe.db).expect("reopen");
    let (session_id, source_session_key) = session_ids(&ledger);
    assert_ne!(
        session_id, source_session_key,
        "the probe is only meaningful while these differ"
    );

    let tool_uses: Vec<_> = events(&ledger)
        .into_iter()
        .filter(|(_, kind, ..)| kind == "tool_use")
        .collect();
    assert_eq!(tool_uses.len(), 2, "both function_calls must ingest");

    let observed = &tool_uses[0].2;
    assert_eq!(
        *observed,
        source_tool_identity(&session_id, "call-obj"),
        "tool_use_id must be keyed on the DERIVED session id"
    );
    assert_ne!(
        *observed,
        source_tool_identity(&source_session_key, "call-obj"),
        "if this ever becomes equal the distinction has collapsed and the binding \
         recipe must be re-derived — do not silently relax this"
    );
}

/// PIN 2 — both `arguments` shapes normalise to a readable command.
///
/// Codex writes `arguments` as an object in some versions and as a JSON STRING
/// in others. Ingest normalises both, so `source_native_command`'s
/// `json_string_field(tool_input, "command")` reads back either. This removes a
/// guess the fixture would otherwise have had to make.
#[test]
fn both_argument_shapes_read_back_as_a_command() {
    let probe = ingest_probe();
    let ledger = Ledger::open(&probe.db).expect("reopen");
    let commands: Vec<Option<String>> = events(&ledger)
        .iter()
        .filter(|(_, kind, ..)| kind == "tool_use")
        .map(|(_, _, _, _, input, _)| thruline_core::derive::json_string_field(input, "command"))
        .collect();
    assert_eq!(
        commands,
        vec![
            Some("git status --porcelain".to_string()), // arguments as an OBJECT
            Some("git diff --stat".to_string()),        // arguments as a JSON STRING
        ],
        "both shapes must read back, or the fixture has to pick one and hope"
    );
}

/// PIN 3 — the claim and its command share one `source_turn_id`, and it is a
/// hash rather than the transcript's raw `turn_id`.
///
/// The binding needs the claim's evidence and the attested run inside one turn.
/// This pins that an ingested transcript actually provides that, and that the
/// turn identity is derived — so a wrapper run must supply the same derived
/// value, never the raw string.
#[test]
fn the_claim_and_its_command_share_a_derived_turn_identity() {
    let probe = ingest_probe();
    let ledger = Ledger::open(&probe.db).expect("reopen");
    let rows = events(&ledger);

    let claim_turn = rows
        .iter()
        .find(|(_, kind, ..)| kind == "assistant_text")
        .map(|(_, _, _, turn, _, _)| turn.clone())
        .expect("the assistant claim must ingest");
    let command_turn = rows
        .iter()
        .find(|(_, kind, ..)| kind == "tool_use")
        .map(|(_, _, _, turn, _, _)| turn.clone())
        .expect("the command must ingest");

    assert_eq!(
        claim_turn, command_turn,
        "claim and command must share a turn, or nothing can bind them"
    );
    assert_ne!(
        claim_turn, "probe-turn",
        "the turn identity is DERIVED; a wrapper run must supply the derived value"
    );
    assert_eq!(claim_turn.len(), 64, "it is a sha256 hex digest");
}

#[test]
fn the_probe_transcript_ingests_as_a_codex_project() {
    let probe = ingest_probe();
    let ledger = Ledger::open(&probe.db).expect("reopen");
    let (id, source): (String, String) = ledger
        .conn()
        .query_row("SELECT id, source FROM projects LIMIT 1", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("project");
    assert_eq!(source, "codex");
    assert!(
        id.starts_with("project_"),
        "codex projects are content-addressed, not munged paths: {id}"
    );
}
