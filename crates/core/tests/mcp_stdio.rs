//! A89 acceptance: the three tools over stdio, against a generated ledger.
//!
//! The fixture is built through the real pipeline (A90's rule), reusing the
//! same generator as the M6.R3 packet golden so the two cannot drift apart.

use std::fs;

use serde_json::{json, Value};
use thruline_core::annotate::annotate;
use thruline_core::config::Config;
use thruline_core::derive::derive_project_with;
use thruline_core::ingest::ingest;
use thruline_core::mcp::Server;
use thruline_core::nli::{EntailmentScorer, Scores};
use thruline_core::packet::{compile, Budget, Scope};
use thruline_core::{Ledger, Result};

const PROJ: &str = "-Users-dev-mcp";
const SESSION: &str = "bbbbbbbb-1111-4222-8333-444444444444";

struct NeverContradicts;
impl EntailmentScorer for NeverContradicts {
    fn score(&mut self, _p: &str, _h: &str) -> Result<Scores> {
        Ok(Scores {
            entailment: 0.30,
            neutral: 0.70,
            contradiction: 0.0,
        })
    }
}

fn uuid(i: usize) -> String {
    format!("00000000-0000-4000-8000-{i:012}")
}

fn envelope(i: usize, parent: Option<usize>, message: Value) -> Value {
    json!({
        "type": message["role"].as_str().unwrap_or("user"),
        "uuid": uuid(i),
        "parentUuid": parent.map(uuid),
        "isSidechain": false,
        "sessionId": SESSION,
        "timestamp": format!("2026-08-05T03:00:{i:02}.000Z"),
        "cwd": "/repo",
        "gitBranch": "main",
        "message": message,
    })
}

fn transcript() -> String {
    let mut lines = vec![envelope(1, None, json!({"role":"user","content":"run it"}))];
    lines.push(envelope(
        2,
        Some(1),
        json!({"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"npm test"}}]}),
    ));
    lines.push(envelope(
        3,
        Some(2),
        json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"# pass 11\n# fail 1"}]}),
    ));
    lines.push(envelope(
        4,
        Some(3),
        json!({"role":"assistant","content":[{"type":"text","text":"All 12 tests pass."}]}),
    ));
    lines
        .into_iter()
        .map(|l| serde_json::to_string(&l).expect("serialize"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Returns the tempdir guard and the db path. The guard must outlive every
/// ledger opened against it.
fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    fs::create_dir_all(root.join(PROJ)).expect("mkdir");
    fs::write(
        root.join(PROJ).join(format!("{SESSION}.jsonl")),
        transcript(),
    )
    .expect("write");
    let db = dir.path().join("ledger.db");
    let mut ledger = Ledger::open(&db).expect("open");
    ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    annotate(
        &ledger,
        PROJ,
        &Config::default(),
        &mut NeverContradicts,
        "mcp-stub",
    )
    .expect("annotate");
    drop(ledger);
    (dir, db)
}

fn call(server: &Server, tool: &str, args: Value) -> Value {
    let line = json!({
        "jsonrpc":"2.0","id":1,"method":"tools/call",
        "params": {"name": tool, "arguments": args}
    })
    .to_string();
    let raw = server
        .handle_line(&line)
        .expect("a request must get a reply");
    let response: Value = serde_json::from_str(&raw).expect("valid json");
    assert!(
        response.get("error").is_none(),
        "tool {tool} errored: {response}"
    );
    let text = response["result"]["content"][0]["text"]
        .as_str()
        .expect("text content");
    serde_json::from_str(text).expect("tool payload is json")
}

#[test]
fn the_handshake_advertises_exactly_three_tools() {
    let (_d, db) = fixture();
    let server = Server::new("claude", db).expect("server");
    let init = server
        .handle_line(&json!({"jsonrpc":"2.0","id":1,"method":"initialize"}).to_string())
        .expect("reply");
    assert!(init.contains("protocolVersion"), "{init}");

    let listed = server
        .handle_line(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}).to_string())
        .expect("reply");
    let value: Value = serde_json::from_str(&listed).expect("json");
    let tools = value["result"]["tools"].as_array().expect("tools");
    assert_eq!(tools.len(), 3);
}

#[test]
fn a_notification_gets_no_reply() {
    let (_d, db) = fixture();
    let server = Server::new("claude", db).expect("server");
    // No id => notification. Answering one is a protocol violation.
    let out = server
        .handle_line(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string());
    assert!(out.is_none());
}

/// The ACCEPT line: the golden packet arrives byte-identical through the tool.
#[test]
fn the_packet_arrives_byte_identical_through_the_tool() {
    let (_d, db) = fixture();
    let direct = {
        let ledger = Ledger::open(&db).expect("open");
        let graph = derive_project_with(&ledger, PROJ, &Config::default()).expect("derive");
        compile(
            &ledger,
            std::slice::from_ref(&graph),
            &Scope::Project(PROJ.to_string()),
            Some("close it out"),
            &Budget::default(),
        )
        .expect("compile")
    };

    let server = Server::new("claude", db).expect("server");
    let via_tool = call(
        &server,
        "get_context_packet",
        json!({"project": PROJ, "task": "close it out"}),
    );

    assert_eq!(
        via_tool["packet_md"].as_str().expect("md"),
        direct.render(),
        "the tool must serve the same bytes the compiler produces"
    );
    assert_eq!(
        via_tool["chain_position"].as_str().expect("position"),
        direct.binding.digest,
        "the chain position IS the snapshot id — there is no second identity"
    );
}

#[test]
fn check_context_answers_current_then_not_current() {
    let (_d, db) = fixture();
    let server = Server::new("claude", db.clone()).expect("server");
    let packet = call(&server, "get_context_packet", json!({"project": PROJ}));
    let position = packet["chain_position"]
        .as_str()
        .expect("position")
        .to_string();

    let fresh = call(
        &server,
        "check_context",
        json!({"project": PROJ, "chain_position": position}),
    );
    assert_eq!(fresh["current"], json!(true));
    assert_eq!(fresh["status"], json!("current"));
    assert_eq!(
        fresh["deltas"],
        json!([]),
        "an unmoved position has an empty change set, not a null one"
    );

    let stale = call(
        &server,
        "check_context",
        json!({"project": PROJ, "chain_position": "0".repeat(64)}),
    );
    assert_eq!(stale["current"], json!(false));
    assert!(
        stale["latest_chain_position"]
            .as_str()
            .is_some_and(|s| s.len() == 64),
        "a not-current answer must carry the latest position"
    );
    // R13: a position we never served cannot be diffed, and must say so rather
    // than imply nothing changed.
    assert_eq!(stale["status"], json!("unknown_position"));
    assert_eq!(stale["deltas"], Value::Null);
}

/// The other ACCEPT line: a stale-position write is rejected.
#[test]
fn a_stale_position_write_is_rejected_with_the_latest() {
    let (_d, db) = fixture();
    let server = Server::new("claude", db).expect("server");
    let rejected = call(
        &server,
        "claim_completion",
        json!({
            "project": PROJ,
            "obligation": "ship it",
            "chain_position": "0".repeat(64),
            "manifest": {"commands_run": ["npm test"], "commit": "abc123"}
        }),
    );
    assert_eq!(rejected["accepted"], json!(false));
    assert_eq!(rejected["status"], json!("rejected_stale_position"));
    assert!(rejected["latest_chain_position"].as_str().is_some());
}

#[test]
fn an_accepted_claim_is_ready_for_verification_and_never_done() {
    let (_d, db) = fixture();
    let server = Server::new("claude", db).expect("server");
    let packet = call(&server, "get_context_packet", json!({"project": PROJ}));
    let position = packet["chain_position"]
        .as_str()
        .expect("position")
        .to_string();

    let accepted = call(
        &server,
        "claim_completion",
        json!({
            "project": PROJ,
            "obligation": "ship it",
            "chain_position": position,
            "manifest": {"commands_run": ["npm test"], "commit": "abc123"}
        }),
    );
    assert_eq!(accepted["accepted"], json!(true));
    assert_eq!(accepted["status"], json!("ready_for_verification"));

    // The response may never imply a verdict.
    let rendered = accepted.to_string().to_lowercase();
    for forbidden in ["\"done\"", "\"complete\"", "\"verified\"", "\"supported\""] {
        assert!(
            !rendered.contains(forbidden),
            "the response asserted {forbidden}, which is the verifier's word, not the claimant's: {rendered}"
        );
    }
    assert_eq!(accepted["axes"]["evidence"], json!("unknown"));

    // A201: the loop is closed, and the response may now truthfully say so —
    // the verifier exists, runs when invoked, and the claim has a typeable
    // handle. It still promises nothing the product does not do.
    assert!(
        accepted["claim_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("tlc_")),
        "the claim hands back its why handle: {accepted}"
    );
    let note = accepted["note"].as_str().expect("note");
    assert!(
        note.contains("thruline verify-claims"),
        "the verifier is named: {note}"
    );
    assert!(
        note.contains("when it is run"),
        "and not overpromised — it runs when invoked: {note}"
    );
}

#[test]
fn a_missing_field_is_rejected_rather_than_guessed() {
    let (_d, db) = fixture();
    let server = Server::new("claude", db).expect("server");
    let out = call(
        &server,
        "claim_completion",
        json!({"project": PROJ, "obligation": "ship it"}),
    );
    assert_eq!(out["status"], json!("rejected_missing_fields"));
    let missing: Vec<&str> = out["missing"]
        .as_array()
        .expect("array")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(missing.contains(&"chain_position") && missing.contains(&"manifest"));
}

/// Every call is recorded, with the agent, and identical calls do not collide.
#[test]
fn every_call_is_recorded_with_its_agent_and_none_are_swallowed() {
    let (_d, db) = fixture();
    let server = Server::new("claude", db.clone()).expect("server");
    let packet = call(&server, "get_context_packet", json!({"project": PROJ}));
    let position = packet["chain_position"]
        .as_str()
        .expect("position")
        .to_string();

    // Two byte-identical polls that both answer "current". If their records
    // collided, the second would vanish and "every call is recorded" would be
    // quietly false.
    for _ in 0..2 {
        let out = call(
            &server,
            "check_context",
            json!({"project": PROJ, "chain_position": position}),
        );
        assert_eq!(out["current"], json!(true));
    }

    let ledger = Ledger::open(&db).expect("reopen");
    let calls = ledger.mcp_calls().expect("calls");
    assert_eq!(calls.len(), 3, "one packet fetch plus two identical polls");
    assert!(
        calls.iter().all(|c| c.agent == "claude"),
        "every call carries its agent"
    );
    let tools: Vec<&str> = calls.iter().map(|c| c.tool.as_str()).collect();
    assert_eq!(
        tools,
        ["get_context_packet", "check_context", "check_context"]
    );
    let ids: std::collections::BTreeSet<&str> = calls.iter().map(|c| c.call_id.as_str()).collect();
    assert_eq!(ids.len(), 3, "identical calls must still get distinct ids");
}

/// Statelessness: a fresh server per call must be indistinguishable from one
/// long-lived server, which is what makes kill/restart invisible.
#[test]
fn a_restart_between_calls_is_invisible() {
    let (_d, db) = fixture();
    let first = Server::new("claude", db.clone()).expect("server");
    let a = call(&first, "get_context_packet", json!({"project": PROJ}));
    drop(first);

    let second = Server::new("claude", db.clone()).expect("server");
    let b = call(&second, "get_context_packet", json!({"project": PROJ}));
    drop(second);

    assert_eq!(
        a["packet_md"], b["packet_md"],
        "a restart changed the answer"
    );
    assert_eq!(a["chain_position"], b["chain_position"]);

    // And the position issued before the restart still gates as current after
    // it — the record did not move just because the process did.
    let third = Server::new("claude", db).expect("server");
    let check = call(
        &third,
        "check_context",
        json!({"project": PROJ, "chain_position": a["chain_position"].as_str().expect("pos")}),
    );
    assert_eq!(check["current"], json!(true));
}

/// Serving a packet must not move the chain position it just issued — the
/// defect that made "ledger every call as an event" self-defeating.
#[test]
fn serving_a_packet_does_not_move_the_position_it_issued() {
    let (_d, db) = fixture();
    let server = Server::new("claude", db).expect("server");
    let first = call(&server, "get_context_packet", json!({"project": PROJ}));
    let second = call(&server, "get_context_packet", json!({"project": PROJ}));
    assert_eq!(
        first["chain_position"], second["chain_position"],
        "asking for a packet must not invalidate the packet"
    );
}

#[test]
fn an_anonymous_server_cannot_be_constructed() {
    let (_d, db) = fixture();
    assert!(Server::new("", db).is_err());
}
