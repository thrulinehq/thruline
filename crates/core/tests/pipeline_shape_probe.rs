//! R17b-2a″ step 1 — the PIPELINE probe. Discovery only.
//!
//! A108 measured the vendor's raw tool names and concluded wrongly, because it
//! never checked what ThruLine stores AFTER ingest. This probe measures the
//! pipeline: one instance of each raw codex tool, ingested through the real
//! sealed mirror, with the STORED rows printed.

mod scratch;

use std::fs;

use scratch::TempTree;
use thruline_core::ingest::{ingest_codex, CodexIngestOptions};
use thruline_core::Ledger;

const OWNER: &str = "probe-owner";

const RAW_TOOLS: [(&str, &str); 8] = [
    ("exec_command", r#""{\"cmd\":\"git status --porcelain\"}""#),
    ("shell_command", r#""{\"command\":\"git diff --stat\"}""#),
    ("exec", "null"),
    ("apply_patch", "null"),
    ("write_stdin", r#""{\"chars\":\"y\"}""#),
    ("bash", r#""{\"command\":\"echo raw-bash\"}""#),
    ("shell", r#""{\"command\":\"echo raw-shell\"}""#),
    ("terminal", r#""{\"command\":\"echo raw-terminal\"}""#),
];

fn transcript() -> String {
    let mut lines = vec![
        r#"{"timestamp":"2030-01-01T00:00:00Z","type":"session_meta","payload":{"id":"probe-pipeline","cwd":"/probe/p"}}"#.to_string(),
        r#"{"timestamp":"2030-01-01T00:00:01Z","type":"turn_context","payload":{"turn_id":"probe-turn","cwd":"/probe/p","branch":"main"}}"#.to_string(),
    ];
    for (i, (name, args)) in RAW_TOOLS.iter().enumerate() {
        lines.push(format!(
            r#"{{"timestamp":"2030-01-01T00:00:{:02}Z","type":"response_item","payload":{{"type":"function_call","id":"fc{i}","call_id":"call-{i}","name":"{name}","arguments":{args}}}}}"#,
            i + 2
        ));
    }
    lines.join("\n") + "\n"
}

fn ingested() -> (TempTree, Ledger) {
    let tmp = TempTree::new();
    let source = tmp.path().join("source").join("rollout-probe.jsonl");
    fs::create_dir_all(source.parent().expect("parent")).expect("mkdir");
    fs::write(&source, transcript()).expect("write");
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
    .expect("ingest");
    println!("PROBE events_by_kind={:?}", report.events_by_kind);
    println!("PROBE adapter_families={:?}", report.adapter_families);
    drop(ledger);
    let ledger = Ledger::open(&db).expect("reopen");
    (tmp, ledger)
}

fn stored(ledger: &Ledger) -> Vec<(String, String)> {
    ledger
        .conn()
        .prepare(
            "SELECT COALESCE(tool_name,''), COALESCE(tool_input,'')
             FROM events WHERE kind='tool_use' ORDER BY seq",
        )
        .expect("prep")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("q")
        .flatten()
        .collect()
}

/// PIN — the CANONICAL column still carries only canonical names.
///
/// Preservation has landed, so the raw name now lives in `source_tool_name`;
/// this pin guards the other half of that split — `tool_name` must stay
/// canonical, or the two columns would drift into meaning the same thing and
/// the accessor would become a coin flip.
#[test]
fn the_canonical_column_carries_only_canonical_names() {
    let (_t, ledger) = ingested();
    let names: Vec<String> = stored(&ledger).into_iter().map(|(n, _)| n).collect();
    assert_eq!(
        names,
        vec![
            "Bash",        // exec_command
            "Bash",        // shell_command
            "exec",        // unmapped: survives
            "Edit",        // apply_patch
            "write_stdin", // unmapped: survives
            "Bash",        // bash
            "Bash",        // shell
            "Bash",        // terminal
        ],
        "the canonical mapping is the shape a raw-keyed registry must work around"
    );
    for destroyed in ["exec_command", "shell_command", "terminal", "apply_patch"] {
        let n: i64 = ledger
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM events WHERE tool_name = ?1",
                [destroyed],
                |r| r.get(0),
            )
            .expect("count");
        assert_eq!(
            n, 0,
            "raw name {destroyed} must not survive today — if it ever \
                          does, preservation has landed and this pin should change"
        );
    }
}

/// PIN — the collision that makes a CANONICAL-name registry unworkable.
///
/// `exec_command` and `shell_command` both store as `Bash`, but their commands
/// live under DIFFERENT keys (`cmd` vs `command`). One canonical name therefore
/// requires two different extractors, which a canonical-keyed registry cannot
/// express. This is the measured justification for keying on the raw name.
#[test]
fn two_raw_tools_collide_on_bash_with_different_command_keys() {
    let (_t, ledger) = ingested();
    let rows = stored(&ledger);
    let exec_command = &rows[0];
    let shell_command = &rows[1];
    assert_eq!(exec_command.0, "Bash");
    assert_eq!(shell_command.0, "Bash");
    assert!(
        exec_command.1.contains("\"cmd\""),
        "exec_command carries `cmd`: {}",
        exec_command.1
    );
    assert!(
        shell_command.1.contains("\"command\""),
        "shell_command carries `command`: {}",
        shell_command.1
    );
    assert_ne!(
        exec_command.1, shell_command.1,
        "same canonical name, different command key — one extractor cannot serve both"
    );
}

/// PIN — A109's surviving claim, re-verified against ROWS rather than rollouts.
///
/// The stored `tool_input` for an `exec_command` is a top-level `cmd`, which the
/// legacy chain (`command` -> `input.cmd` -> `input.command`) cannot read.
#[test]
fn the_legacy_chain_cannot_read_a_stored_exec_command() {
    let (_t, ledger) = ingested();
    let input = &stored(&ledger)[0].1;
    assert_eq!(
        thruline_core::derive::json_string_field(input, "command"),
        None,
        "the legacy chain's first probe finds nothing"
    );
    assert_eq!(
        thruline_core::derive::json_string_field(input, "input"),
        None,
        "and its fallback key is absent too, so the chain returns None"
    );
    assert_eq!(
        thruline_core::derive::json_string_field(input, "cmd").as_deref(),
        Some("git status --porcelain"),
        "the command IS there, under a key the legacy chain never reads"
    );
}

/// PIN — `exec`, the single most common real tool, stores no arguments at all.
#[test]
fn exec_stores_no_arguments_and_is_unbindable() {
    let (_t, ledger) = ingested();
    let (name, input) = &stored(&ledger)[2];
    assert_eq!(name, "exec", "exec has no canonical mapping");
    assert_eq!(
        input, "null",
        "and carries no arguments to bind a command from"
    );
}

/// PIN — the raw vendor name now SURVIVES ingest, and only where it had to.
///
/// `source_tool_name` is populated exactly when canonicalisation changed the
/// name; `NULL` means the raw and canonical names agree, so nothing was lost.
/// That distinction is the whole point: an empty column would otherwise mean
/// both "unchanged" and "never recorded".
#[test]
fn the_raw_tool_name_is_preserved_where_canonicalisation_changed_it() {
    let (_t, ledger) = ingested();
    let rows: Vec<(String, Option<String>)> = ledger
        .conn()
        .prepare(
            "SELECT COALESCE(tool_name,''), source_tool_name
             FROM events WHERE kind='tool_use' ORDER BY seq",
        )
        .expect("prep")
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("q")
        .flatten()
        .collect();

    assert_eq!(
        rows,
        vec![
            ("Bash".to_string(), Some("exec_command".to_string())),
            ("Bash".to_string(), Some("shell_command".to_string())),
            ("exec".to_string(), None), // unmapped: nothing was lost
            ("Edit".to_string(), Some("apply_patch".to_string())),
            ("write_stdin".to_string(), None), // unmapped
            ("Bash".to_string(), Some("bash".to_string())),
            ("Bash".to_string(), Some("shell".to_string())),
            ("Bash".to_string(), Some("terminal".to_string())),
        ],
        "preservation must cover exactly the canonicalising cases"
    );
}

/// PIN — the accessor is the ONLY way the registry may read a tool name.
///
/// Reading `tool_name` directly would silently key five different raw tools on
/// `Bash`; reading `source_tool_name` directly would be `NULL` for the unmapped
/// ones. Neither is correct alone.
#[test]
fn the_accessor_returns_the_raw_name_for_every_tool() {
    let (_t, ledger) = ingested();
    let names: Vec<String> = ledger
        .conn()
        .prepare(
            "SELECT COALESCE(tool_name,''), source_tool_name
             FROM events WHERE kind='tool_use' ORDER BY seq",
        )
        .expect("prep")
        .query_map([], |r| {
            let canonical: String = r.get(0)?;
            let raw: Option<String> = r.get(1)?;
            Ok(thruline_core::capture::raw_tool_name(&canonical, raw.as_deref()).to_string())
        })
        .expect("q")
        .flatten()
        .collect();
    assert_eq!(
        names,
        vec![
            "exec_command",
            "shell_command",
            "exec",
            "apply_patch",
            "write_stdin",
            "bash",
            "shell",
            "terminal",
        ],
        "the accessor must recover the vendor's own name for every tool"
    );
}
