//! A126 — binding recorded MCP calls to the transcript events they became.
//!
//! The call is served and recorded while it happens. The event only exists once
//! the client's session is ingested, later. Nothing can know both at once, which
//! is why `mcp_calls.event_id` was never fillable and why the binding is derived
//! state in its own rebuildable table.
//!
//! The bind is an IDENTITY, not a guess: both sides carry
//! `sha256(canonical(arguments))`.
//!
//! **A135 narrowed what "identity" can mean here.** That hash identifies the
//! *arguments*, not the *call* — two calls carrying identical arguments produce
//! identical hashes, and `mcp_calls` records no session to tell them apart. So the
//! binder now binds only where a `(tool, request_sha256)` group holds exactly one
//! call and exactly one causally-admissible event, and **abstains** otherwise.
//!
//! **Fixture timestamps were re-stamped `2030-01-01` → `2020-01-01` at A135.** The
//! old stamps put every fixture event in the future relative to the call that was
//! served "now", so under the causality floor every fixture would have abstained.
//! The events, not the code, were wrong: a transcript line cannot postdate the call
//! it produced. Named here because a silent fixture edit that turns a suite green
//! is the shape of thing this project exists to catch.

use std::fs;

use serde_json::json;
use tempfile::TempDir;
use thruline_core::ingest::{ingest, IngestReport};
use thruline_core::mcp::{bind::bind_calls, Server};
use thruline_core::Ledger;

const PROJECT: &str = "-bind-proj";

/// A Claude Code session in which the assistant called a thruline MCP tool.
/// This is the shape Claude Code actually writes — the ledger already holds
/// `mcp__<server>__<tool>` names from other servers.
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
) -> IngestReport {
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
    .expect("ingest")
}

/// Serve a real call through the real server, then ingest the session that made
/// it, then bind. Every row on both sides is produced by a production path.
#[test]
fn a_served_call_binds_to_the_ingested_event_it_became() {
    let tmp = TempDir::new().expect("tmp");
    let db = tmp.path().join("ledger.db");
    let root = tmp.path().join("projects");

    // The session must exist first, so the packet has a project to scope to.
    let args = json!({"project": PROJECT, "task": "bind me"});
    {
        let mut ledger = Ledger::open(&db).expect("open");
        ingest_transcript(&mut ledger, &root, "sess-a", &args);
    }

    // A REAL call through the REAL server — this writes the mcp_calls row.
    let server = Server::new("claude", db.clone()).expect("server");
    server
        .call("get_context_packet", &args)
        .expect("serve the call");

    let ledger = Ledger::open(&db).expect("reopen");
    let before: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM mcp_calls", [], |r| r.get(0))
        .expect("count");
    assert_eq!(before, 1, "the server must have recorded exactly one call");

    let report = bind_calls(&ledger).expect("bind");
    assert_eq!(report.calls, 1);
    assert_eq!(report.bound, 1, "the call must bind: {report:?}");
    assert_eq!(report.newly_bound, 1);
    assert_eq!(report.rate(), Some(1.0));
    assert!(report.unmatched.is_empty());

    // And it bound to the RIGHT event — the tool_use, not some neighbour.
    let (kind, tool): (String, String) = ledger
        .conn()
        .query_row(
            "SELECT e.kind, e.tool_name FROM mcp_call_bindings b
             JOIN events e ON e.id = b.event_id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("joined row");
    assert_eq!(kind, "tool_use");
    assert_eq!(tool, "mcp__thruline__get_context_packet");
}

/// THE NEGATIVE. Same tool, different arguments — the hashes differ, so nothing
/// binds. A proximity rule would have bound this one happily.
#[test]
fn an_event_with_different_arguments_does_not_bind() {
    let tmp = TempDir::new().expect("tmp");
    let db = tmp.path().join("ledger.db");
    let root = tmp.path().join("projects");

    let served = json!({"project": PROJECT, "task": "the call that happened"});
    let recorded = json!({"project": PROJECT, "task": "a DIFFERENT task"});
    {
        let mut ledger = Ledger::open(&db).expect("open");
        ingest_transcript(&mut ledger, &root, "sess-a", &recorded);
    }
    let server = Server::new("claude", db.clone()).expect("server");
    server.call("get_context_packet", &served).expect("serve");

    let ledger = Ledger::open(&db).expect("reopen");
    let report = bind_calls(&ledger).expect("bind");
    assert_eq!(report.calls, 1);
    assert_eq!(
        report.bound, 0,
        "arguments differ, so this is not that call: {report:?}"
    );
    assert_eq!(report.unmatched.len(), 1);
    assert_eq!(
        report.rate(),
        Some(0.0),
        "a real measured zero, over a real call"
    );
}

/// Rebuildable and idempotent: running it again binds nothing new and leaves
/// the same rows.
#[test]
fn binding_twice_changes_nothing() {
    let tmp = TempDir::new().expect("tmp");
    let db = tmp.path().join("ledger.db");
    let root = tmp.path().join("projects");
    let args = json!({"project": PROJECT});
    {
        let mut ledger = Ledger::open(&db).expect("open");
        ingest_transcript(&mut ledger, &root, "sess-a", &args);
    }
    Server::new("claude", db.clone())
        .expect("server")
        .call("get_context_packet", &args)
        .expect("serve");

    let ledger = Ledger::open(&db).expect("reopen");
    let first = bind_calls(&ledger).expect("bind");
    let second = bind_calls(&ledger).expect("bind again");
    assert_eq!(first.bound, second.bound);
    assert_eq!(second.newly_bound, 0, "a second pass must bind nothing new");
}

/// A rate over an empty set is UNDEFINED, not zero. A95 recorded that reporting
/// 0% with nothing to measure would be exactly the number this product refuses.
#[test]
fn an_empty_ledger_reports_no_rate_rather_than_zero_percent() {
    let tmp = TempDir::new().expect("tmp");
    let ledger = Ledger::open(tmp.path().join("ledger.db")).expect("open");
    let report = bind_calls(&ledger).expect("bind");
    assert_eq!(report.calls, 0);
    assert_eq!(report.rate(), None, "undefined, not 0.0");
    assert!(
        report.line().contains("nothing to bind"),
        "and it must say so: {}",
        report.line()
    );
}

/// A135, replacing A133's `two_identical_calls_bind_to_two_distinct_events`.
///
/// **The intent that changed.** The old test demanded that two identical calls
/// each claim a different event. Distinctness was the right answer to A133's
/// defect — one event double-claimed, `newly_bound` incremented anyway — but it is
/// the wrong answer to this one. Two calls carrying byte-identical arguments from
/// TWO DIFFERENT SESSIONS are indistinguishable in the record: `mcp_calls` stores
/// `agent`, never a session. Assigning them one event each produced two bindings
/// that were *distinct*, *deterministic*, and *crossed* — on the live ledger, one
/// of them named an event 41.7 s after the call it was bound to.
///
/// Distinct-and-wrong is not better than abstained. This is the concurrent
/// multi-agent case the product exists for, and the honest answer is that the
/// binder cannot tell, and says so.
#[test]
fn identical_calls_from_two_sessions_abstain_rather_than_guess() {
    let tmp = TempDir::new().expect("tmp");
    let db = tmp.path().join("ledger.db");
    let root = tmp.path().join("projects");

    let args = json!({"project": PROJECT, "task": "the same question twice"});
    {
        let mut ledger = Ledger::open(&db).expect("open");
        ingest_transcript(&mut ledger, &root, "sess-a", &args);
        ingest_transcript(&mut ledger, &root, "sess-b", &args);
    }

    let server = Server::new("claude", db.clone()).expect("server");
    server.call("get_context_packet", &args).expect("first");
    server.call("get_context_packet", &args).expect("second");

    let ledger = Ledger::open(&db).expect("reopen");
    let report = bind_calls(&ledger).expect("bind");

    assert_eq!(report.calls, 2);
    assert_eq!(
        report.bound, 0,
        "two events fit both calls equally; neither is identified: {report:?}"
    );
    assert_eq!(report.newly_bound, 0);
    assert_eq!(
        report.unmatched.len(),
        2,
        "an abstention is a fact and both calls are named: {:?}",
        report.unmatched
    );
    // A133's invariant, still load-bearing.
    assert!(
        report.newly_bound <= report.bound,
        "newly_bound must never exceed bound: {report:?}"
    );

    let rows: i64 = ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM mcp_call_bindings", [], |r| r.get(0))
        .expect("count");
    assert_eq!(rows, 0, "a guess must not be written down");
}

/// A135 — the causality floor, on its own. One call, one candidate, and the
/// candidate's transcript stamp POSTDATES the call. That event cannot be that
/// call, so nothing binds even though the hash matches perfectly.
///
/// This is the shape that put a binding 41.7 s into the future on the live ledger.
/// The floor is necessary and NOT sufficient — it cannot separate candidates that
/// all precede all of a group's calls — which is why abstention carries the rest.
#[test]
fn an_event_that_postdates_its_call_is_inadmissible() {
    let tmp = TempDir::new().expect("tmp");
    let db = tmp.path().join("ledger.db");
    let root = tmp.path().join("projects");
    let args = json!({"project": PROJECT, "task": "from the future"});

    // Deliberately future-stamped: the old fixture default, kept here as the
    // negative it always secretly was.
    let dir = root.join(PROJECT);
    fs::create_dir_all(&dir).expect("mkdir");
    let input = serde_json::to_string(&args).expect("args");
    fs::write(
        dir.join("sess-future.jsonl"),
        format!(
            concat!(
                r#"{{"type":"user","uuid":"f-1","sessionId":"sess-future","timestamp":"2030-01-01T00:00:00.000Z","parentUuid":null,"isSidechain":false,"cwd":"/x","message":{{"role":"user","content":"q"}}}}"#,
                "\n",
                r#"{{"type":"assistant","uuid":"f-2","sessionId":"sess-future","timestamp":"2030-01-01T00:00:01.000Z","parentUuid":"f-1","isSidechain":false,"cwd":"/x","message":{{"role":"assistant","content":[{{"type":"tool_use","id":"tu-f","name":"mcp__thruline__get_context_packet","input":{input}}}]}}}}"#,
                "\n"
            ),
            input = input
        ),
    )
    .expect("write");
    {
        let mut ledger = Ledger::open(&db).expect("open");
        ingest(
            &mut ledger,
            std::slice::from_ref(&root.to_path_buf()),
            &root.join("mirrors-scratch"),
        )
        .expect("ingest");
    }

    Server::new("claude", db.clone())
        .expect("server")
        .call("get_context_packet", &args)
        .expect("serve");

    let ledger = Ledger::open(&db).expect("reopen");
    let report = bind_calls(&ledger).expect("bind");
    assert_eq!(report.calls, 1);
    assert_eq!(
        report.bound, 0,
        "a call cannot be caused by a later event: {report:?}"
    );
    assert_eq!(report.unmatched.len(), 1);
}

/// A135 — a binding the data no longer justifies is WITHDRAWN, not left standing.
///
/// This is the live ledger's own situation in miniature. One session calls and
/// binds cleanly. Later a second session issues a byte-identical call, which makes
/// the group ambiguous — so the earlier binding stops being derivable and must go.
/// Leaving it would let a row that was true under old evidence keep presenting as
/// current, which is the `Currency` distinction the whole product turns on.
#[test]
fn a_binding_the_data_no_longer_justifies_is_withdrawn() {
    let tmp = TempDir::new().expect("tmp");
    let db = tmp.path().join("ledger.db");
    let root = tmp.path().join("projects");
    let args = json!({"project": PROJECT, "task": "unambiguous, then not"});

    {
        let mut ledger = Ledger::open(&db).expect("open");
        ingest_transcript(&mut ledger, &root, "sess-a", &args);
    }
    Server::new("claude", db.clone())
        .expect("server")
        .call("get_context_packet", &args)
        .expect("first");
    {
        let ledger = Ledger::open(&db).expect("reopen");
        let first = bind_calls(&ledger).expect("bind");
        assert_eq!(
            first.bound, 1,
            "one call, one event, unambiguous: {first:?}"
        );
    }

    // A second session asks the identical question. Nothing about the first call
    // changed — but what the record can now prove about it did.
    {
        let mut ledger = Ledger::open(&db).expect("open");
        ingest_transcript(&mut ledger, &root, "sess-b", &args);
    }
    Server::new("claude", db.clone())
        .expect("server")
        .call("get_context_packet", &args)
        .expect("second");

    let ledger = Ledger::open(&db).expect("reopen");
    let report = bind_calls(&ledger).expect("rebind");
    assert_eq!(report.calls, 2);
    assert_eq!(report.bound, 0, "the group is now ambiguous: {report:?}");
    assert_eq!(report.withdrawn, 1, "the stale row is removed: {report:?}");
    assert!(
        report.line().contains("withdrawn"),
        "and the withdrawal is reported, not silent: {}",
        report.line()
    );
}

/// A133, rewritten at A135 — two identical calls, only ONE event.
///
/// **A133's intent is preserved and still asserted:** a refusal is routed to
/// `unmatched` and never dropped, and `newly_bound` may never exceed `bound`. Under
/// A133 that produced 1 bound and 1 unmatched — the ledger refused the second
/// insert on `UNIQUE(event_id)` and the loser had to be named rather than vanish.
///
/// **What A135 changed:** the winner is no longer a winner. Nothing identifies
/// which of the two calls the single event belongs to, so both abstain. The count
/// moves 1 → 0; the invariant it was protecting does not move.
#[test]
fn a_call_with_no_free_event_is_unmatched_not_counted() {
    let tmp = TempDir::new().expect("tmp");
    let db = tmp.path().join("ledger.db");
    let root = tmp.path().join("projects");

    let args = json!({"project": PROJECT, "task": "two calls, one transcript"});
    {
        let mut ledger = Ledger::open(&db).expect("open");
        ingest_transcript(&mut ledger, &root, "sess-a", &args);
    }

    let server = Server::new("claude", db.clone()).expect("server");
    server.call("get_context_packet", &args).expect("first");
    server.call("get_context_packet", &args).expect("second");

    let ledger = Ledger::open(&db).expect("reopen");
    let report = bind_calls(&ledger).expect("bind");

    assert_eq!(report.calls, 2);
    assert_eq!(
        report.bound, 0,
        "one event cannot say which call it belongs to: {report:?}"
    );
    assert_eq!(report.newly_bound, 0, "attempts are not bindings");
    assert_eq!(
        report.unmatched.len(),
        2,
        "neither is dropped — both are named: {:?}",
        report.unmatched
    );
    assert!(
        report.newly_bound <= report.bound,
        "newly_bound must never exceed bound: {report:?}"
    );
    assert_eq!(
        report.rate(),
        Some(0.0),
        "a measured zero, over two real calls"
    );
}
