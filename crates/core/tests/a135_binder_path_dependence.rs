//! A135 — is the MCP call/event binding REBUILDABLE?
//!
//! `mcp_binding.rs` states, in its own module docs, that the bind is "an IDENTITY,
//! not a guess", and `bind_calls` documents itself as "Idempotent and rebuildable".
//! Idempotence is tested (`binding_twice_changes_nothing`). Rebuildability is not.
//!
//! They are different properties. Idempotence says a second run over the SAME
//! bindings changes nothing. Rebuildability says the bindings are a function of the
//! DATA — that dropping the derived table and recomputing reproduces the same pairs.
//! The project invariant is the second one: derived state must be rebuildable from
//! events, byte-identically.
//!
//! This test asks only that question. It does not assert which pairing is correct.

use std::fs;

use serde_json::json;
use tempfile::TempDir;
use thruline_core::ingest::ingest;
use thruline_core::mcp::{bind::bind_calls, Server};
use thruline_core::Ledger;

const PROJECT: &str = "-path-dep-proj";

fn transcript(args: &serde_json::Value, session: &str) -> String {
    let input = serde_json::to_string(args).expect("args");
    format!(
        concat!(
            r#"{{"type":"user","uuid":"{s}-1","sessionId":"{s}","timestamp":"2020-01-01T00:00:00.000Z","parentUuid":null,"isSidechain":false,"cwd":"/x","message":{{"role":"user","content":"what is the state"}}}}"#,
            "\n",
            r#"{{"type":"assistant","uuid":"{s}-2","sessionId":"{s}","timestamp":"2020-01-01T00:00:01.000Z","parentUuid":"{s}-1","isSidechain":false,"cwd":"/x","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"tu-{s}","name":"mcp__thruline__get_context_packet","input":{input}}}]}}}}"#,
            "\n"
        ),
        s = session,
        input = input
    )
}

fn ingest_transcript(
    ledger: &mut Ledger,
    root: &std::path::Path,
    session: &str,
    args: &serde_json::Value,
) {
    let dir = root.join(PROJECT);
    fs::create_dir_all(&dir).expect("mkdir");
    fs::write(
        dir.join(format!("{session}.jsonl")),
        transcript(args, session),
    )
    .expect("write");
    ingest(
        ledger,
        std::slice::from_ref(&root.to_path_buf()),
        &root.join("mirrors-scratch"),
    )
    .expect("ingest");
}

/// call_id -> event_id, in a stable order, so two runs can be compared.
fn pairs(ledger: &Ledger) -> Vec<(String, String)> {
    let mut stmt = ledger
        .conn()
        .prepare(
            "SELECT c.accepted_seq, b.event_id FROM mcp_call_bindings b
             JOIN mcp_calls c ON c.call_id = b.call_id
             ORDER BY c.accepted_seq",
        )
        .expect("prepare");
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, i64>(0)?.to_string(), r.get::<_, String>(1)?))
        })
        .expect("query");
    rows.map(|r| r.expect("row")).collect()
}

/// THE QUESTION. Two sessions issue byte-identical calls. The bindings are built
/// INCREMENTALLY — session by session, binding after each — which is how they are
/// built in production, since `ingest` binds on every run.
///
/// Then the derived table is dropped and rebuilt from the same final data.
///
/// If the binding is a function of the data, the two results are equal. The session
/// ids are chosen so that lexicographic order (`sess-aaa` < `sess-zzz`) is the
/// REVERSE of the order the sessions were ingested and called in — which is what
/// separates "function of the data" from "function of the ingest history".
///
/// A third session asks a DIFFERENT question, so exactly one call is identifiable
/// and the comparison is over a non-empty result. Without it both sides would be
/// empty under abstention and the test would pass for the wrong reason.
///
/// **This test failed when it was written, and that failure was its point** — the
/// two pairings came back exactly swapped (A135). It was `#[ignore]`d while the fix
/// was unruled, and un-ignored in the commit that made it pass.
#[test]
fn a_rebuilt_binding_reproduces_the_incremental_one() {
    let tmp = TempDir::new().expect("tmp");
    let db = tmp.path().join("ledger.db");
    let root = tmp.path().join("projects");

    let shared = json!({"project": PROJECT, "task": "identical from two sessions"});
    let unique = json!({"project": PROJECT, "task": "asked once, by one session"});

    // --- History: sess-zzz exists and calls first. Bind. ---
    {
        let mut ledger = Ledger::open(&db).expect("open");
        ingest_transcript(&mut ledger, &root, "sess-zzz", &shared);
    }
    let server = Server::new("claude", db.clone()).expect("server");
    server.call("get_context_packet", &shared).expect("call 1");
    {
        let ledger = Ledger::open(&db).expect("reopen");
        bind_calls(&ledger).expect("bind after first");
    }

    // --- Later: sess-aaa asks the SAME thing, sess-uniq asks its own. ---
    {
        let mut ledger = Ledger::open(&db).expect("open");
        ingest_transcript(&mut ledger, &root, "sess-aaa", &shared);
        ingest_transcript(&mut ledger, &root, "sess-uniq", &unique);
    }
    let server = Server::new("claude", db.clone()).expect("server");
    server.call("get_context_packet", &shared).expect("call 2");
    server.call("get_context_packet", &unique).expect("call 3");

    let incremental = {
        let ledger = Ledger::open(&db).expect("reopen");
        bind_calls(&ledger).expect("bind after second");
        pairs(&ledger)
    };

    // --- Rebuild. Same events, same calls, empty derived table. ---
    // `mcp_call_bindings` carries no append-only trigger: it is derived state and
    // is meant to be reconstructible. Dropping it is the rebuild, not a hack.
    let rebuilt = {
        let ledger = Ledger::open(&db).expect("reopen");
        ledger
            .conn()
            .execute("DELETE FROM mcp_call_bindings", [])
            .expect("clear derived state");
        bind_calls(&ledger).expect("rebind from scratch");
        pairs(&ledger)
    };

    assert_eq!(
        incremental, rebuilt,
        "the binding must be a function of the data, not of the ingest history.\n\
         incremental: {incremental:?}\n\
         rebuilt:     {rebuilt:?}"
    );

    // And the comparison is over something. Only call 3 is identifiable: calls 1
    // and 2 wear the same arguments and cannot be told apart.
    assert_eq!(
        incremental.len(),
        1,
        "exactly the unambiguous call binds: {incremental:?}"
    );
    assert_eq!(incremental[0].0, "3", "and it is the uniquely-argued one");
}
