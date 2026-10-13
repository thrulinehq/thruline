//! Where the agent said it — the file and line of a linked conversation's final
//! claim in its own log, and none of the words.
//!
//! The place is found only from lines the ledger holds unchanged and only from
//! text whose own structured role field names the agent. Every transcript here
//! is written by the test, ingested by the real `ingest`, and linked to a
//! Change by a confirmed association row. Every id is synthetic (`cccccccc-`
//! prefix); no fixture holds private content.

use std::path::PathBuf;

use rusqlite::params;
use thruline_core::agent_log::{self, NotLocated};
use thruline_core::ledger::Ledger;

const SESSION: &str = "cccccccc-0000-4000-8000-00000000a51d";
const CHANGE: &str = "cccccccc-0000-4000-8000-00000000c51d";
const WORDS: &str = "Done. The cart total is added and the tests pass.";

struct Store {
    dir: tempfile::TempDir,
    ledger: Ledger,
}

impl Store {
    fn transcript(&self) -> PathBuf {
        self.dir
            .path()
            .join("projects")
            .join("-synthetic-shop")
            .join(format!("{SESSION}.jsonl"))
    }
}

fn user(n: u32, text: &str) -> String {
    serde_json::json!({
        "type": "user", "uuid": format!("cccccccc-0000-4000-8000-00000000000{n}"),
        "isSidechain": false, "sessionId": SESSION, "cwd": "/synthetic/shop",
        "timestamp": format!("2026-09-23T09:00:0{n}.000Z"),
        "message": { "role": "user", "content": text },
    })
    .to_string()
}

fn tool_use(n: u32) -> String {
    serde_json::json!({
        "type": "assistant", "uuid": format!("cccccccc-0000-4000-8000-00000000000{n}"),
        "isSidechain": false, "sessionId": SESSION, "cwd": "/synthetic/shop",
        "timestamp": format!("2026-09-23T09:00:0{n}.000Z"),
        "message": { "role": "assistant", "content": [
            { "type": "tool_use", "id": "toolu_q1", "name": "Edit",
              "input": { "file_path": "/synthetic/shop/src/lib.rs", "old_string": "x", "new_string": "y" } }
        ] },
    })
    .to_string()
}

fn tool_result(n: u32) -> String {
    serde_json::json!({
        "type": "user", "uuid": format!("cccccccc-0000-4000-8000-00000000000{n}"),
        "isSidechain": false, "sessionId": SESSION, "cwd": "/synthetic/shop",
        "timestamp": format!("2026-09-23T09:00:0{n}.000Z"),
        "message": { "role": "user", "content": [
            { "type": "tool_result", "tool_use_id": "toolu_q1", "content": "The file has been updated." }
        ] },
    })
    .to_string()
}

/// An assistant text line; `role` is what the line's own `message.role`
/// says, and `None` leaves the field out.
fn said(n: u32, role: Option<&str>, text: &str) -> String {
    let mut message = serde_json::json!({ "content": [{ "type": "text", "text": text }] });
    if let Some(role) = role {
        message["role"] = serde_json::Value::String(role.to_string());
    }
    serde_json::json!({
        "type": "assistant", "uuid": format!("cccccccc-0000-4000-8000-00000000000{n}"),
        "isSidechain": false, "sessionId": SESSION, "cwd": "/synthetic/shop",
        "timestamp": format!("2026-09-23T09:00:0{n}.000Z"),
        "message": message,
    })
    .to_string()
}

/// The walk's conversation (docs/loop/run/walk/setup.sh), with the last line's
/// role field as given.
fn walk_lines(role: Option<&str>, words: &str) -> Vec<String> {
    vec![
        user(1, "Add a cart total function."),
        tool_use(2),
        tool_result(3),
        said(4, role, words),
    ]
}

/// Ingest `lines` with the real ingest, open a Change, and link the
/// conversation with the given provenance.
fn store_with(lines: &[String], provenance: &str) -> Store {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    let project = root.join("-synthetic-shop");
    std::fs::create_dir_all(&project).expect("mkdir");
    let file = project.join(format!("{SESSION}.jsonl"));
    std::fs::write(&file, lines.join("\n") + "\n").expect("transcript");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    thruline_core::ingest::ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    let project_id: String = ledger
        .conn()
        .query_row(
            "SELECT project_id FROM events WHERE session_id=?1 LIMIT 1",
            params![SESSION],
            |r| r.get(0),
        )
        .expect("the conversation was ingested");
    ledger
        .conn()
        .execute(
            "INSERT INTO changes(id, project_id, opened_at, title, purpose, request_event_id,
                                 requester, request_source)
             VALUES(?1, ?2, '2026-09-23T08:00:00Z', 'Cart total', 'Add a cart total.', NULL,
                    'sam', 'person')",
            params![CHANGE, project_id],
        )
        .expect("change");
    ledger
        .conn()
        .execute(
            "INSERT INTO change_session_associations(id, change_id, session_id, provenance,
                                                     basis, actor, asserted_at, reason)
             VALUES('cccccccc-0000-4000-8000-00000000as01', ?1, ?2, ?3, 'user', 'sam',
                    '2026-09-23T09:40:00Z', NULL)",
            params![CHANGE, SESSION, provenance],
        )
        .expect("association");
    Store { dir, ledger }
}

fn view(store: &Store) -> agent_log::AgentLogView {
    let change = thruline_core::change::read_change(&store.ledger, CHANGE)
        .expect("read")
        .expect("the Change");
    agent_log::for_change(&store.ledger, &change).expect("agent_log")
}

fn only(view: &agent_log::AgentLogView) -> &agent_log::AgentLogEntry {
    assert_eq!(view.sessions.len(), 1, "{view:?}");
    assert_eq!(view.sessions[0].session_id, SESSION);
    &view.sessions[0]
}

/* ── the place, when the log's own field names the agent ─────────────── */

#[test]
fn the_walks_conversation_is_located_at_its_final_message() {
    let store = store_with(&walk_lines(Some("assistant"), WORDS), "confirmed");
    let v = view(&store);
    let entry = only(&v);
    let receipt = entry.receipt.as_ref().expect("a place");
    assert_eq!(entry.not_located, None);
    assert_eq!(receipt.line, 4, "the line the words are on");
    assert_eq!(receipt.file, store.transcript().to_string_lossy());
}

#[test]
fn the_place_is_the_line_of_the_last_claim_of_the_final_turn() {
    // An earlier turn claims too; the place is the final turn's.
    let lines = vec![
        user(1, "Add a cart total function."),
        said(2, Some("assistant"), "The tests pass."),
        user(3, "Now the coupon field."),
        tool_use(4),
        tool_result(5),
        said(
            6,
            Some("assistant"),
            "The coupon field is added and the tests pass.",
        ),
    ];
    let store = store_with(&lines, "confirmed");
    let v = view(&store);
    assert_eq!(only(&v).receipt.as_ref().expect("a place").line, 6);
}

/* ── no place when the field is missing or ambiguous ──────────────────── */

#[test]
fn a_line_whose_role_field_is_missing_is_never_located() {
    // The parser defaults a missing `message.role` to "assistant"; the place
    // must not ride on that default.
    let store = store_with(&walk_lines(None, WORDS), "confirmed");
    let v = view(&store);
    let entry = only(&v);
    assert_eq!(entry.receipt, None, "{entry:?}");
    assert_eq!(entry.not_located, Some(NotLocated::NoAgentClaim));
}

#[test]
fn a_line_whose_two_fields_disagree_is_never_located() {
    // `type` says assistant, `message.role` says user: ambiguous, so silence.
    let store = store_with(&walk_lines(Some("user"), WORDS), "confirmed");
    let entry = view(&store).sessions.remove(0);
    assert_eq!(entry.receipt, None, "{entry:?}");
    assert_eq!(entry.not_located, Some(NotLocated::NoAgentClaim));
}

#[test]
fn the_speaker_is_never_inferred_from_wording() {
    // A user line saying, word for word, what an agent says when it finishes:
    // its role field names the person, so it is never the agent's claim.
    let lines = vec![user(1, "Add a cart total function."), user(2, WORDS)];
    let store = store_with(&lines, "confirmed");
    let entry = view(&store).sessions.remove(0);
    assert_eq!(entry.receipt, None, "{entry:?}");
    assert_eq!(entry.not_located, Some(NotLocated::NoAgentClaim));
}

#[test]
fn an_earlier_role_marked_claim_is_not_passed_off_as_the_final_turn() {
    // The only role-marked claim is BEFORE the last user prompt; the final
    // turn's own text has no role field. No place: the final-turn rule is not
    // stretched backwards to find something to point at.
    let lines = vec![
        user(1, "Add a cart total function."),
        said(2, Some("assistant"), "The cart total is added."),
        user(3, "And the tests?"),
        said(4, None, "The tests pass."),
    ];
    let store = store_with(&lines, "confirmed");
    let entry = view(&store).sessions.remove(0);
    assert_eq!(entry.receipt, None, "{entry:?}");
}

/* ── only the log ThruLine holds ──────────────────────────────────────── */

#[test]
fn a_line_rewritten_after_it_was_recorded_is_not_located() {
    let store = store_with(&walk_lines(Some("assistant"), WORDS), "confirmed");
    let mut lines = walk_lines(
        Some("assistant"),
        "Done. Everything is finished and shipped.",
    );
    lines.truncate(4);
    std::fs::write(store.transcript(), lines.join("\n") + "\n").expect("rewrite");
    let entry = view(&store).sessions.remove(0);
    assert_eq!(entry.receipt, None, "{entry:?}");
}

#[test]
fn a_log_that_is_gone_is_not_located_and_says_why() {
    let store = store_with(&walk_lines(Some("assistant"), WORDS), "confirmed");
    std::fs::remove_file(store.transcript()).expect("remove");
    let entry = view(&store).sessions.remove(0);
    assert_eq!(entry.receipt, None);
    assert_eq!(entry.not_located, Some(NotLocated::LogUnreadable));
}

#[test]
fn a_conversation_the_ledger_holds_nothing_of_is_not_held() {
    let store = store_with(&walk_lines(Some("assistant"), WORDS), "confirmed");
    store
        .ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             SELECT 'cccccccc-0000-4000-8000-00000000e0e0', project_id, '/synthetic/none.jsonl',
                    '2026-09-23T09:00:00Z' FROM changes WHERE id=?1",
            params![CHANGE],
        )
        .expect("session");
    store
        .ledger
        .conn()
        .execute(
            "INSERT INTO change_session_associations(id, change_id, session_id, provenance,
                                                     basis, actor, asserted_at, reason)
             VALUES('cccccccc-0000-4000-8000-00000000as02', ?1,
                    'cccccccc-0000-4000-8000-00000000e0e0', 'confirmed', 'user', 'sam',
                    '2026-09-23T09:41:00Z', NULL)",
            params![CHANGE],
        )
        .expect("association");
    let v = view(&store);
    let none = v
        .sessions
        .iter()
        .find(|s| s.session_id == "cccccccc-0000-4000-8000-00000000e0e0")
        .expect("listed");
    assert_eq!(none.receipt, None);
    assert_eq!(none.not_located, Some(NotLocated::NotHeld));
}

#[test]
fn only_confirmed_work_is_located() {
    for provenance in ["suggested", "rejected"] {
        let store = store_with(&walk_lines(Some("assistant"), WORDS), provenance);
        assert!(view(&store).sessions.is_empty(), "{provenance}");
    }
}

/* ── a file and a line, and no word ───────────────────────────────────── */

#[test]
fn what_is_served_holds_a_file_and_a_line_and_no_word_of_the_claim() {
    // A claim with a credential-shaped token is located like any other, and
    // nothing of it is in what is served: there is no text to filter.
    let words = "Done. I set CI_TOKEN=glpat-AbCdEfGhIjKlMnOpQrSt and the tests pass.";
    let store = store_with(&walk_lines(Some("assistant"), words), "confirmed");
    let v = view(&store);
    let json = serde_json::to_value(&v).expect("serialise");
    let entry = &json["sessions"][0];
    assert_eq!(entry["receipt"]["line"], 4, "{json}");
    let keys: Vec<&String> = entry["receipt"]
        .as_object()
        .expect("object")
        .keys()
        .collect();
    assert_eq!(keys, ["file", "line"], "{json}");
    let served = json.to_string();
    for word in ["glpat-", "CI_TOKEN", "the tests pass", "Done."] {
        assert!(!served.contains(word), "{word}: {served}");
    }
}

#[test]
fn a_log_path_that_names_a_credential_is_served_masked() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("projects");
    let project = root.join("-synthetic-AKIAIOSFODNN7EXAMPLE-shop");
    std::fs::create_dir_all(&project).expect("mkdir");
    let lines = walk_lines(Some("assistant"), WORDS);
    std::fs::write(
        project.join(format!("{SESSION}.jsonl")),
        lines.join("\n") + "\n",
    )
    .expect("transcript");
    let mut ledger = Ledger::open(dir.path().join("ledger.db")).expect("open");
    thruline_core::ingest::ingest(
        &mut ledger,
        std::slice::from_ref(&root),
        &dir.path().join("mirrors"),
    )
    .expect("ingest");
    let mut record = thruline_core::redact::MaskRecord::default();
    let located = agent_log::locate(&ledger, SESSION, &mut record).expect("locate");
    let agent_log::Located::At(receipt) = located else {
        panic!("not located: {located:?}");
    };
    assert!(
        !receipt.file.contains("AKIAIOSFODNN7EXAMPLE"),
        "{receipt:?}"
    );
    assert!(record.masked > 0, "the masker did not run on the file name");
}

/* ── it reads, and writes nothing ─────────────────────────────────────── */

#[test]
fn locating_writes_nothing() {
    let store = store_with(&walk_lines(Some("assistant"), WORDS), "confirmed");
    let before = store.ledger.conn().total_changes();
    let bytes = std::fs::read(store.transcript()).expect("read");
    let _ = view(&store);
    assert_eq!(store.ledger.conn().total_changes(), before);
    assert_eq!(std::fs::read(store.transcript()).expect("read"), bytes);
}
